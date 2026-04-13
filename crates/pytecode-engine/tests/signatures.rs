use pytecode_engine::descriptors::BaseType;
use pytecode_engine::signatures::{
    ArrayTypeSignature, ClassSignature, ReferenceTypeSignature, ResultSignature, ThrowsSignature,
    TypeArgument, TypeParameter, TypeSignature, TypeVariableSignature, is_valid_class_signature,
    is_valid_field_signature, is_valid_method_signature, is_valid_type_signature,
    parse_class_signature, parse_field_signature, parse_method_signature, parse_type_signature,
};

#[test]
fn parse_class_signature_supports_type_parameters_and_interfaces() {
    let signature = parse_class_signature(
        "<T:Ljava/lang/Object;>Ljava/lang/Object;Ljava/lang/Comparable<TT;>;",
    )
    .unwrap();
    assert_eq!(
        signature,
        ClassSignature {
            type_parameters: vec![TypeParameter {
                identifier: "T".to_owned(),
                class_bound: Some(ReferenceTypeSignature::Class(
                    pytecode_engine::signatures::ClassTypeSignature {
                        package_specifier: vec!["java".to_owned(), "lang".to_owned()],
                        simple_class: pytecode_engine::signatures::SimpleClassTypeSignature {
                            identifier: "Object".to_owned(),
                            type_arguments: Vec::new(),
                        },
                        suffixes: Vec::new(),
                    },
                )),
                interface_bounds: Vec::new(),
            }],
            superclass_signature: pytecode_engine::signatures::ClassTypeSignature {
                package_specifier: vec!["java".to_owned(), "lang".to_owned()],
                simple_class: pytecode_engine::signatures::SimpleClassTypeSignature {
                    identifier: "Object".to_owned(),
                    type_arguments: Vec::new(),
                },
                suffixes: Vec::new(),
            },
            superinterface_signatures: vec![pytecode_engine::signatures::ClassTypeSignature {
                package_specifier: vec!["java".to_owned(), "lang".to_owned()],
                simple_class: pytecode_engine::signatures::SimpleClassTypeSignature {
                    identifier: "Comparable".to_owned(),
                    type_arguments: vec![TypeArgument::Exact(
                        ReferenceTypeSignature::TypeVariable(TypeVariableSignature {
                            identifier: "T".to_owned(),
                        }),
                    )],
                },
                suffixes: Vec::new(),
            }],
        }
    );
}

#[test]
fn parse_method_signature_supports_generics_and_throws() {
    let signature = parse_method_signature(
        "<T:Ljava/lang/Object;>(Ljava/util/List<TT;>;[I)TT;^Ljava/io/IOException;^TT;",
    )
    .unwrap();
    assert_eq!(signature.type_parameters.len(), 1);
    assert_eq!(signature.parameter_types.len(), 2);
    assert!(matches!(
        signature.parameter_types[0],
        TypeSignature::Reference(ReferenceTypeSignature::Class(_))
    ));
    assert_eq!(
        signature.result,
        ResultSignature::Type(TypeSignature::Reference(
            ReferenceTypeSignature::TypeVariable(TypeVariableSignature {
                identifier: "T".to_owned(),
            })
        ))
    );
    assert_eq!(
        signature.throws_signatures,
        vec![
            ThrowsSignature::Class(pytecode_engine::signatures::ClassTypeSignature {
                package_specifier: vec!["java".to_owned(), "io".to_owned()],
                simple_class: pytecode_engine::signatures::SimpleClassTypeSignature {
                    identifier: "IOException".to_owned(),
                    type_arguments: Vec::new(),
                },
                suffixes: Vec::new(),
            }),
            ThrowsSignature::TypeVariable(TypeVariableSignature {
                identifier: "T".to_owned(),
            }),
        ]
    );
}

#[test]
fn parse_field_and_type_signatures_support_wildcards_and_arrays() {
    let field_signature = parse_field_signature("Ljava/util/List<+Ljava/lang/Number;>;").unwrap();
    assert!(matches!(
        field_signature,
        ReferenceTypeSignature::Class(pytecode_engine::signatures::ClassTypeSignature {
            simple_class: pytecode_engine::signatures::SimpleClassTypeSignature { .. },
            ..
        })
    ));

    let type_signature = parse_type_signature("[TT;").unwrap();
    assert_eq!(
        type_signature,
        TypeSignature::Reference(ReferenceTypeSignature::Array(ArrayTypeSignature {
            component_type: Box::new(TypeSignature::Reference(
                ReferenceTypeSignature::TypeVariable(TypeVariableSignature {
                    identifier: "T".to_owned(),
                })
            )),
        }))
    );
}

#[test]
fn signature_validation_rejects_invalid_forms() {
    assert!(is_valid_class_signature(
        "<T:Ljava/lang/Object;>Ljava/lang/Object;Ljava/lang/Comparable<TT;>;"
    ));
    assert!(is_valid_method_signature("()V"));
    assert!(is_valid_field_signature(
        "Ljava/util/List<Ljava/lang/String;>;"
    ));
    assert!(is_valid_type_signature("[TT;"));

    assert!(!is_valid_field_signature("I"));
    assert!(!is_valid_class_signature("<>Ljava/lang/Object;"));
    assert!(!is_valid_method_signature("<T:>(TT;)"));
    assert!(
        parse_type_signature("Lpkg/Outer<;")
            .unwrap_err()
            .to_string()
            .contains("invalid")
    );

    let base = parse_type_signature("I").unwrap();
    assert_eq!(base, TypeSignature::Base(BaseType::Int));
}
