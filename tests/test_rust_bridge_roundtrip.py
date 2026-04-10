"""Verify Rust-path and Python-path roundtrips produce byte-identical output.

For every test fixture class, this module:
1. Parses the class with both the Rust bridge and pure-Python paths
2. Asserts the re-serialised bytes are identical
3. Reports detailed diagnostics on any mismatch
"""

from __future__ import annotations

import zipfile
from pathlib import Path

import pytest

from pytecode.classfile.reader import ClassReader
from pytecode.edit.model import ClassModel

rust = pytest.importorskip("pytecode._rust")

# ---------------------------------------------------------------------------
# Fixture helpers
# ---------------------------------------------------------------------------

_REPO_ROOT = Path(__file__).resolve().parent.parent
_FIXTURE_JARS_DIR = _REPO_ROOT / "crates" / "pytecode-engine" / "fixtures" / "jars"

_JAVA_RESOURCES = sorted(
    (Path(__file__).resolve().parent / "resources").glob("*.java"),
)
_JAVA_RESOURCE_NAMES = [p.stem for p in _JAVA_RESOURCES]


def _compiled_fixture_classes() -> list[Path]:
    """Return all .class files compiled by the Rust fixture build system."""
    target_dir = _REPO_ROOT / "target" / "pytecode-rust-javac"
    if not target_dir.exists():
        pytest.skip("Rust fixture classes not compiled — run cargo test first")
    classes = sorted(target_dir.rglob("*.class"))
    if not classes:
        pytest.skip("No fixture classes found in target/pytecode-rust-javac")
    return classes


def _jar_classes(jar_name: str) -> list[tuple[str, bytes]]:
    """Extract (name, bytes) for every .class in a fixture JAR."""
    jar_path = _FIXTURE_JARS_DIR / jar_name
    if not jar_path.exists():
        pytest.skip(f"Fixture JAR not found: {jar_path}")
    result: list[tuple[str, bytes]] = []
    with zipfile.ZipFile(jar_path, "r") as zf:
        for entry in sorted(zf.namelist()):
            if entry.endswith(".class"):
                result.append((entry, zf.read(entry)))
    return result


# ---------------------------------------------------------------------------
# Core comparison helper
# ---------------------------------------------------------------------------


def _compare_paths(class_bytes: bytes, label: str) -> None:
    """Assert Rust-bridge and pure-Python paths produce identical output."""
    # Rust path
    rust_model = ClassModel.from_bytes(class_bytes)
    rust_out = rust_model.to_bytes()

    # Pure-Python path
    reader = ClassReader.from_bytes(class_bytes)
    py_model = ClassModel.from_classfile(reader.class_info)
    py_out = py_model.to_bytes()

    if rust_out != py_out:
        # Build diagnostic message
        diff_offset = next(
            (i for i in range(min(len(rust_out), len(py_out))) if rust_out[i] != py_out[i]),
            min(len(rust_out), len(py_out)),
        )
        msg_parts = [
            f"Rust/Python mismatch in {label}",
            f"  rust_len={len(rust_out)}, py_len={len(py_out)}",
            f"  first_diff_at_byte={diff_offset}",
        ]
        if diff_offset < len(rust_out) and diff_offset < len(py_out):
            ctx_start = max(0, diff_offset - 4)
            ctx_end = min(min(len(rust_out), len(py_out)), diff_offset + 16)
            msg_parts.append(f"  rust[{ctx_start}:{ctx_end}]={rust_out[ctx_start:ctx_end].hex()}")
            msg_parts.append(f"  py  [{ctx_start}:{ctx_end}]={py_out[ctx_start:ctx_end].hex()}")

        # Check model-level differences
        for attr_name in ("name", "super_name"):
            rv = getattr(rust_model, attr_name, None)
            pv = getattr(py_model, attr_name, None)
            if rv != pv:
                msg_parts.append(f"  model.{attr_name}: rust={rv!r}, py={pv!r}")

        if len(rust_model.methods) != len(py_model.methods):
            msg_parts.append(f"  method_count: rust={len(rust_model.methods)}, py={len(py_model.methods)}")

        pytest.fail("\n".join(msg_parts))


# ---------------------------------------------------------------------------
# Parametrized tests: compiled fixture classes
# ---------------------------------------------------------------------------


@pytest.fixture(scope="module")
def fixture_classes() -> list[Path]:
    return _compiled_fixture_classes()


# Use a simpler approach: test all fixtures in a single test with sub-assertions
def test_all_fixture_classes_roundtrip() -> None:
    """Every compiled fixture class must produce identical bytes via both paths."""
    classes = _compiled_fixture_classes()
    failures: list[str] = []
    for class_path in classes:
        class_bytes = class_path.read_bytes()
        try:
            _compare_paths(class_bytes, class_path.name)
        except Exception as exc:
            failures.append(str(exc))
    if failures:
        pytest.fail(
            f"{len(failures)}/{len(classes)} fixture classes have Rust/Python mismatch:\n"
            + "\n---\n".join(failures[:10])
        )


