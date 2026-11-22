//! Declarative transform specifications that Rust evaluates natively.
//!
//! Each variant represents a model mutation that Python constructs (via PyO3)
//! and Rust applies without crossing the FFI boundary.

use crate::constants::{ClassAccessFlags, FieldAccessFlags, MethodAccessFlags};
use crate::model::{
    ClassModel, CodeItem, CodeModel, FieldModel, InterfaceMethodInsn, LdcValue, MethodInsn,
    MethodModel,
};
use crate::modified_utf8::JavaString;
use crate::transform::InsnMatcher;
use crate::transform::matcher_spec::{CompiledInsnMatcher, InsnMatcherSpec};
use std::fmt;
use std::sync::Arc;

/// A declarative code-level transform that Rust applies natively.
#[derive(Debug, Clone)]
pub enum CodeTransformSpec {
    /// Replace every matching instruction with the given replacement sequence.
    ReplaceInsn {
        matcher: InsnMatcherSpec,
        replacement: Vec<CodeItem>,
    },
    /// Replace every matching instruction sequence with the given replacement sequence.
    ReplaceSequence {
        pattern: Vec<InsnMatcherSpec>,
        replacement: Vec<CodeItem>,
    },
    /// Remove every matching instruction.
    RemoveInsn { matcher: InsnMatcherSpec },
    /// Remove every matching instruction sequence.
    RemoveSequence { pattern: Vec<InsnMatcherSpec> },
    /// Insert instructions before every matching instruction.
    InsertBefore {
        matcher: InsnMatcherSpec,
        items: Vec<CodeItem>,
    },
    /// Insert instructions after every matching instruction.
    InsertAfter {
        matcher: InsnMatcherSpec,
        items: Vec<CodeItem>,
    },
    /// Redirect matching method invocations to a new owner/name/descriptor.
    RedirectMethodCall {
        from_owner: String,
        from_name: String,
        from_descriptor: String,
        to_owner: String,
        to_name: String,
        to_descriptor: Option<String>,
        to_is_interface: Option<bool>,
    },
    /// Redirect matching field accesses to a new owner/name/descriptor.
    RedirectFieldAccess {
        from_owner: String,
        from_name: String,
        from_descriptor: Option<String>,
        to_owner: String,
        to_name: String,
        to_descriptor: Option<String>,
    },
    /// Replace matching string constants.
    ReplaceString { from: JavaString, to: JavaString },
    /// Apply a sequence of code transforms in order.
    Sequence(Vec<CodeTransformSpec>),
}

/// Pre-compiled form of [`CodeTransformSpec`] with matchers compiled once up front.
#[derive(Clone)]
pub enum CompiledCodeTransform {
    ReplaceInsn {
        matcher: InsnMatcher,
        replacement: Vec<CodeItem>,
    },
    ReplaceSequence {
        pattern: Vec<InsnMatcher>,
        replacement: Vec<CodeItem>,
    },
    RemoveInsn {
        matcher: InsnMatcher,
    },
    RemoveSequence {
        pattern: Vec<InsnMatcher>,
    },
    InsertBefore {
        matcher: InsnMatcher,
        items: Vec<CodeItem>,
    },
    InsertAfter {
        matcher: InsnMatcher,
        items: Vec<CodeItem>,
    },
    RedirectMethodCall {
        from_owner: String,
        from_name: String,
        from_descriptor: String,
        to_owner: String,
        to_name: String,
        to_descriptor: Option<String>,
        to_is_interface: Option<bool>,
    },
    RedirectFieldAccess {
        from_owner: String,
        from_name: String,
        from_descriptor: Option<String>,
        to_owner: String,
        to_name: String,
        to_descriptor: Option<String>,
    },
    ReplaceString {
        from: JavaString,
        to: JavaString,
    },
    Sequence(Vec<CompiledCodeTransform>),
}

fn wrap_compiled_matcher(compiled: CompiledInsnMatcher, description: String) -> InsnMatcher {
    let compiled = Arc::new(compiled);
    crate::transform::Matcher::of(move |item| compiled.matches(item), description)
}

