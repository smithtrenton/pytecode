use pyo3::exceptions::{PyOSError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes, PyModule};
use pyo3::wrap_pyfunction;
use pytecode_archive::{ArchiveError, JarEntryMetadata, JarFile, JarInfo, RewriteOptions};
use pytecode_engine::error::{EngineError, EngineErrorKind};
use pytecode_engine::model::ClassModel;
use pytecode_engine::transform::ApplyClassTransform;
use pytecode_engine::transform::pipeline_spec::CompiledPipeline;
use pytecode_engine::transform::transform_spec::ClassTransformSpec;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use zip::{CompressionMethod, DateTime, System};

use crate::model::{
    PyClassModel, PyMappingClassResolver, parse_debug_info_policy, parse_frame_computation_mode,
};
use crate::transforms::{PyClassTransform, PyCompiledPipeline, PyPipeline};

const CALLBACK_ABORT_REASON: &str = "__pytecode_archive_python_callback_failed__";
const PYTHON_TRANSFORM_RETURN_ERROR: &str =
    "JarFile.rewrite() Python transforms must mutate ClassModel in place and return None";

fn archive_error_to_py(error: ArchiveError) -> PyErr {
    match error {
        ArchiveError::Engine(inner) => crate::engine_error_to_py(inner),
        ArchiveError::EmptyFilename
        | ArchiveError::AbsolutePath(_)
        | ArchiveError::ParentTraversal(_)
        | ArchiveError::InvalidTimestamp(_)
        | ArchiveError::NonUtf8Comment(_) => PyValueError::new_err(error.to_string()),
        other => PyOSError::new_err(other.to_string()),
    }
}

#[pyclass(from_py_object, module = "pytecode._rust", name = "_ArchiveEntryState")]
#[derive(Clone)]
struct PyArchiveEntryState {
    filename: String,
    bytes: Vec<u8>,
    compression_method: u16,
    date_time: (u16, u8, u8, u8, u8, u8),
    system: u8,
    unix_mode: Option<u32>,
    is_dir: bool,
    comment: Vec<u8>,
    extra_data: Vec<u8>,
    original_index: Option<usize>,
}

#[pymethods]
impl PyArchiveEntryState {
    #[allow(clippy::too_many_arguments)]
    #[new]
    #[pyo3(signature = (filename, data, compression_method, date_time, *, system=255, unix_mode=None, is_dir=false, comment=None, extra_data=None, original_index=None))]
    fn new(
        filename: String,
        data: Vec<u8>,
        compression_method: u16,
        date_time: (u16, u8, u8, u8, u8, u8),
        system: u8,
        unix_mode: Option<u32>,
        is_dir: bool,
        comment: Option<Vec<u8>>,
        extra_data: Option<Vec<u8>>,
        original_index: Option<usize>,
    ) -> Self {
        Self {
            filename,
            bytes: data,
            compression_method,
            date_time,
            system,
            unix_mode,
            is_dir,
            comment: comment.unwrap_or_default(),
            extra_data: extra_data.unwrap_or_default(),
            original_index,
        }
    }
    #[getter]
    fn filename(&self) -> String {
        self.filename.clone()
    }

    #[getter]
    fn data<'py>(&self, py: Python<'py>) -> Py<PyBytes> {
        PyBytes::new(py, &self.bytes).unbind()
    }

    #[getter]
    fn compression_method(&self) -> u16 {
        self.compression_method
    }

    #[getter]
    fn date_time(&self) -> (u16, u8, u8, u8, u8, u8) {
        self.date_time
    }

    #[getter]
    fn system(&self) -> u8 {
        self.system
    }

    #[getter]
    fn unix_mode(&self) -> Option<u32> {
        self.unix_mode
    }

    #[getter]
    fn is_dir(&self) -> bool {
        self.is_dir
    }

    #[getter]
    fn comment<'py>(&self, py: Python<'py>) -> Py<PyBytes> {
        PyBytes::new(py, &self.comment).unbind()
    }

    #[getter]
    fn extra_data<'py>(&self, py: Python<'py>) -> Py<PyBytes> {
        PyBytes::new(py, &self.extra_data).unbind()
    }

    #[getter]
    fn original_index(&self) -> Option<usize> {
        self.original_index
    }
}

