use crate::modified_utf8::{decode_modified_utf8, encode_modified_utf8};
use crate::raw::{
    ClassInfo, ConstantPoolEntry, DoubleInfo, DynamicInfo, FieldRefInfo, FloatInfo, IntegerInfo,
    InterfaceMethodRefInfo, InvokeDynamicInfo, LongInfo, MethodHandleInfo, MethodRefInfo,
    MethodTypeInfo, ModuleInfo, NameAndTypeInfo, PackageInfo, StringInfo, Utf8Info,
};
use crate::{EngineError, EngineErrorKind, Result};
use std::collections::HashMap;

const CP_MAX_SINGLE_INDEX: usize = 65_534;
const CP_MAX_DOUBLE_INDEX: usize = 65_533;
const UTF8_MAX_BYTES: usize = 65_535;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstantPoolBuilder {
    entries: Vec<Option<ConstantPoolEntry>>,
    key_to_index: HashMap<PoolKey, u16>,
}

impl Default for ConstantPoolBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ConstantPoolBuilder {
    pub fn new() -> Self {
        Self {
            entries: vec![None],
            key_to_index: HashMap::new(),
        }
    }

    pub fn from_pool(pool: &[Option<ConstantPoolEntry>]) -> Self {
        let mut builder = Self {
            entries: pool.to_vec(),
            key_to_index: HashMap::new(),
        };
        for (index, entry) in builder.entries.iter().enumerate().skip(1) {
            let Some(entry) = entry else {
                continue;
            };
            builder
                .key_to_index
                .entry(PoolKey::from_entry(entry))
                .or_insert(index as u16);
        }
        if builder.entries.is_empty() {
            builder.entries.push(None);
        }
        builder
    }

    pub fn build(&self) -> Vec<Option<ConstantPoolEntry>> {
        self.entries.clone()
    }

    pub fn count(&self) -> u16 {
        self.entries.len() as u16
    }

