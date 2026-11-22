//! Python views of owned and class-owned constant pools.
use super::{ConstantPoolAccess, PyObject, SharedClassState, with_live_model, with_live_model_mut};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyString;
use pytecode_engine::indexes::{BootstrapMethodIndex, ClassIndex, CpIndex, Utf8Index};
use pytecode_engine::model::ConstantPoolBuilder;

// ---------------------------------------------------------------------------
// PyConstantPoolBuilder
// ---------------------------------------------------------------------------

/// Mutable constant-pool builder used directly or via ``ClassModel.constant_pool``.
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

    pub(super) fn with_builder<R>(
        &self,
        f: impl FnOnce(&ConstantPoolBuilder) -> PyResult<R>,
    ) -> PyResult<R> {
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

    /// Intern a ``CONSTANT_Utf8_info`` entry and return its index.
    fn add_utf8(&mut self, value: &Bound<'_, PyString>) -> PyResult<u16> {
        let value = crate::java_string::from_python(value)?;
        self.with_builder_mut(|builder| {
            builder
                .add_utf16(&value)
                .map(u16::from)
                .map_err(crate::engine_error_to_py)
        })
    }

    /// Intern a ``CONSTANT_Class_info`` entry and return its index.
    fn add_class(&mut self, name: &str) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_class(name)
                .map(u16::from)
                .map_err(crate::engine_error_to_py)
        })
    }

    /// Intern a ``CONSTANT_String_info`` entry and return its index.
    fn add_string(&mut self, value: &Bound<'_, PyString>) -> PyResult<u16> {
        let value = crate::java_string::from_python(value)?;
        self.with_builder_mut(|builder| {
            builder
                .add_java_string(&value)
                .map(u16::from)
                .map_err(crate::engine_error_to_py)
        })
    }

    /// Intern a ``CONSTANT_Integer_info`` entry from a signed 32-bit int.
    fn add_integer(&mut self, value: i32) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_integer(value)
                .map(u16::from)
                .map_err(crate::engine_error_to_py)
        })
    }

    /// Intern a ``CONSTANT_Long_info`` entry from a signed 64-bit integer.
    fn add_long(&mut self, value: i64) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_long(value)
                .map(u16::from)
                .map_err(crate::engine_error_to_py)
        })
    }

    /// Intern a ``CONSTANT_Float_info`` entry from raw IEEE 754 bit representation.
    fn add_float_bits(&mut self, raw_bits: u32) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_float_bits(raw_bits)
                .map(u16::from)
                .map_err(crate::engine_error_to_py)
        })
    }

    /// Compatibility alias for ``add_float_bits``.
    fn add_float(&mut self, raw_bits: u32) -> PyResult<u16> {
        self.add_float_bits(raw_bits)
    }

    /// Intern a ``CONSTANT_Float_info`` entry from a Python float value.
    fn add_float_value(&mut self, value: f32) -> PyResult<u16> {
        self.add_float_bits(value.to_bits())
    }

    /// Intern a ``CONSTANT_Double_info`` entry from raw 64-bit IEEE 754 bits.
    fn add_double_bits(&mut self, raw_bits: u64) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_double_bits(raw_bits)
                .map(u16::from)
                .map_err(crate::engine_error_to_py)
        })
    }

    /// Intern a ``CONSTANT_Double_info`` entry from explicit high/low 32-bit words.
    fn add_double(&mut self, high: u32, low: u32) -> PyResult<u16> {
        let raw_bits = ((high as u64) << 32) | (low as u64);
        self.add_double_bits(raw_bits)
    }

    /// Intern a ``CONSTANT_Double_info`` entry from a Python float value.
    fn add_double_value(&mut self, value: f64) -> PyResult<u16> {
        self.add_double_bits(value.to_bits())
    }

    /// Intern a ``CONSTANT_NameAndType_info`` entry and return its index.
    fn add_name_and_type(&mut self, name: &str, descriptor: &str) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_name_and_type(name, descriptor)
                .map(u16::from)
                .map_err(crate::engine_error_to_py)
        })
    }

    /// Intern a ``CONSTANT_Fieldref_info`` entry and return its index.
    fn add_field_ref(&mut self, owner: &str, name: &str, descriptor: &str) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_field_ref(owner, name, descriptor)
                .map(u16::from)
                .map_err(crate::engine_error_to_py)
        })
    }

    /// Intern a ``CONSTANT_Methodref_info`` entry and return its index.
    fn add_method_ref(&mut self, owner: &str, name: &str, descriptor: &str) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_method_ref(owner, name, descriptor)
                .map(u16::from)
                .map_err(crate::engine_error_to_py)
        })
    }

    /// Intern a ``CONSTANT_InterfaceMethodref_info`` entry and return its index.
    fn add_interface_method_ref(
        &mut self,
        owner: &str,
        name: &str,
        descriptor: &str,
    ) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_interface_method_ref(owner, name, descriptor)
                .map(u16::from)
                .map_err(crate::engine_error_to_py)
        })
    }

    /// Alias for ``add_field_ref``.
    fn add_fieldref(&mut self, owner: &str, name: &str, descriptor: &str) -> PyResult<u16> {
        self.add_field_ref(owner, name, descriptor)
    }

    /// Alias for ``add_method_ref``.
    fn add_methodref(&mut self, owner: &str, name: &str, descriptor: &str) -> PyResult<u16> {
        self.add_method_ref(owner, name, descriptor)
    }

    /// Alias for ``add_interface_method_ref``.
    fn add_interface_methodref(
        &mut self,
        owner: &str,
        name: &str,
        descriptor: &str,
    ) -> PyResult<u16> {
        self.add_interface_method_ref(owner, name, descriptor)
    }

    /// Intern a ``CONSTANT_MethodType_info`` entry and return its index.
    fn add_method_type(&mut self, descriptor: &str) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_method_type(descriptor)
                .map(u16::from)
                .map_err(crate::engine_error_to_py)
        })
    }

    /// Intern a ``CONSTANT_MethodHandle_info`` entry and return its index.
    fn add_method_handle(&mut self, reference_kind: u8, reference_index: u16) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_method_handle(reference_kind, CpIndex::from(reference_index))
                .map(u16::from)
                .map_err(crate::engine_error_to_py)
        })
    }

    /// Intern a ``CONSTANT_InvokeDynamic_info`` entry and return its index.
    fn add_invoke_dynamic(
        &mut self,
        bootstrap_idx: u16,
        name: &str,
        descriptor: &str,
    ) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_invoke_dynamic(BootstrapMethodIndex::from(bootstrap_idx), name, descriptor)
                .map(u16::from)
                .map_err(crate::engine_error_to_py)
        })
    }

    /// Intern a ``CONSTANT_Dynamic_info`` entry and return its index.
    fn add_dynamic(&mut self, bootstrap_idx: u16, name: &str, descriptor: &str) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_dynamic(BootstrapMethodIndex::from(bootstrap_idx), name, descriptor)
                .map(u16::from)
                .map_err(crate::engine_error_to_py)
        })
    }

    /// Intern a ``CONSTANT_Module_info`` entry and return its index.
    fn add_module(&mut self, name: &str) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_module(name)
                .map(u16::from)
                .map_err(crate::engine_error_to_py)
        })
    }

    /// Intern a ``CONSTANT_Package_info`` entry and return its index.
    fn add_package(&mut self, name: &str) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_package(name)
                .map(u16::from)
                .map_err(crate::engine_error_to_py)
        })
    }

    /// Decode the ``CONSTANT_Utf8_info`` entry at ``index`` (preserving surrogate code units).
    fn resolve_utf8(&self, py: Python<'_>, index: u16) -> PyResult<PyObject> {
        let value = self.with_builder(|builder| {
            builder
                .resolve_java_string(Utf8Index::from(index))
                .map_err(crate::engine_error_to_py)
        })?;
        crate::java_string::to_python(py, &value)
    }

    /// Resolve the internal class name referenced by the ``CONSTANT_Class_info`` at ``index``.
    fn resolve_class_name(&self, index: u16) -> PyResult<String> {
        self.with_builder(|builder| {
            builder
                .resolve_class_name(ClassIndex::from(index))
                .map_err(crate::engine_error_to_py)
        })
    }

    /// Return the JVMS ``constant_pool_count`` (slot table length including slot 0).
    fn count(&self) -> PyResult<u16> {
        self.with_builder(|builder| Ok(builder.count()))
    }

    /// Return the number of occupied entries (excluding slot 0 and wide-entry placeholders).
    fn len(&self) -> PyResult<usize> {
        self.with_builder(|builder| Ok(builder.len()))
    }

    fn __len__(&self) -> PyResult<usize> {
        self.len()
    }

    /// Materialize the constant-pool slots as raw wrapper objects (with ``None`` gaps).
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

    /// Capture an opaque slot-table length token that can later be passed to ``rollback``.
    fn checkpoint(&self) -> PyResult<usize> {
        self.with_builder(|builder| Ok(builder.entries().len()))
    }

    /// Restore the constant pool to a state captured by ``checkpoint``.
    fn rollback(&mut self, checkpoint: usize) -> PyResult<()> {
        self.with_builder_mut(|builder| {
            builder
                .try_truncate(checkpoint)
                .map_err(|err| PyValueError::new_err(err.to_string()))
        })
    }

    /// Look up an existing ``CONSTANT_Utf8_info`` entry (preserving lone surrogates).
    fn find_utf8(&self, value: &Bound<'_, PyString>) -> PyResult<Option<u16>> {
        let value = crate::java_string::from_python(value)?;
        self.with_builder(|builder| Ok(builder.find_utf16(&value).map(u16::from)))
    }

    /// Look up an existing ``CONSTANT_Integer_info`` entry.
    fn find_integer(&self, value: i32) -> PyResult<Option<u16>> {
        self.with_builder(|builder| Ok(builder.find_integer(value).map(u16::from)))
    }

    /// Look up an existing ``CONSTANT_Float_info`` entry by raw 32-bit pattern.
    fn find_float_bits(&self, raw_bits: u32) -> PyResult<Option<u16>> {
        self.with_builder(|builder| Ok(builder.find_float_bits(raw_bits).map(u16::from)))
    }

    /// Compatibility alias for ``find_float_bits``.
    fn find_float(&self, raw_bits: u32) -> PyResult<Option<u16>> {
        self.find_float_bits(raw_bits)
    }

    /// Look up an existing ``CONSTANT_Long_info`` entry.
    fn find_long(&self, value: i64) -> PyResult<Option<u16>> {
        self.with_builder(|builder| Ok(builder.find_long(value).map(u16::from)))
    }

    /// Look up an existing ``CONSTANT_Double_info`` entry by raw 64-bit pattern.
    fn find_double_bits(&self, raw_bits: u64) -> PyResult<Option<u16>> {
        self.with_builder(|builder| Ok(builder.find_double_bits(raw_bits).map(u16::from)))
    }

    /// Look up an existing ``CONSTANT_Double_info`` entry by explicit high/low 32-bit words.
    fn find_double(&self, high: u32, low: u32) -> PyResult<Option<u16>> {
        let raw_bits = ((high as u64) << 32) | (low as u64);
        self.find_double_bits(raw_bits)
    }

    /// Look up an existing ``CONSTANT_String_info`` entry (preserving lone surrogates).
    fn find_string(&self, value: &Bound<'_, PyString>) -> PyResult<Option<u16>> {
        let value = crate::java_string::from_python(value)?;
        self.with_builder(|builder| Ok(builder.find_java_string(&value).map(u16::from)))
    }

    /// Look up an existing ``CONSTANT_Class_info`` entry.
    fn find_class(&self, name: &str) -> PyResult<Option<u16>> {
        self.with_builder(|builder| Ok(builder.find_class(name).map(u16::from)))
    }

    /// Look up an existing ``CONSTANT_NameAndType_info`` entry.
    fn find_name_and_type(&self, name: &str, descriptor: &str) -> PyResult<Option<u16>> {
        self.with_builder(|builder| Ok(builder.find_name_and_type(name, descriptor).map(u16::from)))
    }

    /// Look up an existing ``CONSTANT_MethodType_info`` entry.
    fn find_method_type(&self, descriptor: &str) -> PyResult<Option<u16>> {
        self.with_builder(|builder| Ok(builder.find_method_type(descriptor).map(u16::from)))
    }

    /// Look up an existing ``CONSTANT_Fieldref_info`` entry.
    fn find_field_ref(&self, owner: &str, name: &str, descriptor: &str) -> PyResult<Option<u16>> {
        self.with_builder(|builder| {
            Ok(builder
                .find_field_ref(owner, name, descriptor)
                .map(u16::from))
        })
    }

    /// Alias for ``find_field_ref``.
    fn find_fieldref(&self, owner: &str, name: &str, descriptor: &str) -> PyResult<Option<u16>> {
        self.find_field_ref(owner, name, descriptor)
    }

    /// Look up an existing ``CONSTANT_Methodref_info`` entry.
    fn find_method_ref(&self, owner: &str, name: &str, descriptor: &str) -> PyResult<Option<u16>> {
        self.with_builder(|builder| {
            Ok(builder
                .find_method_ref(owner, name, descriptor)
                .map(u16::from))
        })
    }

    /// Alias for ``find_method_ref``.
    fn find_methodref(&self, owner: &str, name: &str, descriptor: &str) -> PyResult<Option<u16>> {
        self.find_method_ref(owner, name, descriptor)
    }

    /// Look up an existing ``CONSTANT_InterfaceMethodref_info`` entry.
    fn find_interface_method_ref(
        &self,
        owner: &str,
        name: &str,
        descriptor: &str,
    ) -> PyResult<Option<u16>> {
        self.with_builder(|builder| {
            Ok(builder
                .find_interface_method_ref(owner, name, descriptor)
                .map(u16::from))
        })
    }

    /// Alias for ``find_interface_method_ref``.
    fn find_interface_methodref(
        &self,
        owner: &str,
        name: &str,
        descriptor: &str,
    ) -> PyResult<Option<u16>> {
        self.find_interface_method_ref(owner, name, descriptor)
    }

    /// Look up an existing ``CONSTANT_MethodHandle_info`` entry.
    fn find_method_handle(
        &self,
        reference_kind: u8,
        reference_index: u16,
    ) -> PyResult<Option<u16>> {
        self.with_builder(|builder| {
            Ok(builder
                .find_method_handle(reference_kind, CpIndex::from(reference_index))
                .map(u16::from))
        })
    }

    /// Look up an existing ``CONSTANT_Dynamic_info`` entry.
    fn find_dynamic(
        &self,
        bootstrap_method_attr_index: u16,
        name: &str,
        descriptor: &str,
    ) -> PyResult<Option<u16>> {
        self.with_builder(|builder| {
            Ok(builder
                .find_dynamic(
                    BootstrapMethodIndex::from(bootstrap_method_attr_index),
                    name,
                    descriptor,
                )
                .map(u16::from))
        })
    }

    /// Look up an existing ``CONSTANT_InvokeDynamic_info`` entry.
    fn find_invoke_dynamic(
        &self,
        bootstrap_method_attr_index: u16,
        name: &str,
        descriptor: &str,
    ) -> PyResult<Option<u16>> {
        self.with_builder(|builder| {
            Ok(builder
                .find_invoke_dynamic(
                    BootstrapMethodIndex::from(bootstrap_method_attr_index),
                    name,
                    descriptor,
                )
                .map(u16::from))
        })
    }

    fn __repr__(&self) -> PyResult<String> {
        let count = self.count()?;
        let len = self.len()?;
        Ok(format!("ConstantPoolBuilder(count={count}, len={len})"))
    }
}
