from __future__ import annotations

import copy
from collections.abc import Callable
from typing import cast

import pytest

import pytecode.classfile.attributes as attributes
import pytecode.classfile.constant_pool as constant_pool
import pytecode.classfile.constants as constants
import pytecode.classfile.instructions as instructions
from pytecode.edit._attribute_clone import clone_attribute
from tests.helpers import attr_reader, class_reader_with_cp, i1, make_attribute_blob, make_utf8_info, u1, u2, u4

# ---------------------------------------------------------------------------
# Simple marker attributes (zero-length payload)
# ---------------------------------------------------------------------------


def test_synthetic():
    reader = attr_reader("Synthetic", b"")
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.SyntheticAttr)
    assert attr.attribute_name_index == 1
    assert attr.attribute_length == 0


def test_deprecated():
    reader = attr_reader("Deprecated", b"")
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.DeprecatedAttr)
    assert attr.attribute_name_index == 1
    assert attr.attribute_length == 0


# ---------------------------------------------------------------------------
# Simple single-index attributes (u2 payload)
# ---------------------------------------------------------------------------


def test_constant_value():
    reader = attr_reader("ConstantValue", u2(42))
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.ConstantValueAttr)
    assert attr.attribute_name_index == 1
    assert attr.constantvalue_index == 42


def test_signature():
    reader = attr_reader("Signature", u2(7))
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.SignatureAttr)
    assert attr.signature_index == 7


def test_source_file():
    reader = attr_reader("SourceFile", u2(9))
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.SourceFileAttr)
    assert attr.sourcefile_index == 9


def test_module_main_class():
    reader = attr_reader("ModuleMainClass", u2(11))
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.ModuleMainClassAttr)
    assert attr.main_class_index == 11


def test_nest_host():
    reader = attr_reader("NestHost", u2(13))
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.NestHostAttr)
    assert attr.host_class_index == 13


# ---------------------------------------------------------------------------
# EnclosingMethod
# ---------------------------------------------------------------------------


def test_enclosing_method():
    reader = attr_reader("EnclosingMethod", u2(5) + u2(7))
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.EnclosingMethodAttr)
    assert attr.class_index == 5
    assert attr.method_index == 7


# ---------------------------------------------------------------------------
# SourceDebugExtension
# ---------------------------------------------------------------------------


def test_source_debug_extension():
    payload = b"SMAP\nFoo.java"
    reader = attr_reader("SourceDebugExtension", payload)
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.SourceDebugExtensionAttr)
    assert attr.debug_extension == "SMAP\nFoo.java"


# ---------------------------------------------------------------------------
# Code
# ---------------------------------------------------------------------------


def test_code_attr_with_exception_table_and_nested_attributes():
    # Code payload layout:
    #   max_stack=2, max_locals=2
    #   code = bipush 42; istore_1; return
    #   exception table = one handler covering offsets [0, 3) with handler at 3
    #   nested attrs = LineNumberTable + LocalVariableTable
    code_bytes = u1(0x10) + i1(42) + u1(0x3C) + u1(0xB1)
    line_number_payload = u2(2) + u2(0) + u2(10) + u2(3) + u2(11)
    local_variable_payload = u2(1) + u2(0) + u2(4) + u2(7) + u2(8) + u2(1)
    payload = (
        u2(2)
        + u2(2)
        + u4(len(code_bytes))
        + code_bytes
        + u2(1)
        + u2(0)
        + u2(3)
        + u2(3)
        + u2(9)
        + u2(2)
        + make_attribute_blob(2, line_number_payload)
        + make_attribute_blob(3, local_variable_payload)
    )
    cp_list: list[constant_pool.ConstantPoolInfo | None] = [
        None,
        make_utf8_info(1, "Code"),
        make_utf8_info(2, "LineNumberTable"),
        make_utf8_info(3, "LocalVariableTable"),
    ]
    reader = class_reader_with_cp(make_attribute_blob(1, payload), cp_list)

    attr = reader.read_attribute()

    assert isinstance(attr, attributes.CodeAttr)
    assert attr.attribute_name_index == 1
    assert attr.attribute_length == len(payload)
    assert attr.max_stacks == 2
    assert attr.max_locals == 2
    assert attr.code_length == len(code_bytes)
    assert [insn.type for insn in attr.code] == [
        instructions.InsnInfoType.BIPUSH,
        instructions.InsnInfoType.ISTORE_1,
        instructions.InsnInfoType.RETURN,
    ]
    assert [insn.bytecode_offset for insn in attr.code] == [0, 2, 3]
    assert isinstance(attr.code[0], instructions.ByteValue)
    assert attr.code[0].value == 42
    assert attr.exception_table_length == 1
    assert attr.exception_table == [attributes.ExceptionInfo(0, 3, 3, 9)]
    assert attr.attributes_count == 2
    assert len(attr.attributes) == 2

    line_numbers = attr.attributes[0]
    assert isinstance(line_numbers, attributes.LineNumberTableAttr)
    assert line_numbers.line_number_table_length == 2
    assert [(entry.start_pc, entry.line_number) for entry in line_numbers.line_number_table] == [(0, 10), (3, 11)]

    local_variables = attr.attributes[1]
    assert isinstance(local_variables, attributes.LocalVariableTableAttr)
    assert local_variables.local_variable_table_length == 1
    assert [
        (
            entry.start_pc,
            entry.length,
            entry.name_index,
            entry.descriptor_index,
            entry.index,
        )
        for entry in local_variables.local_variable_table
    ] == [(0, 4, 7, 8, 1)]


def test_migrated_attribute_objects_preserve_dataclass_style_semantics():
    base = attributes.AttributeInfo(7, 0)
    line_number = attributes.LineNumberInfo(3, 11)

    assert base == attributes.AttributeInfo(7, 0)
    assert base != attributes.AttributeInfo(8, 0)
    assert repr(base) == "AttributeInfo(attribute_name_index=7, attribute_length=0)"
    assert repr(line_number) == "LineNumberInfo(start_pc=3, line_number=11)"

    ctor, args = line_number.__reduce__()
    assert ctor is attributes.LineNumberInfo
    assert args == (3, 11)

    with pytest.raises(TypeError, match="unhashable type"):
        hash(base)
    with pytest.raises(TypeError, match="unhashable type"):
        hash(line_number)


def test_code_attr_copy_deepcopy_and_clone_attribute_clone_nested_hot_objects():
    original = attributes.CodeAttr(
        1,
        0,
        2,
        1,
        1,
        [instructions.InsnInfo(instructions.InsnInfoType.RETURN, 0)],
        1,
        [attributes.ExceptionInfo(0, 1, 1, 2)],
        3,
        [
            attributes.LineNumberTableAttr(2, 0, 1, [attributes.LineNumberInfo(0, 10)]),
            attributes.LocalVariableTableAttr(3, 0, 1, [attributes.LocalVariableInfo(0, 1, 4, 5, 0)]),
            attributes.StackMapTableAttr(
                4,
                0,
                1,
                [
                    attributes.FullFrameInfo(
                        255,
                        0,
                        1,
                        [attributes.ObjectVariableInfo(constants.VerificationType.OBJECT, 9)],
                        1,
                        [attributes.UninitializedVariableInfo(constants.VerificationType.UNINITIALIZED, 7)],
                    )
                ],
            ),
        ],
    )

    shallow = copy.copy(original)
    deep = copy.deepcopy(original)
    cloned = cast(attributes.CodeAttr, clone_attribute(original))
    original_stack_map = cast(attributes.StackMapTableAttr, original.attributes[2])
    deep_stack_map = cast(attributes.StackMapTableAttr, deep.attributes[2])
    cloned_stack_map = cast(attributes.StackMapTableAttr, cloned.attributes[2])
    original_frame = cast(attributes.FullFrameInfo, original_stack_map.entries[0])
    deep_frame = cast(attributes.FullFrameInfo, deep_stack_map.entries[0])
    cloned_frame = cast(attributes.FullFrameInfo, cloned_stack_map.entries[0])

    assert shallow == original
    assert shallow is not original
    assert shallow.code is original.code
    assert shallow.exception_table is original.exception_table
    assert shallow.attributes is original.attributes

    assert deep == original
    assert deep is not original
    assert deep.code is not original.code
    assert deep.code[0] == original.code[0]
    assert deep.code[0] is not original.code[0]
    assert deep.exception_table is not original.exception_table
    assert deep.exception_table[0] == original.exception_table[0]
    assert deep.exception_table[0] is not original.exception_table[0]
    assert deep.attributes is not original.attributes
    assert deep.attributes[0] is not original.attributes[0]
    assert deep_frame is not original_frame
    assert deep_frame.locals[0] is not original_frame.locals[0]

    assert cloned == original
    assert cloned is not original
    assert cloned.code[0] is not original.code[0]
    assert cloned.exception_table[0] is not original.exception_table[0]
    assert cloned.attributes[0] is not original.attributes[0]
    assert cloned_frame is not original_frame
    assert cloned_frame.stack[0] is not original_frame.stack[0]


