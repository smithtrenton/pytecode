use pyo3::exceptions::{PyIndexError, PyRuntimeError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes, PyDict, PyList};
use pyo3::wrap_pyfunction;
use pytecode_engine::analysis::MappingClassResolver;
use pytecode_engine::indexes::BootstrapMethodIndex;
use pytecode_engine::model::{
    BranchInsn, ClassModel, CodeItem, CodeModel, ConstantPoolBuilder, DebugInfoPolicy,
    DebugInfoState, DynamicValue, ExceptionHandler, FieldInsn, FieldModel, FrameComputationMode,
    IIncInsn, InterfaceMethodInsn, InvokeDynamicInsn, Label, LdcInsn, LdcValue, LookupSwitchInsn,
    MethodHandleValue, MethodInsn, MethodModel, MultiANewArrayInsn, TableSwitchInsn, TypeInsn,
    VarInsn,
};
use pytecode_engine::raw::{ArrayType, ClassFile, Instruction, NewArrayInsn};
use pytecode_engine::write_class;
use std::cell::RefCell;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::Arc;

use crate::PyClassFile;
use crate::transforms::PyInsnMatcher;

// ---------------------------------------------------------------------------
// Helper: convert EngineError → PyErr
// ---------------------------------------------------------------------------

fn analysis_error_to_py(e: pytecode_engine::analysis::AnalysisError) -> PyErr {
    PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string())
}

fn resolved_method_to_py(
    py: Python<'_>,
    method: &pytecode_engine::analysis::ResolvedMethod,
) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    dict.set_item("name", &method.name)?;
    dict.set_item("descriptor", &method.descriptor)?;
    dict.set_item("access_flags", method.access_flags.bits())?;
    Ok(dict.into_any().unbind())
}

fn resolved_class_to_py(
    py: Python<'_>,
    resolved: &pytecode_engine::analysis::ResolvedClass,
) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    let methods = resolved
        .methods
        .iter()
        .map(|method| resolved_method_to_py(py, method))
        .collect::<PyResult<Vec<_>>>()?;
    dict.set_item("name", &resolved.name)?;
    dict.set_item("super_name", &resolved.super_name)?;
    dict.set_item("interfaces", &resolved.interfaces)?;
    dict.set_item("access_flags", resolved.access_flags.bits())?;
    dict.set_item("methods", methods)?;
    Ok(dict.into_any().unbind())
}

type SharedClassState = Rc<RefCell<ClassModelState>>;

struct ClassModelState {
    inner: Option<ClassModel>,
    interfaces_generation: u64,
    fields_generation: u64,
    methods_generation: u64,
}

impl ClassModelState {
    fn new(inner: ClassModel) -> Self {
        Self {
            inner: Some(inner),
            interfaces_generation: 0,
            fields_generation: 0,
            methods_generation: 0,
        }
    }
}

fn dead_model_err() -> PyErr {
    PyErr::new::<PyRuntimeError, _>("ClassModel is no longer live")
}

fn stale_ref_err(kind: &str) -> PyErr {
    PyErr::new::<PyRuntimeError, _>(format!("{kind} is stale after structural mutation"))
}

fn index_err(index: isize, len: usize) -> PyErr {
    PyErr::new::<PyIndexError, _>(format!("index {index} out of range for length {len}"))
}

fn normalize_index(index: isize, len: usize) -> PyResult<usize> {
    let normalized = if index < 0 {
        len as isize + index
    } else {
        index
    };
    if normalized < 0 || normalized >= len as isize {
        return Err(index_err(index, len));
    }
    Ok(normalized as usize)
}

fn with_live_model<R>(
    state: &SharedClassState,
    f: impl FnOnce(&ClassModelState) -> PyResult<R>,
) -> PyResult<R> {
    let borrowed = state.borrow();
    if borrowed.inner.is_none() {
        return Err(dead_model_err());
    }
    f(&borrowed)
}

fn with_live_model_mut<R>(
    state: &SharedClassState,
    f: impl FnOnce(&mut ClassModelState) -> PyResult<R>,
) -> PyResult<R> {
    let mut borrowed = state.borrow_mut();
    if borrowed.inner.is_none() {
        return Err(dead_model_err());
    }
    f(&mut borrowed)
}

fn wrap_line_number(py: Python<'_>, label: &Label, line_number: u16) -> PyResult<PyObject> {
    Ok(Py::new(
        py,
        PyLineNumberEntry {
            label: label.clone(),
            line_number,
        },
    )?
    .into_any())
}

fn wrap_local_variable(
    py: Python<'_>,
    start: &Label,
    end: &Label,
    name: &str,
    descriptor: &str,
    index: u16,
) -> PyResult<PyObject> {
    Ok(Py::new(
        py,
        PyLocalVariableEntry {
            start: start.clone(),
            end: end.clone(),
            name: name.to_owned(),
            descriptor: descriptor.to_owned(),
            index,
        },
    )?
    .into_any())
}

fn wrap_local_variable_type(
    py: Python<'_>,
    start: &Label,
    end: &Label,
    name: &str,
    signature: &str,
    index: u16,
) -> PyResult<PyObject> {
    Ok(Py::new(
        py,
        PyLocalVariableTypeEntry {
            start: start.clone(),
            end: end.clone(),
            name: name.to_owned(),
            signature: signature.to_owned(),
            index,
        },
    )?
    .into_any())
}

// ---------------------------------------------------------------------------
// PyLabel
// ---------------------------------------------------------------------------

#[pyclass(name = "Label", module = "pytecode._rust")]
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

    #[getter]
    fn kind(&self) -> &'static str {
        "label"
    }

    fn __repr__(&self) -> String {
        match &self.inner.name {
            Some(name) => format!("Label(name={name:?})"),
            None => "Label()".to_owned(),
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

#[pyclass(name = "ExceptionHandler", module = "pytecode._rust")]
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

#[pyclass(name = "LineNumberEntry", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyLineNumberEntry {
    label: Label,
    line_number: u16,
}

#[pymethods]
impl PyLineNumberEntry {
    #[new]
    fn new(label: PyLabel, line_number: u16) -> Self {
        Self {
            label: label.inner,
            line_number,
        }
    }

    #[getter]
    fn label(&self) -> PyLabel {
        PyLabel::from(self.label.clone())
    }

    #[getter]
    fn line_number(&self) -> u16 {
        self.line_number
    }
}

#[pyclass(name = "LocalVariableEntry", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyLocalVariableEntry {
    start: Label,
    end: Label,
    name: String,
    descriptor: String,
    index: u16,
}

#[pymethods]
impl PyLocalVariableEntry {
    #[new]
    fn new(start: PyLabel, end: PyLabel, name: String, descriptor: String, index: u16) -> Self {
        Self {
            start: start.inner,
            end: end.inner,
            name,
            descriptor,
            index,
        }
    }

    #[getter]
    fn start(&self) -> PyLabel {
        PyLabel::from(self.start.clone())
    }

    #[getter]
    fn end(&self) -> PyLabel {
        PyLabel::from(self.end.clone())
    }

    #[getter]
    fn name(&self) -> String {
        self.name.clone()
    }

    #[getter]
    fn descriptor(&self) -> String {
        self.descriptor.clone()
    }

    #[getter]
    fn index(&self) -> u16 {
        self.index
    }
}

#[pyclass(name = "LocalVariableTypeEntry", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyLocalVariableTypeEntry {
    start: Label,
    end: Label,
    name: String,
    signature: String,
    index: u16,
}

#[pymethods]
impl PyLocalVariableTypeEntry {
    #[new]
    fn new(start: PyLabel, end: PyLabel, name: String, signature: String, index: u16) -> Self {
        Self {
            start: start.inner,
            end: end.inner,
            name,
            signature,
            index,
        }
    }

    #[getter]
    fn start(&self) -> PyLabel {
        PyLabel::from(self.start.clone())
    }

    #[getter]
    fn end(&self) -> PyLabel {
        PyLabel::from(self.end.clone())
    }

    #[getter]
    fn name(&self) -> String {
        self.name.clone()
    }

    #[getter]
    fn signature(&self) -> String {
        self.signature.clone()
    }

    #[getter]
    fn index(&self) -> u16 {
        self.index
    }
}

#[pyclass(name = "MethodHandleValue", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyMethodHandleValue {
    inner: MethodHandleValue,
}

#[pymethods]
impl PyMethodHandleValue {
    #[new]
    fn new(
        reference_kind: u8,
        owner: String,
        name: String,
        descriptor: String,
        is_interface: bool,
    ) -> Self {
        Self {
            inner: MethodHandleValue {
                reference_kind,
                owner,
                name,
                descriptor,
                is_interface,
            },
        }
    }

    #[getter]
    fn reference_kind(&self) -> u8 {
        self.inner.reference_kind
    }

    #[getter]
    fn owner(&self) -> String {
        self.inner.owner.clone()
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
    fn is_interface(&self) -> bool {
        self.inner.is_interface
    }
}

#[pyclass(name = "DynamicValue", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyDynamicValue {
    inner: DynamicValue,
}

#[pymethods]
impl PyDynamicValue {
    #[new]
    fn new(bootstrap_method_attr_index: u16, name: String, descriptor: String) -> Self {
        Self {
            inner: DynamicValue {
                bootstrap_method_attr_index: BootstrapMethodIndex::new(bootstrap_method_attr_index),
                name,
                descriptor,
            },
        }
    }

    #[getter]
    fn bootstrap_method_attr_index(&self) -> u16 {
        self.inner.bootstrap_method_attr_index.value()
    }

    #[getter]
    fn name(&self) -> String {
        self.inner.name.clone()
    }

    #[getter]
    fn descriptor(&self) -> String {
        self.inner.descriptor.clone()
    }
}

#[pyclass(name = "RawInsn", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyRawInsn {
    opcode: u8,
}

#[pymethods]
impl PyRawInsn {
    #[new]
    fn new(opcode: u8) -> Self {
        Self { opcode }
    }

    #[getter]
    fn kind(&self) -> &'static str {
        "raw"
    }

    #[getter]
    fn opcode(&self) -> u8 {
        self.opcode
    }
}

#[pyclass(name = "ByteInsn", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyByteInsn {
    opcode: u8,
    value: i8,
}

#[pymethods]
impl PyByteInsn {
    #[new]
    fn new(opcode: u8, value: i8) -> Self {
        Self { opcode, value }
    }