impl CompiledCodeTransform {
    /// Compile a [`CodeTransformSpec`], returning a regex error on invalid patterns.
    pub fn try_from_spec(spec: &CodeTransformSpec) -> std::result::Result<Self, regex::Error> {
        let compile_matcher =
            |matcher: &InsnMatcherSpec| -> std::result::Result<InsnMatcher, regex::Error> {
                Ok(wrap_compiled_matcher(
                    matcher.try_compile()?,
                    matcher.to_string(),
                ))
            };
        let compile_pattern =
            |pattern: &[InsnMatcherSpec]| -> std::result::Result<Vec<InsnMatcher>, regex::Error> {
                pattern.iter().map(compile_matcher).collect()
            };

        Ok(match spec {
            CodeTransformSpec::ReplaceInsn {
                matcher,
                replacement,
            } => Self::ReplaceInsn {
                matcher: compile_matcher(matcher)?,
                replacement: replacement.clone(),
            },
            CodeTransformSpec::ReplaceSequence {
                pattern,
                replacement,
            } => Self::ReplaceSequence {
                pattern: compile_pattern(pattern)?,
                replacement: replacement.clone(),
            },
            CodeTransformSpec::RemoveInsn { matcher } => Self::RemoveInsn {
                matcher: compile_matcher(matcher)?,
            },
            CodeTransformSpec::RemoveSequence { pattern } => Self::RemoveSequence {
                pattern: compile_pattern(pattern)?,
            },
            CodeTransformSpec::InsertBefore { matcher, items } => Self::InsertBefore {
                matcher: compile_matcher(matcher)?,
                items: items.clone(),
            },
            CodeTransformSpec::InsertAfter { matcher, items } => Self::InsertAfter {
                matcher: compile_matcher(matcher)?,
                items: items.clone(),
            },
            CodeTransformSpec::RedirectMethodCall {
                from_owner,
                from_name,
                from_descriptor,
                to_owner,
                to_name,
                to_descriptor,
                to_is_interface,
            } => Self::RedirectMethodCall {
                from_owner: from_owner.clone(),
                from_name: from_name.clone(),
                from_descriptor: from_descriptor.clone(),
                to_owner: to_owner.clone(),
                to_name: to_name.clone(),
                to_descriptor: to_descriptor.clone(),
                to_is_interface: *to_is_interface,
            },
            CodeTransformSpec::RedirectFieldAccess {
                from_owner,
                from_name,
                from_descriptor,
                to_owner,
                to_name,
                to_descriptor,
            } => Self::RedirectFieldAccess {
                from_owner: from_owner.clone(),
                from_name: from_name.clone(),
                from_descriptor: from_descriptor.clone(),
                to_owner: to_owner.clone(),
                to_name: to_name.clone(),
                to_descriptor: to_descriptor.clone(),
            },
            CodeTransformSpec::ReplaceString { from, to } => Self::ReplaceString {
                from: from.clone(),
                to: to.clone(),
            },
            CodeTransformSpec::Sequence(specs) => Self::Sequence(
                specs
                    .iter()
                    .map(Self::try_from_spec)
                    .collect::<std::result::Result<Vec<_>, _>>()?,
            ),
        })
    }

    /// Apply this compiled transform to a code model, mutating it in place.
    pub fn apply(&self, code: &mut CodeModel) {
        match self {
            Self::ReplaceInsn {
                matcher,
                replacement,
            } => {
                code.replace_insns(matcher, replacement);
            }
            Self::ReplaceSequence {
                pattern,
                replacement,
            } => {
                code.replace_sequences(pattern, replacement);
            }
            Self::RemoveInsn { matcher } => {
                code.remove_insns(matcher);
            }
            Self::RemoveSequence { pattern } => {
                code.remove_sequences(pattern);
            }
            Self::InsertBefore { matcher, items } => {
                code.insert_before(matcher, items);
            }
            Self::InsertAfter { matcher, items } => {
                code.insert_after(matcher, items);
            }
            Self::RedirectMethodCall {
                from_owner,
                from_name,
                from_descriptor,
                to_owner,
                to_name,
                to_descriptor,
                to_is_interface,
            } => {
                for item in &mut code.instructions {
                    match item {
                        CodeItem::Method(insn)
                            if insn.owner == *from_owner
                                && insn.name == *from_name
                                && insn.descriptor == *from_descriptor =>
                        {
                            let new_descriptor = to_descriptor
                                .clone()
                                .unwrap_or_else(|| insn.descriptor.clone());
                            if *to_is_interface == Some(true) && insn.opcode == 0xB6 {
                                *item = CodeItem::InterfaceMethod(InterfaceMethodInsn {
                                    owner: to_owner.clone(),
                                    name: to_name.clone(),
                                    descriptor: new_descriptor,
                                });
                            } else {
                                insn.owner = to_owner.clone();
                                insn.name = to_name.clone();
                                insn.descriptor = new_descriptor;
                                if let Some(flag) = to_is_interface {
                                    insn.is_interface = *flag;
                                }
                            }
                        }
                        CodeItem::InterfaceMethod(insn)
                            if insn.owner == *from_owner
                                && insn.name == *from_name
                                && insn.descriptor == *from_descriptor =>
                        {
                            let new_descriptor = to_descriptor
                                .clone()
                                .unwrap_or_else(|| insn.descriptor.clone());
                            if *to_is_interface == Some(false) {
                                *item = CodeItem::Method(MethodInsn {
                                    opcode: 0xB6,
                                    owner: to_owner.clone(),
                                    name: to_name.clone(),
                                    descriptor: new_descriptor,
                                    is_interface: false,
                                });
                            } else {
                                insn.owner = to_owner.clone();
                                insn.name = to_name.clone();
                                insn.descriptor = new_descriptor;
                            }
                        }
                        _ => {}
                    }
                }
            }
            Self::RedirectFieldAccess {
                from_owner,
                from_name,
                from_descriptor,
                to_owner,
                to_name,
                to_descriptor,
            } => {
                for item in &mut code.instructions {
                    if let CodeItem::Field(insn) = item
                        && insn.owner == *from_owner
                        && insn.name == *from_name
                        && from_descriptor
                            .as_ref()
                            .is_none_or(|d| insn.descriptor == *d)
                    {
                        insn.owner = to_owner.clone();
                        insn.name = to_name.clone();
                        if let Some(descriptor) = to_descriptor {
                            insn.descriptor = descriptor.clone();
                        }
                    }
                }
            }
            Self::ReplaceString { from, to } => {
                for item in &mut code.instructions {
                    if let CodeItem::Ldc(insn) = item
                        && let LdcValue::String(value) = &mut insn.value
                        && value == from
                    {
                        *value = to.clone();
                    }
                }
            }
            Self::Sequence(specs) => {
                for spec in specs {
                    spec.apply(code);
                }
            }
        }
    }
}

impl CodeTransformSpec {
    /// Compile this transform, validating any embedded regex matchers.
    pub fn try_compile(&self) -> std::result::Result<CompiledCodeTransform, regex::Error> {
        CompiledCodeTransform::try_from_spec(self)
    }

