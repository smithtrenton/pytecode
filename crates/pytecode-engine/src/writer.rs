use crate::bytes::ByteWriter;
use crate::constants::MAGIC;
use crate::error::{EngineError, EngineErrorKind, Result};
use crate::raw::attributes::{
    AnnotationDefaultAttribute, AnnotationInfo, AttributeInfo, BootstrapMethodsAttribute,
    CodeAttribute, ConstantValueAttribute, DeprecatedAttribute, ElementValueInfo, ElementValueTag,
    EnclosingMethodAttribute, ExceptionsAttribute, InnerClassesAttribute, LineNumberTableAttribute,
    LocalVariableTableAttribute, LocalVariableTypeTableAttribute, MethodParametersAttribute,
    ModuleAttribute, ModuleInfo as ModuleAttributeInfo, ModuleMainClassAttribute,
    ModulePackagesAttribute, NestHostAttribute, NestMembersAttribute, ParameterAnnotationInfo,
    PermittedSubclassesAttribute, RecordAttribute, RecordComponentInfo,
    RuntimeInvisibleAnnotationsAttribute, RuntimeInvisibleParameterAnnotationsAttribute,
    RuntimeInvisibleTypeAnnotationsAttribute, RuntimeVisibleAnnotationsAttribute,
    RuntimeVisibleParameterAnnotationsAttribute, RuntimeVisibleTypeAnnotationsAttribute,
    SignatureAttribute, SourceDebugExtensionAttribute, SourceFileAttribute, StackMapFrameInfo,
    StackMapTableAttribute, SyntheticAttribute, TargetInfo, TypeAnnotationInfo, TypePathInfo,
    UnknownAttribute, VerificationTypeInfo,
};
use crate::raw::constant_pool::ConstantPoolEntry;
use crate::raw::info::{ClassFile, FieldInfo, MethodInfo};
use crate::raw::instructions::{
    Branch, ConstantPoolIndexWide, Instruction, InvokeDynamicInsn, InvokeInterfaceInsn,
    LookupSwitchInsn, MatchOffsetPair, NewArrayInsn, TableSwitchInsn, WideInstruction,
};

pub struct ClassWriter;

impl ClassWriter {
    pub fn write(classfile: &ClassFile) -> Result<Vec<u8>> {
        write_class(classfile)
    }
}

pub fn write_class(classfile: &ClassFile) -> Result<Vec<u8>> {
    let mut writer = ByteWriter::with_capacity(4096);
    writer.write_u4(classfile.magic);
    if classfile.magic != MAGIC {
        return Err(EngineError::new(
            0,
            EngineErrorKind::InvalidMagic {
                found: classfile.magic,
                expected: MAGIC,
            },
        ));
    }
    writer.write_u2(classfile.minor_version);
    writer.write_u2(classfile.major_version);

    writer.write_u2(classfile.constant_pool.len() as u16);
    for entry in iter_constant_pool_entries(&classfile.constant_pool)? {
        write_constant_pool_entry(&mut writer, entry);
    }

    writer.write_u2(classfile.access_flags.bits());
    writer.write_u2(classfile.this_class);
    writer.write_u2(classfile.super_class);
    writer.write_u2(classfile.interfaces.len() as u16);
    for interface in &classfile.interfaces {
        writer.write_u2(*interface);
    }

    writer.write_u2(classfile.fields.len() as u16);
    for field in &classfile.fields {
        write_field_info(&mut writer, field)?;
    }

    writer.write_u2(classfile.methods.len() as u16);
    for method in &classfile.methods {
        write_method_info(&mut writer, method)?;
    }

    write_attributes(&mut writer, &classfile.attributes)?;
    Ok(writer.into_bytes())
}

fn iter_constant_pool_entries(
    pool: &[Option<ConstantPoolEntry>],
) -> Result<Vec<&ConstantPoolEntry>> {
    if pool.is_empty() {
        return Err(EngineError::new(
            0,
            EngineErrorKind::InvalidWriterState {
                reason: "constant pool must include slot 0".to_owned(),
            },
        ));
    }
    if pool[0].is_some() {
        return Err(EngineError::new(
            0,
            EngineErrorKind::InvalidWriterState {
                reason: "constant pool slot 0 must be empty".to_owned(),
            },
        ));
    }

    let mut entries = Vec::new();
    let mut expect_gap = false;
    for (index, entry) in pool.iter().enumerate().skip(1) {
        if expect_gap {
            if entry.is_some() {
                return Err(EngineError::new(
                    0,
                    EngineErrorKind::ConstantPoolGapViolation { index },
                ));
            }
            expect_gap = false;
            continue;
        }
        let entry = entry.as_ref().ok_or_else(|| {
            EngineError::new(
                0,
                EngineErrorKind::InvalidWriterState {
                    reason: format!("constant pool slot {index} is unexpectedly empty"),
                },
            )
        })?;
        expect_gap = entry.is_wide();
        entries.push(entry);
    }
    if expect_gap {
        return Err(EngineError::new(
            0,
            EngineErrorKind::MissingTrailingConstantPoolGap,
        ));
    }
    Ok(entries)
}

