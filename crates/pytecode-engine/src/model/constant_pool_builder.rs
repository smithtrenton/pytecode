use crate::indexes::{
    BootstrapMethodIndex, ClassIndex, CpIndex, ModuleIndex, NameAndTypeIndex, PackageIndex,
    Utf8Index,
};
use crate::modified_utf8::{JavaString, decode_modified_utf8, encode_modified_utf8};
use crate::raw::{
    ClassInfo, ConstantPoolEntry, DoubleInfo, DynamicInfo, FieldRefInfo, FloatInfo, IntegerInfo,
    InterfaceMethodRefInfo, InvokeDynamicInfo, LongInfo, MethodHandleInfo, MethodRefInfo,
    MethodTypeInfo, ModuleInfo, NameAndTypeInfo, PackageInfo, StringInfo, Utf8Info,
};
use crate::{EngineError, EngineErrorKind, Result};
use rustc_hash::FxHashMap;

const CP_MAX_SINGLE_INDEX: usize = 65_534;
const CP_MAX_DOUBLE_INDEX: usize = 65_533;
const UTF8_MAX_BYTES: usize = 65_535;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstantPoolBuilder {
    entries: Vec<Option<ConstantPoolEntry>>,
    key_to_index: FxHashMap<PoolKey, u16>,
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
            key_to_index: FxHashMap::default(),
        }
    }

    pub fn from_pool(pool: &[Option<ConstantPoolEntry>]) -> Self {
        let mut entries = pool.to_vec();
        if entries.is_empty() {
            entries.push(None);
        }
        let mut builder = Self {
            entries,
            key_to_index: FxHashMap::default(),
        };

        let raw_keys: Vec<Option<PoolKey>> = builder
            .entries
            .iter()
            .map(|entry| entry.as_ref().map(PoolKey::from_entry))
            .collect();
        let mut index_map = vec![0_u16; builder.entries.len()];

        // Canonicalize in strict JVMS dependency order (leaf -> 1-hop -> 2-hop -> 3-hop)
        // so transitive chains (Utf8 -> NameAndType/Class -> MethodRef -> MethodHandle)
        // collapse to canonical indices in a single build.
        for tier in 1_u8..=4_u8 {
            for (index, entry) in builder.entries.iter().enumerate().skip(1) {
                let Some(entry) = entry else {
                    continue;
                };
                if entry_dependency_tier(entry) != tier {
                    continue;
                }
                let canonical_key = if tier == 1 {
                    raw_keys[index]
                        .as_ref()
                        .expect("non-empty slot should have raw key")
                        .clone()
                } else {
                    PoolKey::canonical_from_entry(entry, &index_map)
                };
                let canonical_index = *builder
                    .key_to_index
                    .entry(canonical_key.clone())
                    .or_insert(index as u16);
                index_map[index] = canonical_index;

                if let Some(raw_key) = raw_keys[index].as_ref()
                    && *raw_key != canonical_key
                {
                    builder
                        .key_to_index
                        .entry(raw_key.clone())
                        .or_insert(canonical_index);
                }
            }
        }

        builder
    }

    pub fn build(&self) -> Vec<Option<ConstantPoolEntry>> {
        self.entries.clone()
    }

    pub fn into_entries(self) -> Vec<Option<ConstantPoolEntry>> {
        self.entries
    }

    pub fn entries(&self) -> &[Option<ConstantPoolEntry>] {
        &self.entries
    }

    pub fn count(&self) -> u16 {
        self.entries.len().min(u16::MAX as usize) as u16
    }

    pub fn len(&self) -> usize {
        self.entries
            .iter()
            .skip(1)
            .filter(|entry| entry.is_some())
            .count()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.len() <= 1
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

    pub fn resolve_utf8(&self, index: Utf8Index) -> Result<String> {
        let entry = self.entry(index.value())?;
        match entry {
            ConstantPoolEntry::Utf8(info) => decode_modified_utf8(&info.bytes),
            _ => Err(EngineError::new(
                0,
                EngineErrorKind::InvalidModelState {
                    reason: format!("constant-pool entry {} is not Utf8", index),
                },
            )),
        }
    }

    pub fn resolve_java_string(&self, index: Utf8Index) -> Result<JavaString> {
        match self.entry(index.value())? {
            ConstantPoolEntry::Utf8(info) => JavaString::from_modified_utf8(&info.bytes),
            _ => Err(EngineError::new(
                0,
                EngineErrorKind::InvalidModelState {
                    reason: format!("constant-pool entry {index} is not Utf8"),
                },
            )),
        }
    }

    pub fn resolve_class_name(&self, index: ClassIndex) -> Result<String> {
        let entry = self.entry(index.value())?;
        match entry {
            ConstantPoolEntry::Class(info) => self.resolve_utf8(info.name_index),
            _ => Err(EngineError::new(
                0,
                EngineErrorKind::InvalidModelState {
                    reason: format!("constant-pool entry {} is not Class", index),
                },
            )),
        }
    }

    pub fn resolve_name_and_type(&self, index: NameAndTypeIndex) -> Result<(String, String)> {
        let entry = self.entry(index.value())?;
        match entry {
            ConstantPoolEntry::NameAndType(info) => Ok((
                self.resolve_utf8(info.name_index)?,
                self.resolve_utf8(info.descriptor_index)?,
            )),
            _ => Err(EngineError::new(
                0,
                EngineErrorKind::InvalidModelState {
                    reason: format!("constant-pool entry {} is not NameAndType", index),
                },
            )),
        }
    }

    pub fn resolve_field_ref(&self, index: CpIndex) -> Result<(String, String, String)> {
        let entry = self.entry(index.value())?;
        match entry {
            ConstantPoolEntry::FieldRef(info) => {
                let owner = self.resolve_class_name(info.class_index)?;
                let (name, descriptor) = self.resolve_name_and_type(info.name_and_type_index)?;
                Ok((owner, name, descriptor))
            }
            _ => Err(EngineError::new(
                0,
                EngineErrorKind::InvalidModelState {
                    reason: format!("constant-pool entry {} is not FieldRef", index),
                },
            )),
        }
    }

    pub fn resolve_method_ref(&self, index: CpIndex) -> Result<(String, String, String)> {
        let entry = self.entry(index.value())?;
        match entry {
            ConstantPoolEntry::MethodRef(info) => {
                let owner = self.resolve_class_name(info.class_index)?;
                let (name, descriptor) = self.resolve_name_and_type(info.name_and_type_index)?;
                Ok((owner, name, descriptor))
            }
            _ => Err(EngineError::new(
                0,
                EngineErrorKind::InvalidModelState {
                    reason: format!("constant-pool entry {} is not MethodRef", index),
                },
            )),
        }
    }

    pub fn resolve_any_method_ref(&self, index: CpIndex) -> Result<(String, String, String, bool)> {
        let entry = self.entry(index.value())?;
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
                        "constant-pool entry {} is not MethodRef or InterfaceMethodRef",
                        index
                    ),
                },
            )),
        }
    }

    pub fn resolve_interface_method_ref(&self, index: CpIndex) -> Result<(String, String, String)> {
        let entry = self.entry(index.value())?;
        match entry {
            ConstantPoolEntry::InterfaceMethodRef(info) => {
                let owner = self.resolve_class_name(info.class_index)?;
                let (name, descriptor) = self.resolve_name_and_type(info.name_and_type_index)?;
                Ok((owner, name, descriptor))
            }
            _ => Err(EngineError::new(
                0,
                EngineErrorKind::InvalidModelState {
                    reason: format!("constant-pool entry {} is not InterfaceMethodRef", index),
                },
            )),
        }
    }

    pub fn resolve_method_type(&self, index: CpIndex) -> Result<String> {
        let entry = self.entry(index.value())?;
        match entry {
            ConstantPoolEntry::MethodType(info) => self.resolve_utf8(info.descriptor_index),
            _ => Err(EngineError::new(
                0,
                EngineErrorKind::InvalidModelState {
                    reason: format!("constant-pool entry {} is not MethodType", index),
                },
            )),
        }
    }

    pub fn resolve_string(&self, index: CpIndex) -> Result<String> {
        let entry = self.entry(index.value())?;
        match entry {
            ConstantPoolEntry::String(info) => self.resolve_utf8(info.string_index),
            _ => Err(EngineError::new(
                0,
                EngineErrorKind::InvalidModelState {
                    reason: format!("constant-pool entry {} is not String", index),
                },
            )),
        }
    }

    pub fn resolve_java_string_constant(&self, index: CpIndex) -> Result<JavaString> {
        let entry = self.entry(index.value())?;
        match entry {
            ConstantPoolEntry::String(info) => self.resolve_java_string(info.string_index),
            _ => Err(EngineError::new(
                0,
                EngineErrorKind::InvalidModelState {
                    reason: format!("constant-pool entry {} is not String", index),
                },
            )),
        }
    }

    fn atomic<T>(&mut self, build: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        let saved_len = self.entries.len();
        let result = build(self);
        if result.is_err() {
            self.truncate(saved_len);
        }
        result
    }

    pub fn add_entry(&mut self, entry: ConstantPoolEntry) -> Result<u16> {
        match &entry {
            ConstantPoolEntry::Utf8(info) if info.bytes.len() > UTF8_MAX_BYTES => {
                return Err(EngineError::new(
                    0,
                    EngineErrorKind::InvalidModelState {
                        reason: format!("UTF-8 payload exceeds {UTF8_MAX_BYTES} bytes"),
                    },
                ));
            }
            ConstantPoolEntry::MethodHandle(info) if !(1..=9).contains(&info.reference_kind) => {
                return Err(EngineError::new(
                    0,
                    EngineErrorKind::InvalidModelState {
                        reason: format!(
                            "invalid method handle reference kind: {}",
                            info.reference_kind
                        ),
                    },
                ));
            }
            _ => {}
        }

        let key = PoolKey::from_entry(&entry);
        if let Some(&index) = self.key_to_index.get(&key) {
            return Ok(index);
        }

        let next_index = self.entries.len();
        let is_wide = entry.is_wide();
        if is_wide {
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
        self.entries.push(Some(entry));
        self.key_to_index.insert(key, index);
        if is_wide {
            self.entries.push(None);
        }
        Ok(index)
    }

    pub fn add_utf8(&mut self, value: &str) -> Result<Utf8Index> {
        let bytes = encode_modified_utf8(value);
        self.add_utf8_bytes(bytes)
    }

    pub fn add_utf16(&mut self, value: &JavaString) -> Result<Utf8Index> {
        self.add_utf8_bytes(value.to_modified_utf8())
    }

    fn add_utf8_bytes(&mut self, bytes: Vec<u8>) -> Result<Utf8Index> {
        if bytes.len() > UTF8_MAX_BYTES {
            return Err(EngineError::new(
                0,
                EngineErrorKind::InvalidModelState {
                    reason: format!("UTF-8 payload exceeds {UTF8_MAX_BYTES} bytes"),
                },
            ));
        }
        self.add_entry(ConstantPoolEntry::Utf8(Utf8Info { bytes }))
            .map(Utf8Index::from)
    }

    pub fn add_integer(&mut self, value: i32) -> Result<CpIndex> {
        self.add_entry(ConstantPoolEntry::Integer(IntegerInfo {
            value_bits: value as u32,
        }))
        .map(CpIndex::from)
    }

    pub fn add_float_bits(&mut self, raw_bits: u32) -> Result<CpIndex> {
        self.add_entry(ConstantPoolEntry::Float(FloatInfo {
            value_bits: raw_bits,
        }))
        .map(CpIndex::from)
    }

    pub fn add_long(&mut self, value: i64) -> Result<CpIndex> {
        let raw = value as u64;
        self.add_entry(ConstantPoolEntry::Long(LongInfo {
            high_bits: (raw >> 32) as u32,
            low_bits: raw as u32,
        }))
        .map(CpIndex::from)
    }

    pub fn add_double_bits(&mut self, raw_bits: u64) -> Result<CpIndex> {
        self.add_entry(ConstantPoolEntry::Double(DoubleInfo {
            high_bits: (raw_bits >> 32) as u32,
            low_bits: raw_bits as u32,
        }))
        .map(CpIndex::from)
    }

    pub fn add_class(&mut self, class_name: &str) -> Result<ClassIndex> {
        self.atomic(|this| {
            let name_index = this.add_utf8(class_name)?;
            this.add_entry(ConstantPoolEntry::Class(ClassInfo { name_index }))
                .map(ClassIndex::from)
        })
    }

    pub fn add_string(&mut self, value: &str) -> Result<CpIndex> {
        self.atomic(|this| {
            let string_index = this.add_utf8(value)?;
            this.add_entry(ConstantPoolEntry::String(StringInfo { string_index }))
                .map(CpIndex::from)
        })
    }

    pub fn add_java_string(&mut self, value: &JavaString) -> Result<CpIndex> {
        self.atomic(|this| {
            let string_index = this.add_utf16(value)?;
            this.add_entry(ConstantPoolEntry::String(StringInfo { string_index }))
                .map(CpIndex::from)
        })
    }

    pub fn add_name_and_type(&mut self, name: &str, descriptor: &str) -> Result<NameAndTypeIndex> {
        self.atomic(|this| {
            let name_index = this.add_utf8(name)?;
            let descriptor_index = this.add_utf8(descriptor)?;
            this.add_entry(ConstantPoolEntry::NameAndType(NameAndTypeInfo {
                name_index,
                descriptor_index,
            }))
            .map(NameAndTypeIndex::from)
        })
    }

    pub fn add_field_ref(&mut self, owner: &str, name: &str, descriptor: &str) -> Result<CpIndex> {
        self.atomic(|this| {
            let class_index = this.add_class(owner)?;
            let name_and_type_index = this.add_name_and_type(name, descriptor)?;
            this.add_entry(ConstantPoolEntry::FieldRef(FieldRefInfo {
                class_index,
                name_and_type_index,
            }))
            .map(CpIndex::from)
        })
    }

    pub fn add_method_ref(&mut self, owner: &str, name: &str, descriptor: &str) -> Result<CpIndex> {
        self.atomic(|this| {
            let class_index = this.add_class(owner)?;
            let name_and_type_index = this.add_name_and_type(name, descriptor)?;
            this.add_entry(ConstantPoolEntry::MethodRef(MethodRefInfo {
                class_index,
                name_and_type_index,
            }))
            .map(CpIndex::from)
        })
    }

    pub fn add_interface_method_ref(
        &mut self,
        owner: &str,
        name: &str,
        descriptor: &str,
    ) -> Result<CpIndex> {
        self.atomic(|this| {
            let class_index = this.add_class(owner)?;
            let name_and_type_index = this.add_name_and_type(name, descriptor)?;
            this.add_entry(ConstantPoolEntry::InterfaceMethodRef(
                InterfaceMethodRefInfo {
                    class_index,
                    name_and_type_index,
                },
            ))
            .map(CpIndex::from)
        })
    }

    pub fn add_method_type(&mut self, descriptor: &str) -> Result<CpIndex> {
        self.atomic(|this| {
            let descriptor_index = this.add_utf8(descriptor)?;
            this.add_entry(ConstantPoolEntry::MethodType(MethodTypeInfo {
                descriptor_index,
            }))
            .map(CpIndex::from)
        })
    }

    pub fn add_method_handle(
        &mut self,
        reference_kind: u8,
        reference_index: CpIndex,
    ) -> Result<CpIndex> {
        self.add_entry(ConstantPoolEntry::MethodHandle(MethodHandleInfo {
            reference_kind,
            reference_index,
        }))
        .map(CpIndex::from)
    }

    pub fn add_dynamic(
        &mut self,
        bootstrap_method_attr_index: BootstrapMethodIndex,
        name: &str,
        descriptor: &str,
    ) -> Result<CpIndex> {
        self.atomic(|this| {
            let name_and_type_index = this.add_name_and_type(name, descriptor)?;
            this.add_entry(ConstantPoolEntry::Dynamic(DynamicInfo {
                bootstrap_method_attr_index,
                name_and_type_index,
            }))
            .map(CpIndex::from)
        })
    }

    pub fn add_invoke_dynamic(
        &mut self,
        bootstrap_method_attr_index: BootstrapMethodIndex,
        name: &str,
        descriptor: &str,
    ) -> Result<CpIndex> {
        self.atomic(|this| {
            let name_and_type_index = this.add_name_and_type(name, descriptor)?;
            this.add_entry(ConstantPoolEntry::InvokeDynamic(InvokeDynamicInfo {
                bootstrap_method_attr_index,
                name_and_type_index,
            }))
            .map(CpIndex::from)
        })
    }

    pub fn add_module(&mut self, name: &str) -> Result<ModuleIndex> {
        self.atomic(|this| {
            let name_index = this.add_utf8(name)?;
            this.add_entry(ConstantPoolEntry::Module(ModuleInfo { name_index }))
                .map(ModuleIndex::from)
        })
    }

    pub fn add_package(&mut self, name: &str) -> Result<PackageIndex> {
        self.atomic(|this| {
            let name_index = this.add_utf8(name)?;
            this.add_entry(ConstantPoolEntry::Package(PackageInfo { name_index }))
                .map(PackageIndex::from)
        })
    }

    pub fn try_truncate(&mut self, len: usize) -> Result<()> {
        if len == 0 {
            return Err(EngineError::new(
                0,
                EngineErrorKind::InvalidModelState {
                    reason: "cannot truncate constant pool below slot 1".to_owned(),
                },
            ));
        }
        if len > self.entries.len() {
            return Err(EngineError::new(
                0,
                EngineErrorKind::InvalidModelState {
                    reason: format!(
                        "truncate length {} exceeds constant pool size {}",
                        len,
                        self.entries.len()
                    ),
                },
            ));
        }
        if len < self.entries.len()
            && len >= 2
            && self.entries[len - 1]
                .as_ref()
                .is_some_and(ConstantPoolEntry::is_wide)
        {
            return Err(EngineError::new(
                0,
                EngineErrorKind::InvalidModelState {
                    reason: format!(
                        "truncate length {} would split wide constant-pool entry at slot {}",
                        len,
                        len - 1
                    ),
                },
            ));
        }
        if len < self.entries.len() {
            self.entries.truncate(len);
            self.key_to_index.retain(|_, v| (*v as usize) < len);
        }
        Ok(())
    }

    pub fn truncate(&mut self, len: usize) {
        let mut target = len.max(1).min(self.entries.len());
        if target < self.entries.len()
            && target >= 2
            && self.entries[target - 1]
                .as_ref()
                .is_some_and(ConstantPoolEntry::is_wide)
        {
            target -= 1;
        }
        if target < self.entries.len() {
            self.entries.truncate(target);
            self.key_to_index.retain(|_, v| (*v as usize) < target);
        }
    }

    pub fn find_utf8(&self, value: &str) -> Option<Utf8Index> {
        let bytes = encode_modified_utf8(value);
        self.key_to_index
            .get(&PoolKey::Utf8(bytes.into_boxed_slice()))
            .copied()
            .map(Utf8Index::from)
    }

    pub fn find_utf16(&self, value: &JavaString) -> Option<Utf8Index> {
        let bytes = value.to_modified_utf8();
        self.key_to_index
            .get(&PoolKey::Utf8(bytes.into_boxed_slice()))
            .copied()
            .map(Utf8Index::from)
    }

    pub fn find_integer(&self, value: i32) -> Option<CpIndex> {
        self.key_to_index
            .get(&PoolKey::Integer(value as u32))
            .copied()
            .map(CpIndex::from)
    }

    pub fn find_float_bits(&self, raw_bits: u32) -> Option<CpIndex> {
        self.key_to_index
            .get(&PoolKey::Float(raw_bits))
            .copied()
            .map(CpIndex::from)
    }

    pub fn find_long(&self, value: i64) -> Option<CpIndex> {
        let raw = value as u64;
        self.key_to_index
            .get(&PoolKey::Long((raw >> 32) as u32, raw as u32))
            .copied()
            .map(CpIndex::from)
    }

    pub fn find_double_bits(&self, raw_bits: u64) -> Option<CpIndex> {
        let high = (raw_bits >> 32) as u32;
        let low = raw_bits as u32;
        self.key_to_index
            .get(&PoolKey::Double(high, low))
            .copied()
            .map(CpIndex::from)
    }

    pub fn find_class(&self, name: &str) -> Option<ClassIndex> {
        let name_index = self.find_utf8(name)?;
        self.key_to_index
            .get(&PoolKey::Class(name_index.value()))
            .copied()
            .map(ClassIndex::from)
    }

    pub fn find_string(&self, value: &str) -> Option<CpIndex> {
        let string_index = self.find_utf8(value)?;
        self.key_to_index
            .get(&PoolKey::String(string_index.value()))
            .copied()
            .map(CpIndex::from)
    }

    pub fn find_java_string(&self, value: &JavaString) -> Option<CpIndex> {
        let string_index = self.find_utf16(value)?;
        self.key_to_index
            .get(&PoolKey::String(string_index.value()))
            .copied()
            .map(CpIndex::from)
    }

    pub fn find_method_type(&self, descriptor: &str) -> Option<CpIndex> {
        let descriptor_index = self.find_utf8(descriptor)?;
        self.key_to_index
            .get(&PoolKey::MethodType(descriptor_index.value()))
            .copied()
            .map(CpIndex::from)
    }

    pub fn find_name_and_type(&self, name: &str, descriptor: &str) -> Option<NameAndTypeIndex> {
        let name_index = self.find_utf8(name)?;
        let descriptor_index = self.find_utf8(descriptor)?;
        self.key_to_index
            .get(&PoolKey::NameAndType(
                name_index.value(),
                descriptor_index.value(),
            ))
            .copied()
            .map(NameAndTypeIndex::from)
    }

    pub fn find_field_ref(&self, owner: &str, name: &str, descriptor: &str) -> Option<CpIndex> {
        let class_index = self.find_class(owner)?;
        let nat_index = self.find_name_and_type(name, descriptor)?;
        self.key_to_index
            .get(&PoolKey::FieldRef(class_index.value(), nat_index.value()))
            .copied()
            .map(CpIndex::from)
    }

    pub fn find_method_ref(&self, owner: &str, name: &str, descriptor: &str) -> Option<CpIndex> {
        let class_index = self.find_class(owner)?;
        let nat_index = self.find_name_and_type(name, descriptor)?;
        self.key_to_index
            .get(&PoolKey::MethodRef(class_index.value(), nat_index.value()))
            .copied()
            .map(CpIndex::from)
    }

    pub fn find_interface_method_ref(
        &self,
        owner: &str,
        name: &str,
        descriptor: &str,
    ) -> Option<CpIndex> {
        let class_index = self.find_class(owner)?;
        let nat_index = self.find_name_and_type(name, descriptor)?;
        self.key_to_index
            .get(&PoolKey::InterfaceMethodRef(
                class_index.value(),
                nat_index.value(),
            ))
            .copied()
            .map(CpIndex::from)
    }

    pub fn find_method_handle(
        &self,
        reference_kind: u8,
        reference_index: CpIndex,
    ) -> Option<CpIndex> {
        self.key_to_index
            .get(&PoolKey::MethodHandle(
                reference_kind,
                reference_index.value(),
            ))
            .copied()
            .map(CpIndex::from)
    }

    pub fn find_dynamic(
        &self,
        bootstrap_method_attr_index: BootstrapMethodIndex,
        name: &str,
        descriptor: &str,
    ) -> Option<CpIndex> {
        let nat_index = self.find_name_and_type(name, descriptor)?;
        self.key_to_index
            .get(&PoolKey::Dynamic(
                bootstrap_method_attr_index.value(),
                nat_index.value(),
            ))
            .copied()
            .map(CpIndex::from)
    }

    pub fn find_invoke_dynamic(
        &self,
        bootstrap_method_attr_index: BootstrapMethodIndex,
        name: &str,
        descriptor: &str,
    ) -> Option<CpIndex> {
        let nat_index = self.find_name_and_type(name, descriptor)?;
        self.key_to_index
            .get(&PoolKey::InvokeDynamic(
                bootstrap_method_attr_index.value(),
                nat_index.value(),
            ))
            .copied()
            .map(CpIndex::from)
    }

    pub fn find_module(&self, name: &str) -> Option<ModuleIndex> {
        let name_index = self.find_utf8(name)?;
        self.key_to_index
            .get(&PoolKey::Module(name_index.value()))
            .copied()
            .map(ModuleIndex::from)
    }

    pub fn find_package(&self, name: &str) -> Option<PackageIndex> {
        let name_index = self.find_utf8(name)?;
        self.key_to_index
            .get(&PoolKey::Package(name_index.value()))
            .copied()
            .map(PackageIndex::from)
    }
}

