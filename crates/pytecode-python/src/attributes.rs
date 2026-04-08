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

#[pyclass(module = "pytecode._rust", name = "TopVariableInfo")]
#[derive(Clone)]
pub(crate) struct PyTopVariableInfo;

#[pymethods]
impl PyTopVariableInfo {
    #[getter]
    fn tag(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::wrap_verification_type(py, pytecode_engine::constants::VerificationType::Top)
    }
}

#[pyclass(module = "pytecode._rust", name = "IntegerVariableInfo")]
#[derive(Clone)]
pub(crate) struct PyIntegerVariableInfo;

#[pymethods]
impl PyIntegerVariableInfo {
    #[getter]
    fn tag(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::wrap_verification_type(py, pytecode_engine::constants::VerificationType::Integer)
    }
}

#[pyclass(module = "pytecode._rust", name = "FloatVariableInfo")]
#[derive(Clone)]
pub(crate) struct PyFloatVariableInfo;

#[pymethods]
impl PyFloatVariableInfo {
    #[getter]
    fn tag(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::wrap_verification_type(py, pytecode_engine::constants::VerificationType::Float)
    }
}

#[pyclass(module = "pytecode._rust", name = "DoubleVariableInfo")]
#[derive(Clone)]
pub(crate) struct PyDoubleVariableInfo;

#[pymethods]
impl PyDoubleVariableInfo {
    #[getter]
    fn tag(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::wrap_verification_type(py, pytecode_engine::constants::VerificationType::Double)
    }
}

#[pyclass(module = "pytecode._rust", name = "LongVariableInfo")]
#[derive(Clone)]
pub(crate) struct PyLongVariableInfo;

#[pymethods]
impl PyLongVariableInfo {
    #[getter]
    fn tag(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::wrap_verification_type(py, pytecode_engine::constants::VerificationType::Long)
    }
}

#[pyclass(module = "pytecode._rust", name = "NullVariableInfo")]
#[derive(Clone)]
pub(crate) struct PyNullVariableInfo;

#[pymethods]
impl PyNullVariableInfo {
    #[getter]
    fn tag(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::wrap_verification_type(py, pytecode_engine::constants::VerificationType::Null)
    }
}

#[pyclass(module = "pytecode._rust", name = "UninitializedThisVariableInfo")]
#[derive(Clone)]
pub(crate) struct PyUninitializedThisVariableInfo;

#[pymethods]
impl PyUninitializedThisVariableInfo {
    #[getter]
    fn tag(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::wrap_verification_type(
            py,
            pytecode_engine::constants::VerificationType::UninitializedThis,
        )
    }
}

#[pyclass(module = "pytecode._rust", name = "ObjectVariableInfo")]
#[derive(Clone)]
pub(crate) struct PyObjectVariableInfo {
    cpool_index: u16,
}

#[pymethods]
impl PyObjectVariableInfo {
    #[getter]
    fn tag(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::wrap_verification_type(py, pytecode_engine::constants::VerificationType::Object)
    }

    #[getter]
    fn cpool_index(&self) -> u16 {
        self.cpool_index
    }
}

#[pyclass(module = "pytecode._rust", name = "UninitializedVariableInfo")]
#[derive(Clone)]
pub(crate) struct PyUninitializedVariableInfo {
    offset: u16,
}

#[pymethods]
impl PyUninitializedVariableInfo {
    #[getter]
    fn tag(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::wrap_verification_type(
            py,
            pytecode_engine::constants::VerificationType::Uninitialized,
        )
    }

    #[getter]
    fn offset(&self) -> u16 {
        self.offset
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
                    cpool_index: *cpool_index,
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

#[pyclass(module = "pytecode._rust", name = "SameFrameInfo")]
#[derive(Clone)]
pub(crate) struct PySameFrameInfo {
    frame_type: u8,
}

#[pymethods]
impl PySameFrameInfo {
    #[getter]
    fn frame_type(&self) -> u8 {
        self.frame_type
    }
}

#[pyclass(module = "pytecode._rust", name = "SameLocals1StackItemFrameInfo")]
#[derive(Clone)]
pub(crate) struct PySameLocals1StackItemFrameInfo {
    frame_type: u8,
    stack: raw::VerificationTypeInfo,
}

#[pymethods]
impl PySameLocals1StackItemFrameInfo {
    #[getter]
    fn frame_type(&self) -> u8 {
        self.frame_type
    }

    #[getter]
    fn stack(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        wrap_verification_type_info(py, &self.stack)
    }
}

#[pyclass(
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
    #[getter]
    fn frame_type(&self) -> u8 {
        self.frame_type
    }

    #[getter]
    fn offset_delta(&self) -> u16 {
        self.offset_delta
    }

    #[getter]
    fn stack(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        wrap_verification_type_info(py, &self.stack)
    }
}

#[pyclass(module = "pytecode._rust", name = "ChopFrameInfo")]
#[derive(Clone)]
pub(crate) struct PyChopFrameInfo {
    frame_type: u8,
    offset_delta: u16,
}

#[pymethods]
impl PyChopFrameInfo {
    #[getter]
    fn frame_type(&self) -> u8 {
        self.frame_type
    }

