"""Shared byte-building utilities for pytecode unit tests."""

from __future__ import annotations

import functools
import hashlib
import json
import os
import shutil
import struct
import subprocess
import tempfile
import urllib.error
import urllib.request
import zipfile
from pathlib import Path
from typing import Any, cast

from pytecode import constant_pool as cp_module
from pytecode.bytes_utils import BytesReader
from pytecode.class_reader import ClassReader
from pytecode.modified_utf8 import encode_modified_utf8

TEST_RESOURCES = Path(__file__).resolve().parent / "resources"
_REPO_ROOT = TEST_RESOURCES.parent.parent
JAVA_COMPILE_CACHE_ROOT = _REPO_ROOT / ".pytest_cache" / "pytecode-javac"
_JAVA_COMPILE_CACHE_SCHEMA_VERSION = 1
ORACLE_RESOURCE_ROOT = TEST_RESOURCES / "oracle"
ORACLE_LIB_ROOT = ORACLE_RESOURCE_ROOT / "lib"
ORACLE_CACHE_ROOT = _REPO_ROOT / ".pytest_cache" / "pytecode-oracle"
ASM_VERSION = "9.7.1"
ASM_ARTIFACTS = ("asm", "asm-tree", "asm-analysis", "asm-util")
ASM_DOWNLOAD_ROOT = ORACLE_CACHE_ROOT / "downloads" / ASM_VERSION
ORACLE_SOURCE_PATH = ORACLE_RESOURCE_ROOT / "RecordingAnalyzer.java"


def _remove_path(path: Path) -> None:
    if not path.exists():
        return
    if path.is_dir():
        shutil.rmtree(path)
    else:
        path.unlink()


def _normalize_existing_files(
    paths: list[Path],
    *,
    item_label: str,
    require_non_empty: bool,
) -> tuple[Path, ...]:
    if require_non_empty and not paths:
        raise AssertionError(f"Expected at least one {item_label}")
    normalized: list[Path] = []
    for path in paths:
        try:
            normalized.append(path.resolve(strict=True))
        except FileNotFoundError as exc:
            raise AssertionError(f"{item_label} {path} does not exist") from exc
    return tuple(normalized)


def _normalize_source_files(source_files: list[Path]) -> tuple[Path, ...]:
    return _normalize_existing_files(source_files, item_label="Java source", require_non_empty=True)


def _normalize_classpath_entries(classpath: list[Path] | None) -> tuple[Path, ...]:
    if classpath is None:
        return ()
    return _normalize_existing_files(classpath, item_label="Classpath entry", require_non_empty=False)


@functools.lru_cache(maxsize=1)
def _get_javac_identity() -> str:
    result = subprocess.run(
        ["javac", "-version"],
        capture_output=True,
        text=True,
        check=False,
    )
    version_text = (result.stdout or result.stderr).strip()
    if result.returncode != 0 or not version_text:
        raise AssertionError(version_text or "Failed to determine javac version")
    return version_text


def _path_cache_inputs(paths: tuple[Path, ...]) -> list[dict[str, str]]:
    cache_inputs: list[dict[str, str]] = []
    for source_path in paths:
        try:
            source_id = source_path.relative_to(_REPO_ROOT).as_posix()
        except ValueError:
            source_id = source_path.as_posix()
        cache_inputs.append(
            {
                "path": source_id,
                "sha256": hashlib.sha256(source_path.read_bytes()).hexdigest(),
            }
        )
    return cache_inputs


