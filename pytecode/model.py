from __future__ import annotations

import copy
from dataclasses import dataclass, field
from typing import TYPE_CHECKING

from .attributes import (
    AttributeInfo,
    CodeAttr,
    LineNumberTableAttr,
    LocalVariableTableAttr,
    LocalVariableTypeTableAttr,
)
from .class_reader import ClassReader
from .class_writer import ClassWriter
from .constant_pool import (
    ClassInfo,
    DoubleInfo,
    DynamicInfo,
    FieldrefInfo,
    FloatInfo,
    IntegerInfo,
    InterfaceMethodrefInfo,
    InvokeDynamicInfo,
    LongInfo,
    MethodHandleInfo,
    MethodrefInfo,
    MethodTypeInfo,
    NameAndTypeInfo,
    StringInfo,
)
from .constant_pool_builder import ConstantPoolBuilder
from .constants import MAGIC, ClassAccessFlag, FieldAccessFlag, MethodAccessFlag
from .debug_info import (
    DebugInfoPolicy,
    DebugInfoState,
    is_class_debug_info_stale,
    normalize_debug_info_policy,
    skip_debug_method_attributes,
    strip_class_debug_attributes,
)
from .info import ClassFile, FieldInfo, MethodInfo
from .instructions import (
    Branch,
    BranchW,
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
    MultiANewArray,
    TableSwitch,
)
from .labels import (
    BranchInsn,
    CodeItem,
    ExceptionHandler,
    Label,
    LineNumberEntry,
    LocalVariableEntry,
    LocalVariableTypeEntry,
    LookupSwitchInsn,
    TableSwitchInsn,
    lower_code,
    resolve_catch_type,
)
from .operands import (
    _IMPLICIT_VAR_SLOTS,
    _WIDE_TO_BASE,
    FieldInsn,
    IIncInsn,
    InterfaceMethodInsn,
    InvokeDynamicInsn,
    LdcClass,
    LdcDouble,
    LdcDynamic,
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
)

if TYPE_CHECKING:
    from .hierarchy import ClassResolver


@dataclass
class CodeModel:
    """Mutable wrapper around a method's code body.

    Carries a mixed instruction stream of raw instructions plus ``Label``
    pseudo-instructions, symbolic exception/debug metadata, and stack/local
    limits from the parsed ``CodeAttr``.
    """

    max_stack: int
    max_locals: int
    instructions: list[CodeItem] = field(default_factory=list)
    exception_handlers: list[ExceptionHandler] = field(default_factory=list)
    line_numbers: list[LineNumberEntry] = field(default_factory=list)
    local_variables: list[LocalVariableEntry] = field(default_factory=list)
    local_variable_types: list[LocalVariableTypeEntry] = field(default_factory=list)
    attributes: list[AttributeInfo] = field(default_factory=list)
    _nested_attribute_layout: tuple[str, ...] = field(default_factory=tuple, repr=False, compare=False)
    debug_info_state: DebugInfoState = DebugInfoState.FRESH


@dataclass
class FieldModel:
    """Mutable representation of a class field with symbolic references."""

    access_flags: FieldAccessFlag
    name: str
    descriptor: str
    attributes: list[AttributeInfo]


@dataclass
class MethodModel:
    """Mutable representation of a class method with symbolic references.

    The ``code`` field is ``None`` for abstract and native methods.
    The ``attributes`` list contains non-Code attributes only; the Code
    attribute is lifted into the ``code`` field.
    """

    access_flags: MethodAccessFlag
    name: str
    descriptor: str
    code: CodeModel | None
    attributes: list[AttributeInfo]


