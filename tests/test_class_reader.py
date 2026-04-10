from __future__ import annotations

import struct
from pathlib import Path

import pytest

import pytecode.classfile.attributes as attributes
import pytecode.classfile.constant_pool as cp_module
import pytecode.classfile.constants as constants
import pytecode.classfile.info as info
import pytecode.classfile.instructions as instructions
from pytecode.classfile.modified_utf8 import decode_modified_utf8
from pytecode.classfile.reader import ClassReader, MalformedClassException
from tests.helpers import (
    class_entry_bytes,
    compile_java_resource,
    double_entry_bytes,
    long_entry_bytes,
    minimal_classfile,
    u1,
    u2,
    u4,
    utf8_entry_bytes,
)


def _method_by_name(cf: info.ClassFile, name: str) -> info.MethodInfo:
    for method in cf.methods:
        entry = cf.constant_pool[method.name_index]
        assert isinstance(entry, cp_module.Utf8Info)
        if decode_modified_utf8(entry.str_bytes) == name:
            return method
    raise AssertionError(f"Method {name!r} not found")


@pytest.fixture
def hello_world_class_file(tmp_path: Path) -> Path:
    return compile_java_resource(tmp_path, "HelloWorld.java")


# ---------------------------------------------------------------------------
# Basic valid classfile structure
# ---------------------------------------------------------------------------


def test_minimal_classfile():
    data = minimal_classfile()
    reader = ClassReader.from_bytes(data)
    cf = reader.class_info
    assert cf.magic == 0xCAFEBABE
    assert cf.major_version == 52
    assert cf.minor_version == 0
    assert cf.this_class == 2
    assert cf.super_class == 4
    assert cf.interfaces_count == 0
    assert cf.fields_count == 0
    assert cf.methods_count == 0
    assert cf.attributes_count == 0


def test_magic_number():
    cf = ClassReader.from_bytes(minimal_classfile()).class_info
    assert cf.magic == 0xCAFEBABE


def test_version_fields():
    cf = ClassReader.from_bytes(minimal_classfile(major=52, minor=0)).class_info
    assert cf.major_version == 52
    assert cf.minor_version == 0


def test_constant_pool_count():
    cf = ClassReader.from_bytes(minimal_classfile()).class_info
    # 4 base entries + 1 for the count value itself
    assert cf.constant_pool_count == 5


def test_constant_pool_index_0_is_none():
    cf = ClassReader.from_bytes(minimal_classfile()).class_info
    assert cf.constant_pool[0] is None


def test_constant_pool_index_1_is_utf8():
    cf = ClassReader.from_bytes(minimal_classfile()).class_info
    entry = cf.constant_pool[1]
    assert isinstance(entry, cp_module.Utf8Info)
    assert entry.str_bytes == b"TestClass"


def test_constant_pool_index_2_is_class():
    cf = ClassReader.from_bytes(minimal_classfile()).class_info
    entry = cf.constant_pool[2]
    assert isinstance(entry, cp_module.ClassInfo)
    assert entry.name_index == 1


def test_access_flags():
    cf = ClassReader.from_bytes(minimal_classfile()).class_info
    assert cf.access_flags == constants.ClassAccessFlag(0x0021)


def test_classfile_is_ClassFile_instance():
    cf = ClassReader.from_bytes(minimal_classfile()).class_info
    assert isinstance(cf, info.ClassFile)


# ---------------------------------------------------------------------------
# from_file and from_bytes
# ---------------------------------------------------------------------------


def test_from_bytes():
    data = minimal_classfile()
    reader = ClassReader.from_bytes(data)
    assert hasattr(reader, "class_info")
    assert reader.class_info is not None


def test_from_file(hello_world_class_file: Path):
    reader = ClassReader.from_file(hello_world_class_file)
    cf = reader.class_info
    assert cf.magic == 0xCAFEBABE
    assert cf is not None


# ---------------------------------------------------------------------------
# Interfaces
# ---------------------------------------------------------------------------


