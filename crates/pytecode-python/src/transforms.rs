//! PyO3 wrappers for declarative matcher and transform specifications.
//!
//! These types expose the Rust `*MatcherSpec`, `*TransformSpec`, and pipeline
//! engines to Python so Rust evaluates matchers and transforms natively.

use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyString};
use pytecode_engine::error::{EngineError, EngineErrorKind};
use pytecode_engine::model::{ClassModel, CodeModel, DebugInfoState};
use pytecode_engine::transform::matcher_spec::{
    ClassMatcherSpec, FieldMatcherSpec, InsnMatcherSpec, MethodMatcherSpec,
};
use pytecode_engine::transform::pipeline_spec::{
    CodeTransformAction, CompiledPipeline, FieldTransformAction, MethodTransformAction,
    PipelineSpec, PipelineStep, TransformAction,
};
use pytecode_engine::transform::transform_spec::{
    ClassTransformSpec, CodeTransformSpec, FieldTransformSpec, MethodTransformSpec,
};
use pytecode_engine::transform::{ClassContext, MethodContext};
use std::cell::RefCell;
use std::sync::Arc;

use crate::model::{PyClassModel, PyCodeModel, extract_code_items_iter};

const TRANSFORMS_CALLBACK_ABORT_REASON: &str = "__pytecode_transforms_python_callback_failed__";

thread_local! {
    static CALLBACK_ERROR_STACK: RefCell<Vec<Option<PyErr>>> = const { RefCell::new(Vec::new()) };
}

struct CallbackErrorScope {
    active: bool,
}

impl CallbackErrorScope {
    fn push() -> Self {
        CALLBACK_ERROR_STACK.with(|stack| stack.borrow_mut().push(None));
        Self { active: true }
    }

    fn take_error(&mut self) -> Option<PyErr> {
        self.active = false;
        CALLBACK_ERROR_STACK.with(|stack| stack.borrow_mut().pop().flatten())
    }
}

impl Drop for CallbackErrorScope {
    fn drop(&mut self) {
        if self.active {
            let _ = CALLBACK_ERROR_STACK.with(|stack| stack.borrow_mut().pop());
        }
    }
}

fn callback_abort_error() -> EngineError {
    EngineError::new(
        0,
        EngineErrorKind::InvalidModelState {
            reason: TRANSFORMS_CALLBACK_ABORT_REASON.to_owned(),
        },
    )
}

fn is_callback_abort(error: &EngineError) -> bool {
    matches!(
        error,
        EngineError {
            offset: 0,
            kind: EngineErrorKind::InvalidModelState { reason },
        } if reason == TRANSFORMS_CALLBACK_ABORT_REASON
    )
}

fn record_callback_error(err: PyErr) -> EngineError {
    let message = err.to_string();
    let stored = CALLBACK_ERROR_STACK.with(|stack| {
        if let Some(slot) = stack.borrow_mut().last_mut() {
            if slot.is_none() {
                *slot = Some(err);
            }
            true
        } else {
            false
        }
    });
    if stored {
        callback_abort_error()
    } else {
        EngineError::new(0, EngineErrorKind::InvalidModelState { reason: message })
    }
}

