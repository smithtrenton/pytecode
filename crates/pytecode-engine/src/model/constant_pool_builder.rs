use crate::indexes::{
    BootstrapMethodIndex, ClassIndex, CpIndex, ModuleIndex, NameAndTypeIndex, PackageIndex,
    Utf8Index,
};
use crate::modified_utf8::{decode_modified_utf8, encode_modified_utf8};
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
        let mut builder = Self {
            entries: pool.to_vec(),
            key_to_index: FxHashMap::default(),
        };
        // First pass: seed key_to_index with the raw (as-stored) keys. or_insert means the first
        // occurrence of a duplicate entry wins (i.e. the lower index is canonical).
        for (index, entry) in builder.entries.iter().enumerate().skip(1) {
            let Some(entry) = entry else {
                continue;
            };
            builder
                .key_to_index
                .entry(PoolKey::from_entry(entry))
                .or_insert(index as u16);
        }

        // Build a raw-index → canonical-index lookup table.  For every entry at position `i`, the
        // canonical index is whatever `key_to_index` mapped to for that entry's raw key.  For
        // duplicate entries the first (lower) index is the canonical one; later duplicates map back
        // to that same slot.
        let index_map: Vec<u16> = builder
            .entries
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                if i == 0 {
                    return 0;
                }
                match entry {
                    None => 0,
                    Some(e) => *builder
                        .key_to_index
                        .get(&PoolKey::from_entry(e))
                        .unwrap_or(&(i as u16)),
                }
            })
            .collect();

        // Second pass: add canonical aliases for compound entries.
        //
        // Problem this solves: if the original class file has two Class entries both pointing to
        // the same Utf8 (e.g. Class@3 and Class@45 → Utf8@21), `or_insert` above maps
        // Class(21) → 3.  But a Methodref in the original CP might use class_index=45, giving it
        // the raw key MethodRef(45, 46).  When `add_method_ref()` is called later it resolves the
        // class name through `add_class()`, which returns index 3 (the canonical class), and then
        // tries to look up MethodRef(3, 46) — which is NOT in the map — and adds a new duplicate.
        //
        // By inserting the canonical key into the map here (pointing to the same output index as
        // the original entry), the later lookup finds the existing entry and no duplicate is added.
        for (index, entry) in builder.entries.iter().enumerate().skip(1) {
            let Some(entry) = entry else {
                continue;
            };
            let canonical_key = PoolKey::canonical_from_entry(entry, &index_map);
            let raw_key = PoolKey::from_entry(entry);
            if canonical_key != raw_key {
                builder
                    .key_to_index
                    .entry(canonical_key)
                    .or_insert(index as u16);
            }
        }

        if builder.entries.is_empty() {
            builder.entries.push(None);
        }
        builder
    }

    pub fn build(&self) -> Vec<Option<ConstantPoolEntry>> {
        self.entries.clone()
    }

    pub fn entries(&self) -> &[Option<ConstantPoolEntry>] {
        &self.entries
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

    pub fn add_entry(&mut self, entry: ConstantPoolEntry) -> Result<u16> {
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

    pub fn add_integer(&mut self, value: u32) -> Result<CpIndex> {
        self.add_entry(ConstantPoolEntry::Integer(IntegerInfo {
            value_bytes: value,
        }))
        .map(CpIndex::from)
    }

    pub fn add_float_bits(&mut self, raw_bits: u32) -> Result<CpIndex> {
        self.add_entry(ConstantPoolEntry::Float(FloatInfo {
            value_bytes: raw_bits,
        }))
        .map(CpIndex::from)
    }

    pub fn add_long(&mut self, value: u64) -> Result<CpIndex> {
        self.add_entry(ConstantPoolEntry::Long(LongInfo {
            high_bytes: (value >> 32) as u32,
            low_bytes: value as u32,
        }))
        .map(CpIndex::from)
    }

    pub fn add_double_bits(&mut self, raw_bits: u64) -> Result<CpIndex> {
        self.add_entry(ConstantPoolEntry::Double(DoubleInfo {
            high_bytes: (raw_bits >> 32) as u32,
            low_bytes: raw_bits as u32,
        }))
        .map(CpIndex::from)
    }

    pub fn add_class(&mut self, class_name: &str) -> Result<ClassIndex> {
        let name_index = self.add_utf8(class_name)?;
        self.add_entry(ConstantPoolEntry::Class(ClassInfo { name_index }))
            .map(ClassIndex::from)
    }

    pub fn add_string(&mut self, value: &str) -> Result<CpIndex> {
        let string_index = self.add_utf8(value)?;
        self.add_entry(ConstantPoolEntry::String(StringInfo { string_index }))
            .map(CpIndex::from)
    }

    pub fn add_name_and_type(&mut self, name: &str, descriptor: &str) -> Result<NameAndTypeIndex> {
        let name_index = self.add_utf8(name)?;
        let descriptor_index = self.add_utf8(descriptor)?;
        self.add_entry(ConstantPoolEntry::NameAndType(NameAndTypeInfo {
            name_index,
            descriptor_index,
        }))
        .map(NameAndTypeIndex::from)
    }

    pub fn add_field_ref(&mut self, owner: &str, name: &str, descriptor: &str) -> Result<CpIndex> {
        let class_index = self.add_class(owner)?;
        let name_and_type_index = self.add_name_and_type(name, descriptor)?;
        self.add_entry(ConstantPoolEntry::FieldRef(FieldRefInfo {
            class_index,
            name_and_type_index,
        }))
        .map(CpIndex::from)
    }

    pub fn add_method_ref(&mut self, owner: &str, name: &str, descriptor: &str) -> Result<CpIndex> {
        let class_index = self.add_class(owner)?;
        let name_and_type_index = self.add_name_and_type(name, descriptor)?;
        self.add_entry(ConstantPoolEntry::MethodRef(MethodRefInfo {
            class_index,
            name_and_type_index,
        }))
        .map(CpIndex::from)
    }

    pub fn add_interface_method_ref(
        &mut self,
        owner: &str,
        name: &str,
        descriptor: &str,
    ) -> Result<CpIndex> {
        let class_index = self.add_class(owner)?;
        let name_and_type_index = self.add_name_and_type(name, descriptor)?;
        self.add_entry(ConstantPoolEntry::InterfaceMethodRef(
            InterfaceMethodRefInfo {
                class_index,
                name_and_type_index,
            },
        ))
        .map(CpIndex::from)
    }

    pub fn add_method_type(&mut self, descriptor: &str) -> Result<CpIndex> {
        let descriptor_index = self.add_utf8(descriptor)?;
        self.add_entry(ConstantPoolEntry::MethodType(MethodTypeInfo {
            descriptor_index,
        }))
        .map(CpIndex::from)
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
        let name_and_type_index = self.add_name_and_type(name, descriptor)?;
        self.add_entry(ConstantPoolEntry::Dynamic(DynamicInfo {
            bootstrap_method_attr_index,
            name_and_type_index,
        }))
        .map(CpIndex::from)
    }

    pub fn add_invoke_dynamic(
        &mut self,
        bootstrap_method_attr_index: BootstrapMethodIndex,
        name: &str,
        descriptor: &str,
    ) -> Result<CpIndex> {
        let name_and_type_index = self.add_name_and_type(name, descriptor)?;
        self.add_entry(ConstantPoolEntry::InvokeDynamic(InvokeDynamicInfo {
            bootstrap_method_attr_index,
            name_and_type_index,
        }))
        .map(CpIndex::from)
    }

    pub fn add_module(&mut self, name: &str) -> Result<ModuleIndex> {
        let name_index = self.add_utf8(name)?;
        self.add_entry(ConstantPoolEntry::Module(ModuleInfo { name_index }))
            .map(ModuleIndex::from)
    }

    pub fn add_package(&mut self, name: &str) -> Result<PackageIndex> {
        let name_index = self.add_utf8(name)?;
        self.add_entry(ConstantPoolEntry::Package(PackageInfo { name_index }))
            .map(PackageIndex::from)
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
            ConstantPoolEntry::Integer(info) => Self::Integer(info.value_bytes),
            ConstantPoolEntry::Float(info) => Self::Float(info.value_bytes),
            ConstantPoolEntry::Long(info) => Self::Long(info.high_bytes, info.low_bytes),
            ConstantPoolEntry::Double(info) => Self::Double(info.high_bytes, info.low_bytes),
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
