use pytecode_engine::analysis::{Category, verify_classfile, verify_classmodel};
use pytecode_engine::constants::{ClassAccessFlags, FieldAccessFlags, MAGIC, MethodAccessFlags};
use pytecode_engine::model::{BranchInsn, ClassModel, CodeItem, FieldModel, Label};
use pytecode_engine::raw::{
    AttributeInfo, ClassFile, CodeAttribute, ConstantPoolEntry, FieldInfo, MethodInfo, Utf8Info,
};

fn base_classfile() -> ClassFile {
    ClassFile {
        magic: MAGIC,
        minor_version: 0,
        major_version: 52,
        constant_pool: vec![
            None,
            Some(ConstantPoolEntry::Utf8(pytecode_engine::raw::Utf8Info {
                bytes: pytecode_engine::modified_utf8::encode_modified_utf8("TestClass"),
            })),
            Some(ConstantPoolEntry::Class(pytecode_engine::raw::ClassInfo {
                name_index: 1.into(),
            })),
            Some(ConstantPoolEntry::Utf8(pytecode_engine::raw::Utf8Info {
                bytes: pytecode_engine::modified_utf8::encode_modified_utf8("java/lang/Object"),
            })),
            Some(ConstantPoolEntry::Class(pytecode_engine::raw::ClassInfo {
                name_index: 3.into(),
            })),
        ],
        access_flags: ClassAccessFlags::PUBLIC | ClassAccessFlags::SUPER,
        this_class: 2.into(),
        super_class: 4.into(),
        interfaces: Vec::new(),
        fields: Vec::new(),
        methods: Vec::new(),
        attributes: Vec::new(),
    }
}

fn push_cp_entry(classfile: &mut ClassFile, entry: ConstantPoolEntry) -> u16 {
    let index = classfile.constant_pool.len() as u16;
    let is_wide = entry.is_wide();
    classfile.constant_pool.push(Some(entry));
    if is_wide {
        classfile.constant_pool.push(None);
    }
    index
}

fn push_utf8(classfile: &mut ClassFile, value: &str) -> u16 {
    push_cp_entry(
        classfile,
        ConstantPoolEntry::Utf8(Utf8Info {
            bytes: pytecode_engine::modified_utf8::encode_modified_utf8(value),
        }),
    )
}

fn push_class(classfile: &mut ClassFile, value: &str) -> u16 {
    let name_index = push_utf8(classfile, value);
    push_cp_entry(
        classfile,
        ConstantPoolEntry::Class(pytecode_engine::raw::ClassInfo {
            name_index: name_index.into(),
        }),
    )
}

fn push_module(classfile: &mut ClassFile, value: &str) -> u16 {
    let name_index = push_utf8(classfile, value);
    push_cp_entry(
        classfile,
        ConstantPoolEntry::Module(pytecode_engine::raw::ModuleInfo {
            name_index: name_index.into(),
        }),
    )
}

fn push_name_and_type(classfile: &mut ClassFile, name: &str, descriptor: &str) -> u16 {
    let name_index = push_utf8(classfile, name);
    let descriptor_index = push_utf8(classfile, descriptor);
    push_cp_entry(
        classfile,
        ConstantPoolEntry::NameAndType(pytecode_engine::raw::NameAndTypeInfo {
            name_index: name_index.into(),
            descriptor_index: descriptor_index.into(),
        }),
    )
}

fn push_field_ref(
    classfile: &mut ClassFile,
    class_index: u16,
    name: &str,
    descriptor: &str,
) -> u16 {
    let name_and_type_index = push_name_and_type(classfile, name, descriptor);
    push_cp_entry(
        classfile,
        ConstantPoolEntry::FieldRef(pytecode_engine::raw::FieldRefInfo {
            class_index: class_index.into(),
            name_and_type_index: name_and_type_index.into(),
        }),
    )
}

fn push_method_ref(
    classfile: &mut ClassFile,
    class_index: u16,
    name: &str,
    descriptor: &str,
) -> u16 {
    let name_and_type_index = push_name_and_type(classfile, name, descriptor);
    push_cp_entry(
        classfile,
        ConstantPoolEntry::MethodRef(pytecode_engine::raw::MethodRefInfo {
            class_index: class_index.into(),
            name_and_type_index: name_and_type_index.into(),
        }),
    )
}

#[test]
fn verify_classfile_reports_magic_and_descriptor_issues() {
    let mut classfile = base_classfile();
    classfile.magic = 0xDEADBEEF;
    let diagnostics = verify_classfile(&classfile);
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.category == Category::Magic)
    );
}

