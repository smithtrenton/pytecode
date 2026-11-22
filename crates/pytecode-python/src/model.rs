mod constant_pool;
pub(crate) use constant_pool::PyConstantPoolBuilder;

use pyo3::exceptions::{PyIndexError, PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyAny, PyBytes, PyDict, PyList, PySlice, PyString};
use pyo3::wrap_pyfunction;
use pytecode_engine::analysis::{MappingClassResolver, ResolvedClass};
use pytecode_engine::constants::ClassAccessFlags;
use pytecode_engine::indexes::BootstrapMethodIndex;
use pytecode_engine::model::{
    BranchInsn, ClassModel, CodeItem, CodeModel, ConstantPoolBuilder, DebugInfoPolicy,
    DebugInfoState, DynamicValue, ExceptionHandler, FieldInsn, FieldModel, FrameComputationMode,
    IIncInsn, InterfaceMethodInsn, InvokeDynamicInsn, Label, LdcInsn, LdcValue, LineNumberEntry,
    LocalVariableEntry, LocalVariableTypeEntry, LookupSwitchInsn, MethodHandleValue, MethodInsn,
    MethodModel, MultiANewArrayInsn, TableSwitchInsn, TypeInsn, VarInsn,
};
use pytecode_engine::parse_class;
use pytecode_engine::raw::{ArrayType, ClassFile, Instruction, NewArrayInsn};
use pytecode_engine::write_class;
use std::cell::RefCell;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::Arc;

use crate::PyClassFile;
use crate::analysis::{analysis_error_to_py, resolved_class_to_py};
use crate::transforms::PyInsnMatcher;

type PyObject = Py<PyAny>;

type SharedClassState = Rc<RefCell<ClassModelState>>;
type OwnedCodeState = Rc<RefCell<Option<CodeModel>>>;

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

struct ClassModelRestoreGuard {
    state: SharedClassState,
    inner: Option<ClassModel>,
}

impl Drop for ClassModelRestoreGuard {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.take()
            && let Ok(mut borrowed) = self.state.try_borrow_mut()
        {
            borrowed.inner = Some(inner);
            borrowed.interfaces_generation += 1;
            borrowed.fields_generation += 1;
            borrowed.methods_generation += 1;
        }
    }
}

fn dead_model_err() -> PyErr {
    PyRuntimeError::new_err("ClassModel is no longer live")
}

fn dead_code_err() -> PyErr {
    PyRuntimeError::new_err("CodeModel is no longer live")
}

fn borrow_err(kind: &str) -> PyErr {
    PyRuntimeError::new_err(format!("{kind} is already mutably borrowed"))
}

fn stale_ref_err(kind: &str) -> PyErr {
    PyRuntimeError::new_err(format!("{kind} is stale after structural mutation"))
}

fn index_err(index: isize, len: usize) -> PyErr {
    PyIndexError::new_err(format!("index {index} out of range for length {len}"))
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

fn slice_indices(slice: &Bound<'_, PySlice>, len: usize) -> PyResult<Vec<usize>> {
    let info = slice.indices(len as isize)?;
    let mut out = Vec::with_capacity(info.slicelength);
    let mut current = info.start;
    for _ in 0..info.slicelength {
        out.push(current as usize);
        current += info.step;
    }
    Ok(out)
}

fn with_live_model<R>(
    state: &SharedClassState,
    f: impl FnOnce(&ClassModelState) -> PyResult<R>,
) -> PyResult<R> {
    let borrowed = state.try_borrow().map_err(|_| borrow_err("ClassModel"))?;
    if borrowed.inner.is_none() {
        return Err(dead_model_err());
    }
    f(&borrowed)
}

fn with_live_model_mut<R>(
    state: &SharedClassState,
    f: impl FnOnce(&mut ClassModelState) -> PyResult<R>,
) -> PyResult<R> {
    let mut borrowed = state
        .try_borrow_mut()
        .map_err(|_| borrow_err("ClassModel"))?;
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

/// Symbolic branch and debug-range anchor inside a ``CodeModel``.
#[pyclass(from_py_object, name = "Label", module = "pytecode._rust")]
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

    /// Construct a symbolic label carrying an optional diagnostic name.
    #[staticmethod]
    fn named(name: &str) -> Self {
        Self {
            inner: Label::named(name),
        }
    }

    /// Optional symbolic name assigned to this label.
    #[getter]
    fn name(&self) -> Option<String> {
        self.inner.name.clone()
    }

    /// Item discriminator tag (``"label"``).
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
// PyModelExceptionHandler & debug entries
// ---------------------------------------------------------------------------

/// Symbolic exception handler range in a ``CodeModel``.
#[pyclass(from_py_object, name = "ExceptionHandler", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyModelExceptionHandler {
    inner: ExceptionHandler,
}

#[pymethods]
impl PyModelExceptionHandler {
    #[new]
    #[pyo3(signature = (start, end, handler, catch_type=None))]
    fn new(start: PyLabel, end: PyLabel, handler: PyLabel, catch_type: Option<String>) -> Self {
        Self {
            inner: ExceptionHandler {
                start: start.inner,
                end: end.inner,
                handler: handler.inner,
                catch_type,
            },
        }
    }

    /// Inclusive start label of the ``try`` block.
    #[getter]
    fn start(&self) -> PyLabel {
        PyLabel::from(self.inner.start.clone())
    }

    /// Exclusive end label of the ``try`` block.
    #[getter]
    fn end(&self) -> PyLabel {
        PyLabel::from(self.inner.end.clone())
    }

    /// Target label where control transfers on exception dispatch.
    #[getter]
    fn handler(&self) -> PyLabel {
        PyLabel::from(self.inner.handler.clone())
    }

    /// Internal class name of the caught exception, or ``None`` for catch-all.
    #[getter]
    fn catch_type(&self) -> Option<String> {
        self.inner.catch_type.clone()
    }

    fn __repr__(&self) -> String {
        format!("ExceptionHandler(catch_type={:?})", self.inner.catch_type)
    }
}

/// Symbolic line-number entry anchored to a ``Label`` in a ``CodeModel``.
#[pyclass(from_py_object, name = "LineNumberEntry", module = "pytecode._rust")]
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

    /// Label marking the start of this source line.
    #[getter]
    fn label(&self) -> PyLabel {
        PyLabel::from(self.label.clone())
    }

    /// 1-based source line number.
    #[getter]
    fn line_number(&self) -> u16 {
        self.line_number
    }

    fn __repr__(&self) -> String {
        format!("LineNumberEntry(line_number={})", self.line_number)
    }
}

/// Symbolic local-variable debug entry anchored to start and end ``Label`` instances.
#[pyclass(from_py_object, name = "LocalVariableEntry", module = "pytecode._rust")]
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

    /// Label marking the start of the variable's live range.
    #[getter]
    fn start(&self) -> PyLabel {
        PyLabel::from(self.start.clone())
    }

    /// Label marking the end of the variable's live range.
    #[getter]
    fn end(&self) -> PyLabel {
        PyLabel::from(self.end.clone())
    }

    /// Source name of the local variable.
    #[getter]
    fn name(&self) -> String {
        self.name.clone()
    }

    /// JVM field descriptor of the local variable.
    #[getter]
    fn descriptor(&self) -> String {
        self.descriptor.clone()
    }

    /// Local-variable slot index.
    #[getter]
    fn index(&self) -> u16 {
        self.index
    }

    fn __repr__(&self) -> String {
        format!(
            "LocalVariableEntry(name={:?}, descriptor={:?}, index={})",
            self.name, self.descriptor, self.index
        )
    }
}

/// Symbolic generic-signature debug entry for a local variable.
#[pyclass(
    from_py_object,
    name = "LocalVariableTypeEntry",
    module = "pytecode._rust"
)]
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

    /// Label marking the start of the variable's live range.
    #[getter]
    fn start(&self) -> PyLabel {
        PyLabel::from(self.start.clone())
    }

    /// Label marking the end of the variable's live range.
    #[getter]
    fn end(&self) -> PyLabel {
        PyLabel::from(self.end.clone())
    }

    /// Source name of the local variable.
    #[getter]
    fn name(&self) -> String {
        self.name.clone()
    }

    /// JVM generic type signature of the local variable.
    #[getter]
    fn signature(&self) -> String {
        self.signature.clone()
    }

    /// Local-variable slot index.
    #[getter]
    fn index(&self) -> u16 {
        self.index
    }

    fn __repr__(&self) -> String {
        format!(
            "LocalVariableTypeEntry(name={:?}, signature={:?}, index={})",
            self.name, self.signature, self.index
        )
    }
}

/// Symbolic representation of a ``CONSTANT_MethodHandle_info`` constant payload.
#[pyclass(from_py_object, name = "MethodHandleValue", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyMethodHandleValue {
    inner: MethodHandleValue,
}

fn validate_reference_kind(reference_kind: u8) -> PyResult<()> {
    if !(1..=9).contains(&reference_kind) {
        return Err(code_item_value_error(format!(
            "method handle reference_kind must be in 1..=9, got {reference_kind}"
        )));
    }
    Ok(())
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
    ) -> PyResult<Self> {
        validate_reference_kind(reference_kind)?;
        Ok(Self {
            inner: MethodHandleValue {
                reference_kind,
                owner,
                name,
                descriptor,
                is_interface,
            },
        })
    }

    /// JVMS reference kind tag in ``1..=9``.
    #[getter]
    fn reference_kind(&self) -> u8 {
        self.inner.reference_kind
    }

    /// Internal JVM name of the target member's owner class.
    #[getter]
    fn owner(&self) -> String {
        self.inner.owner.clone()
    }

    /// Target field or method name.
    #[getter]
    fn name(&self) -> String {
        self.inner.name.clone()
    }

    /// Target field or method descriptor.
    #[getter]
    fn descriptor(&self) -> String {
        self.inner.descriptor.clone()
    }

    /// Whether the target reference uses ``InterfaceMethodref``.
    #[getter]
    fn is_interface(&self) -> bool {
        self.inner.is_interface
    }

    fn __repr__(&self) -> String {
        format!(
            "MethodHandleValue(reference_kind={}, owner={:?}, name={:?})",
            self.inner.reference_kind, self.inner.owner, self.inner.name
        )
    }
}

/// Symbolic representation of a ``CONSTANT_Dynamic_info`` constant payload.
#[pyclass(from_py_object, name = "DynamicValue", module = "pytecode._rust")]
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

    /// Index into the class's ``BootstrapMethods`` attribute table.
    #[getter]
    fn bootstrap_method_attr_index(&self) -> u16 {
        self.inner.bootstrap_method_attr_index.value()
    }

    /// Constant name passed to the bootstrap method.
    #[getter]
    fn name(&self) -> String {
        self.inner.name.clone()
    }

    /// Field descriptor of the resolved dynamic constant.
    #[getter]
    fn descriptor(&self) -> String {
        self.inner.descriptor.clone()
    }

    fn __repr__(&self) -> String {
        format!(
            "DynamicValue(bootstrap_method_attr_index={}, name={:?}, descriptor={:?})",
            self.inner.bootstrap_method_attr_index.value(),
            self.inner.name,
            self.inner.descriptor
        )
    }
}

// ---------------------------------------------------------------------------
// Symbolic instructions
// ---------------------------------------------------------------------------

fn validate_raw_opcode(opcode: u8) -> PyResult<()> {
    if !matches!(
        opcode,
        0x00..=0x0F | 0x1A..=0x35 | 0x3B..=0x83 | 0x85..=0x98 | 0xAC..=0xB1 | 0xBE | 0xBF | 0xC2 | 0xC3
    ) {
        return Err(code_item_value_error(format!(
            "invalid simple raw instruction opcode 0x{opcode:02x}"
        )));
    }
    Ok(())
}

fn validate_byte_opcode(opcode: u8) -> PyResult<()> {
    if opcode != 0x10 {
        return Err(code_item_value_error(format!(
            "ByteInsn requires opcode 0x10 (bipush), got 0x{opcode:02x}"
        )));
    }
    Ok(())
}

fn validate_short_opcode(opcode: u8) -> PyResult<()> {
    if opcode != 0x11 {
        return Err(code_item_value_error(format!(
            "ShortInsn requires opcode 0x11 (sipush), got 0x{opcode:02x}"
        )));
    }
    Ok(())
}

fn validate_field_opcode(opcode: u8) -> PyResult<()> {
    if !matches!(opcode, 0xB2..=0xB5) {
        return Err(code_item_value_error(format!(
            "FieldInsn requires opcode in 0xB2..=0xB5, got 0x{opcode:02x}"
        )));
    }
    Ok(())
}

fn validate_method_opcode(opcode: u8) -> PyResult<()> {
    if !matches!(opcode, 0xB6..=0xB8) {
        return Err(code_item_value_error(format!(
            "MethodInsn requires opcode in 0xB6..=0xB8, got 0x{opcode:02x}"
        )));
    }
    Ok(())
}

fn validate_type_opcode(opcode: u8) -> PyResult<()> {
    if !matches!(opcode, 0xBB | 0xBD | 0xC0 | 0xC1) {
        return Err(code_item_value_error(format!(
            "TypeInsn requires opcode in {{0xBB, 0xBD, 0xC0, 0xC1}}, got 0x{opcode:02x}"
        )));
    }
    Ok(())
}

fn validate_var_opcode(opcode: u8) -> PyResult<()> {
    if !matches!(opcode, 0x15..=0x19 | 0x36..=0x3A | 0xA9) {
        return Err(code_item_value_error(format!(
            "VarInsn requires var/ret opcode in 0x15..=0x19, 0x36..=0x3A, or 0xA9, got 0x{opcode:02x}"
        )));
    }
    Ok(())
}

