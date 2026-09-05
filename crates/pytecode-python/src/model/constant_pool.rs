//! Python views of owned and class-owned constant pools.
use super::{ConstantPoolAccess, PyObject, SharedClassState, with_live_model, with_live_model_mut};
use pyo3::prelude::*;
use pyo3::types::PyString;
use pytecode_engine::model::ConstantPoolBuilder;

// ---------------------------------------------------------------------------
// PyConstantPoolBuilder
// ---------------------------------------------------------------------------

#[pyclass(
    from_py_object,
    name = "ConstantPoolBuilder",
    module = "pytecode._rust",
    unsendable
)]
#[derive(Clone)]
pub struct PyConstantPoolBuilder {
    access: ConstantPoolAccess,
}

impl PyConstantPoolBuilder {
    pub(super) fn from_shared(state: SharedClassState) -> Self {
        Self {
            access: ConstantPoolAccess::Shared(state),
        }
    }

    fn with_builder<R>(&self, f: impl FnOnce(&ConstantPoolBuilder) -> PyResult<R>) -> PyResult<R> {
        match &self.access {
            ConstantPoolAccess::Owned(builder) => f(builder),
            ConstantPoolAccess::Shared(state) => with_live_model(state, |model_state| {
                f(&model_state.inner.as_ref().unwrap().constant_pool)
            }),
        }
    }

    fn with_builder_mut<R>(
        &mut self,
        f: impl FnOnce(&mut ConstantPoolBuilder) -> PyResult<R>,
    ) -> PyResult<R> {
        match &mut self.access {
            ConstantPoolAccess::Owned(builder) => f(builder),
            ConstantPoolAccess::Shared(state) => with_live_model_mut(state, |model_state| {
                f(&mut model_state.inner.as_mut().unwrap().constant_pool)
            }),
        }
    }
}

#[pymethods]
impl PyConstantPoolBuilder {
    #[new]
    fn new() -> Self {
        Self {
            access: ConstantPoolAccess::Owned(ConstantPoolBuilder::new()),
        }
    }

    fn add_utf8(&mut self, value: &Bound<'_, PyString>) -> PyResult<u16> {
        let value = crate::java_string::from_python(value)?;
        self.with_builder_mut(|builder| {
            builder
                .add_utf16(&value)
                .map(|idx| idx.into())
                .map_err(crate::engine_error_to_py)
        })
    }

