# cython: boundscheck=False, wraparound=False, cdivision=True
"""Cython-accelerated attribute cloning for JVM class file attributes.

Drop-in replacement for ``_attribute_clone_py`` with ``cdef`` internal
dispatch functions for reduced Python call overhead and faster
``isinstance()`` checks.
"""

from dataclasses import fields
from functools import cache

from ..classfile._attributes_cy cimport (
    AppendFrameInfo as CAppendFrameInfo,
    BootstrapMethodInfo as CBootstrapMethodInfo,
    ChopFrameInfo as CChopFrameInfo,
    CodeAttr as CCodeAttr,
    FullFrameInfo as CFullFrameInfo,
    ObjectVariableInfo as CObjectVariableInfo,
    ExportInfo as CExportInfo,
    ExceptionInfo as CExceptionInfo,
    InnerClassInfo as CInnerClassInfo,
    LineNumberInfo as CLineNumberInfo,
    LineNumberTableAttr as CLineNumberTableAttr,
    LocalVariableInfo as CLocalVariableInfo,
    LocalVariableTableAttr as CLocalVariableTableAttr,
    LocalVariableTypeInfo as CLocalVariableTypeInfo,
    LocalVariableTypeTableAttr as CLocalVariableTypeTableAttr,
    MethodParameterInfo as CMethodParameterInfo,
    ModuleAttr as CModuleAttr,
    OpensInfo as COpensInfo,
    ProvidesInfo as CProvidesInfo,
    RecordComponentInfo as CRecordComponentInfo,
    RequiresInfo as CRequiresInfo,
    SameFrameExtendedInfo as CSameFrameExtendedInfo,
    SameLocals1StackItemFrameExtendedInfo as CSameLocals1StackItemFrameExtendedInfo,
    SameLocals1StackItemFrameInfo as CSameLocals1StackItemFrameInfo,
    StackMapFrameInfo as CStackMapFrameInfo,
    StackMapTableAttr as CStackMapTableAttr,
    UninitializedVariableInfo as CUninitializedVariableInfo,
    VerificationTypeInfo as CVerificationTypeInfo,
)
from ..classfile.attributes import (
    AnnotationInfo,
    AppendFrameInfo,
    ArrayValueInfo,
    AttributeInfo,
    BootstrapMethodInfo,
    BootstrapMethodsAttr,
    CatchTargetInfo,
    ChopFrameInfo,
    ClassInfoValueInfo,
    ConstantValueAttr,
    ConstValueInfo,
    CodeAttr,
    DeprecatedAttr,
    DoubleVariableInfo,
    ElementValueInfo,
    ElementValuePairInfo,
    EmptyTargetInfo,
    EnclosingMethodAttr,
    EnumConstantValueInfo,
    ExportInfo,
    ExceptionInfo,
    ExceptionsAttr,
    FloatVariableInfo,
    FormalParameterTargetInfo,
    FullFrameInfo,
    InnerClassesAttr,
    InnerClassInfo,
    IntegerVariableInfo,
    LineNumberInfo,
    LineNumberTableAttr,
    LocalVariableInfo,
    LocalVariableTableAttr,
    LocalVariableTypeInfo,
    LocalVariableTypeTableAttr,
    LocalvarTargetInfo,
    LongVariableInfo,
    MethodParameterInfo,
    MethodParametersAttr,
    ModuleAttr,
    ModuleMainClassAttr,
    ModulePackagesAttr,
    NestHostAttr,
    NestMembersAttr,
    NullVariableInfo,
    ObjectVariableInfo,
    OffsetTargetInfo,
    OpensInfo,
    ParameterAnnotationInfo,
    PermittedSubclassesAttr,
    ProvidesInfo,
    RecordAttr,
    RecordComponentInfo,
    RequiresInfo,
    RuntimeInvisibleAnnotationsAttr,
    RuntimeInvisibleParameterAnnotationsAttr,
    RuntimeInvisibleTypeAnnotationsAttr,
    RuntimeVisibleAnnotationsAttr,
    RuntimeVisibleParameterAnnotationsAttr,
    RuntimeVisibleTypeAnnotationsAttr,
    SameFrameExtendedInfo,
    SameFrameInfo,
    SameLocals1StackItemFrameExtendedInfo,
    SameLocals1StackItemFrameInfo,
    SignatureAttr,
    SourceDebugExtensionAttr,
    SourceFileAttr,
    StackMapFrameInfo,
    StackMapTableAttr,
    SupertypeTargetInfo,
    SyntheticAttr,
    TableInfo,
    ThrowsTargetInfo,
    TopVariableInfo,
    TypeAnnotationInfo,
    TypeArgumentTargetInfo,
    TypeParameterBoundTargetInfo,
    TypeParameterTargetInfo,
    TypePathInfo,
    UninitializedThisVariableInfo,
    UninitializedVariableInfo,
    VerificationTypeInfo,
)


