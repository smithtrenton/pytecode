# cython: boundscheck=False, wraparound=False, cdivision=True
"""Cython-accelerated attribute cloning for JVM class file attributes.

Drop-in replacement for ``_attribute_clone_py`` with ``cdef`` internal
dispatch functions for reduced Python call overhead and faster
``isinstance()`` checks.
"""

from dataclasses import fields, is_dataclass
from functools import cache

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
    DeprecatedAttr,
    ElementValueInfo,
    ElementValuePairInfo,
    EmptyTargetInfo,
    EnclosingMethodAttr,
    EnumConstantValueInfo,
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
    ModuleMainClassAttr,
    ModulePackagesAttr,
    NestHostAttr,
    NestMembersAttr,
    NullVariableInfo,
    ObjectVariableInfo,
    OffsetTargetInfo,
    ParameterAnnotationInfo,
    PermittedSubclassesAttr,
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


cdef object _clone_verification_type(object value):
    if isinstance(value, TopVariableInfo):
        return TopVariableInfo(value.tag)
    if isinstance(value, IntegerVariableInfo):
        return IntegerVariableInfo(value.tag)
    if isinstance(value, FloatVariableInfo):
        return FloatVariableInfo(value.tag)
    if isinstance(value, LongVariableInfo):
        return LongVariableInfo(value.tag)
    if isinstance(value, NullVariableInfo):
        return NullVariableInfo(value.tag)
    if isinstance(value, UninitializedThisVariableInfo):
        return UninitializedThisVariableInfo(value.tag)
    if isinstance(value, ObjectVariableInfo):
        return ObjectVariableInfo(value.tag, value.cpool_index)
    if isinstance(value, UninitializedVariableInfo):
        return UninitializedVariableInfo(value.tag, value.offset)
    return _clone_value(value)


cdef object _clone_stack_map_frame(object frame):
    if isinstance(frame, SameFrameInfo):
        return SameFrameInfo(frame.frame_type)
    if isinstance(frame, SameLocals1StackItemFrameInfo):
        return SameLocals1StackItemFrameInfo(frame.frame_type, _clone_verification_type(frame.stack))
    if isinstance(frame, SameLocals1StackItemFrameExtendedInfo):
        return SameLocals1StackItemFrameExtendedInfo(
            frame.frame_type,
            frame.offset_delta,
            _clone_verification_type(frame.stack),
        )
    if isinstance(frame, ChopFrameInfo):
        return ChopFrameInfo(frame.frame_type, frame.offset_delta)
    if isinstance(frame, SameFrameExtendedInfo):
        return SameFrameExtendedInfo(frame.frame_type, frame.offset_delta)
    if isinstance(frame, AppendFrameInfo):
        return AppendFrameInfo(
            frame.frame_type,
            frame.offset_delta,
            [_clone_verification_type(value) for value in frame.locals],
        )
    if isinstance(frame, FullFrameInfo):
        return FullFrameInfo(
            frame.frame_type,
            frame.offset_delta,
            frame.number_of_locals,
            [_clone_verification_type(value) for value in frame.locals],
            frame.number_of_stack_items,
            [_clone_verification_type(value) for value in frame.stack],
        )
    return _clone_value(frame)


cdef object _clone_element_value(object value):
    return ElementValueInfo(value.tag, _clone_element_value_payload(value.value))


cdef object _clone_element_value_payload(object value):
    if isinstance(value, ConstValueInfo):
        return ConstValueInfo(value.const_value_index)
    if isinstance(value, EnumConstantValueInfo):
        return EnumConstantValueInfo(value.type_name_index, value.const_name_index)
    if isinstance(value, ClassInfoValueInfo):
        return ClassInfoValueInfo(value.class_info_index)
    if isinstance(value, ArrayValueInfo):
        return ArrayValueInfo(
            value.num_values,
            [_clone_element_value(entry) for entry in value.values],
        )
    return _clone_annotation(value)


cdef object _clone_annotation(object annotation):
    return AnnotationInfo(
        annotation.type_index,
        annotation.num_element_value_pairs,
        [
            ElementValuePairInfo(pair.element_name_index, _clone_element_value(pair.element_value))
            for pair in annotation.element_value_pairs
        ],
    )


cdef object _clone_parameter_annotation(object parameter):
    return ParameterAnnotationInfo(
        parameter.num_annotations,
        [_clone_annotation(annotation) for annotation in parameter.annotations],
    )


