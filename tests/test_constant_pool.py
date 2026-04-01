from __future__ import annotations

import pytest

import pytecode.classfile.constant_pool as cp_module
from pytecode.classfile.modified_utf8 import encode_modified_utf8
from pytecode.classfile.reader import ClassReader
from tests.helpers import (
    class_entry_bytes,
    class_reader_for_cp,
    double_entry_bytes,
    dynamic_entry_bytes,
    fieldref_entry_bytes,
    float_entry_bytes,
    integer_entry_bytes,
    interface_methodref_entry_bytes,
    invoke_dynamic_entry_bytes,
    long_entry_bytes,
    method_handle_entry_bytes,
    method_type_entry_bytes,
    methodref_entry_bytes,
    minimal_classfile,
    module_entry_bytes,
    name_and_type_entry_bytes,
    package_entry_bytes,
    string_entry_bytes,
    utf8_entry_bytes,
)

# ---------------------------------------------------------------------------
# UTF-8 entries (tag 1)
# ---------------------------------------------------------------------------


def test_utf8_entry():
    data = utf8_entry_bytes("Hello")
    reader = class_reader_for_cp(data)
    cp_info, index_extra = reader.read_constant_pool_index(1)
    assert isinstance(cp_info, cp_module.Utf8Info)
    assert cp_info.index == 1
    assert cp_info.tag == 1
    assert cp_info.length == 5
    assert cp_info.str_bytes == b"Hello"
    assert index_extra == 0


def test_utf8_empty_string():
    data = utf8_entry_bytes("")
    reader = class_reader_for_cp(data)
    cp_info, index_extra = reader.read_constant_pool_index(2)
    assert isinstance(cp_info, cp_module.Utf8Info)
    assert cp_info.index == 2
    assert cp_info.tag == 1
    assert cp_info.length == 0
    assert cp_info.str_bytes == b""
    assert index_extra == 0


def test_utf8_multibyte():
    s = "café"
    data = utf8_entry_bytes(s)
    reader = class_reader_for_cp(data)
    cp_info, index_extra = reader.read_constant_pool_index(3)
    assert isinstance(cp_info, cp_module.Utf8Info)
    assert cp_info.str_bytes == encode_modified_utf8(s)
    assert cp_info.length == len(encode_modified_utf8(s))
    assert index_extra == 0


def test_utf8_nul_uses_modified_encoding():
    data = utf8_entry_bytes("\x00")
    reader = class_reader_for_cp(data)
    cp_info, index_extra = reader.read_constant_pool_index(4)
    assert isinstance(cp_info, cp_module.Utf8Info)
    assert cp_info.str_bytes == b"\xc0\x80"
    assert cp_info.length == 2
    assert index_extra == 0


def test_utf8_supplementary_char_uses_surrogate_encoding():
    data = utf8_entry_bytes("😀")
    reader = class_reader_for_cp(data)
    cp_info, index_extra = reader.read_constant_pool_index(5)
    assert isinstance(cp_info, cp_module.Utf8Info)
    assert cp_info.str_bytes == b"\xed\xa0\xbd\xed\xb8\x80"
    assert cp_info.length == 6
    assert index_extra == 0


# ---------------------------------------------------------------------------
# Integer / Float entries (tags 3, 4)
# ---------------------------------------------------------------------------


def test_integer_entry():
    data = integer_entry_bytes(0x0A0B0C0D)
    reader = class_reader_for_cp(data)
    cp_info, index_extra = reader.read_constant_pool_index(1)
    assert isinstance(cp_info, cp_module.IntegerInfo)
    assert cp_info.index == 1
    assert cp_info.tag == 3
    assert cp_info.value_bytes == 0x0A0B0C0D
    assert index_extra == 0


def test_float_entry():
    raw_bits = 0x3F800000  # IEEE 754 for 1.0
    data = float_entry_bytes(raw_bits)
    reader = class_reader_for_cp(data)
    cp_info, index_extra = reader.read_constant_pool_index(1)
    assert isinstance(cp_info, cp_module.FloatInfo)
    assert cp_info.tag == 4
    assert cp_info.value_bytes == raw_bits
    assert index_extra == 0


# ---------------------------------------------------------------------------
# Long / Double entries (tags 5, 6) — double-slot
# ---------------------------------------------------------------------------


def test_long_entry():
    data = long_entry_bytes(0xDEADBEEF, 0xCAFEBABE)
    reader = class_reader_for_cp(data)
    cp_info, index_extra = reader.read_constant_pool_index(1)
    assert isinstance(cp_info, cp_module.LongInfo)
    assert cp_info.index == 1
    assert cp_info.tag == 5
    assert cp_info.high_bytes == 0xDEADBEEF
    assert cp_info.low_bytes == 0xCAFEBABE
    assert index_extra == 1


