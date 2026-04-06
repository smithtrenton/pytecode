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


cdef InsnInfo _trusted_raw_insn(object type, Py_ssize_t bytecode_offset)
cdef LocalIndex _trusted_local_index(object type, Py_ssize_t bytecode_offset, Py_ssize_t index)
cdef LocalIndexW _trusted_local_index_w(object type, Py_ssize_t bytecode_offset, Py_ssize_t index)
cdef ConstPoolIndex _trusted_const_pool_index(object type, Py_ssize_t bytecode_offset, Py_ssize_t index)
cdef ByteValue _trusted_byte_value(object type, Py_ssize_t bytecode_offset, Py_ssize_t value)
cdef ShortValue _trusted_short_value(object type, Py_ssize_t bytecode_offset, Py_ssize_t value)
cdef Branch _trusted_branch(object type, Py_ssize_t bytecode_offset, Py_ssize_t offset)
cdef BranchW _trusted_branch_w(object type, Py_ssize_t bytecode_offset, Py_ssize_t offset)
cdef IInc _trusted_iinc(object type, Py_ssize_t bytecode_offset, Py_ssize_t index, Py_ssize_t value)
cdef IIncW _trusted_iinc_w(object type, Py_ssize_t bytecode_offset, Py_ssize_t index, Py_ssize_t value)
cdef InvokeDynamic _trusted_invoke_dynamic(
    object type,
    Py_ssize_t bytecode_offset,
    Py_ssize_t index,
    object unused,
)
cdef InvokeInterface _trusted_invoke_interface(
    object type,
    Py_ssize_t bytecode_offset,
    Py_ssize_t index,
    Py_ssize_t count,
    object unused,
)
cdef NewArray _trusted_new_array(object type, Py_ssize_t bytecode_offset, object atype)
cdef MultiANewArray _trusted_multi_anew_array(
    object type,
    Py_ssize_t bytecode_offset,
    Py_ssize_t index,
    Py_ssize_t dimensions,
)
cdef MatchOffsetPair _trusted_match_offset_pair(Py_ssize_t match, Py_ssize_t offset)
cdef LookupSwitch _trusted_lookup_switch(
    object type,
    Py_ssize_t bytecode_offset,
    Py_ssize_t default,
    Py_ssize_t npairs,
    list pairs,
)
cdef TableSwitch _trusted_table_switch(
    object type,
    Py_ssize_t bytecode_offset,
    Py_ssize_t default,
    Py_ssize_t low,
    Py_ssize_t high,
    list offsets,
)
