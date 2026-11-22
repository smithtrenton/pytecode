mod attributes;

use crate::bytes::ByteWriter;
use crate::constants::{MAGIC, validate_class_version};
use crate::error::{EngineError, EngineErrorKind, Result};
use crate::raw::constant_pool::ConstantPoolEntry;
use crate::raw::info::{ClassFile, FieldInfo, MethodInfo};
use crate::raw::instructions::{
    Branch, ConstantPoolIndexWide, Instruction, InvokeDynamicInsn, InvokeInterfaceInsn,
    LookupSwitchInsn, MatchOffsetPair, NewArrayInsn, OperandKind, TableSwitchInsn, WideInstruction,
    operand_kind, validate_wide_opcode,
};
use attributes::write_attributes;

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
    validate_class_version(classfile.major_version, classfile.minor_version)?;
    writer.write_u2(classfile.minor_version);
    writer.write_u2(classfile.major_version);

    let cp_len = u16::try_from(classfile.constant_pool.len()).map_err(|_| {
        EngineError::new(
            0,
            EngineErrorKind::InvalidWriterState {
                reason: format!(
                    "constant pool count {} exceeds u16::MAX",
                    classfile.constant_pool.len()
                ),
            },
        )
    })?;
    writer.write_u2(cp_len);
    for entry in iter_constant_pool_entries(&classfile.constant_pool)? {
        write_constant_pool_entry(&mut writer, entry)?;
    }

    writer.write_u2(classfile.access_flags.bits());
    writer.write_u2(classfile.this_class.into());
    writer.write_u2(classfile.super_class.into());
    let interfaces_len = u16::try_from(classfile.interfaces.len()).map_err(|_| {
        EngineError::new(
            0,
            EngineErrorKind::InvalidWriterState {
                reason: format!(
                    "interfaces count {} exceeds u16::MAX",
                    classfile.interfaces.len()
                ),
            },
        )
    })?;
    writer.write_u2(interfaces_len);
    for interface in &classfile.interfaces {
        writer.write_u2((*interface).into());
    }

    let fields_len = u16::try_from(classfile.fields.len()).map_err(|_| {
        EngineError::new(
            0,
            EngineErrorKind::InvalidWriterState {
                reason: format!("fields count {} exceeds u16::MAX", classfile.fields.len()),
            },
        )
    })?;
    writer.write_u2(fields_len);
    for field in &classfile.fields {
        write_field_info(&mut writer, field, 0)?;
    }

    let methods_len = u16::try_from(classfile.methods.len()).map_err(|_| {
        EngineError::new(
            0,
            EngineErrorKind::InvalidWriterState {
                reason: format!("methods count {} exceeds u16::MAX", classfile.methods.len()),
            },
        )
    })?;
    writer.write_u2(methods_len);
    for method in &classfile.methods {
        write_method_info(&mut writer, method, 0)?;
    }

    write_attributes(&mut writer, &classfile.attributes, 0)?;
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

