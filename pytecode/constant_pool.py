from dataclasses import dataclass
from enum import Enum


@dataclass
class ConstantPoolInfo:
    index: int
    offset: int
    tag: int


@dataclass
class Utf8Info(ConstantPoolInfo):
    length: int
    str_bytes: bytes


@dataclass
class IntegerInfo(ConstantPoolInfo):
    value_bytes: bytes


@dataclass
class FloatInfo(ConstantPoolInfo):
    value_bytes: bytes


@dataclass
class LongInfo(ConstantPoolInfo):
    high_bytes: bytes
    low_bytes: bytes


@dataclass
class DoubleInfo(ConstantPoolInfo):
    high_bytes: bytes
    low_bytes: bytes


@dataclass
class ClassInfo(ConstantPoolInfo):
    name_index: int


@dataclass
class StringInfo(ConstantPoolInfo):
    string_index: int


@dataclass
class FieldrefInfo(ConstantPoolInfo):
    class_index: int
    name_and_type_index: int


@dataclass
class MethodrefInfo(ConstantPoolInfo):
    class_index: int
    name_and_type_index: int


@dataclass
class InterfaceMethodrefInfo(ConstantPoolInfo):
    class_index: int
    name_and_type_index: int


@dataclass
class NameAndTypeInfo(ConstantPoolInfo):
    name_index: int
    descriptor_index: int


@dataclass
class MethodHandleInfo(ConstantPoolInfo):
    reference_kind: int
    reference_index: int


@dataclass
class MethodTypeInfo(ConstantPoolInfo):
    descriptor_index: int


@dataclass
class DynamicInfo(ConstantPoolInfo):
    bootstrap_method_attr_index: int
    name_and_type_index: int


@dataclass
class InvokeDynamicInfo(ConstantPoolInfo):
    bootstrap_method_attr_index: int
    name_and_type_index: int


@dataclass
class ModuleInfo(ConstantPoolInfo):
    name_index: int


@dataclass
class PackageInfo(ConstantPoolInfo):
    name_index: int


class ConstantPoolInfoType(Enum):
    UTF8 = 1, Utf8Info
    INTEGER = 3, IntegerInfo
    FLOAT = 4, FloatInfo
    LONG = 5, LongInfo
    DOUBLE = 6, DoubleInfo
    CLASS = 7, ClassInfo
    STRING = 8, StringInfo
    FIELD_REF = 9, FieldrefInfo
    METHOD_REF = 10, MethodrefInfo
    INTERFACE_METHOD_REF = 11, InterfaceMethodrefInfo
    NAME_AND_TYPE = 12, NameAndTypeInfo
    METHOD_HANDLE = 15, MethodHandleInfo
    METHOD_TYPE = 16, MethodTypeInfo
    DYNAMIC = 17, DynamicInfo
    INVOKE_DYNAMIC = 18, InvokeDynamicInfo
    MODULE = 19, ModuleInfo
    PACKAGE = 20, PackageInfo

    def __new__(cls, tag, cp_class):
        obj = object.__new__(cls)
        obj._value_ = tag
        obj.cp_class = cp_class
        return obj
