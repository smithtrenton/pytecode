# cython: boundscheck=False, wraparound=False, cdivision=True
"""Label-based bytecode instruction editing (Cython-accelerated).

Provides symbolic ``Label`` targets and label-aware instruction types
(``BranchInsn``, ``LookupSwitchInsn``, ``TableSwitchInsn``) so that bytecode
can be manipulated without tracking raw offsets.  ``lower_code`` converts a
label-based ``CodeModel`` into an offset-based ``CodeAttr`` ready for
serialisation, and ``resolve_labels`` computes the offset mapping.
"""

import copy
from typing import TYPE_CHECKING

from ..classfile._instructions_cy cimport (
    Branch as CBranch,
    BranchW as CBranchW,
    ByteValue as CByteValue,
    ConstPoolIndex as CConstPoolIndex,
    IInc as CIInc,
    IIncW as CIIncW,
    InsnInfo as CInsnInfo,
    InvokeDynamic as CInvokeDynamic,
    InvokeInterface as CInvokeInterface,
    LocalIndex as CLocalIndex,
    LocalIndexW as CLocalIndexW,
    LookupSwitch as CLookupSwitch,
    MatchOffsetPair as CMatchOffsetPair,
    MultiANewArray as CMultiANewArray,
    NewArray as CNewArray,
    ShortValue as CShortValue,
    TableSwitch as CTableSwitch,
    _trusted_branch,
    _trusted_branch_w,
    _trusted_byte_value,
    _trusted_const_pool_index,
    _trusted_iinc,
    _trusted_iinc_w,
    _trusted_invoke_dynamic,
    _trusted_invoke_interface,
    _trusted_local_index,
    _trusted_local_index_w,
    _trusted_lookup_switch,
    _trusted_match_offset_pair,
    _trusted_multi_anew_array,
    _trusted_new_array,
    _trusted_raw_insn,
    _trusted_short_value,
    _trusted_table_switch,
)
from ..classfile._attributes_cy cimport (
    AttributeInfo as CAttributeInfo,
    CodeAttr as CCodeAttr,
    ExceptionInfo as CExceptionInfo,
    LineNumberInfo as CLineNumberInfo,
    LineNumberTableAttr as CLineNumberTableAttr,
    LocalVariableInfo as CLocalVariableInfo,
    LocalVariableTableAttr as CLocalVariableTableAttr,
    LocalVariableTypeInfo as CLocalVariableTypeInfo,
    LocalVariableTypeTableAttr as CLocalVariableTypeTableAttr,
)
from ..classfile.attributes import (
    AttributeInfo,
    CodeAttr,
    ExceptionInfo,
    LineNumberInfo,
    LineNumberTableAttr,
    LocalVariableInfo,
    LocalVariableTableAttr,
    LocalVariableTypeInfo,
    LocalVariableTypeTableAttr,
)
from ..classfile.constant_pool import ClassInfo
from ..classfile.descriptors import parameter_slot_count, parse_method_descriptor
from ..classfile.instructions import (
    Branch,
    BranchW,
    ByteValue,
    ConstPoolIndex,
    IInc,
    IIncW,
    InsnInfo,
    InsnInfoType,
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
from ._attribute_clone import clone_attribute, clone_attributes
from ._operands_cy cimport (
    FieldInsn as CFieldInsn,
    IIncInsn as COperandIIncInsn,
    InterfaceMethodInsn as CInterfaceMethodInsn,
    InvokeDynamicInsn as COperandInvokeDynamicInsn,
    LdcClass as CLdcClass,
    LdcDouble as CLdcDouble,
    LdcDynamic as CLdcDynamic,
    LdcFloat as CLdcFloat,
    LdcInsn as CLdcInsn,
    LdcInt as CLdcInt,
    LdcLong as CLdcLong,
    LdcMethodHandle as CLdcMethodHandle,
    LdcMethodType as CLdcMethodType,
    LdcString as CLdcString,
    MethodInsn as CMethodInsn,
    MultiANewArrayInsn as COperandMultiANewArrayInsn,
    TypeInsn as CTypeInsn,
    VarInsn as COperandVarInsn,
)
from .constant_pool_builder import ConstantPoolBuilder
from .debug_info import DebugInfoPolicy, is_code_debug_info_stale, normalize_debug_info_policy
from .operands import (
    _BASE_TO_WIDE,
    _VAR_SHORTCUTS,
    FieldInsn,
    IIncInsn,
    InterfaceMethodInsn,
    InvokeDynamicInsn,
    LdcClass,
    LdcDouble,
    LdcFloat,
    LdcInsn,
    LdcInt,
    LdcLong,
    LdcMethodHandle,
    LdcMethodType,
    LdcString,
    LdcValue,
    MethodInsn,
    MultiANewArrayInsn,
    TypeInsn,
    VarInsn,
    _require_i2,
    _require_u1,
    _require_u2,
)

if TYPE_CHECKING:
    from ..analysis.hierarchy import ClassResolver
    from .model import CodeModel, MethodModel


_BRANCH_TARGET_CONTEXTS = {
    insn_type: f"{insn_type.name} target"
    for insn_type in InsnInfoType
    if insn_type.instinfo in (Branch, BranchW)
}


cdef inline void _init_attribute_info(CAttributeInfo attr, int attribute_name_index, int attribute_length):
    attr.attribute_name_index = attribute_name_index
    attr.attribute_length = attribute_length


cdef inline object _trusted_exception_info(
    int start_pc,
    int end_pc,
    int handler_pc,
    int catch_type,
):
    cdef CExceptionInfo info = ExceptionInfo.__new__(ExceptionInfo)
    info.start_pc = start_pc
    info.end_pc = end_pc
    info.handler_pc = handler_pc
    info.catch_type = catch_type
    return info


cdef inline object _trusted_line_number_info(int start_pc, int line_number):
    cdef CLineNumberInfo info = LineNumberInfo.__new__(LineNumberInfo)
    info.start_pc = start_pc
    info.line_number = line_number
    return info


cdef inline object _trusted_line_number_table_attr(int attribute_name_index, list table):
    cdef CLineNumberTableAttr attr = LineNumberTableAttr.__new__(LineNumberTableAttr)
    _init_attribute_info(attr, attribute_name_index, 2 + (4 * len(table)))
    attr.line_number_table_length = len(table)
    attr.line_number_table = table
    return attr


cdef inline object _trusted_local_variable_info(
    int start_pc,
    int length,
    int name_index,
    int descriptor_index,
    int index,
):
    cdef CLocalVariableInfo info = LocalVariableInfo.__new__(LocalVariableInfo)
    info.start_pc = start_pc
    info.length = length
    info.name_index = name_index
    info.descriptor_index = descriptor_index
    info.index = index
    return info


cdef inline object _trusted_local_variable_table_attr(int attribute_name_index, list table):
    cdef CLocalVariableTableAttr attr = LocalVariableTableAttr.__new__(LocalVariableTableAttr)
    _init_attribute_info(attr, attribute_name_index, 2 + (10 * len(table)))
    attr.local_variable_table_length = len(table)
    attr.local_variable_table = table
    return attr


cdef inline object _trusted_local_variable_type_info(
    int start_pc,
    int length,
    int name_index,
    int signature_index,
    int index,
):
    cdef CLocalVariableTypeInfo info = LocalVariableTypeInfo.__new__(LocalVariableTypeInfo)
    info.start_pc = start_pc
    info.length = length
    info.name_index = name_index
    info.signature_index = signature_index
    info.index = index
    return info


cdef inline object _trusted_local_variable_type_table_attr(int attribute_name_index, list table):
    cdef CLocalVariableTypeTableAttr attr = LocalVariableTypeTableAttr.__new__(LocalVariableTypeTableAttr)
    _init_attribute_info(attr, attribute_name_index, 2 + (10 * len(table)))
    attr.local_variable_type_table_length = len(table)
    attr.local_variable_type_table = table
    return attr


cdef inline object _trusted_code_attr(
    int attribute_name_index,
    int attribute_length,
    int max_stack,
    int max_locals,
    int code_length,
    list code,
    list exception_table,
    list attributes,
):
    cdef CCodeAttr attr = CodeAttr.__new__(CodeAttr)
    _init_attribute_info(attr, attribute_name_index, attribute_length)
    attr.max_stacks = max_stack
    attr.max_locals = max_locals
    attr.code_length = code_length
    attr.code = code
    attr.exception_table_length = len(exception_table)
    attr.exception_table = exception_table
    attr.attributes_count = len(attributes)
    attr.attributes = attributes
    return attr


cdef object _clone_raw_instruction(object insn, object bytecode_offset):
    cdef CLookupSwitch lookup_switch
    cdef CTableSwitch table_switch
    cdef CInsnInfo no_operand_insn
    cdef CLocalIndex local_index
    cdef CLocalIndexW local_index_w
    cdef CConstPoolIndex const_pool_index
    cdef CByteValue byte_value
    cdef CShortValue short_value
    cdef CBranch branch
    cdef CBranchW branch_w
    cdef CIInc iinc
    cdef CIIncW iinc_w
    cdef CInvokeDynamic invoke_dynamic
    cdef CInvokeInterface invoke_interface
    cdef CNewArray new_array
    cdef CMultiANewArray multi_anew_array
    cdef CMatchOffsetPair pair
    offset = insn.bytecode_offset if bytecode_offset is None else bytecode_offset

    insn_type = type(insn)
    if insn_type is LookupSwitch:
        lookup_switch = insn
        return _trusted_lookup_switch(
            lookup_switch.type,
            offset,
            lookup_switch.default,
            lookup_switch.npairs,
            [_trusted_match_offset_pair(pair.match, pair.offset) for pair in lookup_switch.pairs],
        )
    if insn_type is TableSwitch:
        table_switch = insn
        return _trusted_table_switch(
            table_switch.type,
            offset,
            table_switch.default,
            table_switch.low,
            table_switch.high,
            list(table_switch.offsets),
        )
    if insn_type is InsnInfo:
        no_operand_insn = insn
        return _trusted_raw_insn(no_operand_insn.type, offset)
    if insn_type is LocalIndex:
        local_index = insn
        return _trusted_local_index(local_index.type, offset, local_index.index)
    if insn_type is LocalIndexW:
        local_index_w = insn
        return _trusted_local_index_w(local_index_w.type, offset, local_index_w.index)
    if insn_type is ConstPoolIndex:
        const_pool_index = insn
        return _trusted_const_pool_index(const_pool_index.type, offset, const_pool_index.index)
    if insn_type is ByteValue:
        byte_value = insn
        return _trusted_byte_value(byte_value.type, offset, byte_value.value)
    if insn_type is ShortValue:
        short_value = insn
        return _trusted_short_value(short_value.type, offset, short_value.value)
    if insn_type is Branch:
        branch = insn
        return _trusted_branch(branch.type, offset, branch.offset)
    if insn_type is BranchW:
        branch_w = insn
        return _trusted_branch_w(branch_w.type, offset, branch_w.offset)
    if insn_type is IInc:
        iinc = insn
        return _trusted_iinc(iinc.type, offset, iinc.index, iinc.value)
    if insn_type is IIncW:
        iinc_w = insn
        return _trusted_iinc_w(iinc_w.type, offset, iinc_w.index, iinc_w.value)
    if insn_type is InvokeDynamic:
        invoke_dynamic = insn
        return _trusted_invoke_dynamic(
            invoke_dynamic.type,
            offset,
            invoke_dynamic.index,
            invoke_dynamic.unused,
        )
    if insn_type is InvokeInterface:
        invoke_interface = insn
        return _trusted_invoke_interface(
            invoke_interface.type,
            offset,
            invoke_interface.index,
            invoke_interface.count,
            invoke_interface.unused,
        )
    if insn_type is NewArray:
        new_array = insn
        return _trusted_new_array(new_array.type, offset, new_array.atype)
    if insn_type is MultiANewArray:
        multi_anew_array = insn
        return _trusted_multi_anew_array(
            multi_anew_array.type,
            offset,
            multi_anew_array.index,
            multi_anew_array.dimensions,
        )
    lowered = copy.copy(insn)
    lowered.bytecode_offset = offset
    return lowered


__all__ = [
    "BranchInsn",
    "ExceptionHandler",
    "Label",
    "LabelResolution",
    "LineNumberEntry",
    "LocalVariableEntry",
    "LocalVariableTypeEntry",
    "LookupSwitchInsn",
    "TableSwitchInsn",
    "lower_code",
    "resolve_catch_type",
    "resolve_labels",
]


def clone_raw_instruction(insn, *, bytecode_offset=None):
    """Clone a raw JVM instruction without using ``copy.deepcopy``."""
    return _clone_raw_instruction(insn, bytecode_offset)


_I2_MIN = -(1 << 15)
_I2_MAX = (1 << 15) - 1
_I4_MIN = -(1 << 31)
_I4_MAX = (1 << 31) - 1

_BRANCH_WIDENINGS: dict = {
    InsnInfoType.GOTO: InsnInfoType.GOTO_W,
    InsnInfoType.JSR: InsnInfoType.JSR_W,
}

_INVERTED_CONDITIONAL_BRANCHES: dict = {
    InsnInfoType.IFEQ: InsnInfoType.IFNE,
    InsnInfoType.IFNE: InsnInfoType.IFEQ,
    InsnInfoType.IFLT: InsnInfoType.IFGE,
    InsnInfoType.IFGE: InsnInfoType.IFLT,
    InsnInfoType.IFGT: InsnInfoType.IFLE,
    InsnInfoType.IFLE: InsnInfoType.IFGT,
    InsnInfoType.IF_ICMPEQ: InsnInfoType.IF_ICMPNE,
    InsnInfoType.IF_ICMPNE: InsnInfoType.IF_ICMPEQ,
    InsnInfoType.IF_ICMPLT: InsnInfoType.IF_ICMPGE,
    InsnInfoType.IF_ICMPGE: InsnInfoType.IF_ICMPLT,
    InsnInfoType.IF_ICMPGT: InsnInfoType.IF_ICMPLE,
    InsnInfoType.IF_ICMPLE: InsnInfoType.IF_ICMPGT,
    InsnInfoType.IF_ACMPEQ: InsnInfoType.IF_ACMPNE,
    InsnInfoType.IF_ACMPNE: InsnInfoType.IF_ACMPEQ,
    InsnInfoType.IFNULL: InsnInfoType.IFNONNULL,
    InsnInfoType.IFNONNULL: InsnInfoType.IFNULL,
}


def _repr_fields(str class_name, tuple fields):
    return f"{class_name}(" + ", ".join(f"{name}={value!r}" for name, value in fields) + ")"


cdef class _ValueBase:
    def _field_values(self):
        raise NotImplementedError

    def _field_items(self):
        raise NotImplementedError

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


cdef class _SymbolicInsnBase(CInsnInfo):
    def _init_values(self):
        raise NotImplementedError

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
        return type(self)(*self._init_values())

    def __deepcopy__(self, memo):
        return type(self)(*copy.deepcopy(self._init_values(), memo))

    def __reduce__(self):
        return type(self), self._init_values()


cdef class Label:
    """Identity-based marker for a bytecode position.

    Labels use identity equality so that distinct instances targeting the same
    logical position remain distinguishable.

    Attributes:
        name: Optional human-readable name for debugging output.
    """

    def __init__(self, object name=None):
        self.name = name

    def __repr__(self):
        if self.name is not None:
            return f"Label({self.name!r})"
        return f"Label(id=0x{id(self):x})"

    def __copy__(self):
        return type(self)(self.name)

    def __deepcopy__(self, memo):
        return type(self)(copy.deepcopy(self.name, memo))

    def __reduce__(self):
        return type(self), (self.name,)


cdef class ExceptionHandler(_ValueBase):
    """An exception handler entry that uses labels for range and target.

    Attributes:
        start: Label marking the beginning of the protected region (inclusive).
        end: Label marking the end of the protected region (exclusive).
        handler: Label marking the entry point of the handler code.
        catch_type: Internal name of the caught exception class, or ``None``
            for a catch-all (``finally``) handler.
    """

    def __init__(self, object start, object end, object handler, object catch_type=None):
        self.start = start
        self.end = end
        self.handler = handler
        self.catch_type = catch_type

    def _field_values(self):
        return (self.start, self.end, self.handler, self.catch_type)

    def _field_items(self):
        return (
            ("start", self.start),
            ("end", self.end),
            ("handler", self.handler),
            ("catch_type", self.catch_type),
        )


cdef class LineNumberEntry(_ValueBase):
    """Maps a label position to a source line number.

    Attributes:
        label: Label marking the bytecode position.
        line_number: Corresponding source-file line number.
    """

    def __init__(self, object label, Py_ssize_t line_number):
        self.label = label
        self.line_number = line_number

    def _field_values(self):
        return (self.label, self.line_number)

    def _field_items(self):
        return (("label", self.label), ("line_number", self.line_number))


cdef class LocalVariableEntry(_ValueBase):
    """A local variable debug entry using labels for the live range.

    Attributes:
        start: Label marking the start of the variable's scope (inclusive).
        end: Label marking the end of the variable's scope (exclusive).
        name: Variable name as it appears in source.
        descriptor: JVM field descriptor of the variable's type.
        slot: Local variable table slot index.
    """

    def __init__(self, object start, object end, object name, object descriptor, Py_ssize_t slot):
        self.start = start
        self.end = end
        self.name = name
        self.descriptor = descriptor
        self.slot = slot

    def _field_values(self):
        return (self.start, self.end, self.name, self.descriptor, self.slot)

    def _field_items(self):
        return (
            ("start", self.start),
            ("end", self.end),
            ("name", self.name),
            ("descriptor", self.descriptor),
            ("slot", self.slot),
        )


cdef class LocalVariableTypeEntry(_ValueBase):
    """A local variable type debug entry using labels for the live range.

    Similar to ``LocalVariableEntry`` but carries a generic signature instead
    of a plain descriptor.

    Attributes:
        start: Label marking the start of the variable's scope (inclusive).
        end: Label marking the end of the variable's scope (exclusive).
        name: Variable name as it appears in source.
        signature: Generic signature of the variable's type.
        slot: Local variable table slot index.
    """

    def __init__(self, object start, object end, object name, object signature, Py_ssize_t slot):
        self.start = start
        self.end = end
        self.name = name
        self.signature = signature
        self.slot = slot

    def _field_values(self):
        return (self.start, self.end, self.name, self.signature, self.slot)

    def _field_items(self):
        return (
            ("start", self.start),
            ("end", self.end),
            ("name", self.name),
            ("signature", self.signature),
            ("slot", self.slot),
        )


cdef class BranchInsn(_SymbolicInsnBase):
    """A branch instruction that targets a label instead of a raw offset.

    Supports both narrow (2-byte offset) and wide (4-byte offset) branch
    opcodes.  During lowering, narrow branches that overflow are automatically
    widened or inverted as needed.

    Attributes:
        target: The ``Label`` this branch jumps to.
    """

    def __init__(self, object insn_type, object target, Py_ssize_t bytecode_offset=-1):
        if insn_type.instinfo not in {Branch, BranchW}:
            raise ValueError(f"{insn_type.name} is not a branch opcode")
        CInsnInfo.__init__(self, insn_type, bytecode_offset)
        self.target = target

    @classmethod
    def _trusted(cls, object insn_type, object target, Py_ssize_t bytecode_offset=-1):
        self = cls.__new__(cls)
        CInsnInfo.__init__(self, insn_type, bytecode_offset)
        self.target = target
        return self

    def _init_values(self):
        return (self.type, self.target, self.bytecode_offset)

    def _field_values(self):
        return (self.type, self.bytecode_offset, self.target)

    def _field_items(self):
        return _SymbolicInsnBase._field_items(self) + (("target", self.target),)


cdef class LookupSwitchInsn(_SymbolicInsnBase):
    """A ``lookupswitch`` instruction that uses labels for jump targets.

    Attributes:
        default_target: Label for the default branch.
        pairs: Match-value / label pairs for each case.
    """

    def __init__(self, object default_target, object pairs, Py_ssize_t bytecode_offset=-1):
        CInsnInfo.__init__(self, InsnInfoType.LOOKUPSWITCH, bytecode_offset)
        self.default_target = default_target
        self.pairs = list(pairs)

    @classmethod
    def _trusted(cls, object default_target, object pairs, Py_ssize_t bytecode_offset=-1):
        self = cls.__new__(cls)
        CInsnInfo.__init__(self, InsnInfoType.LOOKUPSWITCH, bytecode_offset)
        self.default_target = default_target
        self.pairs = list(pairs)
        return self

    def _init_values(self):
        return (self.default_target, self.pairs, self.bytecode_offset)

    def _field_values(self):
        return (self.type, self.bytecode_offset, self.default_target, self.pairs)

    def _field_items(self):
        return _SymbolicInsnBase._field_items(self) + (
            ("default_target", self.default_target),
            ("pairs", self.pairs),
        )

    def __copy__(self):
        copied = type(self).__new__(type(self))
        CInsnInfo.__init__(copied, InsnInfoType.LOOKUPSWITCH, self.bytecode_offset)
        copied.default_target = self.default_target
        copied.pairs = self.pairs
        return copied


cdef class TableSwitchInsn(_SymbolicInsnBase):
    """A ``tableswitch`` instruction that uses labels for jump targets.

    Attributes:
        default_target: Label for the default branch.
        low: Minimum match value (inclusive).
        high: Maximum match value (inclusive).
        targets: Labels for each case in the ``low..high`` range.
    """

    def __init__(
        self,
        object default_target,
        Py_ssize_t low,
        Py_ssize_t high,
        object targets,
        Py_ssize_t bytecode_offset=-1,
    ):
        if high < low:
            raise ValueError("tableswitch high must be >= low")
        expected_targets = high - low + 1
        if len(targets) != expected_targets:
            raise ValueError(f"tableswitch range {low}..{high} requires {expected_targets} targets, got {len(targets)}")
        CInsnInfo.__init__(self, InsnInfoType.TABLESWITCH, bytecode_offset)
        self.default_target = default_target
        self.low = low
        self.high = high
        self.targets = list(targets)

    @classmethod
    def _trusted(
        cls,
        object default_target,
        Py_ssize_t low,
        Py_ssize_t high,
        object targets,
        Py_ssize_t bytecode_offset=-1,
    ):
        self = cls.__new__(cls)
        CInsnInfo.__init__(self, InsnInfoType.TABLESWITCH, bytecode_offset)
        self.default_target = default_target
        self.low = low
        self.high = high
        self.targets = list(targets)
        return self

    def _init_values(self):
        return (self.default_target, self.low, self.high, self.targets, self.bytecode_offset)

    def _field_values(self):
        return (self.type, self.bytecode_offset, self.default_target, self.low, self.high, self.targets)

    def _field_items(self):
        return _SymbolicInsnBase._field_items(self) + (
            ("default_target", self.default_target),
            ("low", self.low),
            ("high", self.high),
            ("targets", self.targets),
        )

    def __copy__(self):
        copied = type(self).__new__(type(self))
        CInsnInfo.__init__(copied, InsnInfoType.TABLESWITCH, self.bytecode_offset)
        copied.default_target = self.default_target
        copied.low = self.low
        copied.high = self.high
        copied.targets = self.targets
        return copied


cdef class LabelResolution(_ValueBase):
    """Result of resolving labels to bytecode offsets.

    Attributes:
        label_offsets: Mapping from each ``Label`` to its resolved bytecode offset.
        instruction_offsets: Bytecode offset of each item in the instruction list.
        total_code_length: Total byte length of the lowered bytecode.
    """

    def __init__(self, dict label_offsets, list instruction_offsets, Py_ssize_t total_code_length):
        self.label_offsets = label_offsets
        self.instruction_offsets = instruction_offsets
        self.total_code_length = total_code_length

    def _field_values(self):
        return (self.label_offsets, self.instruction_offsets, self.total_code_length)

    def _field_items(self):
        return (
            ("label_offsets", self.label_offsets),
            ("instruction_offsets", self.instruction_offsets),
            ("total_code_length", self.total_code_length),
        )


cdef int _switch_padding(int offset):
    return (4 - ((offset + 1) % 4)) % 4


cdef inline bint _fits_i2(int value):
    return _I2_MIN <= value <= _I2_MAX


cdef inline bint _fits_i4(int value):
    return _I4_MIN <= value <= _I4_MAX


cdef inline int _require_label_offset(dict label_offsets, object label, str context):
    cdef object result = label_offsets.get(label)
    if result is None and label not in label_offsets:
        raise ValueError(f"{context} refers to a label that is not present in CodeModel.instructions")
    return <int>result


cdef inline int _relative_offset(int source_offset, object label, dict label_offsets, str context):
    return _require_label_offset(label_offsets, label, context) - source_offset


def _attribute_marshaled_size(attribute):
    return 6 + attribute.attribute_length


def _code_attribute_length(code_length, exception_table_length, attributes):
    nested_size = sum(_attribute_marshaled_size(attribute) for attribute in attributes)
    return 12 + code_length + (8 * exception_table_length) + nested_size


def _refresh_code_attr_metadata(code_attr):
    code_attr.attributes_count = len(code_attr.attributes)
    code_attr.attribute_length = _code_attribute_length(
        code_attr.code_length,
        code_attr.exception_table_length,
        code_attr.attributes,
    )


def _lifted_debug_attrs(attributes):
    return [
        attribute
        for attribute in attributes
        if not isinstance(
            attribute,
            (LineNumberTableAttr, LocalVariableTableAttr, LocalVariableTypeTableAttr),
        )
    ]


cdef object _ordered_nested_code_attributes(
    object code,
    object line_number_attr,
    object local_variable_attr,
    object local_variable_type_attr,
):
    cdef list other_attrs
    cdef list attrs
    cdef object attribute, token, debug_attr
    cdef type attr_type
    cdef int other_index
    cdef bint has_debug_attrs = False
    cdef object pending_line_numbers = line_number_attr
    cdef object pending_local_variables = local_variable_attr
    cdef object pending_local_variable_types = local_variable_type_attr

    for attribute in code.attributes:
        attr_type = type(attribute)
        if (
            attr_type is LineNumberTableAttr
            or attr_type is LocalVariableTableAttr
            or attr_type is LocalVariableTypeTableAttr
        ):
            has_debug_attrs = True
            break
    if has_debug_attrs:
        other_attrs = []
        for attribute in code.attributes:
            attr_type = type(attribute)
            if (
                attr_type is LineNumberTableAttr
                or attr_type is LocalVariableTableAttr
                or attr_type is LocalVariableTypeTableAttr
            ):
                continue
            other_attrs.append(clone_attribute(attribute))
    else:
        other_attrs = clone_attributes(code.attributes)
    if not code._nested_attribute_layout:
        attrs = other_attrs
        for debug_attr in (line_number_attr, local_variable_attr, local_variable_type_attr):
            if debug_attr is not None:
                attrs.append(debug_attr)
        return attrs

    attrs = []
    other_index = 0
    for token in code._nested_attribute_layout:
        if token == "other":
            if other_index < len(other_attrs):
                attrs.append(other_attrs[other_index])
                other_index += 1
            continue

        if token == "line_numbers":
            if pending_line_numbers is not None:
                attrs.append(pending_line_numbers)
                pending_line_numbers = None
            continue
        if token == "local_variables":
            if pending_local_variables is not None:
                attrs.append(pending_local_variables)
                pending_local_variables = None
            continue
        if token == "local_variable_types":
            if pending_local_variable_types is not None:
                attrs.append(pending_local_variable_types)
                pending_local_variable_types = None

    attrs.extend(other_attrs[other_index:])
    if pending_line_numbers is not None:
        attrs.append(pending_line_numbers)
    if pending_local_variables is not None:
        attrs.append(pending_local_variables)
    if pending_local_variable_types is not None:
        attrs.append(pending_local_variable_types)

    return attrs


cdef bint _needs_ldc_index_cache(list items):
    cdef object item, value
    cdef CLdcInsn ldc_insn
    cdef type value_type
    for item in items:
        if type(item) is not LdcInsn:
            continue
        ldc_insn = item
        value = ldc_insn.value
        value_type = type(value)
        if value_type is not LdcLong and value_type is not LdcDouble:
            return True
    return False


def _build_ldc_index_cache(items, cp):
    cache = {}
    checkpoint = None

    for item in items:
        if type(item) is not LdcInsn:
            continue

        cached = _find_existing_ldc_index(item.value, cp)
        if cached is not None:
            cache[id(item)] = cached
            continue

        if checkpoint is None:
            checkpoint = cp.checkpoint()
        cache[id(item)] = _lower_ldc_value(item.value, cp)

    if checkpoint is not None:
        cp.rollback(checkpoint)

    return cache


def _resolve_labels_with_cache(list items, dict ldc_index_cache=None):
    cdef int offset
    cdef Py_ssize_t index, item_count
    cdef type item_type
    cdef object item
    cdef Label label
    cdef dict label_offsets = {}
    cdef list instruction_offsets
    offset = 0
    item_count = len(items)
    instruction_offsets = [0] * item_count

    for index in range(item_count):
        item = items[index]
        instruction_offsets[index] = offset
        item_type = type(item)
        if item_type is Label:
            label = item
            if label in label_offsets:
                raise ValueError(f"label {label!r} appears multiple times in CodeModel.instructions")
            label_offsets[label] = offset
            continue
        if item_type is InsnInfo:
            offset += 1
            continue
        offset += _instruction_byte_size(item, item_type, offset, ldc_index_cache)

    return LabelResolution(
        label_offsets=label_offsets,
        instruction_offsets=instruction_offsets,
        total_code_length=offset,
    )


cdef int _instruction_byte_size(object insn, type insn_type, int offset, dict ldc_index_cache):
    cdef int slot, increment
    cdef object shortcut
    cdef CLookupSwitch lookup_switch
    cdef CTableSwitch table_switch
    cdef COperandVarInsn var_insn
    cdef CLdcInsn ldc_insn
    cdef COperandIIncInsn iinc_insn
    if insn_type is Label:
        return 0
    if insn_type is InsnInfo:
        return 1
    if insn_type is VarInsn:
        var_insn = insn
        slot = _require_u2(var_insn.slot, context="local variable slot")
        shortcut = _var_shortcut_opcode(var_insn.type, slot)
        if shortcut is not None:
            return 1  # implicit form (e.g. ILOAD_0)
        if slot <= 255:
            return 2  # opcode(1) + u1 slot
        return 4  # WIDE(1) + opcode(1) + u2 slot
    if insn_type is BranchInsn:
        branch_insn = insn
        return 5 if branch_insn.type.instinfo is BranchW else 3
    if insn_type is LookupSwitchInsn:
        lookup_switch_insn = insn
        return 1 + _switch_padding(offset) + 8 + (8 * len(lookup_switch_insn.pairs))
    if insn_type is TableSwitchInsn:
        table_switch_insn = insn
        return 1 + _switch_padding(offset) + 12 + (4 * len(table_switch_insn.targets))
    # Symbolic operand wrappers (operands.py)
    if insn_type in (FieldInsn, MethodInsn, TypeInsn):
        return 3  # opcode(1) + u2 CP index
    if insn_type is InterfaceMethodInsn:
        return 5  # opcode(1) + u2 CP index + u1 count + u1 zero
    if insn_type is InvokeDynamicInsn:
        return 5  # opcode(1) + u2 CP index + u2 zero
    if insn_type is MultiANewArrayInsn:
        return 4  # opcode(1) + u2 CP index + u1 dimensions
    if insn_type is LdcInsn:
        ldc_insn = insn
        ldc_value_type = type(ldc_insn.value)
        if ldc_value_type is LdcLong or ldc_value_type is LdcDouble:
            return 3  # LDC2_W: opcode(1) + u2 CP index
        if ldc_index_cache is None:
            raise ValueError("LdcInsn size requires constant-pool context")
        idx = ldc_index_cache.get(id(ldc_insn))
        if idx is None:
            raise ValueError("LdcInsn is missing from the LDC index cache")
        return 2 if idx <= 255 else 3  # LDC: 2, LDC_W: 3
    if insn_type is IIncInsn:
        iinc_insn = insn
        slot = _require_u2(iinc_insn.slot, context="local variable slot")
        increment = _require_i2(iinc_insn.increment, context="iinc increment")
        if slot <= 255 and -128 <= increment <= 127:
            return 3  # IINC(1) + u1 slot + i1 increment
        return 6  # WIDE(1) + IINC(1) + u2 slot + i2 increment
    # Raw spec-model types
    if insn_type is LocalIndex:
        return 2
    if insn_type is LocalIndexW:
        return 4
    if insn_type is ConstPoolIndex:
        return 3
    if insn_type is ByteValue:
        return 2
    if insn_type is ShortValue:
        return 3
    if insn_type is Branch:
        return 3
    if insn_type is BranchW:
        return 5
    if insn_type is IInc:
        return 3
    if insn_type is IIncW:
        return 6
    if insn_type is InvokeDynamic:
        return 5
    if insn_type is InvokeInterface:
        return 5
    if insn_type is NewArray:
        return 2
    if insn_type is MultiANewArray:
        return 4
    if insn_type is LookupSwitch:
        lookup_switch = insn
        return 1 + _switch_padding(offset) + 8 + (8 * len(lookup_switch.pairs))
    if insn_type is TableSwitch:
        table_switch = insn
        return 1 + _switch_padding(offset) + 12 + (4 * len(table_switch.offsets))
    return 1


def resolve_labels(items, cp=None):
    """Resolve label and instruction offsets for a mixed instruction stream.

    Args:
        items: Instruction stream containing ``InsnInfo`` and ``Label`` items.
        cp: Constant-pool builder, required when the stream contains
            single-slot ``LdcInsn`` values so their byte size can be
            determined without mutating the live pool.

    Returns:
        A ``LabelResolution`` with the computed offsets and total code length.

    Raises:
        ValueError: If a label appears more than once, or if a
            ``ConstantPoolBuilder`` is needed but not provided.
    """

    ldc_index_cache = None
    if _needs_ldc_index_cache(items):
        if cp is None:
            raise ValueError(
                "resolve_labels() requires a ConstantPoolBuilder when instructions contain single-slot LdcInsn values"
            )
        ldc_index_cache = _build_ldc_index_cache(items, cp)

    return _resolve_labels_with_cache(items, ldc_index_cache)


cdef inline object _var_shortcut_opcode(object insn_type, int slot):
    if slot < 0 or slot > 3:
        return None
    if insn_type is InsnInfoType.ILOAD:
        return (InsnInfoType.ILOAD_0, InsnInfoType.ILOAD_1, InsnInfoType.ILOAD_2, InsnInfoType.ILOAD_3)[slot]
    if insn_type is InsnInfoType.ALOAD:
        return (InsnInfoType.ALOAD_0, InsnInfoType.ALOAD_1, InsnInfoType.ALOAD_2, InsnInfoType.ALOAD_3)[slot]
    if insn_type is InsnInfoType.ISTORE:
        return (InsnInfoType.ISTORE_0, InsnInfoType.ISTORE_1, InsnInfoType.ISTORE_2, InsnInfoType.ISTORE_3)[slot]
    if insn_type is InsnInfoType.ASTORE:
        return (InsnInfoType.ASTORE_0, InsnInfoType.ASTORE_1, InsnInfoType.ASTORE_2, InsnInfoType.ASTORE_3)[slot]
    if insn_type is InsnInfoType.LLOAD:
        return (InsnInfoType.LLOAD_0, InsnInfoType.LLOAD_1, InsnInfoType.LLOAD_2, InsnInfoType.LLOAD_3)[slot]
    if insn_type is InsnInfoType.FLOAD:
        return (InsnInfoType.FLOAD_0, InsnInfoType.FLOAD_1, InsnInfoType.FLOAD_2, InsnInfoType.FLOAD_3)[slot]
    if insn_type is InsnInfoType.LSTORE:
        return (InsnInfoType.LSTORE_0, InsnInfoType.LSTORE_1, InsnInfoType.LSTORE_2, InsnInfoType.LSTORE_3)[slot]
    if insn_type is InsnInfoType.FSTORE:
        return (InsnInfoType.FSTORE_0, InsnInfoType.FSTORE_1, InsnInfoType.FSTORE_2, InsnInfoType.FSTORE_3)[slot]
    if insn_type is InsnInfoType.DLOAD:
        return (InsnInfoType.DLOAD_0, InsnInfoType.DLOAD_1, InsnInfoType.DLOAD_2, InsnInfoType.DLOAD_3)[slot]
    if insn_type is InsnInfoType.DSTORE:
        return (InsnInfoType.DSTORE_0, InsnInfoType.DSTORE_1, InsnInfoType.DSTORE_2, InsnInfoType.DSTORE_3)[slot]
    return None


cdef bint _promote_overflow_branches(list items, LabelResolution resolution):
    cdef int index, source_offset, relative
    cdef bint changed = False
    cdef BranchInsn branch_item
    cdef object item
    cdef object widened, inverted, skip_label
    index = 0

    while index < len(items):
        item = items[index]
        if type(item) is not BranchInsn:
            index += 1
            continue

        source_offset = resolution.instruction_offsets[index]
        branch_item = item
        relative = _relative_offset(
            source_offset,
            branch_item.target,
            resolution.label_offsets,
            _BRANCH_TARGET_CONTEXTS[branch_item.type],
        )

        if branch_item.type.instinfo is BranchW:
            if not _fits_i4(relative):
                raise ValueError(f"{branch_item.type.name} branch offset {relative} exceeds JVM i4 range")
            index += 1
            continue

        if _fits_i2(relative):
            index += 1
            continue

        widened = _BRANCH_WIDENINGS.get(branch_item.type)
        if widened is not None:
            items[index] = BranchInsn(widened, branch_item.target)
            changed = True
            index += 1
            continue

        inverted = _INVERTED_CONDITIONAL_BRANCHES.get(branch_item.type)
        if inverted is None:
            raise ValueError(f"{branch_item.type.name} cannot be widened automatically")

        skip_label = Label(f"{branch_item.type.name.lower()}_skip")
        items[index : index + 1] = [
            BranchInsn(inverted, skip_label),
            BranchInsn(InsnInfoType.GOTO_W, branch_item.target),
            skip_label,
        ]
        changed = True
        index += 3

    return changed


cdef object _lower_instruction(object item, int offset, dict label_offsets, object cp):
    cdef int cp_index, slot, increment, count, dimensions, bootstrap_method_attr_index, relative
    cdef object lowered, shortcut
    cdef CInsnInfo no_operand_insn
    cdef BranchInsn branch_item
    cdef LookupSwitchInsn lookup_switch_item
    cdef TableSwitchInsn table_switch_item
    cdef CFieldInsn field_item
    cdef CMethodInsn method_item
    cdef CInterfaceMethodInsn interface_method_item
    cdef CTypeInsn type_item
    cdef CLdcInsn ldc_item
    cdef COperandIIncInsn iinc_item
    cdef COperandInvokeDynamicInsn invoke_dynamic_item
    cdef COperandMultiANewArrayInsn multi_anew_array_item
    item_type = type(item)
    if item_type is Label:
        return None
    if item_type is InsnInfo:
        no_operand_insn = item
        return _trusted_raw_insn(no_operand_insn.type, offset)
    if item_type is VarInsn:
        var_item = item
        slot = _require_u2(var_item.slot, context="local variable slot")
        shortcut = _var_shortcut_opcode(var_item.type, slot)
        if shortcut is not None:
            return _trusted_raw_insn(shortcut, offset)
        if slot > 255:
            wide_type = _BASE_TO_WIDE[var_item.type]
            return _trusted_local_index_w(wide_type, offset, slot)
        return _trusted_local_index(var_item.type, offset, slot)

    if item_type is BranchInsn:
        branch_item = item
        relative = _relative_offset(
            offset,
            branch_item.target,
            label_offsets,
            _BRANCH_TARGET_CONTEXTS[branch_item.type],
        )
        if branch_item.type.instinfo is BranchW:
            if not _fits_i4(relative):
                raise ValueError(f"{branch_item.type.name} branch offset {relative} exceeds JVM i4 range")
            return _trusted_branch_w(branch_item.type, offset, relative)
        if not _fits_i2(relative):
            raise ValueError(f"{branch_item.type.name} branch offset {relative} exceeds JVM i2 range")
        return _trusted_branch(branch_item.type, offset, relative)

    if item_type is LookupSwitchInsn:
        lookup_switch_item = item
        default = _relative_offset(
            offset,
            lookup_switch_item.default_target,
            label_offsets,
            "lookupswitch default target",
        )
        pairs = [
            _trusted_match_offset_pair(
                match,
                _relative_offset(offset, target, label_offsets, "lookupswitch case target"),
            )
            for match, target in lookup_switch_item.pairs
        ]
        return _trusted_lookup_switch(lookup_switch_item.type, offset, default, len(pairs), pairs)

    if item_type is TableSwitchInsn:
        table_switch_item = item
        default = _relative_offset(
            offset,
            table_switch_item.default_target,
            label_offsets,
            "tableswitch default target",
        )
        offsets = [
            _relative_offset(offset, target, label_offsets, "tableswitch case target")
            for target in table_switch_item.targets
        ]
        return _trusted_table_switch(
            table_switch_item.type,
            offset,
            default,
            table_switch_item.low,
            table_switch_item.high,
            offsets,
        )

    # Symbolic operand wrappers from operands.py
    if item_type is FieldInsn:
        field_item = item
        cp_index = cp.add_fieldref(field_item.owner, field_item.name, field_item.descriptor)
        return _trusted_const_pool_index(field_item.type, offset, cp_index)

    if item_type is MethodInsn:
        method_item = item
        if method_item.is_interface:
            cp_index = cp.add_interface_methodref(method_item.owner, method_item.name, method_item.descriptor)
        else:
            cp_index = cp.add_methodref(method_item.owner, method_item.name, method_item.descriptor)
        return _trusted_const_pool_index(method_item.type, offset, cp_index)

    if item_type is InterfaceMethodInsn:
        interface_method_item = item
        cp_index = cp.add_interface_methodref(
            interface_method_item.owner,
            interface_method_item.name,
            interface_method_item.descriptor,
        )
        count = parameter_slot_count(parse_method_descriptor(interface_method_item.descriptor)) + 1
        return _trusted_invoke_interface(InsnInfoType.INVOKEINTERFACE, offset, cp_index, count, b"\x00")

    if item_type is TypeInsn:
        type_item = item
        cp_index = cp.add_class(type_item.class_name)
        return _trusted_const_pool_index(type_item.type, offset, cp_index)

    if item_type is LdcInsn:
        ldc_item = item
        cp_index = _lower_ldc_value(ldc_item.value, cp)
        ldc_value_type = type(ldc_item.value)
        if ldc_value_type is LdcLong or ldc_value_type is LdcDouble:
            return _trusted_const_pool_index(InsnInfoType.LDC2_W, offset, cp_index)
        if cp_index <= 255:
            return _trusted_local_index(InsnInfoType.LDC, offset, cp_index)
        return _trusted_const_pool_index(InsnInfoType.LDC_W, offset, cp_index)

    if item_type is IIncInsn:
        iinc_item = item
        slot = _require_u2(iinc_item.slot, context="local variable slot")
        increment = _require_i2(iinc_item.increment, context="iinc increment")
        if slot <= 255 and -128 <= increment <= 127:
            return _trusted_iinc(InsnInfoType.IINC, offset, slot, increment)
        return _trusted_iinc_w(InsnInfoType.IINCW, offset, slot, increment)

    if item_type is InvokeDynamicInsn:
        invoke_dynamic_item = item
        bootstrap_method_attr_index = _require_u2(
            invoke_dynamic_item.bootstrap_method_attr_index,
            context="bootstrap_method_attr_index",
        )
        cp_index = cp.add_invoke_dynamic(
            bootstrap_method_attr_index,
            invoke_dynamic_item.name,
            invoke_dynamic_item.descriptor,
        )
        return _trusted_invoke_dynamic(InsnInfoType.INVOKEDYNAMIC, offset, cp_index, b"\x00\x00")

    if item_type is MultiANewArrayInsn:
        multi_anew_array_item = item
        dimensions = _require_u1(
            multi_anew_array_item.dimensions,
            context="multianewarray dimensions",
            minimum=1,
        )
        cp_index = cp.add_class(multi_anew_array_item.class_name)
        return _trusted_multi_anew_array(InsnInfoType.MULTIANEWARRAY, offset, cp_index, dimensions)

    lowered = _clone_raw_instruction(item, offset)
    if type(lowered) is LookupSwitch:
        lowered.npairs = len(lowered.pairs)
    return lowered


def _lower_exception_handlers(exception_handlers, label_offsets, cp):
    lowered = []
    for handler in exception_handlers:
        start_pc = _require_label_offset(label_offsets, handler.start, "exception handler start")
        end_pc = _require_label_offset(label_offsets, handler.end, "exception handler end")
        handler_pc = _require_label_offset(label_offsets, handler.handler, "exception handler target")
        if start_pc >= end_pc:
            raise ValueError("exception handler start must be strictly before end")
        catch_type = 0 if handler.catch_type is None else cp.add_class(handler.catch_type)
        lowered.append(_trusted_exception_info(start_pc, end_pc, handler_pc, catch_type))
    return lowered


cdef object _build_line_number_attribute(object line_numbers, dict label_offsets, object cp):
    cdef list table
    cdef LineNumberEntry entry
    cdef int start_pc
    cdef int attribute_name_index
    if not line_numbers:
        return None
    table = []
    for entry in line_numbers:
        start_pc = _require_label_offset(label_offsets, entry.label, "line number entry")
        table.append(_trusted_line_number_info(start_pc, entry.line_number))
    attribute_name_index = cp.add_utf8("LineNumberTable")
    return _trusted_line_number_table_attr(attribute_name_index, table)


cdef inline int _local_range_length(int start, int end, str context):
    if end < start:
        raise ValueError(f"{context} end label must not resolve before start label")
    return end - start


cdef object _build_local_variable_attribute(object local_variables, dict label_offsets, object cp):
    cdef list table
    cdef LocalVariableEntry entry
    cdef int start_pc
    cdef int attribute_name_index
    if not local_variables:
        return None
    table = []
    for entry in local_variables:
        start_pc = _require_label_offset(label_offsets, entry.start, "local variable start")
        table.append(
            _trusted_local_variable_info(
                start_pc,
                _local_range_length(
                    start_pc,
                    _require_label_offset(label_offsets, entry.end, "local variable end"),
                    "local variable range",
                ),
                cp.add_utf8(entry.name),
                cp.add_utf8(entry.descriptor),
                entry.slot,
            )
        )
    attribute_name_index = cp.add_utf8("LocalVariableTable")
    return _trusted_local_variable_table_attr(attribute_name_index, table)


cdef object _build_local_variable_type_attribute(object local_variable_types, dict label_offsets, object cp):
    cdef list table
    cdef LocalVariableTypeEntry entry
    cdef int start_pc
    cdef int attribute_name_index
    if not local_variable_types:
        return None
    table = []
    for entry in local_variable_types:
        start_pc = _require_label_offset(label_offsets, entry.start, "local variable type start")
        table.append(
            _trusted_local_variable_type_info(
                start_pc,
                _local_range_length(
                    start_pc,
                    _require_label_offset(label_offsets, entry.end, "local variable type end"),
                    "local variable type range",
                ),
                cp.add_utf8(entry.name),
                cp.add_utf8(entry.signature),
                entry.slot,
            )
        )
    attribute_name_index = cp.add_utf8("LocalVariableTypeTable")
    return _trusted_local_variable_type_table_attr(attribute_name_index, table)


def _lower_resolved_code(code, list items, LabelResolution resolution, object cp, bint keep_debug_info):
    cdef Py_ssize_t index, item_count, lowered_count
    cdef list lowered_code
    cdef list instruction_offsets = resolution.instruction_offsets
    cdef dict label_offsets = resolution.label_offsets
    cdef int offset
    cdef object item, lowered
    cdef type item_type
    cdef CInsnInfo no_operand_insn

    item_count = len(items)
    lowered_code = [None] * item_count
    lowered_count = 0
    for index in range(item_count):
        item = items[index]
        offset = instruction_offsets[index]
        item_type = type(item)
        if item_type is Label:
            continue
        if item_type is InsnInfo:
            no_operand_insn = item
            lowered_code[lowered_count] = _trusted_raw_insn(no_operand_insn.type, offset)
            lowered_count += 1
            continue
        lowered = _lower_instruction(item, offset, label_offsets, cp)
        if lowered is not None:
            lowered_code[lowered_count] = lowered
            lowered_count += 1
    if lowered_count != item_count:
        del lowered_code[lowered_count:]
    exception_table = _lower_exception_handlers(code.exception_handlers, label_offsets, cp)

    line_number_attr = None
    local_variable_attr = None
    local_variable_type_attr = None
    if keep_debug_info:
        line_number_attr = _build_line_number_attribute(code.line_numbers, label_offsets, cp)
        local_variable_attr = _build_local_variable_attribute(code.local_variables, label_offsets, cp)
        local_variable_type_attr = _build_local_variable_type_attribute(
            code.local_variable_types,
            label_offsets,
            cp,
        )

    attributes = _ordered_nested_code_attributes(
        code,
        line_number_attr,
        local_variable_attr,
        local_variable_type_attr,
    )

    return _trusted_code_attr(
        cp.add_utf8("Code"),
        _code_attribute_length(
            resolution.total_code_length,
            len(exception_table),
            attributes,
        ),
        code.max_stack,
        code.max_locals,
        resolution.total_code_length,
        lowered_code,
        exception_table,
        attributes,
    )


def lower_code(
    code,
    cp,
    *,
    method=None,
    class_name=None,
    resolver=None,
    recompute_frames=False,
    debug_info=DebugInfoPolicy.PRESERVE,
):
    """Lower a label-based ``CodeModel`` into a raw ``CodeAttr``.

    Converts symbolic label references into concrete bytecode offsets,
    automatically widening branches that overflow the signed 16-bit range.

    Args:
        code: The label-based code model to lower.
        cp: Constant-pool builder used to allocate pool entries for operands.
        method: Method that owns *code*.  Required when *recompute_frames*
            is ``True``.
        class_name: Internal name of the class containing the method.
            Required when *recompute_frames* is ``True``.
        resolver: Optional class hierarchy resolver for frame computation.
        recompute_frames: When ``True``, ``max_stack``, ``max_locals``, and
            the ``StackMapTable`` attribute are recomputed via stack
            simulation.
        debug_info: Policy controlling whether debug attributes
            (``LineNumberTable``, ``LocalVariableTable``,
            ``LocalVariableTypeTable``) are preserved or stripped.
            Stale debug metadata is stripped automatically regardless.

    Returns:
        A fully resolved ``CodeAttr`` ready for binary serialisation.

    Raises:
        ValueError: If *recompute_frames* is ``True`` but *method* or
            *class_name* is ``None``, or if the resulting code length
            exceeds the JVM maximum of 65 535 bytes.
    """
    if recompute_frames and (method is None or class_name is None):
        raise ValueError("method and class_name are required when recompute_frames=True")

    cdef LabelResolution resolution
    debug_policy = normalize_debug_info_policy(debug_info)
    keep_debug_info = debug_policy is DebugInfoPolicy.PRESERVE and not is_code_debug_info_stale(code)
    items = list(code.instructions)
    ldc_index_cache = None
    if _needs_ldc_index_cache(items):
        ldc_index_cache = _build_ldc_index_cache(items, cp)

    while True:
        resolution = _resolve_labels_with_cache(items, ldc_index_cache)
        if resolution.total_code_length > 65535:
            raise ValueError(f"code length {resolution.total_code_length} exceeds JVM maximum of 65535 bytes")
        if not _promote_overflow_branches(items, resolution):
            break

    checkpoint = cp.checkpoint()
    try:
        result = _lower_resolved_code(code, items, resolution, cp, keep_debug_info)
    except Exception:
        cp.rollback(checkpoint)
        raise

    if recompute_frames:
        assert method is not None and class_name is not None
        from ..analysis import compute_frames
        from ..classfile.attributes import StackMapTableAttr

        frame_result = compute_frames(
            code,
            method,
            class_name,
            cp,
            resolution.label_offsets,
            resolver,
        )
        result.max_stacks = frame_result.max_stack
        result.max_locals = frame_result.max_locals
        stack_map_index = next(
            (i for i, attr in enumerate(result.attributes) if isinstance(attr, StackMapTableAttr)),
            len(result.attributes),
        )
        result.attributes = [attr for attr in result.attributes if not isinstance(attr, StackMapTableAttr)]
        if frame_result.stack_map_table is not None:
            insert_at = min(stack_map_index, len(result.attributes))
            result.attributes.insert(insert_at, frame_result.stack_map_table)
        _refresh_code_attr_metadata(result)

    return result


def resolve_catch_type(cp, catch_type_index):
    """Resolve an exception handler catch-type constant-pool index.

    Args:
        cp: Constant-pool builder to look up the entry in.
        catch_type_index: Index into the constant pool.  ``0`` denotes a
            catch-all (``finally``) handler.

    Returns:
        The internal class name of the caught exception type, or ``None``
        for a catch-all handler.

    Raises:
        ValueError: If the index is non-zero but does not point to a
            ``CONSTANT_Class`` entry.
    """

    if catch_type_index == 0:
        return None

    entry = cp.peek(catch_type_index)
    if not isinstance(entry, ClassInfo):
        raise ValueError(f"catch_type CP index {catch_type_index} is not a CONSTANT_Class")
    return cp.resolve_utf8(entry.name_index)


# ---------------------------------------------------------------------------
# LDC value lowering helpers
# ---------------------------------------------------------------------------


def _lower_ldc_value(value, cp):
    """Resolve an ``LdcValue`` to a CP index, adding entries as needed."""
    cdef type t = type(value)
    cdef CLdcInt int_value
    cdef CLdcFloat float_value
    cdef CLdcLong long_value
    cdef CLdcDouble double_value
    cdef CLdcString string_value
    cdef CLdcClass class_value
    cdef CLdcMethodType method_type_value
    cdef CLdcMethodHandle method_handle_value
    cdef CLdcDynamic dynamic_value
    if t is LdcInt:
        int_value = value
        return cp.add_integer(int_value.value)
    if t is LdcFloat:
        float_value = value
        return cp.add_float(float_value.raw_bits)
    if t is LdcLong:
        long_value = value
        unsigned = long_value.value & 0xFFFFFFFFFFFFFFFF
        high = (unsigned >> 32) & 0xFFFFFFFF
        low = unsigned & 0xFFFFFFFF
        return cp.add_long(high, low)
    if t is LdcDouble:
        double_value = value
        return cp.add_double(double_value.high_bytes, double_value.low_bytes)
    if t is LdcString:
        string_value = value
        return cp.add_string(string_value.value)
    if t is LdcClass:
        class_value = value
        return cp.add_class(class_value.name)
    if t is LdcMethodType:
        method_type_value = value
        return cp.add_method_type(method_type_value.descriptor)
    if t is LdcMethodHandle:
        method_handle_value = value
        return _lower_ldc_method_handle(method_handle_value, cp)
    dynamic_value = value
    return cp.add_dynamic(dynamic_value.bootstrap_method_attr_index, dynamic_value.name, dynamic_value.descriptor)


def _find_existing_ldc_index(value, cp):
    cdef type t = type(value)
    cdef CLdcInt int_value
    cdef CLdcFloat float_value
    cdef CLdcLong long_value
    cdef CLdcDouble double_value
    cdef CLdcString string_value
    cdef CLdcClass class_value
    cdef CLdcMethodType method_type_value
    cdef CLdcMethodHandle method_handle_value
    cdef CLdcDynamic dynamic_value
    if t is LdcInt:
        int_value = value
        return cp.find_integer(int_value.value)
    if t is LdcFloat:
        float_value = value
        return cp.find_float(float_value.raw_bits)
    if t is LdcLong:
        long_value = value
        unsigned = long_value.value & 0xFFFFFFFFFFFFFFFF
        high = (unsigned >> 32) & 0xFFFFFFFF
        low = unsigned & 0xFFFFFFFF
        return cp.find_long(high, low)
    if t is LdcDouble:
        double_value = value
        return cp.find_double(double_value.high_bytes, double_value.low_bytes)
    if t is LdcString:
        string_value = value
        return cp.find_string(string_value.value)
    if t is LdcClass:
        class_value = value
        return cp.find_class(class_value.name)
    if t is LdcMethodType:
        method_type_value = value
        return cp.find_method_type(method_type_value.descriptor)
    if t is LdcMethodHandle:
        method_handle_value = value
        return _find_existing_ldc_method_handle(method_handle_value, cp)
    dynamic_value = value
    return cp.find_dynamic(dynamic_value.bootstrap_method_attr_index, dynamic_value.name, dynamic_value.descriptor)


def _lower_ldc_method_handle(CLdcMethodHandle value, cp):
    """Lower an ``LdcMethodHandle`` to a CONSTANT_MethodHandle CP index."""
    kind = value.reference_kind
    if kind in (1, 2, 3, 4):  # REF_getField, REF_getStatic, REF_putField, REF_putStatic
        ref_index = cp.add_fieldref(value.owner, value.name, value.descriptor)
    elif kind in (5, 8):  # REF_invokeVirtual, REF_newInvokeSpecial -> always Methodref
        ref_index = cp.add_methodref(value.owner, value.name, value.descriptor)
    elif kind == 9:  # REF_invokeInterface -> always InterfaceMethodref
        ref_index = cp.add_interface_methodref(value.owner, value.name, value.descriptor)
    elif kind in (6, 7):  # REF_invokeStatic, REF_invokeSpecial -> depends on is_interface
        if value.is_interface:
            ref_index = cp.add_interface_methodref(value.owner, value.name, value.descriptor)
        else:
            ref_index = cp.add_methodref(value.owner, value.name, value.descriptor)
    else:
        raise ValueError(f"invalid MethodHandle reference_kind: {kind}")
    return cp.add_method_handle(kind, ref_index)


def _find_existing_ldc_method_handle(CLdcMethodHandle value, cp):
    kind = value.reference_kind
    if kind in (1, 2, 3, 4):
        ref_index = cp.find_fieldref(value.owner, value.name, value.descriptor)
    elif kind in (5, 8):
        ref_index = cp.find_methodref(value.owner, value.name, value.descriptor)
    elif kind == 9:
        ref_index = cp.find_interface_methodref(value.owner, value.name, value.descriptor)
    elif kind in (6, 7):
        if value.is_interface:
            ref_index = cp.find_interface_methodref(value.owner, value.name, value.descriptor)
        else:
            ref_index = cp.find_methodref(value.owner, value.name, value.descriptor)
    else:
        raise ValueError(f"invalid MethodHandle reference_kind: {kind}")

    if ref_index is None:
        return None
    return cp.find_method_handle(kind, ref_index)