@cache
def _clone_field_names(type cls):
    return tuple(field.name for field in fields(cls))


cdef inline list _clone_verification_type_list(list values):
    cdef Py_ssize_t index, count
    cdef list cloned
    count = len(values)
    cloned = [None] * count
    for index in range(count):
        cloned[index] = _clone_verification_type(values[index])
    return cloned


cdef inline list _clone_stack_map_frame_list(list entries):
    cdef Py_ssize_t index, count
    cdef list cloned
    count = len(entries)
    cloned = [None] * count
    for index in range(count):
        cloned[index] = _clone_stack_map_frame(entries[index])
    return cloned


cdef inline list _clone_exception_info_list(list entries):
    cdef Py_ssize_t index, count
    cdef list cloned
    cdef CExceptionInfo entry
    count = len(entries)
    cloned = [None] * count
    for index in range(count):
        entry = entries[index]
        cloned[index] = ExceptionInfo(entry.start_pc, entry.end_pc, entry.handler_pc, entry.catch_type)
    return cloned


cdef inline list _clone_bootstrap_method_info_list(list entries):
    cdef Py_ssize_t index, count
    cdef list cloned
    cdef CBootstrapMethodInfo entry
    count = len(entries)
    cloned = [None] * count
    for index in range(count):
        entry = entries[index]
        cloned[index] = BootstrapMethodInfo(
            entry.bootstrap_method_ref,
            entry.num_boostrap_arguments,
            list(entry.boostrap_arguments),
        )
    return cloned


cdef inline list _clone_inner_class_info_list(list entries):
    cdef Py_ssize_t index, count
    cdef list cloned
    cdef CInnerClassInfo entry
    count = len(entries)
    cloned = [None] * count
    for index in range(count):
        entry = entries[index]
        cloned[index] = InnerClassInfo(
            entry.inner_class_info_index,
            entry.outer_class_info_index,
            entry.inner_name_index,
            entry.inner_class_access_flags,
        )
    return cloned


cdef inline list _clone_method_parameter_info_list(list entries):
    cdef Py_ssize_t index, count
    cdef list cloned
    cdef CMethodParameterInfo entry
    count = len(entries)
    cloned = [None] * count
    for index in range(count):
        entry = entries[index]
        cloned[index] = MethodParameterInfo(entry.name_index, entry.access_flags)
    return cloned


cdef inline list _clone_requires_info_list(list entries):
    cdef Py_ssize_t index, count
    cdef list cloned
    cdef CRequiresInfo entry
    count = len(entries)
    cloned = [None] * count
    for index in range(count):
        entry = entries[index]
        cloned[index] = RequiresInfo(entry.requires_index, entry.requires_flag, entry.requires_version_index)
    return cloned


cdef inline list _clone_export_info_list(list entries):
    cdef Py_ssize_t index, count
    cdef list cloned
    cdef CExportInfo entry
    count = len(entries)
    cloned = [None] * count
    for index in range(count):
        entry = entries[index]
        cloned[index] = ExportInfo(
            entry.exports_index,
            entry.exports_flags,
            entry.exports_to_count,
            list(entry.exports_to_index),
        )
    return cloned


