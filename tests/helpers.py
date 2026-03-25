"""Shared byte-building utilities for pytecode unit tests."""
from __future__ import annotations

import struct
import subprocess
import zipfile
from pathlib import Path

from pytecode import constant_pool as cp_module
from pytecode.bytes_utils import BytesReader
from pytecode.class_reader import ClassReader

TEST_RESOURCES = Path(__file__).resolve().parent / "resources"


def compile_java_sources(tmp_path: Path, source_files: list[Path], *, release: int = 8) -> Path:
    """Compile Java source files into `tmp_path\\classes` and return that directory."""
    classes_dir = tmp_path / "classes"
    classes_dir.mkdir(parents=True, exist_ok=True)

    result = subprocess.run(
        ["javac", "--release", str(release), "-d", str(classes_dir), *(str(path) for path in source_files)],
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        raise AssertionError(result.stderr or result.stdout)

    return classes_dir


def compile_java_resource(tmp_path: Path, resource_name: str, *, release: int = 8) -> Path:
    """Compile one Java resource file and return the generated class path."""
    source_path = TEST_RESOURCES / resource_name
    classes_dir = compile_java_sources(tmp_path, [source_path], release=release)
    return classes_dir / Path(resource_name).with_suffix(".class").name


def make_compiled_jar(
    tmp_path: Path,
    source_files: list[Path],
    *,
    extra_files: dict[str, bytes] | None = None,
    jar_name: str = "fixture.jar",
    release: int = 8,
) -> Path:
    """Compile Java sources and package the resulting classes plus extras into a small JAR."""
    classes_dir = compile_java_sources(tmp_path, source_files, release=release)
    jar_path = tmp_path / jar_name

    with zipfile.ZipFile(jar_path, "w") as zf:
        for class_file in classes_dir.rglob("*.class"):
            zf.write(class_file, class_file.relative_to(classes_dir).as_posix())
        for filename, data in (extra_files or {}).items():
            zf.writestr(filename, data)

    return jar_path

# ---------------------------------------------------------------------------
# Big-endian byte packers
# ---------------------------------------------------------------------------


def u1(v: int) -> bytes:
    return struct.pack(">B", v)


def i1(v: int) -> bytes:
    return struct.pack(">b", v)


def u2(v: int) -> bytes:
    return struct.pack(">H", v)


def i2(v: int) -> bytes:
    return struct.pack(">h", v)


def u4(v: int) -> bytes:
    return struct.pack(">I", v)


def i4(v: int) -> bytes:
    return struct.pack(">i", v)


# ---------------------------------------------------------------------------
# Raw constant pool entry byte builders
# ---------------------------------------------------------------------------


def utf8_entry_bytes(s: str) -> bytes:
    """Tag 1 — UTF8 entry: tag + u2(length) + raw bytes."""
    encoded = s.encode("utf-8")
    return u1(1) + u2(len(encoded)) + encoded


def integer_entry_bytes(value: int) -> bytes:
    """Tag 3 — Integer: tag + u4(value)."""
    return u1(3) + u4(value)


def float_entry_bytes(raw: int) -> bytes:
    """Tag 4 — Float: tag + u4(raw bits)."""
    return u1(4) + u4(raw)


def long_entry_bytes(high: int, low: int) -> bytes:
    """Tag 5 — Long (double-slot): tag + u4(high) + u4(low)."""
    return u1(5) + u4(high) + u4(low)


def double_entry_bytes(high: int, low: int) -> bytes:
    """Tag 6 — Double (double-slot): tag + u4(high) + u4(low)."""
    return u1(6) + u4(high) + u4(low)


def class_entry_bytes(name_index: int) -> bytes:
    """Tag 7 — Class: tag + u2(name_index)."""
    return u1(7) + u2(name_index)


def string_entry_bytes(string_index: int) -> bytes:
    """Tag 8 — String: tag + u2(string_index)."""
    return u1(8) + u2(string_index)


def fieldref_entry_bytes(class_index: int, nat_index: int) -> bytes:
    """Tag 9 — Fieldref: tag + u2(class_index) + u2(name_and_type_index)."""
    return u1(9) + u2(class_index) + u2(nat_index)


def methodref_entry_bytes(class_index: int, nat_index: int) -> bytes:
    """Tag 10 — Methodref: tag + u2(class_index) + u2(name_and_type_index)."""
    return u1(10) + u2(class_index) + u2(nat_index)


def interface_methodref_entry_bytes(class_index: int, nat_index: int) -> bytes:
    """Tag 11 — InterfaceMethodref."""
    return u1(11) + u2(class_index) + u2(nat_index)


def name_and_type_entry_bytes(name_index: int, descriptor_index: int) -> bytes:
    """Tag 12 — NameAndType."""
    return u1(12) + u2(name_index) + u2(descriptor_index)


def method_handle_entry_bytes(ref_kind: int, ref_index: int) -> bytes:
    """Tag 15 — MethodHandle: tag + u1(ref_kind) + u2(ref_index)."""
    return u1(15) + u1(ref_kind) + u2(ref_index)


def method_type_entry_bytes(descriptor_index: int) -> bytes:
    """Tag 16 — MethodType: tag + u2(descriptor_index)."""
    return u1(16) + u2(descriptor_index)


def dynamic_entry_bytes(bootstrap_index: int, nat_index: int) -> bytes:
    """Tag 17 — Dynamic: tag + u2(bootstrap_method_attr_index) + u2(name_and_type_index)."""
    return u1(17) + u2(bootstrap_index) + u2(nat_index)


def invoke_dynamic_entry_bytes(bootstrap_index: int, nat_index: int) -> bytes:
    """Tag 18 — InvokeDynamic: tag + u2(bootstrap_method_attr_index) + u2(name_and_type_index)."""
    return u1(18) + u2(bootstrap_index) + u2(nat_index)


def module_entry_bytes(name_index: int) -> bytes:
    """Tag 19 — Module: tag + u2(name_index)."""
    return u1(19) + u2(name_index)


def package_entry_bytes(name_index: int) -> bytes:
    """Tag 20 — Package: tag + u2(name_index)."""
    return u1(20) + u2(name_index)


# ---------------------------------------------------------------------------
# In-memory constant pool object builders (for use with class_reader_with_cp)
# ---------------------------------------------------------------------------


def make_utf8_info(index: int, s: str) -> cp_module.Utf8Info:
    """Build a Utf8Info dataclass instance for use in cp_list."""
    encoded = s.encode("utf-8")
    return cp_module.Utf8Info(index, 0, 1, len(encoded), encoded)


# ---------------------------------------------------------------------------
# Attribute blob builder
# ---------------------------------------------------------------------------


def make_attribute_blob(name_index: int, payload: bytes) -> bytes:
    """Wrap raw attribute payload with u2(name_index) + u4(length) header."""
    return u2(name_index) + u4(len(payload)) + payload


# ---------------------------------------------------------------------------
# ClassReader factories for isolated unit testing
# ---------------------------------------------------------------------------


def class_reader_with_cp(data: bytes, cp_list: list) -> ClassReader:
    """
    Return a ClassReader-like object whose buffer is `data` and whose
    constant_pool is `cp_list`.  Bypasses read_class() so only the
    specific method under test (e.g. read_attribute) is exercised.
    """
    reader = ClassReader.__new__(ClassReader)
    BytesReader.__init__(reader, data)
    reader.constant_pool = cp_list
    return reader


def class_reader_for_insns(code_bytes: bytes) -> ClassReader:
    """Return a ClassReader-like object positioned at `code_bytes` for instruction testing."""
    reader = ClassReader.__new__(ClassReader)
    BytesReader.__init__(reader, code_bytes)
    reader.constant_pool = []
    return reader


def class_reader_for_cp(data: bytes) -> ClassReader:
    """Return a ClassReader-like object positioned at `data` for CP entry testing."""
    reader = ClassReader.__new__(ClassReader)
    BytesReader.__init__(reader, data)
    reader.constant_pool = []
    return reader


# ---------------------------------------------------------------------------
# Minimal classfile builder
# ---------------------------------------------------------------------------

_MAGIC = b"\xca\xfe\xba\xbe"

# Default constant pool (indices 1–4):
#   1  Utf8  "TestClass"
#   2  Class name_index=1
#   3  Utf8  "java/lang/Object"
#   4  Class name_index=3
_BASE_CP = (
    utf8_entry_bytes("TestClass")
    + class_entry_bytes(1)
    + utf8_entry_bytes("java/lang/Object")
    + class_entry_bytes(3)
)
_BASE_CP_COUNT = 5  # cp_count field value = number of entries + 1


def minimal_classfile(
    *,
    major: int = 52,
    minor: int = 0,
    extra_cp_bytes: bytes = b"",
    extra_cp_count: int = 0,
    access_flags: int = 0x0021,
    this_class: int = 2,
    super_class: int = 4,
    interfaces: list[int] | None = None,
    fields_count: int = 0,
    fields_bytes: bytes = b"",
    methods_count: int = 0,
    methods_bytes: bytes = b"",
    class_attrs_count: int = 0,
    class_attrs_bytes: bytes = b"",
) -> bytes:
    """
    Build a minimal but valid classfile.

    The constant pool always starts with the four base entries (indices 1-4).
    Pass extra_cp_bytes / extra_cp_count to append additional entries starting
    at index 5.  extra_cp_count must account for double-slot entries (Long/Double
    each count as 2).
    """
    cp_count = _BASE_CP_COUNT + extra_cp_count
    ifaces = interfaces or []
    ifaces_bytes = b"".join(u2(i) for i in ifaces)

    return (
        _MAGIC
        + u2(minor)
        + u2(major)
        + u2(cp_count)
        + _BASE_CP
        + extra_cp_bytes
        + u2(access_flags)
        + u2(this_class)
        + u2(super_class)
        + u2(len(ifaces))
        + ifaces_bytes
        + u2(fields_count)
        + fields_bytes
        + u2(methods_count)
        + methods_bytes
        + u2(class_attrs_count)
        + class_attrs_bytes
    )


# ---------------------------------------------------------------------------
# Helper to build a cp_list + attribute blob for read_attribute() tests.
# ---------------------------------------------------------------------------


def attr_reader(attr_name: str, payload: bytes) -> ClassReader:
    """
    Return a ClassReader positioned at a complete attribute blob (name_index
    header + length + payload) whose constant pool has a Utf8Info for
    `attr_name` at index 1.

    Usage::

        reader = attr_reader("ConstantValue", u2(42))
        attr = reader.read_attribute()
        assert isinstance(attr, ConstantValueAttr)
        assert attr.constantvalue_index == 42
    """
    cp_list = [None, make_utf8_info(1, attr_name)]
    blob = make_attribute_blob(1, payload)
    return class_reader_with_cp(blob, cp_list)
