//! Canonical constant-pool emission.
//!
//! Lowering normally keeps the class's original constant pool and appends
//! whatever an edit needs, which is what makes unchanged classes round-trip
//! byte-for-byte. The pool layout is therefore a record of the class's
//! history: two classes with identical content but different pool orders
//! (an obfuscator shuffling each build, or the same edits applied in a
//! different order) lower to different bytes.
//!
//! [`ConstantPoolPolicy::Canonical`] removes that history. The pool is
//! rebuilt from the entries the lowered class actually references, ordered
//! by content alone, and the class is lowered again against it, so classes
//! with identical content lower to identical bytes. Only the pool is
//! canonicalized: the order of fields, methods, attributes, and instructions
//! is content and is kept as-is.

use super::{
    ClassModel, CodeItem, CodeModel, CodeTypeAnnotation, ConstantPoolBuilder, DebugInfoPolicy,
    FrameComputationMode, MethodBody, compute_code_layout, model_error, stack_map,
};
use crate::Result;
use crate::raw::{
    AnnotationInfo, AttributeInfo, ClassFile, ConstantPoolEntry, ElementValueInfo,
    ElementValuePairInfo, Instruction, StackMapFrameInfo, VerificationTypeInfo,
};
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;
use std::rc::Rc;

/// How lowering lays out the constant pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConstantPoolPolicy {
    /// Keep the original pool and append new entries (the default). Unchanged
    /// classes round-trip byte-for-byte; unreferenced entries are kept.
    #[default]
    Preserve,
    /// Rebuild the pool from referenced entries only, deduplicated and
    /// ordered by content, so classes with identical content lower to
    /// identical bytes regardless of how their pools were built.
    ///
    /// Covers the constant pool and what its layout determines: `ldc` versus
    /// `ldc_w` (narrow whenever the canonical index fits) and the offsets of
    /// preserved `StackMapTable` frames, which are relocated and written in
    /// their most compact form. The order of fields, methods, attributes, and
    /// instructions is out of scope and kept as-is. Classes carrying an
    /// attribute pytecode does not parse are rejected, since its payload may
    /// hold constant-pool indices that cannot be remapped.
    Canonical,
}