fn write_constant_pool_entry(writer: &mut ByteWriter, entry: &ConstantPoolEntry) -> Result<()> {
    writer.write_u1(entry.tag() as u8);
    match entry {
        ConstantPoolEntry::Utf8(info) => {
            writer.write_u2(checked_count::<u16>(info.bytes.len(), "info.bytes")?);
            writer.write_bytes(&info.bytes);
        }
        ConstantPoolEntry::Integer(info) => writer.write_u4(info.value_bits),
        ConstantPoolEntry::Float(info) => writer.write_u4(info.value_bits),
        ConstantPoolEntry::Long(info) => {
            writer.write_u4(info.high_bits);
            writer.write_u4(info.low_bits);
        }
        ConstantPoolEntry::Double(info) => {
            writer.write_u4(info.high_bits);
            writer.write_u4(info.low_bits);
        }
        ConstantPoolEntry::Class(info) => writer.write_u2(info.name_index.into()),
        ConstantPoolEntry::String(info) => writer.write_u2(info.string_index.into()),
        ConstantPoolEntry::FieldRef(info) => {
            writer.write_u2(info.class_index.into());
            writer.write_u2(info.name_and_type_index.into());
        }
        ConstantPoolEntry::MethodRef(info) => {
            writer.write_u2(info.class_index.into());
            writer.write_u2(info.name_and_type_index.into());
        }
        ConstantPoolEntry::InterfaceMethodRef(info) => {
            writer.write_u2(info.class_index.into());
            writer.write_u2(info.name_and_type_index.into());
        }
        ConstantPoolEntry::NameAndType(info) => {
            writer.write_u2(info.name_index.into());
            writer.write_u2(info.descriptor_index.into());
        }
        ConstantPoolEntry::MethodHandle(info) => {
            if !(1..=9).contains(&info.reference_kind) {
                return Err(invalid_writer_state(format!(
                    "MethodHandle reference_kind {} must be in range 1..=9",
                    info.reference_kind
                )));
            }
            writer.write_u1(info.reference_kind);
            writer.write_u2(info.reference_index.into());
        }
        ConstantPoolEntry::MethodType(info) => writer.write_u2(info.descriptor_index.into()),
        ConstantPoolEntry::Dynamic(info) => {
            writer.write_u2(info.bootstrap_method_attr_index.into());
            writer.write_u2(info.name_and_type_index.into());
        }
        ConstantPoolEntry::InvokeDynamic(info) => {
            writer.write_u2(info.bootstrap_method_attr_index.into());
            writer.write_u2(info.name_and_type_index.into());
        }
        ConstantPoolEntry::Module(info) => writer.write_u2(info.name_index.into()),
        ConstantPoolEntry::Package(info) => writer.write_u2(info.name_index.into()),
    }

    Ok(())
}

fn write_field_info(writer: &mut ByteWriter, field: &FieldInfo, depth: usize) -> Result<()> {
    writer.write_u2(field.access_flags.bits());
    writer.write_u2(field.name_index.into());
    writer.write_u2(field.descriptor_index.into());
    write_attributes(writer, &field.attributes, depth)
}

fn write_method_info(writer: &mut ByteWriter, method: &MethodInfo, depth: usize) -> Result<()> {
    writer.write_u2(method.access_flags.bits());
    writer.write_u2(method.name_index.into());
    writer.write_u2(method.descriptor_index.into());
    write_attributes(writer, &method.attributes, depth)
}

fn invalid_writer_state(reason: impl Into<String>) -> EngineError {
    EngineError::new(
        0,
        EngineErrorKind::InvalidWriterState {
            reason: reason.into(),
        },
    )
}

