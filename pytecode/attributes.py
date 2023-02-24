from dataclasses import dataclass
from enum import Enum
from typing import Any, List, Union

from . import constants


@dataclass
class AttributeInfo:
    attribute_name_index: int
    attribute_length: int


@dataclass
class UnimplementedAttr(AttributeInfo):
    info: bytes
    attr_type: "AttributeInfoType"


@dataclass
class ConstantValueAttr(AttributeInfo):
    constantvalue_index: int


@dataclass
class ExceptionInfo:
    start_pc: int
    end_pc: int
    handler_pc: int
    catch_type: int


@dataclass
class CodeAttr(AttributeInfo):
    max_stacks: int
    max_locals: int
    code_length: int
    code: bytes
    exception_table_length: int
    exception_table: List[ExceptionInfo]
    attributes_count: int
    attributes: List[AttributeInfo]


@dataclass
class VerificationTypeInfo:
    tag: constants.VerificationType


@dataclass
class TopVariableInfo(VerificationTypeInfo):
    pass


@dataclass
class IntegerVariableInfo(VerificationTypeInfo):
    pass


@dataclass
class FloatVariableInfo(VerificationTypeInfo):
    pass


@dataclass
class DoubleVariableInfo(VerificationTypeInfo):
    pass


@dataclass
class LongVariableInfo(VerificationTypeInfo):
    pass


@dataclass
class NullVariableInfo(VerificationTypeInfo):
    pass


@dataclass
class UninitializedThisVariableInfo(VerificationTypeInfo):
    pass


@dataclass
class ObjectVariableInfo(VerificationTypeInfo):
    cpool_index: int


@dataclass
class UninitializedVariableInfo(VerificationTypeInfo):
    offset: int


@dataclass
class StackMapFrameInfo:
    frame_type: int


@dataclass
class SameFrameInfo(StackMapFrameInfo):
    pass


@dataclass
class SameLocals1StackItemFrameInfo(StackMapFrameInfo):
    stack: VerificationTypeInfo


@dataclass
class SameLocals1StackItemFrameExtendedInfo(StackMapFrameInfo):
    offset_delta: int
    stack: VerificationTypeInfo


@dataclass
class ChopFrameInfo(StackMapFrameInfo):
    offset_delta: int


@dataclass
class SameFrameExtendedInfo(StackMapFrameInfo):
    offset_delta: int


@dataclass
class AppendFrameInfo(StackMapFrameInfo):
    offset_delta: int
    locals: List[VerificationTypeInfo]


@dataclass
class FullFrameInfo(StackMapFrameInfo):
    offset_delta: int
    number_of_locals: int
    locals: List[VerificationTypeInfo]
    number_of_stack_items: int
    stack: list[VerificationTypeInfo]


@dataclass
class StackMapTableAttr(AttributeInfo):
    number_of_entries: int
    entries: List[StackMapFrameInfo]


@dataclass
class ExceptionsAttr(AttributeInfo):
    number_of_exceptions: int
    exception_index_table: List[int]


@dataclass
class InnerClassInfo:
    inner_class_info_index: int
    outer_class_info_index: int
    inner_name_index: int
    inner_class_access_flags: constants.NestedClassAccessFlag


@dataclass
class InnerClassesAttr(AttributeInfo):
    number_of_classes: int
    classes: List[InnerClassInfo]


@dataclass
class EnclosingMethodAttr(AttributeInfo):
    class_index: int
    method_index: int


@dataclass
class SyntheticAttr(AttributeInfo):
    pass


@dataclass
class SignatureAttr(AttributeInfo):
    signature_index: int


@dataclass
class SourceFileAttr(AttributeInfo):
    sourcefile_index: int


@dataclass
class SourceDebugExtensionAttr(AttributeInfo):
    debug_extension: str


@dataclass
class LineNumberInfo:
    start_pc: int
    line_number: int


@dataclass
class LineNumberTableAttr(AttributeInfo):
    line_number_table_length: int
    line_number_table: List[LineNumberInfo]


@dataclass
class LocalVariableInfo:
    start_pc: int
    length: int
    name_index: int
    descriptor_index: int
    index: int


@dataclass
class LocalVariableTableAttr(AttributeInfo):
    local_variable_table_length: int
    local_variable_table: List[LocalVariableInfo]


@dataclass
class LocalVariableTypeInfo:
    start_pc: int
    length: int
    name_index: int
    signature_index: int
    index: int


@dataclass
class LocalVariableTypeTableAttr(AttributeInfo):
    local_variable_type_table_length: int
    local_variable_type_table: List[LocalVariableTypeInfo]


@dataclass
class DeprecatedAttr(AttributeInfo):
    pass


@dataclass
class ConstValueInfo:
    const_value_index: int


@dataclass
class EnumConstantValueInfo:
    type_name_index: int
    const_name_index: int


@dataclass
class ClassInfoValueInfo:
    class_info_index: int


@dataclass
class ArrayValueInfo:
    num_values: int
    values: List["ElementValueInfo"]


@dataclass
class ElementValueInfo:
    tag: int
    value: Union[ConstValueInfo, EnumConstantValueInfo, ClassInfoValueInfo, "AnnotationInfo", ArrayValueInfo]


@dataclass
class ElementValuePairInfo:
    element_name_index: int
    element_value: ElementValueInfo


@dataclass
class AnnotationInfo:
    type_index: int
    num_element_value_pairs: int
    element_value_pairs: List[ElementValuePairInfo]


@dataclass
class RuntimeVisibleAnnotationsAttr(AttributeInfo):
    num_annotations: int
    annotations: List[AnnotationInfo]


