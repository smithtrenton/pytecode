from dataclasses import dataclass, field
from typing import List

from .attributes import AttributeInfo
from .constant_pool import ConstantPoolInfo
from .constants import ClassAccessFlag, FieldAccessFlag, MethodAccessFlag

# TODO: add constantpool
@dataclass
class FieldInfo:
    _access_flags: FieldAccessFlag = field(repr=False)
    _name_index: int = field(repr=False)
    _descriptor_index: int = field(repr=False)
    _attributes_count: int = field(repr=False)
    _attributes: List[AttributeInfo] = field(repr=False)

    # access_flags: FieldAccessFlag = field(init=False)
    # name: str = field(init=False)
    # descriptor: str = field(init=False)
    # attributes: list[AttributeInfo] = field(init=False)

    # def __post_init__(self):
    #     self.access_flags = self._access_flags
    #     self.name = None
    #     self.descriptor = None
    #     self.attributes = self._attributes

# TODO: add constantpool
@dataclass
class MethodInfo:
    _access_flags: MethodAccessFlag = field(repr=False)
    _name_index: int = field(repr=False)
    _descriptor_index: int = field(repr=False)
    _attributes_count: int = field(repr=False)
    _attributes: List[AttributeInfo] = field(repr=False)

    # access_flags: MethodAccessFlag = field(init=False)
    # name: str = field(init=False)
    # descriptor: str = field(init=False)
    # attributes: list[AttributeInfo] = field(init=False)

    # def __post_init__(self):
    #     self.access_flags = self._access_flags
    #     self.name = None
    #     self.descriptor = None
    #     self.attributes = self._attributes



@dataclass
class ClassFile:
    _magic: int = field(repr=False)
    _minor_version: int = field(repr=False)
    _major_version: int = field(repr=False)
    _constant_pool_count: int = field(repr=False)
    _constant_pool: List[ConstantPoolInfo] = field(repr=False)
    _access_flags: ClassAccessFlag = field(repr=False)
    _this_class: int = field(repr=False)
    _super_class: int = field(repr=False)
    _interfaces_count: int = field(repr=False)
    _interfaces: List[int] = field(repr=False)
    _fields_count: int = field(repr=False)
    _fields: List[FieldInfo] = field(repr=False)
    _methods_count: int = field(repr=False)
    _methods: List[MethodInfo] = field(repr=False)
    _attributes_count: int = field(repr=False)
    _attributes: List[AttributeInfo] = field(repr=False)

    constant_pool: list[ConstantPoolInfo] = field(init=False)
    access_flags: ClassAccessFlag = field(init=False)
    class_name: str = field(init=False)
    super_name: str = field(init=False)
    interfaces: List[str] = field(init=False)
    fields: list[FieldInfo] = field(init=False)
    methods: list[MethodInfo] = field(init=False)
    attributes: list[AttributeInfo] = field(init=False)

    def __post_init__(self):
        self.constant_pool = self._constant_pool
        self.access_flags = self._access_flags
        self.class_name = self._get_class_name()
        self.super_name = self._get_super_name()
        self.interfaces = self._get_interfaces()
        self.fields = self._fields
        self.methods = self._methods
        self.attributes = self._attributes

    def _get_class_name(self):
        return self._constant_pool[self._constant_pool[self._this_class].name_index].str_bytes.decode('utf-8')

    def _get_super_name(self):
        if self._super_class >= 0:
            return self._constant_pool[self._constant_pool[self._super_class].name_index].str_bytes.decode('utf-8')

    def _get_interfaces(self):
        return [self._constant_pool[self._constant_pool[i].name_index].str_bytes.decode('utf-8') for i in self._interfaces]