pub(crate) fn write_instruction(
    writer: &mut ByteWriter,
    instruction: &Instruction,
    pc: u32,
) -> Result<()> {
    let kind = operand_kind(instruction.opcode())?;
    match (kind, instruction) {
        (OperandKind::Simple, Instruction::Simple { opcode, .. }) => writer.write_u1(*opcode),
        (OperandKind::LocalIndex, Instruction::LocalIndex { opcode, index, .. }) => {
            writer.write_u1(*opcode);
            writer.write_u1(*index);
        }
        (
            OperandKind::ConstantPoolIndex1,
            Instruction::ConstantPoolIndex1 { opcode, index, .. },
        ) => {
            writer.write_u1(*opcode);
            writer.write_u1(*index);
        }
        (
            OperandKind::ConstantPoolIndexWide,
            Instruction::ConstantPoolIndexWide(ConstantPoolIndexWide { opcode, index, .. }),
        ) => {
            writer.write_u1(*opcode);
            writer.write_u2((*index).into());
        }
        (OperandKind::Byte, Instruction::Byte { opcode, value, .. }) => {
            writer.write_u1(*opcode);
            writer.write_i1(*value);
        }
        (OperandKind::Short, Instruction::Short { opcode, value, .. }) => {
            writer.write_u1(*opcode);
            writer.write_i2(*value);
        }
        (
            OperandKind::Branch,
            Instruction::Branch(Branch {
                opcode,
                branch_offset,
                ..
            }),
        ) => {
            writer.write_u1(*opcode);
            writer.write_i2(*branch_offset);
        }
        (
            OperandKind::BranchWide,
            Instruction::BranchWide {
                opcode,
                branch_offset,
                ..
            },
        ) => {
            writer.write_u1(*opcode);
            writer.write_i4(*branch_offset);
        }
        (OperandKind::IInc, Instruction::IInc { index, value, .. }) => {
            writer.write_u1(0x84);
            writer.write_u1(*index);
            writer.write_i1(*value);
        }
        (
            OperandKind::InvokeDynamic,
            Instruction::InvokeDynamic(InvokeDynamicInsn {
                index, reserved, ..
            }),
        ) => {
            if *reserved != 0 {
                return Err(invalid_writer_state(format!(
                    "invokedynamic reserved bytes must be zero, got {reserved}"
                )));
            }
            writer.write_u1(0xBA);
            writer.write_u2((*index).into());
            writer.write_u2(*reserved);
        }
        (
            OperandKind::InvokeInterface,
            Instruction::InvokeInterface(InvokeInterfaceInsn {
                index,
                count,
                reserved,
                ..
            }),
        ) => {
            if *count == 0 {
                return Err(invalid_writer_state(
                    "invokeinterface count byte must be non-zero",
                ));
            }
            if *reserved != 0 {
                return Err(invalid_writer_state(format!(
                    "invokeinterface reserved byte must be zero, got {reserved}"
                )));
            }
            writer.write_u1(0xB9);
            writer.write_u2((*index).into());
            writer.write_u1(*count);
            writer.write_u1(*reserved);
        }
        (OperandKind::NewArray, Instruction::NewArray(NewArrayInsn { atype, .. })) => {
            writer.write_u1(0xBC);
            writer.write_u1(*atype as u8);
        }
        (
            OperandKind::MultiANewArray,
            Instruction::MultiANewArray {
                index, dimensions, ..
            },
        ) => {
            if *dimensions == 0 {
                return Err(invalid_writer_state(
                    "multianewarray dimensions must be at least 1",
                ));
            }
            writer.write_u1(0xC5);
            writer.write_u2((*index).into());
            writer.write_u1(*dimensions);
        }
        (
            OperandKind::LookupSwitch,
            Instruction::LookupSwitch(LookupSwitchInsn {
                default_offset,
                pairs,
                ..
            }),
        ) => {
            if pairs
                .windows(2)
                .any(|pair| pair[0].match_value >= pair[1].match_value)
            {
                return Err(invalid_writer_state(
                    "lookupswitch keys must be strictly increasing",
                ));
            }
            writer.write_u1(0xAB);
            write_switch_padding(writer, pc);
            writer.write_i4(*default_offset);
            writer.write_u4(checked_count::<u32>(pairs.len(), "pairs")?);
            for MatchOffsetPair {
                match_value,
                offset,
            } in pairs
            {
                writer.write_i4(*match_value);
                writer.write_i4(*offset);
            }
        }
        (
            OperandKind::TableSwitch,
            Instruction::TableSwitch(TableSwitchInsn {
                default_offset,
                low,
                high,
                offsets,
                ..
            }),
        ) => {
            if high < low || i64::from(*high) - i64::from(*low) + 1 != offsets.len() as i64 {
                return Err(invalid_writer_state(
                    "tableswitch range must match its offsets",
                ));
            }
            writer.write_u1(0xAA);
            write_switch_padding(writer, pc);
            writer.write_i4(*default_offset);
            writer.write_i4(*low);
            writer.write_i4(*high);
            for branch in offsets {
                writer.write_i4(*branch);
            }
        }
        (
            OperandKind::Wide,
            Instruction::Wide(WideInstruction {
                opcode,
                index,
                value,
                ..
            }),
        ) => {
            validate_wide_opcode(*opcode, 0)?;
            if (*opcode == 0x84) != value.is_some() {
                return Err(invalid_writer_state(format!(
                    "wide opcode 0x{opcode:02x} value presence mismatch"
                )));
            }
            writer.write_u1(0xC4);
            writer.write_u1(*opcode);
            writer.write_u2(*index);
            if let Some(value) = value {
                writer.write_i2(*value);
            }
        }
        _ => {
            return Err(invalid_writer_state(format!(
                "instruction variant does not match opcode 0x{:02x}",
                instruction.opcode()
            )));
        }
    }

    Ok(())
}

fn write_switch_padding(writer: &mut ByteWriter, offset: u32) {
    let padding = 3 - (offset & 3);
    for _ in 0..padding {
        writer.write_u1(0);
    }
}

fn checked_count<T: TryFrom<usize>>(length: usize, field: &str) -> Result<T> {
    T::try_from(length).map_err(|_| {
        invalid_writer_state(format!(
            "{field} length {length} exceeds {} capacity",
            std::any::type_name::<T>()
        ))
    })
}
