"""Mutable editing models for JVM class files.

Provides dataclass-based wrappers (``ClassModel``, ``MethodModel``,
``FieldModel``, ``CodeModel``) that resolve raw constant-pool indexes
into symbolic string references, making class-file content easy to
inspect and transform programmatically.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import TYPE_CHECKING, cast

__all__ = ["ClassModel", "CodeModel", "FieldModel", "MethodModel"]

from ..classfile.attributes import (
    AttributeInfo,
    CodeAttr,
    LineNumberTableAttr,
    LocalVariableTableAttr,
    LocalVariableTypeTableAttr,
)
from ..classfile.constant_pool import (
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
from ..classfile.constants import MAGIC, ClassAccessFlag, FieldAccessFlag, MethodAccessFlag
from ..classfile.info import ClassFile, FieldInfo, MethodInfo
from ..classfile.instructions import (
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
from ..classfile.reader import ClassReader
from ..classfile.writer import ClassWriter
from ._attribute_clone import clone_attribute, clone_attributes
from .constant_pool_builder import ConstantPoolBuilder
from .debug_info import (
    DebugInfoPolicy,
    DebugInfoState,
    is_class_debug_info_stale,
    normalize_debug_info_policy,
    skip_debug_method_attributes,
    strip_class_debug_attributes,
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
    clone_raw_instruction,
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
    from ..analysis.hierarchy import ClassResolver


_BRANCH_TARGET_CONTEXTS: dict[InsnInfoType, str] = {
    insn_type: f"{insn_type.name} target" for insn_type in InsnInfoType if insn_type.instinfo in (Branch, BranchW)
}
_trusted_branch_insn = BranchInsn._trusted
_trusted_lookup_switch_insn = LookupSwitchInsn._trusted
_trusted_table_switch_insn = TableSwitchInsn._trusted
_trusted_field_insn = FieldInsn._trusted
_trusted_method_insn = MethodInsn._trusted
_trusted_interface_method_insn = InterfaceMethodInsn._trusted
_trusted_invoke_dynamic_insn = InvokeDynamicInsn._trusted
_trusted_ldc_insn = LdcInsn._trusted
_trusted_multi_anew_array_insn = MultiANewArrayInsn._trusted
_trusted_type_insn = TypeInsn._trusted
_trusted_var_insn = VarInsn._trusted
_trusted_iinc_insn = IIncInsn._trusted


@dataclass
class CodeModel:
    """Mutable wrapper around a method's Code attribute (JVMS §4.7.3).

    Carries a mixed instruction stream of raw opcodes and ``Label``
    pseudo-instructions, symbolic exception/debug metadata, and
    stack/local limits lifted from the parsed ``CodeAttr``.

    Attributes:
        max_stack: Maximum operand-stack depth for this method.
        max_locals: Maximum number of local-variable slots.
        instructions: Interleaved opcodes and ``Label`` pseudo-instructions.
        exception_handlers: Symbolic exception-table entries.
        line_numbers: Line-number debug entries (may be empty).
        local_variables: LocalVariableTable debug entries (may be empty).
        local_variable_types: LocalVariableTypeTable debug entries
            (may be empty).
        attributes: Non-lifted Code sub-attributes (e.g. StackMapTable).
        debug_info_state: Tracks whether debug tables are fresh or stale.
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
    """Mutable wrapper around a class field (JVMS §4.5).

    Attributes:
        access_flags: Field access and property flags.
        name: Unqualified field name.
        descriptor: Field descriptor (e.g. ``I``, ``Ljava/lang/String;``).
        attributes: Raw field-level attributes (e.g. ConstantValue).
    """

    access_flags: FieldAccessFlag
    name: str
    descriptor: str
    attributes: list[AttributeInfo]


@dataclass
class MethodModel:
    """Mutable wrapper around a class method (JVMS §4.6).

    The ``code`` field is ``None`` for abstract and native methods.
    The ``attributes`` list holds non-Code attributes only; the Code
    attribute is lifted into the ``code`` field.

    Attributes:
        access_flags: Method access and property flags.
        name: Unqualified method name (or ``<init>``/``<clinit>``).
        descriptor: Method descriptor (e.g. ``(II)V``).
        code: Lifted Code attribute, or ``None`` for abstract/native methods.
        attributes: Non-Code method-level attributes (e.g. Exceptions).
    """

    access_flags: MethodAccessFlag
    name: str
    descriptor: str
    code: CodeModel | None
    attributes: list[AttributeInfo]


