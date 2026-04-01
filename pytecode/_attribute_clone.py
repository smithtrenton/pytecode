"""Internal helpers for cloning attribute dataclass trees."""

from __future__ import annotations

from dataclasses import fields, is_dataclass
from functools import cache
from typing import Any, cast

from .attributes import (
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
def _clone_field_names(cls: type[object]) -> tuple[str, ...]:
    return tuple(field.name for field in fields(cast(Any, cls)))


def _clone_verification_type(value: VerificationTypeInfo) -> VerificationTypeInfo:
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
    return cast(VerificationTypeInfo, _clone_value(value))


def _clone_stack_map_frame(frame: StackMapFrameInfo) -> StackMapFrameInfo:
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
    return cast(StackMapFrameInfo, _clone_value(frame))


def _clone_element_value(value: ElementValueInfo) -> ElementValueInfo:
    return ElementValueInfo(value.tag, _clone_element_value_payload(value.value))


def _clone_element_value_payload(
    value: ConstValueInfo | EnumConstantValueInfo | ClassInfoValueInfo | AnnotationInfo | ArrayValueInfo,
) -> ConstValueInfo | EnumConstantValueInfo | ClassInfoValueInfo | AnnotationInfo | ArrayValueInfo:
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


def _clone_annotation(annotation: AnnotationInfo) -> AnnotationInfo:
    return AnnotationInfo(
        annotation.type_index,
        annotation.num_element_value_pairs,
        [
            ElementValuePairInfo(pair.element_name_index, _clone_element_value(pair.element_value))
            for pair in annotation.element_value_pairs
        ],
    )


def _clone_parameter_annotation(parameter: ParameterAnnotationInfo) -> ParameterAnnotationInfo:
    return ParameterAnnotationInfo(
        parameter.num_annotations,
        [_clone_annotation(annotation) for annotation in parameter.annotations],
    )


def _clone_target_info(value: object) -> object:
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


def _clone_type_annotation(annotation: TypeAnnotationInfo) -> TypeAnnotationInfo:
    return TypeAnnotationInfo(
        annotation.target_type,
        cast(Any, _clone_target_info(annotation.target_info)),
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


def _clone_runtime_annotations_attr(attribute: AttributeInfo) -> AttributeInfo:
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


def _clone_line_number_table_attr(attribute: LineNumberTableAttr) -> LineNumberTableAttr:
    return LineNumberTableAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.line_number_table_length,
        [LineNumberInfo(entry.start_pc, entry.line_number) for entry in attribute.line_number_table],
    )


def _clone_local_variable_table_attr(attribute: LocalVariableTableAttr) -> LocalVariableTableAttr:
    return LocalVariableTableAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.local_variable_table_length,
        [
            LocalVariableInfo(entry.start_pc, entry.length, entry.name_index, entry.descriptor_index, entry.index)
            for entry in attribute.local_variable_table
        ],
    )


def _clone_local_variable_type_table_attr(
    attribute: LocalVariableTypeTableAttr,
) -> LocalVariableTypeTableAttr:
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


def _clone_bootstrap_methods_attr(attribute: BootstrapMethodsAttr) -> BootstrapMethodsAttr:
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


def _clone_inner_classes_attr(attribute: InnerClassesAttr) -> InnerClassesAttr:
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


def _clone_method_parameters_attr(attribute: MethodParametersAttr) -> MethodParametersAttr:
    return MethodParametersAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.parameters_count,
        [MethodParameterInfo(entry.name_index, entry.access_flags) for entry in attribute.parameters],
    )


def _clone_constant_value_attr(attribute: ConstantValueAttr) -> ConstantValueAttr:
    return ConstantValueAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.constantvalue_index,
    )


def _clone_exceptions_attr(attribute: ExceptionsAttr) -> ExceptionsAttr:
    return ExceptionsAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.number_of_exceptions,
        list(attribute.exception_index_table),
    )


def _clone_enclosing_method_attr(attribute: EnclosingMethodAttr) -> EnclosingMethodAttr:
    return EnclosingMethodAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.class_index,
        attribute.method_index,
    )


def _clone_signature_attr(attribute: SignatureAttr) -> SignatureAttr:
    return SignatureAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.signature_index,
    )


def _clone_source_file_attr(attribute: SourceFileAttr) -> SourceFileAttr:
    return SourceFileAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.sourcefile_index,
    )


def _clone_source_debug_extension_attr(
    attribute: SourceDebugExtensionAttr,
) -> SourceDebugExtensionAttr:
    return SourceDebugExtensionAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.debug_extension,
    )


def _clone_module_packages_attr(attribute: ModulePackagesAttr) -> ModulePackagesAttr:
    return ModulePackagesAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.package_count,
        list(attribute.package_index),
    )


def _clone_module_main_class_attr(attribute: ModuleMainClassAttr) -> ModuleMainClassAttr:
    return ModuleMainClassAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.main_class_index,
    )


def _clone_nest_host_attr(attribute: NestHostAttr) -> NestHostAttr:
    return NestHostAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.host_class_index,
    )


def _clone_nest_members_attr(attribute: NestMembersAttr) -> NestMembersAttr:
    return NestMembersAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.number_of_classes,
        list(attribute.classes),
    )


def _clone_permitted_subclasses_attr(
    attribute: PermittedSubclassesAttr,
) -> PermittedSubclassesAttr:
    return PermittedSubclassesAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
        attribute.number_of_classes,
        list(attribute.classes),
    )


def _clone_synthetic_attr(attribute: SyntheticAttr) -> SyntheticAttr:
    return SyntheticAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
    )


def _clone_deprecated_attr(attribute: DeprecatedAttr) -> DeprecatedAttr:
    return DeprecatedAttr(
        attribute.attribute_name_index,
        attribute.attribute_length,
    )


def _clone_simple_attribute(attribute: AttributeInfo) -> AttributeInfo | None:
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


def _clone_fast_attribute(attribute: AttributeInfo) -> AttributeInfo | None:
    if isinstance(attribute, StackMapTableAttr):
        return StackMapTableAttr(
            attribute.attribute_name_index,
            attribute.attribute_length,
            attribute.number_of_entries,
            [_clone_stack_map_frame(entry) for entry in attribute.entries],
        )
    runtime_attr = _clone_runtime_annotations_attr(attribute)
    if runtime_attr is not attribute:
        return runtime_attr
    return _clone_simple_attribute(attribute)


def _clone_value(value: object) -> object:
    if isinstance(value, list):
        items = cast(list[object], value)
        return [_clone_value(item) for item in items]
    if isinstance(value, tuple):
        items = cast(tuple[object, ...], value)
        return tuple(_clone_value(item) for item in items)
    if isinstance(value, AttributeInfo):
        fast = _clone_fast_attribute(value)
        if fast is not None:
            return fast
    if is_dataclass(value) and not isinstance(value, type):
        cls = type(value)
        cloned = {name: _clone_value(getattr(value, name)) for name in _clone_field_names(cls)}
        return cast(Any, cls)(**cloned)
    return value


def clone_attribute(attribute: AttributeInfo) -> AttributeInfo:
    """Clone a JVM attribute tree without using ``copy.deepcopy``."""
    return cast(AttributeInfo, _clone_value(attribute))


def clone_attributes(attributes: list[AttributeInfo]) -> list[AttributeInfo]:
    """Clone a list of JVM attributes without sharing nested mutable state."""
    return [clone_attribute(attribute) for attribute in attributes]