    /// Compile this transform, panicking on invalid regex patterns.
    pub fn compile(&self) -> CompiledCodeTransform {
        self.try_compile()
            .expect("invalid regex in CodeTransformSpec")
    }

    /// Apply this transform to a code model, mutating it in place.
    pub fn apply(&self, code: &mut CodeModel) {
        self.compile().apply(code);
    }
}

impl fmt::Display for CodeTransformSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReplaceInsn {
                matcher,
                replacement,
            } => {
                write!(f, "replace_insn({matcher}, {} items)", replacement.len())
            }
            Self::ReplaceSequence {
                pattern,
                replacement,
            } => {
                write!(
                    f,
                    "replace_sequence({} matchers, {} items)",
                    pattern.len(),
                    replacement.len()
                )
            }
            Self::RemoveInsn { matcher } => write!(f, "remove_insn({matcher})"),
            Self::RemoveSequence { pattern } => {
                write!(f, "remove_sequence({} matchers)", pattern.len())
            }
            Self::InsertBefore { matcher, items } => {
                write!(f, "insert_before({matcher}, {} items)", items.len())
            }
            Self::InsertAfter { matcher, items } => {
                write!(f, "insert_after({matcher}, {} items)", items.len())
            }
            Self::RedirectMethodCall {
                from_owner,
                from_name,
                from_descriptor,
                to_owner,
                to_name,
                to_descriptor,
                to_is_interface,
            } => write!(
                f,
                "redirect_method_call(({from_owner:?}, {from_name:?}, {from_descriptor:?}) -> ({to_owner:?}, {to_name:?}, {to_descriptor:?}, is_interface={to_is_interface:?}))"
            ),
            Self::RedirectFieldAccess {
                from_owner,
                from_name,
                from_descriptor,
                to_owner,
                to_name,
                to_descriptor,
            } => write!(
                f,
                "redirect_field_access(({from_owner:?}, {from_name:?}, {from_descriptor:?}) -> ({to_owner:?}, {to_name:?}, {to_descriptor:?}))"
            ),
            Self::ReplaceString { from, to } => write!(
                f,
                "replace_string({:?}, {:?})",
                from.to_string_lossy(),
                to.to_string_lossy()
            ),
            Self::Sequence(specs) => {
                write!(f, "sequence(")?;
                for (i, spec) in specs.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{spec}")?;
                }
                write!(f, ")")
            }
        }
    }
}

/// A declarative field-level transform that Rust applies natively.
#[derive(Debug, Clone)]
pub enum FieldTransformSpec {
    Rename(String),
    SetDescriptor(String),
    SetAccessFlags(u16),
    AddAccessFlags(u16),
    RemoveAccessFlags(u16),
    Remove,
    Sequence(Vec<FieldTransformSpec>),
}

impl FieldTransformSpec {
    /// Apply this transform to a single field. Returns `false` if the field should be removed.
    pub fn apply_to_field(&self, field: &mut FieldModel) -> bool {
        match self {
            Self::Rename(name) => {
                field.name = name.clone();
                true
            }
            Self::SetDescriptor(descriptor) => {
                field.descriptor = descriptor.clone();
                true
            }
            Self::SetAccessFlags(flags) => {
                field.access_flags = FieldAccessFlags::from_bits_truncate(*flags);
                true
            }
            Self::AddAccessFlags(flags) => {
                field.access_flags |= FieldAccessFlags::from_bits_truncate(*flags);
                true
            }
            Self::RemoveAccessFlags(flags) => {
                field.access_flags &= !FieldAccessFlags::from_bits_truncate(*flags);
                true
            }
            Self::Remove => false,
            Self::Sequence(specs) => {
                for spec in specs {
                    if !spec.apply_to_field(field) {
                        return false;
                    }
                }
                true
            }
        }
    }
}

impl fmt::Display for FieldTransformSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rename(name) => write!(f, "rename_field({name:?})"),
            Self::SetDescriptor(desc) => write!(f, "set_field_descriptor({desc:?})"),
            Self::SetAccessFlags(flags) => write!(f, "set_field_access_flags(0x{flags:04X})"),
            Self::AddAccessFlags(flags) => write!(f, "add_field_access_flags(0x{flags:04X})"),
            Self::RemoveAccessFlags(flags) => {
                write!(f, "remove_field_access_flags(0x{flags:04X})")
            }
            Self::Remove => write!(f, "remove_field()"),
            Self::Sequence(specs) => {
                write!(f, "sequence(")?;
                for (i, spec) in specs.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{spec}")?;
                }
                write!(f, ")")
            }
        }
    }
}

/// A declarative method-level transform that Rust applies natively.
#[derive(Debug, Clone)]
pub enum MethodTransformSpec {
    Rename(String),
    SetDescriptor(String),
    SetAccessFlags(u16),
    AddAccessFlags(u16),
    RemoveAccessFlags(u16),
    Remove,
    CodeTransform(CodeTransformSpec),
    Sequence(Vec<MethodTransformSpec>),
}

/// Pre-compiled form of [`MethodTransformSpec`].
#[derive(Clone)]
pub enum CompiledMethodTransform {
    Rename(String),
    SetDescriptor(String),
    SetAccessFlags(u16),
    AddAccessFlags(u16),
    RemoveAccessFlags(u16),
    Remove,
    CodeTransform(CompiledCodeTransform),
    Sequence(Vec<CompiledMethodTransform>),
}