fn write_constant_pool_entry(writer: &mut ByteWriter, entry: &ConstantPoolEntry) {
    writer.write_u1(entry.tag() as u8);
    match entry {
        ConstantPoolEntry::Utf8(info) => {
            writer.write_u2(info.bytes.len() as u16);
            writer.write_bytes(&info.bytes);
        }
        ConstantPoolEntry::Integer(info) => writer.write_u4(info.value_bytes),
        ConstantPoolEntry::Float(info) => writer.write_u4(info.value_bytes),
        ConstantPoolEntry::Long(info) => {
            writer.write_u4(info.high_bytes);
            writer.write_u4(info.low_bytes);
        }
        ConstantPoolEntry::Double(info) => {
            writer.write_u4(info.high_bytes);
            writer.write_u4(info.low_bytes);
        }
        ConstantPoolEntry::Class(info) => writer.write_u2(info.name_index),
        ConstantPoolEntry::String(info) => writer.write_u2(info.string_index),
        ConstantPoolEntry::FieldRef(info) => {
            writer.write_u2(info.class_index);
            writer.write_u2(info.name_and_type_index);
        }
        ConstantPoolEntry::MethodRef(info) => {
            writer.write_u2(info.class_index);
            writer.write_u2(info.name_and_type_index);
        }
        ConstantPoolEntry::InterfaceMethodRef(info) => {
            writer.write_u2(info.class_index);
            writer.write_u2(info.name_and_type_index);
        }
        ConstantPoolEntry::NameAndType(info) => {
            writer.write_u2(info.name_index);
            writer.write_u2(info.descriptor_index);
        }
        ConstantPoolEntry::MethodHandle(info) => {
            writer.write_u1(info.reference_kind);
            writer.write_u2(info.reference_index);
        }
        ConstantPoolEntry::MethodType(info) => writer.write_u2(info.descriptor_index),
        ConstantPoolEntry::Dynamic(info) => {
            writer.write_u2(info.bootstrap_method_attr_index);
            writer.write_u2(info.name_and_type_index);
        }
        ConstantPoolEntry::InvokeDynamic(info) => {
            writer.write_u2(info.bootstrap_method_attr_index);
            writer.write_u2(info.name_and_type_index);
        }
        ConstantPoolEntry::Module(info) => writer.write_u2(info.name_index),
        ConstantPoolEntry::Package(info) => writer.write_u2(info.name_index),
    }
}

fn write_field_info(writer: &mut ByteWriter, field: &FieldInfo) -> Result<()> {
    writer.write_u2(field.access_flags.bits());
    writer.write_u2(field.name_index);
    writer.write_u2(field.descriptor_index);
    write_attributes(writer, &field.attributes)
}

fn write_method_info(writer: &mut ByteWriter, method: &MethodInfo) -> Result<()> {
    writer.write_u2(method.access_flags.bits());
    writer.write_u2(method.name_index);
    writer.write_u2(method.descriptor_index);
    write_attributes(writer, &method.attributes)
}

fn write_attributes(writer: &mut ByteWriter, attributes: &[AttributeInfo]) -> Result<()> {
    writer.write_u2(attributes.len() as u16);
    for attribute in attributes {
        write_attribute(writer, attribute)?;
    }
    Ok(())
}

