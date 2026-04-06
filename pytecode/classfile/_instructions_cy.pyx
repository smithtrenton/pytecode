"""Data types representing JVM bytecode instruction operands.

Provides instruction operand classes and enums that model the operand formats
for each JVM instruction as defined in the JVM specification (JVMS §6.5). Each
opcode is mapped to an ``InsnInfoType`` member whose associated ``InsnInfo``
subclass describes the shape of its operands.
"""

import copy
from enum import IntEnum

__all__ = [
    "ArrayType",
    "Branch",
    "BranchW",
    "ByteValue",
    "ConstPoolIndex",
    "IInc",
    "IIncW",
    "InsnInfo",
    "InsnInfoType",
    "InvokeDynamic",
    "InvokeInterface",
    "LocalIndex",
    "LocalIndexW",
    "LookupSwitch",
    "MatchOffsetPair",
    "MultiANewArray",
    "NewArray",
    "ShortValue",
    "TableSwitch",
]

def _repr_fields(str class_name, tuple fields):
    return f"{class_name}(" + ", ".join(f"{name}={value!r}" for name, value in fields) + ")"


cdef class InsnInfo:
    """Base operand info for a JVM bytecode instruction.

    Attributes:
        type: The ``InsnInfoType`` member identifying the opcode.
        bytecode_offset: Byte offset of this instruction within the Code
            attribute.
    """

    def __init__(self, object type, Py_ssize_t bytecode_offset):
        self.type = type
        self.bytecode_offset = bytecode_offset

    @classmethod
    def _trusted(cls, object type, Py_ssize_t bytecode_offset):
        return _trusted_raw_insn(type, bytecode_offset)

    def _field_values(self):
        return (self.type, self.bytecode_offset)

    def _field_items(self):
        return (("type", self.type), ("bytecode_offset", self.bytecode_offset))

    def __repr__(self):
        return _repr_fields(type(self).__name__, self._field_items())

    def __richcmp__(self, other, int op):
        equal = type(self) is type(other) and self._field_values() == other._field_values()
        if op == 2:
            return equal
        if op == 3:
            return not equal
        return NotImplemented

    def __hash__(self):
        raise TypeError(f"unhashable type: '{type(self).__name__}'")

    def __copy__(self):
        return type(self)(*self._field_values())

    def __deepcopy__(self, memo):
        return type(self)(*copy.deepcopy(self._field_values(), memo))

    def __reduce__(self):
        return type(self), self._field_values()


cdef class LocalIndex(InsnInfo):
    """Operand carrying a single-byte local variable index (§6.5).

    Attributes:
        index: Local variable slot index (0–255).
    """

    def __init__(self, object type, Py_ssize_t bytecode_offset, Py_ssize_t index):
        InsnInfo.__init__(self, type, bytecode_offset)
        self.index = index

    @classmethod
    def _trusted(cls, object type, Py_ssize_t bytecode_offset, Py_ssize_t index):
        return _trusted_local_index(type, bytecode_offset, index)

    def _field_values(self):
        return (self.type, self.bytecode_offset, self.index)

    def _field_items(self):
        return InsnInfo._field_items(self) + (("index", self.index),)


cdef class LocalIndexW(InsnInfo):
    """Operand carrying a wide (two-byte) local variable index (§6.5.wide).

    Attributes:
        index: Local variable slot index (0–65535).
    """

    def __init__(self, object type, Py_ssize_t bytecode_offset, Py_ssize_t index):
        InsnInfo.__init__(self, type, bytecode_offset)
        self.index = index

    @classmethod
    def _trusted(cls, object type, Py_ssize_t bytecode_offset, Py_ssize_t index):
        return _trusted_local_index_w(type, bytecode_offset, index)

    def _field_values(self):
        return (self.type, self.bytecode_offset, self.index)

    def _field_items(self):
        return InsnInfo._field_items(self) + (("index", self.index),)


cdef class ConstPoolIndex(InsnInfo):
    """Operand carrying a two-byte constant pool index.

    Attributes:
        index: Index into the class file constant pool.
    """

    def __init__(self, object type, Py_ssize_t bytecode_offset, Py_ssize_t index):
        InsnInfo.__init__(self, type, bytecode_offset)
        self.index = index

    @classmethod
    def _trusted(cls, object type, Py_ssize_t bytecode_offset, Py_ssize_t index):
        return _trusted_const_pool_index(type, bytecode_offset, index)

    def _field_values(self):
        return (self.type, self.bytecode_offset, self.index)

    def _field_items(self):
        return InsnInfo._field_items(self) + (("index", self.index),)


