//! Declarative matcher specifications that can be evaluated purely in Rust
//! without crossing the FFI boundary.
//!
//! Each `*MatcherSpec` enum represents a matcher expression tree that Python
//! constructs (via PyO3 wrappers) and Rust evaluates natively. This eliminates
//! per-match FFI overhead when running transform pipelines from Python.

use crate::constants::{ClassAccessFlags, FieldAccessFlags, MethodAccessFlags};
use crate::model::{ClassModel, FieldModel, MethodModel};
use regex::Regex;
use std::fmt;

/// Build a regex that matches the entire string (Python ``re.fullmatch`` semantics).
fn fullmatch_regex(pattern: &str) -> Option<Regex> {
    let anchored = format!("^(?:{pattern})$");
    Regex::new(&anchored).ok()
}

// ---------------------------------------------------------------------------
// ClassMatcherSpec
// ---------------------------------------------------------------------------

/// A declarative matcher for [`ClassModel`] instances.
#[derive(Debug, Clone)]
pub enum ClassMatcherSpec {
    /// Always matches.
    Any,
    /// Match by exact class name.
    Named(String),
    /// Match by regex against class name.
    NameMatches(String),
    /// Match when **all** given access flags are set.
    AccessAll(u16),
    /// Match when **any** of the given access flags are set.
    AccessAny(u16),
    /// Match classes whose `access_flags & (PUBLIC)` is zero.
    IsPackagePrivate,
    /// Match by exact super-class name.
    Extends(String),
    /// Match when the class implements the named interface.
    Implements(String),
    /// Match by exact major version.
    Version(u16),
    /// Match when major version >= value.
    VersionAtLeast(u16),
    /// Match when major version < value.
    VersionBelow(u16),
    /// Logical AND of child specs.
    And(Vec<ClassMatcherSpec>),
    /// Logical OR of child specs.
    Or(Vec<ClassMatcherSpec>),
    /// Logical negation.
    Not(Box<ClassMatcherSpec>),
}

impl ClassMatcherSpec {
    /// Evaluate this spec against a [`ClassModel`].
    pub fn matches(&self, model: &ClassModel) -> bool {
        match self {
            Self::Any => true,
            Self::Named(name) => model.name == *name,
            Self::NameMatches(pattern) => fullmatch_regex(pattern)
                .map(|re| re.is_match(&model.name))
                .unwrap_or(false),
            Self::AccessAll(flags) => {
                let f = ClassAccessFlags::from_bits_truncate(*flags);
                model.access_flags.contains(f)
            }
            Self::AccessAny(flags) => {
                let f = ClassAccessFlags::from_bits_truncate(*flags);
                model.access_flags.intersects(f)
            }
            Self::IsPackagePrivate => !model.access_flags.contains(ClassAccessFlags::PUBLIC),
            Self::Extends(name) => model.super_name.as_deref() == Some(name.as_str()),
            Self::Implements(name) => model.interfaces.iter().any(|i| i == name),
            Self::Version(major) => model.version.0 == *major,
            Self::VersionAtLeast(major) => model.version.0 >= *major,
            Self::VersionBelow(major) => model.version.0 < *major,
            Self::And(specs) => specs.iter().all(|s| s.matches(model)),
            Self::Or(specs) => specs.iter().any(|s| s.matches(model)),
            Self::Not(spec) => !spec.matches(model),
        }
    }

    /// Build a compiled form with pre-compiled regexes for hot-path evaluation.
    pub fn compile(&self) -> CompiledClassMatcher {
        CompiledClassMatcher::from_spec(self)
    }
}

impl fmt::Display for ClassMatcherSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Any => write!(f, "any()"),
            Self::Named(n) => write!(f, "class_named({n:?})"),
            Self::NameMatches(p) => write!(f, "class_name_matches({p:?})"),
            Self::AccessAll(flags) => write!(f, "class_access({flags:#06x})"),
            Self::AccessAny(flags) => write!(f, "class_access_any({flags:#06x})"),
            Self::IsPackagePrivate => write!(f, "class_is_package_private()"),
            Self::Extends(n) => write!(f, "extends({n:?})"),
            Self::Implements(n) => write!(f, "implements({n:?})"),
            Self::Version(v) => write!(f, "class_version({v})"),
            Self::VersionAtLeast(v) => write!(f, "class_version_at_least({v})"),
            Self::VersionBelow(v) => write!(f, "class_version_below({v})"),
            Self::And(specs) => {
                write!(f, "(")?;
                for (i, s) in specs.iter().enumerate() {
                    if i > 0 {
                        write!(f, " & ")?;
                    }
                    write!(f, "{s}")?;
                }
                write!(f, ")")
            }
            Self::Or(specs) => {
                write!(f, "(")?;
                for (i, s) in specs.iter().enumerate() {
                    if i > 0 {
                        write!(f, " | ")?;
                    }
                    write!(f, "{s}")?;
                }
                write!(f, ")")
            }
            Self::Not(spec) => write!(f, "~{spec}"),
        }
    }
}