/// Rewrite `classfile` with a canonical constant pool.
///
/// The result references exactly the same constants, in a pool that holds
/// only referenced entries, deduplicated and ordered by content. See
/// [`ConstantPoolPolicy::Canonical`].
pub fn canonicalize_constant_pool(classfile: &ClassFile) -> Result<ClassFile> {
    let pool = &classfile.constant_pool;

    let mut roots = Vec::new();
    visit_classfile(&mut classfile.clone(), &mut |index: &mut u16| {
        roots.push(*index);
        Ok(())
    })?;
    let loaded = ldc_operands(classfile);

    let mut reachable = vec![false; pool.len()];
    let mut pending = roots;
    while let Some(index) = pending.pop() {
        if index == 0 || reachable.get(usize::from(index)).copied().unwrap_or(false) {
            continue;
        }
        let mut entry = pool_entry(pool, index)?.clone();
        reachable[usize::from(index)] = true;
        visit_entry(&mut entry, &mut |sub: &mut u16| {
            pending.push(*sub);
            Ok(())
        })?;
    }

    // Order unique contents: constants loaded by `ldc`/`ldc_w` first, so they
    // get the low indices a narrow `ldc` can address, then everything else.
    let mut keys = KeyCache::new(pool);
    let mut order: BTreeMap<Rc<EntryKey>, (u8, u16)> = BTreeMap::new();
    for index in (1..pool.len()).filter(|index| reachable[*index]) {
        let index = index as u16;
        let key = keys.key(index, 0)?;
        let group = u8::from(!loaded.contains(&index));
        order
            .entry(key)
            .and_modify(|slot| slot.0 = slot.0.min(group))
            .or_insert((group, index));
    }
    let mut ordered = order.into_iter().collect::<Vec<_>>();
    ordered.sort_by(
        |(left_key, (left_group, _)), (right_key, (right_group, _))| {
            left_group
                .cmp(right_group)
                .then_with(|| left_key.cmp(right_key))
        },
    );

    let mut new_index_of_key: FxHashMap<Rc<EntryKey>, u16> = FxHashMap::default();
    let mut next = 1_usize;
    for (key, (_, representative)) in &ordered {
        if next > usize::from(u16::MAX) - 1 {
            return Err(model_error("canonical constant pool overflows 65535 slots"));
        }
        new_index_of_key.insert(Rc::clone(key), next as u16);
        next += if pool_entry(pool, *representative)?.is_wide() {
            2
        } else {
            1
        };
    }
    let mut remap = vec![0_u16; pool.len()];
    for index in (1..pool.len()).filter(|index| reachable[*index]) {
        remap[index] = new_index_of_key[&keys.key(index as u16, 0)?];
    }
    let mut map = |index: &mut u16| -> Result<()> {
        if *index != 0 {
            *index = remap
                .get(usize::from(*index))
                .copied()
                .filter(|mapped| *mapped != 0)
                .ok_or_else(|| {
                    model_error(format!(
                        "constant-pool index {index} was not reached while canonicalizing"
                    ))
                })?;
        }
        Ok(())
    };

    let mut entries: Vec<Option<ConstantPoolEntry>> = Vec::with_capacity(next);
    entries.push(None);
    for (_, (_, representative)) in &ordered {
        let mut entry = pool_entry(pool, *representative)?.clone();
        visit_entry(&mut entry, &mut map)?;
        let wide = entry.is_wide();
        entries.push(Some(entry));
        if wide {
            entries.push(None);
        }
    }

    // Lower again against the canonical pool: symbolic content re-encodes by
    // value (which also picks `ldc` versus `ldc_w` from the new indices), and
    // the raw indices the model still carries are remapped.
    let mut model = ClassModel::from_classfile(classfile)?;
    remap_model(&mut model, &mut map)?;
    model.constant_pool = ConstantPoolBuilder::from_pool(&entries);
    relocate_preserved_frames(&mut model)?;
    let (lowered, _) = model.to_classfile_with_report(
        DebugInfoPolicy::Preserve,
        FrameComputationMode::Preserve,
        None,
    )?;
    if lowered.constant_pool.len() != entries.len() {
        return Err(model_error(
            "canonical lowering needed constant-pool entries the source class did not reference",
        ));
    }
    Ok(lowered)
}

fn pool_entry(pool: &[Option<ConstantPoolEntry>], index: u16) -> Result<&ConstantPoolEntry> {
    pool.get(usize::from(index))
        .and_then(Option::as_ref)
        .ok_or_else(|| model_error(format!("constant-pool index {index} is not a valid entry")))
}

/// Pool indices loaded by `ldc` or `ldc_w`.
fn ldc_operands(classfile: &ClassFile) -> rustc_hash::FxHashSet<u16> {
    let mut loaded = rustc_hash::FxHashSet::default();
    for method in &classfile.methods {
        for attribute in &method.attributes {
            let AttributeInfo::Code(code) = attribute else {
                continue;
            };
            for instruction in &code.code {
                match instruction {
                    Instruction::ConstantPoolIndex1 {
                        opcode: 0x12,
                        index,
                        ..
                    } => {
                        loaded.insert(u16::from(*index));
                    }
                    Instruction::ConstantPoolIndexWide(insn) if insn.opcode == 0x13 => {
                        loaded.insert(insn.index.value());
                    }
                    _ => {}
                }
            }
        }
    }
    loaded
}

/// A pool entry's content with every reference resolved, so equal constants
/// compare equal however their pools were laid out. The derived order (by
/// variant, then content) is the canonical pool order.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum EntryKey {
    Utf8(Vec<u8>),
    Integer(u32),
    Float(u32),
    Long(u32, u32),
    Double(u32, u32),
    Class(Rc<EntryKey>),
    String(Rc<EntryKey>),
    MethodType(Rc<EntryKey>),
    Module(Rc<EntryKey>),
    Package(Rc<EntryKey>),
    NameAndType(Rc<EntryKey>, Rc<EntryKey>),
    FieldRef(Rc<EntryKey>, Rc<EntryKey>),
    MethodRef(Rc<EntryKey>, Rc<EntryKey>),
    InterfaceMethodRef(Rc<EntryKey>, Rc<EntryKey>),
    MethodHandle(u8, Rc<EntryKey>),
    Dynamic(u16, Rc<EntryKey>),
    InvokeDynamic(u16, Rc<EntryKey>),
}