cdef class ByteValue(InsnInfo):
    """Operand carrying a signed byte immediate value (e.g. ``bipush``).

    Attributes:
        value: Signed byte value (−128–127).
    """

    def __init__(self, object type, Py_ssize_t bytecode_offset, Py_ssize_t value):
        InsnInfo.__init__(self, type, bytecode_offset)
        self.value = value

    @classmethod
    def _trusted(cls, object type, Py_ssize_t bytecode_offset, Py_ssize_t value):
        return _trusted_byte_value(type, bytecode_offset, value)

    def _field_values(self):
        return (self.type, self.bytecode_offset, self.value)

    def _field_items(self):
        return InsnInfo._field_items(self) + (("value", self.value),)


cdef class ShortValue(InsnInfo):
    """Operand carrying a signed short immediate value (e.g. ``sipush``).

    Attributes:
        value: Signed short value (−32768–32767).
    """

    def __init__(self, object type, Py_ssize_t bytecode_offset, Py_ssize_t value):
        InsnInfo.__init__(self, type, bytecode_offset)
        self.value = value

    @classmethod
    def _trusted(cls, object type, Py_ssize_t bytecode_offset, Py_ssize_t value):
        return _trusted_short_value(type, bytecode_offset, value)

    def _field_values(self):
        return (self.type, self.bytecode_offset, self.value)

    def _field_items(self):
        return InsnInfo._field_items(self) + (("value", self.value),)


cdef class Branch(InsnInfo):
    """Operand for a branch instruction with a two-byte signed offset.

    Attributes:
        offset: Signed branch offset relative to this instruction.
    """

    def __init__(self, object type, Py_ssize_t bytecode_offset, Py_ssize_t offset):
        InsnInfo.__init__(self, type, bytecode_offset)
        self.offset = offset

    @classmethod
    def _trusted(cls, object type, Py_ssize_t bytecode_offset, Py_ssize_t offset):
        return _trusted_branch(type, bytecode_offset, offset)

    def _field_values(self):
        return (self.type, self.bytecode_offset, self.offset)

    def _field_items(self):
        return InsnInfo._field_items(self) + (("offset", self.offset),)


cdef class BranchW(InsnInfo):
    """Operand for a wide branch instruction with a four-byte signed offset.

    Attributes:
        offset: Signed branch offset relative to this instruction.
    """

    def __init__(self, object type, Py_ssize_t bytecode_offset, Py_ssize_t offset):
        InsnInfo.__init__(self, type, bytecode_offset)
        self.offset = offset

    @classmethod
    def _trusted(cls, object type, Py_ssize_t bytecode_offset, Py_ssize_t offset):
        return _trusted_branch_w(type, bytecode_offset, offset)

    def _field_values(self):
        return (self.type, self.bytecode_offset, self.offset)

    def _field_items(self):
        return InsnInfo._field_items(self) + (("offset", self.offset),)


cdef class IInc(InsnInfo):
    """Operand for the ``iinc`` instruction (§6.5.iinc).

    Attributes:
        index: Local variable slot index (0–255).
        value: Signed byte increment constant.
    """

    def __init__(self, object type, Py_ssize_t bytecode_offset, Py_ssize_t index, Py_ssize_t value):
        InsnInfo.__init__(self, type, bytecode_offset)
        self.index = index
        self.value = value

    @classmethod
    def _trusted(cls, object type, Py_ssize_t bytecode_offset, Py_ssize_t index, Py_ssize_t value):
        return _trusted_iinc(type, bytecode_offset, index, value)

    def _field_values(self):
        return (self.type, self.bytecode_offset, self.index, self.value)

    def _field_items(self):
        return InsnInfo._field_items(self) + (("index", self.index), ("value", self.value))


