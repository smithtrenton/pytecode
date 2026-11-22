use pyo3::create_exception;
use pyo3::exceptions::{PyOSError, PyTypeError};
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyAny, PyByteArray, PyBytes, PyModule, PyType};
use pytecode_engine::raw;
use pytecode_engine::raw::ClassFile;
use pytecode_engine::{parse_class, write_class};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

mod analysis;
mod archive;
mod attributes;
mod java_string;
mod model;
mod transforms;

type PyObject = Py<PyAny>;

create_exception!(
    pytecode,
    MalformedClassException,
    pyo3::exceptions::PyException,
    "Raised when classfile parsing, lowering, or verification encounters malformed bytecode."
);

pub(crate) fn engine_error_to_py(error: pytecode_engine::EngineError) -> PyErr {
    MalformedClassException::new_err(error.to_string())
}

pub(crate) fn with_bytes<R>(
    obj: &Bound<'_, PyAny>,
    f: impl FnOnce(&[u8]) -> PyResult<R>,
) -> PyResult<R> {
    if let Ok(bytes) = obj.cast::<PyBytes>() {
        return f(bytes.as_bytes());
    }
    if let Ok(bytearray) = obj.cast::<PyByteArray>() {
        // Copy out before dropping the GIL-borrowed slice so nested callbacks cannot invalidate it.
        let copied = unsafe { bytearray.as_bytes().to_vec() };
        return f(&copied);
    }
    Err(PyTypeError::new_err(format!(
        "expected bytes or bytearray, got {}",
        obj.get_type().name()?
    )))
}

pub(crate) fn extract_bytes(obj: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    with_bytes(obj, |bytes| Ok(bytes.to_vec()))
}

macro_rules! wrap_pyclass {
    ($py:expr, $value:expr) => {
        Py::new($py, $value).map(|obj| obj.into_bound($py).into_any().unbind())
    };
}

pub(crate) fn cached_py_attr<'py>(
    lock: &'static PyOnceLock<Py<PyAny>>,
    py: Python<'py>,
    module_name: &str,
    attr_name: &str,
) -> PyResult<&'py Py<PyAny>> {
    lock.get_or_try_init(py, || {
        Ok(PyModule::import(py, module_name)?
            .getattr(attr_name)?
            .unbind())
    })
}

macro_rules! define_cached_enum_wrapper {
    ($fn_name:ident, $arg_ty:ty, $module_name:literal, $class_name:literal, $convert:expr) => {
        pub(crate) fn $fn_name(py: Python<'_>, value: $arg_ty) -> PyResult<Py<PyAny>> {
            static ENUM_TYPE: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
            let enum_type = cached_py_attr(&ENUM_TYPE, py, $module_name, $class_name)?;
            let raw_value: i64 = ($convert)(value);
            Ok(enum_type.bind(py).call1((raw_value,))?.unbind())
        }
    };
}

define_cached_enum_wrapper!(
    wrap_class_access_flags,
    pytecode_engine::constants::ClassAccessFlags,
    "pytecode.classfile.constants",
    "ClassAccessFlag",
    |flags: pytecode_engine::constants::ClassAccessFlags| i64::from(flags.bits())
);

define_cached_enum_wrapper!(
    wrap_field_access_flags,
    pytecode_engine::constants::FieldAccessFlags,
    "pytecode.classfile.constants",
    "FieldAccessFlag",
    |flags: pytecode_engine::constants::FieldAccessFlags| i64::from(flags.bits())
);

define_cached_enum_wrapper!(
    wrap_method_access_flags,
    pytecode_engine::constants::MethodAccessFlags,
    "pytecode.classfile.constants",
    "MethodAccessFlag",
    |flags: pytecode_engine::constants::MethodAccessFlags| i64::from(flags.bits())
);

define_cached_enum_wrapper!(
    wrap_array_type,
    raw::ArrayType,
    "pytecode.classfile.bytecode",
    "ArrayType",
    |atype: raw::ArrayType| i64::from(atype as u8)
);

define_cached_enum_wrapper!(
    wrap_nested_class_access_flags,
    pytecode_engine::constants::NestedClassAccessFlag,
    "pytecode.classfile.constants",
    "NestedClassAccessFlag",
    |flags: pytecode_engine::constants::NestedClassAccessFlag| i64::from(flags.bits())
);

define_cached_enum_wrapper!(
    wrap_method_parameter_access_flags,
    pytecode_engine::constants::MethodParameterAccessFlag,
    "pytecode.classfile.constants",
    "MethodParameterAccessFlag",
    |flags: pytecode_engine::constants::MethodParameterAccessFlag| i64::from(flags.bits())
);

define_cached_enum_wrapper!(
    wrap_module_access_flags,
    pytecode_engine::constants::ModuleAccessFlag,
    "pytecode.classfile.constants",
    "ModuleAccessFlag",
    |flags: pytecode_engine::constants::ModuleAccessFlag| i64::from(flags.bits())
);

define_cached_enum_wrapper!(
    wrap_module_requires_access_flags,
    pytecode_engine::constants::ModuleRequiresAccessFlag,
    "pytecode.classfile.constants",
    "ModuleRequiresAccessFlag",
    |flags: pytecode_engine::constants::ModuleRequiresAccessFlag| i64::from(flags.bits())
);

define_cached_enum_wrapper!(
    wrap_module_exports_access_flags,
    pytecode_engine::constants::ModuleExportsAccessFlag,
    "pytecode.classfile.constants",
    "ModuleExportsAccessFlag",
    |flags: pytecode_engine::constants::ModuleExportsAccessFlag| i64::from(flags.bits())
);

define_cached_enum_wrapper!(
    wrap_module_opens_access_flags,
    pytecode_engine::constants::ModuleOpensAccessFlag,
    "pytecode.classfile.constants",
    "ModuleOpensAccessFlag",
    |flags: pytecode_engine::constants::ModuleOpensAccessFlag| i64::from(flags.bits())
);