// ---------------------------------------------------------------------------
// FieldMatcherSpec
// ---------------------------------------------------------------------------

/// A declarative matcher for [`FieldModel`] instances.
#[derive(Debug, Clone)]
pub enum FieldMatcherSpec {
    /// Always matches.
    Any,
    /// Match by exact field name.
    Named(String),
    /// Match by regex against field name.
    NameMatches(String),
    /// Match by exact field descriptor.
    Descriptor(String),
    /// Match by regex against field descriptor.
    DescriptorMatches(String),
    /// Match when **all** given access flags are set.
    AccessAll(u16),
    /// Match when **any** of the given access flags are set.
    AccessAny(u16),
    /// Match fields with no PUBLIC/PRIVATE/PROTECTED flags.
    IsPackagePrivate,
    /// Logical AND of child specs.
    And(Vec<FieldMatcherSpec>),
    /// Logical OR of child specs.
    Or(Vec<FieldMatcherSpec>),
    /// Logical negation.
    Not(Box<FieldMatcherSpec>),
}

impl FieldMatcherSpec {
    /// Evaluate this spec against a [`FieldModel`].
    pub fn matches(&self, field: &FieldModel) -> bool {
        match self {
            Self::Any => true,
            Self::Named(name) => field.name == *name,
            Self::NameMatches(pattern) => fullmatch_regex(pattern)
                .map(|re| re.is_match(&field.name))
                .unwrap_or(false),
            Self::Descriptor(desc) => field.descriptor == *desc,
            Self::DescriptorMatches(pattern) => fullmatch_regex(pattern)
                .map(|re| re.is_match(&field.descriptor))
                .unwrap_or(false),
            Self::AccessAll(flags) => {
                let f = FieldAccessFlags::from_bits_truncate(*flags);
                field.access_flags.contains(f)
            }
            Self::AccessAny(flags) => {
                let f = FieldAccessFlags::from_bits_truncate(*flags);
                field.access_flags.intersects(f)
            }
            Self::IsPackagePrivate => {
                let visibility = FieldAccessFlags::PUBLIC
                    | FieldAccessFlags::PRIVATE
                    | FieldAccessFlags::PROTECTED;
                !field.access_flags.intersects(visibility)
            }
            Self::And(specs) => specs.iter().all(|s| s.matches(field)),
            Self::Or(specs) => specs.iter().any(|s| s.matches(field)),
            Self::Not(spec) => !spec.matches(field),
        }
    }

    /// Build a compiled form with pre-compiled regexes for hot-path evaluation.
    pub fn compile(&self) -> CompiledFieldMatcher {
        CompiledFieldMatcher::from_spec(self)
    }
}

impl fmt::Display for FieldMatcherSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Any => write!(f, "any()"),
            Self::Named(n) => write!(f, "field_named({n:?})"),
            Self::NameMatches(p) => write!(f, "field_name_matches({p:?})"),
            Self::Descriptor(d) => write!(f, "field_descriptor({d:?})"),
            Self::DescriptorMatches(p) => write!(f, "field_descriptor_matches({p:?})"),
            Self::AccessAll(flags) => write!(f, "field_access({flags:#06x})"),
            Self::AccessAny(flags) => write!(f, "field_access_any({flags:#06x})"),
            Self::IsPackagePrivate => write!(f, "field_is_package_private()"),
            Self::And(specs) => {
                write!(f, "(")?;
                for (i, s) in specs.iter().enumerate() {
                    if i > 0 {
                        write!(f, " & ")?;
                    }
                    write!(f, "{s}")?;
                }
                write!(f, ")")
            }
            Self::Or(specs) => {
                write!(f, "(")?;
                for (i, s) in specs.iter().enumerate() {
                    if i > 0 {
                        write!(f, " | ")?;
                    }
                    write!(f, "{s}")?;
                }
                write!(f, ")")
            }
            Self::Not(spec) => write!(f, "~{spec}"),
        }
    }
}

// ---------------------------------------------------------------------------
// MethodMatcherSpec
// ---------------------------------------------------------------------------

/// A declarative matcher for [`MethodModel`] instances.
#[derive(Debug, Clone)]
pub enum MethodMatcherSpec {
    /// Always matches.
    Any,
    /// Match by exact method name.
    Named(String),
    /// Match by regex against method name.
    NameMatches(String),
    /// Match by exact method descriptor.
    Descriptor(String),
    /// Match by regex against method descriptor.
    DescriptorMatches(String),
    /// Match when **all** given access flags are set.
    AccessAll(u16),
    /// Match when **any** of the given access flags are set.
    AccessAny(u16),
    /// Match methods with no PUBLIC/PRIVATE/PROTECTED flags.
    IsPackagePrivate,
    /// Match methods that have a code attribute.
    HasCode,
    /// Match `<init>` methods.
    IsConstructor,
    /// Match `<clinit>` methods.
    IsStaticInitializer,
    /// Match by return-type descriptor (extracted from method descriptor).
    Returns(String),
    /// Logical AND of child specs.
    And(Vec<MethodMatcherSpec>),
    /// Logical OR of child specs.
    Or(Vec<MethodMatcherSpec>),
    /// Logical negation.
    Not(Box<MethodMatcherSpec>),
}

