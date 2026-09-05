use proptest::prelude::*;
use pytecode_engine::constants::MethodAccessFlags;
use pytecode_engine::descriptors::{parse_field_descriptor, to_descriptor_field};
use pytecode_engine::model::{ClassModel, CodeModel, ConstantPoolBuilder, MethodModel};
use pytecode_engine::modified_utf8::JavaString;
use pytecode_engine::signatures::{
    ArrayTypeSignature, ClassTypeSignature, ReferenceTypeSignature, ResultSignature,
    SimpleClassTypeSignature, TypeArgument, TypeSignature, TypeVariableSignature,
    parse_class_signature, parse_field_signature, parse_method_signature,
};
use pytecode_engine::{parse_class, write_class};

// Generate grammar text and its expected tree independently of the parser.
fn reference_signature() -> impl Strategy<Value = (String, ReferenceTypeSignature)> {
    let leaf = "[A-Z][a-zA-Z0-9]{0,12}".prop_map(|name| {
        (
            format!("T{name};"),
            ReferenceTypeSignature::TypeVariable(TypeVariableSignature { identifier: name }),
        )
    });
    leaf.prop_recursive(4, 64, 6, |inner| {
        prop_oneof![
            inner.clone().prop_map(|(text, tree)| (
                format!("[{text}"),
                ReferenceTypeSignature::Array(ArrayTypeSignature {
                    component_type: Box::new(TypeSignature::Reference(tree))
                })
            )),
            prop::collection::vec((0..4u8, inner), 1..5).prop_map(|arguments| {
                let mut text = String::from("Lpackage/Outer<");
                let mut type_arguments = Vec::new();
                for (kind, (argument, tree)) in arguments {
                    let value = match kind {
                        0 => {
                            text.push('*');
                            TypeArgument::Any
                        }
                        1 => {
                            text.push_str(&argument);
                            TypeArgument::Exact(tree)
                        }
                        2 => {
                            text.push('+');
                            text.push_str(&argument);
                            TypeArgument::Extends(tree)
                        }
                        _ => {
                            text.push('-');
                            text.push_str(&argument);
                            TypeArgument::Super(tree)
                        }
                    };
                    type_arguments.push(value);
                }
                text.push_str(">.Inner;");
                (
                    text,
                    ReferenceTypeSignature::Class(ClassTypeSignature {
                        package_specifier: vec!["package".into()],
                        simple_class: SimpleClassTypeSignature {
                            identifier: "Outer".into(),
                            type_arguments,
                        },
                        suffixes: vec![SimpleClassTypeSignature {
                            identifier: "Inner".into(),
                            type_arguments: vec![],
                        }],
                    }),
                )
            }),
        ]
    })
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 128,
        failure_persistence: Some(Box::new(proptest::test_runner::FileFailurePersistence::WithSource("proptest-regressions"))),
        ..ProptestConfig::default()
    })]

    #[test]
    fn arbitrary_utf16_roundtrips(units in prop::collection::vec(any::<u16>(), 0..2048)) {
        let value = JavaString::from_utf16(units.clone());
        let decoded = JavaString::from_modified_utf8(&value.to_modified_utf8()).unwrap();
        prop_assert_eq!(decoded.as_utf16(), units);
    }

    #[test]
    fn generated_descriptors_roundtrip(dimensions in 0..=255usize, name in "[a-zA-Z][a-zA-Z0-9]{0,30}") {
        let descriptor = format!("{}Lpackage/{name};", "[".repeat(dimensions));
        let parsed = parse_field_descriptor(&descriptor).unwrap();
        prop_assert_eq!(&to_descriptor_field(&parsed), &descriptor);
        prop_assert!(parse_field_descriptor(&(descriptor + "X")).is_err());
    }

    #[test]
    fn bounded_code_lift_lower_is_stable(value in any::<i16>()) {
        let bytes = [0x11, (value >> 8) as u8, value as u8, 0xac];
        let pool = ConstantPoolBuilder::new();
        let code = CodeModel::from_raw_code(1, 0, &bytes, &pool).unwrap();
        let mut model = ClassModel {
            version: (52, 0),
            name: "PropertyCase".into(),
            super_name: Some("java/lang/Object".into()),
            ..ClassModel::default()
        };
        model.methods.push(MethodModel::new(MethodAccessFlags::PUBLIC | MethodAccessFlags::STATIC,
            "value".into(), "()I".into(), Some(code), vec![]));
        let classfile = model.to_classfile().unwrap();
        let emitted = write_class(&classfile).unwrap();
        prop_assert_eq!(&write_class(&parse_class(&emitted).unwrap()).unwrap(), &emitted);
        let lifted = ClassModel::from_bytes(&emitted).unwrap();
        prop_assert_eq!(write_class(&lifted.to_classfile().unwrap()).unwrap(), emitted);
    }

    #[test]
    fn generated_generic_signatures_match_their_trees((text, tree) in reference_signature()) {
        prop_assert_eq!(parse_field_signature(&text).unwrap(), tree.clone());
        let trailing = format!("{text}!");
        prop_assert!(parse_field_signature(&trailing).is_err());
        let method = parse_method_signature(&format!("<T:{text}>(I{text}){text}^TT;^Ljava/lang/Exception;")).unwrap();
        prop_assert_eq!(&method.type_parameters[0].class_bound, &Some(tree.clone()));
        prop_assert_eq!(&method.parameter_types[1], &TypeSignature::Reference(tree.clone()));
        prop_assert_eq!(method.result, ResultSignature::Type(TypeSignature::Reference(tree.clone())));
        prop_assert_eq!(method.throws_signatures.len(), 2);
        let class = parse_class_signature(&format!("<T::{text}>Ljava/lang/Object;Ljava/lang/Comparable<TT;>;")).unwrap();
        prop_assert_eq!(&class.type_parameters[0].interface_bounds, &[tree]);
        prop_assert!(class.type_parameters[0].class_bound.is_none());
        prop_assert_eq!(class.superinterface_signatures.len(), 1);
    }
}