fn run_engine_pipeline_apply(
    apply_fn: impl FnOnce() -> pytecode_engine::Result<()>,
) -> PyResult<()> {
    let mut scope = CallbackErrorScope::push();
    let result = apply_fn();
    let recorded = scope.take_error();
    match (result, recorded) {
        (_, Some(py_err)) => Err(py_err),
        (Ok(()), None) => Ok(()),
        (Err(engine_err), None) => {
            if is_callback_abort(&engine_err) {
                Err(PyValueError::new_err(engine_err.to_string()))
            } else {
                Err(crate::engine_error_to_py(engine_err))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn validate_fullmatch_pattern(pattern: &str) -> PyResult<()> {
    regex::Regex::new(&format!(r"\A(?:{pattern})\z"))
        .map_err(|e| PyValueError::new_err(format!("invalid regex pattern: {e}")))?;
    Ok(())
}

trait MatcherSpecOps: Sized + Clone + std::fmt::Display {
    fn as_and(&self) -> Option<&[Self]>;
    fn make_and(specs: Vec<Self>) -> Self;
    fn as_or(&self) -> Option<&[Self]>;
    fn make_or(specs: Vec<Self>) -> Self;
    fn make_not(spec: Self) -> Self;
}

macro_rules! impl_matcher_spec_ops {
    ($spec_ty:ident) => {
        impl MatcherSpecOps for $spec_ty {
            fn as_and(&self) -> Option<&[Self]> {
                match self {
                    Self::And(specs) => Some(specs),
                    _ => None,
                }
            }

            fn make_and(specs: Vec<Self>) -> Self {
                Self::And(specs)
            }

            fn as_or(&self) -> Option<&[Self]> {
                match self {
                    Self::Or(specs) => Some(specs),
                    _ => None,
                }
            }

            fn make_or(specs: Vec<Self>) -> Self {
                Self::Or(specs)
            }

            fn make_not(spec: Self) -> Self {
                Self::Not(Box::new(spec))
            }
        }
    };
}

impl_matcher_spec_ops!(InsnMatcherSpec);
impl_matcher_spec_ops!(ClassMatcherSpec);
impl_matcher_spec_ops!(FieldMatcherSpec);
impl_matcher_spec_ops!(MethodMatcherSpec);

fn spec_and<T: MatcherSpecOps>(left: &T, right: &T) -> T {
    match (left.as_and(), right.as_and()) {
        (Some(l), Some(r)) => {
            let mut specs = l.to_vec();
            specs.extend(r.iter().cloned());
            T::make_and(specs)
        }
        (Some(l), None) => {
            let mut specs = l.to_vec();
            specs.push(right.clone());
            T::make_and(specs)
        }
        (None, Some(r)) => {
            let mut specs = Vec::with_capacity(1 + r.len());
            specs.push(left.clone());
            specs.extend(r.iter().cloned());
            T::make_and(specs)
        }
        (None, None) => T::make_and(vec![left.clone(), right.clone()]),
    }
}

fn spec_or<T: MatcherSpecOps>(left: &T, right: &T) -> T {
    match (left.as_or(), right.as_or()) {
        (Some(l), Some(r)) => {
            let mut specs = l.to_vec();
            specs.extend(r.iter().cloned());
            T::make_or(specs)
        }
        (Some(l), None) => {
            let mut specs = l.to_vec();
            specs.push(right.clone());
            T::make_or(specs)
        }
        (None, Some(r)) => {
            let mut specs = Vec::with_capacity(1 + r.len());
            specs.push(left.clone());
            specs.extend(r.iter().cloned());
            T::make_or(specs)
        }
        (None, None) => T::make_or(vec![left.clone(), right.clone()]),
    }
}

// ---------------------------------------------------------------------------
// PyInsnMatcher
// ---------------------------------------------------------------------------

/// A declarative instruction matcher that Rust evaluates without FFI per-match.
#[pyclass(from_py_object, name = "InsnMatcher", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyInsnMatcher {
    pub(crate) spec: InsnMatcherSpec,
}

#[pymethods]
impl PyInsnMatcher {
    /// Match instructions with the exact JVM opcode byte.
    #[staticmethod]
    fn opcode(opcode: u8) -> Self {
        Self {
            spec: InsnMatcherSpec::Opcode(opcode),
        }
    }

    /// Match instructions whose opcode byte belongs to ``opcodes``.
    #[staticmethod]
    fn opcode_any(opcodes: Vec<u8>) -> Self {
        Self {
            spec: InsnMatcherSpec::OpcodeAny(opcodes),
        }
    }

    /// Match any field access instruction.
    #[staticmethod]
    fn is_field_access() -> Self {
        Self {
            spec: InsnMatcherSpec::IsFieldAccess,
        }
    }

    /// Match any method invocation instruction.
    #[staticmethod]
    fn is_method_call() -> Self {
        Self {
            spec: InsnMatcherSpec::IsMethodCall,
        }
    }

    /// Match any return opcode (``return``, ``ireturn``, ..., ``areturn``).
    #[staticmethod]
    fn is_return() -> Self {
        Self {
            spec: InsnMatcherSpec::IsReturn,
        }
    }

    /// Match any branch or switch instruction.
    #[staticmethod]
    fn is_branch() -> Self {
        Self {
            spec: InsnMatcherSpec::IsBranch,
        }
    }

    /// Match symbolic ``Label`` items.
    #[staticmethod]
    fn is_label() -> Self {
        Self {
            spec: InsnMatcherSpec::IsLabel,
        }
    }

    /// Match any constant-loading ``LdcInsn``.
    #[staticmethod]
    fn is_ldc() -> Self {
        Self {
            spec: InsnMatcherSpec::IsLdc,
        }
    }

    /// Match any local-variable instruction (``VarInsn``).
    #[staticmethod]
    fn is_var() -> Self {
        Self {
            spec: InsnMatcherSpec::IsVar,
        }
    }

    /// Match field instructions whose target owner equals ``owner``.
    #[staticmethod]
    fn field_owner(owner: String) -> Self {
        Self {
            spec: InsnMatcherSpec::FieldOwner(owner),
        }
    }

    /// Match field instructions whose target field name equals ``name``.
    #[staticmethod]
    fn field_named(name: String) -> Self {
        Self {
            spec: InsnMatcherSpec::FieldNamed(name),
        }
    }

    /// Match field instructions whose target field descriptor equals ``descriptor``.
    #[staticmethod]
    fn field_descriptor(descriptor: String) -> Self {
        Self {
            spec: InsnMatcherSpec::FieldDescriptor(descriptor),
        }
    }

    /// Match field instructions matching exact ``owner``, ``name``, and ``descriptor``.
    #[staticmethod]
    fn field(owner: String, name: String, descriptor: String) -> Self {
        Self {
            spec: InsnMatcherSpec::Field {
                owner,
                name,
                descriptor,
            },
        }
    }

    /// Match method invocations whose target owner equals ``owner``.
    #[staticmethod]
    fn method_owner(owner: String) -> Self {
        Self {
            spec: InsnMatcherSpec::MethodOwner(owner),
        }
    }

    /// Match method invocations whose target method name equals ``name``.
    #[staticmethod]
    fn method_named(name: String) -> Self {
        Self {
            spec: InsnMatcherSpec::MethodNamed(name),
        }
    }

    /// Match method invocations whose target descriptor equals ``descriptor``.
    #[staticmethod]
    fn method_descriptor(descriptor: String) -> Self {
        Self {
            spec: InsnMatcherSpec::MethodDescriptor(descriptor),
        }
    }

    /// Match method invocations with exact ``owner``, ``name``, and ``descriptor``.
    #[staticmethod]
    fn method(owner: String, name: String, descriptor: String) -> Self {
        Self {
            spec: InsnMatcherSpec::Method {
                owner,
                name,
                descriptor,
            },
        }
    }

    /// Match type instructions (``new``, ``anewarray``, ``checkcast``, ``instanceof``) by descriptor.
    #[staticmethod]
    fn type_descriptor(descriptor: String) -> Self {
        Self {
            spec: InsnMatcherSpec::TypeDescriptor(descriptor),
        }
    }

    /// Match string-literal ``ldc`` instructions (preserving lone surrogates).
    #[staticmethod]
    fn ldc_string(value: &Bound<'_, PyString>) -> PyResult<Self> {
        Ok(Self {
            spec: InsnMatcherSpec::LdcString(crate::java_string::from_python(value)?),
        })
    }

    /// Match class-literal ``ldc`` instructions by internal class name or array descriptor.
    #[staticmethod]
    fn ldc_class(value: String) -> Self {
        Self {
            spec: InsnMatcherSpec::LdcClass(value),
        }
    }

    /// Match local-variable instructions (including ``iinc``) targeting ``slot``.
    #[staticmethod]
    fn var_slot(slot: u16) -> Self {
        Self {
            spec: InsnMatcherSpec::VarSlot(slot),
        }
    }

    /// Match field instructions whose owner fully matches regex ``pattern``.
    #[staticmethod]
    fn field_owner_matches(pattern: String) -> PyResult<Self> {
        validate_fullmatch_pattern(&pattern)?;
        Ok(Self {
            spec: InsnMatcherSpec::FieldOwnerMatches(pattern),
        })
    }

    /// Match method invocations whose owner fully matches regex ``pattern``.
    #[staticmethod]
    fn method_owner_matches(pattern: String) -> PyResult<Self> {
        validate_fullmatch_pattern(&pattern)?;
        Ok(Self {
            spec: InsnMatcherSpec::MethodOwnerMatches(pattern),
        })
    }

    /// Match method invocations whose method name fully matches regex ``pattern``.
    #[staticmethod]
    fn method_name_matches(pattern: String) -> PyResult<Self> {
        validate_fullmatch_pattern(&pattern)?;
        Ok(Self {
            spec: InsnMatcherSpec::MethodNameMatches(pattern),
        })
    }

    /// Always matches any code item.
    #[staticmethod]
    fn any() -> Self {
        Self {
            spec: InsnMatcherSpec::Any,
        }
    }

    fn __and__(&self, other: &PyInsnMatcher) -> Self {
        Self {
            spec: spec_and(&self.spec, &other.spec),
        }
    }

    fn __or__(&self, other: &PyInsnMatcher) -> Self {
        Self {
            spec: spec_or(&self.spec, &other.spec),
        }
    }

    fn __invert__(&self) -> Self {
        Self {
            spec: InsnMatcherSpec::make_not(self.spec.clone()),
        }
    }

    fn __repr__(&self) -> String {
        format!("InsnMatcher({})", self.spec)
    }

    fn __str__(&self) -> String {
        self.spec.to_string()
    }
}

// ---------------------------------------------------------------------------
// PyClassMatcher
// ---------------------------------------------------------------------------

/// A declarative class matcher that Rust evaluates without FFI per-match.
#[pyclass(from_py_object, name = "ClassMatcher", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyClassMatcher {
    pub(crate) spec: ClassMatcherSpec,
}

#[pymethods]
impl PyClassMatcher {
    /// Match by exact class name (internal JVM format, e.g. ``"java/lang/String"``).
    #[staticmethod]
    fn named(name: String) -> Self {
        Self {
            spec: ClassMatcherSpec::Named(name),
        }
    }

    /// Match by full-match regex pattern against class name.
    #[staticmethod]
    fn name_matches(pattern: String) -> PyResult<Self> {
        validate_fullmatch_pattern(&pattern)?;
        Ok(Self {
            spec: ClassMatcherSpec::NameMatches(pattern),
        })
    }

    /// Match when all given access flag bits are set.
    #[staticmethod]
    fn access_all(flags: u16) -> Self {
        Self {
            spec: ClassMatcherSpec::AccessAll(flags),
        }
    }

    /// Match when any of the given access flag bits are set.
    #[staticmethod]
    fn access_any(flags: u16) -> Self {
        Self {
            spec: ClassMatcherSpec::AccessAny(flags),
        }
    }

    /// Match package-private classes (no PUBLIC/PROTECTED/PRIVATE/MODULE flag).
    #[staticmethod]
    fn is_package_private() -> Self {
        Self {
            spec: ClassMatcherSpec::IsPackagePrivate,
        }
    }

    /// Match by exact super-class name.
    #[staticmethod]
    fn extends(name: String) -> Self {
        Self {
            spec: ClassMatcherSpec::Extends(name),
        }
    }

    /// Match when the class lists the named interface.
    #[staticmethod]
    fn implements(name: String) -> Self {
        Self {
            spec: ClassMatcherSpec::Implements(name),
        }
    }

    /// Match by exact major version.
    #[staticmethod]
    fn version(major: u16) -> Self {
        Self {
            spec: ClassMatcherSpec::Version(major),
        }
    }

    /// Match when major version >= value.
    #[staticmethod]
    fn version_at_least(major: u16) -> Self {
        Self {
            spec: ClassMatcherSpec::VersionAtLeast(major),
        }
    }

    /// Match when major version < value.
    #[staticmethod]
    fn version_below(major: u16) -> Self {
        Self {
            spec: ClassMatcherSpec::VersionBelow(major),
        }
    }

    /// Always matches any class.
    #[staticmethod]
    fn any() -> Self {
        Self {
            spec: ClassMatcherSpec::Any,
        }
    }

    fn __and__(&self, other: &PyClassMatcher) -> Self {
        Self {
            spec: spec_and(&self.spec, &other.spec),
        }
    }

    fn __or__(&self, other: &PyClassMatcher) -> Self {
        Self {
            spec: spec_or(&self.spec, &other.spec),
        }
    }

    fn __invert__(&self) -> Self {
        Self {
            spec: ClassMatcherSpec::make_not(self.spec.clone()),
        }
    }

    fn __repr__(&self) -> String {
        format!("ClassMatcher({})", self.spec)
    }

    fn __str__(&self) -> String {
        self.spec.to_string()
    }
}

// ---------------------------------------------------------------------------
// PyFieldMatcher
// ---------------------------------------------------------------------------

/// A declarative field matcher that Rust evaluates without FFI per-match.
#[pyclass(from_py_object, name = "FieldMatcher", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyFieldMatcher {
    pub(crate) spec: FieldMatcherSpec,
}

#[pymethods]
impl PyFieldMatcher {
    /// Match by exact field name.
    #[staticmethod]
    fn named(name: String) -> Self {
        Self {
            spec: FieldMatcherSpec::Named(name),
        }
    }

    /// Match by full-match regex pattern against field name.
    #[staticmethod]
    fn name_matches(pattern: String) -> PyResult<Self> {
        validate_fullmatch_pattern(&pattern)?;
        Ok(Self {
            spec: FieldMatcherSpec::NameMatches(pattern),
        })
    }

    /// Match by exact field descriptor.
    #[staticmethod]
    fn descriptor(desc: String) -> Self {
        Self {
            spec: FieldMatcherSpec::Descriptor(desc),
        }
    }

    /// Match by full-match regex pattern against field descriptor.
    #[staticmethod]
    fn descriptor_matches(pattern: String) -> PyResult<Self> {
        validate_fullmatch_pattern(&pattern)?;
        Ok(Self {
            spec: FieldMatcherSpec::DescriptorMatches(pattern),
        })
    }

    /// Match when all given field access flag bits are set.
    #[staticmethod]
    fn access_all(flags: u16) -> Self {
        Self {
            spec: FieldMatcherSpec::AccessAll(flags),
        }
    }

    /// Match when any of the given field access flag bits are set.
    #[staticmethod]
    fn access_any(flags: u16) -> Self {
        Self {
            spec: FieldMatcherSpec::AccessAny(flags),
        }
    }

    /// Match package-private fields (no PUBLIC/PROTECTED/PRIVATE flag).
    #[staticmethod]
    fn is_package_private() -> Self {
        Self {
            spec: FieldMatcherSpec::IsPackagePrivate,
        }
    }

    /// Always matches any field.
    #[staticmethod]
    fn any() -> Self {
        Self {
            spec: FieldMatcherSpec::Any,
        }
    }

    fn __and__(&self, other: &PyFieldMatcher) -> Self {
        Self {
            spec: spec_and(&self.spec, &other.spec),
        }
    }

    fn __or__(&self, other: &PyFieldMatcher) -> Self {
        Self {
            spec: spec_or(&self.spec, &other.spec),
        }
    }

    fn __invert__(&self) -> Self {
        Self {
            spec: FieldMatcherSpec::make_not(self.spec.clone()),
        }
    }

    fn __repr__(&self) -> String {
        format!("FieldMatcher({})", self.spec)
    }

    fn __str__(&self) -> String {
        self.spec.to_string()
    }
}

// ---------------------------------------------------------------------------
// PyMethodMatcher
// ---------------------------------------------------------------------------

/// A declarative method matcher that Rust evaluates without FFI per-match.
#[pyclass(from_py_object, name = "MethodMatcher", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyMethodMatcher {
    pub(crate) spec: MethodMatcherSpec,
}

