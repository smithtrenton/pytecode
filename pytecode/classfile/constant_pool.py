"""Representations of JVM constant pool entry types (┬º4.4)."""

from __future__ import annotations

from dataclasses import dataclass
from enum import Enum

__all__ = [
    "ClassInfo",
    "ConstantPoolInfo",
    "ConstantPoolInfoType",
    "DoubleInfo",
    "DynamicInfo",
    "FieldrefInfo",
    "FloatInfo",
    "IntegerInfo",
    "InterfaceMethodrefInfo",
    "InvokeDynamicInfo",
    "LongInfo",
    "MethodHandleInfo",
    "MethodTypeInfo",
    "MethodrefInfo",
    "ModuleInfo",
    "NameAndTypeInfo",
    "PackageInfo",
    "StringInfo",
    "Utf8Info",
]


@dataclass
class ConstantPoolInfo:
    """Base class for all constant pool entry types (┬º4.4)."""

    index: int
    offset: int
    tag: int


@dataclass
class Utf8Info(ConstantPoolInfo):
    """CONSTANT_Utf8_info entry (┬º4.4.7)."""

    length: int
    str_bytes: bytes


@dataclass
class IntegerInfo(ConstantPoolInfo):
    """CONSTANT_Integer_info entry (┬º4.4.4)."""

    value_bytes: int


@dataclass
class FloatInfo(ConstantPoolInfo):
    """CONSTANT_Float_info entry (┬º4.4.4)."""

    value_bytes: int


@dataclass
class LongInfo(ConstantPoolInfo):
    """CONSTANT_Long_info entry (┬º4.4.5)."""

    high_bytes: int
    low_bytes: int


@dataclass
class DoubleInfo(ConstantPoolInfo):
    """CONSTANT_Double_info entry (┬º4.4.5)."""

    high_bytes: int
    low_bytes: int


@dataclass
class ClassInfo(ConstantPoolInfo):
    """CONSTANT_Class_info entry (┬º4.4.1)."""

    name_index: int


@dataclass
class StringInfo(ConstantPoolInfo):
    """CONSTANT_String_info entry (┬º4.4.3)."""

    string_index: int


@dataclass
class FieldrefInfo(ConstantPoolInfo):
    """CONSTANT_Fieldref_info entry (┬º4.4.2)."""

    class_index: int
    name_and_type_index: int


@dataclass
class MethodrefInfo(ConstantPoolInfo):
    """CONSTANT_Methodref_info entry (┬º4.4.2)."""

    class_index: int
    name_and_type_index: int


@dataclass
class InterfaceMethodrefInfo(ConstantPoolInfo):
    """CONSTANT_InterfaceMethodref_info entry (┬º4.4.2)."""

    class_index: int
    name_and_type_index: int


@dataclass
class NameAndTypeInfo(ConstantPoolInfo):
    """CONSTANT_NameAndType_info entry (┬º4.4.6)."""

    name_index: int
    descriptor_index: int


@dataclass
class MethodHandleInfo(ConstantPoolInfo):
    """CONSTANT_MethodHandle_info entry (┬º4.4.8)."""

    reference_kind: int
    reference_index: int


@dataclass
class MethodTypeInfo(ConstantPoolInfo):
    """CONSTANT_MethodType_info entry (┬º4.4.9)."""

    descriptor_index: int


@dataclass
class DynamicInfo(ConstantPoolInfo):
    """CONSTANT_Dynamic_info entry (┬º4.4.10)."""

    bootstrap_method_attr_index: int
    name_and_type_index: int


@dataclass
class InvokeDynamicInfo(ConstantPoolInfo):
    """CONSTANT_InvokeDynamic_info entry (┬º4.4.10)."""

    bootstrap_method_attr_index: int
    name_and_type_index: int


@dataclass
class ModuleInfo(ConstantPoolInfo):
    """CONSTANT_Module_info entry (┬º4.4.11)."""

    name_index: int


@dataclass
class PackageInfo(ConstantPoolInfo):
    """CONSTANT_Package_info entry (┬º4.4.12)."""

    name_index: int


class ConstantPoolInfoType(Enum):
    """Enumeration of constant pool entry tags and their corresponding classes."""

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

    cp_class: type[ConstantPoolInfo]

    def __new__(cls, tag: int, cp_class: type[ConstantPoolInfo]) -> ConstantPoolInfoType:
        obj = object.__new__(cls)
        obj._value_ = tag
        obj.cp_class = cp_class
        return obj
