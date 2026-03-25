from __future__ import annotations

import pytest

from pytecode import attributes, constants, instructions
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
    cp_list = [
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
    assert attr.number_of_entries == 1
    assert isinstance(attr.entries[0], attributes.SameFrameInfo)
    assert attr.entries[0].frame_type == 0


def test_stackmaptable_same_frame_type63():
    payload = u2(1) + u1(63)
    reader = attr_reader("StackMapTable", payload)
    attr = reader.read_attribute()
    assert isinstance(attr.entries[0], attributes.SameFrameInfo)
    assert attr.entries[0].frame_type == 63


def test_stackmaptable_same_locals_1_stack():
    # frame_type=64, vtype=INTEGER (tag=1)
    payload = u2(1) + u1(64) + u1(1)
    reader = attr_reader("StackMapTable", payload)
    attr = reader.read_attribute()
    frame = attr.entries[0]
    assert isinstance(frame, attributes.SameLocals1StackItemFrameInfo)
    assert frame.frame_type == 64
    assert isinstance(frame.stack, attributes.IntegerVariableInfo)


def test_stackmaptable_same_locals_1_stack_extended():
    # frame_type=247, offset_delta=10, vtype=INTEGER (tag=1)
    payload = u2(1) + u1(247) + u2(10) + u1(1)
    reader = attr_reader("StackMapTable", payload)
    attr = reader.read_attribute()
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
    frame = attr.entries[0]
    assert isinstance(frame, attributes.ChopFrameInfo)
    assert frame.frame_type == 249
    assert frame.offset_delta == 5


def test_stackmaptable_same_frame_extended():
    # frame_type=251, offset_delta=3
    payload = u2(1) + u1(251) + u2(3)
    reader = attr_reader("StackMapTable", payload)
    attr = reader.read_attribute()
    frame = attr.entries[0]
    assert isinstance(frame, attributes.SameFrameExtendedInfo)
    assert frame.frame_type == 251
    assert frame.offset_delta == 3


def test_stackmaptable_append_frame_252():
    # frame_type=252 → 252-251=1 local, vtype=TOP (tag=0)
    payload = u2(1) + u1(252) + u2(1) + u1(0)
    reader = attr_reader("StackMapTable", payload)
    attr = reader.read_attribute()
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
    frame = attr.entries[0]
    assert isinstance(frame, attributes.AppendFrameInfo)
    assert len(frame.locals) == 3
    assert isinstance(frame.locals[0], attributes.IntegerVariableInfo)
    assert isinstance(frame.locals[1], attributes.FloatVariableInfo)
    assert isinstance(frame.locals[2], attributes.DoubleVariableInfo)


def test_stackmaptable_full_frame():
    # frame_type=255, offset_delta=0, 2 locals (INTEGER, FLOAT), 1 stack (LONG)
    payload = (
        u2(1) + u1(255) + u2(0)
        + u2(2) + u1(1) + u1(2)
        + u2(1) + u1(4)
    )
    reader = attr_reader("StackMapTable", payload)
    attr = reader.read_attribute()
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
    return reader.read_attribute()


def test_vtype_top():
    attr = _stackmap_with_vtype(u1(0))
    assert isinstance(attr.entries[0].stack, attributes.TopVariableInfo)
    assert attr.entries[0].stack.tag == constants.VerificationType.TOP


def test_vtype_integer():
    attr = _stackmap_with_vtype(u1(1))
    assert isinstance(attr.entries[0].stack, attributes.IntegerVariableInfo)
    assert attr.entries[0].stack.tag == constants.VerificationType.INTEGER


def test_vtype_float():
    attr = _stackmap_with_vtype(u1(2))
    assert isinstance(attr.entries[0].stack, attributes.FloatVariableInfo)
    assert attr.entries[0].stack.tag == constants.VerificationType.FLOAT


def test_vtype_double():
    attr = _stackmap_with_vtype(u1(3))
    assert isinstance(attr.entries[0].stack, attributes.DoubleVariableInfo)
    assert attr.entries[0].stack.tag == constants.VerificationType.DOUBLE


def test_vtype_long():
    attr = _stackmap_with_vtype(u1(4))
    assert isinstance(attr.entries[0].stack, attributes.LongVariableInfo)
    assert attr.entries[0].stack.tag == constants.VerificationType.LONG


def test_vtype_null():
    attr = _stackmap_with_vtype(u1(5))
    assert isinstance(attr.entries[0].stack, attributes.NullVariableInfo)
    assert attr.entries[0].stack.tag == constants.VerificationType.NULL


def test_vtype_uninitialized_this():
    attr = _stackmap_with_vtype(u1(6))
    assert isinstance(attr.entries[0].stack, attributes.UninitializedThisVariableInfo)
    assert attr.entries[0].stack.tag == constants.VerificationType.UNINITIALIZED_THIS


def test_vtype_object():
    attr = _stackmap_with_vtype(u1(7) + u2(42))
    frame = attr.entries[0]
    assert isinstance(frame.stack, attributes.ObjectVariableInfo)
    assert frame.stack.tag == constants.VerificationType.OBJECT
    assert frame.stack.cpool_index == 42


def test_vtype_uninitialized():
    attr = _stackmap_with_vtype(u1(8) + u2(15))
    frame = attr.entries[0]
    assert isinstance(frame.stack, attributes.UninitializedVariableInfo)
    assert frame.stack.tag == constants.VerificationType.UNINITIALIZED
    assert frame.stack.offset == 15


def test_vtype_unknown_raises():
    payload = u2(1) + u1(64) + u1(9)
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
    return reader.read_attribute()


def test_element_value_const_B():
    attr = _annotation_default(u1(ord("B")) + u2(42))
    assert attr.default_value.tag == "B"
    assert isinstance(attr.default_value.value, attributes.ConstValueInfo)
    assert attr.default_value.value.const_value_index == 42


def test_element_value_const_I():
    attr = _annotation_default(u1(ord("I")) + u2(99))
    assert attr.default_value.tag == "I"
    assert attr.default_value.value.const_value_index == 99


def test_element_value_const_s():
    attr = _annotation_default(u1(ord("s")) + u2(7))
    assert attr.default_value.tag == "s"
    assert isinstance(attr.default_value.value, attributes.ConstValueInfo)
    assert attr.default_value.value.const_value_index == 7


def test_element_value_const_J():
    attr = _annotation_default(u1(ord("J")) + u2(100))
    assert attr.default_value.tag == "J"
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
    assert arr.values[0].value.const_value_index == 10
    assert arr.values[1].tag == "I"
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
    payload = u2(2) + u2(1) + ann_bytes + u2(0)
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
    payload = u2(1) + u2(0)
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
        u1(target_type_byte)
        + target_info_bytes
        + type_path_bytes
        + u2(type_index)
        + u2(0)  # num_element_value_pairs=0
    )
    payload = u2(1) + ann_bytes
    reader = attr_reader("RuntimeVisibleTypeAnnotations", payload)
    return reader.read_attribute()


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
    ann = attr.annotations[0]
    path_info = ann.target_path
    assert isinstance(path_info, attributes.TypePathInfo)
    assert path_info.path_length == 1
    assert len(path_info.path) == 1
    assert path_info.path[0].type_path_kind == constants.TypePathKind.ARRAY_TYPE
    assert path_info.path[0].type_argument_index == 0


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
        u2(2)   # module_name_index
        + u2(0) # module_flags=0
        + u2(3) # module_version_index
        + u2(0) # requires_count=0
        + u2(0) # exports_count=0
        + u2(0) # opens_count=0
        + u2(0) # uses_count=0
        + u2(0) # provides_count=0
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
        u2(2) + u2(0) + u2(0)
        + u2(1)               # requires_count=1
        + u2(3) + u2(0) + u2(0)  # requires_index=3, flags=0, version=0
        + u2(0)               # exports_count=0
        + u2(0)               # opens_count=0
        + u2(0)               # uses_count=0
        + u2(0)               # provides_count=0
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
        u2(2) + u2(0) + u2(0)
        + u2(0)                   # requires_count=0
        + u2(1)                   # exports_count=1
        + u2(5) + u2(0) + u2(2) + u2(6) + u2(7)  # idx=5, flags=0, to_count=2, to=[6,7]
        + u2(0)                   # opens_count=0
        + u2(0)                   # uses_count=0
        + u2(0)                   # provides_count=0
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
