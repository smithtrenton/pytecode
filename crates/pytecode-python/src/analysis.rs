use pyo3::prelude::*;
use pyo3::types::PyDict;
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

use crate::model::{PyClassModel, PyMappingClassResolver};

fn analysis_error_to_py(error: AnalysisError) -> PyErr {
    PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(error.to_string())
}

fn resolved_method_to_py(py: Python<'_>, method: &ResolvedMethod) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    dict.set_item("name", &method.name)?;
    dict.set_item("descriptor", &method.descriptor)?;
    dict.set_item("access_flags", method.access_flags.bits())?;
    Ok(dict.into_any().unbind())
}

fn resolved_class_to_py(py: Python<'_>, resolved: &ResolvedClass) -> PyResult<PyObject> {
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

// ---------------------------------------------------------------------------
// PyDiagnostic
// ---------------------------------------------------------------------------

#[pyclass(name = "RustDiagnostic")]
#[derive(Clone)]
pub struct PyDiagnostic {
    inner: Diagnostic,
}

#[pymethods]
impl PyDiagnostic {
    #[getter]
    fn severity(&self) -> &str {
        match self.inner.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        }
    }

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

    #[getter]
    fn message(&self) -> &str {
        &self.inner.message
    }

    #[getter]
    fn class_name(&self) -> Option<&str> {
        self.inner.location.class_name.as_deref()
    }

    #[getter]
    fn field_name(&self) -> Option<&str> {
        self.inner.location.field_name.as_deref()
    }

    #[getter]
    fn method_name(&self) -> Option<&str> {
        self.inner.location.method_name.as_deref()
    }

    #[getter]
    fn method_descriptor(&self) -> Option<&str> {
        self.inner.location.method_descriptor.as_deref()
    }

    #[getter]
    fn attribute_name(&self) -> Option<&str> {
        self.inner.location.attribute_name.as_deref()
    }

    #[getter]
    fn cp_index(&self) -> Option<u16> {
        self.inner.location.cp_index
    }

    #[getter]
    fn code_index(&self) -> Option<usize> {
        self.inner.location.code_index
    }

    fn __repr__(&self) -> String {
        format!(
            "RustDiagnostic(severity={:?}, category={:?}, message={:?})",
            self.severity(),
            self.category(),
            self.message()
        )
    }
}

// ---------------------------------------------------------------------------
// Top-level verify functions
// ---------------------------------------------------------------------------

#[pyfunction]
#[pyo3(signature = (data, *, fail_fast = false))]
fn rust_verify_classfile(data: &[u8], fail_fast: bool) -> PyResult<Vec<PyDiagnostic>> {
    let classfile = parse_class(data).map_err(crate::engine_error_to_py)?;
    let diagnostics = if fail_fast {
        verify_classfile_with_options(&classfile, true)
    } else {
        verify_classfile(&classfile)
    };
    Ok(diagnostics
        .into_iter()
        .map(|d| PyDiagnostic { inner: d })
        .collect())
}

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

#[pyfunction]
fn rust_resolved_classfile(py: Python<'_>, data: &[u8]) -> PyResult<PyObject> {
    let classfile = parse_class(data).map_err(crate::engine_error_to_py)?;
    let resolved = ResolvedClass::from_classfile(&classfile).map_err(crate::engine_error_to_py)?;
    resolved_class_to_py(py, &resolved)
}

#[pyfunction]
fn rust_resolved_classmodel(py: Python<'_>, model: &PyClassModel) -> PyResult<PyObject> {
    let resolved = model.with_class_model(|inner| Ok(ResolvedClass::from_model(inner)))?;
    resolved_class_to_py(py, &resolved)
}

#[pyfunction]
#[pyo3(signature = (resolver, class_name, *, include_self = false))]
fn rust_iter_superclasses(
    py: Python<'_>,
    resolver: &PyMappingClassResolver,
    class_name: &str,
    include_self: bool,
) -> PyResult<Vec<PyObject>> {
    use pytecode_engine::analysis::ClassResolver;

    let mut out = Vec::new();
    if include_self {
        let resolved = resolver.inner.resolve_class(class_name).ok_or_else(|| {
            analysis_error_to_py(AnalysisError::UnresolvedClass {
                class_name: class_name.to_owned(),
            })
        })?;
        out.push(resolved_class_to_py(py, &resolved)?);
    }
    out.extend(
        engine_iter_superclasses(&resolver.inner, class_name)
            .map_err(analysis_error_to_py)?
            .iter()
            .map(|resolved| resolved_class_to_py(py, resolved))
            .collect::<PyResult<Vec<_>>>()?,
    );
    Ok(out)
}

#[pyfunction]
#[pyo3(signature = (resolver, class_name, *, include_self = false))]
fn rust_iter_supertypes(
    py: Python<'_>,
    resolver: &PyMappingClassResolver,
    class_name: &str,
    include_self: bool,
) -> PyResult<Vec<PyObject>> {
    use pytecode_engine::analysis::ClassResolver;

    let mut out = Vec::new();
    if include_self {
        let resolved = resolver.inner.resolve_class(class_name).ok_or_else(|| {
            analysis_error_to_py(AnalysisError::UnresolvedClass {
                class_name: class_name.to_owned(),
            })
        })?;
        out.push(resolved_class_to_py(py, &resolved)?);
    }
    out.extend(
        engine_iter_supertypes(&resolver.inner, class_name)
            .map_err(analysis_error_to_py)?
            .iter()
            .map(|resolved| resolved_class_to_py(py, resolved))
            .collect::<PyResult<Vec<_>>>()?,
    );
    Ok(out)
}

#[pyfunction]
fn rust_is_subtype(
    resolver: &PyMappingClassResolver,
    class_name: &str,
    super_name: &str,
) -> PyResult<bool> {
    engine_is_subtype(&resolver.inner, class_name, super_name).map_err(analysis_error_to_py)
}

#[pyfunction]
fn rust_common_superclass(
    resolver: &PyMappingClassResolver,
    left: &str,
    right: &str,
) -> PyResult<String> {
    engine_common_superclass(&resolver.inner, left, right).map_err(analysis_error_to_py)
}

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
        .map_err(analysis_error_to_py)?
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