def _assert_migrated_value_object_semantics(value: object) -> None:
    shallow = copy.copy(value)
    deep = copy.deepcopy(value)
    ctor, args = cast(tuple[Callable[..., object], tuple[object, ...]], value.__reduce__())
    rebuilt = ctor(*args)

    assert shallow == value
    assert shallow is not value
    assert deep == value
    assert deep is not value
    assert rebuilt == value
    assert rebuilt is not value

    with pytest.raises(TypeError, match="unhashable type"):
        hash(value)


@pytest.mark.parametrize(
    "value",
    [
        attributes.ConstValueInfo(4),
        attributes.EnumConstantValueInfo(5, 6),
        attributes.ClassInfoValueInfo(7),
        attributes.ArrayValueInfo(1, [attributes.ElementValueInfo("I", attributes.ConstValueInfo(8))]),
        attributes.ElementValueInfo("e", attributes.EnumConstantValueInfo(9, 10)),
        attributes.ElementValuePairInfo(11, attributes.ElementValueInfo("c", attributes.ClassInfoValueInfo(12))),
        attributes.AnnotationInfo(
            13,
            1,
            [attributes.ElementValuePairInfo(14, attributes.ElementValueInfo("s", attributes.ConstValueInfo(15)))],
        ),
        attributes.ParameterAnnotationInfo(1, [attributes.AnnotationInfo(16, 0, [])]),
        attributes.TargetInfo(),
        attributes.TypeParameterTargetInfo(1),
        attributes.SupertypeTargetInfo(2),
        attributes.TypeParameterBoundTargetInfo(3, 4),
        attributes.EmptyTargetInfo(),
        attributes.FormalParameterTargetInfo(5),
        attributes.ThrowsTargetInfo(6),
        attributes.TableInfo(7, 8, 9),
        attributes.LocalvarTargetInfo(1, [attributes.TableInfo(10, 11, 12)]),
        attributes.CatchTargetInfo(13),
        attributes.OffsetTargetInfo(14),
        attributes.TypeArgumentTargetInfo(15, 16),
        attributes.PathInfo(17, 18),
        attributes.TypePathInfo(1, [attributes.PathInfo(19, 20)]),
        attributes.TypeAnnotationInfo(
            0x13,
            attributes.EmptyTargetInfo(),
            attributes.TypePathInfo(1, [attributes.PathInfo(0, 0)]),
            21,
            1,
            [attributes.ElementValuePairInfo(22, attributes.ElementValueInfo("I", attributes.ConstValueInfo(23)))],
        ),
    ],
)
def test_migrated_annotation_payload_objects_preserve_dataclass_style_semantics(value: object):
    _assert_migrated_value_object_semantics(value)


def test_migrated_annotation_payload_objects_preserve_repr_and_mutable_attrs():
    const_value = attributes.ConstValueInfo(4)
    pair = attributes.ElementValuePairInfo(6, attributes.ElementValueInfo("I", const_value))
    annotation = attributes.AnnotationInfo(5, 1, [pair])
    type_path = attributes.TypePathInfo(1, [attributes.PathInfo(0, 0)])

    assert repr(const_value) == "ConstValueInfo(const_value_index=4)"
    assert repr(pair.element_value) == "ElementValueInfo(tag='I', value=ConstValueInfo(const_value_index=4))"
    assert repr(annotation) == (
        "AnnotationInfo(type_index=5, num_element_value_pairs=1, "
        "element_value_pairs=[ElementValuePairInfo(element_name_index=6, "
        "element_value=ElementValueInfo(tag='I', value=ConstValueInfo(const_value_index=4)))])"
    )
    assert repr(attributes.EmptyTargetInfo()) == "EmptyTargetInfo()"
    assert repr(type_path) == "TypePathInfo(path_length=1, path=[PathInfo(type_path_kind=0, type_argument_index=0)])"

    const_value.const_value_index = 9
    pair.element_name_index = 7
    pair.element_value = attributes.ElementValueInfo("c", attributes.ClassInfoValueInfo(10))
    type_path.path_length = 2
    type_path.path.append(attributes.PathInfo(1, 0))

    assert const_value.const_value_index == 9
    assert pair.element_name_index == 7
    assert isinstance(pair.element_value.value, attributes.ClassInfoValueInfo)
    assert type_path.path_length == 2
    assert [(entry.type_path_kind, entry.type_argument_index) for entry in type_path.path] == [(0, 0), (1, 0)]


@pytest.mark.parametrize(
    "value",
    [
        attributes.InnerClassInfo(1, 2, 3, constants.NestedClassAccessFlag.PUBLIC),
        attributes.BootstrapMethodInfo(4, 2, [5, 6]),
        attributes.MethodParameterInfo(7, constants.MethodParameterAccessFlag.FINAL),
        attributes.RecordComponentInfo(8, 9, 1, [attributes.SignatureAttr(10, 0, 11)]),
    ],
)
def test_migrated_non_module_payload_objects_preserve_dataclass_style_semantics(value: object):
    _assert_migrated_value_object_semantics(value)


@pytest.mark.parametrize(
    "value",
    [
        attributes.RequiresInfo(1, constants.ModuleRequiresAccessFlag.TRANSITIVE, 2),
        attributes.ExportInfo(3, constants.ModuleExportsAccessFlag.MANDATED, 2, [4, 5]),
        attributes.OpensInfo(6, constants.ModuleOpensAccessFlag.SYNTHETIC, 1, [7]),
        attributes.ProvidesInfo(8, 2, [9, 10]),
    ],
)
def test_migrated_module_payload_objects_preserve_dataclass_style_semantics(value: object):
    _assert_migrated_value_object_semantics(value)


def test_migrated_non_module_payload_objects_preserve_repr_and_mutable_attrs():
    inner = attributes.InnerClassInfo(1, 2, 3, constants.NestedClassAccessFlag.PUBLIC)
    bootstrap = attributes.BootstrapMethodInfo(4, 2, [5, 6])
    parameter = attributes.MethodParameterInfo(7, constants.MethodParameterAccessFlag.FINAL)
    signature = attributes.SignatureAttr(10, 0, 11)
    component = attributes.RecordComponentInfo(8, 9, 1, [signature])

    assert repr(inner) == (
        "InnerClassInfo("
        "inner_class_info_index=1, outer_class_info_index=2, inner_name_index=3, "
        f"inner_class_access_flags={constants.NestedClassAccessFlag.PUBLIC!r})"
    )
    assert repr(bootstrap) == (
        "BootstrapMethodInfo("
        "bootstrap_method_ref=4, num_boostrap_arguments=2, boostrap_arguments=[5, 6])"
    )
    assert repr(parameter) == (
        "MethodParameterInfo("
        f"name_index=7, access_flags={constants.MethodParameterAccessFlag.FINAL!r})"
    )
    assert repr(component) == (
        "RecordComponentInfo("
        f"name_index=8, descriptor_index=9, attributes_count=1, attributes={[signature]!r})"
    )

    inner.inner_name_index = 12
    inner.inner_class_access_flags = constants.NestedClassAccessFlag.STATIC
    bootstrap.num_boostrap_arguments = 3
    bootstrap.boostrap_arguments.append(7)
    parameter.access_flags = constants.MethodParameterAccessFlag.SYNTHETIC
    component.attributes_count = 2
    component.attributes.append(attributes.DeprecatedAttr(12, 0))

    assert inner.inner_name_index == 12
    assert inner.inner_class_access_flags == constants.NestedClassAccessFlag.STATIC
    assert bootstrap.num_boostrap_arguments == 3
    assert bootstrap.boostrap_arguments == [5, 6, 7]
    assert parameter.access_flags == constants.MethodParameterAccessFlag.SYNTHETIC
    assert component.attributes_count == 2
    assert isinstance(component.attributes[1], attributes.DeprecatedAttr)


