#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ConstantPoolTag {
    Utf8 = 1,
    Integer = 3,
    Float = 4,
    Long = 5,
    Double = 6,
    Class = 7,
    String = 8,
    FieldRef = 9,
    MethodRef = 10,
    InterfaceMethodRef = 11,
    NameAndType = 12,
    MethodHandle = 15,
    MethodType = 16,
    Dynamic = 17,
    InvokeDynamic = 18,
    Module = 19,
    Package = 20,
}

impl TryFrom<u8> for ConstantPoolTag {
    type Error = crate::error::EngineErrorKind;

    fn try_from(value: u8) -> std::result::Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Utf8),
            3 => Ok(Self::Integer),
            4 => Ok(Self::Float),
            5 => Ok(Self::Long),
            6 => Ok(Self::Double),
            7 => Ok(Self::Class),
            8 => Ok(Self::String),
            9 => Ok(Self::FieldRef),
            10 => Ok(Self::MethodRef),
            11 => Ok(Self::InterfaceMethodRef),
            12 => Ok(Self::NameAndType),
            15 => Ok(Self::MethodHandle),
            16 => Ok(Self::MethodType),
            17 => Ok(Self::Dynamic),
            18 => Ok(Self::InvokeDynamic),
            19 => Ok(Self::Module),
            20 => Ok(Self::Package),
            tag => Err(crate::error::EngineErrorKind::InvalidConstantPoolTag { tag }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Utf8Info {
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegerInfo {
    pub value_bytes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FloatInfo {
    pub value_bytes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LongInfo {
    pub high_bytes: u32,
    pub low_bytes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoubleInfo {
    pub high_bytes: u32,
    pub low_bytes: u32,
}

use crate::indexes::{BootstrapMethodIndex, ClassIndex, CpIndex, NameAndTypeIndex, Utf8Index};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassInfo {
    pub name_index: Utf8Index,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StringInfo {
    pub string_index: Utf8Index,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldRefInfo {
    pub class_index: ClassIndex,
    pub name_and_type_index: NameAndTypeIndex,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodRefInfo {
    pub class_index: ClassIndex,
    pub name_and_type_index: NameAndTypeIndex,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceMethodRefInfo {
    pub class_index: ClassIndex,
    pub name_and_type_index: NameAndTypeIndex,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameAndTypeInfo {
    pub name_index: Utf8Index,
    pub descriptor_index: Utf8Index,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodHandleInfo {
    pub reference_kind: u8,
    pub reference_index: CpIndex,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodTypeInfo {
    pub descriptor_index: Utf8Index,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicInfo {
    pub bootstrap_method_attr_index: BootstrapMethodIndex,
    pub name_and_type_index: NameAndTypeIndex,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvokeDynamicInfo {
    pub bootstrap_method_attr_index: BootstrapMethodIndex,
    pub name_and_type_index: NameAndTypeIndex,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleInfo {
    pub name_index: Utf8Index,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageInfo {
    pub name_index: Utf8Index,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstantPoolEntry {
    Utf8(Utf8Info),
    Integer(IntegerInfo),
    Float(FloatInfo),
    Long(LongInfo),
    Double(DoubleInfo),
    Class(ClassInfo),
    String(StringInfo),
    FieldRef(FieldRefInfo),
    MethodRef(MethodRefInfo),
    InterfaceMethodRef(InterfaceMethodRefInfo),
    NameAndType(NameAndTypeInfo),
    MethodHandle(MethodHandleInfo),
    MethodType(MethodTypeInfo),
    Dynamic(DynamicInfo),
    InvokeDynamic(InvokeDynamicInfo),
    Module(ModuleInfo),
    Package(PackageInfo),
}

impl ConstantPoolEntry {
    pub fn tag(&self) -> ConstantPoolTag {
        match self {
            Self::Utf8(_) => ConstantPoolTag::Utf8,
            Self::Integer(_) => ConstantPoolTag::Integer,
            Self::Float(_) => ConstantPoolTag::Float,
            Self::Long(_) => ConstantPoolTag::Long,
            Self::Double(_) => ConstantPoolTag::Double,
            Self::Class(_) => ConstantPoolTag::Class,
            Self::String(_) => ConstantPoolTag::String,
            Self::FieldRef(_) => ConstantPoolTag::FieldRef,
            Self::MethodRef(_) => ConstantPoolTag::MethodRef,
            Self::InterfaceMethodRef(_) => ConstantPoolTag::InterfaceMethodRef,
            Self::NameAndType(_) => ConstantPoolTag::NameAndType,
            Self::MethodHandle(_) => ConstantPoolTag::MethodHandle,
            Self::MethodType(_) => ConstantPoolTag::MethodType,
            Self::Dynamic(_) => ConstantPoolTag::Dynamic,
            Self::InvokeDynamic(_) => ConstantPoolTag::InvokeDynamic,
            Self::Module(_) => ConstantPoolTag::Module,
            Self::Package(_) => ConstantPoolTag::Package,
        }
    }

    pub fn is_wide(&self) -> bool {
        matches!(self, Self::Long(_) | Self::Double(_))
    }
}
