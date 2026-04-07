use crate::bytes::ByteReader;
use crate::constants::{
    ClassAccessFlags, FieldAccessFlags, MAGIC, MethodAccessFlags, validate_class_version,
};
use crate::error::{EngineError, EngineErrorKind, Result};
use crate::modified_utf8::decode_modified_utf8;
use crate::raw::attributes::{
    AttributeInfo, CodeAttribute, ConstantValueAttribute, ExceptionHandler, ExceptionsAttribute,
    SignatureAttribute, SourceDebugExtensionAttribute, SourceFileAttribute, UnknownAttribute,
};
use crate::raw::constant_pool::{
    ClassInfo, ConstantPoolEntry, ConstantPoolTag, DoubleInfo, DynamicInfo, FieldRefInfo,
    FloatInfo, IntegerInfo, InterfaceMethodRefInfo, InvokeDynamicInfo, LongInfo, MethodHandleInfo,
    MethodRefInfo, MethodTypeInfo, ModuleInfo, NameAndTypeInfo, PackageInfo, StringInfo, Utf8Info,
};
use crate::raw::info::{ClassFile, FieldInfo, MethodInfo};
use crate::raw::instructions::{
    ArrayType, Branch, ConstantPoolIndexWide, Instruction, InvokeDynamicInsn, InvokeInterfaceInsn,
    LookupSwitchInsn, MatchOffsetPair, NewArrayInsn, TableSwitchInsn, WideInstruction,
    operand_kind, validate_wide_opcode,
};

pub struct ClassReader<'a> {
    reader: ByteReader<'a>,
    constant_pool: Vec<Option<ConstantPoolEntry>>,
}