    #[getter]
    fn kind(&self) -> &'static str {
        "byte"
    }

    #[getter]
    fn opcode(&self) -> u8 {
        self.opcode
    }

    #[getter]
    fn value(&self) -> i8 {
        self.value
    }
}

#[pyclass(name = "ShortInsn", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyShortInsn {
    opcode: u8,
    value: i16,
}

#[pymethods]
impl PyShortInsn {
    #[new]
    fn new(opcode: u8, value: i16) -> Self {
        Self { opcode, value }
    }

    #[getter]
    fn kind(&self) -> &'static str {
        "short"
    }

    #[getter]
    fn opcode(&self) -> u8 {
        self.opcode
    }

    #[getter]
    fn value(&self) -> i16 {
        self.value
    }
}

#[pyclass(name = "NewArrayInsn", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyNewArrayInsn {
    atype: u8,
}

#[pymethods]
impl PyNewArrayInsn {
    #[new]
    fn new(atype: u8) -> PyResult<Self> {
        ArrayType::try_from(atype)
            .map(|_| Self { atype })
            .map_err(|err| code_item_value_error(format!("invalid newarray atype: {err}")))
    }

    #[getter]
    fn kind(&self) -> &'static str {
        "newarray"
    }

    #[getter]
    fn atype(&self) -> u8 {
        self.atype
    }
}

#[pyclass(name = "FieldInsn", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyFieldInsn {
    inner: FieldInsn,
}

#[pymethods]
impl PyFieldInsn {
    #[new]
    fn new(opcode: u8, owner: String, name: String, descriptor: String) -> Self {
        Self {
            inner: FieldInsn {
                opcode,
                owner,
                name,
                descriptor,
            },
        }
    }

    #[getter]
    fn kind(&self) -> &'static str {
        "field"
    }

    #[getter]
    fn opcode(&self) -> u8 {
        self.inner.opcode
    }

    #[getter]
    fn owner(&self) -> String {
        self.inner.owner.clone()
    }

    #[getter]
    fn name(&self) -> String {
        self.inner.name.clone()
    }

    #[getter]
    fn descriptor(&self) -> String {
        self.inner.descriptor.clone()
    }
}

#[pyclass(name = "MethodInsn", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyMethodInsn {
    inner: MethodInsn,
}

#[pymethods]
impl PyMethodInsn {
    #[new]
    #[pyo3(signature = (opcode, owner, name, descriptor, is_interface=false))]
    fn new(
        opcode: u8,
        owner: String,
        name: String,
        descriptor: String,
        is_interface: bool,
    ) -> Self {
        Self {
            inner: MethodInsn {
                opcode,
                owner,
                name,
                descriptor,
                is_interface,
            },
        }
    }

    #[getter]
    fn kind(&self) -> &'static str {
        "method"
    }

    #[getter]
    fn opcode(&self) -> u8 {
        self.inner.opcode
    }

    #[getter]
    fn owner(&self) -> String {
        self.inner.owner.clone()
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
    fn is_interface(&self) -> bool {
        self.inner.is_interface
    }
}

#[pyclass(name = "InterfaceMethodInsn", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyInterfaceMethodInsn {
    inner: InterfaceMethodInsn,
}

#[pymethods]
impl PyInterfaceMethodInsn {
    #[new]
    fn new(owner: String, name: String, descriptor: String) -> Self {
        Self {
            inner: InterfaceMethodInsn {
                owner,
                name,
                descriptor,
            },
        }
    }

    #[getter]
    fn kind(&self) -> &'static str {
        "interface_method"
    }

    #[getter]
    fn owner(&self) -> String {
        self.inner.owner.clone()
    }

    #[getter]
    fn name(&self) -> String {
        self.inner.name.clone()
    }

    #[getter]
    fn descriptor(&self) -> String {
        self.inner.descriptor.clone()
    }
}

#[pyclass(name = "TypeInsn", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyTypeInsn {
    inner: TypeInsn,
}

#[pymethods]
impl PyTypeInsn {
    #[new]
    fn new(opcode: u8, descriptor: String) -> Self {
        Self {
            inner: TypeInsn { opcode, descriptor },
        }
    }

    #[getter]
    fn kind(&self) -> &'static str {
        "type"
    }

    #[getter]
    fn opcode(&self) -> u8 {
        self.inner.opcode
    }

    #[getter]
    fn descriptor(&self) -> String {
        self.inner.descriptor.clone()
    }
}

#[pyclass(name = "VarInsn", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyVarInsn {
    inner: VarInsn,
}

#[pymethods]
impl PyVarInsn {
    #[new]
    fn new(opcode: u8, slot: u16) -> Self {
        Self {
            inner: VarInsn { opcode, slot },
        }
    }

    #[getter]
    fn kind(&self) -> &'static str {
        "var"
    }

    #[getter]
    fn opcode(&self) -> u8 {
        self.inner.opcode
    }

    #[getter]
    fn slot(&self) -> u16 {
        self.inner.slot
    }
}

#[pyclass(name = "IIncInsn", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyIIncInsn {
    inner: IIncInsn,
}

#[pymethods]
impl PyIIncInsn {
    #[new]
    fn new(slot: u16, value: i16) -> Self {
        Self {
            inner: IIncInsn { slot, value },
        }
    }

    #[getter]
    fn kind(&self) -> &'static str {
        "iinc"
    }

    #[getter]
    fn slot(&self) -> u16 {
        self.inner.slot
    }

    #[getter]
    fn value(&self) -> i16 {
        self.inner.value
    }
}

#[pyclass(name = "LdcInsn", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyLdcInsn {
    inner: LdcInsn,
}

#[pymethods]
impl PyLdcInsn {
    #[staticmethod]
    fn int(value: u32) -> Self {
        Self {
            inner: LdcInsn {
                value: LdcValue::Int(value),
            },
        }
    }

    #[staticmethod]
    fn float_bits(value: u32) -> Self {
        Self {
            inner: LdcInsn {
                value: LdcValue::FloatBits(value),
            },
        }
    }

    #[staticmethod]
    fn long(value: u64) -> Self {
        Self {
            inner: LdcInsn {
                value: LdcValue::Long(value),
            },
        }
    }

    #[staticmethod]
    fn double_bits(value: u64) -> Self {
        Self {
            inner: LdcInsn {
                value: LdcValue::DoubleBits(value),
            },
        }
    }

    #[staticmethod]
    fn string(value: String) -> Self {
        Self {
            inner: LdcInsn {
                value: LdcValue::String(value),
            },
        }
    }

    #[staticmethod]
    fn class_value(value: String) -> Self {
        Self {
            inner: LdcInsn {
                value: LdcValue::Class(value),
            },
        }
    }

    #[staticmethod]
    fn method_type(value: String) -> Self {
        Self {
            inner: LdcInsn {
                value: LdcValue::MethodType(value),
            },
        }
    }

    #[staticmethod]
    fn method_handle(
        reference_kind: u8,
        owner: String,
        name: String,
        descriptor: String,
        is_interface: bool,
    ) -> Self {
        Self {
            inner: LdcInsn {
                value: LdcValue::MethodHandle(MethodHandleValue {
                    reference_kind,
                    owner,
                    name,
                    descriptor,
                    is_interface,
                }),
            },
        }
    }

    #[staticmethod]
    fn dynamic(bootstrap_method_attr_index: u16, name: String, descriptor: String) -> Self {
        Self {
            inner: LdcInsn {
                value: LdcValue::Dynamic(DynamicValue {
                    bootstrap_method_attr_index: BootstrapMethodIndex::new(
                        bootstrap_method_attr_index,
                    ),
                    name,
                    descriptor,
                }),
            },
        }
    }

    #[getter]
    fn kind(&self) -> &'static str {
        "ldc"
    }

    #[getter]
    fn value_type(&self) -> &'static str {
        match &self.inner.value {
            LdcValue::Int(_) => "int",
            LdcValue::FloatBits(_) => "float",
            LdcValue::Long(_) => "long",
            LdcValue::DoubleBits(_) => "double",
            LdcValue::String(_) => "string",
            LdcValue::Class(_) => "class",
            LdcValue::MethodType(_) => "method_type",
            LdcValue::MethodHandle(_) => "method_handle",
            LdcValue::Dynamic(_) => "dynamic",
        }
    }

    #[getter]
    fn value(&self, py: Python<'_>) -> PyResult<PyObject> {
        match &self.inner.value {
            LdcValue::Int(value) => Ok(value.into_pyobject(py)?.unbind().into()),
            LdcValue::FloatBits(value) => Ok(value.into_pyobject(py)?.unbind().into()),
            LdcValue::Long(value) => Ok(value.into_pyobject(py)?.unbind().into()),
            LdcValue::DoubleBits(value) => Ok(value.into_pyobject(py)?.unbind().into()),
            LdcValue::String(value) => Ok(value.into_pyobject(py)?.unbind().into()),
            LdcValue::Class(value) => Ok(value.into_pyobject(py)?.unbind().into()),
            LdcValue::MethodType(value) => Ok(value.into_pyobject(py)?.unbind().into()),
            LdcValue::MethodHandle(value) => wrap_method_handle_value(py, value),
            LdcValue::Dynamic(value) => wrap_dynamic_value(py, value),
        }
    }
}

#[pyclass(name = "InvokeDynamicInsn", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyInvokeDynamicInsn {
    inner: InvokeDynamicInsn,
}

#[pymethods]
impl PyInvokeDynamicInsn {
    #[new]
    fn new(bootstrap_method_attr_index: u16, name: String, descriptor: String) -> Self {
        Self {
            inner: InvokeDynamicInsn {
                bootstrap_method_attr_index: BootstrapMethodIndex::new(bootstrap_method_attr_index),
                name,
                descriptor,
            },
        }
    }

    #[getter]
    fn kind(&self) -> &'static str {
        "invokedynamic"
    }

    #[getter]
    fn bootstrap_method_attr_index(&self) -> u16 {
        self.inner.bootstrap_method_attr_index.value()
    }

    #[getter]
    fn name(&self) -> String {
        self.inner.name.clone()
    }

    #[getter]
    fn descriptor(&self) -> String {
        self.inner.descriptor.clone()
    }
}

#[pyclass(name = "MultiANewArrayInsn", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyMultiANewArrayInsn {
    inner: MultiANewArrayInsn,
}

#[pymethods]
impl PyMultiANewArrayInsn {
    #[new]
    fn new(descriptor: String, dimensions: u8) -> Self {
        Self {
            inner: MultiANewArrayInsn {
                descriptor,
                dimensions,
            },
        }
    }