struct KeyCache<'a> {
    pool: &'a [Option<ConstantPoolEntry>],
    keys: Vec<Option<Rc<EntryKey>>>,
}

impl<'a> KeyCache<'a> {
    fn new(pool: &'a [Option<ConstantPoolEntry>]) -> Self {
        Self {
            pool,
            keys: vec![None; pool.len()],
        }
    }

    fn key(&mut self, index: u16, depth: u8) -> Result<Rc<EntryKey>> {
        if let Some(Some(key)) = self.keys.get(usize::from(index)) {
            return Ok(Rc::clone(key));
        }
        // Valid pools nest at most three references deep (a method handle's
        // member reference's name-and-type's Utf8).
        if depth > 4 {
            return Err(model_error(format!(
                "constant-pool entry {index} is part of a reference cycle"
            )));
        }
        let depth = depth + 1;
        let key = match pool_entry(self.pool, index)? {
            ConstantPoolEntry::Utf8(info) => EntryKey::Utf8(info.bytes.clone()),
            ConstantPoolEntry::Integer(info) => EntryKey::Integer(info.value_bits),
            ConstantPoolEntry::Float(info) => EntryKey::Float(info.value_bits),
            ConstantPoolEntry::Long(info) => EntryKey::Long(info.high_bits, info.low_bits),
            ConstantPoolEntry::Double(info) => EntryKey::Double(info.high_bits, info.low_bits),
            ConstantPoolEntry::Class(info) => EntryKey::Class(self.key(info.name_index.0, depth)?),
            ConstantPoolEntry::String(info) => {
                EntryKey::String(self.key(info.string_index.0, depth)?)
            }
            ConstantPoolEntry::MethodType(info) => {
                EntryKey::MethodType(self.key(info.descriptor_index.0, depth)?)
            }
            ConstantPoolEntry::Module(info) => {
                EntryKey::Module(self.key(info.name_index.0, depth)?)
            }
            ConstantPoolEntry::Package(info) => {
                EntryKey::Package(self.key(info.name_index.0, depth)?)
            }
            ConstantPoolEntry::NameAndType(info) => EntryKey::NameAndType(
                self.key(info.name_index.0, depth)?,
                self.key(info.descriptor_index.0, depth)?,
            ),
            ConstantPoolEntry::FieldRef(info) => EntryKey::FieldRef(
                self.key(info.class_index.0, depth)?,
                self.key(info.name_and_type_index.0, depth)?,
            ),
            ConstantPoolEntry::MethodRef(info) => EntryKey::MethodRef(
                self.key(info.class_index.0, depth)?,
                self.key(info.name_and_type_index.0, depth)?,
            ),
            ConstantPoolEntry::InterfaceMethodRef(info) => EntryKey::InterfaceMethodRef(
                self.key(info.class_index.0, depth)?,
                self.key(info.name_and_type_index.0, depth)?,
            ),
            ConstantPoolEntry::MethodHandle(info) => EntryKey::MethodHandle(
                info.reference_kind,
                self.key(info.reference_index.0, depth)?,
            ),
            ConstantPoolEntry::Dynamic(info) => EntryKey::Dynamic(
                info.bootstrap_method_attr_index.0,
                self.key(info.name_and_type_index.0, depth)?,
            ),
            ConstantPoolEntry::InvokeDynamic(info) => EntryKey::InvokeDynamic(
                info.bootstrap_method_attr_index.0,
                self.key(info.name_and_type_index.0, depth)?,
            ),
        };
        let key = Rc::new(key);
        self.keys[usize::from(index)] = Some(Rc::clone(&key));
        Ok(key)
    }
}

/// Callback receiving every constant-pool index field; `0` means "none".
type IndexVisitor<'a> = dyn FnMut(&mut u16) -> Result<()> + 'a;