@dataclass
class ClassModel:
    """Mutable editing model for a JVM class file.

    Fields use symbolic (resolved) references—plain strings for class
    names, field/method names, and descriptors—instead of raw
    constant-pool indexes.

    A ``ConstantPoolBuilder`` is carried so that raw attributes and
    instructions (which still contain CP indexes) remain valid.  The
    builder is seeded from the original constant pool during
    ``from_classfile`` and can be used to allocate new entries.
    """

    version: tuple[int, int]
    access_flags: ClassAccessFlag
    name: str
    super_name: str | None
    interfaces: list[str]
    fields: list[FieldModel]
    methods: list[MethodModel]
    attributes: list[AttributeInfo]
    constant_pool: ConstantPoolBuilder = field(default_factory=ConstantPoolBuilder)
    debug_info_state: DebugInfoState = DebugInfoState.FRESH

    # ------------------------------------------------------------------
    # Construction helpers
    # ------------------------------------------------------------------

    @classmethod
    def from_classfile(cls, cf: ClassFile, *, skip_debug: bool = False) -> ClassModel:
        """Build a ``ClassModel`` from a parsed ``ClassFile``.

        Resolves constant-pool indexes to symbolic string values and
        seeds the ``ConstantPoolBuilder`` from the original pool so that
        raw attributes and instructions remain valid. Pass ``skip_debug=True``
        to omit ASM-style debug metadata before it enters the mutable model.
        """
        cp = ConstantPoolBuilder.from_pool(cf.constant_pool)

        # Resolve this_class → class name string.
        this_class_entry = cp.get(cf.this_class)
        if not isinstance(this_class_entry, ClassInfo):
            raise ValueError(f"this_class CP index {cf.this_class} is not a CONSTANT_Class")
        name = cp.resolve_utf8(this_class_entry.name_index)

        # Resolve super_class → class name string or None.
        super_name: str | None = None
        if cf.super_class != 0:
            super_entry = cp.get(cf.super_class)
            if not isinstance(super_entry, ClassInfo):
                raise ValueError(f"super_class CP index {cf.super_class} is not a CONSTANT_Class")
            super_name = cp.resolve_utf8(super_entry.name_index)

        # Resolve interfaces.
        interfaces: list[str] = []
        for iface_index in cf.interfaces:
            iface_entry = cp.get(iface_index)
            if not isinstance(iface_entry, ClassInfo):
                raise ValueError(f"interface CP index {iface_index} is not a CONSTANT_Class")
            interfaces.append(cp.resolve_utf8(iface_entry.name_index))

        # Convert fields.
        fields = [_field_from_info(fi, cp) for fi in cf.fields]

        # Convert methods.
        methods = [_method_from_info(mi, cp, skip_debug=skip_debug) for mi in cf.methods]
        class_attributes = copy.deepcopy(cf.attributes)
        if skip_debug:
            class_attributes = strip_class_debug_attributes(class_attributes)

        return cls(
            version=(cf.major_version, cf.minor_version),
            access_flags=cf.access_flags,
            name=name,
            super_name=super_name,
            interfaces=interfaces,
            fields=fields,
            methods=methods,
            attributes=class_attributes,
            constant_pool=cp,
        )

    @classmethod
    def from_bytes(cls, data: bytes | bytearray, *, skip_debug: bool = False) -> ClassModel:
        """Parse raw class-file bytes and build a ``ClassModel``."""
        reader = ClassReader(data)
        return cls.from_classfile(reader.class_info, skip_debug=skip_debug)

    # ------------------------------------------------------------------
    # Lowering
    # ------------------------------------------------------------------

    def to_classfile(
        self,
        *,
        recompute_frames: bool = False,
        resolver: ClassResolver | None = None,
        debug_info: DebugInfoPolicy | str = DebugInfoPolicy.PRESERVE,
    ) -> ClassFile:
        """Lower this model back to a spec-faithful ``ClassFile``.

        Uses the ``ConstantPoolBuilder`` to allocate (or find) constant-pool
        entries for every symbolic reference and reassembles the raw
        ``FieldInfo``/``MethodInfo``/``CodeAttr`` structures.

        When *recompute_frames* is ``True``, ``max_stack``, ``max_locals``,
        and ``StackMapTable`` are recomputed for every method that has code.
        ``debug_info`` controls whether lifted code-debug tables and class-level
        source-debug attributes are preserved or stripped during lowering.
        """
        cp = self.constant_pool
        debug_policy = normalize_debug_info_policy(debug_info)

        # Allocate class-level CP entries.
        this_class_index = cp.add_class(self.name)
        super_class_index = cp.add_class(self.super_name) if self.super_name else 0
        interface_indexes = [cp.add_class(iface) for iface in self.interfaces]

        # Lower fields.
        raw_fields = [_field_to_info(fm, cp) for fm in self.fields]

        # Lower methods.
        raw_methods = [
            _method_to_info(
                mm,
                cp,
                class_name=self.name,
                recompute_frames=recompute_frames,
                resolver=resolver,
                debug_info=debug_policy,
            )
            for mm in self.methods
        ]

        pool = cp.build()
        class_attributes = self.attributes
        if debug_policy is DebugInfoPolicy.STRIP or is_class_debug_info_stale(self):
            class_attributes = strip_class_debug_attributes(class_attributes)

        return ClassFile(
            magic=MAGIC,
            minor_version=self.version[1],
            major_version=self.version[0],
            constant_pool_count=cp.count,
            constant_pool=pool,
            access_flags=self.access_flags,
            this_class=this_class_index,
            super_class=super_class_index,
            interfaces_count=len(interface_indexes),
            interfaces=interface_indexes,
            fields_count=len(raw_fields),
            fields=raw_fields,
            methods_count=len(raw_methods),
            methods=raw_methods,
            attributes_count=len(class_attributes),
            attributes=copy.deepcopy(class_attributes),
        )

    def to_bytes(
        self,
        *,
        recompute_frames: bool = False,
        resolver: ClassResolver | None = None,
        debug_info: DebugInfoPolicy | str = DebugInfoPolicy.PRESERVE,
    ) -> bytes:
        """Lower this model and serialize the resulting ``ClassFile`` to bytes."""
        return ClassWriter.write(
            self.to_classfile(
                recompute_frames=recompute_frames,
                resolver=resolver,
                debug_info=debug_info,
            )
        )


