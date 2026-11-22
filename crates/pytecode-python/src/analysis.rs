use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyAny, PyDict, PyType};
use pytecode_engine::analysis::{
    AnalysisError, Category, Diagnostic, InheritedMethod, ResolvedClass, ResolvedMethod, Severity,
    common_superclass as engine_common_superclass,
    find_overridden_methods as engine_find_overridden_methods, is_subtype as engine_is_subtype,
    iter_superclasses as engine_iter_superclasses, iter_supertypes as engine_iter_supertypes,
    verify_classfile, verify_classfile_with_options, verify_classmodel,
    verify_classmodel_with_options,
};
use pytecode_engine::constants::MethodAccessFlags;
use pytecode_engine::parse_class;
use pytecode_engine::raw::ClassFile;

use crate::model::{PyClassModel, PyMappingClassResolver};

type PyObject = Py<PyAny>;

fn cached_hierarchy_exc<'py>(
    cell: &'static PyOnceLock<Py<PyType>>,
    py: Python<'py>,
    name: &str,
) -> PyResult<&'py Bound<'py, PyType>> {
    cell.get_or_try_init(py, || {
        let module = PyModule::import(py, "pytecode.analysis.hierarchy")?;
        Ok(module.getattr(name)?.cast_into::<PyType>()?.unbind())
    })
    .map(|ty| ty.bind(py))
}

pub(crate) fn analysis_error_to_py(py: Python<'_>, error: AnalysisError) -> PyErr {
    static HIERARCHY_ERROR: PyOnceLock<Py<PyType>> = PyOnceLock::new();
    static UNRESOLVED_CLASS_ERROR: PyOnceLock<Py<PyType>> = PyOnceLock::new();
    static HIERARCHY_CYCLE_ERROR: PyOnceLock<Py<PyType>> = PyOnceLock::new();

    let (cell, attr_name) = match &error {
        AnalysisError::UnresolvedClass { .. } => (&UNRESOLVED_CLASS_ERROR, "UnresolvedClassError"),
        AnalysisError::HierarchyCycle { .. } => (&HIERARCHY_CYCLE_ERROR, "HierarchyCycleError"),
        _ => (&HIERARCHY_ERROR, "HierarchyError"),
    };
    let message = error.to_string();
    match cached_hierarchy_exc(cell, py, attr_name) {
        Ok(exc_type) => PyErr::from_type(exc_type.clone(), (message,)),
        Err(_) => PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(message),
    }
}

pub(crate) fn resolved_method_to_py(py: Python<'_>, method: &ResolvedMethod) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    dict.set_item("name", &method.name)?;
    dict.set_item("descriptor", &method.descriptor)?;
    dict.set_item("access_flags", method.access_flags.bits())?;
    Ok(dict.into_any().unbind())
}

pub(crate) fn resolved_class_to_py(py: Python<'_>, resolved: &ResolvedClass) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    let methods = resolved
        .methods
        .iter()
        .map(|method| resolved_method_to_py(py, method))
        .collect::<PyResult<Vec<_>>>()?;
    dict.set_item("name", &resolved.name)?;
    dict.set_item("super_name", &resolved.super_name)?;
    dict.set_item("interfaces", &resolved.interfaces)?;
    dict.set_item("access_flags", resolved.access_flags.bits())?;
    dict.set_item("methods", methods)?;
    Ok(dict.into_any().unbind())
}

fn inherited_method_to_py(py: Python<'_>, method: &InheritedMethod) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    dict.set_item("owner", &method.owner)?;
    dict.set_item("name", &method.name)?;
    dict.set_item("descriptor", &method.descriptor)?;
    dict.set_item("access_flags", method.access_flags.bits())?;
    Ok(dict.into_any().unbind())
}

fn with_classfile<R>(
    data: &Bound<'_, PyAny>,
    f: impl FnOnce(&ClassFile) -> PyResult<R>,
) -> PyResult<R> {
    if let Ok(cf) = data.extract::<PyRef<'_, crate::PyClassFile>>() {
        return f(&cf.inner);
    }
    if let Ok(reader) = data.extract::<PyRef<'_, crate::PyClassReader>>() {
        return f(&reader.class_info);
    }
    crate::with_bytes(data, |bytes| {
        let cf = parse_class(bytes).map_err(crate::engine_error_to_py)?;
        f(&cf)
    })
}

// ---------------------------------------------------------------------------
// PyDiagnostic
// ---------------------------------------------------------------------------

/// A single diagnostic emitted by the structural classfile/model verifier.
#[pyclass(from_py_object, name = "Diagnostic", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyDiagnostic {
    inner: Diagnostic,
}