    #[getter]
    fn offset_delta(&self) -> u16 {
        self.offset_delta
    }
}

#[pyclass(module = "pytecode._rust", name = "SameFrameExtendedInfo")]
#[derive(Clone)]
pub(crate) struct PySameFrameExtendedInfo {
    frame_type: u8,
    offset_delta: u16,
}

#[pymethods]
impl PySameFrameExtendedInfo {
    #[getter]
    fn frame_type(&self) -> u8 {
        self.frame_type
    }

    #[getter]
    fn offset_delta(&self) -> u16 {
        self.offset_delta
    }
}

#[pyclass(module = "pytecode._rust", name = "AppendFrameInfo")]
#[derive(Clone)]
pub(crate) struct PyAppendFrameInfo {
    frame_type: u8,
    offset_delta: u16,
    locals: Vec<raw::VerificationTypeInfo>,
}

#[pymethods]
impl PyAppendFrameInfo {
    #[getter]
    fn frame_type(&self) -> u8 {
        self.frame_type
    }

    #[getter]
    fn offset_delta(&self) -> u16 {
        self.offset_delta
    }

    #[getter]
    fn locals(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.locals
            .iter()
            .map(|v| wrap_verification_type_info(py, v))
            .collect()
    }
}

#[pyclass(module = "pytecode._rust", name = "FullFrameInfo")]
#[derive(Clone)]
pub(crate) struct PyFullFrameInfo {
    frame_type: u8,
    offset_delta: u16,
    locals: Vec<raw::VerificationTypeInfo>,
    stack: Vec<raw::VerificationTypeInfo>,
}

#[pymethods]
impl PyFullFrameInfo {
    #[getter]
    fn frame_type(&self) -> u8 {
        self.frame_type
    }

    #[getter]
    fn offset_delta(&self) -> u16 {
        self.offset_delta
    }

    #[getter]
    fn number_of_locals(&self) -> usize {
        self.locals.len()
    }

    #[getter]
    fn locals(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.locals
            .iter()
            .map(|v| wrap_verification_type_info(py, v))
            .collect()
    }

    #[getter]
    fn number_of_stack_items(&self) -> usize {
        self.stack.len()
    }

