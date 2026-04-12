"""Mutable symbolic editing surface for classes, methods, and code.

The objects re-exported here are the main API for structural edits: rename
classes, rewrite members, inspect method bodies, and lower the edited model
back to classfile bytes.
"""

from __future__ import annotations

from collections.abc import Callable
from typing import TYPE_CHECKING

from . import _rust

if TYPE_CHECKING:
    from .analysis import MappingClassResolver
    from .archive import FrameComputationMode


def _document_property(
    cls: type[object],
    name: str,
    doc: str,
    return_annotation: object,
    *,
    writable: bool = False,
) -> None:
    descriptor = cls.__dict__[name]

    def getter(self: object) -> object:
        return descriptor.__get__(self, type(self))

    getter.__name__ = name
    getter.__doc__ = doc
    getter.__annotations__ = {"return": return_annotation}

    setter_func: Callable[[object, object], None] | None = None
    if writable:

        def _setter(self: object, value: object) -> None:
            descriptor.__set__(self, value)

        _setter.__name__ = name
        _setter.__annotations__ = {"value": return_annotation, "return": None}
        setter_func = _setter

    setattr(cls, name, property(getter, setter_func, doc=doc))


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

ClassModel.__doc__ = "Mutable symbolic view of a class for structural edits and lowering."

_classmodel_from_bytes = ClassModel.from_bytes


def _documented_classmodel_from_bytes(data: bytes) -> ClassModel:
    """Parse classfile bytes into an editable class model."""

    return _classmodel_from_bytes(data)


ClassModel.from_bytes = staticmethod(_documented_classmodel_from_bytes)

_classmodel_to_bytes = ClassModel.to_bytes


def _documented_classmodel_to_bytes(self: ClassModel) -> bytes:
    """Lower the current model to classfile bytes with default options."""

    return _classmodel_to_bytes(self)


ClassModel.to_bytes = _documented_classmodel_to_bytes

_classmodel_to_classfile = ClassModel.to_classfile


def _documented_classmodel_to_classfile(self: ClassModel) -> _rust.ClassFile:
    """Lower the current model to a raw classfile object."""

    return _classmodel_to_classfile(self)


ClassModel.to_classfile = _documented_classmodel_to_classfile

_classmodel_to_bytes_with_options = ClassModel.to_bytes_with_options


def _documented_classmodel_to_bytes_with_options(
    self: ClassModel,
    frame_mode: FrameComputationMode | None = None,
    resolver: MappingClassResolver | None = None,
    debug_info: str = "preserve",
) -> bytes:
    """Lower the model to bytes with explicit frame and debug-info options."""

    return _classmodel_to_bytes_with_options(
        self,
        frame_mode=frame_mode,
        resolver=resolver,
        debug_info=debug_info,
    )


ClassModel.to_bytes_with_options = _documented_classmodel_to_bytes_with_options

_classmodel_to_classfile_with_options = ClassModel.to_classfile_with_options


def _documented_classmodel_to_classfile_with_options(
    self: ClassModel,
    frame_mode: FrameComputationMode | None = None,
    resolver: MappingClassResolver | None = None,
    debug_info: str = "preserve",
) -> _rust.ClassFile:
    """Lower the model to a raw classfile object with explicit lowering options."""

    return _classmodel_to_classfile_with_options(
        self,
        frame_mode=frame_mode,
        resolver=resolver,
        debug_info=debug_info,
    )


ClassModel.to_classfile_with_options = _documented_classmodel_to_classfile_with_options

_document_property(
    ClassModel,
    "version",
    "Classfile version as ``(major, minor)``.",
    tuple[int, int],
    writable=True,
)
_document_property(
    ClassModel,
    "original_byte_len",
    "Size in bytes of the classfile this model was originally parsed from.",
    int,
)
_document_property(
    ClassModel,
    "interfaces",
    "Live view of declared interface names in internal JVM format.",
    _rust.StringListView,
    writable=True,
)
_document_property(
    ClassModel,
    "name",
    "Internal JVM name of the class, such as ``pkg/Example``.",
    str,
    writable=True,
)
_document_property(
    ClassModel,
    "constant_pool",
    "Mutable constant-pool builder used when lowering edited models.",
    ConstantPoolBuilder,
)
_document_property(
    ClassModel,
    "methods",
    "Live view of the class's method models.",
    _rust.MethodListView,
    writable=True,
)
_document_property(
    ClassModel,
    "super_name",
    "Internal JVM name of the direct superclass, or ``None`` for ``java/lang/Object``.",
    str | None,
    writable=True,
)
_document_property(
    ClassModel,
    "entry_name",
    "Original archive entry name associated with this class model.",
    str,
)
_document_property(
    ClassModel,
    "access_flags",
    "Raw JVM access-flag bitset for the class declaration.",
    int,
    writable=True,
)
_document_property(
    ClassModel,
    "fields",
    "Live view of the class's field models.",
    _rust.FieldListView,
    writable=True,
)
_document_property(
    ClassModel,
    "debug_info_state",
    "Debug-info freshness marker, typically ``fresh`` or ``stale``.",
    str,
)
_document_property(
    ClassModel,
    "attributes",
    "Live view of raw class-level attributes.",
    _rust.AttributeListView,
)

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
