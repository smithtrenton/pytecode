cdef class InsnInfo:
    cdef public object type
    cdef public Py_ssize_t bytecode_offset


cdef class LocalIndex(InsnInfo):
    cdef public Py_ssize_t index


cdef class LocalIndexW(InsnInfo):
    cdef public Py_ssize_t index


cdef class ConstPoolIndex(InsnInfo):
    cdef public Py_ssize_t index


cdef class ByteValue(InsnInfo):
    cdef public Py_ssize_t value


cdef class ShortValue(InsnInfo):
    cdef public Py_ssize_t value


cdef class Branch(InsnInfo):
    cdef public Py_ssize_t offset


cdef class BranchW(InsnInfo):
    cdef public Py_ssize_t offset


cdef class IInc(InsnInfo):
    cdef public Py_ssize_t index
    cdef public Py_ssize_t value


cdef class IIncW(InsnInfo):
    cdef public Py_ssize_t index
    cdef public Py_ssize_t value


cdef class InvokeDynamic(InsnInfo):
    cdef public Py_ssize_t index
    cdef public object unused


cdef class InvokeInterface(InsnInfo):
    cdef public Py_ssize_t index
    cdef public Py_ssize_t count
    cdef public object unused


cdef class NewArray(InsnInfo):
    cdef public object atype


cdef class MultiANewArray(InsnInfo):
    cdef public Py_ssize_t index
    cdef public Py_ssize_t dimensions


cdef class MatchOffsetPair:
    cdef public Py_ssize_t match
    cdef public Py_ssize_t offset


cdef class LookupSwitch(InsnInfo):
    cdef public Py_ssize_t default
    cdef public Py_ssize_t npairs
    cdef public list pairs


cdef class TableSwitch(InsnInfo):
    cdef public Py_ssize_t default
    cdef public Py_ssize_t low
    cdef public Py_ssize_t high
    cdef public list offsets