impl PyArchiveEntryState {
    fn from_jar_info(info: JarInfo) -> PyResult<Self> {
        let original_index = info.original_index();
        let JarInfo {
            filename,
            bytes,
            metadata,
            ..
        } = info;
        let compression_method = match metadata.compression_method {
            CompressionMethod::Stored => 0,
            CompressionMethod::Deflated => 8,
            other => {
                return Err(PyValueError::new_err(format!(
                    "unsupported ZIP compression method for archive read: {other:?}"
                )));
            }
        };
        let last_modified = metadata.last_modified;
        Ok(Self {
            filename,
            bytes,
            compression_method,
            date_time: (
                last_modified.year(),
                last_modified.month(),
                last_modified.day(),
                last_modified.hour(),
                last_modified.minute(),
                last_modified.second(),
            ),
            system: 255,
            unix_mode: metadata.unix_mode,
            is_dir: metadata.is_dir,
            comment: metadata.comment,
            extra_data: metadata.extra_data,
            original_index,
        })
    }

    fn to_jar_info(&self) -> PyResult<JarInfo> {
        let compression_method = match self.compression_method {
            0 => CompressionMethod::Stored,
            8 => CompressionMethod::Deflated,
            other => {
                return Err(PyValueError::new_err(format!(
                    "unsupported ZIP compression method for archive entry state: {other}"
                )));
            }
        };
        let last_modified = DateTime::from_date_and_time(
            self.date_time.0,
            self.date_time.1,
            self.date_time.2,
            self.date_time.3,
            self.date_time.4,
            self.date_time.5,
        )
        .map_err(|_| {
            PyValueError::new_err(format!(
                "archive entry timestamp is out of ZIP range: {}",
                self.filename
            ))
        })?;
        Ok(JarInfo::new(
            self.filename.clone(),
            self.bytes.clone(),
            JarEntryMetadata {
                compression_method,
                last_modified,
                unix_mode: self.unix_mode,
                system: System::from(self.system),
                comment: self.comment.clone(),
                extra_data: self.extra_data.clone(),
                is_dir: self.is_dir,
            },
            self.original_index,
        ))
    }
}

struct CompiledPipelineArchiveTransform<'a> {
    inner: &'a CompiledPipeline,
}

impl ApplyClassTransform for CompiledPipelineArchiveTransform<'_> {
    fn apply(&mut self, model: &mut ClassModel) -> pytecode_engine::Result<()> {
        self.inner.apply(model);
        Ok(())
    }
}

struct ClassTransformArchiveTransform<'a> {
    inner: &'a ClassTransformSpec,
}

impl ApplyClassTransform for ClassTransformArchiveTransform<'_> {
    fn apply(&mut self, model: &mut ClassModel) -> pytecode_engine::Result<()> {
        self.inner.apply(model);
        Ok(())
    }
}

struct CallbackCompiledPipelineArchiveTransform<'a> {
    inner: &'a PyCompiledPipeline,
    callback_error: Arc<Mutex<Option<PyErr>>>,
}

impl ApplyClassTransform for CallbackCompiledPipelineArchiveTransform<'_> {
    fn apply(&mut self, model: &mut ClassModel) -> pytecode_engine::Result<()> {
        match self.inner.apply_to_engine_model(model) {
            Ok(()) => Ok(()),
            Err(err) => {
                store_callback_error(&self.callback_error, err);
                Err(callback_abort_error())
            }
        }
    }
}

struct PythonCallableArchiveTransform {
    callback: Py<PyAny>,
    callback_error: Arc<Mutex<Option<PyErr>>>,
}

impl ApplyClassTransform for PythonCallableArchiveTransform {
    fn apply(&mut self, model: &mut ClassModel) -> pytecode_engine::Result<()> {
        let callback_result = Python::attach(|py| -> PyResult<bool> {
            let mut py_model = PyClassModel::from_model(std::mem::take(model));
            let call_result = (|| -> PyResult<bool> {
                let cell = Py::new(py, py_model.clone())?;
                Ok(self.callback.call1(py, (&cell,))?.is_none(py))
            })();
            let restore_result = py_model.take_inner();

            match (call_result, restore_result) {
                (Ok(is_none), Ok(inner)) => {
                    *model = inner;
                    Ok(is_none)
                }
                (Err(err), Ok(inner)) => {
                    *model = inner;
                    Err(err)
                }
                (Ok(_), Err(err)) | (Err(_), Err(err)) => Err(err),
            }
        });

        match callback_result {
            Ok(true) => Ok(()),
            Ok(false) => {
                store_callback_error(
                    &self.callback_error,
                    PyTypeError::new_err(PYTHON_TRANSFORM_RETURN_ERROR),
                );
                Err(callback_abort_error())
            }
            Err(err) => {
                store_callback_error(&self.callback_error, err);
                Err(callback_abort_error())
            }
        }
    }
}

