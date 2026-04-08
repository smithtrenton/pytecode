use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};
use pytecode_engine::analysis::MappingClassResolver;
use pytecode_engine::model::{
    ClassModel, CodeItem, CodeModel, ConstantPoolBuilder, DebugInfoPolicy, DebugInfoState,
    ExceptionHandler, FieldModel, FrameComputationMode, Label, LdcValue, MethodModel,
};
use pytecode_engine::write_class;
use std::hash::{Hash, Hasher};

// ---------------------------------------------------------------------------
// Helper: convert EngineError → PyErr
// ---------------------------------------------------------------------------

fn analysis_error_to_py(e: pytecode_engine::analysis::AnalysisError) -> PyErr {
    PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string())
}

// ---------------------------------------------------------------------------
// PyLabel
// ---------------------------------------------------------------------------

#[pyclass(name = "RustLabel")]
#[derive(Clone)]
pub struct PyLabel {
    inner: Label,
}

impl From<Label> for PyLabel {
    fn from(inner: Label) -> Self {
        Self { inner }
    }
}

impl From<PyLabel> for Label {
    fn from(py: PyLabel) -> Self {
        py.inner
    }
}

#[pymethods]
impl PyLabel {
    #[new]
    fn new() -> Self {
        Self {
            inner: Label::new(),
        }
    }

    #[staticmethod]
    fn named(name: &str) -> Self {
        Self {
            inner: Label::named(name),
        }
    }

    #[getter]
    fn name(&self) -> Option<String> {
        self.inner.name.clone()
    }

    fn __repr__(&self) -> String {
        match &self.inner.name {
            Some(name) => format!("RustLabel(name={name:?})"),
            None => "RustLabel()".to_owned(),
        }
    }

    fn __eq__(&self, other: &PyLabel) -> bool {
        self.inner == other.inner
    }

    fn __hash__(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.inner.hash(&mut hasher);
        hasher.finish()
    }
}

// ---------------------------------------------------------------------------
// PyModelExceptionHandler
// ---------------------------------------------------------------------------

#[pyclass(name = "RustExceptionHandler")]
#[derive(Clone)]
pub struct PyModelExceptionHandler {
    inner: ExceptionHandler,
}

#[pymethods]
impl PyModelExceptionHandler {
    #[getter]
    fn start(&self) -> PyLabel {
        PyLabel::from(self.inner.start.clone())
    }

    #[getter]
    fn end(&self) -> PyLabel {
        PyLabel::from(self.inner.end.clone())
    }

    #[getter]
    fn handler(&self) -> PyLabel {
        PyLabel::from(self.inner.handler.clone())
    }

    #[getter]
    fn catch_type(&self) -> Option<String> {
        self.inner.catch_type.clone()
    }
}

// ---------------------------------------------------------------------------
// CodeItem → Python dict helper
// ---------------------------------------------------------------------------

fn wrap_label(py: Python<'_>, label: &Label) -> PyResult<PyObject> {
    Ok(Py::new(py, PyLabel::from(label.clone()))?.into_any())
}

