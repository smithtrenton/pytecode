use pyo3::create_exception;
use pyo3::exceptions::PyOSError;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes, PyModule, PyType};
use pytecode_engine::raw;
use pytecode_engine::raw::ClassFile;
use pytecode_engine::{parse_class, write_class};
use std::fs;
use std::path::PathBuf;

create_exception!(
    pytecode,
    MalformedClassException,
    pyo3::exceptions::PyException
);

fn engine_error_to_py(error: pytecode_engine::EngineError) -> PyErr {
    MalformedClassException::new_err(error.to_string())
}

macro_rules! wrap_pyclass {
    ($py:expr, $value:expr) => {
        Py::new($py, $value).map(|obj| obj.into_bound($py).into_any().unbind())
    };
}

fn python_enum(
    py: Python<'_>,
    module_name: &str,
    class_name: &str,
    value: impl Into<i64>,
) -> PyResult<Py<PyAny>> {
    let module = PyModule::import(py, module_name)?;
    let enum_type = module.getattr(class_name)?;
    Ok(enum_type.call1((value.into(),))?.unbind())
}

fn wrap_class_access_flags(
    py: Python<'_>,
    flags: pytecode_engine::constants::ClassAccessFlags,
) -> PyResult<Py<PyAny>> {
    python_enum(
        py,
        "pytecode.classfile.constants",
        "ClassAccessFlag",
        i64::from(flags.bits()),
    )
}

fn wrap_field_access_flags(
    py: Python<'_>,
    flags: pytecode_engine::constants::FieldAccessFlags,
) -> PyResult<Py<PyAny>> {
    python_enum(
        py,
        "pytecode.classfile.constants",
        "FieldAccessFlag",
        i64::from(flags.bits()),
    )
}

fn wrap_method_access_flags(
    py: Python<'_>,
    flags: pytecode_engine::constants::MethodAccessFlags,
) -> PyResult<Py<PyAny>> {
    python_enum(
        py,
        "pytecode.classfile.constants",
        "MethodAccessFlag",
        i64::from(flags.bits()),
    )
}

fn wrap_array_type(py: Python<'_>, atype: raw::ArrayType) -> PyResult<Py<PyAny>> {
    python_enum(
        py,
        "pytecode.classfile.instructions",
        "ArrayType",
        i64::from(atype as u8),
    )
}

fn instruction_type_value(instruction: &raw::Instruction) -> i64 {
    match instruction {
        raw::Instruction::Wide(inner) => i64::from(0xC4_u16 + u16::from(inner.opcode)),
        _ => i64::from(instruction.opcode()),
    }
}

fn wrap_instruction_type(py: Python<'_>, instruction: &raw::Instruction) -> PyResult<Py<PyAny>> {
    python_enum(
        py,
        "pytecode.classfile.instructions",
        "InsnInfoType",
        instruction_type_value(instruction),
    )
}

macro_rules! define_numeric_cp_wrapper {
    ($wrapper:ident, $pyname:literal, $inner:ty, $tag:expr, { $($field:ident : $ty:ty),* $(,)? }) => {
        #[pyclass(module = "pytecode._rust", name = $pyname)]
        #[derive(Clone)]
        pub struct $wrapper {
            index: usize,
            inner: $inner,
        }

        #[pymethods]
        impl $wrapper {
            #[getter]
            fn index(&self) -> usize {
                self.index
            }

            #[getter]
            fn offset(&self) -> Option<usize> {
                None
            }

            #[getter]
            fn tag(&self) -> u8 {
                $tag as u8
            }

            $(
                #[getter]
                fn $field(&self) -> $ty {
                    self.inner.$field
                }
            )*
        }
    };
}

#[pyclass(module = "pytecode._rust", name = "Utf8Info")]
#[derive(Clone)]
pub struct PyUtf8Info {
    index: usize,
    inner: raw::Utf8Info,
}

#[pymethods]
impl PyUtf8Info {
    #[getter]
    fn index(&self) -> usize {
        self.index
    }