cdef class IIncW(InsnInfo):
    """Operand for the wide form of ``iinc`` (§6.5.wide).

    Attributes:
        index: Local variable slot index (0–65535).
        value: Signed short increment constant.
    """

    def __init__(self, object type, Py_ssize_t bytecode_offset, Py_ssize_t index, Py_ssize_t value):
        InsnInfo.__init__(self, type, bytecode_offset)
        self.index = index
        self.value = value

    @classmethod
    def _trusted(cls, object type, Py_ssize_t bytecode_offset, Py_ssize_t index, Py_ssize_t value):
        return _trusted_iinc_w(type, bytecode_offset, index, value)

    def _field_values(self):
        return (self.type, self.bytecode_offset, self.index, self.value)

    def _field_items(self):
        return InsnInfo._field_items(self) + (("index", self.index), ("value", self.value))


cdef class InvokeDynamic(InsnInfo):
    """Operand for the ``invokedynamic`` instruction (§6.5.invokedynamic).

    Attributes:
        index: Constant pool index to a ``CONSTANT_InvokeDynamic_info``.
        unused: Two reserved zero bytes following the index.
    """

    def __init__(self, object type, Py_ssize_t bytecode_offset, Py_ssize_t index, object unused):
        InsnInfo.__init__(self, type, bytecode_offset)
        self.index = index
        self.unused = unused

    @classmethod
    def _trusted(cls, object type, Py_ssize_t bytecode_offset, Py_ssize_t index, object unused):
        return _trusted_invoke_dynamic(type, bytecode_offset, index, unused)

    def _field_values(self):
        return (self.type, self.bytecode_offset, self.index, self.unused)

    def _field_items(self):
        return InsnInfo._field_items(self) + (("index", self.index), ("unused", self.unused))


cdef class InvokeInterface(InsnInfo):
    """Operand for the ``invokeinterface`` instruction (§6.5.invokeinterface).

    Attributes:
        index: Constant pool index to a ``CONSTANT_InterfaceMethodref_info``.
        count: Number of argument slots (non-zero).
        unused: One reserved zero byte following count.
    """

    def __init__(self, object type, Py_ssize_t bytecode_offset, Py_ssize_t index, Py_ssize_t count, object unused):
        InsnInfo.__init__(self, type, bytecode_offset)
        self.index = index
        self.count = count
        self.unused = unused

    @classmethod
    def _trusted(
        cls,
        object type,
        Py_ssize_t bytecode_offset,
        Py_ssize_t index,
        Py_ssize_t count,
        object unused,
    ):
        return _trusted_invoke_interface(type, bytecode_offset, index, count, unused)

    def _field_values(self):
        return (self.type, self.bytecode_offset, self.index, self.count, self.unused)

    def _field_items(self):
        return InsnInfo._field_items(self) + (
            ("index", self.index),
            ("count", self.count),
            ("unused", self.unused),
        )


cdef class NewArray(InsnInfo):
    """Operand for the ``newarray`` instruction (§6.5.newarray).

    Attributes:
        atype: ``ArrayType`` enum member identifying the primitive element type.
    """

    def __init__(self, object type, Py_ssize_t bytecode_offset, object atype):
        InsnInfo.__init__(self, type, bytecode_offset)
        self.atype = atype

    @classmethod
    def _trusted(cls, object type, Py_ssize_t bytecode_offset, object atype):
        return _trusted_new_array(type, bytecode_offset, atype)

    def _field_values(self):
        return (self.type, self.bytecode_offset, self.atype)

    def _field_items(self):
        return InsnInfo._field_items(self) + (("atype", self.atype),)


cdef class MultiANewArray(InsnInfo):
    """Operand for the ``multianewarray`` instruction (§6.5.multianewarray).

    Attributes:
        index: Constant pool index to the array class.
        dimensions: Number of dimensions to allocate (≥ 1).
    """

    def __init__(self, object type, Py_ssize_t bytecode_offset, Py_ssize_t index, Py_ssize_t dimensions):
        InsnInfo.__init__(self, type, bytecode_offset)
        self.index = index
        self.dimensions = dimensions

    @classmethod
    def _trusted(cls, object type, Py_ssize_t bytecode_offset, Py_ssize_t index, Py_ssize_t dimensions):
        return _trusted_multi_anew_array(type, bytecode_offset, index, dimensions)

    def _field_values(self):
        return (self.type, self.bytecode_offset, self.index, self.dimensions)

    def _field_items(self):
        return InsnInfo._field_items(self) + (("index", self.index), ("dimensions", self.dimensions))


