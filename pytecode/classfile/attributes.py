"""Rust-backed raw attribute payload types parsed from JVM class files.

These types are constructed by the Rust classfile reader and re-exported from
``pytecode._rust``. The field names intentionally stay close to the JVM layout
so you can cross-reference them with the JVMS classfile format when needed.
"""

from __future__ import annotations

from enum import Enum
from typing import Protocol, runtime_checkable

from .. import _rust
from . import constants

__all__ = [
    "AnnotationDefaultAttr",
    "AnnotationInfo",
    "AppendFrameInfo",
    "ArrayValueInfo",
    "AttributeInfo",
    "AttributeInfoType",
    "BootstrapMethodInfo",
    "BootstrapMethodsAttr",
    "CatchTargetInfo",
    "ChopFrameInfo",
    "ClassInfoValueInfo",
    "CodeAttr",
    "ConstValueInfo",
    "ConstantValueAttr",
    "DeprecatedAttr",
    "DoubleVariableInfo",
    "ElementValueInfo",
    "ElementValuePairInfo",
    "EmptyTargetInfo",
    "EnclosingMethodAttr",
    "EnumConstantValueInfo",
    "ExceptionsAttr",
    "ExportInfo",
    "FloatVariableInfo",
    "FormalParameterTargetInfo",
    "FullFrameInfo",
    "InnerClassInfo",
    "InnerClassesAttr",
    "IntegerVariableInfo",
    "LineNumberInfo",
    "LineNumberTableAttr",
    "LocalVariableInfo",
    "LocalVariableTableAttr",
    "LocalVariableTypeInfo",
    "LocalVariableTypeTableAttr",
    "LocalvarTargetInfo",
    "LongVariableInfo",
    "MethodParameterInfo",
    "MethodParametersAttr",
    "ModuleAttr",
    "ModuleMainClassAttr",
    "ModulePackagesAttr",
    "NestHostAttr",
    "NestMembersAttr",
    "NullVariableInfo",
    "ObjectVariableInfo",
    "OffsetTargetInfo",
    "OpensInfo",
    "ParameterAnnotationInfo",
    "PathInfo",
    "PermittedSubclassesAttr",
    "ProvidesInfo",
    "RecordAttr",
    "RecordComponentInfo",
    "RequiresInfo",
    "RuntimeInvisibleAnnotationsAttr",
    "RuntimeInvisibleParameterAnnotationsAttr",
    "RuntimeInvisibleTypeAnnotationsAttr",
    "RuntimeTypeAnnotationsAttr",
    "RuntimeVisibleAnnotationsAttr",
    "RuntimeVisibleParameterAnnotationsAttr",
    "RuntimeVisibleTypeAnnotationsAttr",
    "SameFrameExtendedInfo",
    "SameFrameInfo",
    "SameLocals1StackItemFrameExtendedInfo",
    "SameLocals1StackItemFrameInfo",
    "SignatureAttr",
    "SourceDebugExtensionAttr",
    "SourceFileAttr",
    "StackMapFrameInfo",
    "StackMapTableAttr",
    "SupertypeTargetInfo",
    "SyntheticAttr",
    "TableInfo",
    "TargetInfo",
    "ThrowsTargetInfo",
    "TopVariableInfo",
    "TypeAnnotationInfo",
    "TypeArgumentTargetInfo",
    "TypeParameterBoundTargetInfo",
    "TypeParameterTargetInfo",
    "TypePathInfo",
    "UnimplementedAttr",
    "UninitializedThisVariableInfo",
    "UninitializedVariableInfo",
    "VerificationTypeInfo",
]


@runtime_checkable
class AttributeInfo(Protocol):
    """Common header protocol shared by every parsed classfile attribute."""

    @property
    def attribute_name_index(self) -> int:
        """Constant-pool UTF-8 index of the attribute name."""
        ...

    @property
    def attribute_length(self) -> int:
        """Encoded payload length of this attribute in bytes."""
        ...


@runtime_checkable
class VerificationTypeInfo(Protocol):
    """Protocol for StackMapTable verification type entries."""

    @property
    def tag(self) -> constants.VerificationType:
        """JVMS verification type tag identifying this slot entry."""
        ...


@runtime_checkable
class StackMapFrameInfo(Protocol):
    """Protocol for decoded StackMapTable frame records."""

    @property
    def frame_type(self) -> int:
        """Raw JVMS stack-map frame discriminant byte."""
        ...


class TargetInfo(Protocol):
    """Marker protocol for type-annotation ``target_info`` descriptors."""