fn validate_branch_opcode(opcode: u8) -> PyResult<()> {
    if !matches!(opcode, 0x99..=0xA8 | 0xC6..=0xC9) {
        return Err(code_item_value_error(format!(
            "BranchInsn requires branch opcode in 0x99..=0xA8 or 0xC6..=0xC9, got 0x{opcode:02x}"
        )));
    }
    Ok(())
}

fn validate_multianewarray_dimensions(dimensions: u8) -> PyResult<()> {
    if dimensions < 1 {
        return Err(code_item_value_error(
            "MultiANewArrayInsn dimensions must be at least 1",
        ));
    }
    Ok(())
}

fn validate_tableswitch_bounds(low: i32, high: i32, targets_len: usize) -> PyResult<()> {
    if low > high {
        return Err(code_item_value_error(format!(
            "TableSwitchInsn requires low <= high, got low={low}, high={high}"
        )));
    }
    let expected_len = i64::from(high) - i64::from(low) + 1;
    if i64::try_from(targets_len) != Ok(expected_len) {
        return Err(code_item_value_error(format!(
            "TableSwitchInsn expected {expected_len} target(s) for range {low}..={high}, got {targets_len}"
        )));
    }
    Ok(())
}

/// Symbolic zero-operand JVM instruction.
#[pyclass(from_py_object, name = "RawInsn", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyRawInsn {
    opcode: u8,
}

#[pymethods]
impl PyRawInsn {
    #[new]
    fn new(opcode: u8) -> PyResult<Self> {
        validate_raw_opcode(opcode)?;
        Ok(Self { opcode })
    }

    /// Item discriminator tag (``"raw"``).
    #[getter]
    fn kind(&self) -> &'static str {
        "raw"
    }

    /// Raw JVM opcode byte.
    #[getter]
    fn opcode(&self) -> u8 {
        self.opcode
    }

    fn __repr__(&self) -> String {
        format!("RawInsn(opcode=0x{:02x})", self.opcode)
    }
}

/// Symbolic signed-byte-immediate instruction (``bipush``).
#[pyclass(from_py_object, name = "ByteInsn", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyByteInsn {
    opcode: u8,
    value: i8,
}

#[pymethods]
impl PyByteInsn {
    #[new]
    fn new(opcode: u8, value: i8) -> PyResult<Self> {
        validate_byte_opcode(opcode)?;
        Ok(Self { opcode, value })
    }

    /// Item discriminator tag (``"byte"``).
    #[getter]
    fn kind(&self) -> &'static str {
        "byte"
    }

    /// Raw JVM opcode byte.
    #[getter]
    fn opcode(&self) -> u8 {
        self.opcode
    }

    /// Signed 8-bit immediate operand.
    #[getter]
    fn value(&self) -> i8 {
        self.value
    }

    fn __repr__(&self) -> String {
        format!(
            "ByteInsn(opcode=0x{:02x}, value={})",
            self.opcode, self.value
        )
    }
}

/// Symbolic signed-short-immediate instruction (``sipush``).
#[pyclass(from_py_object, name = "ShortInsn", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyShortInsn {
    opcode: u8,
    value: i16,
}

#[pymethods]
impl PyShortInsn {
    #[new]
    fn new(opcode: u8, value: i16) -> PyResult<Self> {
        validate_short_opcode(opcode)?;
        Ok(Self { opcode, value })
    }

    /// Item discriminator tag (``"short"``).
    #[getter]
    fn kind(&self) -> &'static str {
        "short"
    }

    /// Raw JVM opcode byte.
    #[getter]
    fn opcode(&self) -> u8 {
        self.opcode
    }

    /// Signed 16-bit immediate operand.
    #[getter]
    fn value(&self) -> i16 {
        self.value
    }

    fn __repr__(&self) -> String {
        format!(
            "ShortInsn(opcode=0x{:02x}, value={})",
            self.opcode, self.value
        )
    }
}

/// Symbolic ``newarray`` instruction allocating a primitive array.
#[pyclass(from_py_object, name = "NewArrayInsn", module = "pytecode._rust")]
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

    /// Item discriminator tag (``"newarray"``).
    #[getter]
    fn kind(&self) -> &'static str {
        "newarray"
    }

    /// Primitive array element type code (``4..=11``).
    #[getter]
    fn atype(&self) -> u8 {
        self.atype
    }

    fn __repr__(&self) -> String {
        format!("NewArrayInsn(atype={})", self.atype)
    }
}

/// Symbolic field-access instruction (``getstatic``, ``putstatic``, ``getfield``, ``putfield``).
#[pyclass(from_py_object, name = "FieldInsn", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyFieldInsn {
    inner: FieldInsn,
}

#[pymethods]
impl PyFieldInsn {
    #[new]
    fn new(opcode: u8, owner: String, name: String, descriptor: String) -> PyResult<Self> {
        validate_field_opcode(opcode)?;
        Ok(Self {
            inner: FieldInsn {
                opcode,
                owner,
                name,
                descriptor,
            },
        })
    }

    /// Item discriminator tag (``"field"``).
    #[getter]
    fn kind(&self) -> &'static str {
        "field"
    }

    /// Field-access opcode (``0xB2..=0xB5``).
    #[getter]
    fn opcode(&self) -> u8 {
        self.inner.opcode
    }

    /// Internal JVM name of the declaring class.
    #[getter]
    fn owner(&self) -> String {
        self.inner.owner.clone()
    }

    /// Field name.
    #[getter]
    fn name(&self) -> String {
        self.inner.name.clone()
    }

    /// JVM field descriptor.
    #[getter]
    fn descriptor(&self) -> String {
        self.inner.descriptor.clone()
    }

    fn __repr__(&self) -> String {
        format!(
            "FieldInsn(opcode=0x{:02x}, owner={:?}, name={:?}, descriptor={:?})",
            self.inner.opcode, self.inner.owner, self.inner.name, self.inner.descriptor
        )
    }
}

/// Symbolic method invocation instruction (``invokevirtual``, ``invokespecial``, ``invokestatic``).
#[pyclass(from_py_object, name = "MethodInsn", module = "pytecode._rust")]
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
    ) -> PyResult<Self> {
        validate_method_opcode(opcode)?;
        Ok(Self {
            inner: MethodInsn {
                opcode,
                owner,
                name,
                descriptor,
                is_interface,
            },
        })
    }

    /// Item discriminator tag (``"method"``).
    #[getter]
    fn kind(&self) -> &'static str {
        "method"
    }

    /// Method invocation opcode (``0xB6..=0xB8``).
    #[getter]
    fn opcode(&self) -> u8 {
        self.inner.opcode
    }

    /// Internal JVM name of the declaring class or interface.
    #[getter]
    fn owner(&self) -> String {
        self.inner.owner.clone()
    }

    /// Method name.
    #[getter]
    fn name(&self) -> String {
        self.inner.name.clone()
    }

    /// JVM method descriptor.
    #[getter]
    fn descriptor(&self) -> String {
        self.inner.descriptor.clone()
    }

    /// Whether the constant-pool target uses ``InterfaceMethodref``.
    #[getter]
    fn is_interface(&self) -> bool {
        self.inner.is_interface
    }

    fn __repr__(&self) -> String {
        format!(
            "MethodInsn(opcode=0x{:02x}, owner={:?}, name={:?}, descriptor={:?}, is_interface={})",
            self.inner.opcode,
            self.inner.owner,
            self.inner.name,
            self.inner.descriptor,
            self.inner.is_interface
        )
    }
}

/// Symbolic ``invokeinterface`` instruction.
#[pyclass(
    from_py_object,
    name = "InterfaceMethodInsn",
    module = "pytecode._rust"
)]
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

    /// Item discriminator tag (``"interface_method"``).
    #[getter]
    fn kind(&self) -> &'static str {
        "interface_method"
    }

    /// Internal JVM name of the declaring interface.
    #[getter]
    fn owner(&self) -> String {
        self.inner.owner.clone()
    }

    /// Interface method name.
    #[getter]
    fn name(&self) -> String {
        self.inner.name.clone()
    }

    /// JVM method descriptor.
    #[getter]
    fn descriptor(&self) -> String {
        self.inner.descriptor.clone()
    }

    fn __repr__(&self) -> String {
        format!(
            "InterfaceMethodInsn(owner={:?}, name={:?}, descriptor={:?})",
            self.inner.owner, self.inner.name, self.inner.descriptor
        )
    }
}

/// Symbolic type-operand instruction (``new``, ``anewarray``, ``checkcast``, ``instanceof``).
#[pyclass(from_py_object, name = "TypeInsn", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyTypeInsn {
    inner: TypeInsn,
}

#[pymethods]
impl PyTypeInsn {
    #[new]
    fn new(opcode: u8, descriptor: String) -> PyResult<Self> {
        validate_type_opcode(opcode)?;
        Ok(Self {
            inner: TypeInsn { opcode, descriptor },
        })
    }

    /// Item discriminator tag (``"type"``).
    #[getter]
    fn kind(&self) -> &'static str {
        "type"
    }

    /// Type instruction opcode (``0xBB``, ``0xBD``, ``0xC0``, or ``0xC1``).
    #[getter]
    fn opcode(&self) -> u8 {
        self.inner.opcode
    }

    /// Internal class name or array descriptor.
    #[getter]
    fn descriptor(&self) -> String {
        self.inner.descriptor.clone()
    }

    fn __repr__(&self) -> String {
        format!(
            "TypeInsn(opcode=0x{:02x}, descriptor={:?})",
            self.inner.opcode, self.inner.descriptor
        )
    }
}

/// Symbolic local-variable instruction (``iload``, ``istore``, ..., ``ret``).
#[pyclass(from_py_object, name = "VarInsn", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyVarInsn {
    inner: VarInsn,
}

#[pymethods]
impl PyVarInsn {
    #[new]
    fn new(opcode: u8, slot: u16) -> PyResult<Self> {
        validate_var_opcode(opcode)?;
        Ok(Self {
            inner: VarInsn { opcode, slot },
        })
    }

    /// Item discriminator tag (``"var"``).
    #[getter]
    fn kind(&self) -> &'static str {
        "var"
    }

    /// Canonical local-variable opcode (automatically widened if ``slot > 255``).
    #[getter]
    fn opcode(&self) -> u8 {
        self.inner.opcode
    }

    /// Local-variable slot index.
    #[getter]
    fn slot(&self) -> u16 {
        self.inner.slot
    }

    fn __repr__(&self) -> String {
        format!(
            "VarInsn(opcode=0x{:02x}, slot={})",
            self.inner.opcode, self.inner.slot
        )
    }
}

/// Symbolic ``iinc`` instruction incrementing a local integer variable.
#[pyclass(from_py_object, name = "IIncInsn", module = "pytecode._rust")]
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

    /// Item discriminator tag (``"iinc"``).
    #[getter]
    fn kind(&self) -> &'static str {
        "iinc"
    }

    /// Local-variable slot index.
    #[getter]
    fn slot(&self) -> u16 {
        self.inner.slot
    }

    /// Signed increment delta (automatically widened when outside ``i8`` or ``slot > 255``).
    #[getter]
    fn value(&self) -> i16 {
        self.inner.value
    }

    fn __repr__(&self) -> String {
        format!(
            "IIncInsn(slot={}, value={})",
            self.inner.slot, self.inner.value
        )
    }
}

/// Symbolic constant-loading instruction (``ldc``, ``ldc_w``, or ``ldc2_w``).
#[pyclass(from_py_object, name = "LdcInsn", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyLdcInsn {
    inner: LdcInsn,
}

#[pymethods]
impl PyLdcInsn {
    /// Construct an ``ldc`` holding a signed 32-bit integer constant.
    #[staticmethod]
    fn int(value: i32) -> Self {
        Self {
            inner: LdcInsn {
                value: LdcValue::Int(value),
            },
        }
    }

    /// Construct an ``ldc`` holding a float constant from raw IEEE 754 bits.
    #[staticmethod]
    fn float_bits(value: u32) -> Self {
        Self {
            inner: LdcInsn {
                value: LdcValue::FloatBits(value),
            },
        }
    }

    /// Construct an ``ldc2_w`` holding a signed 64-bit integer constant.
    #[staticmethod]
    fn long(value: i64) -> Self {
        Self {
            inner: LdcInsn {
                value: LdcValue::Long(value),
            },
        }
    }

    /// Construct an ``ldc2_w`` holding a double constant from raw 64-bit IEEE 754 bits.
    #[staticmethod]
    fn double_bits(value: u64) -> Self {
        Self {
            inner: LdcInsn {
                value: LdcValue::DoubleBits(value),
            },
        }
    }

    /// Construct an ``ldc`` holding a string literal (preserving lone surrogates).
    #[staticmethod]
    fn string(value: &Bound<'_, PyString>) -> PyResult<Self> {
        Ok(Self {
            inner: LdcInsn {
                value: LdcValue::String(crate::java_string::from_python(value)?),
            },
        })
    }

    /// Construct an ``ldc`` holding a class literal in internal JVM form.
    #[staticmethod]
    fn class_value(value: String) -> Self {
        Self {
            inner: LdcInsn {
                value: LdcValue::Class(value),
            },
        }
    }

    /// Construct an ``ldc`` holding a ``MethodType`` descriptor constant.
    #[staticmethod]
    fn method_type(value: String) -> Self {
        Self {
            inner: LdcInsn {
                value: LdcValue::MethodType(value),
            },
        }
    }