#[pymethods]
impl PyMethodMatcher {
    /// Match by exact method name.
    #[staticmethod]
    fn named(name: String) -> Self {
        Self {
            spec: MethodMatcherSpec::Named(name),
        }
    }

    /// Match by full-match regex pattern against method name.
    #[staticmethod]
    fn name_matches(pattern: String) -> PyResult<Self> {
        validate_fullmatch_pattern(&pattern)?;
        Ok(Self {
            spec: MethodMatcherSpec::NameMatches(pattern),
        })
    }

    /// Match by exact method descriptor.
    #[staticmethod]
    fn descriptor(desc: String) -> Self {
        Self {
            spec: MethodMatcherSpec::Descriptor(desc),
        }
    }

    /// Match by full-match regex pattern against method descriptor.
    #[staticmethod]
    fn descriptor_matches(pattern: String) -> PyResult<Self> {
        validate_fullmatch_pattern(&pattern)?;
        Ok(Self {
            spec: MethodMatcherSpec::DescriptorMatches(pattern),
        })
    }

    /// Match when all given method access flag bits are set.
    #[staticmethod]
    fn access_all(flags: u16) -> Self {
        Self {
            spec: MethodMatcherSpec::AccessAll(flags),
        }
    }

    /// Match when any of the given method access flag bits are set.
    #[staticmethod]
    fn access_any(flags: u16) -> Self {
        Self {
            spec: MethodMatcherSpec::AccessAny(flags),
        }
    }

