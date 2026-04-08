use pytecode_engine::constants::MAGIC;
use pytecode_engine::error::EngineErrorKind;
use pytecode_engine::fixtures::{
    compiled_fixture_paths as rust_compiled_fixture_paths,
    compiled_fixture_paths_for as rust_compiled_fixture_paths_for,
};
use pytecode_engine::modified_utf8::{decode_modified_utf8, encode_modified_utf8};
use pytecode_engine::parse_class;
use pytecode_engine::raw::AttributeInfo;
use pytecode_engine::write_class;

type TestResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[test]
fn minimal_classfile_parses() -> TestResult<()> {
    let parsed = parse_class(&minimal_classfile())?;
    assert_eq!(parsed.magic, MAGIC);
    assert_eq!(parsed.major_version, 52);
    assert_eq!(parsed.minor_version, 0);
    assert_eq!(parsed.this_class, 2);
    assert_eq!(parsed.super_class, 4);
    assert_eq!(parsed.interfaces.len(), 0);
    assert_eq!(parsed.fields.len(), 0);
    assert_eq!(parsed.methods.len(), 0);
    assert_eq!(parsed.attributes.len(), 0);
    Ok(())
}

#[test]
fn writer_roundtrip_preserves_unknown_attribute_bytes() -> TestResult<()> {
    let raw = minimal_classfile_with_options(MinimalClassfileOptions {
        extra_cp_bytes: utf8_entry_bytes("CustomAttr"),
        extra_cp_count: 1,
        class_attrs_count: 1,
        class_attrs_bytes: make_attribute_blob(5, &[0x01, 0x02, 0x03]),
        ..MinimalClassfileOptions::default()
    });
    let parsed = parse_class(&raw)?;
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

#[test]
fn writer_roundtrip_preserves_long_gap_slots() -> TestResult<()> {
    let raw = minimal_classfile_with_options(MinimalClassfileOptions {
        extra_cp_bytes: long_entry_bytes(0x11111111, 0x22222222),
        extra_cp_count: 2,
        ..MinimalClassfileOptions::default()
    });
    let parsed = parse_class(&raw)?;
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

#[test]
fn invalid_magic_and_version_return_structured_errors() {
    let mut invalid_magic = minimal_classfile();
    invalid_magic[0] = 0;
    let err = parse_class(&invalid_magic).unwrap_err();
    assert!(matches!(err.kind, EngineErrorKind::InvalidMagic { .. }));

    let invalid_version = minimal_classfile_with_version(56, 1);
    let err = parse_class(&invalid_version).unwrap_err();
    assert!(matches!(err.kind, EngineErrorKind::InvalidVersion { .. }));

    let old_preview_version = minimal_classfile_with_version(68, u16::MAX);
    let err = parse_class(&old_preview_version).unwrap_err();
    assert!(matches!(err.kind, EngineErrorKind::InvalidVersion { .. }));

    let future_version = minimal_classfile_with_version(70, 0);
    let err = parse_class(&future_version).unwrap_err();
    assert!(matches!(err.kind, EngineErrorKind::InvalidVersion { .. }));
}

#[test]
fn historical_and_current_preview_versions_parse_when_supported() -> TestResult<()> {
    parse_class(&minimal_classfile_with_version(55, 3))?;
    parse_class(&minimal_classfile_with_version(69, u16::MAX))?;
    Ok(())
}

#[test]
fn hello_world_fixture_exposes_code_and_source_file() -> TestResult<()> {
    let class_path = rust_compiled_fixture_paths_for("HelloWorld.java")?
        .into_iter()
        .find(|path| path.file_name().and_then(|name| name.to_str()) == Some("HelloWorld.class"))
        .ok_or("HelloWorld.class not found")?;
    let original = std::fs::read(&class_path)?;
    let parsed = parse_class(&original)?;

    let main_method = parsed
        .methods
        .iter()
        .find(|method| matches!(constant_pool_utf8(&parsed, method.name_index), Ok(name) if name == "main"))
        .ok_or("main method not found")?;

    let code_attr = main_method
        .attributes
        .iter()
        .find_map(|attribute| match attribute {
            AttributeInfo::Code(code) => Some(code),
            _ => None,
        })
        .ok_or("Code attribute not found")?;
    let opcodes: Vec<u8> = code_attr
        .code
        .iter()
        .map(|instruction| instruction.opcode())
        .collect();
    assert_eq!(opcodes, vec![0xB2, 0x12, 0xB6, 0xB1]);

    assert!(
        parsed
            .attributes
            .iter()
            .any(|attribute| matches!(attribute, AttributeInfo::SourceFile(_)))
    );
    assert_eq!(write_class(&parsed)?, original);
    Ok(())
}

#[test]
fn synthetic_attribute_roundtrips_as_typed_zero_length_attr() -> TestResult<()> {
    let raw = minimal_classfile_with_options(MinimalClassfileOptions {
        extra_cp_bytes: utf8_entry_bytes("Synthetic"),
        extra_cp_count: 1,
        class_attrs_count: 1,
        class_attrs_bytes: make_attribute_blob(5, &[]),
        ..MinimalClassfileOptions::default()
    });
    let parsed = parse_class(&raw)?;
    assert!(matches!(
        parsed.attributes.as_slice(),
        [AttributeInfo::Synthetic(_)]
    ));
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

#[test]
fn fixtures_expose_typed_stackmap_debug_and_deprecated_attrs() -> TestResult<()> {
    let hello_world = rust_compiled_fixture_paths_for("HelloWorld.java")?
        .into_iter()
        .find(|path| path.file_name().and_then(|name| name.to_str()) == Some("HelloWorld.class"))
        .ok_or("HelloWorld.class not found")?;
    let hello_world_bytes = std::fs::read(&hello_world)?;
    let hello_world_class = parse_class(&hello_world_bytes)?;
    let main_code = hello_world_class
        .methods
        .iter()
        .find(|method| matches!(constant_pool_utf8(&hello_world_class, method.name_index), Ok(name) if name == "main"))
        .and_then(|method| {
            method.attributes.iter().find_map(|attribute| match attribute {
                AttributeInfo::Code(code) => Some(code),
                _ => None,
            })
        })
        .ok_or("main code attribute not found")?;
    assert!(main_code
        .attributes
        .iter()
        .any(|attribute| matches!(attribute, AttributeInfo::LineNumberTable(table) if !table.line_number_table.is_empty())));
    assert_eq!(write_class(&hello_world_class)?, hello_world_bytes);

    let deprecated_found = hello_world_class.methods.iter().any(|method| {
        matches!(constant_pool_utf8(&hello_world_class, method.name_index), Ok(name) if name == "giveItToMe")
            && method
                .attributes
                .iter()
                .any(|attribute| matches!(attribute, AttributeInfo::Deprecated(_)))
    });
    assert!(deprecated_found);

    let control_flow_path = rust_compiled_fixture_paths_for("ControlFlowExample.java")?
        .into_iter()
        .find(|path| {
            path.file_name().and_then(|name| name.to_str()) == Some("ControlFlowExample.class")
        })
        .ok_or("ControlFlowExample.class not found")?;
    let control_flow_bytes = std::fs::read(&control_flow_path)?;
    let control_flow_class = parse_class(&control_flow_bytes)?;
    let branch_code = control_flow_class
        .methods
        .iter()
        .find(|method| matches!(constant_pool_utf8(&control_flow_class, method.name_index), Ok(name) if name == "branch"))
        .and_then(|method| {
            method.attributes.iter().find_map(|attribute| match attribute {
                AttributeInfo::Code(code) => Some(code),
                _ => None,
            })
        })
        .ok_or("branch code attribute not found")?;
    assert!(branch_code.attributes.iter().any(|attribute| {
        matches!(attribute, AttributeInfo::StackMapTable(table) if !table.entries.is_empty())
    }));
    assert_eq!(write_class(&control_flow_class)?, control_flow_bytes);
    Ok(())
}

#[test]
fn local_variable_tables_roundtrip_as_typed_nested_attrs() -> TestResult<()> {
    let mut local_variable_table = Vec::new();
    local_variable_table.extend_from_slice(&u2(1));
    local_variable_table.extend_from_slice(&u2(0));
    local_variable_table.extend_from_slice(&u2(1));
    local_variable_table.extend_from_slice(&u2(9));
    local_variable_table.extend_from_slice(&u2(10));
    local_variable_table.extend_from_slice(&u2(0));

    let mut local_variable_type_table = Vec::new();
    local_variable_type_table.extend_from_slice(&u2(1));
    local_variable_type_table.extend_from_slice(&u2(0));
    local_variable_type_table.extend_from_slice(&u2(1));
    local_variable_type_table.extend_from_slice(&u2(11));
    local_variable_type_table.extend_from_slice(&u2(12));
    local_variable_type_table.extend_from_slice(&u2(1));

    let mut code_payload = Vec::new();
    code_payload.extend_from_slice(&u2(1));
    code_payload.extend_from_slice(&u2(2));
    code_payload.extend_from_slice(&u4(1));
    code_payload.push(0xB1);
    code_payload.extend_from_slice(&u2(0));
    code_payload.extend_from_slice(&u2(2));
    code_payload.extend_from_slice(&make_attribute_blob(7, &local_variable_table));
    code_payload.extend_from_slice(&make_attribute_blob(8, &local_variable_type_table));

    let raw = minimal_classfile_with_options(MinimalClassfileOptions {
        extra_cp_bytes: [
            utf8_entry_bytes("m"),
            utf8_entry_bytes("()V"),
            utf8_entry_bytes("LocalVariableTable"),
            utf8_entry_bytes("LocalVariableTypeTable"),
            utf8_entry_bytes("this"),
            utf8_entry_bytes("LTestClass;"),
            utf8_entry_bytes("value"),
            utf8_entry_bytes("TT;"),
            utf8_entry_bytes("Code"),
        ]
        .concat(),
        extra_cp_count: 9,
        methods_count: 1,
        methods_bytes: method_info_blob(0x0001, 5, 6, &[make_attribute_blob(13, &code_payload)]),
        ..MinimalClassfileOptions::default()
    });
    let parsed = parse_class(&raw)?;
    let code = parsed.methods[0]
        .attributes
        .iter()
        .find_map(|attribute| match attribute {
            AttributeInfo::Code(code) => Some(code),
            _ => None,
        })
        .ok_or("code attribute missing")?;
    assert!(code.attributes.iter().any(|attribute| {
        matches!(attribute, AttributeInfo::LocalVariableTable(table) if table.local_variable_table.len() == 1)
    }));
    assert!(code.attributes.iter().any(|attribute| {
        matches!(attribute, AttributeInfo::LocalVariableTypeTable(table) if table.local_variable_type_table.len() == 1)
    }));
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

#[test]
fn malformed_stackmap_payload_returns_structured_error() {
    let mut code_payload = Vec::new();
    code_payload.extend_from_slice(&u2(0));
    code_payload.extend_from_slice(&u2(0));
    code_payload.extend_from_slice(&u4(1));
    code_payload.push(0xB1);
    code_payload.extend_from_slice(&u2(0));
    code_payload.extend_from_slice(&u2(1));
    let mut stack_map_payload = Vec::new();
    stack_map_payload.extend_from_slice(&u2(1));
    stack_map_payload.push(200);
    code_payload.extend_from_slice(&make_attribute_blob(8, &stack_map_payload));

    let raw = minimal_classfile_with_options(MinimalClassfileOptions {
        extra_cp_bytes: [
            utf8_entry_bytes("m"),
            utf8_entry_bytes("()V"),
            utf8_entry_bytes("Code"),
            utf8_entry_bytes("StackMapTable"),
        ]
        .concat(),
        extra_cp_count: 4,
        methods_count: 1,
        methods_bytes: method_info_blob(0x0001, 5, 6, &[make_attribute_blob(7, &code_payload)]),
        ..MinimalClassfileOptions::default()
    });

    let err = parse_class(&raw).unwrap_err();
    assert!(matches!(err.kind, EngineErrorKind::InvalidAttribute { .. }));
    assert!(err.to_string().contains("stack map frame type"));
}

#[test]
fn fixtures_expose_typed_metadata_attrs() -> TestResult<()> {
    let outer_path = rust_compiled_fixture_paths_for("Outer.java")?
        .into_iter()
        .find(|path| path.file_name().and_then(|name| name.to_str()) == Some("Outer.class"))
        .ok_or("Outer.class not found")?;
    let outer_bytes = std::fs::read(&outer_path)?;
    let outer_class = parse_class(&outer_bytes)?;
    assert!(outer_class.attributes.iter().any(|attribute| {
        matches!(attribute, AttributeInfo::InnerClasses(attr) if !attr.classes.is_empty())
    }));
    assert_eq!(write_class(&outer_class)?, outer_bytes);

    let anonymous_path = rust_compiled_fixture_paths_for("NestAccess.java")?
        .into_iter()
        .find(|path| path.file_name().and_then(|name| name.to_str()) == Some("NestAccess$1.class"))
        .ok_or("NestAccess$1.class not found")?;
    let anonymous_bytes = std::fs::read(&anonymous_path)?;
    let anonymous_class = parse_class(&anonymous_bytes)?;
    assert!(
        anonymous_class
            .attributes
            .iter()
            .any(|attribute| matches!(attribute, AttributeInfo::EnclosingMethod(_)))
    );
    assert!(
        anonymous_class
            .attributes
            .iter()
            .any(|attribute| matches!(attribute, AttributeInfo::NestHost(_)))
    );
    assert!(anonymous_class.methods.iter().any(|method| {
        method.attributes.iter().any(
            |attribute| matches!(attribute, AttributeInfo::MethodParameters(attr) if !attr.parameters.is_empty()),
        )
    }));
    assert_eq!(write_class(&anonymous_class)?, anonymous_bytes);

    let nest_access_path = rust_compiled_fixture_paths_for("NestAccess.java")?
        .into_iter()
        .find(|path| path.file_name().and_then(|name| name.to_str()) == Some("NestAccess.class"))
        .ok_or("NestAccess.class not found")?;
    let nest_access_bytes = std::fs::read(&nest_access_path)?;
    let nest_access_class = parse_class(&nest_access_bytes)?;
    assert!(nest_access_class.attributes.iter().any(|attribute| {
        matches!(attribute, AttributeInfo::NestMembers(attr) if !attr.classes.is_empty())
    }));
    assert_eq!(write_class(&nest_access_class)?, nest_access_bytes);
    Ok(())
}

#[test]
fn fixtures_and_constructed_inputs_expose_typed_annotation_attrs() -> TestResult<()> {
    let annotated_class_path = rust_compiled_fixture_paths_for("AnnotatedClass.java")?
        .into_iter()
        .find(|path| {
            path.file_name().and_then(|name| name.to_str()) == Some("AnnotatedClass.class")
        })
        .ok_or("AnnotatedClass.class not found")?;
    let annotated_class_bytes = std::fs::read(&annotated_class_path)?;
    let annotated_class = parse_class(&annotated_class_bytes)?;
    assert!(annotated_class.attributes.iter().any(|attribute| {
        matches!(attribute, AttributeInfo::RuntimeVisibleAnnotations(attr) if !attr.annotations.is_empty())
    }));
    assert_eq!(write_class(&annotated_class)?, annotated_class_bytes);

    let parameter_annotations_path = rust_compiled_fixture_paths_for("ParameterAnnotations.java")?
        .into_iter()
        .find(|path| {
            path.file_name().and_then(|name| name.to_str()) == Some("ParameterAnnotations.class")
        })
        .ok_or("ParameterAnnotations.class not found")?;
    let parameter_annotations_bytes = std::fs::read(&parameter_annotations_path)?;
    let parameter_annotations_class = parse_class(&parameter_annotations_bytes)?;
    let annotated_method = parameter_annotations_class
        .methods
        .iter()
        .find(|method| {
            matches!(constant_pool_utf8(&parameter_annotations_class, method.name_index), Ok(name) if name == "annotated")
        })
        .ok_or("annotated method not found")?;
    assert!(annotated_method.attributes.iter().any(|attribute| {
        matches!(attribute, AttributeInfo::RuntimeVisibleParameterAnnotations(attr) if !attr.parameter_annotations.is_empty())
    }));
    assert!(annotated_method.attributes.iter().any(|attribute| {
        matches!(attribute, AttributeInfo::RuntimeInvisibleParameterAnnotations(attr) if !attr.parameter_annotations.is_empty())
    }));
    assert_eq!(
        write_class(&parameter_annotations_class)?,
        parameter_annotations_bytes
    );

    let visible_annotation_payload = annotation_attribute_payload();
    let invisible_annotation_payload = simple_annotation_attribute_payload(9);
    let raw = minimal_classfile_with_options(MinimalClassfileOptions {
        extra_cp_bytes: [
            utf8_entry_bytes("RuntimeVisibleAnnotations"),
            utf8_entry_bytes("RuntimeInvisibleAnnotations"),
            utf8_entry_bytes("LVisible;"),
            utf8_entry_bytes("LNested;"),
            utf8_entry_bytes("LInvisible;"),
            utf8_entry_bytes("text"),
            utf8_entry_bytes("hello"),
            utf8_entry_bytes("nested"),
            utf8_entry_bytes("items"),
            utf8_entry_bytes("Ljava/lang/String;"),
            utf8_entry_bytes("LEnumType;"),
            utf8_entry_bytes("FOO"),
        ]
        .concat(),
        extra_cp_count: 12,
        class_attrs_count: 2,
        class_attrs_bytes: [
            make_attribute_blob(5, &visible_annotation_payload),
            make_attribute_blob(6, &invisible_annotation_payload),
        ]
        .concat(),
        ..MinimalClassfileOptions::default()
    });
    let parsed = parse_class(&raw)?;
    assert!(matches!(
        &parsed.attributes[0],
        AttributeInfo::RuntimeVisibleAnnotations(attr)
            if matches!(
                &attr.annotations[0].element_value_pairs[1].element_value,
                pytecode_engine::raw::ElementValueInfo::Annotation(_)
            )
    ));
    assert!(matches!(
        &parsed.attributes[1],
        AttributeInfo::RuntimeInvisibleAnnotations(attr) if attr.annotations.len() == 1
    ));
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

#[test]
fn annotation_default_roundtrips_as_typed_attr() -> TestResult<()> {
    let raw = minimal_classfile_with_options(MinimalClassfileOptions {
        extra_cp_bytes: [
            utf8_entry_bytes("value"),
            utf8_entry_bytes("()Ljava/lang/String;"),
            utf8_entry_bytes("AnnotationDefault"),
            utf8_entry_bytes("fallback"),
        ]
        .concat(),
        extra_cp_count: 4,
        methods_count: 1,
        methods_bytes: method_info_blob(0x0001, 5, 6, &[make_attribute_blob(7, &[b's', 0, 8])]),
        ..MinimalClassfileOptions::default()
    });
    let parsed = parse_class(&raw)?;
    assert!(matches!(
        parsed.methods[0].attributes.as_slice(),
        [AttributeInfo::AnnotationDefault(_)]
    ));
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

#[test]
fn malformed_annotation_payload_returns_structured_error() {
    let mut annotation_payload = Vec::new();
    annotation_payload.extend_from_slice(&u2(1));
    annotation_payload.extend_from_slice(&u2(6));
    annotation_payload.extend_from_slice(&u2(1));
    annotation_payload.extend_from_slice(&u2(7));
    annotation_payload.push(b'!');

    let raw = minimal_classfile_with_options(MinimalClassfileOptions {
        extra_cp_bytes: [
            utf8_entry_bytes("RuntimeVisibleAnnotations"),
            utf8_entry_bytes("LBroken;"),
            utf8_entry_bytes("value"),
        ]
        .concat(),
        extra_cp_count: 3,
        class_attrs_count: 1,
        class_attrs_bytes: make_attribute_blob(5, &annotation_payload),
        ..MinimalClassfileOptions::default()
    });

    let err = parse_class(&raw).unwrap_err();
    assert!(matches!(err.kind, EngineErrorKind::InvalidAttribute { .. }));
    assert!(err.to_string().contains("element value tag"));
}

#[test]
fn fixtures_and_constructed_inputs_expose_typed_type_annotation_attrs() -> TestResult<()> {
    let type_annotation_path = rust_compiled_fixture_paths_for("TypeAnnotationShowcase.java")?
        .into_iter()
        .find(|path| {
            path.file_name().and_then(|name| name.to_str()) == Some("TypeAnnotationShowcase.class")
        })
        .ok_or("TypeAnnotationShowcase.class not found")?;
    let type_annotation_bytes = std::fs::read(&type_annotation_path)?;
    let type_annotation_class = parse_class(&type_annotation_bytes)?;
    let all_attrs: Vec<_> = type_annotation_class
        .attributes
        .iter()
        .chain(
            type_annotation_class
                .fields
                .iter()
                .flat_map(|field| field.attributes.iter()),
        )
        .chain(
            type_annotation_class
                .methods
                .iter()
                .flat_map(|method| method.attributes.iter()),
        )
        .collect();
    assert!(all_attrs.iter().copied().any(|attribute| {
        matches!(attribute, AttributeInfo::RuntimeVisibleTypeAnnotations(attr) if !attr.annotations.is_empty())
    }));
    assert!(all_attrs.iter().copied().any(|attribute| {
        matches!(attribute, AttributeInfo::RuntimeInvisibleTypeAnnotations(attr) if !attr.annotations.is_empty())
    }));
    assert_eq!(write_class(&type_annotation_class)?, type_annotation_bytes);

    let visible_type_annotation_payload = type_annotation_attribute_payload();
    let invisible_type_annotation_payload = simple_type_annotation_attribute_payload(8);
    let raw = minimal_classfile_with_options(MinimalClassfileOptions {
        extra_cp_bytes: [
            utf8_entry_bytes("RuntimeVisibleTypeAnnotations"),
            utf8_entry_bytes("RuntimeInvisibleTypeAnnotations"),
            utf8_entry_bytes("LVisibleTypeUse;"),
            utf8_entry_bytes("LInvisibleTypeUse;"),
        ]
        .concat(),
        extra_cp_count: 4,
        class_attrs_count: 2,
        class_attrs_bytes: [
            make_attribute_blob(5, &visible_type_annotation_payload),
            make_attribute_blob(6, &invisible_type_annotation_payload),
        ]
        .concat(),
        ..MinimalClassfileOptions::default()
    });
    let parsed = parse_class(&raw)?;
    assert!(matches!(
        &parsed.attributes[0],
        AttributeInfo::RuntimeVisibleTypeAnnotations(attr)
            if attr.annotations.len() == 10
                && attr.annotations.iter().any(|annotation| matches!(
                    &annotation.target_info,
                    pytecode_engine::raw::TargetInfo::Localvar { table } if table.len() == 1
                ))
                && attr.annotations.iter().any(|annotation| matches!(
                    &annotation.target_info,
                    pytecode_engine::raw::TargetInfo::TypeArgument {
                        offset: 5,
                        type_argument_index: 1,
                    }
                ))
    ));
    assert!(matches!(
        &parsed.attributes[1],
        AttributeInfo::RuntimeInvisibleTypeAnnotations(attr)
            if matches!(
                attr.annotations.as_slice(),
                [pytecode_engine::raw::TypeAnnotationInfo {
                    target_type: pytecode_engine::constants::TargetType::ReturnOrObjectType,
                    ..
                }]
            )
    ));
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

#[test]
fn malformed_type_annotation_payload_returns_structured_error() {
    let mut type_annotation_payload = Vec::new();
    type_annotation_payload.extend_from_slice(&u2(1));
    type_annotation_payload.push(0x13);
    type_annotation_payload.push(1);
    type_annotation_payload.push(9);
    type_annotation_payload.push(0);
    type_annotation_payload.extend_from_slice(&u2(6));
    type_annotation_payload.extend_from_slice(&u2(0));

    let raw = minimal_classfile_with_options(MinimalClassfileOptions {
        extra_cp_bytes: [
            utf8_entry_bytes("RuntimeVisibleTypeAnnotations"),
            utf8_entry_bytes("LBrokenTypeUse;"),
        ]
        .concat(),
        extra_cp_count: 2,
        class_attrs_count: 1,
        class_attrs_bytes: make_attribute_blob(5, &type_annotation_payload),
        ..MinimalClassfileOptions::default()
    });

    let err = parse_class(&raw).unwrap_err();
    assert!(matches!(err.kind, EngineErrorKind::InvalidAttribute { .. }));
    assert!(err.to_string().contains("type path kind"));
}

#[test]
fn fixtures_and_constructed_inputs_expose_typed_module_record_bootstrap_attrs() -> TestResult<()> {
    let lambda_path = rust_compiled_fixture_paths_for("LambdaShowcase.java")?
        .into_iter()
        .find(|path| {
            path.file_name().and_then(|name| name.to_str()) == Some("LambdaShowcase.class")
        })
        .ok_or("LambdaShowcase.class not found")?;
    let lambda_bytes = std::fs::read(&lambda_path)?;
    let lambda_class = parse_class(&lambda_bytes)?;
    assert!(lambda_class.attributes.iter().any(|attribute| {
        matches!(attribute, AttributeInfo::BootstrapMethods(attr) if !attr.bootstrap_methods.is_empty())
    }));
    assert_eq!(write_class(&lambda_class)?, lambda_bytes);

    let record_path = rust_compiled_fixture_paths_for("RecordClass.java")?
        .into_iter()
        .find(|path| {
            path.file_name().and_then(|name| name.to_str()) == Some("RecordClass$NamedValue.class")
        })
        .ok_or("RecordClass$NamedValue.class not found")?;
    let record_bytes = std::fs::read(&record_path)?;
    let record_class = parse_class(&record_bytes)?;
    assert!(record_class.attributes.iter().any(|attribute| {
        matches!(attribute, AttributeInfo::Record(attr)
        if !attr.components.is_empty()
            && attr.components.iter().any(|component| {
                component
                    .attributes
                    .iter()
                    .any(|nested| matches!(nested, AttributeInfo::Signature(_)))
            }))
    }));
    assert_eq!(write_class(&record_class)?, record_bytes);

    let sealed_shape_path = rust_compiled_fixture_paths_for("SealedHierarchy.java")?
        .into_iter()
        .find(|path| {
            path.file_name().and_then(|name| name.to_str()) == Some("SealedHierarchy$Shape.class")
        })
        .ok_or("SealedHierarchy$Shape.class not found")?;
    let sealed_shape_bytes = std::fs::read(&sealed_shape_path)?;
    let sealed_shape_class = parse_class(&sealed_shape_bytes)?;
    assert!(sealed_shape_class.attributes.iter().any(|attribute| {
        matches!(attribute, AttributeInfo::PermittedSubclasses(attr) if attr.classes.len() == 3)
    }));
    assert_eq!(write_class(&sealed_shape_class)?, sealed_shape_bytes);

    let raw = minimal_classfile_with_options(MinimalClassfileOptions {
        major_version: 53,
        access_flags: 0x8000,
        super_class: 0,
        extra_cp_bytes: [
            utf8_entry_bytes("Module"),
            utf8_entry_bytes("ModulePackages"),
            utf8_entry_bytes("ModuleMainClass"),
            utf8_entry_bytes("test.module"),
            module_entry_bytes(8),
            utf8_entry_bytes("9.0"),
            utf8_entry_bytes("java.base"),
            module_entry_bytes(11),
            utf8_entry_bytes("test/pkg"),
            package_entry_bytes(13),
            utf8_entry_bytes("test/pkg/internal"),
            package_entry_bytes(15),
            utf8_entry_bytes("test/Main"),
            class_entry_bytes(17),
            utf8_entry_bytes("test/Service"),
            class_entry_bytes(19),
            utf8_entry_bytes("test/ServiceImpl"),
            class_entry_bytes(21),
        ]
        .concat(),
        extra_cp_count: 18,
        class_attrs_count: 3,
        class_attrs_bytes: [
            make_attribute_blob(5, &module_attribute_payload()),
            make_attribute_blob(6, &module_packages_attribute_payload()),
            make_attribute_blob(7, &module_main_class_attribute_payload()),
        ]
        .concat(),
        ..MinimalClassfileOptions::default()
    });
    let parsed = parse_class(&raw)?;
    assert!(matches!(
        &parsed.attributes[0],
        AttributeInfo::Module(attr)
            if attr.module.module_name_index == 9
                && attr.module.requires.len() == 1
                && attr.module.exports.len() == 1
                && attr.module.opens.len() == 1
                && attr.module.uses_index == vec![20]
                && attr.module.provides.len() == 1
    ));
    assert!(matches!(
        &parsed.attributes[1],
        AttributeInfo::ModulePackages(attr) if attr.package_index == vec![14, 16]
    ));
    assert!(matches!(
        &parsed.attributes[2],
        AttributeInfo::ModuleMainClass(attr) if attr.main_class_index == 18
    ));
    assert_eq!(write_class(&parsed)?, raw);
    Ok(())
}

#[test]
fn writer_roundtrip_all_java_resources() -> TestResult<()> {
    for class_path in rust_compiled_fixture_paths(25)? {
        let original = std::fs::read(&class_path)?;
        let parsed = parse_class(&original)?;
        let emitted = write_class(&parsed)?;
        assert_eq!(
            emitted,
            original,
            "roundtrip mismatch for {}",
            class_path.display()
        );
    }
    Ok(())
}

fn constant_pool_utf8(
    classfile: &pytecode_engine::raw::ClassFile,
    index: u16,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let entry = classfile
        .constant_pool
        .get(index as usize)
        .and_then(Option::as_ref)
        .ok_or("missing constant-pool entry")?;
    match entry {
        pytecode_engine::raw::ConstantPoolEntry::Utf8(info) => {
            Ok(decode_modified_utf8(&info.bytes)?)
        }
        _ => Err("constant-pool entry is not Utf8".into()),
    }
}

#[derive(Debug, Clone)]
struct MinimalClassfileOptions {
    minor_version: u16,
    major_version: u16,
    extra_cp_bytes: Vec<u8>,
    extra_cp_count: u16,
    access_flags: u16,
    this_class: u16,
    super_class: u16,
    interfaces: Vec<u16>,
    fields_count: u16,
    fields_bytes: Vec<u8>,
    methods_count: u16,
    methods_bytes: Vec<u8>,
    class_attrs_count: u16,
    class_attrs_bytes: Vec<u8>,
}

impl Default for MinimalClassfileOptions {
    fn default() -> Self {
        Self {
            minor_version: 0,
            major_version: 52,
            extra_cp_bytes: Vec::new(),
            extra_cp_count: 0,
            access_flags: 0x0021,
            this_class: 2,
            super_class: 4,
            interfaces: Vec::new(),
            fields_count: 0,
            fields_bytes: Vec::new(),
            methods_count: 0,
            methods_bytes: Vec::new(),
            class_attrs_count: 0,
            class_attrs_bytes: Vec::new(),
        }
    }
}

fn u2(value: u16) -> [u8; 2] {
    value.to_be_bytes()
}

fn u4(value: u32) -> [u8; 4] {
    value.to_be_bytes()
}

fn utf8_entry_bytes(value: &str) -> Vec<u8> {
    let encoded = encode_modified_utf8(value);
    let mut bytes = vec![1];
    bytes.extend_from_slice(&u2(encoded.len() as u16));
    bytes.extend_from_slice(&encoded);
    bytes
}

fn class_entry_bytes(name_index: u16) -> Vec<u8> {
    let mut bytes = vec![7];
    bytes.extend_from_slice(&u2(name_index));
    bytes
}

fn module_entry_bytes(name_index: u16) -> Vec<u8> {
    let mut bytes = vec![19];
    bytes.extend_from_slice(&u2(name_index));
    bytes
}

fn package_entry_bytes(name_index: u16) -> Vec<u8> {
    let mut bytes = vec![20];
    bytes.extend_from_slice(&u2(name_index));
    bytes
}

fn long_entry_bytes(high: u32, low: u32) -> Vec<u8> {
    let mut bytes = vec![5];
    bytes.extend_from_slice(&u4(high));
    bytes.extend_from_slice(&u4(low));
    bytes
}

fn make_attribute_blob(name_index: u16, payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&u2(name_index));
    bytes.extend_from_slice(&u4(payload.len() as u32));
    bytes.extend_from_slice(payload);
    bytes
}

fn minimal_classfile() -> Vec<u8> {
    minimal_classfile_with_version(52, 0)
}

fn minimal_classfile_with_version(major: u16, minor: u16) -> Vec<u8> {
    minimal_classfile_with_options(MinimalClassfileOptions {
        minor_version: minor,
        major_version: major,
        ..MinimalClassfileOptions::default()
    })
}

fn minimal_classfile_with_options(options: MinimalClassfileOptions) -> Vec<u8> {
    let mut base_cp = Vec::new();
    base_cp.extend_from_slice(&utf8_entry_bytes("TestClass"));
    base_cp.extend_from_slice(&class_entry_bytes(1));
    base_cp.extend_from_slice(&utf8_entry_bytes("java/lang/Object"));
    base_cp.extend_from_slice(&class_entry_bytes(3));

    let cp_count = 5_u16 + options.extra_cp_count;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&MAGIC.to_be_bytes());
    bytes.extend_from_slice(&u2(options.minor_version));
    bytes.extend_from_slice(&u2(options.major_version));
    bytes.extend_from_slice(&u2(cp_count));
    bytes.extend_from_slice(&base_cp);
    bytes.extend_from_slice(&options.extra_cp_bytes);
    bytes.extend_from_slice(&u2(options.access_flags));
    bytes.extend_from_slice(&u2(options.this_class));
    bytes.extend_from_slice(&u2(options.super_class));
    bytes.extend_from_slice(&u2(options.interfaces.len() as u16));
    for interface in &options.interfaces {
        bytes.extend_from_slice(&u2(*interface));
    }
    bytes.extend_from_slice(&u2(options.fields_count));
    bytes.extend_from_slice(&options.fields_bytes);
    bytes.extend_from_slice(&u2(options.methods_count));
    bytes.extend_from_slice(&options.methods_bytes);
    bytes.extend_from_slice(&u2(options.class_attrs_count));
    bytes.extend_from_slice(&options.class_attrs_bytes);
    bytes
}

fn method_info_blob(
    access_flags: u16,
    name_index: u16,
    descriptor_index: u16,
    attributes: &[Vec<u8>],
) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&u2(access_flags));
    bytes.extend_from_slice(&u2(name_index));
    bytes.extend_from_slice(&u2(descriptor_index));
    bytes.extend_from_slice(&u2(attributes.len() as u16));
    for attribute in attributes {
        bytes.extend_from_slice(attribute);
    }
    bytes
}

fn annotation_attribute_payload() -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend_from_slice(&u2(1));
    payload.extend_from_slice(&u2(7));
    payload.extend_from_slice(&u2(3));

    payload.extend_from_slice(&u2(10));
    payload.push(b's');
    payload.extend_from_slice(&u2(11));

    payload.extend_from_slice(&u2(12));
    payload.push(b'@');
    payload.extend_from_slice(&u2(8));
    payload.extend_from_slice(&u2(0));

    payload.extend_from_slice(&u2(13));
    payload.push(b'[');
    payload.extend_from_slice(&u2(2));
    payload.push(b'c');
    payload.extend_from_slice(&u2(14));
    payload.push(b'e');
    payload.extend_from_slice(&u2(15));
    payload.extend_from_slice(&u2(16));
    payload
}

fn simple_annotation_attribute_payload(type_index: u16) -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend_from_slice(&u2(1));
    payload.extend_from_slice(&u2(type_index));
    payload.extend_from_slice(&u2(0));
    payload
}

fn type_annotation_attribute_payload() -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend_from_slice(&u2(10));

    push_type_annotation(&mut payload, 0x00, &[0], &[], 7);
    push_type_annotation(&mut payload, 0x10, &u2(1), &[(0, 0)], 7);
    push_type_annotation(&mut payload, 0x11, &[0, 1], &[], 7);
    push_type_annotation(&mut payload, 0x13, &[], &[(1, 0)], 7);
    push_type_annotation(&mut payload, 0x16, &[0], &[], 7);
    push_type_annotation(&mut payload, 0x17, &u2(2), &[], 7);

    let localvar_target = [
        u2(1).to_vec(),
        u2(0).to_vec(),
        u2(1).to_vec(),
        u2(0).to_vec(),
    ]
    .concat();
    push_type_annotation(&mut payload, 0x40, &localvar_target, &[], 7);
    push_type_annotation(&mut payload, 0x42, &u2(3), &[], 7);
    push_type_annotation(&mut payload, 0x43, &u2(4), &[], 7);

    let type_argument_target = [u2(5).to_vec(), vec![1]].concat();
    push_type_annotation(&mut payload, 0x47, &type_argument_target, &[(3, 0)], 7);

    payload
}

