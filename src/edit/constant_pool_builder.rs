use std::collections::HashMap;

use pyo3::exceptions::{PyIndexError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes, PyList, PyModule, PyType};

use crate::classfile::modified_utf8::{decode_modified_utf8, encode_modified_utf8_bytes};

const TAG_UTF8: u8 = 1;
const TAG_INTEGER: u8 = 3;
const TAG_FLOAT: u8 = 4;
const TAG_LONG: u8 = 5;
const TAG_DOUBLE: u8 = 6;
const TAG_CLASS: u8 = 7;
const TAG_STRING: u8 = 8;
const TAG_FIELDREF: u8 = 9;
const TAG_METHODREF: u8 = 10;
const TAG_INTERFACE_METHODREF: u8 = 11;
const TAG_NAME_AND_TYPE: u8 = 12;
const TAG_METHOD_HANDLE: u8 = 15;
const TAG_METHOD_TYPE: u8 = 16;
const TAG_DYNAMIC: u8 = 17;
const TAG_INVOKE_DYNAMIC: u8 = 18;
const TAG_MODULE: u8 = 19;
const TAG_PACKAGE: u8 = 20;

const CP_MAX_SINGLE_INDEX: usize = 65534;
const CP_MAX_DOUBLE_INDEX: usize = 65533;
const UTF8_MAX_BYTES: usize = 65535;

struct PythonConstantPoolTypes {
    utf8: Py<PyType>,
    integer: Py<PyType>,
    float: Py<PyType>,
    long: Py<PyType>,
    double: Py<PyType>,
    class_info: Py<PyType>,
    string_info: Py<PyType>,
    fieldref: Py<PyType>,
    methodref: Py<PyType>,
    interface_methodref: Py<PyType>,
    name_and_type: Py<PyType>,
    method_handle: Py<PyType>,
    method_type: Py<PyType>,
    dynamic: Py<PyType>,
    invoke_dynamic: Py<PyType>,
    module: Py<PyType>,
    package: Py<PyType>,
}

impl PythonConstantPoolTypes {
    fn load(py: Python<'_>) -> PyResult<Self> {
        let module = py.import("pytecode.classfile.constant_pool")?;
        Ok(Self {
            utf8: module.getattr("Utf8Info")?.cast_into::<PyType>()?.unbind(),
            integer: module
                .getattr("IntegerInfo")?
                .cast_into::<PyType>()?
                .unbind(),
            float: module.getattr("FloatInfo")?.cast_into::<PyType>()?.unbind(),
            long: module.getattr("LongInfo")?.cast_into::<PyType>()?.unbind(),
            double: module
                .getattr("DoubleInfo")?
                .cast_into::<PyType>()?
                .unbind(),
            class_info: module.getattr("ClassInfo")?.cast_into::<PyType>()?.unbind(),
            string_info: module
                .getattr("StringInfo")?
                .cast_into::<PyType>()?
                .unbind(),
            fieldref: module
                .getattr("FieldrefInfo")?
                .cast_into::<PyType>()?
                .unbind(),
            methodref: module
                .getattr("MethodrefInfo")?
                .cast_into::<PyType>()?
                .unbind(),
            interface_methodref: module
                .getattr("InterfaceMethodrefInfo")?
                .cast_into::<PyType>()?
                .unbind(),
            name_and_type: module
                .getattr("NameAndTypeInfo")?
                .cast_into::<PyType>()?
                .unbind(),
            method_handle: module
                .getattr("MethodHandleInfo")?
                .cast_into::<PyType>()?
                .unbind(),
            method_type: module
                .getattr("MethodTypeInfo")?
                .cast_into::<PyType>()?
                .unbind(),
            dynamic: module
                .getattr("DynamicInfo")?
                .cast_into::<PyType>()?
                .unbind(),
            invoke_dynamic: module
                .getattr("InvokeDynamicInfo")?
                .cast_into::<PyType>()?
                .unbind(),
            module: module
                .getattr("ModuleInfo")?
                .cast_into::<PyType>()?
                .unbind(),
            package: module
                .getattr("PackageInfo")?
                .cast_into::<PyType>()?
                .unbind(),
        })
    }

    fn build_entry(&self, py: Python<'_>, data: &EntryData) -> PyResult<Py<PyAny>> {
        let object = match data {
            EntryData::Utf8 {
                index,
                offset,
                length,
                str_bytes,
            } => self.utf8.bind(py).call1((
                *index,
                *offset,
                TAG_UTF8,
                *length,
                PyBytes::new(py, str_bytes),
            ))?,
            EntryData::Integer {
                index,
                offset,
                value_bytes,
            } => {
                self.integer
                    .bind(py)
                    .call1((*index, *offset, TAG_INTEGER, *value_bytes as u64))?
            }
            EntryData::Float {
                index,
                offset,
                value_bytes,
            } => self
                .float
                .bind(py)
                .call1((*index, *offset, TAG_FLOAT, *value_bytes as u64))?,
            EntryData::Long {
                index,
                offset,
                high_bytes,
                low_bytes,
            } => self.long.bind(py).call1((
                *index,
                *offset,
                TAG_LONG,
                *high_bytes as u64,
                *low_bytes as u64,
            ))?,
            EntryData::Double {
                index,
                offset,
                high_bytes,
                low_bytes,
            } => self.double.bind(py).call1((
                *index,
                *offset,
                TAG_DOUBLE,
                *high_bytes as u64,
                *low_bytes as u64,
            ))?,
            EntryData::Class {
                index,
                offset,
                name_index,
            } => self
                .class_info
                .bind(py)
                .call1((*index, *offset, TAG_CLASS, *name_index))?,
            EntryData::String {
                index,
                offset,
                string_index,
            } => self
                .string_info
                .bind(py)
                .call1((*index, *offset, TAG_STRING, *string_index))?,
            EntryData::FieldRef {
                index,
                offset,
                class_index,
                name_and_type_index,
            } => self.fieldref.bind(py).call1((
                *index,
                *offset,
                TAG_FIELDREF,
                *class_index,
                *name_and_type_index,
            ))?,
            EntryData::MethodRef {
                index,
                offset,
                class_index,
                name_and_type_index,
            } => self.methodref.bind(py).call1((
                *index,
                *offset,
                TAG_METHODREF,
                *class_index,
                *name_and_type_index,
            ))?,
            EntryData::InterfaceMethodRef {
                index,
                offset,
                class_index,
                name_and_type_index,
            } => self.interface_methodref.bind(py).call1((
                *index,
                *offset,
                TAG_INTERFACE_METHODREF,
                *class_index,
                *name_and_type_index,
            ))?,
            EntryData::NameAndType {
                index,
                offset,
                name_index,
                descriptor_index,
            } => self.name_and_type.bind(py).call1((
                *index,
                *offset,
                TAG_NAME_AND_TYPE,
                *name_index,
                *descriptor_index,
            ))?,
            EntryData::MethodHandle {
                index,
                offset,
                reference_kind,
                reference_index,
            } => self.method_handle.bind(py).call1((
                *index,
                *offset,
                TAG_METHOD_HANDLE,
                *reference_kind,
                *reference_index,
            ))?,
            EntryData::MethodType {
                index,
                offset,
                descriptor_index,
            } => self.method_type.bind(py).call1((
                *index,
                *offset,
                TAG_METHOD_TYPE,
                *descriptor_index,
            ))?,
            EntryData::Dynamic {
                index,
                offset,
                bootstrap_method_attr_index,
                name_and_type_index,
            } => self.dynamic.bind(py).call1((
                *index,
                *offset,
                TAG_DYNAMIC,
                *bootstrap_method_attr_index,
                *name_and_type_index,
            ))?,
            EntryData::InvokeDynamic {
                index,
                offset,
                bootstrap_method_attr_index,
                name_and_type_index,
            } => self.invoke_dynamic.bind(py).call1((
                *index,
                *offset,
                TAG_INVOKE_DYNAMIC,
                *bootstrap_method_attr_index,
                *name_and_type_index,
            ))?,
            EntryData::Module {
                index,
                offset,
                name_index,
            } => self
                .module
                .bind(py)
                .call1((*index, *offset, TAG_MODULE, *name_index))?,
            EntryData::Package {
                index,
                offset,
                name_index,
            } => self
                .package
                .bind(py)
                .call1((*index, *offset, TAG_PACKAGE, *name_index))?,
        };
        Ok(object.unbind())
    }