    #[getter]
    fn offset(&self) -> Option<usize> {
        None
    }

    #[getter]
    fn tag(&self) -> u8 {
        raw::ConstantPoolTag::Utf8 as u8
    }

    #[getter]
    fn length(&self) -> usize {
        self.inner.bytes.len()
    }

    #[getter]
    fn str_bytes<'py>(&self, py: Python<'py>) -> Py<PyBytes> {
        PyBytes::new(py, &self.inner.bytes).unbind()
    }

    fn __repr__(&self) -> String {
        format!(
            "Utf8Info(index={}, length={})",
            self.index,
            self.inner.bytes.len()
        )
    }
}

define_numeric_cp_wrapper!(
    PyIntegerInfo,
    "IntegerInfo",
    raw::IntegerInfo,
    raw::ConstantPoolTag::Integer,
    { value_bytes: u32 }
);
define_numeric_cp_wrapper!(
    PyFloatInfo,
    "FloatInfo",
    raw::FloatInfo,
    raw::ConstantPoolTag::Float,
    { value_bytes: u32 }
);
define_numeric_cp_wrapper!(
    PyLongInfo,
    "LongInfo",
    raw::LongInfo,
    raw::ConstantPoolTag::Long,
    { high_bytes: u32, low_bytes: u32 }
);
define_numeric_cp_wrapper!(
    PyDoubleInfo,
    "DoubleInfo",
    raw::DoubleInfo,
    raw::ConstantPoolTag::Double,
    { high_bytes: u32, low_bytes: u32 }
);
define_numeric_cp_wrapper!(
    PyConstantPoolClassInfo,
    "ClassInfo",
    raw::ClassInfo,
    raw::ConstantPoolTag::Class,
    { name_index: u16 }
);
define_numeric_cp_wrapper!(
    PyStringInfo,
    "StringInfo",
    raw::StringInfo,
    raw::ConstantPoolTag::String,
    { string_index: u16 }
);
define_numeric_cp_wrapper!(
    PyFieldrefInfo,
    "FieldrefInfo",
    raw::FieldRefInfo,
    raw::ConstantPoolTag::FieldRef,
    { class_index: u16, name_and_type_index: u16 }
);
define_numeric_cp_wrapper!(
    PyMethodrefInfo,
    "MethodrefInfo",
    raw::MethodRefInfo,
    raw::ConstantPoolTag::MethodRef,
    { class_index: u16, name_and_type_index: u16 }
);
define_numeric_cp_wrapper!(
    PyInterfaceMethodrefInfo,
    "InterfaceMethodrefInfo",
    raw::InterfaceMethodRefInfo,
    raw::ConstantPoolTag::InterfaceMethodRef,
    { class_index: u16, name_and_type_index: u16 }
);
define_numeric_cp_wrapper!(
    PyNameAndTypeInfo,
    "NameAndTypeInfo",
    raw::NameAndTypeInfo,
    raw::ConstantPoolTag::NameAndType,
    { name_index: u16, descriptor_index: u16 }
);
define_numeric_cp_wrapper!(
    PyMethodHandleInfo,
    "MethodHandleInfo",
    raw::MethodHandleInfo,
    raw::ConstantPoolTag::MethodHandle,
    { reference_kind: u8, reference_index: u16 }
);
define_numeric_cp_wrapper!(
    PyMethodTypeInfo,
    "MethodTypeInfo",
    raw::MethodTypeInfo,
    raw::ConstantPoolTag::MethodType,
    { descriptor_index: u16 }
);
define_numeric_cp_wrapper!(
    PyDynamicInfo,
    "DynamicInfo",
    raw::DynamicInfo,
    raw::ConstantPoolTag::Dynamic,
    { bootstrap_method_attr_index: u16, name_and_type_index: u16 }
);
define_numeric_cp_wrapper!(
    PyInvokeDynamicInfo,
    "InvokeDynamicInfo",
    raw::InvokeDynamicInfo,
    raw::ConstantPoolTag::InvokeDynamic,
    { bootstrap_method_attr_index: u16, name_and_type_index: u16 }
);
define_numeric_cp_wrapper!(
    PyModuleInfo,
    "ModuleInfo",
    raw::ModuleInfo,
    raw::ConstantPoolTag::Module,
    { name_index: u16 }
);
define_numeric_cp_wrapper!(
    PyPackageInfo,
    "PackageInfo",
    raw::PackageInfo,
    raw::ConstantPoolTag::Package,
    { name_index: u16 }
);

