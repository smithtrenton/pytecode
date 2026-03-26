from __future__ import annotations

import copy
from dataclasses import dataclass, field

from .attributes import AttributeInfo, CodeAttr, ExceptionInfo
from .class_reader import ClassReader
from .constant_pool import ClassInfo
from .constant_pool_builder import ConstantPoolBuilder
from .constants import MAGIC, ClassAccessFlag, FieldAccessFlag, MethodAccessFlag
from .info import ClassFile, FieldInfo, MethodInfo
from .instructions import InsnInfo


@dataclass
class CodeModel:
    """Mutable wrapper around a method's code body.

    Carries the raw instruction list, exception table, and stack/locals
    limits from the parsed ``CodeAttr``.  This type is the extension point
    for label-based instruction editing (#7).
    """

    max_stack: int
    max_locals: int
    instructions: list[InsnInfo]
    exception_table: list[ExceptionInfo]
    attributes: list[AttributeInfo]


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
            raise ValueError(
                f"this_class CP index {cf.this_class} is not a CONSTANT_Class"
            )
        name = cp.resolve_utf8(this_class_entry.name_index)

        # Resolve super_class → class name string or None.
        super_name: str | None = None
        if cf.super_class != 0:
            super_entry = cp.get(cf.super_class)
            if not isinstance(super_entry, ClassInfo):
                raise ValueError(
                    f"super_class CP index {cf.super_class} is not a CONSTANT_Class"
                )
            super_name = cp.resolve_utf8(super_entry.name_index)

        # Resolve interfaces.
        interfaces: list[str] = []
        for iface_index in cf.interfaces:
            iface_entry = cp.get(iface_index)
            if not isinstance(iface_entry, ClassInfo):
                raise ValueError(
                    f"interface CP index {iface_index} is not a CONSTANT_Class"
                )
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
            code = CodeModel(
                max_stack=attr.max_stacks,
                max_locals=attr.max_locals,
                instructions=copy.deepcopy(attr.code),
                exception_table=copy.deepcopy(attr.exception_table),
                attributes=copy.deepcopy(attr.attributes),
            )
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
        code_attr_name_index = cp.add_utf8("Code")
        code_attr = CodeAttr(
            attribute_name_index=code_attr_name_index,
            attribute_length=0,  # placeholder — computed during emission
            max_stacks=mm.code.max_stack,
            max_locals=mm.code.max_locals,
            code_length=0,  # placeholder — computed during emission
            code=copy.deepcopy(mm.code.instructions),
            exception_table_length=len(mm.code.exception_table),
            exception_table=copy.deepcopy(mm.code.exception_table),
            attributes_count=len(mm.code.attributes),
            attributes=copy.deepcopy(mm.code.attributes),
        )
        attrs.insert(0, code_attr)

    return MethodInfo(
        access_flags=mm.access_flags,
        name_index=cp.add_utf8(mm.name),
        descriptor_index=cp.add_utf8(mm.descriptor),
        attributes_count=len(attrs),
        attributes=attrs,
    )
