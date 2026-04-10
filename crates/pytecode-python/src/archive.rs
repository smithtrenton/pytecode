use pyo3::exceptions::{PyNotImplementedError, PyOSError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyModule};
use pyo3::wrap_pyfunction;
use pytecode_archive::{ArchiveError, JarFile, RewriteOptions};
use pytecode_engine::model::{ClassModel, FrameComputationMode};
use pytecode_engine::transform::ApplyClassTransform;
use pytecode_engine::transform::pipeline_spec::CompiledPipeline;
use pytecode_engine::transform::transform_spec::ClassTransformSpec;
use std::path::PathBuf;

use crate::model::{PyMappingClassResolver, parse_debug_info_policy};
use crate::transforms::{PyClassTransform, PyCompiledPipeline, PyPipeline};

fn archive_error_to_py(error: ArchiveError) -> PyErr {
    match error {
        ArchiveError::Engine(inner) => crate::engine_error_to_py(inner),
        other => PyOSError::new_err(other.to_string()),
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

#[pyfunction]
#[pyo3(signature = (source_path, transform, output_path=None, recompute_frames=false, resolver=None, debug_info="preserve"))]
fn rewrite_archive_with_rust_transform(
    source_path: PathBuf,
    transform: &Bound<'_, PyAny>,
    output_path: Option<PathBuf>,
    recompute_frames: bool,
    resolver: Option<&PyMappingClassResolver>,
    debug_info: &str,
) -> PyResult<PathBuf> {
    let debug_info = parse_debug_info_policy(debug_info)?;
    let frame_mode = if recompute_frames {
        FrameComputationMode::Recompute
    } else {
        FrameComputationMode::Preserve
    };
    let options = RewriteOptions {
        frame_mode,
        resolver: resolver
            .map(|value| &value.inner as &dyn pytecode_engine::analysis::ClassResolver),
        debug_info,
    };
    let mut jar = JarFile::open(&source_path).map_err(archive_error_to_py)?;

    if let Ok(pipeline) = transform.extract::<PyRef<'_, PyPipeline>>() {
        if pipeline.has_python_callbacks() {
            return Err(PyNotImplementedError::new_err(
                "Rust archive rewrite does not support Python callback pipeline steps",
            ));
        }
        let compiled = pipeline.spec.compile();
        let mut wrapped = CompiledPipelineArchiveTransform { inner: &compiled };
        return jar
            .rewrite(output_path.as_deref(), Some(&mut wrapped), options)
            .map_err(archive_error_to_py);
    }

    if let Ok(pipeline) = transform.extract::<PyRef<'_, PyCompiledPipeline>>() {
        if pipeline.contains_python_callbacks {
            return Err(PyNotImplementedError::new_err(
                "Rust archive rewrite does not support Python callback pipeline steps",
            ));
        }
        let mut wrapped = CompiledPipelineArchiveTransform {
            inner: &pipeline.inner,
        };
        return jar
            .rewrite(output_path.as_deref(), Some(&mut wrapped), options)
            .map_err(archive_error_to_py);
    }

    if let Ok(transform) = transform.extract::<PyRef<'_, PyClassTransform>>() {
        let mut wrapped = ClassTransformArchiveTransform {
            inner: &transform.spec,
        };
        return jar
            .rewrite(output_path.as_deref(), Some(&mut wrapped), options)
            .map_err(archive_error_to_py);
    }

    Err(PyValueError::new_err(
        "rewrite_archive_with_rust_transform() expected a RustClassTransform, RustPipeline, or RustCompiledPipeline",
    ))
}

pub(crate) fn register(_py: Python<'_>, module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(
        rewrite_archive_with_rust_transform,
        module
    )?)?;
    Ok(())
}