#[test]
fn verify_classfile_applies_java_25_version_rules() {
    let mut old_preview = base_classfile();
    old_preview.major_version = 68;
    old_preview.minor_version = u16::MAX;
    let old_preview_diagnostics = verify_classfile(&old_preview);
    assert!(
        old_preview_diagnostics
            .iter()
            .any(|diag| diag.category == Category::Version)
    );

    let mut future_major = base_classfile();
    future_major.major_version = 70;
    let future_major_diagnostics = verify_classfile(&future_major);
    assert!(
        future_major_diagnostics
            .iter()
            .any(|diag| diag.category == Category::Version)
    );

    let mut current_preview = base_classfile();
    current_preview.major_version = 69;
    current_preview.minor_version = u16::MAX;
    let current_preview_diagnostics = verify_classfile(&current_preview);
    assert!(
        !current_preview_diagnostics
            .iter()
            .any(|diag| diag.category == Category::Version)
    );
}

#[test]
fn verify_classfile_reports_structural_and_access_flag_issues() {
    let mut classfile = base_classfile();
    classfile.constant_pool[0] = Some(ConstantPoolEntry::Utf8(Utf8Info {
        bytes: pytecode_engine::modified_utf8::encode_modified_utf8("bad"),
    }));
    classfile.constant_pool.push(None);
    classfile.super_class = 0.into();

    let field_name = push_utf8(&mut classfile, "value");
    let field_desc = push_utf8(&mut classfile, "I");
    classfile.fields.push(FieldInfo {
        access_flags: FieldAccessFlags::PUBLIC
            | FieldAccessFlags::PRIVATE
            | FieldAccessFlags::FINAL
            | FieldAccessFlags::VOLATILE,
        name_index: field_name.into(),
        descriptor_index: field_desc.into(),
        attributes: Vec::new(),
    });

    let method_name = push_utf8(&mut classfile, "m");
    let method_desc = push_utf8(&mut classfile, "()V");
    let code_name = push_utf8(&mut classfile, "Code");
    classfile.methods.push(MethodInfo {
        access_flags: MethodAccessFlags::PUBLIC
            | MethodAccessFlags::PRIVATE
            | MethodAccessFlags::NATIVE,
        name_index: method_name.into(),
        descriptor_index: method_desc.into(),
        attributes: vec![AttributeInfo::Code(CodeAttribute {
            attribute_name_index: code_name.into(),
            attribute_length: 0,
            max_stack: 0,
            max_locals: 0,
            code_length: 0,
            code: Vec::new(),
            exception_table: Vec::new(),
            attributes: Vec::new(),
        })],
    });

    let diagnostics = verify_classfile(&classfile);
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.category == Category::ConstantPool)
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.category == Category::ClassStructure)
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.category == Category::AccessFlags)
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.category == Category::Method)
    );
}

#[test]
fn verify_classfile_reports_invalid_module_class_structure() {
    let mut classfile = base_classfile();
    classfile.major_version = 53;
    classfile.access_flags = ClassAccessFlags::MODULE;
    let method_name = push_utf8(&mut classfile, "m");
    let method_desc = push_utf8(&mut classfile, "()V");
    classfile.methods.push(MethodInfo {
        access_flags: MethodAccessFlags::PUBLIC,
        name_index: method_name.into(),
        descriptor_index: method_desc.into(),
        attributes: Vec::new(),
    });

    let diagnostics = verify_classfile(&classfile);
    assert!(diagnostics.iter().any(|diag| {
        diag.category == Category::ClassStructure
            && diag
                .message
                .contains("module class must not declare methods")
    }));
    assert!(diagnostics.iter().any(|diag| {
        diag.category == Category::Attribute
            && diag
                .message
                .contains("module class must declare exactly one Module attribute")
    }));
}

