cdef class AttributeInfo:
    cdef public Py_ssize_t attribute_name_index
    cdef public Py_ssize_t attribute_length


cdef class ExceptionInfo:
    cdef public Py_ssize_t start_pc
    cdef public Py_ssize_t end_pc
    cdef public Py_ssize_t handler_pc
    cdef public Py_ssize_t catch_type


cdef class CodeAttr(AttributeInfo):
    cdef public Py_ssize_t max_stacks
    cdef public Py_ssize_t max_locals
    cdef public Py_ssize_t code_length
    cdef public list code
    cdef public Py_ssize_t exception_table_length
    cdef public list exception_table
    cdef public Py_ssize_t attributes_count
    cdef public list attributes


cdef class VerificationTypeInfo:
    cdef public object tag


cdef class TopVariableInfo(VerificationTypeInfo):
    pass


cdef class IntegerVariableInfo(VerificationTypeInfo):
    pass


cdef class FloatVariableInfo(VerificationTypeInfo):
    pass


cdef class DoubleVariableInfo(VerificationTypeInfo):
    pass


cdef class LongVariableInfo(VerificationTypeInfo):
    pass


cdef class NullVariableInfo(VerificationTypeInfo):
    pass


cdef class UninitializedThisVariableInfo(VerificationTypeInfo):
    pass


cdef class ObjectVariableInfo(VerificationTypeInfo):
    cdef public Py_ssize_t cpool_index


cdef class UninitializedVariableInfo(VerificationTypeInfo):
    cdef public Py_ssize_t offset


cdef class StackMapFrameInfo:
    cdef public Py_ssize_t frame_type


cdef class SameFrameInfo(StackMapFrameInfo):
    pass


cdef class SameLocals1StackItemFrameInfo(StackMapFrameInfo):
    cdef public object stack


cdef class SameLocals1StackItemFrameExtendedInfo(StackMapFrameInfo):
    cdef public Py_ssize_t offset_delta
    cdef public object stack


cdef class ChopFrameInfo(StackMapFrameInfo):
    cdef public Py_ssize_t offset_delta


cdef class SameFrameExtendedInfo(StackMapFrameInfo):
    cdef public Py_ssize_t offset_delta


cdef class AppendFrameInfo(StackMapFrameInfo):
    cdef public Py_ssize_t offset_delta
    cdef public list locals


cdef class FullFrameInfo(StackMapFrameInfo):
    cdef public Py_ssize_t offset_delta
    cdef public Py_ssize_t number_of_locals
    cdef public list locals
    cdef public Py_ssize_t number_of_stack_items
    cdef public list stack


cdef class StackMapTableAttr(AttributeInfo):
    cdef public Py_ssize_t number_of_entries
    cdef public list entries


cdef class LineNumberInfo:
    cdef public Py_ssize_t start_pc
    cdef public Py_ssize_t line_number


cdef class LineNumberTableAttr(AttributeInfo):
    cdef public Py_ssize_t line_number_table_length
    cdef public list line_number_table


cdef class LocalVariableInfo:
    cdef public Py_ssize_t start_pc
    cdef public Py_ssize_t length
    cdef public Py_ssize_t name_index
    cdef public Py_ssize_t descriptor_index
    cdef public Py_ssize_t index


cdef class LocalVariableTableAttr(AttributeInfo):
    cdef public Py_ssize_t local_variable_table_length
    cdef public list local_variable_table


cdef class LocalVariableTypeInfo:
    cdef public Py_ssize_t start_pc
    cdef public Py_ssize_t length
    cdef public Py_ssize_t name_index
    cdef public Py_ssize_t signature_index
    cdef public Py_ssize_t index


cdef class LocalVariableTypeTableAttr(AttributeInfo):
    cdef public Py_ssize_t local_variable_type_table_length
    cdef public list local_variable_type_table


cdef class InnerClassInfo:
    cdef public Py_ssize_t inner_class_info_index
    cdef public Py_ssize_t outer_class_info_index
    cdef public Py_ssize_t inner_name_index
    cdef public object inner_class_access_flags


cdef class _PayloadInfoBase:
    pass


cdef class ConstValueInfo(_PayloadInfoBase):
    cdef public Py_ssize_t const_value_index


cdef class EnumConstantValueInfo(_PayloadInfoBase):
    cdef public Py_ssize_t type_name_index
    cdef public Py_ssize_t const_name_index


cdef class ClassInfoValueInfo(_PayloadInfoBase):
    cdef public Py_ssize_t class_info_index