#[pyclass(module = "pytecode._rust", name = "MatchOffsetPair")]
#[derive(Clone)]
pub struct PyMatchOffsetPair {
    inner: raw::MatchOffsetPair,
}

#[pymethods]
impl PyMatchOffsetPair {
    #[getter]
    fn r#match(&self) -> i32 {
        self.inner.match_value
    }

    #[getter]
    fn offset(&self) -> i32 {
        self.inner.offset
    }
}

#[pyclass(module = "pytecode._rust", name = "InsnInfo")]
#[derive(Clone)]
pub struct PyInsnInfo {
    inner: raw::Instruction,
}

#[pymethods]
impl PyInsnInfo {
    #[getter]
    fn r#type(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        wrap_instruction_type(py, &self.inner)
    }

    #[getter]
    fn bytecode_offset(&self) -> u32 {
        self.inner.offset()
    }

    #[getter]
    fn opcode(&self) -> u8 {
        self.inner.opcode()
    }

    #[getter]
    fn index(&self) -> Option<u16> {
        match &self.inner {
            raw::Instruction::LocalIndex { index, .. }
            | raw::Instruction::ConstantPoolIndex1 { index, .. } => Some(u16::from(*index)),
            raw::Instruction::ConstantPoolIndexWide(inner) => Some(inner.index),
            raw::Instruction::IInc { index, .. } => Some(u16::from(*index)),
            raw::Instruction::InvokeDynamic(inner) => Some(inner.index),
            raw::Instruction::InvokeInterface(inner) => Some(inner.index),
            raw::Instruction::MultiANewArray { index, .. } => Some(*index),
            raw::Instruction::Wide(inner) => Some(inner.index),
            _ => None,
        }
    }

    #[getter]
    fn value(&self) -> Option<i16> {
        match &self.inner {
            raw::Instruction::Byte { value, .. } => Some(i16::from(*value)),
            raw::Instruction::Short { value, .. } => Some(*value),
            raw::Instruction::IInc { value, .. } => Some(i16::from(*value)),
            raw::Instruction::Wide(inner) => inner.value,
            _ => None,
        }
    }

    #[getter]
    fn branch_offset(&self) -> Option<i32> {
        match &self.inner {
            raw::Instruction::Branch(inner) => Some(i32::from(inner.branch_offset)),
            raw::Instruction::BranchWide { branch_offset, .. } => Some(*branch_offset),
            _ => None,
        }
    }

    #[getter]
    fn count(&self) -> Option<u8> {
        match &self.inner {
            raw::Instruction::InvokeInterface(inner) => Some(inner.count),
            _ => None,
        }
    }

    #[getter]
    fn reserved<'py>(&self, py: Python<'py>) -> Option<Py<PyBytes>> {
        match &self.inner {
            raw::Instruction::InvokeDynamic(inner) => {
                Some(PyBytes::new(py, &inner.reserved.to_be_bytes()).unbind())
            }
            raw::Instruction::InvokeInterface(inner) => {
                Some(PyBytes::new(py, &[inner.reserved]).unbind())
            }
            _ => None,
        }
    }

    #[getter]
    fn atype(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        match &self.inner {
            raw::Instruction::NewArray(inner) => wrap_array_type(py, inner.atype).map(Some),
            _ => Ok(None),
        }
    }

    #[getter]
    fn dimensions(&self) -> Option<u8> {
        match &self.inner {
            raw::Instruction::MultiANewArray { dimensions, .. } => Some(*dimensions),
            _ => None,
        }
    }

    #[getter]
    fn default(&self) -> Option<i32> {
        match &self.inner {
            raw::Instruction::LookupSwitch(inner) => Some(inner.default_offset),
            raw::Instruction::TableSwitch(inner) => Some(inner.default_offset),
            _ => None,
        }
    }

    #[getter]
    fn npairs(&self) -> Option<usize> {
        match &self.inner {
            raw::Instruction::LookupSwitch(inner) => Some(inner.pairs.len()),
            _ => None,
        }
    }

    #[getter]
    fn pairs(&self) -> Option<Vec<PyMatchOffsetPair>> {
        match &self.inner {
            raw::Instruction::LookupSwitch(inner) => Some(
                inner
                    .pairs
                    .iter()
                    .cloned()
                    .map(|pair| PyMatchOffsetPair { inner: pair })
                    .collect(),
            ),
            _ => None,
        }
    }

    #[getter]
    fn low(&self) -> Option<i32> {
        match &self.inner {
            raw::Instruction::TableSwitch(inner) => Some(inner.low),
            _ => None,
        }
    }

    #[getter]
    fn high(&self) -> Option<i32> {
        match &self.inner {
            raw::Instruction::TableSwitch(inner) => Some(inner.high),
            _ => None,
        }
    }

    #[getter]
    fn offsets(&self) -> Option<Vec<i32>> {
        match &self.inner {
            raw::Instruction::TableSwitch(inner) => Some(inner.offsets.clone()),
            _ => None,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "InsnInfo(type={}, bytecode_offset={}, opcode=0x{:02x})",
            instruction_type_value(&self.inner),
            self.inner.offset(),
            self.inner.opcode()
        )
    }
}

