# cython: boundscheck=False, wraparound=False, cdivision=True
"""Parse JVM ``.class`` file bytes into a :class:`ClassFile` tree.

This module implements a single-pass reader that deserialises the binary
class-file format defined in *The Java Virtual Machine Specification* (JVMS §4)
into the in-memory ``ClassFile`` structure exposed by :mod:`pytecode.classfile.info`.
"""

import os

from pytecode._internal._bytes_utils_cy import BytesReader
from pytecode._internal._bytes_utils_cy cimport BytesReader, _cu1, _ci1, _cu2, _ci2, _cu4, _ci4
from ._attributes_cy cimport (
    AppendFrameInfo,
    AttributeInfo,
    BootstrapMethodInfo,
    ChopFrameInfo,
    CodeAttr,
    DoubleVariableInfo,
    ExceptionInfo,
    FloatVariableInfo,
    FullFrameInfo,
    IntegerVariableInfo,
    InnerClassInfo,
    LineNumberInfo,
    LineNumberTableAttr,
    LocalVariableInfo,
    LocalVariableTableAttr,
    LocalVariableTypeInfo,
    LocalVariableTypeTableAttr,
    LongVariableInfo,
    MethodParameterInfo,
    NullVariableInfo,
    ObjectVariableInfo,
    RecordComponentInfo,
    SameFrameExtendedInfo,
    SameFrameInfo,
    SameLocals1StackItemFrameExtendedInfo,
    SameLocals1StackItemFrameInfo,
    StackMapFrameInfo,
    StackMapTableAttr,
    TopVariableInfo,
    VerificationTypeInfo,
    UninitializedThisVariableInfo,
    UninitializedVariableInfo,
)
from ._instructions_cy cimport (
    Branch,
    BranchW,
    ByteValue,
    ConstPoolIndex,
    IInc,
    IIncW,
    InsnInfo,
    InvokeDynamic,
    InvokeInterface,
    LocalIndex,
    LocalIndexW,
    LookupSwitch,
    MatchOffsetPair,
    MultiANewArray,
    NewArray,
    ShortValue,
    TableSwitch,
)
from . import attributes, constant_pool, constants, info, instructions
from .modified_utf8 import decode_modified_utf8

__all__ = ["ClassReader", "MalformedClassException"]


_constant_pool_info_types = [None] * 256
for _cp_type in constant_pool.ConstantPoolInfoType:
    _constant_pool_info_types[int(_cp_type.value)] = _cp_type
_CONSTANT_POOL_INFO_TYPES = tuple(_constant_pool_info_types)

_MAX_INSTRUCTION_CODE = max(int(inst_type) for inst_type in instructions.InsnInfoType)
_instruction_types = [None] * (_MAX_INSTRUCTION_CODE + 1)
_instruction_infos = [None] * (_MAX_INSTRUCTION_CODE + 1)
for _inst_type in instructions.InsnInfoType:
    _opcode = int(_inst_type)
    _instruction_types[_opcode] = _inst_type
    _instruction_infos[_opcode] = _inst_type.instinfo
_INSTRUCTION_TYPES = tuple(_instruction_types)
_INSTRUCTION_INFOS = tuple(_instruction_infos)

_array_types = [None] * 256
for _array_type_value in instructions.ArrayType:
    _array_types[int(_array_type_value)] = _array_type_value
_ARRAY_TYPES = tuple(_array_types)
_ATTRIBUTE_INFO_TYPES = {
    member.value: member for member in attributes.AttributeInfoType if member.value
}
_ENUM_MEMBER_CACHE = {}


cdef inline void _init_insn_base(InsnInfo insn, object insn_type, Py_ssize_t bytecode_offset):
    insn.type = insn_type
    insn.bytecode_offset = bytecode_offset


cdef inline object _parsed_insn(object insn_type, Py_ssize_t bytecode_offset):
    cdef InsnInfo insn = InsnInfo.__new__(InsnInfo)
    _init_insn_base(insn, insn_type, bytecode_offset)
    return insn


cdef inline object _parsed_local_index(object insn_type, Py_ssize_t bytecode_offset, Py_ssize_t index):
    cdef LocalIndex insn = LocalIndex.__new__(LocalIndex)
    _init_insn_base(insn, insn_type, bytecode_offset)
    insn.index = index
    return insn


cdef inline object _parsed_local_index_w(object insn_type, Py_ssize_t bytecode_offset, Py_ssize_t index):
    cdef LocalIndexW insn = LocalIndexW.__new__(LocalIndexW)
    _init_insn_base(insn, insn_type, bytecode_offset)
    insn.index = index
    return insn


cdef inline object _parsed_const_pool_index(object insn_type, Py_ssize_t bytecode_offset, Py_ssize_t index):
    cdef ConstPoolIndex insn = ConstPoolIndex.__new__(ConstPoolIndex)
    _init_insn_base(insn, insn_type, bytecode_offset)
    insn.index = index
    return insn


cdef inline object _parsed_byte_value(object insn_type, Py_ssize_t bytecode_offset, Py_ssize_t value):
    cdef ByteValue insn = ByteValue.__new__(ByteValue)
    _init_insn_base(insn, insn_type, bytecode_offset)
    insn.value = value
    return insn


cdef inline object _parsed_short_value(object insn_type, Py_ssize_t bytecode_offset, Py_ssize_t value):
    cdef ShortValue insn = ShortValue.__new__(ShortValue)
    _init_insn_base(insn, insn_type, bytecode_offset)
    insn.value = value
    return insn


cdef inline object _parsed_branch(object insn_type, Py_ssize_t bytecode_offset, Py_ssize_t offset):
    cdef Branch insn = Branch.__new__(Branch)
    _init_insn_base(insn, insn_type, bytecode_offset)
    insn.offset = offset
    return insn


cdef inline object _parsed_branch_w(object insn_type, Py_ssize_t bytecode_offset, Py_ssize_t offset):
    cdef BranchW insn = BranchW.__new__(BranchW)
    _init_insn_base(insn, insn_type, bytecode_offset)
    insn.offset = offset
    return insn


cdef inline object _parsed_iinc(object insn_type, Py_ssize_t bytecode_offset, Py_ssize_t index, Py_ssize_t value):
    cdef IInc insn = IInc.__new__(IInc)
    _init_insn_base(insn, insn_type, bytecode_offset)
    insn.index = index
    insn.value = value
    return insn


cdef inline object _parsed_iinc_w(object insn_type, Py_ssize_t bytecode_offset, Py_ssize_t index, Py_ssize_t value):
    cdef IIncW insn = IIncW.__new__(IIncW)
    _init_insn_base(insn, insn_type, bytecode_offset)
    insn.index = index
    insn.value = value
    return insn


cdef inline object _parsed_invoke_dynamic(
    object insn_type,
    Py_ssize_t bytecode_offset,
    Py_ssize_t index,
    object unused,
):
    cdef InvokeDynamic insn = InvokeDynamic.__new__(InvokeDynamic)
    _init_insn_base(insn, insn_type, bytecode_offset)
    insn.index = index
    insn.unused = unused
    return insn