def test_classfile_with_interfaces():
    # Base CP: indices 1-4. Extras: utf8("MyInterface") at 5, Class(5) at 6.
    extra_cp = utf8_entry_bytes("MyInterface") + class_entry_bytes(5)
    data = minimal_classfile(extra_cp_bytes=extra_cp, extra_cp_count=2, interfaces=[6])
    cf = ClassReader.from_bytes(data).class_info
    assert cf.interfaces_count == 1
    assert cf.interfaces == [6]


# ---------------------------------------------------------------------------
# Fields
# ---------------------------------------------------------------------------


def test_classfile_with_field():
    # Extra CP: utf8("myField") at 5, utf8("I") at 6.
    extra_cp = utf8_entry_bytes("myField") + utf8_entry_bytes("I")
    field_bytes = u2(0x0001) + u2(5) + u2(6) + u2(0)
    data = minimal_classfile(
        extra_cp_bytes=extra_cp,
        extra_cp_count=2,
        fields_count=1,
        fields_bytes=field_bytes,
    )
    cf = ClassReader.from_bytes(data).class_info
    assert cf.fields_count == 1
    assert isinstance(cf.fields[0], info.FieldInfo)


# ---------------------------------------------------------------------------
# Methods
# ---------------------------------------------------------------------------


def test_classfile_with_method():
    # Extra CP: utf8("myMethod") at 5, utf8("()V") at 6.
    extra_cp = utf8_entry_bytes("myMethod") + utf8_entry_bytes("()V")
    method_bytes = u2(0x0001) + u2(5) + u2(6) + u2(0)
    data = minimal_classfile(
        extra_cp_bytes=extra_cp,
        extra_cp_count=2,
        methods_count=1,
        methods_bytes=method_bytes,
    )
    cf = ClassReader.from_bytes(data).class_info
    assert cf.methods_count == 1
    assert isinstance(cf.methods[0], info.MethodInfo)


# ---------------------------------------------------------------------------
# Method Code attributes
# ---------------------------------------------------------------------------


def test_helloworld_main_method_code_attr(hello_world_class_file: Path):
    cf = ClassReader.from_file(hello_world_class_file).class_info
    method = _method_by_name(cf, "main")

    assert method.attributes_count == 1
    code_attr = method.attributes[0]
    assert isinstance(code_attr, attributes.CodeAttr)
    assert code_attr.code_length == 9
    assert [insn.type for insn in code_attr.code] == [
        instructions.InsnInfoType.GETSTATIC,
        instructions.InsnInfoType.LDC,
        instructions.InsnInfoType.INVOKEVIRTUAL,
        instructions.InsnInfoType.RETURN,
    ]
    assert [insn.bytecode_offset for insn in code_attr.code] == [0, 3, 5, 8]
    assert code_attr.exception_table_length == 0
    assert code_attr.attributes_count == 1

    line_numbers = code_attr.attributes[0]
    assert isinstance(line_numbers, attributes.LineNumberTableAttr)
    assert line_numbers.line_number_table_length >= 1
    assert line_numbers.line_number_table[0].start_pc == 0
    assert line_numbers.line_number_table[0].line_number > 0


# ---------------------------------------------------------------------------
# Class-level attributes
# ---------------------------------------------------------------------------


def test_classfile_with_source_file_attr():
    # Extra CP: utf8("SourceFile") at 5, utf8("Test.java") at 6.
    extra_cp = utf8_entry_bytes("SourceFile") + utf8_entry_bytes("Test.java")
    # Attribute payload: u2(sourcefile_index) where sourcefile_index=6.
    attr_bytes = u2(5) + u4(2) + u2(6)
    data = minimal_classfile(
        extra_cp_bytes=extra_cp,
        extra_cp_count=2,
        class_attrs_count=1,
        class_attrs_bytes=attr_bytes,
    )
    cf = ClassReader.from_bytes(data).class_info
    assert cf.attributes_count == 1
    assert isinstance(cf.attributes[0], attributes.SourceFileAttr)


# ---------------------------------------------------------------------------
# Version validation
# ---------------------------------------------------------------------------


def test_valid_version_major_55():
    data = minimal_classfile(major=55, minor=0)
    cf = ClassReader.from_bytes(data).class_info
    assert cf.major_version == 55


