use crate::error::{EngineError, EngineErrorKind, Result};

#[derive(Debug, Clone)]
pub(crate) struct ByteReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> ByteReader<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    pub(crate) fn offset(&self) -> usize {
        self.offset
    }

    pub(crate) fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.offset)
    }

    pub(crate) fn read_u1(&mut self) -> Result<u8> {
        Ok(self.read_bytes(1)?[0])
    }

    pub(crate) fn read_i1(&mut self) -> Result<i8> {
        Ok(self.read_u1()? as i8)
    }

    pub(crate) fn read_u2(&mut self) -> Result<u16> {
        let bytes = self.read_bytes(2)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    pub(crate) fn read_i2(&mut self) -> Result<i16> {
        Ok(self.read_u2()? as i16)
    }

    pub(crate) fn read_u4(&mut self) -> Result<u32> {
        let bytes = self.read_bytes(4)?;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub(crate) fn read_i4(&mut self) -> Result<i32> {
        Ok(self.read_u4()? as i32)
    }

    pub(crate) fn read_bytes(&mut self, len: usize) -> Result<&'a [u8]> {
        if self.remaining() < len {
            return Err(EngineError::new(
                self.offset,
                EngineErrorKind::UnexpectedEof {
                    needed: len,
                    remaining: self.remaining(),
                },
            ));
        }
        let start = self.offset;
        self.offset += len;
        Ok(&self.bytes[start..self.offset])
    }
}

#[derive(Debug, Default, Clone)]
pub(crate) struct ByteWriter {
    bytes: Vec<u8>,
}

impl ByteWriter {
    pub(crate) fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(capacity),
        }
    }

    pub(crate) fn write_u1(&mut self, value: u8) {
        self.bytes.push(value);
    }

    pub(crate) fn write_i1(&mut self, value: i8) {
        self.write_u1(value as u8);
    }

    pub(crate) fn write_u2(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    pub(crate) fn write_i2(&mut self, value: i16) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    pub(crate) fn write_u4(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    pub(crate) fn write_i4(&mut self, value: i32) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    pub(crate) fn write_bytes(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    pub(crate) fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}