cdef inline list _clone_opens_info_list(list entries):
    cdef Py_ssize_t index, count
    cdef list cloned
    cdef COpensInfo entry
    count = len(entries)
    cloned = [None] * count
    for index in range(count):
        entry = entries[index]
        cloned[index] = OpensInfo(
            entry.opens_index,
            entry.opens_flags,
            entry.opens_to_count,
            list(entry.opens_to_index),
        )
    return cloned


cdef inline list _clone_provides_info_list(list entries):
    cdef Py_ssize_t index, count
    cdef list cloned
    cdef CProvidesInfo entry
    count = len(entries)
    cloned = [None] * count
    for index in range(count):
        entry = entries[index]
        cloned[index] = ProvidesInfo(
            entry.provides_index,
            entry.provides_with_count,
            list(entry.provides_with_index),
        )
    return cloned


cdef inline list _clone_record_component_info_list(list entries):
    cdef Py_ssize_t index, count
    cdef list cloned
    cdef CRecordComponentInfo entry
    count = len(entries)
    cloned = [None] * count
    for index in range(count):
        entry = entries[index]
        cloned[index] = RecordComponentInfo(
            entry.name_index,
            entry.descriptor_index,
            entry.attributes_count,
            _clone_value(entry.attributes),
        )
    return cloned


cdef inline object _clone_verification_type(object value):
    cdef type t = type(value)
    cdef CVerificationTypeInfo base_value
    cdef CObjectVariableInfo object_value
    cdef CUninitializedVariableInfo uninitialized_value
    if t is TopVariableInfo:
        base_value = value
        return TopVariableInfo(base_value.tag)
    if t is IntegerVariableInfo:
        base_value = value
        return IntegerVariableInfo(base_value.tag)
    if t is FloatVariableInfo:
        base_value = value
        return FloatVariableInfo(base_value.tag)
    if t is DoubleVariableInfo:
        base_value = value
        return DoubleVariableInfo(base_value.tag)
    if t is LongVariableInfo:
        base_value = value
        return LongVariableInfo(base_value.tag)
    if t is NullVariableInfo:
        base_value = value
        return NullVariableInfo(base_value.tag)
    if t is UninitializedThisVariableInfo:
        base_value = value
        return UninitializedThisVariableInfo(base_value.tag)
    if t is ObjectVariableInfo:
        object_value = value
        return ObjectVariableInfo(object_value.tag, object_value.cpool_index)
    if t is UninitializedVariableInfo:
        uninitialized_value = value
        return UninitializedVariableInfo(uninitialized_value.tag, uninitialized_value.offset)
    return _clone_value(value)


cdef inline object _clone_stack_map_frame(object frame):
    cdef type t = type(frame)
    cdef CStackMapFrameInfo base_frame
    cdef CSameLocals1StackItemFrameInfo same_locals1_frame
    cdef CSameLocals1StackItemFrameExtendedInfo same_locals1_frame_extended
    cdef CChopFrameInfo chop_frame
    cdef CSameFrameExtendedInfo same_frame_extended
    cdef CAppendFrameInfo append_frame
    cdef CFullFrameInfo full_frame
    if t is SameFrameInfo:
        base_frame = frame
        return SameFrameInfo(base_frame.frame_type)
    if t is SameLocals1StackItemFrameInfo:
        same_locals1_frame = frame
        return SameLocals1StackItemFrameInfo(
            same_locals1_frame.frame_type,
            _clone_verification_type(same_locals1_frame.stack),
        )
    if t is SameLocals1StackItemFrameExtendedInfo:
        same_locals1_frame_extended = frame
        return SameLocals1StackItemFrameExtendedInfo(
            same_locals1_frame_extended.frame_type,
            same_locals1_frame_extended.offset_delta,
            _clone_verification_type(same_locals1_frame_extended.stack),
        )
    if t is ChopFrameInfo:
        chop_frame = frame
        return ChopFrameInfo(chop_frame.frame_type, chop_frame.offset_delta)
    if t is SameFrameExtendedInfo:
        same_frame_extended = frame
        return SameFrameExtendedInfo(same_frame_extended.frame_type, same_frame_extended.offset_delta)
    if t is AppendFrameInfo:
        append_frame = frame
        return AppendFrameInfo(
            append_frame.frame_type,
            append_frame.offset_delta,
            _clone_verification_type_list(append_frame.locals),
        )
    if t is FullFrameInfo:
        full_frame = frame
        return FullFrameInfo(
            full_frame.frame_type,
            full_frame.offset_delta,
            full_frame.number_of_locals,
            _clone_verification_type_list(full_frame.locals),
            full_frame.number_of_stack_items,
            _clone_verification_type_list(full_frame.stack),
        )
    return _clone_value(frame)