/// Visit the pool references inside one pool entry.
fn visit_entry(entry: &mut ConstantPoolEntry, f: &mut IndexVisitor<'_>) -> Result<()> {
    match entry {
        ConstantPoolEntry::Utf8(_)
        | ConstantPoolEntry::Integer(_)
        | ConstantPoolEntry::Float(_)
        | ConstantPoolEntry::Long(_)
        | ConstantPoolEntry::Double(_) => Ok(()),
        ConstantPoolEntry::Class(info) => f(&mut info.name_index.0),
        ConstantPoolEntry::String(info) => f(&mut info.string_index.0),
        ConstantPoolEntry::MethodType(info) => f(&mut info.descriptor_index.0),
        ConstantPoolEntry::Module(info) => f(&mut info.name_index.0),
        ConstantPoolEntry::Package(info) => f(&mut info.name_index.0),
        ConstantPoolEntry::NameAndType(info) => {
            f(&mut info.name_index.0)?;
            f(&mut info.descriptor_index.0)
        }
        ConstantPoolEntry::FieldRef(info) => {
            f(&mut info.class_index.0)?;
            f(&mut info.name_and_type_index.0)
        }
        ConstantPoolEntry::MethodRef(info) => {
            f(&mut info.class_index.0)?;
            f(&mut info.name_and_type_index.0)
        }
        ConstantPoolEntry::InterfaceMethodRef(info) => {
            f(&mut info.class_index.0)?;
            f(&mut info.name_and_type_index.0)
        }
        ConstantPoolEntry::MethodHandle(info) => f(&mut info.reference_index.0),
        ConstantPoolEntry::Dynamic(info) => f(&mut info.name_and_type_index.0),
        ConstantPoolEntry::InvokeDynamic(info) => f(&mut info.name_and_type_index.0),
    }
}

/// Visit every pool reference a class file makes outside its pool.
fn visit_classfile(classfile: &mut ClassFile, f: &mut IndexVisitor<'_>) -> Result<()> {
    f(&mut classfile.this_class.0)?;
    f(&mut classfile.super_class.0)?;
    for interface in &mut classfile.interfaces {
        f(&mut interface.0)?;
    }
    for field in &mut classfile.fields {
        f(&mut field.name_index.0)?;
        f(&mut field.descriptor_index.0)?;
        visit_attributes(&mut field.attributes, f)?;
    }
    for method in &mut classfile.methods {
        f(&mut method.name_index.0)?;
        f(&mut method.descriptor_index.0)?;
        visit_attributes(&mut method.attributes, f)?;
    }
    visit_attributes(&mut classfile.attributes, f)
}

/// Visit the raw pool references a lifted model still carries; everything
/// else in the model is symbolic and re-encodes by value.
fn remap_model(model: &mut ClassModel, f: &mut IndexVisitor<'_>) -> Result<()> {
    visit_attributes(&mut model.attributes, f)?;
    for field in &mut model.fields {
        visit_attributes(&mut field.attributes, f)?;
    }
    for method in &mut model.methods {
        visit_attributes(&mut method.attributes, f)?;
        match &mut method.body {
            Some(MethodBody::Symbolic(code)) => remap_code(code, f)?,
            Some(MethodBody::Prebuilt(_)) => {
                return Err(model_error(
                    "canonical lowering cannot remap a prebuilt method body",
                ));
            }
            None => {}
        }
    }
    Ok(())
}

fn remap_code(code: &mut CodeModel, f: &mut IndexVisitor<'_>) -> Result<()> {
    visit_attributes(&mut code.attributes, f)?;
    for item in &mut code.instructions {
        if let CodeItem::Raw(instruction) = item {
            visit_instruction(instruction, f)?;
        }
    }
    if let Some(original) = &mut code.original_code_shape {
        for item in &mut original.instructions {
            if let CodeItem::Raw(instruction) = item {
                visit_instruction(instruction, f)?;
            }
        }
    }
    for annotation in code
        .visible_type_annotations
        .iter_mut()
        .chain(&mut code.invisible_type_annotations)
    {
        visit_code_type_annotation(annotation, f)?;
    }
    Ok(())
}

fn visit_code_type_annotation(
    annotation: &mut CodeTypeAnnotation,
    f: &mut IndexVisitor<'_>,
) -> Result<()> {
    f(&mut annotation.type_index.0)?;
    visit_element_value_pairs(&mut annotation.element_value_pairs, f)
}