fn write_attribute(writer: &mut ByteWriter, attribute: &AttributeInfo) -> Result<()> {
    let mut payload = ByteWriter::new();
    match attribute {
        AttributeInfo::ConstantValue(attr) => write_constant_value_attribute(&mut payload, attr),
        AttributeInfo::Signature(attr) => write_signature_attribute(&mut payload, attr),
        AttributeInfo::SourceFile(attr) => write_source_file_attribute(&mut payload, attr),
        AttributeInfo::SourceDebugExtension(attr) => {
            write_source_debug_attribute(&mut payload, attr)
        }
        AttributeInfo::Synthetic(attr) => write_synthetic_attribute(&mut payload, attr),
        AttributeInfo::Deprecated(attr) => write_deprecated_attribute(&mut payload, attr),
        AttributeInfo::StackMapTable(attr) => write_stack_map_table_attribute(&mut payload, attr)?,
        AttributeInfo::Exceptions(attr) => write_exceptions_attribute(&mut payload, attr),
        AttributeInfo::InnerClasses(attr) => write_inner_classes_attribute(&mut payload, attr),
        AttributeInfo::EnclosingMethod(attr) => {
            write_enclosing_method_attribute(&mut payload, attr)
        }
        AttributeInfo::Code(attr) => write_code_attribute(&mut payload, attr)?,
        AttributeInfo::LineNumberTable(attr) => {
            write_line_number_table_attribute(&mut payload, attr)
        }
        AttributeInfo::LocalVariableTable(attr) => {
            write_local_variable_table_attribute(&mut payload, attr)
        }
        AttributeInfo::LocalVariableTypeTable(attr) => {
            write_local_variable_type_table_attribute(&mut payload, attr)
        }
        AttributeInfo::MethodParameters(attr) => {
            write_method_parameters_attribute(&mut payload, attr)
        }
        AttributeInfo::NestHost(attr) => write_nest_host_attribute(&mut payload, attr),
        AttributeInfo::NestMembers(attr) => write_nest_members_attribute(&mut payload, attr),
        AttributeInfo::RuntimeVisibleAnnotations(attr) => {
            write_runtime_visible_annotations_attribute(&mut payload, attr)?
        }
        AttributeInfo::RuntimeInvisibleAnnotations(attr) => {
            write_runtime_invisible_annotations_attribute(&mut payload, attr)?
        }
        AttributeInfo::RuntimeVisibleParameterAnnotations(attr) => {
            write_runtime_visible_parameter_annotations_attribute(&mut payload, attr)?
        }
        AttributeInfo::RuntimeInvisibleParameterAnnotations(attr) => {
            write_runtime_invisible_parameter_annotations_attribute(&mut payload, attr)?
        }
        AttributeInfo::RuntimeVisibleTypeAnnotations(attr) => {
            write_runtime_visible_type_annotations_attribute(&mut payload, attr)?
        }
        AttributeInfo::RuntimeInvisibleTypeAnnotations(attr) => {
            write_runtime_invisible_type_annotations_attribute(&mut payload, attr)?
        }
        AttributeInfo::AnnotationDefault(attr) => {
            write_annotation_default_attribute(&mut payload, attr)?
        }
        AttributeInfo::BootstrapMethods(attr) => {
            write_bootstrap_methods_attribute(&mut payload, attr)
        }
        AttributeInfo::Module(attr) => write_module_attribute(&mut payload, attr),
        AttributeInfo::ModulePackages(attr) => write_module_packages_attribute(&mut payload, attr),
        AttributeInfo::ModuleMainClass(attr) => {
            write_module_main_class_attribute(&mut payload, attr)
        }
        AttributeInfo::Record(attr) => write_record_attribute(&mut payload, attr)?,
        AttributeInfo::PermittedSubclasses(attr) => {
            write_permitted_subclasses_attribute(&mut payload, attr)
        }
        AttributeInfo::Unknown(attr) => write_unknown_attribute(&mut payload, attr),
    }
    let payload_bytes = payload.into_bytes();
    writer.write_u2(attribute.attribute_name_index());
    writer.write_u4(payload_bytes.len() as u32);
    writer.write_bytes(&payload_bytes);
    Ok(())
}

fn write_constant_value_attribute(writer: &mut ByteWriter, attribute: &ConstantValueAttribute) {
    writer.write_u2(attribute.constantvalue_index);
}

fn write_signature_attribute(writer: &mut ByteWriter, attribute: &SignatureAttribute) {
    writer.write_u2(attribute.signature_index);
}

fn write_source_file_attribute(writer: &mut ByteWriter, attribute: &SourceFileAttribute) {
    writer.write_u2(attribute.sourcefile_index);
}

fn write_source_debug_attribute(
    writer: &mut ByteWriter,
    attribute: &SourceDebugExtensionAttribute,
) {
    writer.write_bytes(&attribute.debug_extension);
}

fn write_synthetic_attribute(_writer: &mut ByteWriter, _attribute: &SyntheticAttribute) {}

fn write_deprecated_attribute(_writer: &mut ByteWriter, _attribute: &DeprecatedAttribute) {}

