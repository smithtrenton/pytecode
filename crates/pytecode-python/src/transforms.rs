//! PyO3 wrappers for declarative matcher specs.
//!
//! These types expose the Rust `*MatcherSpec` enums to Python so that
//! Python code can construct matcher trees that Rust evaluates natively.

use pyo3::prelude::*;
use pytecode_engine::model::ClassModel;
use pytecode_engine::transform::matcher_spec::{
    ClassMatcherSpec, FieldMatcherSpec, MethodMatcherSpec,
};
use pytecode_engine::transform::pipeline_spec::{
    CompiledPipeline, PipelineSpec, PipelineStep, TransformAction,
};
use pytecode_engine::transform::transform_spec::ClassTransformSpec;

use crate::model::PyClassModel;

// ---------------------------------------------------------------------------
// PyClassMatcher
// ---------------------------------------------------------------------------

/// A declarative class matcher that Rust evaluates without FFI per-match.
#[pyclass(name = "RustClassMatcher", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyClassMatcher {
    pub(crate) spec: ClassMatcherSpec,
}

#[pymethods]
impl PyClassMatcher {
    /// Match by exact class name (internal JVM format, e.g. `"java/lang/String"`).
    #[staticmethod]
    fn named(name: String) -> Self {
        Self {
            spec: ClassMatcherSpec::Named(name),
        }
    }

    /// Match by regex pattern against class name.
    ///
    /// # Errors
    /// Returns `ValueError` if `pattern` is not a valid regular expression.
    #[staticmethod]
    fn name_matches(pattern: String) -> PyResult<Self> {
        regex::Regex::new(&format!("^(?:{pattern})$")).map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("invalid regex pattern: {e}"))
        })?;
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

    /// Match package-private classes (no PUBLIC flag).
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

    /// Match when the class implements the named interface.
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

    /// Always matches.
    #[staticmethod]
    fn any() -> Self {
        Self {
            spec: ClassMatcherSpec::Any,
        }
    }

    fn __and__(&self, other: &PyClassMatcher) -> Self {
        match (&self.spec, &other.spec) {
            // Flatten nested Ands
            (ClassMatcherSpec::And(left), ClassMatcherSpec::And(right)) => {
                let mut specs = left.clone();
                specs.extend(right.iter().cloned());
                Self {
                    spec: ClassMatcherSpec::And(specs),
                }
            }
            (ClassMatcherSpec::And(left), _) => {
                let mut specs = left.clone();
                specs.push(other.spec.clone());
                Self {
                    spec: ClassMatcherSpec::And(specs),
                }
            }
            (_, ClassMatcherSpec::And(right)) => {
                let mut specs = vec![self.spec.clone()];
                specs.extend(right.iter().cloned());
                Self {
                    spec: ClassMatcherSpec::And(specs),
                }
            }
            _ => Self {
                spec: ClassMatcherSpec::And(vec![self.spec.clone(), other.spec.clone()]),
            },
        }
    }

    fn __or__(&self, other: &PyClassMatcher) -> Self {
        match (&self.spec, &other.spec) {
            (ClassMatcherSpec::Or(left), ClassMatcherSpec::Or(right)) => {
                let mut specs = left.clone();
                specs.extend(right.iter().cloned());
                Self {
                    spec: ClassMatcherSpec::Or(specs),
                }
            }
            (ClassMatcherSpec::Or(left), _) => {
                let mut specs = left.clone();
                specs.push(other.spec.clone());
                Self {
                    spec: ClassMatcherSpec::Or(specs),
                }
            }
            (_, ClassMatcherSpec::Or(right)) => {
                let mut specs = vec![self.spec.clone()];
                specs.extend(right.iter().cloned());
                Self {
                    spec: ClassMatcherSpec::Or(specs),
                }
            }
            _ => Self {
                spec: ClassMatcherSpec::Or(vec![self.spec.clone(), other.spec.clone()]),
            },
        }
    }

    fn __invert__(&self) -> Self {
        Self {
            spec: ClassMatcherSpec::Not(Box::new(self.spec.clone())),
        }
    }

    fn __repr__(&self) -> String {
        format!("RustClassMatcher({})", self.spec)
    }

    fn __str__(&self) -> String {
        self.spec.to_string()
    }
}

// ---------------------------------------------------------------------------
// PyFieldMatcher
// ---------------------------------------------------------------------------

/// A declarative field matcher that Rust evaluates without FFI per-match.
#[pyclass(name = "RustFieldMatcher", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyFieldMatcher {
    pub(crate) spec: FieldMatcherSpec,
}

