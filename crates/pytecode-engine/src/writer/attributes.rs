//! Attribute payload emission and bounded nesting.
use super::{checked_count, invalid_writer_state, write_instruction};
use crate::bytes::ByteWriter;
use crate::error::Result;
use crate::raw::attributes::{
    AnnotationDefaultAttribute, AnnotationInfo, AttributeInfo, BootstrapMethodsAttribute,
    CodeAttribute, ConstantValueAttribute, DeprecatedAttribute, ElementValueInfo, ElementValueTag,
    EnclosingMethodAttribute, ExceptionsAttribute, InnerClassesAttribute, LineNumberTableAttribute,
    LocalVariableTableAttribute, LocalVariableTypeTableAttribute, MethodParametersAttribute,
    ModuleAttribute, ModuleAttributeModuleInfo, ModuleMainClassAttribute, ModulePackagesAttribute,
    NestHostAttribute, NestMembersAttribute, ParameterAnnotationInfo, PermittedSubclassesAttribute,
    RecordAttribute, RecordComponentInfo, RuntimeInvisibleAnnotationsAttribute,
    RuntimeInvisibleParameterAnnotationsAttribute, RuntimeInvisibleTypeAnnotationsAttribute,
    RuntimeVisibleAnnotationsAttribute, RuntimeVisibleParameterAnnotationsAttribute,
    RuntimeVisibleTypeAnnotationsAttribute, SignatureAttribute, SourceDebugExtensionAttribute,
    SourceFileAttribute, StackMapFrameInfo, StackMapTableAttribute, SyntheticAttribute, TargetInfo,
    TypeAnnotationInfo, TypePathInfo, UnknownAttribute, VerificationTypeInfo,
};

pub(super) fn write_attributes(
    writer: &mut ByteWriter,
    attributes: &[AttributeInfo],
    depth: usize,
) -> Result<()> {
    writer.write_u2(checked_count::<u16>(attributes.len(), "attributes")?);
    for attribute in attributes {
        write_attribute(writer, attribute, depth + 1)?;
    }
    Ok(())
}