@dataclass
class ClassModel:
    """Mutable editing model for a JVM class file (JVMS §4.1).

    Fields use symbolic (resolved) references — plain strings for class
    names, field/method names, and descriptors — instead of raw
    constant-pool indexes.

    A ``ConstantPoolBuilder`` is carried so that raw attributes and
    instructions (which still contain CP indexes) remain valid.  The
    builder is seeded from the original constant pool during
    ``from_classfile`` and can be used to allocate new entries.

    Attributes:
        version: ``(major, minor)`` class-file version pair.
        access_flags: Class access and property flags.
        name: Internal class name (e.g. ``java/lang/Object``).
        super_name: Internal name of the superclass, or ``None`` for
            ``java/lang/Object`` itself.
        interfaces: Internal names of directly implemented interfaces.
        fields: Mutable field models.
        methods: Mutable method models.
        attributes: Class-level attributes (e.g. SourceFile, InnerClasses).
        constant_pool: Builder for allocating or resolving CP entries.
        debug_info_state: Tracks whether debug metadata is fresh or stale.
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
        raw attributes and instructions remain valid.

        Args:
            cf: Parsed class-file structure to lift.
            skip_debug: If true, strip debug metadata (line numbers,
                local-variable tables, source-file attributes) during
                lifting.

        Returns:
            A fully resolved ``ClassModel``.

        Raises:
            ValueError: If a constant-pool entry has an unexpected type.
        """
        cp = ConstantPoolBuilder.from_pool(cf.constant_pool)

        # Resolve this_class → class name string.
        this_class_entry = cp.peek(cf.this_class)
        if not isinstance(this_class_entry, ClassInfo):
            raise ValueError(f"this_class CP index {cf.this_class} is not a CONSTANT_Class")
        name = cp.resolve_utf8(this_class_entry.name_index)

        # Resolve super_class → class name string or None.
        super_name: str | None = None
        if cf.super_class != 0:
            super_entry = cp.peek(cf.super_class)
            if not isinstance(super_entry, ClassInfo):
                raise ValueError(f"super_class CP index {cf.super_class} is not a CONSTANT_Class")
            super_name = cp.resolve_utf8(super_entry.name_index)

        # Resolve interfaces.
        interfaces: list[str] = []
        for iface_index in cf.interfaces:
            iface_entry = cp.peek(iface_index)
            if not isinstance(iface_entry, ClassInfo):
                raise ValueError(f"interface CP index {iface_index} is not a CONSTANT_Class")
            interfaces.append(cp.resolve_utf8(iface_entry.name_index))

        # Convert fields.
        fields = [_field_from_info(fi, cp) for fi in cf.fields]

        # Reuse lifted CP-backed wrappers across methods in the same class.
        lifted_cp_item_cache: dict[tuple[InsnInfoType, int], CodeItem] = {}
        # Convert methods.
        methods = [
            _method_from_info(
                mi,
                cp,
                skip_debug=skip_debug,
                const_pool_item_cache=lifted_cp_item_cache,
            )
            for mi in cf.methods
        ]
        class_attributes = clone_attributes(cf.attributes)
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
        """Parse raw class-file bytes and build a ``ClassModel``.

        Args:
            data: Raw ``.class`` file content.
            skip_debug: If true, strip debug metadata during lifting.

        Returns:
            A fully resolved ``ClassModel``.
        """
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

        Args:
            recompute_frames: When true, recompute ``max_stack``,
                ``max_locals``, and ``StackMapTable`` for every method
                that has code.
            resolver: Class hierarchy resolver required when
                *recompute_frames* is true.
            debug_info: Controls whether lifted debug tables and
                class-level source-debug attributes are preserved or
                stripped during lowering.

        Returns:
            A fully assembled ``ClassFile`` ready for serialization.
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
            attributes=clone_attributes(class_attributes),
        )

    def to_bytes(
        self,
        *,
        recompute_frames: bool = False,
        resolver: ClassResolver | None = None,
        debug_info: DebugInfoPolicy | str = DebugInfoPolicy.PRESERVE,
    ) -> bytes:
        """Lower this model and serialize the resulting ``ClassFile`` to bytes.

        Args:
            recompute_frames: When true, recompute stack-map frames
                for every method that has code.
            resolver: Class hierarchy resolver required when
                *recompute_frames* is true.
            debug_info: Controls whether debug metadata is preserved
                or stripped during lowering.

        Returns:
            Raw ``.class`` file content.
        """
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
        attributes=clone_attributes(fi.attributes),
    )


