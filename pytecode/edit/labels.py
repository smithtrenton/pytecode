"""Label-based bytecode instruction editing.

Provides symbolic ``Label`` targets and label-aware instruction types
(``BranchInsn``, ``LookupSwitchInsn``, ``TableSwitchInsn``) so that bytecode
can be manipulated without tracking raw offsets.  ``lower_code`` converts a
label-based ``CodeModel`` into an offset-based ``CodeAttr`` ready for
serialisation, and ``resolve_labels`` computes the offset mapping.
"""

from __future__ import annotations

import copy
from dataclasses import dataclass
from typing import TYPE_CHECKING, cast

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
from ..descriptors import parameter_slot_count, parse_method_descriptor
from ._attribute_clone import clone_attribute
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


def _clone_raw_instruction(insn: InsnInfo, *, bytecode_offset: int | None = None) -> InsnInfo:
    offset = insn.bytecode_offset if bytecode_offset is None else bytecode_offset

    insn_type = type(insn)
    if insn_type is LookupSwitch:
        lookup_switch = cast(LookupSwitch, insn)
        return LookupSwitch(
            lookup_switch.type,
            offset,
            lookup_switch.default,
            lookup_switch.npairs,
            [MatchOffsetPair(pair.match, pair.offset) for pair in lookup_switch.pairs],
        )
    if insn_type is TableSwitch:
        table_switch = cast(TableSwitch, insn)
        return TableSwitch(
            table_switch.type,
            offset,
            table_switch.default,
            table_switch.low,
            table_switch.high,
            list(table_switch.offsets),
        )
    if insn_type is InsnInfo:
        return InsnInfo(insn.type, offset)
    if insn_type is LocalIndex:
        local_index = cast(LocalIndex, insn)
        return LocalIndex(local_index.type, offset, local_index.index)
    if insn_type is LocalIndexW:
        local_index_w = cast(LocalIndexW, insn)
        return LocalIndexW(local_index_w.type, offset, local_index_w.index)
    if insn_type is ConstPoolIndex:
        const_pool_index = cast(ConstPoolIndex, insn)
        return ConstPoolIndex(const_pool_index.type, offset, const_pool_index.index)
    if insn_type is ByteValue:
        byte_value = cast(ByteValue, insn)
        return ByteValue(byte_value.type, offset, byte_value.value)
    if insn_type is ShortValue:
        short_value = cast(ShortValue, insn)
        return ShortValue(short_value.type, offset, short_value.value)
    if insn_type is Branch:
        branch = cast(Branch, insn)
        return Branch(branch.type, offset, branch.offset)
    if insn_type is BranchW:
        branch_w = cast(BranchW, insn)
        return BranchW(branch_w.type, offset, branch_w.offset)
    if insn_type is IInc:
        iinc = cast(IInc, insn)
        return IInc(iinc.type, offset, iinc.index, iinc.value)
    if insn_type is IIncW:
        iinc_w = cast(IIncW, insn)
        return IIncW(iinc_w.type, offset, iinc_w.index, iinc_w.value)
    if insn_type is InvokeDynamic:
        invoke_dynamic = cast(InvokeDynamic, insn)
        return InvokeDynamic(invoke_dynamic.type, offset, invoke_dynamic.index, invoke_dynamic.unused)
    if insn_type is InvokeInterface:
        invoke_interface = cast(InvokeInterface, insn)
        return InvokeInterface(
            invoke_interface.type,
            offset,
            invoke_interface.index,
            invoke_interface.count,
            invoke_interface.unused,
        )
    if insn_type is NewArray:
        new_array = cast(NewArray, insn)
        return NewArray(new_array.type, offset, new_array.atype)
    if insn_type is MultiANewArray:
        multi_anew_array = cast(MultiANewArray, insn)
        return MultiANewArray(
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
    "CodeItem",
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


def clone_raw_instruction(insn: InsnInfo, *, bytecode_offset: int | None = None) -> InsnInfo:
    """Clone a raw JVM instruction without using ``copy.deepcopy``."""
    return _clone_raw_instruction(insn, bytecode_offset=bytecode_offset)


_I2_MIN = -(1 << 15)
_I2_MAX = (1 << 15) - 1
_I4_MIN = -(1 << 31)
_I4_MAX = (1 << 31) - 1

_BRANCH_WIDENINGS: dict[InsnInfoType, InsnInfoType] = {
    InsnInfoType.GOTO: InsnInfoType.GOTO_W,
    InsnInfoType.JSR: InsnInfoType.JSR_W,
}

_INVERTED_CONDITIONAL_BRANCHES: dict[InsnInfoType, InsnInfoType] = {
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


@dataclass(eq=False)
class Label:
    """Identity-based marker for a bytecode position.

    Labels use identity equality so that distinct instances targeting the same
    logical position remain distinguishable.

    Attributes:
        name: Optional human-readable name for debugging output.
    """

    name: str | None = None

    def __repr__(self) -> str:
        if self.name is not None:
            return f"Label({self.name!r})"
        return f"Label(id=0x{id(self):x})"


type CodeItem = InsnInfo | Label


@dataclass
class ExceptionHandler:
    """An exception handler entry that uses labels for range and target.

    Attributes:
        start: Label marking the beginning of the protected region (inclusive).
        end: Label marking the end of the protected region (exclusive).
        handler: Label marking the entry point of the handler code.
        catch_type: Internal name of the caught exception class, or ``None``
            for a catch-all (``finally``) handler.
    """

    start: Label
    end: Label
    handler: Label
    catch_type: str | None


@dataclass
class LineNumberEntry:
    """Maps a label position to a source line number.

    Attributes:
        label: Label marking the bytecode position.
        line_number: Corresponding source-file line number.
    """

    label: Label
    line_number: int


@dataclass
class LocalVariableEntry:
    """A local variable debug entry using labels for the live range.

    Attributes:
        start: Label marking the start of the variable's scope (inclusive).
        end: Label marking the end of the variable's scope (exclusive).
        name: Variable name as it appears in source.
        descriptor: JVM field descriptor of the variable's type.
        slot: Local variable table slot index.
    """

    start: Label
    end: Label
    name: str
    descriptor: str
    slot: int


@dataclass
class LocalVariableTypeEntry:
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

    start: Label
    end: Label
    name: str
    signature: str
    slot: int


@dataclass(init=False)
class BranchInsn(InsnInfo):
    """A branch instruction that targets a label instead of a raw offset.

    Supports both narrow (2-byte offset) and wide (4-byte offset) branch
    opcodes.  During lowering, narrow branches that overflow are automatically
    widened or inverted as needed.

    Attributes:
        target: The ``Label`` this branch jumps to.
    """

    target: Label

    def __init__(self, insn_type: InsnInfoType, target: Label, bytecode_offset: int = -1) -> None:
        if insn_type.instinfo not in {Branch, BranchW}:
            raise ValueError(f"{insn_type.name} is not a branch opcode")
        super().__init__(insn_type, bytecode_offset)
        self.target = target


@dataclass(init=False)
class LookupSwitchInsn(InsnInfo):
    """A ``lookupswitch`` instruction that uses labels for jump targets.

    Attributes:
        default_target: Label for the default branch.
        pairs: Match-value / label pairs for each case.
    """

    default_target: Label
    pairs: list[tuple[int, Label]]

    def __init__(
        self,
        default_target: Label,
        pairs: list[tuple[int, Label]],
        bytecode_offset: int = -1,
    ) -> None:
        super().__init__(InsnInfoType.LOOKUPSWITCH, bytecode_offset)
        self.default_target = default_target
        self.pairs = list(pairs)


@dataclass(init=False)
class TableSwitchInsn(InsnInfo):
    """A ``tableswitch`` instruction that uses labels for jump targets.

    Attributes:
        default_target: Label for the default branch.
        low: Minimum match value (inclusive).
        high: Maximum match value (inclusive).
        targets: Labels for each case in the ``low..high`` range.
    """

    default_target: Label
    low: int
    high: int
    targets: list[Label]

    def __init__(
        self,
        default_target: Label,
        low: int,
        high: int,
        targets: list[Label],
        bytecode_offset: int = -1,
    ) -> None:
        if high < low:
            raise ValueError("tableswitch high must be >= low")
        expected_targets = high - low + 1
        if len(targets) != expected_targets:
            raise ValueError(f"tableswitch range {low}..{high} requires {expected_targets} targets, got {len(targets)}")
        super().__init__(InsnInfoType.TABLESWITCH, bytecode_offset)
        self.default_target = default_target
        self.low = low
        self.high = high
        self.targets = list(targets)


@dataclass
class LabelResolution:
    """Result of resolving labels to bytecode offsets.

    Attributes:
        label_offsets: Mapping from each ``Label`` to its resolved bytecode offset.
        instruction_offsets: Bytecode offset of each item in the instruction list.
        total_code_length: Total byte length of the lowered bytecode.
    """

    label_offsets: dict[Label, int]
    instruction_offsets: list[int]
    total_code_length: int


def _switch_padding(offset: int) -> int:
    return (4 - ((offset + 1) % 4)) % 4


def _fits_i2(value: int) -> bool:
    return _I2_MIN <= value <= _I2_MAX


def _fits_i4(value: int) -> bool:
    return _I4_MIN <= value <= _I4_MAX


def _require_label_offset(label_offsets: dict[Label, int], label: Label, *, context: str) -> int:
    try:
        return label_offsets[label]
    except KeyError as exc:
        raise ValueError(f"{context} refers to a label that is not present in CodeModel.instructions") from exc


def _relative_offset(source_offset: int, label: Label, label_offsets: dict[Label, int], *, context: str) -> int:
    return _require_label_offset(label_offsets, label, context=context) - source_offset


def _attribute_marshaled_size(attribute: AttributeInfo) -> int:
    return 6 + attribute.attribute_length


def _code_attribute_length(
    code_length: int,
    exception_table_length: int,
    attributes: list[AttributeInfo],
) -> int:
    nested_size = sum(_attribute_marshaled_size(attribute) for attribute in attributes)
    return 12 + code_length + (8 * exception_table_length) + nested_size


def _refresh_code_attr_metadata(code_attr: CodeAttr) -> None:
    code_attr.attributes_count = len(code_attr.attributes)
    code_attr.attribute_length = _code_attribute_length(
        code_attr.code_length,
        code_attr.exception_table_length,
        code_attr.attributes,
    )


def _lifted_debug_attrs(attributes: list[AttributeInfo]) -> list[AttributeInfo]:
    return [
        attribute
        for attribute in attributes
        if not isinstance(
            attribute,
            (LineNumberTableAttr, LocalVariableTableAttr, LocalVariableTypeTableAttr),
        )
    ]


def _ordered_nested_code_attributes(
    code: CodeModel,
    line_number_attr: LineNumberTableAttr | None,
    local_variable_attr: LocalVariableTableAttr | None,
    local_variable_type_attr: LocalVariableTypeTableAttr | None,
) -> list[AttributeInfo]:
    other_attrs = [clone_attribute(attribute) for attribute in _lifted_debug_attrs(code.attributes)]
    if not code._nested_attribute_layout:
        attrs = other_attrs
        for debug_attr in (line_number_attr, local_variable_attr, local_variable_type_attr):
            if debug_attr is not None:
                attrs.append(debug_attr)
        return attrs

    attrs: list[AttributeInfo] = []
    other_index = 0
    debug_attrs: dict[str, AttributeInfo | None] = {
        "line_numbers": line_number_attr,
        "local_variables": local_variable_attr,
        "local_variable_types": local_variable_type_attr,
    }

    for token in code._nested_attribute_layout:
        if token == "other":
            if other_index < len(other_attrs):
                attrs.append(other_attrs[other_index])
                other_index += 1
            continue

        debug_attr = debug_attrs.get(token)
        if debug_attr is not None:
            attrs.append(debug_attr)
            debug_attrs[token] = None

    attrs.extend(other_attrs[other_index:])
    for token in ("line_numbers", "local_variables", "local_variable_types"):
        debug_attr = debug_attrs[token]
        if debug_attr is not None:
            attrs.append(debug_attr)

    return attrs


def _needs_ldc_index_cache(items: list[CodeItem]) -> bool:
    return any(isinstance(item, LdcInsn) and not isinstance(item.value, (LdcLong, LdcDouble)) for item in items)


def _build_ldc_index_cache(items: list[CodeItem], cp: ConstantPoolBuilder) -> dict[int, int]:
    cache: dict[int, int] = {}
    checkpoint = None

    for item in items:
        if not isinstance(item, LdcInsn):
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


def _resolve_labels_with_cache(
    items: list[CodeItem],
    ldc_index_cache: dict[int, int] | None = None,
) -> LabelResolution:
    label_offsets: dict[Label, int] = {}
    instruction_offsets: list[int] = []
    instruction_offsets_append = instruction_offsets.append
    offset = 0

    for item in items:
        instruction_offsets_append(offset)
        if type(item) is Label:
            label = item
            if label in label_offsets:
                raise ValueError(f"label {label!r} appears multiple times in CodeModel.instructions")
            label_offsets[label] = offset
            continue
        offset += _instruction_byte_size(item, offset, ldc_index_cache)

    return LabelResolution(
        label_offsets=label_offsets,
        instruction_offsets=instruction_offsets,
        total_code_length=offset,
    )


def _instruction_byte_size(
    insn: CodeItem,
    offset: int,
    ldc_index_cache: dict[int, int] | None = None,
) -> int:
    insn_type = type(insn)
    if insn_type is Label:
        return 0
    if insn_type is BranchInsn:
        branch_insn = cast(BranchInsn, insn)
        return 5 if branch_insn.type.instinfo is BranchW else 3
    if insn_type is LookupSwitchInsn:
        lookup_switch_insn = cast(LookupSwitchInsn, insn)
        return 1 + _switch_padding(offset) + 8 + (8 * len(lookup_switch_insn.pairs))
    if insn_type is TableSwitchInsn:
        table_switch_insn = cast(TableSwitchInsn, insn)
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
        ldc_insn = cast(LdcInsn, insn)
        if isinstance(ldc_insn.value, (LdcLong, LdcDouble)):
            return 3  # LDC2_W: opcode(1) + u2 CP index
        if ldc_index_cache is None:
            raise ValueError("LdcInsn size requires constant-pool context")
        idx = ldc_index_cache.get(id(ldc_insn))
        if idx is None:
            raise ValueError("LdcInsn is missing from the LDC index cache")
        return 2 if idx <= 255 else 3  # LDC: 2, LDC_W: 3
    if insn_type is VarInsn:
        var_insn = cast(VarInsn, insn)
        slot = _require_u2(var_insn.slot, context="local variable slot")
        if _VAR_SHORTCUTS.get((var_insn.type, slot)) is not None:
            return 1  # implicit form (e.g. ILOAD_0)
        if slot <= 255:
            return 2  # opcode(1) + u1 slot
        return 4  # WIDE(1) + opcode(1) + u2 slot
    if insn_type is IIncInsn:
        iinc_insn = cast(IIncInsn, insn)
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
        lookup_switch = cast(LookupSwitch, insn)
        return 1 + _switch_padding(offset) + 8 + (8 * len(lookup_switch.pairs))
    if insn_type is TableSwitch:
        table_switch = cast(TableSwitch, insn)
        return 1 + _switch_padding(offset) + 12 + (4 * len(table_switch.offsets))
    return 1


def resolve_labels(
    items: list[CodeItem],
    cp: ConstantPoolBuilder | None = None,
) -> LabelResolution:
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

    ldc_index_cache: dict[int, int] | None = None
    if _needs_ldc_index_cache(items):
        if cp is None:
            raise ValueError(
                "resolve_labels() requires a ConstantPoolBuilder when instructions contain single-slot LdcInsn values"
            )
        ldc_index_cache = _build_ldc_index_cache(items, cp)

    return _resolve_labels_with_cache(items, ldc_index_cache)


def _promote_overflow_branches(items: list[CodeItem], resolution: LabelResolution) -> bool:
    changed = False
    index = 0

    while index < len(items):
        item = items[index]
        if not isinstance(item, BranchInsn):
            index += 1
            continue

        source_offset = resolution.instruction_offsets[index]
        relative = _relative_offset(
            source_offset,
            item.target,
            resolution.label_offsets,
            context=f"{item.type.name} target",
        )

        if item.type.instinfo is BranchW:
            if not _fits_i4(relative):
                raise ValueError(f"{item.type.name} branch offset {relative} exceeds JVM i4 range")
            index += 1
            continue

        if _fits_i2(relative):
            index += 1
            continue

        widened = _BRANCH_WIDENINGS.get(item.type)
        if widened is not None:
            items[index] = BranchInsn(widened, item.target)
            changed = True
            index += 1
            continue

        inverted = _INVERTED_CONDITIONAL_BRANCHES.get(item.type)
        if inverted is None:
            raise ValueError(f"{item.type.name} cannot be widened automatically")

        skip_label = Label(f"{item.type.name.lower()}_skip")
        items[index : index + 1] = [
            BranchInsn(inverted, skip_label),
            BranchInsn(InsnInfoType.GOTO_W, item.target),
            skip_label,
        ]
        changed = True
        index += 3

    return changed


def _lower_instruction(
    item: CodeItem,
    offset: int,
    label_offsets: dict[Label, int],
    cp: ConstantPoolBuilder,
) -> InsnInfo | None:
    item_type = type(item)
    if item_type is Label:
        return None

    if item_type is BranchInsn:
        branch_item = cast(BranchInsn, item)
        relative = _relative_offset(
            offset, branch_item.target, label_offsets, context=f"{branch_item.type.name} target"
        )
        if branch_item.type.instinfo is BranchW:
            if not _fits_i4(relative):
                raise ValueError(f"{branch_item.type.name} branch offset {relative} exceeds JVM i4 range")
            return BranchW(branch_item.type, offset, relative)
        if not _fits_i2(relative):
            raise ValueError(f"{branch_item.type.name} branch offset {relative} exceeds JVM i2 range")
        return Branch(branch_item.type, offset, relative)

    if item_type is LookupSwitchInsn:
        lookup_switch_item = cast(LookupSwitchInsn, item)
        default = _relative_offset(
            offset,
            lookup_switch_item.default_target,
            label_offsets,
            context="lookupswitch default target",
        )
        pairs = [
            MatchOffsetPair(
                match,
                _relative_offset(offset, target, label_offsets, context="lookupswitch case target"),
            )
            for match, target in lookup_switch_item.pairs
        ]
        return LookupSwitch(lookup_switch_item.type, offset, default, len(pairs), pairs)

    if item_type is TableSwitchInsn:
        table_switch_item = cast(TableSwitchInsn, item)
        default = _relative_offset(
            offset,
            table_switch_item.default_target,
            label_offsets,
            context="tableswitch default target",
        )
        offsets = [
            _relative_offset(offset, target, label_offsets, context="tableswitch case target")
            for target in table_switch_item.targets
        ]
        return TableSwitch(
            table_switch_item.type,
            offset,
            default,
            table_switch_item.low,
            table_switch_item.high,
            offsets,
        )

    # Symbolic operand wrappers from operands.py
    if item_type is FieldInsn:
        field_item = cast(FieldInsn, item)
        cp_index = cp.add_fieldref(field_item.owner, field_item.name, field_item.descriptor)
        return ConstPoolIndex(field_item.type, offset, cp_index)

    if item_type is MethodInsn:
        method_item = cast(MethodInsn, item)
        if method_item.is_interface:
            cp_index = cp.add_interface_methodref(method_item.owner, method_item.name, method_item.descriptor)
        else:
            cp_index = cp.add_methodref(method_item.owner, method_item.name, method_item.descriptor)
        return ConstPoolIndex(method_item.type, offset, cp_index)

    if item_type is InterfaceMethodInsn:
        interface_method_item = cast(InterfaceMethodInsn, item)
        cp_index = cp.add_interface_methodref(
            interface_method_item.owner,
            interface_method_item.name,
            interface_method_item.descriptor,
        )
        desc = parse_method_descriptor(interface_method_item.descriptor)
        count = parameter_slot_count(desc) + 1  # +1 for the object reference
        return InvokeInterface(InsnInfoType.INVOKEINTERFACE, offset, cp_index, count, b"\x00")

    if item_type is TypeInsn:
        type_item = cast(TypeInsn, item)
        cp_index = cp.add_class(type_item.class_name)
        return ConstPoolIndex(type_item.type, offset, cp_index)

    if item_type is LdcInsn:
        ldc_item = cast(LdcInsn, item)
        cp_index = _lower_ldc_value(ldc_item.value, cp)
        if isinstance(ldc_item.value, (LdcLong, LdcDouble)):
            return ConstPoolIndex(InsnInfoType.LDC2_W, offset, cp_index)
        if cp_index <= 255:
            return LocalIndex(InsnInfoType.LDC, offset, cp_index)
        return ConstPoolIndex(InsnInfoType.LDC_W, offset, cp_index)

    if item_type is VarInsn:
        var_item = cast(VarInsn, item)
        slot = _require_u2(var_item.slot, context="local variable slot")
        shortcut = _VAR_SHORTCUTS.get((var_item.type, slot))
        if shortcut is not None:
            return InsnInfo(shortcut, offset)
        if slot > 255:
            wide_type = _BASE_TO_WIDE[var_item.type]
            return LocalIndexW(wide_type, offset, slot)
        return LocalIndex(var_item.type, offset, slot)

    if item_type is IIncInsn:
        iinc_item = cast(IIncInsn, item)
        slot = _require_u2(iinc_item.slot, context="local variable slot")
        increment = _require_i2(iinc_item.increment, context="iinc increment")
        if slot <= 255 and -128 <= increment <= 127:
            return IInc(InsnInfoType.IINC, offset, slot, increment)
        return IIncW(InsnInfoType.IINCW, offset, slot, increment)

    if item_type is InvokeDynamicInsn:
        invoke_dynamic_item = cast(InvokeDynamicInsn, item)
        bootstrap_method_attr_index = _require_u2(
            invoke_dynamic_item.bootstrap_method_attr_index,
            context="bootstrap_method_attr_index",
        )
        cp_index = cp.add_invoke_dynamic(
            bootstrap_method_attr_index,
            invoke_dynamic_item.name,
            invoke_dynamic_item.descriptor,
        )
        return InvokeDynamic(InsnInfoType.INVOKEDYNAMIC, offset, cp_index, b"\x00\x00")

    if item_type is MultiANewArrayInsn:
        multi_anew_array_item = cast(MultiANewArrayInsn, item)
        dimensions = _require_u1(
            multi_anew_array_item.dimensions,
            context="multianewarray dimensions",
            minimum=1,
        )
        cp_index = cp.add_class(multi_anew_array_item.class_name)
        return MultiANewArray(InsnInfoType.MULTIANEWARRAY, offset, cp_index, dimensions)

    lowered = _clone_raw_instruction(cast(InsnInfo, item), bytecode_offset=offset)
    if isinstance(lowered, LookupSwitch):
        lowered.npairs = len(lowered.pairs)
    return lowered


def _lower_exception_handlers(
    exception_handlers: list[ExceptionHandler],
    label_offsets: dict[Label, int],
    cp: ConstantPoolBuilder,
) -> list[ExceptionInfo]:
    lowered: list[ExceptionInfo] = []
    for handler in exception_handlers:
        start_pc = _require_label_offset(label_offsets, handler.start, context="exception handler start")
        end_pc = _require_label_offset(label_offsets, handler.end, context="exception handler end")
        handler_pc = _require_label_offset(label_offsets, handler.handler, context="exception handler target")
        if start_pc >= end_pc:
            raise ValueError("exception handler start must be strictly before end")
        catch_type = 0 if handler.catch_type is None else cp.add_class(handler.catch_type)
        lowered.append(ExceptionInfo(start_pc, end_pc, handler_pc, catch_type))
    return lowered


def _build_line_number_attribute(
    line_numbers: list[LineNumberEntry],
    label_offsets: dict[Label, int],
    cp: ConstantPoolBuilder,
) -> LineNumberTableAttr | None:
    if not line_numbers:
        return None
    table = [
        LineNumberInfo(
            _require_label_offset(label_offsets, entry.label, context="line number entry"),
            entry.line_number,
        )
        for entry in line_numbers
    ]
    return LineNumberTableAttr(
        attribute_name_index=cp.add_utf8("LineNumberTable"),
        attribute_length=2 + (4 * len(table)),
        line_number_table_length=len(table),
        line_number_table=table,
    )


def _local_range_length(start: int, end: int, *, context: str) -> int:
    if end < start:
        raise ValueError(f"{context} end label must not resolve before start label")
    return end - start


def _build_local_variable_attribute(
    local_variables: list[LocalVariableEntry],
    label_offsets: dict[Label, int],
    cp: ConstantPoolBuilder,
) -> LocalVariableTableAttr | None:
    if not local_variables:
        return None
    table = [
        LocalVariableInfo(
            start_pc := _require_label_offset(label_offsets, entry.start, context="local variable start"),
            _local_range_length(
                start_pc,
                _require_label_offset(label_offsets, entry.end, context="local variable end"),
                context="local variable range",
            ),
            cp.add_utf8(entry.name),
            cp.add_utf8(entry.descriptor),
            entry.slot,
        )
        for entry in local_variables
    ]
    return LocalVariableTableAttr(
        attribute_name_index=cp.add_utf8("LocalVariableTable"),
        attribute_length=2 + (10 * len(table)),
        local_variable_table_length=len(table),
        local_variable_table=table,
    )


def _build_local_variable_type_attribute(
    local_variable_types: list[LocalVariableTypeEntry],
    label_offsets: dict[Label, int],
    cp: ConstantPoolBuilder,
) -> LocalVariableTypeTableAttr | None:
    if not local_variable_types:
        return None
    table = [
        LocalVariableTypeInfo(
            start_pc := _require_label_offset(label_offsets, entry.start, context="local variable type start"),
            _local_range_length(
                start_pc,
                _require_label_offset(label_offsets, entry.end, context="local variable type end"),
                context="local variable type range",
            ),
            cp.add_utf8(entry.name),
            cp.add_utf8(entry.signature),
            entry.slot,
        )
        for entry in local_variable_types
    ]
    return LocalVariableTypeTableAttr(
        attribute_name_index=cp.add_utf8("LocalVariableTypeTable"),
        attribute_length=2 + (10 * len(table)),
        local_variable_type_table_length=len(table),
        local_variable_type_table=table,
    )


def _lower_resolved_code(
    code: CodeModel,
    items: list[CodeItem],
    resolution: LabelResolution,
    cp: ConstantPoolBuilder,
    keep_debug_info: bool,
) -> CodeAttr:
    lowered_code: list[InsnInfo] = []
    lowered_code_append = lowered_code.append
    lower_instruction = _lower_instruction
    label_offsets = resolution.label_offsets

    for item, offset in zip(items, resolution.instruction_offsets, strict=True):
        lowered = lower_instruction(item, offset, label_offsets, cp)
        if lowered is not None:
            lowered_code_append(lowered)
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

    return CodeAttr(
        attribute_name_index=cp.add_utf8("Code"),
        attribute_length=_code_attribute_length(
            resolution.total_code_length,
            len(exception_table),
            attributes,
        ),
        max_stacks=code.max_stack,
        max_locals=code.max_locals,
        code_length=resolution.total_code_length,
        code=lowered_code,
        exception_table_length=len(exception_table),
        exception_table=exception_table,
        attributes_count=len(attributes),
        attributes=attributes,
    )


def lower_code(
    code: CodeModel,
    cp: ConstantPoolBuilder,
    *,
    method: MethodModel | None = None,
    class_name: str | None = None,
    resolver: ClassResolver | None = None,
    recompute_frames: bool = False,
    debug_info: DebugInfoPolicy | str = DebugInfoPolicy.PRESERVE,
) -> CodeAttr:
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

    debug_policy = normalize_debug_info_policy(debug_info)
    keep_debug_info = debug_policy is DebugInfoPolicy.PRESERVE and not is_code_debug_info_stale(code)
    items = list(code.instructions)
    ldc_index_cache: dict[int, int] | None = None
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


def resolve_catch_type(cp: ConstantPoolBuilder, catch_type_index: int) -> str | None:
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


def _lower_ldc_value(value: LdcValue, cp: ConstantPoolBuilder) -> int:
    """Resolve an ``LdcValue`` to a CP index, adding entries as needed."""
    if isinstance(value, LdcInt):
        return cp.add_integer(value.value)
    if isinstance(value, LdcFloat):
        return cp.add_float(value.raw_bits)
    if isinstance(value, LdcLong):
        unsigned = value.value & 0xFFFFFFFFFFFFFFFF
        high = (unsigned >> 32) & 0xFFFFFFFF
        low = unsigned & 0xFFFFFFFF
        return cp.add_long(high, low)
    if isinstance(value, LdcDouble):
        return cp.add_double(value.high_bytes, value.low_bytes)
    if isinstance(value, LdcString):
        return cp.add_string(value.value)
    if isinstance(value, LdcClass):
        return cp.add_class(value.name)
    if isinstance(value, LdcMethodType):
        return cp.add_method_type(value.descriptor)
    if isinstance(value, LdcMethodHandle):
        return _lower_ldc_method_handle(value, cp)
    return cp.add_dynamic(value.bootstrap_method_attr_index, value.name, value.descriptor)


def _find_existing_ldc_index(value: LdcValue, cp: ConstantPoolBuilder) -> int | None:
    if isinstance(value, LdcInt):
        return cp.find_integer(value.value)
    if isinstance(value, LdcFloat):
        return cp.find_float(value.raw_bits)
    if isinstance(value, LdcLong):
        unsigned = value.value & 0xFFFFFFFFFFFFFFFF
        high = (unsigned >> 32) & 0xFFFFFFFF
        low = unsigned & 0xFFFFFFFF
        return cp.find_long(high, low)
    if isinstance(value, LdcDouble):
        return cp.find_double(value.high_bytes, value.low_bytes)
    if isinstance(value, LdcString):
        return cp.find_string(value.value)
    if isinstance(value, LdcClass):
        return cp.find_class(value.name)
    if isinstance(value, LdcMethodType):
        return cp.find_method_type(value.descriptor)
    if isinstance(value, LdcMethodHandle):
        return _find_existing_ldc_method_handle(value, cp)
    return cp.find_dynamic(value.bootstrap_method_attr_index, value.name, value.descriptor)


def _lower_ldc_method_handle(value: LdcMethodHandle, cp: ConstantPoolBuilder) -> int:
    """Lower an ``LdcMethodHandle`` to a CONSTANT_MethodHandle CP index."""
    kind = value.reference_kind
    if kind in (1, 2, 3, 4):  # REF_getField, REF_getStatic, REF_putField, REF_putStatic
        ref_index = cp.add_fieldref(value.owner, value.name, value.descriptor)
    elif kind in (5, 8):  # REF_invokeVirtual, REF_newInvokeSpecial → always Methodref
        ref_index = cp.add_methodref(value.owner, value.name, value.descriptor)
    elif kind == 9:  # REF_invokeInterface → always InterfaceMethodref
        ref_index = cp.add_interface_methodref(value.owner, value.name, value.descriptor)
    elif kind in (6, 7):  # REF_invokeStatic, REF_invokeSpecial → depends on is_interface
        if value.is_interface:
            ref_index = cp.add_interface_methodref(value.owner, value.name, value.descriptor)
        else:
            ref_index = cp.add_methodref(value.owner, value.name, value.descriptor)
    else:
        raise ValueError(f"invalid MethodHandle reference_kind: {kind}")
    return cp.add_method_handle(kind, ref_index)


def _find_existing_ldc_method_handle(value: LdcMethodHandle, cp: ConstantPoolBuilder) -> int | None:
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
