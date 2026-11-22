use pyo3::exceptions::PyDeprecationWarning;
use pyo3::prelude::*;
use pytecode_engine::raw;

macro_rules! wrap_pyclass {
    ($py:expr, $value:expr) => {
        Py::new($py, $value).map(|obj| obj.into_bound($py).into_any().unbind())
    };
}

// ===========================================================================
// Verification Type Info variants
// ===========================================================================

/// Stack-map placeholder for an unusable or missing slot.
#[pyclass(from_py_object, module = "pytecode._rust", name = "TopVariableInfo")]
#[derive(Clone)]
pub(crate) struct PyTopVariableInfo;

#[pymethods]
impl PyTopVariableInfo {
    /// Stack-map verification tag (``VerificationType.TOP``).
    #[getter]
    fn tag(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::wrap_verification_type(py, pytecode_engine::constants::VerificationType::Top)
    }

    fn __repr__(&self) -> &'static str {
        "TopVariableInfo()"
    }
}

/// Stack-map entry for an ``int`` value.
#[pyclass(
    from_py_object,
    module = "pytecode._rust",
    name = "IntegerVariableInfo"
)]
#[derive(Clone)]
pub(crate) struct PyIntegerVariableInfo;

#[pymethods]
impl PyIntegerVariableInfo {
    /// Stack-map verification tag (``VerificationType.INTEGER``).
    #[getter]
    fn tag(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::wrap_verification_type(py, pytecode_engine::constants::VerificationType::Integer)
    }

    fn __repr__(&self) -> &'static str {
        "IntegerVariableInfo()"
    }
}

/// Stack-map entry for a ``float`` value.
#[pyclass(from_py_object, module = "pytecode._rust", name = "FloatVariableInfo")]
#[derive(Clone)]
pub(crate) struct PyFloatVariableInfo;

#[pymethods]
impl PyFloatVariableInfo {
    /// Stack-map verification tag (``VerificationType.FLOAT``).
    #[getter]
    fn tag(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::wrap_verification_type(py, pytecode_engine::constants::VerificationType::Float)
    }

    fn __repr__(&self) -> &'static str {
        "FloatVariableInfo()"
    }
}

/// Stack-map entry for a ``double`` value.
#[pyclass(from_py_object, module = "pytecode._rust", name = "DoubleVariableInfo")]
#[derive(Clone)]
pub(crate) struct PyDoubleVariableInfo;

#[pymethods]
impl PyDoubleVariableInfo {
    /// Stack-map verification tag (``VerificationType.DOUBLE``).
    #[getter]
    fn tag(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::wrap_verification_type(py, pytecode_engine::constants::VerificationType::Double)
    }

    fn __repr__(&self) -> &'static str {
        "DoubleVariableInfo()"
    }
}

/// Stack-map entry for a ``long`` value.
#[pyclass(from_py_object, module = "pytecode._rust", name = "LongVariableInfo")]
#[derive(Clone)]
pub(crate) struct PyLongVariableInfo;

#[pymethods]
impl PyLongVariableInfo {
    /// Stack-map verification tag (``VerificationType.LONG``).
    #[getter]
    fn tag(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::wrap_verification_type(py, pytecode_engine::constants::VerificationType::Long)
    }

    fn __repr__(&self) -> &'static str {
        "LongVariableInfo()"
    }
}

/// Stack-map entry for the ``null`` reference.
#[pyclass(from_py_object, module = "pytecode._rust", name = "NullVariableInfo")]
#[derive(Clone)]
pub(crate) struct PyNullVariableInfo;

#[pymethods]
impl PyNullVariableInfo {
    /// Stack-map verification tag (``VerificationType.NULL``).
    #[getter]
    fn tag(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::wrap_verification_type(py, pytecode_engine::constants::VerificationType::Null)
    }

    fn __repr__(&self) -> &'static str {
        "NullVariableInfo()"
    }
}

/// Stack-map entry for ``this`` before superclass construction completes.
#[pyclass(
    from_py_object,
    module = "pytecode._rust",
    name = "UninitializedThisVariableInfo"
)]
#[derive(Clone)]
pub(crate) struct PyUninitializedThisVariableInfo;

#[pymethods]
impl PyUninitializedThisVariableInfo {
    /// Stack-map verification tag (``VerificationType.UNINITIALIZED_THIS``).
    #[getter]
    fn tag(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::wrap_verification_type(
            py,
            pytecode_engine::constants::VerificationType::UninitializedThis,
        )
    }

    fn __repr__(&self) -> &'static str {
        "UninitializedThisVariableInfo()"
    }
}

/// Stack-map entry for an initialized reference type.
#[pyclass(from_py_object, module = "pytecode._rust", name = "ObjectVariableInfo")]
#[derive(Clone)]
pub(crate) struct PyObjectVariableInfo {
    cpool_index: u16,
}

#[pymethods]
impl PyObjectVariableInfo {
    /// Stack-map verification tag (``VerificationType.OBJECT``).
    #[getter]
    fn tag(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::wrap_verification_type(py, pytecode_engine::constants::VerificationType::Object)
    }

    /// Constant-pool index of the ``ClassInfo`` referenced by this entry.
    #[getter]
    fn cpool_index(&self) -> u16 {
        self.cpool_index
    }

    fn __repr__(&self) -> String {
        format!("ObjectVariableInfo(cpool_index={})", self.cpool_index)
    }
}

/// Stack-map entry for a ``new`` object before constructor invocation.
#[pyclass(
    from_py_object,
    module = "pytecode._rust",
    name = "UninitializedVariableInfo"
)]
#[derive(Clone)]
pub(crate) struct PyUninitializedVariableInfo {
    offset: u16,
}

#[pymethods]
impl PyUninitializedVariableInfo {
    /// Stack-map verification tag (``VerificationType.UNINITIALIZED``).
    #[getter]
    fn tag(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::wrap_verification_type(
            py,
            pytecode_engine::constants::VerificationType::Uninitialized,
        )
    }

    /// Bytecode offset of the ``new`` instruction creating the instance.
    #[getter]
    fn offset(&self) -> u16 {
        self.offset
    }

    fn __repr__(&self) -> String {
        format!("UninitializedVariableInfo(offset={})", self.offset)
    }
}

fn wrap_verification_type_info(
    py: Python<'_>,
    info: &raw::VerificationTypeInfo,
) -> PyResult<Py<PyAny>> {
    match info {
        raw::VerificationTypeInfo::Top => wrap_pyclass!(py, PyTopVariableInfo),
        raw::VerificationTypeInfo::Integer => wrap_pyclass!(py, PyIntegerVariableInfo),
        raw::VerificationTypeInfo::Float => wrap_pyclass!(py, PyFloatVariableInfo),
        raw::VerificationTypeInfo::Double => wrap_pyclass!(py, PyDoubleVariableInfo),
        raw::VerificationTypeInfo::Long => wrap_pyclass!(py, PyLongVariableInfo),
        raw::VerificationTypeInfo::Null => wrap_pyclass!(py, PyNullVariableInfo),
        raw::VerificationTypeInfo::UninitializedThis => {
            wrap_pyclass!(py, PyUninitializedThisVariableInfo)
        }
        raw::VerificationTypeInfo::Object { cpool_index } => {
            wrap_pyclass!(
                py,
                PyObjectVariableInfo {
                    cpool_index: (*cpool_index).into(),
                }
            )
        }
        raw::VerificationTypeInfo::Uninitialized { offset } => {
            wrap_pyclass!(py, PyUninitializedVariableInfo { offset: *offset })
        }
    }
}

// ===========================================================================
// Stack Map Frame Info variants
// ===========================================================================

/// Frame that keeps prior locals and records an empty operand stack.
#[pyclass(from_py_object, module = "pytecode._rust", name = "SameFrameInfo")]
#[derive(Clone)]
pub(crate) struct PySameFrameInfo {
    frame_type: u8,
}

#[pymethods]
impl PySameFrameInfo {
    /// Raw JVMS frame tag in ``0..=63``.
    #[getter]
    fn frame_type(&self) -> u8 {
        self.frame_type
    }

    fn __repr__(&self) -> String {
        format!("SameFrameInfo(frame_type={})", self.frame_type)
    }
}

/// Frame that keeps prior locals and records one operand-stack entry.
#[pyclass(
    from_py_object,
    module = "pytecode._rust",
    name = "SameLocals1StackItemFrameInfo"
)]
#[derive(Clone)]
pub(crate) struct PySameLocals1StackItemFrameInfo {
    frame_type: u8,
    stack: raw::VerificationTypeInfo,
}

#[pymethods]
impl PySameLocals1StackItemFrameInfo {
    /// Raw JVMS frame tag in ``64..=127``.
    #[getter]
    fn frame_type(&self) -> u8 {
        self.frame_type
    }

    /// The single verification type on the operand stack.
    #[getter]
    fn stack(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        wrap_verification_type_info(py, &self.stack)
    }

    fn __repr__(&self) -> String {
        format!(
            "SameLocals1StackItemFrameInfo(frame_type={})",
            self.frame_type
        )
    }
}

/// Same-locals-one-stack-item frame that stores an explicit ``offset_delta``.
#[pyclass(
    from_py_object,
    module = "pytecode._rust",
    name = "SameLocals1StackItemFrameExtendedInfo"
)]
#[derive(Clone)]
pub(crate) struct PySameLocals1StackItemFrameExtendedInfo {
    frame_type: u8,
    offset_delta: u16,
    stack: raw::VerificationTypeInfo,
}

#[pymethods]
impl PySameLocals1StackItemFrameExtendedInfo {
    /// Raw JVMS frame tag (``247``).
    #[getter]
    fn frame_type(&self) -> u8 {
        self.frame_type
    }

    /// Bytecode offset delta encoded by this frame.
    #[getter]
    fn offset_delta(&self) -> u16 {
        self.offset_delta
    }

    /// The single verification type on the operand stack.
    #[getter]
    fn stack(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        wrap_verification_type_info(py, &self.stack)
    }

    fn __repr__(&self) -> String {
        format!(
            "SameLocals1StackItemFrameExtendedInfo(frame_type={}, offset_delta={})",
            self.frame_type, self.offset_delta
        )
    }
}

/// Frame that drops one to three trailing locals from the previous state.
#[pyclass(from_py_object, module = "pytecode._rust", name = "ChopFrameInfo")]
#[derive(Clone)]
pub(crate) struct PyChopFrameInfo {
    frame_type: u8,
    offset_delta: u16,
}

