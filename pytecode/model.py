from __future__ import annotations

import copy
from dataclasses import dataclass, field

from .attributes import (
    AttributeInfo,
    CodeAttr,
    LineNumberTableAttr,
    LocalVariableTableAttr,
    LocalVariableTypeTableAttr,
)
from .class_reader import ClassReader
from .constant_pool import ClassInfo
from .constant_pool_builder import ConstantPoolBuilder
from .constants import MAGIC, ClassAccessFlag, FieldAccessFlag, MethodAccessFlag
from .info import ClassFile, FieldInfo, MethodInfo
from .instructions import Branch, BranchW, InsnInfo, LookupSwitch, TableSwitch
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

    # ------------------------------------------------------------------
    # Construction helpers
    # ------------------------------------------------------------------

    @classmethod
    def from_classfile(cls, cf: ClassFile) -> ClassModel:
        """Build a ``ClassModel`` from a parsed ``ClassFile``.

        Resolves constant-pool indexes to symbolic string values and
        seeds the ``ConstantPoolBuilder`` from the original pool so that
        raw attributes and instructions remain valid.
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
        methods = [_method_from_info(mi, cp) for mi in cf.methods]

        return cls(
            version=(cf.major_version, cf.minor_version),
            access_flags=cf.access_flags,
            name=name,
            super_name=super_name,
            interfaces=interfaces,
            fields=fields,
            methods=methods,
            attributes=copy.deepcopy(cf.attributes),
            constant_pool=cp,
        )

    @classmethod
    def from_bytes(cls, data: bytes | bytearray) -> ClassModel:
        """Parse raw class-file bytes and build a ``ClassModel``."""
        reader = ClassReader(data)
        return cls.from_classfile(reader.class_info)

    # ------------------------------------------------------------------
    # Lowering
    # ------------------------------------------------------------------

    def to_classfile(self) -> ClassFile:
        """Lower this model back to a spec-faithful ``ClassFile``.

        Uses the ``ConstantPoolBuilder`` to allocate (or find) constant-pool
        entries for every symbolic reference and reassembles the raw
        ``FieldInfo``/``MethodInfo``/``CodeAttr`` structures.
        """
        cp = self.constant_pool

        # Allocate class-level CP entries.
        this_class_index = cp.add_class(self.name)
        super_class_index = cp.add_class(self.super_name) if self.super_name else 0
        interface_indexes = [cp.add_class(iface) for iface in self.interfaces]

        # Lower fields.
        raw_fields = [_field_to_info(fm, cp) for fm in self.fields]

        # Lower methods.
        raw_methods = [_method_to_info(mm, cp) for mm in self.methods]

        pool = cp.build()

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
            attributes_count=len(self.attributes),
            attributes=copy.deepcopy(self.attributes),
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


def _method_from_info(mi: MethodInfo, cp: ConstantPoolBuilder) -> MethodModel:
    code: CodeModel | None = None
    non_code_attrs: list[AttributeInfo] = []

    for attr in mi.attributes:
        if isinstance(attr, CodeAttr):
            code = _code_from_attr(attr, cp)
        else:
            non_code_attrs.append(copy.deepcopy(attr))

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


def _method_to_info(mm: MethodModel, cp: ConstantPoolBuilder) -> MethodInfo:
    attrs: list[AttributeInfo] = copy.deepcopy(mm.attributes)

    if mm.code is not None:
        code_attr = lower_code(mm.code, cp)
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


def _collect_labels(code_attr: CodeAttr) -> dict[int, Label]:
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


def _lift_instruction(insn: InsnInfo, labels_by_offset: dict[int, Label], code_length: int) -> CodeItem:
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
    return copy.deepcopy(insn)


def _lift_instructions(code_attr: CodeAttr, labels_by_offset: dict[int, Label]) -> list[CodeItem]:
    instructions: list[CodeItem] = []
    inserted_offsets: set[int] = set()

    for insn in code_attr.code:
        label = labels_by_offset.get(insn.bytecode_offset)
        if label is not None and insn.bytecode_offset not in inserted_offsets:
            instructions.append(label)
            inserted_offsets.add(insn.bytecode_offset)
        instructions.append(_lift_instruction(insn, labels_by_offset, code_attr.code_length))

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
) -> tuple[
    list[LineNumberEntry],
    list[LocalVariableEntry],
    list[LocalVariableTypeEntry],
    list[AttributeInfo],
]:
    line_numbers: list[LineNumberEntry] = []
    local_variables: list[LocalVariableEntry] = []
    local_variable_types: list[LocalVariableTypeEntry] = []
    attributes: list[AttributeInfo] = []

    for attribute in code_attr.attributes:
        if isinstance(attribute, LineNumberTableAttr):
            line_numbers.extend(
                LineNumberEntry(
                    label=_label_for_offset(labels_by_offset, entry.start_pc),
                    line_number=entry.line_number,
                )
                for entry in attribute.line_number_table
            )
        elif isinstance(attribute, LocalVariableTableAttr):
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
            attributes.append(copy.deepcopy(attribute))

    return line_numbers, local_variables, local_variable_types, attributes


def _code_from_attr(attr: CodeAttr, cp: ConstantPoolBuilder) -> CodeModel:
    labels_by_offset = _collect_labels(attr)
    line_numbers, local_variables, local_variable_types, attributes = _lift_nested_code_attributes(
        attr,
        labels_by_offset,
        cp,
    )
    return CodeModel(
        max_stack=attr.max_stacks,
        max_locals=attr.max_locals,
        instructions=_lift_instructions(attr, labels_by_offset),
        exception_handlers=_lift_exception_handlers(attr, labels_by_offset, cp),
        line_numbers=line_numbers,
        local_variables=local_variables,
        local_variable_types=local_variable_types,
        attributes=attributes,
    )
