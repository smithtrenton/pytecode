mod constant_pool_builder;
mod debug_info;
mod labels;
mod operands;

use crate::analysis::{ClassResolver, FrameComputationResult, VType, recompute_frames};
pub use constant_pool_builder::ConstantPoolBuilder;
pub use debug_info::{DebugInfoPolicy, DebugInfoState};
pub use labels::{
    BranchInsn, CodeItem, ExceptionHandler, Label, LineNumberEntry, LocalVariableEntry,
    LocalVariableTypeEntry, LookupSwitchInsn, TableSwitchInsn,
};
pub use operands::{
    DynamicValue, FieldInsn, IIncInsn, InterfaceMethodInsn, InvokeDynamicInsn, LdcInsn, LdcValue,
    MethodHandleValue, MethodInsn, MultiANewArrayInsn, TypeInsn, VarInsn,
};

use crate::constants::{ClassAccessFlags, FieldAccessFlags, MAGIC, MethodAccessFlags};
use crate::descriptors::{is_valid_field_descriptor, is_valid_method_descriptor};
use crate::indexes::{ClassIndex, CpIndex, Utf8Index};
use crate::raw::{
    AttributeInfo, Branch, ClassFile, CodeAttribute, ConstantPoolEntry, FieldInfo, Instruction,
    InvokeDynamicInsn as RawInvokeDynamicInsn, InvokeInterfaceInsn as RawInvokeInterfaceInsn,
    LookupSwitchInsn as RawLookupSwitchInsn, MatchOffsetPair, MethodInfo, RawClassStub,
    TableSwitchInsn as RawTableSwitchInsn, UnknownAttribute, WideInstruction,
};
use crate::{EngineError, EngineErrorKind, Result, parse_class, write_class};
use rustc_hash::FxHashMap;
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
pub struct MethodModel {
    pub access_flags: MethodAccessFlags,
    pub name: String,
    pub descriptor: String,
    pub code: Option<CodeModel>,
    pub pre_built_code_bytes: Option<Vec<u8>>,
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
    StackMapTable,
}

impl std::fmt::Display for NestedCodeAttributeLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Other => write!(f, "other"),
            Self::LineNumbers => write!(f, "line_numbers"),
            Self::LocalVariables => write!(f, "local_variables"),
            Self::LocalVariableTypes => write!(f, "local_variable_types"),
            Self::StackMapTable => write!(f, "stack_map_table"),
        }
    }
}