#[pymethods]
impl PyFieldMatcher {
    #[staticmethod]
    fn named(name: String) -> Self {
        Self {
            spec: FieldMatcherSpec::Named(name),
        }
    }

    /// Match by regex pattern against field name.
    ///
    /// # Errors
    /// Returns `ValueError` if `pattern` is not a valid regular expression.
    #[staticmethod]
    fn name_matches(pattern: String) -> PyResult<Self> {
        regex::Regex::new(&format!("^(?:{pattern})$")).map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("invalid regex pattern: {e}"))
        })?;
        Ok(Self {
            spec: FieldMatcherSpec::NameMatches(pattern),
        })
    }

    #[staticmethod]
    fn descriptor(desc: String) -> Self {
        Self {
            spec: FieldMatcherSpec::Descriptor(desc),
        }
    }

    /// Match by regex pattern against field descriptor.
    ///
    /// # Errors
    /// Returns `ValueError` if `pattern` is not a valid regular expression.
    #[staticmethod]
    fn descriptor_matches(pattern: String) -> PyResult<Self> {
        regex::Regex::new(&format!("^(?:{pattern})$")).map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("invalid regex pattern: {e}"))
        })?;
        Ok(Self {
            spec: FieldMatcherSpec::DescriptorMatches(pattern),
        })
    }

    #[staticmethod]
    fn access_all(flags: u16) -> Self {
        Self {
            spec: FieldMatcherSpec::AccessAll(flags),
        }
    }

    #[staticmethod]
    fn access_any(flags: u16) -> Self {
        Self {
            spec: FieldMatcherSpec::AccessAny(flags),
        }
    }

    #[staticmethod]
    fn is_package_private() -> Self {
        Self {
            spec: FieldMatcherSpec::IsPackagePrivate,
        }
    }

    #[staticmethod]
    fn any() -> Self {
        Self {
            spec: FieldMatcherSpec::Any,
        }
    }

    fn __and__(&self, other: &PyFieldMatcher) -> Self {
        match (&self.spec, &other.spec) {
            (FieldMatcherSpec::And(left), FieldMatcherSpec::And(right)) => {
                let mut specs = left.clone();
                specs.extend(right.iter().cloned());
                Self {
                    spec: FieldMatcherSpec::And(specs),
                }
            }
            (FieldMatcherSpec::And(left), _) => {
                let mut specs = left.clone();
                specs.push(other.spec.clone());
                Self {
                    spec: FieldMatcherSpec::And(specs),
                }
            }
            (_, FieldMatcherSpec::And(right)) => {
                let mut specs = vec![self.spec.clone()];
                specs.extend(right.iter().cloned());
                Self {
                    spec: FieldMatcherSpec::And(specs),
                }
            }
            _ => Self {
                spec: FieldMatcherSpec::And(vec![self.spec.clone(), other.spec.clone()]),
            },
        }
    }

    fn __or__(&self, other: &PyFieldMatcher) -> Self {
        match (&self.spec, &other.spec) {
            (FieldMatcherSpec::Or(left), FieldMatcherSpec::Or(right)) => {
                let mut specs = left.clone();
                specs.extend(right.iter().cloned());
                Self {
                    spec: FieldMatcherSpec::Or(specs),
                }
            }
            (FieldMatcherSpec::Or(left), _) => {
                let mut specs = left.clone();
                specs.push(other.spec.clone());
                Self {
                    spec: FieldMatcherSpec::Or(specs),
                }
            }
            (_, FieldMatcherSpec::Or(right)) => {
                let mut specs = vec![self.spec.clone()];
                specs.extend(right.iter().cloned());
                Self {
                    spec: FieldMatcherSpec::Or(specs),
                }
            }
            _ => Self {
                spec: FieldMatcherSpec::Or(vec![self.spec.clone(), other.spec.clone()]),
            },
        }
    }

    fn __invert__(&self) -> Self {
        Self {
            spec: FieldMatcherSpec::Not(Box::new(self.spec.clone())),
        }
    }

    fn __repr__(&self) -> String {
        format!("RustFieldMatcher({})", self.spec)
    }

    fn __str__(&self) -> String {
        self.spec.to_string()
    }
}

// ---------------------------------------------------------------------------
// PyMethodMatcher
// ---------------------------------------------------------------------------

/// A declarative method matcher that Rust evaluates without FFI per-match.
#[pyclass(name = "RustMethodMatcher", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyMethodMatcher {
    pub(crate) spec: MethodMatcherSpec,
}

