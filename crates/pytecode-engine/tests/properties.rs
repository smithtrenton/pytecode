use proptest::prelude::*;
use pytecode_engine::constants::MethodAccessFlags;
use pytecode_engine::descriptors::{parse_field_descriptor, to_descriptor_field};
use pytecode_engine::model::{ClassModel, CodeModel, ConstantPoolBuilder, MethodModel};
use pytecode_engine::modified_utf8::JavaString;
use pytecode_engine::{parse_class, write_class};

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
}