    /// Construct an ``ldc`` holding a ``MethodHandle`` constant.
    #[staticmethod]
    fn method_handle(
        reference_kind: u8,
        owner: String,
        name: String,
        descriptor: String,
        is_interface: bool,
    ) -> PyResult<Self> {
        validate_reference_kind(reference_kind)?;
        Ok(Self {
            inner: LdcInsn {
                value: LdcValue::MethodHandle(MethodHandleValue {
                    reference_kind,
                    owner,
                    name,
                    descriptor,
                    is_interface,
                }),
            },
        })
    }

    /// Construct an ``ldc`` / ``ldc2_w`` holding a dynamic constant.
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

    /// Item discriminator tag (``"ldc"``).
    #[getter]
    fn kind(&self) -> &'static str {
        "ldc"
    }

    /// Constant variant name (``"int"``, ``"float"``, ``"string"``, etc.).
    #[getter]
    fn value_type(&self) -> &'static str {
        ldc_value_type(&self.inner.value)
    }

    /// Payload decoded according to ``value_type``.
    #[getter]
    fn value(&self, py: Python<'_>) -> PyResult<PyObject> {
        ldc_value_to_py(py, &self.inner.value)
    }

    fn __repr__(&self) -> String {
        format!("LdcInsn(value_type={:?})", self.value_type())
    }
}

fn ldc_value_type(value: &LdcValue) -> &'static str {
    match value {
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

fn ldc_value_to_py(py: Python<'_>, value: &LdcValue) -> PyResult<PyObject> {
    match value {
        LdcValue::Int(value) => Ok(value.into_pyobject(py)?.unbind().into()),
        LdcValue::FloatBits(value) => Ok(value.into_pyobject(py)?.unbind().into()),
        LdcValue::Long(value) => Ok(value.into_pyobject(py)?.unbind().into()),
        LdcValue::DoubleBits(value) => Ok(value.into_pyobject(py)?.unbind().into()),
        LdcValue::String(value) => crate::java_string::to_python(py, value),
        LdcValue::Class(value) => Ok(value.into_pyobject(py)?.unbind().into()),
        LdcValue::MethodType(value) => Ok(value.into_pyobject(py)?.unbind().into()),
        LdcValue::MethodHandle(value) => wrap_method_handle_value(py, value),
        LdcValue::Dynamic(value) => wrap_dynamic_value(py, value),
    }
}

/// One static argument of a bootstrap method: a loadable constant.
///
/// Construct with the same factories as ``LdcInsn``; ``value_type`` and
/// ``value`` decode it the same way.
#[pyclass(
    from_py_object,
    name = "BootstrapArgument",
    module = "pytecode._rust",
    eq
)]
#[derive(Clone, PartialEq)]
pub struct PyBootstrapArgument {
    inner: LdcValue,
}

#[pymethods]
impl PyBootstrapArgument {
    /// A signed 32-bit integer constant.
    #[staticmethod]
    fn int(value: i32) -> Self {
        Self {
            inner: LdcValue::Int(value),
        }
    }

    /// A float constant from raw IEEE 754 bits.
    #[staticmethod]
    fn float_bits(value: u32) -> Self {
        Self {
            inner: LdcValue::FloatBits(value),
        }
    }

    /// A signed 64-bit integer constant.
    #[staticmethod]
    fn long(value: i64) -> Self {
        Self {
            inner: LdcValue::Long(value),
        }
    }

    /// A double constant from raw 64-bit IEEE 754 bits.
    #[staticmethod]
    fn double_bits(value: u64) -> Self {
        Self {
            inner: LdcValue::DoubleBits(value),
        }
    }

    /// A string constant (preserving lone surrogates).
    #[staticmethod]
    fn string(value: &Bound<'_, PyString>) -> PyResult<Self> {
        Ok(Self {
            inner: LdcValue::String(crate::java_string::from_python(value)?),
        })
    }

    /// A class literal in internal JVM form.
    #[staticmethod]
    fn class_value(value: String) -> Self {
        Self {
            inner: LdcValue::Class(value),
        }
    }

    /// A ``MethodType`` descriptor constant.
    #[staticmethod]
    fn method_type(value: String) -> Self {
        Self {
            inner: LdcValue::MethodType(value),
        }
    }

    /// A ``MethodHandle`` constant.
    #[staticmethod]
    fn method_handle(
        reference_kind: u8,
        owner: String,
        name: String,
        descriptor: String,
        is_interface: bool,
    ) -> PyResult<Self> {
        validate_reference_kind(reference_kind)?;
        Ok(Self {
            inner: LdcValue::MethodHandle(MethodHandleValue {
                reference_kind,
                owner,
                name,
                descriptor,
                is_interface,
            }),
        })
    }

    /// A dynamic constant (``CONSTANT_Dynamic``).
    #[staticmethod]
    fn dynamic(bootstrap_method_attr_index: u16, name: String, descriptor: String) -> Self {
        Self {
            inner: LdcValue::Dynamic(DynamicValue {
                bootstrap_method_attr_index: BootstrapMethodIndex::new(bootstrap_method_attr_index),
                name,
                descriptor,
            }),
        }
    }

    /// Wrap an existing ``MethodHandleValue``.
    #[staticmethod]
    fn from_method_handle(value: &PyMethodHandleValue) -> Self {
        Self {
            inner: LdcValue::MethodHandle(value.inner.clone()),
        }
    }

    /// Constant variant name (``"int"``, ``"float"``, ``"string"``, etc.).
    #[getter]
    fn value_type(&self) -> &'static str {
        ldc_value_type(&self.inner)
    }

    /// Payload decoded according to ``value_type``.
    #[getter]
    fn value(&self, py: Python<'_>) -> PyResult<PyObject> {
        ldc_value_to_py(py, &self.inner)
    }

    fn __repr__(&self) -> String {
        match &self.inner {
            LdcValue::Class(value) | LdcValue::MethodType(value) => format!(
                "BootstrapArgument(value_type={:?}, value={value:?})",
                self.value_type()
            ),
            LdcValue::MethodHandle(handle) => format!(
                "BootstrapArgument(value_type=\"method_handle\", owner={:?}, name={:?}, descriptor={:?})",
                handle.owner, handle.name, handle.descriptor
            ),
            _ => format!("BootstrapArgument(value_type={:?})", self.value_type()),
        }
    }
}

/// One entry of a class's ``BootstrapMethods`` attribute.
///
/// ``invokedynamic`` instructions and dynamic constants refer to entries by
/// position, so edit entries in place rather than reordering them.
#[pyclass(
    from_py_object,
    name = "BootstrapMethod",
    module = "pytecode._rust",
    eq
)]
#[derive(Clone, PartialEq)]
pub struct PyBootstrapMethod {
    method: MethodHandleValue,
    arguments: Vec<LdcValue>,
}

#[pymethods]
impl PyBootstrapMethod {
    #[new]
    #[pyo3(signature = (method, arguments = None))]
    fn new(method: &PyMethodHandleValue, arguments: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let mut values = Vec::new();
        if let Some(arguments) = arguments {
            for argument in arguments.try_iter()? {
                let argument: PyRef<'_, PyBootstrapArgument> = argument?.extract()?;
                values.push(argument.inner.clone());
            }
        }
        Ok(Self {
            method: method.inner.clone(),
            arguments: values,
        })
    }

    /// The bootstrap method handle (normally ``REF_invokeStatic``).
    #[getter]
    fn method(&self, py: Python<'_>) -> PyResult<PyObject> {
        wrap_method_handle_value(py, &self.method)
    }

    /// The static arguments passed to the bootstrap method.
    #[getter]
    fn arguments(&self) -> Vec<PyBootstrapArgument> {
        self.arguments
            .iter()
            .map(|value| PyBootstrapArgument {
                inner: value.clone(),
            })
            .collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "BootstrapMethod(method={}.{}{}, arguments={})",
            self.method.owner,
            self.method.name,
            self.method.descriptor,
            self.arguments.len()
        )
    }
}

impl PyBootstrapMethod {
    fn to_model(&self) -> pytecode_engine::model::BootstrapMethodModel {
        pytecode_engine::model::BootstrapMethodModel {
            method: self.method.clone(),
            arguments: self.arguments.clone(),
        }
    }

    fn from_model(model: pytecode_engine::model::BootstrapMethodModel) -> Self {
        Self {
            method: model.method,
            arguments: model.arguments,
        }
    }
}

/// Symbolic ``invokedynamic`` instruction.
#[pyclass(from_py_object, name = "InvokeDynamicInsn", module = "pytecode._rust")]
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

    /// Item discriminator tag (``"invokedynamic"``).
    #[getter]
    fn kind(&self) -> &'static str {
        "invokedynamic"
    }

    /// Index into the class's ``BootstrapMethods`` attribute table.
    #[getter]
    fn bootstrap_method_attr_index(&self) -> u16 {
        self.inner.bootstrap_method_attr_index.value()
    }

    /// Call-site method name.
    #[getter]
    fn name(&self) -> String {
        self.inner.name.clone()
    }

    /// Call-site method descriptor.
    #[getter]
    fn descriptor(&self) -> String {
        self.inner.descriptor.clone()
    }

    fn __repr__(&self) -> String {
        format!(
            "InvokeDynamicInsn(bootstrap_method_attr_index={}, name={:?}, descriptor={:?})",
            self.inner.bootstrap_method_attr_index.value(),
            self.inner.name,
            self.inner.descriptor
        )
    }
}

/// Symbolic ``multianewarray`` instruction allocating a multidimensional array.
#[pyclass(from_py_object, name = "MultiANewArrayInsn", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyMultiANewArrayInsn {
    inner: MultiANewArrayInsn,
}

#[pymethods]
impl PyMultiANewArrayInsn {
    #[new]
    fn new(descriptor: String, dimensions: u8) -> PyResult<Self> {
        validate_multianewarray_dimensions(dimensions)?;
        Ok(Self {
            inner: MultiANewArrayInsn {
                descriptor,
                dimensions,
            },
        })
    }

    /// Item discriminator tag (``"multianewarray"``).
    #[getter]
    fn kind(&self) -> &'static str {
        "multianewarray"
    }

    /// Array type descriptor.
    #[getter]
    fn descriptor(&self) -> String {
        self.inner.descriptor.clone()
    }

    /// Number of array dimensions to allocate (>= 1).
    #[getter]
    fn dimensions(&self) -> u8 {
        self.inner.dimensions
    }

    fn __repr__(&self) -> String {
        format!(
            "MultiANewArrayInsn(descriptor={:?}, dimensions={})",
            self.inner.descriptor, self.inner.dimensions
        )
    }
}

/// Symbolic branch instruction targeting a ``Label``.
#[pyclass(from_py_object, name = "BranchInsn", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyBranchInsn {
    inner: BranchInsn,
}

#[pymethods]
impl PyBranchInsn {
    #[new]
    fn new(opcode: u8, target: PyLabel) -> PyResult<Self> {
        validate_branch_opcode(opcode)?;
        Ok(Self {
            inner: BranchInsn {
                opcode,
                target: target.inner,
            },
        })
    }

    /// Item discriminator tag (``"branch"``).
    #[getter]
    fn kind(&self) -> &'static str {
        "branch"
    }

    /// Branch opcode (automatically widened/inverted on lowering if needed).
    #[getter]
    fn opcode(&self) -> u8 {
        self.inner.opcode
    }

    /// Symbolic target ``Label``.
    #[getter]
    fn target(&self) -> PyLabel {
        PyLabel::from(self.inner.target.clone())
    }

    fn __repr__(&self) -> String {
        format!("BranchInsn(opcode=0x{:02x})", self.inner.opcode)
    }
}

/// Symbolic ``lookupswitch`` instruction with key-to-label targets.
#[pyclass(from_py_object, name = "LookupSwitchInsn", module = "pytecode._rust")]
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

    /// Item discriminator tag (``"lookupswitch"``).
    #[getter]
    fn kind(&self) -> &'static str {
        "lookupswitch"
    }

    /// Default jump target ``Label``.
    #[getter]
    fn default_target(&self) -> PyLabel {
        PyLabel::from(self.inner.default_target.clone())
    }

    /// Key-label dispatch pairs.
    #[getter]
    fn pairs(&self) -> Vec<(i32, PyLabel)> {
        self.inner
            .pairs
            .iter()
            .map(|(key, label)| (*key, PyLabel::from(label.clone())))
            .collect()
    }

    fn __repr__(&self) -> String {
        format!("LookupSwitchInsn(pairs={})", self.inner.pairs.len())
    }
}

/// Symbolic ``tableswitch`` instruction covering an inclusive integer range.
#[pyclass(from_py_object, name = "TableSwitchInsn", module = "pytecode._rust")]
#[derive(Clone)]
pub struct PyTableSwitchInsn {
    inner: TableSwitchInsn,
}

#[pymethods]
impl PyTableSwitchInsn {
    #[new]
    fn new(default_target: PyLabel, low: i32, high: i32, targets: Vec<PyLabel>) -> PyResult<Self> {
        validate_tableswitch_bounds(low, high, targets.len())?;
        Ok(Self {
            inner: TableSwitchInsn {
                default_target: default_target.inner,
                low,
                high,
                targets: targets.into_iter().map(|label| label.inner).collect(),
            },
        })
    }