impl MethodMatcherSpec {
    /// Evaluate this spec against a [`MethodModel`].
    pub fn matches(&self, method: &MethodModel) -> bool {
        match self {
            Self::Any => true,
            Self::Named(name) => method.name == *name,
            Self::NameMatches(pattern) => fullmatch_regex(pattern)
                .map(|re| re.is_match(&method.name))
                .unwrap_or(false),
            Self::Descriptor(desc) => method.descriptor == *desc,
            Self::DescriptorMatches(pattern) => fullmatch_regex(pattern)
                .map(|re| re.is_match(&method.descriptor))
                .unwrap_or(false),
            Self::AccessAll(flags) => {
                let f = MethodAccessFlags::from_bits_truncate(*flags);
                method.access_flags.contains(f)
            }
            Self::AccessAny(flags) => {
                let f = MethodAccessFlags::from_bits_truncate(*flags);
                method.access_flags.intersects(f)
            }
            Self::IsPackagePrivate => {
                let visibility = MethodAccessFlags::PUBLIC
                    | MethodAccessFlags::PRIVATE
                    | MethodAccessFlags::PROTECTED;
                !method.access_flags.intersects(visibility)
            }
            Self::HasCode => method.code.is_some(),
            Self::IsConstructor => method.name == "<init>",
            Self::IsStaticInitializer => method.name == "<clinit>",
            Self::Returns(ret_desc) => extract_return_descriptor(&method.descriptor) == *ret_desc,
            Self::And(specs) => specs.iter().all(|s| s.matches(method)),
            Self::Or(specs) => specs.iter().any(|s| s.matches(method)),
            Self::Not(spec) => !spec.matches(method),
        }
    }

    /// Build a compiled form with pre-compiled regexes for hot-path evaluation.
    pub fn compile(&self) -> CompiledMethodMatcher {
        CompiledMethodMatcher::from_spec(self)
    }
}