def test_migrated_module_payload_objects_preserve_repr_and_mutable_attrs():
    requires = attributes.RequiresInfo(1, constants.ModuleRequiresAccessFlag.TRANSITIVE, 2)
    export = attributes.ExportInfo(3, constants.ModuleExportsAccessFlag.MANDATED, 2, [4, 5])
    opened = attributes.OpensInfo(6, constants.ModuleOpensAccessFlag.SYNTHETIC, 1, [7])
    provide = attributes.ProvidesInfo(8, 2, [9, 10])

    assert repr(requires) == (
        "RequiresInfo("
        f"requires_index=1, requires_flag={constants.ModuleRequiresAccessFlag.TRANSITIVE!r}, "
        "requires_version_index=2)"
    )
    assert repr(export) == (
        "ExportInfo("
        f"exports_index=3, exports_flags={constants.ModuleExportsAccessFlag.MANDATED!r}, "
        "exports_to_count=2, exports_to_index=[4, 5])"
    )
    assert repr(opened) == (
        "OpensInfo("
        f"opens_index=6, opens_flags={constants.ModuleOpensAccessFlag.SYNTHETIC!r}, "
        "opens_to_count=1, opens_to_index=[7])"
    )
    assert repr(provide) == "ProvidesInfo(provides_index=8, provides_with_count=2, provides_with_index=[9, 10])"

    requires.requires_version_index = 11
    export.exports_to_count = 3
    export.exports_to_index.append(6)
    opened.opens_flags = constants.ModuleOpensAccessFlag.MANDATED
    opened.opens_to_index.append(8)
    provide.provides_with_count = 3
    provide.provides_with_index.append(11)

    assert requires.requires_version_index == 11
    assert export.exports_to_count == 3
    assert export.exports_to_index == [4, 5, 6]
    assert opened.opens_flags == constants.ModuleOpensAccessFlag.MANDATED
    assert opened.opens_to_index == [7, 8]
    assert provide.provides_with_count == 3
    assert provide.provides_with_index == [9, 10, 11]


def test_annotation_payload_copy_deepcopy_and_clone_attribute_clone_nested_objects():
    original = attributes.RuntimeVisibleTypeAnnotationsAttr(
        1,
        0,
        1,
        [
            attributes.TypeAnnotationInfo(
                constants.TargetType.TYPE_LOCAL_VARIABLE,
                attributes.LocalvarTargetInfo(1, [attributes.TableInfo(0, 5, 1)]),
                attributes.TypePathInfo(1, [attributes.PathInfo(constants.TypePathKind.ARRAY_TYPE, 0)]),
                2,
                1,
                [
                    attributes.ElementValuePairInfo(
                        3,
                        attributes.ElementValueInfo(
                            "[",
                            attributes.ArrayValueInfo(
                                2,
                                [
                                    attributes.ElementValueInfo("s", attributes.ConstValueInfo(4)),
                                    attributes.ElementValueInfo(
                                        "@",
                                        attributes.AnnotationInfo(
                                            5,
                                            1,
                                            [
                                                attributes.ElementValuePairInfo(
                                                    6,
                                                    attributes.ElementValueInfo(
                                                        "c",
                                                        attributes.ClassInfoValueInfo(7),
                                                    ),
                                                )
                                            ],
                                        ),
                                    ),
                                ],
                            ),
                        ),
                    )
                ],
            )
        ],
    )

    shallow = copy.copy(original)
    deep = copy.deepcopy(original)
    cloned = cast(attributes.RuntimeVisibleTypeAnnotationsAttr, clone_attribute(original))
    original_annotation = original.annotations[0]
    deep_annotation = deep.annotations[0]
    cloned_annotation = cloned.annotations[0]
    original_target = cast(attributes.LocalvarTargetInfo, original_annotation.target_info)
    deep_target = cast(attributes.LocalvarTargetInfo, deep_annotation.target_info)
    cloned_target = cast(attributes.LocalvarTargetInfo, cloned_annotation.target_info)
    original_array = cast(attributes.ArrayValueInfo, original_annotation.element_value_pairs[0].element_value.value)
    deep_array = cast(attributes.ArrayValueInfo, deep_annotation.element_value_pairs[0].element_value.value)
    cloned_array = cast(attributes.ArrayValueInfo, cloned_annotation.element_value_pairs[0].element_value.value)
    original_nested_annotation = cast(attributes.AnnotationInfo, original_array.values[1].value)
    deep_nested_annotation = cast(attributes.AnnotationInfo, deep_array.values[1].value)
    cloned_nested_annotation = cast(attributes.AnnotationInfo, cloned_array.values[1].value)

    assert shallow == original
    assert shallow is not original
    assert shallow.annotations is original.annotations

    assert deep == original
    assert deep is not original
    assert deep.annotations is not original.annotations
    assert deep_annotation is not original_annotation
    assert deep_target is not original_target
    assert deep_target.table is not original_target.table
    assert deep_target.table[0] is not original_target.table[0]
    assert deep_annotation.target_path is not original_annotation.target_path
    assert deep_annotation.target_path.path is not original_annotation.target_path.path
    assert deep_annotation.target_path.path[0] is not original_annotation.target_path.path[0]
    assert deep_array is not original_array
    assert deep_array.values is not original_array.values
    assert deep_array.values[0] is not original_array.values[0]
    assert deep_nested_annotation is not original_nested_annotation
    assert deep_nested_annotation.element_value_pairs[0] is not original_nested_annotation.element_value_pairs[0]

    assert cloned == original
    assert cloned is not original
    assert cloned.annotations is not original.annotations
    assert cloned_annotation is not original_annotation
    assert cloned_target is not original_target
    assert cloned_target.table[0] is not original_target.table[0]
    assert cloned_annotation.target_path is not original_annotation.target_path
    assert cloned_annotation.target_path.path[0] is not original_annotation.target_path.path[0]
    assert cloned_array is not original_array
    assert cloned_array.values[1] is not original_array.values[1]
    assert cloned_nested_annotation is not original_nested_annotation
    assert (
        cloned_nested_annotation.element_value_pairs[0].element_value.value
        is not original_nested_annotation.element_value_pairs[0].element_value.value
    )


