use std::collections::HashSet;
use std::fs::File;
use std::io::Write;

use pyo3::exceptions::{PyOSError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyModule};
use zip::result::ZipError;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, System, ZipArchive, ZipWriter};

fn zip_err_to_py(err: ZipError) -> PyErr {
    match err {
        ZipError::Io(io_err) => PyOSError::new_err(io_err.to_string()),
        other => PyValueError::new_err(other.to_string()),
    }
}

fn archive_name(filename: &str) -> String {
    if filename.ends_with('\\') || filename.ends_with('/') {
        format!(
            "{}/",
            filename.trim_end_matches(['\\', '/']).replace('\\', "/")
        )
    } else {
        filename.replace('\\', "/")
    }
}

fn is_class_filename(filename: &str) -> bool {
    !filename.ends_with('\\') && !filename.ends_with('/') && filename.ends_with(".class")
}

fn parse_extra_fields(extra: &[u8]) -> PyResult<Vec<(u16, Vec<u8>)>> {
    let mut fields = Vec::new();
    let mut offset = 0usize;
    while offset < extra.len() {
        if extra.len() - offset < 4 {
            return Err(PyValueError::new_err("ZIP extra field is truncated"));
        }
        let header_id = u16::from_le_bytes([extra[offset], extra[offset + 1]]);
        let data_len = u16::from_le_bytes([extra[offset + 2], extra[offset + 3]]) as usize;
        offset += 4;
        if extra.len() - offset < data_len {
            return Err(PyValueError::new_err(
                "ZIP extra field payload is truncated",
            ));
        }
        fields.push((header_id, extra[offset..offset + data_len].to_vec()));
        offset += data_len;
    }
    Ok(fields)
}

fn entry_options(
    zipinfo: &Bound<'_, PyAny>,
    archive_name: &str,
    is_dir: bool,
) -> PyResult<zip::write::FileOptions<'static, zip::write::ExtendedFileOptions>> {
    let compress_type: i32 = zipinfo.getattr("compress_type")?.extract()?;
    let compression_method = match compress_type {
        0 => CompressionMethod::Stored,
        8 => CompressionMethod::Deflated,
        other => {
            return Err(PyValueError::new_err(format!(
                "unsupported ZIP compression method for Rust rewrite: {other}"
            )));
        }
    };

    let (year, month, day, hour, minute, second): (u16, u8, u8, u8, u8, u8) =
        zipinfo.getattr("date_time")?.extract()?;
    let date_time = DateTime::from_date_and_time(year, month, day, hour, minute, second)
        .map_err(|_| PyValueError::new_err("invalid ZIP timestamp for Rust rewrite"))?;

    let create_system = zipinfo
        .getattr("create_system")?
        .extract::<u8>()
        .unwrap_or(u8::MAX);
    let external_attr: u32 = zipinfo.getattr("external_attr")?.extract()?;
    let comment: Vec<u8> = zipinfo.getattr("comment")?.extract()?;
    let extra: Vec<u8> = zipinfo.getattr("extra")?.extract()?;

    let mut options = SimpleFileOptions::default()
        .compression_method(compression_method)
        .last_modified_time(date_time)
        .system(System::from(create_system))
        .into_full_options();

    let high_bits = external_attr >> 16;
    if is_dir {
        if high_bits != 0o040755 {
            return Err(PyValueError::new_err(format!(
                "unsupported ZIP external_attr for Rust-written directory entry: {external_attr:#x}"
            )));
        }
        options = options.unix_permissions(0o755);
    } else if high_bits != 0 && high_bits != 0o100644 && high_bits <= 0o777 {
        options = options.unix_permissions(high_bits);
    } else if high_bits != 0 && high_bits != 0o100644 {
        return Err(PyValueError::new_err(format!(
            "unsupported ZIP external_attr for Rust-written file entry: {external_attr:#x}"
        )));
    }

    if !comment.is_empty() {
        let comment = String::from_utf8(comment)
            .map_err(|_| PyValueError::new_err("Rust rewrite requires UTF-8 ZIP entry comments"))?;
        options = options.with_file_comment(comment);
    }

    for (header_id, payload) in parse_extra_fields(&extra)? {
        options
            .add_extra_data(header_id, payload, false)
            .map_err(zip_err_to_py)?;
    }

    if archive_name.len() > u16::MAX as usize {
        return Err(PyValueError::new_err(
            "ZIP entry name is too long for Rust rewrite",
        ));
    }

    Ok(options)
}