cdef class MatchOffsetPair:
    """A single match-offset entry used in a ``lookupswitch`` table.

    Attributes:
        match: The integer case value.
        offset: Branch offset relative to the ``lookupswitch`` instruction.
    """

    def __init__(self, Py_ssize_t match, Py_ssize_t offset):
        self.match = match
        self.offset = offset

    @classmethod
    def _trusted(cls, Py_ssize_t match, Py_ssize_t offset):
        return _trusted_match_offset_pair(match, offset)

    def _field_values(self):
        return (self.match, self.offset)

    def _field_items(self):
        return (("match", self.match), ("offset", self.offset))

    def __repr__(self):
        return _repr_fields(type(self).__name__, self._field_items())

    def __richcmp__(self, other, int op):
        equal = type(self) is type(other) and self._field_values() == other._field_values()
        if op == 2:
            return equal
        if op == 3:
            return not equal
        return NotImplemented

    def __hash__(self):
        raise TypeError(f"unhashable type: '{type(self).__name__}'")

    def __copy__(self):
        return type(self)(*self._field_values())

    def __deepcopy__(self, memo):
        return type(self)(*copy.deepcopy(self._field_values(), memo))

    def __reduce__(self):
        return type(self), self._field_values()


cdef class LookupSwitch(InsnInfo):
    """Operand for the ``lookupswitch`` instruction (§6.5.lookupswitch).

    Attributes:
        default: Default branch offset when no key matches.
        npairs: Number of match-offset pairs.
        pairs: Sorted list of ``MatchOffsetPair`` entries.
    """

    def __init__(
        self,
        object type,
        Py_ssize_t bytecode_offset,
        Py_ssize_t default,
        Py_ssize_t npairs,
        list pairs,
    ):
        InsnInfo.__init__(self, type, bytecode_offset)
        self.default = default
        self.npairs = npairs
        self.pairs = pairs

    @classmethod
    def _trusted(
        cls,
        object type,
        Py_ssize_t bytecode_offset,
        Py_ssize_t default,
        Py_ssize_t npairs,
        list pairs,
    ):
        return _trusted_lookup_switch(type, bytecode_offset, default, npairs, pairs)

    def _field_values(self):
        return (self.type, self.bytecode_offset, self.default, self.npairs, self.pairs)

    def _field_items(self):
        return InsnInfo._field_items(self) + (
            ("default", self.default),
            ("npairs", self.npairs),
            ("pairs", self.pairs),
        )


cdef class TableSwitch(InsnInfo):
    """Operand for the ``tableswitch`` instruction (§6.5.tableswitch).

    Attributes:
        default: Default branch offset when the index is out of range.
        low: Lowest case index value.
        high: Highest case index value.
        offsets: Branch offsets for each case from *low* to *high* inclusive.
    """

    def __init__(
        self,
        object type,
        Py_ssize_t bytecode_offset,
        Py_ssize_t default,
        Py_ssize_t low,
        Py_ssize_t high,
        list offsets,
    ):
        InsnInfo.__init__(self, type, bytecode_offset)
        self.default = default
        self.low = low
        self.high = high
        self.offsets = offsets

    @classmethod
    def _trusted(
        cls,
        object type,
        Py_ssize_t bytecode_offset,
        Py_ssize_t default,
        Py_ssize_t low,
        Py_ssize_t high,
        list offsets,
    ):
        return _trusted_table_switch(type, bytecode_offset, default, low, high, offsets)

    def _field_values(self):
        return (self.type, self.bytecode_offset, self.default, self.low, self.high, self.offsets)

    def _field_items(self):
        return InsnInfo._field_items(self) + (
            ("default", self.default),
            ("low", self.low),
            ("high", self.high),
            ("offsets", self.offsets),
        )


cdef inline InsnInfo _trusted_raw_insn(object type, Py_ssize_t bytecode_offset):
    cdef InsnInfo self = InsnInfo.__new__(InsnInfo)
    self.type = type
    self.bytecode_offset = bytecode_offset
    return self


cdef inline LocalIndex _trusted_local_index(object type, Py_ssize_t bytecode_offset, Py_ssize_t index):
    cdef LocalIndex self = LocalIndex.__new__(LocalIndex)
    self.type = type
    self.bytecode_offset = bytecode_offset
    self.index = index
    return self