#[pymethods]
impl PyChopFrameInfo {
    /// Raw JVMS frame tag in ``248..=250``.
    #[getter]
    fn frame_type(&self) -> u8 {
        self.frame_type
    }

    /// Bytecode offset delta encoded by this frame.
    #[getter]
    fn offset_delta(&self) -> u16 {
        self.offset_delta
    }

    fn __repr__(&self) -> String {
        format!(
            "ChopFrameInfo(frame_type={}, offset_delta={})",
            self.frame_type, self.offset_delta
        )
    }
}

/// Same-frame variant with an explicit ``offset_delta``.
#[pyclass(
    from_py_object,
    module = "pytecode._rust",
    name = "SameFrameExtendedInfo"
)]
#[derive(Clone)]
pub(crate) struct PySameFrameExtendedInfo {
    frame_type: u8,
    offset_delta: u16,
}

#[pymethods]
impl PySameFrameExtendedInfo {
    /// Raw JVMS frame tag (``251``).
    #[getter]
    fn frame_type(&self) -> u8 {
        self.frame_type
    }

    /// Bytecode offset delta encoded by this frame.
    #[getter]
    fn offset_delta(&self) -> u16 {
        self.offset_delta
    }

    fn __repr__(&self) -> String {
        format!(
            "SameFrameExtendedInfo(frame_type={}, offset_delta={})",
            self.frame_type, self.offset_delta
        )
    }
}

/// Frame that appends one to three locals to the previous state.
#[pyclass(from_py_object, module = "pytecode._rust", name = "AppendFrameInfo")]
#[derive(Clone)]
pub(crate) struct PyAppendFrameInfo {
    frame_type: u8,
    offset_delta: u16,
    locals: Vec<raw::VerificationTypeInfo>,
}

#[pymethods]
impl PyAppendFrameInfo {
    /// Raw JVMS frame tag in ``252..=254``.
    #[getter]
    fn frame_type(&self) -> u8 {
        self.frame_type
    }

    /// Bytecode offset delta encoded by this frame.
    #[getter]
    fn offset_delta(&self) -> u16 {
        self.offset_delta
    }

    /// Verification types appended to the local-variable table.
    #[getter]
    fn locals(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.locals
            .iter()
            .map(|v| wrap_verification_type_info(py, v))
            .collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "AppendFrameInfo(frame_type={}, offset_delta={}, locals={})",
            self.frame_type,
            self.offset_delta,
            self.locals.len()
        )
    }
}

/// Frame that spells out the complete local and operand-stack state.
#[pyclass(from_py_object, module = "pytecode._rust", name = "FullFrameInfo")]
#[derive(Clone)]
pub(crate) struct PyFullFrameInfo {
    frame_type: u8,
    offset_delta: u16,
    locals: Vec<raw::VerificationTypeInfo>,
    stack: Vec<raw::VerificationTypeInfo>,
}

#[pymethods]
impl PyFullFrameInfo {
    /// Raw JVMS frame tag (``255``).
    #[getter]
    fn frame_type(&self) -> u8 {
        self.frame_type
    }

    /// Bytecode offset delta encoded by this frame.
    #[getter]
    fn offset_delta(&self) -> u16 {
        self.offset_delta
    }

    /// Number of local-variable verification entries.
    #[getter]
    fn number_of_locals(&self) -> usize {
        self.locals.len()
    }

    /// Local-variable verification entries at this frame offset.
    #[getter]
    fn locals(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.locals
            .iter()
            .map(|v| wrap_verification_type_info(py, v))
            .collect()
    }

    /// Number of operand-stack verification entries.
    #[getter]
    fn number_of_stack_items(&self) -> usize {
        self.stack.len()
    }

    /// Operand-stack verification entries at this frame offset.
    #[getter]
    fn stack(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.stack
            .iter()
            .map(|v| wrap_verification_type_info(py, v))
            .collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "FullFrameInfo(frame_type={}, offset_delta={}, locals={}, stack={})",
            self.frame_type,
            self.offset_delta,
            self.locals.len(),
            self.stack.len()
        )
    }
}

fn wrap_stack_map_frame_info(
    py: Python<'_>,
    frame: &raw::StackMapFrameInfo,
) -> PyResult<Py<PyAny>> {
    match frame {
        raw::StackMapFrameInfo::Same { frame_type } => {
            wrap_pyclass!(
                py,
                PySameFrameInfo {
                    frame_type: *frame_type,
                }
            )
        }
        raw::StackMapFrameInfo::SameLocals1StackItem { frame_type, stack } => {
            wrap_pyclass!(
                py,
                PySameLocals1StackItemFrameInfo {
                    frame_type: *frame_type,
                    stack: stack.clone(),
                }
            )
        }
        raw::StackMapFrameInfo::SameLocals1StackItemExtended {
            frame_type,
            offset_delta,
            stack,
        } => wrap_pyclass!(
            py,
            PySameLocals1StackItemFrameExtendedInfo {
                frame_type: *frame_type,
                offset_delta: *offset_delta,
                stack: stack.clone(),
            }
        ),
        raw::StackMapFrameInfo::Chop {
            frame_type,
            offset_delta,
        } => wrap_pyclass!(
            py,
            PyChopFrameInfo {
                frame_type: *frame_type,
                offset_delta: *offset_delta,
            }
        ),
        raw::StackMapFrameInfo::SameExtended {
            frame_type,
            offset_delta,
        } => wrap_pyclass!(
            py,
            PySameFrameExtendedInfo {
                frame_type: *frame_type,
                offset_delta: *offset_delta,
            }
        ),
        raw::StackMapFrameInfo::Append {
            frame_type,
            offset_delta,
            locals,
        } => wrap_pyclass!(
            py,
            PyAppendFrameInfo {
                frame_type: *frame_type,
                offset_delta: *offset_delta,
                locals: locals.clone(),
            }
        ),
        raw::StackMapFrameInfo::Full {
            frame_type,
            offset_delta,
            locals,
            stack,
        } => wrap_pyclass!(
            py,
            PyFullFrameInfo {
                frame_type: *frame_type,
                offset_delta: *offset_delta,
                locals: locals.clone(),
                stack: stack.clone(),
            }
        ),
    }
}

// ===========================================================================
// Simple entry types
// ===========================================================================

/// Mapping from bytecode offset to source line number.
#[pyclass(from_py_object, module = "pytecode._rust", name = "LineNumberInfo")]
#[derive(Clone)]
pub(crate) struct PyLineNumberInfo {
    inner: raw::LineNumberInfo,
}

#[pymethods]
impl PyLineNumberInfo {
    /// Starting bytecode offset covered by this entry.
    #[getter]
    fn start_pc(&self) -> u16 {
        self.inner.start_pc
    }

    /// 1-based source line number.
    #[getter]
    fn line_number(&self) -> u16 {
        self.inner.line_number
    }

    fn __repr__(&self) -> String {
        format!(
            "LineNumberInfo(start_pc={}, line_number={})",
            self.inner.start_pc, self.inner.line_number
        )
    }
}

/// Named local-variable slot with descriptor-based type information.
#[pyclass(from_py_object, module = "pytecode._rust", name = "LocalVariableInfo")]
#[derive(Clone)]
pub(crate) struct PyLocalVariableInfo {
    inner: raw::LocalVariableInfo,
}

#[pymethods]
impl PyLocalVariableInfo {
    /// Starting bytecode offset where the variable is in scope.
    #[getter]
    fn start_pc(&self) -> u16 {
        self.inner.start_pc
    }

    /// Length in bytes of the live scope range.
    #[getter]
    fn length(&self) -> u16 {
        self.inner.length
    }

    /// Constant-pool UTF-8 index of the variable name.
    #[getter]
    fn name_index(&self) -> u16 {
        self.inner.name_index.into()
    }

    /// Constant-pool UTF-8 index of the field descriptor.
    #[getter]
    fn descriptor_index(&self) -> u16 {
        self.inner.descriptor_index.into()
    }

    /// Local-variable slot index.
    #[getter]
    fn index(&self) -> u16 {
        self.inner.index
    }

    fn __repr__(&self) -> String {
        format!(
            "LocalVariableInfo(start_pc={}, length={}, index={})",
            self.inner.start_pc, self.inner.length, self.inner.index
        )
    }
}

/// Named local-variable slot with generic signature information.
#[pyclass(
    from_py_object,
    module = "pytecode._rust",
    name = "LocalVariableTypeInfo"
)]
#[derive(Clone)]
pub(crate) struct PyLocalVariableTypeInfo {
    inner: raw::LocalVariableTypeInfo,
}

#[pymethods]
impl PyLocalVariableTypeInfo {
    /// Starting bytecode offset where the variable is in scope.
    #[getter]
    fn start_pc(&self) -> u16 {
        self.inner.start_pc
    }

    /// Length in bytes of the live scope range.
    #[getter]
    fn length(&self) -> u16 {
        self.inner.length
    }

    /// Constant-pool UTF-8 index of the variable name.
    #[getter]
    fn name_index(&self) -> u16 {
        self.inner.name_index.into()
    }

    /// Constant-pool UTF-8 index of the generic signature.
    #[getter]
    fn signature_index(&self) -> u16 {
        self.inner.signature_index.into()
    }

    /// Local-variable slot index.
    #[getter]
    fn index(&self) -> u16 {
        self.inner.index
    }

    fn __repr__(&self) -> String {
        format!(
            "LocalVariableTypeInfo(start_pc={}, length={}, index={})",
            self.inner.start_pc, self.inner.length, self.inner.index
        )
    }
}

/// One inner-class relationship recorded in ``InnerClasses``.
#[pyclass(from_py_object, module = "pytecode._rust", name = "InnerClassInfo")]
#[derive(Clone)]
pub(crate) struct PyInnerClassInfo {
    inner: raw::InnerClassInfo,
}

#[pymethods]
impl PyInnerClassInfo {
    /// Constant-pool ``ClassInfo`` index of the inner class.
    #[getter]
    fn inner_class_info_index(&self) -> u16 {
        self.inner.inner_class_info_index.into()
    }

    /// Constant-pool ``ClassInfo`` index of the enclosing class, or ``0``.
    #[getter]
    fn outer_class_info_index(&self) -> u16 {
        self.inner.outer_class_info_index.into()
    }