fn wrap_code_item(py: Python<'_>, item: &CodeItem) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    match item {
        CodeItem::Label(label) => {
            dict.set_item("type", "label")?;
            dict.set_item("label", wrap_label(py, label)?)?;
        }
        CodeItem::Raw(insn) => {
            dict.set_item("opcode", insn.opcode())?;
            match insn {
                pytecode_engine::raw::Instruction::Byte { value, .. } => {
                    dict.set_item("type", "byte")?;
                    dict.set_item("value", *value)?;
                }
                pytecode_engine::raw::Instruction::Short { value, .. } => {
                    dict.set_item("type", "short")?;
                    dict.set_item("value", *value)?;
                }
                pytecode_engine::raw::Instruction::NewArray(na) => {
                    dict.set_item("type", "newarray")?;
                    dict.set_item("atype", na.atype as u8)?;
                }
                _ => {
                    dict.set_item("type", "raw")?;
                }
            }
        }
        CodeItem::Field(f) => {
            dict.set_item("type", "field")?;
            dict.set_item("opcode", f.opcode)?;
            dict.set_item("owner", &f.owner)?;
            dict.set_item("name", &f.name)?;
            dict.set_item("descriptor", &f.descriptor)?;
        }
        CodeItem::Method(m) => {
            dict.set_item("type", "method")?;
            dict.set_item("opcode", m.opcode)?;
            dict.set_item("owner", &m.owner)?;
            dict.set_item("name", &m.name)?;
            dict.set_item("descriptor", &m.descriptor)?;
            dict.set_item("is_interface", m.is_interface)?;
        }
        CodeItem::InterfaceMethod(im) => {
            dict.set_item("type", "interface_method")?;
            dict.set_item("owner", &im.owner)?;
            dict.set_item("name", &im.name)?;
            dict.set_item("descriptor", &im.descriptor)?;
        }
        CodeItem::Type(t) => {
            dict.set_item("type", "type")?;
            dict.set_item("opcode", t.opcode)?;
            dict.set_item("descriptor", &t.descriptor)?;
        }
        CodeItem::Var(v) => {
            dict.set_item("type", "var")?;
            dict.set_item("opcode", v.opcode)?;
            dict.set_item("slot", v.slot)?;
        }
        CodeItem::IInc(ii) => {
            dict.set_item("type", "iinc")?;
            dict.set_item("slot", ii.slot)?;
            dict.set_item("value", ii.value)?;
        }
        CodeItem::Ldc(ldc) => {
            dict.set_item("type", "ldc")?;
            match &ldc.value {
                LdcValue::Int(v) => {
                    dict.set_item("value_type", "int")?;
                    dict.set_item("value", *v)?;
                }
                LdcValue::FloatBits(v) => {
                    dict.set_item("value_type", "float")?;
                    dict.set_item("value", *v)?;
                }
                LdcValue::Long(v) => {
                    dict.set_item("value_type", "long")?;
                    dict.set_item("value", *v)?;
                }
                LdcValue::DoubleBits(v) => {
                    dict.set_item("value_type", "double")?;
                    dict.set_item("value", *v)?;
                }
                LdcValue::String(v) => {
                    dict.set_item("value_type", "string")?;
                    dict.set_item("value", v.as_str())?;
                }
                LdcValue::Class(v) => {
                    dict.set_item("value_type", "class")?;
                    dict.set_item("value", v.as_str())?;
                }
                LdcValue::MethodType(v) => {
                    dict.set_item("value_type", "method_type")?;
                    dict.set_item("value", v.as_str())?;
                }
                LdcValue::MethodHandle(mh) => {
                    dict.set_item("value_type", "method_handle")?;
                    dict.set_item("reference_kind", mh.reference_kind)?;
                    dict.set_item("owner", &mh.owner)?;
                    dict.set_item("name", &mh.name)?;
                    dict.set_item("descriptor", &mh.descriptor)?;
                    dict.set_item("is_interface", mh.is_interface)?;
                }
                LdcValue::Dynamic(dv) => {
                    dict.set_item("value_type", "dynamic")?;
                    dict.set_item(
                        "bootstrap_method_attr_index",
                        dv.bootstrap_method_attr_index,
                    )?;
                    dict.set_item("name", &dv.name)?;
                    dict.set_item("descriptor", &dv.descriptor)?;
                }
            }
        }
        CodeItem::InvokeDynamic(id) => {
            dict.set_item("type", "invokedynamic")?;
            dict.set_item(
                "bootstrap_method_attr_index",
                id.bootstrap_method_attr_index,
            )?;
            dict.set_item("name", &id.name)?;
            dict.set_item("descriptor", &id.descriptor)?;
        }
        CodeItem::MultiANewArray(m) => {
            dict.set_item("type", "multianewarray")?;
            dict.set_item("descriptor", &m.descriptor)?;
            dict.set_item("dimensions", m.dimensions)?;
        }
        CodeItem::Branch(b) => {
            dict.set_item("type", "branch")?;
            dict.set_item("opcode", b.opcode)?;
            dict.set_item("target", wrap_label(py, &b.target)?)?;
        }
        CodeItem::LookupSwitch(ls) => {
            dict.set_item("type", "lookupswitch")?;
            dict.set_item("default_target", wrap_label(py, &ls.default_target)?)?;
            let pairs = ls
                .pairs
                .iter()
                .map(|(key, label)| -> PyResult<PyObject> {
                    let tuple = (key, wrap_label(py, label)?);
                    Ok(tuple.into_pyobject(py)?.into_any().unbind())
                })
                .collect::<PyResult<Vec<_>>>()?;
            dict.set_item("pairs", pairs)?;
        }
        CodeItem::TableSwitch(ts) => {
            dict.set_item("type", "tableswitch")?;
            dict.set_item("default_target", wrap_label(py, &ts.default_target)?)?;
            dict.set_item("low", ts.low)?;
            dict.set_item("high", ts.high)?;
            let targets = ts
                .targets
                .iter()
                .map(|label| wrap_label(py, label))
                .collect::<PyResult<Vec<_>>>()?;
            dict.set_item("targets", targets)?;
        }
    }
    Ok(dict.into_any().unbind())
}