    /// Item discriminator tag (``"tableswitch"``).
    #[getter]
    fn kind(&self) -> &'static str {
        "tableswitch"
    }

    /// Default jump target ``Label``.
    #[getter]
    fn default_target(&self) -> PyLabel {
        PyLabel::from(self.inner.default_target.clone())
    }

    /// Inclusive lower bound of the jump table.
    #[getter]
    fn low(&self) -> i32 {
        self.inner.low
    }

    /// Inclusive upper bound of the jump table.
    #[getter]
    fn high(&self) -> i32 {
        self.inner.high
    }

    /// Ordered branch target labels covering ``low..=high``.
    #[getter]
    fn targets(&self) -> Vec<PyLabel> {
        self.inner
            .targets
            .iter()
            .cloned()
            .map(PyLabel::from)
            .collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "TableSwitchInsn(low={}, high={}, targets={})",
            self.inner.low,
            self.inner.high,
            self.inner.targets.len()
        )
    }
}

// ---------------------------------------------------------------------------
// CodeItem <-> Python helpers
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
    PyValueError::new_err(message.into())
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
        "raw" => {
            let opcode: u8 = required_code_item_key(dict, "opcode")?.extract()?;
            validate_raw_opcode(opcode)?;
            Ok(CodeItem::Raw(Instruction::Simple { opcode, offset: 0 }))
        }
        "byte" => {
            let opcode: u8 = required_code_item_key(dict, "opcode")?.extract()?;
            validate_byte_opcode(opcode)?;
            Ok(CodeItem::Raw(Instruction::Byte {
                opcode,
                offset: 0,
                value: required_code_item_key(dict, "value")?.extract()?,
            }))
        }
        "short" => {
            let opcode: u8 = required_code_item_key(dict, "opcode")?.extract()?;
            validate_short_opcode(opcode)?;
            Ok(CodeItem::Raw(Instruction::Short {
                opcode,
                offset: 0,
                value: required_code_item_key(dict, "value")?.extract()?,
            }))
        }
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
        "field" => {
            let opcode: u8 = required_code_item_key(dict, "opcode")?.extract()?;
            validate_field_opcode(opcode)?;
            Ok(CodeItem::Field(FieldInsn {
                opcode,
                owner: required_code_item_key(dict, "owner")?.extract()?,
                name: required_code_item_key(dict, "name")?.extract()?,
                descriptor: required_code_item_key(dict, "descriptor")?.extract()?,
            }))
        }
        "method" => {
            let opcode: u8 = required_code_item_key(dict, "opcode")?.extract()?;
            validate_method_opcode(opcode)?;
            Ok(CodeItem::Method(MethodInsn {
                opcode,
                owner: required_code_item_key(dict, "owner")?.extract()?,
                name: required_code_item_key(dict, "name")?.extract()?,
                descriptor: required_code_item_key(dict, "descriptor")?.extract()?,
                is_interface: required_code_item_key(dict, "is_interface")?.extract()?,
            }))
        }
        "interface_method" => Ok(CodeItem::InterfaceMethod(InterfaceMethodInsn {
            owner: required_code_item_key(dict, "owner")?.extract()?,
            name: required_code_item_key(dict, "name")?.extract()?,
            descriptor: required_code_item_key(dict, "descriptor")?.extract()?,
        })),
        "type" => {
            let opcode: u8 = required_code_item_key(dict, "opcode")?.extract()?;
            validate_type_opcode(opcode)?;
            Ok(CodeItem::Type(TypeInsn {
                opcode,
                descriptor: required_code_item_key(dict, "descriptor")?.extract()?,
            }))
        }
        "var" => {
            let opcode: u8 = required_code_item_key(dict, "opcode")?.extract()?;
            validate_var_opcode(opcode)?;
            Ok(CodeItem::Var(VarInsn {
                opcode,
                slot: required_code_item_key(dict, "slot")?.extract()?,
            }))
        }
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
                "string" => LdcValue::String(crate::java_string::from_python(
                    required_code_item_key(dict, "value")?.cast::<PyString>()?,
                )?),
                "class" => LdcValue::Class(required_code_item_key(dict, "value")?.extract()?),
                "method_type" => {
                    LdcValue::MethodType(required_code_item_key(dict, "value")?.extract()?)
                }
                "method_handle" => {
                    let reference_kind: u8 =
                        required_code_item_key(dict, "reference_kind")?.extract()?;
                    validate_reference_kind(reference_kind)?;
                    LdcValue::MethodHandle(MethodHandleValue {
                        reference_kind,
                        owner: required_code_item_key(dict, "owner")?.extract()?,
                        name: required_code_item_key(dict, "name")?.extract()?,
                        descriptor: required_code_item_key(dict, "descriptor")?.extract()?,
                        is_interface: required_code_item_key(dict, "is_interface")?.extract()?,
                    })
                }
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
        "multianewarray" => {
            let dimensions: u8 = required_code_item_key(dict, "dimensions")?.extract()?;
            validate_multianewarray_dimensions(dimensions)?;
            Ok(CodeItem::MultiANewArray(MultiANewArrayInsn {
                descriptor: required_code_item_key(dict, "descriptor")?.extract()?,
                dimensions,
            }))
        }
        "branch" => {
            let opcode: u8 = required_code_item_key(dict, "opcode")?.extract()?;
            validate_branch_opcode(opcode)?;
            Ok(CodeItem::Branch(BranchInsn {
                opcode,
                target: extract_label(&required_code_item_key(dict, "target")?)?,
            }))
        }
        "lookupswitch" => {
            let pairs = required_code_item_key(dict, "pairs")?
                .cast::<PyList>()
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
                .cast::<PyList>()
                .map_err(|_| code_item_value_error("tableswitch targets must be a list"))?
                .iter()
                .map(|target| extract_label(&target))
                .collect::<PyResult<Vec<_>>>()?;
            let low: i32 = required_code_item_key(dict, "low")?.extract()?;
            let high: i32 = required_code_item_key(dict, "high")?.extract()?;
            validate_tableswitch_bounds(low, high, targets.len())?;
            Ok(CodeItem::TableSwitch(TableSwitchInsn {
                default_target: extract_label(&required_code_item_key(dict, "default_target")?)?,
                low,
                high,
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
    if let Ok(dict) = item.cast::<PyDict>() {
        return extract_code_item_dict(dict);
    }
    Err(code_item_value_error(
        "code items must be typed instruction objects or legacy dicts produced by CodeModel.instructions",
    ))
}

pub(crate) fn extract_code_items_iter(items: &Bound<'_, PyAny>) -> PyResult<Vec<CodeItem>> {
    items
        .try_iter()?
        .map(|entry| extract_code_item(&entry?))
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
    Owned(OwnedCodeState),
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
    OwnedMethodCode(Rc<RefCell<MethodModel>>),
    OwnedCode(OwnedCodeState),
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

/// Live read-only sequence view over interface names on a ``ClassModel``.
#[pyclass(
    from_py_object,
    name = "StringListView",
    module = "pytecode._rust",
    unsendable
)]
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

    fn __getitem__(&self, py: Python<'_>, key: &Bound<'_, PyAny>) -> PyResult<PyObject> {
        with_live_model(&self.state, |model_state| {
            if model_state.interfaces_generation != self.generation {
                return Err(stale_ref_err("interface view"));
            }
            let interfaces = &model_state.inner.as_ref().unwrap().interfaces;
            if let Ok(slice) = key.cast::<PySlice>() {
                let items: Vec<String> = slice_indices(slice, interfaces.len())?
                    .into_iter()
                    .map(|idx| interfaces[idx].clone())
                    .collect();
                return Ok(PyList::new(py, items)?.into_any().unbind());
            }
            let index: isize = key.extract()?;
            let normalized = normalize_index(index, interfaces.len())?;
            Ok(interfaces[normalized]
                .clone()
                .into_pyobject(py)?
                .unbind()
                .into())
        })
    }

    fn __iter__(&self, py: Python<'_>) -> PyResult<PyObject> {
        let items = self.to_list()?;
        Ok(PyList::new(py, items)?.call_method0("__iter__")?.unbind())
    }

    /// Materialize the current view items into a detached Python ``list``.
    fn to_list(&self) -> PyResult<Vec<String>> {
        with_live_model(&self.state, |model_state| {
            if model_state.interfaces_generation != self.generation {
                return Err(stale_ref_err("interface view"));
            }
            Ok(model_state.inner.as_ref().unwrap().interfaces.clone())
        })
    }

    fn __repr__(&self) -> PyResult<String> {
        Ok(format!("StringListView(len={})", self.__len__()?))
    }
}

/// Live read-only sequence view over ``FieldModel`` entries on a ``ClassModel``.
#[pyclass(
    from_py_object,
    name = "FieldListView",
    module = "pytecode._rust",
    unsendable
)]
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

    fn __getitem__(&self, py: Python<'_>, key: &Bound<'_, PyAny>) -> PyResult<PyObject> {
        with_live_model(&self.state, |model_state| {
            if model_state.fields_generation != self.generation {
                return Err(stale_ref_err("field view"));
            }
            let len = model_state.inner.as_ref().unwrap().fields.len();
            if let Ok(slice) = key.cast::<PySlice>() {
                let items: Vec<PyFieldModel> = slice_indices(slice, len)?
                    .into_iter()
                    .map(|index| PyFieldModel {
                        access: FieldAccess::Shared {
                            state: self.state.clone(),
                            index,
                            generation: self.generation,
                        },
                    })
                    .collect();
                return Ok(PyList::new(py, items)?.into_any().unbind());
            }
            let index: isize = key.extract()?;
            let item = PyFieldModel {
                access: FieldAccess::Shared {
                    state: self.state.clone(),
                    index: normalize_index(index, len)?,
                    generation: self.generation,
                },
            };
            Ok(Py::new(py, item)?.into_any())
        })
    }

    fn __iter__(&self, py: Python<'_>) -> PyResult<PyObject> {
        let items = self.to_list()?;
        Ok(PyList::new(py, items)?.call_method0("__iter__")?.unbind())
    }

    /// Materialize the current view items into a detached Python ``list``.
    fn to_list(&self) -> PyResult<Vec<PyFieldModel>> {
        let len = self.__len__()?;
        Ok((0..len)
            .map(|index| PyFieldModel {
                access: FieldAccess::Shared {
                    state: self.state.clone(),
                    index,
                    generation: self.generation,
                },
            })
            .collect())
    }

    fn __repr__(&self) -> PyResult<String> {
        Ok(format!("FieldListView(len={})", self.__len__()?))
    }
}

/// Live read-only sequence view over ``MethodModel`` entries on a ``ClassModel``.
#[pyclass(
    from_py_object,
    name = "MethodListView",
    module = "pytecode._rust",
    unsendable
)]
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

    fn __getitem__(&self, py: Python<'_>, key: &Bound<'_, PyAny>) -> PyResult<PyObject> {
        with_live_model(&self.state, |model_state| {
            if model_state.methods_generation != self.generation {
                return Err(stale_ref_err("method view"));
            }
            let len = model_state.inner.as_ref().unwrap().methods.len();
            if let Ok(slice) = key.cast::<PySlice>() {
                let items: Vec<PyMethodModel> = slice_indices(slice, len)?
                    .into_iter()
                    .map(|index| PyMethodModel {
                        access: MethodAccess::Shared {
                            state: self.state.clone(),
                            index,
                            generation: self.generation,
                        },
                    })
                    .collect();
                return Ok(PyList::new(py, items)?.into_any().unbind());
            }
            let index: isize = key.extract()?;
            let item = PyMethodModel {
                access: MethodAccess::Shared {
                    state: self.state.clone(),
                    index: normalize_index(index, len)?,
                    generation: self.generation,
                },
            };
            Ok(Py::new(py, item)?.into_any())
        })
    }

    fn __iter__(&self, py: Python<'_>) -> PyResult<PyObject> {
        let items = self.to_list()?;
        Ok(PyList::new(py, items)?.call_method0("__iter__")?.unbind())
    }

    /// Materialize the current view items into a detached Python ``list``.
    fn to_list(&self) -> PyResult<Vec<PyMethodModel>> {
        let len = self.__len__()?;
        Ok((0..len)
            .map(|index| PyMethodModel {
                access: MethodAccess::Shared {
                    state: self.state.clone(),
                    index,
                    generation: self.generation,
                },
            })
            .collect())
    }

    fn __repr__(&self) -> PyResult<String> {
        Ok(format!("MethodListView(len={})", self.__len__()?))
    }
}

impl AttributeOwner {
    fn with_attrs<R>(
        &self,
        f: impl FnOnce(&[pytecode_engine::raw::AttributeInfo]) -> PyResult<R>,
    ) -> PyResult<R> {
        match self {
            AttributeOwner::Class(state) => with_live_model(state, |model_state| {
                f(&model_state.inner.as_ref().unwrap().attributes)
            }),
            AttributeOwner::Field {
                state,
                index,
                generation,
            } => with_live_model(state, |model_state| {
                if model_state.fields_generation != *generation {
                    return Err(stale_ref_err("field attribute view"));
                }
                let field = model_state
                    .inner
                    .as_ref()
                    .unwrap()
                    .fields
                    .get(*index)
                    .ok_or_else(|| stale_ref_err("field attribute view"))?;
                f(&field.attributes)
            }),
            AttributeOwner::Method {
                state,
                index,
                generation,
            } => with_live_model(state, |model_state| {
                if model_state.methods_generation != *generation {
                    return Err(stale_ref_err("method attribute view"));
                }
                let method = model_state
                    .inner
                    .as_ref()
                    .unwrap()
                    .methods
                    .get(*index)
                    .ok_or_else(|| stale_ref_err("method attribute view"))?;
                f(&method.attributes)
            }),
            AttributeOwner::Code {
                state,
                method_index,
                methods_generation,
            } => with_live_model(state, |model_state| {
                if model_state.methods_generation != *methods_generation {
                    return Err(stale_ref_err("code attribute view"));
                }
                let method = model_state
                    .inner
                    .as_ref()
                    .unwrap()
                    .methods
                    .get(*method_index)
                    .ok_or_else(|| stale_ref_err("code attribute view"))?;
                let code = method
                    .code()
                    .ok_or_else(|| PyRuntimeError::new_err("method has no code"))?;
                f(&code.attributes)
            }),
            AttributeOwner::OwnedField(field) => {
                let borrowed = field.try_borrow().map_err(|_| borrow_err("FieldModel"))?;
                f(&borrowed.attributes)
            }
            AttributeOwner::OwnedMethod(method) => {
                let borrowed = method.try_borrow().map_err(|_| borrow_err("MethodModel"))?;
                f(&borrowed.attributes)
            }
            AttributeOwner::OwnedMethodCode(method) => {
                let borrowed = method.try_borrow().map_err(|_| borrow_err("MethodModel"))?;
                let code = borrowed
                    .code()
                    .ok_or_else(|| PyRuntimeError::new_err("method has no code"))?;
                f(&code.attributes)
            }
            AttributeOwner::OwnedCode(code_state) => {
                let borrowed = code_state
                    .try_borrow()
                    .map_err(|_| borrow_err("CodeModel"))?;
                let code = borrowed.as_ref().ok_or_else(dead_code_err)?;
                f(&code.attributes)
            }
        }
    }
}