fn write_attribute(writer: &mut ByteWriter, attribute: &AttributeInfo, depth: usize) -> Result<()> {
    if depth >= 128 {
        return Err(invalid_writer_state(
            "attribute/annotation nesting exceeds limit of 128",
        ));
    }

    writer.write_u2(attribute.attribute_name_index().into());
    let length_position = writer.position();
    writer.write_u4(0);
    let payload_start = writer.position();
    match attribute {
        AttributeInfo::ConstantValue(attr) => write_constant_value_attribute(writer, attr)?,
        AttributeInfo::Signature(attr) => write_signature_attribute(writer, attr)?,
        AttributeInfo::SourceFile(attr) => write_source_file_attribute(writer, attr)?,
        AttributeInfo::SourceDebugExtension(attr) => write_source_debug_attribute(writer, attr)?,
        AttributeInfo::Synthetic(attr) => write_synthetic_attribute(writer, attr)?,
        AttributeInfo::Deprecated(attr) => write_deprecated_attribute(writer, attr)?,
        AttributeInfo::StackMapTable(attr) => write_stack_map_table_attribute(writer, attr)?,
        AttributeInfo::Exceptions(attr) => write_exceptions_attribute(writer, attr)?,
        AttributeInfo::InnerClasses(attr) => write_inner_classes_attribute(writer, attr)?,
        AttributeInfo::EnclosingMethod(attr) => write_enclosing_method_attribute(writer, attr)?,
        AttributeInfo::Code(attr) => write_code_attribute(writer, attr, depth)?,
        AttributeInfo::LineNumberTable(attr) => write_line_number_table_attribute(writer, attr)?,
        AttributeInfo::LocalVariableTable(attr) => {
            write_local_variable_table_attribute(writer, attr)?
        }
        AttributeInfo::LocalVariableTypeTable(attr) => {
            write_local_variable_type_table_attribute(writer, attr)?
        }
        AttributeInfo::MethodParameters(attr) => write_method_parameters_attribute(writer, attr)?,
        AttributeInfo::NestHost(attr) => write_nest_host_attribute(writer, attr)?,
        AttributeInfo::NestMembers(attr) => write_nest_members_attribute(writer, attr)?,
        AttributeInfo::RuntimeVisibleAnnotations(attr) => {
            write_runtime_visible_annotations_attribute(writer, attr, depth)?
        }
        AttributeInfo::RuntimeInvisibleAnnotations(attr) => {
            write_runtime_invisible_annotations_attribute(writer, attr, depth)?
        }
        AttributeInfo::RuntimeVisibleParameterAnnotations(attr) => {
            write_runtime_visible_parameter_annotations_attribute(writer, attr, depth)?
        }
        AttributeInfo::RuntimeInvisibleParameterAnnotations(attr) => {
            write_runtime_invisible_parameter_annotations_attribute(writer, attr, depth)?
        }
        AttributeInfo::RuntimeVisibleTypeAnnotations(attr) => {
            write_runtime_visible_type_annotations_attribute(writer, attr, depth)?
        }
        AttributeInfo::RuntimeInvisibleTypeAnnotations(attr) => {
            write_runtime_invisible_type_annotations_attribute(writer, attr, depth)?
        }
        AttributeInfo::AnnotationDefault(attr) => {
            write_annotation_default_attribute(writer, attr, depth)?
        }
        AttributeInfo::BootstrapMethods(attr) => write_bootstrap_methods_attribute(writer, attr)?,
        AttributeInfo::Module(attr) => write_module_attribute(writer, attr)?,
        AttributeInfo::ModulePackages(attr) => write_module_packages_attribute(writer, attr)?,
        AttributeInfo::ModuleMainClass(attr) => write_module_main_class_attribute(writer, attr)?,
        AttributeInfo::Record(attr) => write_record_attribute(writer, attr, depth)?,
        AttributeInfo::PermittedSubclasses(attr) => {
            write_permitted_subclasses_attribute(writer, attr)?
        }
        AttributeInfo::Unknown(attr) => write_unknown_attribute(writer, attr)?,
    }
    let payload_len = checked_count::<u32>(writer.position() - payload_start, "attribute payload")?;
    writer.patch_u4(length_position, payload_len);
    Ok(())
}

fn write_constant_value_attribute(
    writer: &mut ByteWriter,
    attribute: &ConstantValueAttribute,
) -> Result<()> {
    writer.write_u2(attribute.constantvalue_index.into());

    Ok(())
}

fn write_signature_attribute(
    writer: &mut ByteWriter,
    attribute: &SignatureAttribute,
) -> Result<()> {
    writer.write_u2(attribute.signature_index.into());

    Ok(())
}

fn write_source_file_attribute(
    writer: &mut ByteWriter,
    attribute: &SourceFileAttribute,
) -> Result<()> {
    writer.write_u2(attribute.sourcefile_index.into());

    Ok(())
}

fn write_source_debug_attribute(
    writer: &mut ByteWriter,
    attribute: &SourceDebugExtensionAttribute,
) -> Result<()> {
    writer.write_bytes(&attribute.debug_extension);

    Ok(())
}

fn write_synthetic_attribute(
    _writer: &mut ByteWriter,
    _attribute: &SyntheticAttribute,
) -> Result<()> {
    Ok(())
}

fn write_deprecated_attribute(
    _writer: &mut ByteWriter,
    _attribute: &DeprecatedAttribute,
) -> Result<()> {
    Ok(())
}

fn write_stack_map_table_attribute(
    writer: &mut ByteWriter,
    attribute: &StackMapTableAttribute,
) -> Result<()> {
    writer.write_u2(checked_count::<u16>(
        attribute.entries.len(),
        "attribute.entries",
    )?);
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
            write_verification_type_info(writer, stack)?;
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
            write_verification_type_info(writer, stack)?;
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
                write_verification_type_info(writer, local)?;
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
            writer.write_u2(checked_count::<u16>(locals.len(), "locals")?);
            for local in locals {
                write_verification_type_info(writer, local)?;
            }
            writer.write_u2(checked_count::<u16>(stack.len(), "stack")?);
            for stack_item in stack {
                write_verification_type_info(writer, stack_item)?;
            }
        }
    }
    Ok(())
}

