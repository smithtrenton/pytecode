use pyo3::prelude::*;
use pyo3::exceptions::PyValueError;

/// A stateful sequential reader over a byte buffer (big-endian, JVM byte order).
///
/// Mirrors the Python `BytesReader` in `pytecode._internal.bytes_utils`.
#[pyclass]
pub struct RustBytesReader {
    data: Vec<u8>,
    offset: usize,
}

#[pymethods]
impl RustBytesReader {
    #[new]
    fn new(data: Vec<u8>, offset: Option<usize>) -> Self {
        Self {
            data,
            offset: offset.unwrap_or(0),
        }
    }

    #[getter]
    fn offset(&self) -> usize {
        self.offset
    }

    fn read_u1(&mut self) -> PyResult<u8> {
        if self.offset >= self.data.len() {
            return Err(PyValueError::new_err("read_u1: unexpected end of data"));
        }
        let v = self.data[self.offset];
        self.offset += 1;
        Ok(v)
    }

    fn read_i1(&mut self) -> PyResult<i8> {
        self.read_u1().map(|v| v as i8)
    }

    fn read_u2(&mut self) -> PyResult<u16> {
        if self.offset + 2 > self.data.len() {
            return Err(PyValueError::new_err("read_u2: unexpected end of data"));
        }
        let v = u16::from_be_bytes([self.data[self.offset], self.data[self.offset + 1]]);
        self.offset += 2;
        Ok(v)
    }

    fn read_i2(&mut self) -> PyResult<i16> {
        self.read_u2().map(|v| v as i16)
    }

    fn read_u4(&mut self) -> PyResult<u32> {
        if self.offset + 4 > self.data.len() {
            return Err(PyValueError::new_err("read_u4: unexpected end of data"));
        }
        let v = u32::from_be_bytes([
            self.data[self.offset],
            self.data[self.offset + 1],
            self.data[self.offset + 2],
            self.data[self.offset + 3],
        ]);
        self.offset += 4;
        Ok(v)
    }

    fn read_i4(&mut self) -> PyResult<i32> {
        self.read_u4().map(|v| v as i32)
    }

    fn read_bytes(&mut self, size: usize) -> PyResult<Vec<u8>> {
        if self.offset + size > self.data.len() {
            return Err(PyValueError::new_err("read_bytes: unexpected end of data"));
        }
        let v = self.data[self.offset..self.offset + size].to_vec();
        self.offset += size;
        Ok(v)
    }

    fn rewind(&mut self, distance: Option<usize>) {
        match distance {
            Some(d) => self.offset = self.offset.saturating_sub(d),
            None => self.offset = 0,
        }
    }
}

/// A stateful buffer builder for writing big-endian binary data.
///
/// Mirrors the Python `BytesWriter` in `pytecode._internal.bytes_utils`.
#[pyclass]
pub struct RustBytesWriter {
    buf: Vec<u8>,
}

#[pymethods]
impl RustBytesWriter {
    #[new]
    fn new() -> Self {
        Self { buf: Vec::new() }
    }

    #[getter]
    fn position(&self) -> usize {
        self.buf.len()
    }

    fn __len__(&self) -> usize {
        self.buf.len()
    }

    fn to_bytes(&self) -> Vec<u8> {
        self.buf.clone()
    }

    fn write_u1(&mut self, value: u8) {
        self.buf.push(value);
    }

    fn write_i1(&mut self, value: i8) {
        self.buf.push(value as u8);
    }

    fn write_u2(&mut self, value: u16) {
        self.buf.extend_from_slice(&value.to_be_bytes());
    }

    fn write_i2(&mut self, value: i16) {
        self.buf.extend_from_slice(&value.to_be_bytes());
    }

    fn write_u4(&mut self, value: u32) {
        self.buf.extend_from_slice(&value.to_be_bytes());
    }

    fn write_i4(&mut self, value: i32) {
        self.buf.extend_from_slice(&value.to_be_bytes());
    }

    fn write_bytes(&mut self, data: Vec<u8>) {
        self.buf.extend_from_slice(&data);
    }

    fn align(&mut self, alignment: usize) {
        let remainder = self.buf.len() % alignment;
        if remainder != 0 {
            let padding = alignment - remainder;
            self.buf.extend(std::iter::repeat_n(0u8, padding));
        }
    }

    fn reserve_u1(&mut self) -> usize {
        let pos = self.buf.len();
        self.buf.push(0);
        pos
    }

    fn reserve_u2(&mut self) -> usize {
        let pos = self.buf.len();
        self.buf.extend_from_slice(&[0, 0]);
        pos
    }

    fn reserve_u4(&mut self) -> usize {
        let pos = self.buf.len();
        self.buf.extend_from_slice(&[0, 0, 0, 0]);
        pos
    }

    fn patch_u1(&mut self, position: usize, value: u8) -> PyResult<()> {
        if position >= self.buf.len() {
            return Err(PyValueError::new_err("patch_u1: position out of range"));
        }
        self.buf[position] = value;
        Ok(())
    }

    fn patch_u2(&mut self, position: usize, value: u16) -> PyResult<()> {
        if position + 2 > self.buf.len() {
            return Err(PyValueError::new_err("patch_u2: position out of range"));
        }
        let bytes = value.to_be_bytes();
        self.buf[position] = bytes[0];
        self.buf[position + 1] = bytes[1];
        Ok(())
    }

    fn patch_u4(&mut self, position: usize, value: u32) -> PyResult<()> {
        if position + 4 > self.buf.len() {
            return Err(PyValueError::new_err("patch_u4: position out of range"));
        }
        let bytes = value.to_be_bytes();
        self.buf[position] = bytes[0];
        self.buf[position + 1] = bytes[1];
        self.buf[position + 2] = bytes[2];
        self.buf[position + 3] = bytes[3];
        Ok(())
    }
}

/// Register binary I/O types on the parent module.
pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent.py(), "binary_io")?;
    m.add_class::<RustBytesReader>()?;
    m.add_class::<RustBytesWriter>()?;
    crate::register_submodule(parent, &m, "pytecode._rust.binary_io")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reader_u1_u2_u4() {
        let data = vec![0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x01];
        let mut r = RustBytesReader::new(data, None);
        assert_eq!(r.read_u1().unwrap(), 0xCA);
        assert_eq!(r.read_u1().unwrap(), 0xFE);
        assert_eq!(r.read_u2().unwrap(), 0xBABE);
        assert_eq!(r.read_u2().unwrap(), 0x0001);
    }

    #[test]
    fn writer_roundtrip() {
        let mut w = RustBytesWriter::new();
        w.write_u4(0xCAFEBABE);
        w.write_u2(0x0034);
        w.write_u1(0xFF);
        let bytes = w.to_bytes();
        assert_eq!(bytes, vec![0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x34, 0xFF]);
    }

    #[test]
    fn writer_reserve_and_patch() {
        let mut w = RustBytesWriter::new();
        let pos = w.reserve_u2();
        w.write_u1(0xAB);
        w.patch_u2(pos, 0x1234).unwrap();
        assert_eq!(w.to_bytes(), vec![0x12, 0x34, 0xAB]);
    }

    #[test]
    fn writer_align() {
        let mut w = RustBytesWriter::new();
        w.write_u1(0x01);
        w.align(4);
        assert_eq!(w.position(), 4);
        assert_eq!(w.to_bytes(), vec![0x01, 0x00, 0x00, 0x00]);
    }
}