    fn add_class(&mut self, name: &str) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_class(name)
                .map(|idx| idx.into())
                .map_err(crate::engine_error_to_py)
        })
    }

    fn add_string(&mut self, value: &Bound<'_, PyString>) -> PyResult<u16> {
        let value = crate::java_string::from_python(value)?;
        self.with_builder_mut(|builder| {
            builder
                .add_java_string(&value)
                .map(|idx| idx.into())
                .map_err(crate::engine_error_to_py)
        })
    }

    fn add_integer(&mut self, value: u32) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_integer(value)
                .map(|idx| idx.into())
                .map_err(crate::engine_error_to_py)
        })
    }

    fn add_long(&mut self, high: u32, low: u32) -> PyResult<u16> {
        let value = ((high as u64) << 32) | (low as u64);
        self.with_builder_mut(|builder| {
            builder
                .add_long(value)
                .map(|idx| idx.into())
                .map_err(crate::engine_error_to_py)
        })
    }

    fn add_float_bits(&mut self, raw_bits: u32) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_float_bits(raw_bits)
                .map(|idx| idx.into())
                .map_err(crate::engine_error_to_py)
        })
    }

    fn add_float(&mut self, raw_bits: u32) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_float_bits(raw_bits)
                .map(|idx| idx.into())
                .map_err(crate::engine_error_to_py)
        })
    }

    fn add_double_bits(&mut self, raw_bits: u64) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_double_bits(raw_bits)
                .map(|idx| idx.into())
                .map_err(crate::engine_error_to_py)
        })
    }

    fn add_double(&mut self, high: u32, low: u32) -> PyResult<u16> {
        let raw_bits = ((high as u64) << 32) | (low as u64);
        self.with_builder_mut(|builder| {
            builder
                .add_double_bits(raw_bits)
                .map(|idx| idx.into())
                .map_err(crate::engine_error_to_py)
        })
    }

    fn add_field_ref(&mut self, owner: &str, name: &str, descriptor: &str) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_field_ref(owner, name, descriptor)
                .map(|idx| idx.into())
                .map_err(crate::engine_error_to_py)
        })
    }

    fn add_method_ref(&mut self, owner: &str, name: &str, descriptor: &str) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_method_ref(owner, name, descriptor)
                .map(|idx| idx.into())
                .map_err(crate::engine_error_to_py)
        })
    }

    fn add_interface_method_ref(
        &mut self,
        owner: &str,
        name: &str,
        descriptor: &str,
    ) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_interface_method_ref(owner, name, descriptor)
                .map(|idx| idx.into())
                .map_err(crate::engine_error_to_py)
        })
    }

    fn add_fieldref(&mut self, owner: &str, name: &str, descriptor: &str) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_field_ref(owner, name, descriptor)
                .map(|idx| idx.into())
                .map_err(crate::engine_error_to_py)
        })
    }

    fn add_methodref(&mut self, owner: &str, name: &str, descriptor: &str) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_method_ref(owner, name, descriptor)
                .map(|idx| idx.into())
                .map_err(crate::engine_error_to_py)
        })
    }

    fn add_interface_methodref(
        &mut self,
        owner: &str,
        name: &str,
        descriptor: &str,
    ) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_interface_method_ref(owner, name, descriptor)
                .map(|idx| idx.into())
                .map_err(crate::engine_error_to_py)
        })
    }

    fn add_method_type(&mut self, descriptor: &str) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_method_type(descriptor)
                .map(|idx| idx.into())
                .map_err(crate::engine_error_to_py)
        })
    }

    fn add_method_handle(&mut self, reference_kind: u8, reference_index: u16) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_method_handle(
                    reference_kind,
                    pytecode_engine::indexes::CpIndex::from(reference_index),
                )
                .map(|idx| idx.into())
                .map_err(crate::engine_error_to_py)
        })
    }

    fn add_invoke_dynamic(
        &mut self,
        bootstrap_idx: u16,
        name: &str,
        descriptor: &str,
    ) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_invoke_dynamic(
                    pytecode_engine::indexes::BootstrapMethodIndex::from(bootstrap_idx),
                    name,
                    descriptor,
                )
                .map(|idx| idx.into())
                .map_err(crate::engine_error_to_py)
        })
    }

    fn add_dynamic(&mut self, bootstrap_idx: u16, name: &str, descriptor: &str) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_dynamic(
                    pytecode_engine::indexes::BootstrapMethodIndex::from(bootstrap_idx),
                    name,
                    descriptor,
                )
                .map(|idx| idx.into())
                .map_err(crate::engine_error_to_py)
        })
    }

    fn resolve_utf8(&self, py: Python<'_>, index: u16) -> PyResult<PyObject> {
        let value = self.with_builder(|builder| {
            builder
                .resolve_java_string(pytecode_engine::indexes::Utf8Index::from(index))
                .map_err(crate::engine_error_to_py)
        })?;
        crate::java_string::to_python(py, &value)
    }

    fn resolve_class_name(&self, index: u16) -> PyResult<String> {
        self.with_builder(|builder| {
            builder
                .resolve_class_name(pytecode_engine::indexes::ClassIndex::from(index))
                .map_err(crate::engine_error_to_py)
        })
    }

    fn count(&self) -> PyResult<u16> {
        self.with_builder(|builder| Ok(builder.count()))
    }

    fn len(&self) -> PyResult<usize> {
        self.with_builder(|builder| Ok(builder.len()))
    }

    fn raw_constant_pool(&self, py: Python<'_>) -> PyResult<Vec<Option<PyObject>>> {
        self.with_builder(|builder| {
            builder
                .entries()
                .iter()
                .enumerate()
                .map(|(index, entry)| match entry {
                    Some(e) => crate::constant_pool_entry_to_pyobject(py, index, e).map(Some),
                    None => Ok(None),
                })
                .collect()
        })
    }

    fn checkpoint(&self) -> PyResult<usize> {
        self.with_builder(|builder| Ok(builder.len()))
    }

    fn rollback(&mut self, checkpoint: usize) -> PyResult<()> {
        self.with_builder_mut(|builder| {
            builder.truncate(checkpoint);
            Ok(())
        })
    }

    fn find_integer(&self, value: u32) -> PyResult<Option<u16>> {
        self.with_builder(|builder| Ok(builder.find_integer(value).map(|idx| idx.into())))
    }

    fn find_float(&self, raw_bits: u32) -> PyResult<Option<u16>> {
        self.with_builder(|builder| Ok(builder.find_float_bits(raw_bits).map(|idx| idx.into())))
    }

    fn find_long(&self, high: u32, low: u32) -> PyResult<Option<u16>> {
        self.with_builder(|builder| Ok(builder.find_long(high, low).map(|idx| idx.into())))
    }

    fn find_double(&self, high: u32, low: u32) -> PyResult<Option<u16>> {
        let raw_bits = ((high as u64) << 32) | (low as u64);
        self.with_builder(|builder| Ok(builder.find_double_bits(raw_bits).map(|idx| idx.into())))
    }

    fn find_string(&self, value: &str) -> PyResult<Option<u16>> {
        self.with_builder(|builder| Ok(builder.find_string(value).map(|idx| idx.into())))
    }

    fn find_class(&self, name: &str) -> PyResult<Option<u16>> {
        self.with_builder(|builder| Ok(builder.find_class(name).map(|idx| idx.into())))
    }

    fn find_method_type(&self, descriptor: &str) -> PyResult<Option<u16>> {
        self.with_builder(|builder| Ok(builder.find_method_type(descriptor).map(|idx| idx.into())))
    }

    fn find_fieldref(&self, owner: &str, name: &str, descriptor: &str) -> PyResult<Option<u16>> {
        self.with_builder(|builder| {
            Ok(builder
                .find_field_ref(owner, name, descriptor)
                .map(|idx| idx.into()))
        })
    }

    fn find_methodref(&self, owner: &str, name: &str, descriptor: &str) -> PyResult<Option<u16>> {
        self.with_builder(|builder| {
            Ok(builder
                .find_method_ref(owner, name, descriptor)
                .map(|idx| idx.into()))
        })
    }

    fn find_interface_methodref(
        &self,
        owner: &str,
        name: &str,
        descriptor: &str,
    ) -> PyResult<Option<u16>> {
        self.with_builder(|builder| {
            Ok(builder
                .find_interface_method_ref(owner, name, descriptor)
                .map(|idx| idx.into()))
        })
    }

    fn find_method_handle(
        &self,
        reference_kind: u8,
        reference_index: u16,
    ) -> PyResult<Option<u16>> {
        self.with_builder(|builder| {
            Ok(builder
                .find_method_handle(
                    reference_kind,
                    pytecode_engine::indexes::CpIndex::from(reference_index),
                )
                .map(|idx| idx.into()))
        })
    }

    fn find_dynamic(
        &self,
        bootstrap_method_attr_index: u16,
        name: &str,
        descriptor: &str,
    ) -> PyResult<Option<u16>> {
        self.with_builder(|builder| {
            Ok(builder
                .find_dynamic(
                    pytecode_engine::indexes::BootstrapMethodIndex::from(
                        bootstrap_method_attr_index,
                    ),
                    name,
                    descriptor,
                )
                .map(|idx| idx.into()))
        })
    }
}
