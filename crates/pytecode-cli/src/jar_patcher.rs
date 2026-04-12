use crate::{CliError, CliResult, relative_to_repo};
use pytecode_archive::{JarFile, RewriteOptions};
use pytecode_engine::constants::{ClassAccessFlags, FieldAccessFlags, MethodAccessFlags};
use pytecode_engine::model::{
    BranchInsn, ClassModel, CodeItem, CodeModel, FieldInsn, FieldModel, FrameComputationMode,
    IIncInsn, Label, LdcInsn, LdcValue, LookupSwitchInsn, MethodInsn, MethodModel, TableSwitchInsn,
    TypeInsn, VarInsn,
};
use pytecode_engine::raw::Instruction;
use pytecode_engine::transform::matcher_spec::InsnMatcherSpec;
use pytecode_engine::transform::transform_spec::CodeTransformSpec;
use pytecode_engine::transform::{
    ClassMatcher, FieldMatcher, Matcher, MethodMatcher, Pipeline, all_of, class_access,
    class_access_any, class_name_matches, class_named, extends, field_access, field_access_any,
    field_descriptor, field_name_matches, field_named, implements, method_access,
    method_access_any, method_descriptor, method_name_matches, method_named, not_,
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PatchDebugInfoMode {
    #[default]
    Preserve,
    Strip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PatchFrameMode {
    #[default]
    Preserve,
    Recompute,
}

impl From<PatchDebugInfoMode> for pytecode_engine::model::DebugInfoPolicy {
    fn from(value: PatchDebugInfoMode) -> Self {
        match value {
            PatchDebugInfoMode::Preserve => Self::Preserve,
            PatchDebugInfoMode::Strip => Self::Strip,
        }
    }
}

impl From<PatchFrameMode> for FrameComputationMode {
    fn from(value: PatchFrameMode) -> Self {
        match value {
            PatchFrameMode::Preserve => Self::Preserve,
            PatchFrameMode::Recompute => Self::Recompute,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct PatchPlan {
    #[serde(default)]
    options: PatchOptions,
    #[serde(default)]
    rules: Vec<PatchRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct PatchOptions {
    #[serde(default)]
    debug_info: PatchDebugInfoMode,
    #[serde(default)]
    frame_mode: PatchFrameMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum PatchRule {
    Class {
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        matcher: ClassRuleMatcher,
        action: ClassAction,
    },
    Field {
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        owner: ClassRuleMatcher,
        #[serde(default)]
        matcher: FieldRuleMatcher,
        action: FieldAction,
    },
    Method {
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        owner: ClassRuleMatcher,
        #[serde(default)]
        matcher: MethodRuleMatcher,
        #[serde(default)]
        action: Option<MethodAction>,
        #[serde(default)]
        code_actions: Vec<CodeAction>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct ClassRuleMatcher {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    name_matches: Option<String>,
    #[serde(default)]
    access_all: Vec<ClassAccessFlagName>,
    #[serde(default)]
    access_any: Vec<ClassAccessFlagName>,
    #[serde(default)]
    package_private: Option<bool>,
    #[serde(default)]
    extends: Option<String>,
    #[serde(default)]
    implements: Vec<String>,
    #[serde(default)]
    version: Option<u16>,
    #[serde(default)]
    version_at_least: Option<u16>,
    #[serde(default)]
    version_below: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct FieldRuleMatcher {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    name_matches: Option<String>,
    #[serde(default)]
    descriptor: Option<String>,
    #[serde(default)]
    descriptor_matches: Option<String>,
    #[serde(default)]
    access_all: Vec<FieldAccessFlagName>,
    #[serde(default)]
    access_any: Vec<FieldAccessFlagName>,
    #[serde(default)]
    package_private: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct MethodRuleMatcher {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    name_matches: Option<String>,
    #[serde(default)]
    descriptor: Option<String>,
    #[serde(default)]
    descriptor_matches: Option<String>,
    #[serde(default)]
    access_all: Vec<MethodAccessFlagName>,
    #[serde(default)]
    access_any: Vec<MethodAccessFlagName>,
    #[serde(default)]
    package_private: Option<bool>,
    #[serde(default)]
    has_code: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ClassAccessFlagName {
    Public,
    Final,
    Super,
    Interface,
    Abstract,
    Synthetic,
    Annotation,
    Enum,
    Module,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FieldAccessFlagName {
    Public,
    Private,
    Protected,
    Static,
    Final,
    Volatile,
    Transient,
    Synthetic,
    Enum,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum MethodAccessFlagName {
    Public,
    Private,
    Protected,
    Static,
    Final,
    Synchronized,
    Bridge,
    Varargs,
    Native,
    Abstract,
    Strict,
    Synthetic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
enum ClassAction {
    AddAccessFlags { flags: Vec<ClassAccessFlagName> },
    RemoveAccessFlags { flags: Vec<ClassAccessFlagName> },
    SetAccessFlags { flags: Vec<ClassAccessFlagName> },
    SetSuperClass { to: Option<String> },
    AddInterface { name: String },
    RemoveInterface { name: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
enum FieldAction {
    AddAccessFlags { flags: Vec<FieldAccessFlagName> },
    RemoveAccessFlags { flags: Vec<FieldAccessFlagName> },
    SetAccessFlags { flags: Vec<FieldAccessFlagName> },
    Rename { to: String },
    Remove,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
enum MethodAction {
    AddAccessFlags { flags: Vec<MethodAccessFlagName> },
    RemoveAccessFlags { flags: Vec<MethodAccessFlagName> },
    SetAccessFlags { flags: Vec<MethodAccessFlagName> },
    Rename { to: String },
    Remove,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
enum CodeAction {
    ReplaceInsn {
        matcher: CodeInsnMatcher,
        replacement: Vec<CodeReplacementItem>,
    },
    RemoveInsn {
        opcode: u8,
    },
    InsertBefore {
        matcher: CodeInsnMatcher,
        items: Vec<CodeReplacementItem>,
    },
    InsertAfter {
        matcher: CodeInsnMatcher,
        items: Vec<CodeReplacementItem>,
    },
    RemoveSequence {
        pattern: Vec<CodeInsnMatcher>,
    },
    ReplaceSequence {
        pattern: Vec<CodeInsnMatcher>,
        replacement: Vec<CodeReplacementItem>,
    },
    RedirectMethodCall {
        from_owner: String,
        from_name: String,
        from_descriptor: String,
        to_owner: String,
        to_name: String,
        #[serde(default)]
        to_descriptor: Option<String>,
    },
    RedirectFieldAccess {
        from_owner: String,
        from_name: String,
        to_owner: String,
        to_name: String,
    },
    ReplaceString {
        from: String,
        to: String,
    },
    Sequence {
        actions: Vec<CodeAction>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct CodeInsnMatcher {
    #[serde(default)]
    opcode: Option<u8>,
    #[serde(default)]
    opcode_any: Vec<u8>,
    #[serde(default)]
    is_field_access: bool,
    #[serde(default)]
    is_method_call: bool,
    #[serde(default)]
    is_return: bool,
    #[serde(default)]
    is_branch: bool,
    #[serde(default)]
    is_label: bool,
    #[serde(default)]
    is_ldc: bool,
    #[serde(default)]
    is_var: bool,
    #[serde(default)]
    field_owner: Option<String>,
    #[serde(default)]
    field_name: Option<String>,
    #[serde(default)]
    field_descriptor: Option<String>,
    #[serde(default)]
    method_owner: Option<String>,
    #[serde(default)]
    method_name: Option<String>,
    #[serde(default)]
    method_descriptor: Option<String>,
    #[serde(default)]
    type_descriptor: Option<String>,
    #[serde(default)]
    ldc_string: Option<String>,
    #[serde(default)]
    var_slot: Option<u16>,
    #[serde(default)]
    field_owner_matches: Option<String>,
    #[serde(default)]
    method_owner_matches: Option<String>,
    #[serde(default)]
    method_name_matches: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
enum CodeReplacementItem {
    Raw {
        opcode: u8,
    },
    Label {
        name: String,
    },
    LdcString {
        value: String,
    },
    Field {
        opcode: u8,
        owner: String,
        name: String,
        descriptor: String,
    },
    Method {
        opcode: u8,
        owner: String,
        name: String,
        descriptor: String,
        #[serde(default)]
        is_interface: bool,
    },
    Type {
        opcode: u8,
        descriptor: String,
    },
    Var {
        opcode: u8,
        slot: u16,
    },
    Iinc {
        slot: u16,
        value: i16,
    },
    Branch {
        opcode: u8,
        target: String,
    },
    LookupSwitch {
        default_target: String,
        pairs: Vec<CodeSwitchPair>,
    },
    TableSwitch {
        default_target: String,
        low: i32,
        high: i32,
        targets: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CodeSwitchPair {
    key: i32,
    target: String,
}

#[derive(Debug, Clone)]
enum CompiledCodeAction {
    Single {
        matcher: InsnMatcherSpec,
        transform: CodeTransformSpec,
    },
    Sequence {
        pattern: Vec<InsnMatcherSpec>,
        transform: CodeTransformSpec,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JarPatchRuleReport {
    pub name: String,
    pub kind: String,
    pub matched_classes: usize,
    pub changed_classes: usize,
    pub matched_targets: usize,
    pub changed_targets: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JarPatchReport {
    pub input_jar: String,
    pub output_jar: String,
    pub rules_path: String,
    pub class_entries: usize,
    pub resource_entries: usize,
    pub debug_info: PatchDebugInfoMode,
    pub frame_mode: PatchFrameMode,
    pub rules: Vec<JarPatchRuleReport>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct RuleStats {
    matched_classes: usize,
    changed_classes: usize,
    matched_targets: usize,
    changed_targets: usize,
}

pub fn patch_jar(jar: &Path, output: &Path, rules_path: &Path) -> CliResult<JarPatchReport> {
    let plan = load_patch_plan(rules_path)?;
    let mut jar_file = JarFile::open(jar)?;
    let (class_entries, resource_entries) = {
        let (classes, resources) = jar_file.parse_classes();
        (classes.len(), resources.len())
    };
    let (mut pipeline, stats_handles, names, kinds) = build_pipeline(&plan)?;
    let transform = if plan.rules.is_empty() {
        None
    } else {
        Some(&mut pipeline as &mut dyn pytecode_engine::transform::ApplyClassTransform)
    };
    jar_file.rewrite(
        Some(output),
        transform,
        RewriteOptions {
            frame_mode: plan.options.frame_mode.into(),
            resolver: None,
            debug_info: plan.options.debug_info.into(),
        },
    )?;
    let rules = stats_handles
        .iter()
        .enumerate()
        .map(|(index, stats)| {
            let snapshot = *stats
                .lock()
                .expect("rule stats mutex should not be poisoned");
            JarPatchRuleReport {
                name: names[index].clone(),
                kind: kinds[index].clone(),
                matched_classes: snapshot.matched_classes,
                changed_classes: snapshot.changed_classes,
                matched_targets: snapshot.matched_targets,
                changed_targets: snapshot.changed_targets,
            }
        })
        .collect();
    Ok(JarPatchReport {
        input_jar: relative_to_repo(jar),
        output_jar: relative_to_repo(output),
        rules_path: relative_to_repo(rules_path),
        class_entries,
        resource_entries,
        debug_info: plan.options.debug_info,
        frame_mode: plan.options.frame_mode,
        rules,
    })
}

fn load_patch_plan(path: &Path) -> CliResult<PatchPlan> {
    let contents = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&contents)?)
}

type PipelineBuild = (
    Pipeline,
    Vec<Arc<Mutex<RuleStats>>>,
    Vec<String>,
    Vec<String>,
);

fn build_pipeline(plan: &PatchPlan) -> CliResult<PipelineBuild> {
    let mut pipeline = Pipeline::new();
    let mut stats_handles = Vec::with_capacity(plan.rules.len());
    let mut names = Vec::with_capacity(plan.rules.len());
    let mut kinds = Vec::with_capacity(plan.rules.len());
    for (index, rule) in plan.rules.iter().enumerate() {
        let stats = Arc::new(Mutex::new(RuleStats::default()));
        let name = rule
            .display_name(index)
            .unwrap_or_else(|| format!("rule-{}", index + 1));
        match rule {
            PatchRule::Class {
                matcher, action, ..
            } => {
                let matcher = build_class_matcher(matcher)?;
                let action = action.clone();
                let stats_ref = Arc::clone(&stats);
                pipeline = pipeline.then(move |model: &mut ClassModel| {
                    if !matcher.matches(model) {
                        return Ok(());
                    }
                    let changed = apply_class_action(model, &action);
                    let mut stats = stats_ref
                        .lock()
                        .expect("rule stats mutex should not be poisoned");
                    stats.matched_classes += 1;
                    stats.matched_targets += 1;
                    if changed {
                        stats.changed_classes += 1;
                        stats.changed_targets += 1;
                    }
                    Ok(())
                });
                kinds.push("class".to_owned());
            }
            PatchRule::Field {
                owner,
                matcher,
                action,
                ..
            } => {
                let owner_matcher = build_class_matcher(owner)?;
                let field_matcher = build_field_matcher(matcher)?;
                let action = action.clone();
                let stats_ref = Arc::clone(&stats);
                pipeline = pipeline.then(move |model: &mut ClassModel| {
                    if !owner_matcher.matches(model) {
                        return Ok(());
                    }
                    let (matched_targets, changed_targets) =
                        apply_field_action(model, &field_matcher, &action);
                    if matched_targets == 0 {
                        return Ok(());
                    }
                    let mut stats = stats_ref
                        .lock()
                        .expect("rule stats mutex should not be poisoned");
                    stats.matched_classes += 1;
                    stats.matched_targets += matched_targets;
                    if changed_targets > 0 {
                        stats.changed_classes += 1;
                        stats.changed_targets += changed_targets;
                    }
                    Ok(())
                });
                kinds.push("field".to_owned());
            }
            PatchRule::Method {
                owner,
                matcher,
                action,
                code_actions,
                ..
            } => {
                let owner_matcher = build_class_matcher(owner)?;
                let method_matcher = build_method_matcher(matcher)?;
                let action = action.clone();
                let code_actions = code_actions
                    .iter()
                    .map(build_compiled_code_actions)
                    .collect::<CliResult<Vec<_>>>()?
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>();
                let stats_ref = Arc::clone(&stats);
                pipeline = pipeline.then(move |model: &mut ClassModel| {
                    if !owner_matcher.matches(model) {
                        return Ok(());
                    }
                    let (matched_targets, changed_targets) = apply_method_actions(
                        model,
                        &method_matcher,
                        action.as_ref(),
                        &code_actions,
                    );
                    if matched_targets == 0 {
                        return Ok(());
                    }
                    let mut stats = stats_ref
                        .lock()
                        .expect("rule stats mutex should not be poisoned");
                    stats.matched_classes += 1;
                    stats.matched_targets += matched_targets;
                    if changed_targets > 0 {
                        stats.changed_classes += 1;
                        stats.changed_targets += changed_targets;
                    }
                    Ok(())
                });
                kinds.push("method".to_owned());
            }
        }
        stats_handles.push(stats);
        names.push(name);
    }
    Ok((pipeline, stats_handles, names, kinds))
}

impl PatchRule {
    fn display_name(&self, index: usize) -> Option<String> {
        match self {
            Self::Class { name, .. } | Self::Field { name, .. } | Self::Method { name, .. } => {
                name.clone().or_else(|| Some(format!("rule-{}", index + 1)))
            }
        }
    }
}

fn build_class_matcher(config: &ClassRuleMatcher) -> CliResult<ClassMatcher> {
    let mut matchers = Vec::new();
    if let Some(name) = &config.name {
        matchers.push(class_named(name.clone()));
    }
    if let Some(pattern) = &config.name_matches {
        matchers.push(class_name_matches(pattern)?);
    }
    if !config.access_all.is_empty() {
        matchers.push(class_access(class_flags(&config.access_all)));
    }
    if !config.access_any.is_empty() {
        matchers.push(class_access_any(class_flags(&config.access_any)));
    }
    if let Some(package_private) = config.package_private {
        let visibility = class_access_any(ClassAccessFlags::PUBLIC);
        matchers.push(if package_private {
            not_(visibility)
        } else {
            visibility
        });
    }
    if let Some(super_name) = &config.extends {
        matchers.push(extends(super_name.clone()));
    }
    for interface_name in &config.implements {
        matchers.push(implements(interface_name.clone()));
    }
    if let Some(version) = config.version {
        matchers.push(Matcher::of(
            move |model: &ClassModel| model.version.0 == version,
            format!("class_version({version})"),
        ));
    }
    if let Some(version) = config.version_at_least {
        matchers.push(Matcher::of(
            move |model: &ClassModel| model.version.0 >= version,
            format!("class_version_at_least({version})"),
        ));
    }
    if let Some(version) = config.version_below {
        matchers.push(Matcher::of(
            move |model: &ClassModel| model.version.0 < version,
            format!("class_version_below({version})"),
        ));
    }
    Ok(all_of(matchers))
}

fn build_field_matcher(config: &FieldRuleMatcher) -> CliResult<FieldMatcher> {
    let mut matchers = Vec::new();
    if let Some(name) = &config.name {
        matchers.push(field_named(name.clone()));
    }
    if let Some(pattern) = &config.name_matches {
        matchers.push(field_name_matches(pattern)?);
    }
    if let Some(descriptor) = &config.descriptor {
        matchers.push(field_descriptor(descriptor.clone()));
    }
    if let Some(pattern) = &config.descriptor_matches {
        let regex = Regex::new(pattern)?;
        matchers.push(Matcher::of(
            move |field: &FieldModel| regex.is_match(&field.descriptor),
            format!("field_descriptor_matches({pattern:?})"),
        ));
    }
    if !config.access_all.is_empty() {
        matchers.push(field_access(field_flags(&config.access_all)));
    }
    if !config.access_any.is_empty() {
        matchers.push(field_access_any(field_flags(&config.access_any)));
    }
    if let Some(package_private) = config.package_private {
        let visibility = field_access_any(
            FieldAccessFlags::PUBLIC | FieldAccessFlags::PRIVATE | FieldAccessFlags::PROTECTED,
        );
        matchers.push(if package_private {
            not_(visibility)
        } else {
            visibility
        });
    }
    Ok(all_of(matchers))
}

fn build_method_matcher(config: &MethodRuleMatcher) -> CliResult<MethodMatcher> {
    let mut matchers = Vec::new();
    if let Some(name) = &config.name {
        matchers.push(method_named(name.clone()));
    }
    if let Some(pattern) = &config.name_matches {
        matchers.push(method_name_matches(pattern)?);
    }
    if let Some(descriptor) = &config.descriptor {
        matchers.push(method_descriptor(descriptor.clone()));
    }
    if let Some(pattern) = &config.descriptor_matches {
        let regex = Regex::new(pattern)?;
        matchers.push(Matcher::of(
            move |method: &MethodModel| regex.is_match(&method.descriptor),
            format!("method_descriptor_matches({pattern:?})"),
        ));
    }
    if !config.access_all.is_empty() {
        matchers.push(method_access(method_flags(&config.access_all)));
    }
    if !config.access_any.is_empty() {
        matchers.push(method_access_any(method_flags(&config.access_any)));
    }
    if let Some(package_private) = config.package_private {
        let visibility = method_access_any(
            MethodAccessFlags::PUBLIC | MethodAccessFlags::PRIVATE | MethodAccessFlags::PROTECTED,
        );
        matchers.push(if package_private {
            not_(visibility)
        } else {
            visibility
        });
    }
    if let Some(has_code) = config.has_code {
        matchers.push(Matcher::of(
            move |method: &MethodModel| method.code.is_some() == has_code,
            format!("method_has_code({has_code})"),
        ));
    }
    Ok(all_of(matchers))
}

fn build_code_insn_matcher(config: &CodeInsnMatcher) -> CliResult<InsnMatcherSpec> {
    let mut matchers = Vec::new();
    if let Some(opcode) = config.opcode {
        matchers.push(InsnMatcherSpec::Opcode(opcode));
    }
    if !config.opcode_any.is_empty() {
        matchers.push(InsnMatcherSpec::OpcodeAny(config.opcode_any.clone()));
    }
    if config.is_field_access {
        matchers.push(InsnMatcherSpec::IsFieldAccess);
    }
    if config.is_method_call {
        matchers.push(InsnMatcherSpec::IsMethodCall);
    }
    if config.is_return {
        matchers.push(InsnMatcherSpec::IsReturn);
    }
    if config.is_branch {
        matchers.push(InsnMatcherSpec::IsBranch);
    }
    if config.is_label {
        matchers.push(InsnMatcherSpec::IsLabel);
    }
    if config.is_ldc {
        matchers.push(InsnMatcherSpec::IsLdc);
    }
    if config.is_var {
        matchers.push(InsnMatcherSpec::IsVar);
    }
    if let Some(owner) = &config.field_owner {
        matchers.push(InsnMatcherSpec::FieldOwner(owner.clone()));
    }
    if let Some(name) = &config.field_name {
        matchers.push(InsnMatcherSpec::FieldNamed(name.clone()));
    }
    if let Some(descriptor) = &config.field_descriptor {
        matchers.push(InsnMatcherSpec::FieldDescriptor(descriptor.clone()));
    }
    if let Some(owner) = &config.method_owner {
        matchers.push(InsnMatcherSpec::MethodOwner(owner.clone()));
    }
    if let Some(name) = &config.method_name {
        matchers.push(InsnMatcherSpec::MethodNamed(name.clone()));
    }
    if let Some(descriptor) = &config.method_descriptor {
        matchers.push(InsnMatcherSpec::MethodDescriptor(descriptor.clone()));
    }
    if let Some(descriptor) = &config.type_descriptor {
        matchers.push(InsnMatcherSpec::TypeDescriptor(descriptor.clone()));
    }
    if let Some(value) = &config.ldc_string {
        matchers.push(InsnMatcherSpec::LdcString(value.clone()));
    }
    if let Some(slot) = config.var_slot {
        matchers.push(InsnMatcherSpec::VarSlot(slot));
    }
    if let Some(pattern) = &config.field_owner_matches {
        Regex::new(pattern)?;
        matchers.push(InsnMatcherSpec::FieldOwnerMatches(pattern.clone()));
    }
    if let Some(pattern) = &config.method_owner_matches {
        Regex::new(pattern)?;
        matchers.push(InsnMatcherSpec::MethodOwnerMatches(pattern.clone()));
    }
    if let Some(pattern) = &config.method_name_matches {
        Regex::new(pattern)?;
        matchers.push(InsnMatcherSpec::MethodNameMatches(pattern.clone()));
    }
    Ok(match matchers.len() {
        0 => InsnMatcherSpec::Any,
        1 => matchers.into_iter().next().expect("single matcher exists"),
        _ => InsnMatcherSpec::And(matchers),
    })
}

fn build_code_replacement_items(items: &[CodeReplacementItem]) -> CliResult<Vec<CodeItem>> {
    let mut labels = HashMap::new();
    for item in items {
        if let CodeReplacementItem::Label { name } = item {
            if labels.contains_key(name) {
                return Err(CliError::InvalidPatchPlan {
                    message: format!("duplicate replacement label '{name}'"),
                });
            }
            labels.insert(name.clone(), Label::named(name.clone()));
        }
    }

    items
        .iter()
        .map(|item| build_code_replacement_item(item, &labels))
        .collect()
}

fn build_code_replacement_item(
    item: &CodeReplacementItem,
    labels: &HashMap<String, Label>,
) -> CliResult<CodeItem> {
    Ok(match item {
        CodeReplacementItem::Raw { opcode } => CodeItem::Raw(Instruction::Simple {
            opcode: *opcode,
            offset: 0,
        }),
        CodeReplacementItem::Label { name } => {
            let Some(label) = labels.get(name) else {
                return Err(CliError::InvalidPatchPlan {
                    message: format!("replacement label '{name}' was not declared"),
                });
            };
            CodeItem::Label(label.clone())
        }
        CodeReplacementItem::LdcString { value } => CodeItem::Ldc(LdcInsn {
            value: LdcValue::String(value.clone()),
        }),
        CodeReplacementItem::Field {
            opcode,
            owner,
            name,
            descriptor,
        } => CodeItem::Field(FieldInsn {
            opcode: *opcode,
            owner: owner.clone(),
            name: name.clone(),
            descriptor: descriptor.clone(),
        }),
        CodeReplacementItem::Method {
            opcode,
            owner,
            name,
            descriptor,
            is_interface,
        } => CodeItem::Method(MethodInsn {
            opcode: *opcode,
            owner: owner.clone(),
            name: name.clone(),
            descriptor: descriptor.clone(),
            is_interface: *is_interface,
        }),
        CodeReplacementItem::Type { opcode, descriptor } => CodeItem::Type(TypeInsn {
            opcode: *opcode,
            descriptor: descriptor.clone(),
        }),
        CodeReplacementItem::Var { opcode, slot } => CodeItem::Var(VarInsn {
            opcode: *opcode,
            slot: *slot,
        }),
        CodeReplacementItem::Iinc { slot, value } => CodeItem::IInc(IIncInsn {
            slot: *slot,
            value: *value,
        }),
        CodeReplacementItem::Branch { opcode, target } => CodeItem::Branch(BranchInsn {
            opcode: *opcode,
            target: lookup_label(labels, target)?,
        }),
        CodeReplacementItem::LookupSwitch {
            default_target,
            pairs,
        } => CodeItem::LookupSwitch(LookupSwitchInsn {
            default_target: lookup_label(labels, default_target)?,
            pairs: pairs
                .iter()
                .map(|pair| Ok((pair.key, lookup_label(labels, &pair.target)?)))
                .collect::<CliResult<Vec<_>>>()?,
        }),
        CodeReplacementItem::TableSwitch {
            default_target,
            low,
            high,
            targets,
        } => {
            let width = (*high)
                .checked_sub(*low)
                .and_then(|delta| delta.checked_add(1))
                .ok_or_else(|| CliError::InvalidPatchPlan {
                    message: format!("invalid table-switch range {low}..={high}"),
                })? as usize;
            if targets.len() != width {
                return Err(CliError::InvalidPatchPlan {
                    message: format!(
                        "table-switch target count {} does not match range {low}..={high}",
                        targets.len()
                    ),
                });
            }
            CodeItem::TableSwitch(TableSwitchInsn {
                default_target: lookup_label(labels, default_target)?,
                low: *low,
                high: *high,
                targets: targets
                    .iter()
                    .map(|target| lookup_label(labels, target))
                    .collect::<CliResult<Vec<_>>>()?,
            })
        }
    })
}

fn lookup_label(labels: &HashMap<String, Label>, name: &str) -> CliResult<Label> {
    labels
        .get(name)
        .cloned()
        .ok_or_else(|| CliError::InvalidPatchPlan {
            message: format!("replacement branch target '{name}' was not declared"),
        })
}

fn build_compiled_code_actions(action: &CodeAction) -> CliResult<Vec<CompiledCodeAction>> {
    Ok(match action {
        CodeAction::ReplaceInsn {
            matcher,
            replacement,
        } => {
            let matcher = build_code_insn_matcher(matcher)?;
            vec![CompiledCodeAction::Single {
                matcher: matcher.clone(),
                transform: CodeTransformSpec::ReplaceInsn {
                    matcher,
                    replacement: build_code_replacement_items(replacement)?,
                },
            }]
        }
        CodeAction::RemoveInsn { opcode } => vec![CompiledCodeAction::Single {
            matcher: InsnMatcherSpec::Opcode(*opcode),
            transform: CodeTransformSpec::RemoveInsn {
                matcher: InsnMatcherSpec::Opcode(*opcode),
            },
        }],
        CodeAction::InsertBefore { matcher, items } => {
            if items.is_empty() {
                return Err(CliError::InvalidPatchPlan {
                    message: "insert-before items must not be empty".to_owned(),
                });
            }
            let matcher = build_code_insn_matcher(matcher)?;
            vec![CompiledCodeAction::Single {
                matcher: matcher.clone(),
                transform: CodeTransformSpec::InsertBefore {
                    matcher,
                    items: build_code_replacement_items(items)?,
                },
            }]
        }
        CodeAction::InsertAfter { matcher, items } => {
            if items.is_empty() {
                return Err(CliError::InvalidPatchPlan {
                    message: "insert-after items must not be empty".to_owned(),
                });
            }
            let matcher = build_code_insn_matcher(matcher)?;
            vec![CompiledCodeAction::Single {
                matcher: matcher.clone(),
                transform: CodeTransformSpec::InsertAfter {
                    matcher,
                    items: build_code_replacement_items(items)?,
                },
            }]
        }
        CodeAction::RemoveSequence { pattern } => {
            let pattern = pattern
                .iter()
                .map(build_code_insn_matcher)
                .collect::<CliResult<Vec<_>>>()?;
            if pattern.is_empty() {
                return Err(CliError::InvalidPatchPlan {
                    message: "remove-sequence pattern must not be empty".to_owned(),
                });
            }
            vec![CompiledCodeAction::Sequence {
                transform: CodeTransformSpec::RemoveSequence {
                    pattern: pattern.clone(),
                },
                pattern,
            }]
        }
        CodeAction::ReplaceSequence {
            pattern,
            replacement,
        } => {
            let pattern = pattern
                .iter()
                .map(build_code_insn_matcher)
                .collect::<CliResult<Vec<_>>>()?;
            if pattern.is_empty() {
                return Err(CliError::InvalidPatchPlan {
                    message: "replace-sequence pattern must not be empty".to_owned(),
                });
            }
            vec![CompiledCodeAction::Sequence {
                transform: CodeTransformSpec::ReplaceSequence {
                    pattern: pattern.clone(),
                    replacement: build_code_replacement_items(replacement)?,
                },
                pattern,
            }]
        }
        CodeAction::RedirectMethodCall {
            from_owner,
            from_name,
            from_descriptor,
            to_owner,
            to_name,
            to_descriptor,
        } => vec![CompiledCodeAction::Single {
            matcher: InsnMatcherSpec::Method {
                owner: from_owner.clone(),
                name: from_name.clone(),
                descriptor: from_descriptor.clone(),
            },
            transform: CodeTransformSpec::RedirectMethodCall {
                from_owner: from_owner.clone(),
                from_name: from_name.clone(),
                from_descriptor: from_descriptor.clone(),
                to_owner: to_owner.clone(),
                to_name: to_name.clone(),
                to_descriptor: to_descriptor.clone(),
            },
        }],
        CodeAction::RedirectFieldAccess {
            from_owner,
            from_name,
            to_owner,
            to_name,
        } => vec![CompiledCodeAction::Single {
            matcher: InsnMatcherSpec::And(vec![
                InsnMatcherSpec::IsFieldAccess,
                InsnMatcherSpec::FieldOwner(from_owner.clone()),
                InsnMatcherSpec::FieldNamed(from_name.clone()),
            ]),
            transform: CodeTransformSpec::RedirectFieldAccess {
                from_owner: from_owner.clone(),
                from_name: from_name.clone(),
                to_owner: to_owner.clone(),
                to_name: to_name.clone(),
            },
        }],
        CodeAction::ReplaceString { from, to } => vec![CompiledCodeAction::Single {
            matcher: InsnMatcherSpec::LdcString(from.clone()),
            transform: CodeTransformSpec::ReplaceString {
                from: from.clone(),
                to: to.clone(),
            },
        }],
        CodeAction::Sequence { actions } => actions
            .iter()
            .map(build_compiled_code_actions)
            .collect::<CliResult<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>(),
    })
}

fn compile_insn_matcher(spec: &InsnMatcherSpec) -> Matcher<CodeItem> {
    let compiled = spec.compile();
    Matcher::of(
        move |item| compiled.matches(item),
        "compiled_cli_insn_matcher",
    )
}

fn count_matching_insns(code: &CodeModel, matcher: &InsnMatcherSpec) -> usize {
    let matcher = compile_insn_matcher(matcher);
    code.instructions
        .iter()
        .filter(|item| matcher.matches(item))
        .count()
}

fn count_matching_sequences(code: &CodeModel, pattern: &[InsnMatcherSpec]) -> usize {
    if pattern.is_empty() || pattern.len() > code.instructions.len() {
        return 0;
    }
    let pattern = pattern.iter().map(compile_insn_matcher).collect::<Vec<_>>();
    let mut matches = 0_usize;
    let mut index = 0_usize;
    while index + pattern.len() <= code.instructions.len() {
        let window = &code.instructions[index..index + pattern.len()];
        if pattern
            .iter()
            .zip(window.iter())
            .all(|(matcher, item)| matcher.matches(item))
        {
            matches += 1;
            index += pattern.len();
        } else {
            index += 1;
        }
    }
    matches
}

fn apply_class_action(model: &mut ClassModel, action: &ClassAction) -> bool {
    match action {
        ClassAction::AddAccessFlags { flags } => {
            let flags = class_flags(flags);
            let changed = !model.access_flags.contains(flags);
            model.access_flags |= flags;
            changed
        }
        ClassAction::RemoveAccessFlags { flags } => {
            let flags = class_flags(flags);
            let changed = model.access_flags.intersects(flags);
            model.access_flags.remove(flags);
            changed
        }
        ClassAction::SetAccessFlags { flags } => {
            let new_flags = class_flags(flags);
            let changed = model.access_flags != new_flags;
            model.access_flags = new_flags;
            changed
        }
        ClassAction::SetSuperClass { to } => {
            let changed = model.super_name != *to;
            model.super_name = to.clone();
            changed
        }
        ClassAction::AddInterface { name } => {
            if model.interfaces.iter().any(|entry| entry == name) {
                false
            } else {
                model.interfaces.push(name.clone());
                true
            }
        }
        ClassAction::RemoveInterface { name } => {
            let before = model.interfaces.len();
            model.interfaces.retain(|entry| entry != name);
            before != model.interfaces.len()
        }
    }
}

fn apply_field_action(
    model: &mut ClassModel,
    matcher: &FieldMatcher,
    action: &FieldAction,
) -> (usize, usize) {
    match action {
        FieldAction::Remove => {
            let mut matched = 0_usize;
            model.fields.retain(|field| {
                let keep = !matcher.matches(field);
                if !keep {
                    matched += 1;
                }
                keep
            });
            (matched, matched)
        }
        FieldAction::Rename { to } => {
            let mut matched = 0_usize;
            let mut changed = 0_usize;
            for field in &mut model.fields {
                if !matcher.matches(field) {
                    continue;
                }
                matched += 1;
                if field.name != *to {
                    field.name = to.clone();
                    changed += 1;
                }
            }
            (matched, changed)
        }
        FieldAction::AddAccessFlags { flags } => {
            let mut matched = 0_usize;
            let mut changed = 0_usize;
            let flags = field_flags(flags);
            for field in &mut model.fields {
                if !matcher.matches(field) {
                    continue;
                }
                matched += 1;
                if !field.access_flags.contains(flags) {
                    field.access_flags |= flags;
                    changed += 1;
                }
            }
            (matched, changed)
        }
        FieldAction::RemoveAccessFlags { flags } => {
            let mut matched = 0_usize;
            let mut changed = 0_usize;
            let flags = field_flags(flags);
            for field in &mut model.fields {
                if !matcher.matches(field) {
                    continue;
                }
                matched += 1;
                if field.access_flags.intersects(flags) {
                    field.access_flags.remove(flags);
                    changed += 1;
                }
            }
            (matched, changed)
        }
        FieldAction::SetAccessFlags { flags } => {
            let mut matched = 0_usize;
            let mut changed = 0_usize;
            let flags = field_flags(flags);
            for field in &mut model.fields {
                if !matcher.matches(field) {
                    continue;
                }
                matched += 1;
                if field.access_flags != flags {
                    field.access_flags = flags;
                    changed += 1;
                }
            }
            (matched, changed)
        }
    }
}

fn apply_method_action(
    model: &mut ClassModel,
    matcher: &MethodMatcher,
    action: &MethodAction,
) -> (usize, usize) {
    match action {
        MethodAction::Remove => {
            let mut matched = 0_usize;
            model.methods.retain(|method| {
                let keep = !matcher.matches(method);
                if !keep {
                    matched += 1;
                }
                keep
            });
            (matched, matched)
        }
        MethodAction::Rename { to } => {
            let mut matched = 0_usize;
            let mut changed = 0_usize;
            for method in &mut model.methods {
                if !matcher.matches(method) {
                    continue;
                }
                matched += 1;
                if method.name != *to {
                    method.name = to.clone();
                    changed += 1;
                }
            }
            (matched, changed)
        }
        MethodAction::AddAccessFlags { flags } => {
            let mut matched = 0_usize;
            let mut changed = 0_usize;
            let flags = method_flags(flags);
            for method in &mut model.methods {
                if !matcher.matches(method) {
                    continue;
                }
                matched += 1;
                if !method.access_flags.contains(flags) {
                    method.access_flags |= flags;
                    changed += 1;
                }
            }
            (matched, changed)
        }
        MethodAction::RemoveAccessFlags { flags } => {
            let mut matched = 0_usize;
            let mut changed = 0_usize;
            let flags = method_flags(flags);
            for method in &mut model.methods {
                if !matcher.matches(method) {
                    continue;
                }
                matched += 1;
                if method.access_flags.intersects(flags) {
                    method.access_flags.remove(flags);
                    changed += 1;
                }
            }
            (matched, changed)
        }
        MethodAction::SetAccessFlags { flags } => {
            let mut matched = 0_usize;
            let mut changed = 0_usize;
            let flags = method_flags(flags);
            for method in &mut model.methods {
                if !matcher.matches(method) {
                    continue;
                }
                matched += 1;
                if method.access_flags != flags {
                    method.access_flags = flags;
                    changed += 1;
                }
            }
            (matched, changed)
        }
    }
}

fn apply_method_actions(
    model: &mut ClassModel,
    matcher: &MethodMatcher,
    action: Option<&MethodAction>,
    code_actions: &[CompiledCodeAction],
) -> (usize, usize) {
    let mut matched_targets = 0_usize;
    let mut changed_targets = 0_usize;

    if let Some(action) = action {
        let (matched, changed) = apply_method_action(model, matcher, action);
        matched_targets += matched;
        changed_targets += changed;
    }

    for code_action in code_actions {
        let (matched, changed) = apply_code_action(model, matcher, code_action);
        matched_targets += matched;
        changed_targets += changed;
    }

    (matched_targets, changed_targets)
}

fn apply_code_action(
    model: &mut ClassModel,
    matcher: &MethodMatcher,
    action: &CompiledCodeAction,
) -> (usize, usize) {
    let mut matched = 0_usize;
    let mut changed = 0_usize;

    for method in &mut model.methods {
        if !matcher.matches(method) {
            continue;
        }
        let Some(code) = method.code.as_mut() else {
            continue;
        };

        let target_matches = match action {
            CompiledCodeAction::Single { matcher, .. } => count_matching_insns(code, matcher),
            CompiledCodeAction::Sequence { pattern, .. } => count_matching_sequences(code, pattern),
        };
        if target_matches == 0 {
            continue;
        }
        matched += target_matches;
        match action {
            CompiledCodeAction::Single { transform, .. }
            | CompiledCodeAction::Sequence { transform, .. } => transform.apply(code),
        }
        changed += target_matches;
    }

    (matched, changed)
}

fn class_flags(flags: &[ClassAccessFlagName]) -> ClassAccessFlags {
    flags
        .iter()
        .fold(ClassAccessFlags::empty(), |mut acc, flag| {
            acc |= match flag {
                ClassAccessFlagName::Public => ClassAccessFlags::PUBLIC,
                ClassAccessFlagName::Final => ClassAccessFlags::FINAL,
                ClassAccessFlagName::Super => ClassAccessFlags::SUPER,
                ClassAccessFlagName::Interface => ClassAccessFlags::INTERFACE,
                ClassAccessFlagName::Abstract => ClassAccessFlags::ABSTRACT,
                ClassAccessFlagName::Synthetic => ClassAccessFlags::SYNTHETIC,
                ClassAccessFlagName::Annotation => ClassAccessFlags::ANNOTATION,
                ClassAccessFlagName::Enum => ClassAccessFlags::ENUM,
                ClassAccessFlagName::Module => ClassAccessFlags::MODULE,
            };
            acc
        })
}

fn field_flags(flags: &[FieldAccessFlagName]) -> FieldAccessFlags {
    flags
        .iter()
        .fold(FieldAccessFlags::empty(), |mut acc, flag| {
            acc |= match flag {
                FieldAccessFlagName::Public => FieldAccessFlags::PUBLIC,
                FieldAccessFlagName::Private => FieldAccessFlags::PRIVATE,
                FieldAccessFlagName::Protected => FieldAccessFlags::PROTECTED,
                FieldAccessFlagName::Static => FieldAccessFlags::STATIC,
                FieldAccessFlagName::Final => FieldAccessFlags::FINAL,
                FieldAccessFlagName::Volatile => FieldAccessFlags::VOLATILE,
                FieldAccessFlagName::Transient => FieldAccessFlags::TRANSIENT,
                FieldAccessFlagName::Synthetic => FieldAccessFlags::SYNTHETIC,
                FieldAccessFlagName::Enum => FieldAccessFlags::ENUM,
            };
            acc
        })
}

fn method_flags(flags: &[MethodAccessFlagName]) -> MethodAccessFlags {
    flags
        .iter()
        .fold(MethodAccessFlags::empty(), |mut acc, flag| {
            acc |= match flag {
                MethodAccessFlagName::Public => MethodAccessFlags::PUBLIC,
                MethodAccessFlagName::Private => MethodAccessFlags::PRIVATE,
                MethodAccessFlagName::Protected => MethodAccessFlags::PROTECTED,
                MethodAccessFlagName::Static => MethodAccessFlags::STATIC,
                MethodAccessFlagName::Final => MethodAccessFlags::FINAL,
                MethodAccessFlagName::Synchronized => MethodAccessFlags::SYNCHRONIZED,
                MethodAccessFlagName::Bridge => MethodAccessFlags::BRIDGE,
                MethodAccessFlagName::Varargs => MethodAccessFlags::VARARGS,
                MethodAccessFlagName::Native => MethodAccessFlags::NATIVE,
                MethodAccessFlagName::Abstract => MethodAccessFlags::ABSTRACT,
                MethodAccessFlagName::Strict => MethodAccessFlags::STRICT,
                MethodAccessFlagName::Synthetic => MethodAccessFlags::SYNTHETIC,
            };
            acc
        })
}