    pub fn len(&self) -> usize {
        self.entries
            .iter()
            .skip(1)
            .filter(|entry| entry.is_some())
            .count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn peek(&self, index: u16) -> Option<&ConstantPoolEntry> {
        if index == 0 {
            return None;
        }
        self.entries.get(index as usize).and_then(Option::as_ref)
    }

    pub fn entry(&self, index: u16) -> Result<&ConstantPoolEntry> {
        self.peek(index)
            .ok_or_else(|| EngineError::new(0, EngineErrorKind::InvalidConstantPoolIndex { index }))
    }

    pub fn resolve_utf8(&self, index: u16) -> Result<String> {
        let entry = self.entry(index)?;
        match entry {
            ConstantPoolEntry::Utf8(info) => decode_modified_utf8(&info.bytes),
            _ => Err(EngineError::new(
                0,
                EngineErrorKind::InvalidModelState {
                    reason: format!("constant-pool entry {index} is not Utf8"),
                },
            )),
        }
    }

    pub fn resolve_class_name(&self, index: u16) -> Result<String> {
        let entry = self.entry(index)?;
        match entry {
            ConstantPoolEntry::Class(info) => self.resolve_utf8(info.name_index),
            _ => Err(EngineError::new(
                0,
                EngineErrorKind::InvalidModelState {
                    reason: format!("constant-pool entry {index} is not Class"),
                },
            )),
        }
    }

    pub fn resolve_name_and_type(&self, index: u16) -> Result<(String, String)> {
        let entry = self.entry(index)?;
        match entry {
            ConstantPoolEntry::NameAndType(info) => Ok((
                self.resolve_utf8(info.name_index)?,
                self.resolve_utf8(info.descriptor_index)?,
            )),
            _ => Err(EngineError::new(
                0,
                EngineErrorKind::InvalidModelState {
                    reason: format!("constant-pool entry {index} is not NameAndType"),
                },
            )),
        }
    }

    pub fn resolve_field_ref(&self, index: u16) -> Result<(String, String, String)> {
        let entry = self.entry(index)?;
        match entry {
            ConstantPoolEntry::FieldRef(info) => {
                let owner = self.resolve_class_name(info.class_index)?;
                let (name, descriptor) = self.resolve_name_and_type(info.name_and_type_index)?;
                Ok((owner, name, descriptor))
            }
            _ => Err(EngineError::new(
                0,
                EngineErrorKind::InvalidModelState {
                    reason: format!("constant-pool entry {index} is not FieldRef"),
                },
            )),
        }
    }

    pub fn resolve_method_ref(&self, index: u16) -> Result<(String, String, String)> {
        let entry = self.entry(index)?;
        match entry {
            ConstantPoolEntry::MethodRef(info) => {
                let owner = self.resolve_class_name(info.class_index)?;
                let (name, descriptor) = self.resolve_name_and_type(info.name_and_type_index)?;
                Ok((owner, name, descriptor))
            }
            _ => Err(EngineError::new(
                0,
                EngineErrorKind::InvalidModelState {
                    reason: format!("constant-pool entry {index} is not MethodRef"),
                },
            )),
        }
    }

    pub fn resolve_any_method_ref(&self, index: u16) -> Result<(String, String, String, bool)> {
        let entry = self.entry(index)?;
        match entry {
            ConstantPoolEntry::MethodRef(_) => {
                let (owner, name, descriptor) = self.resolve_method_ref(index)?;
                Ok((owner, name, descriptor, false))
            }
            ConstantPoolEntry::InterfaceMethodRef(_) => {
                let (owner, name, descriptor) = self.resolve_interface_method_ref(index)?;
                Ok((owner, name, descriptor, true))
            }
            _ => Err(EngineError::new(
                0,
                EngineErrorKind::InvalidModelState {
                    reason: format!(
                        "constant-pool entry {index} is not MethodRef or InterfaceMethodRef"
                    ),
                },
            )),
        }
    }

    pub fn resolve_interface_method_ref(&self, index: u16) -> Result<(String, String, String)> {
        let entry = self.entry(index)?;
        match entry {
            ConstantPoolEntry::InterfaceMethodRef(info) => {
                let owner = self.resolve_class_name(info.class_index)?;
                let (name, descriptor) = self.resolve_name_and_type(info.name_and_type_index)?;
                Ok((owner, name, descriptor))
            }
            _ => Err(EngineError::new(
                0,
                EngineErrorKind::InvalidModelState {
                    reason: format!("constant-pool entry {index} is not InterfaceMethodRef"),
                },
            )),
        }
    }

    pub fn resolve_method_type(&self, index: u16) -> Result<String> {
        let entry = self.entry(index)?;
        match entry {
            ConstantPoolEntry::MethodType(info) => self.resolve_utf8(info.descriptor_index),
            _ => Err(EngineError::new(
                0,
                EngineErrorKind::InvalidModelState {
                    reason: format!("constant-pool entry {index} is not MethodType"),
                },
            )),
        }
    }

    pub fn resolve_string(&self, index: u16) -> Result<String> {
        let entry = self.entry(index)?;
        match entry {
            ConstantPoolEntry::String(info) => self.resolve_utf8(info.string_index),
            _ => Err(EngineError::new(
                0,
                EngineErrorKind::InvalidModelState {
                    reason: format!("constant-pool entry {index} is not String"),
                },
            )),
        }
    }

    pub fn add_entry(&mut self, entry: ConstantPoolEntry) -> Result<u16> {
        let key = PoolKey::from_entry(&entry);
        if let Some(&index) = self.key_to_index.get(&key) {
            return Ok(index);
        }

        let next_index = self.entries.len();
        if entry.is_wide() {
            if next_index > CP_MAX_DOUBLE_INDEX {
                return Err(EngineError::new(
                    0,
                    EngineErrorKind::InvalidModelState {
                        reason: "constant pool cannot fit another double-slot entry".to_owned(),
                    },
                ));
            }
        } else if next_index > CP_MAX_SINGLE_INDEX {
            return Err(EngineError::new(
                0,
                EngineErrorKind::InvalidModelState {
                    reason: "constant pool cannot fit another single-slot entry".to_owned(),
                },
            ));
        }

        let index = next_index as u16;
        self.entries.push(Some(entry.clone()));
        self.key_to_index.insert(key, index);
        if entry.is_wide() {
            self.entries.push(None);
        }
        Ok(index)
    }

    pub fn add_utf8(&mut self, value: &str) -> Result<u16> {
        let bytes = encode_modified_utf8(value);
        if bytes.len() > UTF8_MAX_BYTES {
            return Err(EngineError::new(
                0,
                EngineErrorKind::InvalidModelState {
                    reason: format!("UTF-8 payload exceeds {UTF8_MAX_BYTES} bytes"),
                },
            ));
        }
        self.add_entry(ConstantPoolEntry::Utf8(Utf8Info { bytes }))
    }

    pub fn add_integer(&mut self, value: i32) -> Result<u16> {
        self.add_entry(ConstantPoolEntry::Integer(IntegerInfo {
            value_bytes: value as u32,
        }))
    }

    pub fn add_float_bits(&mut self, raw_bits: u32) -> Result<u16> {
        self.add_entry(ConstantPoolEntry::Float(FloatInfo {
            value_bytes: raw_bits,
        }))
    }

    pub fn add_long(&mut self, value: i64) -> Result<u16> {
        let raw = value as u64;
        self.add_entry(ConstantPoolEntry::Long(LongInfo {
            high_bytes: (raw >> 32) as u32,
            low_bytes: raw as u32,
        }))
    }

    pub fn add_double_bits(&mut self, raw_bits: u64) -> Result<u16> {
        self.add_entry(ConstantPoolEntry::Double(DoubleInfo {
            high_bytes: (raw_bits >> 32) as u32,
            low_bytes: raw_bits as u32,
        }))
    }

    pub fn add_class(&mut self, class_name: &str) -> Result<u16> {
        let name_index = self.add_utf8(class_name)?;
        self.add_entry(ConstantPoolEntry::Class(ClassInfo { name_index }))
    }

    pub fn add_string(&mut self, value: &str) -> Result<u16> {
        let string_index = self.add_utf8(value)?;
        self.add_entry(ConstantPoolEntry::String(StringInfo { string_index }))
    }

    pub fn add_name_and_type(&mut self, name: &str, descriptor: &str) -> Result<u16> {
        let name_index = self.add_utf8(name)?;
        let descriptor_index = self.add_utf8(descriptor)?;
        self.add_entry(ConstantPoolEntry::NameAndType(NameAndTypeInfo {
            name_index,
            descriptor_index,
        }))
    }

    pub fn add_field_ref(&mut self, owner: &str, name: &str, descriptor: &str) -> Result<u16> {
        let class_index = self.add_class(owner)?;
        let name_and_type_index = self.add_name_and_type(name, descriptor)?;
        self.add_entry(ConstantPoolEntry::FieldRef(FieldRefInfo {
            class_index,
            name_and_type_index,
        }))
    }

    pub fn add_method_ref(&mut self, owner: &str, name: &str, descriptor: &str) -> Result<u16> {
        let class_index = self.add_class(owner)?;
        let name_and_type_index = self.add_name_and_type(name, descriptor)?;
        self.add_entry(ConstantPoolEntry::MethodRef(MethodRefInfo {
            class_index,
            name_and_type_index,
        }))
    }

    pub fn add_interface_method_ref(
        &mut self,
        owner: &str,
        name: &str,
        descriptor: &str,
    ) -> Result<u16> {
        let class_index = self.add_class(owner)?;
        let name_and_type_index = self.add_name_and_type(name, descriptor)?;
        self.add_entry(ConstantPoolEntry::InterfaceMethodRef(
            InterfaceMethodRefInfo {
                class_index,
                name_and_type_index,
            },
        ))
    }

    pub fn add_method_type(&mut self, descriptor: &str) -> Result<u16> {
        let descriptor_index = self.add_utf8(descriptor)?;
        self.add_entry(ConstantPoolEntry::MethodType(MethodTypeInfo {
            descriptor_index,
        }))
    }

    pub fn add_method_handle(&mut self, reference_kind: u8, reference_index: u16) -> Result<u16> {
        self.add_entry(ConstantPoolEntry::MethodHandle(MethodHandleInfo {
            reference_kind,
            reference_index,
        }))
    }

    pub fn add_dynamic(
        &mut self,
        bootstrap_method_attr_index: u16,
        name: &str,
        descriptor: &str,
    ) -> Result<u16> {
        let name_and_type_index = self.add_name_and_type(name, descriptor)?;
        self.add_entry(ConstantPoolEntry::Dynamic(DynamicInfo {
            bootstrap_method_attr_index,
            name_and_type_index,
        }))
    }

    pub fn add_invoke_dynamic(
        &mut self,
        bootstrap_method_attr_index: u16,
        name: &str,
        descriptor: &str,
    ) -> Result<u16> {
        let name_and_type_index = self.add_name_and_type(name, descriptor)?;
        self.add_entry(ConstantPoolEntry::InvokeDynamic(InvokeDynamicInfo {
            bootstrap_method_attr_index,
            name_and_type_index,
        }))
    }

    pub fn add_module(&mut self, name: &str) -> Result<u16> {
        let name_index = self.add_utf8(name)?;
        self.add_entry(ConstantPoolEntry::Module(ModuleInfo { name_index }))
    }

    pub fn add_package(&mut self, name: &str) -> Result<u16> {
        let name_index = self.add_utf8(name)?;
        self.add_entry(ConstantPoolEntry::Package(PackageInfo { name_index }))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum PoolKey {
    Utf8(Vec<u8>),
    Integer(u32),
    Float(u32),
    Long(u32, u32),
    Double(u32, u32),
    Class(u16),
    String(u16),
    FieldRef(u16, u16),
    MethodRef(u16, u16),
    InterfaceMethodRef(u16, u16),
    NameAndType(u16, u16),
    MethodHandle(u8, u16),
    MethodType(u16),
    Dynamic(u16, u16),
    InvokeDynamic(u16, u16),
    Module(u16),
    Package(u16),
}

impl PoolKey {
    fn from_entry(entry: &ConstantPoolEntry) -> Self {
        match entry {
            ConstantPoolEntry::Utf8(info) => Self::Utf8(info.bytes.clone()),
            ConstantPoolEntry::Integer(info) => Self::Integer(info.value_bytes),
            ConstantPoolEntry::Float(info) => Self::Float(info.value_bytes),
            ConstantPoolEntry::Long(info) => Self::Long(info.high_bytes, info.low_bytes),
            ConstantPoolEntry::Double(info) => Self::Double(info.high_bytes, info.low_bytes),
            ConstantPoolEntry::Class(info) => Self::Class(info.name_index),
            ConstantPoolEntry::String(info) => Self::String(info.string_index),
            ConstantPoolEntry::FieldRef(info) => {
                Self::FieldRef(info.class_index, info.name_and_type_index)
            }
            ConstantPoolEntry::MethodRef(info) => {
                Self::MethodRef(info.class_index, info.name_and_type_index)
            }
            ConstantPoolEntry::InterfaceMethodRef(info) => {
                Self::InterfaceMethodRef(info.class_index, info.name_and_type_index)
            }
            ConstantPoolEntry::NameAndType(info) => {
                Self::NameAndType(info.name_index, info.descriptor_index)
            }
            ConstantPoolEntry::MethodHandle(info) => {
                Self::MethodHandle(info.reference_kind, info.reference_index)
            }
            ConstantPoolEntry::MethodType(info) => Self::MethodType(info.descriptor_index),
            ConstantPoolEntry::Dynamic(info) => {
                Self::Dynamic(info.bootstrap_method_attr_index, info.name_and_type_index)
            }
            ConstantPoolEntry::InvokeDynamic(info) => {
                Self::InvokeDynamic(info.bootstrap_method_attr_index, info.name_and_type_index)
            }
            ConstantPoolEntry::Module(info) => Self::Module(info.name_index),
            ConstantPoolEntry::Package(info) => Self::Package(info.name_index),
        }
    }
}