#[pymethods]
impl PyDiagnostic {
    /// Severity level of the diagnostic (``"error"``, ``"warning"``, or ``"info"``).
    #[getter]
    fn severity(&self) -> &str {
        match self.inner.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        }
    }

    /// Structural category of the diagnostic.
    #[getter]
    fn category(&self) -> &str {
        match self.inner.category {
            Category::Magic => "magic",
            Category::Version => "version",
            Category::ConstantPool => "constant_pool",
            Category::AccessFlags => "access_flags",
            Category::ClassStructure => "class_structure",
            Category::Field => "field",
            Category::Method => "method",
            Category::Code => "code",
            Category::Attribute => "attribute",
            Category::Descriptor => "descriptor",
        }
    }

    /// Human-readable description of the diagnostic finding.
    #[getter]
    fn message(&self) -> &str {
        &self.inner.message
    }

    /// Internal class name associated with the diagnostic, if known.
    #[getter]
    fn class_name(&self) -> Option<&str> {
        self.inner.location.class_name.as_deref()
    }

    /// Field name associated with the diagnostic, if applicable.
    #[getter]
    fn field_name(&self) -> Option<&str> {
        self.inner.location.field_name.as_deref()
    }

    /// Method name associated with the diagnostic, if applicable.
    #[getter]
    fn method_name(&self) -> Option<&str> {
        self.inner.location.method_name.as_deref()
    }

    /// Method descriptor associated with the diagnostic, if applicable.
    #[getter]
    fn method_descriptor(&self) -> Option<&str> {
        self.inner.location.method_descriptor.as_deref()
    }

    /// Attribute name associated with the diagnostic, if applicable.
    #[getter]
    fn attribute_name(&self) -> Option<&str> {
        self.inner.location.attribute_name.as_deref()
    }

    /// Constant-pool index associated with the diagnostic, if applicable.
    #[getter]
    fn cp_index(&self) -> Option<u16> {
        self.inner.location.cp_index
    }

    /// Symbolic code-item index associated with the diagnostic, if applicable.
    #[getter]
    fn code_index(&self) -> Option<usize> {
        self.inner.location.code_index
    }

    fn __repr__(&self) -> String {
        format!(
            "Diagnostic(severity={:?}, category={:?}, message={:?})",
            self.severity(),
            self.category(),
            self.message()
        )
    }
}

// ---------------------------------------------------------------------------
// Top-level verify and hierarchy functions
// ---------------------------------------------------------------------------

/// Verify raw classfile bytes, a ``ClassFile``, or a ``ClassReader`` and return diagnostics.
#[pyfunction]
#[pyo3(signature = (data, *, fail_fast = false))]
fn rust_verify_classfile(data: &Bound<'_, PyAny>, fail_fast: bool) -> PyResult<Vec<PyDiagnostic>> {
    with_classfile(data, |classfile| {
        let diagnostics = if fail_fast {
            verify_classfile_with_options(classfile, true)
        } else {
            verify_classfile(classfile)
        };
        Ok(diagnostics
            .into_iter()
            .map(|d| PyDiagnostic { inner: d })
            .collect())
    })
}

/// Verify a mutable ``ClassModel`` with an optional hierarchy resolver.
#[pyfunction]
#[pyo3(signature = (model, resolver = None, *, fail_fast = false))]
fn rust_verify_classmodel(
    model: &PyClassModel,
    resolver: Option<&PyMappingClassResolver>,
    fail_fast: bool,
) -> PyResult<Vec<PyDiagnostic>> {
    let diagnostics = if fail_fast {
        model.with_class_model(|inner| {
            Ok(verify_classmodel_with_options(
                inner,
                resolver.map(|r| &r.inner as &dyn pytecode_engine::analysis::ClassResolver),
                true,
            ))
        })?
    } else {
        model.with_class_model(|inner| {
            Ok(verify_classmodel(
                inner,
                resolver.map(|r| &r.inner as &dyn pytecode_engine::analysis::ClassResolver),
            ))
        })?
    };
    Ok(diagnostics
        .into_iter()
        .map(|d| PyDiagnostic { inner: d })
        .collect())
}