    #[getter]
    fn stack(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.stack
            .iter()
            .map(|v| wrap_verification_type_info(py, v))
            .collect()
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

#[pyclass(module = "pytecode._rust", name = "LineNumberInfo")]
#[derive(Clone)]
pub(crate) struct PyLineNumberInfo {
    inner: raw::LineNumberInfo,
}

#[pymethods]
impl PyLineNumberInfo {
    #[getter]
    fn start_pc(&self) -> u16 {
        self.inner.start_pc
    }

    #[getter]
    fn line_number(&self) -> u16 {
        self.inner.line_number
    }
}

#[pyclass(module = "pytecode._rust", name = "LocalVariableInfo")]
#[derive(Clone)]
pub(crate) struct PyLocalVariableInfo {
    inner: raw::LocalVariableInfo,
}

#[pymethods]
impl PyLocalVariableInfo {
    #[getter]
    fn start_pc(&self) -> u16 {
        self.inner.start_pc
    }

    #[getter]
    fn length(&self) -> u16 {
        self.inner.length
    }

    #[getter]
    fn name_index(&self) -> u16 {
        self.inner.name_index
    }

    #[getter]
    fn descriptor_index(&self) -> u16 {
        self.inner.descriptor_index
    }

    #[getter]
    fn index(&self) -> u16 {
        self.inner.index
    }
}

#[pyclass(module = "pytecode._rust", name = "LocalVariableTypeInfo")]
#[derive(Clone)]
pub(crate) struct PyLocalVariableTypeInfo {
    inner: raw::LocalVariableTypeInfo,
}

#[pymethods]
impl PyLocalVariableTypeInfo {
    #[getter]
    fn start_pc(&self) -> u16 {
        self.inner.start_pc
    }

    #[getter]
    fn length(&self) -> u16 {
        self.inner.length
    }

    #[getter]
    fn name_index(&self) -> u16 {
        self.inner.name_index
    }

    #[getter]
    fn signature_index(&self) -> u16 {
        self.inner.signature_index
    }

    #[getter]
    fn index(&self) -> u16 {
        self.inner.index
    }
}

#[pyclass(module = "pytecode._rust", name = "InnerClassInfo")]
#[derive(Clone)]
pub(crate) struct PyInnerClassInfo {
    inner: raw::InnerClassInfo,
}

#[pymethods]
impl PyInnerClassInfo {
    #[getter]
    fn inner_class_info_index(&self) -> u16 {
        self.inner.inner_class_info_index
    }

    #[getter]
    fn outer_class_info_index(&self) -> u16 {
        self.inner.outer_class_info_index
    }

    #[getter]
    fn inner_name_index(&self) -> u16 {
        self.inner.inner_name_index
    }

    #[getter]
    fn inner_class_access_flags(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::wrap_nested_class_access_flags(py, self.inner.inner_class_access_flags)
    }
}

// ===========================================================================
// Annotation sub-types
// ===========================================================================

#[pyclass(module = "pytecode._rust", name = "ConstValueInfo")]
#[derive(Clone)]
pub(crate) struct PyConstValueInfo {
    const_value_index: u16,
}

#[pymethods]
impl PyConstValueInfo {
    #[getter]
    fn const_value_index(&self) -> u16 {
        self.const_value_index
    }
}

#[pyclass(module = "pytecode._rust", name = "EnumConstantValueInfo")]
#[derive(Clone)]
pub(crate) struct PyEnumConstantValueInfo {
    type_name_index: u16,
    const_name_index: u16,
}

#[pymethods]
impl PyEnumConstantValueInfo {
    #[getter]
    fn type_name_index(&self) -> u16 {
        self.type_name_index
    }

    #[getter]
    fn const_name_index(&self) -> u16 {
        self.const_name_index
    }
}

#[pyclass(module = "pytecode._rust", name = "ClassInfoValueInfo")]
#[derive(Clone)]
pub(crate) struct PyClassInfoValueInfo {
    class_info_index: u16,
}

#[pymethods]
impl PyClassInfoValueInfo {
    #[getter]
    fn class_info_index(&self) -> u16 {
        self.class_info_index
    }
}

#[pyclass(module = "pytecode._rust", name = "ArrayValueInfo")]
#[derive(Clone)]
pub(crate) struct PyArrayValueInfo {
    values: Vec<raw::ElementValueInfo>,
}

#[pymethods]
impl PyArrayValueInfo {
    #[getter]
    fn num_values(&self) -> usize {
        self.values.len()
    }

    #[getter]
    fn values(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.values
            .iter()
            .map(|v| wrap_element_value_info(py, v))
            .collect()
    }
}

#[pyclass(module = "pytecode._rust", name = "ElementValueInfo")]
#[derive(Clone)]
pub(crate) struct PyElementValueInfo {
    inner: raw::ElementValueInfo,
}

#[pymethods]
impl PyElementValueInfo {
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

    #[getter]
    fn value(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        match &self.inner {
            raw::ElementValueInfo::Const {
                const_value_index, ..
            } => wrap_pyclass!(
                py,
                PyConstValueInfo {
                    const_value_index: *const_value_index,
                }
            ),
            raw::ElementValueInfo::Enum {
                type_name_index,
                const_name_index,
            } => wrap_pyclass!(
                py,
                PyEnumConstantValueInfo {
                    type_name_index: *type_name_index,
                    const_name_index: *const_name_index,
                }
            ),
            raw::ElementValueInfo::Class { class_info_index } => {
                wrap_pyclass!(
                    py,
                    PyClassInfoValueInfo {
                        class_info_index: *class_info_index,
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
}

fn wrap_element_value_info(py: Python<'_>, value: &raw::ElementValueInfo) -> PyResult<Py<PyAny>> {
    wrap_pyclass!(
        py,
        PyElementValueInfo {
            inner: value.clone(),
        }
    )
}

#[pyclass(module = "pytecode._rust", name = "ElementValuePairInfo")]
#[derive(Clone)]
pub(crate) struct PyElementValuePairInfo {
    inner: raw::ElementValuePairInfo,
}

#[pymethods]
impl PyElementValuePairInfo {
    #[getter]
    fn element_name_index(&self) -> u16 {
        self.inner.element_name_index
    }

    #[getter]
    fn element_value(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        wrap_element_value_info(py, &self.inner.element_value)
    }
}

#[pyclass(module = "pytecode._rust", name = "AnnotationInfo")]
#[derive(Clone)]
pub(crate) struct PyAnnotationInfo {
    inner: raw::AnnotationInfo,
}

#[pymethods]
impl PyAnnotationInfo {
    #[getter]
    fn type_index(&self) -> u16 {
        self.inner.type_index
    }

    #[getter]
    fn num_element_value_pairs(&self) -> usize {
        self.inner.element_value_pairs.len()
    }

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
}

#[pyclass(module = "pytecode._rust", name = "ParameterAnnotationInfo")]
#[derive(Clone)]
pub(crate) struct PyParameterAnnotationInfo {
    inner: raw::ParameterAnnotationInfo,
}

#[pymethods]
impl PyParameterAnnotationInfo {
    #[getter]
    fn num_annotations(&self) -> usize {
        self.inner.annotations.len()
    }

    #[getter]
    fn annotations(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .annotations
            .iter()
            .map(|a| wrap_pyclass!(py, PyAnnotationInfo { inner: a.clone() }))
            .collect()
    }
}

// ===========================================================================
// Type annotation sub-types
// ===========================================================================

#[pyclass(module = "pytecode._rust", name = "TypeParameterTargetInfo")]
#[derive(Clone)]
pub(crate) struct PyTypeParameterTargetInfo {
    type_parameter_index: u8,
}

#[pymethods]
impl PyTypeParameterTargetInfo {
    #[getter]
    fn type_parameter_index(&self) -> u8 {
        self.type_parameter_index
    }
}

#[pyclass(module = "pytecode._rust", name = "SupertypeTargetInfo")]
#[derive(Clone)]
pub(crate) struct PySupertypeTargetInfo {
    supertype_index: u16,
}

#[pymethods]
impl PySupertypeTargetInfo {
    #[getter]
    fn supertype_index(&self) -> u16 {
        self.supertype_index
    }
}

#[pyclass(module = "pytecode._rust", name = "TypeParameterBoundTargetInfo")]
#[derive(Clone)]
pub(crate) struct PyTypeParameterBoundTargetInfo {
    type_parameter_index: u8,
    bound_index: u8,
}

#[pymethods]
impl PyTypeParameterBoundTargetInfo {
    #[getter]
    fn type_parameter_index(&self) -> u8 {
        self.type_parameter_index
    }

    #[getter]
    fn bound_index(&self) -> u8 {
        self.bound_index
    }
}

#[pyclass(module = "pytecode._rust", name = "EmptyTargetInfo")]
#[derive(Clone)]
pub(crate) struct PyEmptyTargetInfo;

#[pyclass(module = "pytecode._rust", name = "FormalParameterTargetInfo")]
#[derive(Clone)]
pub(crate) struct PyFormalParameterTargetInfo {
    formal_parameter_index: u8,
}

#[pymethods]
impl PyFormalParameterTargetInfo {
    #[getter]
    fn formal_parameter_index(&self) -> u8 {
        self.formal_parameter_index
    }
}

#[pyclass(module = "pytecode._rust", name = "ThrowsTargetInfo")]
#[derive(Clone)]
pub(crate) struct PyThrowsTargetInfo {
    throws_type_index: u16,
}

#[pymethods]
impl PyThrowsTargetInfo {
    #[getter]
    fn throws_type_index(&self) -> u16 {
        self.throws_type_index
    }
}

#[pyclass(module = "pytecode._rust", name = "TableInfo")]
#[derive(Clone)]
pub(crate) struct PyTableInfo {
    inner: raw::TableInfo,
}

#[pymethods]
impl PyTableInfo {
    #[getter]
    fn start_pc(&self) -> u16 {
        self.inner.start_pc
    }

    #[getter]
    fn length(&self) -> u16 {
        self.inner.length
    }

    #[getter]
    fn index(&self) -> u16 {
        self.inner.index
    }
}

#[pyclass(module = "pytecode._rust", name = "LocalvarTargetInfo")]
#[derive(Clone)]
pub(crate) struct PyLocalvarTargetInfo {
    table: Vec<raw::TableInfo>,
}

#[pymethods]
impl PyLocalvarTargetInfo {
    #[getter]
    fn table_length(&self) -> usize {
        self.table.len()
    }

    #[getter]
    fn table(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.table
            .iter()
            .map(|t| wrap_pyclass!(py, PyTableInfo { inner: t.clone() }))
            .collect()
    }
}

#[pyclass(module = "pytecode._rust", name = "CatchTargetInfo")]
#[derive(Clone)]
pub(crate) struct PyCatchTargetInfo {
    exception_table_index: u16,
}

#[pymethods]
impl PyCatchTargetInfo {
    #[getter]
    fn exception_table_index(&self) -> u16 {
        self.exception_table_index
    }
}

#[pyclass(module = "pytecode._rust", name = "OffsetTargetInfo")]
#[derive(Clone)]
pub(crate) struct PyOffsetTargetInfo {
    offset: u16,
}

#[pymethods]
impl PyOffsetTargetInfo {
    #[getter]
    fn offset(&self) -> u16 {
        self.offset
    }
}

#[pyclass(module = "pytecode._rust", name = "TypeArgumentTargetInfo")]
#[derive(Clone)]
pub(crate) struct PyTypeArgumentTargetInfo {
    offset: u16,
    type_argument_index: u8,
}

#[pymethods]
impl PyTypeArgumentTargetInfo {
    #[getter]
    fn offset(&self) -> u16 {
        self.offset
    }

    #[getter]
    fn type_argument_index(&self) -> u8 {
        self.type_argument_index
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

#[pyclass(module = "pytecode._rust", name = "PathInfo")]
#[derive(Clone)]
pub(crate) struct PyPathInfo {
    inner: raw::PathInfo,
}

#[pymethods]
impl PyPathInfo {
    #[getter]
    fn type_path_kind(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::wrap_type_path_kind(py, self.inner.type_path_kind)
    }

    #[getter]
    fn type_argument_index(&self) -> u8 {
        self.inner.type_argument_index
    }
}

#[pyclass(module = "pytecode._rust", name = "TypePathInfo")]
#[derive(Clone)]
pub(crate) struct PyTypePathInfo {
    inner: raw::TypePathInfo,
}

#[pymethods]
impl PyTypePathInfo {
    #[getter]
    fn path_length(&self) -> usize {
        self.inner.path.len()
    }

    #[getter]
    fn path(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .path
            .iter()
            .map(|p| wrap_pyclass!(py, PyPathInfo { inner: p.clone() }))
            .collect()
    }
}

#[pyclass(module = "pytecode._rust", name = "TypeAnnotationInfo")]
#[derive(Clone)]
pub(crate) struct PyTypeAnnotationInfo {
    inner: raw::TypeAnnotationInfo,
}

#[pymethods]
impl PyTypeAnnotationInfo {
    #[getter]
    fn target_type(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::wrap_target_type(py, self.inner.target_type)
    }

    #[getter]
    fn target_info(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        wrap_target_info(py, &self.inner.target_info)
    }

    #[getter]
    fn target_path(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        wrap_pyclass!(
            py,
            PyTypePathInfo {
                inner: self.inner.target_path.clone(),
            }
        )
    }

    #[getter]
    fn type_index(&self) -> u16 {
        self.inner.type_index
    }

    #[getter]
    fn num_element_value_pairs(&self) -> usize {
        self.inner.element_value_pairs.len()
    }

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
}

// ===========================================================================
// Bootstrap, method parameter, and module sub-types
// ===========================================================================

#[pyclass(module = "pytecode._rust", name = "BootstrapMethodInfo")]
#[derive(Clone)]
pub(crate) struct PyBootstrapMethodInfo {
    inner: raw::BootstrapMethodInfo,
}

#[pymethods]
impl PyBootstrapMethodInfo {
    #[getter]
    fn bootstrap_method_ref(&self) -> u16 {
        self.inner.bootstrap_method_ref
    }

    // Python field name has typo "boostrap" — kept for compatibility
    #[getter]
    fn num_boostrap_arguments(&self) -> usize {
        self.inner.bootstrap_arguments.len()
    }

    #[getter]
    fn boostrap_arguments(&self) -> Vec<u16> {
        self.inner.bootstrap_arguments.clone()
    }
}

#[pyclass(module = "pytecode._rust", name = "MethodParameterInfo")]
#[derive(Clone)]
pub(crate) struct PyMethodParameterInfo {
    inner: raw::MethodParameterInfo,
}

#[pymethods]
impl PyMethodParameterInfo {
    #[getter]
    fn name_index(&self) -> u16 {
        self.inner.name_index
    }

    #[getter]
    fn access_flags(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::wrap_method_parameter_access_flags(py, self.inner.access_flags)
    }
}

#[pyclass(module = "pytecode._rust", name = "RequiresInfo")]
#[derive(Clone)]
pub(crate) struct PyRequiresInfo {
    inner: raw::RequiresInfo,
}

#[pymethods]
impl PyRequiresInfo {
    #[getter]
    fn requires_index(&self) -> u16 {
        self.inner.requires_index
    }

    // Python field name is "requires_flag" (singular)
    #[getter]
    fn requires_flag(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::wrap_module_requires_access_flags(py, self.inner.requires_flags)
    }

    #[getter]
    fn requires_version_index(&self) -> u16 {
        self.inner.requires_version_index
    }
}

#[pyclass(module = "pytecode._rust", name = "ExportInfo")]
#[derive(Clone)]
pub(crate) struct PyExportInfo {
    inner: raw::ExportInfo,
}

#[pymethods]
impl PyExportInfo {
    #[getter]
    fn exports_index(&self) -> u16 {
        self.inner.exports_index
    }

    #[getter]
    fn exports_flags(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::wrap_module_exports_access_flags(py, self.inner.exports_flags)
    }

    #[getter]
    fn exports_to_count(&self) -> usize {
        self.inner.exports_to_index.len()
    }

    #[getter]
    fn exports_to_index(&self) -> Vec<u16> {
        self.inner.exports_to_index.clone()
    }
}

#[pyclass(module = "pytecode._rust", name = "OpensInfo")]
#[derive(Clone)]
pub(crate) struct PyOpensInfo {
    inner: raw::OpensInfo,
}

#[pymethods]
impl PyOpensInfo {
    #[getter]
    fn opens_index(&self) -> u16 {
        self.inner.opens_index
    }

    #[getter]
    fn opens_flags(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::wrap_module_opens_access_flags(py, self.inner.opens_flags)
    }

    #[getter]
    fn opens_to_count(&self) -> usize {
        self.inner.opens_to_index.len()
    }

    #[getter]
    fn opens_to_index(&self) -> Vec<u16> {
        self.inner.opens_to_index.clone()
    }
}

#[pyclass(module = "pytecode._rust", name = "ProvidesInfo")]
#[derive(Clone)]
pub(crate) struct PyProvidesInfo {
    inner: raw::ProvidesInfo,
}

#[pymethods]
impl PyProvidesInfo {
    #[getter]
    fn provides_index(&self) -> u16 {
        self.inner.provides_index
    }

    #[getter]
    fn provides_with_count(&self) -> usize {
        self.inner.provides_with_index.len()
    }

    #[getter]
    fn provides_with_index(&self) -> Vec<u16> {
        self.inner.provides_with_index.clone()
    }
}

#[pyclass(module = "pytecode._rust", name = "RecordComponentInfo")]
#[derive(Clone)]
pub(crate) struct PyRecordComponentInfo {
    inner: raw::RecordComponentInfo,
}

#[pymethods]
impl PyRecordComponentInfo {
    #[getter]
    fn name_index(&self) -> u16 {
        self.inner.name_index
    }

    #[getter]
    fn descriptor_index(&self) -> u16 {
        self.inner.descriptor_index
    }

    #[getter]
    fn attributes_count(&self) -> usize {
        self.inner.attributes.len()
    }

    #[getter]
    fn attributes(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .attributes
            .iter()
            .map(|attr| crate::wrap_attribute(py, attr))
            .collect()
    }
}

// ===========================================================================
// Top-level attribute types
// ===========================================================================

#[pyclass(module = "pytecode._rust", name = "StackMapTableAttr")]
#[derive(Clone)]
pub(crate) struct PyStackMapTableAttr {
    pub(crate) inner: raw::StackMapTableAttribute,
}

#[pymethods]
impl PyStackMapTableAttr {
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index
    }

    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    #[getter]
    fn number_of_entries(&self) -> usize {
        self.inner.entries.len()
    }

    #[getter]
    fn entries(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .entries
            .iter()
            .map(|e| wrap_stack_map_frame_info(py, e))
            .collect()
    }
}

#[pyclass(module = "pytecode._rust", name = "InnerClassesAttr")]
#[derive(Clone)]
pub(crate) struct PyInnerClassesAttr {
    pub(crate) inner: raw::InnerClassesAttribute,
}

#[pymethods]
impl PyInnerClassesAttr {
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index
    }

    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    #[getter]
    fn number_of_classes(&self) -> usize {
        self.inner.classes.len()
    }

    #[getter]
    fn classes(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .classes
            .iter()
            .map(|c| wrap_pyclass!(py, PyInnerClassInfo { inner: c.clone() }))
            .collect()
    }
}

#[pyclass(module = "pytecode._rust", name = "EnclosingMethodAttr")]
#[derive(Clone)]
pub(crate) struct PyEnclosingMethodAttr {
    pub(crate) inner: raw::EnclosingMethodAttribute,
}

#[pymethods]
impl PyEnclosingMethodAttr {
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index
    }

    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    #[getter]
    fn class_index(&self) -> u16 {
        self.inner.class_index
    }

    #[getter]
    fn method_index(&self) -> u16 {
        self.inner.method_index
    }
}

#[pyclass(module = "pytecode._rust", name = "SyntheticAttr")]
#[derive(Clone)]
pub(crate) struct PySyntheticAttr {
    pub(crate) inner: raw::SyntheticAttribute,
}

#[pymethods]
impl PySyntheticAttr {
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index
    }

    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }
}

#[pyclass(module = "pytecode._rust", name = "DeprecatedAttr")]
#[derive(Clone)]
pub(crate) struct PyDeprecatedAttr {
    pub(crate) inner: raw::DeprecatedAttribute,
}

#[pymethods]
impl PyDeprecatedAttr {
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index
    }

    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }
}

#[pyclass(module = "pytecode._rust", name = "LineNumberTableAttr")]
#[derive(Clone)]
pub(crate) struct PyLineNumberTableAttr {
    pub(crate) inner: raw::LineNumberTableAttribute,
}

#[pymethods]
impl PyLineNumberTableAttr {
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index
    }

    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    #[getter]
    fn line_number_table_length(&self) -> usize {
        self.inner.line_number_table.len()
    }

    #[getter]
    fn line_number_table(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .line_number_table
            .iter()
            .map(|e| wrap_pyclass!(py, PyLineNumberInfo { inner: e.clone() }))
            .collect()
    }
}

#[pyclass(module = "pytecode._rust", name = "LocalVariableTableAttr")]
#[derive(Clone)]
pub(crate) struct PyLocalVariableTableAttr {
    pub(crate) inner: raw::LocalVariableTableAttribute,
}

#[pymethods]
impl PyLocalVariableTableAttr {
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index
    }

    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    #[getter]
    fn local_variable_table_length(&self) -> usize {
        self.inner.local_variable_table.len()
    }