    fn clone_ref(&self, py: Python<'_>) -> Self {
        Self {
            utf8: self.utf8.clone_ref(py),
            integer: self.integer.clone_ref(py),
            float: self.float.clone_ref(py),
            long: self.long.clone_ref(py),
            double: self.double.clone_ref(py),
            class_info: self.class_info.clone_ref(py),
            string_info: self.string_info.clone_ref(py),
            fieldref: self.fieldref.clone_ref(py),
            methodref: self.methodref.clone_ref(py),
            interface_methodref: self.interface_methodref.clone_ref(py),
            name_and_type: self.name_and_type.clone_ref(py),
            method_handle: self.method_handle.clone_ref(py),
            method_type: self.method_type.clone_ref(py),
            dynamic: self.dynamic.clone_ref(py),
            invoke_dynamic: self.invoke_dynamic.clone_ref(py),
            module: self.module.clone_ref(py),
            package: self.package.clone_ref(py),
        }
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
enum EntryKey {
    Utf8(Vec<u8>),
    Integer(u32),
    Float(u32),
    Long(u32, u32),
    Double(u32, u32),
    Class(usize),
    String(usize),
    FieldRef(usize, usize),
    MethodRef(usize, usize),
    InterfaceMethodRef(usize, usize),
    NameAndType(usize, usize),
    MethodHandle(u8, usize),
    MethodType(usize),
    Dynamic(usize, usize),
    InvokeDynamic(usize, usize),
    Module(usize),
    Package(usize),
}

#[derive(Clone, Debug)]
enum EntryData {
    Utf8 {
        index: usize,
        offset: usize,
        length: usize,
        str_bytes: Vec<u8>,
    },
    Integer {
        index: usize,
        offset: usize,
        value_bytes: u32,
    },
    Float {
        index: usize,
        offset: usize,
        value_bytes: u32,
    },
    Long {
        index: usize,
        offset: usize,
        high_bytes: u32,
        low_bytes: u32,
    },
    Double {
        index: usize,
        offset: usize,
        high_bytes: u32,
        low_bytes: u32,
    },
    Class {
        index: usize,
        offset: usize,
        name_index: usize,
    },
    String {
        index: usize,
        offset: usize,
        string_index: usize,
    },
    FieldRef {
        index: usize,
        offset: usize,
        class_index: usize,
        name_and_type_index: usize,
    },
    MethodRef {
        index: usize,
        offset: usize,
        class_index: usize,
        name_and_type_index: usize,
    },
    InterfaceMethodRef {
        index: usize,
        offset: usize,
        class_index: usize,
        name_and_type_index: usize,
    },
    NameAndType {
        index: usize,
        offset: usize,
        name_index: usize,
        descriptor_index: usize,
    },
    MethodHandle {
        index: usize,
        offset: usize,
        reference_kind: u8,
        reference_index: usize,
    },
    MethodType {
        index: usize,
        offset: usize,
        descriptor_index: usize,
    },
    Dynamic {
        index: usize,
        offset: usize,
        bootstrap_method_attr_index: usize,
        name_and_type_index: usize,
    },
    InvokeDynamic {
        index: usize,
        offset: usize,
        bootstrap_method_attr_index: usize,
        name_and_type_index: usize,
    },
    Module {
        index: usize,
        offset: usize,
        name_index: usize,
    },
    Package {
        index: usize,
        offset: usize,
        name_index: usize,
    },
}

impl EntryData {
    fn key(&self) -> EntryKey {
        match self {
            Self::Utf8 { str_bytes, .. } => EntryKey::Utf8(str_bytes.clone()),
            Self::Integer { value_bytes, .. } => EntryKey::Integer(*value_bytes),
            Self::Float { value_bytes, .. } => EntryKey::Float(*value_bytes),
            Self::Long {
                high_bytes,
                low_bytes,
                ..
            } => EntryKey::Long(*high_bytes, *low_bytes),
            Self::Double {
                high_bytes,
                low_bytes,
                ..
            } => EntryKey::Double(*high_bytes, *low_bytes),
            Self::Class { name_index, .. } => EntryKey::Class(*name_index),
            Self::String { string_index, .. } => EntryKey::String(*string_index),
            Self::FieldRef {
                class_index,
                name_and_type_index,
                ..
            } => EntryKey::FieldRef(*class_index, *name_and_type_index),
            Self::MethodRef {
                class_index,
                name_and_type_index,
                ..
            } => EntryKey::MethodRef(*class_index, *name_and_type_index),
            Self::InterfaceMethodRef {
                class_index,
                name_and_type_index,
                ..
            } => EntryKey::InterfaceMethodRef(*class_index, *name_and_type_index),
            Self::NameAndType {
                name_index,
                descriptor_index,
                ..
            } => EntryKey::NameAndType(*name_index, *descriptor_index),
            Self::MethodHandle {
                reference_kind,
                reference_index,
                ..
            } => EntryKey::MethodHandle(*reference_kind, *reference_index),
            Self::MethodType {
                descriptor_index, ..
            } => EntryKey::MethodType(*descriptor_index),
            Self::Dynamic {
                bootstrap_method_attr_index,
                name_and_type_index,
                ..
            } => EntryKey::Dynamic(*bootstrap_method_attr_index, *name_and_type_index),
            Self::InvokeDynamic {
                bootstrap_method_attr_index,
                name_and_type_index,
                ..
            } => EntryKey::InvokeDynamic(*bootstrap_method_attr_index, *name_and_type_index),
            Self::Module { name_index, .. } => EntryKey::Module(*name_index),
            Self::Package { name_index, .. } => EntryKey::Package(*name_index),
        }
    }

    fn is_double_slot(&self) -> bool {
        matches!(self, Self::Long { .. } | Self::Double { .. })
    }

    fn with_index_offset(&self, index: usize, offset: usize) -> Self {
        match self {
            Self::Utf8 {
                length, str_bytes, ..
            } => Self::Utf8 {
                index,
                offset,
                length: *length,
                str_bytes: str_bytes.clone(),
            },
            Self::Integer { value_bytes, .. } => Self::Integer {
                index,
                offset,
                value_bytes: *value_bytes,
            },
            Self::Float { value_bytes, .. } => Self::Float {
                index,
                offset,
                value_bytes: *value_bytes,
            },
            Self::Long {
                high_bytes,
                low_bytes,
                ..
            } => Self::Long {
                index,
                offset,
                high_bytes: *high_bytes,
                low_bytes: *low_bytes,
            },
            Self::Double {
                high_bytes,
                low_bytes,
                ..
            } => Self::Double {
                index,
                offset,
                high_bytes: *high_bytes,
                low_bytes: *low_bytes,
            },
            Self::Class { name_index, .. } => Self::Class {
                index,
                offset,
                name_index: *name_index,
            },
            Self::String { string_index, .. } => Self::String {
                index,
                offset,
                string_index: *string_index,
            },
            Self::FieldRef {
                class_index,
                name_and_type_index,
                ..
            } => Self::FieldRef {
                index,
                offset,
                class_index: *class_index,
                name_and_type_index: *name_and_type_index,
            },
            Self::MethodRef {
                class_index,
                name_and_type_index,
                ..
            } => Self::MethodRef {
                index,
                offset,
                class_index: *class_index,
                name_and_type_index: *name_and_type_index,
            },
            Self::InterfaceMethodRef {
                class_index,
                name_and_type_index,
                ..
            } => Self::InterfaceMethodRef {
                index,
                offset,
                class_index: *class_index,
                name_and_type_index: *name_and_type_index,
            },
            Self::NameAndType {
                name_index,
                descriptor_index,
                ..
            } => Self::NameAndType {
                index,
                offset,
                name_index: *name_index,
                descriptor_index: *descriptor_index,
            },
            Self::MethodHandle {
                reference_kind,
                reference_index,
                ..
            } => Self::MethodHandle {
                index,
                offset,
                reference_kind: *reference_kind,
                reference_index: *reference_index,
            },
            Self::MethodType {
                descriptor_index, ..
            } => Self::MethodType {
                index,
                offset,
                descriptor_index: *descriptor_index,
            },
            Self::Dynamic {
                bootstrap_method_attr_index,
                name_and_type_index,
                ..
            } => Self::Dynamic {
                index,
                offset,
                bootstrap_method_attr_index: *bootstrap_method_attr_index,
                name_and_type_index: *name_and_type_index,
            },
            Self::InvokeDynamic {
                bootstrap_method_attr_index,
                name_and_type_index,
                ..
            } => Self::InvokeDynamic {
                index,
                offset,
                bootstrap_method_attr_index: *bootstrap_method_attr_index,
                name_and_type_index: *name_and_type_index,
            },
            Self::Module { name_index, .. } => Self::Module {
                index,
                offset,
                name_index: *name_index,
            },
            Self::Package { name_index, .. } => Self::Package {
                index,
                offset,
                name_index: *name_index,
            },
        }
    }
}

fn type_name(obj: &Bound<'_, PyAny>) -> PyResult<String> {
    Ok(obj.get_type().name()?.to_str()?.to_owned())
}

fn extract_usize_attr(obj: &Bound<'_, PyAny>, name: &str) -> PyResult<usize> {
    Ok(obj.getattr(name)?.extract::<u64>()? as usize)
}

fn extract_u32_attr(obj: &Bound<'_, PyAny>, name: &str) -> PyResult<u32> {
    Ok(obj.getattr(name)?.extract::<i64>()? as u32)
}

fn extract_bytes_like_attr(obj: &Bound<'_, PyAny>, name: &str) -> PyResult<Vec<u8>> {
    obj.getattr(name)?.extract::<Vec<u8>>()
}

fn entry_data_from_object(obj: &Bound<'_, PyAny>) -> PyResult<EntryData> {
    let tag = obj.getattr("tag")?.extract::<u8>()?;
    let index = extract_usize_attr(obj, "index")?;
    let offset = extract_usize_attr(obj, "offset")?;
    match tag {
        TAG_UTF8 => {
            let str_bytes = extract_bytes_like_attr(obj, "str_bytes")?;
            Ok(EntryData::Utf8 {
                index,
                offset,
                length: extract_usize_attr(obj, "length")?,
                str_bytes,
            })
        }
        TAG_INTEGER => Ok(EntryData::Integer {
            index,
            offset,
            value_bytes: extract_u32_attr(obj, "value_bytes")?,
        }),
        TAG_FLOAT => Ok(EntryData::Float {
            index,
            offset,
            value_bytes: extract_u32_attr(obj, "value_bytes")?,
        }),
        TAG_LONG => Ok(EntryData::Long {
            index,
            offset,
            high_bytes: extract_u32_attr(obj, "high_bytes")?,
            low_bytes: extract_u32_attr(obj, "low_bytes")?,
        }),
        TAG_DOUBLE => Ok(EntryData::Double {
            index,
            offset,
            high_bytes: extract_u32_attr(obj, "high_bytes")?,
            low_bytes: extract_u32_attr(obj, "low_bytes")?,
        }),
        TAG_CLASS => Ok(EntryData::Class {
            index,
            offset,
            name_index: extract_usize_attr(obj, "name_index")?,
        }),
        TAG_STRING => Ok(EntryData::String {
            index,
            offset,
            string_index: extract_usize_attr(obj, "string_index")?,
        }),
        TAG_FIELDREF => Ok(EntryData::FieldRef {
            index,
            offset,
            class_index: extract_usize_attr(obj, "class_index")?,
            name_and_type_index: extract_usize_attr(obj, "name_and_type_index")?,
        }),
        TAG_METHODREF => Ok(EntryData::MethodRef {
            index,
            offset,
            class_index: extract_usize_attr(obj, "class_index")?,
            name_and_type_index: extract_usize_attr(obj, "name_and_type_index")?,
        }),
        TAG_INTERFACE_METHODREF => Ok(EntryData::InterfaceMethodRef {
            index,
            offset,
            class_index: extract_usize_attr(obj, "class_index")?,
            name_and_type_index: extract_usize_attr(obj, "name_and_type_index")?,
        }),
        TAG_NAME_AND_TYPE => Ok(EntryData::NameAndType {
            index,
            offset,
            name_index: extract_usize_attr(obj, "name_index")?,
            descriptor_index: extract_usize_attr(obj, "descriptor_index")?,
        }),
        TAG_METHOD_HANDLE => Ok(EntryData::MethodHandle {
            index,
            offset,
            reference_kind: obj.getattr("reference_kind")?.extract::<u8>()?,
            reference_index: extract_usize_attr(obj, "reference_index")?,
        }),
        TAG_METHOD_TYPE => Ok(EntryData::MethodType {
            index,
            offset,
            descriptor_index: extract_usize_attr(obj, "descriptor_index")?,
        }),
        TAG_DYNAMIC => Ok(EntryData::Dynamic {
            index,
            offset,
            bootstrap_method_attr_index: extract_usize_attr(obj, "bootstrap_method_attr_index")?,
            name_and_type_index: extract_usize_attr(obj, "name_and_type_index")?,
        }),
        TAG_INVOKE_DYNAMIC => Ok(EntryData::InvokeDynamic {
            index,
            offset,
            bootstrap_method_attr_index: extract_usize_attr(obj, "bootstrap_method_attr_index")?,
            name_and_type_index: extract_usize_attr(obj, "name_and_type_index")?,
        }),
        TAG_MODULE => Ok(EntryData::Module {
            index,
            offset,
            name_index: extract_usize_attr(obj, "name_index")?,
        }),
        TAG_PACKAGE => Ok(EntryData::Package {
            index,
            offset,
            name_index: extract_usize_attr(obj, "name_index")?,
        }),
        _ => Err(PyValueError::new_err(format!(
            "Unknown constant pool entry tag: {tag}"
        ))),
    }
}

fn require_pool_entry(
    pool: &[Option<EntryData>],
    index: usize,
    context: &str,
) -> PyResult<EntryData> {
    if index == 0 || index >= pool.len() {
        return Err(PyValueError::new_err(format!(
            "{context} index {index} out of range [1, {}]",
            pool.len().saturating_sub(1)
        )));
    }
    match &pool[index] {
        Some(entry) => Ok(entry.clone()),
        None => Err(PyValueError::new_err(format!(
            "{context} index {index} points to an empty constant-pool slot"
        ))),
    }
}

fn method_handle_member_name(
    pool: &[Option<EntryData>],
    reference_entry: &EntryData,
) -> PyResult<String> {
    let name_and_type_index = match reference_entry {
        EntryData::MethodRef {
            name_and_type_index,
            ..
        }
        | EntryData::InterfaceMethodRef {
            name_and_type_index,
            ..
        } => *name_and_type_index,
        other => {
            return Err(PyValueError::new_err(format!(
                "MethodHandle reference target must point to a Methodref/InterfaceMethodref, got {other:?}"
            )));
        }
    };
    let nat_entry = require_pool_entry(pool, name_and_type_index, "MethodHandle name_and_type")?;
    let name_index = match nat_entry {
        EntryData::NameAndType { name_index, .. } => name_index,
        _ => {
            return Err(PyValueError::new_err(
                "MethodHandle reference target must point to a Methodref/InterfaceMethodref whose name_and_type_index resolves to CONSTANT_NameAndType",
            ));
        }
    };
    let name_entry = require_pool_entry(pool, name_index, "MethodHandle member name")?;
    match name_entry {
        EntryData::Utf8 { str_bytes, .. } => decode_modified_utf8(&str_bytes),
        _ => Err(PyValueError::new_err(
            "MethodHandle member name must resolve to CONSTANT_Utf8",
        )),
    }
}

fn validate_utf8_entry(data: &EntryData) -> PyResult<()> {
    if let EntryData::Utf8 {
        length, str_bytes, ..
    } = data
    {
        if *length != str_bytes.len() {
            return Err(PyValueError::new_err(format!(
                "Utf8Info length {length} does not match payload size {}",
                str_bytes.len()
            )));
        }
        if *length > UTF8_MAX_BYTES {
            return Err(PyValueError::new_err(format!(
                "Utf8Info payload exceeds JVM u2 length limit of {UTF8_MAX_BYTES} bytes"
            )));
        }
        decode_modified_utf8(str_bytes)?;
    }
    Ok(())
}

fn validate_method_handle(
    pool: &[Option<EntryData>],
    reference_kind: u8,
    reference_index: usize,
) -> PyResult<()> {
    if !(1..=9).contains(&reference_kind) {
        return Err(PyValueError::new_err(format!(
            "reference_kind must be in range [1, 9], got {reference_kind}"
        )));
    }
    let target = require_pool_entry(pool, reference_index, "MethodHandle reference")?;
    match reference_kind {
        1..=4 => {
            if !matches!(target, EntryData::FieldRef { .. }) {
                return Err(PyValueError::new_err(format!(
                    "reference_kind {reference_kind} requires CONSTANT_Fieldref"
                )));
            }
            Ok(())
        }
        5 | 8 => {
            if !matches!(target, EntryData::MethodRef { .. }) {
                return Err(PyValueError::new_err(format!(
                    "reference_kind {reference_kind} requires CONSTANT_Methodref"
                )));
            }
            let member_name = method_handle_member_name(pool, &target)?;
            if reference_kind == 8 {
                if member_name != "<init>" {
                    return Err(PyValueError::new_err(
                        "reference_kind 8 (REF_newInvokeSpecial) must target a <init> method",
                    ));
                }
                return Ok(());
            }
            if member_name == "<init>" || member_name == "<clinit>" {
                return Err(PyValueError::new_err(format!(
                    "reference_kind {reference_kind} cannot target special method {member_name:?}"
                )));
            }
            Ok(())
        }
        6 | 7 => {
            if !matches!(
                target,
                EntryData::MethodRef { .. } | EntryData::InterfaceMethodRef { .. }
            ) {
                return Err(PyValueError::new_err(format!(
                    "reference_kind {reference_kind} requires CONSTANT_Methodref or CONSTANT_InterfaceMethodref"
                )));
            }
            let member_name = method_handle_member_name(pool, &target)?;
            if member_name == "<init>" || member_name == "<clinit>" {
                return Err(PyValueError::new_err(format!(
                    "reference_kind {reference_kind} cannot target special method {member_name:?}"
                )));
            }
            Ok(())
        }
        9 => {
            if !matches!(target, EntryData::InterfaceMethodRef { .. }) {
                return Err(PyValueError::new_err(
                    "reference_kind 9 requires CONSTANT_InterfaceMethodref",
                ));
            }
            let member_name = method_handle_member_name(pool, &target)?;
            if member_name == "<init>" || member_name == "<clinit>" {
                return Err(PyValueError::new_err(format!(
                    "reference_kind {reference_kind} cannot target special method {member_name:?}"
                )));
            }
            Ok(())
        }
        _ => unreachable!(),
    }
}

fn validate_import_pool(pool: &[Option<EntryData>]) -> PyResult<()> {
    if pool.is_empty() {
        return Err(PyValueError::new_err("constant pool must include index 0"));
    }
    if pool[0].is_some() {
        return Err(PyValueError::new_err("constant pool index 0 must be None"));
    }
    let mut index = 1;
    while index < pool.len() {
        let Some(entry) = &pool[index] else {
            return Err(PyValueError::new_err(format!(
                "constant pool index {index} is None but not reserved as a Long/Double gap"
            )));
        };
        match entry {
            EntryData::Utf8 { index: actual, .. }
            | EntryData::Integer { index: actual, .. }
            | EntryData::Float { index: actual, .. }
            | EntryData::Long { index: actual, .. }
            | EntryData::Double { index: actual, .. }
            | EntryData::Class { index: actual, .. }
            | EntryData::String { index: actual, .. }
            | EntryData::FieldRef { index: actual, .. }
            | EntryData::MethodRef { index: actual, .. }
            | EntryData::InterfaceMethodRef { index: actual, .. }
            | EntryData::NameAndType { index: actual, .. }
            | EntryData::MethodHandle { index: actual, .. }
            | EntryData::MethodType { index: actual, .. }
            | EntryData::Dynamic { index: actual, .. }
            | EntryData::InvokeDynamic { index: actual, .. }
            | EntryData::Module { index: actual, .. }
            | EntryData::Package { index: actual, .. } => {
                if *actual != index {
                    return Err(PyValueError::new_err(format!(
                        "constant pool entry at position {index} reports mismatched index {actual}"
                    )));
                }
            }
        }
        if matches!(entry, EntryData::Utf8 { .. }) {
            validate_utf8_entry(entry)?;
        } else if let EntryData::MethodHandle {
            reference_kind,
            reference_index,
            ..
        } = entry
        {
            validate_method_handle(pool, *reference_kind, *reference_index)?;
        }
        if entry.is_double_slot() {
            let gap_index = index + 1;
            if gap_index >= pool.len() || pool[gap_index].is_some() {
                return Err(PyValueError::new_err(format!(
                    "double-slot entry at index {index} must be followed by a None gap slot"
                )));
            }
            index += 2;
        } else {
            index += 1;
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct NameAndTypeKey(String, String);

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct MemberRefKey(String, String, String);

#[pyclass(module = "pytecode._rust.edit")]
pub struct ConstantPoolBuilder {
    #[pyo3(get)]
    _pool: Py<PyList>,
    #[pyo3(get, set)]
    _next_index: usize,
    checkpoint_pool_len: Option<usize>,
    types: PythonConstantPoolTypes,
    key_to_index: HashMap<EntryKey, usize>,
    utf8_to_index: HashMap<Vec<u8>, usize>,
    string_to_utf8_index: HashMap<String, usize>,
    resolved_utf8_cache: HashMap<usize, (Vec<u8>, String)>,
    class_name_to_index: HashMap<String, usize>,
    string_value_to_index: HashMap<String, usize>,
    name_and_type_to_index: HashMap<NameAndTypeKey, usize>,
    fieldref_to_index: HashMap<MemberRefKey, usize>,
    methodref_to_index: HashMap<MemberRefKey, usize>,
    interface_methodref_to_index: HashMap<MemberRefKey, usize>,
}

impl ConstantPoolBuilder {
    fn new_internal(py: Python<'_>) -> PyResult<Self> {
        let pool = PyList::empty(py);
        pool.append(py.None())?;
        Ok(Self {
            _pool: pool.unbind(),
            _next_index: 1,
            checkpoint_pool_len: None,
            types: PythonConstantPoolTypes::load(py)?,
            key_to_index: HashMap::new(),
            utf8_to_index: HashMap::new(),
            string_to_utf8_index: HashMap::new(),
            resolved_utf8_cache: HashMap::new(),
            class_name_to_index: HashMap::new(),
            string_value_to_index: HashMap::new(),
            name_and_type_to_index: HashMap::new(),
            fieldref_to_index: HashMap::new(),
            methodref_to_index: HashMap::new(),
            interface_methodref_to_index: HashMap::new(),
        })
    }

    fn pool_list<'py>(&self, py: Python<'py>) -> Bound<'py, PyList> {
        self._pool.bind(py).clone()
    }

    fn checkpoint_pool_len(&self, py: Python<'_>) -> usize {
        self.checkpoint_pool_len
            .unwrap_or_else(|| self.pool_list(py).len())
    }

    fn clone_builder(&self, py: Python<'_>) -> PyResult<Self> {
        let new_pool = PyList::empty(py);
        for item in self.pool_list(py).iter() {
            if item.is_none() {
                new_pool.append(py.None())?;
            } else {
                new_pool.append(copy_entry(py, &self.types, &item)?)?;
            }
        }
        Ok(Self {
            _pool: new_pool.unbind(),
            _next_index: self._next_index,
            checkpoint_pool_len: None,
            types: self.types.clone_ref(py),
            key_to_index: self.key_to_index.clone(),
            utf8_to_index: self.utf8_to_index.clone(),
            string_to_utf8_index: self.string_to_utf8_index.clone(),
            resolved_utf8_cache: self.resolved_utf8_cache.clone(),
            class_name_to_index: self.class_name_to_index.clone(),
            string_value_to_index: self.string_value_to_index.clone(),
            name_and_type_to_index: self.name_and_type_to_index.clone(),
            fieldref_to_index: self.fieldref_to_index.clone(),
            methodref_to_index: self.methodref_to_index.clone(),
            interface_methodref_to_index: self.interface_methodref_to_index.clone(),
        })
    }

    fn checkpoint_builder(&self, py: Python<'_>) -> Self {
        Self {
            _pool: self._pool.clone_ref(py),
            _next_index: self._next_index,
            checkpoint_pool_len: Some(self.pool_list(py).len()),
            types: self.types.clone_ref(py),
            key_to_index: HashMap::new(),
            utf8_to_index: HashMap::new(),
            string_to_utf8_index: HashMap::new(),
            resolved_utf8_cache: HashMap::new(),
            class_name_to_index: HashMap::new(),
            string_value_to_index: HashMap::new(),
            name_and_type_to_index: HashMap::new(),
            fieldref_to_index: HashMap::new(),
            methodref_to_index: HashMap::new(),
            interface_methodref_to_index: HashMap::new(),
        }
    }

    fn truncate_pool_to(&mut self, py: Python<'_>, pool_len: usize) -> PyResult<()> {
        let pool = self.pool_list(py);
        while pool.len() > pool_len {
            pool.call_method0("pop")?;
        }
        Ok(())
    }

    fn rebuild_indexes_from_pool(&mut self, py: Python<'_>) -> PyResult<()> {
        let mut key_to_index = HashMap::new();
        let mut utf8_to_index = HashMap::new();
        for item in self.pool_list(py).iter() {
            if item.is_none() {
                continue;
            }
            let data = entry_data_from_object(&item)?;
            key_to_index.entry(data.key()).or_insert(data_index(&data));
            if let EntryData::Utf8 {
                str_bytes, index, ..
            } = data
            {
                utf8_to_index.entry(str_bytes).or_insert(index);
            }
        }
        self.key_to_index = key_to_index;
        self.utf8_to_index = utf8_to_index;
        self.string_to_utf8_index.clear();
        self.resolved_utf8_cache.clear();
        self.class_name_to_index.clear();
        self.string_value_to_index.clear();
        self.name_and_type_to_index.clear();
        self.fieldref_to_index.clear();
        self.methodref_to_index.clear();
        self.interface_methodref_to_index.clear();
        Ok(())
    }

    fn restore_from_checkpoint(
        &mut self,
        py: Python<'_>,
        checkpoint: &ConstantPoolBuilder,
    ) -> PyResult<()> {
        let pool_len = checkpoint.checkpoint_pool_len(py);
        self._pool = checkpoint._pool.clone_ref(py);
        self.truncate_pool_to(py, pool_len)?;
        self._next_index = checkpoint._next_index;
        self.checkpoint_pool_len = None;
        self.rebuild_indexes_from_pool(py)
    }

    fn allocate(&mut self, py: Python<'_>, data: EntryData) -> PyResult<usize> {
        let key = data.key();
        if let Some(index) = self.key_to_index.get(&key) {
            return Ok(*index);
        }

        let double = data.is_double_slot();
        let limit = if double {
            CP_MAX_DOUBLE_INDEX
        } else {
            CP_MAX_SINGLE_INDEX
        };
        if self._next_index > limit {
            return Err(PyValueError::new_err(format!(
                "Constant pool overflow: cannot add {}entry at index {} (maximum is {limit})",
                if double { "double-slot " } else { "" },
                self._next_index
            )));
        }

        let index = self._next_index;
        let stored = data.with_index_offset(index, 0);
        let object = self.types.build_entry(py, &stored)?;
        self.pool_list(py).append(object)?;
        self.key_to_index.insert(key, index);
        if let EntryData::Utf8 { str_bytes, .. } = &stored {
            self.utf8_to_index.insert(str_bytes.clone(), index);
        }
        if double {
            self.pool_list(py).append(py.None())?;
            self._next_index += 2;
        } else {
            self._next_index += 1;
        }
        Ok(index)
    }
}

fn copy_entry(
    py: Python<'_>,
    types: &PythonConstantPoolTypes,
    entry: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let data = entry_data_from_object(entry)?;
    types.build_entry(py, &data)
}

#[pymethods]
impl ConstantPoolBuilder {
    #[new]
    fn new(py: Python<'_>) -> PyResult<Self> {
        Self::new_internal(py)
    }

    #[classmethod]
    #[pyo3(signature = (pool, *, skip_validation=false))]
    fn from_pool(
        _cls: &Bound<'_, PyType>,
        py: Python<'_>,
        pool: &Bound<'_, PyList>,
        skip_validation: bool,
    ) -> PyResult<Self> {
        let types = PythonConstantPoolTypes::load(py)?;
        let mut pool_data = Vec::with_capacity(pool.len());
        for item in pool.iter() {
            if item.is_none() {
                pool_data.push(None);
            } else {
                pool_data.push(Some(entry_data_from_object(&item)?));
            }
        }
        if !skip_validation {
            validate_import_pool(&pool_data)?;
        }

        let copied_pool = PyList::empty(py);
        let mut key_to_index = HashMap::new();
        let mut utf8_to_index = HashMap::new();
        for item in pool.iter() {
            if item.is_none() {
                copied_pool.append(py.None())?;
                continue;
            }
            let copy = copy_entry(py, &types, &item)?;
            let data = entry_data_from_object(copy.bind(py))?;
            key_to_index.entry(data.key()).or_insert(data_index(&data));
            if let EntryData::Utf8 {
                str_bytes, index, ..
            } = data
            {
                utf8_to_index.entry(str_bytes).or_insert(index);
            }
            copied_pool.append(copy)?;
        }
        Ok(Self {
            _pool: copied_pool.unbind(),
            _next_index: pool.len(),
            checkpoint_pool_len: None,
            types,
            key_to_index,
            utf8_to_index,
            string_to_utf8_index: HashMap::new(),
            resolved_utf8_cache: HashMap::new(),
            class_name_to_index: HashMap::new(),
            string_value_to_index: HashMap::new(),
            name_and_type_to_index: HashMap::new(),
            fieldref_to_index: HashMap::new(),
            methodref_to_index: HashMap::new(),
            interface_methodref_to_index: HashMap::new(),
        })
    }

    fn clone(&self, py: Python<'_>) -> PyResult<Self> {
        self.clone_builder(py)
    }

    fn checkpoint(&self, py: Python<'_>) -> PyResult<Self> {
        Ok(self.checkpoint_builder(py))
    }

    fn rollback(
        &mut self,
        py: Python<'_>,
        checkpoint: PyRef<'_, ConstantPoolBuilder>,
    ) -> PyResult<()> {
        self.restore_from_checkpoint(py, &checkpoint)
    }

    fn add_entry(&mut self, py: Python<'_>, entry: &Bound<'_, PyAny>) -> PyResult<usize> {
        let data = entry_data_from_object(entry)?.with_index_offset(0, 0);
        if matches!(data, EntryData::Utf8 { .. }) {
            validate_utf8_entry(&data)?;
        } else if let EntryData::MethodHandle {
            reference_kind,
            reference_index,
            ..
        } = data
        {
            let pool_data = pool_data_from_list(&self.pool_list(py))?;
            validate_method_handle(&pool_data, reference_kind, reference_index)?;
        }
        self.allocate(py, data)
    }

    fn add_utf8(&mut self, py: Python<'_>, value: &str) -> PyResult<usize> {
        if let Some(index) = self.string_to_utf8_index.get(value) {
            return Ok(*index);
        }
        let encoded = encode_modified_utf8_bytes(value);
        if encoded.len() > UTF8_MAX_BYTES {
            return Err(PyValueError::new_err(format!(
                "Modified UTF-8 payload exceeds JVM u2 length limit of {UTF8_MAX_BYTES} bytes"
            )));
        }
        if let Some(index) = self.utf8_to_index.get(&encoded) {
            self.string_to_utf8_index.insert(value.to_owned(), *index);
            return Ok(*index);
        }
        let index = self.allocate(
            py,
            EntryData::Utf8 {
                index: 0,
                offset: 0,
                length: encoded.len(),
                str_bytes: encoded.clone(),
            },
        )?;
        self.string_to_utf8_index.insert(value.to_owned(), index);
        self.resolved_utf8_cache
            .insert(index, (encoded, value.to_owned()));
        Ok(index)
    }

    fn add_integer(&mut self, py: Python<'_>, value: i64) -> PyResult<usize> {
        self.allocate(
            py,
            EntryData::Integer {
                index: 0,
                offset: 0,
                value_bytes: value as u32,
            },
        )
    }

    fn add_float(&mut self, py: Python<'_>, raw_bits: i64) -> PyResult<usize> {
        self.allocate(
            py,
            EntryData::Float {
                index: 0,
                offset: 0,
                value_bytes: raw_bits as u32,
            },
        )
    }

    fn add_long(&mut self, py: Python<'_>, high: i64, low: i64) -> PyResult<usize> {
        self.allocate(
            py,
            EntryData::Long {
                index: 0,
                offset: 0,
                high_bytes: high as u32,
                low_bytes: low as u32,
            },
        )
    }

    fn add_double(&mut self, py: Python<'_>, high: i64, low: i64) -> PyResult<usize> {
        self.allocate(
            py,
            EntryData::Double {
                index: 0,
                offset: 0,
                high_bytes: high as u32,
                low_bytes: low as u32,
            },
        )
    }

    fn add_class(&mut self, py: Python<'_>, name: &str) -> PyResult<usize> {
        if let Some(index) = self.class_name_to_index.get(name) {
            return Ok(*index);
        }
        let name_index = self.add_utf8(py, name)?;
        let index = self.allocate(
            py,
            EntryData::Class {
                index: 0,
                offset: 0,
                name_index,
            },
        )?;
        self.class_name_to_index.insert(name.to_owned(), index);
        Ok(index)
    }

    fn add_string(&mut self, py: Python<'_>, value: &str) -> PyResult<usize> {
        if let Some(index) = self.string_value_to_index.get(value) {
            return Ok(*index);
        }
        let string_index = self.add_utf8(py, value)?;
        let index = self.allocate(
            py,
            EntryData::String {
                index: 0,
                offset: 0,
                string_index,
            },
        )?;
        self.string_value_to_index.insert(value.to_owned(), index);
        Ok(index)
    }

    fn add_name_and_type(
        &mut self,
        py: Python<'_>,
        name: &str,
        descriptor: &str,
    ) -> PyResult<usize> {
        let key = NameAndTypeKey(name.to_owned(), descriptor.to_owned());
        if let Some(index) = self.name_and_type_to_index.get(&key) {
            return Ok(*index);
        }
        let name_index = self.add_utf8(py, name)?;
        let descriptor_index = self.add_utf8(py, descriptor)?;
        let index = self.allocate(
            py,
            EntryData::NameAndType {
                index: 0,
                offset: 0,
                name_index,
                descriptor_index,
            },
        )?;
        self.name_and_type_to_index.insert(key, index);
        Ok(index)
    }

    fn add_fieldref(
        &mut self,
        py: Python<'_>,
        class_name: &str,
        field_name: &str,
        descriptor: &str,
    ) -> PyResult<usize> {
        let key = MemberRefKey(
            class_name.to_owned(),
            field_name.to_owned(),
            descriptor.to_owned(),
        );
        if let Some(index) = self.fieldref_to_index.get(&key) {
            return Ok(*index);
        }
        let class_index = self.add_class(py, class_name)?;
        let name_and_type_index = self.add_name_and_type(py, field_name, descriptor)?;
        let index = self.allocate(
            py,
            EntryData::FieldRef {
                index: 0,
                offset: 0,
                class_index,
                name_and_type_index,
            },
        )?;
        self.fieldref_to_index.insert(key, index);
        Ok(index)
    }

    fn add_methodref(
        &mut self,
        py: Python<'_>,
        class_name: &str,
        method_name: &str,
        descriptor: &str,
    ) -> PyResult<usize> {
        let key = MemberRefKey(
            class_name.to_owned(),
            method_name.to_owned(),
            descriptor.to_owned(),
        );
        if let Some(index) = self.methodref_to_index.get(&key) {
            return Ok(*index);
        }
        let class_index = self.add_class(py, class_name)?;
        let name_and_type_index = self.add_name_and_type(py, method_name, descriptor)?;
        let index = self.allocate(
            py,
            EntryData::MethodRef {
                index: 0,
                offset: 0,
                class_index,
                name_and_type_index,
            },
        )?;
        self.methodref_to_index.insert(key, index);
        Ok(index)
    }

    fn add_interface_methodref(
        &mut self,
        py: Python<'_>,
        class_name: &str,
        method_name: &str,
        descriptor: &str,
    ) -> PyResult<usize> {
        let key = MemberRefKey(
            class_name.to_owned(),
            method_name.to_owned(),
            descriptor.to_owned(),
        );
        if let Some(index) = self.interface_methodref_to_index.get(&key) {
            return Ok(*index);
        }
        let class_index = self.add_class(py, class_name)?;
        let name_and_type_index = self.add_name_and_type(py, method_name, descriptor)?;
        let index = self.allocate(
            py,
            EntryData::InterfaceMethodRef {
                index: 0,
                offset: 0,
                class_index,
                name_and_type_index,
            },
        )?;
        self.interface_methodref_to_index.insert(key, index);
        Ok(index)
    }

    fn add_method_handle(
        &mut self,
        py: Python<'_>,
        reference_kind: u8,
        reference_index: usize,
    ) -> PyResult<usize> {
        let pool_data = pool_data_from_list(&self.pool_list(py))?;
        validate_method_handle(&pool_data, reference_kind, reference_index)?;
        self.allocate(
            py,
            EntryData::MethodHandle {
                index: 0,
                offset: 0,
                reference_kind,
                reference_index,
            },
        )
    }

    fn add_method_type(&mut self, py: Python<'_>, descriptor: &str) -> PyResult<usize> {
        let descriptor_index = self.add_utf8(py, descriptor)?;
        self.allocate(
            py,
            EntryData::MethodType {
                index: 0,
                offset: 0,
                descriptor_index,
            },
        )
    }

    fn add_dynamic(
        &mut self,
        py: Python<'_>,
        bootstrap_method_attr_index: usize,
        name: &str,
        descriptor: &str,
    ) -> PyResult<usize> {
        let name_and_type_index = self.add_name_and_type(py, name, descriptor)?;
        self.allocate(
            py,
            EntryData::Dynamic {
                index: 0,
                offset: 0,
                bootstrap_method_attr_index,
                name_and_type_index,
            },
        )
    }

    fn add_invoke_dynamic(
        &mut self,
        py: Python<'_>,
        bootstrap_method_attr_index: usize,
        name: &str,
        descriptor: &str,
    ) -> PyResult<usize> {
        let name_and_type_index = self.add_name_and_type(py, name, descriptor)?;
        self.allocate(
            py,
            EntryData::InvokeDynamic {
                index: 0,
                offset: 0,
                bootstrap_method_attr_index,
                name_and_type_index,
            },
        )
    }

    fn add_module(&mut self, py: Python<'_>, name: &str) -> PyResult<usize> {
        let name_index = self.add_utf8(py, name)?;
        self.allocate(
            py,
            EntryData::Module {
                index: 0,
                offset: 0,
                name_index,
            },
        )
    }

    fn add_package(&mut self, py: Python<'_>, name: &str) -> PyResult<usize> {
        let name_index = self.add_utf8(py, name)?;
        self.allocate(
            py,
            EntryData::Package {
                index: 0,
                offset: 0,
                name_index,
            },
        )
    }

    fn get(&self, py: Python<'_>, index: isize) -> PyResult<Py<PyAny>> {
        let pool = self.pool_list(py);
        if index < 0 || index as usize >= pool.len() {
            return Err(PyIndexError::new_err(format!(
                "CP index {index} out of range [0, {}]",
                pool.len().saturating_sub(1)
            )));
        }
        let item = pool.get_item(index as usize)?;
        if item.is_none() {
            return Ok(py.None());
        }
        copy_entry(py, &self.types, &item)
    }

    fn peek(&self, py: Python<'_>, index: isize) -> PyResult<Py<PyAny>> {
        let pool = self.pool_list(py);
        if index < 0 || index as usize >= pool.len() {
            return Err(PyIndexError::new_err(format!(
                "CP index {index} out of range [0, {}]",
                pool.len().saturating_sub(1)
            )));
        }
        Ok(pool.get_item(index as usize)?.unbind())
    }

    fn find_utf8(&mut self, value: &str) -> PyResult<Option<usize>> {
        if let Some(index) = self.string_to_utf8_index.get(value) {
            return Ok(Some(*index));
        }
        let encoded = encode_modified_utf8_bytes(value);
        let existing = self.utf8_to_index.get(&encoded).copied();
        if let Some(index) = existing {
            self.string_to_utf8_index.insert(value.to_owned(), index);
        }
        Ok(existing)
    }

    fn find_integer(&self, value: i64) -> Option<usize> {
        self.key_to_index
            .get(&EntryKey::Integer(value as u32))
            .copied()
    }

    fn find_float(&self, raw_bits: i64) -> Option<usize> {
        self.key_to_index
            .get(&EntryKey::Float(raw_bits as u32))
            .copied()
    }

    fn find_long(&self, high: i64, low: i64) -> Option<usize> {
        self.key_to_index
            .get(&EntryKey::Long(high as u32, low as u32))
            .copied()
    }

    fn find_double(&self, high: i64, low: i64) -> Option<usize> {
        self.key_to_index
            .get(&EntryKey::Double(high as u32, low as u32))
            .copied()
    }

    fn find_class(&mut self, name: &str) -> PyResult<Option<usize>> {
        if let Some(index) = self.class_name_to_index.get(name) {
            return Ok(Some(*index));
        }
        let Some(utf8_index) = self.find_utf8(name)? else {
            return Ok(None);
        };
        let existing = self.key_to_index.get(&EntryKey::Class(utf8_index)).copied();
        if let Some(index) = existing {
            self.class_name_to_index.insert(name.to_owned(), index);
        }
        Ok(existing)
    }

    fn find_string(&mut self, value: &str) -> PyResult<Option<usize>> {
        if let Some(index) = self.string_value_to_index.get(value) {
            return Ok(Some(*index));
        }
        let Some(string_index) = self.find_utf8(value)? else {
            return Ok(None);
        };
        let existing = self
            .key_to_index
            .get(&EntryKey::String(string_index))
            .copied();
        if let Some(index) = existing {
            self.string_value_to_index.insert(value.to_owned(), index);
        }
        Ok(existing)
    }

    fn find_method_type(&mut self, descriptor: &str) -> PyResult<Option<usize>> {
        let Some(descriptor_index) = self.find_utf8(descriptor)? else {
            return Ok(None);
        };
        Ok(self
            .key_to_index
            .get(&EntryKey::MethodType(descriptor_index))
            .copied())
    }

    fn find_name_and_type(&mut self, name: &str, descriptor: &str) -> PyResult<Option<usize>> {
        let key = NameAndTypeKey(name.to_owned(), descriptor.to_owned());
        if let Some(index) = self.name_and_type_to_index.get(&key) {
            return Ok(Some(*index));
        }
        let Some(name_index) = self.find_utf8(name)? else {
            return Ok(None);
        };
        let Some(descriptor_index) = self.find_utf8(descriptor)? else {
            return Ok(None);
        };
        let existing = self
            .key_to_index
            .get(&EntryKey::NameAndType(name_index, descriptor_index))
            .copied();
        if let Some(index) = existing {
            self.name_and_type_to_index.insert(key, index);
        }
        Ok(existing)
    }

    fn find_fieldref(
        &mut self,
        class_name: &str,
        field_name: &str,
        descriptor: &str,
    ) -> PyResult<Option<usize>> {
        let key = MemberRefKey(
            class_name.to_owned(),
            field_name.to_owned(),
            descriptor.to_owned(),
        );
        if let Some(index) = self.fieldref_to_index.get(&key) {
            return Ok(Some(*index));
        }
        let Some(class_index) = self.find_class(class_name)? else {
            return Ok(None);
        };
        let Some(name_and_type_index) = self.find_name_and_type(field_name, descriptor)? else {
            return Ok(None);
        };
        let existing = self
            .key_to_index
            .get(&EntryKey::FieldRef(class_index, name_and_type_index))
            .copied();
        if let Some(index) = existing {
            self.fieldref_to_index.insert(key, index);
        }
        Ok(existing)
    }

    fn find_methodref(
        &mut self,
        class_name: &str,
        method_name: &str,
        descriptor: &str,
    ) -> PyResult<Option<usize>> {
        let key = MemberRefKey(
            class_name.to_owned(),
            method_name.to_owned(),
            descriptor.to_owned(),
        );
        if let Some(index) = self.methodref_to_index.get(&key) {
            return Ok(Some(*index));
        }
        let Some(class_index) = self.find_class(class_name)? else {
            return Ok(None);
        };
        let Some(name_and_type_index) = self.find_name_and_type(method_name, descriptor)? else {
            return Ok(None);
        };
        let existing = self
            .key_to_index
            .get(&EntryKey::MethodRef(class_index, name_and_type_index))
            .copied();
        if let Some(index) = existing {
            self.methodref_to_index.insert(key, index);
        }
        Ok(existing)
    }

    fn find_interface_methodref(
        &mut self,
        class_name: &str,
        method_name: &str,
        descriptor: &str,
    ) -> PyResult<Option<usize>> {
        let key = MemberRefKey(
            class_name.to_owned(),
            method_name.to_owned(),
            descriptor.to_owned(),
        );
        if let Some(index) = self.interface_methodref_to_index.get(&key) {
            return Ok(Some(*index));
        }
        let Some(class_index) = self.find_class(class_name)? else {
            return Ok(None);
        };
        let Some(name_and_type_index) = self.find_name_and_type(method_name, descriptor)? else {
            return Ok(None);
        };
        let existing = self
            .key_to_index
            .get(&EntryKey::InterfaceMethodRef(
                class_index,
                name_and_type_index,
            ))
            .copied();
        if let Some(index) = existing {
            self.interface_methodref_to_index.insert(key, index);
        }
        Ok(existing)
    }

    fn find_method_handle(&self, reference_kind: u8, reference_index: usize) -> Option<usize> {
        self.key_to_index
            .get(&EntryKey::MethodHandle(reference_kind, reference_index))
            .copied()
    }

    fn find_dynamic(
        &mut self,
        bootstrap_method_attr_index: usize,
        name: &str,
        descriptor: &str,
    ) -> PyResult<Option<usize>> {
        let Some(name_and_type_index) = self.find_name_and_type(name, descriptor)? else {
            return Ok(None);
        };
        Ok(self
            .key_to_index
            .get(&EntryKey::Dynamic(
                bootstrap_method_attr_index,
                name_and_type_index,
            ))
            .copied())
    }

    fn resolve_utf8(&mut self, py: Python<'_>, index: usize) -> PyResult<String> {
        let entry = self.pool_list(py).get_item(index)?;
        let data = if entry.is_none() {
            None
        } else {
            Some(entry_data_from_object(&entry)?)
        };
        let Some(EntryData::Utf8 { str_bytes, .. }) = data else {
            let name = type_name(&entry)?;
            return Err(PyValueError::new_err(format!(
                "CP index {index} is not a CONSTANT_Utf8 entry: {name}"
            )));
        };
        if let Some((cached_bytes, cached_value)) = self.resolved_utf8_cache.get(&index)
            && cached_bytes == &str_bytes
        {
            return Ok(cached_value.clone());
        }
        let value = decode_modified_utf8(&str_bytes)?;
        self.resolved_utf8_cache
            .insert(index, (str_bytes.clone(), value.clone()));
        let resolved_index = self.utf8_to_index.get(&str_bytes).copied().unwrap_or(index);
        self.string_to_utf8_index
            .entry(value.clone())
            .or_insert(resolved_index);
        Ok(value)
    }

    fn build(&self, py: Python<'_>) -> PyResult<Py<PyList>> {
        let out = PyList::empty(py);
        for item in self.pool_list(py).iter() {
            if item.is_none() {
                out.append(py.None())?;
            } else {
                out.append(copy_entry(py, &self.types, &item)?)?;
            }
        }
        Ok(out.unbind())
    }

    #[getter]
    fn count(&self) -> usize {
        self._next_index
    }

    fn __len__(&self, py: Python<'_>) -> PyResult<usize> {
        let mut count = 0;
        for item in self.pool_list(py).iter() {
            if !item.is_none() {
                count += 1;
            }
        }
        Ok(count)
    }
}

fn pool_data_from_list(pool: &Bound<'_, PyList>) -> PyResult<Vec<Option<EntryData>>> {
    let mut out = Vec::with_capacity(pool.len());
    for item in pool.iter() {
        if item.is_none() {
            out.push(None);
        } else {
            out.push(Some(entry_data_from_object(&item)?));
        }
    }
    Ok(out)
}

fn data_index(data: &EntryData) -> usize {
    match data {
        EntryData::Utf8 { index, .. }
        | EntryData::Integer { index, .. }
        | EntryData::Float { index, .. }
        | EntryData::Long { index, .. }
        | EntryData::Double { index, .. }
        | EntryData::Class { index, .. }
        | EntryData::String { index, .. }
        | EntryData::FieldRef { index, .. }
        | EntryData::MethodRef { index, .. }
        | EntryData::InterfaceMethodRef { index, .. }
        | EntryData::NameAndType { index, .. }
        | EntryData::MethodHandle { index, .. }
        | EntryData::MethodType { index, .. }
        | EntryData::Dynamic { index, .. }
        | EntryData::InvokeDynamic { index, .. }
        | EntryData::Module { index, .. }
        | EntryData::Package { index, .. } => *index,
    }
}

pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    parent.add_class::<ConstantPoolBuilder>()?;
    Ok(())
}