// ---------------------------------------------------------------------------
// PyCodeModel
// ---------------------------------------------------------------------------

#[pyclass(name = "RustCodeModel")]
#[derive(Clone)]
pub struct PyCodeModel {
    pub(crate) inner: CodeModel,
}

#[pymethods]
impl PyCodeModel {
    #[getter]
    fn max_stack(&self) -> u16 {
        self.inner.max_stack
    }

    #[getter]
    fn max_locals(&self) -> u16 {
        self.inner.max_locals
    }

    #[getter]
    fn instructions(&self, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        self.inner
            .instructions
            .iter()
            .map(|item| wrap_code_item(py, item))
            .collect()
    }

    #[getter]
    fn exception_handlers(&self) -> Vec<PyModelExceptionHandler> {
        self.inner
            .exception_handlers
            .iter()
            .map(|eh| PyModelExceptionHandler { inner: eh.clone() })
            .collect()
    }

    #[getter]
    fn line_numbers(&self, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        self.inner
            .line_numbers
            .iter()
            .map(|ln| -> PyResult<PyObject> {
                let dict = PyDict::new(py);
                dict.set_item("label", wrap_label(py, &ln.label)?)?;
                dict.set_item("line_number", ln.line_number)?;
                Ok(dict.into_any().unbind())
            })
            .collect()
    }

    #[getter]
    fn local_variables(&self, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        self.inner
            .local_variables
            .iter()
            .map(|lv| -> PyResult<PyObject> {
                let dict = PyDict::new(py);
                dict.set_item("start", wrap_label(py, &lv.start)?)?;
                dict.set_item("end", wrap_label(py, &lv.end)?)?;
                dict.set_item("name", &lv.name)?;
                dict.set_item("descriptor", &lv.descriptor)?;
                dict.set_item("index", lv.index)?;
                Ok(dict.into_any().unbind())
            })
            .collect()
    }

    #[getter]
    fn local_variable_types(&self, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        self.inner
            .local_variable_types
            .iter()
            .map(|lv| -> PyResult<PyObject> {
                let dict = PyDict::new(py);
                dict.set_item("start", wrap_label(py, &lv.start)?)?;
                dict.set_item("end", wrap_label(py, &lv.end)?)?;
                dict.set_item("name", &lv.name)?;
                dict.set_item("signature", &lv.signature)?;
                dict.set_item("index", lv.index)?;
                Ok(dict.into_any().unbind())
            })
            .collect()
    }

    #[getter]
    fn attributes(&self, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        self.inner
            .attributes
            .iter()
            .map(|attr| crate::wrap_attribute(py, attr))
            .collect()
    }

    #[getter]
    fn debug_info_state(&self) -> &str {
        match self.inner.debug_info_state {
            DebugInfoState::Fresh => "fresh",
            DebugInfoState::Stale => "stale",
        }
    }

    #[getter]
    fn nested_attribute_layout(&self) -> Vec<String> {
        self.inner
            .nested_attribute_layout()
            .iter()
            .map(|s| s.to_string())
            .collect()
    }
}

// ---------------------------------------------------------------------------
// PyFieldModel
// ---------------------------------------------------------------------------

#[pyclass(name = "RustFieldModel")]
#[derive(Clone)]
pub struct PyFieldModel {
    pub(crate) inner: FieldModel,
}

