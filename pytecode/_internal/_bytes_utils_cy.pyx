# cython: boundscheck=False, wraparound=False, cdivision=True
"""Cython-accelerated big-endian binary I/O primitives.

Drop-in replacement for ``_bytes_utils_py`` with C-typed memoryview
access and inline struct unpacking for minimal per-call overhead.
"""

import struct as _struct

from libc.string cimport memcpy

# ---------------------------------------------------------------------------
# Read helpers — operate on a raw buffer pointer via memoryview
# ---------------------------------------------------------------------------

cdef inline int _cu1(const unsigned char[:] buf, int off):
    return buf[off]

cdef inline int _ci1(const unsigned char[:] buf, int off):
    cdef unsigned char v = buf[off]
    if v >= 128:
        return <int>v - 256
    return <int>v

cdef inline int _cu2(const unsigned char[:] buf, int off):
    return (buf[off] << 8) | buf[off + 1]

cdef inline int _ci2(const unsigned char[:] buf, int off):
    cdef int v = (buf[off] << 8) | buf[off + 1]
    if v >= 32768:
        return v - 65536
    return v

cdef inline unsigned int _cu4(const unsigned char[:] buf, int off):
    return (
        (<unsigned int>buf[off] << 24)
        | (<unsigned int>buf[off + 1] << 16)
        | (<unsigned int>buf[off + 2] << 8)
        | <unsigned int>buf[off + 3]
    )

cdef inline int _ci4(const unsigned char[:] buf, int off):
    cdef unsigned int u = _cu4(buf, off)
    return <int>u


# ---------------------------------------------------------------------------
# Public standalone read functions (match _bytes_utils_py API)
# ---------------------------------------------------------------------------

def _read_u1(buffer, int offset=0) -> int:
    cdef const unsigned char[:] view = buffer
    return _cu1(view, offset)

def _read_i1(buffer, int offset=0) -> int:
    cdef const unsigned char[:] view = buffer
    return _ci1(view, offset)

def _read_u2(buffer, int offset=0) -> int:
    cdef const unsigned char[:] view = buffer
    return _cu2(view, offset)

def _read_i2(buffer, int offset=0) -> int:
    cdef const unsigned char[:] view = buffer
    return _ci2(view, offset)

def _read_u4(buffer, int offset=0) -> int:
    cdef const unsigned char[:] view = buffer
    return _cu4(view, offset)

def _read_i4(buffer, int offset=0) -> int:
    cdef const unsigned char[:] view = buffer
    return _ci4(view, offset)

def _read_bytes(buffer, int length, int offset=0) -> bytes:
    cdef const unsigned char[:] view = buffer
    return bytes(view[offset:offset + length])


# ---------------------------------------------------------------------------
# Public standalone write functions (match _bytes_utils_py API)
# ---------------------------------------------------------------------------

def _write_u1(int value) -> bytes:
    if not (0 <= value <= 255):
        raise _struct.error("ubyte format requires 0 <= number <= 255")
    return bytes([value])

def _write_i1(int value) -> bytes:
    if not (-128 <= value <= 127):
        raise _struct.error("byte format requires -128 <= number <= 127")
    return bytes([value & 0xFF])

def _write_u2(int value) -> bytes:
    return bytes([(value >> 8) & 0xFF, value & 0xFF])

def _write_i2(int value) -> bytes:
    return bytes([(value >> 8) & 0xFF, value & 0xFF])

def _write_u4(unsigned int value) -> bytes:
    return bytes([
        (value >> 24) & 0xFF,
        (value >> 16) & 0xFF,
        (value >> 8) & 0xFF,
        value & 0xFF,
    ])

def _write_i4(int value) -> bytes:
    return bytes([
        (value >> 24) & 0xFF,
        (value >> 16) & 0xFF,
        (value >> 8) & 0xFF,
        value & 0xFF,
    ])

def _write_bytes(data) -> bytes:
    return bytes(data)


# ---------------------------------------------------------------------------
# BytesReader — stateful cursor over an immutable buffer
# ---------------------------------------------------------------------------

