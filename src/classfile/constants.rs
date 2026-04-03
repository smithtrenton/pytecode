#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModuleAccessFlags(pub u16);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModuleRequiresAccessFlags(pub u16);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModuleExportsAccessFlags(pub u16);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModuleOpensAccessFlags(pub u16);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassAccessFlags(pub u16);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NestedClassAccessFlags(pub u16);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MethodAccessFlags(pub u16);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MethodParameterAccessFlags(pub u16);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldAccessFlags(pub u16);

macro_rules! flag_impl {
    ($name:ident: $bits:ty { $($flag:ident = $value:expr,)+ }) => {
        impl $name {
            $(pub const $flag: Self = Self($value);)+

            pub const fn contains(self, flag: Self) -> bool {
                (self.0 & flag.0) == flag.0
            }
        }
    };
}

pub const MAGIC: u32 = 0xCAFEBABE;

flag_impl!(ModuleAccessFlags: u16 {
    OPEN = 0x0020,
    SYNTHETIC = 0x1000,
    MANDATED = 0x8000,
});

flag_impl!(ModuleRequiresAccessFlags: u16 {
    TRANSITIVE = 0x0020,
    STATIC_PHASE = 0x0040,
    SYNTHETIC = 0x1000,
    MANDATED = 0x8000,
});

flag_impl!(ModuleExportsAccessFlags: u16 {
    SYNTHETIC = 0x1000,
    MANDATED = 0x8000,
});

flag_impl!(ModuleOpensAccessFlags: u16 {
    SYNTHETIC = 0x1000,
    MANDATED = 0x8000,
});

flag_impl!(ClassAccessFlags: u16 {
    PUBLIC = 0x0001,
    FINAL = 0x0010,
    SUPER = 0x0020,
    INTERFACE = 0x0200,
    ABSTRACT = 0x0400,
    SYNTHETIC = 0x1000,
    ANNOTATION = 0x2000,
    ENUM = 0x4000,
    MODULE = 0x8000,
});

flag_impl!(NestedClassAccessFlags: u16 {
    PUBLIC = 0x0001,
    PRIVATE = 0x0002,
    PROTECTED = 0x0004,
    STATIC = 0x0008,
    FINAL = 0x0010,
    INTERFACE = 0x0200,
    ABSTRACT = 0x0400,
    SYNTHETIC = 0x1000,
    ANNOTATION = 0x2000,
    ENUM = 0x4000,
});

flag_impl!(MethodAccessFlags: u16 {
    PUBLIC = 0x0001,
    PRIVATE = 0x0002,
    PROTECTED = 0x0004,
    STATIC = 0x0008,
    FINAL = 0x0010,
    SYNCHRONIZED = 0x0020,
    BRIDGE = 0x0040,
    VARARGS = 0x0080,
    NATIVE = 0x0100,
    ABSTRACT = 0x0400,
    STRICT = 0x0800,
    SYNTHETIC = 0x1000,
});

flag_impl!(MethodParameterAccessFlags: u16 {
    FINAL = 0x0010,
    SYNTHETIC = 0x1000,
    MANDATED = 0x8000,
});

flag_impl!(FieldAccessFlags: u16 {
    PUBLIC = 0x0001,
    PRIVATE = 0x0002,
    PROTECTED = 0x0004,
    STATIC = 0x0008,
    FINAL = 0x0010,
    VOLATILE = 0x0040,
    TRANSIENT = 0x0080,
    SYNTHETIC = 0x1000,
    ENUM = 0x4000,
});

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetInfoKind {
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

impl TargetInfoKind {
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
#[allow(clippy::enum_variant_names)]
pub enum TypePathKind {
    ArrayType = 0,
    NestedType = 1,
    WildcardType = 2,
    ParameterizedType = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_access_flags_contain_expected_bits() {
        let flags = ClassAccessFlags::PUBLIC;
        assert!(flags.contains(ClassAccessFlags::PUBLIC));
        assert!(!flags.contains(ClassAccessFlags::FINAL));
    }

    #[test]
    fn target_info_kind_maps_from_target_type() {
        assert_eq!(
            TargetInfoKind::from_target_type(TargetType::TypeMethodIdentifier),
            TargetInfoKind::Offset
        );
    }
}
