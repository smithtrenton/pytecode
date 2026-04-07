use crate::constants::{ClassAccessFlags, FieldAccessFlags, MethodAccessFlags};
use crate::raw::attributes::AttributeInfo;
use crate::raw::constant_pool::ConstantPoolEntry;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldInfo {
    pub access_flags: FieldAccessFlags,
    pub name_index: u16,
    pub descriptor_index: u16,
    pub attributes: Vec<AttributeInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodInfo {
    pub access_flags: MethodAccessFlags,
    pub name_index: u16,
    pub descriptor_index: u16,
    pub attributes: Vec<AttributeInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassFile {
    pub magic: u32,
    pub minor_version: u16,
    pub major_version: u16,
    pub constant_pool: Vec<Option<ConstantPoolEntry>>,
    pub access_flags: ClassAccessFlags,
    pub this_class: u16,
    pub super_class: u16,
    pub interfaces: Vec<u16>,
    pub fields: Vec<FieldInfo>,
    pub methods: Vec<MethodInfo>,
    pub attributes: Vec<AttributeInfo>,
}