fn write_stack_map_table_attribute(
    writer: &mut ByteWriter,
    attribute: &StackMapTableAttribute,
) -> Result<()> {
    writer.write_u2(attribute.entries.len() as u16);
    for entry in &attribute.entries {
        write_stack_map_frame(writer, entry)?;
    }
    Ok(())
}

fn write_stack_map_frame(writer: &mut ByteWriter, frame: &StackMapFrameInfo) -> Result<()> {
    match frame {
        StackMapFrameInfo::Same { frame_type } => {
            if *frame_type > 63 {
                return Err(invalid_writer_state(format!(
                    "same_frame requires frame_type in 0..=63, got {frame_type}"
                )));
            }
            writer.write_u1(*frame_type);
        }
        StackMapFrameInfo::SameLocals1StackItem { frame_type, stack } => {
            if !(64..=127).contains(frame_type) {
                return Err(invalid_writer_state(format!(
                    "same_locals_1_stack_item_frame requires frame_type in 64..=127, got {frame_type}"
                )));
            }
            writer.write_u1(*frame_type);
            write_verification_type_info(writer, stack);
        }
        StackMapFrameInfo::SameLocals1StackItemExtended {
            frame_type,
            offset_delta,
            stack,
        } => {
            if *frame_type != 247 {
                return Err(invalid_writer_state(format!(
                    "same_locals_1_stack_item_frame_extended requires frame_type 247, got {frame_type}"
                )));
            }
            writer.write_u1(*frame_type);
            writer.write_u2(*offset_delta);
            write_verification_type_info(writer, stack);
        }
        StackMapFrameInfo::Chop {
            frame_type,
            offset_delta,
        } => {
            if !(248..=250).contains(frame_type) {
                return Err(invalid_writer_state(format!(
                    "chop_frame requires frame_type in 248..=250, got {frame_type}"
                )));
            }
            writer.write_u1(*frame_type);
            writer.write_u2(*offset_delta);
        }
        StackMapFrameInfo::SameExtended {
            frame_type,
            offset_delta,
        } => {
            if *frame_type != 251 {
                return Err(invalid_writer_state(format!(
                    "same_frame_extended requires frame_type 251, got {frame_type}"
                )));
            }
            writer.write_u1(*frame_type);
            writer.write_u2(*offset_delta);
        }
        StackMapFrameInfo::Append {
            frame_type,
            offset_delta,
            locals,
        } => {
            if !(252..=254).contains(frame_type) {
                return Err(invalid_writer_state(format!(
                    "append_frame requires frame_type in 252..=254, got {frame_type}"
                )));
            }
            if locals.len() != usize::from(*frame_type - 251) {
                return Err(invalid_writer_state(format!(
                    "append_frame locals length {} does not match frame_type {}",
                    locals.len(),
                    frame_type
                )));
            }
            writer.write_u1(*frame_type);
            writer.write_u2(*offset_delta);
            for local in locals {
                write_verification_type_info(writer, local);
            }
        }
        StackMapFrameInfo::Full {
            frame_type,
            offset_delta,
            locals,
            stack,
        } => {
            if *frame_type != 255 {
                return Err(invalid_writer_state(format!(
                    "full_frame requires frame_type 255, got {frame_type}"
                )));
            }
            writer.write_u1(*frame_type);
            writer.write_u2(*offset_delta);
            writer.write_u2(locals.len() as u16);
            for local in locals {
                write_verification_type_info(writer, local);
            }
            writer.write_u2(stack.len() as u16);
            for stack_item in stack {
                write_verification_type_info(writer, stack_item);
            }
        }
    }
    Ok(())
}

fn write_verification_type_info(writer: &mut ByteWriter, value: &VerificationTypeInfo) {
    writer.write_u1(value.tag() as u8);
    match value {
        VerificationTypeInfo::Object { cpool_index } => writer.write_u2(*cpool_index),
        VerificationTypeInfo::Uninitialized { offset } => writer.write_u2(*offset),
        _ => {}
    }
}

fn write_exceptions_attribute(writer: &mut ByteWriter, attribute: &ExceptionsAttribute) {
    writer.write_u2(attribute.exception_index_table.len() as u16);
    for index in &attribute.exception_index_table {
        writer.write_u2(*index);
    }
}

fn write_inner_classes_attribute(writer: &mut ByteWriter, attribute: &InnerClassesAttribute) {
    writer.write_u2(attribute.classes.len() as u16);
    for entry in &attribute.classes {
        writer.write_u2(entry.inner_class_info_index);
        writer.write_u2(entry.outer_class_info_index);
        writer.write_u2(entry.inner_name_index);
        writer.write_u2(entry.inner_class_access_flags.bits());
    }
}