    #[getter]
    fn local_variable_table(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .local_variable_table
            .iter()
            .map(|e| wrap_pyclass!(py, PyLocalVariableInfo { inner: e.clone() }))
            .collect()
    }
}

#[pyclass(module = "pytecode._rust", name = "LocalVariableTypeTableAttr")]
#[derive(Clone)]
pub(crate) struct PyLocalVariableTypeTableAttr {
    pub(crate) inner: raw::LocalVariableTypeTableAttribute,
}

#[pymethods]
impl PyLocalVariableTypeTableAttr {
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index
    }

    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    #[getter]
    fn local_variable_type_table_length(&self) -> usize {
        self.inner.local_variable_type_table.len()
    }

    #[getter]
    fn local_variable_type_table(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .local_variable_type_table
            .iter()
            .map(|e| wrap_pyclass!(py, PyLocalVariableTypeInfo { inner: e.clone() }))
            .collect()
    }
}

#[pyclass(module = "pytecode._rust", name = "RuntimeVisibleAnnotationsAttr")]
#[derive(Clone)]
pub(crate) struct PyRuntimeVisibleAnnotationsAttr {
    pub(crate) inner: raw::RuntimeVisibleAnnotationsAttribute,
}

#[pymethods]
impl PyRuntimeVisibleAnnotationsAttr {
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index
    }

    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    #[getter]
    fn num_annotations(&self) -> usize {
        self.inner.annotations.len()
    }

    #[getter]
    fn annotations(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .annotations
            .iter()
            .map(|a| wrap_pyclass!(py, PyAnnotationInfo { inner: a.clone() }))
            .collect()
    }
}

