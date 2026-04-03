# cython: boundscheck=False, wraparound=False, cdivision=True
"""Mutable editing models for JVM class files.

Provides dataclass-based wrappers (``ClassModel``, ``MethodModel``,
``FieldModel``, ``CodeModel``) that resolve raw constant-pool indexes
into symbolic string references, making class-file content easy to
inspect and transform programmatically.
"""

from dataclasses import dataclass, field
from typing import TYPE_CHECKING

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
    instructions: list = field(default_factory=list)
    exception_handlers: list = field(default_factory=list)
    line_numbers: list = field(default_factory=list)
    local_variables: list = field(default_factory=list)
    local_variable_types: list = field(default_factory=list)
    attributes: list = field(default_factory=list)
    _nested_attribute_layout: tuple = field(default_factory=tuple, repr=False, compare=False)
    debug_info_state: object = DebugInfoState.FRESH


@dataclass
class FieldModel:
    """Mutable wrapper around a class field (JVMS §4.5).

    Attributes:
        access_flags: Field access and property flags.
        name: Unqualified field name.
        descriptor: Field descriptor (e.g. ``I``, ``Ljava/lang/String;``).
        attributes: Raw field-level attributes (e.g. ConstantValue).
    """

    access_flags: object
    name: str
    descriptor: str
    attributes: list


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

    access_flags: object
    name: str
    descriptor: str
    code: object
    attributes: list


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

    version: tuple
    access_flags: object
    name: str
    super_name: object
    interfaces: list
    fields: list
    methods: list
    attributes: list
    constant_pool: object = field(default_factory=ConstantPoolBuilder)
    debug_info_state: object = DebugInfoState.FRESH

    # ------------------------------------------------------------------
    # Construction helpers
    # ------------------------------------------------------------------

    @classmethod
    def from_classfile(cls, object cf, *, bint skip_debug=False):
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
        super_name = None
        if cf.super_class != 0:
            super_entry = cp.peek(cf.super_class)
            if not isinstance(super_entry, ClassInfo):
                raise ValueError(f"super_class CP index {cf.super_class} is not a CONSTANT_Class")
            super_name = cp.resolve_utf8(super_entry.name_index)

        # Resolve interfaces.
        interfaces = []
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
    def from_bytes(cls, object data, *, bint skip_debug=False):
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
        bint recompute_frames=False,
        object resolver=None,
        object debug_info=DebugInfoPolicy.PRESERVE,
    ):
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
        bint recompute_frames=False,
        object resolver=None,
        object debug_info=DebugInfoPolicy.PRESERVE,
    ):
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


def _field_from_info(object fi, object cp):
    return FieldModel(
        access_flags=fi.access_flags,
        name=cp.resolve_utf8(fi.name_index),
        descriptor=cp.resolve_utf8(fi.descriptor_index),
        attributes=clone_attributes(fi.attributes),
    )


def _method_from_info(object mi, object cp, *, bint skip_debug=False):
    code = None
    cdef list non_code_attrs = []

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


def _field_to_info(object fm, object cp):
    return FieldInfo(
        access_flags=fm.access_flags,
        name_index=cp.add_utf8(fm.name),
        descriptor_index=cp.add_utf8(fm.descriptor),
        attributes_count=len(fm.attributes),
        attributes=clone_attributes(fm.attributes),
    )


def _method_to_info(
    object mm,
    object cp,
    *,
    object class_name=None,
    bint recompute_frames=False,
    object resolver=None,
    object debug_info=DebugInfoPolicy.PRESERVE,
):
    cdef list attrs = clone_attributes(mm.attributes)

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


def _validate_code_offset(int offset, int code_length, *, str context):
    if not 0 <= offset <= code_length:
        raise ValueError(f"{context} offset {offset} is outside code range [0, {code_length}]")
    return offset


def _label_for_offset(dict labels_by_offset, int offset):
    return labels_by_offset[offset]


def _branch_target_offset(object insn, int code_length):
    return _validate_code_offset(
        insn.bytecode_offset + insn.offset,
        code_length,
        context=f"{insn.type.name} target",
    )


