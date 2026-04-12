use crate::{CliResult, relative_to_repo};
use pytecode_archive::{JarFile, RewriteOptions};
use pytecode_engine::model::{
    ClassModel, CodeItem, DebugInfoPolicy, FrameComputationMode, Label, LdcValue,
};
use pytecode_engine::transform::ApplyClassTransform;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeobfuscationPackageStat {
    pub package: String,
    pub class_count: usize,
    pub suspicious_class_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeobfuscationHotspot {
    pub class_name: String,
    pub byte_len: usize,
    pub method_count: usize,
    pub field_count: usize,
    pub readable_string_count: usize,
    pub branch_count: usize,
    pub nop_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeobfuscationStringHint {
    pub class_name: String,
    pub string_count: usize,
    pub samples: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeobfuscationAnalysisReport {
    pub input_jar: String,
    pub class_entries: usize,
    pub resource_entries: usize,
    pub suspicious_class_count: usize,
    pub rl_class_count: usize,
    pub top_packages: Vec<DeobfuscationPackageStat>,
    pub sample_suspicious_classes: Vec<String>,
    pub sample_rl_classes: Vec<String>,
    pub compiler_control_excludes: Vec<String>,
    pub hotspot_classes: Vec<DeobfuscationHotspot>,
    pub string_hint_classes: Vec<DeobfuscationStringHint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeobfuscationRewriteReport {
    pub input_jar: String,
    pub output_jar: String,
    pub class_entries: usize,
    pub resource_entries: usize,
    pub classes_changed: usize,
    pub nops_removed: usize,
    pub noop_gotos_removed: usize,
    pub goto_redirects: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct CompilerControlEntry {
    #[serde(default, rename = "match")]
    matches: Vec<String>,
    #[serde(default)]
    c2: Option<CompilerControlTier>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct CompilerControlTier {
    #[serde(default)]
    exclude: bool,
}

#[derive(Debug, Clone)]
struct ClassMetrics {
    class_name: String,
    package: String,
    byte_len: usize,
    method_count: usize,
    field_count: usize,
    readable_strings: Vec<String>,
    branch_count: usize,
    nop_count: usize,
    suspicious_name: bool,
    rl_name: bool,
}

#[derive(Debug, Default)]
struct RewriteStats {
    classes_changed: usize,
    nops_removed: usize,
    noop_gotos_removed: usize,
    goto_redirects: usize,
}

#[derive(Debug, Default)]
struct ClassRewriteStats {
    nops_removed: usize,
    noop_gotos_removed: usize,
    goto_redirects: usize,
}

impl ClassRewriteStats {
    fn changed(&self) -> bool {
        self.nops_removed > 0 || self.noop_gotos_removed > 0 || self.goto_redirects > 0
    }
}

pub fn analyze_deobfuscation(jar: &Path) -> CliResult<DeobfuscationAnalysisReport> {
    let jar_file = JarFile::open(jar)?;
    let mut class_metrics = Vec::new();

    for entry in jar_file
        .entries
        .iter()
        .filter(|entry| !entry.metadata.is_dir && entry.filename.ends_with(".class"))
    {
        let model = ClassModel::from_bytes(&entry.bytes)?;
        class_metrics.push(class_metrics_from_model(&model, entry.bytes.len()));
    }

    let mut package_stats = HashMap::<String, (usize, usize)>::new();
    let mut suspicious_classes = Vec::new();
    let mut rl_classes = Vec::new();
    for metrics in &class_metrics {
        let counts = package_stats
            .entry(metrics.package.clone())
            .or_insert((0, 0));
        counts.0 += 1;
        if metrics.suspicious_name {
            counts.1 += 1;
            suspicious_classes.push(metrics.class_name.clone());
        }
        if metrics.rl_name {
            rl_classes.push(metrics.class_name.clone());
        }
    }

    let mut top_packages = package_stats
        .into_iter()
        .map(
            |(package, (class_count, suspicious_class_count))| DeobfuscationPackageStat {
                package,
                class_count,
                suspicious_class_count,
            },
        )
        .collect::<Vec<_>>();
    top_packages.sort_by(|left, right| {
        right
            .class_count
            .cmp(&left.class_count)
            .then_with(|| {
                right
                    .suspicious_class_count
                    .cmp(&left.suspicious_class_count)
            })
            .then_with(|| left.package.cmp(&right.package))
    });
    top_packages.truncate(10);

    let mut hotspot_classes = class_metrics
        .iter()
        .map(|metrics| DeobfuscationHotspot {
            class_name: metrics.class_name.clone(),
            byte_len: metrics.byte_len,
            method_count: metrics.method_count,
            field_count: metrics.field_count,
            readable_string_count: metrics.readable_strings.len(),
            branch_count: metrics.branch_count,
            nop_count: metrics.nop_count,
        })
        .collect::<Vec<_>>();
    hotspot_classes.sort_by(|left, right| {
        right
            .method_count
            .cmp(&left.method_count)
            .then_with(|| right.field_count.cmp(&left.field_count))
            .then_with(|| right.byte_len.cmp(&left.byte_len))
            .then_with(|| left.class_name.cmp(&right.class_name))
    });
    hotspot_classes.truncate(10);

    let mut string_hint_classes = class_metrics
        .iter()
        .filter(|metrics| !metrics.readable_strings.is_empty())
        .map(|metrics| DeobfuscationStringHint {
            class_name: metrics.class_name.clone(),
            string_count: metrics.readable_strings.len(),
            samples: metrics.readable_strings.iter().take(3).cloned().collect(),
        })
        .collect::<Vec<_>>();
    string_hint_classes.sort_by(|left, right| {
        right
            .string_count
            .cmp(&left.string_count)
            .then_with(|| left.class_name.cmp(&right.class_name))
    });
    string_hint_classes.truncate(10);

    Ok(DeobfuscationAnalysisReport {
        input_jar: relative_to_repo(jar),
        class_entries: class_metrics.len(),
        resource_entries: jar_file
            .entries
            .iter()
            .filter(|entry| !entry.metadata.is_dir && !entry.filename.ends_with(".class"))
            .count(),
        suspicious_class_count: suspicious_classes.len(),
        rl_class_count: rl_classes.len(),
        top_packages,
        sample_suspicious_classes: suspicious_classes.into_iter().take(50).collect(),
        sample_rl_classes: rl_classes.into_iter().take(25).collect(),
        compiler_control_excludes: compiler_control_excludes(&jar_file)?,
        hotspot_classes,
        string_hint_classes,
    })
}

pub fn rewrite_deobfuscation(jar: &Path, output: &Path) -> CliResult<DeobfuscationRewriteReport> {
    let mut jar_file = JarFile::open(jar)?;
    let class_entries = jar_file
        .entries
        .iter()
        .filter(|entry| !entry.metadata.is_dir && entry.filename.ends_with(".class"))
        .count();
    let resource_entries = jar_file
        .entries
        .iter()
        .filter(|entry| !entry.metadata.is_dir && !entry.filename.ends_with(".class"))
        .count();
    let mut rewrite_stats = RewriteStats::default();
    let mut transform = |model: &mut ClassModel| {
        let class_stats = rewrite_class(model);
        if class_stats.changed() {
            rewrite_stats.classes_changed += 1;
            rewrite_stats.nops_removed += class_stats.nops_removed;
            rewrite_stats.noop_gotos_removed += class_stats.noop_gotos_removed;
            rewrite_stats.goto_redirects += class_stats.goto_redirects;
        }
        Ok(())
    };
    jar_file.rewrite(
        Some(output),
        Some(&mut transform as &mut dyn ApplyClassTransform),
        RewriteOptions {
            frame_mode: FrameComputationMode::Recompute,
            resolver: None,
            debug_info: DebugInfoPolicy::Preserve,
        },
    )?;

    Ok(DeobfuscationRewriteReport {
        input_jar: relative_to_repo(jar),
        output_jar: relative_to_repo(output),
        class_entries,
        resource_entries,
        classes_changed: rewrite_stats.classes_changed,
        nops_removed: rewrite_stats.nops_removed,
        noop_gotos_removed: rewrite_stats.noop_gotos_removed,
        goto_redirects: rewrite_stats.goto_redirects,
    })
}

fn class_metrics_from_model(model: &ClassModel, byte_len: usize) -> ClassMetrics {
    let mut readable_strings = Vec::new();
    let mut seen_strings = HashSet::new();
    let mut branch_count = 0_usize;
    let mut nop_count = 0_usize;

    for method in &model.methods {
        let Some(code) = &method.code else {
            continue;
        };
        for item in &code.instructions {
            match item {
                CodeItem::Ldc(insn) => {
                    if let LdcValue::String(value) = &insn.value
                        && is_readable_string(value)
                        && seen_strings.insert(value.clone())
                    {
                        readable_strings.push(value.clone());
                    }
                }
                CodeItem::Branch(_) => branch_count += 1,
                CodeItem::Raw(pytecode_engine::raw::Instruction::Simple { opcode: 0, .. }) => {
                    nop_count += 1;
                }
                _ => {}
            }
        }
    }

    let package = package_name(&model.name);
    let simple_name = simple_name(&model.name);
    ClassMetrics {
        class_name: model.name.clone(),
        package,
        byte_len,
        method_count: model.methods.len(),
        field_count: model.fields.len(),
        readable_strings,
        branch_count,
        nop_count,
        suspicious_name: is_suspicious_simple_name(simple_name),
        rl_name: is_rl_simple_name(simple_name),
    }
}

fn compiler_control_excludes(jar_file: &JarFile) -> CliResult<Vec<String>> {
    let Some(entry) = jar_file
        .entries
        .iter()
        .find(|entry| entry.filename == "compilercontrol.json")
    else {
        return Ok(Vec::new());
    };
    let parsed = serde_json::from_slice::<Vec<CompilerControlEntry>>(&entry.bytes)?;
    Ok(parsed
        .into_iter()
        .filter(|entry| entry.c2.as_ref().is_some_and(|tier| tier.exclude))
        .flat_map(|entry| entry.matches.into_iter())
        .collect())
}

fn rewrite_class(model: &mut ClassModel) -> ClassRewriteStats {
    let mut stats = ClassRewriteStats::default();
    for method in &mut model.methods {
        let Some(code) = method.code.as_mut() else {
            continue;
        };

        stats.nops_removed += remove_nops(code);
        loop {
            let redirected = redirect_goto_chains(code);
            let removed = remove_noop_gotos(code);
            stats.goto_redirects += redirected;
            stats.noop_gotos_removed += removed;
            if redirected == 0 && removed == 0 {
                break;
            }
        }
    }
    stats
}

fn remove_nops(code: &mut pytecode_engine::model::CodeModel) -> usize {
    let before = code.instructions.len();
    code.instructions.retain(|item| {
        !matches!(
            item,
            CodeItem::Raw(pytecode_engine::raw::Instruction::Simple { opcode: 0, .. })
        )
    });
    before - code.instructions.len()
}

fn remove_noop_gotos(code: &mut pytecode_engine::model::CodeModel) -> usize {
    let mut removed = 0_usize;
    let mut rewritten = Vec::with_capacity(code.instructions.len());
    for (index, item) in code.instructions.iter().enumerate() {
        let should_remove = matches!(
            item,
            CodeItem::Branch(branch)
                if is_unconditional_goto(branch.opcode)
                    && targets_immediate_fallthrough(&code.instructions, index, &branch.target)
        );
        if should_remove {
            removed += 1;
        } else {
            rewritten.push(item.clone());
        }
    }
    if removed > 0 {
        code.instructions = rewritten;
    }
    removed
}

fn redirect_goto_chains(code: &mut pytecode_engine::model::CodeModel) -> usize {
    let snapshot = code.instructions.clone();
    let label_positions = code
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(index, item)| match item {
            CodeItem::Label(label) => Some((label.clone(), index)),
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    let mut redirected = 0_usize;

    for item in &mut code.instructions {
        let CodeItem::Branch(branch) = item else {
            continue;
        };
        if !is_unconditional_goto(branch.opcode) {
            continue;
        }
        let resolved = resolve_terminal_goto_target(&branch.target, &label_positions, &snapshot);
        if let Some(target) = resolved
            && target != branch.target
        {
            branch.target = target;
            redirected += 1;
        }
    }

    redirected
}

fn resolve_terminal_goto_target(
    start: &Label,
    label_positions: &HashMap<Label, usize>,
    instructions: &[CodeItem],
) -> Option<Label> {
    let mut current = start.clone();
    let mut seen = HashSet::new();
    while seen.insert(current.clone()) {
        let index = *label_positions.get(&current)?;
        let next = instructions
            .iter()
            .skip(index + 1)
            .find(|item| !matches!(item, CodeItem::Label(_)));
        match next {
            Some(CodeItem::Branch(branch)) if is_unconditional_goto(branch.opcode) => {
                current = branch.target.clone();
            }
            _ => return Some(current),
        }
    }
    None
}

fn targets_immediate_fallthrough(instructions: &[CodeItem], index: usize, target: &Label) -> bool {
    instructions
        .iter()
        .skip(index + 1)
        .take_while(|item| matches!(item, CodeItem::Label(_)))
        .any(|item| matches!(item, CodeItem::Label(label) if label == target))
}

fn is_unconditional_goto(opcode: u8) -> bool {
    matches!(opcode, 167 | 200)
}

fn package_name(class_name: &str) -> String {
    class_name
        .rsplit_once('/')
        .map(|(package, _)| package.to_owned())
        .unwrap_or_else(|| "<root>".to_owned())
}

fn simple_name(class_name: &str) -> &str {
    class_name.rsplit('/').next().unwrap_or(class_name)
}

fn is_suspicious_simple_name(name: &str) -> bool {
    (1..=2).contains(&name.len()) && name.bytes().all(|byte| byte.is_ascii_lowercase())
}

fn is_rl_simple_name(name: &str) -> bool {
    name.strip_prefix("rl").is_some_and(|suffix| {
        !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
    })
}

fn is_readable_string(value: &str) -> bool {
    value.chars().count() >= 4
        && value.chars().filter(|ch| ch.is_alphabetic()).count() >= 3
        && value.chars().all(|ch| !ch.is_control())
}