cdef inline object _clone_element_value(object value):
    return ElementValueInfo(value.tag, _clone_element_value_payload(value.value))


cdef inline object _clone_element_value_payload(object value):
    cdef type t = type(value)
    if t is ConstValueInfo:
        return ConstValueInfo(value.const_value_index)
    if t is EnumConstantValueInfo:
        return EnumConstantValueInfo(value.type_name_index, value.const_name_index)
    if t is ClassInfoValueInfo:
        return ClassInfoValueInfo(value.class_info_index)
    if t is ArrayValueInfo:
        return ArrayValueInfo(
            value.num_values,
            [_clone_element_value(entry) for entry in value.values],
        )
    return _clone_annotation(value)


cdef inline object _clone_annotation(object annotation):
    return AnnotationInfo(
        annotation.type_index,
        annotation.num_element_value_pairs,
        [
            ElementValuePairInfo(pair.element_name_index, _clone_element_value(pair.element_value))
            for pair in annotation.element_value_pairs
        ],
    )


cdef inline object _clone_parameter_annotation(object parameter):
    return ParameterAnnotationInfo(
        parameter.num_annotations,
        [_clone_annotation(annotation) for annotation in parameter.annotations],
    )


cdef inline object _clone_target_info(object value):
    cdef type t = type(value)
    if t is TypeParameterTargetInfo:
        return TypeParameterTargetInfo(value.type_parameter_index)
    if t is SupertypeTargetInfo:
        return SupertypeTargetInfo(value.supertype_index)
    if t is TypeParameterBoundTargetInfo:
        return TypeParameterBoundTargetInfo(value.type_parameter_index, value.bound_index)
    if t is EmptyTargetInfo:
        return EmptyTargetInfo()
    if t is FormalParameterTargetInfo:
        return FormalParameterTargetInfo(value.formal_parameter_index)
    if t is ThrowsTargetInfo:
        return ThrowsTargetInfo(value.throws_type_index)
    if t is LocalvarTargetInfo:
        return LocalvarTargetInfo(
            value.table_length,
            [TableInfo(entry.start_pc, entry.length, entry.index) for entry in value.table],
        )
    if t is CatchTargetInfo:
        return CatchTargetInfo(value.exception_table_index)
    if t is OffsetTargetInfo:
        return OffsetTargetInfo(value.offset)
    if t is TypeArgumentTargetInfo:
        return TypeArgumentTargetInfo(value.offset, value.type_argument_index)
    return _clone_value(value)


cdef inline object _clone_type_annotation(object annotation):
    return TypeAnnotationInfo(
        annotation.target_type,
        _clone_target_info(annotation.target_info),
        TypePathInfo(
            annotation.target_path.path_length,
            [type(path)(path.type_path_kind, path.type_argument_index) for path in annotation.target_path.path],
        ),
        annotation.type_index,
        annotation.num_element_value_pairs,
        [
            ElementValuePairInfo(pair.element_name_index, _clone_element_value(pair.element_value))
            for pair in annotation.element_value_pairs
        ],
    )