    /// Constant-pool UTF-8 index of the simple name, or ``0`` for anonymous classes.
    #[getter]
    fn inner_name_index(&self) -> u16 {
        self.inner.inner_name_index.into()
    }

    /// Access flags declared for the nested class.
    #[getter]
    fn inner_class_access_flags(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::wrap_nested_class_access_flags(py, self.inner.inner_class_access_flags)
    }

    fn __repr__(&self) -> String {
        format!(
            "InnerClassInfo(inner_class_info_index={}, outer_class_info_index={}, inner_name_index={})",
            u16::from(self.inner.inner_class_info_index),
            u16::from(self.inner.outer_class_info_index),
            u16::from(self.inner.inner_name_index),
        )
    }
}

// ===========================================================================
// Annotation sub-types
// ===========================================================================

/// Annotation element whose value comes from the constant pool.
#[pyclass(from_py_object, module = "pytecode._rust", name = "ConstValueInfo")]
#[derive(Clone)]
pub(crate) struct PyConstValueInfo {
    const_value_index: u16,
}

#[pymethods]
impl PyConstValueInfo {
    /// Constant-pool index of the primitive or string value.
    #[getter]
    fn const_value_index(&self) -> u16 {
        self.const_value_index
    }

    fn __repr__(&self) -> String {
        format!(
            "ConstValueInfo(const_value_index={})",
            self.const_value_index
        )
    }
}

/// Annotation element naming a specific enum constant.
#[pyclass(
    from_py_object,
    module = "pytecode._rust",
    name = "EnumConstantValueInfo"
)]
#[derive(Clone)]
pub(crate) struct PyEnumConstantValueInfo {
    type_name_index: u16,
    const_name_index: u16,
}

#[pymethods]
impl PyEnumConstantValueInfo {
    /// Constant-pool UTF-8 index of the enum type descriptor.
    #[getter]
    fn type_name_index(&self) -> u16 {
        self.type_name_index
    }

    /// Constant-pool UTF-8 index of the enum constant name.
    #[getter]
    fn const_name_index(&self) -> u16 {
        self.const_name_index
    }

    fn __repr__(&self) -> String {
        format!(
            "EnumConstantValueInfo(type_name_index={}, const_name_index={})",
            self.type_name_index, self.const_name_index
        )
    }
}

/// Annotation element storing a class literal.
#[pyclass(from_py_object, module = "pytecode._rust", name = "ClassInfoValueInfo")]
#[derive(Clone)]
pub(crate) struct PyClassInfoValueInfo {
    class_info_index: u16,
}

#[pymethods]
impl PyClassInfoValueInfo {
    /// Constant-pool UTF-8 index of the return/class descriptor.
    #[getter]
    fn class_info_index(&self) -> u16 {
        self.class_info_index
    }

    fn __repr__(&self) -> String {
        format!(
            "ClassInfoValueInfo(class_info_index={})",
            self.class_info_index
        )
    }
}

/// Annotation element storing an ordered list of values.
#[pyclass(from_py_object, module = "pytecode._rust", name = "ArrayValueInfo")]
#[derive(Clone)]
pub(crate) struct PyArrayValueInfo {
    values: Vec<raw::ElementValueInfo>,
}

#[pymethods]
impl PyArrayValueInfo {
    /// Number of element values in the array.
    #[getter]
    fn num_values(&self) -> usize {
        self.values.len()
    }

    /// Ordered list of element-value entries.
    #[getter]
    fn values(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.values
            .iter()
            .map(|v| wrap_element_value_info(py, v))
            .collect()
    }

    fn __repr__(&self) -> String {
        format!("ArrayValueInfo(num_values={})", self.values.len())
    }
}

/// Tagged union for one annotation element value.
#[pyclass(from_py_object, module = "pytecode._rust", name = "ElementValueInfo")]
#[derive(Clone)]
pub(crate) struct PyElementValueInfo {
    inner: raw::ElementValueInfo,
}

#[pymethods]
impl PyElementValueInfo {
    /// Single-character JVMS element-value tag.
    #[getter]
    fn tag(&self) -> String {
        match &self.inner {
            raw::ElementValueInfo::Const { tag, .. } => (*tag as u8 as char).to_string(),
            raw::ElementValueInfo::Enum { .. } => "e".to_string(),
            raw::ElementValueInfo::Class { .. } => "c".to_string(),
            raw::ElementValueInfo::Annotation(_) => "@".to_string(),
            raw::ElementValueInfo::Array { .. } => "[".to_string(),
        }
    }

    /// Wrapped payload object corresponding to ``tag``.
    #[getter]
    fn value(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        match &self.inner {
            raw::ElementValueInfo::Const {
                const_value_index, ..
            } => wrap_pyclass!(
                py,
                PyConstValueInfo {
                    const_value_index: (*const_value_index).into(),
                }
            ),
            raw::ElementValueInfo::Enum {
                type_name_index,
                const_name_index,
            } => wrap_pyclass!(
                py,
                PyEnumConstantValueInfo {
                    type_name_index: (*type_name_index).into(),
                    const_name_index: (*const_name_index).into(),
                }
            ),
            raw::ElementValueInfo::Class { class_info_index } => {
                wrap_pyclass!(
                    py,
                    PyClassInfoValueInfo {
                        class_info_index: (*class_info_index).into(),
                    }
                )
            }
            raw::ElementValueInfo::Annotation(annotation) => {
                wrap_pyclass!(
                    py,
                    PyAnnotationInfo {
                        inner: annotation.clone(),
                    }
                )
            }
            raw::ElementValueInfo::Array { values } => {
                wrap_pyclass!(
                    py,
                    PyArrayValueInfo {
                        values: values.clone(),
                    }
                )
            }
        }
    }

    fn __repr__(&self) -> String {
        format!("ElementValueInfo(tag={:?})", self.tag())
    }
}

fn wrap_element_value_info(py: Python<'_>, value: &raw::ElementValueInfo) -> PyResult<Py<PyAny>> {
    wrap_pyclass!(
        py,
        PyElementValueInfo {
            inner: value.clone(),
        }
    )
}

/// Named value supplied to an annotation element.
#[pyclass(
    from_py_object,
    module = "pytecode._rust",
    name = "ElementValuePairInfo"
)]
#[derive(Clone)]
pub(crate) struct PyElementValuePairInfo {
    inner: raw::ElementValuePairInfo,
}

#[pymethods]
impl PyElementValuePairInfo {
    /// Constant-pool UTF-8 index of the element name.
    #[getter]
    fn element_name_index(&self) -> u16 {
        self.inner.element_name_index.into()
    }

    /// Value assigned to the element.
    #[getter]
    fn element_value(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        wrap_element_value_info(py, &self.inner.element_value)
    }

    fn __repr__(&self) -> String {
        format!(
            "ElementValuePairInfo(element_name_index={})",
            u16::from(self.inner.element_name_index)
        )
    }
}

/// Materialized annotation instance and its element assignments.
#[pyclass(from_py_object, module = "pytecode._rust", name = "AnnotationInfo")]
#[derive(Clone)]
pub(crate) struct PyAnnotationInfo {
    inner: raw::AnnotationInfo,
}

#[pymethods]
impl PyAnnotationInfo {
    /// Constant-pool UTF-8 index of the annotation type descriptor.
    #[getter]
    fn type_index(&self) -> u16 {
        self.inner.type_index.into()
    }

    /// Number of element-value pairs in this annotation.
    #[getter]
    fn num_element_value_pairs(&self) -> usize {
        self.inner.element_value_pairs.len()
    }

    /// List of element-value pairs on this annotation.
    #[getter]
    fn element_value_pairs(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .element_value_pairs
            .iter()
            .map(|pair| {
                wrap_pyclass!(
                    py,
                    PyElementValuePairInfo {
                        inner: pair.clone(),
                    }
                )
            })
            .collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "AnnotationInfo(type_index={}, pairs={})",
            u16::from(self.inner.type_index),
            self.inner.element_value_pairs.len()
        )
    }
}

/// Annotation list for one method or constructor parameter.
#[pyclass(
    from_py_object,
    module = "pytecode._rust",
    name = "ParameterAnnotationInfo"
)]
#[derive(Clone)]
pub(crate) struct PyParameterAnnotationInfo {
    inner: raw::ParameterAnnotationInfo,
}

#[pymethods]
impl PyParameterAnnotationInfo {
    /// Number of annotations on this parameter.
    #[getter]
    fn num_annotations(&self) -> usize {
        self.inner.annotations.len()
    }

    /// List of annotations applied to this parameter.
    #[getter]
    fn annotations(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .annotations
            .iter()
            .map(|a| wrap_pyclass!(py, PyAnnotationInfo { inner: a.clone() }))
            .collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "ParameterAnnotationInfo(num_annotations={})",
            self.inner.annotations.len()
        )
    }
}

// ===========================================================================
// Type annotation sub-types
// ===========================================================================

/// Type annotation attached to a type parameter declaration.
#[pyclass(
    from_py_object,
    module = "pytecode._rust",
    name = "TypeParameterTargetInfo"
)]
#[derive(Clone)]
pub(crate) struct PyTypeParameterTargetInfo {
    type_parameter_index: u8,
}

#[pymethods]
impl PyTypeParameterTargetInfo {
    /// 0-based index of the annotated type parameter.
    #[getter]
    fn type_parameter_index(&self) -> u8 {
        self.type_parameter_index
    }

    fn __repr__(&self) -> String {
        format!(
            "TypeParameterTargetInfo(type_parameter_index={})",
            self.type_parameter_index
        )
    }
}

/// Type annotation attached to a superclass or interface reference.
#[pyclass(
    from_py_object,
    module = "pytecode._rust",
    name = "SupertypeTargetInfo"
)]
#[derive(Clone)]
pub(crate) struct PySupertypeTargetInfo {
    supertype_index: u16,
}

#[pymethods]
impl PySupertypeTargetInfo {
    /// Index in ``interfaces``, or ``65535`` for the direct superclass.
    #[getter]
    fn supertype_index(&self) -> u16 {
        self.supertype_index
    }

    fn __repr__(&self) -> String {
        format!(
            "SupertypeTargetInfo(supertype_index={})",
            self.supertype_index
        )
    }
}