fn visit_instruction(instruction: &mut Instruction, f: &mut IndexVisitor<'_>) -> Result<()> {
    match instruction {
        Instruction::ConstantPoolIndex1 { index, .. } => {
            let mut wide = u16::from(*index);
            f(&mut wide)?;
            *index = u8::try_from(wide).map_err(|_| {
                model_error(format!(
                    "narrow ldc operand moved to constant-pool index {wide}, beyond 255"
                ))
            })?;
            Ok(())
        }
        Instruction::ConstantPoolIndexWide(insn) => f(&mut insn.index.0),
        Instruction::InvokeDynamic(insn) => f(&mut insn.index.0),
        Instruction::InvokeInterface(insn) => f(&mut insn.index.0),
        Instruction::MultiANewArray { index, .. } => f(&mut index.0),
        Instruction::Simple { .. }
        | Instruction::LocalIndex { .. }
        | Instruction::Byte { .. }
        | Instruction::Short { .. }
        | Instruction::Branch(_)
        | Instruction::BranchWide { .. }
        | Instruction::IInc { .. }
        | Instruction::NewArray(_)
        | Instruction::LookupSwitch(_)
        | Instruction::TableSwitch(_)
        | Instruction::Wide(_) => Ok(()),
    }
}

fn visit_attributes(attributes: &mut [AttributeInfo], f: &mut IndexVisitor<'_>) -> Result<()> {
    attributes
        .iter_mut()
        .try_for_each(|attribute| visit_attribute(attribute, f))
}