fn write_verification_type_info(
    writer: &mut ByteWriter,
    value: &VerificationTypeInfo,
) -> Result<()> {
    writer.write_u1(value.tag() as u8);
    match value {
        VerificationTypeInfo::Object { cpool_index } => writer.write_u2((*cpool_index).into()),
        VerificationTypeInfo::Uninitialized { offset } => writer.write_u2(*offset),
        _ => {}
    }

    Ok(())
}

fn write_exceptions_attribute(
    writer: &mut ByteWriter,
    attribute: &ExceptionsAttribute,
) -> Result<()> {
    writer.write_u2(checked_count::<u16>(
        attribute.exception_index_table.len(),
        "attribute.exception_index_table",
    )?);
    for index in &attribute.exception_index_table {
        writer.write_u2((*index).into());
    }

    Ok(())
}

fn write_inner_classes_attribute(
    writer: &mut ByteWriter,
    attribute: &InnerClassesAttribute,
) -> Result<()> {
    writer.write_u2(checked_count::<u16>(
        attribute.classes.len(),
        "attribute.classes",
    )?);
    for entry in &attribute.classes {
        writer.write_u2(entry.inner_class_info_index.into());
        writer.write_u2(entry.outer_class_info_index.into());
        writer.write_u2(entry.inner_name_index.into());
        writer.write_u2(entry.inner_class_access_flags.bits());
    }

    Ok(())
}

fn write_enclosing_method_attribute(
    writer: &mut ByteWriter,
    attribute: &EnclosingMethodAttribute,
) -> Result<()> {
    writer.write_u2(attribute.class_index.into());
    writer.write_u2(attribute.method_index.into());

    Ok(())
}

fn write_code_attribute(
    writer: &mut ByteWriter,
    attribute: &CodeAttribute,
    depth: usize,
) -> Result<()> {
    writer.write_u2(attribute.max_stack);
    writer.write_u2(attribute.max_locals);
    let length_position = writer.position();
    writer.write_u4(0);
    let code_start = writer.position();
    for instruction in &attribute.code {
        write_instruction(writer, instruction)?;
    }
    let code_length = writer.position() - code_start;
    if !(1..=65535).contains(&code_length) {
        return Err(invalid_writer_state("Code length must be in 1..=65535"));
    }
    writer.patch_u4(
        length_position,
        checked_count::<u32>(code_length, "code bytes")?,
    );
    writer.write_u2(checked_count::<u16>(
        attribute.exception_table.len(),
        "attribute.exception_table",
    )?);
    for handler in &attribute.exception_table {
        writer.write_u2(handler.start_pc);
        writer.write_u2(handler.end_pc);
        writer.write_u2(handler.handler_pc);
        writer.write_u2(handler.catch_type.into());
    }
    write_attributes(writer, &attribute.attributes, depth)
}

fn write_line_number_table_attribute(
    writer: &mut ByteWriter,
    attribute: &LineNumberTableAttribute,
) -> Result<()> {
    writer.write_u2(checked_count::<u16>(
        attribute.line_number_table.len(),
        "attribute.line_number_table",
    )?);
    for entry in &attribute.line_number_table {
        writer.write_u2(entry.start_pc);
        writer.write_u2(entry.line_number);
    }

    Ok(())
}

fn write_local_variable_table_attribute(
    writer: &mut ByteWriter,
    attribute: &LocalVariableTableAttribute,
) -> Result<()> {
    writer.write_u2(checked_count::<u16>(
        attribute.local_variable_table.len(),
        "attribute.local_variable_table",
    )?);
    for entry in &attribute.local_variable_table {
        writer.write_u2(entry.start_pc);
        writer.write_u2(entry.length);
        writer.write_u2(entry.name_index.into());
        writer.write_u2(entry.descriptor_index.into());
        writer.write_u2(entry.index);
    }

    Ok(())
}