    /// Match package-private methods (no PUBLIC/PROTECTED/PRIVATE flag).
    #[staticmethod]
    fn is_package_private() -> Self {
        Self {
            spec: MethodMatcherSpec::IsPackagePrivate,
        }
    }

    /// Match methods that have a code body.
    #[staticmethod]
    fn has_code() -> Self {
        Self {
            spec: MethodMatcherSpec::HasCode,
        }
    }

    /// Match instance initializers (``<init>``).
    #[staticmethod]
    fn is_constructor() -> Self {
        Self {
            spec: MethodMatcherSpec::IsConstructor,
        }
    }

    /// Match class/interface static initializers (``<clinit>``).
    #[staticmethod]
    fn is_static_initializer() -> Self {
        Self {
            spec: MethodMatcherSpec::IsStaticInitializer,
        }
    }

    /// Match methods whose return descriptor equals ``descriptor``.
    #[staticmethod]
    fn returns(descriptor: String) -> Self {
        Self {
            spec: MethodMatcherSpec::Returns(descriptor),
        }
    }

    /// Always matches any method.
    #[staticmethod]
    fn any() -> Self {
        Self {
            spec: MethodMatcherSpec::Any,
        }
    }

    fn __and__(&self, other: &PyMethodMatcher) -> Self {
        Self {
            spec: spec_and(&self.spec, &other.spec),
        }
    }

    fn __or__(&self, other: &PyMethodMatcher) -> Self {
        Self {
            spec: spec_or(&self.spec, &other.spec),
        }
    }

    fn __invert__(&self) -> Self {
        Self {
            spec: MethodMatcherSpec::make_not(self.spec.clone()),
        }
    }

    fn __repr__(&self) -> String {
        format!("MethodMatcher({})", self.spec)
    }

    fn __str__(&self) -> String {
        self.spec.to_string()
    }
}

// ---------------------------------------------------------------------------
// PyCodeTransform — wraps CodeTransformSpec
// ---------------------------------------------------------------------------

/// A declarative code transform that Rust applies natively.
#[pyclass(from_py_object, name = "CodeTransform", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyCodeTransform {
    pub(crate) spec: CodeTransformSpec,
}

