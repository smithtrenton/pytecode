from dataclasses import dataclass

from .attributes import AttributeInfo
from .constant_pool import ConstantPoolInfo
from .constants import ClassAccessFlag, FieldAccessFlag, MethodAccessFlag


@dataclass
class FieldInfo:
    access_flags: FieldAccessFlag
    name_index: int
    descriptor_index: int
    attributes_count: int
    attributes: list[AttributeInfo]


@dataclass
class MethodInfo:
    access_flags: MethodAccessFlag
    name_index: int
    descriptor_index: int
    attributes_count: int
    attributes: list[AttributeInfo]


@dataclass
class ClassFile:
    magic: int
    minor_version: int
    major_version: int
    constant_pool_count: int
    constant_pool: list[ConstantPoolInfo]
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
