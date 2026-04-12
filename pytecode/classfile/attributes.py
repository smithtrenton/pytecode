"""Dataclass wrappers for parsed JVM classfile attributes.

These types expose the raw attribute payloads returned by the Rust classfile
reader. The field names intentionally stay close to the JVM layout so you can
cross-reference them with the class file format when needed.
"""

from __future__ import annotations

from dataclasses import dataclass
from enum import Enum
from typing import TYPE_CHECKING

from . import constants

if TYPE_CHECKING:
    from . import ExceptionInfo, InsnInfo

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


@dataclass
class AttributeInfo:
    """Common header shared by every parsed classfile attribute."""

    attribute_name_index: int
    attribute_length: int


@dataclass
class UnimplementedAttr(AttributeInfo):
    """Placeholder for attribute types not yet implemented by pytecode."""

    info: bytes
    attr_type: AttributeInfoType


@dataclass
class ConstantValueAttr(AttributeInfo):
    """Constant value attached to a field declaration."""

    constantvalue_index: int


@dataclass
class CodeAttr(AttributeInfo):
    """Method body, exception handlers, and nested code-scoped attributes."""

    max_stacks: int
    max_locals: int
    code_length: int
    code: list[InsnInfo]
    exception_table_length: int
    exception_table: list[ExceptionInfo]
    attributes_count: int
    attributes: list[AttributeInfo]


@dataclass
class VerificationTypeInfo:
    """Base wrapper for StackMapTable locals and operand-stack entries."""

    tag: constants.VerificationType


@dataclass
class TopVariableInfo(VerificationTypeInfo):
    """Stack-map placeholder for an unusable or missing slot."""

    pass


@dataclass
class IntegerVariableInfo(VerificationTypeInfo):
    """Stack-map entry for an ``int`` value."""

    pass


@dataclass
class FloatVariableInfo(VerificationTypeInfo):
    """Stack-map entry for a ``float`` value."""

    pass


@dataclass
class DoubleVariableInfo(VerificationTypeInfo):
    """Stack-map entry for a ``double`` value."""

    pass


@dataclass
class LongVariableInfo(VerificationTypeInfo):
    """Stack-map entry for a ``long`` value."""

    pass


@dataclass
class NullVariableInfo(VerificationTypeInfo):
    """Stack-map entry for the ``null`` reference."""

    pass


@dataclass
class UninitializedThisVariableInfo(VerificationTypeInfo):
    """Stack-map entry for ``this`` before superclass construction completes."""

    pass


@dataclass
class ObjectVariableInfo(VerificationTypeInfo):
    """Stack-map entry for an initialized reference type."""

    cpool_index: int


@dataclass
class UninitializedVariableInfo(VerificationTypeInfo):
    """Stack-map entry for a ``new`` object before constructor invocation."""

    offset: int


@dataclass
class StackMapFrameInfo:
    """Base wrapper for one StackMapTable frame record."""

    frame_type: int


@dataclass
class SameFrameInfo(StackMapFrameInfo):
    """Frame that keeps prior locals and records an empty operand stack."""

    pass


@dataclass
class SameLocals1StackItemFrameInfo(StackMapFrameInfo):
    """Frame that keeps prior locals and records one operand-stack entry."""

    stack: VerificationTypeInfo


@dataclass
class SameLocals1StackItemFrameExtendedInfo(StackMapFrameInfo):
    """Same-locals-one-stack-item frame that stores an explicit ``offset_delta``."""

    offset_delta: int
    stack: VerificationTypeInfo


@dataclass
class ChopFrameInfo(StackMapFrameInfo):
    """Frame that drops one to three trailing locals from the previous state."""

    offset_delta: int


@dataclass
class SameFrameExtendedInfo(StackMapFrameInfo):
    """Same-frame variant with an explicit ``offset_delta``."""

    offset_delta: int


@dataclass
class AppendFrameInfo(StackMapFrameInfo):
    """Frame that appends one to three locals to the previous state."""

    offset_delta: int
    locals: list[VerificationTypeInfo]


