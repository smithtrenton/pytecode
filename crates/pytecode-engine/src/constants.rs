use bitflags::bitflags;

pub use crate::raw::constant_pool::ConstantPoolTag;

pub const MAGIC: u32 = 0xCAFEBABE;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct ClassAccessFlags: u16 {
        const PUBLIC = 0x0001;
        const FINAL = 0x0010;
        const SUPER = 0x0020;
        const INTERFACE = 0x0200;
        const ABSTRACT = 0x0400;
        const SYNTHETIC = 0x1000;
        const ANNOTATION = 0x2000;
        const ENUM = 0x4000;
        const MODULE = 0x8000;
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct FieldAccessFlags: u16 {
        const PUBLIC = 0x0001;
        const PRIVATE = 0x0002;
        const PROTECTED = 0x0004;
        const STATIC = 0x0008;
        const FINAL = 0x0010;
        const VOLATILE = 0x0040;
        const TRANSIENT = 0x0080;
        const SYNTHETIC = 0x1000;
        const ENUM = 0x4000;
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct MethodAccessFlags: u16 {
        const PUBLIC = 0x0001;
        const PRIVATE = 0x0002;
        const PROTECTED = 0x0004;
        const STATIC = 0x0008;
        const FINAL = 0x0010;
        const SYNCHRONIZED = 0x0020;
        const BRIDGE = 0x0040;
        const VARARGS = 0x0080;
        const NATIVE = 0x0100;
        const ABSTRACT = 0x0400;
        const STRICT = 0x0800;
        const SYNTHETIC = 0x1000;
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct NestedClassAccessFlag: u16 {
        const PUBLIC = 0x0001;
        const PRIVATE = 0x0002;
        const PROTECTED = 0x0004;
        const STATIC = 0x0008;
        const FINAL = 0x0010;
        const INTERFACE = 0x0200;
        const ABSTRACT = 0x0400;
        const SYNTHETIC = 0x1000;
        const ANNOTATION = 0x2000;
        const ENUM = 0x4000;
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct MethodParameterAccessFlag: u16 {
        const FINAL = 0x0010;
        const SYNTHETIC = 0x1000;
        const MANDATED = 0x8000;
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct ModuleAccessFlag: u16 {
        const OPEN = 0x0020;
        const SYNTHETIC = 0x1000;
        const MANDATED = 0x8000;
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct ModuleRequiresAccessFlag: u16 {
        const TRANSITIVE = 0x0020;
        const STATIC_PHASE = 0x0040;
        const SYNTHETIC = 0x1000;
        const MANDATED = 0x8000;
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct ModuleExportsAccessFlag: u16 {
        const SYNTHETIC = 0x1000;
        const MANDATED = 0x8000;
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct ModuleOpensAccessFlag: u16 {
        const SYNTHETIC = 0x1000;
        const MANDATED = 0x8000;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum VerificationType {
    Top = 0,
    Integer = 1,
    Float = 2,
    Double = 3,
    Long = 4,
    Null = 5,
    UninitializedThis = 6,
    Object = 7,
    Uninitialized = 8,
}

impl VerificationType {
    pub const fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            0 => Some(Self::Top),
            1 => Some(Self::Integer),
            2 => Some(Self::Float),
            3 => Some(Self::Double),
            4 => Some(Self::Long),
            5 => Some(Self::Null),
            6 => Some(Self::UninitializedThis),
            7 => Some(Self::Object),
            8 => Some(Self::Uninitialized),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum TargetType {
    TypeParameterGenericClassOrInterface = 0x00,
    TypeParameterGenericMethodOrConstructor = 0x01,
    Supertype = 0x10,
    TypeParameterBoundGenericClassOrInterface = 0x11,
    TypeParameterBoundGenericMethodOrConstructor = 0x12,
    TypeInFieldOrRecord = 0x13,
    ReturnOrObjectType = 0x14,
    ReceiverTypeMethodOrConstructor = 0x15,
    FormalParameterMethodConstructorOrLambda = 0x16,
    TypeThrows = 0x17,
    TypeLocalVariable = 0x40,
    TypeResourceVariable = 0x41,
    TypeExceptionParameter = 0x42,
    TypeInstanceOf = 0x43,
    TypeNew = 0x44,
    TypeMethodNew = 0x45,
    TypeMethodIdentifier = 0x46,
    TypeCast = 0x47,
    TypeGenericConstructor = 0x48,
    TypeGenericMethod = 0x49,
    TypeGenericConstructorNew = 0x4A,
    TypeGenericMethodIdentifier = 0x4B,
}

impl TargetType {
    pub const fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            0x00 => Some(Self::TypeParameterGenericClassOrInterface),
            0x01 => Some(Self::TypeParameterGenericMethodOrConstructor),
            0x10 => Some(Self::Supertype),
            0x11 => Some(Self::TypeParameterBoundGenericClassOrInterface),
            0x12 => Some(Self::TypeParameterBoundGenericMethodOrConstructor),
            0x13 => Some(Self::TypeInFieldOrRecord),
            0x14 => Some(Self::ReturnOrObjectType),
            0x15 => Some(Self::ReceiverTypeMethodOrConstructor),
            0x16 => Some(Self::FormalParameterMethodConstructorOrLambda),
            0x17 => Some(Self::TypeThrows),
            0x40 => Some(Self::TypeLocalVariable),
            0x41 => Some(Self::TypeResourceVariable),
            0x42 => Some(Self::TypeExceptionParameter),
            0x43 => Some(Self::TypeInstanceOf),
            0x44 => Some(Self::TypeNew),
            0x45 => Some(Self::TypeMethodNew),
            0x46 => Some(Self::TypeMethodIdentifier),
            0x47 => Some(Self::TypeCast),
            0x48 => Some(Self::TypeGenericConstructor),
            0x49 => Some(Self::TypeGenericMethod),
            0x4A => Some(Self::TypeGenericConstructorNew),
            0x4B => Some(Self::TypeGenericMethodIdentifier),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TargetInfoType {
    TypeParameter,
    Supertype,
    TypeParameterBound,
    Empty,
    FormalParameter,
    Throws,
    Localvar,
    Catch,
    Offset,
    TypeArgument,
}

impl TargetInfoType {
    pub const fn from_target_type(target_type: TargetType) -> Self {
        match target_type {
            TargetType::TypeParameterGenericClassOrInterface
            | TargetType::TypeParameterGenericMethodOrConstructor => Self::TypeParameter,
            TargetType::Supertype => Self::Supertype,
            TargetType::TypeParameterBoundGenericClassOrInterface
            | TargetType::TypeParameterBoundGenericMethodOrConstructor => Self::TypeParameterBound,
            TargetType::TypeInFieldOrRecord
            | TargetType::ReturnOrObjectType
            | TargetType::ReceiverTypeMethodOrConstructor => Self::Empty,
            TargetType::FormalParameterMethodConstructorOrLambda => Self::FormalParameter,
            TargetType::TypeThrows => Self::Throws,
            TargetType::TypeLocalVariable | TargetType::TypeResourceVariable => Self::Localvar,
            TargetType::TypeExceptionParameter => Self::Catch,
            TargetType::TypeInstanceOf
            | TargetType::TypeNew
            | TargetType::TypeMethodNew
            | TargetType::TypeMethodIdentifier => Self::Offset,
            TargetType::TypeCast
            | TargetType::TypeGenericConstructor
            | TargetType::TypeGenericMethod
            | TargetType::TypeGenericConstructorNew
            | TargetType::TypeGenericMethodIdentifier => Self::TypeArgument,
        }
    }

    pub const fn matches_target_type(self, target_type: TargetType) -> bool {
        matches!(
            (self, target_type),
            (
                Self::TypeParameter,
                TargetType::TypeParameterGenericClassOrInterface
                    | TargetType::TypeParameterGenericMethodOrConstructor,
            ) | (Self::Supertype, TargetType::Supertype)
                | (
                    Self::TypeParameterBound,
                    TargetType::TypeParameterBoundGenericClassOrInterface
                        | TargetType::TypeParameterBoundGenericMethodOrConstructor,
                )
                | (
                    Self::Empty,
                    TargetType::TypeInFieldOrRecord
                        | TargetType::ReturnOrObjectType
                        | TargetType::ReceiverTypeMethodOrConstructor,
                )
                | (
                    Self::FormalParameter,
                    TargetType::FormalParameterMethodConstructorOrLambda,
                )
                | (Self::Throws, TargetType::TypeThrows)
                | (
                    Self::Localvar,
                    TargetType::TypeLocalVariable | TargetType::TypeResourceVariable,
                )
                | (Self::Catch, TargetType::TypeExceptionParameter)
                | (
                    Self::Offset,
                    TargetType::TypeInstanceOf
                        | TargetType::TypeNew
                        | TargetType::TypeMethodNew
                        | TargetType::TypeMethodIdentifier,
                )
                | (
                    Self::TypeArgument,
                    TargetType::TypeCast
                        | TargetType::TypeGenericConstructor
                        | TargetType::TypeGenericMethod
                        | TargetType::TypeGenericConstructorNew
                        | TargetType::TypeGenericMethodIdentifier,
                )
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum TypePathKind {
    ArrayType = 0,
    NestedType = 1,
    WildcardType = 2,
    ParameterizedType = 3,
}

impl TypePathKind {
    pub const fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            0 => Some(Self::ArrayType),
            1 => Some(Self::NestedType),
            2 => Some(Self::WildcardType),
            3 => Some(Self::ParameterizedType),
            _ => None,
        }
    }
}

pub const MIN_SUPPORTED_CLASS_MAJOR: u16 = 45;
pub const MAX_SUPPORTED_CLASS_MAJOR: u16 = 69;

pub const fn class_version_supported_by_java_se_25(major: u16, minor: u16) -> bool {
    match major {
        45..=55 => true,
        56..=69 => minor == 0 || minor == u16::MAX,
        _ => false,
    }
}

pub fn validate_class_version(major: u16, minor: u16) -> crate::error::Result<()> {
    if !class_version_supported_by_java_se_25(major, minor) {
        return Err(crate::error::EngineError::new(
            4,
            crate::error::EngineErrorKind::InvalidVersion { major, minor },
        ));
    }
    Ok(())
}
