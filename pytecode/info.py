"""Data structures for parsed JVM class file components.

Provides dataclass representations of the top-level structures defined in the
JVM specification: the ``ClassFile`` structure (§4.1), ``field_info`` (§4.5),
and ``method_info`` (§4.6).
"""

from __future__ import annotations

from dataclasses import dataclass

from .attributes import AttributeInfo
from .constant_pool import ConstantPoolInfo
from .constants import ClassAccessFlag, FieldAccessFlag, MethodAccessFlag

__all__ = ["ClassFile", "FieldInfo", "MethodInfo"]


@dataclass
class FieldInfo:
    """Parsed ``field_info`` structure (JVM spec §4.5).

    Represents a single field declared in a class or interface.

    Attributes:
        access_flags: Mask of ``FieldAccessFlag`` values denoting access
            permissions and properties of the field.
        name_index: Index into the constant pool for the field's simple name.
        descriptor_index: Index into the constant pool for the field's
            descriptor string.
        attributes_count: Number of additional attributes for this field.
        attributes: Variable-length list of ``AttributeInfo`` structures
            giving additional information about the field.
    """

    access_flags: FieldAccessFlag
    name_index: int
    descriptor_index: int
    attributes_count: int
    attributes: list[AttributeInfo]


@dataclass
class MethodInfo:
    """Parsed ``method_info`` structure (JVM spec §4.6).

    Represents a single method declared in a class or interface, including
    instance methods, class methods, instance initialisation methods, and
    the class/interface initialisation method.

    Attributes:
        access_flags: Mask of ``MethodAccessFlag`` values denoting access
            permissions and properties of the method.
        name_index: Index into the constant pool for the method's simple name.
        descriptor_index: Index into the constant pool for the method's
            descriptor string.
        attributes_count: Number of additional attributes for this method.
        attributes: Variable-length list of ``AttributeInfo`` structures
            giving additional information about the method (e.g. bytecode).
    """

    access_flags: MethodAccessFlag
    name_index: int
    descriptor_index: int
    attributes_count: int
    attributes: list[AttributeInfo]


@dataclass
class ClassFile:
    """Parsed ``ClassFile`` structure (JVM spec §4.1).

    Top-level representation of a ``.class`` file produced by the parser.
    Every field corresponds directly to an item in the ``ClassFile`` table
    defined by the specification.

    Attributes:
        magic: The magic number identifying the class file format
            (``0xCAFEBABE``).
        minor_version: Minor version number of the class file.
        major_version: Major version number of the class file.
        constant_pool_count: Number of entries in the constant pool table
            plus one.
        constant_pool: Table of ``ConstantPoolInfo`` entries (indexed from 1).
            Entries may be ``None`` for the unused slots that follow
            ``CONSTANT_Long`` and ``CONSTANT_Double`` entries.
        access_flags: Mask of ``ClassAccessFlag`` values denoting access
            permissions and properties of this class or interface.
        this_class: Constant pool index of a ``CONSTANT_Class_info`` entry
            representing the class defined by this file.
        super_class: Constant pool index of a ``CONSTANT_Class_info`` entry
            representing the direct superclass, or ``0`` for
            ``java.lang.Object``.
        interfaces_count: Number of direct superinterfaces.
        interfaces: List of constant pool indices, each referencing a
            ``CONSTANT_Class_info`` entry for a direct superinterface.
        fields_count: Number of ``FieldInfo`` structures in *fields*.
        fields: List of ``FieldInfo`` structures representing all fields
            declared by this class or interface.
        methods_count: Number of ``MethodInfo`` structures in *methods*.
        methods: List of ``MethodInfo`` structures representing all methods
            declared by this class or interface.
        attributes_count: Number of attributes in the *attributes* table.
        attributes: List of ``AttributeInfo`` structures giving additional
            class file attributes.
    """

    magic: int
    minor_version: int
    major_version: int
    constant_pool_count: int
    constant_pool: list[ConstantPoolInfo | None]
    access_flags: ClassAccessFlag
    this_class: int
    super_class: int
    interfaces_count: int
    interfaces: list[int]
    fields_count: int
    fields: list[FieldInfo]
    methods_count: int
    methods: list[MethodInfo]
    attributes_count: int
    attributes: list[AttributeInfo]
