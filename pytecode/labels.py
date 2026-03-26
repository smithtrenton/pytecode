from __future__ import annotations

import copy
from dataclasses import dataclass
from typing import TYPE_CHECKING

from .attributes import (
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
from .constant_pool import ClassInfo
from .constant_pool_builder import ConstantPoolBuilder
from .instructions import (
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

if TYPE_CHECKING:
    from .model import CodeModel

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
    """Identity-based marker for a bytecode position."""

    name: str | None = None

    def __repr__(self) -> str:
        if self.name is not None:
            return f"Label({self.name!r})"
        return f"Label(id=0x{id(self):x})"


type CodeItem = InsnInfo | Label


@dataclass
class ExceptionHandler:
    start: Label
    end: Label
    handler: Label
    catch_type: str | None


@dataclass
class LineNumberEntry:
    label: Label
    line_number: int


@dataclass
class LocalVariableEntry:
    start: Label
    end: Label
    name: str
    descriptor: str
    slot: int


@dataclass
class LocalVariableTypeEntry:
    start: Label
    end: Label
    name: str
    signature: str
    slot: int


@dataclass(init=False)
class BranchInsn(InsnInfo):
    """Editing-model branch instruction that targets a label."""

    target: Label

    def __init__(self, insn_type: InsnInfoType, target: Label, bytecode_offset: int = -1) -> None:
        if insn_type.instinfo not in {Branch, BranchW}:
            raise ValueError(f"{insn_type.name} is not a branch opcode")
        super().__init__(insn_type, bytecode_offset)
        self.target = target


@dataclass(init=False)
class LookupSwitchInsn(InsnInfo):
    """Editing-model LOOKUPSWITCH that targets labels."""

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
    """Editing-model TABLESWITCH that targets labels."""

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


def _clone_code_item(item: CodeItem) -> CodeItem:
    return item if isinstance(item, Label) else copy.copy(item)


def _lifted_debug_attrs(attributes: list[AttributeInfo]) -> list[AttributeInfo]:
    return [
        attribute
        for attribute in attributes
        if not isinstance(
            attribute,
            (LineNumberTableAttr, LocalVariableTableAttr, LocalVariableTypeTableAttr),
        )
    ]


def _instruction_byte_size(insn: CodeItem, offset: int) -> int:
    if isinstance(insn, Label):
        return 0
    if isinstance(insn, BranchInsn):
        return 5 if insn.type.instinfo is BranchW else 3
    if isinstance(insn, LookupSwitchInsn):
        return 1 + _switch_padding(offset) + 8 + (8 * len(insn.pairs))
    if isinstance(insn, TableSwitchInsn):
        return 1 + _switch_padding(offset) + 12 + (4 * len(insn.targets))
    if isinstance(insn, LocalIndex):
        return 2
    if isinstance(insn, LocalIndexW):
        return 4
    if isinstance(insn, ConstPoolIndex):
        return 3
    if isinstance(insn, ByteValue):
        return 2
    if isinstance(insn, ShortValue):
        return 3
    if isinstance(insn, Branch):
        return 3
    if isinstance(insn, BranchW):
        return 5
    if isinstance(insn, IInc):
        return 3
    if isinstance(insn, IIncW):
        return 6
    if isinstance(insn, InvokeDynamic):
        return 5
    if isinstance(insn, InvokeInterface):
        return 5
    if isinstance(insn, NewArray):
        return 2
    if isinstance(insn, MultiANewArray):
        return 4
    if isinstance(insn, LookupSwitch):
        return 1 + _switch_padding(offset) + 8 + (8 * len(insn.pairs))
    if isinstance(insn, TableSwitch):
        return 1 + _switch_padding(offset) + 12 + (4 * len(insn.offsets))
    return 1


def resolve_labels(items: list[CodeItem]) -> LabelResolution:
    """Resolve label and instruction offsets for a mixed instruction stream."""

    label_offsets: dict[Label, int] = {}
    instruction_offsets: list[int] = []
    offset = 0

    for item in items:
        instruction_offsets.append(offset)
        if isinstance(item, Label):
            if item in label_offsets:
                raise ValueError(f"label {item!r} appears multiple times in CodeModel.instructions")
            label_offsets[item] = offset
            continue
        offset += _instruction_byte_size(item, offset)

    return LabelResolution(
        label_offsets=label_offsets,
        instruction_offsets=instruction_offsets,
        total_code_length=offset,
    )


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


def _lower_instruction(item: CodeItem, offset: int, label_offsets: dict[Label, int]) -> InsnInfo | None:
    if isinstance(item, Label):
        return None

    if isinstance(item, BranchInsn):
        relative = _relative_offset(offset, item.target, label_offsets, context=f"{item.type.name} target")
        if item.type.instinfo is BranchW:
            if not _fits_i4(relative):
                raise ValueError(f"{item.type.name} branch offset {relative} exceeds JVM i4 range")
            return BranchW(item.type, offset, relative)
        if not _fits_i2(relative):
            raise ValueError(f"{item.type.name} branch offset {relative} exceeds JVM i2 range")
        return Branch(item.type, offset, relative)

    if isinstance(item, LookupSwitchInsn):
        default = _relative_offset(
            offset,
            item.default_target,
            label_offsets,
            context="lookupswitch default target",
        )
        pairs = [
            MatchOffsetPair(
                match,
                _relative_offset(offset, target, label_offsets, context="lookupswitch case target"),
            )
            for match, target in item.pairs
        ]
        return LookupSwitch(item.type, offset, default, len(pairs), pairs)

    if isinstance(item, TableSwitchInsn):
        default = _relative_offset(
            offset,
            item.default_target,
            label_offsets,
            context="tableswitch default target",
        )
        offsets = [
            _relative_offset(offset, target, label_offsets, context="tableswitch case target")
            for target in item.targets
        ]
        return TableSwitch(item.type, offset, default, item.low, item.high, offsets)

    lowered = copy.deepcopy(item)
    lowered.bytecode_offset = offset
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


def lower_code(code: CodeModel, cp: ConstantPoolBuilder) -> CodeAttr:
    """Lower a label-based ``CodeModel`` into a raw ``CodeAttr``."""

    items = [_clone_code_item(item) for item in code.instructions]

    while True:
        resolution = resolve_labels(items)
        if resolution.total_code_length > 65535:
            raise ValueError(f"code length {resolution.total_code_length} exceeds JVM maximum of 65535 bytes")
        if not _promote_overflow_branches(items, resolution):
            break

    resolution = resolve_labels(items)
    if resolution.total_code_length > 65535:
        raise ValueError(f"code length {resolution.total_code_length} exceeds JVM maximum of 65535 bytes")

    lowered_code = [
        lowered
        for item, offset in zip(items, resolution.instruction_offsets, strict=True)
        if (lowered := _lower_instruction(item, offset, resolution.label_offsets)) is not None
    ]
    exception_table = _lower_exception_handlers(code.exception_handlers, resolution.label_offsets, cp)

    attributes = copy.deepcopy(_lifted_debug_attrs(code.attributes))
    for debug_attr in (
        _build_line_number_attribute(code.line_numbers, resolution.label_offsets, cp),
        _build_local_variable_attribute(code.local_variables, resolution.label_offsets, cp),
        _build_local_variable_type_attribute(code.local_variable_types, resolution.label_offsets, cp),
    ):
        if debug_attr is not None:
            attributes.append(debug_attr)

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


def resolve_catch_type(cp: ConstantPoolBuilder, catch_type_index: int) -> str | None:
    """Resolve an exception handler catch type index to a class name."""

    if catch_type_index == 0:
        return None

    entry = cp.get(catch_type_index)
    if not isinstance(entry, ClassInfo):
        raise ValueError(f"catch_type CP index {catch_type_index} is not a CONSTANT_Class")
    return cp.resolve_utf8(entry.name_index)
