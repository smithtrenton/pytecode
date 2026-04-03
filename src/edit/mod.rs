mod constant_pool_builder;

use pyo3::prelude::*;

pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent.py(), "edit")?;
    constant_pool_builder::register(&m)?;
    crate::register_submodule(parent, &m, "pytecode._rust.edit")?;
    Ok(())
}