/// Extract a resolved hierarchy snapshot dictionary from raw classfile bytes, ``ClassFile``, or ``ClassReader``.
#[pyfunction]
fn rust_resolved_classfile(py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<PyObject> {
    with_classfile(data, |classfile| {
        let resolved =
            ResolvedClass::from_classfile(classfile).map_err(crate::engine_error_to_py)?;
        resolved_class_to_py(py, &resolved)
    })
}

/// Extract a resolved hierarchy snapshot dictionary from a ``ClassModel``.
#[pyfunction]
fn rust_resolved_classmodel(py: Python<'_>, model: &PyClassModel) -> PyResult<PyObject> {
    let resolved = model.with_class_model(|inner| Ok(ResolvedClass::from_model(inner)))?;
    resolved_class_to_py(py, &resolved)
}

/// Return the linear superclass chain for ``class_name`` as resolved class dicts.
#[pyfunction]
#[pyo3(signature = (resolver, class_name, *, include_self = false))]
fn rust_iter_superclasses(
    py: Python<'_>,
    resolver: &PyMappingClassResolver,
    class_name: &str,
    include_self: bool,
) -> PyResult<Vec<PyObject>> {
    use pytecode_engine::analysis::ClassResolver;

    let superclasses = engine_iter_superclasses(&resolver.inner, class_name)
        .map_err(|e| analysis_error_to_py(py, e))?;
    let mut out = Vec::new();
    if include_self {
        let resolved = resolver.inner.resolve_class(class_name).ok_or_else(|| {
            analysis_error_to_py(
                py,
                AnalysisError::UnresolvedClass {
                    class_name: class_name.to_owned(),
                },
            )
        })?;
        if !superclasses.iter().any(|entry| entry.name == resolved.name) {
            out.push(resolved_class_to_py(py, &resolved)?);
        }
    }
    for resolved in &superclasses {
        out.push(resolved_class_to_py(py, resolved)?);
    }
    Ok(out)
}

/// Return all reachable supertypes (classes and interfaces) for ``class_name`` as resolved class dicts.
#[pyfunction]
#[pyo3(signature = (resolver, class_name, *, include_self = false))]
fn rust_iter_supertypes(
    py: Python<'_>,
    resolver: &PyMappingClassResolver,
    class_name: &str,
    include_self: bool,
) -> PyResult<Vec<PyObject>> {
    use pytecode_engine::analysis::ClassResolver;

    let supertypes = engine_iter_supertypes(&resolver.inner, class_name)
        .map_err(|e| analysis_error_to_py(py, e))?;
    let mut out = Vec::new();
    if include_self {
        let resolved = resolver.inner.resolve_class(class_name).ok_or_else(|| {
            analysis_error_to_py(
                py,
                AnalysisError::UnresolvedClass {
                    class_name: class_name.to_owned(),
                },
            )
        })?;
        if !supertypes.iter().any(|entry| entry.name == resolved.name) {
            out.push(resolved_class_to_py(py, &resolved)?);
        }
    }
    for resolved in &supertypes {
        out.push(resolved_class_to_py(py, resolved)?);
    }
    Ok(out)
}

/// Return whether ``class_name`` is a subtype of ``super_name``.
#[pyfunction]
fn rust_is_subtype(
    py: Python<'_>,
    resolver: &PyMappingClassResolver,
    class_name: &str,
    super_name: &str,
) -> PyResult<bool> {
    engine_is_subtype(&resolver.inner, class_name, super_name)
        .map_err(|e| analysis_error_to_py(py, e))
}

/// Return the closest common superclass of ``left`` and ``right``.
#[pyfunction]
fn rust_common_superclass(
    py: Python<'_>,
    resolver: &PyMappingClassResolver,
    left: &str,
    right: &str,
) -> PyResult<String> {
    engine_common_superclass(&resolver.inner, left, right).map_err(|e| analysis_error_to_py(py, e))
}

/// Return inherited methods overridden by a method declaration in ``class_name``.
#[pyfunction]
fn rust_find_overridden_methods(
    py: Python<'_>,
    resolver: &PyMappingClassResolver,
    class_name: &str,
    method_name: &str,
    method_descriptor: &str,
    access_flags: u16,
) -> PyResult<Vec<PyObject>> {
    let method = ResolvedMethod {
        name: method_name.to_owned(),
        descriptor: method_descriptor.to_owned(),
        access_flags: MethodAccessFlags::from_bits_truncate(access_flags),
    };
    engine_find_overridden_methods(&resolver.inner, class_name, &method)
        .map_err(|e| analysis_error_to_py(py, e))?
        .iter()
        .map(|entry| inherited_method_to_py(py, entry))
        .collect()
}

// ---------------------------------------------------------------------------
// register
// ---------------------------------------------------------------------------

pub(crate) fn register(_py: Python<'_>, module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyDiagnostic>()?;
    module.add_function(wrap_pyfunction!(rust_resolved_classfile, module)?)?;
    module.add_function(wrap_pyfunction!(rust_resolved_classmodel, module)?)?;
    module.add_function(wrap_pyfunction!(rust_iter_superclasses, module)?)?;
    module.add_function(wrap_pyfunction!(rust_iter_supertypes, module)?)?;
    module.add_function(wrap_pyfunction!(rust_is_subtype, module)?)?;
    module.add_function(wrap_pyfunction!(rust_common_superclass, module)?)?;
    module.add_function(wrap_pyfunction!(rust_find_overridden_methods, module)?)?;
    module.add_function(wrap_pyfunction!(rust_verify_classfile, module)?)?;
    module.add_function(wrap_pyfunction!(rust_verify_classmodel, module)?)?;
    Ok(())
}