define_cached_enum_wrapper!(
    wrap_verification_type,
    pytecode_engine::constants::VerificationType,
    "pytecode.classfile.constants",
    "VerificationType",
    |tag: pytecode_engine::constants::VerificationType| i64::from(tag as u8)
);

define_cached_enum_wrapper!(
    wrap_target_type,
    pytecode_engine::constants::TargetType,
    "pytecode.classfile.constants",
    "TargetType",
    |target_type: pytecode_engine::constants::TargetType| i64::from(target_type as u8)
);

define_cached_enum_wrapper!(
    wrap_type_path_kind,
    pytecode_engine::constants::TypePathKind,
    "pytecode.classfile.constants",
    "TypePathKind",
    |kind: pytecode_engine::constants::TypePathKind| i64::from(kind as u8)
);

define_cached_enum_wrapper!(
    wrap_instruction_type_code,
    i64,
    "pytecode.classfile.bytecode",
    "InsnInfoType",
    |code: i64| code
);

fn wrap_unimplemented_attr_type(py: Python<'_>) -> PyResult<Py<PyAny>> {
    static UNIMPLEMENTED: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
    let val = UNIMPLEMENTED.get_or_try_init(py, || -> PyResult<Py<PyAny>> {
        Ok(PyModule::import(py, "pytecode.classfile.attributes")?
            .getattr("AttributeInfoType")?
            .getattr("UNIMPLEMENTED")?
            .unbind())
    })?;
    Ok(val.clone_ref(py))
}

fn instruction_type_value(instruction: &raw::Instruction) -> i64 {
    match instruction {
        raw::Instruction::Wide(inner) => i64::from(0xC4_u16 + u16::from(inner.opcode)),
        _ => i64::from(instruction.opcode()),
    }
}

fn wrap_instruction_type(py: Python<'_>, instruction: &raw::Instruction) -> PyResult<Py<PyAny>> {
    wrap_instruction_type_code(py, instruction_type_value(instruction))
}

macro_rules! define_numeric_cp_wrapper {
    ($wrapper:ident, $pyname:literal, $doc:literal, $inner:ty, $tag:expr, { $($field:ident : $ty:ty => $fdoc:literal),* $(,)? }) => {
        #[doc = $doc]
        #[pyclass(from_py_object, module = "pytecode._rust", name = $pyname)]
        #[derive(Clone)]
        pub struct $wrapper {
            pub(crate) index: usize,
            pub(crate) inner: $inner,
        }

        #[pymethods]
        impl $wrapper {
            /// 1-based index of this entry in the constant pool.
            #[getter]
            fn index(&self) -> usize {
                self.index
            }

            /// Byte offset of this entry if tracked, otherwise ``None``.
            #[getter]
            fn offset(&self) -> Option<usize> {
                None
            }

            /// Numeric JVMS constant-pool tag for this entry.
            #[getter]
            fn tag(&self) -> u8 {
                $tag as u8
            }

            $(
                #[doc = $fdoc]
                #[getter]
                fn $field(&self) -> $ty {
                    self.inner.$field.into()
                }
            )*

            fn __repr__(&self) -> String {
                format!("{}(index={}, tag={})", $pyname, self.index, $tag as u8)
            }
        }
    };
}

/// Raw ``CONSTANT_Utf8_info`` entry in a classfile constant pool.
#[pyclass(from_py_object, module = "pytecode._rust", name = "Utf8Info")]
#[derive(Clone)]
pub struct PyUtf8Info {
    pub(crate) index: usize,
    pub(crate) inner: raw::Utf8Info,
}

#[pymethods]
impl PyUtf8Info {
    /// 1-based index of this entry in the constant pool.
    #[getter]
    fn index(&self) -> usize {
        self.index
    }

    /// Byte offset of this entry if tracked, otherwise ``None``.
    #[getter]
    fn offset(&self) -> Option<usize> {
        None
    }

    /// Numeric JVMS constant-pool tag (``1``).
    #[getter]
    fn tag(&self) -> u8 {
        raw::ConstantPoolTag::Utf8 as u8
    }

    /// Encoded Modified UTF-8 byte length.
    #[getter]
    fn length(&self) -> usize {
        self.inner.bytes.len()
    }