fn callback_abort_error() -> EngineError {
    EngineError::new(
        0,
        EngineErrorKind::InvalidModelState {
            reason: CALLBACK_ABORT_REASON.to_owned(),
        },
    )
}

fn store_callback_error(slot: &Arc<Mutex<Option<PyErr>>>, err: PyErr) {
    let mut guard = slot.lock().expect("callback error mutex poisoned");
    if guard.is_none() {
        *guard = Some(err);
    }
}

fn take_callback_error(slot: &Arc<Mutex<Option<PyErr>>>) -> Option<PyErr> {
    slot.lock().expect("callback error mutex poisoned").take()
}

fn is_callback_abort(error: &ArchiveError) -> bool {
    matches!(
        error,
        ArchiveError::Engine(EngineError {
            offset: 0,
            kind: EngineErrorKind::InvalidModelState { reason },
        }) if reason == CALLBACK_ABORT_REASON
    )
}

fn rewrite_with_callback_transform(
    jar: &mut JarFile,
    transform: &mut dyn ApplyClassTransform,
    output_path: Option<PathBuf>,
    options: RewriteOptions<'_>,
    callback_error: &Arc<Mutex<Option<PyErr>>>,
) -> PyResult<PathBuf> {
    match jar.rewrite(output_path.as_deref(), Some(transform), options) {
        Ok(path) => {
            if let Some(err) = take_callback_error(callback_error) {
                return Err(err);
            }
            Ok(path)
        }
        Err(error) => {
            if is_callback_abort(&error)
                && let Some(err) = take_callback_error(callback_error)
            {
                return Err(err);
            }
            Err(archive_error_to_py(error))
        }
    }
}

fn rewrite_with_pipeline(
    jar: &mut JarFile,
    pipeline: PyRef<'_, PyPipeline>,
    output_path: Option<PathBuf>,
    options: RewriteOptions<'_>,
) -> PyResult<PathBuf> {
    if pipeline.contains_python_callbacks() {
        let compiled = pipeline.compile_internal();
        let callback_error = Arc::new(Mutex::new(None));
        let mut wrapped = CallbackCompiledPipelineArchiveTransform {
            inner: &compiled,
            callback_error: callback_error.clone(),
        };
        return rewrite_with_callback_transform(
            jar,
            &mut wrapped,
            output_path,
            options,
            &callback_error,
        );
    }

    let compiled = pipeline.spec.compile();
    let mut wrapped = CompiledPipelineArchiveTransform { inner: &compiled };
    jar.rewrite(output_path.as_deref(), Some(&mut wrapped), options)
        .map_err(archive_error_to_py)
}

fn rewrite_with_compiled_pipeline(
    jar: &mut JarFile,
    pipeline: PyRef<'_, PyCompiledPipeline>,
    output_path: Option<PathBuf>,
    options: RewriteOptions<'_>,
) -> PyResult<PathBuf> {
    if pipeline.contains_python_callbacks {
        let callback_error = Arc::new(Mutex::new(None));
        let mut wrapped = CallbackCompiledPipelineArchiveTransform {
            inner: &pipeline,
            callback_error: callback_error.clone(),
        };
        return rewrite_with_callback_transform(
            jar,
            &mut wrapped,
            output_path,
            options,
            &callback_error,
        );
    }

    let mut wrapped = CompiledPipelineArchiveTransform {
        inner: &pipeline.inner,
    };
    jar.rewrite(output_path.as_deref(), Some(&mut wrapped), options)
        .map_err(archive_error_to_py)
}

fn rewrite_with_python_callable(
    jar: &mut JarFile,
    transform: &Bound<'_, PyAny>,
    output_path: Option<PathBuf>,
    options: RewriteOptions<'_>,
) -> PyResult<PathBuf> {
    let callback_error = Arc::new(Mutex::new(None));
    let mut wrapped = PythonCallableArchiveTransform {
        callback: transform.clone().unbind(),
        callback_error: callback_error.clone(),
    };
    rewrite_with_callback_transform(jar, &mut wrapped, output_path, options, &callback_error)
}

fn rewrite_options<'a>(
    frame_mode: Option<&Bound<'_, PyAny>>,
    resolver: Option<&'a PyMappingClassResolver>,
    debug_info: &str,
) -> PyResult<RewriteOptions<'a>> {
    let debug_info = parse_debug_info_policy(debug_info)?;
    let frame_mode = parse_frame_computation_mode(frame_mode)?;
    Ok(RewriteOptions {
        frame_mode,
        resolver: resolver
            .map(|value| &value.inner as &dyn pytecode_engine::analysis::ClassResolver),
        debug_info,
    })
}