cdef object _clone_runtime_annotations_attr(object attribute):
    cdef type t = type(attribute)
    if t is RuntimeVisibleAnnotationsAttr:
        return RuntimeVisibleAnnotationsAttr(
            attribute.attribute_name_index,
            attribute.attribute_length,
            attribute.num_annotations,
            [_clone_annotation(annotation) for annotation in attribute.annotations],
        )
    if t is RuntimeInvisibleAnnotationsAttr:
        return RuntimeInvisibleAnnotationsAttr(
            attribute.attribute_name_index,
            attribute.attribute_length,
            attribute.num_annotations,
            [_clone_annotation(annotation) for annotation in attribute.annotations],
        )
    if t is RuntimeVisibleParameterAnnotationsAttr:
        return RuntimeVisibleParameterAnnotationsAttr(
            attribute.attribute_name_index,
            attribute.attribute_length,
            attribute.num_parameters,
            [_clone_parameter_annotation(parameter) for parameter in attribute.parameter_annotations],
        )
    if t is RuntimeInvisibleParameterAnnotationsAttr:
        return RuntimeInvisibleParameterAnnotationsAttr(
            attribute.attribute_name_index,
            attribute.attribute_length,
            attribute.num_parameters,
            [_clone_parameter_annotation(parameter) for parameter in attribute.parameter_annotations],
        )
    if t is RuntimeVisibleTypeAnnotationsAttr:
        return RuntimeVisibleTypeAnnotationsAttr(
            attribute.attribute_name_index,
            attribute.attribute_length,
            attribute.num_annotations,
            [_clone_type_annotation(annotation) for annotation in attribute.annotations],
        )
    if t is RuntimeInvisibleTypeAnnotationsAttr:
        return RuntimeInvisibleTypeAnnotationsAttr(
            attribute.attribute_name_index,
            attribute.attribute_length,
            attribute.num_annotations,
            [_clone_type_annotation(annotation) for annotation in attribute.annotations],
        )
    return attribute


cdef inline object _clone_line_number_table_attr(object attribute):
    cdef CLineNumberTableAttr line_number_table_attr = attribute
    cdef list entries = line_number_table_attr.line_number_table
    cdef list cloned_entries = [None] * len(entries)
    cdef Py_ssize_t index
    cdef CLineNumberInfo entry
    for index in range(len(entries)):
        entry = entries[index]
        cloned_entries[index] = LineNumberInfo(entry.start_pc, entry.line_number)
    return LineNumberTableAttr(
        line_number_table_attr.attribute_name_index,
        line_number_table_attr.attribute_length,
        line_number_table_attr.line_number_table_length,
        cloned_entries,
    )


cdef inline object _clone_local_variable_table_attr(object attribute):
    cdef CLocalVariableTableAttr local_variable_table_attr = attribute
    cdef list entries = local_variable_table_attr.local_variable_table
    cdef list cloned_entries = [None] * len(entries)
    cdef Py_ssize_t index
    cdef CLocalVariableInfo entry
    for index in range(len(entries)):
        entry = entries[index]
        cloned_entries[index] = LocalVariableInfo(
            entry.start_pc,
            entry.length,
            entry.name_index,
            entry.descriptor_index,
            entry.index,
        )
    return LocalVariableTableAttr(
        local_variable_table_attr.attribute_name_index,
        local_variable_table_attr.attribute_length,
        local_variable_table_attr.local_variable_table_length,
        cloned_entries,
    )


cdef inline object _clone_local_variable_type_table_attr(object attribute):
    cdef CLocalVariableTypeTableAttr local_variable_type_table_attr = attribute
    cdef list entries = local_variable_type_table_attr.local_variable_type_table
    cdef list cloned_entries = [None] * len(entries)
    cdef Py_ssize_t index
    cdef CLocalVariableTypeInfo entry
    for index in range(len(entries)):
        entry = entries[index]
        cloned_entries[index] = LocalVariableTypeInfo(
            entry.start_pc,
            entry.length,
            entry.name_index,
            entry.signature_index,
            entry.index,
        )
    return LocalVariableTypeTableAttr(
        local_variable_type_table_attr.attribute_name_index,
        local_variable_type_table_attr.attribute_length,
        local_variable_type_table_attr.local_variable_type_table_length,
        cloned_entries,
    )