#[pyclass(module = "pytecode._rust", name = "ExceptionInfo")]
#[derive(Clone)]
pub struct PyExceptionInfo {
    inner: raw::ExceptionHandler,
}

#[pymethods]
impl PyExceptionInfo {
    #[getter]
    fn start_pc(&self) -> u16 {
        self.inner.start_pc
    }

    #[getter]
    fn end_pc(&self) -> u16 {
        self.inner.end_pc
    }

    #[getter]
    fn handler_pc(&self) -> u16 {
        self.inner.handler_pc
    }

    #[getter]
    fn catch_type(&self) -> u16 {
        self.inner.catch_type
    }
}

#[pyclass(module = "pytecode._rust", name = "ConstantValueAttr")]
#[derive(Clone)]
pub struct PyConstantValueAttr {
    inner: raw::ConstantValueAttribute,
}

#[pymethods]
impl PyConstantValueAttr {
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index
    }

    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    #[getter]
    fn constantvalue_index(&self) -> u16 {
        self.inner.constantvalue_index
    }
}

#[pyclass(module = "pytecode._rust", name = "SignatureAttr")]
#[derive(Clone)]
pub struct PySignatureAttr {
    inner: raw::SignatureAttribute,
}

#[pymethods]
impl PySignatureAttr {
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index
    }

    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    #[getter]
    fn signature_index(&self) -> u16 {
        self.inner.signature_index
    }
}

#[pyclass(module = "pytecode._rust", name = "SourceFileAttr")]
#[derive(Clone)]
pub struct PySourceFileAttr {
    inner: raw::SourceFileAttribute,
}

#[pymethods]
impl PySourceFileAttr {
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index
    }

    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    #[getter]
    fn sourcefile_index(&self) -> u16 {
        self.inner.sourcefile_index
    }
}

#[pyclass(module = "pytecode._rust", name = "SourceDebugExtensionAttr")]
#[derive(Clone)]
pub struct PySourceDebugExtensionAttr {
    inner: raw::SourceDebugExtensionAttribute,
}

#[pymethods]
impl PySourceDebugExtensionAttr {
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index
    }

    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    #[getter]
    fn debug_extension<'py>(&self, py: Python<'py>) -> Py<PyBytes> {
        PyBytes::new(py, &self.inner.debug_extension).unbind()
    }
}

