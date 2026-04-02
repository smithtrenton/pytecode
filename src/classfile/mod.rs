use pyo3::prelude::*;

pub mod modified_utf8;

/// Register classfile sub-modules on the parent module.
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent.py(), "classfile")?;
    modified_utf8::register(&m)?;
    crate::register_submodule(parent, &m, "pytecode._rust.classfile")?;
    Ok(())
}