impl CompiledMethodTransform {
    pub fn try_from_spec(spec: &MethodTransformSpec) -> std::result::Result<Self, regex::Error> {
        Ok(match spec {
            MethodTransformSpec::Rename(name) => Self::Rename(name.clone()),
            MethodTransformSpec::SetDescriptor(desc) => Self::SetDescriptor(desc.clone()),
            MethodTransformSpec::SetAccessFlags(flags) => Self::SetAccessFlags(*flags),
            MethodTransformSpec::AddAccessFlags(flags) => Self::AddAccessFlags(*flags),
            MethodTransformSpec::RemoveAccessFlags(flags) => Self::RemoveAccessFlags(*flags),
            MethodTransformSpec::Remove => Self::Remove,
            MethodTransformSpec::CodeTransform(code) => Self::CodeTransform(code.try_compile()?),
            MethodTransformSpec::Sequence(specs) => Self::Sequence(
                specs
                    .iter()
                    .map(Self::try_from_spec)
                    .collect::<std::result::Result<Vec<_>, _>>()?,
            ),
        })
    }

    /// Apply this compiled transform to a single method. Returns `false` if the method should be removed.
    pub fn apply_to_method(&self, method: &mut MethodModel) -> bool {
        match self {
            Self::Rename(name) => {
                method.name = name.clone();
                true
            }
            Self::SetDescriptor(descriptor) => {
                method.descriptor = descriptor.clone();
                true
            }
            Self::SetAccessFlags(flags) => {
                method.access_flags = MethodAccessFlags::from_bits_truncate(*flags);
                true
            }
            Self::AddAccessFlags(flags) => {
                method.access_flags |= MethodAccessFlags::from_bits_truncate(*flags);
                true
            }
            Self::RemoveAccessFlags(flags) => {
                method.access_flags &= !MethodAccessFlags::from_bits_truncate(*flags);
                true
            }
            Self::Remove => false,
            Self::CodeTransform(transform) => {
                if let Some(code) = method.code_mut() {
                    transform.apply(code);
                }
                true
            }
            Self::Sequence(specs) => {
                for spec in specs {
                    if !spec.apply_to_method(method) {
                        return false;
                    }
                }
                true
            }
        }
    }
}

impl MethodTransformSpec {
    pub fn try_compile(&self) -> std::result::Result<CompiledMethodTransform, regex::Error> {
        CompiledMethodTransform::try_from_spec(self)
    }

    pub fn compile(&self) -> CompiledMethodTransform {
        self.try_compile()
            .expect("invalid regex in MethodTransformSpec")
    }

    pub fn apply_to_method(&self, method: &mut MethodModel) -> bool {
        self.compile().apply_to_method(method)
    }
}

impl fmt::Display for MethodTransformSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rename(name) => write!(f, "rename_method({name:?})"),
            Self::SetDescriptor(desc) => write!(f, "set_method_descriptor({desc:?})"),
            Self::SetAccessFlags(flags) => write!(f, "set_method_access_flags(0x{flags:04X})"),
            Self::AddAccessFlags(flags) => write!(f, "add_method_access_flags(0x{flags:04X})"),
            Self::RemoveAccessFlags(flags) => {
                write!(f, "remove_method_access_flags(0x{flags:04X})")
            }
            Self::Remove => write!(f, "remove_method()"),
            Self::CodeTransform(code) => write!(f, "code_transform({code})"),
            Self::Sequence(specs) => {
                write!(f, "sequence(")?;
                for (i, spec) in specs.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{spec}")?;
                }
                write!(f, ")")
            }
        }
    }
}

/// A declarative class-level transform that Rust applies natively.
#[derive(Debug, Clone)]
pub enum ClassTransformSpec {
    /// Rename the class to a new internal name.
    RenameClass(String),
    /// Set access flags to an exact value.
    SetAccessFlags(u16),
    /// Add access flags (bitwise OR).
    AddAccessFlags(u16),
    /// Remove access flags (bitwise AND NOT).
    RemoveAccessFlags(u16),
    /// Set the super class name.
    SetSuperClass(String),
    /// Add an interface.
    AddInterface(String),
    /// Remove an interface by name.
    RemoveInterface(String),
    /// Remove methods matching name (and optional descriptor).
    RemoveMethod {
        name: String,
        descriptor: Option<String>,
    },
    /// Remove fields matching name (and optional descriptor).
    RemoveField {
        name: String,
        descriptor: Option<String>,
    },
    /// Rename all methods with matching name.
    RenameMethod { from: String, to: String },
    /// Rename all fields with matching name.
    RenameField { from: String, to: String },
    /// Set access flags on matching methods.
    SetMethodAccessFlags { name: String, flags: u16 },
    /// Set access flags on matching fields.
    SetFieldAccessFlags { name: String, flags: u16 },
    /// Apply a code transform to methods matching name/descriptor filters.
    CodeTransform {
        method_name: Option<String>,
        method_descriptor: Option<String>,
        transform: CodeTransformSpec,
    },
    /// Apply a sequence of transforms in order.
    Sequence(Vec<ClassTransformSpec>),
}