#[pyclass(module = "pytecode._rust", name = "RuntimeInvisibleAnnotationsAttr")]
#[derive(Clone)]
pub(crate) struct PyRuntimeInvisibleAnnotationsAttr {
    pub(crate) inner: raw::RuntimeInvisibleAnnotationsAttribute,
}

#[pymethods]
impl PyRuntimeInvisibleAnnotationsAttr {
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index
    }

    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    #[getter]
    fn num_annotations(&self) -> usize {
        self.inner.annotations.len()
    }

    #[getter]
    fn annotations(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .annotations
            .iter()
            .map(|a| wrap_pyclass!(py, PyAnnotationInfo { inner: a.clone() }))
            .collect()
    }
}

#[pyclass(
    module = "pytecode._rust",
    name = "RuntimeVisibleParameterAnnotationsAttr"
)]
#[derive(Clone)]
pub(crate) struct PyRuntimeVisibleParameterAnnotationsAttr {
    pub(crate) inner: raw::RuntimeVisibleParameterAnnotationsAttribute,
}

#[pymethods]
impl PyRuntimeVisibleParameterAnnotationsAttr {
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index
    }

    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    #[getter]
    fn num_parameters(&self) -> usize {
        self.inner.parameter_annotations.len()
    }

    #[getter]
    fn parameter_annotations(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .parameter_annotations
            .iter()
            .map(|p| wrap_pyclass!(py, PyParameterAnnotationInfo { inner: p.clone() }))
            .collect()
    }
}