/// Type annotation attached to one bound of a type parameter.
#[pyclass(
    from_py_object,
    module = "pytecode._rust",
    name = "TypeParameterBoundTargetInfo"
)]
#[derive(Clone)]
pub(crate) struct PyTypeParameterBoundTargetInfo {
    type_parameter_index: u8,
    bound_index: u8,
}

#[pymethods]
impl PyTypeParameterBoundTargetInfo {
    /// 0-based index of the owning type parameter.
    #[getter]
    fn type_parameter_index(&self) -> u8 {
        self.type_parameter_index
    }

    /// 0-based index of the annotated bound.
    #[getter]
    fn bound_index(&self) -> u8 {
        self.bound_index
    }

    fn __repr__(&self) -> String {
        format!(
            "TypeParameterBoundTargetInfo(type_parameter_index={}, bound_index={})",
            self.type_parameter_index, self.bound_index
        )
    }
}

/// Target marker for sites that need no extra coordinates.
#[pyclass(from_py_object, module = "pytecode._rust", name = "EmptyTargetInfo")]
#[derive(Clone)]
pub(crate) struct PyEmptyTargetInfo;

#[pymethods]
impl PyEmptyTargetInfo {
    fn __repr__(&self) -> &'static str {
        "EmptyTargetInfo()"
    }
}

/// Type annotation attached to a formal parameter declaration.
#[pyclass(
    from_py_object,
    module = "pytecode._rust",
    name = "FormalParameterTargetInfo"
)]
#[derive(Clone)]
pub(crate) struct PyFormalParameterTargetInfo {
    formal_parameter_index: u8,
}

#[pymethods]
impl PyFormalParameterTargetInfo {
    /// 0-based formal parameter index.
    #[getter]
    fn formal_parameter_index(&self) -> u8 {
        self.formal_parameter_index
    }

    fn __repr__(&self) -> String {
        format!(
            "FormalParameterTargetInfo(formal_parameter_index={})",
            self.formal_parameter_index
        )
    }
}

/// Type annotation attached to one declared thrown type.
#[pyclass(from_py_object, module = "pytecode._rust", name = "ThrowsTargetInfo")]
#[derive(Clone)]
pub(crate) struct PyThrowsTargetInfo {
    throws_type_index: u16,
}

#[pymethods]
impl PyThrowsTargetInfo {
    /// Index into the method's ``Exceptions`` attribute table.
    #[getter]
    fn throws_type_index(&self) -> u16 {
        self.throws_type_index
    }

    fn __repr__(&self) -> String {
        format!(
            "ThrowsTargetInfo(throws_type_index={})",
            self.throws_type_index
        )
    }
}

/// Range and slot index for a local-variable type-annotation target.
#[pyclass(from_py_object, module = "pytecode._rust", name = "TableInfo")]
#[derive(Clone)]
pub(crate) struct PyTableInfo {
    inner: raw::TableInfo,
}

#[pymethods]
impl PyTableInfo {
    /// Starting bytecode offset of the local-variable range.
    #[getter]
    fn start_pc(&self) -> u16 {
        self.inner.start_pc
    }

    /// Length in bytes of the local-variable range.
    #[getter]
    fn length(&self) -> u16 {
        self.inner.length
    }

    /// Local-variable slot index.
    #[getter]
    fn index(&self) -> u16 {
        self.inner.index
    }

    fn __repr__(&self) -> String {
        format!(
            "TableInfo(start_pc={}, length={}, index={})",
            self.inner.start_pc, self.inner.length, self.inner.index
        )
    }
}

/// Type annotation attached to one or more local-variable ranges.
#[pyclass(from_py_object, module = "pytecode._rust", name = "LocalvarTargetInfo")]
#[derive(Clone)]
pub(crate) struct PyLocalvarTargetInfo {
    table: Vec<raw::TableInfo>,
}

#[pymethods]
impl PyLocalvarTargetInfo {
    /// Number of local-variable range entries.
    #[getter]
    fn table_length(&self) -> usize {
        self.table.len()
    }

    /// List of ``TableInfo`` local-variable ranges.
    #[getter]
    fn table(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.table
            .iter()
            .map(|t| wrap_pyclass!(py, PyTableInfo { inner: t.clone() }))
            .collect()
    }

    fn __repr__(&self) -> String {
        format!("LocalvarTargetInfo(table_length={})", self.table.len())
    }
}

/// Type annotation attached to an exception handler parameter.
#[pyclass(from_py_object, module = "pytecode._rust", name = "CatchTargetInfo")]
#[derive(Clone)]
pub(crate) struct PyCatchTargetInfo {
    exception_table_index: u16,
}

#[pymethods]
impl PyCatchTargetInfo {
    /// Index into the ``Code`` attribute's exception table.
    #[getter]
    fn exception_table_index(&self) -> u16 {
        self.exception_table_index
    }

    fn __repr__(&self) -> String {
        format!(
            "CatchTargetInfo(exception_table_index={})",
            self.exception_table_index
        )
    }
}

/// Type annotation located at a single bytecode instruction offset.
#[pyclass(from_py_object, module = "pytecode._rust", name = "OffsetTargetInfo")]
#[derive(Clone)]
pub(crate) struct PyOffsetTargetInfo {
    offset: u16,
}

#[pymethods]
impl PyOffsetTargetInfo {
    /// Bytecode offset of the annotated instruction.
    #[getter]
    fn offset(&self) -> u16 {
        self.offset
    }

    fn __repr__(&self) -> String {
        format!("OffsetTargetInfo(offset={})", self.offset)
    }
}

/// Type annotation attached to one type argument at a bytecode offset.
#[pyclass(
    from_py_object,
    module = "pytecode._rust",
    name = "TypeArgumentTargetInfo"
)]
#[derive(Clone)]
pub(crate) struct PyTypeArgumentTargetInfo {
    offset: u16,
    type_argument_index: u8,
}

#[pymethods]
impl PyTypeArgumentTargetInfo {
    /// Bytecode offset of the annotated instruction.
    #[getter]
    fn offset(&self) -> u16 {
        self.offset
    }

    /// 0-based index of the annotated type argument.
    #[getter]
    fn type_argument_index(&self) -> u8 {
        self.type_argument_index
    }

    fn __repr__(&self) -> String {
        format!(
            "TypeArgumentTargetInfo(offset={}, type_argument_index={})",
            self.offset, self.type_argument_index
        )
    }
}

fn wrap_target_info(py: Python<'_>, info: &raw::TargetInfo) -> PyResult<Py<PyAny>> {
    match info {
        raw::TargetInfo::TypeParameter {
            type_parameter_index,
        } => wrap_pyclass!(
            py,
            PyTypeParameterTargetInfo {
                type_parameter_index: *type_parameter_index,
            }
        ),
        raw::TargetInfo::Supertype { supertype_index } => {
            wrap_pyclass!(
                py,
                PySupertypeTargetInfo {
                    supertype_index: *supertype_index,
                }
            )
        }
        raw::TargetInfo::TypeParameterBound {
            type_parameter_index,
            bound_index,
        } => wrap_pyclass!(
            py,
            PyTypeParameterBoundTargetInfo {
                type_parameter_index: *type_parameter_index,
                bound_index: *bound_index,
            }
        ),
        raw::TargetInfo::Empty => wrap_pyclass!(py, PyEmptyTargetInfo),
        raw::TargetInfo::FormalParameter {
            formal_parameter_index,
        } => wrap_pyclass!(
            py,
            PyFormalParameterTargetInfo {
                formal_parameter_index: *formal_parameter_index,
            }
        ),
        raw::TargetInfo::Throws { throws_type_index } => {
            wrap_pyclass!(
                py,
                PyThrowsTargetInfo {
                    throws_type_index: *throws_type_index,
                }
            )
        }
        raw::TargetInfo::Localvar { table } => wrap_pyclass!(
            py,
            PyLocalvarTargetInfo {
                table: table.clone(),
            }
        ),
        raw::TargetInfo::Catch {
            exception_table_index,
        } => wrap_pyclass!(
            py,
            PyCatchTargetInfo {
                exception_table_index: *exception_table_index,
            }
        ),
        raw::TargetInfo::Offset { offset } => {
            wrap_pyclass!(py, PyOffsetTargetInfo { offset: *offset })
        }
        raw::TargetInfo::TypeArgument {
            offset,
            type_argument_index,
        } => wrap_pyclass!(
            py,
            PyTypeArgumentTargetInfo {
                offset: *offset,
                type_argument_index: *type_argument_index,
            }
        ),
    }
}

/// One navigation step within a nested or parameterized type.
#[pyclass(from_py_object, module = "pytecode._rust", name = "PathInfo")]
#[derive(Clone)]
pub(crate) struct PyPathInfo {
    inner: raw::PathInfo,
}

#[pymethods]
impl PyPathInfo {
    /// Kind of step in the type path.
    #[getter]
    fn type_path_kind(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::wrap_type_path_kind(py, self.inner.type_path_kind)
    }

    /// Type-argument index selected by this step (0 if not applicable).
    #[getter]
    fn type_argument_index(&self) -> u8 {
        self.inner.type_argument_index
    }

    fn __repr__(&self) -> String {
        format!(
            "PathInfo(kind={:?}, type_argument_index={})",
            self.inner.type_path_kind, self.inner.type_argument_index
        )
    }
}

/// Path that identifies which nested part of a type is annotated.
#[pyclass(from_py_object, module = "pytecode._rust", name = "TypePathInfo")]
#[derive(Clone)]
pub(crate) struct PyTypePathInfo {
    inner: raw::TypePathInfo,
}

#[pymethods]
impl PyTypePathInfo {
    /// Number of steps in the type path.
    #[getter]
    fn path_length(&self) -> usize {
        self.inner.path.len()
    }

    /// Ordered list of ``PathInfo`` navigation steps.
    #[getter]
    fn path(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .path
            .iter()
            .map(|p| wrap_pyclass!(py, PyPathInfo { inner: p.clone() }))
            .collect()
    }

    fn __repr__(&self) -> String {
        format!("TypePathInfo(path_length={})", self.inner.path.len())
    }
}

/// Fully decoded type annotation with target, path, and element values.
#[pyclass(from_py_object, module = "pytecode._rust", name = "TypeAnnotationInfo")]
#[derive(Clone)]
pub(crate) struct PyTypeAnnotationInfo {
    inner: raw::TypeAnnotationInfo,
}

