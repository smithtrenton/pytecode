//! CLI helpers for compatibility checks, benchmark reporting, and archive smoke flows.

use clap::{Parser, Subcommand};
mod deobfuscate;
mod jar_patcher;
use pytecode_archive::{JarFile, RewriteOptions};
use pytecode_engine::constants::MethodAccessFlags;
use pytecode_engine::fixtures::{compatibility_manifest, default_benchmark_jar, repo_root};
use pytecode_engine::indexes::Utf8Index;
use pytecode_engine::model::ClassModel;
use pytecode_engine::modified_utf8::decode_modified_utf8;
use pytecode_engine::parse_class;
use pytecode_engine::raw::{AttributeInfo, ClassFile, ConstantPoolEntry, RawClassStub};
use pytecode_engine::stages::BenchmarkStage;
use pytecode_engine::transform::{
    Pipeline, class_named, method_is_public, method_is_static, method_named, on_methods,
};
use pytecode_engine::write_class;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};
use std::time::Instant;
use thiserror::Error;

pub use deobfuscate::{
    DeobfuscationAnalysisReport, DeobfuscationHotspot, DeobfuscationPackageStat,
    DeobfuscationRewriteReport, DeobfuscationStringHint, analyze_deobfuscation,
    rewrite_deobfuscation,
};
pub use jar_patcher::{JarPatchReport, JarPatchRuleReport, patch_jar};

pub type CliResult<T> = std::result::Result<T, CliError>;

#[derive(Debug, Error)]
pub enum CliError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Archive(#[from] pytecode_archive::ArchiveError),
    #[error(transparent)]
    Engine(#[from] pytecode_engine::EngineError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Regex(#[from] regex::Error),
    #[error("invalid patch plan: {message}")]
    InvalidPatchPlan { message: String },
    #[error("max_release must be between 8 and 25")]
    InvalidMaxRelease,
    #[error("iterations must be at least 1")]
    InvalidIterations,
    #[error("missing constant-pool entry at index {index}")]
    MissingConstantPoolEntry { index: u16 },
    #[error("constant-pool entry {index} is not Utf8")]
    ConstantPoolEntryNotUtf8 { index: u16 },
}

#[derive(Debug, Parser)]
#[command(about = "Rust compatibility, benchmark, and archive tooling for pytecode", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Emit the Rust fixture and benchmark compatibility manifest as JSON.
    CompatManifest {
        #[arg(long, default_value_t = 25)]
        max_release: u8,
    },
    /// Run isolated-stage Rust benchmarks over a checked-in or provided JAR.
    BenchSmoke {
        #[arg(long)]
        jar: Option<PathBuf>,
        #[arg(long, default_value_t = 1)]
        iterations: usize,
    },
    /// Emit a JSON summary for one compiled classfile fixture or class path.
    ClassSummary {
        #[arg(long)]
        path: PathBuf,
    },
    /// Rewrite one JAR with a small built-in transform for smoke coverage.
    RewriteSmoke {
        #[arg(long)]
        jar: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value = "HelloWorld")]
        class_name: String,
    },
    /// Rewrite one JAR with declarative JSON patch rules.
    PatchJar {
        #[arg(long)]
        jar: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        rules: PathBuf,
    },
    /// Analyze or rewrite an obfuscated JAR with built-in heuristics.
    Deobfuscate {
        #[command(subcommand)]
        command: DeobfuscateCommand,
    },
}