/// Live read-only sequence view over raw attributes attached to a model element.
#[pyclass(
    from_py_object,
    name = "AttributeListView",
    module = "pytecode._rust",
    unsendable
)]
#[derive(Clone)]
pub struct PyAttributeListView {
    owner: AttributeOwner,
}

#[pymethods]
impl PyAttributeListView {
    fn __len__(&self) -> PyResult<usize> {
        self.owner.with_attrs(|attrs| Ok(attrs.len()))
    }

    fn __getitem__(&self, py: Python<'_>, key: &Bound<'_, PyAny>) -> PyResult<PyObject> {
        self.owner.with_attrs(|attrs| {
            if let Ok(slice) = key.cast::<PySlice>() {
                let items = slice_indices(slice, attrs.len())?
                    .into_iter()
                    .map(|idx| crate::wrap_attribute(py, &attrs[idx]))
                    .collect::<PyResult<Vec<_>>>()?;
                return Ok(PyList::new(py, items)?.into_any().unbind());
            }
            let index: isize = key.extract()?;
            let normalized = normalize_index(index, attrs.len())?;
            crate::wrap_attribute(py, &attrs[normalized])
        })
    }

    fn __iter__(&self, py: Python<'_>) -> PyResult<PyObject> {
        let items = self.to_list(py)?;
        Ok(PyList::new(py, items)?.call_method0("__iter__")?.unbind())
    }

    /// Materialize the current view items into a detached Python ``list``.
    fn to_list(&self, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        self.owner.with_attrs(|attrs| {
            attrs
                .iter()
                .map(|attr| crate::wrap_attribute(py, attr))
                .collect()
        })
    }

    fn __repr__(&self) -> PyResult<String> {
        Ok(format!("AttributeListView(len={})", self.__len__()?))
    }
}

fn wrap_code_list_item(
    py: Python<'_>,
    code: &CodeModel,
    kind: CodeListKind,
    index: usize,
) -> PyResult<PyObject> {
    match kind {
        CodeListKind::Instructions => wrap_code_item(py, &code.instructions[index]),
        CodeListKind::ExceptionHandlers => Ok(Py::new(
            py,
            PyModelExceptionHandler {
                inner: code.exception_handlers[index].clone(),
            },
        )?
        .into_any()),
        CodeListKind::LineNumbers => {
            let item = &code.line_numbers[index];
            wrap_line_number(py, &item.label, item.line_number)
        }
        CodeListKind::LocalVariables => {
            let item = &code.local_variables[index];
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
            let item = &code.local_variable_types[index];
            wrap_local_variable_type(
                py,
                &item.start,
                &item.end,
                &item.name,
                &item.signature,
                item.index,
            )
        }
    }
}

/// Live read-only sequence view over code items or debug entries on a ``CodeModel``.
#[pyclass(
    from_py_object,
    name = "CodeListView",
    module = "pytecode._rust",
    unsendable
)]
#[derive(Clone)]
pub struct PyCodeListView {
    access: CodeAccess,
    kind: CodeListKind,
}

#[pymethods]
impl PyCodeListView {
    fn __len__(&self) -> PyResult<usize> {
        self.with_code(|code| Ok(Self::len_for_code(code, self.kind)))
    }

    fn __getitem__(&self, py: Python<'_>, key: &Bound<'_, PyAny>) -> PyResult<PyObject> {
        self.with_code(|code| {
            let len = Self::len_for_code(code, self.kind);
            if let Ok(slice) = key.cast::<PySlice>() {
                let items = slice_indices(slice, len)?
                    .into_iter()
                    .map(|idx| wrap_code_list_item(py, code, self.kind, idx))
                    .collect::<PyResult<Vec<_>>>()?;
                return Ok(PyList::new(py, items)?.into_any().unbind());
            }
            let index: isize = key.extract()?;
            let normalized = normalize_index(index, len)?;
            wrap_code_list_item(py, code, self.kind, normalized)
        })
    }

    fn __iter__(&self, py: Python<'_>) -> PyResult<PyObject> {
        let items = self.to_list(py)?;
        Ok(PyList::new(py, items)?.call_method0("__iter__")?.unbind())
    }

    /// Materialize the current view items into a detached Python ``list``.
    fn to_list(&self, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        self.with_code(|code| {
            let len = Self::len_for_code(code, self.kind);
            (0..len)
                .map(|idx| wrap_code_list_item(py, code, self.kind, idx))
                .collect()
        })
    }

    fn __repr__(&self) -> PyResult<String> {
        Ok(format!("CodeListView(len={})", self.__len__()?))
    }
}

impl PyCodeListView {
    fn len_for_code(code: &CodeModel, kind: CodeListKind) -> usize {
        match kind {
            CodeListKind::Instructions => code.instructions.len(),
            CodeListKind::ExceptionHandlers => code.exception_handlers.len(),
            CodeListKind::LineNumbers => code.line_numbers.len(),
            CodeListKind::LocalVariables => code.local_variables.len(),
            CodeListKind::LocalVariableTypes => code.local_variable_types.len(),
        }
    }

    fn with_code<R>(&self, f: impl FnOnce(&CodeModel) -> PyResult<R>) -> PyResult<R> {
        with_code_access(&self.access, f)
    }
}

fn with_code_access<R>(
    access: &CodeAccess,
    f: impl FnOnce(&CodeModel) -> PyResult<R>,
) -> PyResult<R> {
    match access {
        CodeAccess::Shared {
            state,
            method_index,
            methods_generation,
        } => with_live_model(state, |model_state| {
            if model_state.methods_generation != *methods_generation {
                return Err(stale_ref_err("code ref"));
            }
            let method = model_state
                .inner
                .as_ref()
                .unwrap()
                .methods
                .get(*method_index)
                .ok_or_else(|| stale_ref_err("code ref"))?;
            let code = method
                .code()
                .ok_or_else(|| PyRuntimeError::new_err("method has no code"))?;
            f(code)
        }),
        CodeAccess::OwnedMethod(method) => {
            let borrowed = method.try_borrow().map_err(|_| borrow_err("MethodModel"))?;
            let code = borrowed
                .code()
                .ok_or_else(|| PyRuntimeError::new_err("method has no code"))?;
            f(code)
        }
        CodeAccess::Owned(code_state) => {
            let borrowed = code_state
                .try_borrow()
                .map_err(|_| borrow_err("CodeModel"))?;
            let code = borrowed.as_ref().ok_or_else(dead_code_err)?;
            f(code)
        }
    }
}

fn with_code_access_mut<R>(
    access: &CodeAccess,
    f: impl FnOnce(&mut CodeModel) -> PyResult<R>,
) -> PyResult<R> {
    match access {
        CodeAccess::Shared {
            state,
            method_index,
            methods_generation,
        } => with_live_model_mut(state, |model_state| {
            if model_state.methods_generation != *methods_generation {
                return Err(stale_ref_err("code ref"));
            }
            let method = model_state
                .inner
                .as_mut()
                .unwrap()
                .methods
                .get_mut(*method_index)
                .ok_or_else(|| stale_ref_err("code ref"))?;
            let code = method
                .code_mut()
                .ok_or_else(|| PyRuntimeError::new_err("method has no code"))?;
            f(code)
        }),
        CodeAccess::OwnedMethod(method) => {
            let mut borrowed = method
                .try_borrow_mut()
                .map_err(|_| borrow_err("MethodModel"))?;
            let code = borrowed
                .code_mut()
                .ok_or_else(|| PyRuntimeError::new_err("method has no code"))?;
            f(code)
        }
        CodeAccess::Owned(code_state) => {
            let mut borrowed = code_state
                .try_borrow_mut()
                .map_err(|_| borrow_err("CodeModel"))?;
            let code = borrowed.as_mut().ok_or_else(dead_code_err)?;
            f(code)
        }
    }
}

// ---------------------------------------------------------------------------
// PyCodeModel
// ---------------------------------------------------------------------------

fn compile_insn_matcher(
    matcher: &PyInsnMatcher,
) -> PyResult<pytecode_engine::transform::InsnMatcher> {
    let compiled = Arc::new(
        matcher
            .spec
            .try_compile()
            .map_err(|e| PyValueError::new_err(format!("invalid regex pattern: {e}")))?,
    );
    Ok(pytecode_engine::transform::Matcher::of(
        move |item| compiled.matches(item),
        matcher.spec.to_string(),
    ))
}

fn compile_insn_matcher_seq(
    pattern: &Bound<'_, PyAny>,
) -> PyResult<Vec<pytecode_engine::transform::InsnMatcher>> {
    pattern
        .try_iter()?
        .map(|entry| {
            let matcher: PyRef<'_, PyInsnMatcher> = entry?.extract()?;
            compile_insn_matcher(&matcher)
        })
        .collect()
}

/// Mutable symbolic representation of a method's ``Code`` attribute.
#[pyclass(
    from_py_object,
    name = "CodeModel",
    module = "pytecode._rust",
    unsendable
)]
#[derive(Clone)]
pub struct PyCodeModel {
    access: CodeAccess,
}

impl PyCodeModel {
    pub(crate) fn from_owned(code: CodeModel) -> Self {
        Self {
            access: CodeAccess::Owned(Rc::new(RefCell::new(Some(code)))),
        }
    }

    pub(crate) fn with_code<R>(&self, f: impl FnOnce(&CodeModel) -> PyResult<R>) -> PyResult<R> {
        with_code_access(&self.access, f)
    }

    pub(crate) fn with_code_mut<R>(
        &mut self,
        f: impl FnOnce(&mut CodeModel) -> PyResult<R>,
    ) -> PyResult<R> {
        with_code_access_mut(&self.access, f)
    }

    pub(crate) fn take_owned(&mut self) -> PyResult<CodeModel> {
        match &self.access {
            CodeAccess::Owned(code_state) => {
                let mut borrowed = code_state
                    .try_borrow_mut()
                    .map_err(|_| borrow_err("CodeModel"))?;
                borrowed.take().ok_or_else(dead_code_err)
            }
            _ => Err(PyRuntimeError::new_err(
                "cannot move ownership out of a borrowed CodeModel",
            )),
        }
    }

    fn clone_inner(&self) -> PyResult<CodeModel> {
        self.with_code(|code| Ok(code.clone()))
    }
}