fn write_enclosing_method_attribute(writer: &mut ByteWriter, attribute: &EnclosingMethodAttribute) {
    writer.write_u2(attribute.class_index);
    writer.write_u2(attribute.method_index);
}

fn write_code_attribute(writer: &mut ByteWriter, attribute: &CodeAttribute) -> Result<()> {
    writer.write_u2(attribute.max_stack);
    writer.write_u2(attribute.max_locals);
    let mut code_writer = ByteWriter::new();
    for instruction in &attribute.code {
        write_instruction(&mut code_writer, instruction);
    }
    let code_bytes = code_writer.into_bytes();
    writer.write_u4(code_bytes.len() as u32);
    writer.write_bytes(&code_bytes);
    writer.write_u2(attribute.exception_table.len() as u16);
    for handler in &attribute.exception_table {
        writer.write_u2(handler.start_pc);
        writer.write_u2(handler.end_pc);
        writer.write_u2(handler.handler_pc);
        writer.write_u2(handler.catch_type);
    }
    write_attributes(writer, &attribute.attributes)
}

fn write_line_number_table_attribute(
    writer: &mut ByteWriter,
    attribute: &LineNumberTableAttribute,
) {
    writer.write_u2(attribute.line_number_table.len() as u16);
    for entry in &attribute.line_number_table {
        writer.write_u2(entry.start_pc);
        writer.write_u2(entry.line_number);
    }
}

fn write_local_variable_table_attribute(
    writer: &mut ByteWriter,
    attribute: &LocalVariableTableAttribute,
) {
    writer.write_u2(attribute.local_variable_table.len() as u16);
    for entry in &attribute.local_variable_table {
        writer.write_u2(entry.start_pc);
        writer.write_u2(entry.length);
        writer.write_u2(entry.name_index);
        writer.write_u2(entry.descriptor_index);
        writer.write_u2(entry.index);
    }
}

fn write_local_variable_type_table_attribute(
    writer: &mut ByteWriter,
    attribute: &LocalVariableTypeTableAttribute,
) {
    writer.write_u2(attribute.local_variable_type_table.len() as u16);
    for entry in &attribute.local_variable_type_table {
        writer.write_u2(entry.start_pc);
        writer.write_u2(entry.length);
        writer.write_u2(entry.name_index);
        writer.write_u2(entry.signature_index);
        writer.write_u2(entry.index);
    }
}

fn write_method_parameters_attribute(
    writer: &mut ByteWriter,
    attribute: &MethodParametersAttribute,
) {
    writer.write_u1(attribute.parameters.len() as u8);
    for parameter in &attribute.parameters {
        writer.write_u2(parameter.name_index);
        writer.write_u2(parameter.access_flags.bits());
    }
}

fn write_nest_host_attribute(writer: &mut ByteWriter, attribute: &NestHostAttribute) {
    writer.write_u2(attribute.host_class_index);
}

fn write_nest_members_attribute(writer: &mut ByteWriter, attribute: &NestMembersAttribute) {
    writer.write_u2(attribute.classes.len() as u16);
    for class_index in &attribute.classes {
        writer.write_u2(*class_index);
    }
}

fn write_runtime_visible_annotations_attribute(
    writer: &mut ByteWriter,
    attribute: &RuntimeVisibleAnnotationsAttribute,
) -> Result<()> {
    write_annotations(writer, &attribute.annotations)
}

fn write_runtime_invisible_annotations_attribute(
    writer: &mut ByteWriter,
    attribute: &RuntimeInvisibleAnnotationsAttribute,
) -> Result<()> {
    write_annotations(writer, &attribute.annotations)
}

fn write_runtime_visible_parameter_annotations_attribute(
    writer: &mut ByteWriter,
    attribute: &RuntimeVisibleParameterAnnotationsAttribute,
) -> Result<()> {
    write_parameter_annotations(writer, &attribute.parameter_annotations)
}

fn write_runtime_invisible_parameter_annotations_attribute(
    writer: &mut ByteWriter,
    attribute: &RuntimeInvisibleParameterAnnotationsAttribute,
) -> Result<()> {
    write_parameter_annotations(writer, &attribute.parameter_annotations)
}

fn write_annotation_default_attribute(
    writer: &mut ByteWriter,
    attribute: &AnnotationDefaultAttribute,
) -> Result<()> {
    write_element_value_info(writer, &attribute.default_value)
}

fn write_runtime_visible_type_annotations_attribute(
    writer: &mut ByteWriter,
    attribute: &RuntimeVisibleTypeAnnotationsAttribute,
) -> Result<()> {
    write_type_annotations(writer, &attribute.annotations)
}