    /// Raw Modified UTF-8 payload bytes.
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
    "Raw ``CONSTANT_Integer_info`` constant-pool entry.",
    raw::IntegerInfo,
    raw::ConstantPoolTag::Integer,
    { value_bits: u32 => "Raw 32-bit integer bit representation." }
);
define_numeric_cp_wrapper!(
    PyFloatInfo,
    "FloatInfo",
    "Raw ``CONSTANT_Float_info`` constant-pool entry.",
    raw::FloatInfo,
    raw::ConstantPoolTag::Float,
    { value_bits: u32 => "Raw IEEE 754 float bit representation." }
);
define_numeric_cp_wrapper!(
    PyLongInfo,
    "LongInfo",
    "Raw ``CONSTANT_Long_info`` constant-pool entry.",
    raw::LongInfo,
    raw::ConstantPoolTag::Long,
    {
        high_bits: u32 => "High 32 bits of the 64-bit integer value.",
        low_bits: u32 => "Low 32 bits of the 64-bit integer value.",
    }
);
define_numeric_cp_wrapper!(
    PyDoubleInfo,
    "DoubleInfo",
    "Raw ``CONSTANT_Double_info`` constant-pool entry.",
    raw::DoubleInfo,
    raw::ConstantPoolTag::Double,
    {
        high_bits: u32 => "High 32 bits of the IEEE 754 double representation.",
        low_bits: u32 => "Low 32 bits of the IEEE 754 double representation.",
    }
);
define_numeric_cp_wrapper!(
    PyConstantPoolClassInfo,
    "ClassInfo",
    "Raw ``CONSTANT_Class_info`` constant-pool entry.",
    raw::ClassInfo,
    raw::ConstantPoolTag::Class,
    { name_index: u16 => "Constant-pool UTF-8 index of the internal class or array descriptor." }
);
define_numeric_cp_wrapper!(
    PyStringInfo,
    "StringInfo",
    "Raw ``CONSTANT_String_info`` constant-pool entry.",
    raw::StringInfo,
    raw::ConstantPoolTag::String,
    { string_index: u16 => "Constant-pool UTF-8 index of the string literal contents." }
);
define_numeric_cp_wrapper!(
    PyFieldrefInfo,
    "FieldrefInfo",
    "Raw ``CONSTANT_Fieldref_info`` constant-pool entry.",
    raw::FieldRefInfo,
    raw::ConstantPoolTag::FieldRef,
    {
        class_index: u16 => "Constant-pool ``ClassInfo`` index of the declaring class.",
        name_and_type_index: u16 => "Constant-pool ``NameAndTypeInfo`` index of the field.",
    }
);
define_numeric_cp_wrapper!(
    PyMethodrefInfo,
    "MethodrefInfo",
    "Raw ``CONSTANT_Methodref_info`` constant-pool entry.",
    raw::MethodRefInfo,
    raw::ConstantPoolTag::MethodRef,
    {
        class_index: u16 => "Constant-pool ``ClassInfo`` index of the declaring class.",
        name_and_type_index: u16 => "Constant-pool ``NameAndTypeInfo`` index of the method.",
    }
);
define_numeric_cp_wrapper!(
    PyInterfaceMethodrefInfo,
    "InterfaceMethodrefInfo",
    "Raw ``CONSTANT_InterfaceMethodref_info`` constant-pool entry.",
    raw::InterfaceMethodRefInfo,
    raw::ConstantPoolTag::InterfaceMethodRef,
    {
        class_index: u16 => "Constant-pool ``ClassInfo`` index of the declaring interface.",
        name_and_type_index: u16 => "Constant-pool ``NameAndTypeInfo`` index of the method.",
    }
);
define_numeric_cp_wrapper!(
    PyNameAndTypeInfo,
    "NameAndTypeInfo",
    "Raw ``CONSTANT_NameAndType_info`` constant-pool entry.",
    raw::NameAndTypeInfo,
    raw::ConstantPoolTag::NameAndType,
    {
        name_index: u16 => "Constant-pool UTF-8 index of the member name.",
        descriptor_index: u16 => "Constant-pool UTF-8 index of the member descriptor.",
    }
);
define_numeric_cp_wrapper!(
    PyMethodHandleInfo,
    "MethodHandleInfo",
    "Raw ``CONSTANT_MethodHandle_info`` constant-pool entry.",
    raw::MethodHandleInfo,
    raw::ConstantPoolTag::MethodHandle,
    {
        reference_kind: u8 => "JVMS reference kind bytecode tag in ``1..=9``.",
        reference_index: u16 => "Constant-pool index of the target field or method reference.",
    }
);
define_numeric_cp_wrapper!(
    PyMethodTypeInfo,
    "MethodTypeInfo",
    "Raw ``CONSTANT_MethodType_info`` constant-pool entry.",
    raw::MethodTypeInfo,
    raw::ConstantPoolTag::MethodType,
    { descriptor_index: u16 => "Constant-pool UTF-8 index of the method descriptor." }
);
define_numeric_cp_wrapper!(
    PyDynamicInfo,
    "DynamicInfo",
    "Raw ``CONSTANT_Dynamic_info`` constant-pool entry.",
    raw::DynamicInfo,
    raw::ConstantPoolTag::Dynamic,
    {
        bootstrap_method_attr_index: u16 => "Index into the class's ``BootstrapMethods`` attribute.",
        name_and_type_index: u16 => "Constant-pool ``NameAndTypeInfo`` index of the constant.",
    }
);
define_numeric_cp_wrapper!(
    PyInvokeDynamicInfo,
    "InvokeDynamicInfo",
    "Raw ``CONSTANT_InvokeDynamic_info`` constant-pool entry.",
    raw::InvokeDynamicInfo,
    raw::ConstantPoolTag::InvokeDynamic,
    {
        bootstrap_method_attr_index: u16 => "Index into the class's ``BootstrapMethods`` attribute.",
        name_and_type_index: u16 => "Constant-pool ``NameAndTypeInfo`` index of the call site.",
    }
);
define_numeric_cp_wrapper!(
    PyModuleInfo,
    "ModuleInfo",
    "Raw ``CONSTANT_Module_info`` constant-pool entry.",
    raw::ModuleInfo,
    raw::ConstantPoolTag::Module,
    { name_index: u16 => "Constant-pool UTF-8 index of the module name." }
);
define_numeric_cp_wrapper!(
    PyPackageInfo,
    "PackageInfo",
    "Raw ``CONSTANT_Package_info`` constant-pool entry.",
    raw::PackageInfo,
    raw::ConstantPoolTag::Package,
    { name_index: u16 => "Constant-pool UTF-8 index of the package name." }
);