def _method_from_info(
    mi: MethodInfo,
    cp: ConstantPoolBuilder,
    *,
    skip_debug: bool = False,
    const_pool_item_cache: dict[tuple[InsnInfoType, int], CodeItem] | None = None,
) -> MethodModel:
    code: CodeModel | None = None
    non_code_attrs: list[AttributeInfo] = []

    for attr in mi.attributes:
        if isinstance(attr, CodeAttr):
            code = _code_from_attr(
                attr,
                cp,
                skip_debug=skip_debug,
                const_pool_item_cache=const_pool_item_cache,
            )
        else:
            non_code_attrs.append(clone_attribute(attr))
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
        attributes=clone_attributes(fm.attributes),
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
    attrs: list[AttributeInfo] = clone_attributes(mm.attributes)

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


def _collect_labels(code_attr: CodeAttr, *, skip_debug: bool = False) -> dict[int, Label]:
    code_length = code_attr.code_length
    label_offsets: set[int] = set()
    add_label_offset = label_offsets.add

    for insn in code_attr.code:
        if isinstance(insn, (Branch, BranchW)):
            add_label_offset(
                _validate_code_offset(
                    insn.bytecode_offset + insn.offset,
                    code_length,
                    context=_BRANCH_TARGET_CONTEXTS[insn.type],
                )
            )
        elif isinstance(insn, LookupSwitch):
            add_label_offset(
                _validate_code_offset(
                    insn.bytecode_offset + insn.default,
                    code_length,
                    context="lookupswitch default target",
                )
            )
            for pair in insn.pairs:
                add_label_offset(
                    _validate_code_offset(
                        insn.bytecode_offset + pair.offset,
                        code_length,
                        context="lookupswitch case target",
                    )
                )
        elif isinstance(insn, TableSwitch):
            add_label_offset(
                _validate_code_offset(
                    insn.bytecode_offset + insn.default,
                    code_length,
                    context="tableswitch default target",
                )
            )
            for relative in insn.offsets:
                add_label_offset(
                    _validate_code_offset(
                        insn.bytecode_offset + relative,
                        code_length,
                        context="tableswitch case target",
                    )
                )

    for exception in code_attr.exception_table:
        add_label_offset(_validate_code_offset(exception.start_pc, code_length, context="exception handler start"))
        add_label_offset(_validate_code_offset(exception.end_pc, code_length, context="exception handler end"))
        add_label_offset(_validate_code_offset(exception.handler_pc, code_length, context="exception handler target"))

    if skip_debug:
        return {offset: Label(f"L{offset}") for offset in label_offsets}

    for attribute in code_attr.attributes:
        if isinstance(attribute, LineNumberTableAttr):
            for entry in attribute.line_number_table:
                add_label_offset(_validate_code_offset(entry.start_pc, code_length, context="line number entry"))
        elif isinstance(attribute, LocalVariableTableAttr):
            for entry in attribute.local_variable_table:
                add_label_offset(_validate_code_offset(entry.start_pc, code_length, context="local variable start"))
                add_label_offset(
                    _validate_code_offset(entry.start_pc + entry.length, code_length, context="local variable end")
                )
        elif isinstance(attribute, LocalVariableTypeTableAttr):
            for entry in attribute.local_variable_type_table:
                add_label_offset(
                    _validate_code_offset(entry.start_pc, code_length, context="local variable type start")
                )
                add_label_offset(
                    _validate_code_offset(
                        entry.start_pc + entry.length,
                        code_length,
                        context="local variable type end",
                    )
                )

    return {offset: Label(f"L{offset}") for offset in label_offsets}