#[pymethods]
impl PyCodeTransform {
    /// Replace every instruction matching ``matcher`` with ``replacement``.
    #[staticmethod]
    fn replace_insn(matcher: &PyInsnMatcher, replacement: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            spec: CodeTransformSpec::ReplaceInsn {
                matcher: matcher.spec.clone(),
                replacement: extract_code_items_iter(replacement)?,
            },
        })
    }

    /// Replace every non-overlapping instruction sequence matching ``pattern`` with ``replacement``.
    #[staticmethod]
    fn replace_sequence(
        pattern: &Bound<'_, PyAny>,
        replacement: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        let pattern = pattern
            .try_iter()?
            .map(|entry| Ok(entry?.extract::<PyRef<'_, PyInsnMatcher>>()?.spec.clone()))
            .collect::<PyResult<Vec<_>>>()?;
        Ok(Self {
            spec: CodeTransformSpec::ReplaceSequence {
                pattern,
                replacement: extract_code_items_iter(replacement)?,
            },
        })
    }

    /// Remove every instruction matching ``matcher``.
    #[staticmethod]
    fn remove_insn(matcher: &PyInsnMatcher) -> Self {
        Self {
            spec: CodeTransformSpec::RemoveInsn {
                matcher: matcher.spec.clone(),
            },
        }
    }

    /// Remove every non-overlapping instruction sequence matching ``pattern``.
    #[staticmethod]
    fn remove_sequence(pattern: &Bound<'_, PyAny>) -> PyResult<Self> {
        let pattern = pattern
            .try_iter()?
            .map(|entry| Ok(entry?.extract::<PyRef<'_, PyInsnMatcher>>()?.spec.clone()))
            .collect::<PyResult<Vec<_>>>()?;
        Ok(Self {
            spec: CodeTransformSpec::RemoveSequence { pattern },
        })
    }

    /// Insert ``items`` immediately before every instruction matching ``matcher``.
    #[staticmethod]
    fn insert_before(matcher: &PyInsnMatcher, items: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            spec: CodeTransformSpec::InsertBefore {
                matcher: matcher.spec.clone(),
                items: extract_code_items_iter(items)?,
            },
        })
    }

    /// Insert ``items`` immediately after every instruction matching ``matcher``.
    #[staticmethod]
    fn insert_after(matcher: &PyInsnMatcher, items: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            spec: CodeTransformSpec::InsertAfter {
                matcher: matcher.spec.clone(),
                items: extract_code_items_iter(items)?,
            },
        })
    }

    /// Redirect method invocations matching ``from_*`` to ``to_*``.
    #[staticmethod]
    #[pyo3(signature = (
        from_owner,
        from_name,
        from_descriptor,
        to_owner,
        to_name,
        to_descriptor=None,
        to_is_interface=None,
    ))]
    fn redirect_method_call(
        from_owner: String,
        from_name: String,
        from_descriptor: String,
        to_owner: String,
        to_name: String,
        to_descriptor: Option<String>,
        to_is_interface: Option<bool>,
    ) -> Self {
        Self {
            spec: CodeTransformSpec::RedirectMethodCall {
                from_owner,
                from_name,
                from_descriptor,
                to_owner,
                to_name,
                to_descriptor,
                to_is_interface,
            },
        }
    }

    /// Redirect field accesses matching ``from_*`` (and optional ``from_descriptor``) to ``to_*``.
    #[staticmethod]
    #[pyo3(signature = (
        from_owner,
        from_name,
        to_owner,
        to_name,
        from_descriptor=None,
        to_descriptor=None,
    ))]
    fn redirect_field_access(
        from_owner: String,
        from_name: String,
        to_owner: String,
        to_name: String,
        from_descriptor: Option<String>,
        to_descriptor: Option<String>,
    ) -> Self {
        Self {
            spec: CodeTransformSpec::RedirectFieldAccess {
                from_owner,
                from_name,
                from_descriptor,
                to_owner,
                to_name,
                to_descriptor,
            },
        }
    }

    /// Replace matching ``ldc`` string literals (preserving lone surrogates).
    #[staticmethod]
    fn replace_string(
        from_value: &Bound<'_, PyString>,
        to_value: &Bound<'_, PyString>,
    ) -> PyResult<Self> {
        Ok(Self {
            spec: CodeTransformSpec::ReplaceString {
                from: crate::java_string::from_python(from_value)?,
                to: crate::java_string::from_python(to_value)?,
            },
        })
    }

    /// Compose multiple code transforms in sequence.
    #[staticmethod]
    fn sequence(transforms: &Bound<'_, PyAny>) -> PyResult<Self> {
        let specs = transforms
            .try_iter()?
            .map(|entry| Ok(entry?.extract::<PyRef<'_, PyCodeTransform>>()?.spec.clone()))
            .collect::<PyResult<Vec<_>>>()?;
        Ok(Self {
            spec: CodeTransformSpec::Sequence(specs),
        })
    }

    /// Apply this code transform directly to a ``CodeModel`` in place.
    fn apply(&self, code: &mut PyCodeModel) -> PyResult<()> {
        let compiled = self
            .spec
            .try_compile()
            .map_err(|e| PyValueError::new_err(format!("invalid regex pattern: {e}")))?;
        code.with_code_mut(|inner| {
            compiled.apply(inner);
            Ok(())
        })
    }

    fn __repr__(&self) -> String {
        format!("CodeTransform({})", self.spec)
    }

    fn __str__(&self) -> String {
        self.spec.to_string()
    }
}

// ---------------------------------------------------------------------------
// PyFieldTransform — wraps FieldTransformSpec
// ---------------------------------------------------------------------------

/// A declarative field-level transform that Rust applies per matched field.
#[pyclass(from_py_object, name = "FieldTransform", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyFieldTransform {
    pub(crate) spec: FieldTransformSpec,
}

#[pymethods]
impl PyFieldTransform {
    /// Rename each matched field to ``name``.
    #[staticmethod]
    fn rename(name: String) -> Self {
        Self {
            spec: FieldTransformSpec::Rename(name),
        }
    }

    /// Replace each matched field's descriptor with ``descriptor``.
    #[staticmethod]
    fn set_descriptor(descriptor: String) -> Self {
        Self {
            spec: FieldTransformSpec::SetDescriptor(descriptor),
        }
    }

    /// Replace each matched field's access flags with ``flags``.
    #[staticmethod]
    fn set_access_flags(flags: u16) -> Self {
        Self {
            spec: FieldTransformSpec::SetAccessFlags(flags),
        }
    }

    /// Add access flag bits ``flags`` on each matched field.
    #[staticmethod]
    fn add_access_flags(flags: u16) -> Self {
        Self {
            spec: FieldTransformSpec::AddAccessFlags(flags),
        }
    }

    /// Clear access flag bits ``flags`` on each matched field.
    #[staticmethod]
    fn remove_access_flags(flags: u16) -> Self {
        Self {
            spec: FieldTransformSpec::RemoveAccessFlags(flags),
        }
    }

    /// Remove each matched field from the class.
    #[staticmethod]
    fn remove() -> Self {
        Self {
            spec: FieldTransformSpec::Remove,
        }
    }

    /// Compose multiple field transforms in order.
    #[staticmethod]
    fn sequence(transforms: &Bound<'_, PyAny>) -> PyResult<Self> {
        let specs = transforms
            .try_iter()?
            .map(|entry| {
                Ok(entry?
                    .extract::<PyRef<'_, PyFieldTransform>>()?
                    .spec
                    .clone())
            })
            .collect::<PyResult<Vec<_>>>()?;
        Ok(Self {
            spec: FieldTransformSpec::Sequence(specs),
        })
    }

    fn __repr__(&self) -> String {
        format!("FieldTransform({})", self.spec)
    }

    fn __str__(&self) -> String {
        self.spec.to_string()
    }
}

// ---------------------------------------------------------------------------
// PyMethodTransform — wraps MethodTransformSpec
// ---------------------------------------------------------------------------

/// A declarative method-level transform that Rust applies per matched method.
#[pyclass(from_py_object, name = "MethodTransform", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyMethodTransform {
    pub(crate) spec: MethodTransformSpec,
}

#[pymethods]
impl PyMethodTransform {
    /// Rename each matched method to ``name``.
    #[staticmethod]
    fn rename(name: String) -> Self {
        Self {
            spec: MethodTransformSpec::Rename(name),
        }
    }

    /// Replace each matched method's descriptor with ``descriptor``.
    #[staticmethod]
    fn set_descriptor(descriptor: String) -> Self {
        Self {
            spec: MethodTransformSpec::SetDescriptor(descriptor),
        }
    }

    /// Replace each matched method's access flags with ``flags``.
    #[staticmethod]
    fn set_access_flags(flags: u16) -> Self {
        Self {
            spec: MethodTransformSpec::SetAccessFlags(flags),
        }
    }

    /// Add access flag bits ``flags`` on each matched method.
    #[staticmethod]
    fn add_access_flags(flags: u16) -> Self {
        Self {
            spec: MethodTransformSpec::AddAccessFlags(flags),
        }
    }

    /// Clear access flag bits ``flags`` on each matched method.
    #[staticmethod]
    fn remove_access_flags(flags: u16) -> Self {
        Self {
            spec: MethodTransformSpec::RemoveAccessFlags(flags),
        }
    }

    /// Remove each matched method from the class.
    #[staticmethod]
    fn remove() -> Self {
        Self {
            spec: MethodTransformSpec::Remove,
        }
    }