def test_record_attr_copy_deepcopy_and_clone_attribute_clone_nested_non_module_payload_objects():
    original = attributes.RecordAttr(
        1,
        0,
        1,
        [
            attributes.RecordComponentInfo(
                2,
                3,
                3,
                [
                    attributes.InnerClassesAttr(
                        4,
                        0,
                        1,
                        [attributes.InnerClassInfo(5, 6, 7, constants.NestedClassAccessFlag.PUBLIC)],
                    ),
                    attributes.BootstrapMethodsAttr(
                        5,
                        0,
                        1,
                        [attributes.BootstrapMethodInfo(8, 2, [9, 10])],
                    ),
                    attributes.MethodParametersAttr(
                        6,
                        0,
                        1,
                        [attributes.MethodParameterInfo(11, constants.MethodParameterAccessFlag.FINAL)],
                    ),
                ],
            )
        ],
    )

    shallow = copy.copy(original)
    deep = copy.deepcopy(original)
    cloned = cast(attributes.RecordAttr, clone_attribute(original))
    original_component = original.components[0]
    deep_component = deep.components[0]
    cloned_component = cloned.components[0]
    original_inner_attr = cast(attributes.InnerClassesAttr, original_component.attributes[0])
    deep_inner_attr = cast(attributes.InnerClassesAttr, deep_component.attributes[0])
    cloned_inner_attr = cast(attributes.InnerClassesAttr, cloned_component.attributes[0])
    original_bootstrap_attr = cast(attributes.BootstrapMethodsAttr, original_component.attributes[1])
    deep_bootstrap_attr = cast(attributes.BootstrapMethodsAttr, deep_component.attributes[1])
    cloned_bootstrap_attr = cast(attributes.BootstrapMethodsAttr, cloned_component.attributes[1])
    original_method_attr = cast(attributes.MethodParametersAttr, original_component.attributes[2])
    deep_method_attr = cast(attributes.MethodParametersAttr, deep_component.attributes[2])
    cloned_method_attr = cast(attributes.MethodParametersAttr, cloned_component.attributes[2])

    assert shallow == original
    assert shallow is not original
    assert shallow.components is original.components
    assert shallow.components[0] is original.components[0]

    assert deep == original
    assert deep is not original
    assert deep.components is not original.components
    assert deep_component is not original_component
    assert deep_component.attributes is not original_component.attributes
    assert deep_inner_attr is not original_inner_attr
    assert deep_inner_attr.classes[0] is not original_inner_attr.classes[0]
    assert deep_bootstrap_attr.bootstrap_methods[0] is not original_bootstrap_attr.bootstrap_methods[0]
    assert (
        deep_bootstrap_attr.bootstrap_methods[0].boostrap_arguments
        is not original_bootstrap_attr.bootstrap_methods[0].boostrap_arguments
    )
    assert deep_method_attr.parameters[0] is not original_method_attr.parameters[0]

    assert cloned == original
    assert cloned is not original
    assert cloned.components is not original.components
    assert cloned_component is not original_component
    assert cloned_component.attributes is not original_component.attributes
    assert cloned_inner_attr is not original_inner_attr
    assert cloned_inner_attr.classes[0] is not original_inner_attr.classes[0]
    assert cloned_bootstrap_attr.bootstrap_methods[0] is not original_bootstrap_attr.bootstrap_methods[0]
    assert (
        cloned_bootstrap_attr.bootstrap_methods[0].boostrap_arguments
        is not original_bootstrap_attr.bootstrap_methods[0].boostrap_arguments
    )
    assert cloned_method_attr.parameters[0] is not original_method_attr.parameters[0]


def test_module_attr_copy_deepcopy_and_clone_attribute_clone_nested_module_payload_objects():
    original = attributes.ModuleAttr(
        1,
        0,
        2,
        constants.ModuleAccessFlag.OPEN,
        3,
        1,
        [attributes.RequiresInfo(4, constants.ModuleRequiresAccessFlag.TRANSITIVE, 5)],
        1,
        [attributes.ExportInfo(6, constants.ModuleExportsAccessFlag.MANDATED, 2, [7, 8])],
        1,
        [attributes.OpensInfo(9, constants.ModuleOpensAccessFlag.SYNTHETIC, 1, [10])],
        2,
        [11, 12],
        1,
        [attributes.ProvidesInfo(13, 2, [14, 15])],
    )

    shallow = copy.copy(original)
    deep = copy.deepcopy(original)
    cloned = cast(attributes.ModuleAttr, clone_attribute(original))
    original_requires = original.requires[0]
    deep_requires = deep.requires[0]
    cloned_requires = cloned.requires[0]
    original_export = original.exports[0]
    deep_export = deep.exports[0]
    cloned_export = cloned.exports[0]
    original_open = original.opens[0]
    deep_open = deep.opens[0]
    cloned_open = cloned.opens[0]
    original_provide = original.provides[0]
    deep_provide = deep.provides[0]
    cloned_provide = cloned.provides[0]

    assert shallow == original
    assert shallow is not original
    assert shallow.requires is original.requires
    assert shallow.exports is original.exports
    assert shallow.opens is original.opens
    assert shallow.uses_index is original.uses_index
    assert shallow.provides is original.provides

    assert deep == original
    assert deep is not original
    assert deep.requires is not original.requires
    assert deep_requires is not original_requires
    assert deep.exports is not original.exports
    assert deep_export is not original_export
    assert deep_export.exports_to_index is not original_export.exports_to_index
    assert deep.opens is not original.opens
    assert deep_open is not original_open
    assert deep_open.opens_to_index is not original_open.opens_to_index
    assert deep.uses_index is not original.uses_index
    assert deep.provides is not original.provides
    assert deep_provide is not original_provide
    assert deep_provide.provides_with_index is not original_provide.provides_with_index

    assert cloned == original
    assert cloned is not original
    assert cloned.requires is not original.requires
    assert cloned_requires is not original_requires
    assert cloned.exports is not original.exports
    assert cloned_export is not original_export
    assert cloned_export.exports_to_index is not original_export.exports_to_index
    assert cloned.opens is not original.opens
    assert cloned_open is not original_open
    assert cloned_open.opens_to_index is not original_open.opens_to_index
    assert cloned.uses_index is not original.uses_index
    assert cloned.provides is not original.provides
    assert cloned_provide is not original_provide
    assert cloned_provide.provides_with_index is not original_provide.provides_with_index


# ---------------------------------------------------------------------------
# Exceptions
# ---------------------------------------------------------------------------


def test_exceptions_empty():
    reader = attr_reader("Exceptions", u2(0))
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.ExceptionsAttr)
    assert attr.number_of_exceptions == 0
    assert attr.exception_index_table == []


def test_exceptions_one():
    reader = attr_reader("Exceptions", u2(1) + u2(5))
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.ExceptionsAttr)
    assert attr.number_of_exceptions == 1
    assert attr.exception_index_table == [5]


def test_exceptions_multiple():
    reader = attr_reader("Exceptions", u2(3) + u2(10) + u2(20) + u2(30))
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.ExceptionsAttr)
    assert attr.number_of_exceptions == 3
    assert attr.exception_index_table == [10, 20, 30]


# ---------------------------------------------------------------------------
# LineNumberTable
# ---------------------------------------------------------------------------


def test_line_number_table_empty():
    reader = attr_reader("LineNumberTable", u2(0))
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.LineNumberTableAttr)
    assert attr.line_number_table_length == 0
    assert attr.line_number_table == []


def test_line_number_table_two_entries():
    payload = u2(2) + u2(0) + u2(1) + u2(10) + u2(5)
    reader = attr_reader("LineNumberTable", payload)
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.LineNumberTableAttr)
    assert attr.line_number_table_length == 2
    assert attr.line_number_table[0].start_pc == 0
    assert attr.line_number_table[0].line_number == 1
    assert attr.line_number_table[1].start_pc == 10
    assert attr.line_number_table[1].line_number == 5


# ---------------------------------------------------------------------------
# LocalVariableTable
# ---------------------------------------------------------------------------


def test_local_variable_table_empty():
    reader = attr_reader("LocalVariableTable", u2(0))
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.LocalVariableTableAttr)
    assert attr.local_variable_table_length == 0
    assert attr.local_variable_table == []


def test_local_variable_table_one_entry():
    # start_pc=0, length=10, name_index=2, descriptor_index=3, index=1
    payload = u2(1) + u2(0) + u2(10) + u2(2) + u2(3) + u2(1)
    reader = attr_reader("LocalVariableTable", payload)
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.LocalVariableTableAttr)
    assert attr.local_variable_table_length == 1
    entry = attr.local_variable_table[0]
    assert entry.start_pc == 0
    assert entry.length == 10
    assert entry.name_index == 2
    assert entry.descriptor_index == 3
    assert entry.index == 1


# ---------------------------------------------------------------------------
# LocalVariableTypeTable
# ---------------------------------------------------------------------------


def test_local_variable_type_table_one_entry():
    # start_pc=0, length=8, name_index=4, signature_index=5, index=2
    payload = u2(1) + u2(0) + u2(8) + u2(4) + u2(5) + u2(2)
    reader = attr_reader("LocalVariableTypeTable", payload)
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.LocalVariableTypeTableAttr)
    assert attr.local_variable_type_table_length == 1
    entry = attr.local_variable_type_table[0]
    assert entry.start_pc == 0
    assert entry.length == 8
    assert entry.name_index == 4
    assert entry.signature_index == 5
    assert entry.index == 2


# ---------------------------------------------------------------------------
# InnerClasses
# ---------------------------------------------------------------------------


def test_inner_classes_empty():
    reader = attr_reader("InnerClasses", u2(0))
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.InnerClassesAttr)
    assert attr.number_of_classes == 0
    assert attr.classes == []


