use pyo3::prelude::*;

mod binary_io;
mod classfile;

/// Register a submodule in `sys.modules` so it's importable via `from a.b import c`.
///
/// `full_name` must be the fully-qualified dotted path (e.g. `"pytecode._rust.binary_io"`).
fn register_submodule(
    parent: &Bound<'_, PyModule>,
    child: &Bound<'_, PyModule>,
    full_name: &str,
) -> PyResult<()> {
    let py = parent.py();
    let sys_modules = py.import("sys")?.getattr("modules")?;
    sys_modules.set_item(full_name, child)?;
    parent.add_submodule(child)?;
    Ok(())
}

/// Root Rust module exposed to Python as `pytecode._rust`.
///
/// Each sub-module is registered here and re-exported on the Python side
/// through thin wrappers that preserve the existing public API.
#[pymodule]
fn _rust(m: &Bound<'_, PyModule>) -> PyResult<()> {
    binary_io::register(m)?;
    classfile::register(m)?;
    Ok(())
}