    #[getter]
    fn kind(&self) -> &'static str {
        "multianewarray"
    }

    #[getter]
    fn descriptor(&self) -> String {
        self.inner.descriptor.clone()
    }

    #[getter]
    fn dimensions(&self) -> u8 {
        self.inner.dimensions
    }
}

#[pyclass(name = "BranchInsn", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyBranchInsn {
    inner: BranchInsn,
}

#[pymethods]
impl PyBranchInsn {
    #[new]
    fn new(opcode: u8, target: PyLabel) -> Self {
        Self {
            inner: BranchInsn {
                opcode,
                target: target.inner,
            },
        }
    }

    #[getter]
    fn kind(&self) -> &'static str {
        "branch"
    }

    #[getter]
    fn opcode(&self) -> u8 {
        self.inner.opcode
    }

    #[getter]
    fn target(&self) -> PyLabel {
        PyLabel::from(self.inner.target.clone())
    }
}

#[pyclass(name = "LookupSwitchInsn", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyLookupSwitchInsn {
    inner: LookupSwitchInsn,
}

#[pymethods]
impl PyLookupSwitchInsn {
    #[new]
    fn new(default_target: PyLabel, pairs: Vec<(i32, PyLabel)>) -> Self {
        Self {
            inner: LookupSwitchInsn {
                default_target: default_target.inner,
                pairs: pairs
                    .into_iter()
                    .map(|(key, label)| (key, label.inner))
                    .collect(),
            },
        }
    }

    #[getter]
    fn kind(&self) -> &'static str {
        "lookupswitch"
    }

    #[getter]
    fn default_target(&self) -> PyLabel {
        PyLabel::from(self.inner.default_target.clone())
    }

    #[getter]
    fn pairs(&self) -> Vec<(i32, PyLabel)> {
        self.inner
            .pairs
            .iter()
            .map(|(key, label)| (*key, PyLabel::from(label.clone())))
            .collect()
    }
}

#[pyclass(name = "TableSwitchInsn", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyTableSwitchInsn {
    inner: TableSwitchInsn,
}

#[pymethods]
impl PyTableSwitchInsn {
    #[new]
    fn new(default_target: PyLabel, low: i32, high: i32, targets: Vec<PyLabel>) -> Self {
        Self {
            inner: TableSwitchInsn {
                default_target: default_target.inner,
                low,
                high,
                targets: targets.into_iter().map(|label| label.inner).collect(),
            },
        }
    }

    #[getter]
    fn kind(&self) -> &'static str {
        "tableswitch"
    }

    #[getter]
    fn default_target(&self) -> PyLabel {
        PyLabel::from(self.inner.default_target.clone())
    }

    #[getter]
    fn low(&self) -> i32 {
        self.inner.low
    }

    #[getter]
    fn high(&self) -> i32 {
        self.inner.high
    }

    #[getter]
    fn targets(&self) -> Vec<PyLabel> {
        self.inner
            .targets
            .iter()
            .cloned()
            .map(PyLabel::from)
            .collect()
    }
}

// ---------------------------------------------------------------------------
// CodeItem → Python dict helper
// ---------------------------------------------------------------------------

fn wrap_label(py: Python<'_>, label: &Label) -> PyResult<PyObject> {
    Ok(Py::new(py, PyLabel::from(label.clone()))?.into_any())
}

fn wrap_method_handle_value(py: Python<'_>, value: &MethodHandleValue) -> PyResult<PyObject> {
    Ok(Py::new(
        py,
        PyMethodHandleValue {
            inner: value.clone(),
        },
    )?
    .into_any())
}

fn wrap_dynamic_value(py: Python<'_>, value: &DynamicValue) -> PyResult<PyObject> {
    Ok(Py::new(
        py,
        PyDynamicValue {
            inner: value.clone(),
        },
    )?
    .into_any())
}

fn code_item_value_error(message: impl Into<String>) -> PyErr {
    PyErr::new::<pyo3::exceptions::PyValueError, _>(message.into())
}

fn required_code_item_key<'py>(
    dict: &Bound<'py, PyDict>,
    key: &str,
) -> PyResult<Bound<'py, PyAny>> {
    dict.get_item(key)?
        .ok_or_else(|| code_item_value_error(format!("code item missing required key {key:?}")))
}

fn extract_label(item: &Bound<'_, PyAny>) -> PyResult<Label> {
    item.extract::<PyRef<'_, PyLabel>>()
        .map(|label| label.inner.clone())
        .map_err(|_| code_item_value_error("expected Label for code item label field"))
}

fn extract_code_item_dict(dict: &Bound<'_, PyDict>) -> PyResult<CodeItem> {
    let item_type = required_code_item_key(dict, "type")?.extract::<String>()?;

    match item_type.as_str() {
        "label" => Ok(CodeItem::Label(extract_label(&required_code_item_key(
            dict, "label",
        )?)?)),
        "raw" => Ok(CodeItem::Raw(Instruction::Simple {
            opcode: required_code_item_key(dict, "opcode")?.extract()?,
            offset: 0,
        })),
        "byte" => Ok(CodeItem::Raw(Instruction::Byte {
            opcode: required_code_item_key(dict, "opcode")?.extract()?,
            offset: 0,
            value: required_code_item_key(dict, "value")?.extract()?,
        })),
        "short" => Ok(CodeItem::Raw(Instruction::Short {
            opcode: required_code_item_key(dict, "opcode")?.extract()?,
            offset: 0,
            value: required_code_item_key(dict, "value")?.extract()?,
        })),
        "newarray" => {
            let atype =
                ArrayType::try_from(required_code_item_key(dict, "atype")?.extract::<u8>()?)
                    .map_err(|err| {
                        code_item_value_error(format!("invalid newarray atype: {err}"))
                    })?;
            Ok(CodeItem::Raw(Instruction::NewArray(NewArrayInsn {
                offset: 0,
                atype,
            })))
        }
        "field" => Ok(CodeItem::Field(FieldInsn {
            opcode: required_code_item_key(dict, "opcode")?.extract()?,
            owner: required_code_item_key(dict, "owner")?.extract()?,
            name: required_code_item_key(dict, "name")?.extract()?,
            descriptor: required_code_item_key(dict, "descriptor")?.extract()?,
        })),
        "method" => Ok(CodeItem::Method(MethodInsn {
            opcode: required_code_item_key(dict, "opcode")?.extract()?,
            owner: required_code_item_key(dict, "owner")?.extract()?,
            name: required_code_item_key(dict, "name")?.extract()?,
            descriptor: required_code_item_key(dict, "descriptor")?.extract()?,
            is_interface: required_code_item_key(dict, "is_interface")?.extract()?,
        })),
        "interface_method" => Ok(CodeItem::InterfaceMethod(InterfaceMethodInsn {
            owner: required_code_item_key(dict, "owner")?.extract()?,
            name: required_code_item_key(dict, "name")?.extract()?,
            descriptor: required_code_item_key(dict, "descriptor")?.extract()?,
        })),
        "type" => Ok(CodeItem::Type(TypeInsn {
            opcode: required_code_item_key(dict, "opcode")?.extract()?,
            descriptor: required_code_item_key(dict, "descriptor")?.extract()?,
        })),
        "var" => Ok(CodeItem::Var(VarInsn {
            opcode: required_code_item_key(dict, "opcode")?.extract()?,
            slot: required_code_item_key(dict, "slot")?.extract()?,
        })),
        "iinc" => Ok(CodeItem::IInc(IIncInsn {
            slot: required_code_item_key(dict, "slot")?.extract()?,
            value: required_code_item_key(dict, "value")?.extract()?,
        })),
        "ldc" => {
            let value_type = required_code_item_key(dict, "value_type")?.extract::<String>()?;
            let value = match value_type.as_str() {
                "int" => LdcValue::Int(required_code_item_key(dict, "value")?.extract()?),
                "float" => LdcValue::FloatBits(required_code_item_key(dict, "value")?.extract()?),
                "long" => LdcValue::Long(required_code_item_key(dict, "value")?.extract()?),
                "double" => LdcValue::DoubleBits(required_code_item_key(dict, "value")?.extract()?),
                "string" => LdcValue::String(required_code_item_key(dict, "value")?.extract()?),
                "class" => LdcValue::Class(required_code_item_key(dict, "value")?.extract()?),
                "method_type" => {
                    LdcValue::MethodType(required_code_item_key(dict, "value")?.extract()?)
                }
                "method_handle" => LdcValue::MethodHandle(MethodHandleValue {
                    reference_kind: required_code_item_key(dict, "reference_kind")?.extract()?,
                    owner: required_code_item_key(dict, "owner")?.extract()?,
                    name: required_code_item_key(dict, "name")?.extract()?,
                    descriptor: required_code_item_key(dict, "descriptor")?.extract()?,
                    is_interface: required_code_item_key(dict, "is_interface")?.extract()?,
                }),
                "dynamic" => LdcValue::Dynamic(DynamicValue {
                    bootstrap_method_attr_index: BootstrapMethodIndex::new(
                        required_code_item_key(dict, "bootstrap_method_attr_index")?.extract()?,
                    ),
                    name: required_code_item_key(dict, "name")?.extract()?,
                    descriptor: required_code_item_key(dict, "descriptor")?.extract()?,
                }),
                other => {
                    return Err(code_item_value_error(format!(
                        "unsupported ldc value_type {other:?}"
                    )));
                }
            };
            Ok(CodeItem::Ldc(LdcInsn { value }))
        }
        "invokedynamic" => Ok(CodeItem::InvokeDynamic(InvokeDynamicInsn {
            bootstrap_method_attr_index: BootstrapMethodIndex::new(
                required_code_item_key(dict, "bootstrap_method_attr_index")?.extract()?,
            ),
            name: required_code_item_key(dict, "name")?.extract()?,
            descriptor: required_code_item_key(dict, "descriptor")?.extract()?,
        })),
        "multianewarray" => Ok(CodeItem::MultiANewArray(MultiANewArrayInsn {
            descriptor: required_code_item_key(dict, "descriptor")?.extract()?,
            dimensions: required_code_item_key(dict, "dimensions")?.extract()?,
        })),
        "branch" => Ok(CodeItem::Branch(BranchInsn {
            opcode: required_code_item_key(dict, "opcode")?.extract()?,
            target: extract_label(&required_code_item_key(dict, "target")?)?,
        })),
        "lookupswitch" => {
            let pairs = required_code_item_key(dict, "pairs")?
                .downcast::<PyList>()
                .map_err(|_| code_item_value_error("lookupswitch pairs must be a list"))?
                .iter()
                .map(|entry| {
                    let (key, label_obj): (i32, PyObject) = entry.extract()?;
                    Ok((key, extract_label(label_obj.bind(entry.py()))?))
                })
                .collect::<PyResult<Vec<_>>>()?;
            Ok(CodeItem::LookupSwitch(LookupSwitchInsn {
                default_target: extract_label(&required_code_item_key(dict, "default_target")?)?,
                pairs,
            }))
        }
        "tableswitch" => {
            let targets = required_code_item_key(dict, "targets")?
                .downcast::<PyList>()
                .map_err(|_| code_item_value_error("tableswitch targets must be a list"))?
                .iter()
                .map(|target| extract_label(&target))
                .collect::<PyResult<Vec<_>>>()?;
            Ok(CodeItem::TableSwitch(TableSwitchInsn {
                default_target: extract_label(&required_code_item_key(dict, "default_target")?)?,
                low: required_code_item_key(dict, "low")?.extract()?,
                high: required_code_item_key(dict, "high")?.extract()?,
                targets,
            }))
        }
        other => Err(code_item_value_error(format!(
            "unsupported code item type {other:?}"
        ))),
    }
}