impl<'a> ClassReader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self {
            reader: ByteReader::new(bytes),
            constant_pool: Vec::new(),
        }
    }

    pub fn read_class(mut self) -> Result<ClassFile> {
        let magic = self.reader.read_u4()?;
        if magic != MAGIC {
            return Err(EngineError::new(
                0,
                EngineErrorKind::InvalidMagic {
                    found: magic,
                    expected: MAGIC,
                },
            ));
        }

        let minor_version = self.reader.read_u2()?;
        let major_version = self.reader.read_u2()?;
        validate_class_version(major_version, minor_version)?;

        let constant_pool_count = self.reader.read_u2()? as usize;
        if constant_pool_count == 0 {
            return Err(EngineError::new(
                self.reader.offset(),
                EngineErrorKind::InvalidWriterState {
                    reason: "constant_pool_count must be at least 1".to_owned(),
                },
            ));
        }
        self.constant_pool = vec![None; constant_pool_count];

        let mut index = 1_usize;
        while index < constant_pool_count {
            let entry = self.read_constant_pool_entry()?;
            let is_wide = entry.is_wide();
            self.constant_pool[index] = Some(entry);
            index += if is_wide { 2 } else { 1 };
        }

        let access_flags = ClassAccessFlags::from_bits_retain(self.reader.read_u2()?);
        let this_class = self.reader.read_u2()?;
        let super_class = self.reader.read_u2()?;

        let interfaces_count = self.reader.read_u2()? as usize;
        let interfaces = (0..interfaces_count)
            .map(|_| self.reader.read_u2())
            .collect::<Result<Vec<_>>>()?;

        let fields_count = self.reader.read_u2()? as usize;
        let fields = (0..fields_count)
            .map(|_| self.read_field())
            .collect::<Result<Vec<_>>>()?;

        let methods_count = self.reader.read_u2()? as usize;
        let methods = (0..methods_count)
            .map(|_| self.read_method())
            .collect::<Result<Vec<_>>>()?;

        let attributes_count = self.reader.read_u2()? as usize;
        let attributes = (0..attributes_count)
            .map(|_| self.read_attribute())
            .collect::<Result<Vec<_>>>()?;

        Ok(ClassFile {
            magic,
            minor_version,
            major_version,
            constant_pool: self.constant_pool,
            access_flags,
            this_class,
            super_class,
            interfaces,
            fields,
            methods,
            attributes,
        })
    }

    fn read_field(&mut self) -> Result<FieldInfo> {
        let access_flags = FieldAccessFlags::from_bits_retain(self.reader.read_u2()?);
        let name_index = self.reader.read_u2()?;
        let descriptor_index = self.reader.read_u2()?;
        let attributes_count = self.reader.read_u2()? as usize;
        let attributes = (0..attributes_count)
            .map(|_| self.read_attribute())
            .collect::<Result<Vec<_>>>()?;
        Ok(FieldInfo {
            access_flags,
            name_index,
            descriptor_index,
            attributes,
        })
    }

    fn read_method(&mut self) -> Result<MethodInfo> {
        let access_flags = MethodAccessFlags::from_bits_retain(self.reader.read_u2()?);
        let name_index = self.reader.read_u2()?;
        let descriptor_index = self.reader.read_u2()?;
        let attributes_count = self.reader.read_u2()? as usize;
        let attributes = (0..attributes_count)
            .map(|_| self.read_attribute())
            .collect::<Result<Vec<_>>>()?;
        Ok(MethodInfo {
            access_flags,
            name_index,
            descriptor_index,
            attributes,
        })
    }

    fn read_attribute(&mut self) -> Result<AttributeInfo> {
        let name_index = self.reader.read_u2()?;
        let attribute_length = self.reader.read_u4()?;
        let payload = self.reader.read_bytes(attribute_length as usize)?;
        let name = self.constant_pool_utf8(name_index)?;
        let mut payload_reader = ByteReader::new(payload);

        let attribute = match name.as_str() {
            "ConstantValue" => AttributeInfo::ConstantValue(ConstantValueAttribute {
                attribute_name_index: name_index,
                attribute_length,
                constantvalue_index: payload_reader.read_u2()?,
            }),
            "Signature" => AttributeInfo::Signature(SignatureAttribute {
                attribute_name_index: name_index,
                attribute_length,
                signature_index: payload_reader.read_u2()?,
            }),
            "SourceFile" => AttributeInfo::SourceFile(SourceFileAttribute {
                attribute_name_index: name_index,
                attribute_length,
                sourcefile_index: payload_reader.read_u2()?,
            }),
            "SourceDebugExtension" => {
                AttributeInfo::SourceDebugExtension(SourceDebugExtensionAttribute {
                    attribute_name_index: name_index,
                    attribute_length,
                    debug_extension: payload.to_vec(),
                })
            }
            "Exceptions" => {
                let exception_count = payload_reader.read_u2()? as usize;
                let exception_index_table = (0..exception_count)
                    .map(|_| payload_reader.read_u2())
                    .collect::<Result<Vec<_>>>()?;
                AttributeInfo::Exceptions(ExceptionsAttribute {
                    attribute_name_index: name_index,
                    attribute_length,
                    exception_index_table,
                })
            }
            "Code" => {
                let max_stack = payload_reader.read_u2()?;
                let max_locals = payload_reader.read_u2()?;
                let code_length = payload_reader.read_u4()?;
                let code_bytes = payload_reader.read_bytes(code_length as usize)?;
                let code = read_code_bytes(code_bytes)?;
                let exception_table_length = payload_reader.read_u2()? as usize;
                let exception_table = (0..exception_table_length)
                    .map(|_| {
                        Ok(ExceptionHandler {
                            start_pc: payload_reader.read_u2()?,
                            end_pc: payload_reader.read_u2()?,
                            handler_pc: payload_reader.read_u2()?,
                            catch_type: payload_reader.read_u2()?,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                let nested_attributes_count = payload_reader.read_u2()? as usize;
                let nested_attributes = (0..nested_attributes_count)
                    .map(|_| self.read_attribute_from_payload(&mut payload_reader))
                    .collect::<Result<Vec<_>>>()?;
                AttributeInfo::Code(CodeAttribute {
                    attribute_name_index: name_index,
                    attribute_length,
                    max_stack,
                    max_locals,
                    code_length,
                    code,
                    exception_table,
                    attributes: nested_attributes,
                })
            }
            _ => AttributeInfo::Unknown(UnknownAttribute {
                attribute_name_index: name_index,
                attribute_length,
                name,
                info: payload.to_vec(),
            }),
        };

        if matches!(
            &attribute,
            AttributeInfo::SourceDebugExtension(_) | AttributeInfo::Unknown(_)
        ) {
            let _ = payload_reader.read_bytes(attribute_length as usize)?;
        }

        if payload_reader.remaining() != 0 {
            return Err(EngineError::new(
                self.reader.offset(),
                EngineErrorKind::InvalidAttribute {
                    reason: "attribute parser did not consume the full payload".to_owned(),
                },
            ));
        }
        Ok(attribute)
    }

    fn read_attribute_from_payload(
        &self,
        payload_reader: &mut ByteReader<'_>,
    ) -> Result<AttributeInfo> {
        let name_index = payload_reader.read_u2()?;
        let attribute_length = payload_reader.read_u4()?;
        let payload = payload_reader.read_bytes(attribute_length as usize)?;
        let name = self.constant_pool_utf8(name_index)?;
        let mut nested = ByteReader::new(payload);

        let attribute = match name.as_str() {
            "ConstantValue" => AttributeInfo::ConstantValue(ConstantValueAttribute {
                attribute_name_index: name_index,
                attribute_length,
                constantvalue_index: nested.read_u2()?,
            }),
            "Signature" => AttributeInfo::Signature(SignatureAttribute {
                attribute_name_index: name_index,
                attribute_length,
                signature_index: nested.read_u2()?,
            }),
            "SourceFile" => AttributeInfo::SourceFile(SourceFileAttribute {
                attribute_name_index: name_index,
                attribute_length,
                sourcefile_index: nested.read_u2()?,
            }),
            "SourceDebugExtension" => {
                AttributeInfo::SourceDebugExtension(SourceDebugExtensionAttribute {
                    attribute_name_index: name_index,
                    attribute_length,
                    debug_extension: payload.to_vec(),
                })
            }
            "Exceptions" => {
                let count = nested.read_u2()? as usize;
                let table = (0..count)
                    .map(|_| nested.read_u2())
                    .collect::<Result<Vec<_>>>()?;
                AttributeInfo::Exceptions(ExceptionsAttribute {
                    attribute_name_index: name_index,
                    attribute_length,
                    exception_index_table: table,
                })
            }
            _ => AttributeInfo::Unknown(UnknownAttribute {
                attribute_name_index: name_index,
                attribute_length,
                name,
                info: payload.to_vec(),
            }),
        };

        if matches!(
            &attribute,
            AttributeInfo::SourceDebugExtension(_) | AttributeInfo::Unknown(_)
        ) {
            let _ = nested.read_bytes(attribute_length as usize)?;
        }

        if nested.remaining() != 0 {
            return Err(EngineError::new(
                payload_reader.offset(),
                EngineErrorKind::InvalidAttribute {
                    reason: "nested attribute parser did not consume the full payload".to_owned(),
                },
            ));
        }
        Ok(attribute)
    }

    fn read_constant_pool_entry(&mut self) -> Result<ConstantPoolEntry> {
        let offset = self.reader.offset();
        let tag = ConstantPoolTag::try_from(self.reader.read_u1()?)
            .map_err(|kind| EngineError::new(offset, kind))?;
        let entry = match tag {
            ConstantPoolTag::Utf8 => {
                let length = self.reader.read_u2()? as usize;
                ConstantPoolEntry::Utf8(Utf8Info {
                    bytes: self.reader.read_bytes(length)?.to_vec(),
                })
            }
            ConstantPoolTag::Integer => ConstantPoolEntry::Integer(IntegerInfo {
                value_bytes: self.reader.read_u4()?,
            }),
            ConstantPoolTag::Float => ConstantPoolEntry::Float(FloatInfo {
                value_bytes: self.reader.read_u4()?,
            }),
            ConstantPoolTag::Long => ConstantPoolEntry::Long(LongInfo {
                high_bytes: self.reader.read_u4()?,
                low_bytes: self.reader.read_u4()?,
            }),
            ConstantPoolTag::Double => ConstantPoolEntry::Double(DoubleInfo {
                high_bytes: self.reader.read_u4()?,
                low_bytes: self.reader.read_u4()?,
            }),
            ConstantPoolTag::Class => ConstantPoolEntry::Class(ClassInfo {
                name_index: self.reader.read_u2()?,
            }),
            ConstantPoolTag::String => ConstantPoolEntry::String(StringInfo {
                string_index: self.reader.read_u2()?,
            }),
            ConstantPoolTag::FieldRef => ConstantPoolEntry::FieldRef(FieldRefInfo {
                class_index: self.reader.read_u2()?,
                name_and_type_index: self.reader.read_u2()?,
            }),
            ConstantPoolTag::MethodRef => ConstantPoolEntry::MethodRef(MethodRefInfo {
                class_index: self.reader.read_u2()?,
                name_and_type_index: self.reader.read_u2()?,
            }),
            ConstantPoolTag::InterfaceMethodRef => {
                ConstantPoolEntry::InterfaceMethodRef(InterfaceMethodRefInfo {
                    class_index: self.reader.read_u2()?,
                    name_and_type_index: self.reader.read_u2()?,
                })
            }
            ConstantPoolTag::NameAndType => ConstantPoolEntry::NameAndType(NameAndTypeInfo {
                name_index: self.reader.read_u2()?,
                descriptor_index: self.reader.read_u2()?,
            }),
            ConstantPoolTag::MethodHandle => ConstantPoolEntry::MethodHandle(MethodHandleInfo {
                reference_kind: self.reader.read_u1()?,
                reference_index: self.reader.read_u2()?,
            }),
            ConstantPoolTag::MethodType => ConstantPoolEntry::MethodType(MethodTypeInfo {
                descriptor_index: self.reader.read_u2()?,
            }),
            ConstantPoolTag::Dynamic => ConstantPoolEntry::Dynamic(DynamicInfo {
                bootstrap_method_attr_index: self.reader.read_u2()?,
                name_and_type_index: self.reader.read_u2()?,
            }),
            ConstantPoolTag::InvokeDynamic => ConstantPoolEntry::InvokeDynamic(InvokeDynamicInfo {
                bootstrap_method_attr_index: self.reader.read_u2()?,
                name_and_type_index: self.reader.read_u2()?,
            }),
            ConstantPoolTag::Module => ConstantPoolEntry::Module(ModuleInfo {
                name_index: self.reader.read_u2()?,
            }),
            ConstantPoolTag::Package => ConstantPoolEntry::Package(PackageInfo {
                name_index: self.reader.read_u2()?,
            }),
        };
        Ok(entry)
    }

    fn constant_pool_utf8(&self, index: u16) -> Result<String> {
        let Some(Some(ConstantPoolEntry::Utf8(entry))) = self.constant_pool.get(index as usize)
        else {
            return Err(EngineError::new(
                self.reader.offset(),
                EngineErrorKind::InvalidConstantPoolIndex { index },
            ));
        };
        decode_modified_utf8(&entry.bytes)
    }
}

pub fn parse_class(bytes: &[u8]) -> Result<ClassFile> {
    ClassReader::new(bytes).read_class()
}

pub fn parse_class_bytes(bytes: impl AsRef<[u8]>) -> Result<ClassFile> {
    parse_class(bytes.as_ref())
}

fn read_code_bytes(bytes: &[u8]) -> Result<Vec<Instruction>> {
    let mut reader = ByteReader::new(bytes);
    let mut instructions = Vec::new();

    while reader.remaining() > 0 {
        let offset = reader.offset() as u32;
        let opcode = reader.read_u1()?;
        let instruction = match operand_kind(opcode)? {
            crate::raw::instructions::OperandKind::Simple => Instruction::Simple { opcode, offset },
            crate::raw::instructions::OperandKind::LocalIndex => Instruction::LocalIndex {
                opcode,
                offset,
                index: reader.read_u1()?,
            },
            crate::raw::instructions::OperandKind::ConstantPoolIndex1 => {
                Instruction::ConstantPoolIndex1 {
                    opcode,
                    offset,
                    index: reader.read_u1()?,
                }
            }
            crate::raw::instructions::OperandKind::ConstantPoolIndexWide => {
                Instruction::ConstantPoolIndexWide(ConstantPoolIndexWide {
                    opcode,
                    offset,
                    index: reader.read_u2()?,
                })
            }
            crate::raw::instructions::OperandKind::Byte => Instruction::Byte {
                opcode,
                offset,
                value: reader.read_i1()?,
            },
            crate::raw::instructions::OperandKind::Short => Instruction::Short {
                opcode,
                offset,
                value: reader.read_i2()?,
            },
            crate::raw::instructions::OperandKind::Branch => Instruction::Branch(Branch {
                opcode,
                offset,
                branch_offset: reader.read_i2()?,
            }),
            crate::raw::instructions::OperandKind::BranchWide => Instruction::BranchWide {
                opcode,
                offset,
                branch_offset: reader.read_i4()?,
            },
            crate::raw::instructions::OperandKind::IInc => Instruction::IInc {
                offset,
                index: reader.read_u1()?,
                value: reader.read_i1()?,
            },
            crate::raw::instructions::OperandKind::InvokeDynamic => {
                Instruction::InvokeDynamic(InvokeDynamicInsn {
                    offset,
                    index: reader.read_u2()?,
                    reserved: reader.read_u2()?,
                })
            }
            crate::raw::instructions::OperandKind::InvokeInterface => {
                Instruction::InvokeInterface(InvokeInterfaceInsn {
                    offset,
                    index: reader.read_u2()?,
                    count: reader.read_u1()?,
                    reserved: reader.read_u1()?,
                })
            }
            crate::raw::instructions::OperandKind::NewArray => {
                let atype = ArrayType::try_from(reader.read_u1()?)
                    .map_err(|kind| EngineError::new(reader.offset(), kind))?;
                Instruction::NewArray(NewArrayInsn { offset, atype })
            }
            crate::raw::instructions::OperandKind::MultiANewArray => Instruction::MultiANewArray {
                offset,
                index: reader.read_u2()?,
                dimensions: reader.read_u1()?,
            },
            crate::raw::instructions::OperandKind::LookupSwitch => {
                let padding = (4 - ((offset + 1) % 4)) % 4;
                reader.read_bytes(padding as usize)?;
                let default_offset = reader.read_i4()?;
                let pair_count = reader.read_u4()? as usize;
                let pairs = (0..pair_count)
                    .map(|_| {
                        Ok(MatchOffsetPair {
                            match_value: reader.read_i4()?,
                            offset: reader.read_i4()?,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                Instruction::LookupSwitch(LookupSwitchInsn {
                    offset,
                    default_offset,
                    pairs,
                })
            }
            crate::raw::instructions::OperandKind::TableSwitch => {
                let padding = (4 - ((offset + 1) % 4)) % 4;
                reader.read_bytes(padding as usize)?;
                let default_offset = reader.read_i4()?;
                let low = reader.read_i4()?;
                let high = reader.read_i4()?;
                let count = high
                    .checked_sub(low)
                    .and_then(|delta| delta.checked_add(1))
                    .ok_or_else(|| {
                        EngineError::new(
                            reader.offset(),
                            EngineErrorKind::InvalidAttribute {
                                reason: "invalid tableswitch range".to_owned(),
                            },
                        )
                    })? as usize;
                let offsets = (0..count)
                    .map(|_| reader.read_i4())
                    .collect::<Result<Vec<_>>>()?;
                Instruction::TableSwitch(TableSwitchInsn {
                    offset,
                    default_offset,
                    low,
                    high,
                    offsets,
                })
            }
            crate::raw::instructions::OperandKind::Wide => {
                let wide_opcode = reader.read_u1()?;
                validate_wide_opcode(wide_opcode, reader.offset())?;
                let index = reader.read_u2()?;
                let value = if wide_opcode == 0x84 {
                    Some(reader.read_i2()?)
                } else {
                    None
                };
                Instruction::Wide(WideInstruction {
                    offset,
                    opcode: wide_opcode,
                    index,
                    value,
                })
            }
        };
        instructions.push(instruction);
    }

    Ok(instructions)
}
