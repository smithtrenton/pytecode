"""Rust-backed mutable class model surface.

This module is the semantic home for editable class-model objects and typed
code-item wrappers exposed by the Rust extension.
"""

from __future__ import annotations

from . import _rust

ClassModel = _rust.ClassModel
FieldModel = _rust.FieldModel
MethodModel = _rust.MethodModel
CodeModel = _rust.CodeModel
ConstantPoolBuilder = _rust.ConstantPoolBuilder

Label = _rust.Label
ExceptionHandler = _rust.ExceptionHandler
LineNumberEntry = _rust.LineNumberEntry
LocalVariableEntry = _rust.LocalVariableEntry
LocalVariableTypeEntry = _rust.LocalVariableTypeEntry

MethodHandleValue = _rust.MethodHandleValue
DynamicValue = _rust.DynamicValue

RawInsn = _rust.RawInsn
ByteInsn = _rust.ByteInsn
ShortInsn = _rust.ShortInsn
NewArrayInsn = _rust.NewArrayInsn
FieldInsn = _rust.FieldInsn
MethodInsn = _rust.MethodInsn
InterfaceMethodInsn = _rust.InterfaceMethodInsn
TypeInsn = _rust.TypeInsn
VarInsn = _rust.VarInsn
IIncInsn = _rust.IIncInsn
LdcInsn = _rust.LdcInsn
InvokeDynamicInsn = _rust.InvokeDynamicInsn
MultiANewArrayInsn = _rust.MultiANewArrayInsn
BranchInsn = _rust.BranchInsn
LookupSwitchInsn = _rust.LookupSwitchInsn
TableSwitchInsn = _rust.TableSwitchInsn

__all__ = [
    "BranchInsn",
    "ByteInsn",
    "ClassModel",
    "CodeModel",
    "ConstantPoolBuilder",
    "DynamicValue",
    "ExceptionHandler",
    "FieldInsn",
    "FieldModel",
    "IIncInsn",
    "InterfaceMethodInsn",
    "InvokeDynamicInsn",
    "Label",
    "LdcInsn",
    "LineNumberEntry",
    "LocalVariableEntry",
    "LocalVariableTypeEntry",
    "LookupSwitchInsn",
    "MethodHandleValue",
    "MethodInsn",
    "MethodModel",
    "MultiANewArrayInsn",
    "NewArrayInsn",
    "RawInsn",
    "ShortInsn",
    "TableSwitchInsn",
    "TypeInsn",
    "VarInsn",
]