cdef inline LocalIndexW _trusted_local_index_w(object type, Py_ssize_t bytecode_offset, Py_ssize_t index):
    cdef LocalIndexW self = LocalIndexW.__new__(LocalIndexW)
    self.type = type
    self.bytecode_offset = bytecode_offset
    self.index = index
    return self


cdef inline ConstPoolIndex _trusted_const_pool_index(object type, Py_ssize_t bytecode_offset, Py_ssize_t index):
    cdef ConstPoolIndex self = ConstPoolIndex.__new__(ConstPoolIndex)
    self.type = type
    self.bytecode_offset = bytecode_offset
    self.index = index
    return self


cdef inline ByteValue _trusted_byte_value(object type, Py_ssize_t bytecode_offset, Py_ssize_t value):
    cdef ByteValue self = ByteValue.__new__(ByteValue)
    self.type = type
    self.bytecode_offset = bytecode_offset
    self.value = value
    return self


cdef inline ShortValue _trusted_short_value(object type, Py_ssize_t bytecode_offset, Py_ssize_t value):
    cdef ShortValue self = ShortValue.__new__(ShortValue)
    self.type = type
    self.bytecode_offset = bytecode_offset
    self.value = value
    return self


cdef inline Branch _trusted_branch(object type, Py_ssize_t bytecode_offset, Py_ssize_t offset):
    cdef Branch self = Branch.__new__(Branch)
    self.type = type
    self.bytecode_offset = bytecode_offset
    self.offset = offset
    return self


cdef inline BranchW _trusted_branch_w(object type, Py_ssize_t bytecode_offset, Py_ssize_t offset):
    cdef BranchW self = BranchW.__new__(BranchW)
    self.type = type
    self.bytecode_offset = bytecode_offset
    self.offset = offset
    return self


cdef inline IInc _trusted_iinc(object type, Py_ssize_t bytecode_offset, Py_ssize_t index, Py_ssize_t value):
    cdef IInc self = IInc.__new__(IInc)
    self.type = type
    self.bytecode_offset = bytecode_offset
    self.index = index
    self.value = value
    return self


cdef inline IIncW _trusted_iinc_w(object type, Py_ssize_t bytecode_offset, Py_ssize_t index, Py_ssize_t value):
    cdef IIncW self = IIncW.__new__(IIncW)
    self.type = type
    self.bytecode_offset = bytecode_offset
    self.index = index
    self.value = value
    return self


cdef inline InvokeDynamic _trusted_invoke_dynamic(
    object type,
    Py_ssize_t bytecode_offset,
    Py_ssize_t index,
    object unused,
):
    cdef InvokeDynamic self = InvokeDynamic.__new__(InvokeDynamic)
    self.type = type
    self.bytecode_offset = bytecode_offset
    self.index = index
    self.unused = unused
    return self


cdef inline InvokeInterface _trusted_invoke_interface(
    object type,
    Py_ssize_t bytecode_offset,
    Py_ssize_t index,
    Py_ssize_t count,
    object unused,
):
    cdef InvokeInterface self = InvokeInterface.__new__(InvokeInterface)
    self.type = type
    self.bytecode_offset = bytecode_offset
    self.index = index
    self.count = count
    self.unused = unused
    return self


cdef inline NewArray _trusted_new_array(object type, Py_ssize_t bytecode_offset, object atype):
    cdef NewArray self = NewArray.__new__(NewArray)
    self.type = type
    self.bytecode_offset = bytecode_offset
    self.atype = atype
    return self


cdef inline MultiANewArray _trusted_multi_anew_array(
    object type,
    Py_ssize_t bytecode_offset,
    Py_ssize_t index,
    Py_ssize_t dimensions,
):
    cdef MultiANewArray self = MultiANewArray.__new__(MultiANewArray)
    self.type = type
    self.bytecode_offset = bytecode_offset
    self.index = index
    self.dimensions = dimensions
    return self


cdef inline MatchOffsetPair _trusted_match_offset_pair(Py_ssize_t match, Py_ssize_t offset):
    cdef MatchOffsetPair self = MatchOffsetPair.__new__(MatchOffsetPair)
    self.match = match
    self.offset = offset
    return self