pub(crate) fn constant_pool_entry_to_pyobject(
    py: Python<'_>,
    index: usize,
    entry: &raw::ConstantPoolEntry,
) -> PyResult<PyObject> {
    use raw::ConstantPoolEntry::*;
    match entry {
        Utf8(inner) => wrap_pyclass!(
            py,
            PyUtf8Info {
                index,
                inner: inner.clone(),
            }
        ),
        Integer(inner) => wrap_pyclass!(
            py,
            PyIntegerInfo {
                index,
                inner: inner.clone(),
            }
        ),
        Float(inner) => wrap_pyclass!(
            py,
            PyFloatInfo {
                index,
                inner: inner.clone(),
            }
        ),
        Long(inner) => wrap_pyclass!(
            py,
            PyLongInfo {
                index,
                inner: inner.clone(),
            }
        ),
        Double(inner) => wrap_pyclass!(
            py,
            PyDoubleInfo {
                index,
                inner: inner.clone(),
            }
        ),
        Class(inner) => wrap_pyclass!(
            py,
            PyConstantPoolClassInfo {
                index,
                inner: inner.clone(),
            }
        ),
        String(inner) => wrap_pyclass!(
            py,
            PyStringInfo {
                index,
                inner: inner.clone(),
            }
        ),
        FieldRef(inner) => wrap_pyclass!(
            py,
            PyFieldrefInfo {
                index,
                inner: inner.clone(),
            }
        ),
        MethodRef(inner) => wrap_pyclass!(
            py,
            PyMethodrefInfo {
                index,
                inner: inner.clone(),
            }
        ),
        InterfaceMethodRef(inner) => wrap_pyclass!(
            py,
            PyInterfaceMethodrefInfo {
                index,
                inner: inner.clone(),
            }
        ),
        NameAndType(inner) => wrap_pyclass!(
            py,
            PyNameAndTypeInfo {
                index,
                inner: inner.clone(),
            }
        ),
        MethodHandle(inner) => wrap_pyclass!(
            py,
            PyMethodHandleInfo {
                index,
                inner: inner.clone(),
            }
        ),
        MethodType(inner) => wrap_pyclass!(
            py,
            PyMethodTypeInfo {
                index,
                inner: inner.clone(),
            }
        ),
        Dynamic(inner) => wrap_pyclass!(
            py,
            PyDynamicInfo {
                index,
                inner: inner.clone(),
            }
        ),
        InvokeDynamic(inner) => wrap_pyclass!(
            py,
            PyInvokeDynamicInfo {
                index,
                inner: inner.clone(),
            }
        ),
        Module(inner) => wrap_pyclass!(
            py,
            PyModuleInfo {
                index,
                inner: inner.clone(),
            }
        ),
        Package(inner) => wrap_pyclass!(
            py,
            PyPackageInfo {
                index,
                inner: inner.clone(),
            }
        ),
    }
}

/// Key-offset pair inside a raw ``lookupswitch`` instruction.
#[pyclass(from_py_object, module = "pytecode._rust", name = "MatchOffsetPair")]
#[derive(Clone)]
pub struct PyMatchOffsetPair {
    inner: raw::MatchOffsetPair,
}

#[pymethods]
impl PyMatchOffsetPair {
    /// Switch case integer key.
    #[getter]
    fn r#match(&self) -> i32 {
        self.inner.match_value
    }

    /// Relative jump offset when the key matches.
    #[getter]
    fn offset(&self) -> i32 {
        self.inner.offset
    }

    fn __repr__(&self) -> String {
        format!(
            "MatchOffsetPair(match={}, offset={})",
            self.inner.match_value, self.inner.offset
        )
    }
}

/// Raw decoded JVM bytecode instruction inside a ``CodeAttr``.
#[pyclass(from_py_object, module = "pytecode._rust", name = "InsnInfo")]
#[derive(Clone)]
pub struct PyInsnInfo {
    inner: raw::Instruction,
}

#[pymethods]
impl PyInsnInfo {
    /// Opcode enum member describing this instruction (wide variants encoded distinctly).
    #[getter]
    fn r#type(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        wrap_instruction_type(py, &self.inner)
    }

    /// Byte offset of the instruction inside the ``Code`` array.
    #[getter]
    fn bytecode_offset(&self) -> u32 {
        self.inner.offset()
    }

    /// Raw JVM opcode byte.
    #[getter]
    fn opcode(&self) -> u8 {
        self.inner.opcode()
    }

    /// Local-variable slot or constant-pool index operand, if applicable.
    #[getter]
    fn index(&self) -> Option<u16> {
        match &self.inner {
            raw::Instruction::LocalIndex { index, .. }
            | raw::Instruction::ConstantPoolIndex1 { index, .. } => Some(u16::from(*index)),
            raw::Instruction::ConstantPoolIndexWide(inner) => Some(inner.index.into()),
            raw::Instruction::IInc { index, .. } => Some(u16::from(*index)),
            raw::Instruction::InvokeDynamic(inner) => Some(inner.index.into()),
            raw::Instruction::InvokeInterface(inner) => Some(inner.index.into()),
            raw::Instruction::MultiANewArray { index, .. } => Some((*index).into()),
            raw::Instruction::Wide(inner) => Some(inner.index),
            _ => None,
        }
    }

    /// Immediate signed value or increment amount, if applicable.
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

    /// Relative jump offset for branch instructions, if applicable.
    #[getter]
    fn branch_offset(&self) -> Option<i32> {
        match &self.inner {
            raw::Instruction::Branch(inner) => Some(i32::from(inner.branch_offset)),
            raw::Instruction::BranchWide { branch_offset, .. } => Some(*branch_offset),
            _ => None,
        }
    }

    /// Historical JVMS ``count`` byte for ``invokeinterface``, if applicable.
    #[getter]
    fn count(&self) -> Option<u8> {
        match &self.inner {
            raw::Instruction::InvokeInterface(inner) => Some(inner.count),
            _ => None,
        }
    }

    /// Reserved trailing operand bytes for ``invokeinterface`` / ``invokedynamic``, if applicable.
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

    /// Primitive element type for ``newarray``, if applicable.
    #[getter]
    fn atype(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        match &self.inner {
            raw::Instruction::NewArray(inner) => wrap_array_type(py, inner.atype).map(Some),
            _ => Ok(None),
        }
    }

    /// Number of dimensions allocated by ``multianewarray``, if applicable.
    #[getter]
    fn dimensions(&self) -> Option<u8> {
        match &self.inner {
            raw::Instruction::MultiANewArray { dimensions, .. } => Some(*dimensions),
            _ => None,
        }
    }

    /// Default branch offset for switch instructions, if applicable.
    #[getter]
    fn default(&self) -> Option<i32> {
        match &self.inner {
            raw::Instruction::LookupSwitch(inner) => Some(inner.default_offset),
            raw::Instruction::TableSwitch(inner) => Some(inner.default_offset),
            _ => None,
        }
    }

    /// Number of key-offset pairs for ``lookupswitch``, if applicable.
    #[getter]
    fn npairs(&self) -> Option<usize> {
        match &self.inner {
            raw::Instruction::LookupSwitch(inner) => Some(inner.pairs.len()),
            _ => None,
        }
    }

    /// Ordered key-offset pairs for ``lookupswitch``, if applicable.
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

    /// Inclusive lower case bound for ``tableswitch``, if applicable.
    #[getter]
    fn low(&self) -> Option<i32> {
        match &self.inner {
            raw::Instruction::TableSwitch(inner) => Some(inner.low),
            _ => None,
        }
    }

    /// Inclusive upper case bound for ``tableswitch``, if applicable.
    #[getter]
    fn high(&self) -> Option<i32> {
        match &self.inner {
            raw::Instruction::TableSwitch(inner) => Some(inner.high),
            _ => None,
        }
    }

    /// Dense jump offsets for ``tableswitch``, if applicable.
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