def _collect_labels(object code_attr, *, bint skip_debug=False):
    cdef dict labels_by_offset = {}
    cdef int code_length = code_attr.code_length

    def ensure_label(int offset, *, str context):
        cdef int validated = _validate_code_offset(offset, code_length, context=context)
        labels_by_offset.setdefault(validated, Label(f"L{validated}"))

    for insn in code_attr.code:
        if isinstance(insn, (Branch, BranchW)):
            ensure_label(_branch_target_offset(insn, code_length), context=f"{insn.type.name} target")
        elif isinstance(insn, LookupSwitch):
            ensure_label(
                _validate_code_offset(
                    insn.bytecode_offset + insn.default,
                    code_length,
                    context="lookupswitch default target",
                ),
                context="lookupswitch default target",
            )
            for pair in insn.pairs:
                ensure_label(
                    _validate_code_offset(
                        insn.bytecode_offset + pair.offset,
                        code_length,
                        context="lookupswitch case target",
                    ),
                    context="lookupswitch case target",
                )
        elif isinstance(insn, TableSwitch):
            ensure_label(
                _validate_code_offset(
                    insn.bytecode_offset + insn.default,
                    code_length,
                    context="tableswitch default target",
                ),
                context="tableswitch default target",
            )
            for relative in insn.offsets:
                ensure_label(
                    _validate_code_offset(
                        insn.bytecode_offset + relative,
                        code_length,
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
    object insn,
    dict labels_by_offset,
    int code_length,
    object cp,
):
    insn_type = type(insn)
    if insn_type in (Branch, BranchW):
        branch_insn = insn
        return BranchInsn(
            branch_insn.type,
            _label_for_offset(labels_by_offset, _branch_target_offset(branch_insn, code_length)),
        )
    if insn_type is LookupSwitch:
        lookup_switch = insn
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
        table_switch = insn
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
        iinc_insn = insn
        return IIncInsn(iinc_insn.index, iinc_insn.value)
    if insn_type is InvokeDynamic:
        invoke_dynamic = insn
        return _lift_invoke_dynamic(invoke_dynamic, cp)
    if insn_type is InvokeInterface:
        invoke_interface = insn
        return _lift_invoke_interface(invoke_interface, cp)
    if insn_type is MultiANewArray:
        multi_anew_array = insn
        return MultiANewArrayInsn(
            _resolve_class_name(cp, multi_anew_array.index),
            multi_anew_array.dimensions,
        )
    if insn_type is ConstPoolIndex:
        const_pool_insn = insn
        return _lift_const_pool_index(const_pool_insn, cp)
    if insn_type is LocalIndexW:
        local_index_w = insn
        # WIDE var opcodes: normalize to VarInsn with canonical base opcode.
        base = _WIDE_TO_BASE[local_index_w.type]
        return VarInsn(base, local_index_w.index)
    if insn_type is LocalIndex:
        local_index = insn
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
    object code_attr,
    dict labels_by_offset,
    object cp,
):
    cdef list instructions = []
    cdef set inserted_offsets = set()
    cdef dict const_pool_item_cache = {}
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

    cdef int code_length = code_attr.code_length
    end_label = labels_get(code_length)
    if end_label is not None and code_length not in inserted_offsets:
        append(end_label)
        inserted_add(code_length)

    missing_offsets = sorted(set(labels_by_offset) - inserted_offsets)
    if missing_offsets:
        raise ValueError(
            "labels refer to offsets that are not instruction boundaries: "
            + ", ".join(str(offset) for offset in missing_offsets)
        )

    return instructions


def _clone_lifted_code_item(object item):
    item_type = type(item)
    if item_type is FieldInsn:
        field_item = item
        return FieldInsn(field_item.type, field_item.owner, field_item.name, field_item.descriptor)
    if item_type is MethodInsn:
        method_item = item
        return MethodInsn(
            method_item.type,
            method_item.owner,
            method_item.name,
            method_item.descriptor,
            method_item.is_interface,
        )
    if item_type is TypeInsn:
        type_item = item
        return TypeInsn(type_item.type, type_item.class_name)
    if item_type is LdcInsn:
        ldc_item = item
        return LdcInsn(ldc_item.value)
    return clone_raw_instruction(insn=item)