def _lift_instruction(
    insn: InsnInfo,
    labels_by_offset: dict[int, Label],
    cp: ConstantPoolBuilder,
) -> CodeItem:
    insn_type = type(insn)
    if insn_type in (Branch, BranchW):
        branch_insn = cast(Branch | BranchW, insn)
        return _trusted_branch_insn(
            branch_insn.type,
            _label_for_offset(labels_by_offset, branch_insn.bytecode_offset + branch_insn.offset),
        )
    if insn_type is LookupSwitch:
        lookup_switch = cast(LookupSwitch, insn)
        return _trusted_lookup_switch_insn(
            _label_for_offset(
                labels_by_offset,
                lookup_switch.bytecode_offset + lookup_switch.default,
            ),
            [
                (
                    pair.match,
                    _label_for_offset(
                        labels_by_offset,
                        lookup_switch.bytecode_offset + pair.offset,
                    ),
                )
                for pair in lookup_switch.pairs
            ],
        )
    if insn_type is TableSwitch:
        table_switch = cast(TableSwitch, insn)
        return _trusted_table_switch_insn(
            _label_for_offset(
                labels_by_offset,
                table_switch.bytecode_offset + table_switch.default,
            ),
            table_switch.low,
            table_switch.high,
            [
                _label_for_offset(
                    labels_by_offset,
                    table_switch.bytecode_offset + relative,
                )
                for relative in table_switch.offsets
            ],
        )
    if insn_type in (IInc, IIncW):
        iinc_insn = cast(IInc | IIncW, insn)
        return _trusted_iinc_insn(iinc_insn.index, iinc_insn.value)
    if insn_type is InvokeDynamic:
        invoke_dynamic = cast(InvokeDynamic, insn)
        return _lift_invoke_dynamic(invoke_dynamic, cp)
    if insn_type is InvokeInterface:
        invoke_interface = cast(InvokeInterface, insn)
        return _lift_invoke_interface(invoke_interface, cp)
    if insn_type is MultiANewArray:
        multi_anew_array = cast(MultiANewArray, insn)
        return _trusted_multi_anew_array_insn(
            _resolve_class_name(cp, multi_anew_array.index),
            multi_anew_array.dimensions,
        )
    if insn_type is ConstPoolIndex:
        const_pool_insn = cast(ConstPoolIndex, insn)
        return _lift_const_pool_index(const_pool_insn, cp)
    if insn_type is LocalIndexW:
        local_index_w = cast(LocalIndexW, insn)
        # WIDE var opcodes: normalize to VarInsn with canonical base opcode.
        base = _WIDE_TO_BASE[local_index_w.type]
        return _trusted_var_insn(base, local_index_w.index)
    if insn_type is LocalIndex:
        local_index = cast(LocalIndex, insn)
        # LDC (0x12) uses a u1 CP index stored in LocalIndex.index.
        if local_index.type == InsnInfoType.LDC:
            return _trusted_ldc_insn(_resolve_ldc_value(cp, local_index.index))
        return _trusted_var_insn(local_index.type, local_index.index)
    # Implicit slot variants: ILOAD_0 through ASTORE_3.
    implicit = _IMPLICIT_VAR_SLOTS.get(insn.type)
    if implicit is not None:
        base_opcode, slot = implicit
        return _trusted_var_insn(base_opcode, slot)
    return clone_raw_instruction(insn)


def _lift_instructions(
    code_attr: CodeAttr,
    labels_by_offset: dict[int, Label],
    cp: ConstantPoolBuilder,
    const_pool_item_cache: dict[tuple[InsnInfoType, int], CodeItem] | None = None,
) -> list[CodeItem]:
    instructions: list[CodeItem] = []
    inserted_offsets: set[int] = set()
    if const_pool_item_cache is None:
        const_pool_item_cache = {}
    append = instructions.append
    inserted_add = inserted_offsets.add
    labels_get = labels_by_offset.get

    for insn in code_attr.code:
        label = labels_get(insn.bytecode_offset)
        if label is not None and insn.bytecode_offset not in inserted_offsets:
            append(label)
            inserted_add(insn.bytecode_offset)

        if type(insn) is ConstPoolIndex:
            const_pool_insn = insn
            cache_key = (const_pool_insn.type, const_pool_insn.index)
            cached = const_pool_item_cache.get(cache_key)
            if cached is None:
                cached = _lift_const_pool_index(const_pool_insn, cp)
                const_pool_item_cache[cache_key] = cached
            append(_clone_lifted_code_item(cached))
            continue

        append(_lift_instruction(insn, labels_by_offset, cp))

    end_label = labels_get(code_attr.code_length)
    if end_label is not None and code_attr.code_length not in inserted_offsets:
        append(end_label)
        inserted_add(code_attr.code_length)

    missing_offsets = sorted(set(labels_by_offset) - inserted_offsets)
    if missing_offsets:
        raise ValueError(
            "labels refer to offsets that are not instruction boundaries: "
            + ", ".join(str(offset) for offset in missing_offsets)
        )

    return instructions


