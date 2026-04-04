"""Declaration file for _bytes_utils_cy extension types."""

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