#[pymethods]
impl PyFieldModel {
    #[getter]
    fn access_flags(&self) -> u16 {
        self.inner.access_flags.bits()
    }

    #[getter]
    fn name(&self) -> String {
        self.inner.name.clone()
    }

    #[getter]
    fn descriptor(&self) -> String {
        self.inner.descriptor.clone()
    }

    #[getter]
    fn attributes(&self, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        self.inner
            .attributes
            .iter()
            .map(|attr| crate::wrap_attribute(py, attr))
            .collect()
    }
}

// ---------------------------------------------------------------------------
// PyMethodModel
// ---------------------------------------------------------------------------

#[pyclass(name = "RustMethodModel")]
#[derive(Clone)]
pub struct PyMethodModel {
    pub(crate) inner: MethodModel,
}

#[pymethods]
impl PyMethodModel {
    #[getter]
    fn access_flags(&self) -> u16 {
        self.inner.access_flags.bits()
    }

    #[getter]
    fn name(&self) -> String {
        self.inner.name.clone()
    }

    #[getter]
    fn descriptor(&self) -> String {
        self.inner.descriptor.clone()
    }

    #[getter]
    fn code(&self) -> Option<PyCodeModel> {
        self.inner
            .code
            .as_ref()
            .map(|c| PyCodeModel { inner: c.clone() })
    }

    #[getter]
    fn attributes(&self, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        self.inner
            .attributes
            .iter()
            .map(|attr| crate::wrap_attribute(py, attr))
            .collect()
    }
}

// ---------------------------------------------------------------------------
// PyConstantPoolBuilder
// ---------------------------------------------------------------------------

#[pyclass(name = "RustConstantPoolBuilder")]
#[derive(Clone)]
pub struct PyConstantPoolBuilder {
    inner: ConstantPoolBuilder,
}

#[pymethods]
impl PyConstantPoolBuilder {
    #[new]
    fn new() -> Self {
        Self {
            inner: ConstantPoolBuilder::new(),
        }
    }

    fn add_utf8(&mut self, value: &str) -> PyResult<u16> {
        self.inner
            .add_utf8(value)
            .map_err(crate::engine_error_to_py)
    }

    fn add_class(&mut self, name: &str) -> PyResult<u16> {
        self.inner
            .add_class(name)
            .map_err(crate::engine_error_to_py)
    }

    fn add_string(&mut self, value: &str) -> PyResult<u16> {
        self.inner
            .add_string(value)
            .map_err(crate::engine_error_to_py)
    }

    fn add_integer(&mut self, value: u32) -> PyResult<u16> {
        self.inner
            .add_integer(value)
            .map_err(crate::engine_error_to_py)
    }

    fn add_long(&mut self, value: u64) -> PyResult<u16> {
        self.inner
            .add_long(value)
            .map_err(crate::engine_error_to_py)
    }

    fn add_float_bits(&mut self, raw_bits: u32) -> PyResult<u16> {
        self.inner
            .add_float_bits(raw_bits)
            .map_err(crate::engine_error_to_py)
    }

    fn add_double_bits(&mut self, raw_bits: u64) -> PyResult<u16> {
        self.inner
            .add_double_bits(raw_bits)
            .map_err(crate::engine_error_to_py)
    }

    fn add_field_ref(&mut self, owner: &str, name: &str, descriptor: &str) -> PyResult<u16> {
        self.inner
            .add_field_ref(owner, name, descriptor)
            .map_err(crate::engine_error_to_py)
    }

    fn add_method_ref(&mut self, owner: &str, name: &str, descriptor: &str) -> PyResult<u16> {
        self.inner
            .add_method_ref(owner, name, descriptor)
            .map_err(crate::engine_error_to_py)
    }

    fn add_interface_method_ref(
        &mut self,
        owner: &str,
        name: &str,
        descriptor: &str,
    ) -> PyResult<u16> {
        self.inner
            .add_interface_method_ref(owner, name, descriptor)
            .map_err(crate::engine_error_to_py)
    }

    fn add_method_type(&mut self, descriptor: &str) -> PyResult<u16> {
        self.inner
            .add_method_type(descriptor)
            .map_err(crate::engine_error_to_py)
    }

