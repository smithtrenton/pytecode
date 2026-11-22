"""Mutable symbolic editing surface for classes, methods, and code.

The objects re-exported here are the main API for structural edits: rename
classes, rewrite members, inspect method bodies, and lower the edited model
back to classfile bytes.
"""

from __future__ import annotations

from enum import Enum

from . import _rust


class DebugInfoPolicy(Enum):
    """Policy controlling whether lowering or archive rewrites preserve or strip debug info."""

    PRESERVE = "preserve"
    STRIP = "strip"


class FrameComputationMode(Enum):
    """Policy controlling whether lowering preserves or recomputes stack map frames."""

    PRESERVE = "preserve"
    RECOMPUTE = "recompute"


MalformedClassException = _rust.MalformedClassException

ClassModel = _rust.ClassModel
FieldModel = _rust.FieldModel
MethodModel = _rust.MethodModel
CodeModel = _rust.CodeModel
ConstantPoolBuilder = _rust.ConstantPoolBuilder

StringListView = _rust.StringListView
FieldListView = _rust.FieldListView
MethodListView = _rust.MethodListView
AttributeListView = _rust.AttributeListView
CodeListView = _rust.CodeListView

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
    "AttributeListView",
    "BranchInsn",
    "ByteInsn",
    "ClassModel",
    "CodeListView",
    "CodeModel",
    "ConstantPoolBuilder",
    "DebugInfoPolicy",
    "DynamicValue",
    "ExceptionHandler",
    "FieldInsn",
    "FieldListView",
    "FieldModel",
    "FrameComputationMode",
    "IIncInsn",
    "InterfaceMethodInsn",
    "InvokeDynamicInsn",
    "Label",
    "LdcInsn",
    "LineNumberEntry",
    "LocalVariableEntry",
    "LocalVariableTypeEntry",
    "LookupSwitchInsn",
    "MalformedClassException",
    "MethodHandleValue",
    "MethodInsn",
    "MethodListView",
    "MethodModel",
    "MultiANewArrayInsn",
    "NewArrayInsn",
    "RawInsn",
    "ShortInsn",
    "StringListView",
    "TableSwitchInsn",
    "TypeInsn",
    "VarInsn",
]