cdef inline LookupSwitch _trusted_lookup_switch(
    object type,
    Py_ssize_t bytecode_offset,
    Py_ssize_t default,
    Py_ssize_t npairs,
    list pairs,
):
    cdef LookupSwitch self = LookupSwitch.__new__(LookupSwitch)
    self.type = type
    self.bytecode_offset = bytecode_offset
    self.default = default
    self.npairs = npairs
    self.pairs = pairs
    return self


cdef inline TableSwitch _trusted_table_switch(
    object type,
    Py_ssize_t bytecode_offset,
    Py_ssize_t default,
    Py_ssize_t low,
    Py_ssize_t high,
    list offsets,
):
    cdef TableSwitch self = TableSwitch.__new__(TableSwitch)
    self.type = type
    self.bytecode_offset = bytecode_offset
    self.default = default
    self.low = low
    self.high = high
    self.offsets = offsets
    return self


class InsnInfoType(IntEnum):
    """Enum mapping every JVM opcode to its operand format.

    Each member's integer value is the opcode byte, and its ``instinfo``
    attribute is the ``InsnInfo`` subclass that describes the operand layout.
    """

    AALOAD = 0x32, InsnInfo
    AASTORE = 0x53, InsnInfo
    ACONST_NULL = 0x01, InsnInfo
    ALOAD = 0x19, LocalIndex
    ALOAD_0 = 0x2A, InsnInfo
    ALOAD_1 = 0x2B, InsnInfo
    ALOAD_2 = 0x2C, InsnInfo
    ALOAD_3 = 0x2D, InsnInfo
    ANEWARRAY = 0xBD, ConstPoolIndex
    ARETURN = 0xB0, InsnInfo
    ARRAYLENGTH = 0xBE, InsnInfo
    ASTORE = 0x3A, LocalIndex
    ASTORE_0 = 0x4B, InsnInfo
    ASTORE_1 = 0x4C, InsnInfo
    ASTORE_2 = 0x4D, InsnInfo
    ASTORE_3 = 0x4E, InsnInfo
    ATHROW = 0xBF, InsnInfo
    BALOAD = 0x33, InsnInfo
    BASTORE = 0x54, InsnInfo
    BIPUSH = 0x10, ByteValue
    CALOAD = 0x34, InsnInfo
    CASTORE = 0x55, InsnInfo
    CHECKCAST = 0xC0, ConstPoolIndex
    D2F = 0x90, InsnInfo
    D2I = 0x8E, InsnInfo
    D2L = 0x8F, InsnInfo
    DADD = 0x63, InsnInfo
    DALOAD = 0x31, InsnInfo
    DASTORE = 0x52, InsnInfo
    DCMPG = 0x98, InsnInfo
    DCMPL = 0x97, InsnInfo
    DCONST_0 = 0x0E, InsnInfo
    DCONST_1 = 0x0F, InsnInfo
    DDIV = 0x6F, InsnInfo
    DLOAD = 0x18, LocalIndex
    DLOAD_0 = 0x26, InsnInfo
    DLOAD_1 = 0x27, InsnInfo
    DLOAD_2 = 0x28, InsnInfo
    DLOAD_3 = 0x29, InsnInfo
    DMUL = 0x6B, InsnInfo
    DNEG = 0x77, InsnInfo
    DREM = 0x73, InsnInfo
    DRETURN = 0xAF, InsnInfo
    DSTORE = 0x39, LocalIndex
    DSTORE_0 = 0x47, InsnInfo
    DSTORE_1 = 0x48, InsnInfo
    DSTORE_2 = 0x49, InsnInfo
    DSTORE_3 = 0x4A, InsnInfo
    DSUB = 0x67, InsnInfo
    DUP = 0x59, InsnInfo
    DUP_X1 = 0x5A, InsnInfo
    DUP_X2 = 0x5B, InsnInfo
    DUP2 = 0x5C, InsnInfo
    DUP2_X1 = 0x5D, InsnInfo
    DUP2_X2 = 0x5E, InsnInfo
    F2D = 0x8D, InsnInfo
    F2I = 0x8B, InsnInfo
    F2L = 0x8C, InsnInfo
    FADD = 0x62, InsnInfo
    FALOAD = 0x30, InsnInfo
    FASTORE = 0x51, InsnInfo
    FCMPG = 0x96, InsnInfo
    FCMPL = 0x95, InsnInfo
    FCONST_0 = 0x0B, InsnInfo
    FCONST_1 = 0x0C, InsnInfo
    FCONST_2 = 0x0D, InsnInfo
    FDIV = 0x6E, InsnInfo
    FLOAD = 0x17, LocalIndex
    FLOAD_0 = 0x22, InsnInfo
    FLOAD_1 = 0x23, InsnInfo
    FLOAD_2 = 0x24, InsnInfo
    FLOAD_3 = 0x25, InsnInfo
    FMUL = 0x6A, InsnInfo
    FNEG = 0x76, InsnInfo
    FREM = 0x72, InsnInfo
    FRETURN = 0xAE, InsnInfo
    FSTORE = 0x38, LocalIndex
    FSTORE_0 = 0x43, InsnInfo
    FSTORE_1 = 0x44, InsnInfo
    FSTORE_2 = 0x45, InsnInfo
    FSTORE_3 = 0x46, InsnInfo
    FSUB = 0x66, InsnInfo
    GETFIELD = 0xB4, ConstPoolIndex
    GETSTATIC = 0xB2, ConstPoolIndex
    GOTO = 0xA7, Branch
    GOTO_W = 0xC8, BranchW
    I2B = 0x91, InsnInfo
    I2C = 0x92, InsnInfo
    I2D = 0x87, InsnInfo
    I2F = 0x86, InsnInfo
    I2L = 0x85, InsnInfo
    I2S = 0x93, InsnInfo
    IADD = 0x60, InsnInfo
    IALOAD = 0x2E, InsnInfo
    IAND = 0x7E, InsnInfo
    IASTORE = 0x4F, InsnInfo
    ICONST_M1 = 0x02, InsnInfo
    ICONST_0 = 0x03, InsnInfo
    ICONST_1 = 0x04, InsnInfo
    ICONST_2 = 0x05, InsnInfo
    ICONST_3 = 0x06, InsnInfo
    ICONST_4 = 0x07, InsnInfo
    ICONST_5 = 0x08, InsnInfo
    IDIV = 0x6C, InsnInfo
    IF_ACMPEQ = 0xA5, Branch
    IF_ACMPNE = 0xA6, Branch
    IF_ICMPEQ = 0x9F, Branch
    IF_ICMPGE = 0xA2, Branch
    IF_ICMPGT = 0xA3, Branch
    IF_ICMPLE = 0xA4, Branch
    IF_ICMPLT = 0xA1, Branch
    IF_ICMPNE = 0xA0, Branch
    IFEQ = 0x99, Branch
    IFGE = 0x9C, Branch
    IFGT = 0x9D, Branch
    IFLE = 0x9E, Branch
    IFLT = 0x9B, Branch
    IFNE = 0x9A, Branch
    IFNONNULL = 0xC7, Branch
    IFNULL = 0xC6, Branch
    IINC = 0x84, IInc
    ILOAD = 0x15, LocalIndex
    ILOAD_0 = 0x1A, InsnInfo
    ILOAD_1 = 0x1B, InsnInfo
    ILOAD_2 = 0x1C, InsnInfo
    ILOAD_3 = 0x1D, InsnInfo
    IMUL = 0x68, InsnInfo
    INEG = 0x74, InsnInfo
    INSTANCEOF = 0xC1, ConstPoolIndex
    INVOKEDYNAMIC = 0xBA, InvokeDynamic
    INVOKEINTERFACE = 0xB9, InvokeInterface
    INVOKESPECIAL = 0xB7, ConstPoolIndex
    INVOKESTATIC = 0xB8, ConstPoolIndex
    INVOKEVIRTUAL = 0xB6, ConstPoolIndex
    IOR = 0x80, InsnInfo
    IREM = 0x70, InsnInfo
    IRETURN = 0xAC, InsnInfo
    ISHL = 0x78, InsnInfo
    ISHR = 0x7A, InsnInfo
    ISTORE = 0x36, LocalIndex
    ISTORE_0 = 0x3B, InsnInfo
    ISTORE_1 = 0x3C, InsnInfo
    ISTORE_2 = 0x3D, InsnInfo
    ISTORE_3 = 0x3E, InsnInfo
    ISUB = 0x64, InsnInfo
    IUSHR = 0x7C, InsnInfo
    IXOR = 0x82, InsnInfo
    JSR = 0xA8, Branch
    JSR_W = 0xC9, BranchW
    L2D = 0x8A, InsnInfo
    L2F = 0x89, InsnInfo
    L2I = 0x88, InsnInfo
    LADD = 0x61, InsnInfo
    LALOAD = 0x2F, InsnInfo
    LAND = 0x7F, InsnInfo
    LASTORE = 0x50, InsnInfo
    LCMP = 0x94, InsnInfo
    LCONST_0 = 0x09, InsnInfo
    LCONST_1 = 0x0A, InsnInfo
    LDC = 0x12, LocalIndex
    LDC_W = 0x13, ConstPoolIndex
    LDC2_W = 0x14, ConstPoolIndex
    LDIV = 0x6D, InsnInfo
    LLOAD = 0x16, LocalIndex
    LLOAD_0 = 0x1E, InsnInfo
    LLOAD_1 = 0x1F, InsnInfo
    LLOAD_2 = 0x20, InsnInfo
    LLOAD_3 = 0x21, InsnInfo
    LMUL = 0x69, InsnInfo
    LNEG = 0x75, InsnInfo
    LOOKUPSWITCH = 0xAB, LookupSwitch
    LOR = 0x81, InsnInfo
    LREM = 0x71, InsnInfo
    LRETURN = 0xAD, InsnInfo
    LSHL = 0x79, InsnInfo
    LSHR = 0x7B, InsnInfo
    LSTORE = 0x37, LocalIndex
    LSTORE_0 = 0x3F, InsnInfo
    LSTORE_1 = 0x40, InsnInfo
    LSTORE_2 = 0x41, InsnInfo
    LSTORE_3 = 0x42, InsnInfo
    LSUB = 0x65, InsnInfo
    LUSHR = 0x7D, InsnInfo
    LXOR = 0x83, InsnInfo
    MONITORENTER = 0xC2, InsnInfo
    MONITOREXIT = 0xC3, InsnInfo
    MULTIANEWARRAY = 0xC5, MultiANewArray
    NEW = 0xBB, ConstPoolIndex
    NEWARRAY = 0xBC, NewArray
    NOP = 0x00, InsnInfo
    POP = 0x57, InsnInfo
    POP2 = 0x58, InsnInfo
    PUTFIELD = 0xB5, ConstPoolIndex
    PUTSTATIC = 0xB3, ConstPoolIndex
    RET = 0xA9, LocalIndex
    RETURN = 0xB1, InsnInfo
    SALOAD = 0x35, InsnInfo
    SASTORE = 0x56, InsnInfo
    SIPUSH = 0x11, ShortValue
    SWAP = 0x5F, InsnInfo
    TABLESWITCH = 0xAA, TableSwitch
    WIDE = 0xC4, InsnInfo
    ALOADW = (WIDE[0] + ALOAD[0]), LocalIndexW
    ASTOREW = (WIDE[0] + ASTORE[0]), LocalIndexW
    DLOADW = (WIDE[0] + DLOAD[0]), LocalIndexW
    DSTOREW = (WIDE[0] + DSTORE[0]), LocalIndexW
    FLOADW = (WIDE[0] + FLOAD[0]), LocalIndexW
    FSTOREW = (WIDE[0] + FSTORE[0]), LocalIndexW
    ILOADW = (WIDE[0] + ILOAD[0]), LocalIndexW
    ISTOREW = (WIDE[0] + ISTORE[0]), LocalIndexW
    LLOADW = (WIDE[0] + LLOAD[0]), LocalIndexW
    LSTOREW = (WIDE[0] + LSTORE[0]), LocalIndexW
    RETW = (WIDE[0] + RET[0]), LocalIndexW
    IINCW = (WIDE[0] + IINC[0]), IIncW

    instinfo: type[InsnInfo]

    def __new__(cls, value: int, instinfo: type[InsnInfo]) -> InsnInfoType:
        obj = int.__new__(cls, value)
        obj._value_ = value
        obj.instinfo = instinfo
        return obj


class ArrayType(IntEnum):
    """Primitive array element types for the ``newarray`` instruction (§6.5.newarray).

    Each member's value corresponds to the ``atype`` operand byte.
    """

    BOOLEAN = 4
    CHAR = 5
    FLOAT = 6
    DOUBLE = 7
    BYTE = 8
    SHORT = 9
    INT = 10
    LONG = 11