fn visit_attribute(attribute: &mut AttributeInfo, f: &mut IndexVisitor<'_>) -> Result<()> {
    match attribute {
        AttributeInfo::ConstantValue(attr) => {
            f(&mut attr.attribute_name_index.0)?;
            f(&mut attr.constantvalue_index.0)
        }
        AttributeInfo::Signature(attr) => {
            f(&mut attr.attribute_name_index.0)?;
            f(&mut attr.signature_index.0)
        }
        AttributeInfo::SourceFile(attr) => {
            f(&mut attr.attribute_name_index.0)?;
            f(&mut attr.sourcefile_index.0)
        }
        AttributeInfo::SourceDebugExtension(attr) => f(&mut attr.attribute_name_index.0),
        AttributeInfo::Synthetic(attr) => f(&mut attr.attribute_name_index.0),
        AttributeInfo::Deprecated(attr) => f(&mut attr.attribute_name_index.0),
        AttributeInfo::StackMapTable(attr) => {
            f(&mut attr.attribute_name_index.0)?;
            for frame in &mut attr.entries {
                for vtype in frame_types_mut(frame) {
                    if let VerificationTypeInfo::Object { cpool_index } = vtype {
                        f(&mut cpool_index.0)?;
                    }
                }
            }
            Ok(())
        }
        AttributeInfo::Exceptions(attr) => {
            f(&mut attr.attribute_name_index.0)?;
            attr.exception_index_table
                .iter_mut()
                .try_for_each(|index| f(&mut index.0))
        }
        AttributeInfo::InnerClasses(attr) => {
            f(&mut attr.attribute_name_index.0)?;
            for class in &mut attr.classes {
                f(&mut class.inner_class_info_index.0)?;
                f(&mut class.outer_class_info_index.0)?;
                f(&mut class.inner_name_index.0)?;
            }
            Ok(())
        }
        AttributeInfo::EnclosingMethod(attr) => {
            f(&mut attr.attribute_name_index.0)?;
            f(&mut attr.class_index.0)?;
            f(&mut attr.method_index.0)
        }
        AttributeInfo::Code(attr) => {
            f(&mut attr.attribute_name_index.0)?;
            for instruction in &mut attr.code {
                visit_instruction(instruction, f)?;
            }
            for handler in &mut attr.exception_table {
                f(&mut handler.catch_type.0)?;
            }
            visit_attributes(&mut attr.attributes, f)
        }
        AttributeInfo::LineNumberTable(attr) => f(&mut attr.attribute_name_index.0),
        AttributeInfo::LocalVariableTable(attr) => {
            f(&mut attr.attribute_name_index.0)?;
            for entry in &mut attr.local_variable_table {
                f(&mut entry.name_index.0)?;
                f(&mut entry.descriptor_index.0)?;
            }
            Ok(())
        }
        AttributeInfo::LocalVariableTypeTable(attr) => {
            f(&mut attr.attribute_name_index.0)?;
            for entry in &mut attr.local_variable_type_table {
                f(&mut entry.name_index.0)?;
                f(&mut entry.signature_index.0)?;
            }
            Ok(())
        }
        AttributeInfo::MethodParameters(attr) => {
            f(&mut attr.attribute_name_index.0)?;
            attr.parameters
                .iter_mut()
                .try_for_each(|parameter| f(&mut parameter.name_index.0))
        }
        AttributeInfo::NestHost(attr) => {
            f(&mut attr.attribute_name_index.0)?;
            f(&mut attr.host_class_index.0)
        }
        AttributeInfo::NestMembers(attr) => {
            f(&mut attr.attribute_name_index.0)?;
            attr.classes
                .iter_mut()
                .try_for_each(|index| f(&mut index.0))
        }
        AttributeInfo::RuntimeVisibleAnnotations(attr) => {
            f(&mut attr.attribute_name_index.0)?;
            visit_annotations(&mut attr.annotations, f)
        }
        AttributeInfo::RuntimeInvisibleAnnotations(attr) => {
            f(&mut attr.attribute_name_index.0)?;
            visit_annotations(&mut attr.annotations, f)
        }
        AttributeInfo::RuntimeVisibleParameterAnnotations(attr) => {
            f(&mut attr.attribute_name_index.0)?;
            attr.parameter_annotations
                .iter_mut()
                .try_for_each(|parameter| visit_annotations(&mut parameter.annotations, f))
        }
        AttributeInfo::RuntimeInvisibleParameterAnnotations(attr) => {
            f(&mut attr.attribute_name_index.0)?;
            attr.parameter_annotations
                .iter_mut()
                .try_for_each(|parameter| visit_annotations(&mut parameter.annotations, f))
        }
        AttributeInfo::RuntimeVisibleTypeAnnotations(attr) => {
            f(&mut attr.attribute_name_index.0)?;
            for annotation in &mut attr.annotations {
                f(&mut annotation.type_index.0)?;
                visit_element_value_pairs(&mut annotation.element_value_pairs, f)?;
            }
            Ok(())
        }
        AttributeInfo::RuntimeInvisibleTypeAnnotations(attr) => {
            f(&mut attr.attribute_name_index.0)?;
            for annotation in &mut attr.annotations {
                f(&mut annotation.type_index.0)?;
                visit_element_value_pairs(&mut annotation.element_value_pairs, f)?;
            }
            Ok(())
        }
        AttributeInfo::AnnotationDefault(attr) => {
            f(&mut attr.attribute_name_index.0)?;
            visit_element_value(&mut attr.default_value, f)
        }
        AttributeInfo::BootstrapMethods(attr) => {
            f(&mut attr.attribute_name_index.0)?;
            for method in &mut attr.bootstrap_methods {
                f(&mut method.bootstrap_method_ref.0)?;
                for argument in &mut method.bootstrap_arguments {
                    f(&mut argument.0)?;
                }
            }
            Ok(())
        }
        AttributeInfo::Module(attr) => {
            f(&mut attr.attribute_name_index.0)?;
            let module = &mut attr.module;
            f(&mut module.module_name_index.0)?;
            f(&mut module.module_version_index.0)?;
            for requires in &mut module.requires {
                f(&mut requires.requires_index.0)?;
                f(&mut requires.requires_version_index.0)?;
            }
            for exports in &mut module.exports {
                f(&mut exports.exports_index.0)?;
                for target in &mut exports.exports_to_index {
                    f(&mut target.0)?;
                }
            }
            for opens in &mut module.opens {
                f(&mut opens.opens_index.0)?;
                for target in &mut opens.opens_to_index {
                    f(&mut target.0)?;
                }
            }
            for uses in &mut module.uses_index {
                f(&mut uses.0)?;
            }
            for provides in &mut module.provides {
                f(&mut provides.provides_index.0)?;
                for implementation in &mut provides.provides_with_index {
                    f(&mut implementation.0)?;
                }
            }
            Ok(())
        }
        AttributeInfo::ModulePackages(attr) => {
            f(&mut attr.attribute_name_index.0)?;
            attr.package_index
                .iter_mut()
                .try_for_each(|index| f(&mut index.0))
        }
        AttributeInfo::ModuleMainClass(attr) => {
            f(&mut attr.attribute_name_index.0)?;
            f(&mut attr.main_class_index.0)
        }
        AttributeInfo::Record(attr) => {
            f(&mut attr.attribute_name_index.0)?;
            for component in &mut attr.components {
                f(&mut component.name_index.0)?;
                f(&mut component.descriptor_index.0)?;
                visit_attributes(&mut component.attributes, f)?;
            }
            Ok(())
        }
        AttributeInfo::PermittedSubclasses(attr) => {
            f(&mut attr.attribute_name_index.0)?;
            attr.classes
                .iter_mut()
                .try_for_each(|index| f(&mut index.0))
        }
        AttributeInfo::Unknown(attr) => Err(model_error(format!(
            "canonical constant-pool emission cannot remap the unparsed {:?} attribute; strip it or use ConstantPoolPolicy::Preserve",
            attr.name
        ))),
    }
}