# ------------------------------------------------------------------
# Internal helpers
# ------------------------------------------------------------------


def _field_from_info(fi: FieldInfo, cp: ConstantPoolBuilder) -> FieldModel:
    return FieldModel(
        access_flags=fi.access_flags,
        name=cp.resolve_utf8(fi.name_index),
        descriptor=cp.resolve_utf8(fi.descriptor_index),
        attributes=copy.deepcopy(fi.attributes),
    )


def _method_from_info(mi: MethodInfo, cp: ConstantPoolBuilder, *, skip_debug: bool = False) -> MethodModel:
    code: CodeModel | None = None
    non_code_attrs: list[AttributeInfo] = []

    for attr in mi.attributes:
        if isinstance(attr, CodeAttr):
            code = _code_from_attr(attr, cp, skip_debug=skip_debug)
        else:
            non_code_attrs.append(copy.deepcopy(attr))
    if skip_debug:
        non_code_attrs = skip_debug_method_attributes(non_code_attrs)

    return MethodModel(
        access_flags=mi.access_flags,
        name=cp.resolve_utf8(mi.name_index),
        descriptor=cp.resolve_utf8(mi.descriptor_index),
        code=code,
        attributes=non_code_attrs,
    )


def _field_to_info(fm: FieldModel, cp: ConstantPoolBuilder) -> FieldInfo:
    return FieldInfo(
        access_flags=fm.access_flags,
        name_index=cp.add_utf8(fm.name),
        descriptor_index=cp.add_utf8(fm.descriptor),
        attributes_count=len(fm.attributes),
        attributes=copy.deepcopy(fm.attributes),
    )


def _method_to_info(
    mm: MethodModel,
    cp: ConstantPoolBuilder,
    *,
    class_name: str | None = None,
    recompute_frames: bool = False,
    resolver: ClassResolver | None = None,
    debug_info: DebugInfoPolicy = DebugInfoPolicy.PRESERVE,
) -> MethodInfo:
    attrs: list[AttributeInfo] = copy.deepcopy(mm.attributes)

    if mm.code is not None:
        code_attr = lower_code(
            mm.code,
            cp,
            method=mm if recompute_frames else None,
            class_name=class_name if recompute_frames else None,
            resolver=resolver,
            recompute_frames=recompute_frames,
            debug_info=debug_info,
        )
        attrs.insert(0, code_attr)

    return MethodInfo(
        access_flags=mm.access_flags,
        name_index=cp.add_utf8(mm.name),
        descriptor_index=cp.add_utf8(mm.descriptor),
        attributes_count=len(attrs),
        attributes=attrs,
    )


def _validate_code_offset(offset: int, code_length: int, *, context: str) -> int:
    if not 0 <= offset <= code_length:
        raise ValueError(f"{context} offset {offset} is outside code range [0, {code_length}]")
    return offset


def _label_for_offset(labels_by_offset: dict[int, Label], offset: int) -> Label:
    return labels_by_offset[offset]


def _branch_target_offset(insn: Branch | BranchW, code_length: int) -> int:
    return _validate_code_offset(
        insn.bytecode_offset + insn.offset,
        code_length,
        context=f"{insn.type.name} target",
    )