fn write_local_variable_type_table_attribute(
    writer: &mut ByteWriter,
    attribute: &LocalVariableTypeTableAttribute,
) -> Result<()> {
    writer.write_u2(checked_count::<u16>(
        attribute.local_variable_type_table.len(),
        "attribute.local_variable_type_table",
    )?);
    for entry in &attribute.local_variable_type_table {
        writer.write_u2(entry.start_pc);
        writer.write_u2(entry.length);
        writer.write_u2(entry.name_index.into());
        writer.write_u2(entry.signature_index.into());
        writer.write_u2(entry.index);
    }

    Ok(())
}

fn write_method_parameters_attribute(
    writer: &mut ByteWriter,
    attribute: &MethodParametersAttribute,
) -> Result<()> {
    writer.write_u1(checked_count::<u8>(
        attribute.parameters.len(),
        "attribute.parameters",
    )?);
    for parameter in &attribute.parameters {
        writer.write_u2(parameter.name_index.into());
        writer.write_u2(parameter.access_flags.bits());
    }

    Ok(())
}

fn write_nest_host_attribute(writer: &mut ByteWriter, attribute: &NestHostAttribute) -> Result<()> {
    writer.write_u2(attribute.host_class_index.into());

    Ok(())
}

fn write_nest_members_attribute(
    writer: &mut ByteWriter,
    attribute: &NestMembersAttribute,
) -> Result<()> {
    writer.write_u2(checked_count::<u16>(
        attribute.classes.len(),
        "attribute.classes",
    )?);
    for class_index in &attribute.classes {
        writer.write_u2((*class_index).into());
    }

    Ok(())
}

fn write_runtime_visible_annotations_attribute(
    writer: &mut ByteWriter,
    attribute: &RuntimeVisibleAnnotationsAttribute,
    depth: usize,
) -> Result<()> {
    write_annotations(writer, &attribute.annotations, depth)
}

fn write_runtime_invisible_annotations_attribute(
    writer: &mut ByteWriter,
    attribute: &RuntimeInvisibleAnnotationsAttribute,
    depth: usize,
) -> Result<()> {
    write_annotations(writer, &attribute.annotations, depth)
}

fn write_runtime_visible_parameter_annotations_attribute(
    writer: &mut ByteWriter,
    attribute: &RuntimeVisibleParameterAnnotationsAttribute,
    depth: usize,
) -> Result<()> {
    write_parameter_annotations(writer, &attribute.parameter_annotations, depth)
}

fn write_runtime_invisible_parameter_annotations_attribute(
    writer: &mut ByteWriter,
    attribute: &RuntimeInvisibleParameterAnnotationsAttribute,
    depth: usize,
) -> Result<()> {
    write_parameter_annotations(writer, &attribute.parameter_annotations, depth)
}

fn write_annotation_default_attribute(
    writer: &mut ByteWriter,
    attribute: &AnnotationDefaultAttribute,
    depth: usize,
) -> Result<()> {
    write_element_value_info(writer, &attribute.default_value, depth + 1)
}

fn write_runtime_visible_type_annotations_attribute(
    writer: &mut ByteWriter,
    attribute: &RuntimeVisibleTypeAnnotationsAttribute,
    depth: usize,
) -> Result<()> {
    write_type_annotations(writer, &attribute.annotations, depth)
}

fn write_runtime_invisible_type_annotations_attribute(
    writer: &mut ByteWriter,
    attribute: &RuntimeInvisibleTypeAnnotationsAttribute,
    depth: usize,
) -> Result<()> {
    write_type_annotations(writer, &attribute.annotations, depth)
}

fn write_bootstrap_methods_attribute(
    writer: &mut ByteWriter,
    attribute: &BootstrapMethodsAttribute,
) -> Result<()> {
    writer.write_u2(checked_count::<u16>(
        attribute.bootstrap_methods.len(),
        "attribute.bootstrap_methods",
    )?);
    for bootstrap_method in &attribute.bootstrap_methods {
        writer.write_u2(bootstrap_method.bootstrap_method_ref.into());
        writer.write_u2(checked_count::<u16>(
            bootstrap_method.bootstrap_arguments.len(),
            "bootstrap_method.bootstrap_arguments",
        )?);
        for argument in &bootstrap_method.bootstrap_arguments {
            writer.write_u2((*argument).into());
        }
    }

    Ok(())
}

