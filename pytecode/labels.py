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
from .debug_info import DebugInfoPolicy, is_code_debug_info_stale, normalize_debug_info_policy
from .descriptors import parameter_slot_count, parse_method_descriptor
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
    from .hierarchy import ClassResolver
    from .model import CodeModel, MethodModel

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


def _refresh_code_attr_metadata(code_attr: CodeAttr) -> None:
    code_attr.attributes_count = len(code_attr.attributes)
    code_attr.attribute_length = _code_attribute_length(
        code_attr.code_length,
        code_attr.exception_table_length,
        code_attr.attributes,
    )


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


def _ordered_nested_code_attributes(
    code: CodeModel,
    line_number_attr: LineNumberTableAttr | None,
    local_variable_attr: LocalVariableTableAttr | None,
    local_variable_type_attr: LocalVariableTypeTableAttr | None,
) -> list[AttributeInfo]:
    other_attrs = copy.deepcopy(_lifted_debug_attrs(code.attributes))
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


def _clone_constant_pool_builder(cp: ConstantPoolBuilder) -> ConstantPoolBuilder:
    return ConstantPoolBuilder.from_pool(cp.build())


def _needs_ldc_index_cache(items: list[CodeItem]) -> bool:
    return any(isinstance(item, LdcInsn) and not isinstance(item.value, (LdcLong, LdcDouble)) for item in items)


def _build_ldc_index_cache(items: list[CodeItem], cp: ConstantPoolBuilder) -> dict[int, int]:
    probe_cp = _clone_constant_pool_builder(cp)
    return {id(item): _lower_ldc_value(item.value, probe_cp) for item in items if isinstance(item, LdcInsn)}


def _instruction_byte_size(
    insn: CodeItem,
    offset: int,
    ldc_index_cache: dict[int, int] | None = None,
) -> int:
    if isinstance(insn, Label):
        return 0
    if isinstance(insn, BranchInsn):
        return 5 if insn.type.instinfo is BranchW else 3
    if isinstance(insn, LookupSwitchInsn):
        return 1 + _switch_padding(offset) + 8 + (8 * len(insn.pairs))
    if isinstance(insn, TableSwitchInsn):
        return 1 + _switch_padding(offset) + 12 + (4 * len(insn.targets))
    # Symbolic operand wrappers (operands.py)
    if isinstance(insn, (FieldInsn, MethodInsn, TypeInsn)):
        return 3  # opcode(1) + u2 CP index
    if isinstance(insn, InterfaceMethodInsn):
        return 5  # opcode(1) + u2 CP index + u1 count + u1 zero
    if isinstance(insn, InvokeDynamicInsn):
        return 5  # opcode(1) + u2 CP index + u2 zero
    if isinstance(insn, MultiANewArrayInsn):
        return 4  # opcode(1) + u2 CP index + u1 dimensions
    if isinstance(insn, LdcInsn):
        if isinstance(insn.value, (LdcLong, LdcDouble)):
            return 3  # LDC2_W: opcode(1) + u2 CP index
        if ldc_index_cache is None:
            raise ValueError("LdcInsn size requires constant-pool context")
        idx = ldc_index_cache.get(id(insn))
        if idx is None:
            raise ValueError("LdcInsn is missing from the LDC index cache")
        return 2 if idx <= 255 else 3  # LDC: 2, LDC_W: 3
    if isinstance(insn, VarInsn):
        slot = _require_u2(insn.slot, context="local variable slot")
        if _VAR_SHORTCUTS.get((insn.type, slot)) is not None:
            return 1  # implicit form (e.g. ILOAD_0)
        if slot <= 255:
            return 2  # opcode(1) + u1 slot
        return 4  # WIDE(1) + opcode(1) + u2 slot
    if isinstance(insn, IIncInsn):
        slot = _require_u2(insn.slot, context="local variable slot")
        increment = _require_i2(insn.increment, context="iinc increment")
        if slot <= 255 and -128 <= increment <= 127:
            return 3  # IINC(1) + u1 slot + i1 increment
        return 6  # WIDE(1) + IINC(1) + u2 slot + i2 increment
    # Raw spec-model types
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


def resolve_labels(
    items: list[CodeItem],
    cp: ConstantPoolBuilder | None = None,
) -> LabelResolution:
    """Resolve label and instruction offsets for a mixed instruction stream.

    When the stream contains single-slot ``LdcInsn`` values, pass the current
    ``ConstantPoolBuilder`` so their byte size can be resolved exactly without
    mutating the live pool.
    """

    ldc_index_cache: dict[int, int] | None = None
    if _needs_ldc_index_cache(items):
        if cp is None:
            raise ValueError(
                "resolve_labels() requires a ConstantPoolBuilder when instructions contain single-slot LdcInsn values"
            )
        ldc_index_cache = _build_ldc_index_cache(items, cp)

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
        offset += _instruction_byte_size(item, offset, ldc_index_cache)

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