    fn add_method_handle(&mut self, reference_kind: u8, reference_index: u16) -> PyResult<u16> {
        self.inner
            .add_method_handle(reference_kind, reference_index)
            .map_err(crate::engine_error_to_py)
    }

    fn add_invoke_dynamic(
        &mut self,
        bootstrap_idx: u16,
        name: &str,
        descriptor: &str,
    ) -> PyResult<u16> {
        self.inner
            .add_invoke_dynamic(bootstrap_idx, name, descriptor)
            .map_err(crate::engine_error_to_py)
    }

    fn resolve_utf8(&self, index: u16) -> PyResult<String> {
        self.inner
            .resolve_utf8(index)
            .map_err(crate::engine_error_to_py)
    }

    fn resolve_class_name(&self, index: u16) -> PyResult<String> {
        self.inner
            .resolve_class_name(index)
            .map_err(crate::engine_error_to_py)
    }

    fn count(&self) -> u16 {
        self.inner.count()
    }

    fn len(&self) -> usize {
        self.inner.len()
    }
}

// ---------------------------------------------------------------------------
// PyMappingClassResolver
// ---------------------------------------------------------------------------

#[pyclass(name = "RustMappingClassResolver")]
#[derive(Clone)]
pub struct PyMappingClassResolver {
    pub(crate) inner: MappingClassResolver,
}

#[pymethods]
impl PyMappingClassResolver {
    #[new]
    fn new() -> PyResult<Self> {
        Ok(Self {
            inner: MappingClassResolver::new(std::iter::empty()).map_err(analysis_error_to_py)?,
        })
    }

    #[staticmethod]
    fn from_bytes(class_bytes_list: Vec<Vec<u8>>) -> PyResult<Self> {
        let models: Vec<ClassModel> = class_bytes_list
            .iter()
            .map(|bytes| ClassModel::from_bytes(bytes).map_err(crate::engine_error_to_py))
            .collect::<PyResult<Vec<_>>>()?;
        let resolver = MappingClassResolver::from_models(models).map_err(analysis_error_to_py)?;
        Ok(Self { inner: resolver })
    }

    fn resolve_class(&self, py: Python<'_>, name: &str) -> PyResult<Option<PyObject>> {
        use pytecode_engine::analysis::ClassResolver;
        match self.inner.resolve_class(name) {
            Some(rc) => {
                let dict = PyDict::new(py);
                dict.set_item("name", &rc.name)?;
                dict.set_item("super_name", &rc.super_name)?;
                dict.set_item("interfaces", &rc.interfaces)?;
                dict.set_item("access_flags", rc.access_flags.bits())?;
                Ok(Some(dict.into_any().unbind()))
            }
            None => Ok(None),
        }
    }
}

// ---------------------------------------------------------------------------
// PyClassModel
// ---------------------------------------------------------------------------

fn parse_debug_info_policy(s: &str) -> PyResult<DebugInfoPolicy> {
    match s {
        "preserve" => Ok(DebugInfoPolicy::Preserve),
        "strip" => Ok(DebugInfoPolicy::Strip),
        _ => Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
            "invalid debug info policy: {s:?}, expected \"preserve\" or \"strip\""
        ))),
    }
}

#[pyclass(name = "RustClassModel")]
#[derive(Clone)]
pub struct PyClassModel {
    pub(crate) inner: ClassModel,
}

#[pymethods]
impl PyClassModel {
    // -- constructors -------------------------------------------------------

    #[staticmethod]
    fn from_bytes(data: &[u8]) -> PyResult<Self> {
        let model = ClassModel::from_bytes(data).map_err(crate::engine_error_to_py)?;
        Ok(Self { inner: model })
    }

    // -- serialisation ------------------------------------------------------