def test_valid_version_major_56_minor_0():
    data = minimal_classfile(major=56, minor=0)
    cf = ClassReader.from_bytes(data).class_info
    assert cf.major_version == 56
    assert cf.minor_version == 0


def test_valid_version_major_56_minor_65535():
    data = minimal_classfile(major=56, minor=65535)
    cf = ClassReader.from_bytes(data).class_info
    assert cf.major_version == 56
    assert cf.minor_version == 65535


def test_invalid_version_major_56_minor_1():
    data = minimal_classfile(major=56, minor=1)
    with pytest.raises(MalformedClassException):
        ClassReader.from_bytes(data)


def test_invalid_version_major_60_minor_2():
    data = minimal_classfile(major=60, minor=2)
    with pytest.raises(MalformedClassException):
        ClassReader.from_bytes(data)


# ---------------------------------------------------------------------------
# Error paths
# ---------------------------------------------------------------------------


def test_invalid_magic():
    data = b"\xde\xad\xbe\xef" + minimal_classfile()[4:]
    with pytest.raises(MalformedClassException):
        ClassReader.from_bytes(data)


def test_invalid_magic_message():
    data = b"\xde\xad\xbe\xef" + minimal_classfile()[4:]
    with pytest.raises(MalformedClassException, match="deadbeef"):
        ClassReader.from_bytes(data)


def test_unknown_cp_tag():
    # Inject an extra CP entry with tag=255 (unknown).
    bad_tag = u1(255)
    data = minimal_classfile(extra_cp_bytes=bad_tag, extra_cp_count=1)
    with pytest.raises((ValueError, MalformedClassException)):
        ClassReader.from_bytes(data)


def test_truncated_classfile():
    with pytest.raises((struct.error, MalformedClassException)):
        ClassReader.from_bytes(b"\xca\xfe\xba\xbe")


# ---------------------------------------------------------------------------
# CP double-slot entries
# ---------------------------------------------------------------------------


def test_long_in_constant_pool():
    extra_cp = long_entry_bytes(0xDEAD, 0xBEEF)
    data = minimal_classfile(extra_cp_bytes=extra_cp, extra_cp_count=2)
    cf = ClassReader.from_bytes(data).class_info
    assert isinstance(cf.constant_pool[5], cp_module.LongInfo)
    assert cf.constant_pool[6] is None
    assert cf.constant_pool[5].high_bytes == 0xDEAD
    assert cf.constant_pool[5].low_bytes == 0xBEEF


def test_double_in_constant_pool():
    extra_cp = double_entry_bytes(0x4000, 0x0000)
    data = minimal_classfile(extra_cp_bytes=extra_cp, extra_cp_count=2)
    cf = ClassReader.from_bytes(data).class_info
    assert isinstance(cf.constant_pool[5], cp_module.DoubleInfo)
    assert cf.constant_pool[6] is None


def test_entry_after_long():
    # Long at 5 (takes slots 5+6), then Utf8("after") at 7.
    extra_cp = long_entry_bytes(0, 0) + utf8_entry_bytes("after")
    data = minimal_classfile(extra_cp_bytes=extra_cp, extra_cp_count=3)
    cf = ClassReader.from_bytes(data).class_info
    assert isinstance(cf.constant_pool[7], cp_module.Utf8Info)
    assert cf.constant_pool[7].str_bytes == b"after"


# ---------------------------------------------------------------------------
# Truncation and corruption edge cases
# ---------------------------------------------------------------------------


def test_truncated_constant_pool_utf8():
    """Truncate a UTF-8 CP entry mid-way through its declared length."""
    # Build header + CP that declares a UTF-8 entry with 10 bytes but
    # the file ends after only 5 bytes of string data.
    magic = u4(0xCAFEBABE)
    version = u2(0) + u2(52)
    # CP has 5 real entries (indices 1-4) plus one truncated UTF-8 at index 5
    cp_count = u2(6)
    cp_entries = (
        utf8_entry_bytes("TestClass")
        + class_entry_bytes(1)
        + utf8_entry_bytes("java/lang/Object")
        + class_entry_bytes(3)
    )
    tag_and_length = u1(1) + u2(10)  # tag=1, length=10
    partial_string = b"hello"  # only 5 bytes instead of 10
    truncated = magic + version + cp_count + cp_entries + tag_and_length + partial_string
    with pytest.raises((struct.error, MalformedClassException)):
        ClassReader.from_bytes(truncated)