#[pyclass(module = "pytecode._rust", name = "ExceptionsAttr")]
#[derive(Clone)]
pub struct PyExceptionsAttr {
    inner: raw::ExceptionsAttribute,
}

#[pymethods]
impl PyExceptionsAttr {
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index
    }

    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    #[getter]
    fn number_of_exceptions(&self) -> usize {
        self.inner.exception_index_table.len()
    }

    #[getter]
    fn exception_index_table(&self) -> Vec<u16> {
        self.inner.exception_index_table.clone()
    }
}

#[pyclass(module = "pytecode._rust", name = "CodeAttr")]
#[derive(Clone)]
pub struct PyCodeAttr {
    inner: raw::CodeAttribute,
}

#[pymethods]
impl PyCodeAttr {
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index
    }

    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    #[getter]
    fn max_stack(&self) -> u16 {
        self.inner.max_stack
    }

    #[getter]
    fn max_stacks(&self) -> u16 {
        self.inner.max_stack
    }

    #[getter]
    fn max_locals(&self) -> u16 {
        self.inner.max_locals
    }

    #[getter]
    fn code_length(&self) -> u32 {
        self.inner.code_length
    }

    #[getter]
    fn code(&self) -> Vec<PyInsnInfo> {
        self.inner
            .code
            .iter()
            .cloned()
            .map(|inner| PyInsnInfo { inner })
            .collect()
    }

    #[getter]
    fn exception_table_length(&self) -> usize {
        self.inner.exception_table.len()
    }

    #[getter]
    fn exception_table(&self) -> Vec<PyExceptionInfo> {
        self.inner
            .exception_table
            .iter()
            .cloned()
            .map(|inner| PyExceptionInfo { inner })
            .collect()
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
            .map(|attribute| wrap_attribute(py, attribute))
            .collect()
    }
}

#[pyclass(module = "pytecode._rust", name = "UnimplementedAttr")]
#[derive(Clone)]
pub struct PyUnimplementedAttr {
    inner: raw::UnknownAttribute,
}

#[pymethods]
impl PyUnimplementedAttr {
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index
    }

    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    #[getter]
    fn name(&self) -> String {
        self.inner.name.clone()
    }

    #[getter]
    fn info<'py>(&self, py: Python<'py>) -> Py<PyBytes> {
        PyBytes::new(py, &self.inner.info).unbind()
    }
}