#[pyclass(
    module = "pytecode._rust",
    name = "RuntimeInvisibleParameterAnnotationsAttr"
)]
#[derive(Clone)]
pub(crate) struct PyRuntimeInvisibleParameterAnnotationsAttr {
    pub(crate) inner: raw::RuntimeInvisibleParameterAnnotationsAttribute,
}

#[pymethods]
impl PyRuntimeInvisibleParameterAnnotationsAttr {
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index
    }

    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    #[getter]
    fn num_parameters(&self) -> usize {
        self.inner.parameter_annotations.len()
    }

    #[getter]
    fn parameter_annotations(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .parameter_annotations
            .iter()
            .map(|p| wrap_pyclass!(py, PyParameterAnnotationInfo { inner: p.clone() }))
            .collect()
    }
}

#[pyclass(module = "pytecode._rust", name = "RuntimeVisibleTypeAnnotationsAttr")]
#[derive(Clone)]
pub(crate) struct PyRuntimeVisibleTypeAnnotationsAttr {
    pub(crate) inner: raw::RuntimeVisibleTypeAnnotationsAttribute,
}

#[pymethods]
impl PyRuntimeVisibleTypeAnnotationsAttr {
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index
    }

    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    #[getter]
    fn num_annotations(&self) -> usize {
        self.inner.annotations.len()
    }

    #[getter]
    fn annotations(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .annotations
            .iter()
            .map(|a| wrap_pyclass!(py, PyTypeAnnotationInfo { inner: a.clone() }))
            .collect()
    }
}

