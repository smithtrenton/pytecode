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


class FrameComputationWarning(UserWarning):
    """Recomputed stack map frames are likely to fail JVM verification.

    ``FrameComputationMode.RECOMPUTE`` merges reference types at control-flow
    joins, which needs the class hierarchy. When a merged class is unknown —
    no ``resolver`` was passed, or the resolver lacks the class or one of its
    ancestors — the merge falls back to ``java/lang/Object``. That is only
    reported when the merged value later reaches an instruction requiring a
    more specific type, because only then will the JVM reject the frames.
    The message names the method and the classes to add to the resolver.

    Lowering still returns bytes; turn the warning into an error with
    ``warnings.simplefilter("error", FrameComputationWarning)``.
    """


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
BootstrapArgument = _rust.BootstrapArgument
BootstrapMethod = _rust.BootstrapMethod

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
    "BootstrapArgument",
    "BootstrapMethod",
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
    "FrameComputationWarning",
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
