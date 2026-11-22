use crate::{CliResult, relative_to_repo};
use pytecode_archive::{JarFile, RewriteOptions};
use pytecode_engine::model::{
    ClassModel, CodeItem, DebugInfoPolicy, ExceptionHandler, FrameComputationMode, Label, LdcValue,
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
    /// Methods whose recomputed frames will likely fail JVM verification
    /// because part of the class hierarchy was unavailable.
    #[serde(default)]
    pub frame_warnings: Vec<String>,
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

    for entry in jar_file.entries.iter().filter(|entry| entry.is_class()) {
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
            .filter(|entry| !entry.metadata.is_dir && !entry.is_class())
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
        .filter(|entry| entry.is_class())
        .count();
    let resource_entries = jar_file
        .entries
        .iter()
        .filter(|entry| !entry.metadata.is_dir && !entry.is_class())
        .count();
    let resolver = crate::build_jar_class_resolver(&jar_file)?;
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
    let prepared = jar_file.prepare_rewrite(
        Some(output),
        Some(&mut transform as &mut dyn ApplyClassTransform),
        RewriteOptions {
            frame_mode: FrameComputationMode::Recompute,
            resolver: Some(&resolver),
            debug_info: DebugInfoPolicy::Preserve,
        },
    )?;
    let frame_warnings = prepared
        .frame_warnings()
        .iter()
        .map(ToString::to_string)
        .collect();
    prepared.commit(&mut jar_file)?;

    Ok(DeobfuscationRewriteReport {
        input_jar: relative_to_repo(jar),
        output_jar: relative_to_repo(output),
        class_entries,
        resource_entries,
        classes_changed: rewrite_stats.classes_changed,
        nops_removed: rewrite_stats.nops_removed,
        noop_gotos_removed: rewrite_stats.noop_gotos_removed,
        goto_redirects: rewrite_stats.goto_redirects,
        frame_warnings,
    })
}

fn class_metrics_from_model(model: &ClassModel, byte_len: usize) -> ClassMetrics {
    let mut readable_strings = Vec::new();
    let mut seen_strings = HashSet::new();
    let mut branch_count = 0_usize;
    let mut nop_count = 0_usize;

    for method in &model.methods {
        let Some(code) = method.code() else {
            continue;
        };
        for item in &code.instructions {
            match item {
                CodeItem::Ldc(insn) => {
                    if let LdcValue::String(value) = &insn.value
                        && let Ok(value) = value.to_unicode()
                        && is_readable_string(&value)
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
        let Some(code) = method.code_mut() else {
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

fn can_safely_remove_index(
    instructions: &[CodeItem],
    exception_handlers: &[ExceptionHandler],
    idx: usize,
) -> bool {
    let has_following_insn = instructions[idx + 1..]
        .iter()
        .any(|item| !matches!(item, CodeItem::Label(_)));
    if !has_following_insn {
        return false;
    }
    for handler in exception_handlers {
        let Some(start_pos) = instructions
            .iter()
            .position(|item| matches!(item, CodeItem::Label(label) if *label == handler.start))
        else {
            continue;
        };
        let Some(end_pos) = instructions
            .iter()
            .position(|item| matches!(item, CodeItem::Label(label) if *label == handler.end))
        else {
            continue;
        };
        if start_pos < idx && idx < end_pos {
            let has_other_insn =
                instructions[start_pos + 1..end_pos]
                    .iter()
                    .enumerate()
                    .any(|(offset, item)| {
                        start_pos + 1 + offset != idx && !matches!(item, CodeItem::Label(_))
                    });
            if !has_other_insn {
                return false;
            }
        }
    }
    true
}

fn remove_nops(code: &mut pytecode_engine::model::CodeModel) -> usize {
    let mut removed = 0_usize;
    let mut index = 0_usize;
    while index < code.instructions.len() {
        let is_nop = matches!(
            &code.instructions[index],
            CodeItem::Raw(pytecode_engine::raw::Instruction::Simple { opcode: 0, .. })
        );
        if is_nop && can_safely_remove_index(&code.instructions, &code.exception_handlers, index) {
            code.instructions.remove(index);
            removed += 1;
        } else {
            index += 1;
        }
    }
    removed
}

fn remove_noop_gotos(code: &mut pytecode_engine::model::CodeModel) -> usize {
    let mut removed = 0_usize;
    let mut index = 0_usize;
    while index < code.instructions.len() {
        let is_noop_goto = matches!(
            &code.instructions[index],
            CodeItem::Branch(branch)
                if is_unconditional_goto(branch.opcode)
                    && targets_immediate_fallthrough(&code.instructions, index, &branch.target)
        );
        if is_noop_goto
            && can_safely_remove_index(&code.instructions, &code.exception_handlers, index)
        {
            code.instructions.remove(index);
            removed += 1;
        } else {
            index += 1;
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use pytecode_engine::constants::{ClassAccessFlags, MethodAccessFlags};
    use pytecode_engine::model::{CodeModel, ConstantPoolBuilder, DebugInfoState, MethodModel};
    use pytecode_engine::raw::Instruction;

    #[test]
    fn rewrite_class_preserves_trailing_nop_and_sole_handler_nop() {
        let try_start = Label::named("try_start");
        let try_end = Label::named("try_end");
        let handler_lbl = Label::named("handler");
        let mut handler_code = CodeModel::new(1, 1, DebugInfoState::Fresh);
        handler_code.instructions = vec![
            CodeItem::Label(try_start.clone()),
            CodeItem::Raw(Instruction::Simple {
                opcode: 0,
                offset: 0,
            }),
            CodeItem::Raw(Instruction::Simple {
                opcode: 0,
                offset: 0,
            }),
            CodeItem::Label(try_end.clone()),
            CodeItem::Raw(Instruction::Simple {
                opcode: 0xB1,
                offset: 0,
            }),
            CodeItem::Label(handler_lbl.clone()),
            CodeItem::Raw(Instruction::Simple {
                opcode: 0xBF,
                offset: 0,
            }),
        ];
        handler_code.exception_handlers.push(ExceptionHandler {
            start: try_start,
            end: try_end,
            handler: handler_lbl,
            catch_type: None,
        });

        let end_lbl = Label::named("end_lbl");
        let mut trailing_code = CodeModel::new(0, 1, DebugInfoState::Fresh);
        trailing_code.instructions = vec![
            CodeItem::Raw(Instruction::Simple {
                opcode: 0xB1,
                offset: 0,
            }),
            CodeItem::Label(end_lbl),
            CodeItem::Raw(Instruction::Simple {
                opcode: 0,
                offset: 0,
            }),
        ];

        let mut model = ClassModel {
            entry_name: "Test.class".to_owned(),
            original_byte_len: 0,
            version: (52, 0),
            constant_pool: ConstantPoolBuilder::new(),
            access_flags: ClassAccessFlags::PUBLIC | ClassAccessFlags::SUPER,
            name: "Test".to_owned(),
            super_name: Some("java/lang/Object".to_owned()),
            interfaces: vec![],
            debug_info_state: DebugInfoState::Fresh,
            fields: vec![],
            methods: vec![
                MethodModel::new(
                    MethodAccessFlags::PUBLIC | MethodAccessFlags::STATIC,
                    "handlerPair".to_owned(),
                    "()V".to_owned(),
                    Some(handler_code),
                    vec![],
                ),
                MethodModel::new(
                    MethodAccessFlags::PUBLIC | MethodAccessFlags::STATIC,
                    "trailing".to_owned(),
                    "()V".to_owned(),
                    Some(trailing_code),
                    vec![],
                ),
            ],
            attributes: vec![],
        };

        let stats = rewrite_class(&mut model);
        assert_eq!(stats.nops_removed, 1);

        let handler_nops = model.methods[0]
            .code()
            .unwrap()
            .instructions
            .iter()
            .filter(|item| matches!(item, CodeItem::Raw(Instruction::Simple { opcode: 0, .. })))
            .count();
        assert_eq!(handler_nops, 1);

        let trailing_nops = model.methods[1]
            .code()
            .unwrap()
            .instructions
            .iter()
            .filter(|item| matches!(item, CodeItem::Raw(Instruction::Simple { opcode: 0, .. })))
            .count();
        assert_eq!(trailing_nops, 1);
    }
}