fn write_module_attribute(writer: &mut ByteWriter, attribute: &ModuleAttribute) -> Result<()> {
    write_module_info(writer, &attribute.module)?;

    Ok(())
}

fn write_module_info(writer: &mut ByteWriter, module: &ModuleAttributeModuleInfo) -> Result<()> {
    writer.write_u2(module.module_name_index.into());
    writer.write_u2(module.module_flags.bits());
    writer.write_u2(module.module_version_index.into());

    writer.write_u2(checked_count::<u16>(
        module.requires.len(),
        "module.requires",
    )?);
    for requires in &module.requires {
        writer.write_u2(requires.requires_index.into());
        writer.write_u2(requires.requires_flags.bits());
        writer.write_u2(requires.requires_version_index.into());
    }

    writer.write_u2(checked_count::<u16>(
        module.exports.len(),
        "module.exports",
    )?);
    for exports in &module.exports {
        writer.write_u2(exports.exports_index.into());
        writer.write_u2(exports.exports_flags.bits());
        writer.write_u2(checked_count::<u16>(
            exports.exports_to_index.len(),
            "exports.exports_to_index",
        )?);
        for target in &exports.exports_to_index {
            writer.write_u2((*target).into());
        }
    }

    writer.write_u2(checked_count::<u16>(module.opens.len(), "module.opens")?);
    for opens in &module.opens {
        writer.write_u2(opens.opens_index.into());
        writer.write_u2(opens.opens_flags.bits());
        writer.write_u2(checked_count::<u16>(
            opens.opens_to_index.len(),
            "opens.opens_to_index",
        )?);
        for target in &opens.opens_to_index {
            writer.write_u2((*target).into());
        }
    }

    writer.write_u2(checked_count::<u16>(
        module.uses_index.len(),
        "module.uses_index",
    )?);
    for use_index in &module.uses_index {
        writer.write_u2((*use_index).into());
    }

    writer.write_u2(checked_count::<u16>(
        module.provides.len(),
        "module.provides",
    )?);
    for provides in &module.provides {
        writer.write_u2(provides.provides_index.into());
        writer.write_u2(checked_count::<u16>(
            provides.provides_with_index.len(),
            "provides.provides_with_index",
        )?);
        for implementation in &provides.provides_with_index {
            writer.write_u2((*implementation).into());
        }
    }

    Ok(())
}

fn write_module_packages_attribute(
    writer: &mut ByteWriter,
    attribute: &ModulePackagesAttribute,
) -> Result<()> {
    writer.write_u2(checked_count::<u16>(
        attribute.package_index.len(),
        "attribute.package_index",
    )?);
    for package_index in &attribute.package_index {
        writer.write_u2((*package_index).into());
    }

    Ok(())
}

fn write_module_main_class_attribute(
    writer: &mut ByteWriter,
    attribute: &ModuleMainClassAttribute,
) -> Result<()> {
    writer.write_u2(attribute.main_class_index.into());

    Ok(())
}

fn write_record_attribute(
    writer: &mut ByteWriter,
    attribute: &RecordAttribute,
    depth: usize,
) -> Result<()> {
    writer.write_u2(checked_count::<u16>(
        attribute.components.len(),
        "attribute.components",
    )?);
    for component in &attribute.components {
        write_record_component_info(writer, component, depth)?;
    }
    Ok(())
}

fn write_record_component_info(
    writer: &mut ByteWriter,
    component: &RecordComponentInfo,
    depth: usize,
) -> Result<()> {
    writer.write_u2(component.name_index.into());
    writer.write_u2(component.descriptor_index.into());
    write_attributes(writer, &component.attributes, depth)
}

fn write_permitted_subclasses_attribute(
    writer: &mut ByteWriter,
    attribute: &PermittedSubclassesAttribute,
) -> Result<()> {
    writer.write_u2(checked_count::<u16>(
        attribute.classes.len(),
        "attribute.classes",
    )?);
    for class_index in &attribute.classes {
        writer.write_u2((*class_index).into());
    }

    Ok(())
}

fn write_type_annotations(
    writer: &mut ByteWriter,
    annotations: &[TypeAnnotationInfo],
    depth: usize,
) -> Result<()> {
    writer.write_u2(checked_count::<u16>(annotations.len(), "annotations")?);
    for annotation in annotations {
        write_type_annotation_info(writer, annotation, depth)?;
    }
    Ok(())
}