def _collect_labels(code_attr: CodeAttr, *, skip_debug: bool = False) -> dict[int, Label]:
    labels_by_offset: dict[int, Label] = {}

    def ensure_label(offset: int, *, context: str) -> None:
        validated = _validate_code_offset(offset, code_attr.code_length, context=context)
        labels_by_offset.setdefault(validated, Label(f"L{validated}"))

    for insn in code_attr.code:
        if isinstance(insn, (Branch, BranchW)):
            ensure_label(_branch_target_offset(insn, code_attr.code_length), context=f"{insn.type.name} target")
        elif isinstance(insn, LookupSwitch):
            ensure_label(
                _validate_code_offset(
                    insn.bytecode_offset + insn.default,
                    code_attr.code_length,
                    context="lookupswitch default target",
                ),
                context="lookupswitch default target",
            )
            for pair in insn.pairs:
                ensure_label(
                    _validate_code_offset(
                        insn.bytecode_offset + pair.offset,
                        code_attr.code_length,
                        context="lookupswitch case target",
                    ),
                    context="lookupswitch case target",
                )
        elif isinstance(insn, TableSwitch):
            ensure_label(
                _validate_code_offset(
                    insn.bytecode_offset + insn.default,
                    code_attr.code_length,
                    context="tableswitch default target",
                ),
                context="tableswitch default target",
            )
            for relative in insn.offsets:
                ensure_label(
                    _validate_code_offset(
                        insn.bytecode_offset + relative,
                        code_attr.code_length,
                        context="tableswitch case target",
                    ),
                    context="tableswitch case target",
                )

    for exception in code_attr.exception_table:
        ensure_label(exception.start_pc, context="exception handler start")
        ensure_label(exception.end_pc, context="exception handler end")
        ensure_label(exception.handler_pc, context="exception handler target")

    if skip_debug:
        return labels_by_offset

    for attribute in code_attr.attributes:
        if isinstance(attribute, LineNumberTableAttr):
            for entry in attribute.line_number_table:
                ensure_label(entry.start_pc, context="line number entry")
        elif isinstance(attribute, LocalVariableTableAttr):
            for entry in attribute.local_variable_table:
                ensure_label(entry.start_pc, context="local variable start")
                ensure_label(entry.start_pc + entry.length, context="local variable end")
        elif isinstance(attribute, LocalVariableTypeTableAttr):
            for entry in attribute.local_variable_type_table:
                ensure_label(entry.start_pc, context="local variable type start")
                ensure_label(entry.start_pc + entry.length, context="local variable type end")

    return labels_by_offset


def _lift_instruction(
    insn: InsnInfo,
    labels_by_offset: dict[int, Label],
    code_length: int,
    cp: ConstantPoolBuilder,
) -> CodeItem:
    if isinstance(insn, (Branch, BranchW)):
        return BranchInsn(
            insn.type,
            _label_for_offset(labels_by_offset, _branch_target_offset(insn, code_length)),
        )
    if isinstance(insn, LookupSwitch):
        return LookupSwitchInsn(
            _label_for_offset(
                labels_by_offset,
                _validate_code_offset(
                    insn.bytecode_offset + insn.default,
                    code_length,
                    context="lookupswitch default target",
                ),
            ),
            [
                (
                    pair.match,
                    _label_for_offset(
                        labels_by_offset,
                        _validate_code_offset(
                            insn.bytecode_offset + pair.offset,
                            code_length,
                            context="lookupswitch case target",
                        ),
                    ),
                )
                for pair in insn.pairs
            ],
        )
    if isinstance(insn, TableSwitch):
        return TableSwitchInsn(
            _label_for_offset(
                labels_by_offset,
                _validate_code_offset(
                    insn.bytecode_offset + insn.default,
                    code_length,
                    context="tableswitch default target",
                ),
            ),
            insn.low,
            insn.high,
            [
                _label_for_offset(
                    labels_by_offset,
                    _validate_code_offset(
                        insn.bytecode_offset + relative,
                        code_length,
                        context="tableswitch case target",
                    ),
                )
                for relative in insn.offsets
            ],
        )
    if isinstance(insn, IInc):
        return IIncInsn(insn.index, insn.value)
    if isinstance(insn, IIncW):
        return IIncInsn(insn.index, insn.value)
    if isinstance(insn, InvokeDynamic):
        return _lift_invoke_dynamic(insn, cp)
    if isinstance(insn, InvokeInterface):
        return _lift_invoke_interface(insn, cp)
    if isinstance(insn, MultiANewArray):
        return MultiANewArrayInsn(_resolve_class_name(cp, insn.index), insn.dimensions)
    if isinstance(insn, ConstPoolIndex):
        return _lift_const_pool_index(insn, cp)
    if isinstance(insn, LocalIndexW):
        # WIDE var opcodes: normalize to VarInsn with canonical base opcode.
        base = _WIDE_TO_BASE[insn.type]
        return VarInsn(base, insn.index)
    if isinstance(insn, LocalIndex):
        # LDC (0x12) uses a u1 CP index stored in LocalIndex.index.
        if insn.type == InsnInfoType.LDC:
            return LdcInsn(_resolve_ldc_value(cp, insn.index))
        return VarInsn(insn.type, insn.index)
    # Implicit slot variants: ILOAD_0 through ASTORE_3.
    implicit = _IMPLICIT_VAR_SLOTS.get(insn.type)
    if implicit is not None:
        base_opcode, slot = implicit
        return VarInsn(base_opcode, slot)
    return copy.deepcopy(insn)