# ---------------------------------------------------------------------------
# JAR-level roundtrip (subset sampling for speed)
# ---------------------------------------------------------------------------


_FIXTURE_JAR_NAMES: list[str] = sorted(
    p.name for p in _FIXTURE_JARS_DIR.iterdir() if _FIXTURE_JARS_DIR.exists() and p.suffix == ".jar"
)


@pytest.mark.parametrize("jar_name", _FIXTURE_JAR_NAMES or ["no-jars"])
def test_fixture_jar_roundtrip(jar_name: str) -> None:
    """Every class in each fixture JAR must be byte-exact via the Rust path."""
    if jar_name == "no-jars":
        pytest.skip("No fixture JARs available")
    entries = _jar_classes(jar_name)
    failures: list[str] = []
    for name, data in entries:
        try:
            model = ClassModel.from_bytes(data)
            out = model.to_bytes()
            if out != data:
                diff_offset = next(
                    (i for i in range(min(len(out), len(data))) if out[i] != data[i]),
                    min(len(out), len(data)),
                )
                failures.append(f"{jar_name}!{name}: orig={len(data)} out={len(out)} first_diff={diff_offset}")
        except Exception as exc:
            failures.append(f"{jar_name}!{name}: {exc}")
    if failures:
        pytest.fail(
            f"{len(failures)}/{len(entries)} classes in {jar_name} are not byte-exact via Rust:\n"
            + "\n".join(failures[:10])
        )


# ---------------------------------------------------------------------------
# Model attribute fidelity checks
# ---------------------------------------------------------------------------


def test_nested_attribute_layout_propagated() -> None:
    """Rust bridge must propagate _nested_attribute_layout to Python CodeModel."""
    classes = _compiled_fixture_classes()
    for class_path in classes:
        class_bytes = class_path.read_bytes()
        model = ClassModel.from_bytes(class_bytes)
        for method in model.methods:
            if method.code is None:
                continue
            # Layout must not be empty if the code has any sub-attributes
            has_sub_attrs = (
                method.code.line_numbers
                or method.code.local_variables
                or method.code.local_variable_types
                or method.code.attributes
            )
            if has_sub_attrs:
                assert method.code._nested_attribute_layout, (
                    f"{class_path.name}::{method.name} has sub-attributes but empty layout"
                )


def test_ldc_values_unsigned() -> None:
    """LdcInt and LdcLong values must be unsigned per JVMS."""
    from pytecode.edit.operands import LdcInt, LdcLong

    classes = _compiled_fixture_classes()
    for class_path in classes:
        class_bytes = class_path.read_bytes()
        model = ClassModel.from_bytes(class_bytes)
        for method in model.methods:
            if method.code is None:
                continue
            for item in method.code.instructions:
                if isinstance(item, LdcInt):
                    assert item.value >= 0, f"LdcInt should be unsigned: {item.value}"
                elif isinstance(item, LdcLong):
                    assert item.value >= 0, f"LdcLong should be unsigned: {item.value}"


def test_operand_types_preserved() -> None:
    """BIPUSH/SIPUSH/NEWARRAY must preserve their operand types through bridge."""
    from pytecode.classfile.instructions import ByteValue, NewArray

    classes = _compiled_fixture_classes()
    has_byte = False
    has_newarray = False
    for class_path in classes:
        class_bytes = class_path.read_bytes()
        model = ClassModel.from_bytes(class_bytes)
        for method in model.methods:
            if method.code is None:
                continue
            for item in method.code.instructions:
                if isinstance(item, ByteValue):
                    has_byte = True
                elif isinstance(item, NewArray):
                    has_newarray = True

    assert has_byte, "Expected at least one ByteValue (BIPUSH) in fixture classes"
    assert has_newarray, "Expected at least one NewArray in fixture classes"


# ---------------------------------------------------------------------------
# Byte-exact original roundtrip
# ---------------------------------------------------------------------------


def test_fixture_classes_byte_exact_original_roundtrip() -> None:
    """Every fixture class must produce byte-exact output matching the original bytes."""
    classes = _compiled_fixture_classes()
    failures: list[str] = []
    for class_path in classes:
        original = class_path.read_bytes()
        reader = ClassReader.from_bytes(original)
        model = ClassModel.from_classfile(reader.class_info)
        output = model.to_bytes()
        if output != original:
            failures.append(
                f"{class_path.name}: orig={len(original)} out={len(output)} diff={len(output) - len(original)}"
            )
    if failures:
        pytest.fail(
            f"{len(failures)}/{len(classes)} fixture classes differ from original:\n" + "\n".join(failures[:10])
        )