fn write_type_annotation_info(
    writer: &mut ByteWriter,
    annotation: &TypeAnnotationInfo,
    depth: usize,
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
    write_target_info(writer, &annotation.target_info)?;
    write_type_path_info(writer, &annotation.target_path)?;
    writer.write_u2(annotation.type_index.into());
    writer.write_u2(checked_count::<u16>(
        annotation.element_value_pairs.len(),
        "annotation.element_value_pairs",
    )?);
    for pair in &annotation.element_value_pairs {
        writer.write_u2(pair.element_name_index.into());
        write_element_value_info(writer, &pair.element_value, depth + 1)?;
    }
    Ok(())
}

fn write_target_info(writer: &mut ByteWriter, target_info: &TargetInfo) -> Result<()> {
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
            writer.write_u2(checked_count::<u16>(table.len(), "table")?);
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

    Ok(())
}

fn write_type_path_info(writer: &mut ByteWriter, type_path: &TypePathInfo) -> Result<()> {
    writer.write_u1(checked_count::<u8>(type_path.path.len(), "type_path.path")?);
    for entry in &type_path.path {
        writer.write_u1(entry.type_path_kind as u8);
        writer.write_u1(entry.type_argument_index);
    }

    Ok(())
}

fn write_annotations(
    writer: &mut ByteWriter,
    annotations: &[AnnotationInfo],
    depth: usize,
) -> Result<()> {
    writer.write_u2(checked_count::<u16>(annotations.len(), "annotations")?);
    for annotation in annotations {
        write_annotation_info(writer, annotation, depth)?;
    }
    Ok(())
}

fn write_parameter_annotations(
    writer: &mut ByteWriter,
    annotations: &[ParameterAnnotationInfo],
    depth: usize,
) -> Result<()> {
    writer.write_u1(checked_count::<u8>(annotations.len(), "annotations")?);
    for annotation in annotations {
        writer.write_u2(checked_count::<u16>(
            annotation.annotations.len(),
            "annotation.annotations",
        )?);
        for nested in &annotation.annotations {
            write_annotation_info(writer, nested, depth)?;
        }
    }
    Ok(())
}

fn write_annotation_info(
    writer: &mut ByteWriter,
    annotation: &AnnotationInfo,
    depth: usize,
) -> Result<()> {
    writer.write_u2(annotation.type_index.into());
    writer.write_u2(checked_count::<u16>(
        annotation.element_value_pairs.len(),
        "annotation.element_value_pairs",
    )?);
    for pair in &annotation.element_value_pairs {
        writer.write_u2(pair.element_name_index.into());
        write_element_value_info(writer, &pair.element_value, depth + 1)?;
    }
    Ok(())
}

fn write_element_value_info(
    writer: &mut ByteWriter,
    value: &ElementValueInfo,
    depth: usize,
) -> Result<()> {
    if depth >= 128 {
        return Err(invalid_writer_state(
            "attribute/annotation nesting exceeds limit of 128",
        ));
    }

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
                writer.write_u2((*const_value_index).into());
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
            writer.write_u2((*type_name_index).into());
            writer.write_u2((*const_name_index).into());
        }
        ElementValueInfo::Class { class_info_index } => {
            writer.write_u1(ElementValueTag::Class as u8);
            writer.write_u2((*class_info_index).into());
        }
        ElementValueInfo::Annotation(annotation) => {
            writer.write_u1(ElementValueTag::Annotation as u8);
            write_annotation_info(writer, annotation, depth)?;
        }
        ElementValueInfo::Array { values } => {
            writer.write_u1(ElementValueTag::Array as u8);
            writer.write_u2(checked_count::<u16>(values.len(), "values")?);
            for nested in values {
                write_element_value_info(writer, nested, depth + 1)?;
            }
        }
    }
    Ok(())
}

fn write_unknown_attribute(writer: &mut ByteWriter, attribute: &UnknownAttribute) -> Result<()> {
    writer.write_bytes(&attribute.info);

    Ok(())
}