AnnotationDefaultAttr = _rust.AnnotationDefaultAttr
AnnotationInfo = _rust.AnnotationInfo
AppendFrameInfo = _rust.AppendFrameInfo
ArrayValueInfo = _rust.ArrayValueInfo
BootstrapMethodInfo = _rust.BootstrapMethodInfo
BootstrapMethodsAttr = _rust.BootstrapMethodsAttr
CatchTargetInfo = _rust.CatchTargetInfo
ChopFrameInfo = _rust.ChopFrameInfo
ClassInfoValueInfo = _rust.ClassInfoValueInfo
CodeAttr = _rust.CodeAttr
ConstValueInfo = _rust.ConstValueInfo
ConstantValueAttr = _rust.ConstantValueAttr
DeprecatedAttr = _rust.DeprecatedAttr
DoubleVariableInfo = _rust.DoubleVariableInfo
ElementValueInfo = _rust.ElementValueInfo
ElementValuePairInfo = _rust.ElementValuePairInfo
EmptyTargetInfo = _rust.EmptyTargetInfo
EnclosingMethodAttr = _rust.EnclosingMethodAttr
EnumConstantValueInfo = _rust.EnumConstantValueInfo
ExceptionsAttr = _rust.ExceptionsAttr
ExportInfo = _rust.ExportInfo
FloatVariableInfo = _rust.FloatVariableInfo
FormalParameterTargetInfo = _rust.FormalParameterTargetInfo
FullFrameInfo = _rust.FullFrameInfo
InnerClassInfo = _rust.InnerClassInfo
InnerClassesAttr = _rust.InnerClassesAttr
IntegerVariableInfo = _rust.IntegerVariableInfo
LineNumberInfo = _rust.LineNumberInfo
LineNumberTableAttr = _rust.LineNumberTableAttr
LocalVariableInfo = _rust.LocalVariableInfo
LocalVariableTableAttr = _rust.LocalVariableTableAttr
LocalVariableTypeInfo = _rust.LocalVariableTypeInfo
LocalVariableTypeTableAttr = _rust.LocalVariableTypeTableAttr
LocalvarTargetInfo = _rust.LocalvarTargetInfo
LongVariableInfo = _rust.LongVariableInfo
MethodParameterInfo = _rust.MethodParameterInfo
MethodParametersAttr = _rust.MethodParametersAttr
ModuleAttr = _rust.ModuleAttr
ModuleMainClassAttr = _rust.ModuleMainClassAttr
ModulePackagesAttr = _rust.ModulePackagesAttr
NestHostAttr = _rust.NestHostAttr
NestMembersAttr = _rust.NestMembersAttr
NullVariableInfo = _rust.NullVariableInfo
ObjectVariableInfo = _rust.ObjectVariableInfo
OffsetTargetInfo = _rust.OffsetTargetInfo
OpensInfo = _rust.OpensInfo
ParameterAnnotationInfo = _rust.ParameterAnnotationInfo
PathInfo = _rust.PathInfo
PermittedSubclassesAttr = _rust.PermittedSubclassesAttr
ProvidesInfo = _rust.ProvidesInfo
RecordAttr = _rust.RecordAttr
RecordComponentInfo = _rust.RecordComponentInfo
RequiresInfo = _rust.RequiresInfo
RuntimeInvisibleAnnotationsAttr = _rust.RuntimeInvisibleAnnotationsAttr
RuntimeInvisibleParameterAnnotationsAttr = _rust.RuntimeInvisibleParameterAnnotationsAttr
RuntimeInvisibleTypeAnnotationsAttr = _rust.RuntimeInvisibleTypeAnnotationsAttr
RuntimeVisibleAnnotationsAttr = _rust.RuntimeVisibleAnnotationsAttr
RuntimeVisibleParameterAnnotationsAttr = _rust.RuntimeVisibleParameterAnnotationsAttr
RuntimeVisibleTypeAnnotationsAttr = _rust.RuntimeVisibleTypeAnnotationsAttr
SameFrameExtendedInfo = _rust.SameFrameExtendedInfo
SameFrameInfo = _rust.SameFrameInfo
SameLocals1StackItemFrameExtendedInfo = _rust.SameLocals1StackItemFrameExtendedInfo
SameLocals1StackItemFrameInfo = _rust.SameLocals1StackItemFrameInfo
SignatureAttr = _rust.SignatureAttr
SourceDebugExtensionAttr = _rust.SourceDebugExtensionAttr
SourceFileAttr = _rust.SourceFileAttr
StackMapTableAttr = _rust.StackMapTableAttr
SupertypeTargetInfo = _rust.SupertypeTargetInfo
SyntheticAttr = _rust.SyntheticAttr
TableInfo = _rust.TableInfo
ThrowsTargetInfo = _rust.ThrowsTargetInfo
TopVariableInfo = _rust.TopVariableInfo
TypeAnnotationInfo = _rust.TypeAnnotationInfo
TypeArgumentTargetInfo = _rust.TypeArgumentTargetInfo
TypeParameterBoundTargetInfo = _rust.TypeParameterBoundTargetInfo
TypeParameterTargetInfo = _rust.TypeParameterTargetInfo
TypePathInfo = _rust.TypePathInfo
UnimplementedAttr = _rust.UnimplementedAttr
UninitializedThisVariableInfo = _rust.UninitializedThisVariableInfo
UninitializedVariableInfo = _rust.UninitializedVariableInfo


