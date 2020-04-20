from dataclasses import dataclass
from typing import List

from . import attributes, constants, constant_pool


@dataclass
class FieldInfo:
    access_flags: constants.FieldAccessFlag
    name_index: int
    descriptor_index: int
    attributes_count: int
    attributes: List[attributes.AttributeInfo]


@dataclass
class MethodInfo:
    access_flags: constants.MethodAccessFlag
    name_index: int
    descriptor_index: int
    attributes_count: int
    attributes: List[attributes.AttributeInfo]


@dataclass
class ClassInfo:
    magic: int
    minor_version: int
    major_version: int
    contant_pool_count: int
    constant_pool: List[constant_pool.ConstantPoolInfo]
    access_flags: constants.ClassAccessFlag
    this_class: int
    super_class: int
    interfaces_count: int
    interfaces: List[int]
    fields_count: int
    fields: List[FieldInfo]
    methods_count: int
    methods: List[MethodInfo]
    attributes_count: int
    attributes: List[attributes.AttributeInfo]