pub(crate) fn extract_code_item(item: &Bound<'_, PyAny>) -> PyResult<CodeItem> {
    if let Ok(label) = item.extract::<PyRef<'_, PyLabel>>() {
        return Ok(CodeItem::Label(label.inner.clone()));
    }
    if let Ok(insn) = item.extract::<PyRef<'_, PyRawInsn>>() {
        return Ok(CodeItem::Raw(Instruction::Simple {
            opcode: insn.opcode,
            offset: 0,
        }));
    }
    if let Ok(insn) = item.extract::<PyRef<'_, PyByteInsn>>() {
        return Ok(CodeItem::Raw(Instruction::Byte {
            opcode: insn.opcode,
            offset: 0,
            value: insn.value,
        }));
    }
    if let Ok(insn) = item.extract::<PyRef<'_, PyShortInsn>>() {
        return Ok(CodeItem::Raw(Instruction::Short {
            opcode: insn.opcode,
            offset: 0,
            value: insn.value,
        }));
    }
    if let Ok(insn) = item.extract::<PyRef<'_, PyNewArrayInsn>>() {
        let atype = ArrayType::try_from(insn.atype)
            .map_err(|err| code_item_value_error(format!("invalid newarray atype: {err}")))?;
        return Ok(CodeItem::Raw(Instruction::NewArray(NewArrayInsn {
            offset: 0,
            atype,
        })));
    }
    if let Ok(insn) = item.extract::<PyRef<'_, PyFieldInsn>>() {
        return Ok(CodeItem::Field(insn.inner.clone()));
    }
    if let Ok(insn) = item.extract::<PyRef<'_, PyMethodInsn>>() {
        return Ok(CodeItem::Method(insn.inner.clone()));
    }
    if let Ok(insn) = item.extract::<PyRef<'_, PyInterfaceMethodInsn>>() {
        return Ok(CodeItem::InterfaceMethod(insn.inner.clone()));
    }
    if let Ok(insn) = item.extract::<PyRef<'_, PyTypeInsn>>() {
        return Ok(CodeItem::Type(insn.inner.clone()));
    }
    if let Ok(insn) = item.extract::<PyRef<'_, PyVarInsn>>() {
        return Ok(CodeItem::Var(insn.inner.clone()));
    }
    if let Ok(insn) = item.extract::<PyRef<'_, PyIIncInsn>>() {
        return Ok(CodeItem::IInc(insn.inner.clone()));
    }
    if let Ok(insn) = item.extract::<PyRef<'_, PyLdcInsn>>() {
        return Ok(CodeItem::Ldc(insn.inner.clone()));
    }
    if let Ok(insn) = item.extract::<PyRef<'_, PyInvokeDynamicInsn>>() {
        return Ok(CodeItem::InvokeDynamic(insn.inner.clone()));
    }
    if let Ok(insn) = item.extract::<PyRef<'_, PyMultiANewArrayInsn>>() {
        return Ok(CodeItem::MultiANewArray(insn.inner.clone()));
    }
    if let Ok(insn) = item.extract::<PyRef<'_, PyBranchInsn>>() {
        return Ok(CodeItem::Branch(insn.inner.clone()));
    }
    if let Ok(insn) = item.extract::<PyRef<'_, PyLookupSwitchInsn>>() {
        return Ok(CodeItem::LookupSwitch(insn.inner.clone()));
    }
    if let Ok(insn) = item.extract::<PyRef<'_, PyTableSwitchInsn>>() {
        return Ok(CodeItem::TableSwitch(insn.inner.clone()));
    }
    if let Ok(dict) = item.downcast::<PyDict>() {
        return extract_code_item_dict(&dict);
    }
    Err(code_item_value_error(
        "code items must be typed instruction objects or legacy dicts produced by CodeModel.instructions",
    ))
}

pub(crate) fn extract_code_items(py: Python<'_>, items: Vec<PyObject>) -> PyResult<Vec<CodeItem>> {
    items
        .into_iter()
        .map(|item| extract_code_item(item.bind(py)))
        .collect()
}

fn wrap_code_item(py: Python<'_>, item: &CodeItem) -> PyResult<PyObject> {
    match item {
        CodeItem::Label(label) => wrap_label(py, label),
        CodeItem::Raw(insn) => match insn {
            Instruction::Byte { opcode, value, .. } => Ok(Py::new(
                py,
                PyByteInsn {
                    opcode: *opcode,
                    value: *value,
                },
            )?
            .into_any()),
            Instruction::Short { opcode, value, .. } => Ok(Py::new(
                py,
                PyShortInsn {
                    opcode: *opcode,
                    value: *value,
                },
            )?
            .into_any()),
            Instruction::NewArray(na) => Ok(Py::new(
                py,
                PyNewArrayInsn {
                    atype: na.atype as u8,
                },
            )?
            .into_any()),
            _ => Ok(Py::new(
                py,
                PyRawInsn {
                    opcode: insn.opcode(),
                },
            )?
            .into_any()),
        },
        CodeItem::Field(f) => Ok(Py::new(py, PyFieldInsn { inner: f.clone() })?.into_any()),
        CodeItem::Method(m) => Ok(Py::new(py, PyMethodInsn { inner: m.clone() })?.into_any()),
        CodeItem::InterfaceMethod(im) => {
            Ok(Py::new(py, PyInterfaceMethodInsn { inner: im.clone() })?.into_any())
        }
        CodeItem::Type(t) => Ok(Py::new(py, PyTypeInsn { inner: t.clone() })?.into_any()),
        CodeItem::Var(v) => Ok(Py::new(py, PyVarInsn { inner: v.clone() })?.into_any()),
        CodeItem::IInc(ii) => Ok(Py::new(py, PyIIncInsn { inner: ii.clone() })?.into_any()),
        CodeItem::Ldc(ldc) => Ok(Py::new(py, PyLdcInsn { inner: ldc.clone() })?.into_any()),
        CodeItem::InvokeDynamic(id) => {
            Ok(Py::new(py, PyInvokeDynamicInsn { inner: id.clone() })?.into_any())
        }
        CodeItem::MultiANewArray(m) => {
            Ok(Py::new(py, PyMultiANewArrayInsn { inner: m.clone() })?.into_any())
        }
        CodeItem::Branch(b) => Ok(Py::new(py, PyBranchInsn { inner: b.clone() })?.into_any()),
        CodeItem::LookupSwitch(ls) => {
            Ok(Py::new(py, PyLookupSwitchInsn { inner: ls.clone() })?.into_any())
        }
        CodeItem::TableSwitch(ts) => {
            Ok(Py::new(py, PyTableSwitchInsn { inner: ts.clone() })?.into_any())
        }
    }
}

// ---------------------------------------------------------------------------
// Shared bridge access
// ---------------------------------------------------------------------------

#[derive(Clone)]
enum FieldAccess {
    Shared {
        state: SharedClassState,
        index: usize,
        generation: u64,
    },
    Owned(Rc<RefCell<FieldModel>>),
}

#[derive(Clone)]
enum MethodAccess {
    Shared {
        state: SharedClassState,
        index: usize,
        generation: u64,
    },
    Owned(Rc<RefCell<MethodModel>>),
}

#[derive(Clone)]
enum CodeAccess {
    Shared {
        state: SharedClassState,
        method_index: usize,
        methods_generation: u64,
    },
    OwnedMethod(Rc<RefCell<MethodModel>>),
}

#[derive(Clone)]
enum ConstantPoolAccess {
    Owned(ConstantPoolBuilder),
    Shared(SharedClassState),
}

#[derive(Clone)]
enum AttributeOwner {
    Class(SharedClassState),
    Field {
        state: SharedClassState,
        index: usize,
        generation: u64,
    },
    Method {
        state: SharedClassState,
        index: usize,
        generation: u64,
    },
    Code {
        state: SharedClassState,
        method_index: usize,
        methods_generation: u64,
    },
    OwnedField(Rc<RefCell<FieldModel>>),
    OwnedMethod(Rc<RefCell<MethodModel>>),
}

#[derive(Clone, Copy)]
enum CodeListKind {
    Instructions,
    ExceptionHandlers,
    LineNumbers,
    LocalVariables,
    LocalVariableTypes,
}

// ---------------------------------------------------------------------------
// View objects
// ---------------------------------------------------------------------------

#[pyclass(name = "StringListView", module = "pytecode._rust", unsendable)]
#[derive(Clone)]
pub struct PyStringListView {
    state: SharedClassState,
    generation: u64,
}

#[pymethods]
impl PyStringListView {
    fn __len__(&self) -> PyResult<usize> {
        with_live_model(&self.state, |model_state| {
            if model_state.interfaces_generation != self.generation {
                return Err(stale_ref_err("interface view"));
            }
            Ok(model_state.inner.as_ref().unwrap().interfaces.len())
        })
    }

    fn __getitem__(&self, index: isize) -> PyResult<String> {
        with_live_model(&self.state, |model_state| {
            if model_state.interfaces_generation != self.generation {
                return Err(stale_ref_err("interface view"));
            }
            let interfaces = &model_state.inner.as_ref().unwrap().interfaces;
            Ok(interfaces[normalize_index(index, interfaces.len())?].clone())
        })
    }