def _lift_instructions(
    code_attr: CodeAttr,
    labels_by_offset: dict[int, Label],
    cp: ConstantPoolBuilder,
) -> list[CodeItem]:
    instructions: list[CodeItem] = []
    inserted_offsets: set[int] = set()

    for insn in code_attr.code:
        label = labels_by_offset.get(insn.bytecode_offset)
        if label is not None and insn.bytecode_offset not in inserted_offsets:
            instructions.append(label)
            inserted_offsets.add(insn.bytecode_offset)
        instructions.append(_lift_instruction(insn, labels_by_offset, code_attr.code_length, cp))

    end_label = labels_by_offset.get(code_attr.code_length)
    if end_label is not None and code_attr.code_length not in inserted_offsets:
        instructions.append(end_label)
        inserted_offsets.add(code_attr.code_length)

    missing_offsets = sorted(set(labels_by_offset) - inserted_offsets)
    if missing_offsets:
        raise ValueError(
            "labels refer to offsets that are not instruction boundaries: "
            + ", ".join(str(offset) for offset in missing_offsets)
        )

    return instructions


def _lift_exception_handlers(
    code_attr: CodeAttr,
    labels_by_offset: dict[int, Label],
    cp: ConstantPoolBuilder,
) -> list[ExceptionHandler]:
    return [
        ExceptionHandler(
            start=_label_for_offset(labels_by_offset, exception.start_pc),
            end=_label_for_offset(labels_by_offset, exception.end_pc),
            handler=_label_for_offset(labels_by_offset, exception.handler_pc),
            catch_type=resolve_catch_type(cp, exception.catch_type),
        )
        for exception in code_attr.exception_table
    ]


def _lift_nested_code_attributes(
    code_attr: CodeAttr,
    labels_by_offset: dict[int, Label],
    cp: ConstantPoolBuilder,
    *,
    skip_debug: bool = False,
) -> tuple[
    list[LineNumberEntry],
    list[LocalVariableEntry],
    list[LocalVariableTypeEntry],
    list[AttributeInfo],
    tuple[str, ...],
]:
    line_numbers: list[LineNumberEntry] = []
    local_variables: list[LocalVariableEntry] = []
    local_variable_types: list[LocalVariableTypeEntry] = []
    attributes: list[AttributeInfo] = []
    layout: list[str] = []

    for attribute in code_attr.attributes:
        if isinstance(attribute, LineNumberTableAttr):
            layout.append("line_numbers")
            if skip_debug:
                continue
            line_numbers.extend(
                LineNumberEntry(
                    label=_label_for_offset(labels_by_offset, entry.start_pc),
                    line_number=entry.line_number,
                )
                for entry in attribute.line_number_table
            )
        elif isinstance(attribute, LocalVariableTableAttr):
            layout.append("local_variables")
            if skip_debug:
                continue
            local_variables.extend(
                LocalVariableEntry(
                    start=_label_for_offset(labels_by_offset, entry.start_pc),
                    end=_label_for_offset(labels_by_offset, entry.start_pc + entry.length),
                    name=cp.resolve_utf8(entry.name_index),
                    descriptor=cp.resolve_utf8(entry.descriptor_index),
                    slot=entry.index,
                )
                for entry in attribute.local_variable_table
            )
        elif isinstance(attribute, LocalVariableTypeTableAttr):
            layout.append("local_variable_types")
            if skip_debug:
                continue
            local_variable_types.extend(
                LocalVariableTypeEntry(
                    start=_label_for_offset(labels_by_offset, entry.start_pc),
                    end=_label_for_offset(labels_by_offset, entry.start_pc + entry.length),
                    name=cp.resolve_utf8(entry.name_index),
                    signature=cp.resolve_utf8(entry.signature_index),
                    slot=entry.index,
                )
                for entry in attribute.local_variable_type_table
            )
        else:
            layout.append("other")
            attributes.append(copy.deepcopy(attribute))

    return line_numbers, local_variables, local_variable_types, attributes, tuple(layout)


