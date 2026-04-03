use crate::classfile::constants::{
    MethodParameterAccessFlags, ModuleAccessFlags, ModuleExportsAccessFlags,
    ModuleOpensAccessFlags, ModuleRequiresAccessFlags, NestedClassAccessFlags, VerificationType,
};
use crate::classfile::instructions::Instruction;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttributeHeader {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExceptionInfo {
    pub start_pc: u16,
    pub end_pc: u16,
    pub handler_pc: u16,
    pub catch_type: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VerificationTypeInfo {
    Top,
    Integer,
    Float,
    Double,
    Long,
    Null,
    UninitializedThis,
    Object { cpool_index: u16 },
    Uninitialized { offset: u16 },
}

impl VerificationTypeInfo {
    pub const fn tag(&self) -> VerificationType {
        match self {
            Self::Top => VerificationType::Top,
            Self::Integer => VerificationType::Integer,
            Self::Float => VerificationType::Float,
            Self::Double => VerificationType::Double,
            Self::Long => VerificationType::Long,
            Self::Null => VerificationType::Null,
            Self::UninitializedThis => VerificationType::UninitializedThis,
            Self::Object { .. } => VerificationType::Object,
            Self::Uninitialized { .. } => VerificationType::Uninitialized,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StackMapFrameInfo {
    Same {
        frame_type: u8,
    },
    SameLocals1StackItem {
        frame_type: u8,
        stack: VerificationTypeInfo,
    },
    SameLocals1StackItemExtended {
        frame_type: u8,
        offset_delta: u16,
        stack: VerificationTypeInfo,
    },
    Chop {
        frame_type: u8,
        offset_delta: u16,
    },
    SameExtended {
        frame_type: u8,
        offset_delta: u16,
    },
    Append {
        frame_type: u8,
        offset_delta: u16,
        locals: Vec<VerificationTypeInfo>,
    },
    Full {
        frame_type: u8,
        offset_delta: u16,
        locals: Vec<VerificationTypeInfo>,
        stack: Vec<VerificationTypeInfo>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InnerClassInfo {
    pub inner_class_info_index: u16,
    pub outer_class_info_index: u16,
    pub inner_name_index: u16,
    pub inner_class_access_flags: NestedClassAccessFlags,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineNumberInfo {
    pub start_pc: u16,
    pub line_number: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalVariableInfo {
    pub start_pc: u16,
    pub length: u16,
    pub name_index: u16,
    pub descriptor_index: u16,
    pub index: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalVariableTypeInfo {
    pub start_pc: u16,
    pub length: u16,
    pub name_index: u16,
    pub signature_index: u16,
    pub index: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstValueInfo {
    pub const_value_index: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnumConstantValueInfo {
    pub type_name_index: u16,
    pub const_name_index: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassInfoValueInfo {
    pub class_info_index: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnnotationInfo {
    pub type_index: u16,
    pub element_value_pairs: Vec<ElementValuePairInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArrayValueInfo {
    pub values: Vec<ElementValueInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ElementValuePayload {
    ConstValue(ConstValueInfo),
    EnumConstant(EnumConstantValueInfo),
    ClassInfo(ClassInfoValueInfo),
    Annotation(Box<AnnotationInfo>),
    Array(ArrayValueInfo),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ElementValueInfo {
    pub tag: u8,
    pub value: ElementValuePayload,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ElementValuePairInfo {
    pub element_name_index: u16,
    pub element_value: ElementValueInfo,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParameterAnnotationInfo {
    pub annotations: Vec<AnnotationInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableInfo {
    pub start_pc: u16,
    pub length: u16,
    pub index: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TargetInfo {
    TypeParameter {
        type_parameter_index: u8,
    },
    Supertype {
        supertype_index: u16,
    },
    TypeParameterBound {
        type_parameter_index: u8,
        bound_index: u8,
    },
    Empty,
    FormalParameter {
        formal_parameter_index: u8,
    },
    Throws {
        throws_type_index: u16,
    },
    Localvar {
        table: Vec<TableInfo>,
    },
    Catch {
        exception_table_index: u16,
    },
    Offset {
        offset: u16,
    },
    TypeArgument {
        offset: u16,
        type_argument_index: u8,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathInfo {
    pub type_path_kind: u8,
    pub type_argument_index: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypePathInfo {
    pub path: Vec<PathInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeAnnotationInfo {
    pub target_type: u8,
    pub target_info: TargetInfo,
    pub target_path: TypePathInfo,
    pub type_index: u16,
    pub element_value_pairs: Vec<ElementValuePairInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootstrapMethodInfo {
    pub bootstrap_method_ref: u16,
    pub bootstrap_arguments: Vec<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MethodParameterInfo {
    pub name_index: u16,
    pub access_flags: MethodParameterAccessFlags,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequiresInfo {
    pub requires_index: u16,
    pub requires_flag: ModuleRequiresAccessFlags,
    pub requires_version_index: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportInfo {
    pub exports_index: u16,
    pub exports_flags: ModuleExportsAccessFlags,
    pub exports_to_index: Vec<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpensInfo {
    pub opens_index: u16,
    pub opens_flags: ModuleOpensAccessFlags,
    pub opens_to_index: Vec<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProvidesInfo {
    pub provides_index: u16,
    pub provides_with_index: Vec<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordComponentInfo {
    pub name_index: u16,
    pub descriptor_index: u16,
    pub attributes: Vec<AttributeInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AttributeBody {
    Unimplemented {
        info: Vec<u8>,
        attr_name: String,
    },
    ConstantValue {
        constantvalue_index: u16,
    },
    Code {
        max_stacks: u16,
        max_locals: u16,
        code: Vec<Instruction>,
        exception_table: Vec<ExceptionInfo>,
        attributes: Vec<AttributeInfo>,
    },
    StackMapTable {
        entries: Vec<StackMapFrameInfo>,
    },
    Exceptions {
        exception_index_table: Vec<u16>,
    },
    InnerClasses {
        classes: Vec<InnerClassInfo>,
    },
    EnclosingMethod {
        class_index: u16,
        method_index: u16,
    },
    Synthetic,
    Signature {
        signature_index: u16,
    },
    SourceFile {
        sourcefile_index: u16,
    },
    SourceDebugExtension {
        debug_extension: String,
    },
    LineNumberTable {
        line_number_table: Vec<LineNumberInfo>,
    },
    LocalVariableTable {
        local_variable_table: Vec<LocalVariableInfo>,
    },
    LocalVariableTypeTable {
        local_variable_type_table: Vec<LocalVariableTypeInfo>,
    },
    Deprecated,
    RuntimeVisibleAnnotations {
        annotations: Vec<AnnotationInfo>,
    },
    RuntimeInvisibleAnnotations {
        annotations: Vec<AnnotationInfo>,
    },
    RuntimeVisibleParameterAnnotations {
        parameter_annotations: Vec<ParameterAnnotationInfo>,
    },
    RuntimeInvisibleParameterAnnotations {
        parameter_annotations: Vec<ParameterAnnotationInfo>,
    },
    RuntimeVisibleTypeAnnotations {
        annotations: Vec<TypeAnnotationInfo>,
    },
    RuntimeInvisibleTypeAnnotations {
        annotations: Vec<TypeAnnotationInfo>,
    },
    AnnotationDefault {
        default_value: ElementValueInfo,
    },
    BootstrapMethods {
        bootstrap_methods: Vec<BootstrapMethodInfo>,
    },
    MethodParameters {
        parameters: Vec<MethodParameterInfo>,
    },
    Module {
        module_name_index: u16,
        module_flags: ModuleAccessFlags,
        module_version_index: u16,
        requires: Vec<RequiresInfo>,
        exports: Vec<ExportInfo>,
        opens: Vec<OpensInfo>,
        uses_index: Vec<u16>,
        provides: Vec<ProvidesInfo>,
    },
    ModulePackages {
        package_index: Vec<u16>,
    },
    ModuleMainClass {
        main_class_index: u16,
    },
    NestHost {
        host_class_index: u16,
    },
    NestMembers {
        classes: Vec<u16>,
    },
    Record {
        components: Vec<RecordComponentInfo>,
    },
    PermittedSubclasses {
        classes: Vec<u16>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttributeInfo {
    pub header: AttributeHeader,
    pub body: AttributeBody,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verification_type_tags_match_variant() {
        assert_eq!(VerificationTypeInfo::Null.tag(), VerificationType::Null);
        assert_eq!(
            VerificationTypeInfo::Object { cpool_index: 7 }.tag(),
            VerificationType::Object
        );
    }

    #[test]
    fn code_attribute_can_embed_nested_attributes() {
        let attr = AttributeInfo {
            header: AttributeHeader {
                attribute_name_index: 1,
                attribute_length: 0,
            },
            body: AttributeBody::Code {
                max_stacks: 1,
                max_locals: 1,
                code: Vec::new(),
                exception_table: Vec::new(),
                attributes: vec![AttributeInfo {
                    header: AttributeHeader {
                        attribute_name_index: 2,
                        attribute_length: 0,
                    },
                    body: AttributeBody::Synthetic,
                }],
            },
        };
        match attr.body {
            AttributeBody::Code { attributes, .. } => assert_eq!(attributes.len(), 1),
            _ => panic!("expected code attribute"),
        }
    }
}