    fn to_list(&self) -> PyResult<Vec<String>> {
        with_live_model(&self.state, |model_state| {
            if model_state.interfaces_generation != self.generation {
                return Err(stale_ref_err("interface view"));
            }
            Ok(model_state.inner.as_ref().unwrap().interfaces.clone())
        })
    }
}

#[pyclass(name = "FieldListView", module = "pytecode._rust", unsendable)]
#[derive(Clone)]
pub struct PyFieldListView {
    state: SharedClassState,
    generation: u64,
}

#[pymethods]
impl PyFieldListView {
    fn __len__(&self) -> PyResult<usize> {
        with_live_model(&self.state, |model_state| {
            if model_state.fields_generation != self.generation {
                return Err(stale_ref_err("field view"));
            }
            Ok(model_state.inner.as_ref().unwrap().fields.len())
        })
    }

    fn __getitem__(&self, index: isize) -> PyResult<PyFieldModel> {
        with_live_model(&self.state, |model_state| {
            if model_state.fields_generation != self.generation {
                return Err(stale_ref_err("field view"));
            }
            let fields = &model_state.inner.as_ref().unwrap().fields;
            Ok(PyFieldModel {
                access: FieldAccess::Shared {
                    state: self.state.clone(),
                    index: normalize_index(index, fields.len())?,
                    generation: self.generation,
                },
            })
        })
    }

    fn to_list(&self) -> PyResult<Vec<PyFieldModel>> {
        let len = self.__len__()?;
        (0..len)
            .map(|index| self.__getitem__(index as isize))
            .collect::<PyResult<Vec<_>>>()
    }
}

#[pyclass(name = "MethodListView", module = "pytecode._rust", unsendable)]
#[derive(Clone)]
pub struct PyMethodListView {
    state: SharedClassState,
    generation: u64,
}

#[pymethods]
impl PyMethodListView {
    fn __len__(&self) -> PyResult<usize> {
        with_live_model(&self.state, |model_state| {
            if model_state.methods_generation != self.generation {
                return Err(stale_ref_err("method view"));
            }
            Ok(model_state.inner.as_ref().unwrap().methods.len())
        })
    }

    fn __getitem__(&self, index: isize) -> PyResult<PyMethodModel> {
        with_live_model(&self.state, |model_state| {
            if model_state.methods_generation != self.generation {
                return Err(stale_ref_err("method view"));
            }
            let methods = &model_state.inner.as_ref().unwrap().methods;
            Ok(PyMethodModel {
                access: MethodAccess::Shared {
                    state: self.state.clone(),
                    index: normalize_index(index, methods.len())?,
                    generation: self.generation,
                },
            })
        })
    }

    fn to_list(&self) -> PyResult<Vec<PyMethodModel>> {
        let len = self.__len__()?;
        (0..len)
            .map(|index| self.__getitem__(index as isize))
            .collect::<PyResult<Vec<_>>>()
    }
}

impl AttributeOwner {
    fn len(&self) -> PyResult<usize> {
        match self {
            AttributeOwner::Class(state) => with_live_model(state, |model_state| {
                Ok(model_state.inner.as_ref().unwrap().attributes.len())
            }),
            AttributeOwner::Field {
                state,
                index,
                generation,
            } => with_live_model(state, |model_state| {
                if model_state.fields_generation != *generation {
                    return Err(stale_ref_err("field attribute view"));
                }
                Ok(model_state.inner.as_ref().unwrap().fields[*index]
                    .attributes
                    .len())
            }),
            AttributeOwner::Method {
                state,
                index,
                generation,
            } => with_live_model(state, |model_state| {
                if model_state.methods_generation != *generation {
                    return Err(stale_ref_err("method attribute view"));
                }
                Ok(model_state.inner.as_ref().unwrap().methods[*index]
                    .attributes
                    .len())
            }),
            AttributeOwner::Code {
                state,
                method_index,
                methods_generation,
            } => with_live_model(state, |model_state| {
                if model_state.methods_generation != *methods_generation {
                    return Err(stale_ref_err("code attribute view"));
                }
                let method = &model_state.inner.as_ref().unwrap().methods[*method_index];
                let code = method
                    .code
                    .as_ref()
                    .ok_or_else(|| PyErr::new::<PyRuntimeError, _>("method has no code"))?;
                Ok(code.attributes.len())
            }),
            AttributeOwner::OwnedField(field) => Ok(field.borrow().attributes.len()),
            AttributeOwner::OwnedMethod(method) => Ok(method.borrow().attributes.len()),
        }
    }

    fn get(&self, py: Python<'_>, index: isize) -> PyResult<PyObject> {
        match self {
            AttributeOwner::Class(state) => with_live_model(state, |model_state| {
                let attrs = &model_state.inner.as_ref().unwrap().attributes;
                crate::wrap_attribute(py, &attrs[normalize_index(index, attrs.len())?])
            }),
            AttributeOwner::Field {
                state,
                index: field_index,
                generation,
            } => with_live_model(state, |model_state| {
                if model_state.fields_generation != *generation {
                    return Err(stale_ref_err("field attribute view"));
                }
                let attrs = &model_state.inner.as_ref().unwrap().fields[*field_index].attributes;
                crate::wrap_attribute(py, &attrs[normalize_index(index, attrs.len())?])
            }),
            AttributeOwner::Method {
                state,
                index: method_index,
                generation,
            } => with_live_model(state, |model_state| {
                if model_state.methods_generation != *generation {
                    return Err(stale_ref_err("method attribute view"));
                }
                let attrs = &model_state.inner.as_ref().unwrap().methods[*method_index].attributes;
                crate::wrap_attribute(py, &attrs[normalize_index(index, attrs.len())?])
            }),
            AttributeOwner::Code {
                state,
                method_index,
                methods_generation,
            } => with_live_model(state, |model_state| {
                if model_state.methods_generation != *methods_generation {
                    return Err(stale_ref_err("code attribute view"));
                }
                let method = &model_state.inner.as_ref().unwrap().methods[*method_index];
                let code = method
                    .code
                    .as_ref()
                    .ok_or_else(|| PyErr::new::<PyRuntimeError, _>("method has no code"))?;
                let attrs = &code.attributes;
                crate::wrap_attribute(py, &attrs[normalize_index(index, attrs.len())?])
            }),
            AttributeOwner::OwnedField(field) => {
                let borrowed = field.borrow();
                let attrs = &borrowed.attributes;
                crate::wrap_attribute(py, &attrs[normalize_index(index, attrs.len())?])
            }
            AttributeOwner::OwnedMethod(method) => {
                let borrowed = method.borrow();
                let attrs = &borrowed.attributes;
                crate::wrap_attribute(py, &attrs[normalize_index(index, attrs.len())?])
            }
        }
    }
}

#[pyclass(name = "AttributeListView", module = "pytecode._rust", unsendable)]
#[derive(Clone)]
pub struct PyAttributeListView {
    owner: AttributeOwner,
}

#[pymethods]
impl PyAttributeListView {
    fn __len__(&self) -> PyResult<usize> {
        self.owner.len()
    }

    fn __getitem__(&self, py: Python<'_>, index: isize) -> PyResult<PyObject> {
        self.owner.get(py, index)
    }

    fn to_list(&self, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        let len = self.__len__()?;
        (0..len)
            .map(|index| self.__getitem__(py, index as isize))
            .collect::<PyResult<Vec<_>>>()
    }
}

#[pyclass(name = "CodeListView", module = "pytecode._rust", unsendable)]
#[derive(Clone)]
pub struct PyCodeListView {
    access: CodeAccess,
    kind: CodeListKind,
}

#[pymethods]
impl PyCodeListView {
    fn __len__(&self) -> PyResult<usize> {
        self.with_code(|code| {
            Ok(match self.kind {
                CodeListKind::Instructions => code.instructions.len(),
                CodeListKind::ExceptionHandlers => code.exception_handlers.len(),
                CodeListKind::LineNumbers => code.line_numbers.len(),
                CodeListKind::LocalVariables => code.local_variables.len(),
                CodeListKind::LocalVariableTypes => code.local_variable_types.len(),
            })
        })
    }

    fn __getitem__(&self, py: Python<'_>, index: isize) -> PyResult<PyObject> {
        self.with_code(|code| match self.kind {
            CodeListKind::Instructions => wrap_code_item(
                py,
                &code.instructions[normalize_index(index, code.instructions.len())?],
            ),
            CodeListKind::ExceptionHandlers => {
                let item = &code.exception_handlers
                    [normalize_index(index, code.exception_handlers.len())?];
                Ok(Py::new(
                    py,
                    PyModelExceptionHandler {
                        inner: item.clone(),
                    },
                )?
                .into_any())
            }
            CodeListKind::LineNumbers => {
                let item = &code.line_numbers[normalize_index(index, code.line_numbers.len())?];
                wrap_line_number(py, &item.label, item.line_number)
            }
            CodeListKind::LocalVariables => {
                let item =
                    &code.local_variables[normalize_index(index, code.local_variables.len())?];
                wrap_local_variable(
                    py,
                    &item.start,
                    &item.end,
                    &item.name,
                    &item.descriptor,
                    item.index,
                )
            }
            CodeListKind::LocalVariableTypes => {
                let item = &code.local_variable_types
                    [normalize_index(index, code.local_variable_types.len())?];
                wrap_local_variable_type(
                    py,
                    &item.start,
                    &item.end,
                    &item.name,
                    &item.signature,
                    item.index,
                )
            }
        })
    }

    fn to_list(&self, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        let len = self.__len__()?;
        (0..len)
            .map(|index| self.__getitem__(py, index as isize))
            .collect::<PyResult<Vec<_>>>()
    }
}