fn write_runtime_invisible_type_annotations_attribute(
    writer: &mut ByteWriter,
    attribute: &RuntimeInvisibleTypeAnnotationsAttribute,
) -> Result<()> {
    write_type_annotations(writer, &attribute.annotations)
}

fn write_bootstrap_methods_attribute(
    writer: &mut ByteWriter,
    attribute: &BootstrapMethodsAttribute,
) {
    writer.write_u2(attribute.bootstrap_methods.len() as u16);
    for bootstrap_method in &attribute.bootstrap_methods {
        writer.write_u2(bootstrap_method.bootstrap_method_ref);
        writer.write_u2(bootstrap_method.bootstrap_arguments.len() as u16);
        for argument in &bootstrap_method.bootstrap_arguments {
            writer.write_u2(*argument);
        }
    }
}

fn write_module_attribute(writer: &mut ByteWriter, attribute: &ModuleAttribute) {
    write_module_info(writer, &attribute.module);
}

fn write_module_info(writer: &mut ByteWriter, module: &ModuleAttributeInfo) {
    writer.write_u2(module.module_name_index);
    writer.write_u2(module.module_flags.bits());
    writer.write_u2(module.module_version_index);

    writer.write_u2(module.requires.len() as u16);
    for requires in &module.requires {
        writer.write_u2(requires.requires_index);
        writer.write_u2(requires.requires_flags.bits());
        writer.write_u2(requires.requires_version_index);
    }

    writer.write_u2(module.exports.len() as u16);
    for exports in &module.exports {
        writer.write_u2(exports.exports_index);
        writer.write_u2(exports.exports_flags.bits());
        writer.write_u2(exports.exports_to_index.len() as u16);
        for target in &exports.exports_to_index {
            writer.write_u2(*target);
        }
    }

    writer.write_u2(module.opens.len() as u16);
    for opens in &module.opens {
        writer.write_u2(opens.opens_index);
        writer.write_u2(opens.opens_flags.bits());
        writer.write_u2(opens.opens_to_index.len() as u16);
        for target in &opens.opens_to_index {
            writer.write_u2(*target);
        }
    }

    writer.write_u2(module.uses_index.len() as u16);
    for use_index in &module.uses_index {
        writer.write_u2(*use_index);
    }

    writer.write_u2(module.provides.len() as u16);
    for provides in &module.provides {
        writer.write_u2(provides.provides_index);
        writer.write_u2(provides.provides_with_index.len() as u16);
        for implementation in &provides.provides_with_index {
            writer.write_u2(*implementation);
        }
    }
}

fn write_module_packages_attribute(writer: &mut ByteWriter, attribute: &ModulePackagesAttribute) {
    writer.write_u2(attribute.package_index.len() as u16);
    for package_index in &attribute.package_index {
        writer.write_u2(*package_index);
    }
}

fn write_module_main_class_attribute(
    writer: &mut ByteWriter,
    attribute: &ModuleMainClassAttribute,
) {
    writer.write_u2(attribute.main_class_index);
}

fn write_record_attribute(writer: &mut ByteWriter, attribute: &RecordAttribute) -> Result<()> {
    writer.write_u2(attribute.components.len() as u16);
    for component in &attribute.components {
        write_record_component_info(writer, component)?;
    }
    Ok(())
}

fn write_record_component_info(
    writer: &mut ByteWriter,
    component: &RecordComponentInfo,
) -> Result<()> {
    writer.write_u2(component.name_index);
    writer.write_u2(component.descriptor_index);
    write_attributes(writer, &component.attributes)
}

fn write_permitted_subclasses_attribute(
    writer: &mut ByteWriter,
    attribute: &PermittedSubclassesAttribute,
) {
    writer.write_u2(attribute.classes.len() as u16);
    for class_index in &attribute.classes {
        writer.write_u2(*class_index);
    }
}

fn write_type_annotations(
    writer: &mut ByteWriter,
    annotations: &[TypeAnnotationInfo],
) -> Result<()> {
    writer.write_u2(annotations.len() as u16);
    for annotation in annotations {
        write_type_annotation_info(writer, annotation)?;
    }
    Ok(())
}

