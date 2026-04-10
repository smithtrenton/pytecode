"""Dataclass definitions for JVM class file attributes (JVM spec ┬º4.7)."""

from __future__ import annotations

from dataclasses import dataclass
from enum import Enum

from . import constants, instructions

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
    "ExceptionInfo",
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
    """Base class for all JVM class file attribute structures (┬º4.7)."""

    attribute_name_index: int
    attribute_length: int


@dataclass
class UnimplementedAttr(AttributeInfo):
    """Placeholder for attribute types not yet implemented by pytecode."""

    info: bytes
    attr_type: AttributeInfoType


@dataclass
class ConstantValueAttr(AttributeInfo):
    """Represents the ConstantValue attribute (┬º4.7.2)."""

    constantvalue_index: int


@dataclass
class ExceptionInfo:
    """Entry in the Code attribute exception_table (┬º4.7.3)."""

    start_pc: int
    end_pc: int
    handler_pc: int
    catch_type: int


@dataclass
class CodeAttr(AttributeInfo):
    """Represents the Code attribute (┬º4.7.3)."""

    max_stacks: int
    max_locals: int
    code_length: int
    code: list[instructions.InsnInfo]
    exception_table_length: int
    exception_table: list[ExceptionInfo]
    attributes_count: int
    attributes: list[AttributeInfo]


@dataclass
class VerificationTypeInfo:
    """Base class for verification type info entries in StackMapTable frames (┬º4.7.4)."""

    tag: constants.VerificationType


@dataclass
class TopVariableInfo(VerificationTypeInfo):
    """Verification type indicating the top type (┬º4.7.4)."""

    pass


@dataclass
class IntegerVariableInfo(VerificationTypeInfo):
    """Verification type indicating the integer type (┬º4.7.4)."""

    pass


@dataclass
class FloatVariableInfo(VerificationTypeInfo):
    """Verification type indicating the float type (┬º4.7.4)."""

    pass


@dataclass
class DoubleVariableInfo(VerificationTypeInfo):
    """Verification type indicating the double type (┬º4.7.4)."""

    pass


@dataclass
class LongVariableInfo(VerificationTypeInfo):
    """Verification type indicating the long type (┬º4.7.4)."""

    pass


@dataclass
class NullVariableInfo(VerificationTypeInfo):
    """Verification type indicating the null type (┬º4.7.4)."""

    pass


@dataclass
class UninitializedThisVariableInfo(VerificationTypeInfo):
    """Verification type indicating the uninitializedThis type (┬º4.7.4)."""

    pass


@dataclass
class ObjectVariableInfo(VerificationTypeInfo):
    """Verification type indicating an object type (┬º4.7.4)."""

    cpool_index: int


@dataclass
class UninitializedVariableInfo(VerificationTypeInfo):
    """Verification type indicating an uninitialized type (┬º4.7.4)."""

    offset: int


@dataclass
class StackMapFrameInfo:
    """Base class for stack map frame entries in the StackMapTable attribute (┬º4.7.4)."""

    frame_type: int


@dataclass
class SameFrameInfo(StackMapFrameInfo):
    """Stack map frame with the same locals and empty stack (┬º4.7.4)."""

    pass


@dataclass
class SameLocals1StackItemFrameInfo(StackMapFrameInfo):
    """Stack map frame with the same locals and one stack item (┬º4.7.4)."""

    stack: VerificationTypeInfo


@dataclass
class SameLocals1StackItemFrameExtendedInfo(StackMapFrameInfo):
    """Extended same-locals-1-stack-item frame with explicit offset_delta (┬º4.7.4)."""

    offset_delta: int
    stack: VerificationTypeInfo


@dataclass
class ChopFrameInfo(StackMapFrameInfo):
    """Stack map frame indicating removal of locals (┬º4.7.4)."""

    offset_delta: int


@dataclass
class SameFrameExtendedInfo(StackMapFrameInfo):
    """Extended same frame with explicit offset_delta (┬º4.7.4)."""

    offset_delta: int


@dataclass
class AppendFrameInfo(StackMapFrameInfo):
    """Stack map frame indicating additional locals (┬º4.7.4)."""

    offset_delta: int
    locals: list[VerificationTypeInfo]