impl PyCodeListView {
    fn with_code<R>(&self, f: impl FnOnce(&CodeModel) -> PyResult<R>) -> PyResult<R> {
        match &self.access {
            CodeAccess::Shared {
                state,
                method_index,
                methods_generation,
            } => with_live_model(state, |model_state| {
                if model_state.methods_generation != *methods_generation {
                    return Err(stale_ref_err("code view"));
                }
                let method = &model_state.inner.as_ref().unwrap().methods[*method_index];
                let code = method
                    .code
                    .as_ref()
                    .ok_or_else(|| PyErr::new::<PyRuntimeError, _>("method has no code"))?;
                f(code)
            }),
            CodeAccess::OwnedMethod(method) => {
                let borrowed = method.borrow();
                let code = borrowed
                    .code
                    .as_ref()
                    .ok_or_else(|| PyErr::new::<PyRuntimeError, _>("method has no code"))?;
                f(code)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// PyCodeModel
// ---------------------------------------------------------------------------

#[pyclass(name = "CodeModel", module = "pytecode._rust", unsendable)]
#[derive(Clone)]
pub struct PyCodeModel {
    access: CodeAccess,
}

impl PyCodeModel {
    fn with_code<R>(&self, f: impl FnOnce(&CodeModel) -> PyResult<R>) -> PyResult<R> {
        match &self.access {
            CodeAccess::Shared {
                state,
                method_index,
                methods_generation,
            } => with_live_model(state, |model_state| {
                if model_state.methods_generation != *methods_generation {
                    return Err(stale_ref_err("code ref"));
                }
                let method = &model_state.inner.as_ref().unwrap().methods[*method_index];
                let code = method
                    .code
                    .as_ref()
                    .ok_or_else(|| PyErr::new::<PyRuntimeError, _>("method has no code"))?;
                f(code)
            }),
            CodeAccess::OwnedMethod(method) => {
                let borrowed = method.borrow();
                let code = borrowed
                    .code
                    .as_ref()
                    .ok_or_else(|| PyErr::new::<PyRuntimeError, _>("method has no code"))?;
                f(code)
            }
        }
    }
}

#[pymethods]
impl PyCodeModel {
    #[getter]
    fn max_stack(&self) -> PyResult<u16> {
        self.with_code(|code| Ok(code.max_stack))
    }

    #[getter]
    fn max_locals(&self) -> PyResult<u16> {
        self.with_code(|code| Ok(code.max_locals))
    }

    #[getter]
    fn instructions(&self) -> PyCodeListView {
        PyCodeListView {
            access: self.access.clone(),
            kind: CodeListKind::Instructions,
        }
    }

    #[getter]
    fn exception_handlers(&self) -> PyCodeListView {
        PyCodeListView {
            access: self.access.clone(),
            kind: CodeListKind::ExceptionHandlers,
        }
    }

    #[getter]
    fn line_numbers(&self) -> PyCodeListView {
        PyCodeListView {
            access: self.access.clone(),
            kind: CodeListKind::LineNumbers,
        }
    }

    #[getter]
    fn local_variables(&self) -> PyCodeListView {
        PyCodeListView {
            access: self.access.clone(),
            kind: CodeListKind::LocalVariables,
        }
    }

    #[getter]
    fn local_variable_types(&self) -> PyCodeListView {
        PyCodeListView {
            access: self.access.clone(),
            kind: CodeListKind::LocalVariableTypes,
        }
    }

    #[getter]
    fn attributes(&self) -> PyAttributeListView {
        let owner = match &self.access {
            CodeAccess::Shared {
                state,
                method_index,
                methods_generation,
            } => AttributeOwner::Code {
                state: state.clone(),
                method_index: *method_index,
                methods_generation: *methods_generation,
            },
            CodeAccess::OwnedMethod(method) => AttributeOwner::OwnedMethod(method.clone()),
        };
        PyAttributeListView { owner }
    }

    #[getter]
    fn debug_info_state(&self) -> PyResult<&'static str> {
        self.with_code(|code| {
            Ok(match code.debug_info_state {
                DebugInfoState::Fresh => "fresh",
                DebugInfoState::Stale => "stale",
            })
        })
    }

    #[getter]
    fn nested_attribute_layout(&self) -> PyResult<Vec<String>> {
        self.with_code(|code| {
            Ok(code
                .nested_attribute_layout()
                .iter()
                .map(|s| s.to_string())
                .collect())
        })
    }

    fn find_insns(&self, matcher: &PyInsnMatcher) -> PyResult<Vec<usize>> {
        let matcher = matcher.spec.compile();
        self.with_code(|code| {
            Ok(code
                .instructions
                .iter()
                .enumerate()
                .filter_map(|(index, item)| matcher.matches(item).then_some(index))
                .collect())
        })
    }

    #[pyo3(signature = (matcher, start=0))]
    fn find_insn(&self, matcher: &PyInsnMatcher, start: usize) -> PyResult<Option<usize>> {
        let matcher = matcher.spec.compile();
        self.with_code(|code| {
            Ok(code
                .instructions
                .iter()
                .enumerate()
                .skip(start)
                .find_map(|(index, item)| matcher.matches(item).then_some(index)))
        })
    }

    fn contains_insn(&self, matcher: &PyInsnMatcher) -> PyResult<bool> {
        let matcher = matcher.spec.compile();
        self.with_code(|code| Ok(code.instructions.iter().any(|item| matcher.matches(item))))
    }

    fn count_insns(&self, matcher: &PyInsnMatcher) -> PyResult<usize> {
        let matcher = matcher.spec.compile();
        self.with_code(|code| {
            Ok(code
                .instructions
                .iter()
                .filter(|item| matcher.matches(item))
                .count())
        })
    }

    fn find_sequences(&self, matchers: Vec<PyInsnMatcher>) -> PyResult<Vec<usize>> {
        let matchers = matchers
            .into_iter()
            .map(|matcher| matcher.spec.compile())
            .collect::<Vec<_>>();
        self.with_code(|code| {
            if matchers.is_empty() || matchers.len() > code.instructions.len() {
                return Ok(Vec::new());
            }
            Ok(code
                .instructions
                .windows(matchers.len())
                .enumerate()
                .filter_map(|(index, window)| {
                    matchers
                        .iter()
                        .zip(window.iter())
                        .all(|(matcher, item)| matcher.matches(item))
                        .then_some(index)
                })
                .collect())
        })
    }
}

// ---------------------------------------------------------------------------
// PyFieldModel
// ---------------------------------------------------------------------------

#[pyclass(name = "FieldModel", module = "pytecode._rust", unsendable)]
#[derive(Clone)]
pub struct PyFieldModel {
    access: FieldAccess,
}

impl PyFieldModel {
    fn with_field<R>(&self, f: impl FnOnce(&FieldModel) -> PyResult<R>) -> PyResult<R> {
        match &self.access {
            FieldAccess::Shared {
                state,
                index,
                generation,
            } => with_live_model(state, |model_state| {
                if model_state.fields_generation != *generation {
                    return Err(stale_ref_err("field ref"));
                }
                f(&model_state.inner.as_ref().unwrap().fields[*index])
            }),
            FieldAccess::Owned(field) => f(&field.borrow()),
        }
    }

    fn with_field_mut<R>(&self, f: impl FnOnce(&mut FieldModel) -> PyResult<R>) -> PyResult<R> {
        match &self.access {
            FieldAccess::Shared {
                state,
                index,
                generation,
            } => {
                let (state, index, generation) = (state.clone(), *index, *generation);
                with_live_model_mut(&state, |model_state| {
                    if model_state.fields_generation != generation {
                        return Err(stale_ref_err("field ref"));
                    }
                    f(&mut model_state.inner.as_mut().unwrap().fields[index])
                })
            }
            FieldAccess::Owned(field) => {
                let mut borrowed = field.borrow_mut();
                f(&mut borrowed)
            }
        }
    }

    fn clone_inner(&self) -> PyResult<FieldModel> {
        self.with_field(|field| Ok(field.clone()))
    }
}

#[pymethods]
impl PyFieldModel {
    #[new]
    fn new(name: String, descriptor: String, access_flags: u16) -> Self {
        let field = FieldModel {
            access_flags: pytecode_engine::constants::FieldAccessFlags::from_bits_truncate(
                access_flags,
            ),
            name,
            descriptor,
            attributes: Vec::new(),
        };
        Self {
            access: FieldAccess::Owned(Rc::new(RefCell::new(field))),
        }
    }

    #[getter]
    fn access_flags(&self) -> PyResult<u16> {
        self.with_field(|field| Ok(field.access_flags.bits()))
    }

    #[setter]
    fn set_access_flags(&mut self, value: u16) -> PyResult<()> {
        self.with_field_mut(|field| {
            field.access_flags =
                pytecode_engine::constants::FieldAccessFlags::from_bits_truncate(value);
            Ok(())
        })
    }

    #[getter]
    fn name(&self) -> PyResult<String> {
        self.with_field(|field| Ok(field.name.clone()))
    }

    #[setter]
    fn set_name(&mut self, value: String) -> PyResult<()> {
        self.with_field_mut(|field| {
            field.name = value;
            Ok(())
        })
    }

    #[getter]
    fn descriptor(&self) -> PyResult<String> {
        self.with_field(|field| Ok(field.descriptor.clone()))
    }

    #[setter]
    fn set_descriptor(&mut self, value: String) -> PyResult<()> {
        self.with_field_mut(|field| {
            field.descriptor = value;
            Ok(())
        })
    }

    #[getter]
    fn attributes(&self) -> PyAttributeListView {
        let owner = match &self.access {
            FieldAccess::Shared {
                state,
                index,
                generation,
            } => AttributeOwner::Field {
                state: state.clone(),
                index: *index,
                generation: *generation,
            },
            FieldAccess::Owned(field) => AttributeOwner::OwnedField(field.clone()),
        };
        PyAttributeListView { owner }
    }
}

// ---------------------------------------------------------------------------
// PyMethodModel
// ---------------------------------------------------------------------------

#[pyclass(name = "MethodModel", module = "pytecode._rust", unsendable)]
#[derive(Clone)]
pub struct PyMethodModel {
    access: MethodAccess,
}

impl PyMethodModel {
    fn with_method<R>(&self, f: impl FnOnce(&MethodModel) -> PyResult<R>) -> PyResult<R> {
        match &self.access {
            MethodAccess::Shared {
                state,
                index,
                generation,
            } => with_live_model(state, |model_state| {
                if model_state.methods_generation != *generation {
                    return Err(stale_ref_err("method ref"));
                }
                f(&model_state.inner.as_ref().unwrap().methods[*index])
            }),
            MethodAccess::Owned(method) => f(&method.borrow()),
        }
    }

    fn with_method_mut<R>(&self, f: impl FnOnce(&mut MethodModel) -> PyResult<R>) -> PyResult<R> {
        match &self.access {
            MethodAccess::Shared {
                state,
                index,
                generation,
            } => {
                let (state, index, generation) = (state.clone(), *index, *generation);
                with_live_model_mut(&state, |model_state| {
                    if model_state.methods_generation != generation {
                        return Err(stale_ref_err("method ref"));
                    }
                    f(&mut model_state.inner.as_mut().unwrap().methods[index])
                })
            }
            MethodAccess::Owned(method) => {
                let mut borrowed = method.borrow_mut();
                f(&mut borrowed)
            }
        }
    }

    fn clone_inner(&self) -> PyResult<MethodModel> {
        self.with_method(|method| Ok(method.clone()))
    }
}

#[pymethods]
impl PyMethodModel {
    #[new]
    fn new(name: String, descriptor: String, access_flags: u16) -> Self {
        let method = MethodModel::new(
            pytecode_engine::constants::MethodAccessFlags::from_bits_truncate(access_flags),
            name,
            descriptor,
            None,
            Vec::new(),
        );
        Self {
            access: MethodAccess::Owned(Rc::new(RefCell::new(method))),
        }
    }

    #[getter]
    fn access_flags(&self) -> PyResult<u16> {
        self.with_method(|method| Ok(method.access_flags.bits()))
    }

    #[setter]
    fn set_access_flags(&mut self, value: u16) -> PyResult<()> {
        self.with_method_mut(|method| {
            method.access_flags =
                pytecode_engine::constants::MethodAccessFlags::from_bits_truncate(value);
            Ok(())
        })
    }

    #[getter]
    fn name(&self) -> PyResult<String> {
        self.with_method(|method| Ok(method.name.clone()))
    }

    #[setter]
    fn set_name(&mut self, value: String) -> PyResult<()> {
        self.with_method_mut(|method| {
            method.name = value;
            Ok(())
        })
    }

    #[getter]
    fn descriptor(&self) -> PyResult<String> {
        self.with_method(|method| Ok(method.descriptor.clone()))
    }

    #[setter]
    fn set_descriptor(&mut self, value: String) -> PyResult<()> {
        self.with_method_mut(|method| {
            method.descriptor = value;
            Ok(())
        })
    }

    #[getter]
    fn code(&self) -> PyResult<Option<PyCodeModel>> {
        match &self.access {
            MethodAccess::Shared {
                state,
                index,
                generation,
            } => with_live_model(state, |model_state| {
                if model_state.methods_generation != *generation {
                    return Err(stale_ref_err("method ref"));
                }
                let method = &model_state.inner.as_ref().unwrap().methods[*index];
                Ok(method.code.as_ref().map(|_| PyCodeModel {
                    access: CodeAccess::Shared {
                        state: state.clone(),
                        method_index: *index,
                        methods_generation: *generation,
                    },
                }))
            }),
            MethodAccess::Owned(method) => {
                let has_code = method.borrow().code.is_some();
                Ok(has_code.then(|| PyCodeModel {
                    access: CodeAccess::OwnedMethod(method.clone()),
                }))
            }
        }
    }

    fn set_raw_code(&mut self, max_stack: u16, max_locals: u16, code_bytes: &[u8]) -> PyResult<()> {
        let instructions = pytecode_engine::parse_instructions(code_bytes)
            .map_err(|e| PyErr::new::<PyRuntimeError, _>(e.to_string()))?;
        let mut code_model = CodeModel::new(max_stack, max_locals, DebugInfoState::Fresh);
        code_model.instructions = instructions.into_iter().map(CodeItem::Raw).collect();
        self.with_method_mut(|method| {
            method.code = Some(code_model);
            Ok(())
        })
    }

    fn set_prebuilt_code(&mut self, code_body_bytes: &[u8]) -> PyResult<()> {
        self.with_method_mut(|method| {
            method.set_prebuilt_code_bytes(code_body_bytes.to_vec());
            Ok(())
        })
    }

    #[getter]
    fn attributes(&self) -> PyAttributeListView {
        let owner = match &self.access {
            MethodAccess::Shared {
                state,
                index,
                generation,
            } => AttributeOwner::Method {
                state: state.clone(),
                index: *index,
                generation: *generation,
            },
            MethodAccess::Owned(method) => AttributeOwner::OwnedMethod(method.clone()),
        };
        PyAttributeListView { owner }
    }
}

// ---------------------------------------------------------------------------
// PyConstantPoolBuilder
// ---------------------------------------------------------------------------

#[pyclass(name = "ConstantPoolBuilder", module = "pytecode._rust", unsendable)]
#[derive(Clone)]
pub struct PyConstantPoolBuilder {
    access: ConstantPoolAccess,
}

impl PyConstantPoolBuilder {
    fn from_shared(state: SharedClassState) -> Self {
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

    fn add_utf8(&mut self, value: &str) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_utf8(value)
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

    fn add_string(&mut self, value: &str) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_string(value)
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

    fn resolve_utf8(&self, index: u16) -> PyResult<String> {
        self.with_builder(|builder| {
            builder
                .resolve_utf8(pytecode_engine::indexes::Utf8Index::from(index))
                .map_err(crate::engine_error_to_py)
        })
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

// ---------------------------------------------------------------------------
// PyMappingClassResolver
// ---------------------------------------------------------------------------

#[pyclass(name = "MappingClassResolver", module = "pytecode._rust")]
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

    #[staticmethod]
    fn from_models(py: Python<'_>, models: Vec<Py<PyClassModel>>) -> PyResult<Self> {
        let models = models
            .into_iter()
            .map(|model| {
                let model = model.bind(py).borrow();
                model.with_class_model(|inner| Ok(inner.clone()))
            })
            .collect::<PyResult<Vec<_>>>()?;
        let resolver = MappingClassResolver::from_models(models).map_err(analysis_error_to_py)?;
        Ok(Self { inner: resolver })
    }

    fn resolve_class(&self, py: Python<'_>, name: &str) -> PyResult<Option<PyObject>> {
        use pytecode_engine::analysis::ClassResolver;
        match self.inner.resolve_class(name) {
            Some(rc) => Ok(Some(resolved_class_to_py(py, &rc)?)),
            None => Ok(None),
        }
    }
}

// ---------------------------------------------------------------------------
// PyClassModel
// ---------------------------------------------------------------------------

pub(crate) fn parse_debug_info_policy(s: &str) -> PyResult<DebugInfoPolicy> {
    match s {
        "preserve" => Ok(DebugInfoPolicy::Preserve),
        "strip" => Ok(DebugInfoPolicy::Strip),
        _ => Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
            "invalid debug info policy: {s:?}, expected \"preserve\" or \"strip\""
        ))),
    }
}

fn parse_frame_mode_str(s: &str) -> PyResult<FrameComputationMode> {
    match s {
        "preserve" => Ok(FrameComputationMode::Preserve),
        "recompute" => Ok(FrameComputationMode::Recompute),
        _ => Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
            "invalid frame mode: {s:?}, expected \"preserve\" or \"recompute\""
        ))),
    }
}