def _clone_lifted_code_item(item: CodeItem) -> CodeItem:
    item_type = type(item)
    if item_type is FieldInsn:
        field_item = cast(FieldInsn, item)
        return _trusted_field_insn(field_item.type, field_item.owner, field_item.name, field_item.descriptor)
    if item_type is MethodInsn:
        method_item = cast(MethodInsn, item)
        return _trusted_method_insn(
            method_item.type,
            method_item.owner,
            method_item.name,
            method_item.descriptor,
            method_item.is_interface,
        )
    if item_type is TypeInsn:
        type_item = cast(TypeInsn, item)
        return _trusted_type_insn(type_item.type, type_item.class_name)
    if item_type is LdcInsn:
        ldc_item = cast(LdcInsn, item)
        return _trusted_ldc_insn(ldc_item.value)
    return clone_raw_instruction(cast(InsnInfo, item))


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
            attributes.append(clone_attribute(attribute))

    return line_numbers, local_variables, local_variable_types, attributes, tuple(layout)


def _code_from_attr(
    attr: CodeAttr,
    cp: ConstantPoolBuilder,
    *,
    skip_debug: bool = False,
    const_pool_item_cache: dict[tuple[InsnInfoType, int], CodeItem] | None = None,
) -> CodeModel:
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
        instructions=_lift_instructions(
            attr,
            labels_by_offset,
            cp,
            const_pool_item_cache,
        ),
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
    entry = cp.peek(index)
    if not isinstance(entry, ClassInfo):
        raise ValueError(f"CP index {index} is not a CONSTANT_Class: {type(entry).__name__}")
    return cp.resolve_utf8(entry.name_index)


def _resolve_member_ref(cp: ConstantPoolBuilder, index: int) -> tuple[str, str, str, bool]:
    """Resolve a Fieldref/Methodref/InterfaceMethodref to (owner, name, descriptor, is_interface)."""
    entry = cp.peek(index)
    if not isinstance(entry, (FieldrefInfo, MethodrefInfo, InterfaceMethodrefInfo)):
        raise ValueError(f"CP index {index} is not a member ref entry: {type(entry).__name__}")
    is_interface = isinstance(entry, InterfaceMethodrefInfo)
    owner = _resolve_class_name(cp, entry.class_index)
    nat = cp.peek(entry.name_and_type_index)
    if not isinstance(nat, NameAndTypeInfo):
        raise ValueError(f"CP index {entry.name_and_type_index} is not a CONSTANT_NameAndType")
    name = cp.resolve_utf8(nat.name_index)
    descriptor = cp.resolve_utf8(nat.descriptor_index)
    return owner, name, descriptor, is_interface


