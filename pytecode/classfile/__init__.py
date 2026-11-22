"""Raw classfile parsing and bytecode inspection surface.

Use this module when you want a close-to-the-bytes view of a class file rather
than the mutable symbolic editing API in :mod:`pytecode.model`.
"""

from __future__ import annotations

from .. import _rust
from .bytecode import ArrayType, InsnInfoType

ClassFile = _rust.ClassFile
ClassInfo = _rust.ClassInfo
ClassReader = _rust.ClassReader
ClassWriter = _rust.ClassWriter
DoubleInfo = _rust.DoubleInfo
DynamicInfo = _rust.DynamicInfo
ExceptionInfo = _rust.ExceptionInfo
FieldInfo = _rust.FieldInfo
FieldrefInfo = _rust.FieldrefInfo
FloatInfo = _rust.FloatInfo
InsnInfo = _rust.InsnInfo
IntegerInfo = _rust.IntegerInfo
InterfaceMethodrefInfo = _rust.InterfaceMethodrefInfo
InvokeDynamicInfo = _rust.InvokeDynamicInfo
LongInfo = _rust.LongInfo
MalformedClassException = _rust.MalformedClassException
MatchOffsetPair = _rust.MatchOffsetPair
MethodHandleInfo = _rust.MethodHandleInfo
MethodInfo = _rust.MethodInfo
MethodTypeInfo = _rust.MethodTypeInfo
MethodrefInfo = _rust.MethodrefInfo
ModuleInfo = _rust.ModuleInfo
NameAndTypeInfo = _rust.NameAndTypeInfo
PackageInfo = _rust.PackageInfo
StringInfo = _rust.StringInfo
Utf8Info = _rust.Utf8Info

__all__ = [
    "ArrayType",
    "ClassFile",
    "ClassInfo",
    "ClassReader",
    "ClassWriter",
    "DoubleInfo",
    "DynamicInfo",
    "ExceptionInfo",
    "FieldInfo",
    "FieldrefInfo",
    "FloatInfo",
    "InsnInfo",
    "InsnInfoType",
    "IntegerInfo",
    "InterfaceMethodrefInfo",
    "InvokeDynamicInfo",
    "LongInfo",
    "MalformedClassException",
    "MatchOffsetPair",
    "MethodHandleInfo",
    "MethodInfo",
    "MethodTypeInfo",
    "MethodrefInfo",
    "ModuleInfo",
    "NameAndTypeInfo",
    "PackageInfo",
    "StringInfo",
    "Utf8Info",
]