cdef inline object _clone_code_attr(object attribute):
    cdef CCodeAttr code_attr = attribute
    return CodeAttr(
        code_attr.attribute_name_index,
        code_attr.attribute_length,
        code_attr.max_stacks,
        code_attr.max_locals,
        code_attr.code_length,
        _clone_value(code_attr.code),
        code_attr.exception_table_length,
        _clone_exception_info_list(code_attr.exception_table),
        code_attr.attributes_count,
        _clone_value(code_attr.attributes),
    )


cdef inline object _clone_bootstrap_methods_attr(object attribute):
    return BootstrapMethodsAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.num_bootstrap_methods,
        _clone_bootstrap_method_info_list(attribute.bootstrap_methods),
    )


cdef inline object _clone_inner_classes_attr(object attribute):
    return InnerClassesAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.number_of_classes,
        _clone_inner_class_info_list(attribute.classes),
    )


cdef inline object _clone_method_parameters_attr(object attribute):
    return MethodParametersAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.parameters_count,
        _clone_method_parameter_info_list(attribute.parameters),
    )


cdef inline object _clone_record_attr(object attribute):
    return RecordAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.components_count,
        _clone_record_component_info_list(attribute.components),
    )


cdef inline object _clone_constant_value_attr(object attribute):
    return ConstantValueAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.constantvalue_index,
    )


cdef inline object _clone_exceptions_attr(object attribute):
    return ExceptionsAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.number_of_exceptions,
        list(attribute.exception_index_table),
    )


cdef inline object _clone_enclosing_method_attr(object attribute):
    return EnclosingMethodAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.class_index,
        attribute.method_index,
    )


cdef inline object _clone_signature_attr(object attribute):
    return SignatureAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.signature_index,
    )


cdef inline object _clone_source_file_attr(object attribute):
    return SourceFileAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.sourcefile_index,
    )


cdef inline object _clone_source_debug_extension_attr(object attribute):
    return SourceDebugExtensionAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.debug_extension,
    )


cdef inline object _clone_module_packages_attr(object attribute):
    return ModulePackagesAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.package_count,
        list(attribute.package_index),
    )


cdef inline object _clone_module_attr(object attribute):
    cdef CModuleAttr module_attr = attribute
    return ModuleAttr(
        module_attr.attribute_name_index,
        module_attr.attribute_length,
        module_attr.module_name_index,
        module_attr.module_flags,
        module_attr.module_version_index,
        module_attr.requires_count,
        _clone_requires_info_list(module_attr.requires),
        module_attr.exports_count,
        _clone_export_info_list(module_attr.exports),
        module_attr.opens_count,
        _clone_opens_info_list(module_attr.opens),
        module_attr.uses_count,
        list(module_attr.uses_index),
        module_attr.provides_count,
        _clone_provides_info_list(module_attr.provides),
    )


cdef inline object _clone_module_main_class_attr(object attribute):
    return ModuleMainClassAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.main_class_index,
    )


cdef inline object _clone_nest_host_attr(object attribute):
    return NestHostAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.host_class_index,
    )


cdef inline object _clone_nest_members_attr(object attribute):
    return NestMembersAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.number_of_classes,
        list(attribute.classes),
    )


cdef inline object _clone_permitted_subclasses_attr(object attribute):
    return PermittedSubclassesAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.number_of_classes,
        list(attribute.classes),
    )


cdef inline object _clone_synthetic_attr(object attribute):
    return SyntheticAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
    )


cdef inline object _clone_deprecated_attr(object attribute):
    return DeprecatedAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
    )