def _resolve_ldc_value(cp: ConstantPoolBuilder, index: int) -> LdcValue:
    """Resolve an LDC/LDC_W/LDC2_W CP index to a typed ``LdcValue``."""
    entry = cp.peek(index)
    if isinstance(entry, IntegerInfo):
        value: LdcValue = LdcInt(entry.value_bytes)
    elif isinstance(entry, FloatInfo):
        value = LdcFloat(entry.value_bytes)
    elif isinstance(entry, LongInfo):
        unsigned = (entry.high_bytes << 32) | (entry.low_bytes & 0xFFFFFFFF)
        long_value = unsigned - (1 << 64) if unsigned >= (1 << 63) else unsigned
        value = LdcLong(long_value)
    elif isinstance(entry, DoubleInfo):
        value = LdcDouble(entry.high_bytes, entry.low_bytes)
    elif isinstance(entry, StringInfo):
        value = LdcString(cp.resolve_utf8(entry.string_index))
    elif isinstance(entry, ClassInfo):
        value = LdcClass(cp.resolve_utf8(entry.name_index))
    elif isinstance(entry, MethodTypeInfo):
        value = LdcMethodType(cp.resolve_utf8(entry.descriptor_index))
    elif isinstance(entry, MethodHandleInfo):
        value = _resolve_ldc_method_handle(cp, entry)
    elif isinstance(entry, DynamicInfo):
        nat = cp.peek(entry.name_and_type_index)
        if not isinstance(nat, NameAndTypeInfo):
            raise ValueError(f"CP index {entry.name_and_type_index} is not a CONSTANT_NameAndType")
        value = LdcDynamic(
            entry.bootstrap_method_attr_index,
            cp.resolve_utf8(nat.name_index),
            cp.resolve_utf8(nat.descriptor_index),
        )
    else:
        raise ValueError(f"CP index {index} has unsupported type for LDC: {type(entry).__name__}")
    return value


def _resolve_ldc_method_handle(cp: ConstantPoolBuilder, entry: MethodHandleInfo) -> LdcMethodHandle:
    """Resolve a CONSTANT_MethodHandle entry to an ``LdcMethodHandle``."""
    ref_entry = cp.peek(entry.reference_index)
    if not isinstance(ref_entry, (FieldrefInfo, MethodrefInfo, InterfaceMethodrefInfo)):
        raise ValueError(
            f"MethodHandle reference index {entry.reference_index} has unexpected type: {type(ref_entry).__name__}"
        )
    is_interface = isinstance(ref_entry, InterfaceMethodrefInfo)
    owner = _resolve_class_name(cp, ref_entry.class_index)
    nat = cp.peek(ref_entry.name_and_type_index)
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
        return _trusted_field_insn(opcode, owner, name, descriptor)
    if opcode in (
        InsnInfoType.INVOKEVIRTUAL,
        InsnInfoType.INVOKESPECIAL,
        InsnInfoType.INVOKESTATIC,
    ):
        owner, name, descriptor, is_interface = _resolve_member_ref(cp, insn.index)
        return _trusted_method_insn(opcode, owner, name, descriptor, is_interface)
    if opcode in (
        InsnInfoType.NEW,
        InsnInfoType.CHECKCAST,
        InsnInfoType.INSTANCEOF,
        InsnInfoType.ANEWARRAY,
    ):
        return _trusted_type_insn(opcode, _resolve_class_name(cp, insn.index))
    if opcode in (InsnInfoType.LDC_W, InsnInfoType.LDC2_W):
        return _trusted_ldc_insn(_resolve_ldc_value(cp, insn.index))
    # Unknown ConstPoolIndex opcode: pass through unchanged (e.g. future opcodes).
    return clone_raw_instruction(insn)


def _lift_invoke_dynamic(insn: InvokeDynamic, cp: ConstantPoolBuilder) -> InvokeDynamicInsn:
    """Lift a raw ``InvokeDynamic`` instruction to an ``InvokeDynamicInsn``."""
    entry = cp.peek(insn.index)
    if not isinstance(entry, InvokeDynamicInfo):
        raise ValueError(f"CP index {insn.index} is not a CONSTANT_InvokeDynamic: {type(entry).__name__}")
    nat = cp.peek(entry.name_and_type_index)
    if not isinstance(nat, NameAndTypeInfo):
        raise ValueError(f"CP index {entry.name_and_type_index} is not a CONSTANT_NameAndType")
    return _trusted_invoke_dynamic_insn(
        entry.bootstrap_method_attr_index,
        cp.resolve_utf8(nat.name_index),
        cp.resolve_utf8(nat.descriptor_index),
    )


def _lift_invoke_interface(insn: InvokeInterface, cp: ConstantPoolBuilder) -> InterfaceMethodInsn:
    """Lift a raw ``InvokeInterface`` instruction to an ``InterfaceMethodInsn``."""
    owner, name, descriptor, _ = _resolve_member_ref(cp, insn.index)
    return _trusted_interface_method_insn(owner, name, descriptor)