/// Pre-compiled form of [`ClassTransformSpec`].
#[derive(Clone)]
pub enum CompiledClassTransform {
    RenameClass(String),
    SetAccessFlags(u16),
    AddAccessFlags(u16),
    RemoveAccessFlags(u16),
    SetSuperClass(String),
    AddInterface(String),
    RemoveInterface(String),
    RemoveMethod {
        name: String,
        descriptor: Option<String>,
    },
    RemoveField {
        name: String,
        descriptor: Option<String>,
    },
    RenameMethod {
        from: String,
        to: String,
    },
    RenameField {
        from: String,
        to: String,
    },
    SetMethodAccessFlags {
        name: String,
        flags: u16,
    },
    SetFieldAccessFlags {
        name: String,
        flags: u16,
    },
    CodeTransform {
        method_name: Option<String>,
        method_descriptor: Option<String>,
        transform: CompiledCodeTransform,
    },
    Sequence(Vec<CompiledClassTransform>),
}

impl CompiledClassTransform {
    pub fn try_from_spec(spec: &ClassTransformSpec) -> std::result::Result<Self, regex::Error> {
        Ok(match spec {
            ClassTransformSpec::RenameClass(n) => Self::RenameClass(n.clone()),
            ClassTransformSpec::SetAccessFlags(f) => Self::SetAccessFlags(*f),
            ClassTransformSpec::AddAccessFlags(f) => Self::AddAccessFlags(*f),
            ClassTransformSpec::RemoveAccessFlags(f) => Self::RemoveAccessFlags(*f),
            ClassTransformSpec::SetSuperClass(n) => Self::SetSuperClass(n.clone()),
            ClassTransformSpec::AddInterface(n) => Self::AddInterface(n.clone()),
            ClassTransformSpec::RemoveInterface(n) => Self::RemoveInterface(n.clone()),
            ClassTransformSpec::RemoveMethod { name, descriptor } => Self::RemoveMethod {
                name: name.clone(),
                descriptor: descriptor.clone(),
            },
            ClassTransformSpec::RemoveField { name, descriptor } => Self::RemoveField {
                name: name.clone(),
                descriptor: descriptor.clone(),
            },
            ClassTransformSpec::RenameMethod { from, to } => Self::RenameMethod {
                from: from.clone(),
                to: to.clone(),
            },
            ClassTransformSpec::RenameField { from, to } => Self::RenameField {
                from: from.clone(),
                to: to.clone(),
            },
            ClassTransformSpec::SetMethodAccessFlags { name, flags } => {
                Self::SetMethodAccessFlags {
                    name: name.clone(),
                    flags: *flags,
                }
            }
            ClassTransformSpec::SetFieldAccessFlags { name, flags } => Self::SetFieldAccessFlags {
                name: name.clone(),
                flags: *flags,
            },
            ClassTransformSpec::CodeTransform {
                method_name,
                method_descriptor,
                transform,
            } => Self::CodeTransform {
                method_name: method_name.clone(),
                method_descriptor: method_descriptor.clone(),
                transform: transform.try_compile()?,
            },
            ClassTransformSpec::Sequence(specs) => Self::Sequence(
                specs
                    .iter()
                    .map(Self::try_from_spec)
                    .collect::<std::result::Result<Vec<_>, _>>()?,
            ),
        })
    }

    pub fn apply(&self, model: &mut ClassModel) {
        match self {
            Self::RenameClass(name) => {
                model.name = name.clone();
            }
            Self::SetAccessFlags(flags) => {
                model.access_flags = ClassAccessFlags::from_bits_truncate(*flags);
            }
            Self::AddAccessFlags(flags) => {
                model.access_flags |= ClassAccessFlags::from_bits_truncate(*flags);
            }
            Self::RemoveAccessFlags(flags) => {
                model.access_flags &= !ClassAccessFlags::from_bits_truncate(*flags);
            }
            Self::SetSuperClass(name) => {
                model.super_name = Some(name.clone());
            }
            Self::AddInterface(name) => {
                if !model.interfaces.contains(name) {
                    model.interfaces.push(name.clone());
                }
            }
            Self::RemoveInterface(name) => {
                model.interfaces.retain(|i| i != name);
            }
            Self::RemoveMethod { name, descriptor } => {
                model.methods.retain(|m| {
                    !(m.name == *name && descriptor.as_ref().is_none_or(|d| m.descriptor == *d))
                });
            }
            Self::RemoveField { name, descriptor } => {
                model.fields.retain(|f| {
                    !(f.name == *name && descriptor.as_ref().is_none_or(|d| f.descriptor == *d))
                });
            }
            Self::RenameMethod { from, to } => {
                for m in &mut model.methods {
                    if m.name == *from {
                        m.name = to.clone();
                    }
                }
            }
            Self::RenameField { from, to } => {
                for f in &mut model.fields {
                    if f.name == *from {
                        f.name = to.clone();
                    }
                }
            }
            Self::SetMethodAccessFlags { name, flags } => {
                for m in &mut model.methods {
                    if m.name == *name {
                        m.access_flags = MethodAccessFlags::from_bits_truncate(*flags);
                    }
                }
            }
            Self::SetFieldAccessFlags { name, flags } => {
                for f in &mut model.fields {
                    if f.name == *name {
                        f.access_flags = FieldAccessFlags::from_bits_truncate(*flags);
                    }
                }
            }
            Self::CodeTransform {
                method_name,
                method_descriptor,
                transform,
            } => {
                for method in &mut model.methods {
                    let matches_name = method_name.as_ref().is_none_or(|name| method.name == *name);
                    let matches_descriptor = method_descriptor
                        .as_ref()
                        .is_none_or(|descriptor| method.descriptor == *descriptor);
                    if matches_name
                        && matches_descriptor
                        && let Some(code) = method.code_mut()
                    {
                        transform.apply(code);
                    }
                }
            }
            Self::Sequence(specs) => {
                for spec in specs {
                    spec.apply(model);
                }
            }
        }
    }
}