#[derive(Debug, Subcommand)]
enum DeobfuscateCommand {
    /// Report naming, package, compiler-control, and string-hint signals for a JAR.
    Analyze {
        #[arg(long)]
        jar: PathBuf,
    },
    /// Apply safe bytecode cleanup passes and write a rewritten JAR.
    Rewrite {
        #[arg(long)]
        jar: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenchmarkStageReport {
    pub stage: BenchmarkStage,
    pub iterations: usize,
    pub samples_milliseconds: Vec<u128>,
    pub median_milliseconds: u128,
    pub spread_milliseconds: u128,
    pub min_milliseconds: u128,
    pub max_milliseconds: u128,
    pub units: usize,
    pub bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenchmarkReport {
    pub jar: String,
    pub iterations: usize,
    pub stage_reports: Vec<BenchmarkStageReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassSummary {
    pub major: u16,
    pub minor: u16,
    pub constant_pool_count: usize,
    pub class_attr_names: Vec<String>,
    pub methods: Vec<MethodSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RewriteSmokeReport {
    pub input_jar: String,
    pub output_jar: String,
    pub class_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MethodSummary {
    pub name: String,
    pub descriptor: String,
    pub opcodes: Option<Vec<u8>>,
}

pub fn run_cli() -> CliResult<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::CompatManifest { max_release } => {
            if !(8..=25).contains(&max_release) {
                return Err(CliError::InvalidMaxRelease);
            }
            let manifest = compatibility_manifest(max_release)?;
            println!("{}", serde_json::to_string_pretty(&manifest)?);
        }
        Commands::BenchSmoke { jar, iterations } => {
            if iterations == 0 {
                return Err(CliError::InvalidIterations);
            }
            let report = run_smoke_benchmark(jar, iterations)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Commands::ClassSummary { path } => {
            let summary = export_class_summary(&path)?;
            println!("{}", serde_json::to_string_pretty(&summary)?);
        }
        Commands::RewriteSmoke {
            jar,
            output,
            class_name,
        } => {
            let report = rewrite_smoke(&jar, &output, &class_name)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Commands::PatchJar { jar, output, rules } => {
            let report = patch_jar(&jar, &output, &rules)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Commands::Deobfuscate { command } => match command {
            DeobfuscateCommand::Analyze { jar } => {
                let report = analyze_deobfuscation(&jar)?;
                println!("{}", serde_json::to_string_pretty(&report)?);
            }
            DeobfuscateCommand::Rewrite { jar, output } => {
                let report = rewrite_deobfuscation(&jar, &output)?;
                println!("{}", serde_json::to_string_pretty(&report)?);
            }
        },
    }
    Ok(())
}

pub fn run_smoke_benchmark(jar: Option<PathBuf>, iterations: usize) -> CliResult<BenchmarkReport> {
    if iterations == 0 {
        return Err(CliError::InvalidIterations);
    }
    let jar_path = jar.unwrap_or(default_benchmark_jar()?);
    let jar_display = relative_to_repo(&jar_path);
    let stage_reports = BenchmarkStage::ALL
        .into_iter()
        .map(|stage| benchmark_stage(&jar_path, stage, iterations))
        .collect::<CliResult<Vec<_>>>()?;

    Ok(BenchmarkReport {
        jar: jar_display,
        iterations,
        stage_reports,
    })
}

pub fn export_class_summary(path: &Path) -> CliResult<ClassSummary> {
    let bytes = std::fs::read(path)?;
    let classfile = parse_class(&bytes)?;
    class_summary(&classfile)
}

pub fn rewrite_smoke(jar: &Path, output: &Path, class_name: &str) -> CliResult<RewriteSmokeReport> {
    let mut jar_file = JarFile::open(jar)?;
    let method_matcher = method_named("main") & method_is_public() & method_is_static();
    let owner_matcher = class_named(class_name.to_owned());
    let mut transform = Pipeline::of(on_methods(
        |method, _owner| {
            method.access_flags |= MethodAccessFlags::FINAL;
            Ok(())
        },
        Some(method_matcher),
        Some(owner_matcher),
    ));
    jar_file.rewrite(
        Some(output),
        Some(&mut transform),
        RewriteOptions::default(),
    )?;
    Ok(RewriteSmokeReport {
        input_jar: relative_to_repo(jar),
        output_jar: relative_to_repo(output),
        class_name: class_name.to_owned(),
    })
}

fn benchmark_stage(
    path: &Path,
    stage: BenchmarkStage,
    iterations: usize,
) -> CliResult<BenchmarkStageReport> {
    let mut inputs = BenchmarkInputs::new(path);
    prepare_benchmark_inputs(&mut inputs, stage)?;
    let mut samples_milliseconds = Vec::with_capacity(iterations);
    let mut units = 0_usize;
    let mut bytes = 0_usize;

    for _ in 0..iterations {
        let started = Instant::now();
        let (stage_units, stage_bytes) = execute_benchmark_stage(path, &mut inputs, stage)?;
        samples_milliseconds.push(started.elapsed().as_millis());
        units = stage_units;
        bytes = stage_bytes;
    }
    let (median_milliseconds, spread_milliseconds, min_milliseconds, max_milliseconds) =
        summarize_samples(&samples_milliseconds);

    Ok(BenchmarkStageReport {
        stage,
        iterations,
        samples_milliseconds,
        median_milliseconds,
        spread_milliseconds,
        min_milliseconds,
        max_milliseconds,
        units,
        bytes,
    })
}

#[derive(Debug)]
struct BenchmarkInputs {
    path: PathBuf,
    archive: Option<JarFile>,
    class_entries: Option<Vec<RawClassStub>>,
    parsed_classfiles: Option<Vec<ClassFile>>,
    lifted_models: Option<Vec<ClassModel>>,
    lowered_classfiles: Option<Vec<ClassFile>>,
}

impl BenchmarkInputs {
    fn new(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            archive: None,
            class_entries: None,
            parsed_classfiles: None,
            lifted_models: None,
            lowered_classfiles: None,
        }
    }

    fn archive(&mut self) -> CliResult<&JarFile> {
        if self.archive.is_none() {
            self.archive = Some(JarFile::open(&self.path)?);
        }
        Ok(self.archive.as_ref().expect("archive cache initialized"))
    }

    fn class_entries(&mut self) -> CliResult<&Vec<RawClassStub>> {
        if self.class_entries.is_none() {
            let (classes, _resources) = self.archive()?.parse_classes();
            self.class_entries = Some(classes.into_iter().map(|(_info, class)| class).collect());
        }
        Ok(self
            .class_entries
            .as_ref()
            .expect("class entry cache initialized"))
    }

    fn parsed_classfiles(&mut self) -> CliResult<&Vec<ClassFile>> {
        if self.parsed_classfiles.is_none() {
            let parsed_classfiles = self
                .class_entries()?
                .iter()
                .map(|class| parse_class(&class.bytes))
                .collect::<std::result::Result<Vec<_>, _>>()?;
            self.parsed_classfiles = Some(parsed_classfiles);
        }
        Ok(self
            .parsed_classfiles
            .as_ref()
            .expect("parsed class cache initialized"))
    }

    fn lifted_models(&mut self) -> CliResult<&Vec<ClassModel>> {
        if self.lifted_models.is_none() {
            let lifted_models = self
                .parsed_classfiles()?
                .iter()
                .map(ClassModel::from_classfile)
                .collect::<std::result::Result<Vec<_>, _>>()?;
            self.lifted_models = Some(lifted_models);
        }
        Ok(self
            .lifted_models
            .as_ref()
            .expect("lifted model cache initialized"))
    }

    fn lowered_classfiles(&mut self) -> CliResult<&Vec<ClassFile>> {
        if self.lowered_classfiles.is_none() {
            let lowered_classfiles = self
                .lifted_models()?
                .iter()
                .map(ClassModel::to_classfile)
                .collect::<std::result::Result<Vec<_>, _>>()?;
            self.lowered_classfiles = Some(lowered_classfiles);
        }
        Ok(self
            .lowered_classfiles
            .as_ref()
            .expect("lowered classfile cache initialized"))
    }
}

fn prepare_benchmark_inputs(inputs: &mut BenchmarkInputs, stage: BenchmarkStage) -> CliResult<()> {
    match stage {
        BenchmarkStage::JarRead => {}
        BenchmarkStage::ClassParse => {
            inputs.class_entries()?;
        }
        BenchmarkStage::ModelLift => {
            inputs.parsed_classfiles()?;
        }
        BenchmarkStage::ModelLower => {
            inputs.lifted_models()?;
        }
        BenchmarkStage::ClassWrite => {
            inputs.lowered_classfiles()?;
        }
    }
    Ok(())
}

fn execute_benchmark_stage(
    path: &Path,
    inputs: &mut BenchmarkInputs,
    stage: BenchmarkStage,
) -> CliResult<(usize, usize)> {
    match stage {
        BenchmarkStage::JarRead => {
            let jar = JarFile::open(path)?;
            let (class_entries, resource_entries) = jar.parse_classes();
            let total_bytes = jar.entries.iter().map(|entry| entry.bytes.len()).sum();
            Ok((class_entries.len() + resource_entries.len(), total_bytes))
        }
        BenchmarkStage::ClassParse => {
            let class_entries = inputs.class_entries()?;
            let parsed_classfiles = class_entries
                .iter()
                .map(|class| parse_class(&class.bytes))
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let class_bytes = class_entries.iter().map(|class| class.bytes.len()).sum();
            Ok((parsed_classfiles.len(), class_bytes))
        }
        BenchmarkStage::ModelLift => {
            let class_bytes = inputs
                .class_entries()?
                .iter()
                .map(|class| class.bytes.len())
                .sum();
            let lifted_models = inputs
                .parsed_classfiles()?
                .iter()
                .map(ClassModel::from_classfile)
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok((lifted_models.len(), class_bytes))
        }
        BenchmarkStage::ModelLower => {
            let class_bytes = inputs
                .class_entries()?
                .iter()
                .map(|class| class.bytes.len())
                .sum();
            let lowered_classfiles = inputs
                .lifted_models()?
                .iter()
                .map(ClassModel::to_classfile)
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok((lowered_classfiles.len(), class_bytes))
        }
        BenchmarkStage::ClassWrite => {
            let serialized_classes = inputs
                .lowered_classfiles()?
                .iter()
                .map(write_class)
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let class_bytes = serialized_classes.iter().map(Vec::len).sum();
            Ok((serialized_classes.len(), class_bytes))
        }
    }
}

fn summarize_samples(samples: &[u128]) -> (u128, u128, u128, u128) {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let min_milliseconds = *sorted.first().expect("benchmark samples are non-empty");
    let max_milliseconds = *sorted.last().expect("benchmark samples are non-empty");
    let median_milliseconds = if sorted.len() % 2 == 1 {
        sorted[sorted.len() / 2]
    } else {
        let upper = sorted.len() / 2;
        let lower = upper - 1;
        (sorted[lower] + sorted[upper]) / 2
    };
    let spread_milliseconds = max_milliseconds - min_milliseconds;
    (
        median_milliseconds,
        spread_milliseconds,
        min_milliseconds,
        max_milliseconds,
    )
}

pub(crate) fn relative_to_repo(path: &Path) -> String {
    match path.strip_prefix(repo_root()) {
        Ok(relative) => relative.to_string_lossy().replace('\\', "/"),
        Err(_) => path.to_string_lossy().replace('\\', "/"),
    }
}

fn class_summary(classfile: &ClassFile) -> CliResult<ClassSummary> {
    let methods = classfile
        .methods
        .iter()
        .map(|method| {
            Ok(MethodSummary {
                name: constant_pool_utf8(classfile, method.name_index)?,
                descriptor: constant_pool_utf8(classfile, method.descriptor_index)?,
                opcodes: method
                    .attributes
                    .iter()
                    .find_map(|attribute| match attribute {
                        AttributeInfo::Code(code) => Some(
                            code.code
                                .iter()
                                .map(|instruction| instruction.opcode())
                                .collect(),
                        ),
                        _ => None,
                    }),
            })
        })
        .collect::<CliResult<Vec<_>>>()?;

    Ok(ClassSummary {
        major: classfile.major_version,
        minor: classfile.minor_version,
        constant_pool_count: classfile.constant_pool.len(),
        class_attr_names: classfile
            .attributes
            .iter()
            .map(|attribute| constant_pool_utf8(classfile, attribute.attribute_name_index()))
            .collect::<CliResult<Vec<_>>>()?,
        methods,
    })
}

fn constant_pool_utf8(classfile: &ClassFile, index: Utf8Index) -> CliResult<String> {
    let raw = index.value();
    let entry = classfile
        .constant_pool
        .get(raw as usize)
        .and_then(Option::as_ref)
        .ok_or(CliError::MissingConstantPoolEntry { index: raw })?;
    match entry {
        ConstantPoolEntry::Utf8(info) => Ok(decode_modified_utf8(&info.bytes)?),
        _ => Err(CliError::ConstantPoolEntryNotUtf8 { index: raw }),
    }
}