cdef class BytesReader:

    def __init__(self, bytes_or_bytearray, int offset=0):
        self._buffer_obj = bytes_or_bytearray
        self.buffer_view = bytes_or_bytearray
        self.offset = offset

    @property
    def buffer(self):
        return self._buffer_obj

    def rewind(self, distance=None):
        if distance is None:
            self.offset = 0
        else:
            self.offset = max(self.offset - <int>distance, 0)

    cpdef int read_u1(self):
        if self.offset + 1 > len(self.buffer_view):
            raise _struct.error("unpack requires a buffer of at least 1 byte")
        cdef int res = _cu1(self.buffer_view, self.offset)
        self.offset += 1
        return res

    cpdef int read_i1(self):
        if self.offset + 1 > len(self.buffer_view):
            raise _struct.error("unpack requires a buffer of at least 1 byte")
        cdef int res = _ci1(self.buffer_view, self.offset)
        self.offset += 1
        return res

    cpdef int read_u2(self):
        if self.offset + 2 > len(self.buffer_view):
            raise _struct.error("unpack requires a buffer of at least 2 bytes")
        cdef int res = _cu2(self.buffer_view, self.offset)
        self.offset += 2
        return res

    cpdef int read_i2(self):
        if self.offset + 2 > len(self.buffer_view):
            raise _struct.error("unpack requires a buffer of at least 2 bytes")
        cdef int res = _ci2(self.buffer_view, self.offset)
        self.offset += 2
        return res

    cpdef unsigned int read_u4(self):
        if self.offset + 4 > len(self.buffer_view):
            raise _struct.error("unpack requires a buffer of at least 4 bytes")
        cdef unsigned int res = _cu4(self.buffer_view, self.offset)
        self.offset += 4
        return res

    cpdef int read_i4(self):
        if self.offset + 4 > len(self.buffer_view):
            raise _struct.error("unpack requires a buffer of at least 4 bytes")
        cdef int res = _ci4(self.buffer_view, self.offset)
        self.offset += 4
        return res

    cpdef bytes read_bytes(self, int size):
        cdef bytes res = bytes(self.buffer_view[self.offset:self.offset + size])
        self.offset += size
        return res


# ---------------------------------------------------------------------------
# BytesWriter — append-only buffer with reserve/patch support
# ---------------------------------------------------------------------------

cdef class BytesWriter:
    cdef bytearray _buf

    def __init__(self):
        self._buf = bytearray()

    @property
    def position(self) -> int:
        return len(self._buf)

    def __len__(self) -> int:
        return len(self._buf)

    def to_bytes(self) -> bytes:
        return bytes(self._buf)

    def write_u1(self, int value):
        if not (0 <= value <= 255):
            raise OverflowError("ubyte format requires 0 <= number <= 255")
        self._buf.append(value)

    def write_i1(self, int value):
        if not (-128 <= value <= 127):
            raise OverflowError("byte format requires -128 <= number <= 127")
        self._buf.append(value & 0xFF)

    def write_u2(self, int value):
        self._buf.append((value >> 8) & 0xFF)
        self._buf.append(value & 0xFF)

    def write_i2(self, int value):
        self._buf.append((value >> 8) & 0xFF)
        self._buf.append(value & 0xFF)

    def write_u4(self, unsigned int value):
        self._buf.append((value >> 24) & 0xFF)
        self._buf.append((value >> 16) & 0xFF)
        self._buf.append((value >> 8) & 0xFF)
        self._buf.append(value & 0xFF)

    def write_i4(self, int value):
        self._buf.append((value >> 24) & 0xFF)
        self._buf.append((value >> 16) & 0xFF)
        self._buf.append((value >> 8) & 0xFF)
        self._buf.append(value & 0xFF)

    def write_bytes(self, data):
        self._buf.extend(data)

    def align(self, int alignment):
        cdef int remainder = len(self._buf) % alignment
        if remainder != 0:
            self._buf.extend(b'\x00' * (alignment - remainder))

    def reserve_u1(self) -> int:
        cdef int pos = len(self._buf)
        self._buf.append(0)
        return pos

    def reserve_i1(self) -> int:
        cdef int pos = len(self._buf)
        self._buf.append(0)
        return pos

    def reserve_u2(self) -> int:
        cdef int pos = len(self._buf)
        self._buf.extend(b'\x00\x00')
        return pos

    def reserve_i2(self) -> int:
        cdef int pos = len(self._buf)
        self._buf.extend(b'\x00\x00')
        return pos

    def reserve_u4(self) -> int:
        cdef int pos = len(self._buf)
        self._buf.extend(b'\x00\x00\x00\x00')
        return pos

    def reserve_i4(self) -> int:
        cdef int pos = len(self._buf)
        self._buf.extend(b'\x00\x00\x00\x00')
        return pos

    def patch_u1(self, int position, int value):
        self._buf[position] = value & 0xFF

    def patch_i1(self, int position, int value):
        self._buf[position] = value & 0xFF

    def patch_u2(self, int position, int value):
        self._buf[position] = (value >> 8) & 0xFF
        self._buf[position + 1] = value & 0xFF

    def patch_i2(self, int position, int value):
        self._buf[position] = (value >> 8) & 0xFF
        self._buf[position + 1] = value & 0xFF

    def patch_u4(self, int position, int value):
        self._buf[position] = (value >> 24) & 0xFF
        self._buf[position + 1] = (value >> 16) & 0xFF
        self._buf[position + 2] = (value >> 8) & 0xFF
        self._buf[position + 3] = value & 0xFF

    def patch_i4(self, int position, int value):
        self._buf[position] = (value >> 24) & 0xFF
        self._buf[position + 1] = (value >> 16) & 0xFF
        self._buf[position + 2] = (value >> 8) & 0xFF
        self._buf[position + 3] = value & 0xFF