@runtime_checkable
class RuntimeTypeAnnotationsAttr(AttributeInfo, Protocol):
    """Protocol shared by runtime-visible and runtime-invisible type annotation attributes."""

    @property
    def num_annotations(self) -> int:
        """Number of type annotations contained in this attribute."""
        ...

    @property
    def annotations(self) -> list[TypeAnnotationInfo]:
        """List of ``TypeAnnotationInfo`` entries."""
        ...


class AttributeInfoType(Enum):
    """Map each JVM attribute name to the wrapper class used for its payload.

    Each enum member exposes ``attr_class`` so callers can look up the Python
    wrapper type associated with a parsed attribute name.
    """

    CONSTANT_VALUE = "ConstantValue", ConstantValueAttr
    CODE = "Code", CodeAttr
    STACK_MAP_TABLE = "StackMapTable", StackMapTableAttr
    EXCEPTIONS = "Exceptions", ExceptionsAttr
    INNER_CLASSES = "InnerClasses", InnerClassesAttr
    ENCLOSING_METHOD = "EnclosingMethod", EnclosingMethodAttr
    SYNTHETIC = "Synthetic", SyntheticAttr
    SIGNATURE = "Signature", SignatureAttr
    SOURCE_FILE = "SourceFile", SourceFileAttr
    SOURCE_DEBUG_EXTENSION = "SourceDebugExtension", SourceDebugExtensionAttr
    LINE_NUMBER_TABLE = "LineNumberTable", LineNumberTableAttr
    LOCAL_VARIABLE_TABLE = "LocalVariableTable", LocalVariableTableAttr
    LOCAL_VARIABLE_TYPE_TABLE = "LocalVariableTypeTable", LocalVariableTypeTableAttr
    DEPRECATED = "Deprecated", DeprecatedAttr
    RUNTIME_VISIBLE_ANNOTATIONS = (
        "RuntimeVisibleAnnotations",
        RuntimeVisibleAnnotationsAttr,
    )
    RUNTIME_INVISIBLE_ANNOTATIONS = (
        "RuntimeInvisibleAnnotations",
        RuntimeInvisibleAnnotationsAttr,
    )
    RUNTIME_VISIBLE_PARAMETER_ANNOTATIONS = (
        "RuntimeVisibleParameterAnnotations",
        RuntimeVisibleParameterAnnotationsAttr,
    )
    RUNTIME_INVISIBLE_PARAMETER_ANNOTATIONS = (
        "RuntimeInvisibleParameterAnnotations",
        RuntimeInvisibleParameterAnnotationsAttr,
    )
    RUNTIME_VISIBLE_TYPE_ANNOTATIONS = (
        "RuntimeVisibleTypeAnnotations",
        RuntimeVisibleTypeAnnotationsAttr,
    )
    RUNTIME_INVISIBLE_TYPE_ANNOTATIONS = (
        "RuntimeInvisibleTypeAnnotations",
        RuntimeInvisibleTypeAnnotationsAttr,
    )
    ANNOTATION_DEFAULT = "AnnotationDefault", AnnotationDefaultAttr
    BOOTSTRAP_METHODS = "BootstrapMethods", BootstrapMethodsAttr
    METHOD_PARAMETERS = "MethodParameters", MethodParametersAttr
    MODULE = "Module", ModuleAttr
    MODULE_PACKAGES = "ModulePackages", ModulePackagesAttr
    MODULE_MAIN_CLASS = "ModuleMainClass", ModuleMainClassAttr
    NEST_HOST = "NestHost", NestHostAttr
    NEST_MEMBERS = "NestMembers", NestMembersAttr
    RECORD = "Record", RecordAttr
    PERMITTED_SUBCLASSES = "PermittedSubclasses", PermittedSubclassesAttr

    UNIMPLEMENTED = "", UnimplementedAttr

    attr_class: type[_rust.RawAttribute]

    def __new__(cls, name: str, attr_class: type[_rust.RawAttribute]) -> AttributeInfoType:
        obj = object.__new__(cls)
        obj._value_ = name
        obj.attr_class = attr_class
        return obj

    @classmethod
    def _missing_(cls, value: object) -> AttributeInfoType | None:
        if isinstance(value, str):
            return cls.UNIMPLEMENTED
        return None