#[test]
fn verify_classfile_reports_invalid_attribute_placement_and_versions() {
    let mut classfile = base_classfile();
    classfile.major_version = 51;

    let module_attr_name = push_utf8(&mut classfile, "Module");
    let module_main_class_attr_name = push_utf8(&mut classfile, "ModuleMainClass");
    let record_attr_name = push_utf8(&mut classfile, "Record");
    let nest_host_attr_name = push_utf8(&mut classfile, "NestHost");
    let code_attr_name = push_utf8(&mut classfile, "Code");
    let method_parameters_attr_name = push_utf8(&mut classfile, "MethodParameters");
    let stack_map_attr_name = push_utf8(&mut classfile, "StackMapTable");
    let component_name = push_utf8(&mut classfile, "value");
    let component_desc = push_utf8(&mut classfile, "I");
    let module_index = push_module(&mut classfile, "test.module");
    let main_class_index = push_class(&mut classfile, "pkg/Main");
    let host_class_index = push_class(&mut classfile, "pkg/Host");

    classfile.attributes = vec![
        AttributeInfo::Module(pytecode_engine::raw::ModuleAttribute {
            attribute_name_index: module_attr_name.into(),
            attribute_length: 0,
            module: pytecode_engine::raw::ModuleAttributeModuleInfo {
                module_name_index: module_index.into(),
                module_flags: pytecode_engine::constants::ModuleAccessFlag::empty(),
                module_version_index: 0.into(),
                requires: Vec::new(),
                exports: Vec::new(),
                opens: Vec::new(),
                uses_index: Vec::new(),
                provides: Vec::new(),
            },
        }),
        AttributeInfo::ModuleMainClass(pytecode_engine::raw::ModuleMainClassAttribute {
            attribute_name_index: module_main_class_attr_name.into(),
            attribute_length: 0,
            main_class_index: main_class_index.into(),
        }),
        AttributeInfo::Record(pytecode_engine::raw::RecordAttribute {
            attribute_name_index: record_attr_name.into(),
            attribute_length: 0,
            components: vec![pytecode_engine::raw::RecordComponentInfo {
                name_index: component_name.into(),
                descriptor_index: component_desc.into(),
                attributes: vec![AttributeInfo::Code(CodeAttribute {
                    attribute_name_index: code_attr_name.into(),
                    attribute_length: 0,
                    max_stack: 0,
                    max_locals: 0,
                    code_length: 0,
                    code: Vec::new(),
                    exception_table: Vec::new(),
                    attributes: Vec::new(),
                })],
            }],
        }),
        AttributeInfo::NestHost(pytecode_engine::raw::NestHostAttribute {
            attribute_name_index: nest_host_attr_name.into(),
            attribute_length: 0,
            host_class_index: host_class_index.into(),
        }),
    ];

    let method_name = push_utf8(&mut classfile, "m");
    let method_desc = push_utf8(&mut classfile, "()V");
    classfile.methods.push(MethodInfo {
        access_flags: MethodAccessFlags::PUBLIC,
        name_index: method_name.into(),
        descriptor_index: method_desc.into(),
        attributes: vec![AttributeInfo::Code(CodeAttribute {
            attribute_name_index: code_attr_name.into(),
            attribute_length: 0,
            max_stack: 0,
            max_locals: 0,
            code_length: 0,
            code: Vec::new(),
            exception_table: Vec::new(),
            attributes: vec![
                AttributeInfo::MethodParameters(pytecode_engine::raw::MethodParametersAttribute {
                    attribute_name_index: method_parameters_attr_name.into(),
                    attribute_length: 0,
                    parameters: vec![pytecode_engine::raw::MethodParameterInfo {
                        name_index: 0.into(),
                        access_flags: pytecode_engine::constants::MethodParameterAccessFlag::empty(
                        ),
                    }],
                }),
                AttributeInfo::StackMapTable(pytecode_engine::raw::StackMapTableAttribute {
                    attribute_name_index: stack_map_attr_name.into(),
                    attribute_length: 0,
                    entries: Vec::new(),
                }),
                AttributeInfo::StackMapTable(pytecode_engine::raw::StackMapTableAttribute {
                    attribute_name_index: stack_map_attr_name.into(),
                    attribute_length: 0,
                    entries: Vec::new(),
                }),
            ],
        })],
    });

    let diagnostics = verify_classfile(&classfile);
    assert!(diagnostics.iter().any(|diag| {
        diag.category == Category::Attribute
            && diag
                .message
                .contains("Module attribute is only allowed on module classes")
    }));
    assert!(diagnostics.iter().any(|diag| {
        diag.category == Category::Attribute
            && diag
                .message
                .contains("ModuleMainClass attribute is only allowed on module classes")
    }));
    assert!(diagnostics.iter().any(|diag| {
        diag.category == Category::Attribute
            && diag
                .message
                .contains("Record attribute requires class file version 60.0 or newer")
    }));
    assert!(diagnostics.iter().any(|diag| {
        diag.category == Category::Attribute
            && diag
                .message
                .contains("NestHost attribute requires class file version 55.0 or newer")
    }));
    assert!(diagnostics.iter().any(|diag| {
        diag.category == Category::Attribute
            && diag
                .message
                .contains("Code attribute is not allowed on record components")
    }));
    assert!(diagnostics.iter().any(|diag| {
        diag.category == Category::Attribute
            && diag
                .message
                .contains("MethodParameters attribute is not allowed on code attributes")
    }));
    assert!(diagnostics.iter().any(|diag| {
        diag.category == Category::Attribute
            && diag
                .message
                .contains("Code attribute must not declare multiple StackMapTable attributes")
    }));
}