/// One exception-table entry in a raw ``CodeAttr``.
#[pyclass(from_py_object, module = "pytecode._rust", name = "ExceptionInfo")]
#[derive(Clone)]
pub struct PyExceptionInfo {
    inner: raw::ExceptionHandler,
}

#[pymethods]
impl PyExceptionInfo {
    /// Inclusive start bytecode offset of the guarded region.
    #[getter]
    fn start_pc(&self) -> u16 {
        self.inner.start_pc
    }

    /// Exclusive end bytecode offset of the guarded region.
    #[getter]
    fn end_pc(&self) -> u16 {
        self.inner.end_pc
    }

    /// Entry bytecode offset of the exception handler.
    #[getter]
    fn handler_pc(&self) -> u16 {
        self.inner.handler_pc
    }

    /// Constant-pool ``ClassInfo`` index of the caught exception, or ``0`` for catch-all.
    #[getter]
    fn catch_type(&self) -> u16 {
        self.inner.catch_type.into()
    }

    fn __repr__(&self) -> String {
        format!(
            "ExceptionInfo(start_pc={}, end_pc={}, handler_pc={}, catch_type={})",
            self.inner.start_pc,
            self.inner.end_pc,
            self.inner.handler_pc,
            u16::from(self.inner.catch_type)
        )
    }
}

/// Constant value attached to a field declaration.
#[pyclass(from_py_object, module = "pytecode._rust", name = "ConstantValueAttr")]
#[derive(Clone)]
pub struct PyConstantValueAttr {
    inner: raw::ConstantValueAttribute,
}

#[pymethods]
impl PyConstantValueAttr {
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

    /// Constant-pool index of the field's constant value.
    #[getter]
    fn constantvalue_index(&self) -> u16 {
        self.inner.constantvalue_index.into()
    }

    fn __repr__(&self) -> String {
        format!(
            "ConstantValueAttr(constantvalue_index={})",
            u16::from(self.inner.constantvalue_index)
        )
    }
}

/// Generic signature string for a class, field, method, or record component.
#[pyclass(from_py_object, module = "pytecode._rust", name = "SignatureAttr")]
#[derive(Clone)]
pub struct PySignatureAttr {
    inner: raw::SignatureAttribute,
}

#[pymethods]
impl PySignatureAttr {
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

    /// Constant-pool UTF-8 index of the JVM generic signature.
    #[getter]
    fn signature_index(&self) -> u16 {
        self.inner.signature_index.into()
    }

    fn __repr__(&self) -> String {
        format!(
            "SignatureAttr(signature_index={})",
            u16::from(self.inner.signature_index)
        )
    }
}

/// Original source file name recorded for the class.
#[pyclass(from_py_object, module = "pytecode._rust", name = "SourceFileAttr")]
#[derive(Clone)]
pub struct PySourceFileAttr {
    inner: raw::SourceFileAttribute,
}

#[pymethods]
impl PySourceFileAttr {
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

    /// Constant-pool UTF-8 index of the source file name.
    #[getter]
    fn sourcefile_index(&self) -> u16 {
        self.inner.sourcefile_index.into()
    }

    fn __repr__(&self) -> String {
        format!(
            "SourceFileAttr(sourcefile_index={})",
            u16::from(self.inner.sourcefile_index)
        )
    }
}

/// Extended source-debug payload such as SMAP data.
#[pyclass(
    from_py_object,
    module = "pytecode._rust",
    name = "SourceDebugExtensionAttr"
)]
#[derive(Clone)]
pub struct PySourceDebugExtensionAttr {
    inner: raw::SourceDebugExtensionAttribute,
}

#[pymethods]
impl PySourceDebugExtensionAttr {
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

    /// Raw ``SourceDebugExtension`` payload bytes.
    #[getter]
    fn debug_extension<'py>(&self, py: Python<'py>) -> Py<PyBytes> {
        PyBytes::new(py, &self.inner.debug_extension).unbind()
    }

    fn __repr__(&self) -> String {
        format!(
            "SourceDebugExtensionAttr(length={})",
            self.inner.debug_extension.len()
        )
    }
}

/// Checked exception types declared by a method.
#[pyclass(from_py_object, module = "pytecode._rust", name = "ExceptionsAttr")]
#[derive(Clone)]
pub struct PyExceptionsAttr {
    inner: raw::ExceptionsAttribute,
}

#[pymethods]
impl PyExceptionsAttr {
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

    /// Number of checked exception classes in the table.
    #[getter]
    fn number_of_exceptions(&self) -> usize {
        self.inner.exception_index_table.len()
    }

    /// Constant-pool ``ClassInfo`` indices of thrown exception classes.
    #[getter]
    fn exception_index_table(&self) -> Vec<u16> {
        self.inner
            .exception_index_table
            .iter()
            .map(|x| (*x).into())
            .collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "ExceptionsAttr(number_of_exceptions={})",
            self.inner.exception_index_table.len()
        )
    }
}

/// Method body, exception handlers, and nested code-scoped attributes.
#[pyclass(from_py_object, module = "pytecode._rust", name = "CodeAttr")]
#[derive(Clone)]
pub struct PyCodeAttr {
    inner: raw::CodeAttribute,
}

#[pymethods]
impl PyCodeAttr {
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