    fn to_bytes<'py>(&self, py: Python<'py>) -> PyResult<Py<PyBytes>> {
        let bytes = self.inner.to_bytes().map_err(crate::engine_error_to_py)?;
        Ok(PyBytes::new(py, &bytes).unbind())
    }

    #[pyo3(signature = (recompute_frames = false, resolver = None, debug_info = "preserve"))]
    fn to_bytes_with_options<'py>(
        &self,
        py: Python<'py>,
        recompute_frames: bool,
        resolver: Option<&PyMappingClassResolver>,
        debug_info: &str,
    ) -> PyResult<Py<PyBytes>> {
        let policy = parse_debug_info_policy(debug_info)?;
        let frame_mode = if recompute_frames {
            FrameComputationMode::Recompute
        } else {
            FrameComputationMode::Preserve
        };
        let classfile = self
            .inner
            .to_classfile_with_options(
                policy,
                frame_mode,
                resolver.map(|r| &r.inner as &dyn pytecode_engine::analysis::ClassResolver),
            )
            .map_err(crate::engine_error_to_py)?;
        let bytes = write_class(&classfile).map_err(crate::engine_error_to_py)?;
        Ok(PyBytes::new(py, &bytes).unbind())
    }

    // -- getters ------------------------------------------------------------

    #[getter]
    fn entry_name(&self) -> String {
        self.inner.entry_name.clone()
    }

    #[getter]
    fn original_byte_len(&self) -> usize {
        self.inner.original_byte_len
    }

    #[getter]
    fn version(&self) -> (u16, u16) {
        self.inner.version
    }

    #[getter]
    fn access_flags(&self) -> u16 {
        self.inner.access_flags.bits()
    }

    #[getter]
    fn name(&self) -> String {
        self.inner.name.clone()
    }

    #[getter]
    fn super_name(&self) -> Option<String> {
        self.inner.super_name.clone()
    }

    #[getter]
    fn interfaces(&self) -> Vec<String> {
        self.inner.interfaces.clone()
    }

    #[getter]
    fn fields(&self) -> Vec<PyFieldModel> {
        self.inner
            .fields
            .iter()
            .map(|f| PyFieldModel { inner: f.clone() })
            .collect()
    }

    #[getter]
    fn methods(&self) -> Vec<PyMethodModel> {
        self.inner
            .methods
            .iter()
            .map(|m| PyMethodModel { inner: m.clone() })
            .collect()
    }

    #[getter]
    fn attributes(&self, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        self.inner
            .attributes
            .iter()
            .map(|attr| crate::wrap_attribute(py, attr))
            .collect()
    }

    #[getter]
    fn constant_pool(&self) -> PyConstantPoolBuilder {
        PyConstantPoolBuilder {
            inner: self.inner.constant_pool.clone(),
        }
    }

    #[getter]
    fn debug_info_state(&self) -> &str {
        match self.inner.debug_info_state {
            DebugInfoState::Fresh => "fresh",
            DebugInfoState::Stale => "stale",
        }
    }

    // -- setters ------------------------------------------------------------

    #[setter]
    fn set_name(&mut self, value: String) {
        self.inner.name = value;
    }

    #[setter]
    fn set_super_name(&mut self, value: Option<String>) {
        self.inner.super_name = value;
    }

    #[setter]
    fn set_interfaces(&mut self, value: Vec<String>) {
        self.inner.interfaces = value;
    }

    #[setter]
    fn set_access_flags(&mut self, value: u16) {
        self.inner.access_flags =
            pytecode_engine::constants::ClassAccessFlags::from_bits_truncate(value);
    }

    #[setter]
    fn set_version(&mut self, value: (u16, u16)) {
        self.inner.version = value;
    }

    #[setter]
    fn set_fields(&mut self, value: Vec<PyFieldModel>) {
        self.inner.fields = value.into_iter().map(|f| f.inner).collect();
    }

    #[setter]
    fn set_methods(&mut self, value: Vec<PyMethodModel>) {
        self.inner.methods = value.into_iter().map(|m| m.inner).collect();
    }
}

// ---------------------------------------------------------------------------
// register
// ---------------------------------------------------------------------------

pub(crate) fn register(_py: Python<'_>, module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyClassModel>()?;
    module.add_class::<PyMethodModel>()?;
    module.add_class::<PyFieldModel>()?;
    module.add_class::<PyCodeModel>()?;
    module.add_class::<PyLabel>()?;
    module.add_class::<PyConstantPoolBuilder>()?;
    module.add_class::<PyMappingClassResolver>()?;
    module.add_class::<PyModelExceptionHandler>()?;
    Ok(())
}
