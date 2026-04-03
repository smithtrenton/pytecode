use pyo3::prelude::*;

pub mod attributes;
pub mod code;
pub mod constant_pool;
pub mod constants;
pub mod descriptors;
pub mod instructions;
pub mod modified_utf8;

/// Register classfile sub-modules on the parent module.
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent.py(), "classfile")?;
    code::register(&m)?;
    constant_pool::register(&m)?;
    descriptors::register(&m)?;
    modified_utf8::register(&m)?;
    crate::register_submodule(parent, &m, "pytecode._rust.classfile")?;
    Ok(())
}