@dataclass
class RuntimeInvisibleAnnotationsAttr(AttributeInfo):
    num_annotations: int
    annotations: List[AnnotationInfo]


@dataclass
class ParameterAnnotationInfo:
    num_annotations: int
    annotations: List[AnnotationInfo]


@dataclass
class RuntimeVisibleParameterAnnotationsAttr(AttributeInfo):
    num_parameters: int
    parameter_annotations: List[ParameterAnnotationInfo]


@dataclass
class RuntimeInvisbleParameterAnnotationsAttr(AttributeInfo):
    num_parameters: int
    parameter_annotations: List[ParameterAnnotationInfo]


@dataclass
class TargetInfo:
    pass


@dataclass
class TypeParameterTargetInfo(TargetInfo):
    type_parameter_index: int


@dataclass
class SupertypeTargetInfo(TargetInfo):
    supertype_index: int


@dataclass
class TypeParameterBoundTargetInfo(TargetInfo):
    type_parameter_index: int
    bound_index: int


@dataclass
class EmptyTargetInfo(TargetInfo):
    pass


@dataclass
class FormalParameterTargetInfo(TargetInfo):
    formal_parameter_index: int


@dataclass
class ThrowsTargetInfo(TargetInfo):
    throws_type_index: int


@dataclass
class TableInfo:
    start_pc: int
    length: int
    index: int


@dataclass
class LocalvarTargetInfo(TargetInfo):
    table_length: int
    table: List[TableInfo]


@dataclass
class CatchTargetInfo(TargetInfo):
    exception_table_index: int


@dataclass
class OffsetTargetInfo(TargetInfo):
    offset: int


@dataclass
class TypeArgumentTargetInfo(TargetInfo):
    offset: int
    type_argument_index: int


@dataclass
class PathInfo:
    type_path_kind: constants.TypePathKind
    type_argument_index: int


@dataclass
class TypePathInfo:
    path_length: int
    path: List[PathInfo]


@dataclass
class TypeAnnotationInfo:
    target_type: constants.TargetType
    target_info: TargetInfo
    target_path: TypePathInfo
    type_index: int
    num_element_value_pairs: int
    element_value_pairs: List[ElementValuePairInfo]


@dataclass
class RuntimeTypeAnnotationsAttr(AttributeInfo):
    num_annotations: int
    annotations: List[TypeAnnotationInfo]


@dataclass
class RuntimeVisibleTypeAnnotationsAttr(RuntimeTypeAnnotationsAttr):
    pass

@dataclass
class RuntimeInvisibleTypeAnnotationsAttr(RuntimeTypeAnnotationsAttr):
    pass


@dataclass
class AnnotationDefaultAttr(AttributeInfo):
    default_value: ElementValueInfo


@dataclass
class BootstrapMethodInfo:
    bootstrap_method_ref: int
    num_boostrap_arguments: int
    boostrap_arguments: List[int]


@dataclass
class BootstrapMethodsAttr(AttributeInfo):
    num_bootstrap_methods: int
    bootstrap_methods: List[BootstrapMethodInfo]


@dataclass
class MethodParameterInfo:
    name_index: int
    access_flags: constants.MethodParameterAccessFlag


@dataclass
class MethodParametersAttr(AttributeInfo):
    parameters_count: int
    parameters: List[MethodParameterInfo]


@dataclass
class RequiresInfo:
    requires_index: int
    requires_flag: constants.ModuleRequiresAccessFlag
    requires_version_index: int


@dataclass
class ExportInfo:
    exports_index: int
    exports_flags: constants.ModuleExportsAccessFlag
    exports_to_count: int
    exports_to_index: List[int]


@dataclass
class OpensInfo:
    opens_index: int
    opens_flags: constants.ModuleOpensAccessFlag
    opens_to_count: int
    opens_to_index: List[int]


@dataclass
class ProvidesInfo:
    provides_index: int
    provides_with_count: int
    provides_with_index: List[int]


@dataclass
class ModuleAttr(AttributeInfo):
    module_name_index: int
    module_flags: constants.ModuleAccessFlag
    module_version_index: int
    requires_count: int
    requires: List[RequiresInfo]
    exports_count: int
    exports: List[ExportInfo]
    opens_count: int
    opens: List[OpensInfo]
    uses_count: int
    uses_index: List[int]
    provides_count: int
    provides: List[ProvidesInfo]


@dataclass
class ModulePackagesAttr(AttributeInfo):
    package_count: int
    package_index: List[int]


@dataclass
class ModuleMainClassAttr(AttributeInfo):
    main_class_index: int


@dataclass
class NestHostAttr(AttributeInfo):
    host_class_index: int


@dataclass
class NestMembersAttr(AttributeInfo):
    number_of_classes: int
    classes: List[int]


@dataclass
class RecordComponentInfo:
    name_index: int
    descriptor_index: int
    attributes_count: int
    attributes: List[AttributeInfo]


@dataclass
class RecordAttr(AttributeInfo):
    components_count: int
    components: List[RecordComponentInfo]


@dataclass
class PermittedSubclassesAttr(AttributeInfo):
    number_of_classes: int
    classes: List[int]


class AttributeInfoType(Enum):
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
        "RuntimeInvisbleParameterAnnotations",
        RuntimeInvisbleParameterAnnotationsAttr,
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

    def __new__(cls, name, attr_class):
        obj = object.__new__(cls)
        obj._value_ = name
        obj.attr_class = attr_class
        return obj

    @classmethod
    def _missing_(cls, value):
        obj = cls.UNIMPLEMENTED
        obj._value_ = value
        return obj