#[pymethods]
impl PyTypeAnnotationInfo {
    /// Kind of program element annotated by this entry.
    #[getter]
    fn target_type(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::wrap_target_type(py, self.inner.target_type)
    }

    /// Location details selected by ``target_type``.
    #[getter]
    fn target_info(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        wrap_target_info(py, &self.inner.target_info)
    }

    /// Path identifying which nested type part is annotated.
    #[getter]
    fn target_path(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        wrap_pyclass!(
            py,
            PyTypePathInfo {
                inner: self.inner.target_path.clone(),
            }
        )
    }

    /// Constant-pool UTF-8 index of the annotation type descriptor.
    #[getter]
    fn type_index(&self) -> u16 {
        self.inner.type_index.into()
    }

    /// Number of element-value pairs in this annotation.
    #[getter]
    fn num_element_value_pairs(&self) -> usize {
        self.inner.element_value_pairs.len()
    }

    /// List of element-value pairs on this annotation.
    #[getter]
    fn element_value_pairs(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .element_value_pairs
            .iter()
            .map(|pair| {
                wrap_pyclass!(
                    py,
                    PyElementValuePairInfo {
                        inner: pair.clone(),
                    }
                )
            })
            .collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "TypeAnnotationInfo(type_index={}, pairs={})",
            u16::from(self.inner.type_index),
            self.inner.element_value_pairs.len()
        )
    }
}

// ===========================================================================
// Bootstrap, method parameter, and module sub-types
// ===========================================================================

/// One bootstrap method and its constant-pool arguments.
#[pyclass(
    from_py_object,
    module = "pytecode._rust",
    name = "BootstrapMethodInfo"
)]
#[derive(Clone)]
pub(crate) struct PyBootstrapMethodInfo {
    inner: raw::BootstrapMethodInfo,
}

#[pymethods]
impl PyBootstrapMethodInfo {
    /// Constant-pool index of the ``MethodHandleInfo`` bootstrap method.
    #[getter]
    fn bootstrap_method_ref(&self) -> u16 {
        self.inner.bootstrap_method_ref.into()
    }

    /// Number of bootstrap arguments passed to the bootstrap method.
    #[getter]
    fn num_bootstrap_arguments(&self) -> usize {
        self.inner.bootstrap_arguments.len()
    }

    /// Constant-pool indices of static arguments passed to the bootstrap method.
    #[getter]
    fn bootstrap_arguments(&self) -> Vec<u16> {
        self.inner
            .bootstrap_arguments
            .iter()
            .map(|x| (*x).into())
            .collect()
    }

    /// Deprecated alias for ``num_bootstrap_arguments``.
    #[getter]
    fn num_boostrap_arguments(&self, py: Python<'_>) -> PyResult<usize> {
        PyErr::warn(
            py,
            &py.get_type::<PyDeprecationWarning>(),
            c"num_boostrap_arguments is deprecated; use num_bootstrap_arguments",
            1,
        )?;
        Ok(self.num_bootstrap_arguments())
    }

    /// Deprecated alias for ``bootstrap_arguments``.
    #[getter]
    fn boostrap_arguments(&self, py: Python<'_>) -> PyResult<Vec<u16>> {
        PyErr::warn(
            py,
            &py.get_type::<PyDeprecationWarning>(),
            c"boostrap_arguments is deprecated; use bootstrap_arguments",
            1,
        )?;
        Ok(self.bootstrap_arguments())
    }

    fn __repr__(&self) -> String {
        format!(
            "BootstrapMethodInfo(bootstrap_method_ref={}, num_bootstrap_arguments={})",
            u16::from(self.inner.bootstrap_method_ref),
            self.inner.bootstrap_arguments.len()
        )
    }
}

/// Name and flags for one method parameter.
#[pyclass(
    from_py_object,
    module = "pytecode._rust",
    name = "MethodParameterInfo"
)]
#[derive(Clone)]
pub(crate) struct PyMethodParameterInfo {
    inner: raw::MethodParameterInfo,
}

#[pymethods]
impl PyMethodParameterInfo {
    /// Constant-pool UTF-8 index of the parameter name, or ``0`` if unnamed.
    #[getter]
    fn name_index(&self) -> u16 {
        self.inner.name_index.into()
    }

    /// Access flags applied to the method parameter.
    #[getter]
    fn access_flags(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::wrap_method_parameter_access_flags(py, self.inner.access_flags)
    }

    fn __repr__(&self) -> String {
        format!(
            "MethodParameterInfo(name_index={})",
            u16::from(self.inner.name_index)
        )
    }
}

/// One ``requires`` directive inside a module declaration.
#[pyclass(from_py_object, module = "pytecode._rust", name = "RequiresInfo")]
#[derive(Clone)]
pub(crate) struct PyRequiresInfo {
    inner: raw::RequiresInfo,
}

#[pymethods]
impl PyRequiresInfo {
    /// Constant-pool ``ModuleInfo`` index of the required module.
    #[getter]
    fn requires_index(&self) -> u16 {
        self.inner.requires_index.into()
    }

    /// Flags governing the ``requires`` directive.
    #[getter]
    fn requires_flags(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::wrap_module_requires_access_flags(py, self.inner.requires_flags)
    }

    /// Deprecated alias for ``requires_flags``.
    #[getter]
    fn requires_flag(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        PyErr::warn(
            py,
            &py.get_type::<PyDeprecationWarning>(),
            c"requires_flag is deprecated; use requires_flags",
            1,
        )?;
        self.requires_flags(py)
    }

    /// Constant-pool UTF-8 index of the required module version, or ``0``.
    #[getter]
    fn requires_version_index(&self) -> u16 {
        self.inner.requires_version_index.into()
    }

    fn __repr__(&self) -> String {
        format!(
            "RequiresInfo(requires_index={}, requires_version_index={})",
            u16::from(self.inner.requires_index),
            u16::from(self.inner.requires_version_index)
        )
    }
}

/// One ``exports`` directive inside a module declaration.
#[pyclass(from_py_object, module = "pytecode._rust", name = "ExportInfo")]
#[derive(Clone)]
pub(crate) struct PyExportInfo {
    inner: raw::ExportInfo,
}

#[pymethods]
impl PyExportInfo {
    /// Constant-pool ``PackageInfo`` index of the exported package.
    #[getter]
    fn exports_index(&self) -> u16 {
        self.inner.exports_index.into()
    }

    /// Flags governing the ``exports`` directive.
    #[getter]
    fn exports_flags(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::wrap_module_exports_access_flags(py, self.inner.exports_flags)
    }

    /// Number of target module indices in ``exports_to_index``.
    #[getter]
    fn exports_to_count(&self) -> usize {
        self.inner.exports_to_index.len()
    }

    /// Constant-pool ``ModuleInfo`` indices for qualified exports.
    #[getter]
    fn exports_to_index(&self) -> Vec<u16> {
        self.inner
            .exports_to_index
            .iter()
            .map(|x| (*x).into())
            .collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "ExportInfo(exports_index={}, exports_to_count={})",
            u16::from(self.inner.exports_index),
            self.inner.exports_to_index.len()
        )
    }
}

/// One ``opens`` directive inside a module declaration.
#[pyclass(from_py_object, module = "pytecode._rust", name = "OpensInfo")]
#[derive(Clone)]
pub(crate) struct PyOpensInfo {
    inner: raw::OpensInfo,
}

#[pymethods]
impl PyOpensInfo {
    /// Constant-pool ``PackageInfo`` index of the opened package.
    #[getter]
    fn opens_index(&self) -> u16 {
        self.inner.opens_index.into()
    }

    /// Flags governing the ``opens`` directive.
    #[getter]
    fn opens_flags(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::wrap_module_opens_access_flags(py, self.inner.opens_flags)
    }

    /// Number of target module indices in ``opens_to_index``.
    #[getter]
    fn opens_to_count(&self) -> usize {
        self.inner.opens_to_index.len()
    }

    /// Constant-pool ``ModuleInfo`` indices for qualified opens.
    #[getter]
    fn opens_to_index(&self) -> Vec<u16> {
        self.inner
            .opens_to_index
            .iter()
            .map(|x| (*x).into())
            .collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "OpensInfo(opens_index={}, opens_to_count={})",
            u16::from(self.inner.opens_index),
            self.inner.opens_to_index.len()
        )
    }
}

/// One ``provides ... with ...`` directive inside a module declaration.
#[pyclass(from_py_object, module = "pytecode._rust", name = "ProvidesInfo")]
#[derive(Clone)]
pub(crate) struct PyProvidesInfo {
    inner: raw::ProvidesInfo,
}

#[pymethods]
impl PyProvidesInfo {
    /// Constant-pool ``ClassInfo`` index of the service interface.
    #[getter]
    fn provides_index(&self) -> u16 {
        self.inner.provides_index.into()
    }

    /// Number of implementation class indices in ``provides_with_index``.
    #[getter]
    fn provides_with_count(&self) -> usize {
        self.inner.provides_with_index.len()
    }

    /// Constant-pool ``ClassInfo`` indices of service implementations.
    #[getter]
    fn provides_with_index(&self) -> Vec<u16> {
        self.inner
            .provides_with_index
            .iter()
            .map(|x| (*x).into())
            .collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "ProvidesInfo(provides_index={}, provides_with_count={})",
            u16::from(self.inner.provides_index),
            self.inner.provides_with_index.len()
        )
    }
}

/// Name, descriptor, and attributes for one record component.
#[pyclass(
    from_py_object,
    module = "pytecode._rust",
    name = "RecordComponentInfo"
)]
#[derive(Clone)]
pub(crate) struct PyRecordComponentInfo {
    inner: raw::RecordComponentInfo,
}

#[pymethods]
impl PyRecordComponentInfo {
    /// Constant-pool UTF-8 index of the record component name.
    #[getter]
    fn name_index(&self) -> u16 {
        self.inner.name_index.into()
    }

    /// Constant-pool UTF-8 index of the record component descriptor.
    #[getter]
    fn descriptor_index(&self) -> u16 {
        self.inner.descriptor_index.into()
    }

    /// Number of attributes attached to this record component.
    #[getter]
    fn attributes_count(&self) -> usize {
        self.inner.attributes.len()
    }