#[pyclass(
    module = "pytecode._rust",
    name = "RuntimeInvisibleTypeAnnotationsAttr"
)]
#[derive(Clone)]
pub(crate) struct PyRuntimeInvisibleTypeAnnotationsAttr {
    pub(crate) inner: raw::RuntimeInvisibleTypeAnnotationsAttribute,
}

#[pymethods]
impl PyRuntimeInvisibleTypeAnnotationsAttr {
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index
    }

    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    #[getter]
    fn num_annotations(&self) -> usize {
        self.inner.annotations.len()
    }

    #[getter]
    fn annotations(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .annotations
            .iter()
            .map(|a| wrap_pyclass!(py, PyTypeAnnotationInfo { inner: a.clone() }))
            .collect()
    }
}

#[pyclass(module = "pytecode._rust", name = "AnnotationDefaultAttr")]
#[derive(Clone)]
pub(crate) struct PyAnnotationDefaultAttr {
    pub(crate) inner: raw::AnnotationDefaultAttribute,
}

#[pymethods]
impl PyAnnotationDefaultAttr {
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index
    }

    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    #[getter]
    fn default_value(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        wrap_element_value_info(py, &self.inner.default_value)
    }
}

#[pyclass(module = "pytecode._rust", name = "BootstrapMethodsAttr")]
#[derive(Clone)]
pub(crate) struct PyBootstrapMethodsAttr {
    pub(crate) inner: raw::BootstrapMethodsAttribute,
}

#[pymethods]
impl PyBootstrapMethodsAttr {
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index
    }

    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    #[getter]
    fn num_bootstrap_methods(&self) -> usize {
        self.inner.bootstrap_methods.len()
    }

    #[getter]
    fn bootstrap_methods(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .bootstrap_methods
            .iter()
            .map(|m| wrap_pyclass!(py, PyBootstrapMethodInfo { inner: m.clone() }))
            .collect()
    }
}

#[pyclass(module = "pytecode._rust", name = "MethodParametersAttr")]
#[derive(Clone)]
pub(crate) struct PyMethodParametersAttr {
    pub(crate) inner: raw::MethodParametersAttribute,
}

#[pymethods]
impl PyMethodParametersAttr {
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index
    }

    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    #[getter]
    fn parameters_count(&self) -> usize {
        self.inner.parameters.len()
    }

    #[getter]
    fn parameters(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .parameters
            .iter()
            .map(|p| wrap_pyclass!(py, PyMethodParameterInfo { inner: p.clone() }))
            .collect()
    }
}