#[pymethods]
impl PyCodeModel {
    #[new]
    #[pyo3(signature = (
        max_stack=0,
        max_locals=0,
        instructions=None,
        exception_handlers=None,
        line_numbers=None,
        local_variables=None,
        local_variable_types=None
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        max_stack: u16,
        max_locals: u16,
        instructions: Option<&Bound<'_, PyAny>>,
        exception_handlers: Option<&Bound<'_, PyAny>>,
        line_numbers: Option<&Bound<'_, PyAny>>,
        local_variables: Option<&Bound<'_, PyAny>>,
        local_variable_types: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let mut code = CodeModel::new(max_stack, max_locals, DebugInfoState::Fresh);
        if let Some(items) = instructions {
            code.instructions = extract_code_items_iter(items)?;
        }
        if let Some(items) = exception_handlers {
            code.exception_handlers = items
                .try_iter()?
                .map(|entry| {
                    Ok(entry?
                        .extract::<PyRef<'_, PyModelExceptionHandler>>()?
                        .inner
                        .clone())
                })
                .collect::<PyResult<Vec<_>>>()?;
        }
        if let Some(items) = line_numbers {
            code.line_numbers = items
                .try_iter()?
                .map(|entry| {
                    let entry: PyRef<'_, PyLineNumberEntry> = entry?.extract()?;
                    Ok(LineNumberEntry {
                        label: entry.label.clone(),
                        line_number: entry.line_number,
                    })
                })
                .collect::<PyResult<Vec<_>>>()?;
        }
        if let Some(items) = local_variables {
            code.local_variables = items
                .try_iter()?
                .map(|entry| {
                    let entry: PyRef<'_, PyLocalVariableEntry> = entry?.extract()?;
                    Ok(LocalVariableEntry {
                        start: entry.start.clone(),
                        end: entry.end.clone(),
                        name: entry.name.clone(),
                        descriptor: entry.descriptor.clone(),
                        index: entry.index,
                    })
                })
                .collect::<PyResult<Vec<_>>>()?;
        }
        if let Some(items) = local_variable_types {
            code.local_variable_types = items
                .try_iter()?
                .map(|entry| {
                    let entry: PyRef<'_, PyLocalVariableTypeEntry> = entry?.extract()?;
                    Ok(LocalVariableTypeEntry {
                        start: entry.start.clone(),
                        end: entry.end.clone(),
                        name: entry.name.clone(),
                        signature: entry.signature.clone(),
                        index: entry.index,
                    })
                })
                .collect::<PyResult<Vec<_>>>()?;
        }
        Ok(Self::from_owned(code))
    }

    /// Declared or recomputed maximum operand-stack depth.
    #[getter]
    fn max_stack(&self) -> PyResult<u16> {
        self.with_code(|code| Ok(code.max_stack))
    }

    #[setter]
    fn set_max_stack(&mut self, value: u16) -> PyResult<()> {
        self.with_code_mut(|code| {
            code.max_stack = value;
            Ok(())
        })
    }

    /// Declared or recomputed maximum local-variable slots.
    #[getter]
    fn max_locals(&self) -> PyResult<u16> {
        self.with_code(|code| Ok(code.max_locals))
    }

    #[setter]
    fn set_max_locals(&mut self, value: u16) -> PyResult<()> {
        self.with_code_mut(|code| {
            code.max_locals = value;
            Ok(())
        })
    }

    /// Live view of the symbolic instruction and label sequence.
    #[getter]
    fn instructions(&self) -> PyCodeListView {
        PyCodeListView {
            access: self.access.clone(),
            kind: CodeListKind::Instructions,
        }
    }

    #[setter]
    fn set_instructions(&mut self, items: &Bound<'_, PyAny>) -> PyResult<()> {
        let extracted = extract_code_items_iter(items)?;
        self.with_code_mut(|code| {
            code.instructions = extracted;
            Ok(())
        })
    }

    /// Live view of symbolic exception handlers.
    #[getter]
    fn exception_handlers(&self) -> PyCodeListView {
        PyCodeListView {
            access: self.access.clone(),
            kind: CodeListKind::ExceptionHandlers,
        }
    }

    #[setter]
    fn set_exception_handlers(&mut self, items: &Bound<'_, PyAny>) -> PyResult<()> {
        let extracted = items
            .try_iter()?
            .map(|entry| {
                Ok(entry?
                    .extract::<PyRef<'_, PyModelExceptionHandler>>()?
                    .inner
                    .clone())
            })
            .collect::<PyResult<Vec<_>>>()?;
        self.with_code_mut(|code| {
            code.exception_handlers = extracted;
            Ok(())
        })
    }

    /// Live view of symbolic line-number entries.
    #[getter]
    fn line_numbers(&self) -> PyCodeListView {
        PyCodeListView {
            access: self.access.clone(),
            kind: CodeListKind::LineNumbers,
        }
    }

    #[setter]
    fn set_line_numbers(&mut self, items: &Bound<'_, PyAny>) -> PyResult<()> {
        let extracted = items
            .try_iter()?
            .map(|entry| {
                let entry: PyRef<'_, PyLineNumberEntry> = entry?.extract()?;
                Ok(LineNumberEntry {
                    label: entry.label.clone(),
                    line_number: entry.line_number,
                })
            })
            .collect::<PyResult<Vec<_>>>()?;
        self.with_code_mut(|code| {
            code.line_numbers = extracted;
            Ok(())
        })
    }

    /// Live view of symbolic local-variable debug entries.
    #[getter]
    fn local_variables(&self) -> PyCodeListView {
        PyCodeListView {
            access: self.access.clone(),
            kind: CodeListKind::LocalVariables,
        }
    }

    #[setter]
    fn set_local_variables(&mut self, items: &Bound<'_, PyAny>) -> PyResult<()> {
        let extracted = items
            .try_iter()?
            .map(|entry| {
                let entry: PyRef<'_, PyLocalVariableEntry> = entry?.extract()?;
                Ok(LocalVariableEntry {
                    start: entry.start.clone(),
                    end: entry.end.clone(),
                    name: entry.name.clone(),
                    descriptor: entry.descriptor.clone(),
                    index: entry.index,
                })
            })
            .collect::<PyResult<Vec<_>>>()?;
        self.with_code_mut(|code| {
            code.local_variables = extracted;
            Ok(())
        })
    }

    /// Live view of symbolic local-variable generic signature entries.
    #[getter]
    fn local_variable_types(&self) -> PyCodeListView {
        PyCodeListView {
            access: self.access.clone(),
            kind: CodeListKind::LocalVariableTypes,
        }
    }

    #[setter]
    fn set_local_variable_types(&mut self, items: &Bound<'_, PyAny>) -> PyResult<()> {
        let extracted = items
            .try_iter()?
            .map(|entry| {
                let entry: PyRef<'_, PyLocalVariableTypeEntry> = entry?.extract()?;
                Ok(LocalVariableTypeEntry {
                    start: entry.start.clone(),
                    end: entry.end.clone(),
                    name: entry.name.clone(),
                    signature: entry.signature.clone(),
                    index: entry.index,
                })
            })
            .collect::<PyResult<Vec<_>>>()?;
        self.with_code_mut(|code| {
            code.local_variable_types = extracted;
            Ok(())
        })
    }

    /// Live view of non-debug custom attributes nested on this ``Code`` attribute.
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
            CodeAccess::OwnedMethod(method) => AttributeOwner::OwnedMethodCode(method.clone()),
            CodeAccess::Owned(code) => AttributeOwner::OwnedCode(code.clone()),
        };
        PyAttributeListView { owner }
    }

    /// Debug-info freshness marker (``"fresh"`` or ``"stale"``).
    #[getter]
    fn debug_info_state(&self) -> PyResult<&'static str> {
        self.with_code(|code| {
            Ok(match code.debug_info_state {
                DebugInfoState::Fresh => "fresh",
                DebugInfoState::Stale => "stale",
            })
        })
    }

    /// Ordered nested code attribute category layout preserved from the source classfile.
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

    /// Return indices of all instructions matching ``matcher``.
    fn find_insns(&self, matcher: &PyInsnMatcher) -> PyResult<Vec<usize>> {
        let compiled = matcher
            .spec
            .try_compile()
            .map_err(|e| PyValueError::new_err(format!("invalid regex pattern: {e}")))?;
        self.with_code(|code| {
            Ok(code
                .instructions
                .iter()
                .enumerate()
                .filter_map(|(index, item)| compiled.matches(item).then_some(index))
                .collect())
        })
    }

    /// Return the first index at or after ``start`` matching ``matcher``, or ``None``.
    #[pyo3(signature = (matcher, start=0))]
    fn find_insn(&self, matcher: &PyInsnMatcher, start: usize) -> PyResult<Option<usize>> {
        let compiled = matcher
            .spec
            .try_compile()
            .map_err(|e| PyValueError::new_err(format!("invalid regex pattern: {e}")))?;
        self.with_code(|code| {
            Ok(code
                .instructions
                .iter()
                .enumerate()
                .skip(start)
                .find_map(|(index, item)| compiled.matches(item).then_some(index)))
        })
    }

    /// Return whether any instruction matches ``matcher``.
    fn contains_insn(&self, matcher: &PyInsnMatcher) -> PyResult<bool> {
        let compiled = matcher
            .spec
            .try_compile()
            .map_err(|e| PyValueError::new_err(format!("invalid regex pattern: {e}")))?;
        self.with_code(|code| Ok(code.instructions.iter().any(|item| compiled.matches(item))))
    }

    /// Count instructions matching ``matcher``.
    fn count_insns(&self, matcher: &PyInsnMatcher) -> PyResult<usize> {
        let compiled = matcher
            .spec
            .try_compile()
            .map_err(|e| PyValueError::new_err(format!("invalid regex pattern: {e}")))?;
        self.with_code(|code| {
            Ok(code
                .instructions
                .iter()
                .filter(|item| compiled.matches(item))
                .count())
        })
    }

    /// Find all starting indices of contiguous instruction sequences matching ``matchers``.
    fn find_sequences(&self, matchers: &Bound<'_, PyAny>) -> PyResult<Vec<usize>> {
        let compiled = matchers
            .try_iter()?
            .map(|entry| {
                let matcher: PyRef<'_, PyInsnMatcher> = entry?.extract()?;
                matcher
                    .spec
                    .try_compile()
                    .map_err(|e| PyValueError::new_err(format!("invalid regex pattern: {e}")))
            })
            .collect::<PyResult<Vec<_>>>()?;
        self.with_code(|code| {
            if compiled.is_empty() || compiled.len() > code.instructions.len() {
                return Ok(Vec::new());
            }
            Ok(code
                .instructions
                .windows(compiled.len())
                .enumerate()
                .filter_map(|(index, window)| {
                    compiled
                        .iter()
                        .zip(window.iter())
                        .all(|(matcher, item)| matcher.matches(item))
                        .then_some(index)
                })
                .collect())
        })
    }

    /// Replace every instruction matching ``matcher`` with ``replacement``; returns replacement count.
    fn replace_insns(
        &mut self,
        matcher: &PyInsnMatcher,
        replacement: &Bound<'_, PyAny>,
    ) -> PyResult<usize> {
        let compiled = compile_insn_matcher(matcher)?;
        let replacement = extract_code_items_iter(replacement)?;
        self.with_code_mut(|code| Ok(code.replace_insns(&compiled, &replacement)))
    }

    /// Remove every instruction matching ``matcher``; returns removed count.
    fn remove_insns(&mut self, matcher: &PyInsnMatcher) -> PyResult<usize> {
        let compiled = compile_insn_matcher(matcher)?;
        self.with_code_mut(|code| Ok(code.remove_insns(&compiled)))
    }

    /// Insert ``items`` immediately before every instruction matching ``matcher``; returns insertion site count.
    fn insert_before(
        &mut self,
        matcher: &PyInsnMatcher,
        items: &Bound<'_, PyAny>,
    ) -> PyResult<usize> {
        let compiled = compile_insn_matcher(matcher)?;
        let items = extract_code_items_iter(items)?;
        self.with_code_mut(|code| Ok(code.insert_before(&compiled, &items)))
    }

    /// Insert ``items`` immediately after every instruction matching ``matcher``; returns insertion site count.
    fn insert_after(
        &mut self,
        matcher: &PyInsnMatcher,
        items: &Bound<'_, PyAny>,
    ) -> PyResult<usize> {
        let compiled = compile_insn_matcher(matcher)?;
        let items = extract_code_items_iter(items)?;
        self.with_code_mut(|code| Ok(code.insert_after(&compiled, &items)))
    }

    /// Replace every non-overlapping sequence matching ``pattern`` with ``replacement``; returns site count.
    fn replace_sequences(
        &mut self,
        pattern: &Bound<'_, PyAny>,
        replacement: &Bound<'_, PyAny>,
    ) -> PyResult<usize> {
        let compiled = compile_insn_matcher_seq(pattern)?;
        let replacement = extract_code_items_iter(replacement)?;
        self.with_code_mut(|code| Ok(code.replace_sequences(&compiled, &replacement)))
    }

    /// Remove every non-overlapping sequence matching ``pattern``; returns site count.
    fn remove_sequences(&mut self, pattern: &Bound<'_, PyAny>) -> PyResult<usize> {
        let compiled = compile_insn_matcher_seq(pattern)?;
        self.with_code_mut(|code| Ok(code.remove_sequences(&compiled)))
    }

    fn __repr__(&self) -> PyResult<String> {
        self.with_code(|code| {
            Ok(format!(
                "CodeModel(max_stack={}, max_locals={}, insns={})",
                code.max_stack,
                code.max_locals,
                code.instructions.len()
            ))
        })
    }
}

// ---------------------------------------------------------------------------
// PyFieldModel
// ---------------------------------------------------------------------------

/// Mutable symbolic representation of a field declaration on a ``ClassModel``.
#[pyclass(
    from_py_object,
    name = "FieldModel",
    module = "pytecode._rust",
    unsendable
)]
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
                let field = model_state
                    .inner
                    .as_ref()
                    .unwrap()
                    .fields
                    .get(*index)
                    .ok_or_else(|| stale_ref_err("field ref"))?;
                f(field)
            }),
            FieldAccess::Owned(field) => {
                let borrowed = field.try_borrow().map_err(|_| borrow_err("FieldModel"))?;
                f(&borrowed)
            }
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
                    let field = model_state
                        .inner
                        .as_mut()
                        .unwrap()
                        .fields
                        .get_mut(index)
                        .ok_or_else(|| stale_ref_err("field ref"))?;
                    f(field)
                })
            }
            FieldAccess::Owned(field) => {
                let mut borrowed = field
                    .try_borrow_mut()
                    .map_err(|_| borrow_err("FieldModel"))?;
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

    /// Raw JVM field access flags bitset.
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

    /// Field identifier name.
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

    /// JVM field descriptor.
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

    /// Live view of raw attributes attached to this field.
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

    fn __repr__(&self) -> PyResult<String> {
        self.with_field(|field| {
            Ok(format!(
                "FieldModel(name={:?}, descriptor={:?}, access_flags=0x{:04x})",
                field.name,
                field.descriptor,
                field.access_flags.bits()
            ))
        })
    }
}

// ---------------------------------------------------------------------------
// PyMethodModel
// ---------------------------------------------------------------------------