@dataclass
class FullFrameInfo(StackMapFrameInfo):
    """Full stack map frame with explicit locals and stack (┬º4.7.4)."""

    offset_delta: int
    number_of_locals: int
    locals: list[VerificationTypeInfo]
    number_of_stack_items: int
    stack: list[VerificationTypeInfo]


@dataclass
class StackMapTableAttr(AttributeInfo):
    """Represents the StackMapTable attribute (┬º4.7.4)."""

    number_of_entries: int
    entries: list[StackMapFrameInfo]


@dataclass
class ExceptionsAttr(AttributeInfo):
    """Represents the Exceptions attribute (┬º4.7.5)."""

    number_of_exceptions: int
    exception_index_table: list[int]


@dataclass
class InnerClassInfo:
    """Entry in the InnerClasses attribute classes table (┬º4.7.6)."""

    inner_class_info_index: int
    outer_class_info_index: int
    inner_name_index: int
    inner_class_access_flags: constants.NestedClassAccessFlag


@dataclass
class InnerClassesAttr(AttributeInfo):
    """Represents the InnerClasses attribute (┬º4.7.6)."""

    number_of_classes: int
    classes: list[InnerClassInfo]


@dataclass
class EnclosingMethodAttr(AttributeInfo):
    """Represents the EnclosingMethod attribute (┬º4.7.7)."""

    class_index: int
    method_index: int


@dataclass
class SyntheticAttr(AttributeInfo):
    """Represents the Synthetic attribute (┬º4.7.8)."""

    pass


@dataclass
class SignatureAttr(AttributeInfo):
    """Represents the Signature attribute (┬º4.7.9)."""

    signature_index: int


@dataclass
class SourceFileAttr(AttributeInfo):
    """Represents the SourceFile attribute (┬º4.7.10)."""

    sourcefile_index: int


@dataclass
class SourceDebugExtensionAttr(AttributeInfo):
    """Represents the SourceDebugExtension attribute (┬º4.7.11)."""

    debug_extension: str


@dataclass
class LineNumberInfo:
    """Entry in the LineNumberTable attribute (┬º4.7.12)."""

    start_pc: int
    line_number: int


@dataclass
class LineNumberTableAttr(AttributeInfo):
    """Represents the LineNumberTable attribute (┬º4.7.12)."""

    line_number_table_length: int
    line_number_table: list[LineNumberInfo]


@dataclass
class LocalVariableInfo:
    """Entry in the LocalVariableTable attribute (┬º4.7.13)."""

    start_pc: int
    length: int
    name_index: int
    descriptor_index: int
    index: int


@dataclass
class LocalVariableTableAttr(AttributeInfo):
    """Represents the LocalVariableTable attribute (┬º4.7.13)."""

    local_variable_table_length: int
    local_variable_table: list[LocalVariableInfo]


@dataclass
class LocalVariableTypeInfo:
    """Entry in the LocalVariableTypeTable attribute (┬º4.7.14)."""

    start_pc: int
    length: int
    name_index: int
    signature_index: int
    index: int


@dataclass
class LocalVariableTypeTableAttr(AttributeInfo):
    """Represents the LocalVariableTypeTable attribute (┬º4.7.14)."""

    local_variable_type_table_length: int
    local_variable_type_table: list[LocalVariableTypeInfo]


@dataclass
class DeprecatedAttr(AttributeInfo):
    """Represents the Deprecated attribute (┬º4.7.15)."""

    pass


@dataclass
class ConstValueInfo:
    """Constant value in an element_value structure (┬º4.7.16.1)."""

    const_value_index: int


@dataclass
class EnumConstantValueInfo:
    """Enum constant value in an element_value structure (┬º4.7.16.1)."""

    type_name_index: int
    const_name_index: int


@dataclass
class ClassInfoValueInfo:
    """Class literal value in an element_value structure (┬º4.7.16.1)."""

    class_info_index: int


@dataclass
class ArrayValueInfo:
    """Array value in an element_value structure (┬º4.7.16.1)."""

    num_values: int
    values: list[ElementValueInfo]


@dataclass
class ElementValueInfo:
    """Represents an element_value structure (┬º4.7.16.1)."""

    tag: int | str
    value: ConstValueInfo | EnumConstantValueInfo | ClassInfoValueInfo | AnnotationInfo | ArrayValueInfo