def test_truncated_constant_pool_fieldref():
    """Truncate in the middle of a Fieldref CP entry (needs 4 bytes, give 2)."""
    # Build header + CP that declares a Fieldref but the file ends after
    # only the class_index (2 bytes), missing the name_and_type_index.
    magic = u4(0xCAFEBABE)
    version = u2(0) + u2(52)
    cp_count = u2(6)
    cp_entries = (
        utf8_entry_bytes("TestClass")
        + class_entry_bytes(1)
        + utf8_entry_bytes("java/lang/Object")
        + class_entry_bytes(3)
    )
    tag = u1(9)  # Fieldref tag
    partial = u2(2)  # class_index only, missing name_and_type_index
    truncated = magic + version + cp_count + cp_entries + tag + partial
    with pytest.raises((struct.error, MalformedClassException)):
        ClassReader.from_bytes(truncated)


def test_truncated_before_access_flags():
    """File ends right after constant pool — no access_flags, this_class, etc."""
    # Build just magic + version + a valid minimal CP, but nothing after.
    magic = u4(0xCAFEBABE)
    version = u2(0) + u2(52)
    cp_count = u2(5)
    cp_entries = (
        utf8_entry_bytes("TestClass")
        + class_entry_bytes(1)
        + utf8_entry_bytes("java/lang/Object")
        + class_entry_bytes(3)
    )
    truncated = magic + version + cp_count + cp_entries
    with pytest.raises((struct.error, MalformedClassException)):
        ClassReader.from_bytes(truncated)


def test_truncated_method_attributes():
    """Method declares 1 attribute but the attribute data is missing."""
    extra_cp = utf8_entry_bytes("myMethod") + utf8_entry_bytes("()V")
    # Method header: access=PUBLIC, name=5, desc=6, attrs_count=1
    # But no attribute bytes follow.
    method_bytes = u2(0x0001) + u2(5) + u2(6) + u2(1)
    data = minimal_classfile(
        extra_cp_bytes=extra_cp,
        extra_cp_count=2,
        methods_count=1,
        methods_bytes=method_bytes,
    )
    with pytest.raises((struct.error, MalformedClassException)):
        ClassReader.from_bytes(data)


def test_truncated_code_attribute_body():
    """Code attribute declares code_length=10 but provides fewer bytes."""
    extra_cp = utf8_entry_bytes("foo") + utf8_entry_bytes("()V") + utf8_entry_bytes("Code")
    # Code attribute: name_index=7 (the "Code" utf8), length=large, max_stack, max_locals, code_length=10
    code_attr_name = u2(7)
    code_attr_len = u4(20)  # declared attr length
    max_stack = u2(1)
    max_locals = u2(1)
    code_length = u4(10)
    code_bytes = b"\xb1" * 3  # only 3 bytes instead of 10
    code_attr = code_attr_name + code_attr_len + max_stack + max_locals + code_length + code_bytes
    method_bytes = u2(0x0001) + u2(5) + u2(6) + u2(1) + code_attr
    data = minimal_classfile(
        extra_cp_bytes=extra_cp,
        extra_cp_count=3,
        methods_count=1,
        methods_bytes=method_bytes,
    )
    with pytest.raises((struct.error, MalformedClassException)):
        ClassReader.from_bytes(data)


def test_empty_file():
    """A zero-length file should fail immediately."""
    with pytest.raises((struct.error, MalformedClassException)):
        ClassReader.from_bytes(b"")


def test_only_magic():
    """File has magic number but nothing else."""
    with pytest.raises((struct.error, MalformedClassException)):
        ClassReader.from_bytes(b"\xca\xfe\xba\xbe")
