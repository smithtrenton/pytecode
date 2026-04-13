"""Shared helper utilities for the surviving Rust-first test suite."""

from __future__ import annotations

import os
import subprocess
import zipfile
from pathlib import Path

TEST_RESOURCES = Path(__file__).resolve().parent / "resources"


def _jdk_tool(name: str) -> str:
    """Return the path to a JDK tool, preferring ``JAVA_HOME`` when present."""

    java_home = os.environ.get("JAVA_HOME")
    if java_home:
        bin_dir = Path(java_home) / "bin"
        for suffix in ("", ".exe"):
            candidate = bin_dir / f"{name}{suffix}"
            if candidate.exists():
                return str(candidate)
    return name


def compile_java_sources(
    tmp_path: Path,
    source_files: list[Path],
    *,
    release: int = 8,
    classpath: list[Path] | None = None,
) -> Path:
    """Compile Java source files into ``tmp_path\\classes`` and return that directory."""

    classes_dir = tmp_path / "classes"
    classes_dir.mkdir(parents=True, exist_ok=True)

    command = [_jdk_tool("javac"), "--release", str(release), "-d", str(classes_dir)]
    if classpath:
        command.extend(["-cp", os.pathsep.join(str(path) for path in classpath)])
    command.extend(str(path) for path in source_files)

    result = subprocess.run(command, capture_output=True, text=True, check=False)
    if result.returncode != 0:
        raise AssertionError(result.stderr or result.stdout or "javac failed")
    return classes_dir


def compile_java_resource(tmp_path: Path, resource_name: str, *, release: int = 8) -> Path:
    """Compile one Java fixture and return the primary emitted ``.class`` path."""

    source_path = TEST_RESOURCES / resource_name
    classes_dir = compile_java_sources(tmp_path, [source_path], release=release)
    return classes_dir / Path(resource_name).with_suffix(".class").name


def compile_java_resource_classes(tmp_path: Path, resource_name: str, *, release: int = 8) -> list[Path]:
    """Compile one Java fixture and return every emitted ``.class`` file."""

    source_path = TEST_RESOURCES / resource_name
    classes_dir = compile_java_sources(tmp_path, [source_path], release=release)
    class_files = sorted(classes_dir.rglob("*.class"), key=lambda path: path.relative_to(classes_dir).as_posix())
    if not class_files:
        raise AssertionError(f"Java resource {resource_name!r} produced no .class files")
    return class_files


def make_compiled_jar(
    tmp_path: Path,
    source_files: list[Path],
    *,
    extra_files: dict[str, bytes] | None = None,
    jar_name: str = "fixture.jar",
    release: int = 8,
) -> Path:
    """Compile Java sources and package them into a small JAR."""

    classes_dir = compile_java_sources(tmp_path, source_files, release=release)
    jar_path = tmp_path / jar_name
    with zipfile.ZipFile(jar_path, "w") as archive:
        for class_file in classes_dir.rglob("*.class"):
            archive.write(class_file, class_file.relative_to(classes_dir).as_posix())
        for filename, data in (extra_files or {}).items():
            archive.writestr(filename, data)
    return jar_path


def u2(value: int) -> bytes:
    """Encode an unsigned JVM ``u2`` value."""

    return value.to_bytes(2, "big")


def utf8_entry_bytes(value: str) -> bytes:
    """Encode a constant-pool ``Utf8`` entry."""

    encoded = value.encode("utf-8")
    return b"\x01" + u2(len(encoded)) + encoded


def class_entry_bytes(name_index: int) -> bytes:
    """Encode a constant-pool ``Class`` entry."""

    return b"\x07" + u2(name_index)


_MAGIC = b"\xca\xfe\xba\xbe"
_BASE_CP = (
    utf8_entry_bytes("TestClass") + class_entry_bytes(1) + utf8_entry_bytes("java/lang/Object") + class_entry_bytes(3)
)
_BASE_CP_COUNT = 5


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
    """Build a minimal but valid classfile."""

    interface_indexes = interfaces or []
    interface_bytes = b"".join(u2(index) for index in interface_indexes)
    return (
        _MAGIC
        + u2(minor)
        + u2(major)
        + u2(_BASE_CP_COUNT + extra_cp_count)
        + _BASE_CP
        + extra_cp_bytes
        + u2(access_flags)
        + u2(this_class)
        + u2(super_class)
        + u2(len(interface_indexes))
        + interface_bytes
        + u2(fields_count)
        + fields_bytes
        + u2(methods_count)
        + methods_bytes
        + u2(class_attrs_count)
        + class_attrs_bytes
    )
