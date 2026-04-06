from ..classfile._instructions_cy cimport InsnInfo as CInsnInfo


cdef class _FrozenValueBase:
    pass


cdef class _OperandInsnBase(CInsnInfo):
    pass


cdef class LdcInt(_FrozenValueBase):
    cdef public object value


cdef class LdcFloat(_FrozenValueBase):
    cdef public object raw_bits


cdef class LdcLong(_FrozenValueBase):
    cdef public object value


cdef class LdcDouble(_FrozenValueBase):
    cdef public object high_bytes
    cdef public object low_bytes


cdef class LdcString(_FrozenValueBase):
    cdef public object value


cdef class LdcClass(_FrozenValueBase):
    cdef public object name


cdef class LdcMethodType(_FrozenValueBase):
    cdef public object descriptor


cdef class LdcMethodHandle(_FrozenValueBase):
    cdef public Py_ssize_t reference_kind
    cdef public object owner
    cdef public object name
    cdef public object descriptor
    cdef public bint is_interface


cdef class LdcDynamic(_FrozenValueBase):
    cdef public Py_ssize_t bootstrap_method_attr_index
    cdef public object name
    cdef public object descriptor


cdef class FieldInsn(_OperandInsnBase):
    cdef public object owner
    cdef public object name
    cdef public object descriptor


cdef class MethodInsn(_OperandInsnBase):
    cdef public object owner
    cdef public object name
    cdef public object descriptor
    cdef public bint is_interface


cdef class InterfaceMethodInsn(_OperandInsnBase):
    cdef public object owner
    cdef public object name
    cdef public object descriptor


cdef class TypeInsn(_OperandInsnBase):
    cdef public object class_name


cdef class VarInsn(_OperandInsnBase):
    cdef public Py_ssize_t slot


cdef class IIncInsn(_OperandInsnBase):
    cdef public Py_ssize_t slot
    cdef public Py_ssize_t increment


cdef class LdcInsn(_OperandInsnBase):
    cdef public object value


cdef class InvokeDynamicInsn(_OperandInsnBase):
    cdef public Py_ssize_t bootstrap_method_attr_index
    cdef public object name
    cdef public object descriptor


cdef class MultiANewArrayInsn(_OperandInsnBase):
    cdef public object class_name
    cdef public Py_ssize_t dimensions


cdef FieldInsn _trusted_field_insn(
    object insn_type,
    object owner,
    object name,
    object descriptor,
    Py_ssize_t bytecode_offset=*,
)
cdef MethodInsn _trusted_method_insn(
    object insn_type,
    object owner,
    object name,
    object descriptor,
    bint is_interface=*,
    Py_ssize_t bytecode_offset=*,
)
cdef InterfaceMethodInsn _trusted_interface_method_insn(
    object owner,
    object name,
    object descriptor,
    Py_ssize_t bytecode_offset=*,
)
cdef TypeInsn _trusted_type_insn(
    object insn_type,
    object class_name,
    Py_ssize_t bytecode_offset=*,
)
cdef VarInsn _trusted_var_insn(
    object insn_type,
    Py_ssize_t slot,
    Py_ssize_t bytecode_offset=*,
)
cdef IIncInsn _trusted_iinc_insn(
    Py_ssize_t slot,
    Py_ssize_t increment,
    Py_ssize_t bytecode_offset=*,
)
cdef LdcInsn _trusted_ldc_insn(
    object value,
    Py_ssize_t bytecode_offset=*,
)
cdef InvokeDynamicInsn _trusted_invoke_dynamic_insn(
    Py_ssize_t bootstrap_method_attr_index,
    object name,
    object descriptor,
    Py_ssize_t bytecode_offset=*,
)
cdef MultiANewArrayInsn _trusted_multi_anew_array_insn(
    object class_name,
    Py_ssize_t dimensions,
    Py_ssize_t bytecode_offset=*,
)