cdef class ArrayValueInfo(_PayloadInfoBase):
    cdef public Py_ssize_t num_values
    cdef public list values


cdef class ElementValueInfo(_PayloadInfoBase):
    cdef public object tag
    cdef public object value


cdef class ElementValuePairInfo(_PayloadInfoBase):
    cdef public Py_ssize_t element_name_index
    cdef public object element_value


cdef class AnnotationInfo(_PayloadInfoBase):
    cdef public Py_ssize_t type_index
    cdef public Py_ssize_t num_element_value_pairs
    cdef public list element_value_pairs


cdef class ParameterAnnotationInfo(_PayloadInfoBase):
    cdef public Py_ssize_t num_annotations
    cdef public list annotations


cdef class TargetInfo(_PayloadInfoBase):
    pass


cdef class TypeParameterTargetInfo(TargetInfo):
    cdef public Py_ssize_t type_parameter_index


cdef class SupertypeTargetInfo(TargetInfo):
    cdef public Py_ssize_t supertype_index


cdef class TypeParameterBoundTargetInfo(TargetInfo):
    cdef public Py_ssize_t type_parameter_index
    cdef public Py_ssize_t bound_index


cdef class EmptyTargetInfo(TargetInfo):
    pass


cdef class FormalParameterTargetInfo(TargetInfo):
    cdef public Py_ssize_t formal_parameter_index


cdef class ThrowsTargetInfo(TargetInfo):
    cdef public Py_ssize_t throws_type_index


cdef class TableInfo(_PayloadInfoBase):
    cdef public Py_ssize_t start_pc
    cdef public Py_ssize_t length
    cdef public Py_ssize_t index


cdef class LocalvarTargetInfo(TargetInfo):
    cdef public Py_ssize_t table_length
    cdef public list table


cdef class CatchTargetInfo(TargetInfo):
    cdef public Py_ssize_t exception_table_index


cdef class OffsetTargetInfo(TargetInfo):
    cdef public Py_ssize_t offset


cdef class TypeArgumentTargetInfo(TargetInfo):
    cdef public Py_ssize_t offset
    cdef public Py_ssize_t type_argument_index


cdef class PathInfo(_PayloadInfoBase):
    cdef public Py_ssize_t type_path_kind
    cdef public Py_ssize_t type_argument_index


cdef class TypePathInfo(_PayloadInfoBase):
    cdef public Py_ssize_t path_length
    cdef public list path


cdef class TypeAnnotationInfo(_PayloadInfoBase):
    cdef public Py_ssize_t target_type
    cdef public object target_info
    cdef public object target_path
    cdef public Py_ssize_t type_index
    cdef public Py_ssize_t num_element_value_pairs
    cdef public list element_value_pairs


cdef class BootstrapMethodInfo:
    cdef public Py_ssize_t bootstrap_method_ref
    cdef public Py_ssize_t num_boostrap_arguments
    cdef public list boostrap_arguments


cdef class MethodParameterInfo:
    cdef public Py_ssize_t name_index
    cdef public object access_flags


cdef class RecordComponentInfo:
    cdef public Py_ssize_t name_index
    cdef public Py_ssize_t descriptor_index
    cdef public Py_ssize_t attributes_count
    cdef public list attributes


cdef class RequiresInfo:
    cdef public Py_ssize_t requires_index
    cdef public object requires_flag
    cdef public Py_ssize_t requires_version_index


cdef class ExportInfo:
    cdef public Py_ssize_t exports_index
    cdef public object exports_flags
    cdef public Py_ssize_t exports_to_count
    cdef public list exports_to_index


cdef class OpensInfo:
    cdef public Py_ssize_t opens_index
    cdef public object opens_flags
    cdef public Py_ssize_t opens_to_count
    cdef public list opens_to_index


cdef class ProvidesInfo:
    cdef public Py_ssize_t provides_index
    cdef public Py_ssize_t provides_with_count
    cdef public list provides_with_index


cdef class ModuleAttr(AttributeInfo):
    cdef public Py_ssize_t module_name_index
    cdef public object module_flags
    cdef public Py_ssize_t module_version_index
    cdef public Py_ssize_t requires_count
    cdef public list requires
    cdef public Py_ssize_t exports_count
    cdef public list exports
    cdef public Py_ssize_t opens_count
    cdef public list opens
    cdef public Py_ssize_t uses_count
    cdef public list uses_index
    cdef public Py_ssize_t provides_count
    cdef public list provides
