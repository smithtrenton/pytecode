use crate::bytes::ByteReader;
use crate::constants::{
    ClassAccessFlags, FieldAccessFlags, MAGIC, MethodAccessFlags, MethodParameterAccessFlag,
    ModuleAccessFlag, ModuleExportsAccessFlag, ModuleOpensAccessFlag, ModuleRequiresAccessFlag,
    NestedClassAccessFlag, TargetInfoType, TargetType, TypePathKind, VerificationType,
    validate_class_version,
};
use crate::error::{EngineError, EngineErrorKind, Result};
use crate::modified_utf8::decode_modified_utf8;
use crate::raw::attributes::{
    AnnotationDefaultAttribute, AnnotationInfo, AttributeInfo, BootstrapMethodInfo,
    BootstrapMethodsAttribute, CodeAttribute, ConstantValueAttribute, DeprecatedAttribute,
    ElementValueInfo, ElementValuePairInfo, ElementValueTag, EnclosingMethodAttribute,
    ExceptionHandler, ExceptionsAttribute, ExportInfo, InnerClassInfo, InnerClassesAttribute,
    LineNumberInfo, LineNumberTableAttribute, LocalVariableInfo, LocalVariableTableAttribute,
    LocalVariableTypeInfo, LocalVariableTypeTableAttribute, MethodParameterInfo,
    MethodParametersAttribute, ModuleAttribute, ModuleInfo as ModuleAttributeInfo,
    ModuleMainClassAttribute, ModulePackagesAttribute, NestHostAttribute, NestMembersAttribute,
    OpensInfo, ParameterAnnotationInfo, PathInfo, PermittedSubclassesAttribute, ProvidesInfo,
    RecordAttribute, RecordComponentInfo, RequiresInfo, RuntimeInvisibleAnnotationsAttribute,
    RuntimeInvisibleParameterAnnotationsAttribute, RuntimeInvisibleTypeAnnotationsAttribute,
    RuntimeVisibleAnnotationsAttribute, RuntimeVisibleParameterAnnotationsAttribute,
    RuntimeVisibleTypeAnnotationsAttribute, SignatureAttribute, SourceDebugExtensionAttribute,
    SourceFileAttribute, StackMapFrameInfo, StackMapTableAttribute, SyntheticAttribute, TableInfo,
    TargetInfo, TypeAnnotationInfo, TypePathInfo, UnknownAttribute, VerificationTypeInfo,
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
        self.parse_attribute_payload(
            name_index,
            attribute_length,
            name.as_str(),
            payload,
            self.reader.offset(),
            "attribute parser did not consume the full payload",
        )
    }

    fn read_attribute_from_payload(
        &self,
        payload_reader: &mut ByteReader<'_>,
    ) -> Result<AttributeInfo> {
        let name_index = payload_reader.read_u2()?;
        let attribute_length = payload_reader.read_u4()?;
        let payload = payload_reader.read_bytes(attribute_length as usize)?;
        let name = self.constant_pool_utf8(name_index)?;
        self.parse_attribute_payload(
            name_index,
            attribute_length,
            name.as_str(),
            payload,
            payload_reader.offset(),
            "nested attribute parser did not consume the full payload",
        )
    }

    fn parse_attribute_payload(
        &self,
        name_index: u16,
        attribute_length: u32,
        name: &str,
        payload: &[u8],
        error_offset: usize,
        consume_error_reason: &str,
    ) -> Result<AttributeInfo> {
        let mut payload_reader = ByteReader::new(payload);
        let attribute = match name {
            "Synthetic" => AttributeInfo::Synthetic(SyntheticAttribute {
                attribute_name_index: name_index,
                attribute_length,
            }),
            "Deprecated" => AttributeInfo::Deprecated(DeprecatedAttribute {
                attribute_name_index: name_index,
                attribute_length,
            }),
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
                    debug_extension: payload_reader.read_bytes(payload.len())?.to_vec(),
                })
            }
            "LineNumberTable" => {
                let count = payload_reader.read_u2()? as usize;
                let line_number_table = (0..count)
                    .map(|_| {
                        Ok(LineNumberInfo {
                            start_pc: payload_reader.read_u2()?,
                            line_number: payload_reader.read_u2()?,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                AttributeInfo::LineNumberTable(LineNumberTableAttribute {
                    attribute_name_index: name_index,
                    attribute_length,
                    line_number_table,
                })
            }
            "LocalVariableTable" => {
                let count = payload_reader.read_u2()? as usize;
                let local_variable_table = (0..count)
                    .map(|_| {
                        Ok(LocalVariableInfo {
                            start_pc: payload_reader.read_u2()?,
                            length: payload_reader.read_u2()?,
                            name_index: payload_reader.read_u2()?,
                            descriptor_index: payload_reader.read_u2()?,
                            index: payload_reader.read_u2()?,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                AttributeInfo::LocalVariableTable(LocalVariableTableAttribute {
                    attribute_name_index: name_index,
                    attribute_length,
                    local_variable_table,
                })
            }
            "LocalVariableTypeTable" => {
                let count = payload_reader.read_u2()? as usize;
                let local_variable_type_table = (0..count)
                    .map(|_| {
                        Ok(LocalVariableTypeInfo {
                            start_pc: payload_reader.read_u2()?,
                            length: payload_reader.read_u2()?,
                            name_index: payload_reader.read_u2()?,
                            signature_index: payload_reader.read_u2()?,
                            index: payload_reader.read_u2()?,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                AttributeInfo::LocalVariableTypeTable(LocalVariableTypeTableAttribute {
                    attribute_name_index: name_index,
                    attribute_length,
                    local_variable_type_table,
                })
            }
            "StackMapTable" => {
                let count = payload_reader.read_u2()? as usize;
                let entries = (0..count)
                    .map(|_| self.read_stack_map_frame(&mut payload_reader, error_offset))
                    .collect::<Result<Vec<_>>>()?;
                AttributeInfo::StackMapTable(StackMapTableAttribute {
                    attribute_name_index: name_index,
                    attribute_length,
                    entries,
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
            "InnerClasses" => {
                let count = payload_reader.read_u2()? as usize;
                let classes = (0..count)
                    .map(|_| {
                        Ok(InnerClassInfo {
                            inner_class_info_index: payload_reader.read_u2()?,
                            outer_class_info_index: payload_reader.read_u2()?,
                            inner_name_index: payload_reader.read_u2()?,
                            inner_class_access_flags: NestedClassAccessFlag::from_bits_retain(
                                payload_reader.read_u2()?,
                            ),
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                AttributeInfo::InnerClasses(InnerClassesAttribute {
                    attribute_name_index: name_index,
                    attribute_length,
                    classes,
                })
            }
            "EnclosingMethod" => AttributeInfo::EnclosingMethod(EnclosingMethodAttribute {
                attribute_name_index: name_index,
                attribute_length,
                class_index: payload_reader.read_u2()?,
                method_index: payload_reader.read_u2()?,
            }),
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
            "MethodParameters" => {
                let count = payload_reader.read_u1()? as usize;
                let parameters = (0..count)
                    .map(|_| {
                        Ok(MethodParameterInfo {
                            name_index: payload_reader.read_u2()?,
                            access_flags: MethodParameterAccessFlag::from_bits_retain(
                                payload_reader.read_u2()?,
                            ),
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                AttributeInfo::MethodParameters(MethodParametersAttribute {
                    attribute_name_index: name_index,
                    attribute_length,
                    parameters,
                })
            }
            "NestHost" => AttributeInfo::NestHost(NestHostAttribute {
                attribute_name_index: name_index,
                attribute_length,
                host_class_index: payload_reader.read_u2()?,
            }),
            "NestMembers" => {
                let count = payload_reader.read_u2()? as usize;
                let classes = (0..count)
                    .map(|_| payload_reader.read_u2())
                    .collect::<Result<Vec<_>>>()?;
                AttributeInfo::NestMembers(NestMembersAttribute {
                    attribute_name_index: name_index,
                    attribute_length,
                    classes,
                })
            }
            "RuntimeVisibleAnnotations" => {
                let count = payload_reader.read_u2()? as usize;
                let annotations = (0..count)
                    .map(|_| self.read_annotation_info(&mut payload_reader, error_offset))
                    .collect::<Result<Vec<_>>>()?;
                AttributeInfo::RuntimeVisibleAnnotations(RuntimeVisibleAnnotationsAttribute {
                    attribute_name_index: name_index,
                    attribute_length,
                    annotations,
                })
            }
            "RuntimeInvisibleAnnotations" => {
                let count = payload_reader.read_u2()? as usize;
                let annotations = (0..count)
                    .map(|_| self.read_annotation_info(&mut payload_reader, error_offset))
                    .collect::<Result<Vec<_>>>()?;
                AttributeInfo::RuntimeInvisibleAnnotations(RuntimeInvisibleAnnotationsAttribute {
                    attribute_name_index: name_index,
                    attribute_length,
                    annotations,
                })
            }
            "RuntimeVisibleParameterAnnotations" => {
                let count = payload_reader.read_u1()? as usize;
                let parameter_annotations = (0..count)
                    .map(|_| self.read_parameter_annotation_info(&mut payload_reader, error_offset))
                    .collect::<Result<Vec<_>>>()?;
                AttributeInfo::RuntimeVisibleParameterAnnotations(
                    RuntimeVisibleParameterAnnotationsAttribute {
                        attribute_name_index: name_index,
                        attribute_length,
                        parameter_annotations,
                    },
                )
            }
            "RuntimeInvisibleParameterAnnotations" => {
                let count = payload_reader.read_u1()? as usize;
                let parameter_annotations = (0..count)
                    .map(|_| self.read_parameter_annotation_info(&mut payload_reader, error_offset))
                    .collect::<Result<Vec<_>>>()?;
                AttributeInfo::RuntimeInvisibleParameterAnnotations(
                    RuntimeInvisibleParameterAnnotationsAttribute {
                        attribute_name_index: name_index,
                        attribute_length,
                        parameter_annotations,
                    },
                )
            }
            "RuntimeVisibleTypeAnnotations" => {
                let count = payload_reader.read_u2()? as usize;
                let annotations = (0..count)
                    .map(|_| self.read_type_annotation_info(&mut payload_reader, error_offset))
                    .collect::<Result<Vec<_>>>()?;
                AttributeInfo::RuntimeVisibleTypeAnnotations(
                    RuntimeVisibleTypeAnnotationsAttribute {
                        attribute_name_index: name_index,
                        attribute_length,
                        annotations,
                    },
                )
            }
            "RuntimeInvisibleTypeAnnotations" => {
                let count = payload_reader.read_u2()? as usize;
                let annotations = (0..count)
                    .map(|_| self.read_type_annotation_info(&mut payload_reader, error_offset))
                    .collect::<Result<Vec<_>>>()?;
                AttributeInfo::RuntimeInvisibleTypeAnnotations(
                    RuntimeInvisibleTypeAnnotationsAttribute {
                        attribute_name_index: name_index,
                        attribute_length,
                        annotations,
                    },
                )
            }
            "AnnotationDefault" => AttributeInfo::AnnotationDefault(AnnotationDefaultAttribute {
                attribute_name_index: name_index,
                attribute_length,
                default_value: self.read_element_value_info(&mut payload_reader, error_offset)?,
            }),
            "BootstrapMethods" => {
                let count = payload_reader.read_u2()? as usize;
                let bootstrap_methods = (0..count)
                    .map(|_| {
                        let bootstrap_method_ref = payload_reader.read_u2()?;
                        let argument_count = payload_reader.read_u2()? as usize;
                        let bootstrap_arguments = (0..argument_count)
                            .map(|_| payload_reader.read_u2())
                            .collect::<Result<Vec<_>>>()?;
                        Ok(BootstrapMethodInfo {
                            bootstrap_method_ref,
                            bootstrap_arguments,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                AttributeInfo::BootstrapMethods(BootstrapMethodsAttribute {
                    attribute_name_index: name_index,
                    attribute_length,
                    bootstrap_methods,
                })
            }
            "Module" => {
                let module_name_index = payload_reader.read_u2()?;
                let module_flags = ModuleAccessFlag::from_bits_retain(payload_reader.read_u2()?);
                let module_version_index = payload_reader.read_u2()?;

                let requires_count = payload_reader.read_u2()? as usize;
                let requires = (0..requires_count)
                    .map(|_| {
                        Ok(RequiresInfo {
                            requires_index: payload_reader.read_u2()?,
                            requires_flags: ModuleRequiresAccessFlag::from_bits_retain(
                                payload_reader.read_u2()?,
                            ),
                            requires_version_index: payload_reader.read_u2()?,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;

                let exports_count = payload_reader.read_u2()? as usize;
                let exports = (0..exports_count)
                    .map(|_| {
                        let exports_index = payload_reader.read_u2()?;
                        let exports_flags =
                            ModuleExportsAccessFlag::from_bits_retain(payload_reader.read_u2()?);
                        let exports_to_count = payload_reader.read_u2()? as usize;
                        let exports_to_index = (0..exports_to_count)
                            .map(|_| payload_reader.read_u2())
                            .collect::<Result<Vec<_>>>()?;
                        Ok(ExportInfo {
                            exports_index,
                            exports_flags,
                            exports_to_index,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;

                let opens_count = payload_reader.read_u2()? as usize;
                let opens = (0..opens_count)
                    .map(|_| {
                        let opens_index = payload_reader.read_u2()?;
                        let opens_flags =
                            ModuleOpensAccessFlag::from_bits_retain(payload_reader.read_u2()?);
                        let opens_to_count = payload_reader.read_u2()? as usize;
                        let opens_to_index = (0..opens_to_count)
                            .map(|_| payload_reader.read_u2())
                            .collect::<Result<Vec<_>>>()?;
                        Ok(OpensInfo {
                            opens_index,
                            opens_flags,
                            opens_to_index,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;

                let uses_count = payload_reader.read_u2()? as usize;
                let uses_index = (0..uses_count)
                    .map(|_| payload_reader.read_u2())
                    .collect::<Result<Vec<_>>>()?;

                let provides_count = payload_reader.read_u2()? as usize;
                let provides = (0..provides_count)
                    .map(|_| {
                        let provides_index = payload_reader.read_u2()?;
                        let provides_with_count = payload_reader.read_u2()? as usize;
                        let provides_with_index = (0..provides_with_count)
                            .map(|_| payload_reader.read_u2())
                            .collect::<Result<Vec<_>>>()?;
                        Ok(ProvidesInfo {
                            provides_index,
                            provides_with_index,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;

                AttributeInfo::Module(ModuleAttribute {
                    attribute_name_index: name_index,
                    attribute_length,
                    module: ModuleAttributeInfo {
                        module_name_index,
                        module_flags,
                        module_version_index,
                        requires,
                        exports,
                        opens,
                        uses_index,
                        provides,
                    },
                })
            }
            "ModulePackages" => {
                let count = payload_reader.read_u2()? as usize;
                let package_index = (0..count)
                    .map(|_| payload_reader.read_u2())
                    .collect::<Result<Vec<_>>>()?;
                AttributeInfo::ModulePackages(ModulePackagesAttribute {
                    attribute_name_index: name_index,
                    attribute_length,
                    package_index,
                })
            }
            "ModuleMainClass" => AttributeInfo::ModuleMainClass(ModuleMainClassAttribute {
                attribute_name_index: name_index,
                attribute_length,
                main_class_index: payload_reader.read_u2()?,
            }),
            "Record" => {
                let count = payload_reader.read_u2()? as usize;
                let components = (0..count)
                    .map(|_| {
                        let name_index = payload_reader.read_u2()?;
                        let descriptor_index = payload_reader.read_u2()?;
                        let nested_count = payload_reader.read_u2()? as usize;
                        let attributes = (0..nested_count)
                            .map(|_| self.read_attribute_from_payload(&mut payload_reader))
                            .collect::<Result<Vec<_>>>()?;
                        Ok(RecordComponentInfo {
                            name_index,
                            descriptor_index,
                            attributes,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                AttributeInfo::Record(RecordAttribute {
                    attribute_name_index: name_index,
                    attribute_length,
                    components,
                })
            }
            "PermittedSubclasses" => {
                let count = payload_reader.read_u2()? as usize;
                let classes = (0..count)
                    .map(|_| payload_reader.read_u2())
                    .collect::<Result<Vec<_>>>()?;
                AttributeInfo::PermittedSubclasses(PermittedSubclassesAttribute {
                    attribute_name_index: name_index,
                    attribute_length,
                    classes,
                })
            }
            _ => AttributeInfo::Unknown(UnknownAttribute {
                attribute_name_index: name_index,
                attribute_length,
                name: name.to_owned(),
                info: payload_reader.read_bytes(payload.len())?.to_vec(),
            }),
        };

        if payload_reader.remaining() != 0 {
            return Err(EngineError::new(
                error_offset,
                EngineErrorKind::InvalidAttribute {
                    reason: consume_error_reason.to_owned(),
                },
            ));
        }
        Ok(attribute)
    }

    fn read_annotation_info(
        &self,
        payload_reader: &mut ByteReader<'_>,
        error_offset: usize,
    ) -> Result<AnnotationInfo> {
        let type_index = payload_reader.read_u2()?;
        let pair_count = payload_reader.read_u2()? as usize;
        let element_value_pairs = (0..pair_count)
            .map(|_| {
                Ok(ElementValuePairInfo {
                    element_name_index: payload_reader.read_u2()?,
                    element_value: self.read_element_value_info(payload_reader, error_offset)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(AnnotationInfo {
            type_index,
            element_value_pairs,
        })
    }

    fn read_parameter_annotation_info(
        &self,
        payload_reader: &mut ByteReader<'_>,
        error_offset: usize,
    ) -> Result<ParameterAnnotationInfo> {
        let count = payload_reader.read_u2()? as usize;
        let annotations = (0..count)
            .map(|_| self.read_annotation_info(payload_reader, error_offset))
            .collect::<Result<Vec<_>>>()?;
        Ok(ParameterAnnotationInfo { annotations })
    }

    fn read_type_annotation_info(
        &self,
        payload_reader: &mut ByteReader<'_>,
        error_offset: usize,
    ) -> Result<TypeAnnotationInfo> {
        let target_type_tag = payload_reader.read_u1()?;
        let target_type = TargetType::from_tag(target_type_tag).ok_or_else(|| {
            EngineError::new(
                error_offset,
                EngineErrorKind::InvalidAttribute {
                    reason: format!("unknown target type: {target_type_tag}"),
                },
            )
        })?;
        let target_info = self.read_target_info(payload_reader, target_type, error_offset)?;
        let target_path = self.read_type_path_info(payload_reader, error_offset)?;
        let type_index = payload_reader.read_u2()?;
        let pair_count = payload_reader.read_u2()? as usize;
        let element_value_pairs = (0..pair_count)
            .map(|_| {
                Ok(ElementValuePairInfo {
                    element_name_index: payload_reader.read_u2()?,
                    element_value: self.read_element_value_info(payload_reader, error_offset)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(TypeAnnotationInfo {
            target_type,
            target_info,
            target_path,
            type_index,
            element_value_pairs,
        })
    }

    fn read_target_info(
        &self,
        payload_reader: &mut ByteReader<'_>,
        target_type: TargetType,
        _error_offset: usize,
    ) -> Result<TargetInfo> {
        match TargetInfoType::from_target_type(target_type) {
            TargetInfoType::TypeParameter => Ok(TargetInfo::TypeParameter {
                type_parameter_index: payload_reader.read_u1()?,
            }),
            TargetInfoType::Supertype => Ok(TargetInfo::Supertype {
                supertype_index: payload_reader.read_u2()?,
            }),
            TargetInfoType::TypeParameterBound => Ok(TargetInfo::TypeParameterBound {
                type_parameter_index: payload_reader.read_u1()?,
                bound_index: payload_reader.read_u1()?,
            }),
            TargetInfoType::Empty => Ok(TargetInfo::Empty),
            TargetInfoType::FormalParameter => Ok(TargetInfo::FormalParameter {
                formal_parameter_index: payload_reader.read_u1()?,
            }),
            TargetInfoType::Throws => Ok(TargetInfo::Throws {
                throws_type_index: payload_reader.read_u2()?,
            }),
            TargetInfoType::Localvar => {
                let table_length = payload_reader.read_u2()? as usize;
                let table = (0..table_length)
                    .map(|_| {
                        Ok(TableInfo {
                            start_pc: payload_reader.read_u2()?,
                            length: payload_reader.read_u2()?,
                            index: payload_reader.read_u2()?,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(TargetInfo::Localvar { table })
            }
            TargetInfoType::Catch => Ok(TargetInfo::Catch {
                exception_table_index: payload_reader.read_u2()?,
            }),
            TargetInfoType::Offset => Ok(TargetInfo::Offset {
                offset: payload_reader.read_u2()?,
            }),
            TargetInfoType::TypeArgument => Ok(TargetInfo::TypeArgument {
                offset: payload_reader.read_u2()?,
                type_argument_index: payload_reader.read_u1()?,
            }),
        }
    }

    fn read_type_path_info(
        &self,
        payload_reader: &mut ByteReader<'_>,
        error_offset: usize,
    ) -> Result<TypePathInfo> {
        let path_length = payload_reader.read_u1()? as usize;
        let path = (0..path_length)
            .map(|_| {
                let type_path_kind_tag = payload_reader.read_u1()?;
                let type_path_kind =
                    TypePathKind::from_tag(type_path_kind_tag).ok_or_else(|| {
                        EngineError::new(
                            error_offset,
                            EngineErrorKind::InvalidAttribute {
                                reason: format!("unknown type path kind: {type_path_kind_tag}"),
                            },
                        )
                    })?;
                Ok(PathInfo {
                    type_path_kind,
                    type_argument_index: payload_reader.read_u1()?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(TypePathInfo { path })
    }

    fn read_element_value_info(
        &self,
        payload_reader: &mut ByteReader<'_>,
        error_offset: usize,
    ) -> Result<ElementValueInfo> {
        let tag = payload_reader.read_u1()?;
        match ElementValueTag::from_tag(tag) {
            Some(
                tag @ (ElementValueTag::Byte
                | ElementValueTag::Char
                | ElementValueTag::Double
                | ElementValueTag::Float
                | ElementValueTag::Int
                | ElementValueTag::Long
                | ElementValueTag::Short
                | ElementValueTag::Boolean
                | ElementValueTag::String),
            ) => Ok(ElementValueInfo::Const {
                tag,
                const_value_index: payload_reader.read_u2()?,
            }),
            Some(ElementValueTag::Enum) => Ok(ElementValueInfo::Enum {
                type_name_index: payload_reader.read_u2()?,
                const_name_index: payload_reader.read_u2()?,
            }),
            Some(ElementValueTag::Class) => Ok(ElementValueInfo::Class {
                class_info_index: payload_reader.read_u2()?,
            }),
            Some(ElementValueTag::Annotation) => Ok(ElementValueInfo::Annotation(
                self.read_annotation_info(payload_reader, error_offset)?,
            )),
            Some(ElementValueTag::Array) => {
                let count = payload_reader.read_u2()? as usize;
                let values = (0..count)
                    .map(|_| self.read_element_value_info(payload_reader, error_offset))
                    .collect::<Result<Vec<_>>>()?;
                Ok(ElementValueInfo::Array { values })
            }
            None => Err(EngineError::new(
                error_offset,
                EngineErrorKind::InvalidAttribute {
                    reason: format!("unknown element value tag: {tag}"),
                },
            )),
        }
    }

    fn read_stack_map_frame(
        &self,
        payload_reader: &mut ByteReader<'_>,
        error_offset: usize,
    ) -> Result<StackMapFrameInfo> {
        let frame_type = payload_reader.read_u1()?;
        match frame_type {
            0..=63 => Ok(StackMapFrameInfo::Same { frame_type }),
            64..=127 => Ok(StackMapFrameInfo::SameLocals1StackItem {
                frame_type,
                stack: self.read_verification_type(payload_reader, error_offset)?,
            }),
            247 => Ok(StackMapFrameInfo::SameLocals1StackItemExtended {
                frame_type,
                offset_delta: payload_reader.read_u2()?,
                stack: self.read_verification_type(payload_reader, error_offset)?,
            }),
            248..=250 => Ok(StackMapFrameInfo::Chop {
                frame_type,
                offset_delta: payload_reader.read_u2()?,
            }),
            251 => Ok(StackMapFrameInfo::SameExtended {
                frame_type,
                offset_delta: payload_reader.read_u2()?,
            }),
            252..=254 => {
                let offset_delta = payload_reader.read_u2()?;
                let locals = (0..usize::from(frame_type - 251))
                    .map(|_| self.read_verification_type(payload_reader, error_offset))
                    .collect::<Result<Vec<_>>>()?;
                Ok(StackMapFrameInfo::Append {
                    frame_type,
                    offset_delta,
                    locals,
                })
            }
            255 => {
                let offset_delta = payload_reader.read_u2()?;
                let number_of_locals = payload_reader.read_u2()? as usize;
                let locals = (0..number_of_locals)
                    .map(|_| self.read_verification_type(payload_reader, error_offset))
                    .collect::<Result<Vec<_>>>()?;
                let number_of_stack_items = payload_reader.read_u2()? as usize;
                let stack = (0..number_of_stack_items)
                    .map(|_| self.read_verification_type(payload_reader, error_offset))
                    .collect::<Result<Vec<_>>>()?;
                Ok(StackMapFrameInfo::Full {
                    frame_type,
                    offset_delta,
                    locals,
                    stack,
                })
            }
            _ => Err(EngineError::new(
                error_offset,
                EngineErrorKind::InvalidAttribute {
                    reason: format!("unknown stack map frame type: {frame_type}"),
                },
            )),
        }
    }

    fn read_verification_type(
        &self,
        payload_reader: &mut ByteReader<'_>,
        error_offset: usize,
    ) -> Result<VerificationTypeInfo> {
        let tag = payload_reader.read_u1()?;
        match VerificationType::from_tag(tag) {
            Some(VerificationType::Top) => Ok(VerificationTypeInfo::Top),
            Some(VerificationType::Integer) => Ok(VerificationTypeInfo::Integer),
            Some(VerificationType::Float) => Ok(VerificationTypeInfo::Float),
            Some(VerificationType::Double) => Ok(VerificationTypeInfo::Double),
            Some(VerificationType::Long) => Ok(VerificationTypeInfo::Long),
            Some(VerificationType::Null) => Ok(VerificationTypeInfo::Null),
            Some(VerificationType::UninitializedThis) => {
                Ok(VerificationTypeInfo::UninitializedThis)
            }
            Some(VerificationType::Object) => Ok(VerificationTypeInfo::Object {
                cpool_index: payload_reader.read_u2()?,
            }),
            Some(VerificationType::Uninitialized) => Ok(VerificationTypeInfo::Uninitialized {
                offset: payload_reader.read_u2()?,
            }),
            None => Err(EngineError::new(
                error_offset,
                EngineErrorKind::InvalidAttribute {
                    reason: format!("unknown verification type tag: {tag}"),
                },
            )),
        }
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
