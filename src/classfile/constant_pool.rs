use pyo3::IntoPyObjectExt;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyList};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ConstantPoolTag {
    Utf8 = 1,
    Integer = 3,
    Float = 4,
    Long = 5,
    Double = 6,
    Class = 7,
    String = 8,
    FieldRef = 9,
    MethodRef = 10,
    InterfaceMethodRef = 11,
    NameAndType = 12,
    MethodHandle = 15,
    MethodType = 16,
    Dynamic = 17,
    InvokeDynamic = 18,
    Module = 19,
    Package = 20,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConstantPoolEntry {
    Utf8 {
        index: usize,
        offset: usize,
        length: u16,
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
        name_index: u16,
    },
    String {
        index: usize,
        offset: usize,
        string_index: u16,
    },
    FieldRef {
        index: usize,
        offset: usize,
        class_index: u16,
        name_and_type_index: u16,
    },
    MethodRef {
        index: usize,
        offset: usize,
        class_index: u16,
        name_and_type_index: u16,
    },
    InterfaceMethodRef {
        index: usize,
        offset: usize,
        class_index: u16,
        name_and_type_index: u16,
    },
    NameAndType {
        index: usize,
        offset: usize,
        name_index: u16,
        descriptor_index: u16,
    },
    MethodHandle {
        index: usize,
        offset: usize,
        reference_kind: u8,
        reference_index: u16,
    },
    MethodType {
        index: usize,
        offset: usize,
        descriptor_index: u16,
    },
    Dynamic {
        index: usize,
        offset: usize,
        bootstrap_method_attr_index: u16,
        name_and_type_index: u16,
    },
    InvokeDynamic {
        index: usize,
        offset: usize,
        bootstrap_method_attr_index: u16,
        name_and_type_index: u16,
    },
    Module {
        index: usize,
        offset: usize,
        name_index: u16,
    },
    Package {
        index: usize,
        offset: usize,
        name_index: u16,
    },
}

impl ConstantPoolEntry {
    pub const fn tag(&self) -> ConstantPoolTag {
        match self {
            Self::Utf8 { .. } => ConstantPoolTag::Utf8,
            Self::Integer { .. } => ConstantPoolTag::Integer,
            Self::Float { .. } => ConstantPoolTag::Float,
            Self::Long { .. } => ConstantPoolTag::Long,
            Self::Double { .. } => ConstantPoolTag::Double,
            Self::Class { .. } => ConstantPoolTag::Class,
            Self::String { .. } => ConstantPoolTag::String,
            Self::FieldRef { .. } => ConstantPoolTag::FieldRef,
            Self::MethodRef { .. } => ConstantPoolTag::MethodRef,
            Self::InterfaceMethodRef { .. } => ConstantPoolTag::InterfaceMethodRef,
            Self::NameAndType { .. } => ConstantPoolTag::NameAndType,
            Self::MethodHandle { .. } => ConstantPoolTag::MethodHandle,
            Self::MethodType { .. } => ConstantPoolTag::MethodType,
            Self::Dynamic { .. } => ConstantPoolTag::Dynamic,
            Self::InvokeDynamic { .. } => ConstantPoolTag::InvokeDynamic,
            Self::Module { .. } => ConstantPoolTag::Module,
            Self::Package { .. } => ConstantPoolTag::Package,
        }
    }

    fn to_python<'py>(&self, py: Python<'py>) -> PyResult<Py<PyAny>> {
        let item = match self {
            Self::Utf8 {
                index,
                offset,
                length,
                str_bytes,
            } => (
                self.tag() as u8,
                *index,
                *offset,
                *length,
                PyBytes::new(py, str_bytes),
            )
                .into_py_any(py)?,
            Self::Integer {
                index,
                offset,
                value_bytes,
            }
            | Self::Float {
                index,
                offset,
                value_bytes,
            } => (self.tag() as u8, *index, *offset, *value_bytes).into_py_any(py)?,
            Self::Long {
                index,
                offset,
                high_bytes,
                low_bytes,
            }
            | Self::Double {
                index,
                offset,
                high_bytes,
                low_bytes,
            } => (self.tag() as u8, *index, *offset, *high_bytes, *low_bytes).into_py_any(py)?,
            Self::Class {
                index,
                offset,
                name_index,
            }
            | Self::MethodType {
                index,
                offset,
                descriptor_index: name_index,
            }
            | Self::Module {
                index,
                offset,
                name_index,
            }
            | Self::Package {
                index,
                offset,
                name_index,
            }
            | Self::String {
                index,
                offset,
                string_index: name_index,
            } => (self.tag() as u8, *index, *offset, *name_index).into_py_any(py)?,
            Self::FieldRef {
                index,
                offset,
                class_index,
                name_and_type_index,
            }
            | Self::MethodRef {
                index,
                offset,
                class_index,
                name_and_type_index,
            }
            | Self::InterfaceMethodRef {
                index,
                offset,
                class_index,
                name_and_type_index,
            }
            | Self::Dynamic {
                index,
                offset,
                bootstrap_method_attr_index: class_index,
                name_and_type_index,
            }
            | Self::InvokeDynamic {
                index,
                offset,
                bootstrap_method_attr_index: class_index,
                name_and_type_index,
            }
            | Self::NameAndType {
                index,
                offset,
                name_index: class_index,
                descriptor_index: name_and_type_index,
            } => (
                self.tag() as u8,
                *index,
                *offset,
                *class_index,
                *name_and_type_index,
            )
                .into_py_any(py)?,
            Self::MethodHandle {
                index,
                offset,
                reference_kind,
                reference_index,
            } => (
                self.tag() as u8,
                *index,
                *offset,
                *reference_kind,
                *reference_index,
            )
                .into_py_any(py)?,
        };
        Ok(item)
    }
}