/// Mutable symbolic representation of a method declaration on a ``ClassModel``.
#[pyclass(
    from_py_object,
    name = "MethodModel",
    module = "pytecode._rust",
    unsendable
)]
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
                let method = model_state
                    .inner
                    .as_ref()
                    .unwrap()
                    .methods
                    .get(*index)
                    .ok_or_else(|| stale_ref_err("method ref"))?;
                f(method)
            }),
            MethodAccess::Owned(method) => {
                let borrowed = method.try_borrow().map_err(|_| borrow_err("MethodModel"))?;
                f(&borrowed)
            }
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
                    let method = model_state
                        .inner
                        .as_mut()
                        .unwrap()
                        .methods
                        .get_mut(index)
                        .ok_or_else(|| stale_ref_err("method ref"))?;
                    f(method)
                })
            }
            MethodAccess::Owned(method) => {
                let mut borrowed = method
                    .try_borrow_mut()
                    .map_err(|_| borrow_err("MethodModel"))?;
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

    /// Raw JVM method access flags bitset.
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

    /// Method name (including ``<init>`` and ``<clinit>``).
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

    /// JVM method descriptor.
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

    /// Whether this method currently has a symbolic or prebuilt body.
    #[getter]
    fn has_code(&self) -> PyResult<bool> {
        self.with_method(|method| Ok(method.has_code()))
    }

    /// Editable symbolic ``CodeModel`` for this method, or ``None``.
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
                let method = model_state
                    .inner
                    .as_ref()
                    .unwrap()
                    .methods
                    .get(*index)
                    .ok_or_else(|| stale_ref_err("method ref"))?;
                Ok(method.code().map(|_| PyCodeModel {
                    access: CodeAccess::Shared {
                        state: state.clone(),
                        method_index: *index,
                        methods_generation: *generation,
                    },
                }))
            }),
            MethodAccess::Owned(method) => {
                let borrowed = method.try_borrow().map_err(|_| borrow_err("MethodModel"))?;
                let has_symbolic = borrowed.code().is_some();
                Ok(has_symbolic.then(|| PyCodeModel {
                    access: CodeAccess::OwnedMethod(method.clone()),
                }))
            }
        }
    }

    #[setter]
    fn set_code(&mut self, value: Option<PyCodeModel>) -> PyResult<()> {
        let code = value.map(|c| c.clone_inner()).transpose()?;
        self.with_method_mut(|method| {
            method.set_code(code);
            Ok(())
        })
    }

    /// Lift raw JVM instruction bytes into this method's symbolic ``CodeModel``.
    #[pyo3(signature = (max_stack, max_locals, code_bytes, cp=None))]
    fn set_raw_code(
        &mut self,
        max_stack: u16,
        max_locals: u16,
        code_bytes: &Bound<'_, PyAny>,
        cp: Option<&PyConstantPoolBuilder>,
    ) -> PyResult<()> {
        let code_model = crate::with_bytes(code_bytes, |raw| {
            if let Some(cp_builder) = cp {
                cp_builder.with_builder(|builder| {
                    CodeModel::from_raw_code(max_stack, max_locals, raw, builder)
                        .map_err(crate::engine_error_to_py)
                })
            } else {
                match &self.access {
                    MethodAccess::Shared {
                        state,
                        index,
                        generation,
                    } => with_live_model(state, |model_state| {
                        if model_state.methods_generation != *generation {
                            return Err(stale_ref_err("method ref"));
                        }
                        let model = model_state.inner.as_ref().unwrap();
                        if model.methods.get(*index).is_none() {
                            return Err(stale_ref_err("method ref"));
                        }
                        CodeModel::from_raw_code(max_stack, max_locals, raw, &model.constant_pool)
                            .map_err(crate::engine_error_to_py)
                    }),
                    MethodAccess::Owned(_) => {
                        let empty = ConstantPoolBuilder::new();
                        CodeModel::from_raw_code(max_stack, max_locals, raw, &empty)
                            .map_err(crate::engine_error_to_py)
                    }
                }
            }
        })?;
        self.with_method_mut(|method| {
            method.set_code(Some(code_model));
            Ok(())
        })
    }

    /// Attach raw prebuilt ``Code`` attribute payload bytes to this method.
    fn set_prebuilt_code(&mut self, code_body_bytes: &Bound<'_, PyAny>) -> PyResult<()> {
        let bytes = crate::extract_bytes(code_body_bytes)?;
        self.with_method_mut(|method| {
            method.set_prebuilt_code_bytes(bytes);
            Ok(())
        })
    }

    /// Live view of raw attributes attached to this method declaration.
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

    fn __repr__(&self) -> PyResult<String> {
        self.with_method(|method| {
            Ok(format!(
                "MethodModel(name={:?}, descriptor={:?}, access_flags=0x{:04x}, has_code={})",
                method.name,
                method.descriptor,
                method.access_flags.bits(),
                method.has_code()
            ))
        })
    }
}

// ---------------------------------------------------------------------------
// PyMappingClassResolver
// ---------------------------------------------------------------------------

/// Pre-resolved in-memory class hierarchy map indexed by internal JVM class name.
#[pyclass(
    from_py_object,
    name = "MappingClassResolver",
    module = "pytecode._rust"
)]
#[derive(Clone)]
pub struct PyMappingClassResolver {
    pub(crate) inner: MappingClassResolver,
}

#[pymethods]
impl PyMappingClassResolver {
    #[new]
    fn new(py: Python<'_>) -> PyResult<Self> {
        Ok(Self {
            inner: MappingClassResolver::new(std::iter::empty())
                .map_err(|e| analysis_error_to_py(py, e))?,
        })
    }

    /// Build a resolver from an iterable of raw classfile ``bytes`` or ``bytearray`` payloads.
    ///
    /// ``duplicates`` decides what happens when two payloads declare the same
    /// class: ``"error"`` (default) rejects the input, ``"first"`` keeps the
    /// earliest definition, and ``"last"`` keeps the latest.
    #[staticmethod]
    #[pyo3(signature = (class_bytes_list, *, duplicates = "error"))]
    fn from_bytes(
        py: Python<'_>,
        class_bytes_list: &Bound<'_, PyAny>,
        duplicates: &str,
    ) -> PyResult<Self> {
        let policy = parse_duplicate_policy(duplicates)?;
        let mut resolved = Vec::new();
        for item in class_bytes_list.try_iter()? {
            let item = item?;
            let rc = crate::with_bytes(&item, |bytes| {
                let classfile = parse_class(bytes).map_err(crate::engine_error_to_py)?;
                ResolvedClass::from_classfile(&classfile).map_err(crate::engine_error_to_py)
            })?;
            resolved.push(rc);
        }
        let resolver = MappingClassResolver::with_duplicate_policy(resolved, policy)
            .map_err(|e| analysis_error_to_py(py, e))?;
        Ok(Self { inner: resolver })
    }

    /// Build a resolver from an iterable of mutable ``ClassModel`` instances.
    ///
    /// See ``from_bytes`` for ``duplicates``.
    #[staticmethod]
    #[pyo3(signature = (models, *, duplicates = "error"))]
    fn from_models(py: Python<'_>, models: &Bound<'_, PyAny>, duplicates: &str) -> PyResult<Self> {
        let policy = parse_duplicate_policy(duplicates)?;
        let mut resolved = Vec::new();
        for item in models.try_iter()? {
            let model: PyRef<'_, PyClassModel> = item?.extract()?;
            resolved.push(model.with_class_model(|inner| Ok(ResolvedClass::from_model(inner)))?);
        }
        let resolver = MappingClassResolver::with_duplicate_policy(resolved, policy)
            .map_err(|e| analysis_error_to_py(py, e))?;
        Ok(Self { inner: resolver })
    }

    /// Build a resolver from an iterable of raw ``ClassFile`` instances.
    ///
    /// See ``from_bytes`` for ``duplicates``.
    #[staticmethod]
    #[pyo3(signature = (classfiles, *, duplicates = "error"))]
    fn from_classfiles(
        py: Python<'_>,
        classfiles: &Bound<'_, PyAny>,
        duplicates: &str,
    ) -> PyResult<Self> {
        let policy = parse_duplicate_policy(duplicates)?;
        let mut resolved = Vec::new();
        for item in classfiles.try_iter()? {
            let cf: PyRef<'_, PyClassFile> = item?.extract()?;
            resolved
                .push(ResolvedClass::from_classfile(&cf.inner).map_err(crate::engine_error_to_py)?);
        }
        let resolver = MappingClassResolver::with_duplicate_policy(resolved, policy)
            .map_err(|e| analysis_error_to_py(py, e))?;
        Ok(Self { inner: resolver })
    }

    /// Return a resolver holding the classes of both resolvers.
    ///
    /// Classes in ``self`` come first, so ``duplicates="first"`` prefers them.
    #[pyo3(signature = (other, *, duplicates = "error"))]
    fn merged(
        &self,
        py: Python<'_>,
        other: &PyMappingClassResolver,
        duplicates: &str,
    ) -> PyResult<Self> {
        let policy = parse_duplicate_policy(duplicates)?;
        Ok(Self {
            inner: self
                .inner
                .merged(&other.inner, policy)
                .map_err(|e| analysis_error_to_py(py, e))?,
        })
    }

    fn __len__(&self) -> usize {
        self.inner.len()
    }

    fn __contains__(&self, name: &Bound<'_, PyAny>) -> bool {
        name.extract::<String>()
            .is_ok_and(|name| self.inner.contains(&name))
    }

    /// Resolve a class snapshot dictionary by internal JVM name, or ``None``.
    fn resolve_class(&self, py: Python<'_>, name: &str) -> PyResult<Option<PyObject>> {
        use pytecode_engine::analysis::ClassResolver;
        match self.inner.resolve_class(name) {
            Some(rc) => Ok(Some(resolved_class_to_py(py, &rc)?)),
            None => Ok(None),
        }
    }

    fn __repr__(&self) -> &'static str {
        "MappingClassResolver()"
    }
}

fn parse_duplicate_policy(
    value: &str,
) -> PyResult<pytecode_engine::analysis::DuplicateClassPolicy> {
    use pytecode_engine::analysis::DuplicateClassPolicy;
    match value {
        "error" => Ok(DuplicateClassPolicy::Error),
        "first" => Ok(DuplicateClassPolicy::KeepFirst),
        "last" => Ok(DuplicateClassPolicy::KeepLast),
        other => Err(PyValueError::new_err(format!(
            "duplicates must be 'error', 'first', or 'last', got {other:?}"
        ))),
    }
}

// ---------------------------------------------------------------------------
// Lowering & policy options
// ---------------------------------------------------------------------------

fn cached_archive_policy_type<'py>(
    cell: &'static PyOnceLock<Py<PyAny>>,
    py: Python<'py>,
    name: &str,
) -> PyResult<&'py Py<PyAny>> {
    crate::cached_py_attr(cell, py, "pytecode.archive", name)
}

pub(crate) fn parse_debug_info_policy(
    value: Option<&Bound<'_, PyAny>>,
) -> PyResult<DebugInfoPolicy> {
    static POLICY_ENUM: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
    let Some(obj) = value else {
        return Ok(DebugInfoPolicy::Preserve);
    };
    let s = if let Ok(raw) = obj.extract::<String>() {
        raw
    } else {
        let enum_cls = cached_archive_policy_type(&POLICY_ENUM, obj.py(), "DebugInfoPolicy")?;
        if !obj.is_instance(enum_cls.bind(obj.py()))? {
            return Err(PyTypeError::new_err(
                "debug_info must be a pytecode.archive.DebugInfoPolicy or 'preserve'/'strip'",
            ));
        }
        obj.getattr("value")?.extract::<String>()?
    };
    match s.as_str() {
        "preserve" => Ok(DebugInfoPolicy::Preserve),
        "strip" => Ok(DebugInfoPolicy::Strip),
        _ => Err(PyValueError::new_err(format!(
            "invalid debug info policy: {s:?}, expected \"preserve\" or \"strip\""
        ))),
    }
}

pub(crate) fn parse_frame_computation_mode(
    value: Option<&Bound<'_, PyAny>>,
) -> PyResult<FrameComputationMode> {
    static MODE_ENUM: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
    let Some(obj) = value else {
        return Ok(FrameComputationMode::Preserve);
    };
    let s = if let Ok(raw) = obj.extract::<String>() {
        raw
    } else {
        let enum_cls = cached_archive_policy_type(&MODE_ENUM, obj.py(), "FrameComputationMode")?;
        if !obj.is_instance(enum_cls.bind(obj.py()))? {
            return Err(PyTypeError::new_err(
                "frame_mode must be a pytecode.archive.FrameComputationMode or 'preserve'/'recompute'",
            ));
        }
        obj.getattr("value")?.extract::<String>()?
    };
    match s.as_str() {
        "preserve" => Ok(FrameComputationMode::Preserve),
        "recompute" => Ok(FrameComputationMode::Recompute),
        _ => Err(PyValueError::new_err(format!(
            "invalid frame mode: {s:?}, expected \"preserve\" or \"recompute\""
        ))),
    }
}

/// Emit one ``pytecode.model.FrameComputationWarning`` per method whose
/// recomputed frames are likely to fail JVM verification.
pub(crate) fn emit_frame_warnings(
    py: Python<'_>,
    warnings: &[pytecode_engine::analysis::FrameWarning],
) -> PyResult<()> {
    if warnings.is_empty() {
        return Ok(());
    }
    static CATEGORY: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
    let category =
        crate::cached_py_attr(&CATEGORY, py, "pytecode.model", "FrameComputationWarning")?;
    for warning in warnings {
        let message = std::ffi::CString::new(warning.to_string().replace(' ', " "))
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        PyErr::warn(py, category.bind(py), &message, 1)?;
    }
    Ok(())
}

fn lower_class_model_bytes(
    py: Python<'_>,
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

    write_class(&lower_class_model(
        py, model, debug_info, frame_mode, resolver,
    )?)
    .map_err(crate::engine_error_to_py)
}