@dataclass
class FullFrameInfo(StackMapFrameInfo):
    """Frame that spells out the complete local and operand-stack state."""

    offset_delta: int
    number_of_locals: int
    locals: list[VerificationTypeInfo]
    number_of_stack_items: int
    stack: list[VerificationTypeInfo]


@dataclass
class StackMapTableAttr(AttributeInfo):
    """Verification frames used by the bytecode verifier."""

    number_of_entries: int
    entries: list[StackMapFrameInfo]


@dataclass
class ExceptionsAttr(AttributeInfo):
    """Checked exception types declared by a method."""

    number_of_exceptions: int
    exception_index_table: list[int]


@dataclass
class InnerClassInfo:
    """One inner-class relationship recorded in ``InnerClasses``."""

    inner_class_info_index: int
    outer_class_info_index: int
    inner_name_index: int
    inner_class_access_flags: constants.NestedClassAccessFlag


@dataclass
class InnerClassesAttr(AttributeInfo):
    """Inner and nested class metadata declared for the current class."""

    number_of_classes: int
    classes: list[InnerClassInfo]


@dataclass
class EnclosingMethodAttr(AttributeInfo):
    """Owner class and method for a local or anonymous class."""

    class_index: int
    method_index: int


@dataclass
class SyntheticAttr(AttributeInfo):
    """Marker attribute for compiler-synthesized declarations."""

    pass


@dataclass
class SignatureAttr(AttributeInfo):
    """Generic signature string for a class, field, method, or record component."""

    signature_index: int


@dataclass
class SourceFileAttr(AttributeInfo):
    """Original source file name recorded for the class."""

    sourcefile_index: int


@dataclass
class SourceDebugExtensionAttr(AttributeInfo):
    """Extended source-debug payload such as SMAP data."""

    debug_extension: str


@dataclass
class LineNumberInfo:
    """Mapping from bytecode offset to source line number."""

    start_pc: int
    line_number: int


@dataclass
class LineNumberTableAttr(AttributeInfo):
    """Source line mappings for a method body."""

    line_number_table_length: int
    line_number_table: list[LineNumberInfo]


@dataclass
class LocalVariableInfo:
    """Named local-variable slot with descriptor-based type information."""

    start_pc: int
    length: int
    name_index: int
    descriptor_index: int
    index: int


@dataclass
class LocalVariableTableAttr(AttributeInfo):
    """Debug names and descriptors for local-variable slots."""

    local_variable_table_length: int
    local_variable_table: list[LocalVariableInfo]


@dataclass
class LocalVariableTypeInfo:
    """Named local-variable slot with generic signature information."""

    start_pc: int
    length: int
    name_index: int
    signature_index: int
    index: int


@dataclass
class LocalVariableTypeTableAttr(AttributeInfo):
    """Generic-signature companion to ``LocalVariableTable``."""

    local_variable_type_table_length: int
    local_variable_type_table: list[LocalVariableTypeInfo]


@dataclass
class DeprecatedAttr(AttributeInfo):
    """Marker attribute declaring the class member deprecated."""

    pass


@dataclass
class ConstValueInfo:
    """Annotation element whose value comes from the constant pool."""

    const_value_index: int


@dataclass
class EnumConstantValueInfo:
    """Annotation element naming a specific enum constant."""

    type_name_index: int
    const_name_index: int


@dataclass
class ClassInfoValueInfo:
    """Annotation element storing a class literal."""

    class_info_index: int


@dataclass
class ArrayValueInfo:
    """Annotation element storing an ordered list of values."""

    num_values: int
    values: list[ElementValueInfo]


@dataclass
class ElementValueInfo:
    """Tagged union for one annotation element value."""

    tag: int | str
    value: ConstValueInfo | EnumConstantValueInfo | ClassInfoValueInfo | AnnotationInfo | ArrayValueInfo


@dataclass
class ElementValuePairInfo:
    """Named value supplied to an annotation element."""

    element_name_index: int
    element_value: ElementValueInfo