    /// Apply a ``CodeTransform`` to each matched method that has code.
    #[staticmethod]
    fn code_transform(transform: &PyCodeTransform) -> Self {
        Self {
            spec: MethodTransformSpec::CodeTransform(transform.spec.clone()),
        }
    }

    /// Compose multiple method transforms in order.
    #[staticmethod]
    fn sequence(transforms: &Bound<'_, PyAny>) -> PyResult<Self> {
        let specs = transforms
            .try_iter()?
            .map(|entry| {
                Ok(entry?
                    .extract::<PyRef<'_, PyMethodTransform>>()?
                    .spec
                    .clone())
            })
            .collect::<PyResult<Vec<_>>>()?;
        Ok(Self {
            spec: MethodTransformSpec::Sequence(specs),
        })
    }

    fn __repr__(&self) -> String {
        format!("MethodTransform({})", self.spec)
    }

    fn __str__(&self) -> String {
        self.spec.to_string()
    }
}

// ---------------------------------------------------------------------------
// PyClassTransform — wraps ClassTransformSpec
// ---------------------------------------------------------------------------

/// A declarative class transform that Rust applies natively.
#[pyclass(from_py_object, name = "ClassTransform", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyClassTransform {
    pub(crate) spec: ClassTransformSpec,
}

#[pymethods]
impl PyClassTransform {
    /// Rename the class to ``name`` in internal JVM form.
    #[staticmethod]
    fn rename_class(name: String) -> Self {
        Self {
            spec: ClassTransformSpec::RenameClass(name),
        }
    }

    /// Replace the class access flags with ``flags``.
    #[staticmethod]
    fn set_access_flags(flags: u16) -> Self {
        Self {
            spec: ClassTransformSpec::SetAccessFlags(flags),
        }
    }

    /// Set access flag bits ``flags`` on the class.
    #[staticmethod]
    fn add_access_flags(flags: u16) -> Self {
        Self {
            spec: ClassTransformSpec::AddAccessFlags(flags),
        }
    }

    /// Clear access flag bits ``flags`` from the class.
    #[staticmethod]
    fn remove_access_flags(flags: u16) -> Self {
        Self {
            spec: ClassTransformSpec::RemoveAccessFlags(flags),
        }
    }

    /// Set the direct superclass name to ``name``.
    #[staticmethod]
    fn set_super_class(name: String) -> Self {
        Self {
            spec: ClassTransformSpec::SetSuperClass(name),
        }
    }

    /// Append interface ``name`` if not already present.
    #[staticmethod]
    fn add_interface(name: String) -> Self {
        Self {
            spec: ClassTransformSpec::AddInterface(name),
        }
    }

    /// Remove interface ``name`` from declared interfaces.
    #[staticmethod]
    fn remove_interface(name: String) -> Self {
        Self {
            spec: ClassTransformSpec::RemoveInterface(name),
        }
    }

    /// Remove methods named ``name`` (optionally matching ``descriptor``).
    #[staticmethod]
    #[pyo3(signature = (name, descriptor=None))]
    fn remove_method(name: String, descriptor: Option<String>) -> Self {
        Self {
            spec: ClassTransformSpec::RemoveMethod { name, descriptor },
        }
    }

    /// Remove fields named ``name`` (optionally matching ``descriptor``).
    #[staticmethod]
    #[pyo3(signature = (name, descriptor=None))]
    fn remove_field(name: String, descriptor: Option<String>) -> Self {
        Self {
            spec: ClassTransformSpec::RemoveField { name, descriptor },
        }
    }

    /// Rename every method named ``from_name`` to ``to_name``.
    #[staticmethod]
    #[pyo3(signature = (from_name, to_name))]
    fn rename_method(from_name: String, to_name: String) -> Self {
        Self {
            spec: ClassTransformSpec::RenameMethod {
                from: from_name,
                to: to_name,
            },
        }
    }

    /// Rename every field named ``from_name`` to ``to_name``.
    #[staticmethod]
    #[pyo3(signature = (from_name, to_name))]
    fn rename_field(from_name: String, to_name: String) -> Self {
        Self {
            spec: ClassTransformSpec::RenameField {
                from: from_name,
                to: to_name,
            },
        }
    }

    /// Replace access flags on all methods named ``name``.
    #[staticmethod]
    fn set_method_access_flags(name: String, flags: u16) -> Self {
        Self {
            spec: ClassTransformSpec::SetMethodAccessFlags { name, flags },
        }
    }

    /// Replace access flags on all fields named ``name``.
    #[staticmethod]
    fn set_field_access_flags(name: String, flags: u16) -> Self {
        Self {
            spec: ClassTransformSpec::SetFieldAccessFlags { name, flags },
        }
    }

    /// Apply ``transform`` to method bodies matching optional ``method_name`` / ``method_descriptor``.
    #[staticmethod]
    #[pyo3(signature = (transform, method_name=None, method_descriptor=None))]
    fn code_transform(
        transform: &PyCodeTransform,
        method_name: Option<String>,
        method_descriptor: Option<String>,
    ) -> Self {
        Self {
            spec: ClassTransformSpec::CodeTransform {
                method_name,
                method_descriptor,
                transform: transform.spec.clone(),
            },
        }
    }

    /// Compose multiple class transforms in sequence.
    #[staticmethod]
    fn sequence(transforms: &Bound<'_, PyAny>) -> PyResult<Self> {
        let specs = transforms
            .try_iter()?
            .map(|entry| {
                Ok(entry?
                    .extract::<PyRef<'_, PyClassTransform>>()?
                    .spec
                    .clone())
            })
            .collect::<PyResult<Vec<_>>>()?;
        Ok(Self {
            spec: ClassTransformSpec::Sequence(specs),
        })
    }

    /// Apply this class transform directly to a ``ClassModel`` in place.
    fn apply(&self, model: &mut PyClassModel) -> PyResult<()> {
        let compiled = self
            .spec
            .try_compile()
            .map_err(|e| PyValueError::new_err(format!("invalid regex pattern: {e}")))?;
        model.with_class_model_mut(|inner| {
            compiled.apply(inner);
            Ok(())
        })
    }

    fn __repr__(&self) -> String {
        format!("ClassTransform({})", self.spec)
    }

    fn __str__(&self) -> String {
        self.spec.to_string()
    }
}

// ---------------------------------------------------------------------------
// Custom callback factories
// ---------------------------------------------------------------------------

type ClassCallback = Arc<dyn Fn(&mut ClassModel) -> pytecode_engine::Result<()> + Send + Sync>;
type CodeCallback = Arc<
    dyn Fn(&mut CodeModel, &MethodContext, &ClassContext) -> pytecode_engine::Result<()>
        + Send
        + Sync,