#[pyclass(module = "pytecode._rust", name = "ModuleAttr")]
#[derive(Clone)]
pub(crate) struct PyModuleAttr {
    pub(crate) inner: raw::ModuleAttribute,
}

#[pymethods]
impl PyModuleAttr {
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index
    }

    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    #[getter]
    fn module_name_index(&self) -> u16 {
        self.inner.module.module_name_index
    }

    #[getter]
    fn module_flags(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        crate::wrap_module_access_flags(py, self.inner.module.module_flags)
    }

    #[getter]
    fn module_version_index(&self) -> u16 {
        self.inner.module.module_version_index
    }

    #[getter]
    fn requires_count(&self) -> usize {
        self.inner.module.requires.len()
    }

    #[getter]
    fn requires(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .module
            .requires
            .iter()
            .map(|r| wrap_pyclass!(py, PyRequiresInfo { inner: r.clone() }))
            .collect()
    }

    #[getter]
    fn exports_count(&self) -> usize {
        self.inner.module.exports.len()
    }

    #[getter]
    fn exports(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .module
            .exports
            .iter()
            .map(|e| wrap_pyclass!(py, PyExportInfo { inner: e.clone() }))
            .collect()
    }

    #[getter]
    fn opens_count(&self) -> usize {
        self.inner.module.opens.len()
    }

    #[getter]
    fn opens(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .module
            .opens
            .iter()
            .map(|o| wrap_pyclass!(py, PyOpensInfo { inner: o.clone() }))
            .collect()
    }

    #[getter]
    fn uses_count(&self) -> usize {
        self.inner.module.uses_index.len()
    }

    #[getter]
    fn uses_index(&self) -> Vec<u16> {
        self.inner.module.uses_index.clone()
    }

    #[getter]
    fn provides_count(&self) -> usize {
        self.inner.module.provides.len()
    }

    #[getter]
    fn provides(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .module
            .provides
            .iter()
            .map(|p| wrap_pyclass!(py, PyProvidesInfo { inner: p.clone() }))
            .collect()
    }
}

#[pyclass(module = "pytecode._rust", name = "ModulePackagesAttr")]
#[derive(Clone)]
pub(crate) struct PyModulePackagesAttr {
    pub(crate) inner: raw::ModulePackagesAttribute,
}

#[pymethods]
impl PyModulePackagesAttr {
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index
    }

    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    #[getter]
    fn package_count(&self) -> usize {
        self.inner.package_index.len()
    }

    #[getter]
    fn package_index(&self) -> Vec<u16> {
        self.inner.package_index.clone()
    }
}

#[pyclass(module = "pytecode._rust", name = "ModuleMainClassAttr")]
#[derive(Clone)]
pub(crate) struct PyModuleMainClassAttr {
    pub(crate) inner: raw::ModuleMainClassAttribute,
}

#[pymethods]
impl PyModuleMainClassAttr {
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index
    }

    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    #[getter]
    fn main_class_index(&self) -> u16 {
        self.inner.main_class_index
    }
}

#[pyclass(module = "pytecode._rust", name = "NestHostAttr")]
#[derive(Clone)]
pub(crate) struct PyNestHostAttr {
    pub(crate) inner: raw::NestHostAttribute,
}

#[pymethods]
impl PyNestHostAttr {
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index
    }

    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    #[getter]
    fn host_class_index(&self) -> u16 {
        self.inner.host_class_index
    }
}

#[pyclass(module = "pytecode._rust", name = "NestMembersAttr")]
#[derive(Clone)]
pub(crate) struct PyNestMembersAttr {
    pub(crate) inner: raw::NestMembersAttribute,
}

#[pymethods]
impl PyNestMembersAttr {
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index
    }

    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    #[getter]
    fn number_of_classes(&self) -> usize {
        self.inner.classes.len()
    }

    #[getter]
    fn classes(&self) -> Vec<u16> {
        self.inner.classes.clone()
    }
}

#[pyclass(module = "pytecode._rust", name = "RecordAttr")]
#[derive(Clone)]
pub(crate) struct PyRecordAttr {
    pub(crate) inner: raw::RecordAttribute,
}

#[pymethods]
impl PyRecordAttr {
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index
    }

    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    #[getter]
    fn components_count(&self) -> usize {
        self.inner.components.len()
    }

    #[getter]
    fn components(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .components
            .iter()
            .map(|c| wrap_pyclass!(py, PyRecordComponentInfo { inner: c.clone() }))
            .collect()
    }
}

#[pyclass(module = "pytecode._rust", name = "PermittedSubclassesAttr")]
#[derive(Clone)]
pub(crate) struct PyPermittedSubclassesAttr {
    pub(crate) inner: raw::PermittedSubclassesAttribute,
}

#[pymethods]
impl PyPermittedSubclassesAttr {
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index
    }

    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    #[getter]
    fn number_of_classes(&self) -> usize {
        self.inner.classes.len()
    }

    #[getter]
    fn classes(&self) -> Vec<u16> {
        self.inner.classes.clone()
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