fn wrap_constant_pool_entry(
    py: Python<'_>,
    index: usize,
    entry: &raw::ConstantPoolEntry,
) -> PyResult<Py<PyAny>> {
    match entry {
        raw::ConstantPoolEntry::Utf8(inner) => wrap_pyclass!(
            py,
            PyUtf8Info {
                index,
                inner: inner.clone(),
            }
        ),
        raw::ConstantPoolEntry::Integer(inner) => wrap_pyclass!(
            py,
            PyIntegerInfo {
                index,
                inner: inner.clone(),
            }
        ),
        raw::ConstantPoolEntry::Float(inner) => wrap_pyclass!(
            py,
            PyFloatInfo {
                index,
                inner: inner.clone(),
            }
        ),
        raw::ConstantPoolEntry::Long(inner) => wrap_pyclass!(
            py,
            PyLongInfo {
                index,
                inner: inner.clone(),
            }
        ),
        raw::ConstantPoolEntry::Double(inner) => wrap_pyclass!(
            py,
            PyDoubleInfo {
                index,
                inner: inner.clone(),
            }
        ),
        raw::ConstantPoolEntry::Class(inner) => wrap_pyclass!(
            py,
            PyConstantPoolClassInfo {
                index,
                inner: inner.clone(),
            }
        ),
        raw::ConstantPoolEntry::String(inner) => wrap_pyclass!(
            py,
            PyStringInfo {
                index,
                inner: inner.clone(),
            }
        ),
        raw::ConstantPoolEntry::FieldRef(inner) => wrap_pyclass!(
            py,
            PyFieldrefInfo {
                index,
                inner: inner.clone(),
            }
        ),
        raw::ConstantPoolEntry::MethodRef(inner) => wrap_pyclass!(
            py,
            PyMethodrefInfo {
                index,
                inner: inner.clone(),
            }
        ),
        raw::ConstantPoolEntry::InterfaceMethodRef(inner) => wrap_pyclass!(
            py,
            PyInterfaceMethodrefInfo {
                index,
                inner: inner.clone(),
            }
        ),
        raw::ConstantPoolEntry::NameAndType(inner) => wrap_pyclass!(
            py,
            PyNameAndTypeInfo {
                index,
                inner: inner.clone(),
            }
        ),
        raw::ConstantPoolEntry::MethodHandle(inner) => wrap_pyclass!(
            py,
            PyMethodHandleInfo {
                index,
                inner: inner.clone(),
            }
        ),
        raw::ConstantPoolEntry::MethodType(inner) => wrap_pyclass!(
            py,
            PyMethodTypeInfo {
                index,
                inner: inner.clone(),
            }
        ),
        raw::ConstantPoolEntry::Dynamic(inner) => wrap_pyclass!(
            py,
            PyDynamicInfo {
                index,
                inner: inner.clone(),
            }
        ),
        raw::ConstantPoolEntry::InvokeDynamic(inner) => wrap_pyclass!(
            py,
            PyInvokeDynamicInfo {
                index,
                inner: inner.clone(),
            }
        ),
        raw::ConstantPoolEntry::Module(inner) => wrap_pyclass!(
            py,
            PyModuleInfo {
                index,
                inner: inner.clone(),
            }
        ),
        raw::ConstantPoolEntry::Package(inner) => wrap_pyclass!(
            py,
            PyPackageInfo {
                index,
                inner: inner.clone(),
            }
        ),
    }
}

fn wrap_attribute(py: Python<'_>, attribute: &raw::AttributeInfo) -> PyResult<Py<PyAny>> {
    match attribute {
        raw::AttributeInfo::ConstantValue(inner) => wrap_pyclass!(
            py,
            PyConstantValueAttr {
                inner: inner.clone(),
            }
        ),
        raw::AttributeInfo::Signature(inner) => wrap_pyclass!(
            py,
            PySignatureAttr {
                inner: inner.clone(),
            }
        ),
        raw::AttributeInfo::SourceFile(inner) => wrap_pyclass!(
            py,
            PySourceFileAttr {
                inner: inner.clone(),
            }
        ),
        raw::AttributeInfo::SourceDebugExtension(inner) => wrap_pyclass!(
            py,
            PySourceDebugExtensionAttr {
                inner: inner.clone(),
            }
        ),
        raw::AttributeInfo::Exceptions(inner) => wrap_pyclass!(
            py,
            PyExceptionsAttr {
                inner: inner.clone(),
            }
        ),
        raw::AttributeInfo::Code(inner) => wrap_pyclass!(
            py,
            PyCodeAttr {
                inner: inner.clone(),
            }
        ),
        raw::AttributeInfo::Unknown(inner) => wrap_pyclass!(
            py,
            PyUnimplementedAttr {
                inner: inner.clone(),
            }
        ),
    }
}

#[pyclass(module = "pytecode._rust", name = "FieldInfo")]
#[derive(Clone)]
pub struct PyFieldInfo {
    inner: raw::FieldInfo,
}

#[pymethods]
impl PyFieldInfo {
    #[getter]
    fn access_flags(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        wrap_field_access_flags(py, self.inner.access_flags)
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
    fn attributes_count(&self) -> usize {
        self.inner.attributes.len()
    }

    #[getter]
    fn attributes(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .attributes
            .iter()
            .map(|attribute| wrap_attribute(py, attribute))
            .collect()
    }
}

#[pyclass(module = "pytecode._rust", name = "MethodInfo")]
#[derive(Clone)]
pub struct PyMethodInfo {
    inner: raw::MethodInfo,
}

#[pymethods]
impl PyMethodInfo {
    #[getter]
    fn access_flags(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        wrap_method_access_flags(py, self.inner.access_flags)
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
    fn attributes_count(&self) -> usize {
        self.inner.attributes.len()
    }

    #[getter]
    fn attributes(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .attributes
            .iter()
            .map(|attribute| wrap_attribute(py, attribute))
            .collect()
    }
}

#[pyclass(module = "pytecode._rust", name = "ClassFile")]
#[derive(Clone)]
pub struct PyClassFile {
    inner: ClassFile,
}

#[pymethods]
impl PyClassFile {
    #[getter]
    fn magic(&self) -> u32 {
        self.inner.magic
    }