#[allow(clippy::too_many_arguments)]
fn rewrite_class_bytes(
    py: Python<'_>,
    class_model_type: &Bound<'_, PyAny>,
    data: &[u8],
    transform: Option<&Py<PyAny>>,
    recompute_frames: bool,
    resolver: Option<&Py<PyAny>>,
    debug_policy: &Py<PyAny>,
    skip_debug: bool,
) -> PyResult<Vec<u8>> {
    let from_kwargs = PyDict::new(py);
    from_kwargs.set_item("skip_debug", skip_debug)?;
    let model = class_model_type.call_method(
        "from_bytes",
        (pyo3::types::PyBytes::new(py, data),),
        Some(&from_kwargs),
    )?;

    if let Some(transform) = transform {
        let result = transform.bind(py).call1((model.clone(),))?;
        if !result.is_none() {
            return Err(PyTypeError::new_err(
                "JarFile.rewrite() transforms must mutate ClassModel in place and return None",
            ));
        }
    }

    let to_kwargs = PyDict::new(py);
    to_kwargs.set_item("recompute_frames", recompute_frames)?;
    if let Some(resolver) = resolver {
        to_kwargs.set_item("resolver", resolver)?;
    } else {
        to_kwargs.set_item("resolver", py.None())?;
    }
    to_kwargs.set_item("debug_info", debug_policy)?;
    model
        .call_method("to_bytes", (), Some(&to_kwargs))?
        .extract::<Vec<u8>>()
}

#[pyfunction]
#[allow(clippy::too_many_arguments)]
fn write_archive(
    py: Python<'_>,
    source_path: &str,
    entries: Vec<Py<PyAny>>,
    output_path: &str,
    raw_copy_filenames: Vec<String>,
    should_rewrite_classes: bool,
    transform: Option<Py<PyAny>>,
    recompute_frames: bool,
    resolver: Option<Py<PyAny>>,
    debug_policy: Py<PyAny>,
    skip_debug: bool,
) -> PyResult<()> {
    let output = File::create(output_path).map_err(|err| PyOSError::new_err(err.to_string()))?;
    let mut writer = ZipWriter::new(output);
    let raw_copy_filenames: HashSet<String> = raw_copy_filenames.into_iter().collect();

    let mut source_archive = if raw_copy_filenames.is_empty() {
        None
    } else {
        let source = File::open(source_path).map_err(|err| PyOSError::new_err(err.to_string()))?;
        Some(ZipArchive::new(source).map_err(zip_err_to_py)?)
    };

    let class_model_type = py.import("pytecode.edit.model")?.getattr("ClassModel")?;

    for entry in entries {
        let jar_info = entry.bind(py);
        let filename: String = jar_info.getattr("filename")?.extract()?;
        let zipinfo = jar_info.getattr("zipinfo")?;
        let archive_name = archive_name(&filename);

        if raw_copy_filenames.contains(&filename) {
            let source_archive = source_archive
                .as_mut()
                .ok_or_else(|| PyValueError::new_err("missing source archive for raw ZIP copy"))?;
            let source_file = source_archive
                .by_name(&archive_name)
                .map_err(zip_err_to_py)?;
            writer
                .raw_copy_file_rename(source_file, &archive_name)
                .map_err(zip_err_to_py)?;
            continue;
        }

        let mut data: Vec<u8> = jar_info.getattr("bytes")?.extract()?;
        if should_rewrite_classes && is_class_filename(&filename) {
            data = rewrite_class_bytes(
                py,
                &class_model_type,
                &data,
                transform.as_ref(),
                recompute_frames,
                resolver.as_ref(),
                &debug_policy,
                skip_debug,
            )?;
        }

        let options = entry_options(
            &zipinfo,
            &archive_name,
            filename.ends_with('\\') || filename.ends_with('/'),
        )?;
        if filename.ends_with('\\') || filename.ends_with('/') {
            writer
                .add_directory(&archive_name, options)
                .map_err(zip_err_to_py)?;
        } else {
            writer
                .start_file(&archive_name, options)
                .map_err(zip_err_to_py)?;
            writer
                .write_all(&data)
                .map_err(|err| PyOSError::new_err(err.to_string()))?;
        }
    }

    writer.finish().map_err(zip_err_to_py)?;
    Ok(())
}

pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent.py(), "archive")?;
    m.add_function(wrap_pyfunction!(write_archive, &m)?)?;
    crate::register_submodule(parent, &m, "pytecode._rust.archive")?;
    Ok(())
}