    /// Raw attributes attached to this record component.
    #[getter]
    fn attributes(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .attributes
            .iter()
            .map(|attr| crate::wrap_attribute(py, attr))
            .collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "RecordComponentInfo(name_index={}, descriptor_index={}, attributes={})",
            u16::from(self.inner.name_index),
            u16::from(self.inner.descriptor_index),
            self.inner.attributes.len()
        )
    }
}

// ===========================================================================
// Top-level attribute types
// ===========================================================================

/// Verification frames used by the bytecode verifier.
#[pyclass(from_py_object, module = "pytecode._rust", name = "StackMapTableAttr")]
#[derive(Clone)]
pub(crate) struct PyStackMapTableAttr {
    pub(crate) inner: raw::StackMapTableAttribute,
}

#[pymethods]
impl PyStackMapTableAttr {
    /// Constant-pool UTF-8 index of the attribute name.
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index.into()
    }

    /// Encoded payload length of this attribute in bytes.
    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    /// Number of stack-map frame entries.
    #[getter]
    fn number_of_entries(&self) -> usize {
        self.inner.entries.len()
    }

    /// Decoded stack-map frames.
    #[getter]
    fn entries(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .entries
            .iter()
            .map(|e| wrap_stack_map_frame_info(py, e))
            .collect()
    }

    fn __repr__(&self) -> String {
        format!("StackMapTableAttr(entries={})", self.inner.entries.len())
    }
}

/// Inner and nested class metadata declared for the current class.
#[pyclass(from_py_object, module = "pytecode._rust", name = "InnerClassesAttr")]
#[derive(Clone)]
pub(crate) struct PyInnerClassesAttr {
    pub(crate) inner: raw::InnerClassesAttribute,
}

#[pymethods]
impl PyInnerClassesAttr {
    /// Constant-pool UTF-8 index of the attribute name.
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index.into()
    }

    /// Encoded payload length of this attribute in bytes.
    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    /// Number of nested class records.
    #[getter]
    fn number_of_classes(&self) -> usize {
        self.inner.classes.len()
    }

    /// List of ``InnerClassInfo`` entries.
    #[getter]
    fn classes(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .classes
            .iter()
            .map(|c| wrap_pyclass!(py, PyInnerClassInfo { inner: c.clone() }))
            .collect()
    }

    fn __repr__(&self) -> String {
        format!("InnerClassesAttr(classes={})", self.inner.classes.len())
    }
}

/// Owner class and method for a local or anonymous class.
#[pyclass(
    from_py_object,
    module = "pytecode._rust",
    name = "EnclosingMethodAttr"
)]
#[derive(Clone)]
pub(crate) struct PyEnclosingMethodAttr {
    pub(crate) inner: raw::EnclosingMethodAttribute,
}

#[pymethods]
impl PyEnclosingMethodAttr {
    /// Constant-pool UTF-8 index of the attribute name.
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index.into()
    }

    /// Encoded payload length of this attribute in bytes.
    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    /// Constant-pool ``ClassInfo`` index of the innermost enclosing class.
    #[getter]
    fn class_index(&self) -> u16 {
        self.inner.class_index.into()
    }

    /// Constant-pool ``NameAndTypeInfo`` index of the enclosing method, or ``0``.
    #[getter]
    fn method_index(&self) -> u16 {
        self.inner.method_index.into()
    }

    fn __repr__(&self) -> String {
        format!(
            "EnclosingMethodAttr(class_index={}, method_index={})",
            u16::from(self.inner.class_index),
            u16::from(self.inner.method_index)
        )
    }
}

/// Marker attribute for compiler-synthesized declarations.
#[pyclass(from_py_object, module = "pytecode._rust", name = "SyntheticAttr")]
#[derive(Clone)]
pub(crate) struct PySyntheticAttr {
    pub(crate) inner: raw::SyntheticAttribute,
}

#[pymethods]
impl PySyntheticAttr {
    /// Constant-pool UTF-8 index of the attribute name.
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index.into()
    }

    /// Encoded payload length of this attribute in bytes.
    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    fn __repr__(&self) -> &'static str {
        "SyntheticAttr()"
    }
}

/// Marker attribute declaring the class member deprecated.
#[pyclass(from_py_object, module = "pytecode._rust", name = "DeprecatedAttr")]
#[derive(Clone)]
pub(crate) struct PyDeprecatedAttr {
    pub(crate) inner: raw::DeprecatedAttribute,
}

#[pymethods]
impl PyDeprecatedAttr {
    /// Constant-pool UTF-8 index of the attribute name.
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index.into()
    }

    /// Encoded payload length of this attribute in bytes.
    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    fn __repr__(&self) -> &'static str {
        "DeprecatedAttr()"
    }
}

/// Source line mappings for a method body.
#[pyclass(
    from_py_object,
    module = "pytecode._rust",
    name = "LineNumberTableAttr"
)]
#[derive(Clone)]
pub(crate) struct PyLineNumberTableAttr {
    pub(crate) inner: raw::LineNumberTableAttribute,
}

#[pymethods]
impl PyLineNumberTableAttr {
    /// Constant-pool UTF-8 index of the attribute name.
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index.into()
    }

    /// Encoded payload length of this attribute in bytes.
    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    /// Number of line-number mapping entries.
    #[getter]
    fn line_number_table_length(&self) -> usize {
        self.inner.line_number_table.len()
    }

    /// List of ``LineNumberInfo`` entries.
    #[getter]
    fn line_number_table(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .line_number_table
            .iter()
            .map(|e| wrap_pyclass!(py, PyLineNumberInfo { inner: e.clone() }))
            .collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "LineNumberTableAttr(entries={})",
            self.inner.line_number_table.len()
        )
    }
}

/// Debug names and descriptors for local-variable slots.
#[pyclass(
    from_py_object,
    module = "pytecode._rust",
    name = "LocalVariableTableAttr"
)]
#[derive(Clone)]
pub(crate) struct PyLocalVariableTableAttr {
    pub(crate) inner: raw::LocalVariableTableAttribute,
}

#[pymethods]
impl PyLocalVariableTableAttr {
    /// Constant-pool UTF-8 index of the attribute name.
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index.into()
    }

    /// Encoded payload length of this attribute in bytes.
    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    /// Number of local-variable table entries.
    #[getter]
    fn local_variable_table_length(&self) -> usize {
        self.inner.local_variable_table.len()
    }

    /// List of ``LocalVariableInfo`` entries.
    #[getter]
    fn local_variable_table(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .local_variable_table
            .iter()
            .map(|e| wrap_pyclass!(py, PyLocalVariableInfo { inner: e.clone() }))
            .collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "LocalVariableTableAttr(entries={})",
            self.inner.local_variable_table.len()
        )
    }
}

/// Generic-signature companion to ``LocalVariableTableAttr``.
#[pyclass(
    from_py_object,
    module = "pytecode._rust",
    name = "LocalVariableTypeTableAttr"
)]
#[derive(Clone)]
pub(crate) struct PyLocalVariableTypeTableAttr {
    pub(crate) inner: raw::LocalVariableTypeTableAttribute,
}

#[pymethods]
impl PyLocalVariableTypeTableAttr {
    /// Constant-pool UTF-8 index of the attribute name.
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index.into()
    }

    /// Encoded payload length of this attribute in bytes.
    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    /// Number of local-variable generic signature entries.
    #[getter]
    fn local_variable_type_table_length(&self) -> usize {
        self.inner.local_variable_type_table.len()
    }

    /// List of ``LocalVariableTypeInfo`` entries.
    #[getter]
    fn local_variable_type_table(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .local_variable_type_table
            .iter()
            .map(|e| wrap_pyclass!(py, PyLocalVariableTypeInfo { inner: e.clone() }))
            .collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "LocalVariableTypeTableAttr(entries={})",
            self.inner.local_variable_type_table.len()
        )
    }
}

/// Annotations retained in the runtime-visible set.
#[pyclass(
    from_py_object,
    module = "pytecode._rust",
    name = "RuntimeVisibleAnnotationsAttr"
)]
#[derive(Clone)]
pub(crate) struct PyRuntimeVisibleAnnotationsAttr {
    pub(crate) inner: raw::RuntimeVisibleAnnotationsAttribute,
}

#[pymethods]
impl PyRuntimeVisibleAnnotationsAttr {
    /// Constant-pool UTF-8 index of the attribute name.
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index.into()
    }

    /// Encoded payload length of this attribute in bytes.
    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    /// Number of annotations.
    #[getter]
    fn num_annotations(&self) -> usize {
        self.inner.annotations.len()
    }

    /// List of ``AnnotationInfo`` entries.
    #[getter]
    fn annotations(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .annotations
            .iter()
            .map(|a| wrap_pyclass!(py, PyAnnotationInfo { inner: a.clone() }))
            .collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "RuntimeVisibleAnnotationsAttr(annotations={})",
            self.inner.annotations.len()
        )
    }
}

/// Annotations stored in the runtime-invisible set.
#[pyclass(
    from_py_object,
    module = "pytecode._rust",
    name = "RuntimeInvisibleAnnotationsAttr"
)]
#[derive(Clone)]
pub(crate) struct PyRuntimeInvisibleAnnotationsAttr {
    pub(crate) inner: raw::RuntimeInvisibleAnnotationsAttribute,
}

#[pymethods]
impl PyRuntimeInvisibleAnnotationsAttr {
    /// Constant-pool UTF-8 index of the attribute name.
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index.into()
    }

    /// Encoded payload length of this attribute in bytes.
    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    /// Number of annotations.
    #[getter]
    fn num_annotations(&self) -> usize {
        self.inner.annotations.len()
    }

    /// List of ``AnnotationInfo`` entries.
    #[getter]
    fn annotations(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .annotations
            .iter()
            .map(|a| wrap_pyclass!(py, PyAnnotationInfo { inner: a.clone() }))
            .collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "RuntimeInvisibleAnnotationsAttr(annotations={})",
            self.inner.annotations.len()
        )
    }
}

/// Runtime-visible annotations grouped by parameter position.
#[pyclass(
    from_py_object,
    module = "pytecode._rust",
    name = "RuntimeVisibleParameterAnnotationsAttr"
)]
#[derive(Clone)]
pub(crate) struct PyRuntimeVisibleParameterAnnotationsAttr {
    pub(crate) inner: raw::RuntimeVisibleParameterAnnotationsAttribute,
}