def _java_compile_cache_key(
    source_inputs: list[dict[str, str]],
    *,
    release: int,
    classpath_inputs: list[dict[str, str]],
) -> str:
    payload = {
        "classpath": classpath_inputs,
        "javac": _get_javac_identity(),
        "release": release,
        "schema_version": _JAVA_COMPILE_CACHE_SCHEMA_VERSION,
        "sources": source_inputs,
    }
    encoded = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def _compile_java_sources_uncached(
    classes_dir: Path,
    source_files: tuple[Path, ...],
    *,
    release: int = 8,
    classpath: tuple[Path, ...] = (),
) -> None:
    classes_dir.mkdir(parents=True, exist_ok=True)
    command = ["javac", "--release", str(release)]
    if classpath:
        command.extend(["-cp", os.pathsep.join(str(path) for path in classpath)])
    command.extend(["-d", str(classes_dir), *(str(path) for path in source_files)])

    result = subprocess.run(
        command,
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        raise AssertionError(result.stderr or result.stdout)


def _cache_entry_manifest_path(entry_dir: Path) -> Path:
    return entry_dir / "manifest.json"


def _cache_entry_classes_dir(entry_dir: Path) -> Path:
    return entry_dir / "classes"


def _is_valid_cache_entry(entry_dir: Path) -> bool:
    return _cache_entry_manifest_path(entry_dir).is_file() and _cache_entry_classes_dir(entry_dir).is_dir()


def _write_cache_manifest(
    entry_dir: Path,
    source_inputs: list[dict[str, str]],
    *,
    release: int,
    classpath_inputs: list[dict[str, str]],
) -> None:
    classes_dir = _cache_entry_classes_dir(entry_dir)
    class_files = sorted(path.relative_to(classes_dir).as_posix() for path in classes_dir.rglob("*.class"))
    manifest = {
        "class_files": class_files,
        "classpath": classpath_inputs,
        "javac": _get_javac_identity(),
        "release": release,
        "schema_version": _JAVA_COMPILE_CACHE_SCHEMA_VERSION,
        "sources": source_inputs,
    }
    manifest_text = json.dumps(manifest, indent=2, sort_keys=True) + "\n"
    _cache_entry_manifest_path(entry_dir).write_text(manifest_text, encoding="utf-8")


def _cached_java_classes_dir(
    source_files: tuple[Path, ...],
    *,
    release: int = 8,
    classpath: tuple[Path, ...] = (),
) -> Path:
    source_inputs = _path_cache_inputs(source_files)
    classpath_inputs = _path_cache_inputs(classpath)
    cache_key = _java_compile_cache_key(source_inputs, release=release, classpath_inputs=classpath_inputs)
    entry_dir = JAVA_COMPILE_CACHE_ROOT / cache_key
    if _is_valid_cache_entry(entry_dir):
        return _cache_entry_classes_dir(entry_dir)

    if entry_dir.exists():
        _remove_path(entry_dir)

    JAVA_COMPILE_CACHE_ROOT.mkdir(parents=True, exist_ok=True)
    staging_root = JAVA_COMPILE_CACHE_ROOT / "staging"
    staging_root.mkdir(parents=True, exist_ok=True)
    staging_dir = Path(tempfile.mkdtemp(prefix=f"{cache_key}-", dir=staging_root))
    try:
        if classpath:
            _compile_java_sources_uncached(
                _cache_entry_classes_dir(staging_dir),
                source_files,
                release=release,
                classpath=classpath,
            )
        else:
            _compile_java_sources_uncached(
                _cache_entry_classes_dir(staging_dir),
                source_files,
                release=release,
            )
        _write_cache_manifest(staging_dir, source_inputs, release=release, classpath_inputs=classpath_inputs)
        try:
            staging_dir.rename(entry_dir)
        except OSError:
            if _is_valid_cache_entry(entry_dir):
                _remove_path(staging_dir)
            else:
                if entry_dir.exists():
                    _remove_path(entry_dir)
                try:
                    shutil.copytree(staging_dir, entry_dir)
                except OSError as copy_exc:
                    raise AssertionError(f"Cache entry {entry_dir} was created incompletely") from copy_exc
                _remove_path(staging_dir)
        if not _is_valid_cache_entry(entry_dir):
            raise AssertionError(f"Failed to publish cached Java compilation {cache_key}")
        return _cache_entry_classes_dir(entry_dir)
    except Exception:
        _remove_path(staging_dir)
        raise


def _materialize_cached_classes(cache_classes_dir: Path, classes_dir: Path) -> None:
    _remove_path(classes_dir)
    classes_dir.parent.mkdir(parents=True, exist_ok=True)
    shutil.copytree(cache_classes_dir, classes_dir)


def compile_java_sources(
    tmp_path: Path,
    source_files: list[Path],
    *,
    release: int = 8,
    classpath: list[Path] | None = None,
) -> Path:
    """Compile Java source files into `tmp_path\\classes` and return that directory."""
    classes_dir = tmp_path / "classes"
    cache_classes_dir = _cached_java_classes_dir(
        _normalize_source_files(source_files),
        release=release,
        classpath=_normalize_classpath_entries(classpath),
    )
    _materialize_cached_classes(cache_classes_dir, classes_dir)
    return classes_dir


def compile_java_resource(tmp_path: Path, resource_name: str, *, release: int = 8) -> Path:
    """Compile one Java resource file and return the generated class path."""
    source_path = TEST_RESOURCES / Path(resource_name)
    classes_dir = compile_java_sources(tmp_path, [source_path], release=release)
    return classes_dir / Path(resource_name).with_suffix(".class").name


def compile_java_resource_classes(tmp_path: Path, resource_name: str, *, release: int = 8) -> list[Path]:
    """Compile one Java resource file and return every generated ``.class`` path."""

    source_path = TEST_RESOURCES / Path(resource_name)
    classes_dir = compile_java_sources(tmp_path, [source_path], release=release)
    class_files = sorted(classes_dir.rglob("*.class"), key=lambda path: str(path.relative_to(classes_dir)))
    if not class_files:
        raise AssertionError(f"Java resource {resource_name!r} produced no .class files")
    return class_files


def list_java_resources() -> list[str]:
    """Return Java source fixtures under ``tests/resources`` excluding oracle helpers."""

    return sorted(
        path.relative_to(TEST_RESOURCES).as_posix()
        for path in TEST_RESOURCES.rglob("*.java")
        if not path.is_relative_to(ORACLE_RESOURCE_ROOT)
    )


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


def _asm_jar_name(artifact: str) -> str:
    return f"{artifact}-{ASM_VERSION}.jar"


def _asm_jar_url(artifact: str) -> str:
    return f"https://repo1.maven.org/maven2/org/ow2/asm/{artifact}/{ASM_VERSION}/{_asm_jar_name(artifact)}"


def _download_file(url: str, destination: Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    request = urllib.request.Request(url, headers={"User-Agent": "pytecode-tests"})
    temp_path = destination.with_suffix(destination.suffix + ".tmp")
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            data = response.read()
    except urllib.error.URLError as exc:
        raise AssertionError(f"Failed to download {url}: {exc}") from exc

    if not data:
        raise AssertionError(f"Download from {url} returned no data")

    try:
        temp_path.write_bytes(data)
        temp_path.replace(destination)
    finally:
        if temp_path.exists():
            temp_path.unlink()


@functools.lru_cache(maxsize=1)
def ensure_asm_jars() -> tuple[Path, ...]:
    """Return the cached ASM 9.7.1 jars required by the CFG oracle."""

    jars: list[Path] = []
    for artifact in ASM_ARTIFACTS:
        local_path = ORACLE_LIB_ROOT / _asm_jar_name(artifact)
        cached_path = ASM_DOWNLOAD_ROOT / _asm_jar_name(artifact)
        if local_path.is_file():
            jars.append(local_path.resolve(strict=True))
            continue
        if not cached_path.is_file():
            _download_file(_asm_jar_url(artifact), cached_path)
        jars.append(cached_path.resolve(strict=True))
    return tuple(jars)


def compile_oracle(tmp_path: Path) -> Path:
    """Compile ``RecordingAnalyzer.java`` against the cached ASM jars."""

    classes_dir = compile_java_sources(
        tmp_path,
        [ORACLE_SOURCE_PATH],
        release=8,
        classpath=list(ensure_asm_jars()),
    )
    oracle_class = classes_dir / "RecordingAnalyzer.class"
    if not oracle_class.is_file():
        raise AssertionError("RecordingAnalyzer.java did not produce RecordingAnalyzer.class")
    return classes_dir


def run_oracle(class_file: Path, method_name: str | None = None) -> dict[str, Any]:
    """Run the ASM oracle against ``class_file`` and return its parsed JSON output."""

    resolved_class_file = class_file.resolve(strict=True)
    asm_jars = ensure_asm_jars()

    with tempfile.TemporaryDirectory() as temp_dir:
        classes_dir = compile_oracle(Path(temp_dir))
        classpath = os.pathsep.join(str(path) for path in (*asm_jars, classes_dir))
        command = ["java", "-cp", classpath, "RecordingAnalyzer", str(resolved_class_file)]
        if method_name is not None:
            command.append(method_name)

        result = subprocess.run(
            command,
            capture_output=True,
            text=True,
            check=False,
        )

    if result.returncode != 0:
        raise AssertionError(result.stderr or result.stdout or "Oracle execution failed")

    try:
        payload = json.loads(result.stdout)
    except json.JSONDecodeError as exc:
        raise AssertionError(f"Oracle returned invalid JSON: {exc}") from exc

    if not isinstance(payload, dict):
        raise AssertionError("Oracle output must be a JSON object")
    return cast(dict[str, Any], payload)


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
    """Tag 1 — CONSTANT_Utf8 entry: tag + u2(length) + modified UTF-8 bytes."""
    encoded = encode_modified_utf8(s)
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
    encoded = encode_modified_utf8(s)
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


def class_reader_with_cp(data: bytes, cp_list: list[cp_module.ConstantPoolInfo | None]) -> ClassReader:
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
    utf8_entry_bytes("TestClass") + class_entry_bytes(1) + utf8_entry_bytes("java/lang/Object") + class_entry_bytes(3)
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
    cp_list: list[cp_module.ConstantPoolInfo | None] = [None, make_utf8_info(1, attr_name)]
    blob = make_attribute_blob(1, payload)
    return class_reader_with_cp(blob, cp_list)