def _lift_exception_handlers(
    object code_attr,
    dict labels_by_offset,
    object cp,
):
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
    object code_attr,
    dict labels_by_offset,
    object cp,
    *,
    bint skip_debug=False,
):
    cdef list line_numbers = []
    cdef list local_variables = []
    cdef list local_variable_types = []
    cdef list attributes = []
    cdef list layout = []

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


def _code_from_attr(object attr, object cp, *, bint skip_debug=False):
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


def _resolve_class_name(object cp, int index):
    """Resolve a CONSTANT_Class CP index to its internal name string."""
    entry = cp.peek(index)
    if not isinstance(entry, ClassInfo):
        raise ValueError(f"CP index {index} is not a CONSTANT_Class: {type(entry).__name__}")
    return cp.resolve_utf8(entry.name_index)


def _resolve_member_ref(
    object cp,
    int index,
):
    """Resolve a Fieldref/Methodref/InterfaceMethodref to (owner, name, descriptor, is_interface)."""
    entry = cp.peek(index)
    if not isinstance(entry, (FieldrefInfo, MethodrefInfo, InterfaceMethodrefInfo)):
        raise ValueError(f"CP index {index} is not a member ref entry: {type(entry).__name__}")
    cdef bint is_interface = isinstance(entry, InterfaceMethodrefInfo)
    owner = _resolve_class_name(cp, entry.class_index)
    nat = cp.peek(entry.name_and_type_index)
    if not isinstance(nat, NameAndTypeInfo):
        raise ValueError(f"CP index {entry.name_and_type_index} is not a CONSTANT_NameAndType")
    name = cp.resolve_utf8(nat.name_index)
    descriptor = cp.resolve_utf8(nat.descriptor_index)
    return owner, name, descriptor, is_interface


def _resolve_ldc_value(object cp, int index):
    """Resolve an LDC/LDC_W/LDC2_W CP index to a typed ``LdcValue``."""
    entry = cp.peek(index)
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
        nat = cp.peek(entry.name_and_type_index)
        if not isinstance(nat, NameAndTypeInfo):
            raise ValueError(f"CP index {entry.name_and_type_index} is not a CONSTANT_NameAndType")
        return LdcDynamic(
            entry.bootstrap_method_attr_index,
            cp.resolve_utf8(nat.name_index),
            cp.resolve_utf8(nat.descriptor_index),
        )
    raise ValueError(f"CP index {index} has unsupported type for LDC: {type(entry).__name__}")


def _resolve_ldc_method_handle(object cp, object entry):
    """Resolve a CONSTANT_MethodHandle entry to an ``LdcMethodHandle``."""
    ref_entry = cp.peek(entry.reference_index)
    if not isinstance(ref_entry, (FieldrefInfo, MethodrefInfo, InterfaceMethodrefInfo)):
        raise ValueError(
            f"MethodHandle reference index {entry.reference_index} has unexpected type: {type(ref_entry).__name__}"
        )
    cdef bint is_interface = isinstance(ref_entry, InterfaceMethodrefInfo)
    owner = _resolve_class_name(cp, ref_entry.class_index)
    nat = cp.peek(ref_entry.name_and_type_index)
    if not isinstance(nat, NameAndTypeInfo):
        raise ValueError(f"CP index {ref_entry.name_and_type_index} is not a CONSTANT_NameAndType")
    name = cp.resolve_utf8(nat.name_index)
    descriptor = cp.resolve_utf8(nat.descriptor_index)
    return LdcMethodHandle(entry.reference_kind, owner, name, descriptor, is_interface)


def _lift_const_pool_index(object insn, object cp):
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


def _lift_invoke_dynamic(object insn, object cp):
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


def _lift_invoke_interface(object insn, object cp):
    """Lift a raw ``InvokeInterface`` instruction to an ``InterfaceMethodInsn``."""
    owner, name, descriptor, _ = _resolve_member_ref(cp, insn.index)
    return InterfaceMethodInsn(owner, name, descriptor)