fn visit_annotations(annotations: &mut [AnnotationInfo], f: &mut IndexVisitor<'_>) -> Result<()> {
    annotations
        .iter_mut()
        .try_for_each(|annotation| visit_annotation(annotation, f))
}

fn visit_annotation(annotation: &mut AnnotationInfo, f: &mut IndexVisitor<'_>) -> Result<()> {
    f(&mut annotation.type_index.0)?;
    visit_element_value_pairs(&mut annotation.element_value_pairs, f)
}

fn visit_element_value_pairs(
    pairs: &mut [ElementValuePairInfo],
    f: &mut IndexVisitor<'_>,
) -> Result<()> {
    for pair in pairs {
        f(&mut pair.element_name_index.0)?;
        visit_element_value(&mut pair.element_value, f)?;
    }
    Ok(())
}

fn visit_element_value(value: &mut ElementValueInfo, f: &mut IndexVisitor<'_>) -> Result<()> {
    match value {
        ElementValueInfo::Const {
            const_value_index, ..
        } => f(&mut const_value_index.0),
        ElementValueInfo::Enum {
            type_name_index,
            const_name_index,
        } => {
            f(&mut type_name_index.0)?;
            f(&mut const_name_index.0)
        }
        ElementValueInfo::Class { class_info_index } => f(&mut class_info_index.0),
        ElementValueInfo::Annotation(annotation) => visit_annotation(annotation, f),
        ElementValueInfo::Array { values } => values
            .iter_mut()
            .try_for_each(|value| visit_element_value(value, f)),
    }
}

fn frame_types_mut(frame: &mut StackMapFrameInfo) -> Vec<&mut VerificationTypeInfo> {
    match frame {
        StackMapFrameInfo::Same { .. }
        | StackMapFrameInfo::Chop { .. }
        | StackMapFrameInfo::SameExtended { .. } => Vec::new(),
        StackMapFrameInfo::SameLocals1StackItem { stack, .. }
        | StackMapFrameInfo::SameLocals1StackItemExtended { stack, .. } => vec![stack],
        StackMapFrameInfo::Append { locals, .. } => locals.iter_mut().collect(),
        StackMapFrameInfo::Full { locals, stack, .. } => {
            locals.iter_mut().chain(stack.iter_mut()).collect()
        }
    }
}

/// Re-anchor preserved `StackMapTable` frames to the code layout the
/// canonical pool produces.
///
/// The pool decides between `ldc` and `ldc_w`, so instruction offsets can
/// shift even though the instructions themselves are unchanged. Frames are
/// moved from each instruction's old offset to its new one and re-encoded in
/// their most compact form, which also makes the encoding independent of the
/// layout the class arrived with.
fn relocate_preserved_frames(model: &mut ClassModel) -> Result<()> {
    let mut cp = model.constant_pool.clone();
    for method in &mut model.methods {
        let Some(code) = method.code_mut() else {
            continue;
        };
        if !code
            .attributes
            .iter()
            .any(|attribute| matches!(attribute, AttributeInfo::StackMapTable(_)))
        {
            continue;
        }
        let layout = compute_code_layout(code, &mut cp)?;
        let Some(original) = &mut code.original_code_shape else {
            return Err(model_error(
                "a lifted method with a StackMapTable is missing its original layout",
            ));
        };
        let mut moved: FxHashMap<u32, u32> = FxHashMap::default();
        for (old, new) in original.item_offsets.iter().zip(&layout.item_offsets) {
            if let (Some(old), Some(new)) = (old, new) {
                moved.insert(*old, *new);
            }
        }
        let relocate = |offset: u32| -> Result<u32> {
            moved.get(&offset).copied().ok_or_else(|| {
                model_error(format!(
                    "StackMapTable refers to offset {offset}, which starts no instruction"
                ))
            })
        };
        for attribute in &mut code.attributes {
            if let AttributeInfo::StackMapTable(table) = attribute {
                relocate_frames(&mut table.entries, &relocate)?;
                table.attribute_length = stack_map::stack_map_table_length(&table.entries);
            }
        }
        original.item_offsets = layout.item_offsets;
        original.code_length = layout.code_length;
    }
    Ok(())
}

