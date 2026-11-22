mod hierarchy;
mod simulation;
mod verify;

use simulation::ClassContext;
pub(crate) use simulation::{
    initial_method_locals, recompute_and_normalize_code, recompute_frames_for_class,
};
pub use simulation::{recompute_frames, simulate};

pub use hierarchy::{
    ClassResolver, DuplicateClassPolicy, InheritedMethod, JAVA_LANG_OBJECT, MappingClassResolver,
    ResolvedClass, ResolvedMethod, common_superclass, find_overridden_methods, is_subtype,
    iter_superclasses, iter_supertypes,
};
pub use verify::{
    Category, Diagnostic, FailFastError, Location, Severity, verify_classfile,
    verify_classfile_with_options, verify_classmodel, verify_classmodel_with_options,
};

use crate::constants::{FieldAccessFlags, MethodAccessFlags};
use crate::descriptors::{
    BaseType, FieldDescriptor, ReturnType, parameter_slot_count, parse_field_descriptor,
    parse_method_descriptor,
};
use crate::model::{
    BranchInsn, ClassModel, CodeItem, CodeModel, FieldInsn, IIncInsn, InterfaceMethodInsn,
    InvokeDynamicInsn, LdcInsn, LdcValue, MethodInsn, MultiANewArrayInsn, TypeInsn, VarInsn,
};
use crate::names::is_valid_internal_name;
use crate::raw::ArrayType;
use std::collections::{HashMap, HashSet, VecDeque};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AnalysisError {
    #[error("invalid control flow: {reason}")]
    InvalidControlFlow { reason: String },
    #[error("unsupported instruction opcode 0x{opcode:02x}")]
    UnsupportedInstruction { opcode: u8 },
    #[error("stack underflow: needed {needed} slots but only {available} available")]
    StackUnderflow { needed: usize, available: usize },
    #[error("invalid local slot {index}: {reason}")]
    InvalidLocal { index: usize, reason: String },
    #[error("type merge error: {reason}")]
    TypeMerge { reason: String },
    #[error("unresolved class {class_name}")]
    UnresolvedClass { class_name: String },
    #[error("hierarchy cycle detected: {cycle:?}")]
    HierarchyCycle { cycle: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum VType {
    Top,
    Integer,
    Float,
    Long,
    Double,
    Null,
    ReturnAddress(Vec<usize>),
    Object(String),
    UninitializedThis,
    Uninitialized {
        code_index: usize,
        class_name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FrameState {
    pub stack: Vec<VType>,
    pub locals: Vec<VType>,
}

impl FrameState {
    pub fn push(&self, types: impl IntoIterator<Item = VType>) -> Self {
        let mut stack = self.stack.clone();
        for value in types {
            let category2 = is_category2(&value);
            stack.push(value);
            if category2 {
                stack.push(VType::Top);
            }
        }
        Self {
            stack,
            locals: self.locals.clone(),
        }
    }

    pub fn pop(&self, slots: usize) -> Result<(Self, Vec<VType>), AnalysisError> {
        self.check_stack_groups(&[slots])?;
        let remaining = self.stack[..self.stack.len() - slots].to_vec();
        let popped = self.stack[self.stack.len() - slots..]
            .iter()
            .rev()
            .cloned()
            .collect::<Vec<_>>();
        Ok((
            Self {
                stack: remaining,
                locals: self.locals.clone(),
            },
            popped,
        ))
    }

    pub fn peek(&self, depth: usize) -> Result<&VType, AnalysisError> {
        let needed = depth.checked_add(1).ok_or(AnalysisError::StackUnderflow {
            needed: usize::MAX,
            available: self.stack.len(),
        })?;
        let index = self
            .stack
            .len()
            .checked_sub(needed)
            .ok_or(AnalysisError::StackUnderflow {
                needed,
                available: self.stack.len(),
            })?;
        Ok(&self.stack[index])
    }

    pub fn set_local(&self, index: usize, value: VType) -> Self {
        let width = if is_category2(&value) { 2 } else { 1 };
        let needed = index
            .checked_add(width)
            .expect("local slot index overflows usize");
        let mut locals = self.locals.clone();
        if locals.len() < needed {
            locals.resize(needed, VType::Top);
        }
        if index > 0 && is_category2(&locals[index - 1]) {
            locals[index - 1] = VType::Top;
        }
        locals[index] = value.clone();
        if is_category2(&value) {
            locals[index + 1] = VType::Top;
        }
        Self {
            stack: self.stack.clone(),
            locals,
        }
    }

    pub fn get_local(&self, index: usize) -> Result<&VType, AnalysisError> {
        let value = self.locals.get(index).ok_or(AnalysisError::InvalidLocal {
            index,
            reason: "slot is out of range".to_owned(),
        })?;
        if matches!(value, VType::Top) {
            return Err(AnalysisError::InvalidLocal {
                index,
                reason: "slot is not initialized".to_owned(),
            });
        }
        Ok(value)
    }

    pub fn stack_depth(&self) -> usize {
        self.stack.len()
    }

    // Groups are expressed in slots, from the top of the stack down. Every
    // boundary must fall between complete values, never inside a long/double.
    fn check_stack_groups(&self, groups: &[usize]) -> Result<(), AnalysisError> {
        let needed: usize = groups.iter().sum();
        if self.stack.len() < needed {
            return Err(AnalysisError::StackUnderflow {
                needed,
                available: self.stack.len(),
            });
        }
        let mut end = self.stack.len();
        for &group in groups {
            let start = end - group;
            while end > start {
                match &self.stack[end - 1] {
                    VType::Top if end >= start + 2 && is_category2(&self.stack[end - 2]) => {
                        end -= 2
                    }
                    VType::Top | VType::Long | VType::Double => {
                        return Err(type_error("stack operation splits a category-2 value"));
                    }
                    _ => end -= 1,
                }
            }
        }
        Ok(())
    }

    fn pop_reference(&self) -> Result<(Self, VType), AnalysisError> {
        self.check_stack_groups(&[1])?;
        let value = self.stack[self.stack.len() - 1].clone();
        if !is_reference(&value) {
            return Err(type_error(format!(
                "expected reference type, found {value:?}"
            )));
        }
        Ok((
            Self {
                stack: self.stack[..self.stack.len() - 1].to_vec(),
                locals: self.locals.clone(),
            },
            value,
        ))
    }

    fn pop_typed(&self, expected: &VType, env: &TypeEnv<'_>) -> Result<Self, AnalysisError> {
        let width = if is_category2(expected) { 2 } else { 1 };
        self.check_stack_groups(&[width])?;
        let actual = &self.stack[self.stack.len() - width];
        require_type(actual, expected, env)?;
        Ok(Self {
            stack: self.stack[..self.stack.len() - width].to_vec(),
            locals: self.locals.clone(),
        })
    }
}

fn type_error(reason: impl Into<String>) -> AnalysisError {
    AnalysisError::InvalidControlFlow {
        reason: reason.into(),
    }
}

/// A type the recomputed frames could not prove verifiable, and why.
///
/// Frame computation needs the class hierarchy to merge reference types at
/// join points. When the hierarchy of a merged class is unavailable (no
/// resolver, or a class missing from it), the merge falls back to
/// `java/lang/Object`. That is harmless unless the merged value later
/// reaches an instruction that requires a more specific class — then the
/// JVM verifier will most likely reject the frames. Each warning records
/// one method where that happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameWarning {
    pub class_name: String,
    pub method_name: String,
    pub method_descriptor: String,
    /// Code-item indices of the instructions that received the imprecise type.
    pub code_indices: Vec<usize>,
    /// The types those instructions require, sorted and deduplicated.
    pub required_types: Vec<String>,
    /// Classes whose ancestry the merges needed but could not resolve,
    /// sorted and deduplicated. Supplying them to the resolver fixes the
    /// warning.
    pub unresolved_classes: Vec<String>,
    /// Whether a class resolver was supplied to the computation at all.
    pub resolver_supplied: bool,
}

impl std::fmt::Display for FrameWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn listed(items: &[impl std::fmt::Display], limit: usize) -> String {
            let mut out = items
                .iter()
                .take(limit)
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            if items.len() > limit {
                out.push_str(&format!(", ... ({} more)", items.len() - limit));
            }
            out
        }
        write!(
            f,
            "frames for {}.{}{} may fail JVM verification: a merged reference type fell back to \
             java/lang/Object and reaches instruction(s) {} that require {}; ",
            self.class_name,
            self.method_name,
            self.method_descriptor,
            listed(&self.code_indices, 5),
            listed(&self.required_types, 5),
        )?;
        if self.resolver_supplied {
            write!(
                f,
                "the resolver could not provide the hierarchy of {}",
                listed(&self.unresolved_classes, 8)
            )
        } else {
            write!(
                f,
                "no class resolver was supplied (merges needed the hierarchy of {})",
                listed(&self.unresolved_classes, 8)
            )
        }
    }
}

/// An operand whose type fell back to `java/lang/Object` for lack of
/// hierarchy data and reached an instruction requiring a specific class.
#[derive(Debug, Clone)]
struct UnprovenUse {
    code_index: usize,
    required: String,
}

/// Most dataflow passes one method may take while learning hierarchy facts.
pub(crate) const MAX_INFERENCE_PASSES: usize = 4;

/// Hierarchy context for one method's frame computation.
///
/// Combines the caller's resolver with what the class being analyzed
/// declares about itself, and with facts deduced from the code.
///
/// When a merge needs the ancestry of a class nobody can resolve, the
/// merged value is tracked as a *degraded* type: internally it remembers
/// which classes were merged (see [`DegradedType`]); externally it is
/// `java/lang/Object`. If a degraded value reaches an instruction that
/// requires class `T`, then — because the input is assumed to verify with
/// *some* frames — every merged class must be a subtype of `T`. Those facts
/// are recorded as deduced superclass edges and the dataflow runs again,
/// which then merges to `T` instead of `Object`. Whatever is still unproven
/// after the last pass is reported as a [`FrameWarning`].
pub(crate) struct TypeEnv<'a> {
    resolver: Option<&'a dyn ClassResolver>,
    current: Option<ResolvedClass>,
    /// Deduced superclass edges: class whose ancestry ran out -> an ancestor.
    facts: std::cell::RefCell<std::collections::BTreeMap<String, String>>,
    learned: std::cell::Cell<bool>,
    code_index: std::cell::Cell<usize>,
    degraded: std::cell::RefCell<std::collections::BTreeSet<String>>,
    unproven: std::cell::RefCell<Vec<UnprovenUse>>,
}