#[pymethods]
impl PyRuntimeVisibleParameterAnnotationsAttr {
    /// Constant-pool UTF-8 index of the attribute name.
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index.into()
    }

    /// Encoded payload length of this attribute in bytes.
    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    /// Number of parameter annotation entries.
    #[getter]
    fn num_parameters(&self) -> usize {
        self.inner.parameter_annotations.len()
    }

    /// List of ``ParameterAnnotationInfo`` entries.
    #[getter]
    fn parameter_annotations(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .parameter_annotations
            .iter()
            .map(|p| wrap_pyclass!(py, PyParameterAnnotationInfo { inner: p.clone() }))
            .collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "RuntimeVisibleParameterAnnotationsAttr(parameters={})",
            self.inner.parameter_annotations.len()
        )
    }
}

/// Runtime-invisible annotations grouped by parameter position.
#[pyclass(
    from_py_object,
    module = "pytecode._rust",
    name = "RuntimeInvisibleParameterAnnotationsAttr"
)]
#[derive(Clone)]
pub(crate) struct PyRuntimeInvisibleParameterAnnotationsAttr {
    pub(crate) inner: raw::RuntimeInvisibleParameterAnnotationsAttribute,
}

#[pymethods]
impl PyRuntimeInvisibleParameterAnnotationsAttr {
    /// Constant-pool UTF-8 index of the attribute name.
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index.into()
    }

    /// Encoded payload length of this attribute in bytes.
    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    /// Number of parameter annotation entries.
    #[getter]
    fn num_parameters(&self) -> usize {
        self.inner.parameter_annotations.len()
    }

    /// List of ``ParameterAnnotationInfo`` entries.
    #[getter]
    fn parameter_annotations(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .parameter_annotations
            .iter()
            .map(|p| wrap_pyclass!(py, PyParameterAnnotationInfo { inner: p.clone() }))
            .collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "RuntimeInvisibleParameterAnnotationsAttr(parameters={})",
            self.inner.parameter_annotations.len()
        )
    }
}

/// Runtime-visible type annotations.
#[pyclass(
    from_py_object,
    module = "pytecode._rust",
    name = "RuntimeVisibleTypeAnnotationsAttr"
)]
#[derive(Clone)]
pub(crate) struct PyRuntimeVisibleTypeAnnotationsAttr {
    pub(crate) inner: raw::RuntimeVisibleTypeAnnotationsAttribute,
}

#[pymethods]
impl PyRuntimeVisibleTypeAnnotationsAttr {
    /// Constant-pool UTF-8 index of the attribute name.
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index.into()
    }

    /// Encoded payload length of this attribute in bytes.
    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    /// Number of type annotations.
    #[getter]
    fn num_annotations(&self) -> usize {
        self.inner.annotations.len()
    }

    /// List of ``TypeAnnotationInfo`` entries.
    #[getter]
    fn annotations(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .annotations
            .iter()
            .map(|a| wrap_pyclass!(py, PyTypeAnnotationInfo { inner: a.clone() }))
            .collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "RuntimeVisibleTypeAnnotationsAttr(annotations={})",
            self.inner.annotations.len()
        )
    }
}

/// Runtime-invisible type annotations.
#[pyclass(
    from_py_object,
    module = "pytecode._rust",
    name = "RuntimeInvisibleTypeAnnotationsAttr"
)]
#[derive(Clone)]
pub(crate) struct PyRuntimeInvisibleTypeAnnotationsAttr {
    pub(crate) inner: raw::RuntimeInvisibleTypeAnnotationsAttribute,
}

#[pymethods]
impl PyRuntimeInvisibleTypeAnnotationsAttr {
    /// Constant-pool UTF-8 index of the attribute name.
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index.into()
    }

    /// Encoded payload length of this attribute in bytes.
    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    /// Number of type annotations.
    #[getter]
    fn num_annotations(&self) -> usize {
        self.inner.annotations.len()
    }

    /// List of ``TypeAnnotationInfo`` entries.
    #[getter]
    fn annotations(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .annotations
            .iter()
            .map(|a| wrap_pyclass!(py, PyTypeAnnotationInfo { inner: a.clone() }))
            .collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "RuntimeInvisibleTypeAnnotationsAttr(annotations={})",
            self.inner.annotations.len()
        )
    }
}

/// Default value recorded for an annotation element method.
#[pyclass(
    from_py_object,
    module = "pytecode._rust",
    name = "AnnotationDefaultAttr"
)]
#[derive(Clone)]
pub(crate) struct PyAnnotationDefaultAttr {
    pub(crate) inner: raw::AnnotationDefaultAttribute,
}

#[pymethods]
impl PyAnnotationDefaultAttr {
    /// Constant-pool UTF-8 index of the attribute name.
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index.into()
    }

    /// Encoded payload length of this attribute in bytes.
    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    /// Default ``ElementValueInfo`` payload.
    #[getter]
    fn default_value(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        wrap_element_value_info(py, &self.inner.default_value)
    }

    fn __repr__(&self) -> &'static str {
        "AnnotationDefaultAttr()"
    }
}

/// Bootstrap method table used by ``invokedynamic`` and dynamic constants.
#[pyclass(
    from_py_object,
    module = "pytecode._rust",
    name = "BootstrapMethodsAttr"
)]
#[derive(Clone)]
pub(crate) struct PyBootstrapMethodsAttr {
    pub(crate) inner: raw::BootstrapMethodsAttribute,
}

#[pymethods]
impl PyBootstrapMethodsAttr {
    /// Constant-pool UTF-8 index of the attribute name.
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index.into()
    }

    /// Encoded payload length of this attribute in bytes.
    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    /// Number of bootstrap method entries.
    #[getter]
    fn num_bootstrap_methods(&self) -> usize {
        self.inner.bootstrap_methods.len()
    }

    /// List of ``BootstrapMethodInfo`` entries.
    #[getter]
    fn bootstrap_methods(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .bootstrap_methods
            .iter()
            .map(|m| wrap_pyclass!(py, PyBootstrapMethodInfo { inner: m.clone() }))
            .collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "BootstrapMethodsAttr(num_bootstrap_methods={})",
            self.inner.bootstrap_methods.len()
        )
    }
}

/// Parameter names and flags recorded on a method.
#[pyclass(
    from_py_object,
    module = "pytecode._rust",
    name = "MethodParametersAttr"
)]
#[derive(Clone)]
pub(crate) struct PyMethodParametersAttr {
    pub(crate) inner: raw::MethodParametersAttribute,
}

#[pymethods]
impl PyMethodParametersAttr {
    /// Constant-pool UTF-8 index of the attribute name.
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index.into()
    }

    /// Encoded payload length of this attribute in bytes.
    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    /// Number of parameter entries.
    #[getter]
    fn parameters_count(&self) -> usize {
        self.inner.parameters.len()
    }

    /// List of ``MethodParameterInfo`` entries.
    #[getter]
    fn parameters(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .parameters
            .iter()
            .map(|p| wrap_pyclass!(py, PyMethodParameterInfo { inner: p.clone() }))
            .collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "MethodParametersAttr(parameters_count={})",
            self.inner.parameters.len()
        )
    }
}

/// Structured contents of a ``module-info`` declaration.
#[pyclass(from_py_object, module = "pytecode._rust", name = "ModuleAttr")]
#[derive(Clone)]
pub(crate) struct PyModuleAttr {
    pub(crate) inner: raw::ModuleAttribute,
}

#[pymethods]
impl PyModuleAttr {
    /// Constant-pool UTF-8 index of the attribute name.
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index.into()
    }

    /// Encoded payload length of this attribute in bytes.
    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    /// Constant-pool ``ModuleInfo`` index of the module name.
    #[getter]
    fn module_name_index(&self) -> u16 {
        self.inner.module.module_name_index.into()
    }

    /// Module access flags.
    #[getter]
    fn module_flags(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::wrap_module_access_flags(py, self.inner.module.module_flags)
    }

    /// Constant-pool UTF-8 index of the module version, or ``0``.
    #[getter]
    fn module_version_index(&self) -> u16 {
        self.inner.module.module_version_index.into()
    }

    /// Number of ``requires`` directives.
    #[getter]
    fn requires_count(&self) -> usize {
        self.inner.module.requires.len()
    }

    /// List of ``RequiresInfo`` directives.
    #[getter]
    fn requires(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .module
            .requires
            .iter()
            .map(|r| wrap_pyclass!(py, PyRequiresInfo { inner: r.clone() }))
            .collect()
    }

    /// Number of ``exports`` directives.
    #[getter]
    fn exports_count(&self) -> usize {
        self.inner.module.exports.len()
    }

    /// List of ``ExportInfo`` directives.
    #[getter]
    fn exports(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .module
            .exports
            .iter()
            .map(|e| wrap_pyclass!(py, PyExportInfo { inner: e.clone() }))
            .collect()
    }

    /// Number of ``opens`` directives.
    #[getter]
    fn opens_count(&self) -> usize {
        self.inner.module.opens.len()
    }

    /// List of ``OpensInfo`` directives.
    #[getter]
    fn opens(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .module
            .opens
            .iter()
            .map(|o| wrap_pyclass!(py, PyOpensInfo { inner: o.clone() }))
            .collect()
    }

    /// Number of ``uses`` service interface entries.
    #[getter]
    fn uses_count(&self) -> usize {
        self.inner.module.uses_index.len()
    }

    /// Constant-pool ``ClassInfo`` indices of used services.
    #[getter]
    fn uses_index(&self) -> Vec<u16> {
        self.inner
            .module
            .uses_index
            .iter()
            .map(|x| (*x).into())
            .collect()
    }

    /// Number of ``provides`` directives.
    #[getter]
    fn provides_count(&self) -> usize {
        self.inner.module.provides.len()
    }

    /// List of ``ProvidesInfo`` directives.
    #[getter]
    fn provides(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .module
            .provides
            .iter()
            .map(|p| wrap_pyclass!(py, PyProvidesInfo { inner: p.clone() }))
            .collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "ModuleAttr(module_name_index={}, requires={}, exports={}, opens={}, provides={})",
            u16::from(self.inner.module.module_name_index),
            self.inner.module.requires.len(),
            self.inner.module.exports.len(),
            self.inner.module.opens.len(),
            self.inner.module.provides.len()
        )
    }
}