def test_inner_classes_one():
    # inner_class_info_index=2, outer_class_info_index=3, inner_name_index=4, flags=PUBLIC(0x0001)
    payload = u2(1) + u2(2) + u2(3) + u2(4) + u2(0x0001)
    reader = attr_reader("InnerClasses", payload)
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.InnerClassesAttr)
    assert attr.number_of_classes == 1
    info = attr.classes[0]
    assert info.inner_class_info_index == 2
    assert info.outer_class_info_index == 3
    assert info.inner_name_index == 4
    assert isinstance(info.inner_class_access_flags, constants.NestedClassAccessFlag)
    assert info.inner_class_access_flags == constants.NestedClassAccessFlag.PUBLIC


# ---------------------------------------------------------------------------
# BootstrapMethods
# ---------------------------------------------------------------------------


def test_bootstrap_methods_empty():
    reader = attr_reader("BootstrapMethods", u2(0))
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.BootstrapMethodsAttr)
    assert attr.num_bootstrap_methods == 0
    assert attr.bootstrap_methods == []


def test_bootstrap_methods_one_no_args():
    # bootstrap_method_ref=5, num_args=0
    payload = u2(1) + u2(5) + u2(0)
    reader = attr_reader("BootstrapMethods", payload)
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.BootstrapMethodsAttr)
    assert attr.num_bootstrap_methods == 1
    bm = attr.bootstrap_methods[0]
    assert bm.bootstrap_method_ref == 5
    assert bm.num_boostrap_arguments == 0
    assert bm.boostrap_arguments == []


def test_bootstrap_methods_one_with_args():
    # bootstrap_method_ref=5, num_args=2, args=[6, 7]
    payload = u2(1) + u2(5) + u2(2) + u2(6) + u2(7)
    reader = attr_reader("BootstrapMethods", payload)
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.BootstrapMethodsAttr)
    bm = attr.bootstrap_methods[0]
    assert bm.bootstrap_method_ref == 5
    assert bm.num_boostrap_arguments == 2
    assert bm.boostrap_arguments == [6, 7]


# ---------------------------------------------------------------------------
# MethodParameters (parameters_count is u1, not u2)
# ---------------------------------------------------------------------------


def test_method_parameters_empty():
    reader = attr_reader("MethodParameters", u1(0))
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.MethodParametersAttr)
    assert attr.parameters_count == 0
    assert attr.parameters == []


def test_method_parameters_one():
    # parameters_count=1, name_index=3, access_flags=0
    payload = u1(1) + u2(3) + u2(0)
    reader = attr_reader("MethodParameters", payload)
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.MethodParametersAttr)
    assert attr.parameters_count == 1
    param = attr.parameters[0]
    assert param.name_index == 3
    assert param.access_flags == constants.MethodParameterAccessFlag(0)


# ---------------------------------------------------------------------------
# NestMembers
# ---------------------------------------------------------------------------


def test_nest_members_empty():
    reader = attr_reader("NestMembers", u2(0))
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.NestMembersAttr)
    assert attr.number_of_classes == 0
    assert attr.classes == []


def test_nest_members_two():
    reader = attr_reader("NestMembers", u2(2) + u2(3) + u2(4))
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.NestMembersAttr)
    assert attr.number_of_classes == 2
    assert attr.classes == [3, 4]


# ---------------------------------------------------------------------------
# PermittedSubclasses
# ---------------------------------------------------------------------------


def test_permitted_subclasses_empty():
    reader = attr_reader("PermittedSubclasses", u2(0))
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.PermittedSubclassesAttr)
    assert attr.number_of_classes == 0
    assert attr.classes == []


def test_permitted_subclasses_three():
    reader = attr_reader("PermittedSubclasses", u2(3) + u2(5) + u2(6) + u2(7))
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.PermittedSubclassesAttr)
    assert attr.number_of_classes == 3
    assert attr.classes == [5, 6, 7]


# ---------------------------------------------------------------------------
# ModulePackages
# ---------------------------------------------------------------------------


def test_module_packages_empty():
    reader = attr_reader("ModulePackages", u2(0))
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.ModulePackagesAttr)
    assert attr.package_count == 0
    assert attr.package_index == []


def test_module_packages_two():
    reader = attr_reader("ModulePackages", u2(2) + u2(8) + u2(9))
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.ModulePackagesAttr)
    assert attr.package_count == 2
    assert attr.package_index == [8, 9]


# ---------------------------------------------------------------------------
# StackMapTable — frame types
# ---------------------------------------------------------------------------


def test_stackmaptable_empty():
    reader = attr_reader("StackMapTable", u2(0))
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.StackMapTableAttr)
    assert attr.number_of_entries == 0
    assert attr.entries == []


def test_stackmaptable_same_frame():
    payload = u2(1) + u1(0)
    reader = attr_reader("StackMapTable", payload)
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.StackMapTableAttr)
    assert attr.number_of_entries == 1
    assert isinstance(attr.entries[0], attributes.SameFrameInfo)
    assert attr.entries[0].frame_type == 0


def test_stackmaptable_same_frame_type63():
    payload = u2(1) + u1(63)
    reader = attr_reader("StackMapTable", payload)
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.StackMapTableAttr)
    assert isinstance(attr.entries[0], attributes.SameFrameInfo)
    assert attr.entries[0].frame_type == 63


def test_stackmaptable_same_locals_1_stack():
    # frame_type=64, vtype=INTEGER (tag=1)
    payload = u2(1) + u1(64) + u1(1)
    reader = attr_reader("StackMapTable", payload)
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.StackMapTableAttr)
    frame = attr.entries[0]
    assert isinstance(frame, attributes.SameLocals1StackItemFrameInfo)
    assert frame.frame_type == 64
    assert isinstance(frame.stack, attributes.IntegerVariableInfo)


def test_stackmaptable_same_locals_1_stack_extended():
    # frame_type=247, offset_delta=10, vtype=INTEGER (tag=1)
    payload = u2(1) + u1(247) + u2(10) + u1(1)
    reader = attr_reader("StackMapTable", payload)
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.StackMapTableAttr)
    frame = attr.entries[0]
    assert isinstance(frame, attributes.SameLocals1StackItemFrameExtendedInfo)
    assert frame.frame_type == 247
    assert frame.offset_delta == 10
    assert isinstance(frame.stack, attributes.IntegerVariableInfo)


def test_stackmaptable_chop_frame():
    # frame_type=249 (range 248-250), offset_delta=5
    payload = u2(1) + u1(249) + u2(5)
    reader = attr_reader("StackMapTable", payload)
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.StackMapTableAttr)
    frame = attr.entries[0]
    assert isinstance(frame, attributes.ChopFrameInfo)
    assert frame.frame_type == 249
    assert frame.offset_delta == 5


def test_stackmaptable_same_frame_extended():
    # frame_type=251, offset_delta=3
    payload = u2(1) + u1(251) + u2(3)
    reader = attr_reader("StackMapTable", payload)
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.StackMapTableAttr)
    frame = attr.entries[0]
    assert isinstance(frame, attributes.SameFrameExtendedInfo)
    assert frame.frame_type == 251
    assert frame.offset_delta == 3


def test_stackmaptable_append_frame_252():
    # frame_type=252 → 252-251=1 local, vtype=TOP (tag=0)
    payload = u2(1) + u1(252) + u2(1) + u1(0)
    reader = attr_reader("StackMapTable", payload)
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.StackMapTableAttr)
    frame = attr.entries[0]
    assert isinstance(frame, attributes.AppendFrameInfo)
    assert frame.frame_type == 252
    assert frame.offset_delta == 1
    assert len(frame.locals) == 1
    assert isinstance(frame.locals[0], attributes.TopVariableInfo)


def test_stackmaptable_append_frame_254():
    # frame_type=254 → 254-251=3 locals, vtypes: INTEGER, FLOAT, DOUBLE
    payload = u2(1) + u1(254) + u2(0) + u1(1) + u1(2) + u1(3)
    reader = attr_reader("StackMapTable", payload)
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.StackMapTableAttr)
    frame = attr.entries[0]
    assert isinstance(frame, attributes.AppendFrameInfo)
    assert len(frame.locals) == 3
    assert isinstance(frame.locals[0], attributes.IntegerVariableInfo)
    assert isinstance(frame.locals[1], attributes.FloatVariableInfo)
    assert isinstance(frame.locals[2], attributes.DoubleVariableInfo)