    /// Maximum operand-stack depth required by the method body.
    #[getter]
    fn max_stack(&self) -> u16 {
        self.inner.max_stack
    }

    /// Compatibility alias for ``max_stack``.
    #[getter]
    fn max_stacks(&self) -> u16 {
        self.inner.max_stack
    }

    /// Maximum local-variable slots allocated by the method body.
    #[getter]
    fn max_locals(&self) -> u16 {
        self.inner.max_locals
    }

    /// Length of the encoded bytecode array in bytes.
    #[getter]
    fn code_length(&self) -> u32 {
        self.inner.code_length
    }

    /// Decoded raw bytecode instructions.
    #[getter]
    fn code(&self) -> Vec<PyInsnInfo> {
        self.inner
            .code
            .iter()
            .cloned()
            .map(|inner| PyInsnInfo { inner })
            .collect()
    }

    /// Number of entries in the exception handler table.
    #[getter]
    fn exception_table_length(&self) -> usize {
        self.inner.exception_table.len()
    }

    /// Exception handlers declared on this code block.
    #[getter]
    fn exception_table(&self) -> Vec<PyExceptionInfo> {
        self.inner
            .exception_table
            .iter()
            .cloned()
            .map(|inner| PyExceptionInfo { inner })
            .collect()
    }

    /// Number of nested attributes attached to this code block.
    #[getter]
    fn attributes_count(&self) -> usize {
        self.inner.attributes.len()
    }

    /// Nested attributes inside the code attribute (line numbers, frames, etc.).
    #[getter]
    fn attributes(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .attributes
            .iter()
            .map(|attribute| wrap_attribute(py, attribute))
            .collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "CodeAttr(max_stack={}, max_locals={}, code_length={}, insns={})",
            self.inner.max_stack,
            self.inner.max_locals,
            self.inner.code_length,
            self.inner.code.len()
        )
    }
}

/// Placeholder for unknown or custom classfile attributes.
#[pyclass(from_py_object, module = "pytecode._rust", name = "UnimplementedAttr")]
#[derive(Clone)]
pub struct PyUnimplementedAttr {
    inner: raw::UnknownAttribute,
}

#[pymethods]
impl PyUnimplementedAttr {
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

    /// Resolved UTF-8 name of this attribute.
    #[getter]
    fn name(&self) -> String {
        self.inner.name.clone()
    }

    /// Unparsed raw attribute payload bytes.
    #[getter]
    fn info<'py>(&self, py: Python<'py>) -> Py<PyBytes> {
        PyBytes::new(py, &self.inner.info).unbind()
    }

    /// Attribute type marker (``AttributeInfoType.UNIMPLEMENTED``).
    #[getter]
    fn attr_type(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        wrap_unimplemented_attr_type(py)
    }

    fn __repr__(&self) -> String {
        format!(
            "UnimplementedAttr(name={:?}, attribute_length={})",
            self.inner.name, self.inner.attribute_length
        )
    }
}