fn lower_class_model(
    py: Python<'_>,
    model: &PyClassModel,
    debug_info: DebugInfoPolicy,
    frame_mode: FrameComputationMode,
    resolver: Option<&dyn pytecode_engine::analysis::ClassResolver>,
) -> PyResult<ClassFile> {
    let (classfile, warnings) = with_live_model(&model.state, |model_state| {
        model_state
            .inner
            .as_ref()
            .unwrap()
            .to_classfile_with_report(debug_info, frame_mode, resolver)
            .map_err(crate::engine_error_to_py)
    })?;
    emit_frame_warnings(py, &warnings)?;
    Ok(classfile)
}

/// Lower an iterable of ``ClassModel`` objects into raw ``ClassFile`` structures.
#[pyfunction]
#[pyo3(signature = (models, frame_mode = None, resolver = None, debug_info = None))]
fn rust_lower_classmodels(
    py: Python<'_>,
    models: &Bound<'_, PyAny>,
    frame_mode: Option<&Bound<'_, PyAny>>,
    resolver: Option<&PyMappingClassResolver>,
    debug_info: Option<&Bound<'_, PyAny>>,
) -> PyResult<Vec<PyClassFile>> {
    let policy = parse_debug_info_policy(debug_info)?;
    let frame_mode = parse_frame_computation_mode(frame_mode)?;
    let resolver =
        resolver.map(|value| &value.inner as &dyn pytecode_engine::analysis::ClassResolver);

    let mut out = Vec::new();
    for item in models.try_iter()? {
        let model: PyRef<'_, PyClassModel> = item?.extract()?;
        out.push(PyClassFile {
            inner: Arc::new(lower_class_model(py, &model, policy, frame_mode, resolver)?),
        });
    }
    Ok(out)
}

/// Lower an iterable of ``ClassModel`` objects directly into serialized classfile ``bytes``.
#[pyfunction]
#[pyo3(signature = (models, frame_mode = None, resolver = None, debug_info = None))]
fn rust_lower_classmodels_to_bytes<'py>(
    py: Python<'py>,
    models: &Bound<'py, PyAny>,
    frame_mode: Option<&Bound<'py, PyAny>>,
    resolver: Option<&PyMappingClassResolver>,
    debug_info: Option<&Bound<'py, PyAny>>,
) -> PyResult<Vec<Py<PyBytes>>> {
    let policy = parse_debug_info_policy(debug_info)?;
    let frame_mode = parse_frame_computation_mode(frame_mode)?;
    let resolver =
        resolver.map(|value| &value.inner as &dyn pytecode_engine::analysis::ClassResolver);

    let mut out = Vec::new();
    for item in models.try_iter()? {
        let model: PyRef<'_, PyClassModel> = item?.extract()?;
        let bytes = lower_class_model_bytes(py, &model, policy, frame_mode, resolver)?;
        out.push(PyBytes::new(py, &bytes).unbind());
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// PyClassModel
// ---------------------------------------------------------------------------

/// Mutable symbolic view of a class for structural edits and lowering.
#[pyclass(
    from_py_object,
    name = "ClassModel",
    module = "pytecode._rust",
    unsendable
)]
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
        let inner = {
            let mut borrowed = self
                .state
                .try_borrow_mut()
                .map_err(|_| borrow_err("ClassModel"))?;
            borrowed.inner.take().ok_or_else(dead_model_err)?
        };
        let mut guard = ClassModelRestoreGuard {
            state: self.state.clone(),
            inner: Some(inner),
        };
        f(guard.inner.as_mut().unwrap())
    }

    pub(crate) fn take_inner(&mut self) -> PyResult<ClassModel> {
        let mut borrowed = self
            .state
            .try_borrow_mut()
            .map_err(|_| borrow_err("ClassModel"))?;
        borrowed.inner.take().ok_or_else(dead_model_err)
    }
}

#[pymethods]
impl PyClassModel {
    #[new]
    #[pyo3(signature = (
        name,
        *,
        super_name = Some("java/lang/Object".to_owned()),
        access_flags = 0x0021,
        version = (52, 0),
        interfaces = None,
        fields = None,
        methods = None,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        name: String,
        super_name: Option<String>,
        access_flags: u16,
        version: (u16, u16),
        interfaces: Option<&Bound<'_, PyAny>>,
        fields: Option<&Bound<'_, PyAny>>,
        methods: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let interfaces = match interfaces {
            Some(items) => items
                .try_iter()?
                .map(|entry| entry?.extract::<String>())
                .collect::<PyResult<Vec<_>>>()?,
            None => Vec::new(),
        };
        let fields = match fields {
            Some(items) => items
                .try_iter()?
                .map(|entry| {
                    let field: PyRef<'_, PyFieldModel> = entry?.extract()?;
                    field.clone_inner()
                })
                .collect::<PyResult<Vec<_>>>()?,
            None => Vec::new(),
        };
        let methods = match methods {
            Some(items) => items
                .try_iter()?
                .map(|entry| {
                    let method: PyRef<'_, PyMethodModel> = entry?.extract()?;
                    method.clone_inner()
                })
                .collect::<PyResult<Vec<_>>>()?,
            None => Vec::new(),
        };
        Ok(Self::from_model(ClassModel {
            entry_name: String::new(),
            original_byte_len: 0,
            version,
            access_flags: ClassAccessFlags::from_bits_truncate(access_flags),
            name,
            super_name,
            interfaces,
            fields,
            methods,
            attributes: Vec::new(),
            constant_pool: ConstantPoolBuilder::new(),
            debug_info_state: DebugInfoState::Fresh,
        }))
    }

    // -- constructors -------------------------------------------------------

    /// Parse classfile ``bytes`` or ``bytearray`` into an editable class model.
    #[staticmethod]
    fn from_bytes(data: &Bound<'_, PyAny>) -> PyResult<Self> {
        crate::with_bytes(data, |bytes| {
            let model = ClassModel::from_bytes(bytes).map_err(crate::engine_error_to_py)?;
            Ok(Self::from_model(model))
        })
    }

    /// Lift a raw ``ClassFile`` into an editable class model.
    #[staticmethod]
    fn from_classfile(classfile: &PyClassFile) -> PyResult<Self> {
        let model =
            ClassModel::from_classfile(&classfile.inner).map_err(crate::engine_error_to_py)?;
        Ok(Self::from_model(model))
    }

    // -- serialisation ------------------------------------------------------

    /// Lower the current model to classfile bytes with default options.
    fn to_bytes<'py>(&self, py: Python<'py>) -> PyResult<Py<PyBytes>> {
        let bytes = lower_class_model_bytes(
            py,
            self,
            DebugInfoPolicy::Preserve,
            FrameComputationMode::Preserve,
            None,
        )?;
        Ok(PyBytes::new(py, &bytes).unbind())
    }

    /// Lower the current model to a raw ``ClassFile`` object.
    fn to_classfile(&self, py: Python<'_>) -> PyResult<PyClassFile> {
        Ok(PyClassFile {
            inner: Arc::new(lower_class_model(
                py,
                self,
                DebugInfoPolicy::Preserve,
                FrameComputationMode::Preserve,
                None,
            )?),
        })
    }

    /// Lower the model to bytes with explicit frame and debug-info options.
    #[pyo3(signature = (frame_mode = None, resolver = None, debug_info = None))]
    fn to_bytes_with_options<'py>(
        &self,
        py: Python<'py>,
        frame_mode: Option<&Bound<'py, PyAny>>,
        resolver: Option<&PyMappingClassResolver>,
        debug_info: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Py<PyBytes>> {
        let policy = parse_debug_info_policy(debug_info)?;
        let frame_mode = parse_frame_computation_mode(frame_mode)?;
        let bytes = lower_class_model_bytes(
            py,
            self,
            policy,
            frame_mode,
            resolver.map(|r| &r.inner as &dyn pytecode_engine::analysis::ClassResolver),
        )?;
        Ok(PyBytes::new(py, &bytes).unbind())
    }

    /// Lower the model to a raw ``ClassFile`` object with explicit lowering options.
    #[pyo3(signature = (frame_mode = None, resolver = None, debug_info = None))]
    fn to_classfile_with_options(
        &self,
        py: Python<'_>,
        frame_mode: Option<&Bound<'_, PyAny>>,
        resolver: Option<&PyMappingClassResolver>,
        debug_info: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<PyClassFile> {
        let policy = parse_debug_info_policy(debug_info)?;
        let frame_mode = parse_frame_computation_mode(frame_mode)?;
        Ok(PyClassFile {
            inner: Arc::new(lower_class_model(
                py,
                self,
                policy,
                frame_mode,
                resolver.map(|r| &r.inner as &dyn pytecode_engine::analysis::ClassResolver),
            )?),
        })
    }

    // -- getters ------------------------------------------------------------

    /// Original archive entry name associated with this class model.
    #[getter]
    fn entry_name(&self) -> PyResult<String> {
        with_live_model(&self.state, |model_state| {
            Ok(model_state.inner.as_ref().unwrap().entry_name.clone())
        })
    }

    /// Size in bytes of the classfile this model was originally parsed from.
    #[getter]
    fn original_byte_len(&self) -> PyResult<usize> {
        with_live_model(&self.state, |model_state| {
            Ok(model_state.inner.as_ref().unwrap().original_byte_len)
        })
    }

    /// Classfile version as ``(major, minor)``.
    #[getter]
    fn version(&self) -> PyResult<(u16, u16)> {
        with_live_model(&self.state, |model_state| {
            Ok(model_state.inner.as_ref().unwrap().version)
        })
    }

    /// Raw JVM access-flag bitset for the class declaration.
    #[getter]
    fn access_flags(&self) -> PyResult<u16> {
        with_live_model(&self.state, |model_state| {
            Ok(model_state.inner.as_ref().unwrap().access_flags.bits())
        })
    }

    /// Internal JVM name of the class, such as ``pkg/Example``.
    #[getter]
    fn name(&self) -> PyResult<String> {
        with_live_model(&self.state, |model_state| {
            Ok(model_state.inner.as_ref().unwrap().name.clone())
        })
    }

    /// Internal JVM name of the direct superclass, or ``None`` for ``java/lang/Object``.
    #[getter]
    fn super_name(&self) -> PyResult<Option<String>> {
        with_live_model(&self.state, |model_state| {
            Ok(model_state.inner.as_ref().unwrap().super_name.clone())
        })
    }

    /// Live view of declared interface names in internal JVM format.
    #[getter]
    fn interfaces(&self) -> PyResult<PyStringListView> {
        with_live_model(&self.state, |model_state| {
            Ok(PyStringListView {
                state: self.state.clone(),
                generation: model_state.interfaces_generation,
            })
        })
    }

    /// Live view of the class's field models.
    #[getter]
    fn fields(&self) -> PyResult<PyFieldListView> {
        with_live_model(&self.state, |model_state| {
            Ok(PyFieldListView {
                state: self.state.clone(),
                generation: model_state.fields_generation,
            })
        })
    }

    /// Live view of the class's method models.
    #[getter]
    fn methods(&self) -> PyResult<PyMethodListView> {
        with_live_model(&self.state, |model_state| {
            Ok(PyMethodListView {
                state: self.state.clone(),
                generation: model_state.methods_generation,
            })
        })
    }

    /// Live view of raw class-level attributes.
    #[getter]
    fn attributes(&self) -> PyAttributeListView {
        PyAttributeListView {
            owner: AttributeOwner::Class(self.state.clone()),
        }
    }

    /// Mutable constant-pool builder used when lowering edited models.
    #[getter]
    fn constant_pool(&self) -> PyConstantPoolBuilder {
        PyConstantPoolBuilder::from_shared(self.state.clone())
    }

    /// Debug-info freshness marker, typically ``fresh`` or ``stale``.
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

    /// Symbolic entries of the class's ``BootstrapMethods`` attribute.
    ///
    /// Assigning a list re-encodes the attribute in place through the
    /// constant pool (existing entries are reused, so unchanged entries
    /// round-trip byte-for-byte); an empty list removes it. Entries are
    /// addressed by position from ``invokedynamic`` instructions and dynamic
    /// constants, so keep existing entries at their index. Rewrite the
    /// arguments here when renaming classes: method handles, method types,
    /// and class literals among them name classes the JVM resolves when the
    /// call site links.
    #[getter]
    fn bootstrap_methods(&self) -> PyResult<Vec<PyBootstrapMethod>> {
        let methods = with_live_model(&self.state, |model_state| {
            model_state
                .inner
                .as_ref()
                .unwrap()
                .bootstrap_methods()
                .map_err(crate::engine_error_to_py)
        })?;
        Ok(methods
            .into_iter()
            .map(PyBootstrapMethod::from_model)
            .collect())
    }

    #[setter]
    fn set_bootstrap_methods(&mut self, value: Vec<PyBootstrapMethod>) -> PyResult<()> {
        let methods = value
            .iter()
            .map(PyBootstrapMethod::to_model)
            .collect::<Vec<_>>();
        with_live_model_mut(&self.state, |model_state| {
            model_state
                .inner
                .as_mut()
                .unwrap()
                .set_bootstrap_methods(&methods)
                .map_err(crate::engine_error_to_py)
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
                ClassAccessFlags::from_bits_truncate(value);
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

    fn __repr__(&self) -> PyResult<String> {
        with_live_model(&self.state, |model_state| {
            let inner = model_state.inner.as_ref().unwrap();
            Ok(format!(
                "ClassModel(name={:?}, version={:?}, fields={}, methods={})",
                inner.name,
                inner.version,
                inner.fields.len(),
                inner.methods.len()
            ))
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
    module.add_class::<PyBootstrapArgument>()?;
    module.add_class::<PyBootstrapMethod>()?;
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
