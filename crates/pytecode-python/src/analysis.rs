use pyo3::prelude::*;
use pytecode_engine::analysis::{
    Category, Diagnostic, Severity, verify_classfile, verify_classfile_with_options,
    verify_classmodel, verify_classmodel_with_options,
};

use crate::model::{PyClassModel, PyMappingClassResolver};

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
    let classfile = pytecode_engine::parse_class(data).map_err(crate::engine_error_to_py)?;
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
        verify_classmodel_with_options(
            &model.inner,
            resolver.map(|r| &r.inner as &dyn pytecode_engine::analysis::ClassResolver),
            true,
        )
    } else {
        verify_classmodel(
            &model.inner,
            resolver.map(|r| &r.inner as &dyn pytecode_engine::analysis::ClassResolver),
        )
    };
    Ok(diagnostics
        .into_iter()
        .map(|d| PyDiagnostic { inner: d })
        .collect())
}

// ---------------------------------------------------------------------------
// register
// ---------------------------------------------------------------------------

pub(crate) fn register(_py: Python<'_>, module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyDiagnostic>()?;
    module.add_function(wrap_pyfunction!(rust_verify_classfile, module)?)?;
    module.add_function(wrap_pyfunction!(rust_verify_classmodel, module)?)?;
    Ok(())
}
