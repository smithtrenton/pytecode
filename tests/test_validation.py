"""Multi-release validation tests for pytecode classfile emission.

For each ``(fixture, release)`` pair, the suite requires byte-for-byte
roundtrip identity, preserves cheap in-process verification coverage, and
then runs external acceptance checks on the emitted bytes with ``javap`` and,
when available, the JVM verifier harness.

All tests skip gracefully when ``javac``/``java`` are unavailable.
"""

from __future__ import annotations

from pathlib import Path

import pytest

from pytecode import ClassModel, ClassReader, ClassWriter
from pytecode.info import ClassFile
from pytecode.verify import Severity, verify_classfile
from tests.helpers import (
    cached_java_resource_classes,
    cached_java_resource_classes_dir,
    can_java,
    can_javac,
    run_javap_many,
)
from tests.jvm_harness import execute_class, verify_classes
from tests.validation_fixtures import validation_matrix

_requires_javac = pytest.mark.skipif(not can_javac(), reason="javac not available")
_requires_java = pytest.mark.skipif(not can_java(), reason="java not available")

_MATRIX = validation_matrix()


def _error_messages(cf: ClassFile) -> set[str]:
    """Return the set of ERROR-level diagnostic messages from ``verify_classfile``."""
    diags = verify_classfile(cf)
    return {diag.message for diag in diags if diag.severity is Severity.ERROR}


def _roundtrip_class_bytes(original: bytes) -> bytes:
    """Parse → emit → return emitted bytes."""
    parsed = ClassReader(original).class_info
    return ClassWriter.write(parsed)


@_requires_javac
@pytest.mark.parametrize("fixture_name,release", _MATRIX, ids=[f"{f}@{r}" for f, r in _MATRIX])
def test_all_tiers(tmp_path: Path, fixture_name: str, release: int) -> None:
    """Run roundtrip plus external acceptance checks on every generated class."""
    compilation_root = cached_java_resource_classes_dir(fixture_name, release=release)
    class_paths = cached_java_resource_classes(fixture_name, release=release)
    roundtripped_paths: list[Path] = []

    for class_path in class_paths:
        original = class_path.read_bytes()

        # ── Tier 1: Roundtrip ─────────────────────────────────────────
        parsed = ClassReader(original).class_info

        emitted = ClassWriter.write(parsed)
        assert emitted == original, f"ClassWriter roundtrip failed for {class_path.name}"

        emitted_parsed = ClassReader(emitted).class_info
        gold_errors = _error_messages(parsed)
        our_errors = _error_messages(emitted_parsed)
        new_errors = our_errors - gold_errors
        assert not new_errors, f"verify_classfile new errors for {class_path.name}: {new_errors}"

        model = ClassModel.from_classfile(parsed)
        lowered = model.to_bytes()
        assert lowered == original, f"ClassModel roundtrip failed for {class_path.name}"

        roundtripped_path = tmp_path / f"roundtripped_{class_path.name}"
        roundtripped_path.write_bytes(emitted)
        roundtripped_paths.append(roundtripped_path)

    # Byte-for-byte identity makes a separate gold-file javap comparison
    # redundant; one batched javap pass still proves the emitted files remain
    # externally readable.
    run_javap_many(roundtripped_paths)

    if can_java():
        results = verify_classes(
            roundtripped_paths,
            extra_classpath=[compilation_root],
        )
        failures = [
            f"{path.name}: {result.message or result.status}"
            for path, result in zip(roundtripped_paths, results, strict=True)
            if not result.ok
        ]
        assert failures == [], "JVM rejected roundtripped classes:\n" + "\n".join(failures)


# ---------------------------------------------------------------------------
# Execution tests — verify known-output fixtures produce correct runtime output
# ---------------------------------------------------------------------------

_EXECUTION_FIXTURES: list[tuple[str, int, str, str]] = [
    # (fixture, release, main_class, expected_substring)
    ("HelloWorld.java", 8, "HelloWorld", "Hello from fixture"),
]


@_requires_javac
@_requires_java
@pytest.mark.parametrize(
    "fixture_name,release,main_class,expected",
    _EXECUTION_FIXTURES,
    ids=[f"{f}@{r}" for f, r, _, _ in _EXECUTION_FIXTURES],
)
def test_execution(
    tmp_path: Path,
    fixture_name: str,
    release: int,
    main_class: str,
    expected: str,
) -> None:
    """Verify that roundtripped classes produce correct runtime output."""
    compilation_root = cached_java_resource_classes_dir(fixture_name, release=release)
    class_paths = cached_java_resource_classes(fixture_name, release=release)

    # Find the main class file
    main_path: Path | None = None
    for cp in class_paths:
        if cp.stem == main_class:
            main_path = cp
            break
    assert main_path is not None, f"Main class {main_class} not found in compiled output"

    original = main_path.read_bytes()
    emitted = _roundtrip_class_bytes(original)

    roundtripped = tmp_path / f"exec_{main_path.name}"
    roundtripped.write_bytes(emitted)

    result = execute_class(
        roundtripped,
        main_class,
        extra_classpath=[compilation_root],
    )
    assert result.ok, f"Execution failed: {result.message or result.exec_error}"
    assert result.stdout is not None
    assert expected in result.stdout, f"Expected {expected!r} in stdout, got: {result.stdout!r}"