fn write_type_annotation_info(
    writer: &mut ByteWriter,
    annotation: &TypeAnnotationInfo,
) -> Result<()> {
    if !annotation
        .target_info
        .target_info_type()
        .matches_target_type(annotation.target_type)
    {
        return Err(invalid_writer_state(format!(
            "type annotation target_info {:?} does not match target_type {:?}",
            annotation.target_info.target_info_type(),
            annotation.target_type
        )));
    }
    writer.write_u1(annotation.target_type as u8);
    write_target_info(writer, &annotation.target_info);
    write_type_path_info(writer, &annotation.target_path);
    writer.write_u2(annotation.type_index);
    writer.write_u2(annotation.element_value_pairs.len() as u16);
    for pair in &annotation.element_value_pairs {
        writer.write_u2(pair.element_name_index);
        write_element_value_info(writer, &pair.element_value)?;
    }
    Ok(())
}

fn write_target_info(writer: &mut ByteWriter, target_info: &TargetInfo) {
    match target_info {
        TargetInfo::TypeParameter {
            type_parameter_index,
        } => writer.write_u1(*type_parameter_index),
        TargetInfo::Supertype { supertype_index } => writer.write_u2(*supertype_index),
        TargetInfo::TypeParameterBound {
            type_parameter_index,
            bound_index,
        } => {
            writer.write_u1(*type_parameter_index);
            writer.write_u1(*bound_index);
        }
        TargetInfo::Empty => {}
        TargetInfo::FormalParameter {
            formal_parameter_index,
        } => writer.write_u1(*formal_parameter_index),
        TargetInfo::Throws { throws_type_index } => writer.write_u2(*throws_type_index),
        TargetInfo::Localvar { table } => {
            writer.write_u2(table.len() as u16);
            for entry in table {
                writer.write_u2(entry.start_pc);
                writer.write_u2(entry.length);
                writer.write_u2(entry.index);
            }
        }
        TargetInfo::Catch {
            exception_table_index,
        } => writer.write_u2(*exception_table_index),
        TargetInfo::Offset { offset } => writer.write_u2(*offset),
        TargetInfo::TypeArgument {
            offset,
            type_argument_index,
        } => {
            writer.write_u2(*offset);
            writer.write_u1(*type_argument_index);
        }
    }
}

fn write_type_path_info(writer: &mut ByteWriter, type_path: &TypePathInfo) {
    writer.write_u1(type_path.path.len() as u8);
    for entry in &type_path.path {
        writer.write_u1(entry.type_path_kind as u8);
        writer.write_u1(entry.type_argument_index);
    }
}

fn write_annotations(writer: &mut ByteWriter, annotations: &[AnnotationInfo]) -> Result<()> {
    writer.write_u2(annotations.len() as u16);
    for annotation in annotations {
        write_annotation_info(writer, annotation)?;
    }
    Ok(())
}

fn write_parameter_annotations(
    writer: &mut ByteWriter,
    annotations: &[ParameterAnnotationInfo],
) -> Result<()> {
    writer.write_u1(annotations.len() as u8);
    for annotation in annotations {
        writer.write_u2(annotation.annotations.len() as u16);
        for nested in &annotation.annotations {
            write_annotation_info(writer, nested)?;
        }
    }
    Ok(())
}

fn write_annotation_info(writer: &mut ByteWriter, annotation: &AnnotationInfo) -> Result<()> {
    writer.write_u2(annotation.type_index);
    writer.write_u2(annotation.element_value_pairs.len() as u16);
    for pair in &annotation.element_value_pairs {
        writer.write_u2(pair.element_name_index);
        write_element_value_info(writer, &pair.element_value)?;
    }
    Ok(())
}

fn write_element_value_info(writer: &mut ByteWriter, value: &ElementValueInfo) -> Result<()> {
    match value {
        ElementValueInfo::Const {
            tag,
            const_value_index,
        } => match tag {
            ElementValueTag::Byte
            | ElementValueTag::Char
            | ElementValueTag::Double
            | ElementValueTag::Float
            | ElementValueTag::Int
            | ElementValueTag::Long
            | ElementValueTag::Short
            | ElementValueTag::Boolean
            | ElementValueTag::String => {
                writer.write_u1(*tag as u8);
                writer.write_u2(*const_value_index);
            }
            _ => {
                return Err(invalid_writer_state(format!(
                    "const element value cannot use tag {:?}",
                    tag
                )));
            }
        },
        ElementValueInfo::Enum {
            type_name_index,
            const_name_index,
        } => {
            writer.write_u1(ElementValueTag::Enum as u8);
            writer.write_u2(*type_name_index);
            writer.write_u2(*const_name_index);
        }
        ElementValueInfo::Class { class_info_index } => {
            writer.write_u1(ElementValueTag::Class as u8);
            writer.write_u2(*class_info_index);
        }
        ElementValueInfo::Annotation(annotation) => {
            writer.write_u1(ElementValueTag::Annotation as u8);
            write_annotation_info(writer, annotation)?;
        }
        ElementValueInfo::Array { values } => {
            writer.write_u1(ElementValueTag::Array as u8);
            writer.write_u2(values.len() as u16);
            for nested in values {
                write_element_value_info(writer, nested)?;
            }
        }
    }
    Ok(())
}