pub(crate) fn wrap_attribute(
    py: Python<'_>,
    attribute: &raw::AttributeInfo,
) -> PyResult<Py<PyAny>> {
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
        raw::AttributeInfo::Code(inner) => wrap_pyclass!(
            py,
            PyCodeAttr {
                inner: inner.clone(),
            }
        ),
        raw::AttributeInfo::StackMapTable(inner) => wrap_pyclass!(
            py,
            attributes::PyStackMapTableAttr {
                inner: inner.clone(),
            }
        ),
        raw::AttributeInfo::Exceptions(inner) => wrap_pyclass!(
            py,
            PyExceptionsAttr {
                inner: inner.clone(),
            }
        ),
        raw::AttributeInfo::InnerClasses(inner) => wrap_pyclass!(
            py,
            attributes::PyInnerClassesAttr {
                inner: inner.clone(),
            }
        ),
        raw::AttributeInfo::EnclosingMethod(inner) => wrap_pyclass!(
            py,
            attributes::PyEnclosingMethodAttr {
                inner: inner.clone(),
            }
        ),
        raw::AttributeInfo::Synthetic(inner) => wrap_pyclass!(
            py,
            attributes::PySyntheticAttr {
                inner: inner.clone(),
            }
        ),
        raw::AttributeInfo::Deprecated(inner) => wrap_pyclass!(
            py,
            attributes::PyDeprecatedAttr {
                inner: inner.clone(),
            }
        ),
        raw::AttributeInfo::LineNumberTable(inner) => wrap_pyclass!(
            py,
            attributes::PyLineNumberTableAttr {
                inner: inner.clone(),
            }
        ),
        raw::AttributeInfo::LocalVariableTable(inner) => wrap_pyclass!(
            py,
            attributes::PyLocalVariableTableAttr {
                inner: inner.clone(),
            }
        ),
        raw::AttributeInfo::LocalVariableTypeTable(inner) => wrap_pyclass!(
            py,
            attributes::PyLocalVariableTypeTableAttr {
                inner: inner.clone(),
            }
        ),
        raw::AttributeInfo::RuntimeVisibleAnnotations(inner) => wrap_pyclass!(
            py,
            attributes::PyRuntimeVisibleAnnotationsAttr {
                inner: inner.clone(),
            }
        ),
        raw::AttributeInfo::RuntimeInvisibleAnnotations(inner) => wrap_pyclass!(
            py,
            attributes::PyRuntimeInvisibleAnnotationsAttr {
                inner: inner.clone(),
            }
        ),
        raw::AttributeInfo::RuntimeVisibleParameterAnnotations(inner) => wrap_pyclass!(
            py,
            attributes::PyRuntimeVisibleParameterAnnotationsAttr {
                inner: inner.clone(),
            }
        ),
        raw::AttributeInfo::RuntimeInvisibleParameterAnnotations(inner) => wrap_pyclass!(
            py,
            attributes::PyRuntimeInvisibleParameterAnnotationsAttr {
                inner: inner.clone(),
            }
        ),
        raw::AttributeInfo::RuntimeVisibleTypeAnnotations(inner) => wrap_pyclass!(
            py,
            attributes::PyRuntimeVisibleTypeAnnotationsAttr {
                inner: inner.clone(),
            }
        ),
        raw::AttributeInfo::RuntimeInvisibleTypeAnnotations(inner) => wrap_pyclass!(
            py,
            attributes::PyRuntimeInvisibleTypeAnnotationsAttr {
                inner: inner.clone(),
            }
        ),
        raw::AttributeInfo::AnnotationDefault(inner) => wrap_pyclass!(
            py,
            attributes::PyAnnotationDefaultAttr {
                inner: inner.clone(),
            }
        ),
        raw::AttributeInfo::BootstrapMethods(inner) => wrap_pyclass!(
            py,
            attributes::PyBootstrapMethodsAttr {
                inner: inner.clone(),
            }
        ),
        raw::AttributeInfo::MethodParameters(inner) => wrap_pyclass!(
            py,
            attributes::PyMethodParametersAttr {
                inner: inner.clone(),
            }
        ),
        raw::AttributeInfo::Module(inner) => wrap_pyclass!(
            py,
            attributes::PyModuleAttr {
                inner: inner.clone(),
            }
        ),
        raw::AttributeInfo::ModulePackages(inner) => wrap_pyclass!(
            py,
            attributes::PyModulePackagesAttr {
                inner: inner.clone(),
            }
        ),
        raw::AttributeInfo::ModuleMainClass(inner) => wrap_pyclass!(
            py,
            attributes::PyModuleMainClassAttr {
                inner: inner.clone(),
            }
        ),
        raw::AttributeInfo::NestHost(inner) => wrap_pyclass!(
            py,
            attributes::PyNestHostAttr {
                inner: inner.clone(),
            }
        ),
        raw::AttributeInfo::NestMembers(inner) => wrap_pyclass!(
            py,
            attributes::PyNestMembersAttr {
                inner: inner.clone(),
            }
        ),
        raw::AttributeInfo::Record(inner) => wrap_pyclass!(
            py,
            attributes::PyRecordAttr {
                inner: inner.clone(),
            }
        ),
        raw::AttributeInfo::PermittedSubclasses(inner) => wrap_pyclass!(
            py,
            attributes::PyPermittedSubclassesAttr {
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

/// Raw ``field_info`` structure parsed from a JVM classfile.
#[pyclass(from_py_object, module = "pytecode._rust", name = "FieldInfo")]
#[derive(Clone)]
pub struct PyFieldInfo {
    inner: raw::FieldInfo,
}

#[pymethods]
impl PyFieldInfo {
    /// Field access flags wrapped in ``FieldAccessFlag``.
    #[getter]
    fn access_flags(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        wrap_field_access_flags(py, self.inner.access_flags)
    }

    /// Constant-pool UTF-8 index of the field name.
    #[getter]
    fn name_index(&self) -> u16 {
        self.inner.name_index.into()
    }

    /// Constant-pool UTF-8 index of the field descriptor.
    #[getter]
    fn descriptor_index(&self) -> u16 {
        self.inner.descriptor_index.into()
    }

    /// Number of attributes attached to this field.
    #[getter]
    fn attributes_count(&self) -> usize {
        self.inner.attributes.len()
    }

    /// Raw attributes attached to this field.
    #[getter]
    fn attributes(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .attributes
            .iter()
            .map(|attribute| wrap_attribute(py, attribute))
            .collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "FieldInfo(name_index={}, descriptor_index={}, attributes={})",
            u16::from(self.inner.name_index),
            u16::from(self.inner.descriptor_index),
            self.inner.attributes.len()
        )
    }
}

/// Raw ``method_info`` structure parsed from a JVM classfile.
#[pyclass(from_py_object, module = "pytecode._rust", name = "MethodInfo")]
#[derive(Clone)]
pub struct PyMethodInfo {
    inner: raw::MethodInfo,
}

#[pymethods]
impl PyMethodInfo {
    /// Method access flags wrapped in ``MethodAccessFlag``.
    #[getter]
    fn access_flags(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        wrap_method_access_flags(py, self.inner.access_flags)
    }

    /// Constant-pool UTF-8 index of the method name.
    #[getter]
    fn name_index(&self) -> u16 {
        self.inner.name_index.into()
    }

    /// Constant-pool UTF-8 index of the method descriptor.
    #[getter]
    fn descriptor_index(&self) -> u16 {
        self.inner.descriptor_index.into()
    }

    /// Number of attributes attached to this method.
    #[getter]
    fn attributes_count(&self) -> usize {
        self.inner.attributes.len()
    }

    /// Raw attributes attached to this method.
    #[getter]
    fn attributes(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .attributes
            .iter()
            .map(|attribute| wrap_attribute(py, attribute))
            .collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "MethodInfo(name_index={}, descriptor_index={}, attributes={})",
            u16::from(self.inner.name_index),
            u16::from(self.inner.descriptor_index),
            self.inner.attributes.len()
        )
    }
}

/// Read-only raw JVMS classfile structure exposing constant pool, members, and attributes.
#[pyclass(from_py_object, module = "pytecode._rust", name = "ClassFile")]
#[derive(Clone)]
pub struct PyClassFile {
    pub(crate) inner: Arc<ClassFile>,
}

#[pymethods]
impl PyClassFile {
    /// Classfile magic header value (``0xCAFEBABE``).
    #[getter]
    fn magic(&self) -> u32 {
        self.inner.magic
    }

    /// Classfile minor version number.
    #[getter]
    fn minor_version(&self) -> u16 {
        self.inner.minor_version
    }

    /// Classfile major version number.
    #[getter]
    fn major_version(&self) -> u16 {
        self.inner.major_version
    }

    /// Constant-pool table slot count (including slot 0 and wide-entry placeholders).
    #[getter]
    fn constant_pool_count(&self) -> usize {
        self.inner.constant_pool.len()
    }

    /// Raw constant-pool table with ``None`` at slot 0 and after wide entries.
    #[getter]
    fn constant_pool(&self, py: Python<'_>) -> PyResult<Vec<Option<Py<PyAny>>>> {
        self.inner
            .constant_pool
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                entry
                    .as_ref()
                    .map(|entry| constant_pool_entry_to_pyobject(py, index, entry))
                    .transpose()
            })
            .collect()
    }

    /// Class access flags wrapped in ``ClassAccessFlag``.
    #[getter]
    fn access_flags(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        wrap_class_access_flags(py, self.inner.access_flags)
    }

    /// Constant-pool ``ClassInfo`` index for this class.
    #[getter]
    fn this_class(&self) -> u16 {
        self.inner.this_class.into()
    }

    /// Constant-pool ``ClassInfo`` index for the direct superclass, or ``0``.
    #[getter]
    fn super_class(&self) -> u16 {
        self.inner.super_class.into()
    }

    /// Number of implemented interfaces.
    #[getter]
    fn interfaces_count(&self) -> usize {
        self.inner.interfaces.len()
    }

    /// Compatibility alias for ``interfaces_count``.
    #[getter]
    fn interface_count(&self) -> usize {
        self.inner.interfaces.len()
    }

    /// Constant-pool ``ClassInfo`` indices of directly implemented interfaces.
    #[getter]
    fn interfaces(&self) -> Vec<u16> {
        self.inner.interfaces.iter().map(|x| (*x).into()).collect()
    }

    /// Number of declared fields.
    #[getter]
    fn fields_count(&self) -> usize {
        self.inner.fields.len()
    }

    /// Compatibility alias for ``fields_count``.
    #[getter]
    fn field_count(&self) -> usize {
        self.inner.fields.len()
    }

    /// Raw field declarations on this classfile.
    #[getter]
    fn fields(&self) -> Vec<PyFieldInfo> {
        self.inner
            .fields
            .iter()
            .cloned()
            .map(|inner| PyFieldInfo { inner })
            .collect()
    }

    /// Number of declared methods.
    #[getter]
    fn methods_count(&self) -> usize {
        self.inner.methods.len()
    }

    /// Compatibility alias for ``methods_count``.
    #[getter]
    fn method_count(&self) -> usize {
        self.inner.methods.len()
    }

    /// Raw method declarations on this classfile.
    #[getter]
    fn methods(&self) -> Vec<PyMethodInfo> {
        self.inner
            .methods
            .iter()
            .cloned()
            .map(|inner| PyMethodInfo { inner })
            .collect()
    }

    /// Number of class-level attributes.
    #[getter]
    fn attributes_count(&self) -> usize {
        self.inner.attributes.len()
    }

    /// Compatibility alias for ``attributes_count``.
    #[getter]
    fn attribute_count(&self) -> usize {
        self.inner.attributes.len()
    }

    /// Raw class-level attributes.
    #[getter]
    fn attributes(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .attributes
            .iter()
            .map(|attribute| wrap_attribute(py, attribute))
            .collect()
    }

    /// Serialize this raw classfile back to JVM classfile bytes.
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