cdef object _clone_target_info(object value):
    if isinstance(value, TypeParameterTargetInfo):
        return TypeParameterTargetInfo(value.type_parameter_index)
    if isinstance(value, SupertypeTargetInfo):
        return SupertypeTargetInfo(value.supertype_index)
    if isinstance(value, TypeParameterBoundTargetInfo):
        return TypeParameterBoundTargetInfo(value.type_parameter_index, value.bound_index)
    if isinstance(value, EmptyTargetInfo):
        return EmptyTargetInfo()
    if isinstance(value, FormalParameterTargetInfo):
        return FormalParameterTargetInfo(value.formal_parameter_index)
    if isinstance(value, ThrowsTargetInfo):
        return ThrowsTargetInfo(value.throws_type_index)
    if isinstance(value, LocalvarTargetInfo):
        return LocalvarTargetInfo(
            value.table_length,
            [TableInfo(entry.start_pc, entry.length, entry.index) for entry in value.table],
        )
    if isinstance(value, CatchTargetInfo):
        return CatchTargetInfo(value.exception_table_index)
    if isinstance(value, OffsetTargetInfo):
        return OffsetTargetInfo(value.offset)
    if isinstance(value, TypeArgumentTargetInfo):
        return TypeArgumentTargetInfo(value.offset, value.type_argument_index)
    return _clone_value(value)


cdef object _clone_type_annotation(object annotation):
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
    if isinstance(attribute, RuntimeVisibleAnnotationsAttr):
        return RuntimeVisibleAnnotationsAttr(
            attribute.attribute_name_index,
            attribute.attribute_length,
            attribute.num_annotations,
            [_clone_annotation(annotation) for annotation in attribute.annotations],
        )
    if isinstance(attribute, RuntimeInvisibleAnnotationsAttr):
        return RuntimeInvisibleAnnotationsAttr(
            attribute.attribute_name_index,
            attribute.attribute_length,
            attribute.num_annotations,
            [_clone_annotation(annotation) for annotation in attribute.annotations],
        )
    if isinstance(attribute, RuntimeVisibleParameterAnnotationsAttr):
        return RuntimeVisibleParameterAnnotationsAttr(
            attribute.attribute_name_index,
            attribute.attribute_length,
            attribute.num_parameters,
            [_clone_parameter_annotation(parameter) for parameter in attribute.parameter_annotations],
        )
    if isinstance(attribute, RuntimeInvisibleParameterAnnotationsAttr):
        return RuntimeInvisibleParameterAnnotationsAttr(
            attribute.attribute_name_index,
            attribute.attribute_length,
            attribute.num_parameters,
            [_clone_parameter_annotation(parameter) for parameter in attribute.parameter_annotations],
        )
    if isinstance(attribute, RuntimeVisibleTypeAnnotationsAttr):
        return RuntimeVisibleTypeAnnotationsAttr(
            attribute.attribute_name_index,
            attribute.attribute_length,
            attribute.num_annotations,
            [_clone_type_annotation(annotation) for annotation in attribute.annotations],
        )
    if isinstance(attribute, RuntimeInvisibleTypeAnnotationsAttr):
        return RuntimeInvisibleTypeAnnotationsAttr(
            attribute.attribute_name_index,
            attribute.attribute_length,
            attribute.num_annotations,
            [_clone_type_annotation(annotation) for annotation in attribute.annotations],
        )
    return attribute


cdef object _clone_line_number_table_attr(object attribute):
    return LineNumberTableAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.line_number_table_length,
        [LineNumberInfo(entry.start_pc, entry.line_number) for entry in attribute.line_number_table],
    )


cdef object _clone_local_variable_table_attr(object attribute):
    return LocalVariableTableAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.local_variable_table_length,
        [
            LocalVariableInfo(entry.start_pc, entry.length, entry.name_index, entry.descriptor_index, entry.index)
            for entry in attribute.local_variable_table
        ],
    )


cdef object _clone_local_variable_type_table_attr(object attribute):
    return LocalVariableTypeTableAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.local_variable_type_table_length,
        [
            LocalVariableTypeInfo(
                entry.start_pc,
                entry.length,
                entry.name_index,
                entry.signature_index,
                entry.index,
            )
            for entry in attribute.local_variable_type_table
        ],
    )


cdef object _clone_bootstrap_methods_attr(object attribute):
    return BootstrapMethodsAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.num_bootstrap_methods,
        [
            BootstrapMethodInfo(
                entry.bootstrap_method_ref,
                entry.num_boostrap_arguments,
                list(entry.boostrap_arguments),
            )
            for entry in attribute.bootstrap_methods
        ],
    )


cdef object _clone_inner_classes_attr(object attribute):
    return InnerClassesAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.number_of_classes,
        [
            InnerClassInfo(
                entry.inner_class_info_index,
                entry.outer_class_info_index,
                entry.inner_name_index,
                entry.inner_class_access_flags,
            )
            for entry in attribute.classes
        ],
    )


cdef object _clone_method_parameters_attr(object attribute):
    return MethodParametersAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.parameters_count,
        [MethodParameterInfo(entry.name_index, entry.access_flags) for entry in attribute.parameters],
    )


cdef object _clone_constant_value_attr(object attribute):
    return ConstantValueAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.constantvalue_index,
    )