#[test]
fn verify_classfile_reports_invalid_generic_signatures() {
    let mut classfile = base_classfile();
    let signature_attr_name = push_utf8(&mut classfile, "Signature");
    let code_attr_name = push_utf8(&mut classfile, "Code");
    let lvtt_attr_name = push_utf8(&mut classfile, "LocalVariableTypeTable");
    let class_signature = push_utf8(&mut classfile, "<>Ljava/lang/Object;");
    let field_signature = push_utf8(&mut classfile, "I");
    let method_signature = push_utf8(&mut classfile, "<T:>(TT;)");
    let local_signature = push_utf8(&mut classfile, "Ljava/util/List<;");
    let field_name = push_utf8(&mut classfile, "value");
    let field_desc = push_utf8(&mut classfile, "Ljava/lang/Object;");
    let method_name = push_utf8(&mut classfile, "m");
    let method_desc = push_utf8(&mut classfile, "(I)V");
    let local_name = push_utf8(&mut classfile, "local");

    classfile.attributes.push(AttributeInfo::Signature(
        pytecode_engine::raw::SignatureAttribute {
            attribute_name_index: signature_attr_name.into(),
            attribute_length: 0,
            signature_index: class_signature.into(),
        },
    ));
    classfile.fields.push(FieldInfo {
        access_flags: FieldAccessFlags::PRIVATE,
        name_index: field_name.into(),
        descriptor_index: field_desc.into(),
        attributes: vec![AttributeInfo::Signature(
            pytecode_engine::raw::SignatureAttribute {
                attribute_name_index: signature_attr_name.into(),
                attribute_length: 0,
                signature_index: field_signature.into(),
            },
        )],
    });
    classfile.methods.push(MethodInfo {
        access_flags: MethodAccessFlags::PUBLIC,
        name_index: method_name.into(),
        descriptor_index: method_desc.into(),
        attributes: vec![
            AttributeInfo::Signature(pytecode_engine::raw::SignatureAttribute {
                attribute_name_index: signature_attr_name.into(),
                attribute_length: 0,
                signature_index: method_signature.into(),
            }),
            AttributeInfo::Code(CodeAttribute {
                attribute_name_index: code_attr_name.into(),
                attribute_length: 0,
                max_stack: 1,
                max_locals: 2,
                code_length: 1,
                code: vec![pytecode_engine::raw::Instruction::Simple {
                    opcode: 0xB1,
                    offset: 0,
                }],
                exception_table: Vec::new(),
                attributes: vec![AttributeInfo::LocalVariableTypeTable(
                    pytecode_engine::raw::LocalVariableTypeTableAttribute {
                        attribute_name_index: lvtt_attr_name.into(),
                        attribute_length: 0,
                        local_variable_type_table: vec![
                            pytecode_engine::raw::LocalVariableTypeInfo {
                                start_pc: 0,
                                length: 1,
                                name_index: local_name.into(),
                                signature_index: local_signature.into(),
                                index: 1,
                            },
                        ],
                    },
                )],
            }),
        ],
    });

    let diagnostics = verify_classfile(&classfile);
    assert!(diagnostics.iter().any(|diag| {
        diag.category == Category::Attribute && diag.message.contains("invalid class signature")
    }));
    assert!(diagnostics.iter().any(|diag| {
        diag.category == Category::Attribute && diag.message.contains("invalid field signature")
    }));
    assert!(diagnostics.iter().any(|diag| {
        diag.category == Category::Attribute && diag.message.contains("invalid method signature")
    }));
    assert!(diagnostics.iter().any(|diag| {
        diag.category == Category::Attribute
            && diag
                .message
                .contains("invalid local variable type signature")
    }));
}