#[pymethods]
impl PyMethodMatcher {
    #[staticmethod]
    fn named(name: String) -> Self {
        Self {
            spec: MethodMatcherSpec::Named(name),
        }
    }

    /// Match by regex pattern against method name.
    ///
    /// # Errors
    /// Returns `ValueError` if `pattern` is not a valid regular expression.
    #[staticmethod]
    fn name_matches(pattern: String) -> PyResult<Self> {
        regex::Regex::new(&format!("^(?:{pattern})$")).map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("invalid regex pattern: {e}"))
        })?;
        Ok(Self {
            spec: MethodMatcherSpec::NameMatches(pattern),
        })
    }

    #[staticmethod]
    fn descriptor(desc: String) -> Self {
        Self {
            spec: MethodMatcherSpec::Descriptor(desc),
        }
    }

    /// Match by regex pattern against method descriptor.
    ///
    /// # Errors
    /// Returns `ValueError` if `pattern` is not a valid regular expression.
    #[staticmethod]
    fn descriptor_matches(pattern: String) -> PyResult<Self> {
        regex::Regex::new(&format!("^(?:{pattern})$")).map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("invalid regex pattern: {e}"))
        })?;
        Ok(Self {
            spec: MethodMatcherSpec::DescriptorMatches(pattern),
        })
    }

    #[staticmethod]
    fn access_all(flags: u16) -> Self {
        Self {
            spec: MethodMatcherSpec::AccessAll(flags),
        }
    }

    #[staticmethod]
    fn access_any(flags: u16) -> Self {
        Self {
            spec: MethodMatcherSpec::AccessAny(flags),
        }
    }

    #[staticmethod]
    fn is_package_private() -> Self {
        Self {
            spec: MethodMatcherSpec::IsPackagePrivate,
        }
    }

    #[staticmethod]
    fn has_code() -> Self {
        Self {
            spec: MethodMatcherSpec::HasCode,
        }
    }

    #[staticmethod]
    fn is_constructor() -> Self {
        Self {
            spec: MethodMatcherSpec::IsConstructor,
        }
    }

    #[staticmethod]
    fn is_static_initializer() -> Self {
        Self {
            spec: MethodMatcherSpec::IsStaticInitializer,
        }
    }

    #[staticmethod]
    fn returns(descriptor: String) -> Self {
        Self {
            spec: MethodMatcherSpec::Returns(descriptor),
        }
    }

    #[staticmethod]
    fn any() -> Self {
        Self {
            spec: MethodMatcherSpec::Any,
        }
    }

    fn __and__(&self, other: &PyMethodMatcher) -> Self {
        match (&self.spec, &other.spec) {
            (MethodMatcherSpec::And(left), MethodMatcherSpec::And(right)) => {
                let mut specs = left.clone();
                specs.extend(right.iter().cloned());
                Self {
                    spec: MethodMatcherSpec::And(specs),
                }
            }
            (MethodMatcherSpec::And(left), _) => {
                let mut specs = left.clone();
                specs.push(other.spec.clone());
                Self {
                    spec: MethodMatcherSpec::And(specs),
                }
            }
            (_, MethodMatcherSpec::And(right)) => {
                let mut specs = vec![self.spec.clone()];
                specs.extend(right.iter().cloned());
                Self {
                    spec: MethodMatcherSpec::And(specs),
                }
            }
            _ => Self {
                spec: MethodMatcherSpec::And(vec![self.spec.clone(), other.spec.clone()]),
            },
        }
    }

    fn __or__(&self, other: &PyMethodMatcher) -> Self {
        match (&self.spec, &other.spec) {
            (MethodMatcherSpec::Or(left), MethodMatcherSpec::Or(right)) => {
                let mut specs = left.clone();
                specs.extend(right.iter().cloned());
                Self {
                    spec: MethodMatcherSpec::Or(specs),
                }
            }
            (MethodMatcherSpec::Or(left), _) => {
                let mut specs = left.clone();
                specs.push(other.spec.clone());
                Self {
                    spec: MethodMatcherSpec::Or(specs),
                }
            }
            (_, MethodMatcherSpec::Or(right)) => {
                let mut specs = vec![self.spec.clone()];
                specs.extend(right.iter().cloned());
                Self {
                    spec: MethodMatcherSpec::Or(specs),
                }
            }
            _ => Self {
                spec: MethodMatcherSpec::Or(vec![self.spec.clone(), other.spec.clone()]),
            },
        }
    }

    fn __invert__(&self) -> Self {
        Self {
            spec: MethodMatcherSpec::Not(Box::new(self.spec.clone())),
        }
    }

    fn __repr__(&self) -> String {
        format!("RustMethodMatcher({})", self.spec)
    }

    fn __str__(&self) -> String {
        self.spec.to_string()
    }
}