def test_double_entry():
    # IEEE 754 double for π: 0x400921FB54442D18
    data = double_entry_bytes(0x400921FB, 0x54442D18)
    reader = class_reader_for_cp(data)
    cp_info, index_extra = reader.read_constant_pool_index(1)
    assert isinstance(cp_info, cp_module.DoubleInfo)
    assert cp_info.index == 1
    assert cp_info.tag == 6
    assert cp_info.high_bytes == 0x400921FB
    assert cp_info.low_bytes == 0x54442D18
    assert index_extra == 1


# ---------------------------------------------------------------------------
# Reference entries (tags 7, 8, 9, 10, 11, 12)
# ---------------------------------------------------------------------------


def test_class_entry():
    data = class_entry_bytes(42)
    reader = class_reader_for_cp(data)
    cp_info, index_extra = reader.read_constant_pool_index(1)
    assert isinstance(cp_info, cp_module.ClassInfo)
    assert cp_info.index == 1
    assert cp_info.tag == 7
    assert cp_info.name_index == 42
    assert index_extra == 0


def test_string_entry():
    data = string_entry_bytes(99)
    reader = class_reader_for_cp(data)
    cp_info, index_extra = reader.read_constant_pool_index(1)
    assert isinstance(cp_info, cp_module.StringInfo)
    assert cp_info.tag == 8
    assert cp_info.string_index == 99
    assert index_extra == 0


def test_fieldref_entry():
    data = fieldref_entry_bytes(3, 7)
    reader = class_reader_for_cp(data)
    cp_info, index_extra = reader.read_constant_pool_index(1)
    assert isinstance(cp_info, cp_module.FieldrefInfo)
    assert cp_info.tag == 9
    assert cp_info.class_index == 3
    assert cp_info.name_and_type_index == 7
    assert index_extra == 0


def test_methodref_entry():
    data = methodref_entry_bytes(5, 12)
    reader = class_reader_for_cp(data)
    cp_info, index_extra = reader.read_constant_pool_index(1)
    assert isinstance(cp_info, cp_module.MethodrefInfo)
    assert cp_info.tag == 10
    assert cp_info.class_index == 5
    assert cp_info.name_and_type_index == 12
    assert index_extra == 0


def test_interface_methodref_entry():
    data = interface_methodref_entry_bytes(6, 14)
    reader = class_reader_for_cp(data)
    cp_info, index_extra = reader.read_constant_pool_index(1)
    assert isinstance(cp_info, cp_module.InterfaceMethodrefInfo)
    assert cp_info.tag == 11
    assert cp_info.class_index == 6
    assert cp_info.name_and_type_index == 14
    assert index_extra == 0


def test_name_and_type_entry():
    data = name_and_type_entry_bytes(8, 9)
    reader = class_reader_for_cp(data)
    cp_info, index_extra = reader.read_constant_pool_index(1)
    assert isinstance(cp_info, cp_module.NameAndTypeInfo)
    assert cp_info.tag == 12
    assert cp_info.name_index == 8
    assert cp_info.descriptor_index == 9
    assert index_extra == 0


# ---------------------------------------------------------------------------
# Method handle / type entries (tags 15, 16)
# ---------------------------------------------------------------------------


def test_method_handle_entry():
    data = method_handle_entry_bytes(6, 25)
    reader = class_reader_for_cp(data)
    cp_info, index_extra = reader.read_constant_pool_index(1)
    assert isinstance(cp_info, cp_module.MethodHandleInfo)
    assert cp_info.tag == 15
    assert cp_info.reference_kind == 6
    assert cp_info.reference_index == 25
    assert index_extra == 0


def test_method_type_entry():
    data = method_type_entry_bytes(33)
    reader = class_reader_for_cp(data)
    cp_info, index_extra = reader.read_constant_pool_index(1)
    assert isinstance(cp_info, cp_module.MethodTypeInfo)
    assert cp_info.tag == 16
    assert cp_info.descriptor_index == 33
    assert index_extra == 0


# ---------------------------------------------------------------------------
# Dynamic / InvokeDynamic entries (tags 17, 18)
# ---------------------------------------------------------------------------


def test_dynamic_entry():
    data = dynamic_entry_bytes(0, 10)
    reader = class_reader_for_cp(data)
    cp_info, index_extra = reader.read_constant_pool_index(1)
    assert isinstance(cp_info, cp_module.DynamicInfo)
    assert cp_info.tag == 17
    assert cp_info.bootstrap_method_attr_index == 0
    assert cp_info.name_and_type_index == 10
    assert index_extra == 0


def test_invoke_dynamic_entry():
    data = invoke_dynamic_entry_bytes(2, 15)
    reader = class_reader_for_cp(data)
    cp_info, index_extra = reader.read_constant_pool_index(1)
    assert isinstance(cp_info, cp_module.InvokeDynamicInfo)
    assert cp_info.tag == 18
    assert cp_info.bootstrap_method_attr_index == 2
    assert cp_info.name_and_type_index == 15
    assert index_extra == 0


# ---------------------------------------------------------------------------
# Module / Package entries (tags 19, 20)
# ---------------------------------------------------------------------------