@dataclass
class AnnotationInfo:
    """Materialized annotation instance and its element assignments."""

    type_index: int
    num_element_value_pairs: int
    element_value_pairs: list[ElementValuePairInfo]


@dataclass
class RuntimeVisibleAnnotationsAttr(AttributeInfo):
    """Annotations retained in the runtime-visible set."""

    num_annotations: int
    annotations: list[AnnotationInfo]


@dataclass
class RuntimeInvisibleAnnotationsAttr(AttributeInfo):
    """Annotations stored in the runtime-invisible set."""

    num_annotations: int
    annotations: list[AnnotationInfo]


@dataclass
class ParameterAnnotationInfo:
    """Annotation list for one method or constructor parameter."""

    num_annotations: int
    annotations: list[AnnotationInfo]


@dataclass
class RuntimeVisibleParameterAnnotationsAttr(AttributeInfo):
    """Runtime-visible annotations grouped by parameter position."""

    num_parameters: int
    parameter_annotations: list[ParameterAnnotationInfo]


@dataclass
class RuntimeInvisibleParameterAnnotationsAttr(AttributeInfo):
    """Runtime-invisible annotations grouped by parameter position."""

    num_parameters: int
    parameter_annotations: list[ParameterAnnotationInfo]


@dataclass
class TargetInfo:
    """Base wrapper for the target selected by a type annotation."""

    pass


@dataclass
class TypeParameterTargetInfo(TargetInfo):
    """Type annotation attached to a type parameter declaration."""

    type_parameter_index: int


@dataclass
class SupertypeTargetInfo(TargetInfo):
    """Type annotation attached to a superclass or interface reference."""

    supertype_index: int


@dataclass
class TypeParameterBoundTargetInfo(TargetInfo):
    """Type annotation attached to one bound of a type parameter."""

    type_parameter_index: int
    bound_index: int


@dataclass
class EmptyTargetInfo(TargetInfo):
    """Target marker for sites that need no extra coordinates."""

    pass


@dataclass
class FormalParameterTargetInfo(TargetInfo):
    """Type annotation attached to a formal parameter declaration."""

    formal_parameter_index: int


@dataclass
class ThrowsTargetInfo(TargetInfo):
    """Type annotation attached to one declared thrown type."""

    throws_type_index: int


@dataclass
class TableInfo:
    """Range and slot index for a local-variable type-annotation target."""

    start_pc: int
    length: int
    index: int


@dataclass
class LocalvarTargetInfo(TargetInfo):
    """Type annotation attached to one or more local-variable ranges."""

    table_length: int
    table: list[TableInfo]


@dataclass
class CatchTargetInfo(TargetInfo):
    """Type annotation attached to an exception handler parameter."""

    exception_table_index: int


@dataclass
class OffsetTargetInfo(TargetInfo):
    """Type annotation located at a single bytecode instruction offset."""

    offset: int


@dataclass
class TypeArgumentTargetInfo(TargetInfo):
    """Type annotation attached to one type argument at a bytecode offset."""

    offset: int
    type_argument_index: int


@dataclass
class PathInfo:
    """One navigation step within a nested or parameterized type."""

    type_path_kind: int
    type_argument_index: int


@dataclass
class TypePathInfo:
    """Path that identifies which nested part of a type is annotated."""

    path_length: int
    path: list[PathInfo]


@dataclass
class TypeAnnotationInfo:
    """Fully decoded type annotation with target, path, and element values."""

    target_type: int
    target_info: TargetInfo
    target_path: TypePathInfo
    type_index: int
    num_element_value_pairs: int
    element_value_pairs: list[ElementValuePairInfo]


@dataclass
class RuntimeTypeAnnotationsAttr(AttributeInfo):
    """Shared payload for visible and invisible runtime type annotations."""

    num_annotations: int
    annotations: list[TypeAnnotationInfo]


@dataclass
class RuntimeVisibleTypeAnnotationsAttr(RuntimeTypeAnnotationsAttr):
    """Runtime-visible type annotations."""

    pass