impl<'a> TypeEnv<'a> {
    pub(crate) fn new(
        resolver: Option<&'a dyn ClassResolver>,
        current_class: Option<&ClassModel>,
    ) -> Self {
        Self {
            resolver,
            // Only the hierarchy matters here; skip the per-method snapshot.
            current: current_class.map(|model| ResolvedClass {
                name: model.name.clone(),
                super_name: model.super_name.clone(),
                interfaces: model.interfaces.clone(),
                access_flags: model.access_flags,
                methods: Vec::new(),
            }),
            facts: std::cell::RefCell::default(),
            learned: std::cell::Cell::new(false),
            code_index: std::cell::Cell::new(0),
            degraded: std::cell::RefCell::default(),
            unproven: std::cell::RefCell::default(),
        }
    }

    /// Reset per-pass state, keeping the facts learned so far.
    pub(crate) fn begin_pass(&self) {
        self.learned.set(false);
        self.degraded.borrow_mut().clear();
        self.unproven.borrow_mut().clear();
    }

    /// Whether another pass could make the frames more precise.
    pub(crate) fn wants_another_pass(&self) -> bool {
        self.learned.get() && !self.unproven.borrow().is_empty()
    }

    pub(crate) fn set_code_index(&self, code_index: usize) {
        self.code_index.set(code_index);
    }

    /// Resolution from real sources only (the current class, the resolver,
    /// and the implicit `java/lang/Object`), never from deduced facts.
    fn resolve_known(&self, class_name: &str) -> Option<ResolvedClass> {
        // The model being lowered is authoritative for its own declaration,
        // even when the resolver holds an older snapshot of it.
        if let Some(current) = &self.current
            && current.name == class_name
        {
            return Some(current.clone());
        }
        if let Some(resolved) = self
            .resolver
            .and_then(|resolver| resolver.resolve_class(class_name))
        {
            return Some(resolved);
        }
        (class_name == JAVA_LANG_OBJECT).then(|| synthetic_class(JAVA_LANG_OBJECT, None))
    }

    /// Whether `class_name` is known (not deduced) to be a class rather
    /// than an interface.
    fn is_known_class(&self, class_name: &str) -> bool {
        self.resolve_known(class_name)
            .is_some_and(|resolved| !resolved.is_interface())
    }

    fn is_known_interface(&self, class_name: &str) -> bool {
        self.resolve_known(class_name)
            .is_some_and(|resolved| resolved.is_interface())
    }

    /// Merge two reference types (internal names or array descriptors).
    pub(crate) fn merge_references(
        &self,
        left: &str,
        right: &str,
    ) -> Result<String, AnalysisError> {
        // Peel matching reference-array dimensions so a fallback is tracked
        // at the element level: `[LA;` + `[LB;` degrades to an array of a
        // degraded element rather than to a plain `[Ljava/lang/Object;`.
        let mut dims = 0_usize;
        let (mut left_base, mut right_base) = (left, right);
        while dims <= 255 {
            let (Some(left_component), Some(right_component)) =
                (left_base.strip_prefix('['), right_base.strip_prefix('['))
            else {
                break;
            };
            match (
                hierarchy::peel_array_component_reference(left_component),
                hierarchy::peel_array_component_reference(right_component),
            ) {
                (Some(next_left), Some(next_right)) => {
                    left_base = next_left;
                    right_base = next_right;
                    dims += 1;
                }
                _ => break,
            }
        }
        let base = self.merge_base(left_base, right_base)?;
        Ok(array_of(dims, base))
    }

    fn merge_base(&self, left: &str, right: &str) -> Result<String, AnalysisError> {
        if left == right {
            return Ok(left.to_owned());
        }
        let left_degraded = DegradedType::parse(left).filter(|degraded| degraded.dims == 0);
        let right_degraded = DegradedType::parse(right).filter(|degraded| degraded.dims == 0);
        if left_degraded.is_none() && right_degraded.is_none() {
            let common = hierarchy::common_reference_type_lenient(self, left, right)?;
            if common.unresolved.is_empty() {
                return Ok(common.name);
            }
            self.degraded.borrow_mut().extend(common.unresolved);
            return Ok(DegradedType::encode([left, right]));
        }
        // A degraded side stands for plain class instances; merging those
        // with Object, an array, or an array-shaped degraded type is Object.
        for (degraded, other) in [(&left_degraded, right), (&right_degraded, left)] {
            if degraded.is_some() && (other == JAVA_LANG_OBJECT || other.starts_with('[')) {
                return Ok(JAVA_LANG_OBJECT.to_owned());
            }
        }
        let mut members = std::collections::BTreeSet::new();
        for (degraded, plain) in [(&left_degraded, left), (&right_degraded, right)] {
            match degraded {
                Some(degraded) => members.extend(degraded.members.iter().copied()),
                None => {
                    members.insert(plain);
                }
            }
        }
        let mut members_iter = members.iter();
        let mut acc = members_iter
            .next()
            .map(|name| (*name).to_owned())
            .unwrap_or_else(|| JAVA_LANG_OBJECT.to_owned());
        for member in members_iter {
            let common = hierarchy::common_reference_type_lenient(self, &acc, member)?;
            if !common.unresolved.is_empty() {
                self.degraded.borrow_mut().extend(common.unresolved);
                return Ok(DegradedType::encode(members.iter().copied()));
            }
            acc = common.name;
        }
        Ok(acc)
    }

