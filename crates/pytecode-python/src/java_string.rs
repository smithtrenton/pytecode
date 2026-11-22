use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes, PyString};
use pytecode_engine::modified_utf8::JavaString;

pub(crate) fn from_python(value: &Bound<'_, PyString>) -> PyResult<JavaString> {
    if let Ok(text) = value.to_str() {
        return Ok(JavaString::from_utf16(text.encode_utf16().collect()));
    }
    let bytes: Vec<u8> = value
        .py()
        .get_type::<PyString>()
        .call_method1("encode", (value, "utf-16-le", "surrogatepass"))?
        .extract()?;
    Ok(JavaString::from_utf16(
        bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect(),
    ))
}

pub(crate) fn to_python(py: Python<'_>, value: &JavaString) -> PyResult<Py<PyAny>> {
    if let Ok(text) = String::from_utf16(value.as_utf16()) {
        return Ok(PyString::new(py, &text).into_any().unbind());
    }
    let bytes: Vec<u8> = value
        .as_utf16()
        .iter()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();
    Ok(PyBytes::new(py, &bytes)
        .call_method1("decode", ("utf-16-le", "surrogatepass"))?
        .unbind())
}