def test_stackmaptable_full_frame():
    # frame_type=255, offset_delta=0, 2 locals (INTEGER, FLOAT), 1 stack (LONG)
    payload = u2(1) + u1(255) + u2(0) + u2(2) + u1(1) + u1(2) + u2(1) + u1(4)
    reader = attr_reader("StackMapTable", payload)
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.StackMapTableAttr)
    frame = attr.entries[0]
    assert isinstance(frame, attributes.FullFrameInfo)
    assert frame.frame_type == 255
    assert frame.offset_delta == 0
    assert frame.number_of_locals == 2
    assert isinstance(frame.locals[0], attributes.IntegerVariableInfo)
    assert isinstance(frame.locals[1], attributes.FloatVariableInfo)
    assert frame.number_of_stack_items == 1
    assert isinstance(frame.stack[0], attributes.LongVariableInfo)


# ---------------------------------------------------------------------------
# Verification types (all 9 tags) via SameLocals1StackItemFrame (frame_type=64)
# ---------------------------------------------------------------------------


def _stackmap_with_vtype(vtype_bytes: bytes) -> attributes.StackMapTableAttr:
    payload = u2(1) + u1(64) + vtype_bytes
    reader = attr_reader("StackMapTable", payload)
    result = reader.read_attribute()
    assert isinstance(result, attributes.StackMapTableAttr)
    return result


def test_vtype_top():
    attr = _stackmap_with_vtype(u1(0))
    frame = attr.entries[0]
    assert isinstance(frame, attributes.SameLocals1StackItemFrameInfo)
    assert isinstance(frame.stack, attributes.TopVariableInfo)
    assert frame.stack.tag == constants.VerificationType.TOP


def test_vtype_integer():
    attr = _stackmap_with_vtype(u1(1))
    frame = attr.entries[0]
    assert isinstance(frame, attributes.SameLocals1StackItemFrameInfo)
    assert isinstance(frame.stack, attributes.IntegerVariableInfo)
    assert frame.stack.tag == constants.VerificationType.INTEGER


def test_vtype_float():
    attr = _stackmap_with_vtype(u1(2))
    frame = attr.entries[0]
    assert isinstance(frame, attributes.SameLocals1StackItemFrameInfo)
    assert isinstance(frame.stack, attributes.FloatVariableInfo)
    assert frame.stack.tag == constants.VerificationType.FLOAT


def test_vtype_double():
    attr = _stackmap_with_vtype(u1(3))
    frame = attr.entries[0]
    assert isinstance(frame, attributes.SameLocals1StackItemFrameInfo)
    assert isinstance(frame.stack, attributes.DoubleVariableInfo)
    assert frame.stack.tag == constants.VerificationType.DOUBLE


def test_vtype_long():
    attr = _stackmap_with_vtype(u1(4))
    frame = attr.entries[0]
    assert isinstance(frame, attributes.SameLocals1StackItemFrameInfo)
    assert isinstance(frame.stack, attributes.LongVariableInfo)
    assert frame.stack.tag == constants.VerificationType.LONG


def test_vtype_null():
    attr = _stackmap_with_vtype(u1(5))
    frame = attr.entries[0]
    assert isinstance(frame, attributes.SameLocals1StackItemFrameInfo)
    assert isinstance(frame.stack, attributes.NullVariableInfo)
    assert frame.stack.tag == constants.VerificationType.NULL


def test_vtype_uninitialized_this():
    attr = _stackmap_with_vtype(u1(6))
    frame = attr.entries[0]
    assert isinstance(frame, attributes.SameLocals1StackItemFrameInfo)
    assert isinstance(frame.stack, attributes.UninitializedThisVariableInfo)
    assert frame.stack.tag == constants.VerificationType.UNINITIALIZED_THIS


def test_vtype_object():
    attr = _stackmap_with_vtype(u1(7) + u2(42))
    frame = attr.entries[0]
    assert isinstance(frame, attributes.SameLocals1StackItemFrameInfo)
    assert isinstance(frame.stack, attributes.ObjectVariableInfo)
    assert frame.stack.tag == constants.VerificationType.OBJECT
    assert frame.stack.cpool_index == 42


def test_vtype_uninitialized():
    attr = _stackmap_with_vtype(u1(8) + u2(15))
    frame = attr.entries[0]
    assert isinstance(frame, attributes.SameLocals1StackItemFrameInfo)
    assert isinstance(frame.stack, attributes.UninitializedVariableInfo)
    assert frame.stack.tag == constants.VerificationType.UNINITIALIZED
    assert frame.stack.offset == 15


def test_vtype_unknown_raises():
    payload = u2(1) + u1(64) + u1(9)
    reader = attr_reader("StackMapTable", payload)
    with pytest.raises(ValueError):
        reader.read_attribute()


def test_stackmaptable_unknown_frame_type_raises():
    # frame_type=200 falls in the gap 128-246 (excluding 247), not valid
    payload = u2(1) + u1(200)
    reader = attr_reader("StackMapTable", payload)
    with pytest.raises(ValueError):
        reader.read_attribute()


# ---------------------------------------------------------------------------
# Annotations — RuntimeVisibleAnnotations / RuntimeInvisibleAnnotations
# ---------------------------------------------------------------------------


def test_runtime_visible_annotations_empty():
    reader = attr_reader("RuntimeVisibleAnnotations", u2(0))
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.RuntimeVisibleAnnotationsAttr)
    assert attr.num_annotations == 0
    assert attr.annotations == []


def test_runtime_visible_annotations_one():
    # one annotation: type_index=2, num_pairs=0
    payload = u2(1) + u2(2) + u2(0)
    reader = attr_reader("RuntimeVisibleAnnotations", payload)
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.RuntimeVisibleAnnotationsAttr)
    assert attr.num_annotations == 1
    ann = attr.annotations[0]
    assert isinstance(ann, attributes.AnnotationInfo)
    assert ann.type_index == 2
    assert ann.num_element_value_pairs == 0
    assert ann.element_value_pairs == []


def test_runtime_invisible_annotations_one():
    payload = u2(1) + u2(2) + u2(0)
    reader = attr_reader("RuntimeInvisibleAnnotations", payload)
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.RuntimeInvisibleAnnotationsAttr)
    assert attr.num_annotations == 1
    assert attr.annotations[0].type_index == 2


def test_annotation_with_element_value_pair():
    # one annotation, type_index=2, 1 pair: name_index=3, tag='I', const_value_index=4
    pair_bytes = u2(3) + u1(ord("I")) + u2(4)
    payload = u2(1) + u2(2) + u2(1) + pair_bytes
    reader = attr_reader("RuntimeVisibleAnnotations", payload)
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.RuntimeVisibleAnnotationsAttr)
    ann = attr.annotations[0]
    assert ann.num_element_value_pairs == 1
    pair = ann.element_value_pairs[0]
    assert pair.element_name_index == 3
    assert pair.element_value.tag == "I"
    assert isinstance(pair.element_value.value, attributes.ConstValueInfo)
    assert pair.element_value.value.const_value_index == 4


# ---------------------------------------------------------------------------
# Element value types (all 9 tag types) via AnnotationDefault
# ---------------------------------------------------------------------------


def _annotation_default(ev_bytes: bytes) -> attributes.AnnotationDefaultAttr:
    reader = attr_reader("AnnotationDefault", ev_bytes)
    result = reader.read_attribute()
    assert isinstance(result, attributes.AnnotationDefaultAttr)
    return result


def test_element_value_const_B():
    attr = _annotation_default(u1(ord("B")) + u2(42))
    assert attr.default_value.tag == "B"
    assert isinstance(attr.default_value.value, attributes.ConstValueInfo)
    assert attr.default_value.value.const_value_index == 42


def test_element_value_const_I():
    attr = _annotation_default(u1(ord("I")) + u2(99))
    assert attr.default_value.tag == "I"
    assert isinstance(attr.default_value.value, attributes.ConstValueInfo)
    assert attr.default_value.value.const_value_index == 99