struct SliceReader<'a> {
    data: &'a [u8],
    offset: usize,
}

fn read_constant_pool_entry(
    reader: &mut SliceReader<'_>,
    index: usize,
) -> PyResult<(ConstantPoolEntry, usize)> {
    let offset = reader.offset;
    let tag = reader.read_u1()?;
    let entry = match tag {
        1 => {
            let length = reader.read_u2()?;
            ConstantPoolEntry::Utf8 {
                index,
                offset,
                length,
                str_bytes: reader.read_bytes(length as usize)?.to_vec(),
            }
        }
        3 => ConstantPoolEntry::Integer {
            index,
            offset,
            value_bytes: reader.read_u4()?,
        },
        4 => ConstantPoolEntry::Float {
            index,
            offset,
            value_bytes: reader.read_u4()?,
        },
        5 => ConstantPoolEntry::Long {
            index,
            offset,
            high_bytes: reader.read_u4()?,
            low_bytes: reader.read_u4()?,
        },
        6 => ConstantPoolEntry::Double {
            index,
            offset,
            high_bytes: reader.read_u4()?,
            low_bytes: reader.read_u4()?,
        },
        7 => ConstantPoolEntry::Class {
            index,
            offset,
            name_index: reader.read_u2()?,
        },
        8 => ConstantPoolEntry::String {
            index,
            offset,
            string_index: reader.read_u2()?,
        },
        9 => ConstantPoolEntry::FieldRef {
            index,
            offset,
            class_index: reader.read_u2()?,
            name_and_type_index: reader.read_u2()?,
        },
        10 => ConstantPoolEntry::MethodRef {
            index,
            offset,
            class_index: reader.read_u2()?,
            name_and_type_index: reader.read_u2()?,
        },
        11 => ConstantPoolEntry::InterfaceMethodRef {
            index,
            offset,
            class_index: reader.read_u2()?,
            name_and_type_index: reader.read_u2()?,
        },
        12 => ConstantPoolEntry::NameAndType {
            index,
            offset,
            name_index: reader.read_u2()?,
            descriptor_index: reader.read_u2()?,
        },
        15 => ConstantPoolEntry::MethodHandle {
            index,
            offset,
            reference_kind: reader.read_u1()?,
            reference_index: reader.read_u2()?,
        },
        16 => ConstantPoolEntry::MethodType {
            index,
            offset,
            descriptor_index: reader.read_u2()?,
        },
        17 => ConstantPoolEntry::Dynamic {
            index,
            offset,
            bootstrap_method_attr_index: reader.read_u2()?,
            name_and_type_index: reader.read_u2()?,
        },
        18 => ConstantPoolEntry::InvokeDynamic {
            index,
            offset,
            bootstrap_method_attr_index: reader.read_u2()?,
            name_and_type_index: reader.read_u2()?,
        },
        19 => ConstantPoolEntry::Module {
            index,
            offset,
            name_index: reader.read_u2()?,
        },
        20 => ConstantPoolEntry::Package {
            index,
            offset,
            name_index: reader.read_u2()?,
        },
        _ => {
            return Err(PyValueError::new_err(format!(
                "Unknown ConstantPoolInfoType tag: {tag}"
            )));
        }
    };
    let index_extra = usize::from(matches!(
        entry,
        ConstantPoolEntry::Long { .. } | ConstantPoolEntry::Double { .. }
    ));
    Ok((entry, index_extra))
}