#[test]
fn verify_classfile_reports_invalid_bootstrap_and_method_handle_links() {
    let mut classfile = base_classfile();
    let owner_class = push_class(&mut classfile, "pkg/Owner");
    let field_ref = push_field_ref(&mut classfile, owner_class, "value", "I");
    let method_ref = push_method_ref(&mut classfile, owner_class, "run", "()V");
    let dynamic_nat = push_name_and_type(&mut classfile, "dyn", "()V");
    let indy_nat = push_name_and_type(&mut classfile, "callsite", "I");
    let bootstrap_attr_name = push_utf8(&mut classfile, "BootstrapMethods");

    classfile
        .constant_pool
        .push(Some(ConstantPoolEntry::MethodHandle(
            pytecode_engine::raw::MethodHandleInfo {
                reference_kind: 0,
                reference_index: field_ref.into(),
            },
        )));
    classfile
        .constant_pool
        .push(Some(ConstantPoolEntry::MethodHandle(
            pytecode_engine::raw::MethodHandleInfo {
                reference_kind: 8,
                reference_index: method_ref.into(),
            },
        )));
    classfile
        .constant_pool
        .push(Some(ConstantPoolEntry::Dynamic(
            pytecode_engine::raw::DynamicInfo {
                bootstrap_method_attr_index: 1.into(),
                name_and_type_index: dynamic_nat.into(),
            },
        )));
    classfile
        .constant_pool
        .push(Some(ConstantPoolEntry::InvokeDynamic(
            pytecode_engine::raw::InvokeDynamicInfo {
                bootstrap_method_attr_index: 1.into(),
                name_and_type_index: indy_nat.into(),
            },
        )));

    classfile.attributes.push(AttributeInfo::BootstrapMethods(
        pytecode_engine::raw::BootstrapMethodsAttribute {
            attribute_name_index: bootstrap_attr_name.into(),
            attribute_length: 0,
            bootstrap_methods: vec![pytecode_engine::raw::BootstrapMethodInfo {
                bootstrap_method_ref: field_ref.into(),
                bootstrap_arguments: vec![field_ref.into()],
            }],
        },
    ));

    let diagnostics = verify_classfile(&classfile);
    assert!(diagnostics.iter().any(|diag| {
        diag.category == Category::ConstantPool
            && diag
                .message
                .contains("method handle reference_kind must be in 1..=9")
    }));
    assert!(diagnostics.iter().any(|diag| {
        diag.category == Category::ConstantPool
            && diag
                .message
                .contains("REF_newInvokeSpecial method handle must reference <init>")
    }));
    assert!(diagnostics.iter().any(|diag| {
        diag.category == Category::ConstantPool
            && diag
                .message
                .contains("dynamic entry descriptor must be a field descriptor")
    }));
    assert!(diagnostics.iter().any(|diag| {
        diag.category == Category::ConstantPool
            && diag
                .message
                .contains("invokedynamic entry descriptor must be a method descriptor")
    }));
    assert!(diagnostics.iter().any(|diag| {
        diag.category == Category::ConstantPool
            && diag
                .message
                .contains("dynamic entry bootstrap_method_attr_index is out of range")
    }));
    assert!(diagnostics.iter().any(|diag| {
        diag.category == Category::ConstantPool
            && diag
                .message
                .contains("invokedynamic entry bootstrap_method_attr_index is out of range")
    }));
    assert!(diagnostics.iter().any(|diag| {
        diag.category == Category::ConstantPool
            && diag.message.contains(
                "BootstrapMethods bootstrap_method_ref must reference CONSTANT_MethodHandle",
            )
    }));
    assert!(diagnostics.iter().any(|diag| {
        diag.category == Category::ConstantPool
            && diag
                .message
                .contains("BootstrapMethods arguments must reference loadable constants")
    }));
}

#[test]
fn verify_classmodel_reports_invalid_descriptors_and_bad_cfg() {
    let bytes = std::fs::read(
        pytecode_engine::fixtures::compiled_fixture_paths_for("ControlFlowExample.java")
            .expect("fixture paths should load")
            .into_iter()
            .find(|path| path.ends_with("ControlFlowExample.class"))
            .expect("fixture should exist"),
    )
    .expect("fixture bytes should read");
    let mut model = ClassModel::from_bytes(&bytes).expect("fixture should parse");
    model.fields.push(FieldModel {
        access_flags: pytecode_engine::constants::FieldAccessFlags::PUBLIC,
        name: "bad".to_owned(),
        descriptor: "not-a-descriptor".to_owned(),
        attributes: Vec::new(),
    });

    let code = model
        .methods
        .iter_mut()
        .find(|method| method.name == "branch")
        .and_then(|method| method.code.as_mut())
        .expect("branch method should have code");
    let branch = code
        .instructions
        .iter_mut()
        .find_map(|item| match item {
            CodeItem::Branch(branch) => Some(branch),
            _ => None,
        })
        .expect("branch instruction should exist");
    *branch = BranchInsn {
        opcode: branch.opcode,
        target: Label::named("missing"),
    };

    let diagnostics = verify_classmodel(&model, None);
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.category == Category::Descriptor)
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.category == Category::Code)
    );
}