@dataclass
class RuntimeInvisibleTypeAnnotationsAttr(RuntimeTypeAnnotationsAttr):
    """Runtime-invisible type annotations."""

    pass


@dataclass
class AnnotationDefaultAttr(AttributeInfo):
    """Default value recorded for an annotation element method."""

    default_value: ElementValueInfo


@dataclass
class BootstrapMethodInfo:
    """One bootstrap method and its constant-pool arguments."""

    bootstrap_method_ref: int
    num_boostrap_arguments: int
    boostrap_arguments: list[int]


@dataclass
class BootstrapMethodsAttr(AttributeInfo):
    """Bootstrap method table used by invokedynamic and dynamic constants."""

    num_bootstrap_methods: int
    bootstrap_methods: list[BootstrapMethodInfo]


@dataclass
class MethodParameterInfo:
    """Name and flags for one method parameter."""

    name_index: int
    access_flags: constants.MethodParameterAccessFlag


@dataclass
class MethodParametersAttr(AttributeInfo):
    """Parameter names and flags recorded on a method."""

    parameters_count: int
    parameters: list[MethodParameterInfo]


@dataclass
class RequiresInfo:
    """One ``requires`` directive inside a module declaration."""

    requires_index: int
    requires_flag: constants.ModuleRequiresAccessFlag
    requires_version_index: int


@dataclass
class ExportInfo:
    """One ``exports`` directive inside a module declaration."""

    exports_index: int
    exports_flags: constants.ModuleExportsAccessFlag
    exports_to_count: int
    exports_to_index: list[int]


@dataclass
class OpensInfo:
    """One ``opens`` directive inside a module declaration."""

    opens_index: int
    opens_flags: constants.ModuleOpensAccessFlag
    opens_to_count: int
    opens_to_index: list[int]


@dataclass
class ProvidesInfo:
    """One ``provides ... with ...`` directive inside a module declaration."""

    provides_index: int
    provides_with_count: int
    provides_with_index: list[int]


@dataclass
class ModuleAttr(AttributeInfo):
    """Structured contents of a ``module-info`` declaration."""

    module_name_index: int
    module_flags: constants.ModuleAccessFlag
    module_version_index: int
    requires_count: int
    requires: list[RequiresInfo]
    exports_count: int
    exports: list[ExportInfo]
    opens_count: int
    opens: list[OpensInfo]
    uses_count: int
    uses_index: list[int]
    provides_count: int
    provides: list[ProvidesInfo]


@dataclass
class ModulePackagesAttr(AttributeInfo):
    """Package list associated with a module declaration."""

    package_count: int
    package_index: list[int]


@dataclass
class ModuleMainClassAttr(AttributeInfo):
    """Main class recorded for a module."""

    main_class_index: int


@dataclass
class NestHostAttr(AttributeInfo):
    """Host class reference for a nest member."""

    host_class_index: int


@dataclass
class NestMembersAttr(AttributeInfo):
    """List of classes that belong to the same nest."""

    number_of_classes: int
    classes: list[int]


@dataclass
class RecordComponentInfo:
    """Name, descriptor, and attributes for one record component."""

    name_index: int
    descriptor_index: int
    attributes_count: int
    attributes: list[AttributeInfo]


@dataclass
class RecordAttr(AttributeInfo):
    """Record component list for a record class."""

    components_count: int
    components: list[RecordComponentInfo]


@dataclass
class PermittedSubclassesAttr(AttributeInfo):
    """Sealed-class permit list."""

    number_of_classes: int
    classes: list[int]


class AttributeInfoType(Enum):
    """Map each JVM attribute name to the dataclass used for its payload.

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

    attr_class: type[AttributeInfo]

    def __new__(cls, name: str, attr_class: type[AttributeInfo]) -> AttributeInfoType:
        obj = object.__new__(cls)
        obj._value_ = name
        obj.attr_class = attr_class
        return obj

    @classmethod
    def _missing_(cls, value: object) -> AttributeInfoType:
        obj = cls.UNIMPLEMENTED
        obj._value_ = value
        return obj