fn rewrite_with_transform(
    jar: &mut JarFile,
    transform: &Bound<'_, PyAny>,
    output_path: Option<PathBuf>,
    options: RewriteOptions<'_>,
) -> PyResult<PathBuf> {
    if let Ok(owner) = transform.getattr("__self__")
        && !owner.is_none()
    {
        if let Ok(pipeline) = owner.extract::<PyRef<'_, PyPipeline>>() {
            return rewrite_with_pipeline(jar, pipeline, output_path, options);
        }
        if let Ok(pipeline) = owner.extract::<PyRef<'_, PyCompiledPipeline>>() {
            return rewrite_with_compiled_pipeline(jar, pipeline, output_path, options);
        }
    }

    if let Ok(pipeline) = transform.extract::<PyRef<'_, PyPipeline>>() {
        return rewrite_with_pipeline(jar, pipeline, output_path, options);
    }

    if let Ok(pipeline) = transform.extract::<PyRef<'_, PyCompiledPipeline>>() {
        return rewrite_with_compiled_pipeline(jar, pipeline, output_path, options);
    }

    if let Ok(transform) = transform.extract::<PyRef<'_, PyClassTransform>>() {
        let mut wrapped = ClassTransformArchiveTransform {
            inner: &transform.spec,
        };
        return jar
            .rewrite(output_path.as_deref(), Some(&mut wrapped), options)
            .map_err(archive_error_to_py);
    }

    if transform.is_callable() {
        return rewrite_with_python_callable(jar, transform, output_path, options);
    }

    Err(PyTypeError::new_err(
        "rewrite_archive_with_rust_transform() expected a callable, ClassTransform, Pipeline, CompiledPipeline, or bound .apply method",
    ))
}

fn jar_from_state(
    py: Python<'_>,
    source_path: PathBuf,
    entries: Vec<Py<PyArchiveEntryState>>,
) -> PyResult<JarFile> {
    let mut jar_entries = Vec::with_capacity(entries.len());
    for entry in entries {
        let entry = entry.borrow(py);
        jar_entries.push(entry.to_jar_info()?);
    }
    Ok(JarFile::from_entries(source_path, jar_entries))
}

#[pyfunction]
#[pyo3(signature = (source_path, transform, output_path=None, frame_mode=None, resolver=None, debug_info="preserve"))]
fn rewrite_archive_with_rust_transform(
    source_path: PathBuf,
    transform: &Bound<'_, PyAny>,
    output_path: Option<PathBuf>,
    frame_mode: Option<&Bound<'_, PyAny>>,
    resolver: Option<&PyMappingClassResolver>,
    debug_info: &str,
) -> PyResult<PathBuf> {
    let options = rewrite_options(frame_mode, resolver, debug_info)?;
    let mut jar = JarFile::open(&source_path).map_err(archive_error_to_py)?;
    rewrite_with_transform(&mut jar, transform, output_path, options)
}

#[pyfunction]
#[pyo3(signature = (source_path, entries, transform=None, output_path=None, frame_mode=None, resolver=None, debug_info="preserve"))]
#[allow(clippy::too_many_arguments)]
fn rewrite_archive_state(
    py: Python<'_>,
    source_path: PathBuf,
    entries: Vec<Py<PyArchiveEntryState>>,
    transform: Option<&Bound<'_, PyAny>>,
    output_path: Option<PathBuf>,
    frame_mode: Option<&Bound<'_, PyAny>>,
    resolver: Option<&PyMappingClassResolver>,
    debug_info: &str,
) -> PyResult<PathBuf> {
    let options = rewrite_options(frame_mode, resolver, debug_info)?;
    let mut jar = jar_from_state(py, source_path, entries)?;
    if let Some(transform) = transform {
        rewrite_with_transform(&mut jar, transform, output_path, options)
    } else {
        jar.rewrite(output_path.as_deref(), None, options)
            .map_err(archive_error_to_py)
    }
}

#[pyfunction]
fn read_archive_state(source_path: PathBuf) -> PyResult<Vec<PyArchiveEntryState>> {
    let jar = JarFile::open(&source_path).map_err(archive_error_to_py)?;
    jar.entries
        .into_iter()
        .map(PyArchiveEntryState::from_jar_info)
        .collect::<PyResult<Vec<_>>>()
}

pub(crate) fn register(_py: Python<'_>, module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyArchiveEntryState>()?;
    module.add_function(wrap_pyfunction!(read_archive_state, module)?)?;
    module.add_function(wrap_pyfunction!(
        rewrite_archive_with_rust_transform,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(rewrite_archive_state, module)?)?;
    Ok(())
}