def _code_from_attr(attr: CodeAttr, cp: ConstantPoolBuilder, *, skip_debug: bool = False) -> CodeModel:
    labels_by_offset = _collect_labels(attr, skip_debug=skip_debug)
    line_numbers, local_variables, local_variable_types, attributes, layout = _lift_nested_code_attributes(
        attr,
        labels_by_offset,
        cp,
        skip_debug=skip_debug,
    )
    return CodeModel(
        max_stack=attr.max_stacks,
        max_locals=attr.max_locals,
        instructions=_lift_instructions(attr, labels_by_offset, cp),
        exception_handlers=_lift_exception_handlers(attr, labels_by_offset, cp),
        line_numbers=line_numbers,
        local_variables=local_variables,
        local_variable_types=local_variable_types,
        attributes=attributes,
        _nested_attribute_layout=layout,
    )


# ---------------------------------------------------------------------------
# CP resolution helpers (lifting direction)
# ---------------------------------------------------------------------------


def _resolve_class_name(cp: ConstantPoolBuilder, index: int) -> str:
    """Resolve a CONSTANT_Class CP index to its internal name string."""
    entry = cp.get(index)
    if not isinstance(entry, ClassInfo):
        raise ValueError(f"CP index {index} is not a CONSTANT_Class: {type(entry).__name__}")
    return cp.resolve_utf8(entry.name_index)


def _resolve_member_ref(
    cp: ConstantPoolBuilder,
    index: int,
) -> tuple[str, str, str, bool]:
    """Resolve a Fieldref/Methodref/InterfaceMethodref to (owner, name, descriptor, is_interface)."""
    entry = cp.get(index)
    if not isinstance(entry, (FieldrefInfo, MethodrefInfo, InterfaceMethodrefInfo)):
        raise ValueError(f"CP index {index} is not a member ref entry: {type(entry).__name__}")
    is_interface = isinstance(entry, InterfaceMethodrefInfo)
    owner = _resolve_class_name(cp, entry.class_index)
    nat = cp.get(entry.name_and_type_index)
    if not isinstance(nat, NameAndTypeInfo):
        raise ValueError(f"CP index {entry.name_and_type_index} is not a CONSTANT_NameAndType")
    name = cp.resolve_utf8(nat.name_index)
    descriptor = cp.resolve_utf8(nat.descriptor_index)
    return owner, name, descriptor, is_interface


def _resolve_ldc_value(cp: ConstantPoolBuilder, index: int) -> LdcValue:
    """Resolve an LDC/LDC_W/LDC2_W CP index to a typed ``LdcValue``."""
    entry = cp.get(index)
    if isinstance(entry, IntegerInfo):
        return LdcInt(entry.value_bytes)
    if isinstance(entry, FloatInfo):
        return LdcFloat(entry.value_bytes)
    if isinstance(entry, LongInfo):
        unsigned = (entry.high_bytes << 32) | (entry.low_bytes & 0xFFFFFFFF)
        value = unsigned - (1 << 64) if unsigned >= (1 << 63) else unsigned
        return LdcLong(value)
    if isinstance(entry, DoubleInfo):
        return LdcDouble(entry.high_bytes, entry.low_bytes)
    if isinstance(entry, StringInfo):
        return LdcString(cp.resolve_utf8(entry.string_index))
    if isinstance(entry, ClassInfo):
        return LdcClass(cp.resolve_utf8(entry.name_index))
    if isinstance(entry, MethodTypeInfo):
        return LdcMethodType(cp.resolve_utf8(entry.descriptor_index))
    if isinstance(entry, MethodHandleInfo):
        return _resolve_ldc_method_handle(cp, entry)
    if isinstance(entry, DynamicInfo):
        nat = cp.get(entry.name_and_type_index)
        if not isinstance(nat, NameAndTypeInfo):
            raise ValueError(f"CP index {entry.name_and_type_index} is not a CONSTANT_NameAndType")
        return LdcDynamic(
            entry.bootstrap_method_attr_index,
            cp.resolve_utf8(nat.name_index),
            cp.resolve_utf8(nat.descriptor_index),
        )
    raise ValueError(f"CP index {index} has unsupported type for LDC: {type(entry).__name__}")


