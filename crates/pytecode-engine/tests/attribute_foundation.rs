use pytecode_engine::constants::{
    MethodParameterAccessFlag, ModuleAccessFlag, ModuleExportsAccessFlag, ModuleOpensAccessFlag,
    ModuleRequiresAccessFlag, NestedClassAccessFlag, TargetInfoType, TargetType, TypePathKind,
    VerificationType,
};
use pytecode_engine::indexes::*;
use pytecode_engine::raw::{
    AnnotationInfo, ElementValueInfo, ElementValuePairInfo, ElementValueTag, PathInfo,
    StackMapFrameInfo, TableInfo, TargetInfo, TypeAnnotationInfo, TypePathInfo,
    VerificationTypeInfo,
};

#[test]
fn foundation_constants_match_python_values() {
    assert_eq!(NestedClassAccessFlag::ANNOTATION.bits(), 0x2000);
    assert_eq!(MethodParameterAccessFlag::MANDATED.bits(), 0x8000);
    assert_eq!(ModuleAccessFlag::OPEN.bits(), 0x0020);
    assert_eq!(ModuleRequiresAccessFlag::STATIC_PHASE.bits(), 0x0040);
    assert_eq!(ModuleExportsAccessFlag::MANDATED.bits(), 0x8000);
    assert_eq!(ModuleOpensAccessFlag::SYNTHETIC.bits(), 0x1000);
    assert_eq!(VerificationType::Object as u8, 7);
    assert_eq!(TargetType::TypeGenericMethodIdentifier as u8, 0x4B);
    assert_eq!(TypePathKind::ParameterizedType as u8, 3);
}

#[test]
fn target_info_type_groups_match_target_types() {
    assert_eq!(
        TargetInfoType::from_target_type(TargetType::TypeParameterGenericMethodOrConstructor),
        TargetInfoType::TypeParameter
    );
    assert_eq!(
        TargetInfoType::from_target_type(TargetType::TypeResourceVariable),
        TargetInfoType::Localvar
    );
    assert_eq!(
        TargetInfoType::from_target_type(TargetType::TypeMethodIdentifier),
        TargetInfoType::Offset
    );
    assert_eq!(
        TargetInfoType::from_target_type(TargetType::TypeCast),
        TargetInfoType::TypeArgument
    );
    assert!(TargetInfoType::Empty.matches_target_type(TargetType::ReturnOrObjectType));
    assert!(!TargetInfoType::Empty.matches_target_type(TargetType::TypeThrows));
}

#[test]
fn shared_raw_support_types_preserve_discriminants() {
    let object = VerificationTypeInfo::Object {
        cpool_index: ClassIndex::from(9),
    };
    assert_eq!(object.tag(), VerificationType::Object);

    let frame = StackMapFrameInfo::Append {
        frame_type: 252,
        offset_delta: 5,
        locals: vec![object.clone()],
    };
    assert_eq!(frame.frame_type(), 252);

    let element_value = ElementValueInfo::Array {
        values: vec![
            ElementValueInfo::Const {
                tag: ElementValueTag::String,
                const_value_index: 7.into(),
            },
            ElementValueInfo::Annotation(AnnotationInfo {
                type_index: 12.into(),
                element_value_pairs: vec![ElementValuePairInfo {
                    element_name_index: 13.into(),
                    element_value: ElementValueInfo::Class {
                        class_info_index: 14.into(),
                    },
                }],
            }),
        ],
    };
    assert_eq!(element_value.tag(), ElementValueTag::Array);

    let target_info = TargetInfo::TypeArgument {
        offset: 11,
        type_argument_index: 2,
    };
    assert_eq!(target_info.target_info_type(), TargetInfoType::TypeArgument);

    let type_annotation = TypeAnnotationInfo {
        target_type: TargetType::TypeGenericMethod,
        target_info,
        target_path: TypePathInfo {
            path: vec![PathInfo {
                type_path_kind: TypePathKind::NestedType,
                type_argument_index: 0,
            }],
        },
        type_index: 20.into(),
        element_value_pairs: vec![],
    };
    assert_eq!(
        TargetInfoType::from_target_type(type_annotation.target_type),
        type_annotation.target_info.target_info_type()
    );

    let table = TableInfo {
        start_pc: 1,
        length: 2,
        index: 3,
    };
    let localvar_target = TargetInfo::Localvar { table: vec![table] };
    assert_eq!(localvar_target.target_info_type(), TargetInfoType::Localvar);
}