impl fmt::Display for MethodMatcherSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Any => write!(f, "any()"),
            Self::Named(n) => write!(f, "method_named({n:?})"),
            Self::NameMatches(p) => write!(f, "method_name_matches({p:?})"),
            Self::Descriptor(d) => write!(f, "method_descriptor({d:?})"),
            Self::DescriptorMatches(p) => write!(f, "method_descriptor_matches({p:?})"),
            Self::AccessAll(flags) => write!(f, "method_access({flags:#06x})"),
            Self::AccessAny(flags) => write!(f, "method_access_any({flags:#06x})"),
            Self::IsPackagePrivate => write!(f, "method_is_package_private()"),
            Self::HasCode => write!(f, "has_code()"),
            Self::IsConstructor => write!(f, "is_constructor()"),
            Self::IsStaticInitializer => write!(f, "is_static_initializer()"),
            Self::Returns(d) => write!(f, "method_returns({d:?})"),
            Self::And(specs) => {
                write!(f, "(")?;
                for (i, s) in specs.iter().enumerate() {
                    if i > 0 {
                        write!(f, " & ")?;
                    }
                    write!(f, "{s}")?;
                }
                write!(f, ")")
            }
            Self::Or(specs) => {
                write!(f, "(")?;
                for (i, s) in specs.iter().enumerate() {
                    if i > 0 {
                        write!(f, " | ")?;
                    }
                    write!(f, "{s}")?;
                }
                write!(f, ")")
            }
            Self::Not(spec) => write!(f, "~{spec}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Return-type extraction helper
// ---------------------------------------------------------------------------

/// Extract the return-type descriptor from a method descriptor string.
///
/// Given `(Ljava/lang/String;I)V`, returns `"V"`.
fn extract_return_descriptor(descriptor: &str) -> &str {
    // Method descriptor grammar: '(' ParameterDescriptor* ')' ReturnDescriptor
    match descriptor.rfind(')') {
        Some(pos) => &descriptor[pos + 1..],
        None => descriptor,
    }
}

// ---------------------------------------------------------------------------
// Compiled matchers — pre-compiled regex for hot-path evaluation
// ---------------------------------------------------------------------------

/// A [`ClassMatcherSpec`] compiled for repeated evaluation (regexes pre-built).
pub enum CompiledClassMatcher {
    Any,
    Named(String),
    NameMatches(Regex),
    AccessAll(ClassAccessFlags),
    AccessAny(ClassAccessFlags),
    IsPackagePrivate,
    Extends(String),
    Implements(String),
    Version(u16),
    VersionAtLeast(u16),
    VersionBelow(u16),
    And(Vec<CompiledClassMatcher>),
    Or(Vec<CompiledClassMatcher>),
    Not(Box<CompiledClassMatcher>),
}

impl CompiledClassMatcher {
    /// Compile from a spec, panicking on invalid regex patterns.
    pub fn from_spec(spec: &ClassMatcherSpec) -> Self {
        match spec {
            ClassMatcherSpec::Any => Self::Any,
            ClassMatcherSpec::Named(n) => Self::Named(n.clone()),
            ClassMatcherSpec::NameMatches(p) => {
                Self::NameMatches(fullmatch_regex(p).expect("invalid regex in ClassMatcherSpec"))
            }
            ClassMatcherSpec::AccessAll(f) => {
                Self::AccessAll(ClassAccessFlags::from_bits_truncate(*f))
            }
            ClassMatcherSpec::AccessAny(f) => {
                Self::AccessAny(ClassAccessFlags::from_bits_truncate(*f))
            }
            ClassMatcherSpec::IsPackagePrivate => Self::IsPackagePrivate,
            ClassMatcherSpec::Extends(n) => Self::Extends(n.clone()),
            ClassMatcherSpec::Implements(n) => Self::Implements(n.clone()),
            ClassMatcherSpec::Version(v) => Self::Version(*v),
            ClassMatcherSpec::VersionAtLeast(v) => Self::VersionAtLeast(*v),
            ClassMatcherSpec::VersionBelow(v) => Self::VersionBelow(*v),
            ClassMatcherSpec::And(specs) => Self::And(specs.iter().map(Self::from_spec).collect()),
            ClassMatcherSpec::Or(specs) => Self::Or(specs.iter().map(Self::from_spec).collect()),
            ClassMatcherSpec::Not(spec) => Self::Not(Box::new(Self::from_spec(spec))),
        }
    }

    /// Evaluate against a [`ClassModel`].
    pub fn matches(&self, model: &ClassModel) -> bool {
        match self {
            Self::Any => true,
            Self::Named(name) => model.name == *name,
            Self::NameMatches(re) => re.is_match(&model.name),
            Self::AccessAll(flags) => model.access_flags.contains(*flags),
            Self::AccessAny(flags) => model.access_flags.intersects(*flags),
            Self::IsPackagePrivate => !model.access_flags.contains(ClassAccessFlags::PUBLIC),
            Self::Extends(name) => model.super_name.as_deref() == Some(name.as_str()),
            Self::Implements(name) => model.interfaces.iter().any(|i| i == name),
            Self::Version(major) => model.version.0 == *major,
            Self::VersionAtLeast(major) => model.version.0 >= *major,
            Self::VersionBelow(major) => model.version.0 < *major,
            Self::And(matchers) => matchers.iter().all(|m| m.matches(model)),
            Self::Or(matchers) => matchers.iter().any(|m| m.matches(model)),
            Self::Not(matcher) => !matcher.matches(model),
        }
    }
}

/// A [`FieldMatcherSpec`] compiled for repeated evaluation.
pub enum CompiledFieldMatcher {
    Any,
    Named(String),
    NameMatches(Regex),
    Descriptor(String),
    DescriptorMatches(Regex),
    AccessAll(FieldAccessFlags),
    AccessAny(FieldAccessFlags),
    IsPackagePrivate,
    And(Vec<CompiledFieldMatcher>),
    Or(Vec<CompiledFieldMatcher>),
    Not(Box<CompiledFieldMatcher>),
}

impl CompiledFieldMatcher {
    pub fn from_spec(spec: &FieldMatcherSpec) -> Self {
        match spec {
            FieldMatcherSpec::Any => Self::Any,
            FieldMatcherSpec::Named(n) => Self::Named(n.clone()),
            FieldMatcherSpec::NameMatches(p) => {
                Self::NameMatches(fullmatch_regex(p).expect("invalid regex in FieldMatcherSpec"))
            }
            FieldMatcherSpec::Descriptor(d) => Self::Descriptor(d.clone()),
            FieldMatcherSpec::DescriptorMatches(p) => Self::DescriptorMatches(
                fullmatch_regex(p).expect("invalid regex in FieldMatcherSpec"),
            ),
            FieldMatcherSpec::AccessAll(f) => {
                Self::AccessAll(FieldAccessFlags::from_bits_truncate(*f))
            }
            FieldMatcherSpec::AccessAny(f) => {
                Self::AccessAny(FieldAccessFlags::from_bits_truncate(*f))
            }
            FieldMatcherSpec::IsPackagePrivate => Self::IsPackagePrivate,
            FieldMatcherSpec::And(specs) => Self::And(specs.iter().map(Self::from_spec).collect()),
            FieldMatcherSpec::Or(specs) => Self::Or(specs.iter().map(Self::from_spec).collect()),
            FieldMatcherSpec::Not(spec) => Self::Not(Box::new(Self::from_spec(spec))),
        }
    }

    pub fn matches(&self, field: &FieldModel) -> bool {
        match self {
            Self::Any => true,
            Self::Named(name) => field.name == *name,
            Self::NameMatches(re) => re.is_match(&field.name),
            Self::Descriptor(desc) => field.descriptor == *desc,
            Self::DescriptorMatches(re) => re.is_match(&field.descriptor),
            Self::AccessAll(flags) => field.access_flags.contains(*flags),
            Self::AccessAny(flags) => field.access_flags.intersects(*flags),
            Self::IsPackagePrivate => {
                let visibility = FieldAccessFlags::PUBLIC
                    | FieldAccessFlags::PRIVATE
                    | FieldAccessFlags::PROTECTED;
                !field.access_flags.intersects(visibility)
            }
            Self::And(matchers) => matchers.iter().all(|m| m.matches(field)),
            Self::Or(matchers) => matchers.iter().any(|m| m.matches(field)),
            Self::Not(matcher) => !matcher.matches(field),
        }
    }
}

/// A [`MethodMatcherSpec`] compiled for repeated evaluation.
pub enum CompiledMethodMatcher {
    Any,
    Named(String),
    NameMatches(Regex),
    Descriptor(String),
    DescriptorMatches(Regex),
    AccessAll(MethodAccessFlags),
    AccessAny(MethodAccessFlags),
    IsPackagePrivate,
    HasCode,
    IsConstructor,
    IsStaticInitializer,
    Returns(String),
    And(Vec<CompiledMethodMatcher>),
    Or(Vec<CompiledMethodMatcher>),
    Not(Box<CompiledMethodMatcher>),
}

impl CompiledMethodMatcher {
    pub fn from_spec(spec: &MethodMatcherSpec) -> Self {
        match spec {
            MethodMatcherSpec::Any => Self::Any,
            MethodMatcherSpec::Named(n) => Self::Named(n.clone()),
            MethodMatcherSpec::NameMatches(p) => {
                Self::NameMatches(fullmatch_regex(p).expect("invalid regex in MethodMatcherSpec"))
            }
            MethodMatcherSpec::Descriptor(d) => Self::Descriptor(d.clone()),
            MethodMatcherSpec::DescriptorMatches(p) => Self::DescriptorMatches(
                fullmatch_regex(p).expect("invalid regex in MethodMatcherSpec"),
            ),
            MethodMatcherSpec::AccessAll(f) => {
                Self::AccessAll(MethodAccessFlags::from_bits_truncate(*f))
            }
            MethodMatcherSpec::AccessAny(f) => {
                Self::AccessAny(MethodAccessFlags::from_bits_truncate(*f))
            }
            MethodMatcherSpec::IsPackagePrivate => Self::IsPackagePrivate,
            MethodMatcherSpec::HasCode => Self::HasCode,
            MethodMatcherSpec::IsConstructor => Self::IsConstructor,
            MethodMatcherSpec::IsStaticInitializer => Self::IsStaticInitializer,
            MethodMatcherSpec::Returns(d) => Self::Returns(d.clone()),
            MethodMatcherSpec::And(specs) => Self::And(specs.iter().map(Self::from_spec).collect()),
            MethodMatcherSpec::Or(specs) => Self::Or(specs.iter().map(Self::from_spec).collect()),
            MethodMatcherSpec::Not(spec) => Self::Not(Box::new(Self::from_spec(spec))),
        }
    }

    pub fn matches(&self, method: &MethodModel) -> bool {
        match self {
            Self::Any => true,
            Self::Named(name) => method.name == *name,
            Self::NameMatches(re) => re.is_match(&method.name),
            Self::Descriptor(desc) => method.descriptor == *desc,
            Self::DescriptorMatches(re) => re.is_match(&method.descriptor),
            Self::AccessAll(flags) => method.access_flags.contains(*flags),
            Self::AccessAny(flags) => method.access_flags.intersects(*flags),
            Self::IsPackagePrivate => {
                let visibility = MethodAccessFlags::PUBLIC
                    | MethodAccessFlags::PRIVATE
                    | MethodAccessFlags::PROTECTED;
                !method.access_flags.intersects(visibility)
            }
            Self::HasCode => method.code.is_some(),
            Self::IsConstructor => method.name == "<init>",
            Self::IsStaticInitializer => method.name == "<clinit>",
            Self::Returns(ret_desc) => extract_return_descriptor(&method.descriptor) == *ret_desc,
            Self::And(matchers) => matchers.iter().all(|m| m.matches(method)),
            Self::Or(matchers) => matchers.iter().any(|m| m.matches(method)),
            Self::Not(matcher) => !matcher.matches(method),
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
            version: (61, 0), // Java 17
            access_flags: ClassAccessFlags::PUBLIC | ClassAccessFlags::FINAL,
            name: "test/Sample".into(),
            super_name: Some("java/lang/Object".into()),
            interfaces: vec!["java/io/Serializable".into()],
            fields: vec![
                FieldModel {
                    access_flags: FieldAccessFlags::PRIVATE | FieldAccessFlags::FINAL,
                    name: "count".into(),
                    descriptor: "I".into(),
                    attributes: vec![],
                },
                FieldModel {
                    access_flags: FieldAccessFlags::PUBLIC | FieldAccessFlags::STATIC,
                    name: "INSTANCE".into(),
                    descriptor: "Ltest/Sample;".into(),
                    attributes: vec![],
                },
                FieldModel {
                    access_flags: FieldAccessFlags::empty(),
                    name: "data".into(),
                    descriptor: "[B".into(),
                    attributes: vec![],
                },
            ],
            methods: vec![
                MethodModel::new(
                    MethodAccessFlags::PUBLIC,
                    "<init>".into(),
                    "()V".into(),
                    Some(CodeModel::new(1, 1, DebugInfoState::Fresh)),
                    vec![],
                ),
                MethodModel::new(
                    MethodAccessFlags::PUBLIC | MethodAccessFlags::STATIC,
                    "main".into(),
                    "([Ljava/lang/String;)V".into(),
                    Some(CodeModel::new(2, 1, DebugInfoState::Fresh)),
                    vec![],
                ),
                MethodModel::new(
                    MethodAccessFlags::PRIVATE,
                    "helper".into(),
                    "(II)Ljava/lang/String;".into(),
                    Some(CodeModel::new(1, 3, DebugInfoState::Fresh)),
                    vec![],
                ),
                MethodModel::new(
                    MethodAccessFlags::PUBLIC | MethodAccessFlags::ABSTRACT,
                    "abstractMethod".into(),
                    "()I".into(),
                    None,
                    vec![],
                ),
                MethodModel::new(
                    MethodAccessFlags::STATIC,
                    "<clinit>".into(),
                    "()V".into(),
                    Some(CodeModel::new(1, 0, DebugInfoState::Fresh)),
                    vec![],
                ),
            ],
            attributes: vec![],
            constant_pool: ConstantPoolBuilder::new(),
            debug_info_state: DebugInfoState::Fresh,
        }
    }

    // -----------------------------------------------------------------------
    // ClassMatcherSpec
    // -----------------------------------------------------------------------

    #[test]
    fn class_any_matches_everything() {
        let model = sample_class();
        assert!(ClassMatcherSpec::Any.matches(&model));
    }

    #[test]
    fn class_named_exact_match() {
        let model = sample_class();
        assert!(ClassMatcherSpec::Named("test/Sample".into()).matches(&model));
        assert!(!ClassMatcherSpec::Named("test/Other".into()).matches(&model));
    }

    #[test]
    fn class_name_matches_regex() {
        let model = sample_class();
        assert!(ClassMatcherSpec::NameMatches("test/.*".into()).matches(&model));
        assert!(!ClassMatcherSpec::NameMatches("^other/".into()).matches(&model));
    }

    #[test]
    fn class_access_all() {
        let model = sample_class();
        let pub_final = (ClassAccessFlags::PUBLIC | ClassAccessFlags::FINAL).bits();
        assert!(ClassMatcherSpec::AccessAll(pub_final).matches(&model));
        let pub_abstract = (ClassAccessFlags::PUBLIC | ClassAccessFlags::ABSTRACT).bits();
        assert!(!ClassMatcherSpec::AccessAll(pub_abstract).matches(&model));
    }

    #[test]
    fn class_access_any() {
        let model = sample_class();
        let abstract_or_final = (ClassAccessFlags::ABSTRACT | ClassAccessFlags::FINAL).bits();
        assert!(ClassMatcherSpec::AccessAny(abstract_or_final).matches(&model));
    }

    #[test]
    fn class_is_package_private() {
        let model = sample_class();
        assert!(!ClassMatcherSpec::IsPackagePrivate.matches(&model)); // PUBLIC

        let mut pp = sample_class();
        pp.access_flags = ClassAccessFlags::FINAL;
        assert!(ClassMatcherSpec::IsPackagePrivate.matches(&pp));
    }

    #[test]
    fn class_extends() {
        let model = sample_class();
        assert!(ClassMatcherSpec::Extends("java/lang/Object".into()).matches(&model));
        assert!(!ClassMatcherSpec::Extends("java/lang/Thread".into()).matches(&model));
    }

    #[test]
    fn class_implements() {
        let model = sample_class();
        assert!(ClassMatcherSpec::Implements("java/io/Serializable".into()).matches(&model));
        assert!(!ClassMatcherSpec::Implements("java/lang/Cloneable".into()).matches(&model));
    }

    #[test]
    fn class_version_matchers() {
        let model = sample_class(); // version (61, 0)
        assert!(ClassMatcherSpec::Version(61).matches(&model));
        assert!(!ClassMatcherSpec::Version(52).matches(&model));
        assert!(ClassMatcherSpec::VersionAtLeast(52).matches(&model));
        assert!(ClassMatcherSpec::VersionAtLeast(61).matches(&model));
        assert!(!ClassMatcherSpec::VersionAtLeast(62).matches(&model));
        assert!(ClassMatcherSpec::VersionBelow(62).matches(&model));
        assert!(!ClassMatcherSpec::VersionBelow(61).matches(&model));
    }

    #[test]
    fn class_and_or_not_combinators() {
        let model = sample_class();
        let spec = ClassMatcherSpec::And(vec![
            ClassMatcherSpec::Named("test/Sample".into()),
            ClassMatcherSpec::Extends("java/lang/Object".into()),
        ]);
        assert!(spec.matches(&model));

        let spec = ClassMatcherSpec::Or(vec![
            ClassMatcherSpec::Named("test/Other".into()),
            ClassMatcherSpec::Named("test/Sample".into()),
        ]);
        assert!(spec.matches(&model));

        let spec = ClassMatcherSpec::Not(Box::new(ClassMatcherSpec::Named("test/Other".into())));
        assert!(spec.matches(&model));
    }

    // -----------------------------------------------------------------------
    // FieldMatcherSpec
    // -----------------------------------------------------------------------

    #[test]
    fn field_named_and_descriptor() {
        let model = sample_class();
        let count_field = &model.fields[0];
        assert!(FieldMatcherSpec::Named("count".into()).matches(count_field));
        assert!(FieldMatcherSpec::Descriptor("I".into()).matches(count_field));
        assert!(!FieldMatcherSpec::Named("other".into()).matches(count_field));
    }

    #[test]
    fn field_name_matches_regex() {
        let model = sample_class();
        assert!(FieldMatcherSpec::NameMatches("^count$".into()).matches(&model.fields[0]));
        assert!(FieldMatcherSpec::NameMatches("^INST.*".into()).matches(&model.fields[1]));
    }

    #[test]
    fn field_descriptor_matches_regex() {
        let model = sample_class();
        let instance = &model.fields[1];
        assert!(FieldMatcherSpec::DescriptorMatches("^Ltest/.*".into()).matches(instance));
    }

    #[test]
    fn field_access_flags() {
        let model = sample_class();
        let count = &model.fields[0]; // PRIVATE FINAL
        assert!(
            FieldMatcherSpec::AccessAll(
                (FieldAccessFlags::PRIVATE | FieldAccessFlags::FINAL).bits()
            )
            .matches(count)
        );
        assert!(
            !FieldMatcherSpec::AccessAll(
                (FieldAccessFlags::PUBLIC | FieldAccessFlags::FINAL).bits()
            )
            .matches(count)
        );
    }

    #[test]
    fn field_is_package_private() {
        let model = sample_class();
        let data = &model.fields[2]; // no visibility flags
        assert!(FieldMatcherSpec::IsPackagePrivate.matches(data));
        assert!(!FieldMatcherSpec::IsPackagePrivate.matches(&model.fields[0])); // PRIVATE
    }

    // -----------------------------------------------------------------------
    // MethodMatcherSpec
    // -----------------------------------------------------------------------

    #[test]
    fn method_named_and_descriptor() {
        let model = sample_class();
        let main = &model.methods[1];
        assert!(MethodMatcherSpec::Named("main".into()).matches(main));
        assert!(MethodMatcherSpec::Descriptor("([Ljava/lang/String;)V".into()).matches(main));
    }

    #[test]
    fn method_name_and_descriptor_matches_regex() {
        let model = sample_class();
        let main = &model.methods[1];
        assert!(MethodMatcherSpec::NameMatches("^main$".into()).matches(main));
        assert!(MethodMatcherSpec::DescriptorMatches(r".*\[L.*".into()).matches(main));
    }

    #[test]
    fn method_has_code() {
        let model = sample_class();
        assert!(MethodMatcherSpec::HasCode.matches(&model.methods[0])); // <init> has code
        assert!(!MethodMatcherSpec::HasCode.matches(&model.methods[3])); // abstract, no code
    }

    #[test]
    fn method_is_constructor_and_clinit() {
        let model = sample_class();
        assert!(MethodMatcherSpec::IsConstructor.matches(&model.methods[0]));
        assert!(!MethodMatcherSpec::IsConstructor.matches(&model.methods[1]));
        assert!(MethodMatcherSpec::IsStaticInitializer.matches(&model.methods[4]));
    }

    #[test]
    fn method_returns() {
        let model = sample_class();
        assert!(MethodMatcherSpec::Returns("V".into()).matches(&model.methods[0])); // ()V
        assert!(MethodMatcherSpec::Returns("Ljava/lang/String;".into()).matches(&model.methods[2])); // (II)Ljava/lang/String;
        assert!(MethodMatcherSpec::Returns("I".into()).matches(&model.methods[3])); // ()I
    }

    #[test]
    fn method_is_package_private() {
        let model = sample_class();
        let clinit = &model.methods[4]; // STATIC only, no visibility
        assert!(MethodMatcherSpec::IsPackagePrivate.matches(clinit));
        assert!(!MethodMatcherSpec::IsPackagePrivate.matches(&model.methods[0])); // PUBLIC
    }

    #[test]
    fn method_combinators() {
        let model = sample_class();
        let spec = MethodMatcherSpec::And(vec![
            MethodMatcherSpec::Named("main".into()),
            MethodMatcherSpec::HasCode,
            MethodMatcherSpec::AccessAll(MethodAccessFlags::STATIC.bits()),
        ]);
        assert!(spec.matches(&model.methods[1]));
        assert!(!spec.matches(&model.methods[0]));

        let spec = MethodMatcherSpec::Or(vec![
            MethodMatcherSpec::IsConstructor,
            MethodMatcherSpec::IsStaticInitializer,
        ]);
        assert!(spec.matches(&model.methods[0])); // <init>
        assert!(spec.matches(&model.methods[4])); // <clinit>
        assert!(!spec.matches(&model.methods[1])); // main

        let spec = MethodMatcherSpec::Not(Box::new(MethodMatcherSpec::HasCode));
        assert!(!spec.matches(&model.methods[0])); // has code
        assert!(spec.matches(&model.methods[3])); // abstract, no code
    }

    // -----------------------------------------------------------------------
    // Compiled matchers
    // -----------------------------------------------------------------------

    #[test]
    fn compiled_class_matcher_matches_same_as_spec() {
        let model = sample_class();
        let specs = vec![
            ClassMatcherSpec::Named("test/Sample".into()),
            ClassMatcherSpec::NameMatches("test/.*".into()),
            ClassMatcherSpec::AccessAll(ClassAccessFlags::PUBLIC.bits()),
            ClassMatcherSpec::Extends("java/lang/Object".into()),
            ClassMatcherSpec::Implements("java/io/Serializable".into()),
            ClassMatcherSpec::Version(61),
            ClassMatcherSpec::VersionAtLeast(52),
            ClassMatcherSpec::VersionBelow(62),
            ClassMatcherSpec::IsPackagePrivate,
            ClassMatcherSpec::And(vec![
                ClassMatcherSpec::Named("test/Sample".into()),
                ClassMatcherSpec::Version(61),
            ]),
            ClassMatcherSpec::Not(Box::new(ClassMatcherSpec::Named("wrong".into()))),
        ];
        for spec in &specs {
            let compiled = spec.compile();
            assert_eq!(
                spec.matches(&model),
                compiled.matches(&model),
                "mismatch for spec: {spec}"
            );
        }
    }

    #[test]
    fn compiled_field_matcher_matches_same_as_spec() {
        let model = sample_class();
        let specs = vec![
            FieldMatcherSpec::Named("count".into()),
            FieldMatcherSpec::NameMatches("^c.*".into()),
            FieldMatcherSpec::Descriptor("I".into()),
            FieldMatcherSpec::AccessAll(FieldAccessFlags::PRIVATE.bits()),
            FieldMatcherSpec::IsPackagePrivate,
        ];
        for field in &model.fields {
            for spec in &specs {
                let compiled = spec.compile();
                assert_eq!(
                    spec.matches(field),
                    compiled.matches(field),
                    "mismatch for field={} spec={spec}",
                    field.name
                );
            }
        }
    }

    #[test]
    fn compiled_method_matcher_matches_same_as_spec() {
        let model = sample_class();
        let specs = vec![
            MethodMatcherSpec::Named("main".into()),
            MethodMatcherSpec::HasCode,
            MethodMatcherSpec::IsConstructor,
            MethodMatcherSpec::IsStaticInitializer,
            MethodMatcherSpec::Returns("V".into()),
            MethodMatcherSpec::IsPackagePrivate,
            MethodMatcherSpec::DescriptorMatches(r"\(\)".into()),
        ];
        for method in &model.methods {
            for spec in &specs {
                let compiled = spec.compile();
                assert_eq!(
                    spec.matches(method),
                    compiled.matches(method),
                    "mismatch for method={} spec={spec}",
                    method.name
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // Display
    // -----------------------------------------------------------------------

    #[test]
    fn display_format() {
        assert_eq!(
            ClassMatcherSpec::Named("Foo".into()).to_string(),
            "class_named(\"Foo\")"
        );
        assert_eq!(
            MethodMatcherSpec::And(vec![
                MethodMatcherSpec::Named("main".into()),
                MethodMatcherSpec::HasCode,
            ])
            .to_string(),
            "(method_named(\"main\") & has_code())"
        );
        assert_eq!(
            FieldMatcherSpec::Not(Box::new(FieldMatcherSpec::Named("x".into()))).to_string(),
            "~field_named(\"x\")"
        );
    }

    // -----------------------------------------------------------------------
    // extract_return_descriptor
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_return_descriptor() {
        assert_eq!(extract_return_descriptor("()V"), "V");
        assert_eq!(extract_return_descriptor("(II)I"), "I");
        assert_eq!(
            extract_return_descriptor("(Ljava/lang/String;)Ljava/lang/Object;"),
            "Ljava/lang/Object;"
        );
        assert_eq!(extract_return_descriptor("()[B"), "[B");
    }
}