fn simple_type_annotation_attribute_payload(type_index: u16) -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend_from_slice(&u2(1));
    push_type_annotation(&mut payload, 0x14, &[], &[], type_index);
    payload
}

fn push_type_annotation(
    payload: &mut Vec<u8>,
    target_type: u8,
    target_info: &[u8],
    path: &[(u8, u8)],
    type_index: u16,
) {
    payload.push(target_type);
    payload.extend_from_slice(target_info);
    payload.push(path.len() as u8);
    for (type_path_kind, type_argument_index) in path {
        payload.push(*type_path_kind);
        payload.push(*type_argument_index);
    }
    payload.extend_from_slice(&u2(type_index));
    payload.extend_from_slice(&u2(0));
}

fn module_attribute_payload() -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend_from_slice(&u2(9));
    payload.extend_from_slice(&u2(0x0020));
    payload.extend_from_slice(&u2(10));

    payload.extend_from_slice(&u2(1));
    payload.extend_from_slice(&u2(12));
    payload.extend_from_slice(&u2(0x0020));
    payload.extend_from_slice(&u2(10));

    payload.extend_from_slice(&u2(1));
    payload.extend_from_slice(&u2(14));
    payload.extend_from_slice(&u2(0x1000));
    payload.extend_from_slice(&u2(1));
    payload.extend_from_slice(&u2(12));

    payload.extend_from_slice(&u2(1));
    payload.extend_from_slice(&u2(16));
    payload.extend_from_slice(&u2(0x1000));
    payload.extend_from_slice(&u2(1));
    payload.extend_from_slice(&u2(12));

    payload.extend_from_slice(&u2(1));
    payload.extend_from_slice(&u2(20));

    payload.extend_from_slice(&u2(1));
    payload.extend_from_slice(&u2(20));
    payload.extend_from_slice(&u2(1));
    payload.extend_from_slice(&u2(22));
    payload
}

fn module_packages_attribute_payload() -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend_from_slice(&u2(2));
    payload.extend_from_slice(&u2(14));
    payload.extend_from_slice(&u2(16));
    payload
}

fn module_main_class_attribute_payload() -> Vec<u8> {
    u2(18).to_vec()
}