// ---------------------------------------------------------------------------
// PyClassTransform — wraps ClassTransformSpec
// ---------------------------------------------------------------------------

/// A declarative class transform that Rust applies natively.
#[pyclass(name = "RustClassTransform", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyClassTransform {
    pub(crate) spec: ClassTransformSpec,
}

#[pymethods]
impl PyClassTransform {
    #[staticmethod]
    fn rename_class(name: String) -> Self {
        Self {
            spec: ClassTransformSpec::RenameClass(name),
        }
    }

    #[staticmethod]
    fn set_access_flags(flags: u16) -> Self {
        Self {
            spec: ClassTransformSpec::SetAccessFlags(flags),
        }
    }

    #[staticmethod]
    fn add_access_flags(flags: u16) -> Self {
        Self {
            spec: ClassTransformSpec::AddAccessFlags(flags),
        }
    }

    #[staticmethod]
    fn remove_access_flags(flags: u16) -> Self {
        Self {
            spec: ClassTransformSpec::RemoveAccessFlags(flags),
        }
    }

    #[staticmethod]
    fn set_super_class(name: String) -> Self {
        Self {
            spec: ClassTransformSpec::SetSuperClass(name),
        }
    }

    #[staticmethod]
    fn add_interface(name: String) -> Self {
        Self {
            spec: ClassTransformSpec::AddInterface(name),
        }
    }

    #[staticmethod]
    fn remove_interface(name: String) -> Self {
        Self {
            spec: ClassTransformSpec::RemoveInterface(name),
        }
    }

    #[staticmethod]
    #[pyo3(signature = (name, descriptor=None))]
    fn remove_method(name: String, descriptor: Option<String>) -> Self {
        Self {
            spec: ClassTransformSpec::RemoveMethod { name, descriptor },
        }
    }

    #[staticmethod]
    #[pyo3(signature = (name, descriptor=None))]
    fn remove_field(name: String, descriptor: Option<String>) -> Self {
        Self {
            spec: ClassTransformSpec::RemoveField { name, descriptor },
        }
    }

    #[staticmethod]
    fn rename_method(from: String, to: String) -> Self {
        Self {
            spec: ClassTransformSpec::RenameMethod { from, to },
        }
    }

    #[staticmethod]
    fn rename_field(from: String, to: String) -> Self {
        Self {
            spec: ClassTransformSpec::RenameField { from, to },
        }
    }

    #[staticmethod]
    fn set_method_access_flags(name: String, flags: u16) -> Self {
        Self {
            spec: ClassTransformSpec::SetMethodAccessFlags { name, flags },
        }
    }

    #[staticmethod]
    fn set_field_access_flags(name: String, flags: u16) -> Self {
        Self {
            spec: ClassTransformSpec::SetFieldAccessFlags { name, flags },
        }
    }

    #[staticmethod]
    fn sequence(transforms: Vec<PyClassTransform>) -> Self {
        Self {
            spec: ClassTransformSpec::Sequence(transforms.into_iter().map(|t| t.spec).collect()),
        }
    }

    fn __repr__(&self) -> String {
        format!("RustClassTransform({})", self.spec)
    }

    fn __str__(&self) -> String {
        self.spec.to_string()
    }
}

// ---------------------------------------------------------------------------
// PyPipeline — wraps PipelineSpec
// ---------------------------------------------------------------------------

/// A declarative transform pipeline that Rust evaluates natively.
#[pyclass(name = "RustPipeline", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyPipeline {
    pub(crate) spec: PipelineSpec,
    /// Shared error slot: custom callbacks write here when they raise a Python exception.
    /// Checked and cleared by `apply`/`apply_all` after the pipeline runs.
    callback_error: std::sync::Arc<std::sync::Mutex<Option<pyo3::PyErr>>>,
}

impl PyPipeline {
    pub(crate) fn has_python_callbacks(&self) -> bool {
        self.spec.steps.iter().any(|step| match step {
            PipelineStep::Class { action, .. }
            | PipelineStep::Field { action, .. }
            | PipelineStep::Method { action, .. } => matches!(action, TransformAction::Custom(_)),
        })
    }
}

