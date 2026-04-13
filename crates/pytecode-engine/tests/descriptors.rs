use pytecode_engine::descriptors::{
    ArrayType, BaseType, FieldDescriptor, MethodDescriptor, ObjectType, ReturnType, VOID,
    is_valid_field_descriptor, is_valid_method_descriptor, parameter_slot_count,
    parse_field_descriptor, parse_method_descriptor, to_descriptor_field, to_descriptor_method,
};

#[test]
fn parse_field_descriptor_supports_base_object_and_array_forms() {
    assert_eq!(
        parse_field_descriptor("Ljava/lang/String;").unwrap(),
        FieldDescriptor::Object(ObjectType {
            class_name: "java/lang/String".to_owned(),
        })
    );
    assert_eq!(
        parse_field_descriptor("[[I").unwrap(),
        FieldDescriptor::Array(ArrayType {
            component_type: Box::new(FieldDescriptor::Array(ArrayType {
                component_type: Box::new(FieldDescriptor::Base(BaseType::Int)),
            })),
        })
    );
}

#[test]
fn parse_method_descriptor_supports_multiple_parameters() {
    let descriptor = parse_method_descriptor("(IDLjava/lang/Thread;)Ljava/lang/Object;").unwrap();
    assert_eq!(
        descriptor,
        MethodDescriptor {
            parameter_types: vec![
                FieldDescriptor::Base(BaseType::Int),
                FieldDescriptor::Base(BaseType::Double),
                FieldDescriptor::Object(ObjectType {
                    class_name: "java/lang/Thread".to_owned(),
                }),
            ],
            return_type: ReturnType::Field(FieldDescriptor::Object(ObjectType {
                class_name: "java/lang/Object".to_owned(),
            })),
        }
    );
    assert_eq!(parameter_slot_count(&descriptor), 4);
}

#[test]
fn descriptor_formatting_round_trips() {
    let field = FieldDescriptor::Array(ArrayType {
        component_type: Box::new(FieldDescriptor::Object(ObjectType {
            class_name: "java/lang/String".to_owned(),
        })),
    });
    assert_eq!(to_descriptor_field(&field), "[Ljava/lang/String;");

    let method = MethodDescriptor {
        parameter_types: vec![FieldDescriptor::Base(BaseType::Int), field.clone()],
        return_type: ReturnType::Void,
    };
    assert_eq!(to_descriptor_method(&method), "(I[Ljava/lang/String;)V");
    assert_eq!(
        parse_method_descriptor("(I[Ljava/lang/String;)V").unwrap(),
        method
    );
}

#[test]
fn descriptor_validation_rejects_invalid_forms() {
    assert!(is_valid_field_descriptor("Ljava/lang/String;"));
    assert!(is_valid_method_descriptor("()V"));
    assert!(!is_valid_field_descriptor("Ljava.lang.String;"));
    assert!(!is_valid_method_descriptor("()VX"));
    assert!(
        parse_field_descriptor("L;")
            .unwrap_err()
            .to_string()
            .contains("empty class name")
    );
    let _ = VOID;
}
