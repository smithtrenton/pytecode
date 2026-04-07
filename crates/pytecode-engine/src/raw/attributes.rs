use crate::constants::{
    MethodParameterAccessFlag, ModuleAccessFlag, ModuleExportsAccessFlag, ModuleOpensAccessFlag,
    ModuleRequiresAccessFlag, NestedClassAccessFlag, TargetInfoType, TargetType, TypePathKind,
    VerificationType,
};
use crate::raw::instructions::Instruction;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstantValueAttribute {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
    pub constantvalue_index: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureAttribute {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
    pub signature_index: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFileAttribute {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
    pub sourcefile_index: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceDebugExtensionAttribute {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
    pub debug_extension: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExceptionsAttribute {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
    pub exception_index_table: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExceptionHandler {
    pub start_pc: u16,
    pub end_pc: u16,
    pub handler_pc: u16,
    pub catch_type: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeAttribute {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
    pub max_stack: u16,
    pub max_locals: u16,
    pub code_length: u32,
    pub code: Vec<Instruction>,
    pub exception_table: Vec<ExceptionHandler>,
    pub attributes: Vec<AttributeInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackMapTableAttribute {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
    pub entries: Vec<StackMapFrameInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntheticAttribute {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineNumberTableAttribute {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
    pub line_number_table: Vec<LineNumberInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalVariableTableAttribute {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
    pub local_variable_table: Vec<LocalVariableInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalVariableTypeTableAttribute {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
    pub local_variable_type_table: Vec<LocalVariableTypeInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeprecatedAttribute {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InnerClassesAttribute {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
    pub classes: Vec<InnerClassInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnclosingMethodAttribute {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
    pub class_index: u16,
    pub method_index: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodParametersAttribute {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
    pub parameters: Vec<MethodParameterInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NestHostAttribute {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
    pub host_class_index: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NestMembersAttribute {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
    pub classes: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeVisibleAnnotationsAttribute {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
    pub annotations: Vec<AnnotationInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeInvisibleAnnotationsAttribute {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
    pub annotations: Vec<AnnotationInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeVisibleParameterAnnotationsAttribute {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
    pub parameter_annotations: Vec<ParameterAnnotationInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeInvisibleParameterAnnotationsAttribute {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
    pub parameter_annotations: Vec<ParameterAnnotationInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnotationDefaultAttribute {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
    pub default_value: ElementValueInfo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeVisibleTypeAnnotationsAttribute {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
    pub annotations: Vec<TypeAnnotationInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeInvisibleTypeAnnotationsAttribute {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
    pub annotations: Vec<TypeAnnotationInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapMethodsAttribute {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
    pub bootstrap_methods: Vec<BootstrapMethodInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleAttribute {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
    pub module: ModuleInfo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModulePackagesAttribute {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
    pub package_index: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleMainClassAttribute {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
    pub main_class_index: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordAttribute {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
    pub components: Vec<RecordComponentInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermittedSubclassesAttribute {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
    pub classes: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
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

impl StackMapFrameInfo {
    pub const fn frame_type(&self) -> u8 {
        match self {
            Self::Same { frame_type }
            | Self::SameLocals1StackItem { frame_type, .. }
            | Self::SameLocals1StackItemExtended { frame_type, .. }
            | Self::Chop { frame_type, .. }
            | Self::SameExtended { frame_type, .. }
            | Self::Append { frame_type, .. }
            | Self::Full { frame_type, .. } => *frame_type,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ElementValueTag {
    Byte = b'B',
    Char = b'C',
    Double = b'D',
    Float = b'F',
    Int = b'I',
    Long = b'J',
    Short = b'S',
    Boolean = b'Z',
    String = b's',
    Enum = b'e',
    Class = b'c',
    Annotation = b'@',
    Array = b'[',
}

impl ElementValueTag {
    pub const fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            b'B' => Some(Self::Byte),
            b'C' => Some(Self::Char),
            b'D' => Some(Self::Double),
            b'F' => Some(Self::Float),
            b'I' => Some(Self::Int),
            b'J' => Some(Self::Long),
            b'S' => Some(Self::Short),
            b'Z' => Some(Self::Boolean),
            b's' => Some(Self::String),
            b'e' => Some(Self::Enum),
            b'c' => Some(Self::Class),
            b'@' => Some(Self::Annotation),
            b'[' => Some(Self::Array),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElementValueInfo {
    Const {
        tag: ElementValueTag,
        const_value_index: u16,
    },
    Enum {
        type_name_index: u16,
        const_name_index: u16,
    },
    Class {
        class_info_index: u16,
    },
    Annotation(AnnotationInfo),
    Array {
        values: Vec<ElementValueInfo>,
    },
}

impl ElementValueInfo {
    pub const fn tag(&self) -> ElementValueTag {
        match self {
            Self::Const { tag, .. } => *tag,
            Self::Enum { .. } => ElementValueTag::Enum,
            Self::Class { .. } => ElementValueTag::Class,
            Self::Annotation(_) => ElementValueTag::Annotation,
            Self::Array { .. } => ElementValueTag::Array,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementValuePairInfo {
    pub element_name_index: u16,
    pub element_value: ElementValueInfo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnotationInfo {
    pub type_index: u16,
    pub element_value_pairs: Vec<ElementValuePairInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParameterAnnotationInfo {
    pub annotations: Vec<AnnotationInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableInfo {
    pub start_pc: u16,
    pub length: u16,
    pub index: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

impl TargetInfo {
    pub const fn target_info_type(&self) -> TargetInfoType {
        match self {
            Self::TypeParameter { .. } => TargetInfoType::TypeParameter,
            Self::Supertype { .. } => TargetInfoType::Supertype,
            Self::TypeParameterBound { .. } => TargetInfoType::TypeParameterBound,
            Self::Empty => TargetInfoType::Empty,
            Self::FormalParameter { .. } => TargetInfoType::FormalParameter,
            Self::Throws { .. } => TargetInfoType::Throws,
            Self::Localvar { .. } => TargetInfoType::Localvar,
            Self::Catch { .. } => TargetInfoType::Catch,
            Self::Offset { .. } => TargetInfoType::Offset,
            Self::TypeArgument { .. } => TargetInfoType::TypeArgument,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathInfo {
    pub type_path_kind: TypePathKind,
    pub type_argument_index: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypePathInfo {
    pub path: Vec<PathInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeAnnotationInfo {
    pub target_type: TargetType,
    pub target_info: TargetInfo,
    pub target_path: TypePathInfo,
    pub type_index: u16,
    pub element_value_pairs: Vec<ElementValuePairInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InnerClassInfo {
    pub inner_class_info_index: u16,
    pub outer_class_info_index: u16,
    pub inner_name_index: u16,
    pub inner_class_access_flags: NestedClassAccessFlag,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineNumberInfo {
    pub start_pc: u16,
    pub line_number: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalVariableInfo {
    pub start_pc: u16,
    pub length: u16,
    pub name_index: u16,
    pub descriptor_index: u16,
    pub index: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalVariableTypeInfo {
    pub start_pc: u16,
    pub length: u16,
    pub name_index: u16,
    pub signature_index: u16,
    pub index: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapMethodInfo {
    pub bootstrap_method_ref: u16,
    pub bootstrap_arguments: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodParameterInfo {
    pub name_index: u16,
    pub access_flags: MethodParameterAccessFlag,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequiresInfo {
    pub requires_index: u16,
    pub requires_flags: ModuleRequiresAccessFlag,
    pub requires_version_index: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportInfo {
    pub exports_index: u16,
    pub exports_flags: ModuleExportsAccessFlag,
    pub exports_to_index: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpensInfo {
    pub opens_index: u16,
    pub opens_flags: ModuleOpensAccessFlag,
    pub opens_to_index: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvidesInfo {
    pub provides_index: u16,
    pub provides_with_index: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleInfo {
    pub module_name_index: u16,
    pub module_flags: ModuleAccessFlag,
    pub module_version_index: u16,
    pub requires: Vec<RequiresInfo>,
    pub exports: Vec<ExportInfo>,
    pub opens: Vec<OpensInfo>,
    pub uses_index: Vec<u16>,
    pub provides: Vec<ProvidesInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordComponentInfo {
    pub name_index: u16,
    pub descriptor_index: u16,
    pub attributes: Vec<AttributeInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownAttribute {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
    pub name: String,
    pub info: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttributeInfo {
    ConstantValue(ConstantValueAttribute),
    Signature(SignatureAttribute),
    SourceFile(SourceFileAttribute),
    SourceDebugExtension(SourceDebugExtensionAttribute),
    Synthetic(SyntheticAttribute),
    Deprecated(DeprecatedAttribute),
    StackMapTable(StackMapTableAttribute),
    Exceptions(ExceptionsAttribute),
    InnerClasses(InnerClassesAttribute),
    EnclosingMethod(EnclosingMethodAttribute),
    Code(CodeAttribute),
    LineNumberTable(LineNumberTableAttribute),
    LocalVariableTable(LocalVariableTableAttribute),
    LocalVariableTypeTable(LocalVariableTypeTableAttribute),
    MethodParameters(MethodParametersAttribute),
    NestHost(NestHostAttribute),
    NestMembers(NestMembersAttribute),
    RuntimeVisibleAnnotations(RuntimeVisibleAnnotationsAttribute),
    RuntimeInvisibleAnnotations(RuntimeInvisibleAnnotationsAttribute),
    RuntimeVisibleParameterAnnotations(RuntimeVisibleParameterAnnotationsAttribute),
    RuntimeInvisibleParameterAnnotations(RuntimeInvisibleParameterAnnotationsAttribute),
    RuntimeVisibleTypeAnnotations(RuntimeVisibleTypeAnnotationsAttribute),
    RuntimeInvisibleTypeAnnotations(RuntimeInvisibleTypeAnnotationsAttribute),
    AnnotationDefault(AnnotationDefaultAttribute),
    BootstrapMethods(BootstrapMethodsAttribute),
    Module(ModuleAttribute),
    ModulePackages(ModulePackagesAttribute),
    ModuleMainClass(ModuleMainClassAttribute),
    Record(RecordAttribute),
    PermittedSubclasses(PermittedSubclassesAttribute),
    Unknown(UnknownAttribute),
}

impl AttributeInfo {
    pub fn attribute_name_index(&self) -> u16 {
        match self {
            Self::ConstantValue(attr) => attr.attribute_name_index,
            Self::Signature(attr) => attr.attribute_name_index,
            Self::SourceFile(attr) => attr.attribute_name_index,
            Self::SourceDebugExtension(attr) => attr.attribute_name_index,
            Self::Synthetic(attr) => attr.attribute_name_index,
            Self::Deprecated(attr) => attr.attribute_name_index,
            Self::StackMapTable(attr) => attr.attribute_name_index,
            Self::Exceptions(attr) => attr.attribute_name_index,
            Self::InnerClasses(attr) => attr.attribute_name_index,
            Self::EnclosingMethod(attr) => attr.attribute_name_index,
            Self::Code(attr) => attr.attribute_name_index,
            Self::LineNumberTable(attr) => attr.attribute_name_index,
            Self::LocalVariableTable(attr) => attr.attribute_name_index,
            Self::LocalVariableTypeTable(attr) => attr.attribute_name_index,
            Self::MethodParameters(attr) => attr.attribute_name_index,
            Self::NestHost(attr) => attr.attribute_name_index,
            Self::NestMembers(attr) => attr.attribute_name_index,
            Self::RuntimeVisibleAnnotations(attr) => attr.attribute_name_index,
            Self::RuntimeInvisibleAnnotations(attr) => attr.attribute_name_index,
            Self::RuntimeVisibleParameterAnnotations(attr) => attr.attribute_name_index,
            Self::RuntimeInvisibleParameterAnnotations(attr) => attr.attribute_name_index,
            Self::RuntimeVisibleTypeAnnotations(attr) => attr.attribute_name_index,
            Self::RuntimeInvisibleTypeAnnotations(attr) => attr.attribute_name_index,
            Self::AnnotationDefault(attr) => attr.attribute_name_index,
            Self::BootstrapMethods(attr) => attr.attribute_name_index,
            Self::Module(attr) => attr.attribute_name_index,
            Self::ModulePackages(attr) => attr.attribute_name_index,
            Self::ModuleMainClass(attr) => attr.attribute_name_index,
            Self::Record(attr) => attr.attribute_name_index,
            Self::PermittedSubclasses(attr) => attr.attribute_name_index,
            Self::Unknown(attr) => attr.attribute_name_index,
        }
    }
}