def _lower_instruction(
    item: CodeItem,
    offset: int,
    label_offsets: dict[Label, int],
    cp: ConstantPoolBuilder,
) -> InsnInfo | None:
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

    # Symbolic operand wrappers from operands.py
    if isinstance(item, FieldInsn):
        cp_index = cp.add_fieldref(item.owner, item.name, item.descriptor)
        return ConstPoolIndex(item.type, offset, cp_index)

    if isinstance(item, MethodInsn):
        if item.is_interface:
            cp_index = cp.add_interface_methodref(item.owner, item.name, item.descriptor)
        else:
            cp_index = cp.add_methodref(item.owner, item.name, item.descriptor)
        return ConstPoolIndex(item.type, offset, cp_index)

    if isinstance(item, InterfaceMethodInsn):
        cp_index = cp.add_interface_methodref(item.owner, item.name, item.descriptor)
        desc = parse_method_descriptor(item.descriptor)
        count = parameter_slot_count(desc) + 1  # +1 for the object reference
        return InvokeInterface(InsnInfoType.INVOKEINTERFACE, offset, cp_index, count, b"\x00")

    if isinstance(item, TypeInsn):
        cp_index = cp.add_class(item.class_name)
        return ConstPoolIndex(item.type, offset, cp_index)

    if isinstance(item, LdcInsn):
        cp_index = _lower_ldc_value(item.value, cp)
        if isinstance(item.value, (LdcLong, LdcDouble)):
            return ConstPoolIndex(InsnInfoType.LDC2_W, offset, cp_index)
        if cp_index <= 255:
            return LocalIndex(InsnInfoType.LDC, offset, cp_index)
        return ConstPoolIndex(InsnInfoType.LDC_W, offset, cp_index)

    if isinstance(item, VarInsn):
        slot = _require_u2(item.slot, context="local variable slot")
        shortcut = _VAR_SHORTCUTS.get((item.type, slot))
        if shortcut is not None:
            return InsnInfo(shortcut, offset)
        if slot > 255:
            wide_type = _BASE_TO_WIDE[item.type]
            return LocalIndexW(wide_type, offset, slot)
        return LocalIndex(item.type, offset, slot)

    if isinstance(item, IIncInsn):
        slot = _require_u2(item.slot, context="local variable slot")
        increment = _require_i2(item.increment, context="iinc increment")
        if slot <= 255 and -128 <= increment <= 127:
            return IInc(InsnInfoType.IINC, offset, slot, increment)
        return IIncW(InsnInfoType.IINCW, offset, slot, increment)

    if isinstance(item, InvokeDynamicInsn):
        bootstrap_method_attr_index = _require_u2(
            item.bootstrap_method_attr_index,
            context="bootstrap_method_attr_index",
        )
        cp_index = cp.add_invoke_dynamic(bootstrap_method_attr_index, item.name, item.descriptor)
        return InvokeDynamic(InsnInfoType.INVOKEDYNAMIC, offset, cp_index, b"\x00\x00")

    if isinstance(item, MultiANewArrayInsn):
        dimensions = _require_u1(
            item.dimensions,
            context="multianewarray dimensions",
            minimum=1,
        )
        cp_index = cp.add_class(item.class_name)
        return MultiANewArray(InsnInfoType.MULTIANEWARRAY, offset, cp_index, dimensions)

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


def _lower_resolved_code(
    code: CodeModel,
    items: list[CodeItem],
    resolution: LabelResolution,
    cp: ConstantPoolBuilder,
    keep_debug_info: bool,
) -> CodeAttr:
    lowered_code = [
        lowered
        for item, offset in zip(items, resolution.instruction_offsets, strict=True)
        if (lowered := _lower_instruction(item, offset, resolution.label_offsets, cp)) is not None
    ]
    exception_table = _lower_exception_handlers(code.exception_handlers, resolution.label_offsets, cp)

    line_number_attr = None
    local_variable_attr = None
    local_variable_type_attr = None
    if keep_debug_info:
        line_number_attr = _build_line_number_attribute(code.line_numbers, resolution.label_offsets, cp)
        local_variable_attr = _build_local_variable_attribute(code.local_variables, resolution.label_offsets, cp)
        local_variable_type_attr = _build_local_variable_type_attribute(
            code.local_variable_types,
            resolution.label_offsets,
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

    When *recompute_frames* is ``True``, the frame computation pipeline
    runs automatically: ``max_stack`` and ``max_locals`` are recomputed
    via stack simulation, and a fresh ``StackMapTable`` attribute is
    generated and attached to the returned ``CodeAttr``.  This requires
    *method* and *class_name* to be provided. ``debug_info`` controls
    whether lifted LineNumberTable/LocalVariableTable/LocalVariableTypeTable
    entries are preserved or stripped during lowering. Explicitly stale
    code-debug metadata is stripped automatically even when
    ``debug_info="preserve"``.
    """
    if recompute_frames and (method is None or class_name is None):
        raise ValueError("method and class_name are required when recompute_frames=True")

    debug_policy = normalize_debug_info_policy(debug_info)
    keep_debug_info = debug_policy is DebugInfoPolicy.PRESERVE and not is_code_debug_info_stale(code)
    items = [_clone_code_item(item) for item in code.instructions]

    while True:
        resolution = resolve_labels(items, cp)
        if resolution.total_code_length > 65535:
            raise ValueError(f"code length {resolution.total_code_length} exceeds JVM maximum of 65535 bytes")
        if not _promote_overflow_branches(items, resolution):
            break

    resolution = resolve_labels(items, cp)
    if resolution.total_code_length > 65535:
        raise ValueError(f"code length {resolution.total_code_length} exceeds JVM maximum of 65535 bytes")

    _lower_resolved_code(code, items, resolution, _clone_constant_pool_builder(cp), keep_debug_info)
    result = _lower_resolved_code(code, items, resolution, cp, keep_debug_info)

    if recompute_frames:
        assert method is not None and class_name is not None
        from .analysis import compute_frames
        from .attributes import StackMapTableAttr

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
    """Resolve an exception handler catch type index to a class name."""

    if catch_type_index == 0:
        return None

    entry = cp.get(catch_type_index)
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