>;

fn make_class_callback(callback: &Bound<'_, PyAny>) -> PyResult<ClassCallback> {
    if !callback.is_callable() {
        return Err(PyTypeError::new_err("callback must be callable"));
    }
    let cb = Arc::new(callback.clone().unbind());
    Ok(Arc::new(move |model: &mut ClassModel| {
        Python::attach(|py| {
            let mut py_model = PyClassModel::from_model(std::mem::take(model));
            let call_res: PyResult<()> = (|| {
                let cell = Py::new(py, py_model.clone())?;
                cb.call1(py, (&cell,))?;
                Ok(())
            })();
            let restore_res = py_model.take_inner();
            if let Ok(inner) = restore_res {
                *model = inner;
            } else if call_res.is_ok()
                && let Err(err) = restore_res
            {
                return Err(record_callback_error(err));
            }
            if let Err(err) = call_res {
                return Err(record_callback_error(err));
            }
            Ok(())
        })
    }))
}

fn make_code_callback(callback: &Bound<'_, PyAny>) -> PyResult<CodeCallback> {
    if !callback.is_callable() {
        return Err(PyTypeError::new_err("callback must be callable"));
    }
    let cb = Arc::new(callback.clone().unbind());
    Ok(Arc::new(
        move |code: &mut CodeModel, _method_ctx: &MethodContext, _owner_ctx: &ClassContext| {
            Python::attach(|py| {
                let owned = std::mem::replace(code, CodeModel::new(0, 0, DebugInfoState::Fresh));
                let mut py_code = PyCodeModel::from_owned(owned);
                let call_res: PyResult<()> = (|| {
                    let cell = Py::new(py, py_code.clone())?;
                    cb.call1(py, (&cell,))?;
                    Ok(())
                })();
                let restore_res = py_code.take_owned();
                if let Ok(inner) = restore_res {
                    *code = inner;
                } else if call_res.is_ok()
                    && let Err(err) = restore_res
                {
                    return Err(record_callback_error(err));
                }
                if let Err(err) = call_res {
                    return Err(record_callback_error(err));
                }
                Ok(())
            })
        },
    ))
}

// ---------------------------------------------------------------------------
// PyPipeline — wraps PipelineSpec
// ---------------------------------------------------------------------------

/// A declarative transform pipeline that Rust evaluates natively.
#[pyclass(from_py_object, name = "Pipeline", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyPipeline {
    pub(crate) spec: PipelineSpec,
}

impl PyPipeline {
    pub(crate) fn contains_python_callbacks(&self) -> bool {
        self.spec.steps.iter().any(|step| match step {
            PipelineStep::Class { action, .. } => matches!(action, TransformAction::Custom(_)),
            PipelineStep::Field { action, .. } => matches!(
                action,
                FieldTransformAction::Custom(_)
                    | FieldTransformAction::ClassAction(TransformAction::Custom(_))
            ),
            PipelineStep::Method { action, .. } => matches!(
                action,
                MethodTransformAction::Custom(_)
                    | MethodTransformAction::ClassAction(TransformAction::Custom(_))
            ),
            PipelineStep::Code { action, .. } => matches!(action, CodeTransformAction::Custom(_)),
        })
    }

    pub(crate) fn compile_internal(&self) -> PyResult<PyCompiledPipeline> {
        let inner = self
            .spec
            .try_compile()
            .map_err(|e| PyValueError::new_err(format!("invalid regex pattern: {e}")))?;
        Ok(PyCompiledPipeline {
            inner,
            contains_python_callbacks: self.contains_python_callbacks(),
        })
    }
}

#[pymethods]
impl PyPipeline {
    #[new]
    fn new() -> Self {
        Self {
            spec: PipelineSpec::new(),
        }
    }

    /// Add a class-level step: apply transform to classes matching the matcher.
    fn on_classes(&mut self, matcher: &PyClassMatcher, transform: &PyClassTransform) {
        self.spec.steps.push(PipelineStep::Class {
            matcher: matcher.spec.clone(),
            action: TransformAction::BuiltIn(transform.spec.clone()),
        });
    }

    /// Add a class-level step with a custom Python callback receiving a mutable ``ClassModel``.
    fn on_classes_custom(
        &mut self,
        matcher: &PyClassMatcher,
        callback: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let cb = make_class_callback(callback)?;
        self.spec.steps.push(PipelineStep::Class {
            matcher: matcher.spec.clone(),
            action: TransformAction::Custom(cb),
        });
        Ok(())
    }

    /// Add a field-level step accepting either ``FieldTransform`` or class-scoped ``ClassTransform``.
    #[pyo3(signature = (field_matcher, transform, owner_matcher=None))]
    fn on_fields(
        &mut self,
        field_matcher: &PyFieldMatcher,
        transform: &Bound<'_, PyAny>,
        owner_matcher: Option<&PyClassMatcher>,
    ) -> PyResult<()> {
        let owner = owner_matcher
            .map(|m| m.spec.clone())
            .unwrap_or(ClassMatcherSpec::Any);
        if let Ok(ft) = transform.cast::<PyFieldTransform>() {
            self.spec.steps.push(PipelineStep::Field {
                owner_matcher: owner,
                field_matcher: field_matcher.spec.clone(),
                action: FieldTransformAction::BuiltIn(ft.borrow().spec.clone()),
            });
            return Ok(());
        }
        if let Ok(ct) = transform.cast::<PyClassTransform>() {
            self.spec.steps.push(PipelineStep::Field {
                owner_matcher: owner,
                field_matcher: field_matcher.spec.clone(),
                action: FieldTransformAction::ClassAction(TransformAction::BuiltIn(
                    ct.borrow().spec.clone(),
                )),
            });
            return Ok(());
        }
        Err(PyTypeError::new_err(
            "on_fields() transform must be a FieldTransform or ClassTransform",
        ))
    }

    /// Add a field-level guard step with a custom Python callback receiving ``ClassModel``.
    #[pyo3(signature = (field_matcher, callback, owner_matcher=None))]
    fn on_fields_custom(
        &mut self,
        field_matcher: &PyFieldMatcher,
        callback: &Bound<'_, PyAny>,
        owner_matcher: Option<&PyClassMatcher>,
    ) -> PyResult<()> {
        let cb = make_class_callback(callback)?;
        self.spec.steps.push(PipelineStep::Field {
            owner_matcher: owner_matcher
                .map(|m| m.spec.clone())
                .unwrap_or(ClassMatcherSpec::Any),
            field_matcher: field_matcher.spec.clone(),
            action: FieldTransformAction::ClassAction(TransformAction::Custom(cb)),
        });
        Ok(())
    }