    /// Record that a degraded value reached a use requiring `required`, and
    /// deduce that each merged class is a subtype of it.
    fn degraded_use(&self, members: &[&str], required: &str) -> Result<(), AnalysisError> {
        // A member whose superclass chain is fully known (without deduced
        // edges) and lacks `required` can only flow into that use if
        // `required` is an interface the resolver lacks — and the verifier
        // accepts any reference for an interface. Nothing is imprecise then.
        // If `required` is known to be a class, the use cannot verify with
        // any frames; report it rather than deduce anything.
        let required_is_known_class = self.is_known_class(required);
        for member in members {
            let (chain, stop) = hierarchy::partial_superclass_chain(&KnownOnly(self), member)?;
            if stop.is_none() && !chain.iter().any(|name| name == required) {
                if required_is_known_class {
                    self.unproven.borrow_mut().push(UnprovenUse {
                        code_index: self.code_index.get(),
                        required: required.to_owned(),
                    });
                }
                return Ok(());
            }
        }
        for member in members {
            let (chain, stop) = hierarchy::partial_superclass_chain(self, member)?;
            if chain.iter().any(|name| name == required) {
                continue;
            }
            let Some(stop) = stop else {
                continue;
            };
            if stop == required || self.facts.borrow().contains_key(&stop) {
                continue;
            }
            // Never close a cycle with a deduced edge.
            let (required_chain, _) = hierarchy::partial_superclass_chain(self, required)?;
            if required_chain.contains(&stop) {
                continue;
            }
            self.facts.borrow_mut().insert(stop, required.to_owned());
            self.learned.set(true);
        }
        self.unproven.borrow_mut().push(UnprovenUse {
            code_index: self.code_index.get(),
            required: required.to_owned(),
        });
        Ok(())
    }

    /// Report what stayed imprecise after the last pass.
    pub(crate) fn finish(
        &self,
        class_name: &str,
        method_name: &str,
        method_descriptor: &str,
    ) -> Option<FrameWarning> {
        let unproven = self.unproven.take();
        if unproven.is_empty() {
            return None;
        }
        let mut code_indices = unproven
            .iter()
            .map(|use_| use_.code_index)
            .collect::<Vec<_>>();
        code_indices.sort_unstable();
        code_indices.dedup();
        let required_types = unproven
            .into_iter()
            .map(|use_| use_.required)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        Some(FrameWarning {
            class_name: class_name.to_owned(),
            method_name: method_name.to_owned(),
            method_descriptor: method_descriptor.to_owned(),
            code_indices,
            required_types,
            unresolved_classes: self.degraded.borrow().iter().cloned().collect(),
            resolver_supplied: self.resolver.is_some(),
        })
    }
}

impl ClassResolver for TypeEnv<'_> {
    fn resolve_class(&self, class_name: &str) -> Option<ResolvedClass> {
        self.resolve_known(class_name).or_else(|| {
            self.facts
                .borrow()
                .get(class_name)
                .map(|ancestor| synthetic_class(class_name, Some(ancestor)))
        })
    }
}

/// A view of a [`TypeEnv`] without its deduced facts.
struct KnownOnly<'e, 'a>(&'e TypeEnv<'a>);

impl ClassResolver for KnownOnly<'_, '_> {
    fn resolve_class(&self, class_name: &str) -> Option<ResolvedClass> {
        self.0.resolve_known(class_name)
    }
}

fn synthetic_class(name: &str, super_name: Option<&str>) -> ResolvedClass {
    ResolvedClass {
        name: name.to_owned(),
        super_name: super_name.map(ToOwned::to_owned),
        interfaces: Vec::new(),
        access_flags: crate::constants::ClassAccessFlags::PUBLIC
            | crate::constants::ClassAccessFlags::SUPER,
        methods: Vec::new(),
    }
}

fn array_of(dims: usize, element: String) -> String {
    if dims == 0 {
        element
    } else if element.starts_with('[') {
        format!("{}{element}", "[".repeat(dims))
    } else {
        format!("{}L{element};", "[".repeat(dims))
    }
}

/// The internal form of a merge that fell back for lack of hierarchy data.
///
/// Encoded as `.` followed by the merged class names joined with `.` —
/// never a valid internal name, since `.` is illegal in one — optionally
/// wrapped in array dimensions (`[L.A.B;`). It exists only inside one
/// method's dataflow: every frame or state handed out goes through
/// [`public_vtype`], which turns it into the `java/lang/Object` it stands for.
struct DegradedType<'s> {
    dims: usize,
    members: Vec<&'s str>,
}

impl<'s> DegradedType<'s> {
    fn parse(name: &'s str) -> Option<Self> {
        let element = name.trim_start_matches('[');
        let dims = name.len() - element.len();
        let base = if dims == 0 {
            element
        } else {
            element.strip_prefix('L')?.strip_suffix(';')?
        };
        let members = base.strip_prefix('.')?;
        Some(Self {
            dims,
            members: members.split('.').collect(),
        })
    }

    fn encode<'n>(members: impl IntoIterator<Item = &'n str>) -> String {
        let mut names = std::collections::BTreeSet::new();
        for member in members {
            match DegradedType::parse(member) {
                Some(degraded) => names.extend(degraded.members),
                None => {
                    names.insert(member);
                }
            }
        }
        let mut out = String::new();
        for name in names {
            out.push('.');
            out.push_str(name);
        }
        out
    }
}

/// Replace internal degraded types with the `java/lang/Object` they stand for.
pub(crate) fn public_vtype(value: &VType) -> VType {
    match value {
        VType::Object(name) => match DegradedType::parse(name) {
            Some(degraded) => VType::Object(array_of(degraded.dims, JAVA_LANG_OBJECT.to_owned())),
            None => value.clone(),
        },
        other => other.clone(),
    }
}

pub(crate) fn public_frame(frame: &FrameState) -> FrameState {
    FrameState {
        stack: frame.stack.iter().map(public_vtype).collect(),
        locals: frame.locals.iter().map(public_vtype).collect(),
    }
}

fn require_type(actual: &VType, expected: &VType, env: &TypeEnv<'_>) -> Result<(), AnalysisError> {
    if actual == expected {
        return Ok(());
    }
    match (actual, expected) {
        (VType::Null, VType::Object(_)) => Ok(()),
        (VType::Object(actual_name), VType::Object(expected_name)) => {
            if expected_name == JAVA_LANG_OBJECT {
                return Ok(());
            }
            if let Some(degraded) = DegradedType::parse(actual_name) {
                return require_degraded(&degraded, actual, expected, expected_name, env);
            }
            if actual_name.starts_with('[') {
                if expected_name.starts_with('[') {
                    return match hierarchy::is_subtype(env, actual_name, expected_name) {
                        Ok(true) | Err(AnalysisError::UnresolvedClass { .. }) => Ok(()),
                        Ok(false) => unassignable(actual, expected, expected_name, env),
                        Err(error) => Err(error),
                    };
                }
                if matches!(
                    expected_name.as_str(),
                    "java/lang/Cloneable" | "java/io/Serializable"
                ) {
                    return Ok(());
                }
                return Err(type_error(format!(
                    "expected {expected:?}, found {actual:?}"
                )));
            }
            if expected_name.starts_with('[') {
                return Err(type_error(format!(
                    "expected {expected:?}, found {actual:?}"
                )));
            }
            // The verifier accepts any reference where an interface is required.
            if env.is_known_interface(expected_name) {
                return Ok(());
            }
            match hierarchy::is_subtype(env, actual_name, expected_name) {
                Ok(true) | Err(AnalysisError::UnresolvedClass { .. }) => Ok(()),
                Ok(false) => unassignable(actual, expected, expected_name, env),
                Err(error) => Err(error),
            }
        }
        _ => Err(type_error(format!(
            "expected {}, found {}",
            debug_public(expected),
            debug_public(actual)
        ))),
    }
}

/// A reference the known hierarchy shows is not a subtype of the required
/// type. That is certain only when the required element type is primitive
/// or known to be a class; an unknown one may be an interface, which
/// accepts any reference.
fn unassignable(
    actual: &VType,
    expected: &VType,
    expected_name: &str,
    env: &TypeEnv<'_>,
) -> Result<(), AnalysisError> {
    let trimmed = expected_name.trim_start_matches('[');
    let (primitive_element, element) = if trimmed.len() == expected_name.len() {
        (false, trimmed)
    } else {
        match trimmed
            .strip_prefix('L')
            .and_then(|rest| rest.strip_suffix(';'))
        {
            Some(element) => (false, element),
            None => (true, trimmed),
        }
    };
    if primitive_element || env.is_known_class(element) {
        return Err(type_error(format!(
            "expected {expected:?}, found {actual:?}"
        )));
    }
    Ok(())
}

