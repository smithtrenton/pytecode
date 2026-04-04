"""Declaration file for _bytes_utils_cy extension types."""

# Inline read helpers — available to any module that cimports this .pxd.
# Defined here (not in .pyx) so Cython can inline them at the call site.

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


cdef class BytesReader:
    cdef const unsigned char[:] buffer_view
    cdef public int offset
    cdef object _buffer_obj

    cpdef int read_u1(self)
    cpdef int read_i1(self)
    cpdef int read_u2(self)
    cpdef int read_i2(self)
    cpdef unsigned int read_u4(self)
    cpdef int read_i4(self)
    cpdef bytes read_bytes(self, int size)