    /// Add a method-level step accepting either ``MethodTransform`` or class-scoped ``ClassTransform``.
    #[pyo3(signature = (method_matcher, transform, owner_matcher=None))]
    fn on_methods(
        &mut self,
        method_matcher: &PyMethodMatcher,
        transform: &Bound<'_, PyAny>,
        owner_matcher: Option<&PyClassMatcher>,
    ) -> PyResult<()> {
        let owner = owner_matcher
            .map(|m| m.spec.clone())
            .unwrap_or(ClassMatcherSpec::Any);
        if let Ok(mt) = transform.cast::<PyMethodTransform>() {
            self.spec.steps.push(PipelineStep::Method {
                owner_matcher: owner,
                method_matcher: method_matcher.spec.clone(),
                action: MethodTransformAction::BuiltIn(mt.borrow().spec.clone()),
            });
            return Ok(());
        }
        if let Ok(ct) = transform.cast::<PyClassTransform>() {
            self.spec.steps.push(PipelineStep::Method {
                owner_matcher: owner,
                method_matcher: method_matcher.spec.clone(),
                action: MethodTransformAction::ClassAction(TransformAction::BuiltIn(
                    ct.borrow().spec.clone(),
                )),
            });
            return Ok(());
        }
        Err(PyTypeError::new_err(
            "on_methods() transform must be a MethodTransform or ClassTransform",
        ))
    }

    /// Add a method-level guard step with a custom Python callback receiving ``ClassModel``.
    #[pyo3(signature = (method_matcher, callback, owner_matcher=None))]
    fn on_methods_custom(
        &mut self,
        method_matcher: &PyMethodMatcher,
        callback: &Bound<'_, PyAny>,
        owner_matcher: Option<&PyClassMatcher>,
    ) -> PyResult<()> {
        let cb = make_class_callback(callback)?;
        self.spec.steps.push(PipelineStep::Method {
            owner_matcher: owner_matcher
                .map(|m| m.spec.clone())
                .unwrap_or(ClassMatcherSpec::Any),
            method_matcher: method_matcher.spec.clone(),
            action: MethodTransformAction::ClassAction(TransformAction::Custom(cb)),
        });
        Ok(())
    }

    /// Add a code-level step: apply transform to matching methods with code.
    #[pyo3(signature = (method_matcher, transform, owner_matcher=None))]
    fn on_code(
        &mut self,
        method_matcher: &PyMethodMatcher,
        transform: &PyCodeTransform,
        owner_matcher: Option<&PyClassMatcher>,
    ) {
        self.spec.steps.push(PipelineStep::Code {
            owner_matcher: owner_matcher
                .map(|m| m.spec.clone())
                .unwrap_or(ClassMatcherSpec::Any),
            method_matcher: method_matcher.spec.clone(),
            action: CodeTransformAction::BuiltIn(transform.spec.clone()),
        });
    }

    /// Add a code-level step with a custom Python callback receiving ``CodeModel``.
    #[pyo3(signature = (method_matcher, callback, owner_matcher=None))]
    fn on_code_custom(
        &mut self,
        method_matcher: &PyMethodMatcher,
        callback: &Bound<'_, PyAny>,
        owner_matcher: Option<&PyClassMatcher>,
    ) -> PyResult<()> {
        let cb = make_code_callback(callback)?;
        self.spec.steps.push(PipelineStep::Code {
            owner_matcher: owner_matcher
                .map(|m| m.spec.clone())
                .unwrap_or(ClassMatcherSpec::Any),
            method_matcher: method_matcher.spec.clone(),
            action: CodeTransformAction::Custom(cb),
        });
        Ok(())
    }

    /// Apply pipeline to a single model (mutates in-place).
    fn apply(&self, model: &mut PyClassModel) -> PyResult<()> {
        let compiled = self.compile_internal()?;
        model.with_class_model_mut(|inner| compiled.apply_to_engine_model(inner))
    }

    /// Apply pipeline to many models (mutates in-place).
    fn apply_all(&self, models: &Bound<'_, PyAny>) -> PyResult<()> {
        let compiled = self.compile_internal()?;
        for item in models.try_iter()? {
            let item = item?;
            let mut model: PyRefMut<'_, PyClassModel> = item.extract()?;
            model.with_class_model_mut(|inner| compiled.apply_to_engine_model(inner))?;
        }
        Ok(())
    }

    /// Report whether this pipeline contains custom Python callback steps.
    fn has_python_callbacks(&self) -> bool {
        self.contains_python_callbacks()
    }

    /// Compile the pipeline for repeated application (pre-compiles regexes).
    fn compile(&self) -> PyResult<PyCompiledPipeline> {
        self.compile_internal()
    }

    /// Return a shallow snapshot clone of this pipeline.
    fn copy(&self) -> Self {
        self.clone()
    }

    fn __len__(&self) -> usize {
        self.spec.steps.len()
    }

    fn __repr__(&self) -> String {
        format!("Pipeline(steps={})", self.spec.steps.len())
    }
}

// ---------------------------------------------------------------------------
// PyCompiledPipeline — wraps CompiledPipeline (pre-compiled regexes)
// ---------------------------------------------------------------------------

/// A compiled pipeline with pre-compiled regexes for hot-path evaluation.
#[pyclass(name = "CompiledPipeline", module = "pytecode._rust")]
pub struct PyCompiledPipeline {
    pub(crate) inner: CompiledPipeline,
    pub(crate) contains_python_callbacks: bool,
}

impl PyCompiledPipeline {
    pub(crate) fn apply_to_engine_model(&self, model: &mut ClassModel) -> PyResult<()> {
        run_engine_pipeline_apply(|| self.inner.apply(model))
    }
}

#[pymethods]
impl PyCompiledPipeline {
    /// Apply compiled pipeline to a single model (mutates in-place).
    fn apply(&self, model: &mut PyClassModel) -> PyResult<()> {
        model.with_class_model_mut(|inner| self.apply_to_engine_model(inner))
    }

    /// Report whether this compiled pipeline contains custom Python callback steps.
    fn has_python_callbacks(&self) -> bool {
        self.contains_python_callbacks
    }

    /// Apply compiled pipeline to many models (mutates in-place).
    fn apply_all(&self, models: &Bound<'_, PyAny>) -> PyResult<()> {
        for item in models.try_iter()? {
            let item = item?;
            let mut model: PyRefMut<'_, PyClassModel> = item.extract()?;
            model.with_class_model_mut(|inner| self.apply_to_engine_model(inner))?;
        }
        Ok(())
    }

    fn __repr__(&self) -> String {
        "CompiledPipeline()".to_string()
    }
}