fn entry_dependency_tier(entry: &ConstantPoolEntry) -> u8 {
    match entry {
        ConstantPoolEntry::Utf8(_)
        | ConstantPoolEntry::Integer(_)
        | ConstantPoolEntry::Float(_)
        | ConstantPoolEntry::Long(_)
        | ConstantPoolEntry::Double(_) => 1,
        ConstantPoolEntry::Class(_)
        | ConstantPoolEntry::String(_)
        | ConstantPoolEntry::MethodType(_)
        | ConstantPoolEntry::NameAndType(_)
        | ConstantPoolEntry::Module(_)
        | ConstantPoolEntry::Package(_) => 2,
        ConstantPoolEntry::FieldRef(_)
        | ConstantPoolEntry::MethodRef(_)
        | ConstantPoolEntry::InterfaceMethodRef(_)
        | ConstantPoolEntry::Dynamic(_)
        | ConstantPoolEntry::InvokeDynamic(_) => 3,
        ConstantPoolEntry::MethodHandle(_) => 4,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum PoolKey {
    Utf8(Box<[u8]>),
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
            ConstantPoolEntry::Utf8(info) => Self::Utf8(info.bytes.clone().into_boxed_slice()),
            ConstantPoolEntry::Integer(info) => Self::Integer(info.value_bits),
            ConstantPoolEntry::Float(info) => Self::Float(info.value_bits),
            ConstantPoolEntry::Long(info) => Self::Long(info.high_bits, info.low_bits),
            ConstantPoolEntry::Double(info) => Self::Double(info.high_bits, info.low_bits),
            ConstantPoolEntry::Class(info) => Self::Class(info.name_index.value()),
            ConstantPoolEntry::String(info) => Self::String(info.string_index.value()),
            ConstantPoolEntry::FieldRef(info) => {
                Self::FieldRef(info.class_index.value(), info.name_and_type_index.value())
            }
            ConstantPoolEntry::MethodRef(info) => {
                Self::MethodRef(info.class_index.value(), info.name_and_type_index.value())
            }
            ConstantPoolEntry::InterfaceMethodRef(info) => {
                Self::InterfaceMethodRef(info.class_index.value(), info.name_and_type_index.value())
            }
            ConstantPoolEntry::NameAndType(info) => {
                Self::NameAndType(info.name_index.value(), info.descriptor_index.value())
            }
            ConstantPoolEntry::MethodHandle(info) => {
                Self::MethodHandle(info.reference_kind, info.reference_index.value())
            }
            ConstantPoolEntry::MethodType(info) => Self::MethodType(info.descriptor_index.value()),
            ConstantPoolEntry::Dynamic(info) => Self::Dynamic(
                info.bootstrap_method_attr_index.value(),
                info.name_and_type_index.value(),
            ),
            ConstantPoolEntry::InvokeDynamic(info) => Self::InvokeDynamic(
                info.bootstrap_method_attr_index.value(),
                info.name_and_type_index.value(),
            ),
            ConstantPoolEntry::Module(info) => Self::Module(info.name_index.value()),
            ConstantPoolEntry::Package(info) => Self::Package(info.name_index.value()),
        }
    }

    /// Compute the canonical key for a compound entry by replacing each stored sub-index with its
    /// canonical equivalent (looked up via `index_map[raw_idx]`).  For leaf entries (Utf8,
    /// Integer, etc.) the key is already canonical, so the same key is returned.
    fn canonical_from_entry(entry: &ConstantPoolEntry, index_map: &[u16]) -> Self {
        let remap = |idx: u16| -> u16 {
            index_map
                .get(idx as usize)
                .copied()
                .filter(|&c| c != 0)
                .unwrap_or(idx)
        };
        match entry {
            ConstantPoolEntry::Class(info) => Self::Class(remap(info.name_index.value())),
            ConstantPoolEntry::String(info) => Self::String(remap(info.string_index.value())),
            ConstantPoolEntry::FieldRef(info) => Self::FieldRef(
                remap(info.class_index.value()),
                remap(info.name_and_type_index.value()),
            ),
            ConstantPoolEntry::MethodRef(info) => Self::MethodRef(
                remap(info.class_index.value()),
                remap(info.name_and_type_index.value()),
            ),
            ConstantPoolEntry::InterfaceMethodRef(info) => Self::InterfaceMethodRef(
                remap(info.class_index.value()),
                remap(info.name_and_type_index.value()),
            ),
            ConstantPoolEntry::NameAndType(info) => Self::NameAndType(
                remap(info.name_index.value()),
                remap(info.descriptor_index.value()),
            ),
            ConstantPoolEntry::MethodHandle(info) => {
                Self::MethodHandle(info.reference_kind, remap(info.reference_index.value()))
            }
            ConstantPoolEntry::MethodType(info) => {
                Self::MethodType(remap(info.descriptor_index.value()))
            }
            ConstantPoolEntry::Dynamic(info) => Self::Dynamic(
                info.bootstrap_method_attr_index.value(),
                remap(info.name_and_type_index.value()),
            ),
            ConstantPoolEntry::InvokeDynamic(info) => Self::InvokeDynamic(
                info.bootstrap_method_attr_index.value(),
                remap(info.name_and_type_index.value()),
            ),
            ConstantPoolEntry::Module(info) => Self::Module(remap(info.name_index.value())),
            ConstantPoolEntry::Package(info) => Self::Package(remap(info.name_index.value())),
            // Leaf entries: canonical key == raw key
            _ => Self::from_entry(entry),
        }
    }
}
