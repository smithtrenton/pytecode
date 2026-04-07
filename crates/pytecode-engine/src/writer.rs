use crate::bytes::ByteWriter;
use crate::constants::MAGIC;
use crate::error::{EngineError, EngineErrorKind, Result};
use crate::raw::attributes::{
    AttributeInfo, CodeAttribute, ConstantValueAttribute, ExceptionsAttribute, SignatureAttribute,
    SourceDebugExtensionAttribute, SourceFileAttribute, UnknownAttribute,
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
        AttributeInfo::Exceptions(attr) => write_exceptions_attribute(&mut payload, attr),
        AttributeInfo::Code(attr) => write_code_attribute(&mut payload, attr)?,
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

fn write_exceptions_attribute(writer: &mut ByteWriter, attribute: &ExceptionsAttribute) {
    writer.write_u2(attribute.exception_index_table.len() as u16);
    for index in &attribute.exception_index_table {
        writer.write_u2(*index);
    }
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

fn write_unknown_attribute(writer: &mut ByteWriter, attribute: &UnknownAttribute) {
    writer.write_bytes(&attribute.info);
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
