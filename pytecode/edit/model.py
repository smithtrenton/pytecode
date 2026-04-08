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

from ..classfile._rust_bridge import coerce_python_classfile
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
        cf = coerce_python_classfile(cf)
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

        # Convert methods.
        methods = [_method_from_info(mi, cp, skip_debug=skip_debug) for mi in cf.methods]
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

        Uses the Rust model layer when available for faster parsing,
        falling back to the pure-Python path otherwise.

        Args:
            data: Raw ``.class`` file content.
            skip_debug: If true, strip debug metadata during lifting.

        Returns:
            A fully resolved ``ClassModel``.
        """
        try:
            from .._rust import RustClassModel  # type: ignore[attr-defined]

            rust_model = RustClassModel.from_bytes(data)
            from ._rust_bridge_model import from_rust_model

            model = from_rust_model(rust_model, skip_debug=skip_debug)
            # Seed the CP builder from the original class bytes so that
            # raw attributes referencing CP indexes remain valid.
            reader = ClassReader.from_bytes(data)
            from ..classfile._rust_bridge import _convert_constant_pool_entry

            py_pool = [
                _convert_constant_pool_entry(entry) if entry is not None else None
                for entry in reader.class_info.constant_pool
            ]
            model.constant_pool = ConstantPoolBuilder.from_pool(py_pool)
            return model
        except ImportError, Exception:
            pass
        reader = ClassReader.from_bytes(data)
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


def _method_from_info(mi: MethodInfo, cp: ConstantPoolBuilder, *, skip_debug: bool = False) -> MethodModel:
    code: CodeModel | None = None
    non_code_attrs: list[AttributeInfo] = []

    for attr in mi.attributes:
        if isinstance(attr, CodeAttr):
            code = _code_from_attr(attr, cp, skip_debug=skip_debug)
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
    insn_type = type(insn)
    if insn_type in (Branch, BranchW):
        branch_insn = cast(Branch | BranchW, insn)
        return BranchInsn(
            branch_insn.type,
            _label_for_offset(labels_by_offset, _branch_target_offset(branch_insn, code_length)),
        )
    if insn_type is LookupSwitch:
        lookup_switch = cast(LookupSwitch, insn)
        return LookupSwitchInsn(
            _label_for_offset(
                labels_by_offset,
                _validate_code_offset(
                    lookup_switch.bytecode_offset + lookup_switch.default,
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
                            lookup_switch.bytecode_offset + pair.offset,
                            code_length,
                            context="lookupswitch case target",
                        ),
                    ),
                )
                for pair in lookup_switch.pairs
            ],
        )
    if insn_type is TableSwitch:
        table_switch = cast(TableSwitch, insn)
        return TableSwitchInsn(
            _label_for_offset(
                labels_by_offset,
                _validate_code_offset(
                    table_switch.bytecode_offset + table_switch.default,
                    code_length,
                    context="tableswitch default target",
                ),
            ),
            table_switch.low,
            table_switch.high,
            [
                _label_for_offset(
                    labels_by_offset,
                    _validate_code_offset(
                        table_switch.bytecode_offset + relative,
                        code_length,
                        context="tableswitch case target",
                    ),
                )
                for relative in table_switch.offsets
            ],
        )
    if insn_type in (IInc, IIncW):
        iinc_insn = cast(IInc | IIncW, insn)
        return IIncInsn(iinc_insn.index, iinc_insn.value)
    if insn_type is InvokeDynamic:
        invoke_dynamic = cast(InvokeDynamic, insn)
        return _lift_invoke_dynamic(invoke_dynamic, cp)
    if insn_type is InvokeInterface:
        invoke_interface = cast(InvokeInterface, insn)
        return _lift_invoke_interface(invoke_interface, cp)
    if insn_type is MultiANewArray:
        multi_anew_array = cast(MultiANewArray, insn)
        return MultiANewArrayInsn(
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
        return VarInsn(base, local_index_w.index)
    if insn_type is LocalIndex:
        local_index = cast(LocalIndex, insn)
        # LDC (0x12) uses a u1 CP index stored in LocalIndex.index.
        if local_index.type == InsnInfoType.LDC:
            return LdcInsn(_resolve_ldc_value(cp, local_index.index))
        return VarInsn(local_index.type, local_index.index)
    # Implicit slot variants: ILOAD_0 through ASTORE_3.
    implicit = _IMPLICIT_VAR_SLOTS.get(insn.type)
    if implicit is not None:
        base_opcode, slot = implicit
        return VarInsn(base_opcode, slot)
    return clone_raw_instruction(insn)


def _lift_instructions(
    code_attr: CodeAttr,
    labels_by_offset: dict[int, Label],
    cp: ConstantPoolBuilder,
) -> list[CodeItem]:
    instructions: list[CodeItem] = []
    inserted_offsets: set[int] = set()
    const_pool_item_cache: dict[tuple[InsnInfoType, int], CodeItem] = {}
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

        append(_lift_instruction(insn, labels_by_offset, code_attr.code_length, cp))

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
        return FieldInsn(field_item.type, field_item.owner, field_item.name, field_item.descriptor)
    if item_type is MethodInsn:
        method_item = cast(MethodInsn, item)
        return MethodInsn(
            method_item.type,
            method_item.owner,
            method_item.name,
            method_item.descriptor,
            method_item.is_interface,
        )
    if item_type is TypeInsn:
        type_item = cast(TypeInsn, item)
        return TypeInsn(type_item.type, type_item.class_name)
    if item_type is LdcInsn:
        ldc_item = cast(LdcInsn, item)
        return LdcInsn(ldc_item.value)
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
            if not skip_debug:
                layout.append("line_numbers")
                line_numbers.extend(
                    LineNumberEntry(
                        label=_label_for_offset(labels_by_offset, entry.start_pc),
                        line_number=entry.line_number,
                    )
                    for entry in attribute.line_number_table
                )
        elif isinstance(attribute, LocalVariableTableAttr):
            if not skip_debug:
                layout.append("local_variables")
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
            if not skip_debug:
                layout.append("local_variable_types")
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
    entry = cp.peek(index)
    if not isinstance(entry, ClassInfo):
        raise ValueError(f"CP index {index} is not a CONSTANT_Class: {type(entry).__name__}")
    return cp.resolve_utf8(entry.name_index)


def _resolve_member_ref(
    cp: ConstantPoolBuilder,
    index: int,
) -> tuple[str, str, str, bool]:
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
        return LdcInt(entry.value_bytes)
    if isinstance(entry, FloatInfo):
        return LdcFloat(entry.value_bytes)
    if isinstance(entry, LongInfo):
        return LdcLong((entry.high_bytes << 32) | (entry.low_bytes & 0xFFFFFFFF))
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
        nat = cp.peek(entry.name_and_type_index)
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
    return clone_raw_instruction(insn)


def _lift_invoke_dynamic(insn: InvokeDynamic, cp: ConstantPoolBuilder) -> InvokeDynamicInsn:
    """Lift a raw ``InvokeDynamic`` instruction to an ``InvokeDynamicInsn``."""
    entry = cp.peek(insn.index)
    if not isinstance(entry, InvokeDynamicInfo):
        raise ValueError(f"CP index {insn.index} is not a CONSTANT_InvokeDynamic: {type(entry).__name__}")
    nat = cp.peek(entry.name_and_type_index)
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