impl ClassTransformSpec {
    /// Compile this transform, validating any embedded regex matchers.
    pub fn try_compile(&self) -> std::result::Result<CompiledClassTransform, regex::Error> {
        CompiledClassTransform::try_from_spec(self)
    }

    /// Compile this transform, panicking on invalid regex patterns.
    pub fn compile(&self) -> CompiledClassTransform {
        self.try_compile()
            .expect("invalid regex in ClassTransformSpec")
    }

    /// Apply this transform to a class model, mutating it in place.
    pub fn apply(&self, model: &mut ClassModel) {
        self.compile().apply(model);
    }
}

impl fmt::Display for ClassTransformSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RenameClass(n) => write!(f, "rename_class({n:?})"),
            Self::SetAccessFlags(fl) => write!(f, "set_access_flags(0x{fl:04X})"),
            Self::AddAccessFlags(fl) => write!(f, "add_access_flags(0x{fl:04X})"),
            Self::RemoveAccessFlags(fl) => write!(f, "remove_access_flags(0x{fl:04X})"),
            Self::SetSuperClass(n) => write!(f, "set_super_class({n:?})"),
            Self::AddInterface(n) => write!(f, "add_interface({n:?})"),
            Self::RemoveInterface(n) => write!(f, "remove_interface({n:?})"),
            Self::RemoveMethod { name, descriptor } => {
                write!(f, "remove_method({name:?}, {descriptor:?})")
            }
            Self::RemoveField { name, descriptor } => {
                write!(f, "remove_field({name:?}, {descriptor:?})")
            }
            Self::RenameMethod { from, to } => write!(f, "rename_method({from:?}, {to:?})"),
            Self::RenameField { from, to } => write!(f, "rename_field({from:?}, {to:?})"),
            Self::SetMethodAccessFlags { name, flags } => {
                write!(f, "set_method_access_flags({name:?}, 0x{flags:04X})")
            }
            Self::SetFieldAccessFlags { name, flags } => {
                write!(f, "set_field_access_flags({name:?}, 0x{flags:04X})")
            }
            Self::CodeTransform {
                method_name,
                method_descriptor,
                transform,
            } => write!(
                f,
                "code_transform(method_name={method_name:?}, method_descriptor={method_descriptor:?}, {transform})"
            ),
            Self::Sequence(specs) => {
                write!(f, "sequence(")?;
                for (i, spec) in specs.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{spec}")?;
                }
                write!(f, ")")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::{ClassAccessFlags, FieldAccessFlags, MethodAccessFlags};
    use crate::model::DebugInfoState;
    use crate::model::{ClassModel, CodeModel, ConstantPoolBuilder, FieldModel, MethodModel};

    fn sample_class() -> ClassModel {
        ClassModel {
            entry_name: "test/Sample.class".into(),
            original_byte_len: 0,
            version: (52, 0),
            constant_pool: ConstantPoolBuilder::new(),
            access_flags: ClassAccessFlags::PUBLIC | ClassAccessFlags::SUPER,
            name: "test/Sample".to_string(),
            super_name: Some("java/lang/Object".to_string()),
            interfaces: vec!["java/io/Serializable".to_string()],
            debug_info_state: DebugInfoState::Fresh,
            fields: vec![
                FieldModel {
                    access_flags: FieldAccessFlags::PRIVATE,
                    name: "count".to_string(),
                    descriptor: "I".to_string(),
                    attributes: vec![],
                },
                FieldModel {
                    access_flags: FieldAccessFlags::PUBLIC | FieldAccessFlags::STATIC,
                    name: "INSTANCE".to_string(),
                    descriptor: "Ltest/Sample;".to_string(),
                    attributes: vec![],
                },
            ],
            methods: vec![
                MethodModel::new(
                    MethodAccessFlags::PUBLIC,
                    "<init>".to_string(),
                    "()V".to_string(),
                    Some(CodeModel::new(1, 1, DebugInfoState::Fresh)),
                    vec![],
                ),
                MethodModel::new(
                    MethodAccessFlags::PUBLIC | MethodAccessFlags::STATIC,
                    "main".to_string(),
                    "([Ljava/lang/String;)V".to_string(),
                    Some(CodeModel::new(2, 1, DebugInfoState::Fresh)),
                    vec![],
                ),
                MethodModel::new(
                    MethodAccessFlags::PRIVATE,
                    "helper".to_string(),
                    "(II)Ljava/lang/String;".to_string(),
                    Some(CodeModel::new(3, 3, DebugInfoState::Fresh)),
                    vec![],
                ),
            ],
            attributes: vec![],
        }
    }

    #[test]
    fn rename_class() {
        let mut model = sample_class();
        ClassTransformSpec::RenameClass("test/Renamed".into()).apply(&mut model);
        assert_eq!(model.name, "test/Renamed");
    }

    #[test]
    fn set_access_flags() {
        let mut model = sample_class();
        let flags = (ClassAccessFlags::PUBLIC | ClassAccessFlags::FINAL).bits();
        ClassTransformSpec::SetAccessFlags(flags).apply(&mut model);
        assert_eq!(
            model.access_flags,
            ClassAccessFlags::PUBLIC | ClassAccessFlags::FINAL
        );
    }

    #[test]
    fn add_access_flags() {
        let mut model = sample_class();
        ClassTransformSpec::AddAccessFlags(ClassAccessFlags::FINAL.bits()).apply(&mut model);
        assert!(model.access_flags.contains(ClassAccessFlags::FINAL));
        assert!(model.access_flags.contains(ClassAccessFlags::PUBLIC));
    }

    #[test]
    fn remove_access_flags() {
        let mut model = sample_class();
        ClassTransformSpec::RemoveAccessFlags(ClassAccessFlags::PUBLIC.bits()).apply(&mut model);
        assert!(!model.access_flags.contains(ClassAccessFlags::PUBLIC));
        assert!(model.access_flags.contains(ClassAccessFlags::SUPER));
    }

    #[test]
    fn set_super_class() {
        let mut model = sample_class();
        ClassTransformSpec::SetSuperClass("java/lang/Thread".into()).apply(&mut model);
        assert_eq!(model.super_name.as_deref(), Some("java/lang/Thread"));
    }

    #[test]
    fn add_interface() {
        let mut model = sample_class();
        ClassTransformSpec::AddInterface("java/lang/Runnable".into()).apply(&mut model);
        assert_eq!(model.interfaces.len(), 2);
        assert!(model.interfaces.contains(&"java/lang/Runnable".to_string()));
    }

    #[test]
    fn add_interface_dedup() {
        let mut model = sample_class();
        ClassTransformSpec::AddInterface("java/io/Serializable".into()).apply(&mut model);
        assert_eq!(model.interfaces.len(), 1); // no duplicate
    }

    #[test]
    fn remove_interface() {
        let mut model = sample_class();
        ClassTransformSpec::RemoveInterface("java/io/Serializable".into()).apply(&mut model);
        assert!(model.interfaces.is_empty());
    }

    #[test]
    fn remove_method_by_name() {
        let mut model = sample_class();
        assert_eq!(model.methods.len(), 3);
        ClassTransformSpec::RemoveMethod {
            name: "helper".into(),
            descriptor: None,
        }
        .apply(&mut model);
        assert_eq!(model.methods.len(), 2);
        assert!(model.methods.iter().all(|m| m.name != "helper"));
    }

    #[test]
    fn remove_method_by_name_and_descriptor() {
        let mut model = sample_class();
        ClassTransformSpec::RemoveMethod {
            name: "main".into(),
            descriptor: Some("([Ljava/lang/String;)V".into()),
        }
        .apply(&mut model);
        assert_eq!(model.methods.len(), 2);
        assert!(model.methods.iter().all(|m| m.name != "main"));
    }

    #[test]
    fn remove_method_wrong_descriptor_no_op() {
        let mut model = sample_class();
        ClassTransformSpec::RemoveMethod {
            name: "main".into(),
            descriptor: Some("()V".into()),
        }
        .apply(&mut model);
        assert_eq!(model.methods.len(), 3); // still 3
    }

    #[test]
    fn remove_field() {
        let mut model = sample_class();
        ClassTransformSpec::RemoveField {
            name: "count".into(),
            descriptor: None,
        }
        .apply(&mut model);
        assert_eq!(model.fields.len(), 1);
    }

    #[test]
    fn rename_method() {
        let mut model = sample_class();
        ClassTransformSpec::RenameMethod {
            from: "helper".into(),
            to: "doWork".into(),
        }
        .apply(&mut model);
        assert!(model.methods.iter().any(|m| m.name == "doWork"));
        assert!(model.methods.iter().all(|m| m.name != "helper"));
    }

    #[test]
    fn rename_field() {
        let mut model = sample_class();
        ClassTransformSpec::RenameField {
            from: "count".into(),
            to: "total".into(),
        }
        .apply(&mut model);
        assert!(model.fields.iter().any(|f| f.name == "total"));
        assert!(model.fields.iter().all(|f| f.name != "count"));
    }

    #[test]
    fn set_method_access_flags() {
        let mut model = sample_class();
        let flags = (MethodAccessFlags::PUBLIC | MethodAccessFlags::FINAL).bits();
        ClassTransformSpec::SetMethodAccessFlags {
            name: "main".into(),
            flags,
        }
        .apply(&mut model);
        let main = model.methods.iter().find(|m| m.name == "main").unwrap();
        assert_eq!(
            main.access_flags,
            MethodAccessFlags::PUBLIC | MethodAccessFlags::FINAL
        );
    }

    #[test]
    fn set_field_access_flags() {
        let mut model = sample_class();
        let flags = (FieldAccessFlags::PUBLIC | FieldAccessFlags::FINAL).bits();
        ClassTransformSpec::SetFieldAccessFlags {
            name: "count".into(),
            flags,
        }
        .apply(&mut model);
        let count = model.fields.iter().find(|f| f.name == "count").unwrap();
        assert_eq!(
            count.access_flags,
            FieldAccessFlags::PUBLIC | FieldAccessFlags::FINAL
        );
    }

    #[test]
    fn sequence() {
        let mut model = sample_class();
        ClassTransformSpec::Sequence(vec![
            ClassTransformSpec::RenameClass("test/New".into()),
            ClassTransformSpec::AddAccessFlags(ClassAccessFlags::FINAL.bits()),
            ClassTransformSpec::RemoveInterface("java/io/Serializable".into()),
        ])
        .apply(&mut model);
        assert_eq!(model.name, "test/New");
        assert!(model.access_flags.contains(ClassAccessFlags::FINAL));
        assert!(model.interfaces.is_empty());
    }

    #[test]
    fn rename_method_renames_all_overloads() {
        let mut class = sample_class();
        class.methods.push(MethodModel::new(
            MethodAccessFlags::PUBLIC,
            "foo".to_string(),
            "()V".to_string(),
            Some(CodeModel::new(1, 1, DebugInfoState::Fresh)),
            vec![],
        ));
        class.methods.push(MethodModel::new(
            MethodAccessFlags::PUBLIC,
            "foo".to_string(),
            "(I)V".to_string(),
            Some(CodeModel::new(1, 1, DebugInfoState::Fresh)),
            vec![],
        ));
        ClassTransformSpec::RenameMethod {
            from: "foo".to_string(),
            to: "bar".to_string(),
        }
        .apply(&mut class);
        assert_eq!(class.methods.iter().filter(|m| m.name == "bar").count(), 2);
        assert!(!class.methods.iter().any(|m| m.name == "foo"));
    }

    #[test]
    fn display() {
        let spec = ClassTransformSpec::RenameClass("test/Foo".into());
        assert_eq!(format!("{spec}"), "rename_class(\"test/Foo\")");

        let seq = ClassTransformSpec::Sequence(vec![
            ClassTransformSpec::AddInterface("java/lang/Runnable".into()),
            ClassTransformSpec::RemoveMethod {
                name: "foo".into(),
                descriptor: None,
            },
        ]);
        let s = format!("{seq}");
        assert!(s.starts_with("sequence("));
        assert!(s.contains("add_interface"));
        assert!(s.contains("remove_method"));
    }

    #[test]
    fn redirect_method_and_field_and_replace_string() {
        let mut code = CodeModel::new(2, 1, DebugInfoState::Fresh);
        code.instructions = vec![
            CodeItem::Method(MethodInsn {
                opcode: 0xB6,
                owner: "pkg/A".into(),
                name: "m".into(),
                descriptor: "()V".into(),
                is_interface: false,
            }),
            CodeItem::InterfaceMethod(InterfaceMethodInsn {
                owner: "pkg/I".into(),
                name: "im".into(),
                descriptor: "()V".into(),
            }),
            CodeItem::Field(crate::model::FieldInsn {
                opcode: 0xB4,
                owner: "pkg/A".into(),
                name: "f".into(),
                descriptor: "I".into(),
            }),
            CodeItem::Field(crate::model::FieldInsn {
                opcode: 0xB4,
                owner: "pkg/A".into(),
                name: "f".into(),
                descriptor: "J".into(),
            }),
            CodeItem::Ldc(crate::model::LdcInsn {
                value: LdcValue::String(JavaString::from_utf16(vec![0xD800])),
            }),
        ];

        CodeTransformSpec::Sequence(vec![
            CodeTransformSpec::RedirectMethodCall {
                from_owner: "pkg/A".into(),
                from_name: "m".into(),
                from_descriptor: "()V".into(),
                to_owner: "pkg/I2".into(),
                to_name: "m2".into(),
                to_descriptor: Some("(I)V".into()),
                to_is_interface: Some(true),
            },
            CodeTransformSpec::RedirectMethodCall {
                from_owner: "pkg/I".into(),
                from_name: "im".into(),
                from_descriptor: "()V".into(),
                to_owner: "pkg/Impl".into(),
                to_name: "im2".into(),
                to_descriptor: None,
                to_is_interface: Some(false),
            },
            CodeTransformSpec::RedirectFieldAccess {
                from_owner: "pkg/A".into(),
                from_name: "f".into(),
                from_descriptor: Some("I".into()),
                to_owner: "pkg/B".into(),
                to_name: "g".into(),
                to_descriptor: Some("Z".into()),
            },
            CodeTransformSpec::ReplaceString {
                from: JavaString::from_utf16(vec![0xD800]),
                to: JavaString::from_utf16(vec![0xD801]),
            },
        ])
        .apply(&mut code);

        assert!(matches!(
            &code.instructions[0],
            CodeItem::InterfaceMethod(i) if i.owner == "pkg/I2" && i.name == "m2" && i.descriptor == "(I)V"
        ));
        assert!(matches!(
            &code.instructions[1],
            CodeItem::Method(m) if m.opcode == 0xB6 && !m.is_interface && m.owner == "pkg/Impl" && m.name == "im2"
        ));
        assert!(matches!(
            &code.instructions[2],
            CodeItem::Field(f) if f.owner == "pkg/B" && f.name == "g" && f.descriptor == "Z"
        ));
        assert!(matches!(
            &code.instructions[3],
            CodeItem::Field(f) if f.owner == "pkg/A" && f.name == "f" && f.descriptor == "J"
        ));
        assert!(matches!(
            &code.instructions[4],
            CodeItem::Ldc(l) if l.value == LdcValue::String(JavaString::from_utf16(vec![0xD801]))
        ));
    }

    #[test]
    fn field_and_method_transform_specs() {
        let mut class = sample_class();
        assert!(
            FieldTransformSpec::Rename("renamedCount".into()).apply_to_field(&mut class.fields[0])
        );
        assert_eq!(class.fields[0].name, "renamedCount");
        assert!(!FieldTransformSpec::Remove.apply_to_field(&mut class.fields[0]));

        assert!(
            MethodTransformSpec::Sequence(vec![
                MethodTransformSpec::Rename("entry".into()),
                MethodTransformSpec::SetDescriptor("()V".into()),
                MethodTransformSpec::CodeTransform(CodeTransformSpec::ReplaceString {
                    from: "a".into(),
                    to: "b".into(),
                }),
            ])
            .apply_to_method(&mut class.methods[1])
        );
        assert_eq!(class.methods[1].name, "entry");
        assert_eq!(class.methods[1].descriptor, "()V");
        assert!(!MethodTransformSpec::Remove.apply_to_method(&mut class.methods[1]));
    }
}