@dataclass
class ElementValuePairInfo:
    """Represents an element-value pair in an annotation (┬º4.7.16)."""

    element_name_index: int
    element_value: ElementValueInfo


@dataclass
class AnnotationInfo:
    """Represents an annotation structure (┬º4.7.16)."""

    type_index: int
    num_element_value_pairs: int
    element_value_pairs: list[ElementValuePairInfo]


@dataclass
class RuntimeVisibleAnnotationsAttr(AttributeInfo):
    """Represents the RuntimeVisibleAnnotations attribute (┬º4.7.16)."""

    num_annotations: int
    annotations: list[AnnotationInfo]


@dataclass
class RuntimeInvisibleAnnotationsAttr(AttributeInfo):
    """Represents the RuntimeInvisibleAnnotations attribute (┬º4.7.17)."""

    num_annotations: int
    annotations: list[AnnotationInfo]


@dataclass
class ParameterAnnotationInfo:
    """Annotations for a single parameter (┬º4.7.18)."""

    num_annotations: int
    annotations: list[AnnotationInfo]


@dataclass
class RuntimeVisibleParameterAnnotationsAttr(AttributeInfo):
    """Represents the RuntimeVisibleParameterAnnotations attribute (┬º4.7.18)."""

    num_parameters: int
    parameter_annotations: list[ParameterAnnotationInfo]


@dataclass
class RuntimeInvisibleParameterAnnotationsAttr(AttributeInfo):
    """Represents the RuntimeInvisibleParameterAnnotations attribute (┬º4.7.19)."""

    num_parameters: int
    parameter_annotations: list[ParameterAnnotationInfo]


@dataclass
class TargetInfo:
    """Base class for type annotation target_info union variants (┬º4.7.20.1)."""

    pass


@dataclass
class TypeParameterTargetInfo(TargetInfo):
    """Target info for type parameter declarations (┬º4.7.20.1)."""

    type_parameter_index: int


@dataclass
class SupertypeTargetInfo(TargetInfo):
    """Target info for extends/implements clauses (┬º4.7.20.1)."""

    supertype_index: int


@dataclass
class TypeParameterBoundTargetInfo(TargetInfo):
    """Target info for type parameter bounds (┬º4.7.20.1)."""

    type_parameter_index: int
    bound_index: int


@dataclass
class EmptyTargetInfo(TargetInfo):
    """Target info for return types, receiver types, or field types (┬º4.7.20.1)."""

    pass


@dataclass
class FormalParameterTargetInfo(TargetInfo):
    """Target info for formal parameter declarations (┬º4.7.20.1)."""

    formal_parameter_index: int


@dataclass
class ThrowsTargetInfo(TargetInfo):
    """Target info for throws clause types (┬º4.7.20.1)."""

    throws_type_index: int


@dataclass
class TableInfo:
    """Entry in the localvar_target table (┬º4.7.20.1)."""

    start_pc: int
    length: int
    index: int


@dataclass
class LocalvarTargetInfo(TargetInfo):
    """Target info for local variable type annotations (┬º4.7.20.1)."""

    table_length: int
    table: list[TableInfo]


@dataclass
class CatchTargetInfo(TargetInfo):
    """Target info for exception parameter types (┬º4.7.20.1)."""

    exception_table_index: int


@dataclass
class OffsetTargetInfo(TargetInfo):
    """Target info for instanceof, new, or method reference expressions (┬º4.7.20.1)."""

    offset: int


@dataclass
class TypeArgumentTargetInfo(TargetInfo):
    """Target info for cast or type argument expressions (┬º4.7.20.1)."""

    offset: int
    type_argument_index: int


@dataclass
class PathInfo:
    """Single entry in a type_path structure (┬º4.7.20.2)."""

    type_path_kind: int
    type_argument_index: int


@dataclass
class TypePathInfo:
    """Represents a type_path structure (┬º4.7.20.2)."""

    path_length: int
    path: list[PathInfo]


@dataclass
class TypeAnnotationInfo:
    """Represents a type_annotation structure (┬º4.7.20)."""

    target_type: int
    target_info: TargetInfo
    target_path: TypePathInfo
    type_index: int
    num_element_value_pairs: int
    element_value_pairs: list[ElementValuePairInfo]