fn parse_frame_mode_value(frame_mode: &Bound<'_, PyAny>) -> PyResult<FrameComputationMode> {
    let value = frame_mode
        .getattr("value")
        .map_err(|_| {
            PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                "frame_mode must be a pytecode.archive.FrameComputationMode",
            )
        })?
        .extract::<String>()
        .map_err(|_| {
            PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                "frame_mode must be a pytecode.archive.FrameComputationMode",
            )
        })?;
    parse_frame_mode_str(&value)
}

pub(crate) fn parse_frame_computation_mode(
    frame_mode: Option<&Bound<'_, PyAny>>,
) -> PyResult<FrameComputationMode> {
    Ok(match frame_mode {
        Some(value) => parse_frame_mode_value(value)?,
        None => FrameComputationMode::Preserve,
    })
}

fn lower_class_model_bytes(
    model: &PyClassModel,
    debug_info: DebugInfoPolicy,
    frame_mode: FrameComputationMode,
    resolver: Option<&dyn pytecode_engine::analysis::ClassResolver>,
) -> PyResult<Vec<u8>> {
    if debug_info == DebugInfoPolicy::Preserve
        && frame_mode == FrameComputationMode::Preserve
        && resolver.is_none()
    {
        return with_live_model(&model.state, |model_state| {
            model_state
                .inner
                .as_ref()
                .unwrap()
                .to_bytes()
                .map_err(crate::engine_error_to_py)
        });
    }

    write_class(&lower_class_model(model, debug_info, frame_mode, resolver)?)
        .map_err(crate::engine_error_to_py)
}

fn lower_class_model(
    model: &PyClassModel,
    debug_info: DebugInfoPolicy,
    frame_mode: FrameComputationMode,
    resolver: Option<&dyn pytecode_engine::analysis::ClassResolver>,
) -> PyResult<ClassFile> {
    with_live_model(&model.state, |model_state| {
        model_state
            .inner
            .as_ref()
            .unwrap()
            .to_classfile_with_options(debug_info, frame_mode, resolver)
            .map_err(crate::engine_error_to_py)
    })
}

#[pyfunction]
#[pyo3(signature = (models, frame_mode = None, resolver = None, debug_info = "preserve"))]
fn rust_lower_classmodels(
    models: &Bound<'_, PyList>,
    frame_mode: Option<&Bound<'_, PyAny>>,
    resolver: Option<&PyMappingClassResolver>,
    debug_info: &str,
) -> PyResult<Vec<PyClassFile>> {
    let policy = parse_debug_info_policy(debug_info)?;
    let frame_mode = parse_frame_computation_mode(frame_mode)?;
    let resolver =
        resolver.map(|value| &value.inner as &dyn pytecode_engine::analysis::ClassResolver);

    models
        .iter()
        .map(|item| {
            let model: PyRef<'_, PyClassModel> = item.extract()?;
            Ok(PyClassFile {
                inner: Arc::new(lower_class_model(&model, policy, frame_mode, resolver)?),
            })
        })
        .collect()
}

#[pyfunction]
#[pyo3(signature = (models, frame_mode = None, resolver = None, debug_info = "preserve"))]
fn rust_lower_classmodels_to_bytes<'py>(
    py: Python<'py>,
    models: &Bound<'py, PyList>,
    frame_mode: Option<&Bound<'py, PyAny>>,
    resolver: Option<&PyMappingClassResolver>,
    debug_info: &str,
) -> PyResult<Vec<Py<PyBytes>>> {
    let policy = parse_debug_info_policy(debug_info)?;
    let frame_mode = parse_frame_computation_mode(frame_mode)?;
    let resolver =
        resolver.map(|value| &value.inner as &dyn pytecode_engine::analysis::ClassResolver);

    models
        .iter()
        .map(|item| {
            let model: PyRef<'_, PyClassModel> = item.extract()?;
            let bytes = lower_class_model_bytes(&model, policy, frame_mode, resolver)?;
            Ok(PyBytes::new(py, &bytes).unbind())
        })
        .collect()
}