/// A degraded value used where `expected` is required.
fn require_degraded(
    degraded: &DegradedType<'_>,
    actual: &VType,
    expected: &VType,
    expected_name: &str,
    env: &TypeEnv<'_>,
) -> Result<(), AnalysisError> {
    let mismatch = || {
        type_error(format!(
            "expected {}, found {}",
            debug_public(expected),
            debug_public(actual)
        ))
    };
    let mut required = expected_name;
    for _ in 0..degraded.dims {
        required = required
            .strip_prefix('[')
            .and_then(hierarchy::peel_array_component_reference)
            .ok_or_else(mismatch)?;
    }
    if required == JAVA_LANG_OBJECT || env.is_known_interface(required) {
        return Ok(());
    }
    if required.starts_with('[') {
        // The merged values are class instances, never arrays.
        return Err(mismatch());
    }
    env.degraded_use(&degraded.members, required)
}

fn debug_public(value: &VType) -> String {
    format!("{:?}", public_vtype(value))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExceptionSuccessor {
    pub target: usize,
    pub catch_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlFlowNode {
    pub node_index: usize,
    pub code_index: usize,
    pub normal_successors: Vec<usize>,
    pub exception_successors: Vec<ExceptionSuccessor>,
    pub is_block_start: bool,
    pub is_jump_target: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlFlowGraph {
    pub entry_node: usize,
    pub nodes: Vec<ControlFlowNode>,
    pub code_index_to_node: HashMap<usize, usize>,
    pub label_targets: HashMap<crate::model::Label, Option<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulationResult {
    pub cfg: ControlFlowGraph,
    pub entry_frames: Vec<Option<FrameState>>,
    pub max_stack: u16,
    pub max_locals: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackMapFrameState {
    pub code_index: usize,
    pub locals: Vec<VType>,
    pub stack: Vec<VType>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameComputationResult {
    pub max_stack: u16,
    pub max_locals: u16,
    pub frames: Vec<StackMapFrameState>,
    /// Places where missing hierarchy data made the frames imprecise enough
    /// that the JVM verifier will likely reject them. Empty when the frames
    /// are as precise as the code allows.
    pub warnings: Vec<FrameWarning>,
}

pub fn is_category2(value: &VType) -> bool {
    matches!(value, VType::Long | VType::Double)
}

pub fn is_reference(value: &VType) -> bool {
    matches!(
        value,
        VType::Null | VType::Object(_) | VType::UninitializedThis | VType::Uninitialized { .. }
    )
}

pub fn vtype_from_descriptor(descriptor: &FieldDescriptor) -> VType {
    match descriptor {
        FieldDescriptor::Base(
            BaseType::Boolean | BaseType::Byte | BaseType::Char | BaseType::Short | BaseType::Int,
        ) => VType::Integer,
        FieldDescriptor::Base(BaseType::Long) => VType::Long,
        FieldDescriptor::Base(BaseType::Float) => VType::Float,
        FieldDescriptor::Base(BaseType::Double) => VType::Double,
        FieldDescriptor::Object(object) => VType::Object(object.class_name.clone()),
        FieldDescriptor::Array(array) => VType::Object(format!(
            "[{}",
            descriptor_component(array.component_type.as_ref())
        )),
    }
}

pub fn vtype_from_field_descriptor_str(descriptor: &str) -> Result<VType, crate::EngineError> {
    Ok(vtype_from_descriptor(&parse_field_descriptor(descriptor)?))
}

pub fn merge_vtypes(left: &VType, right: &VType, resolver: Option<&dyn ClassResolver>) -> VType {
    if left == right {
        return left.clone();
    }
    if let (VType::ReturnAddress(left_targets), VType::ReturnAddress(right_targets)) = (left, right)
    {
        return VType::ReturnAddress(merge_return_targets(left_targets, right_targets));
    }
    if matches!(left, VType::Null) && matches!(right, VType::Object(_)) {
        return right.clone();
    }
    if matches!(right, VType::Null) && matches!(left, VType::Object(_)) {
        return left.clone();
    }
    match (left, right) {
        (VType::Object(left_name), VType::Object(right_name)) => VType::Object(
            hierarchy::common_reference_type(resolver, left_name, right_name)
                .unwrap_or_else(|_| JAVA_LANG_OBJECT.to_owned()),
        ),
        _ => VType::Top,
    }
}

pub fn build_cfg(code: &CodeModel) -> Result<ControlFlowGraph, AnalysisError> {
    let executable_indices = code
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(index, item)| (!matches!(item, CodeItem::Label(_))).then_some(index))
        .collect::<Vec<_>>();
    if executable_indices.is_empty() {
        return Err(AnalysisError::InvalidControlFlow {
            reason: "code model contains no executable instructions".to_owned(),
        });
    }

    let mut code_index_to_node = HashMap::new();
    for (node_index, code_index) in executable_indices.iter().copied().enumerate() {
        code_index_to_node.insert(code_index, node_index);
    }

    let mut label_targets = HashMap::new();
    let mut label_positions = HashMap::new();
    let mut pending_labels = Vec::new();
    for (code_index, item) in code.instructions.iter().enumerate() {
        match item {
            CodeItem::Label(label) => {
                if label_positions.insert(label.clone(), code_index).is_some() {
                    return Err(AnalysisError::InvalidControlFlow {
                        reason: "duplicate label in CodeModel instructions".to_owned(),
                    });
                }
                pending_labels.push(label.clone());
            }
            _ => {
                let node_index = code_index_to_node[&code_index];
                for label in pending_labels.drain(..) {
                    label_targets.insert(label, Some(node_index));
                }
            }
        }
    }
    for label in pending_labels {
        label_targets.insert(label.clone(), None);
        label_positions.insert(label, code.instructions.len());
    }

    let mut jump_targets = HashSet::new();
    let mut leaders = HashSet::new();
    leaders.insert(0_usize);

    for (node_index, code_index) in executable_indices.iter().copied().enumerate() {
        let item = &code.instructions[code_index];
        for label in branch_targets(item) {
            let Some(Some(target)) = label_targets.get(&label) else {
                return Err(AnalysisError::InvalidControlFlow {
                    reason: "control-flow target label does not point to an instruction".to_owned(),
                });
            };
            jump_targets.insert(*target);
            leaders.insert(*target);
        }
        if terminates_block(item) && node_index + 1 < executable_indices.len() {
            leaders.insert(node_index + 1);
        }
    }

    for handler in &code.exception_handlers {
        let Some(Some(target)) = label_targets.get(&handler.handler) else {
            return Err(AnalysisError::InvalidControlFlow {
                reason: "exception handler target label does not point to an instruction"
                    .to_owned(),
            });
        };
        jump_targets.insert(*target);
        leaders.insert(*target);
    }

    let mut nodes = executable_indices
        .iter()
        .copied()
        .enumerate()
        .map(|(node_index, code_index)| ControlFlowNode {
            node_index,
            code_index,
            normal_successors: Vec::new(),
            exception_successors: Vec::new(),
            is_block_start: leaders.contains(&node_index),
            is_jump_target: jump_targets.contains(&node_index),
        })
        .collect::<Vec<_>>();

    for (node_index, code_index) in executable_indices.iter().copied().enumerate() {
        let item = &code.instructions[code_index];
        let next = (node_index + 1 < executable_indices.len()).then_some(node_index + 1);
        match item {
            CodeItem::Branch(branch) => {
                let Some(Some(target)) = label_targets.get(&branch.target) else {
                    return Err(AnalysisError::InvalidControlFlow {
                        reason: "branch target label does not point to an instruction".to_owned(),
                    });
                };
                nodes[node_index].normal_successors.push(*target);
                if !is_unconditional_branch(branch.opcode)
                    && let Some(next) = next
                {
                    nodes[node_index].normal_successors.push(next);
                }
            }
            CodeItem::LookupSwitch(switch) => {
                let Some(Some(default_target)) = label_targets.get(&switch.default_target) else {
                    return Err(AnalysisError::InvalidControlFlow {
                        reason: "switch default target label does not point to an instruction"
                            .to_owned(),
                    });
                };
                let mut seen_targets = HashSet::new();
                if seen_targets.insert(*default_target) {
                    nodes[node_index].normal_successors.push(*default_target);
                }
                for (_, label) in &switch.pairs {
                    let Some(Some(target)) = label_targets.get(label) else {
                        return Err(AnalysisError::InvalidControlFlow {
                            reason: "switch target label does not point to an instruction"
                                .to_owned(),
                        });
                    };
                    if seen_targets.insert(*target) {
                        nodes[node_index].normal_successors.push(*target);
                    }
                }
            }
            CodeItem::TableSwitch(switch) => {
                let Some(Some(default_target)) = label_targets.get(&switch.default_target) else {
                    return Err(AnalysisError::InvalidControlFlow {
                        reason: "switch default target label does not point to an instruction"
                            .to_owned(),
                    });
                };
                let mut seen_targets = HashSet::new();
                if seen_targets.insert(*default_target) {
                    nodes[node_index].normal_successors.push(*default_target);
                }
                for label in &switch.targets {
                    let Some(Some(target)) = label_targets.get(label) else {
                        return Err(AnalysisError::InvalidControlFlow {
                            reason: "switch target label does not point to an instruction"
                                .to_owned(),
                        });
                    };
                    if seen_targets.insert(*target) {
                        nodes[node_index].normal_successors.push(*target);
                    }
                }
            }
            _ if is_terminal_item(item) => {}
            _ => {
                if let Some(next) = next {
                    nodes[node_index].normal_successors.push(next);
                }
            }
        }
    }

    for handler in &code.exception_handlers {
        let start = label_positions
            .get(&handler.start)
            .copied()
            .ok_or_else(|| AnalysisError::InvalidControlFlow {
                reason: "exception start label missing".to_owned(),
            })?;
        let end = label_positions.get(&handler.end).copied().ok_or_else(|| {
            AnalysisError::InvalidControlFlow {
                reason: "exception end label missing".to_owned(),
            }
        })?;
        if start >= end {
            return Err(AnalysisError::InvalidControlFlow {
                reason: "exception handler start label must precede end label".to_owned(),
            });
        }
        let Some(Some(target)) = label_targets.get(&handler.handler) else {
            return Err(AnalysisError::InvalidControlFlow {
                reason: "exception handler label does not point to an instruction".to_owned(),
            });
        };
        let first_node = executable_indices.partition_point(|&code_index| code_index < start);
        let stop_node = executable_indices.partition_point(|&code_index| code_index < end);
        if first_node >= stop_node {
            return Err(AnalysisError::InvalidControlFlow {
                reason: "exception handler range covers no executable instructions".to_owned(),
            });
        }
        for node in &mut nodes[first_node..stop_node] {
            let edge = ExceptionSuccessor {
                target: *target,
                catch_type: handler.catch_type.clone(),
            };
            if !node.exception_successors.contains(&edge) {
                node.exception_successors.push(edge);
            }
        }
    }

    Ok(ControlFlowGraph {
        entry_node: 0,
        nodes,
        code_index_to_node,
        label_targets,
    })
}

fn simulate_item(
    item: &CodeItem,
    state: &FrameState,
    class: ClassContext<'_>,
    code_index: usize,
    next_node: Option<usize>,
    expected_return: Option<&VType>,
    env: &TypeEnv<'_>,
) -> Result<FrameState, AnalysisError> {
    match item {
        CodeItem::Raw(raw) => {
            simulate_raw_opcode(raw.opcode(), state, raw, code_index, expected_return, env)
        }
        CodeItem::Var(var) => simulate_var(var, state, env),
        CodeItem::IInc(iinc) => simulate_iinc(iinc, state),
        CodeItem::Field(field) => simulate_field(field, state, class, env),
        CodeItem::Method(method) => simulate_method(method, state, class, env),
        CodeItem::InterfaceMethod(method) => simulate_interface_method(method, state, env),
        CodeItem::InvokeDynamic(insn) => simulate_invokedynamic(insn, state, env),
        CodeItem::Type(insn) => simulate_type(insn, state, code_index, env),
        CodeItem::Ldc(insn) => simulate_ldc(insn, state),
        CodeItem::MultiANewArray(insn) => simulate_multianewarray(insn, state, env),
        CodeItem::Branch(branch) => simulate_branch(branch, state, next_node, env),
        CodeItem::LookupSwitch(_) | CodeItem::TableSwitch(_) => {
            state.pop_typed(&VType::Integer, env)
        }
        CodeItem::Label(_) => Err(AnalysisError::InvalidControlFlow {
            reason: "labels do not execute".to_owned(),
        }),
    }
}

fn simulate_var(
    var: &VarInsn,
    state: &FrameState,
    env: &TypeEnv<'_>,
) -> Result<FrameState, AnalysisError> {
    let primitive = match var.opcode {
        0x15 | 0x36 => Some(VType::Integer),
        0x16 | 0x37 => Some(VType::Long),
        0x17 | 0x38 => Some(VType::Float),
        0x18 | 0x39 => Some(VType::Double),
        _ => None,
    };
    if let Some(expected) = primitive {
        if var.opcode <= 0x18 {
            let actual = state.get_local(var.slot as usize)?;
            require_type(actual, &expected, env)?;
            if is_category2(actual) && state.locals.get(var.slot as usize + 1) != Some(&VType::Top)
            {
                return Err(type_error("category-2 local is missing its second slot"));
            }
            return Ok(state.push([expected]));
        }
        return Ok(state
            .pop_typed(&expected, env)?
            .set_local(var.slot as usize, expected));
    }
    match var.opcode {
        0x19 => {
            let value = state.get_local(var.slot as usize)?;
            if !is_reference(value) {
                return Err(type_error("aload requires a reference local"));
            }
            Ok(state.push([value.clone()]))
        }
        0x3A => {
            let (next, popped) = state.pop(1)?;
            let value = popped
                .first()
                .cloned()
                .ok_or(AnalysisError::StackUnderflow {
                    needed: 1,
                    available: 0,
                })?;
            if !is_reference(&value) && !matches!(value, VType::ReturnAddress(_)) {
                return Err(type_error("astore requires a reference or returnAddress"));
            }
            Ok(next.set_local(var.slot as usize, value))
        }
        0xA9 => {
            let value = state.get_local(var.slot as usize)?;
            if matches!(value, VType::ReturnAddress(targets) if !targets.is_empty()) {
                Ok(state.clone())
            } else {
                Err(AnalysisError::InvalidLocal {
                    index: var.slot as usize,
                    reason: "ret requires returnAddress local".to_owned(),
                })
            }
        }
        opcode => Err(AnalysisError::UnsupportedInstruction { opcode }),
    }
}

fn simulate_iinc(iinc: &IIncInsn, state: &FrameState) -> Result<FrameState, AnalysisError> {
    let value = state.get_local(iinc.slot as usize)?;
    if !matches!(value, VType::Integer) {
        return Err(AnalysisError::InvalidLocal {
            index: iinc.slot as usize,
            reason: "iinc requires integer local".to_owned(),
        });
    }
    Ok(state.clone())
}

fn simulate_field(
    field: &FieldInsn,
    state: &FrameState,
    class: ClassContext<'_>,
    env: &TypeEnv<'_>,
) -> Result<FrameState, AnalysisError> {
    let descriptor = parse_field_descriptor(&field.descriptor).map_err(|error| {
        AnalysisError::InvalidControlFlow {
            reason: error.to_string(),
        }
    })?;
    let field_type = vtype_from_descriptor(&descriptor);
    match field.opcode {
        0xB2 => Ok(state.push([field_type])),
        0xB3 => state.pop_typed(&field_type, env),
        0xB4 => {
            let next = state.pop_typed(&VType::Object(field.owner.clone()), env)?;
            Ok(next.push([field_type]))
        }
        0xB5 => {
            let next = state.pop_typed(&field_type, env)?;
            // A constructor may assign its own instance fields before invoking super.
            if next.peek(0)? == &VType::UninitializedThis {
                if field.owner != class.name
                    || class.model.is_some_and(|model| {
                        !model.fields.iter().any(|declared| {
                            declared.name == field.name
                                && declared.descriptor == field.descriptor
                                && !declared.access_flags.contains(FieldAccessFlags::STATIC)
                        })
                    })
                {
                    return Err(type_error(
                        "putfield before initialization requires an instance field declared by the current class",
                    ));
                }
                Ok(next.pop(1)?.0)
            } else {
                next.pop_typed(&VType::Object(field.owner.clone()), env)
            }
        }
        opcode => Err(AnalysisError::UnsupportedInstruction { opcode }),
    }
}

fn simulate_method(
    method: &MethodInsn,
    state: &FrameState,
    class: ClassContext<'_>,
    env: &TypeEnv<'_>,
) -> Result<FrameState, AnalysisError> {
    if !matches!(method.opcode, 0xB6..=0xB8) {
        return Err(AnalysisError::UnsupportedInstruction {
            opcode: method.opcode,
        });
    }
    if method.opcode == 0xB6 && method.is_interface {
        return Err(type_error(
            "invokevirtual (0xb6) cannot target an InterfaceMethodref",
        ));
    }
    let parsed = parse_method_descriptor(&method.descriptor).map_err(|error| {
        AnalysisError::InvalidControlFlow {
            reason: error.to_string(),
        }
    })?;
    if method.name == "<clinit>"
        || (method.name == "<init>"
            && (method.opcode != 0xb7
                || method.is_interface
                || parsed.return_type != ReturnType::Void))
    {
        return Err(type_error(
            "constructor invocation requires non-interface invokespecial and a void descriptor; <clinit> cannot be invoked",
        ));
    }
    let max_param_slots = if method.opcode == 0xB8 { 255 } else { 254 };
    if parameter_slot_count(&parsed) > max_param_slots {
        return Err(type_error(
            "method invocation parameter slots exceed JVMS maximum",
        ));
    }
    let after_args = pop_arguments(state, &parsed.parameter_types, env)?;
    let receiver_state = if method.opcode == 0xB8 {
        after_args
    } else {
        let (after_receiver, receiver) = after_args.pop(1)?;
        if method.opcode == 0xB7 && method.name == "<init>" {
            let replacement = match receiver.first() {
                Some(VType::UninitializedThis) => {
                    if method.owner != class.name
                        && class
                            .model
                            .is_some_and(|model| model.super_name.as_deref() != Some(&method.owner))
                    {
                        return Err(type_error(
                            "constructor receiver requires the current class or its direct superclass",
                        ));
                    }
                    VType::Object(class.name.to_owned())
                }
                Some(VType::Uninitialized { class_name, .. }) if *class_name == method.owner => {
                    VType::Object(class_name.clone())
                }
                _ => {
                    return Err(type_error(
                        "invokespecial <init> requires a matching uninitialized receiver",
                    ));
                }
            };
            initialize_receiver(&after_receiver, receiver.first().cloned(), replacement)
        } else {
            require_type(&receiver[0], &VType::Object(method.owner.clone()), env)?;
            after_receiver
        }
    };
    match &parsed.return_type {
        ReturnType::Void => Ok(receiver_state),
        ReturnType::Field(field) => Ok(receiver_state.push([vtype_from_descriptor(field)])),
    }
}

fn simulate_interface_method(
    method: &InterfaceMethodInsn,
    state: &FrameState,
    env: &TypeEnv<'_>,
) -> Result<FrameState, AnalysisError> {
    if matches!(method.name.as_str(), "<init>" | "<clinit>") {
        return Err(type_error(
            "invokeinterface cannot invoke <init> or <clinit>",
        ));
    }
    let parsed = parse_method_descriptor(&method.descriptor).map_err(|error| {
        AnalysisError::InvalidControlFlow {
            reason: error.to_string(),
        }
    })?;
    if parameter_slot_count(&parsed) > 254 {
        return Err(type_error(
            "invokeinterface parameter slots exceed 254 (count limit 255)",
        ));
    }
    let after_args = pop_arguments(state, &parsed.parameter_types, env)?;
    let after_receiver = after_args.pop_typed(&VType::Object(method.owner.clone()), env)?;
    match &parsed.return_type {
        ReturnType::Void => Ok(after_receiver),
        ReturnType::Field(field) => Ok(after_receiver.push([vtype_from_descriptor(field)])),
    }
}

fn simulate_invokedynamic(
    insn: &InvokeDynamicInsn,
    state: &FrameState,
    env: &TypeEnv<'_>,
) -> Result<FrameState, AnalysisError> {
    if matches!(insn.name.as_str(), "<init>" | "<clinit>") {
        return Err(type_error("invokedynamic cannot invoke <init> or <clinit>"));
    }
    let parsed = parse_method_descriptor(&insn.descriptor).map_err(|error| {
        AnalysisError::InvalidControlFlow {
            reason: error.to_string(),
        }
    })?;
    if parameter_slot_count(&parsed) > 255 {
        return Err(type_error("invokedynamic parameter slots exceed 255"));
    }
    let after_args = pop_arguments(state, &parsed.parameter_types, env)?;
    match &parsed.return_type {
        ReturnType::Void => Ok(after_args),
        ReturnType::Field(field) => Ok(after_args.push([vtype_from_descriptor(field)])),
    }
}

fn pop_arguments(
    state: &FrameState,
    parameters: &[FieldDescriptor],
    env: &TypeEnv<'_>,
) -> Result<FrameState, AnalysisError> {
    let mut next = state.clone();
    for parameter in parameters.iter().rev() {
        next = next.pop_typed(&vtype_from_descriptor(parameter), env)?;
    }
    Ok(next)
}

fn simulate_type(
    insn: &TypeInsn,
    state: &FrameState,
    code_index: usize,
    env: &TypeEnv<'_>,
) -> Result<FrameState, AnalysisError> {
    match insn.opcode {
        0xBB => {
            if insn.descriptor.starts_with('[') || !is_valid_internal_name(&insn.descriptor) {
                return Err(type_error(format!(
                    "new requires a non-array internal class name, got {}",
                    insn.descriptor
                )));
            }
            if state.stack.iter().any(|value| {
                matches!(
                    value,
                    VType::Uninitialized { code_index: existing, .. } if *existing == code_index
                )
            }) {
                return Err(type_error(
                    "uninitialized reference from the same new instruction already exists on stack",
                ));
            }
            let sanitized_locals = state
                .locals
                .iter()
                .map(|value| match value {
                    VType::Uninitialized {
                        code_index: existing,
                        ..
                    } if *existing == code_index => VType::Top,
                    other => other.clone(),
                })
                .collect();
            let next_state = FrameState {
                stack: state.stack.clone(),
                locals: sanitized_locals,
            };
            Ok(next_state.push([VType::Uninitialized {
                code_index,
                class_name: insn.descriptor.clone(),
            }]))
        }
        0xBD => {
            let next = state.pop_typed(&VType::Integer, env)?;
            let array_descriptor = anewarray_descriptor(&insn.descriptor);
            let array_dims = array_descriptor.bytes().take_while(|&b| b == b'[').count();
            if array_dims > 255 || parse_field_descriptor(&array_descriptor).is_err() {
                return Err(type_error(format!(
                    "anewarray descriptor is invalid or exceeds 255 dimensions: {array_descriptor}"
                )));
            }
            Ok(next.push([VType::Object(array_descriptor)]))
        }
        0xC0 => {
            let next = state.pop_typed(&VType::Object(JAVA_LANG_OBJECT.to_owned()), env)?;
            Ok(next.push([VType::Object(insn.descriptor.clone())]))
        }
        0xC1 => {
            let next = state.pop_typed(&VType::Object(JAVA_LANG_OBJECT.to_owned()), env)?;
            Ok(next.push([VType::Integer]))
        }
        opcode => Err(AnalysisError::UnsupportedInstruction { opcode }),
    }
}

fn simulate_ldc(insn: &LdcInsn, state: &FrameState) -> Result<FrameState, AnalysisError> {
    let value = match &insn.value {
        LdcValue::Int(_) => VType::Integer,
        LdcValue::FloatBits(_) => VType::Float,
        LdcValue::Long(_) => VType::Long,
        LdcValue::DoubleBits(_) => VType::Double,
        LdcValue::String(_) => VType::Object("java/lang/String".to_owned()),
        LdcValue::Class(_) => VType::Object("java/lang/Class".to_owned()),
        LdcValue::MethodType(_) => VType::Object("java/lang/invoke/MethodType".to_owned()),
        LdcValue::MethodHandle(_) => VType::Object("java/lang/invoke/MethodHandle".to_owned()),
        LdcValue::Dynamic(dynamic) => {
            let descriptor = parse_field_descriptor(&dynamic.descriptor).map_err(|error| {
                AnalysisError::InvalidControlFlow {
                    reason: error.to_string(),
                }
            })?;
            vtype_from_descriptor(&descriptor)
        }
    };
    Ok(state.push([value]))
}

fn simulate_multianewarray(
    insn: &MultiANewArrayInsn,
    state: &FrameState,
    env: &TypeEnv<'_>,
) -> Result<FrameState, AnalysisError> {
    if insn.dimensions == 0 {
        return Err(type_error("multianewarray dimensions must be at least 1"));
    }
    let array_dims = insn.descriptor.bytes().take_while(|&b| b == b'[').count();
    if array_dims < usize::from(insn.dimensions)
        || array_dims > 255
        || parse_field_descriptor(&insn.descriptor).is_err()
    {
        return Err(type_error(format!(
            "multianewarray descriptor {} does not have at least {} valid dimensions",
            insn.descriptor, insn.dimensions
        )));
    }
    let mut next = state.clone();
    for _ in 0..insn.dimensions {
        next = next.pop_typed(&VType::Integer, env)?;
    }
    Ok(next.push([VType::Object(insn.descriptor.clone())]))
}

fn simulate_branch(
    branch: &BranchInsn,
    state: &FrameState,
    next_node: Option<usize>,
    env: &TypeEnv<'_>,
) -> Result<FrameState, AnalysisError> {
    match branch.opcode {
        0x99..=0x9E => state.pop_typed(&VType::Integer, env),
        0xC6 | 0xC7 => Ok(state.pop_reference()?.0),
        0x9F..=0xA4 => state
            .pop_typed(&VType::Integer, env)?
            .pop_typed(&VType::Integer, env),
        0xA5 | 0xA6 => Ok(state.pop_reference()?.0.pop_reference()?.0),
        0xA7 | 0xC8 => Ok(state.clone()),
        0xA8 | 0xC9 => simulate_jsr(state, next_node),
        opcode => Err(AnalysisError::UnsupportedInstruction { opcode }),
    }
}

fn simulate_jsr(state: &FrameState, next_node: Option<usize>) -> Result<FrameState, AnalysisError> {
    let return_target = next_node.ok_or_else(|| AnalysisError::InvalidControlFlow {
        reason: "jsr/jsr_w requires a reachable continuation instruction".to_owned(),
    })?;
    Ok(state.push([VType::ReturnAddress(vec![return_target])]))
}

fn dynamic_successors(item: &CodeItem, state: &FrameState) -> Option<Vec<usize>> {
    match item {
        CodeItem::Var(var) if var.opcode == 0xA9 => match state.get_local(var.slot as usize) {
            Ok(VType::ReturnAddress(targets)) => Some(targets.clone()),
            _ => Some(Vec::new()),
        },
        _ => None,
    }
}

fn simulate_raw_opcode(
    opcode: u8,
    state: &FrameState,
    raw: &crate::raw::Instruction,
    _code_index: usize,
    expected_return: Option<&VType>,
    env: &TypeEnv<'_>,
) -> Result<FrameState, AnalysisError> {
    match opcode {
        0x59 => state.check_stack_groups(&[1])?,
        0x5a | 0x5f => state.check_stack_groups(&[1, 1])?,
        0x5b => state.check_stack_groups(&[1, 2])?,
        0x5c => state.check_stack_groups(&[2])?,
        0x5d => state.check_stack_groups(&[2, 1])?,
        0x5e => state.check_stack_groups(&[2, 2])?,
        _ => {}
    }
    match opcode {
        0x00 => Ok(state.clone()),
        0x01 => Ok(state.push([VType::Null])),
        0x02..=0x08 | 0x10 | 0x11 => Ok(state.push([VType::Integer])),
        0x09..=0x0A => Ok(state.push([VType::Long])),
        0x0B..=0x0D => Ok(state.push([VType::Float])),
        0x0E..=0x0F => Ok(state.push([VType::Double])),
        0x57 => Ok(state.pop(1)?.0),
        0x58 => Ok(state.pop(2)?.0),
        0x59 => {
            let value = state.peek(0)?.clone();
            Ok(FrameState {
                stack: [state.stack.clone(), vec![value]].concat(),
                locals: state.locals.clone(),
            })
        }
        0x5A => {
            let v1 = state.peek(0)?.clone();
            let v2 = state.peek(1)?.clone();
            Ok(FrameState {
                stack: [
                    state.stack[..state.stack.len() - 2].to_vec(),
                    vec![v1.clone(), v2, v1],
                ]
                .concat(),
                locals: state.locals.clone(),
            })
        }
        0x5B => {
            let v1 = state.peek(0)?.clone();
            let v2 = state.peek(1)?.clone();
            let v3 = state.peek(2)?.clone();
            Ok(FrameState {
                stack: [
                    state.stack[..state.stack.len() - 3].to_vec(),
                    vec![v1.clone(), v3, v2, v1],
                ]
                .concat(),
                locals: state.locals.clone(),
            })
        }
        0x5C => {
            let v1 = state.peek(0)?.clone();
            let v2 = state.peek(1)?.clone();
            Ok(FrameState {
                stack: [state.stack.clone(), vec![v2, v1]].concat(),
                locals: state.locals.clone(),
            })
        }
        0x5D => {
            let v1 = state.peek(0)?.clone();
            let v2 = state.peek(1)?.clone();
            let v3 = state.peek(2)?.clone();
            Ok(FrameState {
                stack: [
                    state.stack[..state.stack.len() - 3].to_vec(),
                    vec![v2.clone(), v1.clone(), v3, v2, v1],
                ]
                .concat(),
                locals: state.locals.clone(),
            })
        }
        0x5E => {
            let v1 = state.peek(0)?.clone();
            let v2 = state.peek(1)?.clone();
            let v3 = state.peek(2)?.clone();
            let v4 = state.peek(3)?.clone();
            Ok(FrameState {
                stack: [
                    state.stack[..state.stack.len() - 4].to_vec(),
                    vec![v2.clone(), v1.clone(), v4, v3, v2, v1],
                ]
                .concat(),
                locals: state.locals.clone(),
            })
        }
        0x5F => {
            let v1 = state.peek(0)?.clone();
            let v2 = state.peek(1)?.clone();
            Ok(FrameState {
                stack: [state.stack[..state.stack.len() - 2].to_vec(), vec![v1, v2]].concat(),
                locals: state.locals.clone(),
            })
        }
        0x74 => Ok(state
            .pop_typed(&VType::Integer, env)?
            .push([VType::Integer])),
        0x75 => Ok(state.pop_typed(&VType::Long, env)?.push([VType::Long])),
        0x76 => Ok(state.pop_typed(&VType::Float, env)?.push([VType::Float])),
        0x77 => Ok(state.pop_typed(&VType::Double, env)?.push([VType::Double])),
        0x79 | 0x7b | 0x7d => Ok(state
            .pop_typed(&VType::Integer, env)?
            .pop_typed(&VType::Long, env)?
            .push([VType::Long])),
        0x60 | 0x64 | 0x68 | 0x6C | 0x70 | 0x78 | 0x7A | 0x7C | 0x7E | 0x80 | 0x82 => Ok(state
            .pop_typed(&VType::Integer, env)?
            .pop_typed(&VType::Integer, env)?
            .push([VType::Integer])),
        0x61 | 0x65 | 0x69 | 0x6D | 0x71 | 0x7F | 0x81 | 0x83 => Ok(state
            .pop_typed(&VType::Long, env)?
            .pop_typed(&VType::Long, env)?
            .push([VType::Long])),
        0x62 | 0x66 | 0x6A | 0x6E | 0x72 => Ok(state
            .pop_typed(&VType::Float, env)?
            .pop_typed(&VType::Float, env)?
            .push([VType::Float])),
        0x63 | 0x67 | 0x6B | 0x6F | 0x73 => Ok(state
            .pop_typed(&VType::Double, env)?
            .pop_typed(&VType::Double, env)?
            .push([VType::Double])),
        0x85 => Ok(state.pop_typed(&VType::Integer, env)?.push([VType::Long])),
        0x86 => Ok(state.pop_typed(&VType::Integer, env)?.push([VType::Float])),
        0x87 => Ok(state.pop_typed(&VType::Integer, env)?.push([VType::Double])),
        0x88 => Ok(state.pop_typed(&VType::Long, env)?.push([VType::Integer])),
        0x89 => Ok(state.pop_typed(&VType::Long, env)?.push([VType::Float])),
        0x8A => Ok(state.pop_typed(&VType::Long, env)?.push([VType::Double])),
        0x8B => Ok(state.pop_typed(&VType::Float, env)?.push([VType::Integer])),
        0x91..=0x93 => Ok(state
            .pop_typed(&VType::Integer, env)?
            .push([VType::Integer])),
        0x8C => Ok(state.pop_typed(&VType::Float, env)?.push([VType::Long])),
        0x8D => Ok(state.pop_typed(&VType::Float, env)?.push([VType::Double])),
        0x8E => Ok(state.pop_typed(&VType::Double, env)?.push([VType::Integer])),
        0x8F => Ok(state.pop_typed(&VType::Double, env)?.push([VType::Long])),
        0x90 => Ok(state.pop_typed(&VType::Double, env)?.push([VType::Float])),
        0x94..=0x98 => {
            let expected = match opcode {
                0x94 => VType::Long,
                0x95 | 0x96 => VType::Float,
                _ => VType::Double,
            };
            Ok(state
                .pop_typed(&expected, env)?
                .pop_typed(&expected, env)?
                .push([VType::Integer]))
        }
        0x2e..=0x35 => {
            let next = state.pop_typed(&VType::Integer, env)?;
            let (next, array) = pop_array(&next)?;
            Ok(next.push([array_element_type(opcode, &array)?]))
        }
        0x4f..=0x56 => {
            let expected = match opcode {
                0x50 => VType::Long,
                0x51 => VType::Float,
                0x52 => VType::Double,
                0x53 => VType::Object(JAVA_LANG_OBJECT.to_owned()),
                _ => VType::Integer,
            };
            let next = state
                .pop_typed(&expected, env)?
                .pop_typed(&VType::Integer, env)?;
            let (next, array) = pop_array(&next)?;
            array_element_type(opcode - 0x21, &array)?;
            Ok(next)
        }
        0xAC => state.pop_typed(&VType::Integer, env),
        0xAD => state.pop_typed(&VType::Long, env),
        0xAE => state.pop_typed(&VType::Float, env),
        0xAF => state.pop_typed(&VType::Double, env),
        0xB0 => state.pop_typed(
            expected_return.unwrap_or(&VType::Object(JAVA_LANG_OBJECT.to_owned())),
            env,
        ),
        0xBF => state.pop_typed(&VType::Object("java/lang/Throwable".to_owned()), env),
        0xC2 | 0xC3 => Ok(state.pop_reference()?.0),
        0xBE => Ok(pop_array(state)?.0.push([VType::Integer])),
        0xB1 => Ok(state.clone()),
        0xBC => {
            let next = state.pop_typed(&VType::Integer, env)?;
            let descriptor = match raw {
                crate::raw::Instruction::NewArray(insn) => newarray_descriptor(insn.atype),
                _ => return Err(AnalysisError::UnsupportedInstruction { opcode }),
            };
            Ok(next.push([VType::Object(descriptor)]))
        }
        opcode => Err(AnalysisError::UnsupportedInstruction { opcode }),
    }
}

fn pop_array(state: &FrameState) -> Result<(FrameState, VType), AnalysisError> {
    let value = state.peek(0)?.clone();
    if !matches!(&value, VType::Null)
        && !matches!(&value, VType::Object(name) if name.starts_with('['))
    {
        return Err(type_error("array instruction requires an array reference"));
    }
    Ok((state.pop(1)?.0, value))
}

fn array_element_type(opcode: u8, array: &VType) -> Result<VType, AnalysisError> {
    let (descriptors, value): (&[&str], _) = match opcode {
        0x2e => (&["[I"], VType::Integer),
        0x2f => (&["[J"], VType::Long),
        0x30 => (&["[F"], VType::Float),
        0x31 => (&["[D"], VType::Double),
        0x32 => (&[], aaload_type(array)),
        0x33 => (&["[B", "[Z"], VType::Integer),
        0x34 => (&["[C"], VType::Integer),
        0x35 => (&["[S"], VType::Integer),
        _ => return Err(AnalysisError::UnsupportedInstruction { opcode }),
    };
    if let VType::Object(name) = array {
        let valid = if opcode == 0x32 {
            name.starts_with("[L") || name.starts_with("[[")
        } else {
            descriptors.contains(&name.as_str())
        };
        if !valid {
            return Err(type_error(format!(
                "array type {name} does not match opcode {opcode:02x}"
            )));
        }
    }
    Ok(value)
}

fn initialize_receiver(
    state: &FrameState,
    receiver: Option<VType>,
    replacement: VType,
) -> FrameState {
    let Some(receiver) = receiver else {
        return state.clone();
    };
    match receiver {
        VType::UninitializedThis | VType::Uninitialized { .. } => FrameState {
            stack: state
                .stack
                .iter()
                .map(|value| {
                    if value == &receiver {
                        replacement.clone()
                    } else {
                        value.clone()
                    }
                })
                .collect(),
            locals: state
                .locals
                .iter()
                .map(|value| {
                    if value == &receiver {
                        replacement.clone()
                    } else {
                        value.clone()
                    }
                })
                .collect(),
        },
        _ => state.clone(),
    }
}

fn branch_targets(item: &CodeItem) -> Vec<crate::model::Label> {
    match item {
        CodeItem::Branch(branch) => vec![branch.target.clone()],
        CodeItem::LookupSwitch(switch) => {
            let mut labels = vec![switch.default_target.clone()];
            labels.extend(switch.pairs.iter().map(|(_, label)| label.clone()));
            labels
        }
        CodeItem::TableSwitch(switch) => {
            let mut labels = vec![switch.default_target.clone()];
            labels.extend(switch.targets.iter().cloned());
            labels
        }
        _ => Vec::new(),
    }
}

fn terminates_block(item: &CodeItem) -> bool {
    matches!(
        item,
        CodeItem::Branch(_) | CodeItem::LookupSwitch(_) | CodeItem::TableSwitch(_)
    ) || is_terminal_item(item)
}

fn is_terminal_item(item: &CodeItem) -> bool {
    matches!(
        item,
        CodeItem::Var(VarInsn { opcode: 0xA9, .. })
            | CodeItem::Raw(crate::raw::Instruction::Simple {
                opcode: 0xAC..=0xB1 | 0xBF,
                ..
            })
    )
}

fn is_unconditional_branch(opcode: u8) -> bool {
    matches!(opcode, 0xA7 | 0xA8 | 0xC8 | 0xC9)
}

fn merge_return_targets(left: &[usize], right: &[usize]) -> Vec<usize> {
    let mut targets = left.to_vec();
    for target in right {
        if !targets.contains(target) {
            targets.push(*target);
        }
    }
    targets.sort_unstable();
    targets.dedup();
    targets
}

fn descriptor_component(descriptor: &FieldDescriptor) -> String {
    match descriptor {
        FieldDescriptor::Base(base) => match base {
            BaseType::Boolean => "Z".to_owned(),
            BaseType::Byte => "B".to_owned(),
            BaseType::Char => "C".to_owned(),
            BaseType::Short => "S".to_owned(),
            BaseType::Int => "I".to_owned(),
            BaseType::Long => "J".to_owned(),
            BaseType::Float => "F".to_owned(),
            BaseType::Double => "D".to_owned(),
        },
        FieldDescriptor::Object(object) => format!("L{};", object.class_name),
        FieldDescriptor::Array(array) => {
            format!("[{}", descriptor_component(array.component_type.as_ref()))
        }
    }
}

fn newarray_descriptor(atype: ArrayType) -> String {
    match atype {
        ArrayType::Boolean => "[Z".to_owned(),
        ArrayType::Char => "[C".to_owned(),
        ArrayType::Float => "[F".to_owned(),
        ArrayType::Double => "[D".to_owned(),
        ArrayType::Byte => "[B".to_owned(),
        ArrayType::Short => "[S".to_owned(),
        ArrayType::Int => "[I".to_owned(),
        ArrayType::Long => "[J".to_owned(),
    }
}

fn anewarray_descriptor(descriptor: &str) -> String {
    if descriptor.starts_with('[') {
        format!("[{descriptor}")
    } else {
        format!("[L{descriptor};")
    }
}

fn aaload_type(array: &VType) -> VType {
    if matches!(array, VType::Null) {
        return VType::Null;
    }
    let VType::Object(class_name) = array else {
        return VType::Object(JAVA_LANG_OBJECT.to_owned());
    };
    if !class_name.starts_with('[') {
        return VType::Object(JAVA_LANG_OBJECT.to_owned());
    }
    let component = &class_name[1..];
    if component.starts_with('L') && component.ends_with(';') {
        return VType::Object(component[1..component.len() - 1].to_owned());
    }
    if component.starts_with('[') {
        return VType::Object(component.to_owned());
    }
    VType::Object(JAVA_LANG_OBJECT.to_owned())
}