fn relocate_frames(
    frames: &mut [StackMapFrameInfo],
    relocate: &dyn Fn(u32) -> Result<u32>,
) -> Result<()> {
    let mut old_offset: Option<u32> = None;
    let mut new_offset: Option<u32> = None;
    for frame in frames {
        let delta = u32::from(frame_offset_delta(frame));
        let old = match old_offset {
            None => delta,
            Some(previous) => previous + delta + 1,
        };
        let new = relocate(old)?;
        let new_delta = match new_offset {
            None => new,
            Some(previous) => new
                .checked_sub(previous + 1)
                .ok_or_else(|| model_error("relocated stack-map frames are out of order"))?,
        };
        let new_delta = u16::try_from(new_delta)
            .map_err(|_| model_error("relocated stack-map offset delta exceeds 65535"))?;
        for vtype in frame_types_mut(frame) {
            if let VerificationTypeInfo::Uninitialized { offset } = vtype {
                *offset = u16::try_from(relocate(u32::from(*offset))?).map_err(|_| {
                    model_error("relocated uninitialized-type offset exceeds 65535")
                })?;
            }
        }
        *frame = with_offset_delta(frame.clone(), new_delta);
        old_offset = Some(old);
        new_offset = Some(new);
    }
    Ok(())
}

fn frame_offset_delta(frame: &StackMapFrameInfo) -> u16 {
    match frame {
        StackMapFrameInfo::Same { frame_type } => u16::from(*frame_type),
        StackMapFrameInfo::SameLocals1StackItem { frame_type, .. } => u16::from(*frame_type - 64),
        StackMapFrameInfo::SameLocals1StackItemExtended { offset_delta, .. }
        | StackMapFrameInfo::Chop { offset_delta, .. }
        | StackMapFrameInfo::SameExtended { offset_delta, .. }
        | StackMapFrameInfo::Append { offset_delta, .. }
        | StackMapFrameInfo::Full { offset_delta, .. } => *offset_delta,
    }
}

/// Rebuild `frame` with a new offset delta, in the most compact form.
fn with_offset_delta(frame: StackMapFrameInfo, offset_delta: u16) -> StackMapFrameInfo {
    let compact = offset_delta <= 63;
    match frame {
        StackMapFrameInfo::Same { .. } | StackMapFrameInfo::SameExtended { .. } => {
            if compact {
                StackMapFrameInfo::Same {
                    frame_type: offset_delta as u8,
                }
            } else {
                StackMapFrameInfo::SameExtended {
                    frame_type: 251,
                    offset_delta,
                }
            }
        }
        StackMapFrameInfo::SameLocals1StackItem { stack, .. }
        | StackMapFrameInfo::SameLocals1StackItemExtended { stack, .. } => {
            if compact {
                StackMapFrameInfo::SameLocals1StackItem {
                    frame_type: 64 + offset_delta as u8,
                    stack,
                }
            } else {
                StackMapFrameInfo::SameLocals1StackItemExtended {
                    frame_type: 247,
                    offset_delta,
                    stack,
                }
            }
        }
        StackMapFrameInfo::Chop { frame_type, .. } => StackMapFrameInfo::Chop {
            frame_type,
            offset_delta,
        },
        StackMapFrameInfo::Append {
            frame_type, locals, ..
        } => StackMapFrameInfo::Append {
            frame_type,
            offset_delta,
            locals,
        },
        StackMapFrameInfo::Full {
            frame_type,
            locals,
            stack,
            ..
        } => StackMapFrameInfo::Full {
            frame_type,
            offset_delta,
            locals,
            stack,
        },
    }
}
