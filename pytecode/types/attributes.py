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
    attr_type: 'AttributeInfoType'


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


# TODO: Finish this
@dataclass
class StackMapFrameInfo:
    pass


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
    debug_extension: bytes


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
class EnumConstantInfo:
    type_name_index: int
    const_name_index: int


@dataclass
class ArrayValueInfo:
    num_values: int
    values: List['ElementValueInfo']


@dataclass
class ElementValueInfo:
    tag: int
    value: Union[int, EnumConstantInfo, 'AnnotationInfo', ArrayValueInfo]


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
class ParameterAnnotation:
    num_annotations: int
    annotations: List[AnnotationInfo]


@dataclass
class RuntimeVisibleParameterAnnotationsAttr(AttributeInfo):
    num_parameters: int
    parameter_annotations: List[ParameterAnnotation]


@dataclass
class RuntimeInvisbleParameterAnnotationsAttr(AttributeInfo):
    num_parameters: int
    parameter_annotations: List[ParameterAnnotation]


# TODO: Finish this
@dataclass
class TypePath:
    pass


# TODO: Finish this
@dataclass
class TypeAnnotation:
    target_type: int
    target_info: Any  # https://docs.oracle.com/javase/specs/jvms/se14/html/jvms-4.html#jvms-4.7.20.1
    target_path: TypePath
    type_index: int
    num_element_value_pairs: int
    element_value_pairs: List[ElementValuePairInfo]


@dataclass
class RuntimeVisibleTypeAnnotationsAttr(AttributeInfo):
    num_annotations: int
    annotations: List[TypeAnnotation]


@dataclass
class RuntimeInvisibleTypeAnnotationsAttr(AttributeInfo):
    num_annotations: int
    annotations: List[TypeAnnotation]


@dataclass
class AnnotationDefaultAttr(AttributeInfo):
    default_value: ElementValueInfo


@dataclass
class BootstrapMethod:
    bootstrap_method_ref: int
    num_boostrap_arguments: int
    boostrap_arguments: List[int]


@dataclass
class BootstrapMethodsAttr(AttributeInfo):
    num_bootstrap_methods: int
    bootstrap_methods: List[BootstrapMethod]


@dataclass
class MethodParameter:
    name_index: int
    access_flags: constants.MethodParameterAccessFlag


@dataclass
class MethodParametersAttr(AttributeInfo):
    parameters_count: int
    parameters: List[MethodParameter]


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
    package_indices: List[int]


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


class AttributeInfoType(Enum):
    CONSTANT_VALUE = 'ConstantValue', ConstantValueAttr
    CODE = 'Code', CodeAttr
    STACK_MAP_TABLE = 'StackMapTable', StackMapTableAttr
    EXCEPTIONS = 'Exceptions', ExceptionsAttr
    INNER_CLASSES = 'InnerClasses', InnerClassesAttr
    ENCLOSING_METHOD = 'EnclosingMethod', EnclosingMethodAttr
    SYNTHETIC = 'Synthetic', SyntheticAttr
    SIGNATURE = 'Signature', SignatureAttr
    SOURCE_FILE = 'SourceFile', SourceFileAttr
    SOURCE_DEBUG_EXTENSION = 'SourceDebugExtension', SourceDebugExtensionAttr
    LINE_NUMBER_TABLE = 'LineNumberTable', LineNumberTableAttr
    LOCAL_VARIABLE_TABLE = 'LocalVariableTable', LocalVariableTableAttr
    LOCAL_VARIABLE_TYPE_TABLE = 'LocalVariableTypeTable', LocalVariableTypeTableAttr
    DEPRECATED = 'Deprecated', DeprecatedAttr
    RUNTIME_VISIBLE_ANNOTATIONS = 'RuntimeVisibleAnnotations', RuntimeVisibleAnnotationsAttr
    RUNTIME_INVISIBLE_ANNOTATIONS = 'RuntimeInvisibleAnnotations', RuntimeInvisibleAnnotationsAttr
    RUNTIME_VISIBLE_PARAMETER_ANNOTATIONS = 'RuntimeVisibleParameterAnnotations', RuntimeVisibleParameterAnnotationsAttr
    RUNTIME_INVISIBLE_PARAMETER_ANNOTATIONS = 'RuntimeInvisbleParameterAnnotations', RuntimeInvisbleParameterAnnotationsAttr
    RUNTIME_VISIBLE_TYPE_ANNOTATIONS = 'RuntimeVisibleTypeAnnotations', RuntimeVisibleTypeAnnotationsAttr
    RUNTIME_INVISIBLE_TYPE_ANNOTATIONS = 'RuntimeInvisibleTypeAnnotations', RuntimeInvisibleTypeAnnotationsAttr
    ANNOTATION_DEFAULT = 'AnnotationDefault', AnnotationDefaultAttr
    BOOTSTRAP_METHODS = 'BootstrapMethods', BootstrapMethodsAttr
    METHOD_PARAMETERS = 'MethodParameters', MethodParametersAttr
    MODULE = 'Module', ModuleAttr
    MODULE_PACKAGES = 'ModulePackages', ModulePackagesAttr
    MODULE_MAIN_CLASS = 'ModuleMainClass', ModuleMainClassAttr
    NEST_HOST = 'NestHost', NestHostAttr
    NEST_MEMBERS = 'NestMembers', NestMembersAttr

    def __new__(cls, name, attr_class):
        obj = object.__new__(cls)
        obj._value_ = name
        obj.attr_class = attr_class
        return obj