#[pymethods]
impl PyPipeline {
    #[new]
    fn new() -> Self {
        Self {
            spec: PipelineSpec::new(),
            callback_error: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Add a class-level step: apply transform to classes matching the matcher.
    fn on_classes(&mut self, matcher: &PyClassMatcher, transform: &PyClassTransform) {
        self.spec.steps.push(PipelineStep::Class {
            matcher: matcher.spec.clone(),
            action: TransformAction::BuiltIn(transform.spec.clone()),
        });
    }

    /// Add a class-level step with a custom Python callback.
    ///
    /// The callback receives a mutable `RustClassModel` and should modify it
    /// in-place. Matching is still done natively in Rust.
    fn on_classes_custom(&mut self, matcher: &PyClassMatcher, callback: PyObject) {
        let cb = std::sync::Arc::new(callback);
        let error_slot = self.callback_error.clone();
        self.spec.steps.push(PipelineStep::Class {
            matcher: matcher.spec.clone(),
            action: TransformAction::Custom(std::sync::Arc::new(move |model: &mut ClassModel| {
                Python::with_gil(|py| {
                    // Swap model into wrapper — zero-copy move, not clone
                    let py_model = PyClassModel::from_model(std::mem::take(model));
                    let cell = Py::new(py, py_model).expect("failed to create PyClassModel");
                    if let Err(e) = cb.call1(py, (&cell,)) {
                        *error_slot.lock().unwrap() = Some(e);
                    }
                    // Move back out — zero-copy
                    *model = cell
                        .borrow_mut(py)
                        .take_inner()
                        .expect("failed to take RustClassModel back from Python");
                });
            })),
        });
    }

    /// Add a field-level step: apply transform when any field matches.
    #[pyo3(signature = (field_matcher, transform, owner_matcher=None))]
    fn on_fields(
        &mut self,
        field_matcher: &PyFieldMatcher,
        transform: &PyClassTransform,
        owner_matcher: Option<&PyClassMatcher>,
    ) {
        self.spec.steps.push(PipelineStep::Field {
            owner_matcher: owner_matcher
                .map(|m| m.spec.clone())
                .unwrap_or(ClassMatcherSpec::Any),
            field_matcher: field_matcher.spec.clone(),
            action: TransformAction::BuiltIn(transform.spec.clone()),
        });
    }

    /// Add a field-level step with a custom Python callback.
    #[pyo3(signature = (field_matcher, callback, owner_matcher=None))]
    fn on_fields_custom(
        &mut self,
        field_matcher: &PyFieldMatcher,
        callback: PyObject,
        owner_matcher: Option<&PyClassMatcher>,
    ) {
        let cb = std::sync::Arc::new(callback);
        let error_slot = self.callback_error.clone();
        self.spec.steps.push(PipelineStep::Field {
            owner_matcher: owner_matcher
                .map(|m| m.spec.clone())
                .unwrap_or(ClassMatcherSpec::Any),
            field_matcher: field_matcher.spec.clone(),
            action: TransformAction::Custom(std::sync::Arc::new(move |model: &mut ClassModel| {
                Python::with_gil(|py| {
                    let py_model = PyClassModel::from_model(std::mem::take(model));
                    let cell = Py::new(py, py_model).expect("failed to create PyClassModel");
                    if let Err(e) = cb.call1(py, (&cell,)) {
                        *error_slot.lock().unwrap() = Some(e);
                    }
                    *model = cell
                        .borrow_mut(py)
                        .take_inner()
                        .expect("failed to take RustClassModel back from Python");
                });
            })),
        });
    }

    /// Add a method-level step: apply transform when any method matches.
    #[pyo3(signature = (method_matcher, transform, owner_matcher=None))]
    fn on_methods(
        &mut self,
        method_matcher: &PyMethodMatcher,
        transform: &PyClassTransform,
        owner_matcher: Option<&PyClassMatcher>,
    ) {
        self.spec.steps.push(PipelineStep::Method {
            owner_matcher: owner_matcher
                .map(|m| m.spec.clone())
                .unwrap_or(ClassMatcherSpec::Any),
            method_matcher: method_matcher.spec.clone(),
            action: TransformAction::BuiltIn(transform.spec.clone()),
        });
    }

    /// Add a method-level step with a custom Python callback.
    #[pyo3(signature = (method_matcher, callback, owner_matcher=None))]
    fn on_methods_custom(
        &mut self,
        method_matcher: &PyMethodMatcher,
        callback: PyObject,
        owner_matcher: Option<&PyClassMatcher>,
    ) {
        let cb = std::sync::Arc::new(callback);
        let error_slot = self.callback_error.clone();
        self.spec.steps.push(PipelineStep::Method {
            owner_matcher: owner_matcher
                .map(|m| m.spec.clone())
                .unwrap_or(ClassMatcherSpec::Any),
            method_matcher: method_matcher.spec.clone(),
            action: TransformAction::Custom(std::sync::Arc::new(move |model: &mut ClassModel| {
                Python::with_gil(|py| {
                    let py_model = PyClassModel::from_model(std::mem::take(model));
                    let cell = Py::new(py, py_model).expect("failed to create PyClassModel");
                    if let Err(e) = cb.call1(py, (&cell,)) {
                        *error_slot.lock().unwrap() = Some(e);
                    }
                    *model = cell
                        .borrow_mut(py)
                        .take_inner()
                        .expect("failed to take RustClassModel back from Python");
                });
            })),
        });
    }

    /// Apply pipeline to a single model (mutates in-place).
    ///
    /// For repeated single-model calls in a loop, prefer [`PyPipeline::compile`] to avoid
    /// re-compiling regex matchers on every invocation.
    fn apply(&self, model: &mut PyClassModel) -> PyResult<()> {
        let compiled = self.spec.compile();
        model.with_class_model_mut(|inner| {
            compiled.apply(inner);
            Ok(())
        })?;
        if let Some(err) = self.callback_error.lock().unwrap().take() {
            return Err(err);
        }
        Ok(())
    }

    /// Apply pipeline to many models (mutates in-place).
    fn apply_all(&self, _py: Python<'_>, models: &Bound<'_, pyo3::types::PyList>) -> PyResult<()> {
        let compiled = self.spec.compile();
        for item in models.iter() {
            let mut model: PyRefMut<'_, PyClassModel> = item.extract()?;
            model.with_class_model_mut(|inner| {
                compiled.apply(inner);
                Ok(())
            })?;
            if let Some(err) = self.callback_error.lock().unwrap().take() {
                return Err(err);
            }
        }
        Ok(())
    }

    /// Compile the pipeline for repeated application (pre-compiles regexes).
    fn compile(&self) -> PyCompiledPipeline {
        PyCompiledPipeline {
            inner: self.spec.compile(),
            contains_python_callbacks: self.has_python_callbacks(),
            callback_error: self.callback_error.clone(),
        }
    }

    /// Return the number of steps in this pipeline.
    fn __len__(&self) -> usize {
        self.spec.steps.len()
    }

    fn __repr__(&self) -> String {
        format!("RustPipeline(steps={})", self.spec.steps.len())
    }
}

// ---------------------------------------------------------------------------
// PyCompiledPipeline — wraps CompiledPipeline (pre-compiled regexes)
// ---------------------------------------------------------------------------

/// A compiled pipeline with pre-compiled regexes for hot-path evaluation.
#[pyclass(name = "RustCompiledPipeline", module = "pytecode._rust")]
pub struct PyCompiledPipeline {
    pub(crate) inner: CompiledPipeline,
    pub(crate) contains_python_callbacks: bool,
    /// Shared error slot from the originating `PyPipeline`.
    callback_error: std::sync::Arc<std::sync::Mutex<Option<pyo3::PyErr>>>,
}

#[pymethods]
impl PyCompiledPipeline {
    /// Apply compiled pipeline to a single model (mutates in-place).
    fn apply(&self, model: &mut PyClassModel) -> PyResult<()> {
        model.with_class_model_mut(|inner| {
            self.inner.apply(inner);
            Ok(())
        })?;
        if let Some(err) = self.callback_error.lock().unwrap().take() {
            return Err(err);
        }
        Ok(())
    }

    /// Apply compiled pipeline to many models (mutates in-place).
    fn apply_all(&self, _py: Python<'_>, models: &Bound<'_, pyo3::types::PyList>) -> PyResult<()> {
        for item in models.iter() {
            let mut model: PyRefMut<'_, PyClassModel> = item.extract()?;
            model.with_class_model_mut(|inner| {
                self.inner.apply(inner);
                Ok(())
            })?;
            if let Some(err) = self.callback_error.lock().unwrap().take() {
                return Err(err);
            }
        }
        Ok(())
    }

    fn __repr__(&self) -> String {
        "RustCompiledPipeline()".to_string()
    }
}
