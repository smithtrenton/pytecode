from ..classfile._instructions_cy cimport InsnInfo as CInsnInfo


cdef class _ValueBase:
    pass


cdef class _SymbolicInsnBase(CInsnInfo):
    pass


cdef class Label:
    cdef public object name


cdef class ExceptionHandler(_ValueBase):
    cdef public object start
    cdef public object end
    cdef public object handler
    cdef public object catch_type


cdef class LineNumberEntry(_ValueBase):
    cdef public object label
    cdef public Py_ssize_t line_number


cdef class LocalVariableEntry(_ValueBase):
    cdef public object start
    cdef public object end
    cdef public object name
    cdef public object descriptor
    cdef public Py_ssize_t slot


cdef class LocalVariableTypeEntry(_ValueBase):
    cdef public object start
    cdef public object end
    cdef public object name
    cdef public object signature
    cdef public Py_ssize_t slot


cdef class BranchInsn(_SymbolicInsnBase):
    cdef public object target


cdef class LookupSwitchInsn(_SymbolicInsnBase):
    cdef public object default_target
    cdef public list pairs


cdef class TableSwitchInsn(_SymbolicInsnBase):
    cdef public object default_target
    cdef public Py_ssize_t low
    cdef public Py_ssize_t high
    cdef public list targets


cdef class LabelResolution(_ValueBase):
    cdef public dict label_offsets
    cdef public list instruction_offsets
    cdef public Py_ssize_t total_code_length