def test_element_value_const_s():
    attr = _annotation_default(u1(ord("s")) + u2(7))
    assert attr.default_value.tag == "s"
    assert isinstance(attr.default_value.value, attributes.ConstValueInfo)
    assert attr.default_value.value.const_value_index == 7


def test_element_value_const_J():
    attr = _annotation_default(u1(ord("J")) + u2(100))
    assert attr.default_value.tag == "J"
    assert isinstance(attr.default_value.value, attributes.ConstValueInfo)
    assert attr.default_value.value.const_value_index == 100


def test_element_value_enum():
    # tag='e', type_name_index=5, const_name_index=6
    attr = _annotation_default(u1(ord("e")) + u2(5) + u2(6))
    assert attr.default_value.tag == "e"
    assert isinstance(attr.default_value.value, attributes.EnumConstantValueInfo)
    assert attr.default_value.value.type_name_index == 5
    assert attr.default_value.value.const_name_index == 6


def test_element_value_class():
    # tag='c', class_info_index=8
    attr = _annotation_default(u1(ord("c")) + u2(8))
    assert attr.default_value.tag == "c"
    assert isinstance(attr.default_value.value, attributes.ClassInfoValueInfo)
    assert attr.default_value.value.class_info_index == 8


def test_element_value_annotation():
    # tag='@', nested annotation: type_index=2, 0 pairs
    attr = _annotation_default(u1(ord("@")) + u2(2) + u2(0))
    assert attr.default_value.tag == "@"
    assert isinstance(attr.default_value.value, attributes.AnnotationInfo)
    assert attr.default_value.value.type_index == 2
    assert attr.default_value.value.num_element_value_pairs == 0


def test_element_value_array():
    # tag='[', 2 values: both 'I' tag
    array_bytes = u1(ord("[")) + u2(2) + u1(ord("I")) + u2(10) + u1(ord("I")) + u2(20)
    attr = _annotation_default(array_bytes)
    assert attr.default_value.tag == "["
    arr = attr.default_value.value
    assert isinstance(arr, attributes.ArrayValueInfo)
    assert arr.num_values == 2
    assert arr.values[0].tag == "I"
    assert isinstance(arr.values[0].value, attributes.ConstValueInfo)
    assert arr.values[0].value.const_value_index == 10
    assert arr.values[1].tag == "I"
    assert isinstance(arr.values[1].value, attributes.ConstValueInfo)
    assert arr.values[1].value.const_value_index == 20


def test_element_value_unknown_tag():
    reader = attr_reader("AnnotationDefault", u1(ord("X")))
    with pytest.raises(ValueError):
        reader.read_attribute()


# ---------------------------------------------------------------------------
# AnnotationDefault (const value)
# ---------------------------------------------------------------------------


def test_annotation_default_const():
    attr = _annotation_default(u1(ord("I")) + u2(42))
    assert isinstance(attr, attributes.AnnotationDefaultAttr)
    ev = attr.default_value
    assert ev.tag == "I"
    assert isinstance(ev.value, attributes.ConstValueInfo)
    assert ev.value.const_value_index == 42


# ---------------------------------------------------------------------------
# Parameter annotations
# ---------------------------------------------------------------------------


def test_runtime_visible_parameter_annotations():
    # 2 parameters: first has 1 annotation (type_index=2, 0 pairs), second has 0
    ann_bytes = u2(2) + u2(0)
    payload = u1(2) + u2(1) + ann_bytes + u2(0)
    reader = attr_reader("RuntimeVisibleParameterAnnotations", payload)
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.RuntimeVisibleParameterAnnotationsAttr)
    assert attr.num_parameters == 2
    first = attr.parameter_annotations[0]
    assert first.num_annotations == 1
    assert first.annotations[0].type_index == 2
    second = attr.parameter_annotations[1]
    assert second.num_annotations == 0
    assert second.annotations == []


def test_runtime_invisible_parameter_annotations():
    # 1 parameter with 0 annotations
    payload = u1(1) + u2(0)
    reader = attr_reader("RuntimeInvisibleParameterAnnotations", payload)
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.RuntimeInvisibleParameterAnnotationsAttr)
    assert attr.num_parameters == 1
    assert attr.parameter_annotations[0].num_annotations == 0


# ---------------------------------------------------------------------------
# Type annotations — RuntimeVisibleTypeAnnotations
# ---------------------------------------------------------------------------


# Helper: wrap a TypeAnnotationInfo inside a RuntimeVisibleTypeAnnotations attr
def _type_ann_attr(
    target_type_byte: int, target_info_bytes: bytes, type_index: int = 2
) -> attributes.RuntimeVisibleTypeAnnotationsAttr:
    type_path_bytes = u1(0)  # path_length=0
    ann_bytes = (
        u1(target_type_byte) + target_info_bytes + type_path_bytes + u2(type_index) + u2(0)  # num_element_value_pairs=0
    )
    payload = u2(1) + ann_bytes
    reader = attr_reader("RuntimeVisibleTypeAnnotations", payload)
    result = reader.read_attribute()
    assert isinstance(result, attributes.RuntimeVisibleTypeAnnotationsAttr)
    return result


def test_type_annotation_type_parameter_target():
    attr = _type_ann_attr(0x00, u1(3))
    assert isinstance(attr, attributes.RuntimeVisibleTypeAnnotationsAttr)
    ann = attr.annotations[0]
    assert ann.target_type == constants.TargetType.TYPE_PARAMETER_GENERIC_CLASS_OR_INTERFACE
    assert isinstance(ann.target_info, attributes.TypeParameterTargetInfo)
    assert ann.target_info.type_parameter_index == 3


def test_type_annotation_supertype_target():
    attr = _type_ann_attr(0x10, u2(5))
    ann = attr.annotations[0]
    assert ann.target_type == constants.TargetType.SUPERTYPE
    assert isinstance(ann.target_info, attributes.SupertypeTargetInfo)
    assert ann.target_info.supertype_index == 5


def test_type_annotation_type_parameter_bound():
    attr = _type_ann_attr(0x11, u1(2) + u1(1))
    ann = attr.annotations[0]
    assert ann.target_type == constants.TargetType.TYPE_PARAMETER_BOUND_GENERIC_CLASS_OR_INTERFACE
    assert isinstance(ann.target_info, attributes.TypeParameterBoundTargetInfo)
    assert ann.target_info.type_parameter_index == 2
    assert ann.target_info.bound_index == 1


def test_type_annotation_empty_target():
    # target_type=0x13 = TYPE_IN_FIELD_OR_RECORD → EmptyTargetInfo (no bytes)
    attr = _type_ann_attr(0x13, b"")
    ann = attr.annotations[0]
    assert ann.target_type == constants.TargetType.TYPE_IN_FIELD_OR_RECORD
    assert isinstance(ann.target_info, attributes.EmptyTargetInfo)


def test_type_annotation_formal_parameter():
    attr = _type_ann_attr(0x16, u1(1))
    ann = attr.annotations[0]
    assert ann.target_type == constants.TargetType.FORMAL_PARAMETER_METHOD_CONSTRUCTOR_OR_LAMBDA
    assert isinstance(ann.target_info, attributes.FormalParameterTargetInfo)
    assert ann.target_info.formal_parameter_index == 1


def test_type_annotation_throws_target():
    attr = _type_ann_attr(0x17, u2(4))
    ann = attr.annotations[0]
    assert ann.target_type == constants.TargetType.TYPE_THROWS
    assert isinstance(ann.target_info, attributes.ThrowsTargetInfo)
    assert ann.target_info.throws_type_index == 4


def test_type_annotation_localvar_target():
    # target_type=0x40, table_length=1, entry: start_pc=0, length=5, index=1
    table_bytes = u2(1) + u2(0) + u2(5) + u2(1)
    attr = _type_ann_attr(0x40, table_bytes)
    ann = attr.annotations[0]
    assert ann.target_type == constants.TargetType.TYPE_LOCAL_VARIABLE
    assert isinstance(ann.target_info, attributes.LocalvarTargetInfo)
    assert ann.target_info.table_length == 1
    entry = ann.target_info.table[0]
    assert entry.start_pc == 0
    assert entry.length == 5
    assert entry.index == 1