/// Parse raw classfile bytes into a read-only ``ClassFile`` view.
#[pyclass(module = "pytecode._rust", name = "ClassReader")]
pub struct PyClassReader {
    pub(crate) class_info: Arc<ClassFile>,
}

#[pymethods]
impl PyClassReader {
    #[new]
    fn new(bytes_or_bytearray: &Bound<'_, PyAny>) -> PyResult<Self> {
        with_bytes(bytes_or_bytearray, |bytes| {
            let class_info = parse_class(bytes).map_err(engine_error_to_py)?;
            Ok(Self {
                class_info: Arc::new(class_info),
            })
        })
    }

    /// Create a reader from in-memory ``bytes`` or ``bytearray``.
    #[classmethod]
    fn from_bytes(
        _cls: &Bound<'_, PyType>,
        bytes_or_bytearray: &Bound<'_, PyAny>,
    ) -> PyResult<Self> {
        Self::new(bytes_or_bytearray)
    }

    /// Read a classfile from disk and return a new reader.
    #[classmethod]
    fn from_file(_cls: &Bound<'_, PyType>, path: PathBuf) -> PyResult<Self> {
        let bytes = fs::read(&path).map_err(PyOSError::new_err)?;
        let class_info = parse_class(&bytes).map_err(engine_error_to_py)?;
        Ok(Self {
            class_info: Arc::new(class_info),
        })
    }

    /// Parsed raw classfile structure exposed by this reader.
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

/// Serialize raw ``ClassFile`` objects back to bytes.
#[pyclass(module = "pytecode._rust", name = "ClassWriter")]
pub struct PyClassWriter;

#[pymethods]
impl PyClassWriter {
    /// Encode a raw classfile object into JVM classfile bytes.
    #[staticmethod]
    fn write<'py>(py: Python<'py>, classfile: &PyClassFile) -> PyResult<Py<PyBytes>> {
        let bytes = write_class(&classfile.inner).map_err(engine_error_to_py)?;
        Ok(PyBytes::new(py, &bytes).unbind())
    }
}

// Mutable model views have thread ownership; do not advertise GIL-free execution.
#[pymodule(gil_used = true)]
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
    module.add_class::<transforms::PyInsnMatcher>()?;
    module.add_class::<transforms::PyClassMatcher>()?;
    module.add_class::<transforms::PyFieldMatcher>()?;
    module.add_class::<transforms::PyMethodMatcher>()?;
    module.add_class::<transforms::PyCodeTransform>()?;
    module.add_class::<transforms::PyFieldTransform>()?;
    module.add_class::<transforms::PyMethodTransform>()?;
    module.add_class::<transforms::PyClassTransform>()?;
    module.add_class::<transforms::PyPipeline>()?;
    module.add_class::<transforms::PyCompiledPipeline>()?;
    model::register(py, module)?;
    analysis::register(py, module)?;
    archive::register(py, module)?;
    attributes::register(py, module)?;
    Ok(())
}