    #[getter]
    fn minor_version(&self) -> u16 {
        self.inner.minor_version
    }

    #[getter]
    fn major_version(&self) -> u16 {
        self.inner.major_version
    }

    #[getter]
    fn constant_pool_count(&self) -> usize {
        self.inner.constant_pool.len()
    }

    #[getter]
    fn constant_pool(&self, py: Python<'_>) -> PyResult<Vec<Option<Py<PyAny>>>> {
        self.inner
            .constant_pool
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                entry
                    .as_ref()
                    .map(|entry| wrap_constant_pool_entry(py, index, entry))
                    .transpose()
            })
            .collect()
    }

    #[getter]
    fn access_flags(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        wrap_class_access_flags(py, self.inner.access_flags)
    }

    #[getter]
    fn this_class(&self) -> u16 {
        self.inner.this_class
    }

    #[getter]
    fn super_class(&self) -> u16 {
        self.inner.super_class
    }

    #[getter]
    fn interfaces_count(&self) -> usize {
        self.inner.interfaces.len()
    }

    #[getter]
    fn interface_count(&self) -> usize {
        self.inner.interfaces.len()
    }

    #[getter]
    fn interfaces(&self) -> Vec<u16> {
        self.inner.interfaces.clone()
    }

    #[getter]
    fn fields_count(&self) -> usize {
        self.inner.fields.len()
    }

    #[getter]
    fn field_count(&self) -> usize {
        self.inner.fields.len()
    }

    #[getter]
    fn fields(&self) -> Vec<PyFieldInfo> {
        self.inner
            .fields
            .iter()
            .cloned()
            .map(|inner| PyFieldInfo { inner })
            .collect()
    }

    #[getter]
    fn methods_count(&self) -> usize {
        self.inner.methods.len()
    }

    #[getter]
    fn method_count(&self) -> usize {
        self.inner.methods.len()
    }

    #[getter]
    fn methods(&self) -> Vec<PyMethodInfo> {
        self.inner
            .methods
            .iter()
            .cloned()
            .map(|inner| PyMethodInfo { inner })
            .collect()
    }

    #[getter]
    fn attributes_count(&self) -> usize {
        self.inner.attributes.len()
    }

    #[getter]
    fn attribute_count(&self) -> usize {
        self.inner.attributes.len()
    }

    #[getter]
    fn attributes(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .attributes
            .iter()
            .map(|attribute| wrap_attribute(py, attribute))
            .collect()
    }

    fn to_bytes<'py>(&self, py: Python<'py>) -> PyResult<Py<PyBytes>> {
        let bytes = write_class(&self.inner).map_err(engine_error_to_py)?;
        Ok(PyBytes::new(py, &bytes).unbind())
    }

    fn __repr__(&self) -> String {
        format!(
            "ClassFile(major_version={}, minor_version={}, methods={}, fields={})",
            self.inner.major_version,
            self.inner.minor_version,
            self.inner.methods.len(),
            self.inner.fields.len()
        )
    }
}

#[pyclass(module = "pytecode._rust", name = "ClassReader")]
pub struct PyClassReader {
    class_info: ClassFile,
}

#[pymethods]
impl PyClassReader {
    #[new]
    fn new(bytes_or_bytearray: &[u8]) -> PyResult<Self> {
        let class_info = parse_class(bytes_or_bytearray).map_err(engine_error_to_py)?;
        Ok(Self { class_info })
    }