@dataclass
class RuntimeTypeAnnotationsAttr(AttributeInfo):
    """Base class for RuntimeVisibleTypeAnnotations and RuntimeInvisibleTypeAnnotations."""

    num_annotations: int
    annotations: list[TypeAnnotationInfo]


@dataclass
class RuntimeVisibleTypeAnnotationsAttr(RuntimeTypeAnnotationsAttr):
    """Represents the RuntimeVisibleTypeAnnotations attribute (┬º4.7.20)."""

    pass


@dataclass
class RuntimeInvisibleTypeAnnotationsAttr(RuntimeTypeAnnotationsAttr):
    """Represents the RuntimeInvisibleTypeAnnotations attribute (┬º4.7.21)."""

    pass


@dataclass
class AnnotationDefaultAttr(AttributeInfo):
    """Represents the AnnotationDefault attribute (┬º4.7.22)."""

    default_value: ElementValueInfo


@dataclass
class BootstrapMethodInfo:
    """Entry in the BootstrapMethods attribute (┬º4.7.23)."""

    bootstrap_method_ref: int
    num_boostrap_arguments: int
    boostrap_arguments: list[int]


@dataclass
class BootstrapMethodsAttr(AttributeInfo):
    """Represents the BootstrapMethods attribute (┬º4.7.23)."""

    num_bootstrap_methods: int
    bootstrap_methods: list[BootstrapMethodInfo]


@dataclass
class MethodParameterInfo:
    """Entry in the MethodParameters attribute (┬º4.7.24)."""

    name_index: int
    access_flags: constants.MethodParameterAccessFlag


@dataclass
class MethodParametersAttr(AttributeInfo):
    """Represents the MethodParameters attribute (┬º4.7.24)."""

    parameters_count: int
    parameters: list[MethodParameterInfo]


@dataclass
class RequiresInfo:
    """Entry in the Module attribute requires table (┬º4.7.25)."""

    requires_index: int
    requires_flag: constants.ModuleRequiresAccessFlag
    requires_version_index: int


@dataclass
class ExportInfo:
    """Entry in the Module attribute exports table (┬º4.7.25)."""

    exports_index: int
    exports_flags: constants.ModuleExportsAccessFlag
    exports_to_count: int
    exports_to_index: list[int]


@dataclass
class OpensInfo:
    """Entry in the Module attribute opens table (┬º4.7.25)."""

    opens_index: int
    opens_flags: constants.ModuleOpensAccessFlag
    opens_to_count: int
    opens_to_index: list[int]


@dataclass
class ProvidesInfo:
    """Entry in the Module attribute provides table (┬º4.7.25)."""

    provides_index: int
    provides_with_count: int
    provides_with_index: list[int]


@dataclass
class ModuleAttr(AttributeInfo):
    """Represents the Module attribute (┬º4.7.25)."""

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
    """Represents the ModulePackages attribute (┬º4.7.26)."""

    package_count: int
    package_index: list[int]


@dataclass
class ModuleMainClassAttr(AttributeInfo):
    """Represents the ModuleMainClass attribute (┬º4.7.27)."""

    main_class_index: int


@dataclass
class NestHostAttr(AttributeInfo):
    """Represents the NestHost attribute (┬º4.7.28)."""

    host_class_index: int


@dataclass
class NestMembersAttr(AttributeInfo):
    """Represents the NestMembers attribute (┬º4.7.29)."""

    number_of_classes: int
    classes: list[int]


@dataclass
class RecordComponentInfo:
    """Describes a single record component in the Record attribute (┬º4.7.30)."""

    name_index: int
    descriptor_index: int
    attributes_count: int
    attributes: list[AttributeInfo]


@dataclass
class RecordAttr(AttributeInfo):
    """Represents the Record attribute (┬º4.7.30)."""

    components_count: int
    components: list[RecordComponentInfo]


@dataclass
class PermittedSubclassesAttr(AttributeInfo):
    """Represents the PermittedSubclasses attribute (┬º4.7.31)."""

    number_of_classes: int
    classes: list[int]


class AttributeInfoType(Enum):
    """Enum mapping JVM attribute names to their dataclass types.

    Attributes:
        attr_class: The dataclass type that represents this attribute.
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