cdef object _clone_exceptions_attr(object attribute):
    return ExceptionsAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.number_of_exceptions,
        list(attribute.exception_index_table),
    )


cdef object _clone_enclosing_method_attr(object attribute):
    return EnclosingMethodAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.class_index,
        attribute.method_index,
    )


cdef object _clone_signature_attr(object attribute):
    return SignatureAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.signature_index,
    )


cdef object _clone_source_file_attr(object attribute):
    return SourceFileAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.sourcefile_index,
    )


cdef object _clone_source_debug_extension_attr(object attribute):
    return SourceDebugExtensionAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.debug_extension,
    )


cdef object _clone_module_packages_attr(object attribute):
    return ModulePackagesAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.package_count,
        list(attribute.package_index),
    )


cdef object _clone_module_main_class_attr(object attribute):
    return ModuleMainClassAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.main_class_index,
    )


cdef object _clone_nest_host_attr(object attribute):
    return NestHostAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.host_class_index,
    )


cdef object _clone_nest_members_attr(object attribute):
    return NestMembersAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.number_of_classes,
        list(attribute.classes),
    )


cdef object _clone_permitted_subclasses_attr(object attribute):
    return PermittedSubclassesAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.number_of_classes,
        list(attribute.classes),
    )


cdef object _clone_synthetic_attr(object attribute):
    return SyntheticAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
    )


cdef object _clone_deprecated_attr(object attribute):
    return DeprecatedAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
    )


cdef object _clone_simple_attribute(object attribute):
    if isinstance(attribute, LineNumberTableAttr):
        return _clone_line_number_table_attr(attribute)
    if isinstance(attribute, LocalVariableTableAttr):
        return _clone_local_variable_table_attr(attribute)
    if isinstance(attribute, LocalVariableTypeTableAttr):
        return _clone_local_variable_type_table_attr(attribute)
    if isinstance(attribute, BootstrapMethodsAttr):
        return _clone_bootstrap_methods_attr(attribute)
    if isinstance(attribute, InnerClassesAttr):
        return _clone_inner_classes_attr(attribute)
    if isinstance(attribute, MethodParametersAttr):
        return _clone_method_parameters_attr(attribute)
    if isinstance(attribute, ConstantValueAttr):
        return _clone_constant_value_attr(attribute)
    if isinstance(attribute, ExceptionsAttr):
        return _clone_exceptions_attr(attribute)
    if isinstance(attribute, EnclosingMethodAttr):
        return _clone_enclosing_method_attr(attribute)
    if isinstance(attribute, SignatureAttr):
        return _clone_signature_attr(attribute)
    if isinstance(attribute, SourceFileAttr):
        return _clone_source_file_attr(attribute)
    if isinstance(attribute, SourceDebugExtensionAttr):
        return _clone_source_debug_extension_attr(attribute)
    if isinstance(attribute, ModulePackagesAttr):
        return _clone_module_packages_attr(attribute)
    if isinstance(attribute, ModuleMainClassAttr):
        return _clone_module_main_class_attr(attribute)
    if isinstance(attribute, NestHostAttr):
        return _clone_nest_host_attr(attribute)
    if isinstance(attribute, NestMembersAttr):
        return _clone_nest_members_attr(attribute)
    if isinstance(attribute, PermittedSubclassesAttr):
        return _clone_permitted_subclasses_attr(attribute)
    if isinstance(attribute, SyntheticAttr):
        return _clone_synthetic_attr(attribute)
    if isinstance(attribute, DeprecatedAttr):
        return _clone_deprecated_attr(attribute)
    return None


cdef object _clone_fast_attribute(object attribute):
    if isinstance(attribute, StackMapTableAttr):
        return StackMapTableAttr(
            attribute.attribute_name_index,
            attribute.attribute_length,
            attribute.number_of_entries,
            [_clone_stack_map_frame(entry) for entry in attribute.entries],
        )
    cdef object runtime_attr = _clone_runtime_annotations_attr(attribute)
    if runtime_attr is not attribute:
        return runtime_attr
    return _clone_simple_attribute(attribute)


cdef object _clone_value(object value):
    if isinstance(value, list):
        return [_clone_value(item) for item in <list>value]
    if isinstance(value, tuple):
        return tuple(_clone_value(item) for item in <tuple>value)
    if isinstance(value, AttributeInfo):
        fast = _clone_fast_attribute(value)
        if fast is not None:
            return fast
    if is_dataclass(value) and not isinstance(value, type):
        cls = type(value)
        cloned = {name: _clone_value(getattr(value, name)) for name in _clone_field_names(cls)}
        return cls(**cloned)
    return value


def clone_attribute(object attribute):
    """Clone a JVM attribute tree without using ``copy.deepcopy``."""
    return _clone_value(attribute)


def clone_attributes(list attributes):
    """Clone a list of JVM attributes without sharing nested mutable state."""
    return [_clone_value(attribute) for attribute in attributes]