def test_module_entry():
    data = module_entry_bytes(77)
    reader = class_reader_for_cp(data)
    cp_info, index_extra = reader.read_constant_pool_index(1)
    assert isinstance(cp_info, cp_module.ModuleInfo)
    assert cp_info.tag == 19
    assert cp_info.name_index == 77
    assert index_extra == 0


def test_package_entry():
    data = package_entry_bytes(88)
    reader = class_reader_for_cp(data)
    cp_info, index_extra = reader.read_constant_pool_index(1)
    assert isinstance(cp_info, cp_module.PackageInfo)
    assert cp_info.tag == 20
    assert cp_info.name_index == 88
    assert index_extra == 0


# ---------------------------------------------------------------------------
# index_extra / double-slot handling
# ---------------------------------------------------------------------------


def test_long_index_extra_is_one():
    data = long_entry_bytes(0, 1)
    reader = class_reader_for_cp(data)
    _, index_extra = reader.read_constant_pool_index(1)
    assert index_extra == 1


def test_double_index_extra_is_one():
    data = double_entry_bytes(0, 1)
    reader = class_reader_for_cp(data)
    _, index_extra = reader.read_constant_pool_index(1)
    assert index_extra == 1


def test_other_entries_index_extra_is_zero():
    entries = [
        utf8_entry_bytes("x"),
        integer_entry_bytes(0),
        float_entry_bytes(0),
        class_entry_bytes(1),
        string_entry_bytes(1),
        fieldref_entry_bytes(1, 2),
        methodref_entry_bytes(1, 2),
        interface_methodref_entry_bytes(1, 2),
        name_and_type_entry_bytes(1, 2),
        method_handle_entry_bytes(1, 1),
        method_type_entry_bytes(1),
        dynamic_entry_bytes(0, 1),
        invoke_dynamic_entry_bytes(0, 1),
        module_entry_bytes(1),
        package_entry_bytes(1),
    ]
    for data in entries:
        reader = class_reader_for_cp(data)
        _, index_extra = reader.read_constant_pool_index(1)
        assert index_extra == 0, f"Expected index_extra==0 for tag {data[0]}"


# ---------------------------------------------------------------------------
# Full classfile CP parsing — double-slot integration
# ---------------------------------------------------------------------------


def test_double_slot_skips_next_index():
    # Long at index 5 occupies slots 5 and 6; Utf8 lands at index 7.
    extra_cp_bytes = long_entry_bytes(0xABCD, 0xEF01) + utf8_entry_bytes("after")
    data = minimal_classfile(extra_cp_bytes=extra_cp_bytes, extra_cp_count=3)
    reader = ClassReader.from_bytes(data)
    cp = reader.class_info.constant_pool

    assert isinstance(cp[5], cp_module.LongInfo)
    assert cp[5].high_bytes == 0xABCD
    assert cp[5].low_bytes == 0xEF01
    assert cp[6] is None
    assert isinstance(cp[7], cp_module.Utf8Info)
    assert cp[7].str_bytes == b"after"


def test_mixed_pool_all_simple_types():
    # Indices 5–9: Integer, Float, String, Class, Utf8
    extra_cp_bytes = (
        integer_entry_bytes(100)  # index 5
        + float_entry_bytes(0x3F800000)  # index 6  (1.0f bits)
        + string_entry_bytes(9)  # index 7
        + class_entry_bytes(9)  # index 8
        + utf8_entry_bytes("hello")  # index 9
    )
    data = minimal_classfile(extra_cp_bytes=extra_cp_bytes, extra_cp_count=5)
    reader = ClassReader.from_bytes(data)
    cp = reader.class_info.constant_pool

    assert isinstance(cp[5], cp_module.IntegerInfo)
    assert cp[5].value_bytes == 100

    assert isinstance(cp[6], cp_module.FloatInfo)
    assert cp[6].value_bytes == 0x3F800000

    assert isinstance(cp[7], cp_module.StringInfo)
    assert cp[7].string_index == 9

    assert isinstance(cp[8], cp_module.ClassInfo)
    assert cp[8].name_index == 9

    assert isinstance(cp[9], cp_module.Utf8Info)
    assert cp[9].str_bytes == b"hello"


def test_double_at_end_of_pool():
    # Double as the last logical entry — occupies two slots, no overflow.
    extra_cp_bytes = double_entry_bytes(0x40091EB8, 0x51EB851F)  # ≈ 3.14
    data = minimal_classfile(extra_cp_bytes=extra_cp_bytes, extra_cp_count=2)
    reader = ClassReader.from_bytes(data)
    cp = reader.class_info.constant_pool

    assert isinstance(cp[5], cp_module.DoubleInfo)
    assert cp[5].high_bytes == 0x40091EB8
    assert cp[5].low_bytes == 0x51EB851F
    assert cp[6] is None


# ---------------------------------------------------------------------------
# Error cases
# ---------------------------------------------------------------------------


def test_unknown_tag_raises():
    from tests.helpers import u1

    data = u1(255)
    reader = class_reader_for_cp(data)
    with pytest.raises(ValueError):
        reader.read_constant_pool_index(1)