#[pyclass(name = "ClassModel", module = "pytecode._rust", unsendable)]
#[derive(Clone)]
pub struct PyClassModel {
    state: SharedClassState,
}

impl PyClassModel {
    pub(crate) fn from_model(inner: ClassModel) -> Self {
        Self {
            state: Rc::new(RefCell::new(ClassModelState::new(inner))),
        }
    }

    pub(crate) fn with_class_model<R>(
        &self,
        f: impl FnOnce(&ClassModel) -> PyResult<R>,
    ) -> PyResult<R> {
        with_live_model(&self.state, |model_state| {
            f(model_state.inner.as_ref().unwrap())
        })
    }

    pub(crate) fn with_class_model_mut<R>(
        &mut self,
        f: impl FnOnce(&mut ClassModel) -> PyResult<R>,
    ) -> PyResult<R> {
        with_live_model_mut(&self.state, |model_state| {
            f(model_state.inner.as_mut().unwrap())
        })
    }

    pub(crate) fn take_inner(&mut self) -> PyResult<ClassModel> {
        let mut borrowed = self.state.borrow_mut();
        borrowed.inner.take().ok_or_else(dead_model_err)
    }
}

#[pymethods]
impl PyClassModel {
    // -- constructors -------------------------------------------------------

    #[staticmethod]
    fn from_bytes(data: &[u8]) -> PyResult<Self> {
        let model = ClassModel::from_bytes(data).map_err(crate::engine_error_to_py)?;
        Ok(Self::from_model(model))
    }

    // -- serialisation ------------------------------------------------------

    fn to_bytes<'py>(&self, py: Python<'py>) -> PyResult<Py<PyBytes>> {
        let bytes = lower_class_model_bytes(
            self,
            DebugInfoPolicy::Preserve,
            FrameComputationMode::Preserve,
            None,
        )?;
        Ok(PyBytes::new(py, &bytes).unbind())
    }

    fn to_classfile(&self) -> PyResult<PyClassFile> {
        Ok(PyClassFile {
            inner: Arc::new(lower_class_model(
                self,
                DebugInfoPolicy::Preserve,
                FrameComputationMode::Preserve,
                None,
            )?),
        })
    }

    #[pyo3(signature = (frame_mode = None, resolver = None, debug_info = "preserve"))]
    fn to_bytes_with_options<'py>(
        &self,
        py: Python<'py>,
        frame_mode: Option<&Bound<'py, PyAny>>,
        resolver: Option<&PyMappingClassResolver>,
        debug_info: &str,
    ) -> PyResult<Py<PyBytes>> {
        let policy = parse_debug_info_policy(debug_info)?;
        let frame_mode = parse_frame_computation_mode(frame_mode)?;
        let bytes = lower_class_model_bytes(
            self,
            policy,
            frame_mode,
            resolver.map(|r| &r.inner as &dyn pytecode_engine::analysis::ClassResolver),
        )?;
        Ok(PyBytes::new(py, &bytes).unbind())
    }

    #[pyo3(signature = (frame_mode = None, resolver = None, debug_info = "preserve"))]
    fn to_classfile_with_options(
        &self,
        frame_mode: Option<&Bound<'_, PyAny>>,
        resolver: Option<&PyMappingClassResolver>,
        debug_info: &str,
    ) -> PyResult<PyClassFile> {
        let policy = parse_debug_info_policy(debug_info)?;
        let frame_mode = parse_frame_computation_mode(frame_mode)?;
        Ok(PyClassFile {
            inner: Arc::new(lower_class_model(
                self,
                policy,
                frame_mode,
                resolver.map(|r| &r.inner as &dyn pytecode_engine::analysis::ClassResolver),
            )?),
        })
    }

    // -- getters ------------------------------------------------------------

    #[getter]
    fn entry_name(&self) -> PyResult<String> {
        with_live_model(&self.state, |model_state| {
            Ok(model_state.inner.as_ref().unwrap().entry_name.clone())
        })
    }

    #[getter]
    fn original_byte_len(&self) -> PyResult<usize> {
        with_live_model(&self.state, |model_state| {
            Ok(model_state.inner.as_ref().unwrap().original_byte_len)
        })
    }

    #[getter]
    fn version(&self) -> PyResult<(u16, u16)> {
        with_live_model(&self.state, |model_state| {
            Ok(model_state.inner.as_ref().unwrap().version)
        })
    }

    #[getter]
    fn access_flags(&self) -> PyResult<u16> {
        with_live_model(&self.state, |model_state| {
            Ok(model_state.inner.as_ref().unwrap().access_flags.bits())
        })
    }

    #[getter]
    fn name(&self) -> PyResult<String> {
        with_live_model(&self.state, |model_state| {
            Ok(model_state.inner.as_ref().unwrap().name.clone())
        })
    }

    #[getter]
    fn super_name(&self) -> PyResult<Option<String>> {
        with_live_model(&self.state, |model_state| {
            Ok(model_state.inner.as_ref().unwrap().super_name.clone())
        })
    }

    #[getter]
    fn interfaces(&self) -> PyResult<PyStringListView> {
        with_live_model(&self.state, |model_state| {
            Ok(PyStringListView {
                state: self.state.clone(),
                generation: model_state.interfaces_generation,
            })
        })
    }

    #[getter]
    fn fields(&self) -> PyResult<PyFieldListView> {
        with_live_model(&self.state, |model_state| {
            Ok(PyFieldListView {
                state: self.state.clone(),
                generation: model_state.fields_generation,
            })
        })
    }

    #[getter]
    fn methods(&self) -> PyResult<PyMethodListView> {
        with_live_model(&self.state, |model_state| {
            Ok(PyMethodListView {
                state: self.state.clone(),
                generation: model_state.methods_generation,
            })
        })
    }

    #[getter]
    fn attributes(&self) -> PyAttributeListView {
        PyAttributeListView {
            owner: AttributeOwner::Class(self.state.clone()),
        }
    }

    #[getter]
    fn constant_pool(&self) -> PyConstantPoolBuilder {
        PyConstantPoolBuilder::from_shared(self.state.clone())
    }

    #[getter]
    fn debug_info_state(&self) -> PyResult<&'static str> {
        with_live_model(&self.state, |model_state| {
            Ok(match model_state.inner.as_ref().unwrap().debug_info_state {
                DebugInfoState::Fresh => "fresh",
                DebugInfoState::Stale => "stale",
            })
        })
    }

    // -- setters ------------------------------------------------------------

    #[setter]
    fn set_name(&mut self, value: String) -> PyResult<()> {
        with_live_model_mut(&self.state, |model_state| {
            model_state.inner.as_mut().unwrap().name = value;
            Ok(())
        })
    }

    #[setter]
    fn set_super_name(&mut self, value: Option<String>) -> PyResult<()> {
        with_live_model_mut(&self.state, |model_state| {
            model_state.inner.as_mut().unwrap().super_name = value;
            Ok(())
        })
    }

    #[setter]
    fn set_interfaces(&mut self, value: Vec<String>) -> PyResult<()> {
        with_live_model_mut(&self.state, |model_state| {
            model_state.inner.as_mut().unwrap().interfaces = value;
            model_state.interfaces_generation += 1;
            Ok(())
        })
    }

    #[setter]
    fn set_access_flags(&mut self, value: u16) -> PyResult<()> {
        with_live_model_mut(&self.state, |model_state| {
            model_state.inner.as_mut().unwrap().access_flags =
                pytecode_engine::constants::ClassAccessFlags::from_bits_truncate(value);
            Ok(())
        })
    }

    #[setter]
    fn set_version(&mut self, value: (u16, u16)) -> PyResult<()> {
        with_live_model_mut(&self.state, |model_state| {
            model_state.inner.as_mut().unwrap().version = value;
            Ok(())
        })
    }

    #[setter]
    fn set_fields(&mut self, value: Vec<PyFieldModel>) -> PyResult<()> {
        let fields = value
            .into_iter()
            .map(|field| field.clone_inner())
            .collect::<PyResult<Vec<_>>>()?;
        with_live_model_mut(&self.state, |model_state| {
            model_state.inner.as_mut().unwrap().fields = fields;
            model_state.fields_generation += 1;
            Ok(())
        })
    }

    #[setter]
    fn set_methods(&mut self, value: Vec<PyMethodModel>) -> PyResult<()> {
        let methods = value
            .into_iter()
            .map(|method| method.clone_inner())
            .collect::<PyResult<Vec<_>>>()?;
        with_live_model_mut(&self.state, |model_state| {
            model_state.inner.as_mut().unwrap().methods = methods;
            model_state.methods_generation += 1;
            Ok(())
        })
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
    module.add_class::<PyStringListView>()?;
    module.add_class::<PyFieldListView>()?;
    module.add_class::<PyMethodListView>()?;
    module.add_class::<PyAttributeListView>()?;
    module.add_class::<PyCodeListView>()?;
    module.add_class::<PyLabel>()?;
    module.add_class::<PyLineNumberEntry>()?;
    module.add_class::<PyLocalVariableEntry>()?;
    module.add_class::<PyLocalVariableTypeEntry>()?;
    module.add_class::<PyMethodHandleValue>()?;
    module.add_class::<PyDynamicValue>()?;
    module.add_class::<PyRawInsn>()?;
    module.add_class::<PyByteInsn>()?;
    module.add_class::<PyShortInsn>()?;
    module.add_class::<PyNewArrayInsn>()?;
    module.add_class::<PyFieldInsn>()?;
    module.add_class::<PyMethodInsn>()?;
    module.add_class::<PyInterfaceMethodInsn>()?;
    module.add_class::<PyTypeInsn>()?;
    module.add_class::<PyVarInsn>()?;
    module.add_class::<PyIIncInsn>()?;
    module.add_class::<PyLdcInsn>()?;
    module.add_class::<PyInvokeDynamicInsn>()?;
    module.add_class::<PyMultiANewArrayInsn>()?;
    module.add_class::<PyBranchInsn>()?;
    module.add_class::<PyLookupSwitchInsn>()?;
    module.add_class::<PyTableSwitchInsn>()?;
    module.add_class::<PyConstantPoolBuilder>()?;
    module.add_class::<PyMappingClassResolver>()?;
    module.add_class::<PyModelExceptionHandler>()?;
    module.add_function(wrap_pyfunction!(rust_lower_classmodels, module)?)?;
    module.add_function(wrap_pyfunction!(rust_lower_classmodels_to_bytes, module)?)?;
    Ok(())
}