impl CodeModel {
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
            attributes: Vec::new(),
            debug_info_state,
            nested_attribute_layout: Vec::new(),
            original_code_shape: None,
        }
    }

    pub fn nested_attribute_layout(&self) -> &[NestedCodeAttributeLayout] {
        &self.nested_attribute_layout
    }
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
            code,
            pre_built_code_bytes: None,
            attributes,
            attribute_layout,
        }
    }

    pub fn set_prebuilt_code_bytes(&mut self, bytes: Vec<u8>) {
        self.code = None;
        self.pre_built_code_bytes = Some(bytes);
        if !self.attribute_layout.contains(&MethodAttributeLayout::Code) {
            self.attribute_layout.push(MethodAttributeLayout::Code);
        }
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
            .map(|method| lift_method_model(method, &cp))
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
                &self.name,
                self.version.0,
                &mut cp,
                debug_info,
                frame_mode,
                resolver,
            )?);
        }

        Ok(ClassFile {
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
        })
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
    if let Some(code) = &mut method.code {
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

fn lift_method_model(method: &MethodInfo, cp: &ConstantPoolBuilder) -> Result<MethodModel> {
    let mut code = None;
    let mut attributes = Vec::new();
    let mut attribute_layout = Vec::new();
    for attribute in &method.attributes {
        match attribute {
            AttributeInfo::Code(code_attr) => {
                code = Some(lift_code_model(code_attr, cp)?);
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
        descriptor: cp.resolve_utf8(method.descriptor_index)?,
        code,
        pre_built_code_bytes: None,
        attributes,
        attribute_layout,
    })
}

fn lift_code_model(code: &CodeAttribute, cp: &ConstantPoolBuilder) -> Result<CodeModel> {
    let mut labels_by_offset = BTreeMap::new();
    let end_offset = code.code_length;

    for instruction in &code.code {
        register_instruction_labels(instruction, &mut labels_by_offset)?;
    }
    for handler in &code.exception_table {
        label_for_offset(&mut labels_by_offset, handler.start_pc as u32);
        label_for_offset(&mut labels_by_offset, handler.end_pc as u32);
        label_for_offset(&mut labels_by_offset, handler.handler_pc as u32);
    }

    let mut line_numbers = Vec::new();
    let mut local_variables = Vec::new();
    let mut local_variable_types = Vec::new();
    let mut attributes = Vec::new();
    let mut layout = Vec::new();
    for attribute in &code.attributes {
        match parse_debug_attribute(attribute, cp, &mut labels_by_offset)? {
            ParsedDebugAttribute::LineNumbers(entries) => {
                line_numbers = entries;
                layout.push(NestedCodeAttributeLayout::LineNumbers);
            }
            ParsedDebugAttribute::LocalVariables(entries) => {
                local_variables = entries;
                layout.push(NestedCodeAttributeLayout::LocalVariables);
            }
            ParsedDebugAttribute::LocalVariableTypes(entries) => {
                local_variable_types = entries;
                layout.push(NestedCodeAttributeLayout::LocalVariableTypes);
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

    for entry in &line_numbers {
        label_for_existing_offset(&mut labels_by_offset, &entry.label);
    }
    for entry in &local_variables {
        label_for_existing_offset(&mut labels_by_offset, &entry.start);
        label_for_existing_offset(&mut labels_by_offset, &entry.end);
    }
    for entry in &local_variable_types {
        label_for_existing_offset(&mut labels_by_offset, &entry.start);
        label_for_existing_offset(&mut labels_by_offset, &entry.end);
    }

    let mut instructions = Vec::new();
    for instruction in &code.code {
        if let Some(label) = labels_by_offset.get(&instruction.offset()) {
            instructions.push(CodeItem::Label(label.clone()));
        }
        instructions.push(lift_instruction(instruction, cp, &labels_by_offset)?);
    }
    if let Some(label) = labels_by_offset.get(&end_offset) {
        instructions.push(CodeItem::Label(label.clone()));
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
    };

    Ok(CodeModel {
        max_stack: code.max_stack,
        max_locals: code.max_locals,
        instructions,
        exception_handlers,
        line_numbers,
        local_variables,
        local_variable_types,
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
            lower_cp_index_item(*opcode, CpIndex::from(*index as u16), cp)
        }
        Instruction::ConstantPoolIndexWide(insn) => {
            lower_cp_index_item(insn.opcode, insn.index, cp)
        }
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
                Ok(Item::IInc(IIncInsn {
                    slot: insn.index,
                    value: insn.value.unwrap_or(0),
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

fn lower_cp_index_item(opcode: u8, index: CpIndex, cp: &ConstantPoolBuilder) -> Result<CodeItem> {
    use CodeItem as Item;
    match opcode {
        0x12..=0x14 => Ok(Item::Ldc(LdcInsn {
            value: lift_ldc_value(index, cp)?,
        })),
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
        ConstantPoolEntry::Integer(info) => Ok(LdcValue::Int(info.value_bytes)),
        ConstantPoolEntry::Float(info) => Ok(LdcValue::FloatBits(info.value_bytes)),
        ConstantPoolEntry::Long(info) => {
            let raw = ((info.high_bytes as u64) << 32) | (info.low_bytes as u64);
            Ok(LdcValue::Long(raw))
        }
        ConstantPoolEntry::Double(info) => {
            let raw = ((info.high_bytes as u64) << 32) | (info.low_bytes as u64);
            Ok(LdcValue::DoubleBits(raw))
        }
        ConstantPoolEntry::String(info) => {
            Ok(LdcValue::String(cp.resolve_utf8(info.string_index)?))
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
    class_name: &str,
    class_major_version: u16,
    cp: &mut ConstantPoolBuilder,
    debug_info: DebugInfoPolicy,
    frame_mode: FrameComputationMode,
    resolver: Option<&dyn ClassResolver>,
) -> Result<MethodInfo> {
    if !is_valid_method_descriptor(&method.descriptor) {
        return Err(model_error(format!(
            "invalid method descriptor {}",
            method.descriptor
        )));
    }
    let lowered_code: Option<AttributeInfo> = if let Some(code_bytes) = &method.pre_built_code_bytes
    {
        let name_index = cp.add_utf8("Code")?;
        Some(AttributeInfo::Unknown(UnknownAttribute {
            attribute_name_index: name_index,
            attribute_length: code_bytes.len() as u32,
            name: "Code".to_owned(),
            info: code_bytes.clone(),
        }))
    } else {
        method
            .code
            .as_ref()
            .map(|code| {
                lower_code_model(
                    code,
                    method,
                    class_name,
                    class_major_version,
                    cp,
                    debug_info,
                    frame_mode,
                    resolver,
                )
                .map(AttributeInfo::Code)
            })
            .transpose()?
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

#[allow(clippy::too_many_arguments)]
fn lower_code_model(
    code: &CodeModel,
    method: &MethodModel,
    class_name: &str,
    class_major_version: u16,
    cp: &mut ConstantPoolBuilder,
    debug_info: DebugInfoPolicy,
    frame_mode: FrameComputationMode,
    resolver: Option<&dyn ClassResolver>,
) -> Result<CodeAttribute> {
    if frame_mode == FrameComputationMode::Preserve {
        ensure_frame_sensitive_attrs_supported(code)?;
    }
    let layout = compute_code_layout(code, cp)?;
    let frame_result = if frame_mode == FrameComputationMode::Recompute {
        Some(
            recompute_frames(
                code,
                class_name,
                &method.name,
                &method.descriptor,
                method.access_flags,
                resolver,
            )
            .map_err(|error| model_error(error.to_string()))?,
        )
    } else {
        None
    };

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
            let start_pc = *layout
                .label_offsets
                .get(&handler.start)
                .ok_or_else(|| model_error("exception start label missing"))?
                as u16;
            let end_pc = *layout
                .label_offsets
                .get(&handler.end)
                .ok_or_else(|| model_error("exception end label missing"))?
                as u16;
            let handler_pc = *layout
                .label_offsets
                .get(&handler.handler)
                .ok_or_else(|| model_error("exception handler label missing"))?
                as u16;
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
        Some(frames) if frame_result_contains_return_address(frames) => {
            if class_major_version >= 50 {
                return Err(model_error(
                    "recomputed StackMapTable cannot encode returnAddress states for jsr/ret methods in class file version 50.0 or newer",
                ));
            }
            None
        }
        Some(frames) => Some(lower_stack_map_table(frames, cp, &layout.item_offsets)?),
        None => None,
    };
    let attributes = lower_nested_attributes(
        code,
        cp,
        &layout.label_offsets,
        code.debug_info_state,
        debug_info,
        frame_mode,
        stack_map_table,
    )?;
    Ok(CodeAttribute {
        attribute_name_index: cp.add_utf8("Code")?,
        attribute_length: 0,
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

fn compute_code_layout(code: &CodeModel, cp: &mut ConstantPoolBuilder) -> Result<CodeLayout> {
    let instruction_count = code.instructions.len();
    let mut branch_encodings = vec![BranchEncoding::Short; instruction_count];
    let mut label_offsets = FxHashMap::default();
    let mut ldc_indexes = vec![None; instruction_count];
    for _ in 0..8 {
        label_offsets =
            compute_label_offsets(&code.instructions, cp, &branch_encodings, &mut ldc_indexes)?;
        let mut changed = false;
        let mut offset = 0_u32;
        for (item_index, item) in code.instructions.iter().enumerate() {
            if let CodeItem::Label(label) = item {
                let current = label_offsets.get(label).copied().unwrap_or(offset);
                offset = current;
                continue;
            }
            if let CodeItem::Branch(branch) = item {
                let target = *label_offsets
                    .get(&branch.target)
                    .ok_or_else(|| model_error("branch target label not present"))?;
                let delta = target as i64 - offset as i64;
                let required = if fits_i16(delta) {
                    BranchEncoding::Short
                } else if matches!(branch.opcode, 0xA7 | 0xA8) {
                    BranchEncoding::Wide
                } else {
                    BranchEncoding::InvertedWide
                };
                if branch_encodings[item_index] != required {
                    branch_encodings[item_index] = required;
                    changed = true;
                }
            }
            offset += instruction_size(
                item_index,
                item,
                offset,
                cp,
                &branch_encodings,
                &mut ldc_indexes,
            )?;
        }
        if !changed {
            break;
        }
    }
    let mut item_offsets = vec![None; instruction_count];
    let mut current = 0_u32;
    for (item_index, item) in code.instructions.iter().enumerate() {
        match item {
            CodeItem::Label(label) => {
                current = *label_offsets.get(label).unwrap_or(&current);
            }
            _ => {
                item_offsets[item_index] = Some(current);
                current += instruction_size(
                    item_index,
                    item,
                    current,
                    cp,
                    &branch_encodings,
                    &mut ldc_indexes,
                )?;
            }
        }
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
                offsets.insert(label.clone(), current);
            }
            _ => {
                current +=
                    instruction_size(item_index, item, current, cp, branch_encodings, ldc_indexes)?
            }
        }
    }
    Ok(offsets)
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
        CodeItem::Raw(instruction) => instruction_length(instruction),
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
            1 + padding + 8 + (insn.pairs.len() as u32 * 8)
        }
        CodeItem::TableSwitch(insn) => {
            let padding = switch_padding(offset);
            1 + padding + 12 + (insn.targets.len() as u32 * 4)
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
        CodeItem::Ldc(insn) => match &insn.value {
            LdcValue::Long(_) | LdcValue::DoubleBits(_) => {
                let index = if let Some(index) = ldc_indexes[item_index] {
                    index
                } else {
                    let index = add_ldc_value(cp, &insn.value)?;
                    ldc_indexes[item_index] = Some(index);
                    index
                };
                debug_assert!(index.value() > 0);
                3
            }
            value => {
                let index = if let Some(index) = ldc_indexes[item_index] {
                    index
                } else {
                    let index = add_ldc_value(cp, value)?;
                    ldc_indexes[item_index] = Some(index);
                    index
                };
                if index.value() <= u8::MAX as u16 {
                    2
                } else {
                    3
                }
            }
        },
    })
}

#[allow(clippy::too_many_arguments)]
fn lower_instruction_sequence(
    item_index: usize,
    item: &CodeItem,
    offset: u32,
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
            let delta_from_here = target as i64 - offset as i64;
            match branch_encodings[item_index] {
                BranchEncoding::Short => out.push(Instruction::Branch(Branch {
                    opcode: insn.opcode,
                    offset,
                    branch_offset: i16::try_from(delta_from_here)
                        .map_err(|_| model_error("branch offset exceeds i16"))?,
                })),
                BranchEncoding::Wide => out.push(Instruction::BranchWide {
                    opcode: if insn.opcode == 0xA7 { 0xC8 } else { 0xC9 },
                    offset,
                    branch_offset: i32::try_from(delta_from_here)
                        .map_err(|_| model_error("wide branch offset exceeds i32"))?,
                }),
                BranchEncoding::InvertedWide => {
                    let inverted_opcode = invert_conditional_branch_opcode(insn.opcode)?;
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
            default_offset: switch_target_offset(offset, label_offsets, &insn.default_target)?,
            pairs: insn
                .pairs
                .iter()
                .map(|(match_value, label)| {
                    Ok(MatchOffsetPair {
                        match_value: *match_value,
                        offset: switch_target_offset(offset, label_offsets, label)?,
                    })
                })
                .collect::<Result<Vec<_>>>()?,
        })),
        CodeItem::TableSwitch(insn) => out.push(Instruction::TableSwitch(RawTableSwitchInsn {
            offset,
            default_offset: switch_target_offset(offset, label_offsets, &insn.default_target)?,
            low: insn.low,
            high: insn.high,
            offsets: insn
                .targets
                .iter()
                .map(|label| switch_target_offset(offset, label_offsets, label))
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
    Ok(match value {
        LdcValue::Long(_) | LdcValue::DoubleBits(_) => {
            Instruction::ConstantPoolIndexWide(crate::raw::ConstantPoolIndexWide {
                opcode: 0x14,
                offset,
                index,
            })
        }
        _ if index.value() <= u8::MAX as u16 => Instruction::ConstantPoolIndex1 {
            opcode: 0x12,
            offset,
            index: index.value() as u8,
        },
        _ => Instruction::ConstantPoolIndexWide(crate::raw::ConstantPoolIndexWide {
            opcode: 0x13,
            offset,
            index,
        }),
    })
}

fn add_ldc_value(cp: &mut ConstantPoolBuilder, value: &LdcValue) -> Result<CpIndex> {
    match value {
        LdcValue::Int(value) => cp.add_integer(*value),
        LdcValue::FloatBits(raw_bits) => cp.add_float_bits(*raw_bits),
        LdcValue::Long(value) => cp.add_long(*value),
        LdcValue::DoubleBits(raw_bits) => cp.add_double_bits(*raw_bits),
        LdcValue::String(value) => cp.add_string(value),
        LdcValue::Class(value) => Ok(cp.add_class(value)?.into()),
        LdcValue::MethodType(value) => cp.add_method_type(value),
        LdcValue::MethodHandle(value) => {
            let reference_index = if value.is_interface {
                cp.add_interface_method_ref(&value.owner, &value.name, &value.descriptor)?
            } else if matches!(value.reference_kind, 1..=4) {
                cp.add_field_ref(&value.owner, &value.name, &value.descriptor)?
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

fn lower_nested_attributes(
    code: &CodeModel,
    cp: &mut ConstantPoolBuilder,
    label_offsets: &FxHashMap<Label, u32>,
    debug_info_state: DebugInfoState,
    debug_info: DebugInfoPolicy,
    frame_mode: FrameComputationMode,
    stack_map_table: Option<AttributeInfo>,
) -> Result<Vec<AttributeInfo>> {
    let strip_debug_info = debug_info.should_strip() || debug_info_state == DebugInfoState::Stale;
    let line_number_attr = if strip_debug_info || code.line_numbers.is_empty() {
        None
    } else {
        Some(lower_line_number_table(
            &code.line_numbers,
            cp,
            label_offsets,
        )?)
    };
    let local_variable_attr = if strip_debug_info || code.local_variables.is_empty() {
        None
    } else {
        Some(lower_local_variable_table(
            &code.local_variables,
            cp,
            label_offsets,
            false,
        )?)
    };
    let local_variable_type_attr = if strip_debug_info || code.local_variable_types.is_empty() {
        None
    } else {
        Some(lower_local_variable_type_table(
            &code.local_variable_types,
            cp,
            label_offsets,
        )?)
    };

    let mut other_iter = code
        .attributes
        .iter()
        .filter(|attribute| {
            !(frame_mode == FrameComputationMode::Recompute
                && stack_map_attr_name(attribute).is_some())
        })
        .cloned();
    let mut line_number_attr = line_number_attr;
    let mut local_variable_attr = local_variable_attr;
    let mut local_variable_type_attr = local_variable_type_attr;
    let mut stack_map_table = stack_map_table;
    let mut attributes = Vec::with_capacity(code.attributes.len() + 4);
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
            NestedCodeAttributeLayout::StackMapTable => {
                if frame_mode == FrameComputationMode::Recompute {
                    if let Some(attribute) = stack_map_table.take() {
                        attributes.push(attribute);
                    }
                } else if let Some(attribute) = other_iter.next() {
                    attributes.push(attribute);
                }
            }
        }
    }

    attributes.extend(other_iter);
    if let Some(attribute) = line_number_attr {
        attributes.push(attribute);
    }
    if let Some(attribute) = local_variable_attr {
        attributes.push(attribute);
    }
    if let Some(attribute) = local_variable_type_attr {
        attributes.push(attribute);
    }
    if let Some(attribute) = stack_map_table {
        attributes.push(attribute);
    }
    Ok(attributes)
}

fn lower_stack_map_table(
    frames: &FrameComputationResult,
    cp: &mut ConstantPoolBuilder,
    item_offsets: &[Option<u32>],
) -> Result<AttributeInfo> {
    let mut entries = Vec::with_capacity(frames.frames.len());
    let mut previous_offset = 0_u32;
    for (frame_index, frame) in frames.frames.iter().enumerate() {
        let offset = item_offset(
            item_offsets,
            frame.code_index,
            "stack-map frame instruction offset missing",
        )?;
        let offset_delta = if frame_index == 0 {
            offset
        } else {
            offset
                .checked_sub(previous_offset + 1)
                .ok_or_else(|| model_error("stack-map frame offsets are not monotonic"))?
        };
        previous_offset = offset;
        let locals = frame
            .locals
            .iter()
            .map(|value| raw_verification_type(value, cp, item_offsets))
            .collect::<Result<Vec<_>>>()?;
        let stack = frame
            .stack
            .iter()
            .map(|value| raw_verification_type(value, cp, item_offsets))
            .collect::<Result<Vec<_>>>()?;
        entries.push(crate::raw::StackMapFrameInfo::Full {
            frame_type: 255,
            offset_delta: offset_delta as u16,
            locals,
            stack,
        });
    }
    Ok(AttributeInfo::StackMapTable(
        crate::raw::StackMapTableAttribute {
            attribute_name_index: cp.add_utf8("StackMapTable")?,
            attribute_length: 2,
            entries,
        },
    ))
}

fn raw_verification_type(
    value: &VType,
    cp: &mut ConstantPoolBuilder,
    item_offsets: &[Option<u32>],
) -> Result<crate::raw::VerificationTypeInfo> {
    match value {
        VType::Top => Ok(crate::raw::VerificationTypeInfo::Top),
        VType::Integer => Ok(crate::raw::VerificationTypeInfo::Integer),
        VType::Float => Ok(crate::raw::VerificationTypeInfo::Float),
        VType::Double => Ok(crate::raw::VerificationTypeInfo::Double),
        VType::Long => Ok(crate::raw::VerificationTypeInfo::Long),
        VType::Null => Ok(crate::raw::VerificationTypeInfo::Null),
        VType::ReturnAddress(_) => Err(model_error(
            "StackMapTable cannot encode returnAddress verification types",
        )),
        VType::UninitializedThis => Ok(crate::raw::VerificationTypeInfo::UninitializedThis),
        VType::Object(class_name) => Ok(crate::raw::VerificationTypeInfo::Object {
            cpool_index: cp.add_class(class_name)?,
        }),
        VType::Uninitialized { code_index, .. } => {
            let offset = item_offset(
                item_offsets,
                *code_index,
                "missing offset for uninitialized new instruction",
            )?;
            Ok(crate::raw::VerificationTypeInfo::Uninitialized {
                offset: offset as u16,
            })
        }
    }
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
) -> Result<AttributeInfo> {
    let line_number_table = entries
        .iter()
        .map(|entry| {
            Ok(crate::raw::LineNumberInfo {
                start_pc: *label_offsets
                    .get(&entry.label)
                    .ok_or_else(|| model_error("line number label missing"))?
                    as u16,
                line_number: entry.line_number,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(AttributeInfo::LineNumberTable(
        crate::raw::LineNumberTableAttribute {
            attribute_name_index: cp.add_utf8("LineNumberTable")?,
            attribute_length: 2,
            line_number_table,
        },
    ))
}

fn lower_local_variable_table(
    entries: &[LocalVariableEntry],
    cp: &mut ConstantPoolBuilder,
    label_offsets: &FxHashMap<Label, u32>,
    type_table: bool,
) -> Result<AttributeInfo> {
    debug_assert!(!type_table);
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
            Ok(crate::raw::LocalVariableInfo {
                start_pc: start_pc as u16,
                length: (end_pc - start_pc) as u16,
                name_index: cp.add_utf8(&entry.name)?,
                descriptor_index: cp.add_utf8(&entry.descriptor)?,
                index: entry.index,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(AttributeInfo::LocalVariableTable(
        crate::raw::LocalVariableTableAttribute {
            attribute_name_index: cp.add_utf8("LocalVariableTable")?,
            attribute_length: 2,
            local_variable_table,
        },
    ))
}

fn lower_local_variable_type_table(
    entries: &[LocalVariableTypeEntry],
    cp: &mut ConstantPoolBuilder,
    label_offsets: &FxHashMap<Label, u32>,
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
            Ok(crate::raw::LocalVariableTypeInfo {
                start_pc: start_pc as u16,
                length: (end_pc - start_pc) as u16,
                name_index: cp.add_utf8(&entry.name)?,
                signature_index: cp.add_utf8(&entry.signature)?,
                index: entry.index,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(AttributeInfo::LocalVariableTypeTable(
        crate::raw::LocalVariableTypeTableAttribute {
            attribute_name_index: cp.add_utf8("LocalVariableTypeTable")?,
            attribute_length: 2,
            local_variable_type_table,
        },
    ))
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
        .filter(|attribute| {
            !matches!(
                attribute,
                AttributeInfo::SourceFile(_) | AttributeInfo::SourceDebugExtension(_)
            )
        })
        .cloned()
        .collect()
}

enum ParsedDebugAttribute {
    LineNumbers(Vec<LineNumberEntry>),
    LocalVariables(Vec<LocalVariableEntry>),
    LocalVariableTypes(Vec<LocalVariableTypeEntry>),
    StackMapTable(AttributeInfo),
    Other(AttributeInfo),
}

fn parse_debug_attribute(
    attribute: &AttributeInfo,
    cp: &ConstantPoolBuilder,
    labels_by_offset: &mut BTreeMap<u32, Label>,
) -> Result<ParsedDebugAttribute> {
    match attribute {
        AttributeInfo::LineNumberTable(attribute) => Ok(ParsedDebugAttribute::LineNumbers(
            attribute
                .line_number_table
                .iter()
                .map(|entry| LineNumberEntry {
                    label: label_for_offset(labels_by_offset, entry.start_pc as u32),
                    line_number: entry.line_number,
                })
                .collect(),
        )),
        AttributeInfo::LocalVariableTable(attribute) => Ok(ParsedDebugAttribute::LocalVariables(
            attribute
                .local_variable_table
                .iter()
                .map(|entry| {
                    Ok(LocalVariableEntry {
                        start: label_for_offset(labels_by_offset, entry.start_pc as u32),
                        end: label_for_offset(
                            labels_by_offset,
                            entry.start_pc as u32 + entry.length as u32,
                        ),
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
                        Ok(LocalVariableTypeEntry {
                            start: label_for_offset(labels_by_offset, entry.start_pc as u32),
                            end: label_for_offset(
                                labels_by_offset,
                                entry.start_pc as u32 + entry.length as u32,
                            ),
                            name: cp.resolve_utf8(entry.name_index)?,
                            signature: cp.resolve_utf8(entry.signature_index)?,
                            index: entry.index,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?,
            ))
        }
        AttributeInfo::Unknown(unknown) => match unknown.name.as_str() {
            "LineNumberTable" => Ok(ParsedDebugAttribute::LineNumbers(parse_line_number_table(
                &unknown.info,
                labels_by_offset,
            )?)),
            "LocalVariableTable" => Ok(ParsedDebugAttribute::LocalVariables(
                parse_local_variable_table(&unknown.info, cp, labels_by_offset, false)?,
            )),
            "LocalVariableTypeTable" => Ok(ParsedDebugAttribute::LocalVariableTypes(
                parse_local_variable_type_table(&unknown.info, cp, labels_by_offset)?,
            )),
            "StackMapTable" | "StackMap" => {
                Ok(ParsedDebugAttribute::StackMapTable(attribute.clone()))
            }
            _ => Ok(ParsedDebugAttribute::Other(attribute.clone())),
        },
        _ if stack_map_attr_name(attribute).is_some() => {
            Ok(ParsedDebugAttribute::StackMapTable(attribute.clone()))
        }
        _ => Ok(ParsedDebugAttribute::Other(attribute.clone())),
    }
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
    labels_by_offset: &mut BTreeMap<u32, Label>,
) -> Result<Vec<LineNumberEntry>> {
    let mut cursor = 0;
    let count = read_u2(info, &mut cursor)? as usize;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let start_pc = read_u2(info, &mut cursor)? as u32;
        let line_number = read_u2(info, &mut cursor)?;
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
    labels_by_offset: &mut BTreeMap<u32, Label>,
    _type_table: bool,
) -> Result<Vec<LocalVariableEntry>> {
    let mut cursor = 0;
    let count = read_u2(info, &mut cursor)? as usize;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let start_pc = read_u2(info, &mut cursor)? as u32;
        let length = read_u2(info, &mut cursor)? as u32;
        let name_index = read_u2(info, &mut cursor)?;
        let descriptor_index = read_u2(info, &mut cursor)?;
        let index = read_u2(info, &mut cursor)?;
        entries.push(LocalVariableEntry {
            start: label_for_offset(labels_by_offset, start_pc),
            end: label_for_offset(labels_by_offset, start_pc + length),
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
    labels_by_offset: &mut BTreeMap<u32, Label>,
) -> Result<Vec<LocalVariableTypeEntry>> {
    let mut cursor = 0;
    let count = read_u2(info, &mut cursor)? as usize;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let start_pc = read_u2(info, &mut cursor)? as u32;
        let length = read_u2(info, &mut cursor)? as u32;
        let name_index = read_u2(info, &mut cursor)?;
        let signature_index = read_u2(info, &mut cursor)?;
        let index = read_u2(info, &mut cursor)?;
        entries.push(LocalVariableTypeEntry {
            start: label_for_offset(labels_by_offset, start_pc),
            end: label_for_offset(labels_by_offset, start_pc + length),
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

fn register_instruction_labels(
    instruction: &Instruction,
    labels_by_offset: &mut BTreeMap<u32, Label>,
) -> Result<()> {
    match instruction {
        Instruction::Branch(branch) => {
            label_for_offset(
                labels_by_offset,
                (branch.offset as i64 + branch.branch_offset as i64) as u32,
            );
        }
        Instruction::BranchWide {
            offset,
            branch_offset,
            ..
        } => {
            label_for_offset(
                labels_by_offset,
                (*offset as i64 + *branch_offset as i64) as u32,
            );
        }
        Instruction::LookupSwitch(insn) => {
            label_for_offset(
                labels_by_offset,
                (insn.offset as i64 + insn.default_offset as i64) as u32,
            );
            for pair in &insn.pairs {
                label_for_offset(
                    labels_by_offset,
                    (insn.offset as i64 + pair.offset as i64) as u32,
                );
            }
        }
        Instruction::TableSwitch(insn) => {
            label_for_offset(
                labels_by_offset,
                (insn.offset as i64 + insn.default_offset as i64) as u32,
            );
            for branch_offset in &insn.offsets {
                label_for_offset(
                    labels_by_offset,
                    (insn.offset as i64 + *branch_offset as i64) as u32,
                );
            }
        }
        _ => {}
    }
    Ok(())
}

fn label_for_offset(labels_by_offset: &mut BTreeMap<u32, Label>, offset: u32) -> Label {
    labels_by_offset.entry(offset).or_default().clone()
}

fn label_for_existing_offset(_labels_by_offset: &mut BTreeMap<u32, Label>, _label: &Label) {}

fn target_label(labels_by_offset: &BTreeMap<u32, Label>, target: i64) -> Result<Label> {
    if target < 0 {
        return Err(model_error("branch target offset is negative"));
    }
    labels_by_offset
        .get(&(target as u32))
        .cloned()
        .ok_or_else(|| model_error("branch target label not found"))
}

fn switch_target_offset(
    offset: u32,
    label_offsets: &FxHashMap<Label, u32>,
    label: &Label,
) -> Result<i32> {
    let target = *label_offsets
        .get(label)
        .ok_or_else(|| model_error("switch target label missing"))?;
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

fn ensure_frame_sensitive_attrs_supported(code: &CodeModel) -> Result<()> {
    if !code_shape_edited(code) {
        return Ok(());
    }
    if let Some(attr_name) = first_frame_sensitive_attr_name(&code.attributes) {
        return Err(model_error(format!(
            "code-shape edit would stale {attr_name}; Phase 4 recomputation is required"
        )));
    }
    Ok(())
}

fn code_shape_edited(code: &CodeModel) -> bool {
    let Some(original) = &code.original_code_shape else {
        return false;
    };
    code.instructions != original.instructions
        || code.exception_handlers != original.exception_handlers
}

fn first_frame_sensitive_attr_name(attributes: &[AttributeInfo]) -> Option<&str> {
    attributes.iter().find_map(stack_map_attr_name)
}

fn instruction_length(instruction: &Instruction) -> u32 {
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
            1 + switch_padding(insn.offset) + 8 + (insn.pairs.len() as u32 * 8)
        }
        Instruction::TableSwitch(insn) => {
            1 + switch_padding(insn.offset) + 12 + (insn.offsets.len() as u32 * 4)
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
    let after_opcode = offset + 1;
    (4 - (after_opcode % 4)) % 4
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