fn write_unknown_attribute(writer: &mut ByteWriter, attribute: &UnknownAttribute) {
    writer.write_bytes(&attribute.info);
}

fn invalid_writer_state(reason: impl Into<String>) -> EngineError {
    EngineError::new(
        0,
        EngineErrorKind::InvalidWriterState {
            reason: reason.into(),
        },
    )
}

fn write_instruction(writer: &mut ByteWriter, instruction: &Instruction) {
    match instruction {
        Instruction::Simple { opcode, .. } => writer.write_u1(*opcode),
        Instruction::LocalIndex { opcode, index, .. } => {
            writer.write_u1(*opcode);
            writer.write_u1(*index);
        }
        Instruction::ConstantPoolIndex1 { opcode, index, .. } => {
            writer.write_u1(*opcode);
            writer.write_u1(*index);
        }
        Instruction::ConstantPoolIndexWide(ConstantPoolIndexWide { opcode, index, .. }) => {
            writer.write_u1(*opcode);
            writer.write_u2(*index);
        }
        Instruction::Byte { opcode, value, .. } => {
            writer.write_u1(*opcode);
            writer.write_i1(*value);
        }
        Instruction::Short { opcode, value, .. } => {
            writer.write_u1(*opcode);
            writer.write_i2(*value);
        }
        Instruction::Branch(Branch {
            opcode,
            branch_offset,
            ..
        }) => {
            writer.write_u1(*opcode);
            writer.write_i2(*branch_offset);
        }
        Instruction::BranchWide {
            opcode,
            branch_offset,
            ..
        } => {
            writer.write_u1(*opcode);
            writer.write_i4(*branch_offset);
        }
        Instruction::IInc { index, value, .. } => {
            writer.write_u1(0x84);
            writer.write_u1(*index);
            writer.write_i1(*value);
        }
        Instruction::InvokeDynamic(InvokeDynamicInsn {
            index, reserved, ..
        }) => {
            writer.write_u1(0xBA);
            writer.write_u2(*index);
            writer.write_u2(*reserved);
        }
        Instruction::InvokeInterface(InvokeInterfaceInsn {
            index,
            count,
            reserved,
            ..
        }) => {
            writer.write_u1(0xB9);
            writer.write_u2(*index);
            writer.write_u1(*count);
            writer.write_u1(*reserved);
        }
        Instruction::NewArray(NewArrayInsn { atype, .. }) => {
            writer.write_u1(0xBC);
            writer.write_u1(*atype as u8);
        }
        Instruction::MultiANewArray {
            index, dimensions, ..
        } => {
            writer.write_u1(0xC5);
            writer.write_u2(*index);
            writer.write_u1(*dimensions);
        }
        Instruction::LookupSwitch(LookupSwitchInsn {
            offset,
            default_offset,
            pairs,
        }) => {
            writer.write_u1(0xAB);
            write_switch_padding(writer, *offset);
            writer.write_i4(*default_offset);
            writer.write_u4(pairs.len() as u32);
            for MatchOffsetPair {
                match_value,
                offset,
            } in pairs
            {
                writer.write_i4(*match_value);
                writer.write_i4(*offset);
            }
        }
        Instruction::TableSwitch(TableSwitchInsn {
            offset,
            default_offset,
            low,
            high,
            offsets,
        }) => {
            writer.write_u1(0xAA);
            write_switch_padding(writer, *offset);
            writer.write_i4(*default_offset);
            writer.write_i4(*low);
            writer.write_i4(*high);
            for branch in offsets {
                writer.write_i4(*branch);
            }
        }
        Instruction::Wide(WideInstruction {
            opcode,
            index,
            value,
            ..
        }) => {
            writer.write_u1(0xC4);
            writer.write_u1(*opcode);
            writer.write_u2(*index);
            if let Some(value) = value {
                writer.write_i2(*value);
            }
        }
    }
}

fn write_switch_padding(writer: &mut ByteWriter, offset: u32) {
    let padding = (4 - ((offset + 1) % 4)) % 4;
    for _ in 0..padding {
        writer.write_u1(0);
    }
}