cdef object _clone_simple_attribute(object attribute):
    cdef type t = type(attribute)
    if t is LineNumberTableAttr:
        return _clone_line_number_table_attr(attribute)
    if t is LocalVariableTableAttr:
        return _clone_local_variable_table_attr(attribute)
    if t is LocalVariableTypeTableAttr:
        return _clone_local_variable_type_table_attr(attribute)
    if t is BootstrapMethodsAttr:
        return _clone_bootstrap_methods_attr(attribute)
    if t is InnerClassesAttr:
        return _clone_inner_classes_attr(attribute)
    if t is MethodParametersAttr:
        return _clone_method_parameters_attr(attribute)
    if t is RecordAttr:
        return _clone_record_attr(attribute)
    if t is ConstantValueAttr:
        return _clone_constant_value_attr(attribute)
    if t is ExceptionsAttr:
        return _clone_exceptions_attr(attribute)
    if t is EnclosingMethodAttr:
        return _clone_enclosing_method_attr(attribute)
    if t is SignatureAttr:
        return _clone_signature_attr(attribute)
    if t is SourceFileAttr:
        return _clone_source_file_attr(attribute)
    if t is SourceDebugExtensionAttr:
        return _clone_source_debug_extension_attr(attribute)
    if t is ModuleAttr:
        return _clone_module_attr(attribute)
    if t is ModulePackagesAttr:
        return _clone_module_packages_attr(attribute)
    if t is ModuleMainClassAttr:
        return _clone_module_main_class_attr(attribute)
    if t is NestHostAttr:
        return _clone_nest_host_attr(attribute)
    if t is NestMembersAttr:
        return _clone_nest_members_attr(attribute)
    if t is PermittedSubclassesAttr:
        return _clone_permitted_subclasses_attr(attribute)
    if t is SyntheticAttr:
        return _clone_synthetic_attr(attribute)
    if t is DeprecatedAttr:
        return _clone_deprecated_attr(attribute)
    return None


cdef object _clone_fast_attribute(object attribute):
    cdef CStackMapTableAttr stack_map_table_attr
    if type(attribute) is CodeAttr:
        return _clone_code_attr(attribute)
    if type(attribute) is StackMapTableAttr:
        stack_map_table_attr = attribute
        return StackMapTableAttr(
            stack_map_table_attr.attribute_name_index,
            stack_map_table_attr.attribute_length,
            stack_map_table_attr.number_of_entries,
            _clone_stack_map_frame_list(stack_map_table_attr.entries),
        )
    cdef object runtime_attr = _clone_runtime_annotations_attr(attribute)
    if runtime_attr is not attribute:
        return runtime_attr
    return _clone_simple_attribute(attribute)


cdef object _clone_value(object value):
    cdef type cls
    cdef object reducer, reduced, factory, args
    cls = type(value)
    if cls is list:
        return [_clone_value(item) for item in <list>value]
    if cls is tuple:
        return tuple(_clone_value(item) for item in <tuple>value)
    if isinstance(value, AttributeInfo):
        fast = _clone_fast_attribute(value)
        if fast is not None:
            return fast
    if cls is not type and hasattr(value, "_field_values"):
        reducer = getattr(value, "__reduce__", None)
        if reducer is not None:
            try:
                reduced = reducer()
            except TypeError:
                reduced = None
            if type(reduced) is tuple and len(reduced) == 2:
                factory, args = reduced
                if factory is cls and type(args) is tuple:
                    return factory(*tuple(_clone_value(arg) for arg in args))
    if cls is not type and getattr(cls, "__dataclass_fields__", None) is not None:
        cloned = {name: _clone_value(getattr(value, name)) for name in _clone_field_names(cls)}
        return cls(**cloned)
    return value


def clone_attribute(object attribute):
    """Clone a JVM attribute tree without using ``copy.deepcopy``."""
    return _clone_value(attribute)


def clone_attributes(list attributes):
    """Clone a list of JVM attributes without sharing nested mutable state."""
    return [_clone_value(attribute) for attribute in attributes]