cdef inline object _parsed_invoke_interface(
    object insn_type,
    Py_ssize_t bytecode_offset,
    Py_ssize_t index,
    Py_ssize_t count,
    object unused,
):
    cdef InvokeInterface insn = InvokeInterface.__new__(InvokeInterface)
    _init_insn_base(insn, insn_type, bytecode_offset)
    insn.index = index
    insn.count = count
    insn.unused = unused
    return insn


cdef inline object _parsed_new_array(object insn_type, Py_ssize_t bytecode_offset, object atype):
    cdef NewArray insn = NewArray.__new__(NewArray)
    _init_insn_base(insn, insn_type, bytecode_offset)
    insn.atype = atype
    return insn


cdef inline object _parsed_multi_anew_array(
    object insn_type,
    Py_ssize_t bytecode_offset,
    Py_ssize_t index,
    Py_ssize_t dimensions,
):
    cdef MultiANewArray insn = MultiANewArray.__new__(MultiANewArray)
    _init_insn_base(insn, insn_type, bytecode_offset)
    insn.index = index
    insn.dimensions = dimensions
    return insn


cdef inline object _parsed_match_offset_pair(Py_ssize_t match, Py_ssize_t offset):
    cdef MatchOffsetPair pair = MatchOffsetPair.__new__(MatchOffsetPair)
    pair.match = match
    pair.offset = offset
    return pair


cdef inline object _parsed_lookup_switch(
    object insn_type,
    Py_ssize_t bytecode_offset,
    Py_ssize_t default,
    Py_ssize_t npairs,
    list pairs,
):
    cdef LookupSwitch insn = LookupSwitch.__new__(LookupSwitch)
    _init_insn_base(insn, insn_type, bytecode_offset)
    insn.default = default
    insn.npairs = npairs
    insn.pairs = pairs
    return insn


cdef inline object _parsed_table_switch(
    object insn_type,
    Py_ssize_t bytecode_offset,
    Py_ssize_t default,
    Py_ssize_t low,
    Py_ssize_t high,
    list offsets,
):
    cdef TableSwitch insn = TableSwitch.__new__(TableSwitch)
    _init_insn_base(insn, insn_type, bytecode_offset)
    insn.default = default
    insn.low = low
    insn.high = high
    insn.offsets = offsets
    return insn


cdef inline void _init_attribute_base(AttributeInfo attr, Py_ssize_t attribute_name_index, Py_ssize_t attribute_length):
    attr.attribute_name_index = attribute_name_index
    attr.attribute_length = attribute_length


cdef inline object _parsed_exception_info(
    Py_ssize_t start_pc,
    Py_ssize_t end_pc,
    Py_ssize_t handler_pc,
    Py_ssize_t catch_type,
):
    cdef ExceptionInfo info = ExceptionInfo.__new__(ExceptionInfo)
    info.start_pc = start_pc
    info.end_pc = end_pc
    info.handler_pc = handler_pc
    info.catch_type = catch_type
    return info


cdef inline void _init_verification_type_base(VerificationTypeInfo info, object tag):
    info.tag = tag


cdef inline object _parsed_top_variable_info(object tag):
    cdef TopVariableInfo info = TopVariableInfo.__new__(TopVariableInfo)
    _init_verification_type_base(info, tag)
    return info


cdef inline object _parsed_integer_variable_info(object tag):
    cdef IntegerVariableInfo info = IntegerVariableInfo.__new__(IntegerVariableInfo)
    _init_verification_type_base(info, tag)
    return info


cdef inline object _parsed_float_variable_info(object tag):
    cdef FloatVariableInfo info = FloatVariableInfo.__new__(FloatVariableInfo)
    _init_verification_type_base(info, tag)
    return info


cdef inline object _parsed_double_variable_info(object tag):
    cdef DoubleVariableInfo info = DoubleVariableInfo.__new__(DoubleVariableInfo)
    info.tag = tag
    return info


cdef inline object _parsed_long_variable_info(object tag):
    cdef LongVariableInfo info = LongVariableInfo.__new__(LongVariableInfo)
    _init_verification_type_base(info, tag)
    return info


cdef inline object _parsed_null_variable_info(object tag):
    cdef NullVariableInfo info = NullVariableInfo.__new__(NullVariableInfo)
    _init_verification_type_base(info, tag)
    return info


cdef inline object _parsed_uninitialized_this_variable_info(object tag):
    cdef UninitializedThisVariableInfo info = UninitializedThisVariableInfo.__new__(UninitializedThisVariableInfo)
    _init_verification_type_base(info, tag)
    return info


cdef inline object _parsed_object_variable_info(object tag, Py_ssize_t cpool_index):
    cdef ObjectVariableInfo info = ObjectVariableInfo.__new__(ObjectVariableInfo)
    _init_verification_type_base(info, tag)
    info.cpool_index = cpool_index
    return info


cdef inline object _parsed_uninitialized_variable_info(object tag, Py_ssize_t offset):
    cdef UninitializedVariableInfo info = UninitializedVariableInfo.__new__(UninitializedVariableInfo)
    _init_verification_type_base(info, tag)
    info.offset = offset
    return info


cdef inline void _init_stack_map_frame_base(StackMapFrameInfo frame, Py_ssize_t frame_type):
    frame.frame_type = frame_type


cdef inline object _parsed_same_frame_info(Py_ssize_t frame_type):
    cdef SameFrameInfo info = SameFrameInfo.__new__(SameFrameInfo)
    _init_stack_map_frame_base(info, frame_type)
    return info


cdef inline object _parsed_same_locals_1_stack_item_frame_info(Py_ssize_t frame_type, object stack):
    cdef SameLocals1StackItemFrameInfo info = SameLocals1StackItemFrameInfo.__new__(SameLocals1StackItemFrameInfo)
    _init_stack_map_frame_base(info, frame_type)
    info.stack = stack
    return info


cdef inline object _parsed_same_locals_1_stack_item_frame_extended_info(
    Py_ssize_t frame_type,
    Py_ssize_t offset_delta,
    object stack,
):
    cdef SameLocals1StackItemFrameExtendedInfo info = SameLocals1StackItemFrameExtendedInfo.__new__(
        SameLocals1StackItemFrameExtendedInfo
    )
    _init_stack_map_frame_base(info, frame_type)
    info.offset_delta = offset_delta
    info.stack = stack
    return info


cdef inline object _parsed_chop_frame_info(Py_ssize_t frame_type, Py_ssize_t offset_delta):
    cdef ChopFrameInfo info = ChopFrameInfo.__new__(ChopFrameInfo)
    _init_stack_map_frame_base(info, frame_type)
    info.offset_delta = offset_delta
    return info


cdef inline object _parsed_same_frame_extended_info(Py_ssize_t frame_type, Py_ssize_t offset_delta):
    cdef SameFrameExtendedInfo info = SameFrameExtendedInfo.__new__(SameFrameExtendedInfo)
    _init_stack_map_frame_base(info, frame_type)
    info.offset_delta = offset_delta
    return info


cdef inline object _parsed_append_frame_info(Py_ssize_t frame_type, Py_ssize_t offset_delta, list locals):
    cdef AppendFrameInfo info = AppendFrameInfo.__new__(AppendFrameInfo)
    _init_stack_map_frame_base(info, frame_type)
    info.offset_delta = offset_delta
    info.locals = locals
    return info