    #[classmethod]
    fn from_bytes(_cls: &Bound<'_, PyType>, bytes_or_bytearray: &[u8]) -> PyResult<Self> {
        Self::new(bytes_or_bytearray)
    }

    #[classmethod]
    fn from_file(_cls: &Bound<'_, PyType>, path: PathBuf) -> PyResult<Self> {
        let bytes = fs::read(&path).map_err(PyOSError::new_err)?;
        Self::new(&bytes)
    }

    #[getter]
    fn class_info(&self) -> PyClassFile {
        PyClassFile {
            inner: self.class_info.clone(),
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "ClassReader(major_version={}, minor_version={})",
            self.class_info.major_version, self.class_info.minor_version
        )
    }
}

#[pyclass(module = "pytecode._rust", name = "ClassWriter")]
pub struct PyClassWriter;

#[pymethods]
impl PyClassWriter {
    #[staticmethod]
    fn write<'py>(py: Python<'py>, classfile: &PyClassFile) -> PyResult<Py<PyBytes>> {
        let bytes = write_class(&classfile.inner).map_err(engine_error_to_py)?;
        Ok(PyBytes::new(py, &bytes).unbind())
    }
}

#[pyfunction]
fn backend_info() -> (&'static str, &'static str, Vec<&'static str>) {
    (
        "pytecode._rust",
        env!("CARGO_PKG_VERSION"),
        vec![
            "ClassFile",
            "ClassReader",
            "ClassWriter",
            "FieldInfo",
            "MethodInfo",
            "InsnInfo",
            "CodeAttr",
        ],
    )
}

#[pymodule]
fn _rust(py: Python<'_>, module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add(
        "MalformedClassException",
        py.get_type::<MalformedClassException>(),
    )?;
    module.add_class::<PyUtf8Info>()?;
    module.add_class::<PyIntegerInfo>()?;
    module.add_class::<PyFloatInfo>()?;
    module.add_class::<PyLongInfo>()?;
    module.add_class::<PyDoubleInfo>()?;
    module.add_class::<PyConstantPoolClassInfo>()?;
    module.add_class::<PyStringInfo>()?;
    module.add_class::<PyFieldrefInfo>()?;
    module.add_class::<PyMethodrefInfo>()?;
    module.add_class::<PyInterfaceMethodrefInfo>()?;
    module.add_class::<PyNameAndTypeInfo>()?;
    module.add_class::<PyMethodHandleInfo>()?;
    module.add_class::<PyMethodTypeInfo>()?;
    module.add_class::<PyDynamicInfo>()?;
    module.add_class::<PyInvokeDynamicInfo>()?;
    module.add_class::<PyModuleInfo>()?;
    module.add_class::<PyPackageInfo>()?;
    module.add_class::<PyMatchOffsetPair>()?;
    module.add_class::<PyInsnInfo>()?;
    module.add_class::<PyExceptionInfo>()?;
    module.add_class::<PyConstantValueAttr>()?;
    module.add_class::<PySignatureAttr>()?;
    module.add_class::<PySourceFileAttr>()?;
    module.add_class::<PySourceDebugExtensionAttr>()?;
    module.add_class::<PyExceptionsAttr>()?;
    module.add_class::<PyCodeAttr>()?;
    module.add_class::<PyUnimplementedAttr>()?;
    module.add_class::<PyFieldInfo>()?;
    module.add_class::<PyMethodInfo>()?;
    module.add_class::<PyClassFile>()?;
    module.add_class::<PyClassReader>()?;
    module.add_class::<PyClassWriter>()?;
    module.add_function(wrap_pyfunction!(backend_info, module)?)?;
    module.add(
        "__all__",
        vec![
            "MalformedClassException",
            "ClassFile",
            "ClassReader",
            "ClassWriter",
            "FieldInfo",
            "MethodInfo",
            "InsnInfo",
            "CodeAttr",
            "Utf8Info",
            "FieldrefInfo",
            "MethodrefInfo",
            "InterfaceMethodrefInfo",
            "backend_info",
        ],
    )?;
    Ok(())
}
