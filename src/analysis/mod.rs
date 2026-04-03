mod hierarchy;
mod verify;

use pyo3::prelude::*;

pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent.py(), "analysis")?;
    hierarchy::register(&m)?;
    verify::register(&m)?;
    crate::register_submodule(parent, &m, "pytecode._rust.analysis")?;
    Ok(())
}