def _resolve_ldc_method_handle(cp: ConstantPoolBuilder, entry: MethodHandleInfo) -> LdcMethodHandle:
    """Resolve a CONSTANT_MethodHandle entry to an ``LdcMethodHandle``."""
    ref_entry = cp.get(entry.reference_index)
    if not isinstance(ref_entry, (FieldrefInfo, MethodrefInfo, InterfaceMethodrefInfo)):
        raise ValueError(
            f"MethodHandle reference index {entry.reference_index} has unexpected type: {type(ref_entry).__name__}"
        )
    is_interface = isinstance(ref_entry, InterfaceMethodrefInfo)
    owner = _resolve_class_name(cp, ref_entry.class_index)
    nat = cp.get(ref_entry.name_and_type_index)
    if not isinstance(nat, NameAndTypeInfo):
        raise ValueError(f"CP index {ref_entry.name_and_type_index} is not a CONSTANT_NameAndType")
    name = cp.resolve_utf8(nat.name_index)
    descriptor = cp.resolve_utf8(nat.descriptor_index)
    return LdcMethodHandle(entry.reference_kind, owner, name, descriptor, is_interface)


def _lift_const_pool_index(insn: ConstPoolIndex, cp: ConstantPoolBuilder) -> CodeItem:
    """Lift a raw ``ConstPoolIndex`` instruction to a symbolic wrapper."""
    opcode = insn.type
    if opcode in (
        InsnInfoType.GETFIELD,
        InsnInfoType.PUTFIELD,
        InsnInfoType.GETSTATIC,
        InsnInfoType.PUTSTATIC,
    ):
        owner, name, descriptor, _ = _resolve_member_ref(cp, insn.index)
        return FieldInsn(opcode, owner, name, descriptor)
    if opcode in (
        InsnInfoType.INVOKEVIRTUAL,
        InsnInfoType.INVOKESPECIAL,
        InsnInfoType.INVOKESTATIC,
    ):
        owner, name, descriptor, is_interface = _resolve_member_ref(cp, insn.index)
        return MethodInsn(opcode, owner, name, descriptor, is_interface)
    if opcode in (
        InsnInfoType.NEW,
        InsnInfoType.CHECKCAST,
        InsnInfoType.INSTANCEOF,
        InsnInfoType.ANEWARRAY,
    ):
        return TypeInsn(opcode, _resolve_class_name(cp, insn.index))
    if opcode in (InsnInfoType.LDC_W, InsnInfoType.LDC2_W):
        return LdcInsn(_resolve_ldc_value(cp, insn.index))
    # Unknown ConstPoolIndex opcode: pass through unchanged (e.g. future opcodes).
    return copy.deepcopy(insn)


def _lift_invoke_dynamic(insn: InvokeDynamic, cp: ConstantPoolBuilder) -> InvokeDynamicInsn:
    """Lift a raw ``InvokeDynamic`` instruction to an ``InvokeDynamicInsn``."""
    entry = cp.get(insn.index)
    if not isinstance(entry, InvokeDynamicInfo):
        raise ValueError(f"CP index {insn.index} is not a CONSTANT_InvokeDynamic: {type(entry).__name__}")
    nat = cp.get(entry.name_and_type_index)
    if not isinstance(nat, NameAndTypeInfo):
        raise ValueError(f"CP index {entry.name_and_type_index} is not a CONSTANT_NameAndType")
    return InvokeDynamicInsn(
        entry.bootstrap_method_attr_index,
        cp.resolve_utf8(nat.name_index),
        cp.resolve_utf8(nat.descriptor_index),
    )


def _lift_invoke_interface(insn: InvokeInterface, cp: ConstantPoolBuilder) -> InterfaceMethodInsn:
    """Lift a raw ``InvokeInterface`` instruction to an ``InterfaceMethodInsn``."""
    owner, name, descriptor, _ = _resolve_member_ref(cp, insn.index)
    return InterfaceMethodInsn(owner, name, descriptor)