cdef inline object _parsed_full_frame_info(
    Py_ssize_t frame_type,
    Py_ssize_t offset_delta,
    Py_ssize_t number_of_locals,
    list locals,
    Py_ssize_t number_of_stack_items,
    list stack,
):
    cdef FullFrameInfo info = FullFrameInfo.__new__(FullFrameInfo)
    _init_stack_map_frame_base(info, frame_type)
    info.offset_delta = offset_delta
    info.number_of_locals = number_of_locals
    info.locals = locals
    info.number_of_stack_items = number_of_stack_items
    info.stack = stack
    return info


cdef inline object _parsed_code_attr(
    Py_ssize_t attribute_name_index,
    Py_ssize_t attribute_length,
    Py_ssize_t max_stacks,
    Py_ssize_t max_locals,
    Py_ssize_t code_length,
    list code,
    Py_ssize_t exception_table_length,
    list exception_table,
    Py_ssize_t attributes_count,
    list attributes_list,
):
    cdef CodeAttr attr = CodeAttr.__new__(CodeAttr)
    _init_attribute_base(attr, attribute_name_index, attribute_length)
    attr.max_stacks = max_stacks
    attr.max_locals = max_locals
    attr.code_length = code_length
    attr.code = code
    attr.exception_table_length = exception_table_length
    attr.exception_table = exception_table
    attr.attributes_count = attributes_count
    attr.attributes = attributes_list
    return attr


cdef inline object _parsed_stack_map_table_attr(
    Py_ssize_t attribute_name_index,
    Py_ssize_t attribute_length,
    Py_ssize_t number_of_entries,
    list entries,
):
    cdef StackMapTableAttr attr = StackMapTableAttr.__new__(StackMapTableAttr)
    _init_attribute_base(attr, attribute_name_index, attribute_length)
    attr.number_of_entries = number_of_entries
    attr.entries = entries
    return attr


cdef inline object _parsed_line_number_info(Py_ssize_t start_pc, Py_ssize_t line_number):
    cdef LineNumberInfo info = LineNumberInfo.__new__(LineNumberInfo)
    info.start_pc = start_pc
    info.line_number = line_number
    return info


cdef inline object _parsed_line_number_table_attr(
    Py_ssize_t attribute_name_index,
    Py_ssize_t attribute_length,
    Py_ssize_t line_number_table_length,
    list line_number_table,
):
    cdef LineNumberTableAttr attr = LineNumberTableAttr.__new__(LineNumberTableAttr)
    _init_attribute_base(attr, attribute_name_index, attribute_length)
    attr.line_number_table_length = line_number_table_length
    attr.line_number_table = line_number_table
    return attr


cdef inline object _parsed_local_variable_info(
    Py_ssize_t start_pc,
    Py_ssize_t length,
    Py_ssize_t name_index,
    Py_ssize_t descriptor_index,
    Py_ssize_t index,
):
    cdef LocalVariableInfo info = LocalVariableInfo.__new__(LocalVariableInfo)
    info.start_pc = start_pc
    info.length = length
    info.name_index = name_index
    info.descriptor_index = descriptor_index
    info.index = index
    return info


cdef inline object _parsed_local_variable_table_attr(
    Py_ssize_t attribute_name_index,
    Py_ssize_t attribute_length,
    Py_ssize_t local_variable_table_length,
    list local_variable_table,
):
    cdef LocalVariableTableAttr attr = LocalVariableTableAttr.__new__(LocalVariableTableAttr)
    _init_attribute_base(attr, attribute_name_index, attribute_length)
    attr.local_variable_table_length = local_variable_table_length
    attr.local_variable_table = local_variable_table
    return attr


cdef inline object _parsed_local_variable_type_info(
    Py_ssize_t start_pc,
    Py_ssize_t length,
    Py_ssize_t name_index,
    Py_ssize_t signature_index,
    Py_ssize_t index,
):
    cdef LocalVariableTypeInfo info = LocalVariableTypeInfo.__new__(LocalVariableTypeInfo)
    info.start_pc = start_pc
    info.length = length
    info.name_index = name_index
    info.signature_index = signature_index
    info.index = index
    return info


cdef inline object _parsed_local_variable_type_table_attr(
    Py_ssize_t attribute_name_index,
    Py_ssize_t attribute_length,
    Py_ssize_t local_variable_type_table_length,
    list local_variable_type_table,
):
    cdef LocalVariableTypeTableAttr attr = LocalVariableTypeTableAttr.__new__(LocalVariableTypeTableAttr)
    _init_attribute_base(attr, attribute_name_index, attribute_length)
    attr.local_variable_type_table_length = local_variable_type_table_length
    attr.local_variable_type_table = local_variable_type_table
    return attr


cdef inline object _constant_pool_info_type(int tag):
    cdef object result = _CONSTANT_POOL_INFO_TYPES[tag]
    if result is not None:
        return result
    return constant_pool.ConstantPoolInfoType(tag)


cdef inline object _array_type(int atype):
    cdef object result = _ARRAY_TYPES[atype]
    if result is not None:
        return result
    return instructions.ArrayType(atype)


cdef inline object _attribute_info_type(str name):
    result = _ATTRIBUTE_INFO_TYPES.get(name)
    if result is not None:
        return result
    return attributes.AttributeInfoType(name)


cdef inline object _enum_member(object enum_type, int value):
    cdef tuple key = (enum_type, value)
    result = _ENUM_MEMBER_CACHE.get(key)
    if result is not None:
        return result
    result = enum_type(value)
    _ENUM_MEMBER_CACHE[key] = result
    return result


class MalformedClassException(Exception):
    """Raised when the input bytes do not conform to the JVM class-file format (JVMS §4)."""


# Fast byte-read helpers: bypass cpdef dispatch and bounds checks.
# These call the .pxd inline functions directly on the BytesReader's buffer.
cdef inline int _fast_u1(BytesReader r):
    cdef int v = _cu1(r.buffer_view, r.offset)
    r.offset += 1
    return v

cdef inline int _fast_i1(BytesReader r):
    cdef int v = _ci1(r.buffer_view, r.offset)
    r.offset += 1
    return v

cdef inline int _fast_u2(BytesReader r):
    cdef int v = _cu2(r.buffer_view, r.offset)
    r.offset += 2
    return v

cdef inline int _fast_i2(BytesReader r):
    cdef int v = _ci2(r.buffer_view, r.offset)
    r.offset += 2
    return v

cdef inline unsigned int _fast_u4(BytesReader r):
    cdef unsigned int v = _cu4(r.buffer_view, r.offset)
    r.offset += 4
    return v

cdef inline int _fast_i4(BytesReader r):
    cdef int v = _ci4(r.buffer_view, r.offset)
    r.offset += 4
    return v