def test_type_annotation_catch_target():
    attr = _type_ann_attr(0x42, u2(3))
    ann = attr.annotations[0]
    assert ann.target_type == constants.TargetType.TYPE_EXCEPTION_PARAMETER
    assert isinstance(ann.target_info, attributes.CatchTargetInfo)
    assert ann.target_info.exception_table_index == 3


def test_type_annotation_offset_target():
    attr = _type_ann_attr(0x43, u2(10))
    ann = attr.annotations[0]
    assert ann.target_type == constants.TargetType.TYPE_INSTANCEOF
    assert isinstance(ann.target_info, attributes.OffsetTargetInfo)
    assert ann.target_info.offset == 10


def test_type_annotation_type_argument_target():
    # target_type=0x47 = TYPE_CAST → TypeArgumentTargetInfo(offset, type_arg_index)
    attr = _type_ann_attr(0x47, u2(20) + u1(2))
    ann = attr.annotations[0]
    assert ann.target_type == constants.TargetType.TYPE_CAST
    assert isinstance(ann.target_info, attributes.TypeArgumentTargetInfo)
    assert ann.target_info.offset == 20
    assert ann.target_info.type_argument_index == 2


def test_type_annotation_type_path():
    # target_type=0x13 (empty target), path_length=1, kind=0 (ARRAY_TYPE), arg_idx=0
    type_path_bytes = u1(1) + u1(0) + u1(0)
    ann_bytes = u1(0x13) + b"" + type_path_bytes + u2(2) + u2(0)
    payload = u2(1) + ann_bytes
    reader = attr_reader("RuntimeVisibleTypeAnnotations", payload)
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.RuntimeVisibleTypeAnnotationsAttr)
    ann = attr.annotations[0]
    path_info = ann.target_path
    assert isinstance(path_info, attributes.TypePathInfo)
    assert path_info.path_length == 1
    assert len(path_info.path) == 1
    assert path_info.path[0].type_path_kind == constants.TypePathKind.ARRAY_TYPE
    assert path_info.path[0].type_argument_index == 0


def test_type_annotation_unknown_target_type_raises():
    # target_type=0xFF is not in any valid TargetInfoType range
    type_path_bytes = u1(0)
    ann_bytes = u1(0xFF) + type_path_bytes + u2(2) + u2(0)
    payload = u2(1) + ann_bytes
    reader = attr_reader("RuntimeVisibleTypeAnnotations", payload)
    with pytest.raises(ValueError):
        reader.read_attribute()


def test_runtime_invisible_type_annotations():
    type_path_bytes = u1(0)
    ann_bytes = u1(0x13) + b"" + type_path_bytes + u2(2) + u2(0)
    payload = u2(1) + ann_bytes
    reader = attr_reader("RuntimeInvisibleTypeAnnotations", payload)
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.RuntimeInvisibleTypeAnnotationsAttr)
    assert attr.num_annotations == 1
    assert isinstance(attr.annotations[0].target_info, attributes.EmptyTargetInfo)


# ---------------------------------------------------------------------------
# Module attribute
# ---------------------------------------------------------------------------


def test_module_attr_empty():
    payload = (
        u2(2)  # module_name_index
        + u2(0)  # module_flags=0
        + u2(3)  # module_version_index
        + u2(0)  # requires_count=0
        + u2(0)  # exports_count=0
        + u2(0)  # opens_count=0
        + u2(0)  # uses_count=0
        + u2(0)  # provides_count=0
    )
    reader = attr_reader("Module", payload)
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.ModuleAttr)
    assert attr.module_name_index == 2
    assert attr.module_flags == constants.ModuleAccessFlag(0)
    assert attr.module_version_index == 3
    assert attr.requires_count == 0
    assert attr.requires == []
    assert attr.exports_count == 0
    assert attr.exports == []
    assert attr.opens_count == 0
    assert attr.opens == []
    assert attr.uses_count == 0
    assert attr.uses_index == []
    assert attr.provides_count == 0
    assert attr.provides == []


def test_module_attr_with_requires():
    payload = (
        u2(2)
        + u2(0)
        + u2(0)
        + u2(1)  # requires_count=1
        + u2(3)
        + u2(0)
        + u2(0)  # requires_index=3, flags=0, version=0
        + u2(0)  # exports_count=0
        + u2(0)  # opens_count=0
        + u2(0)  # uses_count=0
        + u2(0)  # provides_count=0
    )
    reader = attr_reader("Module", payload)
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.ModuleAttr)
    assert attr.requires_count == 1
    req = attr.requires[0]
    assert isinstance(req, attributes.RequiresInfo)
    assert req.requires_index == 3
    assert req.requires_flag == constants.ModuleRequiresAccessFlag(0)
    assert req.requires_version_index == 0


def test_module_attr_with_exports():
    payload = (
        u2(2)
        + u2(0)
        + u2(0)
        + u2(0)  # requires_count=0
        + u2(1)  # exports_count=1
        + u2(5)
        + u2(0)
        + u2(2)
        + u2(6)
        + u2(7)  # idx=5, flags=0, to_count=2, to=[6,7]
        + u2(0)  # opens_count=0
        + u2(0)  # uses_count=0
        + u2(0)  # provides_count=0
    )
    reader = attr_reader("Module", payload)
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.ModuleAttr)
    assert attr.exports_count == 1
    exp = attr.exports[0]
    assert isinstance(exp, attributes.ExportInfo)
    assert exp.exports_index == 5
    assert exp.exports_flags == constants.ModuleExportsAccessFlag(0)
    assert exp.exports_to_count == 2
    assert exp.exports_to_index == [6, 7]


def test_module_attr_with_opens_and_provides():
    payload = (
        u2(2)
        + u2(0)
        + u2(0)
        + u2(0)  # requires_count=0
        + u2(0)  # exports_count=0
        + u2(1)  # opens_count=1
        + u2(8)
        + u2(0)
        + u2(2)
        + u2(9)
        + u2(10)  # idx=8, flags=0, to_count=2, to=[9,10]
        + u2(1)  # uses_count=1
        + u2(11)
        + u2(1)  # provides_count=1
        + u2(12)
        + u2(2)
        + u2(13)
        + u2(14)  # idx=12, with_count=2, with=[13,14]
    )
    reader = attr_reader("Module", payload)
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.ModuleAttr)
    assert attr.opens_count == 1
    opened = attr.opens[0]
    assert isinstance(opened, attributes.OpensInfo)
    assert opened.opens_index == 8
    assert opened.opens_flags == constants.ModuleOpensAccessFlag(0)
    assert opened.opens_to_count == 2
    assert opened.opens_to_index == [9, 10]
    assert attr.uses_count == 1
    assert attr.uses_index == [11]
    assert attr.provides_count == 1
    provide = attr.provides[0]
    assert isinstance(provide, attributes.ProvidesInfo)
    assert provide.provides_index == 12
    assert provide.provides_with_count == 2
    assert provide.provides_with_index == [13, 14]


# ---------------------------------------------------------------------------
# Record
# ---------------------------------------------------------------------------


def test_record_empty():
    reader = attr_reader("Record", u2(0))
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.RecordAttr)
    assert attr.components_count == 0
    assert attr.components == []


def test_record_one_component_no_attrs():
    # components_count=1, name_index=2, descriptor_index=3, attributes_count=0
    payload = u2(1) + u2(2) + u2(3) + u2(0)
    reader = attr_reader("Record", payload)
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.RecordAttr)
    assert attr.components_count == 1
    comp = attr.components[0]
    assert isinstance(comp, attributes.RecordComponentInfo)
    assert comp.name_index == 2
    assert comp.descriptor_index == 3
    assert comp.attributes_count == 0
    assert comp.attributes == []


# ---------------------------------------------------------------------------
# UnimplementedAttr (unknown attribute names)
# ---------------------------------------------------------------------------


def test_unimplemented_attr():
    reader = attr_reader("UnknownFooBar", b"\x01\x02\x03")
    attr = reader.read_attribute()
    assert isinstance(attr, attributes.UnimplementedAttr)
    assert attr.info == b"\x01\x02\x03"
    assert attr.attribute_name_index == 1