impl<'a> SliceReader<'a> {
    fn new(data: &'a [u8], offset: usize) -> Self {
        Self { data, offset }
    }

    fn read_u1(&mut self) -> PyResult<u8> {
        if self.offset >= self.data.len() {
            return Err(PyValueError::new_err("read_u1: unexpected end of data"));
        }
        let value = self.data[self.offset];
        self.offset += 1;
        Ok(value)
    }

    fn read_u2(&mut self) -> PyResult<u16> {
        if self.offset + 2 > self.data.len() {
            return Err(PyValueError::new_err("read_u2: unexpected end of data"));
        }
        let value = u16::from_be_bytes([self.data[self.offset], self.data[self.offset + 1]]);
        self.offset += 2;
        Ok(value)
    }

    fn read_u4(&mut self) -> PyResult<u32> {
        if self.offset + 4 > self.data.len() {
            return Err(PyValueError::new_err("read_u4: unexpected end of data"));
        }
        let value = u32::from_be_bytes([
            self.data[self.offset],
            self.data[self.offset + 1],
            self.data[self.offset + 2],
            self.data[self.offset + 3],
        ]);
        self.offset += 4;
        Ok(value)
    }

    fn read_bytes(&mut self, size: usize) -> PyResult<&'a [u8]> {
        if self.offset + size > self.data.len() {
            return Err(PyValueError::new_err("read_bytes: unexpected end of data"));
        }
        let bytes = &self.data[self.offset..self.offset + size];
        self.offset += size;
        Ok(bytes)
    }
}

#[pyfunction]
pub fn read_constant_pool<'py>(
    py: Python<'py>,
    data: &[u8],
    cp_count: usize,
    offset: Option<usize>,
) -> PyResult<(Bound<'py, PyList>, usize)> {
    let mut reader = SliceReader::new(data, offset.unwrap_or(0));
    let entries = PyList::empty(py);
    entries.append(py.None())?;

    let mut index = 1usize;
    while index < cp_count {
        let (entry, index_extra) = read_constant_pool_entry(&mut reader, index)?;
        entries.append(entry.to_python(py)?)?;
        if index_extra == 1 && index + 1 < cp_count {
            entries.append(py.None())?;
        }
        index += 1 + index_extra;
    }

    Ok((entries, reader.offset))
}

/// Register constant-pool functions on the parent module.
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent.py(), "constant_pool")?;
    m.add_function(wrap_pyfunction!(read_constant_pool, &m)?)?;
    crate::register_submodule(parent, &m, "pytecode._rust.classfile.constant_pool")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ConstantPoolEntry, SliceReader, read_constant_pool_entry};
    use pyo3::Python;

    #[test]
    fn slice_reader_reads_big_endian_values() {
        let data = [0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x34];
        let mut reader = SliceReader::new(&data, 0);
        assert_eq!(reader.read_u2().unwrap(), 0xCAFE);
        assert_eq!(reader.read_u4().unwrap(), 0xBABE0034);
    }

    #[test]
    fn slice_reader_errors_on_truncated_u4() {
        Python::initialize();
        let data = [0x00, 0x01, 0x02];
        let mut reader = SliceReader::new(&data, 0);
        let err = reader.read_u4().unwrap_err();
        assert_eq!(
            err.to_string(),
            "ValueError: read_u4: unexpected end of data"
        );
    }

    #[test]
    fn reads_utf8_entry_into_typed_model() {
        let data = [1, 0, 3, b'f', b'o', b'o'];
        let mut reader = SliceReader::new(&data, 0);
        let (entry, extra) = read_constant_pool_entry(&mut reader, 1).unwrap();
        assert_eq!(extra, 0);
        match entry {
            ConstantPoolEntry::Utf8 {
                index,
                length,
                str_bytes,
                ..
            } => {
                assert_eq!(index, 1);
                assert_eq!(length, 3);
                assert_eq!(str_bytes, b"foo");
            }
            _ => panic!("expected utf8 constant pool entry"),
        }
    }
}