/// Package list associated with a module declaration.
#[pyclass(from_py_object, module = "pytecode._rust", name = "ModulePackagesAttr")]
#[derive(Clone)]
pub(crate) struct PyModulePackagesAttr {
    pub(crate) inner: raw::ModulePackagesAttribute,
}

#[pymethods]
impl PyModulePackagesAttr {
    /// Constant-pool UTF-8 index of the attribute name.
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index.into()
    }

    /// Encoded payload length of this attribute in bytes.
    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    /// Number of packages listed by this module attribute.
    #[getter]
    fn package_count(&self) -> usize {
        self.inner.package_index.len()
    }

    /// Constant-pool ``PackageInfo`` indices.
    #[getter]
    fn package_index(&self) -> Vec<u16> {
        self.inner
            .package_index
            .iter()
            .map(|x| (*x).into())
            .collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "ModulePackagesAttr(package_count={})",
            self.inner.package_index.len()
        )
    }
}

/// Main class recorded for a module.
#[pyclass(
    from_py_object,
    module = "pytecode._rust",
    name = "ModuleMainClassAttr"
)]
#[derive(Clone)]
pub(crate) struct PyModuleMainClassAttr {
    pub(crate) inner: raw::ModuleMainClassAttribute,
}

#[pymethods]
impl PyModuleMainClassAttr {
    /// Constant-pool UTF-8 index of the attribute name.
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index.into()
    }

    /// Encoded payload length of this attribute in bytes.
    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    /// Constant-pool ``ClassInfo`` index of the module main class.
    #[getter]
    fn main_class_index(&self) -> u16 {
        self.inner.main_class_index.into()
    }

    fn __repr__(&self) -> String {
        format!(
            "ModuleMainClassAttr(main_class_index={})",
            u16::from(self.inner.main_class_index)
        )
    }
}

/// Host class reference for a nest member.
#[pyclass(from_py_object, module = "pytecode._rust", name = "NestHostAttr")]
#[derive(Clone)]
pub(crate) struct PyNestHostAttr {
    pub(crate) inner: raw::NestHostAttribute,
}

#[pymethods]
impl PyNestHostAttr {
    /// Constant-pool UTF-8 index of the attribute name.
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index.into()
    }

    /// Encoded payload length of this attribute in bytes.
    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    /// Constant-pool ``ClassInfo`` index of the nest host class.
    #[getter]
    fn host_class_index(&self) -> u16 {
        self.inner.host_class_index.into()
    }

    fn __repr__(&self) -> String {
        format!(
            "NestHostAttr(host_class_index={})",
            u16::from(self.inner.host_class_index)
        )
    }
}

/// List of classes that belong to the same nest.
#[pyclass(from_py_object, module = "pytecode._rust", name = "NestMembersAttr")]
#[derive(Clone)]
pub(crate) struct PyNestMembersAttr {
    pub(crate) inner: raw::NestMembersAttribute,
}

#[pymethods]
impl PyNestMembersAttr {
    /// Constant-pool UTF-8 index of the attribute name.
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index.into()
    }

    /// Encoded payload length of this attribute in bytes.
    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    /// Number of nest member class indices.
    #[getter]
    fn number_of_classes(&self) -> usize {
        self.inner.classes.len()
    }

    /// Constant-pool ``ClassInfo`` indices of nest members.
    #[getter]
    fn classes(&self) -> Vec<u16> {
        self.inner.classes.iter().map(|x| (*x).into()).collect()
    }

    fn __repr__(&self) -> String {
        format!("NestMembersAttr(classes={})", self.inner.classes.len())
    }
}

/// Record component list for a record class.
#[pyclass(from_py_object, module = "pytecode._rust", name = "RecordAttr")]
#[derive(Clone)]
pub(crate) struct PyRecordAttr {
    pub(crate) inner: raw::RecordAttribute,
}

#[pymethods]
impl PyRecordAttr {
    /// Constant-pool UTF-8 index of the attribute name.
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index.into()
    }

    /// Encoded payload length of this attribute in bytes.
    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    /// Number of record components.
    #[getter]
    fn components_count(&self) -> usize {
        self.inner.components.len()
    }

    /// List of ``RecordComponentInfo`` declarations.
    #[getter]
    fn components(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .components
            .iter()
            .map(|c| wrap_pyclass!(py, PyRecordComponentInfo { inner: c.clone() }))
            .collect()
    }

    fn __repr__(&self) -> String {
        format!("RecordAttr(components={})", self.inner.components.len())
    }
}

/// Sealed-class permit list.
#[pyclass(
    from_py_object,
    module = "pytecode._rust",
    name = "PermittedSubclassesAttr"
)]
#[derive(Clone)]
pub(crate) struct PyPermittedSubclassesAttr {
    pub(crate) inner: raw::PermittedSubclassesAttribute,
}

#[pymethods]
impl PyPermittedSubclassesAttr {
    /// Constant-pool UTF-8 index of the attribute name.
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index.into()
    }

    /// Encoded payload length of this attribute in bytes.
    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    /// Number of permitted subclass entries.
    #[getter]
    fn number_of_classes(&self) -> usize {
        self.inner.classes.len()
    }

    /// Constant-pool ``ClassInfo`` indices of permitted subclasses.
    #[getter]
    fn classes(&self) -> Vec<u16> {
        self.inner.classes.iter().map(|x| (*x).into()).collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "PermittedSubclassesAttr(classes={})",
            self.inner.classes.len()
        )
    }
}

// ===========================================================================
// Registration
// ===========================================================================

#[allow(unused_variables)]
pub(crate) fn register(py: Python<'_>, module: &Bound<'_, PyModule>) -> PyResult<()> {
    // Verification type info variants
    module.add_class::<PyTopVariableInfo>()?;
    module.add_class::<PyIntegerVariableInfo>()?;
    module.add_class::<PyFloatVariableInfo>()?;
    module.add_class::<PyDoubleVariableInfo>()?;
    module.add_class::<PyLongVariableInfo>()?;
    module.add_class::<PyNullVariableInfo>()?;
    module.add_class::<PyUninitializedThisVariableInfo>()?;
    module.add_class::<PyObjectVariableInfo>()?;
    module.add_class::<PyUninitializedVariableInfo>()?;

    // Stack map frame info variants
    module.add_class::<PySameFrameInfo>()?;
    module.add_class::<PySameLocals1StackItemFrameInfo>()?;
    module.add_class::<PySameLocals1StackItemFrameExtendedInfo>()?;
    module.add_class::<PyChopFrameInfo>()?;
    module.add_class::<PySameFrameExtendedInfo>()?;
    module.add_class::<PyAppendFrameInfo>()?;
    module.add_class::<PyFullFrameInfo>()?;

    // Simple entry types
    module.add_class::<PyLineNumberInfo>()?;
    module.add_class::<PyLocalVariableInfo>()?;
    module.add_class::<PyLocalVariableTypeInfo>()?;
    module.add_class::<PyInnerClassInfo>()?;

    // Annotation sub-types
    module.add_class::<PyConstValueInfo>()?;
    module.add_class::<PyEnumConstantValueInfo>()?;
    module.add_class::<PyClassInfoValueInfo>()?;
    module.add_class::<PyArrayValueInfo>()?;
    module.add_class::<PyElementValueInfo>()?;
    module.add_class::<PyElementValuePairInfo>()?;
    module.add_class::<PyAnnotationInfo>()?;
    module.add_class::<PyParameterAnnotationInfo>()?;

    // Type annotation sub-types
    module.add_class::<PyTypeParameterTargetInfo>()?;
    module.add_class::<PySupertypeTargetInfo>()?;
    module.add_class::<PyTypeParameterBoundTargetInfo>()?;
    module.add_class::<PyEmptyTargetInfo>()?;
    module.add_class::<PyFormalParameterTargetInfo>()?;
    module.add_class::<PyThrowsTargetInfo>()?;
    module.add_class::<PyTableInfo>()?;
    module.add_class::<PyLocalvarTargetInfo>()?;
    module.add_class::<PyCatchTargetInfo>()?;
    module.add_class::<PyOffsetTargetInfo>()?;
    module.add_class::<PyTypeArgumentTargetInfo>()?;
    module.add_class::<PyPathInfo>()?;
    module.add_class::<PyTypePathInfo>()?;
    module.add_class::<PyTypeAnnotationInfo>()?;

    // Bootstrap, method parameter, module sub-types
    module.add_class::<PyBootstrapMethodInfo>()?;
    module.add_class::<PyMethodParameterInfo>()?;
    module.add_class::<PyRequiresInfo>()?;
    module.add_class::<PyExportInfo>()?;
    module.add_class::<PyOpensInfo>()?;
    module.add_class::<PyProvidesInfo>()?;
    module.add_class::<PyRecordComponentInfo>()?;

    // Top-level attribute types
    module.add_class::<PyStackMapTableAttr>()?;
    module.add_class::<PyInnerClassesAttr>()?;
    module.add_class::<PyEnclosingMethodAttr>()?;
    module.add_class::<PySyntheticAttr>()?;
    module.add_class::<PyDeprecatedAttr>()?;
    module.add_class::<PyLineNumberTableAttr>()?;
    module.add_class::<PyLocalVariableTableAttr>()?;
    module.add_class::<PyLocalVariableTypeTableAttr>()?;
    module.add_class::<PyRuntimeVisibleAnnotationsAttr>()?;
    module.add_class::<PyRuntimeInvisibleAnnotationsAttr>()?;
    module.add_class::<PyRuntimeVisibleParameterAnnotationsAttr>()?;
    module.add_class::<PyRuntimeInvisibleParameterAnnotationsAttr>()?;
    module.add_class::<PyRuntimeVisibleTypeAnnotationsAttr>()?;
    module.add_class::<PyRuntimeInvisibleTypeAnnotationsAttr>()?;
    module.add_class::<PyAnnotationDefaultAttr>()?;
    module.add_class::<PyBootstrapMethodsAttr>()?;
    module.add_class::<PyMethodParametersAttr>()?;
    module.add_class::<PyModuleAttr>()?;
    module.add_class::<PyModulePackagesAttr>()?;
    module.add_class::<PyModuleMainClassAttr>()?;
    module.add_class::<PyNestHostAttr>()?;
    module.add_class::<PyNestMembersAttr>()?;
    module.add_class::<PyRecordAttr>()?;
    module.add_class::<PyPermittedSubclassesAttr>()?;

    Ok(())
}