cdef class ClassReader(BytesReader):
    """Single-pass parser that converts ``.class`` file bytes into a :class:`~pytecode.classfile.info.ClassFile` tree.

    The reader walks the binary layout defined in JVMS §4.1, populating the
    constant pool first (§4.4) and then deserialising fields, methods, and
    attributes in declaration order.  The resulting :attr:`class_info` object
    mirrors the on-disk ``ClassFile`` structure.
    """

    cdef public list constant_pool
    cdef public object class_info

    def __init__(self, bytes_or_bytearray):
        """Initialise the reader and immediately parse the class-file bytes.

        Args:
            bytes_or_bytearray: Raw bytes of a ``.class`` file.

        Raises:
            MalformedClassException: If the bytes are not a valid class file.
        """
        super().__init__(bytes_or_bytearray)
        self.constant_pool = []
        self.read_class()

    @classmethod
    def from_file(cls, path):
        """Construct a :class:`ClassReader` from a ``.class`` file on disk.

        Args:
            path: Filesystem path to the ``.class`` file.

        Returns:
            A fully-parsed :class:`ClassReader` instance.
        """
        with open(path, "rb") as f:
            file_bytes = f.read()
        return cls(file_bytes)

    @classmethod
    def from_bytes(cls, bytes_or_bytearray):
        """Construct a :class:`ClassReader` from raw bytes.

        Args:
            bytes_or_bytearray: Raw bytes of a ``.class`` file.

        Returns:
            A fully-parsed :class:`ClassReader` instance.
        """
        return cls(bytes_or_bytearray)

    cpdef object read_constant_pool_index(self, int index):
        """Read a single constant-pool entry at the given logical index (JVMS §4.4).

        Args:
            index: One-based constant-pool index for the entry being read.

        Returns:
            A tuple of the parsed constant-pool info object and the number of
            extra index slots consumed (1 for ``long``/``double``, else 0).

        Raises:
            ValueError: If the constant-pool tag is unrecognised.
        """
        cdef int index_extra, offset, tag
        index_extra, offset, tag = 0, self.offset, _fast_u1(self)
        cp_type = _constant_pool_info_type(tag)

        if cp_type is constant_pool.ConstantPoolInfoType.CLASS:
            cp_info = constant_pool.ClassInfo(index, offset, tag, _fast_u2(self))
        elif cp_type is constant_pool.ConstantPoolInfoType.STRING:
            cp_info = constant_pool.StringInfo(index, offset, tag, _fast_u2(self))
        elif cp_type is constant_pool.ConstantPoolInfoType.METHOD_TYPE:
            cp_info = constant_pool.MethodTypeInfo(index, offset, tag, _fast_u2(self))
        elif cp_type is constant_pool.ConstantPoolInfoType.MODULE:
            cp_info = constant_pool.ModuleInfo(index, offset, tag, _fast_u2(self))
        elif cp_type is constant_pool.ConstantPoolInfoType.PACKAGE:
            cp_info = constant_pool.PackageInfo(index, offset, tag, _fast_u2(self))
        elif cp_type is constant_pool.ConstantPoolInfoType.FIELD_REF:
            cp_info = constant_pool.FieldrefInfo(index, offset, tag, _fast_u2(self), _fast_u2(self))
        elif cp_type is constant_pool.ConstantPoolInfoType.METHOD_REF:
            cp_info = constant_pool.MethodrefInfo(index, offset, tag, _fast_u2(self), _fast_u2(self))
        elif cp_type is constant_pool.ConstantPoolInfoType.INTERFACE_METHOD_REF:
            cp_info = constant_pool.InterfaceMethodrefInfo(index, offset, tag, _fast_u2(self), _fast_u2(self))
        elif cp_type is constant_pool.ConstantPoolInfoType.NAME_AND_TYPE:
            cp_info = constant_pool.NameAndTypeInfo(index, offset, tag, _fast_u2(self), _fast_u2(self))
        elif cp_type is constant_pool.ConstantPoolInfoType.DYNAMIC:
            cp_info = constant_pool.DynamicInfo(index, offset, tag, _fast_u2(self), _fast_u2(self))
        elif cp_type is constant_pool.ConstantPoolInfoType.INVOKE_DYNAMIC:
            cp_info = constant_pool.InvokeDynamicInfo(index, offset, tag, _fast_u2(self), _fast_u2(self))
        elif cp_type is constant_pool.ConstantPoolInfoType.INTEGER:
            cp_info = constant_pool.IntegerInfo(index, offset, tag, _fast_u4(self))
        elif cp_type is constant_pool.ConstantPoolInfoType.FLOAT:
            cp_info = constant_pool.FloatInfo(index, offset, tag, _fast_u4(self))
        elif cp_type is constant_pool.ConstantPoolInfoType.LONG:
            cp_info = constant_pool.LongInfo(index, offset, tag, _fast_u4(self), _fast_u4(self))
            index_extra = 1
        elif cp_type is constant_pool.ConstantPoolInfoType.DOUBLE:
            cp_info = constant_pool.DoubleInfo(index, offset, tag, _fast_u4(self), _fast_u4(self))
            index_extra = 1
        elif cp_type is constant_pool.ConstantPoolInfoType.UTF8:
            length = _fast_u2(self)
            str_bytes = self.read_bytes(length)
            cp_info = constant_pool.Utf8Info(index, offset, tag, length, str_bytes)
        elif cp_type is constant_pool.ConstantPoolInfoType.METHOD_HANDLE:
            cp_info = constant_pool.MethodHandleInfo(index, offset, tag, _fast_u1(self), _fast_u2(self))
        else:
            raise ValueError(f"Unknown ConstantPoolInfoType: {cp_type}")
        return cp_info, index_extra

    cpdef object read_align_bytes(self, int current_offset):
        """Read and discard padding bytes to reach 4-byte alignment.

        Used by ``tableswitch`` and ``lookupswitch`` instructions whose
        operands must be 4-byte aligned (JVMS §6.5).

        Args:
            current_offset: Current bytecode offset within the method body.

        Returns:
            The consumed padding bytes (0–3 bytes).
        """
        cdef int align_bytes
        align_bytes = (4 - current_offset % 4) % 4
        return self.read_bytes(align_bytes)

    cpdef object read_instruction(self, int current_method_offset):
        """Read a single JVM bytecode instruction (JVMS §6.5).

        Args:
            current_method_offset: Byte offset of this instruction relative to
                the start of the method's ``Code`` attribute bytecode array.

        Returns:
            The decoded instruction info object.

        Raises:
            Exception: If the opcode or its ``wide`` variant is invalid.
        """
        cdef int opcode, wide_opcode
        cdef object instinfo, wide_inst_type, atype
        cdef Py_ssize_t index, value, count, dimensions, default, npairs, low, high
        opcode = _fast_u1(self)
        inst_type = _INSTRUCTION_TYPES[opcode]
        if inst_type is None:
            inst_type = instructions.InsnInfoType(opcode)
            instinfo = inst_type.instinfo
        else:
            instinfo = _INSTRUCTION_INFOS[opcode]
        if instinfo is LocalIndex:
            return _parsed_local_index(inst_type, current_method_offset, _fast_u1(self))
        elif instinfo is ConstPoolIndex:
            return _parsed_const_pool_index(inst_type, current_method_offset, _fast_u2(self))
        elif instinfo is ByteValue:
            return _parsed_byte_value(inst_type, current_method_offset, _fast_i1(self))
        elif instinfo is ShortValue:
            return _parsed_short_value(inst_type, current_method_offset, _fast_i2(self))
        elif instinfo is Branch:
            return _parsed_branch(inst_type, current_method_offset, _fast_i2(self))
        elif instinfo is BranchW:
            return _parsed_branch_w(inst_type, current_method_offset, _fast_i4(self))
        elif instinfo is IInc:
            index, value = _fast_u1(self), _fast_i1(self)
            return _parsed_iinc(inst_type, current_method_offset, index, value)
        elif instinfo is InvokeDynamic:
            index, unused = _fast_u2(self), self.read_bytes(2)
            return _parsed_invoke_dynamic(inst_type, current_method_offset, index, unused)
        elif instinfo is InvokeInterface:
            index, count, unused = _fast_u2(self), _fast_u1(self), self.read_bytes(1)
            return _parsed_invoke_interface(inst_type, current_method_offset, index, count, unused)
        elif instinfo is MultiANewArray:
            index, dimensions = _fast_u2(self), _fast_u1(self)
            return _parsed_multi_anew_array(inst_type, current_method_offset, index, dimensions)
        elif instinfo is NewArray:
            atype = _array_type(_fast_u1(self))
            return _parsed_new_array(inst_type, current_method_offset, atype)
        elif instinfo is LookupSwitch:
            self.read_align_bytes(current_method_offset + 1)
            default, npairs = _fast_i4(self), _fast_u4(self)
            pairs = [_parsed_match_offset_pair(_fast_i4(self), _fast_i4(self)) for _ in range(npairs)]
            return _parsed_lookup_switch(inst_type, current_method_offset, default, npairs, pairs)
        elif instinfo is TableSwitch:
            self.read_align_bytes(current_method_offset + 1)
            default, low, high = _fast_i4(self), _fast_i4(self), _fast_i4(self)
            offsets = [_fast_i4(self) for _ in range(high - low + 1)]
            return _parsed_table_switch(inst_type, current_method_offset, default, low, high, offsets)
        elif inst_type is instructions.InsnInfoType.WIDE:
            wide_opcode = _fast_u1(self)
            wide_inst_type = _INSTRUCTION_TYPES[opcode + wide_opcode]
            if wide_inst_type is None:
                wide_inst_type = instructions.InsnInfoType(opcode + wide_opcode)
                instinfo = wide_inst_type.instinfo
            else:
                instinfo = _INSTRUCTION_INFOS[opcode + wide_opcode]
            if instinfo is LocalIndexW:
                return _parsed_local_index_w(wide_inst_type, current_method_offset, _fast_u2(self))
            elif instinfo is IIncW:
                index, value = _fast_u2(self), _fast_i2(self)
                return _parsed_iinc_w(wide_inst_type, current_method_offset, index, value)
        elif instinfo is InsnInfo:
            return _parsed_insn(inst_type, current_method_offset)

        raise Exception(f"Invalid InstInfoType: {inst_type.name} {inst_type.instinfo}")

    cpdef list read_code_bytes(self, int code_length):
        """Read the full bytecode array of a ``Code`` attribute (JVMS §4.7.3).

        Args:
            code_length: Number of bytes in the bytecode array.

        Returns:
            Ordered list of decoded instructions.
        """
        cdef int start_method_offset, current_method_offset
        start_method_offset = self.offset
        results = []
        current_method_offset = self.offset - start_method_offset
        while current_method_offset < code_length:
            insn = self.read_instruction(current_method_offset)
            results.append(insn)
            current_method_offset = self.offset - start_method_offset
        return results

    cpdef object read_verification_type_info(self):
        """Read a single ``verification_type_info`` union (JVMS §4.7.4).

        Returns:
            The decoded verification-type info variant.

        Raises:
            ValueError: If the verification-type tag is unrecognised.
        """
        cdef int tag
        tag = _fast_u1(self)
        if tag == constants.VerificationType.TOP:
            return _parsed_top_variable_info(tag)
        elif tag == constants.VerificationType.INTEGER:
            return _parsed_integer_variable_info(tag)
        elif tag == constants.VerificationType.FLOAT:
            return _parsed_float_variable_info(tag)
        elif tag == constants.VerificationType.DOUBLE:
            return _parsed_double_variable_info(tag)
        elif tag == constants.VerificationType.LONG:
            return _parsed_long_variable_info(tag)
        elif tag == constants.VerificationType.NULL:
            return _parsed_null_variable_info(tag)
        elif tag == constants.VerificationType.UNINITIALIZED_THIS:
            return _parsed_uninitialized_this_variable_info(tag)
        elif tag == constants.VerificationType.OBJECT:
            return _parsed_object_variable_info(tag, _fast_u2(self))
        elif tag == constants.VerificationType.UNINITIALIZED:
            return _parsed_uninitialized_variable_info(tag, _fast_u2(self))
        else:
            raise ValueError(f"Unknown verification type tag: {tag}")

    cpdef object read_element_value_info(self):
        """Read an ``element_value`` structure from an annotation (JVMS §4.7.16.1).

        Returns:
            The decoded element-value info.

        Raises:
            ValueError: If the element-value tag character is unrecognised.
        """
        cdef int num_values
        tag = _fast_u1(self).to_bytes(1, "big").decode("ascii")

        if tag in ("B", "C", "D", "F", "I", "J", "S", "Z", "s"):
            return attributes.ElementValueInfo(tag, attributes.ConstValueInfo(_fast_u2(self)))
        elif tag == "e":
            return attributes.ElementValueInfo(
                tag,
                attributes.EnumConstantValueInfo(_fast_u2(self), _fast_u2(self)),
            )
        elif tag == "c":
            return attributes.ElementValueInfo(tag, attributes.ClassInfoValueInfo(_fast_u2(self)))
        elif tag == "@":
            return attributes.ElementValueInfo(tag, self.read_annotation_info())
        elif tag == "[":
            num_values = _fast_u2(self)
            values = [self.read_element_value_info() for _ in range(num_values)]
            return attributes.ElementValueInfo(tag, attributes.ArrayValueInfo(num_values, values))
        else:
            raise ValueError(f"Unknown element value tag: {tag}")

    cpdef object read_annotation_info(self):
        """Read an ``annotation`` structure (JVMS §4.7.16).

        Returns:
            The decoded annotation info including its element-value pairs.
        """
        cdef int type_index, num_element_value_pairs
        type_index = _fast_u2(self)
        num_element_value_pairs = _fast_u2(self)
        element_value_pairs = [
            attributes.ElementValuePairInfo(_fast_u2(self), self.read_element_value_info())
            for _ in range(num_element_value_pairs)
        ]
        return attributes.AnnotationInfo(type_index, num_element_value_pairs, element_value_pairs)

    cpdef object read_target_info(self, int target_type):
        """Read a ``target_info`` union for a type annotation (JVMS §4.7.20).

        Args:
            target_type: The ``target_type`` byte that selects the union variant.

        Returns:
            The decoded target info variant.

        Raises:
            ValueError: If the target type is unrecognised.
        """
        cdef int table_length
        if target_type in constants.TargetInfoType.TYPE_PARAMETER.value:
            return attributes.TypeParameterTargetInfo(_fast_u1(self))
        elif target_type in constants.TargetInfoType.SUPERTYPE.value:
            return attributes.SupertypeTargetInfo(_fast_u2(self))
        elif target_type in constants.TargetInfoType.TYPE_PARAMETER_BOUND.value:
            return attributes.TypeParameterBoundTargetInfo(_fast_u1(self), _fast_u1(self))
        elif target_type in constants.TargetInfoType.EMPTY.value:
            return attributes.EmptyTargetInfo()
        elif target_type in constants.TargetInfoType.FORMAL_PARAMETER.value:
            return attributes.FormalParameterTargetInfo(_fast_u1(self))
        elif target_type in constants.TargetInfoType.THROWS.value:
            return attributes.ThrowsTargetInfo(_fast_u2(self))
        elif target_type in constants.TargetInfoType.LOCALVAR.value:
            table_length = _fast_u2(self)
            table = [
                attributes.TableInfo(_fast_u2(self), _fast_u2(self), _fast_u2(self)) for _ in range(table_length)
            ]
            return attributes.LocalvarTargetInfo(table_length, table)
        elif target_type in constants.TargetInfoType.CATCH.value:
            return attributes.CatchTargetInfo(_fast_u2(self))
        elif target_type in constants.TargetInfoType.OFFSET.value:
            return attributes.OffsetTargetInfo(_fast_u2(self))
        elif target_type in constants.TargetInfoType.TYPE_ARGUMENT.value:
            return attributes.TypeArgumentTargetInfo(_fast_u2(self), _fast_u1(self))
        else:
            raise ValueError(f"Unknown target info type: {target_type}")

    cpdef object read_target_path(self):
        """Read a ``type_path`` structure for a type annotation (JVMS §4.7.20.2).

        Returns:
            The decoded type-path info.
        """
        cdef int path_length
        path_length = _fast_u1(self)
        path = [attributes.PathInfo(_fast_u1(self), _fast_u1(self)) for _ in range(path_length)]
        return attributes.TypePathInfo(path_length, path)

    cpdef object read_type_annotation_info(self):
        """Read a ``type_annotation`` structure (JVMS §4.7.20).

        Returns:
            The decoded type-annotation info.
        """
        cdef int target_type, type_index, num_element_value_pairs
        target_type = _fast_u1(self)
        target_info = self.read_target_info(target_type)
        target_path = self.read_target_path()
        type_index = _fast_u2(self)
        num_element_value_pairs = _fast_u2(self)
        element_value_pairs = [
            attributes.ElementValuePairInfo(_fast_u2(self), self.read_element_value_info())
            for _ in range(num_element_value_pairs)
        ]
        return attributes.TypeAnnotationInfo(
            target_type,
            target_info,
            target_path,
            type_index,
            num_element_value_pairs,
            element_value_pairs,
        )

    cpdef object read_attribute(self):
        """Read a single ``attribute_info`` structure (JVMS §4.7).

        Recognised attribute names are decoded into their specific subtypes;
        unknown attributes are returned as :class:`~pytecode.classfile.attributes.UnimplementedAttr`.

        Returns:
            The decoded attribute info.

        Raises:
            ValueError: If the attribute name index does not reference a
                ``CONSTANT_Utf8_info`` entry.
        """
        cdef int name_index
        cdef unsigned int length
        cdef int number_of_entries, frame_type
        cdef int number_of_exceptions
        cdef int number_of_classes
        cdef int num_annotations, num_parameters
        cdef int num_bootstrap_methods, parameters_count
        cdef int components_count
        cdef int line_number_table_length
        cdef int local_variable_table_length, local_variable_type_table_length
        cdef int exception_table_length, attributes_count
        cdef int max_stack, max_locals
        cdef unsigned int code_length

        name_index, length = _fast_u2(self), _fast_u4(self)

        name_cp = self.constant_pool[name_index]
        if type(name_cp) is not constant_pool.Utf8Info:
            raise ValueError(f"name_index({name_index}) should be Utf8Info, not {type(name_cp)}")

        name = decode_modified_utf8(name_cp.str_bytes)
        attr_type = _attribute_info_type(name)

        if attr_type is attributes.AttributeInfoType.SYNTHETIC:
            return attributes.SyntheticAttr(name_index, length)

        elif attr_type is attributes.AttributeInfoType.DEPRECATED:
            return attributes.DeprecatedAttr(name_index, length)

        elif attr_type is attributes.AttributeInfoType.CONSTANT_VALUE:
            return attributes.ConstantValueAttr(name_index, length, _fast_u2(self))

        elif attr_type is attributes.AttributeInfoType.SIGNATURE:
            return attributes.SignatureAttr(name_index, length, _fast_u2(self))

        elif attr_type is attributes.AttributeInfoType.SOURCE_FILE:
            return attributes.SourceFileAttr(name_index, length, _fast_u2(self))

        elif attr_type is attributes.AttributeInfoType.MODULE_MAIN_CLASS:
            return attributes.ModuleMainClassAttr(name_index, length, _fast_u2(self))

        elif attr_type is attributes.AttributeInfoType.NEST_HOST:
            return attributes.NestHostAttr(name_index, length, _fast_u2(self))

        elif attr_type is attributes.AttributeInfoType.CODE:
            max_stack, max_locals = _fast_u2(self), _fast_u2(self)
            code_length = _fast_u4(self)
            code = self.read_code_bytes(code_length)
            exception_table_length = _fast_u2(self)
            exception_table = [
                _parsed_exception_info(_fast_u2(self), _fast_u2(self), _fast_u2(self), _fast_u2(self))
                for _ in range(exception_table_length)
            ]
            attributes_count = _fast_u2(self)
            attributes_list = [self.read_attribute() for _ in range(attributes_count)]
            return _parsed_code_attr(
                name_index,
                length,
                max_stack,
                max_locals,
                code_length,
                code,
                exception_table_length,
                exception_table,
                attributes_count,
                attributes_list,
            )

        elif attr_type is attributes.AttributeInfoType.STACK_MAP_TABLE:
            number_of_entries = _fast_u2(self)
            entries = []
            for _ in range(number_of_entries):
                frame_type = _fast_u1(self)

                if 0 <= frame_type < 64:
                    entries.append(_parsed_same_frame_info(frame_type))
                elif 64 <= frame_type < 128:
                    entries.append(_parsed_same_locals_1_stack_item_frame_info(frame_type, self.read_verification_type_info()))
                elif frame_type == 247:
                    entries.append(
                        _parsed_same_locals_1_stack_item_frame_extended_info(
                            frame_type,
                            _fast_u2(self),
                            self.read_verification_type_info(),
                        )
                    )
                elif 248 <= frame_type <= 250:
                    entries.append(_parsed_chop_frame_info(frame_type, _fast_u2(self)))
                elif frame_type == 251:
                    entries.append(_parsed_same_frame_extended_info(frame_type, _fast_u2(self)))
                elif 252 <= frame_type <= 254:
                    offset_delta = _fast_u2(self)
                    verification_type_infos = [self.read_verification_type_info() for __ in range(frame_type - 251)]
                    entries.append(_parsed_append_frame_info(frame_type, offset_delta, verification_type_infos))
                elif frame_type == 255:
                    offset_delta = _fast_u2(self)
                    number_of_locals = _fast_u2(self)
                    locals = [self.read_verification_type_info() for __ in range(number_of_locals)]
                    number_of_stack_items = _fast_u2(self)
                    stack = [self.read_verification_type_info() for __ in range(number_of_stack_items)]
                    entries.append(
                        _parsed_full_frame_info(
                            frame_type,
                            offset_delta,
                            number_of_locals,
                            locals,
                            number_of_stack_items,
                            stack,
                        )
                    )
                else:
                    raise ValueError(f"Unknown stack map frame type: {frame_type}")

            return _parsed_stack_map_table_attr(name_index, length, number_of_entries, entries)

        elif attr_type is attributes.AttributeInfoType.EXCEPTIONS:
            number_of_exceptions = _fast_u2(self)
            exception_index_table = [_fast_u2(self) for _ in range(number_of_exceptions)]
            return attributes.ExceptionsAttr(name_index, length, number_of_exceptions, exception_index_table)

        elif attr_type is attributes.AttributeInfoType.INNER_CLASSES:
            number_of_classes = _fast_u2(self)
            classes = [
                InnerClassInfo(
                    _fast_u2(self),
                    _fast_u2(self),
                    _fast_u2(self),
                    _enum_member(constants.NestedClassAccessFlag, _fast_u2(self)),
                )
                for _ in range(number_of_classes)
            ]
            return attributes.InnerClassesAttr(name_index, length, number_of_classes, classes)

        elif attr_type is attributes.AttributeInfoType.ENCLOSING_METHOD:
            return attributes.EnclosingMethodAttr(name_index, length, _fast_u2(self), _fast_u2(self))

        elif attr_type is attributes.AttributeInfoType.SOURCE_DEBUG_EXTENSION:
            return attributes.SourceDebugExtensionAttr(name_index, length, self.read_bytes(length).decode("utf-8"))

        elif attr_type is attributes.AttributeInfoType.LINE_NUMBER_TABLE:
            line_number_table_length = _fast_u2(self)
            line_number_table = [
                _parsed_line_number_info(_fast_u2(self), _fast_u2(self)) for _ in range(line_number_table_length)
            ]
            return _parsed_line_number_table_attr(name_index, length, line_number_table_length, line_number_table)

        elif attr_type is attributes.AttributeInfoType.LOCAL_VARIABLE_TABLE:
            local_variable_table_length = _fast_u2(self)
            local_variable_table = [
                _parsed_local_variable_info(
                    _fast_u2(self),
                    _fast_u2(self),
                    _fast_u2(self),
                    _fast_u2(self),
                    _fast_u2(self),
                )
                for _ in range(local_variable_table_length)
            ]
            return _parsed_local_variable_table_attr(name_index, length, local_variable_table_length, local_variable_table)

        elif attr_type is attributes.AttributeInfoType.LOCAL_VARIABLE_TYPE_TABLE:
            local_variable_type_table_length = _fast_u2(self)
            local_variable_type_table = [
                _parsed_local_variable_type_info(
                    _fast_u2(self),
                    _fast_u2(self),
                    _fast_u2(self),
                    _fast_u2(self),
                    _fast_u2(self),
                )
                for _ in range(local_variable_type_table_length)
            ]
            return _parsed_local_variable_type_table_attr(
                name_index,
                length,
                local_variable_type_table_length,
                local_variable_type_table,
            )

        elif attr_type is attributes.AttributeInfoType.RUNTIME_VISIBLE_ANNOTATIONS:
            num_annotations = _fast_u2(self)
            annotation_list = [self.read_annotation_info() for _ in range(num_annotations)]
            return attributes.RuntimeVisibleAnnotationsAttr(name_index, length, num_annotations, annotation_list)

        elif attr_type is attributes.AttributeInfoType.RUNTIME_INVISIBLE_ANNOTATIONS:
            num_annotations = _fast_u2(self)
            annotation_list = [self.read_annotation_info() for _ in range(num_annotations)]
            return attributes.RuntimeInvisibleAnnotationsAttr(name_index, length, num_annotations, annotation_list)

        elif attr_type is attributes.AttributeInfoType.RUNTIME_VISIBLE_PARAMETER_ANNOTATIONS:
            num_parameters = _fast_u1(self)
            parameter_annotations = []
            for _ in range(num_parameters):
                num_annotations = _fast_u2(self)
                annotation_list = [self.read_annotation_info() for _ in range(num_annotations)]
                parameter_annotations.append(attributes.ParameterAnnotationInfo(num_annotations, annotation_list))
            return attributes.RuntimeVisibleParameterAnnotationsAttr(
                name_index, length, num_parameters, parameter_annotations
            )

        elif attr_type is attributes.AttributeInfoType.RUNTIME_INVISIBLE_PARAMETER_ANNOTATIONS:
            num_parameters = _fast_u1(self)
            parameter_annotations_list = []
            for _ in range(num_parameters):
                num_annotations = _fast_u2(self)
                annotation_list = [self.read_annotation_info() for _ in range(num_annotations)]
                parameter_annotations_list.append(attributes.ParameterAnnotationInfo(num_annotations, annotation_list))
            return attributes.RuntimeInvisibleParameterAnnotationsAttr(
                name_index, length, num_parameters, parameter_annotations_list
            )

        elif attr_type is attributes.AttributeInfoType.RUNTIME_VISIBLE_TYPE_ANNOTATIONS:
            num_annotations = _fast_u2(self)
            type_annotation_list = [self.read_type_annotation_info() for _ in range(num_annotations)]
            return attributes.RuntimeVisibleTypeAnnotationsAttr(
                name_index, length, num_annotations, type_annotation_list
            )

        elif attr_type is attributes.AttributeInfoType.RUNTIME_INVISIBLE_TYPE_ANNOTATIONS:
            num_annotations = _fast_u2(self)
            type_annotation_list = [self.read_type_annotation_info() for _ in range(num_annotations)]
            return attributes.RuntimeInvisibleTypeAnnotationsAttr(
                name_index, length, num_annotations, type_annotation_list
            )

        elif attr_type is attributes.AttributeInfoType.ANNOTATION_DEFAULT:
            return attributes.AnnotationDefaultAttr(name_index, length, self.read_element_value_info())

        elif attr_type is attributes.AttributeInfoType.BOOTSTRAP_METHODS:
            num_bootstrap_methods = _fast_u2(self)
            bootstrap_methods = []
            for _ in range(num_bootstrap_methods):
                bootstrap_method_ref = _fast_u2(self)
                num_bootstrap_arguments = _fast_u2(self)
                bootstrap_arguments = [_fast_u2(self) for __ in range(num_bootstrap_arguments)]
                bootstrap_methods.append(
                    BootstrapMethodInfo(
                        bootstrap_method_ref,
                        num_bootstrap_arguments,
                        bootstrap_arguments,
                    )
                )
            return attributes.BootstrapMethodsAttr(name_index, length, num_bootstrap_methods, bootstrap_methods)

        elif attr_type is attributes.AttributeInfoType.METHOD_PARAMETERS:
            parameters_count = _fast_u1(self)
            parameters = [
                MethodParameterInfo(
                    _fast_u2(self),
                    _enum_member(constants.MethodParameterAccessFlag, _fast_u2(self)),
                )
                for _ in range(parameters_count)
            ]
            return attributes.MethodParametersAttr(name_index, length, parameters_count, parameters)

        elif attr_type is attributes.AttributeInfoType.MODULE:
            module_name_index = _fast_u2(self)
            module_flags = _enum_member(constants.ModuleAccessFlag, _fast_u2(self))
            module_version_index = _fast_u2(self)

            requires_count = _fast_u2(self)
            requires = [
                attributes.RequiresInfo(
                    _fast_u2(self),
                    _enum_member(constants.ModuleRequiresAccessFlag, _fast_u2(self)),
                    _fast_u2(self),
                )
                for _ in range(requires_count)
            ]

            exports_count = _fast_u2(self)
            exports = []
            for _ in range(exports_count):
                exports_index = _fast_u2(self)
                exports_flags = _enum_member(constants.ModuleExportsAccessFlag, _fast_u2(self))
                exports_to_count = _fast_u2(self)
                exports_to_index = [_fast_u2(self) for __ in range(exports_to_count)]
                exports.append(attributes.ExportInfo(exports_index, exports_flags, exports_to_count, exports_to_index))

            opens_count = _fast_u2(self)
            opens = []
            for _ in range(opens_count):
                opens_index = _fast_u2(self)
                opens_flags = _enum_member(constants.ModuleOpensAccessFlag, _fast_u2(self))
                opens_to_count = _fast_u2(self)
                opens_to_index = [_fast_u2(self) for __ in range(opens_to_count)]
                opens.append(attributes.OpensInfo(opens_index, opens_flags, opens_to_count, opens_to_index))

            uses_count = _fast_u2(self)
            uses = [_fast_u2(self) for _ in range(uses_count)]

            provides_count = _fast_u2(self)
            provides = []
            for _ in range(provides_count):
                provides_index = _fast_u2(self)
                provides_with_count = _fast_u2(self)
                provides_with_index = [_fast_u2(self) for __ in range(provides_with_count)]
                provides.append(attributes.ProvidesInfo(provides_index, provides_with_count, provides_with_index))

            return attributes.ModuleAttr(
                name_index,
                length,
                module_name_index,
                module_flags,
                module_version_index,
                requires_count,
                requires,
                exports_count,
                exports,
                opens_count,
                opens,
                uses_count,
                uses,
                provides_count,
                provides,
            )

        elif attr_type is attributes.AttributeInfoType.MODULE_PACKAGES:
            package_count = _fast_u2(self)
            package_index = [_fast_u2(self) for _ in range(package_count)]
            return attributes.ModulePackagesAttr(name_index, length, package_count, package_index)

        elif attr_type is attributes.AttributeInfoType.NEST_MEMBERS:
            number_of_classes = _fast_u2(self)
            classes_list = [_fast_u2(self) for _ in range(number_of_classes)]
            return attributes.NestMembersAttr(name_index, length, number_of_classes, classes_list)

        elif attr_type is attributes.AttributeInfoType.RECORD:
            components_count = _fast_u2(self)
            components = []
            for _ in range(components_count):
                comp_name_index = _fast_u2(self)
                descriptor_index = _fast_u2(self)
                attributes_count = _fast_u2(self)
                _attributes = [self.read_attribute() for _ in range(attributes_count)]
                components.append(
                    RecordComponentInfo(comp_name_index, descriptor_index, attributes_count, _attributes)
                )
            return attributes.RecordAttr(name_index, length, components_count, components)

        elif attr_type is attributes.AttributeInfoType.PERMITTED_SUBCLASSES:
            number_of_classes = _fast_u2(self)
            classes_list = [_fast_u2(self) for _ in range(number_of_classes)]
            return attributes.PermittedSubclassesAttr(name_index, length, number_of_classes, classes_list)

        return attributes.UnimplementedAttr(name_index, length, self.read_bytes(length), attr_type)

    cpdef object read_field(self):
        """Read a single ``field_info`` structure (JVMS §4.5).

        Returns:
            The decoded field info including its attributes.
        """
        cdef int name_index, descriptor_index, attributes_count
        access_flags = _enum_member(constants.FieldAccessFlag, _fast_u2(self))
        name_index = _fast_u2(self)
        descriptor_index = _fast_u2(self)
        attributes_count = _fast_u2(self)
        attributes = [self.read_attribute() for _ in range(attributes_count)]
        return info.FieldInfo(access_flags, name_index, descriptor_index, attributes_count, attributes)

    cpdef object read_method(self):
        """Read a single ``method_info`` structure (JVMS §4.6).

        Returns:
            The decoded method info including its attributes.
        """
        cdef int name_index, descriptor_index, attributes_count
        access_flags = _enum_member(constants.MethodAccessFlag, _fast_u2(self))
        name_index = _fast_u2(self)
        descriptor_index = _fast_u2(self)
        attributes_count = _fast_u2(self)
        attributes = [self.read_attribute() for _ in range(attributes_count)]
        return info.MethodInfo(access_flags, name_index, descriptor_index, attributes_count, attributes)

    cpdef object read_class(self):
        """Parse the complete ``ClassFile`` structure (JVMS §4.1).

        Validates the magic number and version, reads the constant pool,
        access flags, class hierarchy info, fields, methods, and attributes.
        The result is stored in :attr:`class_info`.

        Raises:
            MalformedClassException: If the magic number or version is invalid.
        """
        cdef unsigned int magic
        cdef int minor, major, cp_count, index, index_extra
        cdef int this_class, super_class
        cdef int interfaces_count, fields_count, methods_count, attributes_count

        self.rewind()
        magic = _fast_u4(self)
        if magic != constants.MAGIC:
            raise MalformedClassException(f"Invalid magic number 0x{magic:x}, requires 0x{constants.MAGIC:x}")

        minor, major = _fast_u2(self), _fast_u2(self)
        if major >= 56 and minor not in (0, 65535):
            raise MalformedClassException(f"Invalid version {major}/{minor}")

        cp_count = _fast_u2(self)

        self.constant_pool = [None] * cp_count
        index = 1
        while index < cp_count:
            cp_info, index_extra = self.read_constant_pool_index(index)
            self.constant_pool[index] = cp_info
            index += 1 + index_extra

        access_flags = _enum_member(constants.ClassAccessFlag, _fast_u2(self))
        this_class = _fast_u2(self)
        super_class = _fast_u2(self)

        interfaces_count = _fast_u2(self)
        interfaces = [_fast_u2(self) for _ in range(interfaces_count)]

        fields_count = _fast_u2(self)
        fields = [self.read_field() for _ in range(fields_count)]

        methods_count = _fast_u2(self)
        methods = [self.read_method() for _ in range(methods_count)]

        attributes_count = _fast_u2(self)
        attributes = [self.read_attribute() for _ in range(attributes_count)]

        self.class_info = info.ClassFile(
            magic,
            minor,
            major,
            cp_count,
            self.constant_pool,
            access_flags,
            this_class,
            super_class,
            interfaces_count,
            interfaces,
            fields_count,
            fields,
            methods_count,
            methods,
            attributes_count,
            attributes,
        )
