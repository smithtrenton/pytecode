mod stack_map;
use stack_map::lower_stack_map_table;

mod canonical_pool;
mod constant_pool_builder;
mod debug_info;
mod labels;
mod operands;

use crate::analysis::{
    ClassResolver, FrameComputationResult, FrameWarning, StackMapFrameState, VType,
    initial_method_locals, recompute_and_normalize_code,
};
pub use canonical_pool::{ConstantPoolPolicy, canonicalize_constant_pool};
pub use constant_pool_builder::ConstantPoolBuilder;
pub use debug_info::{DebugInfoPolicy, DebugInfoState};
pub use labels::{
    BranchInsn, CodeItem, CodeTypeAnnotation, CodeTypeAnnotationTarget, ExceptionHandler, Label,
    LineNumberEntry, LocalVariableEntry, LocalVariableTypeEntry, LocalvarTargetEntry,
    LookupSwitchInsn, TableSwitchInsn,
};
pub use operands::{
    DynamicValue, FieldInsn, IIncInsn, InterfaceMethodInsn, InvokeDynamicInsn, LdcInsn, LdcValue,
    MethodHandleValue, MethodInsn, MultiANewArrayInsn, TypeInsn, VarInsn,
};

use crate::constants::{ClassAccessFlags, FieldAccessFlags, MAGIC, MethodAccessFlags};
use crate::descriptors::{
    is_valid_field_descriptor, is_valid_method_descriptor, parameter_slot_count,
    parse_method_descriptor,
};
use crate::indexes::{ClassIndex, CpIndex, Utf8Index};
use crate::raw::instructions::operand_kind;
use crate::raw::{
    AttributeInfo, Branch, ClassFile, CodeAttribute, ConstantPoolEntry, ElementValueInfo,
    FieldInfo, Instruction, InvokeDynamicInsn as RawInvokeDynamicInsn,
    InvokeInterfaceInsn as RawInvokeInterfaceInsn, LookupSwitchInsn as RawLookupSwitchInsn,
    MatchOffsetPair, MethodInfo, RawClassStub, RuntimeInvisibleTypeAnnotationsAttribute,
    RuntimeVisibleTypeAnnotationsAttribute, TableInfo, TableSwitchInsn as RawTableSwitchInsn,
    TargetInfo, TypeAnnotationInfo, WideInstruction,
};
use crate::transform::InsnMatcher;
use crate::{EngineError, EngineErrorKind, Result, parse_class, write_class};
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeModel {
    pub max_stack: u16,
    pub max_locals: u16,
    pub instructions: Vec<CodeItem>,
    pub exception_handlers: Vec<ExceptionHandler>,
    pub line_numbers: Vec<LineNumberEntry>,
    pub local_variables: Vec<LocalVariableEntry>,
    pub local_variable_types: Vec<LocalVariableTypeEntry>,
    pub visible_type_annotations: Vec<CodeTypeAnnotation>,
    pub invisible_type_annotations: Vec<CodeTypeAnnotation>,
    pub attributes: Vec<AttributeInfo>,
    pub debug_info_state: DebugInfoState,
    nested_attribute_layout: Vec<NestedCodeAttributeLayout>,
    original_code_shape: Option<OriginalCodeShape>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldModel {
    pub access_flags: FieldAccessFlags,
    pub name: String,
    pub descriptor: String,
    pub attributes: Vec<AttributeInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MethodBody {
    Symbolic(Box<CodeModel>),
    Prebuilt(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodModel {
    pub access_flags: MethodAccessFlags,
    pub name: String,
    pub descriptor: String,
    pub body: Option<MethodBody>,
    pub attributes: Vec<AttributeInfo>,
    attribute_layout: Vec<MethodAttributeLayout>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassModel {
    pub entry_name: String,
    pub original_byte_len: usize,
    pub version: (u16, u16),
    pub access_flags: ClassAccessFlags,
    pub name: String,
    pub super_name: Option<String>,
    pub interfaces: Vec<String>,
    pub fields: Vec<FieldModel>,
    pub methods: Vec<MethodModel>,
    pub attributes: Vec<AttributeInfo>,
    pub constant_pool: ConstantPoolBuilder,
    pub debug_info_state: DebugInfoState,
}

impl Default for ClassModel {
    fn default() -> Self {
        Self {
            entry_name: String::new(),
            original_byte_len: 0,
            version: (0, 0),
            access_flags: ClassAccessFlags::empty(),
            name: String::new(),
            super_name: None,
            interfaces: Vec::new(),
            fields: Vec::new(),
            methods: Vec::new(),
            attributes: Vec::new(),
            constant_pool: ConstantPoolBuilder::default(),
            debug_info_state: DebugInfoState::default(),
        }
    }
}

/// One entry of a class's `BootstrapMethods` attribute, in symbolic form.
///
/// `invokedynamic` instructions and dynamic constants refer to entries by
/// position (their `bootstrap_method_attr_index`), so editing an entry in
/// place keeps every such reference valid. The static `arguments` are
/// loadable constants — exactly the values an `ldc` can push — and can name
/// classes through method handles, method types, and class literals, which
/// a class rename has to rewrite along with the instructions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapMethodModel {
    /// The bootstrap method itself (normally `REF_invokeStatic`).
    pub method: MethodHandleValue,
    /// The static arguments passed to the bootstrap method.
    pub arguments: Vec<LdcValue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameComputationMode {
    Preserve,
    Recompute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NestedCodeAttributeLayout {
    Other,
    LineNumbers,
    LocalVariables,
    LocalVariableTypes,
    VisibleTypeAnnotations,
    InvisibleTypeAnnotations,
    StackMapTable,
}

impl std::fmt::Display for NestedCodeAttributeLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Other => write!(f, "other"),
            Self::LineNumbers => write!(f, "line_numbers"),
            Self::LocalVariables => write!(f, "local_variables"),
            Self::LocalVariableTypes => write!(f, "local_variable_types"),
            Self::VisibleTypeAnnotations => write!(f, "visible_type_annotations"),
            Self::InvisibleTypeAnnotations => write!(f, "invisible_type_annotations"),
            Self::StackMapTable => write!(f, "stack_map_table"),
        }
    }
}

impl CodeModel {
    /// Parse a complete bytecode body into symbolic instructions using its owner's pool.
    pub fn from_raw_code(
        max_stack: u16,
        max_locals: u16,
        bytes: &[u8],
        cp: &ConstantPoolBuilder,
    ) -> Result<Self> {
        if bytes.is_empty() || bytes.len() > 65535 {
            return Err(model_error("Code length must be between 1 and 65535 bytes"));
        }
        let mut model = lift_code_model(
            &CodeAttribute {
                attribute_name_index: Utf8Index::from(0),
                attribute_length: 0,
                max_stack,
                max_locals,
                code_length: bytes.len() as u32,
                code: crate::parse_instructions(bytes)?,
                exception_table: Vec::new(),
                attributes: Vec::new(),
            },
            cp,
            "",
            "",
            MethodAccessFlags::empty(),
        )?;
        model.original_code_shape = None;
        Ok(model)
    }

    /// Create a new empty `CodeModel` for testing or programmatic construction.
    pub fn new(max_stack: u16, max_locals: u16, debug_info_state: DebugInfoState) -> Self {
        Self {
            max_stack,
            max_locals,
            instructions: Vec::new(),
            exception_handlers: Vec::new(),
            line_numbers: Vec::new(),
            local_variables: Vec::new(),
            local_variable_types: Vec::new(),
            visible_type_annotations: Vec::new(),
            invisible_type_annotations: Vec::new(),
            attributes: Vec::new(),
            debug_info_state,
            nested_attribute_layout: Vec::new(),
            original_code_shape: None,
        }
    }

    pub fn nested_attribute_layout(&self) -> &[NestedCodeAttributeLayout] {
        &self.nested_attribute_layout
    }

    /// Find the indexes of all instructions matching `matcher`.
    pub fn find_insns(&self, matcher: &InsnMatcher) -> Vec<usize> {
        self.instructions
            .iter()
            .enumerate()
            .filter_map(|(index, item)| matcher.matches(item).then_some(index))
            .collect()
    }

    /// Find the first instruction matching `matcher` at or after `start`.
    pub fn find_insn(&self, matcher: &InsnMatcher, start: usize) -> Option<usize> {
        self.instructions
            .iter()
            .enumerate()
            .skip(start)
            .find_map(|(index, item)| matcher.matches(item).then_some(index))
    }

    /// Return whether any instruction matches `matcher`.
    pub fn contains_insn(&self, matcher: &InsnMatcher) -> bool {
        self.instructions.iter().any(|item| matcher.matches(item))
    }

    /// Count instructions matching `matcher`.
    pub fn count_insns(&self, matcher: &InsnMatcher) -> usize {
        self.instructions
            .iter()
            .filter(|item| matcher.matches(item))
            .count()
    }

    /// Find all contiguous sequences whose items match `pattern`.
    pub fn find_sequences(&self, pattern: &[InsnMatcher]) -> Vec<usize> {
        if pattern.is_empty() || pattern.len() > self.instructions.len() {
            return Vec::new();
        }
        self.instructions
            .windows(pattern.len())
            .enumerate()
            .filter_map(|(index, window)| {
                pattern
                    .iter()
                    .zip(window.iter())
                    .all(|(matcher, item)| matcher.matches(item))
                    .then_some(index)
            })
            .collect()
    }

    fn find_non_overlapping_sequences(&self, pattern: &[InsnMatcher]) -> Vec<usize> {
        if pattern.is_empty() || pattern.len() > self.instructions.len() {
            return Vec::new();
        }

        let mut matches = Vec::new();
        let mut index = 0;
        while index + pattern.len() <= self.instructions.len() {
            let window = &self.instructions[index..index + pattern.len()];
            if pattern
                .iter()
                .zip(window.iter())
                .all(|(matcher, item)| matcher.matches(item))
            {
                matches.push(index);
                index += pattern.len();
            } else {
                index += 1;
            }
        }
        matches
    }

    pub fn replace_insns(&mut self, matcher: &InsnMatcher, replacement: &[CodeItem]) -> usize {
        let matches = self.find_insns(matcher);
        if matches.is_empty() {
            return 0;
        }
        let mut out = Vec::with_capacity(
            self.instructions.len() - matches.len() + matches.len() * replacement.len(),
        );
        let mut cursor = 0_usize;
        for (site_index, &match_index) in matches.iter().enumerate() {
            out.extend_from_slice(&self.instructions[cursor..match_index]);
            out.extend(replacement_for_site(replacement, site_index));
            cursor = match_index + 1;
        }
        out.extend_from_slice(&self.instructions[cursor..]);
        self.instructions = out;
        matches.len()
    }

    pub fn remove_insns(&mut self, matcher: &InsnMatcher) -> usize {
        let mut removed = 0_usize;
        self.instructions.retain(|item| {
            let keep = !matcher.matches(item);
            if !keep {
                removed += 1;
            }
            keep
        });
        removed
    }

    pub fn insert_before(&mut self, matcher: &InsnMatcher, items: &[CodeItem]) -> usize {
        if items.is_empty() {
            return 0;
        }
        let matches = self.find_insns(matcher);
        if matches.is_empty() {
            return 0;
        }
        let mut out = Vec::with_capacity(self.instructions.len() + matches.len() * items.len());
        let mut cursor = 0_usize;
        for (site_index, &match_index) in matches.iter().enumerate() {
            out.extend_from_slice(&self.instructions[cursor..match_index]);
            out.extend(replacement_for_site(items, site_index));
            out.push(self.instructions[match_index].clone());
            cursor = match_index + 1;
        }
        out.extend_from_slice(&self.instructions[cursor..]);
        self.instructions = out;
        matches.len()
    }

    pub fn insert_after(&mut self, matcher: &InsnMatcher, items: &[CodeItem]) -> usize {
        if items.is_empty() {
            return 0;
        }
        let matches = self.find_insns(matcher);
        if matches.is_empty() {
            return 0;
        }
        let mut out = Vec::with_capacity(self.instructions.len() + matches.len() * items.len());
        let mut cursor = 0_usize;
        for (site_index, &match_index) in matches.iter().enumerate() {
            out.extend_from_slice(&self.instructions[cursor..=match_index]);
            out.extend(replacement_for_site(items, site_index));
            cursor = match_index + 1;
        }
        out.extend_from_slice(&self.instructions[cursor..]);
        self.instructions = out;
        matches.len()
    }

    pub fn replace_sequences(
        &mut self,
        pattern: &[InsnMatcher],
        replacement: &[CodeItem],
    ) -> usize {
        let matches = self.find_non_overlapping_sequences(pattern);
        if matches.is_empty() {
            return 0;
        }
        let mut out = Vec::with_capacity(
            self.instructions.len() - matches.len() * pattern.len()
                + matches.len() * replacement.len(),
        );
        let mut cursor = 0_usize;
        for (site_index, &match_index) in matches.iter().enumerate() {
            out.extend_from_slice(&self.instructions[cursor..match_index]);
            out.extend(replacement_for_site(replacement, site_index));
            cursor = match_index + pattern.len();
        }
        out.extend_from_slice(&self.instructions[cursor..]);
        self.instructions = out;
        matches.len()
    }

    pub fn remove_sequences(&mut self, pattern: &[InsnMatcher]) -> usize {
        self.replace_sequences(pattern, &[])
    }
}

fn replacement_for_site(items: &[CodeItem], site_index: usize) -> Vec<CodeItem> {
    if site_index == 0 {
        items.to_vec()
    } else {
        clone_items_with_fresh_labels(items)
    }
}

fn clone_items_with_fresh_labels(items: &[CodeItem]) -> Vec<CodeItem> {
    let mut remapped = FxHashMap::default();
    for item in items {
        if let CodeItem::Label(label) = item {
            remapped
                .entry(label.clone())
                .or_insert_with(|| match &label.name {
                    Some(name) => Label::named(name.clone()),
                    None => Label::new(),
                });
        }
    }
    if remapped.is_empty() {
        return items.to_vec();
    }
    let remap_label = |label: &Label| {
        remapped
            .get(label)
            .cloned()
            .unwrap_or_else(|| label.clone())
    };
    items
        .iter()
        .map(|item| match item {
            CodeItem::Label(label) => CodeItem::Label(remap_label(label)),
            CodeItem::Branch(insn) => CodeItem::Branch(BranchInsn {
                opcode: insn.opcode,
                target: remap_label(&insn.target),
            }),
            CodeItem::LookupSwitch(insn) => CodeItem::LookupSwitch(LookupSwitchInsn {
                default_target: remap_label(&insn.default_target),
                pairs: insn
                    .pairs
                    .iter()
                    .map(|(value, label)| (*value, remap_label(label)))
                    .collect(),
            }),
            CodeItem::TableSwitch(insn) => CodeItem::TableSwitch(TableSwitchInsn {
                default_target: remap_label(&insn.default_target),
                low: insn.low,
                high: insn.high,
                targets: insn.targets.iter().map(&remap_label).collect(),
            }),
            other => other.clone(),
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MethodAttributeLayout {
    Other,
    Code,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OriginalCodeShape {
    instructions: Vec<CodeItem>,
    exception_handlers: Vec<ExceptionHandler>,
    item_offsets: Vec<Option<u32>>,
    code_length: u32,
    owner_class_name: String,
    method_descriptor: String,
    method_access_flags: MethodAccessFlags,
}

impl MethodModel {
    /// Create a new `MethodModel` for testing or programmatic construction.
    pub fn new(
        access_flags: MethodAccessFlags,
        name: String,
        descriptor: String,
        code: Option<CodeModel>,
        attributes: Vec<AttributeInfo>,
    ) -> Self {
        let mut attribute_layout = Vec::new();
        if code.is_some() {
            attribute_layout.push(MethodAttributeLayout::Code);
        }
        Self {
            access_flags,
            name,
            descriptor,
            body: code.map(|code| MethodBody::Symbolic(Box::new(code))),
            attributes,
            attribute_layout,
        }
    }

    pub fn code(&self) -> Option<&CodeModel> {
        match &self.body {
            Some(MethodBody::Symbolic(code)) => Some(code),
            _ => None,
        }
    }

    pub fn code_mut(&mut self) -> Option<&mut CodeModel> {
        match &mut self.body {
            Some(MethodBody::Symbolic(code)) => Some(code),
            _ => None,
        }
    }

    pub fn set_code(&mut self, code: Option<CodeModel>) {
        self.body = code.map(|code| MethodBody::Symbolic(Box::new(code)));
        if self.body.is_some() && !self.attribute_layout.contains(&MethodAttributeLayout::Code) {
            self.attribute_layout.push(MethodAttributeLayout::Code);
        }
    }

    pub fn prebuilt_code_bytes(&self) -> Option<&[u8]> {
        match &self.body {
            Some(MethodBody::Prebuilt(bytes)) => Some(bytes.as_slice()),
            _ => None,
        }
    }

    pub fn set_prebuilt_code_bytes(&mut self, bytes: Vec<u8>) {
        self.body = Some(MethodBody::Prebuilt(bytes));
        if !self.attribute_layout.contains(&MethodAttributeLayout::Code) {
            self.attribute_layout.push(MethodAttributeLayout::Code);
        }
    }

    pub fn has_code(&self) -> bool {
        self.body.is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BranchEncoding {
    Short,
    Wide,
    InvertedWide,
}

pub fn lift_classes(raw_classes: &[RawClassStub]) -> Result<Vec<ClassModel>> {
    raw_classes.iter().map(ClassModel::from_raw_class).collect()
}

pub fn lower_models(models: &[ClassModel]) -> Result<Vec<RawClassStub>> {
    models.iter().map(ClassModel::lower_to_raw_class).collect()
}

impl ClassModel {
    pub fn from_raw_class(raw_class: &RawClassStub) -> Result<Self> {
        let classfile = parse_class(&raw_class.bytes)?;
        let mut model = Self::from_classfile(&classfile)?;
        model.entry_name = raw_class.entry_name.clone();
        model.original_byte_len = raw_class.bytes.len();
        Ok(model)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let classfile = parse_class(bytes)?;
        let mut model = Self::from_classfile(&classfile)?;
        model.original_byte_len = bytes.len();
        Ok(model)
    }

    pub fn from_classfile(classfile: &ClassFile) -> Result<Self> {
        let cp = ConstantPoolBuilder::from_pool(&classfile.constant_pool);
        let name = cp.resolve_class_name(classfile.this_class)?;
        let super_name = if classfile.super_class.value() == 0 {
            None
        } else {
            Some(cp.resolve_class_name(classfile.super_class)?)
        };
        let interfaces = classfile
            .interfaces
            .iter()
            .map(|index| cp.resolve_class_name(*index))
            .collect::<Result<Vec<_>>>()?;
        let fields = classfile
            .fields
            .iter()
            .map(|field| lift_field_model(field, &cp))
            .collect::<Result<Vec<_>>>()?;
        let methods = classfile
            .methods
            .iter()
            .map(|method| lift_method_model(method, &cp, &name))
            .collect::<Result<Vec<_>>>()?;

        Ok(Self {
            entry_name: String::new(),
            original_byte_len: 0,
            version: (classfile.major_version, classfile.minor_version),
            access_flags: classfile.access_flags,
            name,
            super_name,
            interfaces,
            fields,
            methods,
            attributes: classfile.attributes.clone(),
            constant_pool: cp,
            debug_info_state: DebugInfoState::Fresh,
        })
    }

    /// Decode the `BootstrapMethods` attribute into symbolic entries.
    ///
    /// Returns an empty list when the class has no such attribute.
    pub fn bootstrap_methods(&self) -> Result<Vec<BootstrapMethodModel>> {
        let Some(attribute) = self
            .attributes
            .iter()
            .find_map(|attribute| match attribute {
                AttributeInfo::BootstrapMethods(attribute) => Some(attribute),
                _ => None,
            })
        else {
            return Ok(Vec::new());
        };
        attribute
            .bootstrap_methods
            .iter()
            .enumerate()
            .map(|(position, entry)| {
                let LdcValue::MethodHandle(method) =
                    lift_ldc_value(entry.bootstrap_method_ref, &self.constant_pool)?
                else {
                    return Err(model_error(format!(
                        "bootstrap method {position} does not reference a MethodHandle"
                    )));
                };
                let arguments = entry
                    .bootstrap_arguments
                    .iter()
                    .map(|index| lift_ldc_value(*index, &self.constant_pool))
                    .collect::<Result<Vec<_>>>()?;
                Ok(BootstrapMethodModel { method, arguments })
            })
            .collect()
    }

    /// Replace the `BootstrapMethods` attribute with `methods`.
    ///
    /// Constant-pool entries are reused when they already exist, so writing
    /// back unchanged entries leaves the class byte-for-byte identical. The
    /// attribute keeps its position among the class attributes; an empty
    /// list removes it. Entries are addressed by position, so keep existing
    /// entries where they are when `invokedynamic` instructions or dynamic
    /// constants refer to them.
    pub fn set_bootstrap_methods(&mut self, methods: &[BootstrapMethodModel]) -> Result<()> {
        let position = self
            .attributes
            .iter()
            .position(|attribute| matches!(attribute, AttributeInfo::BootstrapMethods(_)));
        if methods.is_empty() {
            if let Some(position) = position {
                self.attributes.remove(position);
            }
            return Ok(());
        }
        if methods.len() > usize::from(u16::MAX) {
            return Err(model_error("BootstrapMethods holds at most 65535 entries"));
        }
        // Encode into a copy so a failure leaves the model untouched.
        let mut cp = self.constant_pool.clone();
        let existing = position.and_then(|position| match &self.attributes[position] {
            AttributeInfo::BootstrapMethods(attribute) => Some(attribute.bootstrap_methods.clone()),
            _ => None,
        });
        // Keep the original index wherever the value at that position is
        // unchanged: obfuscated pools often hold duplicate constants, and
        // re-encoding would otherwise pick the first equal entry.
        let encode = |cp: &mut ConstantPoolBuilder,
                      value: &LdcValue,
                      original: Option<CpIndex>|
         -> Result<CpIndex> {
            if let Some(original) = original
                && lift_ldc_value(original, cp).is_ok_and(|current| current == *value)
            {
                return Ok(original);
            }
            add_ldc_value(cp, value)
        };
        let mut entries = Vec::with_capacity(methods.len());
        let mut attribute_length = 2_u32;
        for (position, method) in methods.iter().enumerate() {
            if method.arguments.len() > usize::from(u16::MAX) {
                return Err(model_error(
                    "a bootstrap method takes at most 65535 static arguments",
                ));
            }
            let original = existing.as_ref().and_then(|entries| entries.get(position));
            let bootstrap_method_ref = encode(
                &mut cp,
                &LdcValue::MethodHandle(method.method.clone()),
                original.map(|entry| entry.bootstrap_method_ref),
            )?;
            let bootstrap_arguments = method
                .arguments
                .iter()
                .enumerate()
                .map(|(index, argument)| {
                    encode(
                        &mut cp,
                        argument,
                        original.and_then(|entry| entry.bootstrap_arguments.get(index).copied()),
                    )
                })
                .collect::<Result<Vec<_>>>()?;
            attribute_length = attribute_length
                .checked_add(4 + 2 * bootstrap_arguments.len() as u32)
                .ok_or_else(|| model_error("BootstrapMethods attribute is too large"))?;
            entries.push(crate::raw::BootstrapMethodInfo {
                bootstrap_method_ref,
                bootstrap_arguments,
            });
        }
        let attribute = AttributeInfo::BootstrapMethods(crate::raw::BootstrapMethodsAttribute {
            attribute_name_index: cp.add_utf8("BootstrapMethods")?,
            attribute_length,
            bootstrap_methods: entries,
        });
        self.constant_pool = cp;
        match position {
            Some(position) => self.attributes[position] = attribute,
            None => self.attributes.push(attribute),
        }
        Ok(())
    }

    pub fn to_classfile(&self) -> Result<ClassFile> {
        self.to_classfile_with_options(
            DebugInfoPolicy::Preserve,
            FrameComputationMode::Preserve,
            None,
        )
    }

    pub fn to_classfile_with_policy(&self, debug_info: DebugInfoPolicy) -> Result<ClassFile> {
        self.to_classfile_with_options(debug_info, FrameComputationMode::Preserve, None)
    }

    pub fn to_classfile_with_options(
        &self,
        debug_info: DebugInfoPolicy,
        frame_mode: FrameComputationMode,
        resolver: Option<&dyn ClassResolver>,
    ) -> Result<ClassFile> {
        self.to_classfile_with_report(debug_info, frame_mode, resolver)
            .map(|(classfile, _)| classfile)
    }

    /// Lower the model like [`Self::to_classfile_with_options`], also
    /// returning the frame-computation warnings for every method.
    ///
    /// With [`FrameComputationMode::Recompute`], a warning means missing
    /// hierarchy data (no resolver, or classes absent from it) forced a
    /// merged reference type to `java/lang/Object` where the method later
    /// needs something more specific, so the JVM verifier will likely reject
    /// those frames. Supplying the listed classes to the resolver fixes it.
    pub fn to_classfile_with_report(
        &self,
        debug_info: DebugInfoPolicy,
        frame_mode: FrameComputationMode,
        resolver: Option<&dyn ClassResolver>,
    ) -> Result<(ClassFile, Vec<FrameWarning>)> {
        let mut warnings = Vec::new();
        let mut cp = self.constant_pool.clone();
        let this_class = cp.add_class(&self.name)?;
        let super_class = match &self.super_name {
            Some(super_name) => cp.add_class(super_name)?,
            None => ClassIndex::default(),
        };
        let mut interfaces = Vec::with_capacity(self.interfaces.len());
        for name in &self.interfaces {
            interfaces.push(cp.add_class(name)?);
        }
        let mut fields = Vec::with_capacity(self.fields.len());
        for field in &self.fields {
            fields.push(lower_field_model(field, &mut cp)?);
        }
        let mut methods = Vec::with_capacity(self.methods.len());
        for method in &self.methods {
            methods.push(lower_method_model(
                method,
                self,
                &mut cp,
                debug_info,
                frame_mode,
                resolver,
                &mut warnings,
            )?);
        }

        let classfile = ClassFile {
            magic: MAGIC,
            minor_version: self.version.1,
            major_version: self.version.0,
            constant_pool: cp.into_entries(),
            access_flags: self.access_flags,
            this_class,
            super_class,
            interfaces,
            fields,
            methods,
            attributes: class_attributes_for_policy(
                &self.attributes,
                self.debug_info_state,
                debug_info,
            ),
        };
        Ok((classfile, warnings))
    }

    pub fn to_classfile_with_recomputed_frames(
        &self,
        debug_info: DebugInfoPolicy,
        resolver: Option<&dyn ClassResolver>,
    ) -> Result<ClassFile> {
        self.to_classfile_with_options(debug_info, FrameComputationMode::Recompute, resolver)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        write_class(&self.to_classfile()?)
    }

    pub fn to_bytes_with_policy(&self, debug_info: DebugInfoPolicy) -> Result<Vec<u8>> {
        write_class(&self.to_classfile_with_policy(debug_info)?)
    }

    pub fn to_bytes_with_options(
        &self,
        debug_info: DebugInfoPolicy,
        frame_mode: FrameComputationMode,
        resolver: Option<&dyn ClassResolver>,
    ) -> Result<Vec<u8>> {
        write_class(&self.to_classfile_with_options(debug_info, frame_mode, resolver)?)
    }

    /// Serialize like [`Self::to_bytes_with_options`], also returning the
    /// frame-computation warnings (see [`Self::to_classfile_with_report`]).
    pub fn to_bytes_with_report(
        &self,
        debug_info: DebugInfoPolicy,
        frame_mode: FrameComputationMode,
        resolver: Option<&dyn ClassResolver>,
    ) -> Result<(Vec<u8>, Vec<FrameWarning>)> {
        let (classfile, warnings) =
            self.to_classfile_with_report(debug_info, frame_mode, resolver)?;
        Ok((write_class(&classfile)?, warnings))
    }

    /// Lower like [`Self::to_classfile_with_report`] with an explicit
    /// constant-pool layout.
    ///
    /// [`ConstantPoolPolicy::Preserve`] is exactly
    /// [`Self::to_classfile_with_report`]. [`ConstantPoolPolicy::Canonical`]
    /// lowers the same way, then rebuilds the pool in canonical order (see
    /// [`canonicalize_constant_pool`]), so classes with identical content
    /// lower to identical bytes however their pools were built.
    pub fn to_classfile_with_pool_policy(
        &self,
        debug_info: DebugInfoPolicy,
        frame_mode: FrameComputationMode,
        resolver: Option<&dyn ClassResolver>,
        pool: ConstantPoolPolicy,
    ) -> Result<(ClassFile, Vec<FrameWarning>)> {
        let (classfile, warnings) =
            self.to_classfile_with_report(debug_info, frame_mode, resolver)?;
        match pool {
            ConstantPoolPolicy::Preserve => Ok((classfile, warnings)),
            ConstantPoolPolicy::Canonical => {
                Ok((canonicalize_constant_pool(&classfile)?, warnings))
            }
        }
    }

    /// Serialize like [`Self::to_classfile_with_pool_policy`].
    pub fn to_bytes_with_pool_policy(
        &self,
        debug_info: DebugInfoPolicy,
        frame_mode: FrameComputationMode,
        resolver: Option<&dyn ClassResolver>,
        pool: ConstantPoolPolicy,
    ) -> Result<(Vec<u8>, Vec<FrameWarning>)> {
        let (classfile, warnings) =
            self.to_classfile_with_pool_policy(debug_info, frame_mode, resolver, pool)?;
        Ok((write_class(&classfile)?, warnings))
    }

    pub fn to_bytes_with_recomputed_frames(
        &self,
        debug_info: DebugInfoPolicy,
        resolver: Option<&dyn ClassResolver>,
    ) -> Result<Vec<u8>> {
        write_class(&self.to_classfile_with_recomputed_frames(debug_info, resolver)?)
    }

    pub fn lower_to_raw_class(&self) -> Result<RawClassStub> {
        Ok(RawClassStub {
            entry_name: self.entry_name.clone(),
            bytes: self.to_bytes()?,
        })
    }
}

pub fn mark_class_debug_info_stale(model: &mut ClassModel) {
    model.debug_info_state = DebugInfoState::Stale;
}

pub fn mark_method_debug_info_stale(method: &mut MethodModel) {
    if let Some(code) = method.code_mut() {
        code.debug_info_state = DebugInfoState::Stale;
    }
}

fn lift_field_model(field: &FieldInfo, cp: &ConstantPoolBuilder) -> Result<FieldModel> {
    Ok(FieldModel {
        access_flags: field.access_flags,
        name: cp.resolve_utf8(field.name_index)?,
        descriptor: cp.resolve_utf8(field.descriptor_index)?,
        attributes: field.attributes.clone(),
    })
}

fn lift_method_model(
    method: &MethodInfo,
    cp: &ConstantPoolBuilder,
    owner_class_name: &str,
) -> Result<MethodModel> {
    let descriptor = cp.resolve_utf8(method.descriptor_index)?;
    let mut body = None;
    let mut attributes = Vec::new();
    let mut attribute_layout = Vec::new();
    for attribute in &method.attributes {
        match attribute {
            AttributeInfo::Code(code_attr) => {
                body = Some(MethodBody::Symbolic(Box::new(lift_code_model(
                    code_attr,
                    cp,
                    owner_class_name,
                    &descriptor,
                    method.access_flags,
                )?)));
                attribute_layout.push(MethodAttributeLayout::Code);
            }
            _ => {
                attributes.push(attribute.clone());
                attribute_layout.push(MethodAttributeLayout::Other);
            }
        }
    }

    Ok(MethodModel {
        access_flags: method.access_flags,
        name: cp.resolve_utf8(method.name_index)?,
        descriptor,
        body,
        attributes,
        attribute_layout,
    })
}

fn lift_code_model(
    code: &CodeAttribute,
    cp: &ConstantPoolBuilder,
    owner_class_name: &str,
    method_descriptor: &str,
    method_access_flags: MethodAccessFlags,
) -> Result<CodeModel> {
    if !(1..=65535).contains(&code.code_length) {
        return Err(model_error(format!(
            "Code length {} must be between 1 and 65535 bytes",
            code.code_length
        )));
    }
    let end_offset = code.code_length;
    let mut valid_offsets = FxHashSet::default();
    for instruction in &code.code {
        if !valid_offsets.insert(instruction.offset()) {
            return Err(model_error(format!(
                "duplicate instruction offset {}",
                instruction.offset()
            )));
        }
    }

    let mut labels_by_offset = BTreeMap::new();
    for instruction in &code.code {
        register_instruction_labels(instruction, &valid_offsets, &mut labels_by_offset)?;
    }
    for handler in &code.exception_table {
        let start_pc = u32::from(handler.start_pc);
        let end_pc = u32::from(handler.end_pc);
        let handler_pc = u32::from(handler.handler_pc);
        if start_pc >= end_pc {
            return Err(model_error(format!(
                "exception handler start_pc {} must be less than end_pc {}",
                start_pc, end_pc
            )));
        }
        require_valid_instruction_offset(&valid_offsets, start_pc, "exception start_pc")?;
        require_valid_end_offset(&valid_offsets, end_offset, end_pc, "exception end_pc")?;
        require_valid_instruction_offset(&valid_offsets, handler_pc, "exception handler_pc")?;
        label_for_offset(&mut labels_by_offset, start_pc);
        label_for_offset(&mut labels_by_offset, end_pc);
        label_for_offset(&mut labels_by_offset, handler_pc);
    }

    let mut line_numbers = Vec::new();
    let mut local_variables = Vec::new();
    let mut local_variable_types = Vec::new();
    let mut visible_type_annotations = Vec::new();
    let mut invisible_type_annotations = Vec::new();
    let mut attributes = Vec::new();
    let mut layout = Vec::new();
    fn push_layout_once(
        layout: &mut Vec<NestedCodeAttributeLayout>,
        entry: NestedCodeAttributeLayout,
    ) {
        if !layout.contains(&entry) {
            layout.push(entry);
        }
    }
    for attribute in &code.attributes {
        match parse_debug_attribute(
            attribute,
            cp,
            &valid_offsets,
            end_offset,
            code.exception_table.len(),
            &mut labels_by_offset,
        )? {
            ParsedDebugAttribute::LineNumbers(entries) => {
                line_numbers.extend(entries);
                push_layout_once(&mut layout, NestedCodeAttributeLayout::LineNumbers);
            }
            ParsedDebugAttribute::LocalVariables(entries) => {
                local_variables.extend(entries);
                push_layout_once(&mut layout, NestedCodeAttributeLayout::LocalVariables);
            }
            ParsedDebugAttribute::LocalVariableTypes(entries) => {
                local_variable_types.extend(entries);
                push_layout_once(&mut layout, NestedCodeAttributeLayout::LocalVariableTypes);
            }
            ParsedDebugAttribute::VisibleTypeAnnotations(entries) => {
                visible_type_annotations.extend(entries);
                push_layout_once(
                    &mut layout,
                    NestedCodeAttributeLayout::VisibleTypeAnnotations,
                );
            }
            ParsedDebugAttribute::InvisibleTypeAnnotations(entries) => {
                invisible_type_annotations.extend(entries);
                push_layout_once(
                    &mut layout,
                    NestedCodeAttributeLayout::InvisibleTypeAnnotations,
                );
            }
            ParsedDebugAttribute::StackMapTable(attribute) => {
                attributes.push(attribute);
                layout.push(NestedCodeAttributeLayout::StackMapTable);
            }
            ParsedDebugAttribute::Other(attribute) => {
                attributes.push(attribute);
                layout.push(NestedCodeAttributeLayout::Other);
            }
        }
    }

    let mut instructions = Vec::with_capacity(code.code.len() + labels_by_offset.len());
    let mut original_item_offsets = Vec::with_capacity(code.code.len() + labels_by_offset.len());
    for instruction in &code.code {
        if let Some(label) = labels_by_offset.get(&instruction.offset()) {
            instructions.push(CodeItem::Label(label.clone()));
            original_item_offsets.push(None);
        }
        instructions.push(lift_instruction(instruction, cp, &labels_by_offset)?);
        original_item_offsets.push(Some(instruction.offset()));
    }
    if let Some(label) = labels_by_offset.get(&end_offset) {
        instructions.push(CodeItem::Label(label.clone()));
        original_item_offsets.push(None);
    }

    let exception_handlers = code
        .exception_table
        .iter()
        .map(|handler| {
            Ok(ExceptionHandler {
                start: labels_by_offset
                    .get(&(handler.start_pc as u32))
                    .cloned()
                    .ok_or_else(|| model_error("missing start handler label"))?,
                end: labels_by_offset
                    .get(&(handler.end_pc as u32))
                    .cloned()
                    .ok_or_else(|| model_error("missing end handler label"))?,
                handler: labels_by_offset
                    .get(&(handler.handler_pc as u32))
                    .cloned()
                    .ok_or_else(|| model_error("missing handler label"))?,
                catch_type: if handler.catch_type.value() == 0 {
                    None
                } else {
                    Some(cp.resolve_class_name(handler.catch_type)?)
                },
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let original_code_shape = OriginalCodeShape {
        instructions: instructions.clone(),
        exception_handlers: exception_handlers.clone(),
        item_offsets: original_item_offsets,
        code_length: code.code_length,
        owner_class_name: owner_class_name.to_owned(),
        method_descriptor: method_descriptor.to_owned(),
        method_access_flags,
    };

    Ok(CodeModel {
        max_stack: code.max_stack,
        max_locals: code.max_locals,
        instructions,
        exception_handlers,
        line_numbers,
        local_variables,
        local_variable_types,
        visible_type_annotations,
        invisible_type_annotations,
        attributes,
        debug_info_state: DebugInfoState::Fresh,
        nested_attribute_layout: layout,
        original_code_shape: Some(original_code_shape),
    })
}

fn lift_instruction(
    instruction: &Instruction,
    cp: &ConstantPoolBuilder,
    labels_by_offset: &BTreeMap<u32, Label>,
) -> Result<CodeItem> {
    use CodeItem as Item;
    match instruction {
        Instruction::Simple { opcode, .. } => {
            if let Some((canonical_opcode, slot)) = operands::implicit_var_slot(*opcode) {
                return Ok(Item::Var(VarInsn {
                    opcode: canonical_opcode,
                    slot,
                }));
            }
            Ok(Item::Raw(instruction.clone()))
        }
        Instruction::LocalIndex { opcode, index, .. } => Ok(Item::Var(VarInsn {
            opcode: *opcode,
            slot: *index as u16,
        })),
        Instruction::ConstantPoolIndex1 { opcode, index, .. } => {
            lift_cp_index_item(*opcode, CpIndex::from(*index as u16), cp)
        }
        Instruction::ConstantPoolIndexWide(insn) => lift_cp_index_item(insn.opcode, insn.index, cp),
        Instruction::Byte { .. } | Instruction::Short { .. } | Instruction::NewArray(_) => {
            Ok(Item::Raw(instruction.clone()))
        }
        Instruction::Branch(branch) => Ok(Item::Branch(BranchInsn {
            opcode: canonical_branch_opcode(branch.opcode),
            target: target_label(
                labels_by_offset,
                branch.offset as i64 + branch.branch_offset as i64,
            )?,
        })),
        Instruction::BranchWide {
            opcode,
            offset,
            branch_offset,
        } => Ok(Item::Branch(BranchInsn {
            opcode: canonical_branch_opcode(*opcode),
            target: target_label(labels_by_offset, *offset as i64 + *branch_offset as i64)?,
        })),
        Instruction::IInc { index, value, .. } => Ok(Item::IInc(IIncInsn {
            slot: *index as u16,
            value: *value as i16,
        })),
        Instruction::InvokeDynamic(insn) => {
            let entry = cp.entry(insn.index.value())?;
            let ConstantPoolEntry::InvokeDynamic(info) = entry else {
                return Err(model_error(
                    "invokedynamic constant-pool entry is not InvokeDynamic",
                ));
            };
            let (name, descriptor) = cp.resolve_name_and_type(info.name_and_type_index)?;
            Ok(Item::InvokeDynamic(InvokeDynamicInsn {
                bootstrap_method_attr_index: info.bootstrap_method_attr_index,
                name,
                descriptor,
            }))
        }
        Instruction::InvokeInterface(insn) => {
            let (owner, name, descriptor) = cp.resolve_interface_method_ref(insn.index)?;
            Ok(Item::InterfaceMethod(InterfaceMethodInsn {
                owner,
                name,
                descriptor,
            }))
        }
        Instruction::MultiANewArray {
            index, dimensions, ..
        } => Ok(Item::MultiANewArray(MultiANewArrayInsn {
            descriptor: cp.resolve_class_name(*index)?,
            dimensions: *dimensions,
        })),
        Instruction::LookupSwitch(insn) => Ok(Item::LookupSwitch(LookupSwitchInsn {
            default_target: target_label(
                labels_by_offset,
                insn.offset as i64 + insn.default_offset as i64,
            )?,
            pairs: insn
                .pairs
                .iter()
                .map(|pair| {
                    Ok((
                        pair.match_value,
                        target_label(labels_by_offset, insn.offset as i64 + pair.offset as i64)?,
                    ))
                })
                .collect::<Result<Vec<_>>>()?,
        })),
        Instruction::TableSwitch(insn) => Ok(Item::TableSwitch(TableSwitchInsn {
            default_target: target_label(
                labels_by_offset,
                insn.offset as i64 + insn.default_offset as i64,
            )?,
            low: insn.low,
            high: insn.high,
            targets: insn
                .offsets
                .iter()
                .map(|relative| {
                    target_label(labels_by_offset, insn.offset as i64 + *relative as i64)
                })
                .collect::<Result<Vec<_>>>()?,
        })),
        Instruction::Wide(insn) => {
            if insn.opcode == 0x84 {
                let value = insn.value.ok_or_else(|| {
                    model_error("wide iinc instruction is missing increment value")
                })?;
                Ok(Item::IInc(IIncInsn {
                    slot: insn.index,
                    value,
                }))
            } else {
                Ok(Item::Var(VarInsn {
                    opcode: insn.opcode,
                    slot: insn.index,
                }))
            }
        }
    }
}

fn lift_cp_index_item(opcode: u8, index: CpIndex, cp: &ConstantPoolBuilder) -> Result<CodeItem> {
    use CodeItem as Item;
    match opcode {
        0x12..=0x14 => {
            let value = lift_ldc_value(index, cp)?;
            let is_wide_constant = is_ldc2_value(&value);
            if opcode == 0x14 && !is_wide_constant {
                return Err(model_error("ldc2_w requires category-2 constant"));
            }
            if matches!(opcode, 0x12 | 0x13) && is_wide_constant {
                return Err(model_error("ldc/ldc_w cannot load category-2 constant"));
            }
            Ok(Item::Ldc(LdcInsn { value }))
        }
        0xB2..=0xB5 => {
            let (owner, name, descriptor) = cp.resolve_field_ref(index)?;
            Ok(Item::Field(FieldInsn {
                opcode,
                owner,
                name,
                descriptor,
            }))
        }
        0xB6..=0xB8 => {
            let (owner, name, descriptor, is_interface) = cp.resolve_any_method_ref(index)?;
            Ok(Item::Method(MethodInsn {
                opcode,
                owner,
                name,
                descriptor,
                is_interface,
            }))
        }
        0xBB | 0xBD | 0xC0 | 0xC1 => Ok(Item::Type(TypeInsn {
            opcode,
            descriptor: cp.resolve_class_name(ClassIndex::from(index.value()))?,
        })),
        _ => Err(model_error(format!(
            "unsupported constant-pool-backed opcode 0x{opcode:02x}"
        ))),
    }
}

fn lift_ldc_value(index: CpIndex, cp: &ConstantPoolBuilder) -> Result<LdcValue> {
    let entry = cp.entry(index.value())?;
    match entry {
        ConstantPoolEntry::Integer(info) => Ok(LdcValue::Int(info.value_bits as i32)),
        ConstantPoolEntry::Float(info) => Ok(LdcValue::FloatBits(info.value_bits)),
        ConstantPoolEntry::Long(info) => {
            let raw = ((info.high_bits as u64) << 32) | (info.low_bits as u64);
            Ok(LdcValue::Long(raw as i64))
        }
        ConstantPoolEntry::Double(info) => {
            let raw = ((info.high_bits as u64) << 32) | (info.low_bits as u64);
            Ok(LdcValue::DoubleBits(raw))
        }
        ConstantPoolEntry::String(info) => {
            Ok(LdcValue::String(cp.resolve_java_string(info.string_index)?))
        }
        ConstantPoolEntry::Class(info) => Ok(LdcValue::Class(cp.resolve_utf8(info.name_index)?)),
        ConstantPoolEntry::MethodType(info) => Ok(LdcValue::MethodType(
            cp.resolve_utf8(info.descriptor_index)?,
        )),
        ConstantPoolEntry::MethodHandle(info) => {
            let referenced = cp.entry(info.reference_index.value())?;
            let (owner, name, descriptor, is_interface) = match referenced {
                ConstantPoolEntry::FieldRef(_) => {
                    let (owner, name, descriptor) = cp.resolve_field_ref(info.reference_index)?;
                    (owner, name, descriptor, false)
                }
                ConstantPoolEntry::MethodRef(_) => {
                    let (owner, name, descriptor) = cp.resolve_method_ref(info.reference_index)?;
                    (owner, name, descriptor, false)
                }
                ConstantPoolEntry::InterfaceMethodRef(_) => {
                    let (owner, name, descriptor) =
                        cp.resolve_interface_method_ref(info.reference_index)?;
                    (owner, name, descriptor, true)
                }
                _ => {
                    return Err(model_error(
                        "method handle reference is not a field or method reference",
                    ));
                }
            };
            Ok(LdcValue::MethodHandle(MethodHandleValue {
                reference_kind: info.reference_kind,
                owner,
                name,
                descriptor,
                is_interface,
            }))
        }
        ConstantPoolEntry::Dynamic(info) => {
            let (name, descriptor) = cp.resolve_name_and_type(info.name_and_type_index)?;
            Ok(LdcValue::Dynamic(DynamicValue {
                bootstrap_method_attr_index: info.bootstrap_method_attr_index,
                name,
                descriptor,
            }))
        }
        _ => Err(model_error(format!(
            "constant-pool entry {index} is not a valid ldc constant"
        ))),
    }
}

fn lower_field_model(field: &FieldModel, cp: &mut ConstantPoolBuilder) -> Result<FieldInfo> {
    if !is_valid_field_descriptor(&field.descriptor) {
        return Err(model_error(format!(
            "invalid field descriptor {}",
            field.descriptor
        )));
    }
    Ok(FieldInfo {
        access_flags: field.access_flags,
        name_index: cp.add_utf8(&field.name)?,
        descriptor_index: cp.add_utf8(&field.descriptor)?,
        attributes: field.attributes.clone(),
    })
}

fn lower_method_model(
    method: &MethodModel,
    class: &ClassModel,
    cp: &mut ConstantPoolBuilder,
    debug_info: DebugInfoPolicy,
    frame_mode: FrameComputationMode,
    resolver: Option<&dyn ClassResolver>,
    warnings: &mut Vec<FrameWarning>,
) -> Result<MethodInfo> {
    if !is_valid_method_descriptor(&method.descriptor) {
        return Err(model_error(format!(
            "invalid method descriptor {}",
            method.descriptor
        )));
    }
    let parsed_descriptor = parse_method_descriptor(&method.descriptor)?;
    if !method.access_flags.contains(MethodAccessFlags::STATIC)
        && parameter_slot_count(&parsed_descriptor) > 254
    {
        return Err(model_error(format!(
            "instance method {}{} exceeds 254 parameter slots",
            method.name, method.descriptor
        )));
    }
    let lowered_code: Option<AttributeInfo> = match &method.body {
        Some(MethodBody::Prebuilt(code_bytes)) => {
            let name_index = cp.add_utf8("Code")?;
            let code_attr =
                crate::reader::parse_code_attribute_payload(name_index, code_bytes, cp.entries())?;
            if frame_mode == FrameComputationMode::Recompute || debug_info.should_strip() {
                let lifted = lift_code_model(
                    &code_attr,
                    cp,
                    &class.name,
                    &method.descriptor,
                    method.access_flags,
                )?;
                Some(AttributeInfo::Code(lower_code_model(
                    &lifted, method, class, cp, debug_info, frame_mode, resolver, warnings,
                )?))
            } else {
                Some(AttributeInfo::Code(code_attr))
            }
        }
        Some(MethodBody::Symbolic(code)) => Some(AttributeInfo::Code(lower_code_model(
            code, method, class, cp, debug_info, frame_mode, resolver, warnings,
        )?)),
        None => None,
    };
    let mut other_iter = method.attributes.iter().cloned();
    let mut lowered_code = lowered_code;
    let mut code_placed = false;
    let mut attributes =
        Vec::with_capacity(method.attributes.len() + usize::from(lowered_code.is_some()));
    for item in &method.attribute_layout {
        match item {
            MethodAttributeLayout::Other => {
                if let Some(attribute) = other_iter.next() {
                    attributes.push(attribute);
                }
            }
            MethodAttributeLayout::Code => {
                if let Some(code) = lowered_code.take() {
                    attributes.push(code);
                    code_placed = true;
                }
            }
        }
    }
    attributes.extend(other_iter);
    if !code_placed && let Some(code) = lowered_code {
        attributes.push(code);
    }

    Ok(MethodInfo {
        access_flags: method.access_flags,
        name_index: cp.add_utf8(&method.name)?,
        descriptor_index: cp.add_utf8(&method.descriptor)?,
        attributes,
    })
}

/// A conditional branch whose target is out of 16-bit range is emitted as
/// the inverted condition jumping over a `goto_w`. Its fall-through
/// instruction then follows an unconditional jump and is a branch target,
/// so it needs a stack map frame that ordinary frame placement omits.
fn add_inverted_branch_frames(
    code: &CodeModel,
    layout: &CodeLayout,
    frames: &mut FrameComputationResult,
    fallthrough_frames: Vec<StackMapFrameState>,
) {
    if fallthrough_frames.is_empty()
        || !layout
            .branch_encodings
            .contains(&BranchEncoding::InvertedWide)
    {
        return;
    }
    let mut needed = std::collections::HashSet::new();
    for (item_index, item) in code.instructions.iter().enumerate() {
        if matches!(item, CodeItem::Branch(_))
            && layout.branch_encodings[item_index] == BranchEncoding::InvertedWide
            && let Some(next) = code.instructions[item_index + 1..]
                .iter()
                .position(|item| !matches!(item, CodeItem::Label(_)))
        {
            needed.insert(item_index + 1 + next);
        }
    }
    let before = frames.frames.len();
    frames.frames.extend(
        fallthrough_frames
            .into_iter()
            .filter(|frame| needed.contains(&frame.code_index)),
    );
    if frames.frames.len() != before {
        frames.frames.sort_by_key(|frame| frame.code_index);
        frames.frames.dedup_by_key(|frame| frame.code_index);
    }
}

#[allow(clippy::too_many_arguments)]
fn lower_code_model(
    code: &CodeModel,
    method: &MethodModel,
    class: &ClassModel,
    cp: &mut ConstantPoolBuilder,
    debug_info: DebugInfoPolicy,
    frame_mode: FrameComputationMode,
    resolver: Option<&dyn ClassResolver>,
    warnings: &mut Vec<FrameWarning>,
) -> Result<CodeAttribute> {
    let class_major_version = class.version.0;
    let (code_cow, frame_result, catch_remap, fallthrough_frames) =
        if frame_mode == FrameComputationMode::Recompute {
            let (normalized, mut frames, remap, fallthrough_frames) =
                recompute_and_normalize_code(code, class, method, resolver).map_err(|error| {
                    model_error(format!(
                        "frame recomputation failed for {}.{}{}: {error}",
                        class.name, method.name, method.descriptor
                    ))
                })?;
            warnings.append(&mut frames.warnings);
            (normalized, Some(frames), remap, fallthrough_frames)
        } else {
            (std::borrow::Cow::Borrowed(code), None, None, Vec::new())
        };
    let code = code_cow.as_ref();

    let layout = compute_code_layout(code, cp)?;
    let frame_result = frame_result.map(|mut frames| {
        add_inverted_branch_frames(code, &layout, &mut frames, fallthrough_frames);
        frames
    });
    if frame_mode == FrameComputationMode::Preserve {
        ensure_preserve_mode_valid(code, method, class, &layout)?;
    }

    let mut raw_instructions = Vec::with_capacity(code.instructions.len());
    for (item_index, item) in code.instructions.iter().enumerate() {
        if matches!(item, CodeItem::Label(_)) {
            continue;
        }
        let offset = item_offset(
            &layout.item_offsets,
            item_index,
            "instruction offset missing from layout",
        )?;
        lower_instruction_sequence(
            item_index,
            item,
            offset,
            layout.code_length,
            cp,
            &layout.label_offsets,
            &layout.branch_encodings,
            &layout.ldc_indexes,
            &mut raw_instructions,
        )?;
    }

    let exception_table = code
        .exception_handlers
        .iter()
        .map(|handler| {
            let start_offset = *layout
                .label_offsets
                .get(&handler.start)
                .ok_or_else(|| model_error("exception start label missing"))?;
            let end_offset = *layout
                .label_offsets
                .get(&handler.end)
                .ok_or_else(|| model_error("exception end label missing"))?;
            let handler_offset = *layout
                .label_offsets
                .get(&handler.handler)
                .ok_or_else(|| model_error("exception handler label missing"))?;
            if start_offset >= end_offset {
                return Err(model_error(
                    "exception handler start_pc must precede end_pc",
                ));
            }
            if end_offset > layout.code_length {
                return Err(model_error("exception handler end_pc exceeds code length"));
            }
            if handler_offset >= layout.code_length {
                return Err(model_error("exception handler_pc exceeds code length"));
            }
            let start_pc = u16::try_from(start_offset)
                .map_err(|_| model_error("exception start_pc exceeds u16"))?;
            let end_pc = u16::try_from(end_offset)
                .map_err(|_| model_error("exception end_pc exceeds u16"))?;
            let handler_pc = u16::try_from(handler_offset)
                .map_err(|_| model_error("exception handler_pc exceeds u16"))?;
            let catch_type: ClassIndex = match &handler.catch_type {
                Some(name) => cp.add_class(name)?,
                None => ClassIndex::default(),
            };
            Ok(crate::raw::ExceptionHandler {
                start_pc,
                end_pc,
                handler_pc,
                catch_type,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let stack_map_table = match frame_result.as_ref() {
        Some(frames) => {
            if class_major_version >= 50 {
                if frame_result_contains_return_address(frames) {
                    return Err(model_error(
                        "recomputed StackMapTable cannot encode returnAddress states for jsr/ret methods in class file version 50.0 or newer",
                    ));
                }
                if frames.frames.is_empty() {
                    None
                } else {
                    let initial_locals = initial_method_locals(
                        &class.name,
                        &method.name,
                        &method.descriptor,
                        method.access_flags,
                    )
                    .map_err(|error| model_error(error.to_string()))?;
                    Some(lower_stack_map_table(
                        frames,
                        &initial_locals,
                        cp,
                        &layout.item_offsets,
                    )?)
                }
            } else {
                None
            }
        }
        None => None,
    };
    let attributes = lower_nested_attributes(
        code,
        cp,
        &layout.label_offsets,
        layout.code_length,
        exception_table.len(),
        code.debug_info_state,
        debug_info,
        frame_mode,
        stack_map_table,
        catch_remap.as_deref(),
    )?;
    let nested_len: u32 = attributes
        .iter()
        .map(|attr| 6_u32.saturating_add(attribute_payload_len(attr)))
        .sum();
    let attribute_length = 2_u32
        + 2
        + 4
        + layout.code_length
        + 2
        + (exception_table.len() as u32) * 8
        + 2
        + nested_len;
    Ok(CodeAttribute {
        attribute_name_index: cp.add_utf8("Code")?,
        attribute_length,
        max_stack: frame_result
            .as_ref()
            .map_or(code.max_stack, |result| result.max_stack),
        max_locals: frame_result
            .as_ref()
            .map_or(code.max_locals, |result| result.max_locals),
        code_length: layout.code_length,
        code: raw_instructions,
        exception_table,
        attributes,
    })
}

struct CodeLayout {
    branch_encodings: Vec<BranchEncoding>,
    label_offsets: FxHashMap<Label, u32>,
    item_offsets: Vec<Option<u32>>,
    ldc_indexes: Vec<Option<CpIndex>>,
    code_length: u32,
}

fn validate_code_item(item: &CodeItem) -> Result<()> {
    match item {
        CodeItem::Label(_) => Ok(()),
        CodeItem::Raw(insn) => {
            let kind = operand_kind(insn.opcode())?;
            let valid_variant = matches!(
                (kind, insn),
                (
                    crate::raw::instructions::OperandKind::Simple,
                    Instruction::Simple { .. }
                ) | (
                    crate::raw::instructions::OperandKind::LocalIndex,
                    Instruction::LocalIndex { .. }
                ) | (
                    crate::raw::instructions::OperandKind::ConstantPoolIndex1,
                    Instruction::ConstantPoolIndex1 { .. }
                ) | (
                    crate::raw::instructions::OperandKind::ConstantPoolIndexWide,
                    Instruction::ConstantPoolIndexWide(_)
                ) | (
                    crate::raw::instructions::OperandKind::Byte,
                    Instruction::Byte { .. }
                ) | (
                    crate::raw::instructions::OperandKind::Short,
                    Instruction::Short { .. }
                ) | (
                    crate::raw::instructions::OperandKind::Branch,
                    Instruction::Branch(_)
                ) | (
                    crate::raw::instructions::OperandKind::BranchWide,
                    Instruction::BranchWide { .. }
                ) | (
                    crate::raw::instructions::OperandKind::IInc,
                    Instruction::IInc { .. }
                ) | (
                    crate::raw::instructions::OperandKind::InvokeDynamic,
                    Instruction::InvokeDynamic(_)
                ) | (
                    crate::raw::instructions::OperandKind::InvokeInterface,
                    Instruction::InvokeInterface(_)
                ) | (
                    crate::raw::instructions::OperandKind::NewArray,
                    Instruction::NewArray(_)
                ) | (
                    crate::raw::instructions::OperandKind::MultiANewArray,
                    Instruction::MultiANewArray { .. }
                ) | (
                    crate::raw::instructions::OperandKind::LookupSwitch,
                    Instruction::LookupSwitch(_)
                ) | (
                    crate::raw::instructions::OperandKind::TableSwitch,
                    Instruction::TableSwitch(_)
                ) | (
                    crate::raw::instructions::OperandKind::Wide,
                    Instruction::Wide(_)
                )
            );
            if !valid_variant {
                return Err(model_error(format!(
                    "raw instruction variant does not match opcode 0x{:02x}",
                    insn.opcode()
                )));
            }
            Ok(())
        }
        CodeItem::Field(insn) => {
            if !(0xB2..=0xB5).contains(&insn.opcode) {
                return Err(model_error(format!(
                    "invalid field instruction opcode 0x{:02x}",
                    insn.opcode
                )));
            }
            Ok(())
        }
        CodeItem::Method(insn) => {
            if !(0xB6..=0xB8).contains(&insn.opcode) {
                return Err(model_error(format!(
                    "invalid method instruction opcode 0x{:02x}",
                    insn.opcode
                )));
            }
            if insn.opcode == 0xB6 && insn.is_interface {
                return Err(model_error(
                    "invokevirtual (0xb6) cannot target an InterfaceMethodref",
                ));
            }
            Ok(())
        }
        CodeItem::InterfaceMethod(_)
        | CodeItem::InvokeDynamic(_)
        | CodeItem::IInc(_)
        | CodeItem::Ldc(_) => Ok(()),
        CodeItem::Type(insn) => {
            if !matches!(insn.opcode, 0xBB | 0xBD | 0xC0 | 0xC1) {
                return Err(model_error(format!(
                    "invalid type instruction opcode 0x{:02x}",
                    insn.opcode
                )));
            }
            Ok(())
        }
        CodeItem::Var(insn) => {
            if !matches!(insn.opcode, 0x15..=0x19 | 0x36..=0x3A | 0xA9) {
                return Err(model_error(format!(
                    "invalid var instruction opcode 0x{:02x}",
                    insn.opcode
                )));
            }
            Ok(())
        }
        CodeItem::Branch(insn) => {
            if !matches!(insn.opcode, 0x99..=0xA8 | 0xC6..=0xC9) {
                return Err(model_error(format!(
                    "invalid branch instruction opcode 0x{:02x}",
                    insn.opcode
                )));
            }
            Ok(())
        }
        CodeItem::MultiANewArray(insn) => {
            if insn.dimensions == 0 {
                return Err(model_error("multianewarray dimensions must be at least 1"));
            }
            Ok(())
        }
        CodeItem::LookupSwitch(insn) => {
            if insn.pairs.windows(2).any(|pair| pair[0].0 >= pair[1].0) {
                return Err(model_error("lookupswitch keys must be strictly increasing"));
            }
            Ok(())
        }
        CodeItem::TableSwitch(insn) => {
            if insn.low > insn.high {
                return Err(model_error("tableswitch low must not exceed high"));
            }
            let expected = i64::from(insn.high) - i64::from(insn.low) + 1;
            if insn.targets.len() as i64 != expected {
                return Err(model_error(format!(
                    "tableswitch targets count {} does not match range {}..={}",
                    insn.targets.len(),
                    insn.low,
                    insn.high
                )));
            }
            Ok(())
        }
    }
}

fn compute_code_layout(code: &CodeModel, cp: &mut ConstantPoolBuilder) -> Result<CodeLayout> {
    if code.instructions.is_empty() {
        return Err(model_error("Code instructions cannot be empty"));
    }
    for item in &code.instructions {
        validate_code_item(item)?;
    }

    let instruction_count = code.instructions.len();
    let mut branch_encodings = vec![BranchEncoding::Short; instruction_count];
    let mut ldc_indexes = vec![None; instruction_count];
    let max_passes = instruction_count.max(16) + 1;
    let mut converged = false;

    for _ in 0..max_passes {
        let label_offsets =
            compute_label_offsets(&code.instructions, cp, &branch_encodings, &mut ldc_indexes)?;
        let mut next_encodings = branch_encodings.clone();
        let mut changed = false;
        let mut offset = 0_u32;
        for (item_index, item) in code.instructions.iter().enumerate() {
            if matches!(item, CodeItem::Label(_)) {
                continue;
            }
            if let CodeItem::Branch(branch) = item {
                let target = *label_offsets
                    .get(&branch.target)
                    .ok_or_else(|| model_error("branch target label not present"))?;
                let delta = target as i64 - offset as i64;
                let canonical = canonical_branch_opcode(branch.opcode);
                let required =
                    if branch_encodings[item_index] == BranchEncoding::Short && fits_i16(delta) {
                        BranchEncoding::Short
                    } else if matches!(canonical, 0xA7 | 0xA8) {
                        BranchEncoding::Wide
                    } else {
                        BranchEncoding::InvertedWide
                    };
                // Monotonic: once widened, stay widened so fixed-point always converges.
                if branch_encodings[item_index] == BranchEncoding::Short
                    && required != BranchEncoding::Short
                {
                    next_encodings[item_index] = required;
                    changed = true;
                }
            }
            let size = instruction_size(
                item_index,
                item,
                offset,
                cp,
                &branch_encodings,
                &mut ldc_indexes,
            )?;
            offset = offset
                .checked_add(size)
                .ok_or_else(|| model_error("Code byte length overflows u32"))?;
        }
        branch_encodings = next_encodings;
        if !changed {
            converged = true;
            break;
        }
    }
    if !converged {
        return Err(model_error(
            "branch layout failed to converge within pass limit",
        ));
    }

    let mut label_offsets = FxHashMap::default();
    let mut item_offsets = vec![None; instruction_count];
    let mut current = 0_u32;
    for (item_index, item) in code.instructions.iter().enumerate() {
        match item {
            CodeItem::Label(label) => {
                if label_offsets.insert(label.clone(), current).is_some() {
                    return Err(model_error("duplicate label in CodeModel instructions"));
                }
            }
            _ => {
                item_offsets[item_index] = Some(current);
                let size = instruction_size(
                    item_index,
                    item,
                    current,
                    cp,
                    &branch_encodings,
                    &mut ldc_indexes,
                )?;
                current = current
                    .checked_add(size)
                    .ok_or_else(|| model_error("Code byte length overflows u32"))?;
            }
        }
    }
    if !(1..=65535).contains(&current) {
        return Err(model_error(format!(
            "Code length {} out of JVMS range 1..=65535",
            current
        )));
    }
    Ok(CodeLayout {
        branch_encodings,
        label_offsets,
        item_offsets,
        ldc_indexes,
        code_length: current,
    })
}

fn compute_label_offsets(
    items: &[CodeItem],
    cp: &mut ConstantPoolBuilder,
    branch_encodings: &[BranchEncoding],
    ldc_indexes: &mut [Option<CpIndex>],
) -> Result<FxHashMap<Label, u32>> {
    let mut offsets = FxHashMap::default();
    let mut current = 0_u32;
    for (item_index, item) in items.iter().enumerate() {
        match item {
            CodeItem::Label(label) => {
                if offsets.insert(label.clone(), current).is_some() {
                    return Err(model_error("duplicate label in CodeModel instructions"));
                }
            }
            _ => {
                let size =
                    instruction_size(item_index, item, current, cp, branch_encodings, ldc_indexes)?;
                current = current
                    .checked_add(size)
                    .ok_or_else(|| model_error("Code byte length overflows u32"))?;
            }
        }
    }
    Ok(offsets)
}

fn is_ldc2_value(value: &LdcValue) -> bool {
    matches!(value, LdcValue::Long(_) | LdcValue::DoubleBits(_))
        || matches!(value, LdcValue::Dynamic(dynamic) if matches!(dynamic.descriptor.as_str(), "J" | "D"))
}

fn instruction_size(
    item_index: usize,
    item: &CodeItem,
    offset: u32,
    cp: &mut ConstantPoolBuilder,
    branch_encodings: &[BranchEncoding],
    ldc_indexes: &mut [Option<CpIndex>],
) -> Result<u32> {
    Ok(match item {
        CodeItem::Label(_) => 0,
        CodeItem::Raw(instruction) => instruction_length(instruction, offset),
        CodeItem::Field(_) | CodeItem::Method(_) | CodeItem::Type(_) => 3,
        CodeItem::InvokeDynamic(_) => 5,
        CodeItem::InterfaceMethod(_) => 5,
        CodeItem::MultiANewArray(_) => 4,
        CodeItem::Branch(_) => match branch_encodings[item_index] {
            BranchEncoding::Short => 3,
            BranchEncoding::Wide => 5,
            BranchEncoding::InvertedWide => 8,
        },
        CodeItem::LookupSwitch(insn) => {
            let padding = switch_padding(offset);
            let pairs_bytes = u32::try_from(insn.pairs.len())
                .ok()
                .and_then(|count| count.checked_mul(8))
                .ok_or_else(|| model_error("lookupswitch size overflows u32"))?;
            1_u32
                .checked_add(padding)
                .and_then(|v| v.checked_add(8))
                .and_then(|v| v.checked_add(pairs_bytes))
                .ok_or_else(|| model_error("lookupswitch size overflows u32"))?
        }
        CodeItem::TableSwitch(insn) => {
            let padding = switch_padding(offset);
            let targets_bytes = u32::try_from(insn.targets.len())
                .ok()
                .and_then(|count| count.checked_mul(4))
                .ok_or_else(|| model_error("tableswitch size overflows u32"))?;
            1_u32
                .checked_add(padding)
                .and_then(|v| v.checked_add(12))
                .and_then(|v| v.checked_add(targets_bytes))
                .ok_or_else(|| model_error("tableswitch size overflows u32"))?
        }
        CodeItem::Var(var) => {
            if operands::var_shortcut_opcode(var.opcode, var.slot).is_some() {
                1
            } else if var.slot <= u8::MAX as u16 {
                2
            } else {
                4
            }
        }
        CodeItem::IInc(insn) => {
            if insn.slot <= u8::MAX as u16 && i8::try_from(insn.value).is_ok() {
                3
            } else {
                6
            }
        }
        CodeItem::Ldc(insn) => {
            let index = if let Some(index) = ldc_indexes[item_index] {
                index
            } else {
                let index = add_ldc_value(cp, &insn.value)?;
                ldc_indexes[item_index] = Some(index);
                index
            };
            debug_assert!(index.value() > 0);
            if is_ldc2_value(&insn.value) || index.value() > u8::MAX as u16 {
                3
            } else {
                2
            }
        }
    })
}

#[allow(clippy::too_many_arguments)]
fn lower_instruction_sequence(
    item_index: usize,
    item: &CodeItem,
    offset: u32,
    code_length: u32,
    cp: &mut ConstantPoolBuilder,
    label_offsets: &FxHashMap<Label, u32>,
    branch_encodings: &[BranchEncoding],
    ldc_indexes: &[Option<CpIndex>],
    out: &mut Vec<Instruction>,
) -> Result<()> {
    match item {
        CodeItem::Label(_) => return Err(model_error("labels do not lower to raw instructions")),
        CodeItem::Raw(instruction) => out.push(rebase_instruction(instruction, offset)),
        CodeItem::Field(insn) => out.push(Instruction::ConstantPoolIndexWide(
            crate::raw::ConstantPoolIndexWide {
                opcode: insn.opcode,
                offset,
                index: cp.add_field_ref(&insn.owner, &insn.name, &insn.descriptor)?,
            },
        )),
        CodeItem::Method(insn) => out.push(Instruction::ConstantPoolIndexWide(
            crate::raw::ConstantPoolIndexWide {
                opcode: insn.opcode,
                offset,
                index: if insn.is_interface {
                    cp.add_interface_method_ref(&insn.owner, &insn.name, &insn.descriptor)?
                } else {
                    cp.add_method_ref(&insn.owner, &insn.name, &insn.descriptor)?
                },
            },
        )),
        CodeItem::InterfaceMethod(insn) => {
            out.push(Instruction::InvokeInterface(RawInvokeInterfaceInsn {
                offset,
                index: cp.add_interface_method_ref(&insn.owner, &insn.name, &insn.descriptor)?,
                count: operands::interface_method_count(&insn.descriptor)?,
                reserved: 0,
            }))
        }
        CodeItem::Type(insn) => out.push(Instruction::ConstantPoolIndexWide(
            crate::raw::ConstantPoolIndexWide {
                opcode: insn.opcode,
                offset,
                index: cp.add_class(&insn.descriptor)?.into(),
            },
        )),
        CodeItem::Var(insn) => {
            if let Some(opcode) = operands::var_shortcut_opcode(insn.opcode, insn.slot) {
                out.push(Instruction::Simple { opcode, offset });
            } else if insn.slot <= u8::MAX as u16 {
                out.push(Instruction::LocalIndex {
                    opcode: insn.opcode,
                    offset,
                    index: insn.slot as u8,
                });
            } else {
                out.push(Instruction::Wide(WideInstruction {
                    offset,
                    opcode: insn.opcode,
                    index: insn.slot,
                    value: None,
                }));
            }
        }
        CodeItem::IInc(insn) => {
            if insn.slot <= u8::MAX as u16 && i8::try_from(insn.value).is_ok() {
                out.push(Instruction::IInc {
                    offset,
                    index: insn.slot as u8,
                    value: insn.value as i8,
                });
            } else {
                out.push(Instruction::Wide(WideInstruction {
                    offset,
                    opcode: 0x84,
                    index: insn.slot,
                    value: Some(insn.value),
                }));
            }
        }
        CodeItem::Ldc(insn) => out.push(lower_ldc_instruction(
            &insn.value,
            offset,
            cp,
            ldc_indexes[item_index],
        )?),
        CodeItem::InvokeDynamic(insn) => {
            out.push(Instruction::InvokeDynamic(RawInvokeDynamicInsn {
                offset,
                index: cp.add_invoke_dynamic(
                    insn.bootstrap_method_attr_index,
                    &insn.name,
                    &insn.descriptor,
                )?,
                reserved: 0,
            }))
        }
        CodeItem::MultiANewArray(insn) => out.push(Instruction::MultiANewArray {
            offset,
            index: cp.add_class(&insn.descriptor)?,
            dimensions: insn.dimensions,
        }),
        CodeItem::Branch(insn) => {
            let target = *label_offsets
                .get(&insn.target)
                .ok_or_else(|| model_error("branch target label missing"))?;
            if target >= code_length {
                return Err(model_error("branch target offset exceeds code length"));
            }
            let canonical_opcode = canonical_branch_opcode(insn.opcode);
            let delta_from_here = target as i64 - offset as i64;
            match branch_encodings[item_index] {
                BranchEncoding::Short => out.push(Instruction::Branch(Branch {
                    opcode: canonical_opcode,
                    offset,
                    branch_offset: i16::try_from(delta_from_here)
                        .map_err(|_| model_error("branch offset exceeds i16"))?,
                })),
                BranchEncoding::Wide => out.push(Instruction::BranchWide {
                    opcode: if canonical_opcode == 0xA7 { 0xC8 } else { 0xC9 },
                    offset,
                    branch_offset: i32::try_from(delta_from_here)
                        .map_err(|_| model_error("wide branch offset exceeds i32"))?,
                }),
                BranchEncoding::InvertedWide => {
                    let inverted_opcode = invert_conditional_branch_opcode(canonical_opcode)?;
                    let goto_offset = offset + 3;
                    let goto_delta = target as i64 - goto_offset as i64;
                    out.push(Instruction::Branch(Branch {
                        opcode: inverted_opcode,
                        offset,
                        branch_offset: 8,
                    }));
                    out.push(Instruction::BranchWide {
                        opcode: 0xC8,
                        offset: goto_offset,
                        branch_offset: i32::try_from(goto_delta)
                            .map_err(|_| model_error("wide branch offset exceeds i32"))?,
                    });
                }
            }
        }
        CodeItem::LookupSwitch(insn) => out.push(Instruction::LookupSwitch(RawLookupSwitchInsn {
            offset,
            default_offset: switch_target_offset(
                offset,
                code_length,
                label_offsets,
                &insn.default_target,
            )?,
            pairs: insn
                .pairs
                .iter()
                .map(|(match_value, label)| {
                    Ok(MatchOffsetPair {
                        match_value: *match_value,
                        offset: switch_target_offset(offset, code_length, label_offsets, label)?,
                    })
                })
                .collect::<Result<Vec<_>>>()?,
        })),
        CodeItem::TableSwitch(insn) => out.push(Instruction::TableSwitch(RawTableSwitchInsn {
            offset,
            default_offset: switch_target_offset(
                offset,
                code_length,
                label_offsets,
                &insn.default_target,
            )?,
            low: insn.low,
            high: insn.high,
            offsets: insn
                .targets
                .iter()
                .map(|label| switch_target_offset(offset, code_length, label_offsets, label))
                .collect::<Result<Vec<_>>>()?,
        })),
    }
    Ok(())
}

fn lower_ldc_instruction(
    value: &LdcValue,
    offset: u32,
    cp: &mut ConstantPoolBuilder,
    cached_index: Option<CpIndex>,
) -> Result<Instruction> {
    let index = match cached_index {
        Some(index) => index,
        None => add_ldc_value(cp, value)?,
    };
    Ok(if is_ldc2_value(value) {
        Instruction::ConstantPoolIndexWide(crate::raw::ConstantPoolIndexWide {
            opcode: 0x14,
            offset,
            index,
        })
    } else if index.value() <= u8::MAX as u16 {
        Instruction::ConstantPoolIndex1 {
            opcode: 0x12,
            offset,
            index: index.value() as u8,
        }
    } else {
        Instruction::ConstantPoolIndexWide(crate::raw::ConstantPoolIndexWide {
            opcode: 0x13,
            offset,
            index,
        })
    })
}

fn add_ldc_value(cp: &mut ConstantPoolBuilder, value: &LdcValue) -> Result<CpIndex> {
    match value {
        LdcValue::Int(value) => cp.add_integer(*value),
        LdcValue::FloatBits(raw_bits) => cp.add_float_bits(*raw_bits),
        LdcValue::Long(value) => cp.add_long(*value),
        LdcValue::DoubleBits(raw_bits) => cp.add_double_bits(*raw_bits),
        LdcValue::String(value) => cp.add_java_string(value),
        LdcValue::Class(value) => Ok(cp.add_class(value)?.into()),
        LdcValue::MethodType(value) => cp.add_method_type(value),
        LdcValue::MethodHandle(value) => {
            if !(1..=9).contains(&value.reference_kind) {
                return Err(model_error(format!(
                    "invalid method handle reference kind: {}",
                    value.reference_kind
                )));
            }
            let reference_index = if matches!(value.reference_kind, 1..=4) {
                if value.is_interface {
                    return Err(model_error(
                        "field method handle reference kind cannot set is_interface",
                    ));
                }
                cp.add_field_ref(&value.owner, &value.name, &value.descriptor)?
            } else if value.is_interface || value.reference_kind == 9 {
                cp.add_interface_method_ref(&value.owner, &value.name, &value.descriptor)?
            } else {
                cp.add_method_ref(&value.owner, &value.name, &value.descriptor)?
            };
            cp.add_method_handle(value.reference_kind, reference_index)
        }
        LdcValue::Dynamic(value) => cp.add_dynamic(
            value.bootstrap_method_attr_index,
            &value.name,
            &value.descriptor,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn lower_nested_attributes(
    code: &CodeModel,
    cp: &mut ConstantPoolBuilder,
    label_offsets: &FxHashMap<Label, u32>,
    code_length: u32,
    exception_table_len: usize,
    debug_info_state: DebugInfoState,
    debug_info: DebugInfoPolicy,
    frame_mode: FrameComputationMode,
    stack_map_table: Option<AttributeInfo>,
    catch_remap: Option<&[Option<usize>]>,
) -> Result<Vec<AttributeInfo>> {
    let strip_debug_info = debug_info.should_strip() || debug_info_state == DebugInfoState::Stale;
    let line_number_attr = if strip_debug_info || code.line_numbers.is_empty() {
        None
    } else {
        Some(lower_line_number_table(
            &code.line_numbers,
            cp,
            label_offsets,
            code_length,
        )?)
    };
    let local_variable_attr = if strip_debug_info || code.local_variables.is_empty() {
        None
    } else {
        Some(lower_local_variable_table(
            &code.local_variables,
            cp,
            label_offsets,
            code_length,
        )?)
    };
    let local_variable_type_attr = if strip_debug_info || code.local_variable_types.is_empty() {
        None
    } else {
        Some(lower_local_variable_type_table(
            &code.local_variable_types,
            cp,
            label_offsets,
            code_length,
        )?)
    };
    let visible_type_attr = lower_code_type_annotations(
        &code.visible_type_annotations,
        true,
        cp,
        label_offsets,
        code_length,
        exception_table_len,
        catch_remap,
    )?;
    let invisible_type_attr = lower_code_type_annotations(
        &code.invisible_type_annotations,
        false,
        cp,
        label_offsets,
        code_length,
        exception_table_len,
        catch_remap,
    )?;

    let mut stack_maps_iter = if frame_mode == FrameComputationMode::Recompute {
        Vec::new().into_iter()
    } else {
        code.attributes
            .iter()
            .filter(|attribute| stack_map_attr_name(attribute).is_some())
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
    };
    let mut other_iter = code
        .attributes
        .iter()
        .filter(|attribute| {
            stack_map_attr_name(attribute).is_none()
                && !(strip_debug_info && is_nested_debug_attribute(attribute))
        })
        .cloned();
    let mut line_number_attr = line_number_attr;
    let mut local_variable_attr = local_variable_attr;
    let mut local_variable_type_attr = local_variable_type_attr;
    let mut visible_type_attr = visible_type_attr;
    let mut invisible_type_attr = invisible_type_attr;
    let mut stack_map_table = stack_map_table;
    let mut attributes = Vec::with_capacity(code.attributes.len() + 6);
    for item in &code.nested_attribute_layout {
        match item {
            NestedCodeAttributeLayout::Other => {
                if let Some(attribute) = other_iter.next() {
                    attributes.push(attribute);
                }
            }
            NestedCodeAttributeLayout::LineNumbers => {
                if let Some(attribute) = line_number_attr.take() {
                    attributes.push(attribute);
                }
            }
            NestedCodeAttributeLayout::LocalVariables => {
                if let Some(attribute) = local_variable_attr.take() {
                    attributes.push(attribute);
                }
            }
            NestedCodeAttributeLayout::LocalVariableTypes => {
                if let Some(attribute) = local_variable_type_attr.take() {
                    attributes.push(attribute);
                }
            }
            NestedCodeAttributeLayout::VisibleTypeAnnotations => {
                if let Some(attribute) = visible_type_attr.take() {
                    attributes.push(attribute);
                }
            }
            NestedCodeAttributeLayout::InvisibleTypeAnnotations => {
                if let Some(attribute) = invisible_type_attr.take() {
                    attributes.push(attribute);
                }
            }
            NestedCodeAttributeLayout::StackMapTable => {
                if frame_mode == FrameComputationMode::Recompute {
                    if let Some(attribute) = stack_map_table.take() {
                        attributes.push(attribute);
                    }
                } else if let Some(attribute) = stack_maps_iter.next() {
                    attributes.push(attribute);
                }
            }
        }
    }

    attributes.extend(other_iter);
    attributes.extend(stack_maps_iter);
    if let Some(attribute) = line_number_attr {
        attributes.push(attribute);
    }
    if let Some(attribute) = local_variable_attr {
        attributes.push(attribute);
    }
    if let Some(attribute) = local_variable_type_attr {
        attributes.push(attribute);
    }
    if let Some(attribute) = visible_type_attr {
        attributes.push(attribute);
    }
    if let Some(attribute) = invisible_type_attr {
        attributes.push(attribute);
    }
    if let Some(attribute) = stack_map_table {
        attributes.push(attribute);
    }
    Ok(attributes)
}

fn item_offset(
    item_offsets: &[Option<u32>],
    item_index: usize,
    message: &'static str,
) -> Result<u32> {
    item_offsets
        .get(item_index)
        .copied()
        .flatten()
        .ok_or_else(|| model_error(message))
}

fn frame_result_contains_return_address(frames: &FrameComputationResult) -> bool {
    frames.frames.iter().any(|frame| {
        frame
            .locals
            .iter()
            .chain(&frame.stack)
            .any(|value| matches!(value, VType::ReturnAddress(_)))
    })
}

fn lower_line_number_table(
    entries: &[LineNumberEntry],
    cp: &mut ConstantPoolBuilder,
    label_offsets: &FxHashMap<Label, u32>,
    code_length: u32,
) -> Result<AttributeInfo> {
    let line_number_table = entries
        .iter()
        .map(|entry| {
            let offset = *label_offsets
                .get(&entry.label)
                .ok_or_else(|| model_error("line number label missing"))?;
            if offset >= code_length {
                return Err(model_error("line number start_pc exceeds code length"));
            }
            Ok(crate::raw::LineNumberInfo {
                start_pc: u16::try_from(offset)
                    .map_err(|_| model_error("line number start_pc exceeds u16"))?,
                line_number: entry.line_number,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let attribute_length = 2_u32 + (line_number_table.len() as u32) * 4;
    Ok(AttributeInfo::LineNumberTable(
        crate::raw::LineNumberTableAttribute {
            attribute_name_index: cp.add_utf8("LineNumberTable")?,
            attribute_length,
            line_number_table,
        },
    ))
}

fn lower_local_variable_table(
    entries: &[LocalVariableEntry],
    cp: &mut ConstantPoolBuilder,
    label_offsets: &FxHashMap<Label, u32>,
    code_length: u32,
) -> Result<AttributeInfo> {
    let local_variable_table = entries
        .iter()
        .map(|entry| {
            let start_pc = *label_offsets
                .get(&entry.start)
                .ok_or_else(|| model_error("local variable start label missing"))?;
            let end_pc = *label_offsets
                .get(&entry.end)
                .ok_or_else(|| model_error("local variable end label missing"))?;
            if end_pc < start_pc {
                return Err(model_error("local variable end precedes start"));
            }
            if end_pc > code_length {
                return Err(model_error("local variable range exceeds code length"));
            }
            Ok(crate::raw::LocalVariableInfo {
                start_pc: u16::try_from(start_pc)
                    .map_err(|_| model_error("local variable start_pc exceeds u16"))?,
                length: u16::try_from(end_pc - start_pc)
                    .map_err(|_| model_error("local variable length exceeds u16"))?,
                name_index: cp.add_utf8(&entry.name)?,
                descriptor_index: cp.add_utf8(&entry.descriptor)?,
                index: entry.index,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let attribute_length = 2_u32 + (local_variable_table.len() as u32) * 10;
    Ok(AttributeInfo::LocalVariableTable(
        crate::raw::LocalVariableTableAttribute {
            attribute_name_index: cp.add_utf8("LocalVariableTable")?,
            attribute_length,
            local_variable_table,
        },
    ))
}

fn lower_local_variable_type_table(
    entries: &[LocalVariableTypeEntry],
    cp: &mut ConstantPoolBuilder,
    label_offsets: &FxHashMap<Label, u32>,
    code_length: u32,
) -> Result<AttributeInfo> {
    let local_variable_type_table = entries
        .iter()
        .map(|entry| {
            let start_pc = *label_offsets
                .get(&entry.start)
                .ok_or_else(|| model_error("local variable type start label missing"))?;
            let end_pc = *label_offsets
                .get(&entry.end)
                .ok_or_else(|| model_error("local variable type end label missing"))?;
            if end_pc < start_pc {
                return Err(model_error("local variable type end precedes start"));
            }
            if end_pc > code_length {
                return Err(model_error("local variable type range exceeds code length"));
            }
            Ok(crate::raw::LocalVariableTypeInfo {
                start_pc: u16::try_from(start_pc)
                    .map_err(|_| model_error("local variable type start_pc exceeds u16"))?,
                length: u16::try_from(end_pc - start_pc)
                    .map_err(|_| model_error("local variable type length exceeds u16"))?,
                name_index: cp.add_utf8(&entry.name)?,
                signature_index: cp.add_utf8(&entry.signature)?,
                index: entry.index,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let attribute_length = 2_u32 + (local_variable_type_table.len() as u32) * 10;
    Ok(AttributeInfo::LocalVariableTypeTable(
        crate::raw::LocalVariableTypeTableAttribute {
            attribute_name_index: cp.add_utf8("LocalVariableTypeTable")?,
            attribute_length,
            local_variable_type_table,
        },
    ))
}

fn lower_code_type_annotations(
    entries: &[CodeTypeAnnotation],
    visible: bool,
    cp: &mut ConstantPoolBuilder,
    label_offsets: &FxHashMap<Label, u32>,
    code_length: u32,
    exception_table_len: usize,
    catch_remap: Option<&[Option<usize>]>,
) -> Result<Option<AttributeInfo>> {
    if entries.is_empty() {
        return Ok(None);
    }
    let mut annotations = Vec::with_capacity(entries.len());
    for entry in entries {
        let target_info = match &entry.target {
            CodeTypeAnnotationTarget::Localvar { table } => {
                let raw_table = table
                    .iter()
                    .map(|row| match row {
                        LocalvarTargetEntry::NoLiveRange { index } => Ok(TableInfo {
                            start_pc: 0xFFFF,
                            length: 0xFFFF,
                            index: *index,
                        }),
                        LocalvarTargetEntry::Range { start, end, index } => {
                            let start_pc = *label_offsets.get(start).ok_or_else(|| {
                                model_error("type-annotation localvar start label missing")
                            })?;
                            let end_pc = *label_offsets.get(end).ok_or_else(|| {
                                model_error("type-annotation localvar end label missing")
                            })?;
                            if end_pc < start_pc {
                                return Err(model_error(
                                    "type-annotation localvar end precedes start",
                                ));
                            }
                            if end_pc > code_length {
                                return Err(model_error(
                                    "type-annotation localvar range exceeds code length",
                                ));
                            }
                            Ok(TableInfo {
                                start_pc: u16::try_from(start_pc).map_err(|_| {
                                    model_error("type-annotation localvar start_pc exceeds u16")
                                })?,
                                length: u16::try_from(end_pc - start_pc).map_err(|_| {
                                    model_error("type-annotation localvar length exceeds u16")
                                })?,
                                index: *index,
                            })
                        }
                    })
                    .collect::<Result<Vec<_>>>()?;
                TargetInfo::Localvar { table: raw_table }
            }
            CodeTypeAnnotationTarget::Catch {
                exception_table_index,
            } => {
                let mapped_index = match catch_remap {
                    Some(remap) => {
                        let Some(&mapped) = remap.get(*exception_table_index) else {
                            return Err(model_error(format!(
                                "type-annotation catch index {} exceeds exception table length {}",
                                exception_table_index,
                                remap.len()
                            )));
                        };
                        match mapped {
                            Some(idx) => idx,
                            None => continue,
                        }
                    }
                    None => *exception_table_index,
                };
                if mapped_index >= exception_table_len {
                    return Err(model_error(format!(
                        "type-annotation catch index {} exceeds exception table length {}",
                        mapped_index, exception_table_len
                    )));
                }
                TargetInfo::Catch {
                    exception_table_index: u16::try_from(mapped_index)
                        .map_err(|_| model_error("type-annotation catch index exceeds u16"))?,
                }
            }
            CodeTypeAnnotationTarget::Offset { target } => {
                let offset = *label_offsets
                    .get(target)
                    .ok_or_else(|| model_error("type-annotation target label missing"))?;
                if offset >= code_length {
                    return Err(model_error(
                        "type-annotation target offset exceeds code length",
                    ));
                }
                TargetInfo::Offset {
                    offset: u16::try_from(offset)
                        .map_err(|_| model_error("type-annotation target offset exceeds u16"))?,
                }
            }
            CodeTypeAnnotationTarget::TypeArgument {
                target,
                type_argument_index,
            } => {
                let offset = *label_offsets
                    .get(target)
                    .ok_or_else(|| model_error("type-annotation target label missing"))?;
                if offset >= code_length {
                    return Err(model_error(
                        "type-annotation target offset exceeds code length",
                    ));
                }
                TargetInfo::TypeArgument {
                    offset: u16::try_from(offset)
                        .map_err(|_| model_error("type-annotation target offset exceeds u16"))?,
                    type_argument_index: *type_argument_index,
                }
            }
            CodeTypeAnnotationTarget::Other(info) => info.clone(),
        };
        annotations.push(TypeAnnotationInfo {
            target_type: entry.target_type,
            target_info,
            target_path: entry.target_path.clone(),
            type_index: entry.type_index,
            element_value_pairs: entry.element_value_pairs.clone(),
        });
    }
    if annotations.is_empty() {
        return Ok(None);
    }
    let attribute_length = 2_u32
        + annotations
            .iter()
            .map(type_annotation_encoded_len)
            .sum::<u32>();
    Ok(Some(if visible {
        AttributeInfo::RuntimeVisibleTypeAnnotations(RuntimeVisibleTypeAnnotationsAttribute {
            attribute_name_index: cp.add_utf8("RuntimeVisibleTypeAnnotations")?,
            attribute_length,
            annotations,
        })
    } else {
        AttributeInfo::RuntimeInvisibleTypeAnnotations(RuntimeInvisibleTypeAnnotationsAttribute {
            attribute_name_index: cp.add_utf8("RuntimeInvisibleTypeAnnotations")?,
            attribute_length,
            annotations,
        })
    }))
}

fn is_class_debug_attribute(attribute: &AttributeInfo) -> bool {
    matches!(
        attribute,
        AttributeInfo::SourceFile(_) | AttributeInfo::SourceDebugExtension(_)
    ) || matches!(
        attribute,
        AttributeInfo::Unknown(unknown)
            if matches!(unknown.name.as_str(), "SourceFile" | "SourceDebugExtension")
    )
}

fn is_nested_debug_attribute(attribute: &AttributeInfo) -> bool {
    matches!(
        attribute,
        AttributeInfo::LineNumberTable(_)
            | AttributeInfo::LocalVariableTable(_)
            | AttributeInfo::LocalVariableTypeTable(_)
    ) || matches!(
        attribute,
        AttributeInfo::Unknown(unknown)
            if matches!(
                unknown.name.as_str(),
                "LineNumberTable" | "LocalVariableTable" | "LocalVariableTypeTable"
            )
    )
}

fn class_attributes_for_policy(
    attributes: &[AttributeInfo],
    debug_info_state: DebugInfoState,
    debug_info: DebugInfoPolicy,
) -> Vec<AttributeInfo> {
    if !debug_info.should_strip() && debug_info_state != DebugInfoState::Stale {
        return attributes.to_vec();
    }
    attributes
        .iter()
        .filter(|attribute| !is_class_debug_attribute(attribute))
        .cloned()
        .collect()
}

enum ParsedDebugAttribute {
    LineNumbers(Vec<LineNumberEntry>),
    LocalVariables(Vec<LocalVariableEntry>),
    LocalVariableTypes(Vec<LocalVariableTypeEntry>),
    VisibleTypeAnnotations(Vec<CodeTypeAnnotation>),
    InvisibleTypeAnnotations(Vec<CodeTypeAnnotation>),
    StackMapTable(AttributeInfo),
    Other(AttributeInfo),
}

fn parse_debug_attribute(
    attribute: &AttributeInfo,
    cp: &ConstantPoolBuilder,
    valid_offsets: &FxHashSet<u32>,
    end_offset: u32,
    exception_table_len: usize,
    labels_by_offset: &mut BTreeMap<u32, Label>,
) -> Result<ParsedDebugAttribute> {
    match attribute {
        AttributeInfo::LineNumberTable(attribute) => Ok(ParsedDebugAttribute::LineNumbers(
            attribute
                .line_number_table
                .iter()
                .map(|entry| {
                    let start_pc = u32::from(entry.start_pc);
                    require_valid_instruction_offset(
                        valid_offsets,
                        start_pc,
                        "LineNumberTable start_pc",
                    )?;
                    Ok(LineNumberEntry {
                        label: label_for_offset(labels_by_offset, start_pc),
                        line_number: entry.line_number,
                    })
                })
                .collect::<Result<Vec<_>>>()?,
        )),
        AttributeInfo::LocalVariableTable(attribute) => Ok(ParsedDebugAttribute::LocalVariables(
            attribute
                .local_variable_table
                .iter()
                .map(|entry| {
                    let start_pc = u32::from(entry.start_pc);
                    let end_pc = start_pc + u32::from(entry.length);
                    require_valid_instruction_offset(
                        valid_offsets,
                        start_pc,
                        "LocalVariableTable start_pc",
                    )?;
                    require_valid_end_offset(
                        valid_offsets,
                        end_offset,
                        end_pc,
                        "LocalVariableTable end_pc",
                    )?;
                    Ok(LocalVariableEntry {
                        start: label_for_offset(labels_by_offset, start_pc),
                        end: label_for_offset(labels_by_offset, end_pc),
                        name: cp.resolve_utf8(entry.name_index)?,
                        descriptor: cp.resolve_utf8(entry.descriptor_index)?,
                        index: entry.index,
                    })
                })
                .collect::<Result<Vec<_>>>()?,
        )),
        AttributeInfo::LocalVariableTypeTable(attribute) => {
            Ok(ParsedDebugAttribute::LocalVariableTypes(
                attribute
                    .local_variable_type_table
                    .iter()
                    .map(|entry| {
                        let start_pc = u32::from(entry.start_pc);
                        let end_pc = start_pc + u32::from(entry.length);
                        require_valid_instruction_offset(
                            valid_offsets,
                            start_pc,
                            "LocalVariableTypeTable start_pc",
                        )?;
                        require_valid_end_offset(
                            valid_offsets,
                            end_offset,
                            end_pc,
                            "LocalVariableTypeTable end_pc",
                        )?;
                        Ok(LocalVariableTypeEntry {
                            start: label_for_offset(labels_by_offset, start_pc),
                            end: label_for_offset(labels_by_offset, end_pc),
                            name: cp.resolve_utf8(entry.name_index)?,
                            signature: cp.resolve_utf8(entry.signature_index)?,
                            index: entry.index,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?,
            ))
        }
        AttributeInfo::RuntimeVisibleTypeAnnotations(attribute) => {
            Ok(ParsedDebugAttribute::VisibleTypeAnnotations(
                attribute
                    .annotations
                    .iter()
                    .map(|annotation| {
                        lift_code_type_annotation(
                            annotation,
                            valid_offsets,
                            end_offset,
                            exception_table_len,
                            labels_by_offset,
                        )
                    })
                    .collect::<Result<Vec<_>>>()?,
            ))
        }
        AttributeInfo::RuntimeInvisibleTypeAnnotations(attribute) => {
            Ok(ParsedDebugAttribute::InvisibleTypeAnnotations(
                attribute
                    .annotations
                    .iter()
                    .map(|annotation| {
                        lift_code_type_annotation(
                            annotation,
                            valid_offsets,
                            end_offset,
                            exception_table_len,
                            labels_by_offset,
                        )
                    })
                    .collect::<Result<Vec<_>>>()?,
            ))
        }
        AttributeInfo::Unknown(unknown) => {
            match unknown.name.as_str() {
                "LineNumberTable" => Ok(ParsedDebugAttribute::LineNumbers(
                    parse_line_number_table(&unknown.info, valid_offsets, labels_by_offset)?,
                )),
                "LocalVariableTable" => Ok(ParsedDebugAttribute::LocalVariables(
                    parse_local_variable_table(
                        &unknown.info,
                        cp,
                        valid_offsets,
                        end_offset,
                        labels_by_offset,
                    )?,
                )),
                "LocalVariableTypeTable" => Ok(ParsedDebugAttribute::LocalVariableTypes(
                    parse_local_variable_type_table(
                        &unknown.info,
                        cp,
                        valid_offsets,
                        end_offset,
                        labels_by_offset,
                    )?,
                )),
                "StackMapTable" | "StackMap" => {
                    Ok(ParsedDebugAttribute::StackMapTable(attribute.clone()))
                }
                _ => Ok(ParsedDebugAttribute::Other(attribute.clone())),
            }
        }
        _ if stack_map_attr_name(attribute).is_some() => {
            Ok(ParsedDebugAttribute::StackMapTable(attribute.clone()))
        }
        _ => Ok(ParsedDebugAttribute::Other(attribute.clone())),
    }
}

fn lift_code_type_annotation(
    annotation: &TypeAnnotationInfo,
    valid_offsets: &FxHashSet<u32>,
    end_offset: u32,
    exception_table_len: usize,
    labels_by_offset: &mut BTreeMap<u32, Label>,
) -> Result<CodeTypeAnnotation> {
    let target = match &annotation.target_info {
        TargetInfo::Localvar { table } => {
            let entries = table
                .iter()
                .map(|row| {
                    if row.start_pc == 0xFFFF && row.length == 0xFFFF && row.index == 0xFFFF {
                        return Ok(LocalvarTargetEntry::NoLiveRange { index: row.index });
                    }
                    let start_pc = u32::from(row.start_pc);
                    let end_pc = start_pc + u32::from(row.length);
                    require_valid_instruction_offset(
                        valid_offsets,
                        start_pc,
                        "type-annotation localvar start_pc",
                    )?;
                    require_valid_end_offset(
                        valid_offsets,
                        end_offset,
                        end_pc,
                        "type-annotation localvar end_pc",
                    )?;
                    Ok(LocalvarTargetEntry::Range {
                        start: label_for_offset(labels_by_offset, start_pc),
                        end: label_for_offset(labels_by_offset, end_pc),
                        index: row.index,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            CodeTypeAnnotationTarget::Localvar { table: entries }
        }
        TargetInfo::Catch {
            exception_table_index,
        } => {
            let index = usize::from(*exception_table_index);
            if index >= exception_table_len {
                return Err(model_error(format!(
                    "type-annotation catch index {} out of bounds for exception table length {}",
                    index, exception_table_len
                )));
            }
            CodeTypeAnnotationTarget::Catch {
                exception_table_index: index,
            }
        }
        TargetInfo::Offset { offset } => {
            let target_offset = u32::from(*offset);
            require_valid_instruction_offset(
                valid_offsets,
                target_offset,
                "type-annotation offset",
            )?;
            CodeTypeAnnotationTarget::Offset {
                target: label_for_offset(labels_by_offset, target_offset),
            }
        }
        TargetInfo::TypeArgument {
            offset,
            type_argument_index,
        } => {
            let target_offset = u32::from(*offset);
            require_valid_instruction_offset(
                valid_offsets,
                target_offset,
                "type-annotation type_argument offset",
            )?;
            CodeTypeAnnotationTarget::TypeArgument {
                target: label_for_offset(labels_by_offset, target_offset),
                type_argument_index: *type_argument_index,
            }
        }
        other => CodeTypeAnnotationTarget::Other(other.clone()),
    };
    Ok(CodeTypeAnnotation {
        target_type: annotation.target_type,
        target,
        target_path: annotation.target_path.clone(),
        type_index: annotation.type_index,
        element_value_pairs: annotation.element_value_pairs.clone(),
    })
}

fn stack_map_attr_name(attribute: &AttributeInfo) -> Option<&str> {
    match attribute {
        AttributeInfo::StackMapTable(_) => Some("StackMapTable"),
        AttributeInfo::Unknown(unknown)
            if matches!(unknown.name.as_str(), "StackMapTable" | "StackMap") =>
        {
            Some(unknown.name.as_str())
        }
        _ => None,
    }
}

fn parse_line_number_table(
    info: &[u8],
    valid_offsets: &FxHashSet<u32>,
    labels_by_offset: &mut BTreeMap<u32, Label>,
) -> Result<Vec<LineNumberEntry>> {
    let mut cursor = 0;
    let count = read_u2(info, &mut cursor)? as usize;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let start_pc = u32::from(read_u2(info, &mut cursor)?);
        let line_number = read_u2(info, &mut cursor)?;
        require_valid_instruction_offset(valid_offsets, start_pc, "LineNumberTable start_pc")?;
        entries.push(LineNumberEntry {
            label: label_for_offset(labels_by_offset, start_pc),
            line_number,
        });
    }
    ensure_consumed(info, cursor, "LineNumberTable")?;
    Ok(entries)
}

fn parse_local_variable_table(
    info: &[u8],
    cp: &ConstantPoolBuilder,
    valid_offsets: &FxHashSet<u32>,
    end_offset: u32,
    labels_by_offset: &mut BTreeMap<u32, Label>,
) -> Result<Vec<LocalVariableEntry>> {
    let mut cursor = 0;
    let count = read_u2(info, &mut cursor)? as usize;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let start_pc = u32::from(read_u2(info, &mut cursor)?);
        let length = u32::from(read_u2(info, &mut cursor)?);
        let end_pc = start_pc + length;
        require_valid_instruction_offset(valid_offsets, start_pc, "LocalVariableTable start_pc")?;
        require_valid_end_offset(
            valid_offsets,
            end_offset,
            end_pc,
            "LocalVariableTable end_pc",
        )?;
        let name_index = read_u2(info, &mut cursor)?;
        let descriptor_index = read_u2(info, &mut cursor)?;
        let index = read_u2(info, &mut cursor)?;
        entries.push(LocalVariableEntry {
            start: label_for_offset(labels_by_offset, start_pc),
            end: label_for_offset(labels_by_offset, end_pc),
            name: cp.resolve_utf8(Utf8Index::from(name_index))?,
            descriptor: cp.resolve_utf8(Utf8Index::from(descriptor_index))?,
            index,
        });
    }
    ensure_consumed(info, cursor, "LocalVariableTable")?;
    Ok(entries)
}

fn parse_local_variable_type_table(
    info: &[u8],
    cp: &ConstantPoolBuilder,
    valid_offsets: &FxHashSet<u32>,
    end_offset: u32,
    labels_by_offset: &mut BTreeMap<u32, Label>,
) -> Result<Vec<LocalVariableTypeEntry>> {
    let mut cursor = 0;
    let count = read_u2(info, &mut cursor)? as usize;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let start_pc = u32::from(read_u2(info, &mut cursor)?);
        let length = u32::from(read_u2(info, &mut cursor)?);
        let end_pc = start_pc + length;
        require_valid_instruction_offset(
            valid_offsets,
            start_pc,
            "LocalVariableTypeTable start_pc",
        )?;
        require_valid_end_offset(
            valid_offsets,
            end_offset,
            end_pc,
            "LocalVariableTypeTable end_pc",
        )?;
        let name_index = read_u2(info, &mut cursor)?;
        let signature_index = read_u2(info, &mut cursor)?;
        let index = read_u2(info, &mut cursor)?;
        entries.push(LocalVariableTypeEntry {
            start: label_for_offset(labels_by_offset, start_pc),
            end: label_for_offset(labels_by_offset, end_pc),
            name: cp.resolve_utf8(Utf8Index::from(name_index))?,
            signature: cp.resolve_utf8(Utf8Index::from(signature_index))?,
            index,
        });
    }
    ensure_consumed(info, cursor, "LocalVariableTypeTable")?;
    Ok(entries)
}

fn read_u2(info: &[u8], cursor: &mut usize) -> Result<u16> {
    if info.len().saturating_sub(*cursor) < 2 {
        return Err(EngineError::new(
            0,
            EngineErrorKind::UnexpectedEof {
                needed: 2,
                remaining: info.len().saturating_sub(*cursor),
            },
        ));
    }
    let value = u16::from_be_bytes([info[*cursor], info[*cursor + 1]]);
    *cursor += 2;
    Ok(value)
}

fn ensure_consumed(info: &[u8], cursor: usize, name: &str) -> Result<()> {
    if cursor != info.len() {
        return Err(model_error(format!(
            "{name} parser did not consume full payload"
        )));
    }
    Ok(())
}

fn register_checked_target(
    valid_offsets: &FxHashSet<u32>,
    labels_by_offset: &mut BTreeMap<u32, Label>,
    target: i64,
    kind: &str,
) -> Result<()> {
    let target_u32 = u32::try_from(target)
        .map_err(|_| model_error(format!("{kind} target offset {target} is out of range")))?;
    require_valid_instruction_offset(valid_offsets, target_u32, kind)?;
    label_for_offset(labels_by_offset, target_u32);
    Ok(())
}

fn register_instruction_labels(
    instruction: &Instruction,
    valid_offsets: &FxHashSet<u32>,
    labels_by_offset: &mut BTreeMap<u32, Label>,
) -> Result<()> {
    match instruction {
        Instruction::Branch(branch) => {
            register_checked_target(
                valid_offsets,
                labels_by_offset,
                branch.offset as i64 + branch.branch_offset as i64,
                "branch",
            )?;
        }
        Instruction::BranchWide {
            offset,
            branch_offset,
            ..
        } => {
            register_checked_target(
                valid_offsets,
                labels_by_offset,
                *offset as i64 + *branch_offset as i64,
                "wide branch",
            )?;
        }
        Instruction::LookupSwitch(insn) => {
            register_checked_target(
                valid_offsets,
                labels_by_offset,
                insn.offset as i64 + insn.default_offset as i64,
                "lookupswitch default",
            )?;
            for pair in &insn.pairs {
                register_checked_target(
                    valid_offsets,
                    labels_by_offset,
                    insn.offset as i64 + pair.offset as i64,
                    "lookupswitch case",
                )?;
            }
        }
        Instruction::TableSwitch(insn) => {
            register_checked_target(
                valid_offsets,
                labels_by_offset,
                insn.offset as i64 + insn.default_offset as i64,
                "tableswitch default",
            )?;
            for branch_offset in &insn.offsets {
                register_checked_target(
                    valid_offsets,
                    labels_by_offset,
                    insn.offset as i64 + *branch_offset as i64,
                    "tableswitch case",
                )?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn require_valid_instruction_offset(
    valid_offsets: &FxHashSet<u32>,
    offset: u32,
    kind: &str,
) -> Result<()> {
    if !valid_offsets.contains(&offset) {
        return Err(model_error(format!(
            "{kind} offset {offset} does not align with an instruction boundary"
        )));
    }
    Ok(())
}

fn require_valid_end_offset(
    valid_offsets: &FxHashSet<u32>,
    end_offset: u32,
    offset: u32,
    kind: &str,
) -> Result<()> {
    if offset != end_offset && !valid_offsets.contains(&offset) {
        return Err(model_error(format!(
            "{kind} offset {offset} does not align with an instruction boundary or code end"
        )));
    }
    Ok(())
}

fn label_for_offset(labels_by_offset: &mut BTreeMap<u32, Label>, offset: u32) -> Label {
    labels_by_offset.entry(offset).or_default().clone()
}

fn target_label(labels_by_offset: &BTreeMap<u32, Label>, target: i64) -> Result<Label> {
    let target_u32 =
        u32::try_from(target).map_err(|_| model_error("branch target offset is out of range"))?;
    labels_by_offset
        .get(&target_u32)
        .cloned()
        .ok_or_else(|| model_error("branch target label not found"))
}

fn switch_target_offset(
    offset: u32,
    code_length: u32,
    label_offsets: &FxHashMap<Label, u32>,
    label: &Label,
) -> Result<i32> {
    let target = *label_offsets
        .get(label)
        .ok_or_else(|| model_error("switch target label missing"))?;
    if target >= code_length {
        return Err(model_error("switch target offset exceeds code length"));
    }
    i32::try_from(target as i64 - offset as i64)
        .map_err(|_| model_error("switch target offset exceeds i32"))
}

fn canonical_branch_opcode(opcode: u8) -> u8 {
    match opcode {
        0xC8 => 0xA7,
        0xC9 => 0xA8,
        _ => opcode,
    }
}

fn invert_conditional_branch_opcode(opcode: u8) -> Result<u8> {
    match opcode {
        0x99 => Ok(0x9A),
        0x9A => Ok(0x99),
        0x9B => Ok(0x9C),
        0x9C => Ok(0x9B),
        0x9D => Ok(0x9E),
        0x9E => Ok(0x9D),
        0x9F => Ok(0xA0),
        0xA0 => Ok(0x9F),
        0xA1 => Ok(0xA2),
        0xA2 => Ok(0xA1),
        0xA3 => Ok(0xA4),
        0xA4 => Ok(0xA3),
        0xA5 => Ok(0xA6),
        0xA6 => Ok(0xA5),
        0xC6 => Ok(0xC7),
        0xC7 => Ok(0xC6),
        _ => Err(model_error(format!(
            "opcode 0x{opcode:02x} does not support conditional widening"
        ))),
    }
}

fn ensure_preserve_mode_valid(
    code: &CodeModel,
    method: &MethodModel,
    class: &ClassModel,
    layout: &CodeLayout,
) -> Result<()> {
    let Some(attr_name) = first_frame_sensitive_attr_name(&code.attributes) else {
        return Ok(());
    };
    let Some(original) = &code.original_code_shape else {
        return Err(model_error(format!(
            "{attr_name} is present without lifted layout; use FrameComputationMode::Recompute to recompute frames"
        )));
    };
    if !same_code_items(&code.instructions, &original.instructions)
        || code.exception_handlers != original.exception_handlers
        || layout.item_offsets != original.item_offsets
        || layout.code_length != original.code_length
        || class.name != original.owner_class_name
        || method.descriptor != original.method_descriptor
        || method.access_flags != original.method_access_flags
    {
        return Err(model_error(format!(
            "code-shape, layout offsets, or method signature changed and would stale {attr_name}; use FrameComputationMode::Recompute to recompute frames"
        )));
    }
    Ok(())
}

/// Compare instruction sequences the way lowering sees them.
///
/// A raw instruction's recorded offset is ignored: lowering re-bases every raw
/// instruction to its laid-out position, and instructions rebuilt outside the
/// engine (for example, written back from Python) carry offset 0.
fn same_code_items(left: &[CodeItem], right: &[CodeItem]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|pair| match pair {
            (CodeItem::Raw(left), CodeItem::Raw(right)) => {
                rebase_instruction(left, 0) == rebase_instruction(right, 0)
            }
            (left, right) => left == right,
        })
}

fn first_frame_sensitive_attr_name(attributes: &[AttributeInfo]) -> Option<&str> {
    attributes.iter().find_map(stack_map_attr_name)
}

fn element_value_encoded_len(value: &ElementValueInfo) -> u32 {
    match value {
        ElementValueInfo::Const { .. } | ElementValueInfo::Class { .. } => 3,
        ElementValueInfo::Enum { .. } => 5,
        ElementValueInfo::Annotation(annotation) => {
            1 + 2
                + 2
                + annotation
                    .element_value_pairs
                    .iter()
                    .map(|pair| 2 + element_value_encoded_len(&pair.element_value))
                    .sum::<u32>()
        }
        ElementValueInfo::Array { values } => {
            1 + 2 + values.iter().map(element_value_encoded_len).sum::<u32>()
        }
    }
}

fn type_annotation_encoded_len(annotation: &TypeAnnotationInfo) -> u32 {
    let target_info_len = match &annotation.target_info {
        TargetInfo::TypeParameter { .. } | TargetInfo::FormalParameter { .. } => 1,
        TargetInfo::Supertype { .. }
        | TargetInfo::TypeParameterBound { .. }
        | TargetInfo::Throws { .. }
        | TargetInfo::Catch { .. }
        | TargetInfo::Offset { .. } => 2,
        TargetInfo::Empty => 0,
        TargetInfo::Localvar { table } => 2 + (table.len() as u32) * 6,
        TargetInfo::TypeArgument { .. } => 3,
    };
    let type_path_len = 1 + (annotation.target_path.path.len() as u32) * 2;
    let pairs_len: u32 = annotation
        .element_value_pairs
        .iter()
        .map(|pair| 2 + element_value_encoded_len(&pair.element_value))
        .sum();
    1 + target_info_len + type_path_len + 2 + 2 + pairs_len
}

fn attribute_payload_len(attribute: &AttributeInfo) -> u32 {
    match attribute {
        AttributeInfo::StackMapTable(attr) => attr.attribute_length,
        AttributeInfo::LineNumberTable(attr) => 2 + (attr.line_number_table.len() as u32) * 4,
        AttributeInfo::LocalVariableTable(attr) => {
            2 + (attr.local_variable_table.len() as u32) * 10
        }
        AttributeInfo::LocalVariableTypeTable(attr) => {
            2 + (attr.local_variable_type_table.len() as u32) * 10
        }
        AttributeInfo::RuntimeVisibleTypeAnnotations(attr) => {
            2 + attr
                .annotations
                .iter()
                .map(type_annotation_encoded_len)
                .sum::<u32>()
        }
        AttributeInfo::RuntimeInvisibleTypeAnnotations(attr) => {
            2 + attr
                .annotations
                .iter()
                .map(type_annotation_encoded_len)
                .sum::<u32>()
        }
        AttributeInfo::Unknown(attr) => attr.info.len() as u32,
        other => other.attribute_length(),
    }
}

fn instruction_length(instruction: &Instruction, offset: u32) -> u32 {
    match instruction {
        Instruction::Simple { .. } => 1,
        Instruction::LocalIndex { .. } => 2,
        Instruction::ConstantPoolIndex1 { .. } => 2,
        Instruction::ConstantPoolIndexWide(_) => 3,
        Instruction::Byte { .. } => 2,
        Instruction::Short { .. } => 3,
        Instruction::Branch(_) => 3,
        Instruction::BranchWide { .. } => 5,
        Instruction::IInc { .. } => 3,
        Instruction::InvokeDynamic(_) => 5,
        Instruction::InvokeInterface(_) => 5,
        Instruction::NewArray(_) => 2,
        Instruction::MultiANewArray { .. } => 4,
        Instruction::LookupSwitch(insn) => {
            1 + switch_padding(offset) + 8 + (insn.pairs.len() as u32 * 8)
        }
        Instruction::TableSwitch(insn) => {
            1 + switch_padding(offset) + 12 + (insn.offsets.len() as u32 * 4)
        }
        Instruction::Wide(insn) => {
            if insn.opcode == 0x84 {
                6
            } else {
                4
            }
        }
    }
}

fn switch_padding(offset: u32) -> u32 {
    3 - (offset & 3)
}

fn fits_i16(value: i64) -> bool {
    i16::try_from(value).is_ok()
}

fn model_error(reason: impl Into<String>) -> EngineError {
    EngineError::new(
        0,
        EngineErrorKind::InvalidModelState {
            reason: reason.into(),
        },
    )
}

fn rebase_instruction(instruction: &Instruction, offset: u32) -> Instruction {
    match instruction {
        Instruction::Simple { opcode, .. } => Instruction::Simple {
            opcode: *opcode,
            offset,
        },
        Instruction::LocalIndex { opcode, index, .. } => Instruction::LocalIndex {
            opcode: *opcode,
            offset,
            index: *index,
        },
        Instruction::ConstantPoolIndex1 { opcode, index, .. } => Instruction::ConstantPoolIndex1 {
            opcode: *opcode,
            offset,
            index: *index,
        },
        Instruction::ConstantPoolIndexWide(insn) => {
            Instruction::ConstantPoolIndexWide(crate::raw::ConstantPoolIndexWide {
                opcode: insn.opcode,
                offset,
                index: insn.index,
            })
        }
        Instruction::Byte { opcode, value, .. } => Instruction::Byte {
            opcode: *opcode,
            offset,
            value: *value,
        },
        Instruction::Short { opcode, value, .. } => Instruction::Short {
            opcode: *opcode,
            offset,
            value: *value,
        },
        Instruction::Branch(branch) => Instruction::Branch(Branch {
            opcode: branch.opcode,
            offset,
            branch_offset: branch.branch_offset,
        }),
        Instruction::BranchWide {
            opcode,
            branch_offset,
            ..
        } => Instruction::BranchWide {
            opcode: *opcode,
            offset,
            branch_offset: *branch_offset,
        },
        Instruction::IInc { index, value, .. } => Instruction::IInc {
            offset,
            index: *index,
            value: *value,
        },
        Instruction::InvokeDynamic(insn) => Instruction::InvokeDynamic(RawInvokeDynamicInsn {
            offset,
            index: insn.index,
            reserved: insn.reserved,
        }),
        Instruction::InvokeInterface(insn) => {
            Instruction::InvokeInterface(RawInvokeInterfaceInsn {
                offset,
                index: insn.index,
                count: insn.count,
                reserved: insn.reserved,
            })
        }
        Instruction::NewArray(insn) => Instruction::NewArray(crate::raw::NewArrayInsn {
            offset,
            atype: insn.atype,
        }),
        Instruction::MultiANewArray {
            index, dimensions, ..
        } => Instruction::MultiANewArray {
            offset,
            index: *index,
            dimensions: *dimensions,
        },
        Instruction::LookupSwitch(insn) => Instruction::LookupSwitch(RawLookupSwitchInsn {
            offset,
            default_offset: insn.default_offset,
            pairs: insn.pairs.clone(),
        }),
        Instruction::TableSwitch(insn) => Instruction::TableSwitch(RawTableSwitchInsn {
            offset,
            default_offset: insn.default_offset,
            low: insn.low,
            high: insn.high,
            offsets: insn.offsets.clone(),
        }),
        Instruction::Wide(insn) => Instruction::Wide(WideInstruction {
            offset,
            opcode: insn.opcode,
            index: insn.index,
            value: insn.value,
        }),
    }
}
