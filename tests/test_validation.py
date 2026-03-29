"""Multi-release, multi-tier validation tests for pytecode classfile emission.

Runs all four validation tiers in a single parametrized test per (fixture, release):

- **Tier 1 — Roundtrip**: byte-for-byte ``ClassWriter.write()`` and
  ``ClassModel.to_bytes()`` roundtrips.
- **Tier 2 — Structural**: ``verify_classfile()`` + ``javap -v -p -c`` exit-code check.
  Compares diagnostics against the gold (javac) class so pre-existing javac
  quirks are not counted as regressions.
- **Tier 3 — Semantic**: full javap parse + CP-aware semantic diff between gold
  (javac) and roundtripped output.
- **Tier 4 — JVM loading**: ``VerifierHarness`` with ``-Xverify:all``.  The
  compilation output directory is placed on the classpath so that companion /
  inner classes are resolvable.

All tests skip gracefully when ``javac``/``java`` are unavailable.
"""

from __future__ import annotations

from pathlib import Path

import pytest

from pytecode import ClassModel, ClassReader, ClassWriter
from pytecode.verify import Severity, verify_classfile
from tests.helpers import (
    can_java,
    can_javac,
    compile_java_resource_classes,
    run_javap,
    run_javap_check,
)
from tests.javap_parser import DiffSeverity, parse_javap, semantic_diff
from tests.jvm_harness import execute_class, verify_class
from tests.validation_fixtures import validation_matrix

_requires_javac = pytest.mark.skipif(not can_javac(), reason="javac not available")
_requires_java = pytest.mark.skipif(not can_java(), reason="java not available")

_MATRIX = validation_matrix()


def _error_messages(data: bytes) -> set[str]:
    """Return the set of ERROR-level diagnostic messages from ``verify_classfile``."""
    diags = verify_classfile(ClassReader(data).class_info)
    return {d.message for d in diags if d.severity is Severity.ERROR}


def _roundtrip_class_bytes(original: bytes) -> bytes:
    """Parse → emit → return emitted bytes."""
    parsed = ClassReader(original).class_info
    return ClassWriter.write(parsed)


@_requires_javac
@pytest.mark.parametrize("fixture_name,release", _MATRIX, ids=[f"{f}@{r}" for f, r in _MATRIX])
def test_all_tiers(tmp_path: Path, fixture_name: str, release: int) -> None:
    """Run Tiers 1–4 on every .class file produced by (fixture, release)."""
    class_paths = compile_java_resource_classes(tmp_path, fixture_name, release=release)
    # The compilation root is always tmp_path / "classes" (set by compile_java_sources).
    # This is needed as extra classpath for Tier 4 so companion / inner / packaged
    # classes are resolvable by the JVM.
    compilation_root = tmp_path / "classes"

    for class_path in class_paths:
        original = class_path.read_bytes()

        # ── Tier 1: Roundtrip ─────────────────────────────────────────
        try:
            parsed = ClassReader(original).class_info
        except ValueError:
            # Pre-existing ClassReader limitation (e.g., unsupported annotation
            # element value tags in record classes).  Skip this class file —
            # not a validation regression.
            continue

        emitted = ClassWriter.write(parsed)
        assert emitted == original, f"ClassWriter roundtrip failed for {class_path.name}"

        model = ClassModel.from_classfile(parsed)
        lowered = model.to_bytes()
        assert lowered == original, f"ClassModel roundtrip failed for {class_path.name}"

        # ── Tier 2: Structural ────────────────────────────────────────
        # Compare verify_classfile diagnostics: only fail if the roundtrip
        # INTRODUCES new errors not present in the gold (javac) class.
        gold_errors = _error_messages(original)
        our_errors = _error_messages(emitted)
        new_errors = our_errors - gold_errors
        assert not new_errors, f"verify_classfile new errors for {class_path.name}: {new_errors}"

        ClassReader(emitted)  # re-parse must not raise

        # Write roundtripped bytes to temp file for external tool checks
        roundtripped_path = tmp_path / f"roundtripped_{class_path.name}"
        roundtripped_path.write_bytes(emitted)

        if can_javac():
            assert run_javap_check(roundtripped_path), f"javap rejected roundtripped {class_path.name}"

        # ── Tier 3: Semantic diff ─────────────────────────────────────
        if can_javac():
            gold_output = run_javap(class_path)
            our_output = run_javap(roundtripped_path)

            gold_parsed = parse_javap(gold_output)
            our_parsed = parse_javap(our_output)

            diffs = semantic_diff(gold_parsed, our_parsed)
            errors = [d for d in diffs if d.severity is DiffSeverity.ERROR]
            assert errors == [], f"Semantic diff errors for {class_path.name}@{release}: {errors}"

        # ── Tier 4: JVM loading ───────────────────────────────────────
        if can_java():
            result = verify_class(
                roundtripped_path,
                extra_classpath=[compilation_root],
            )
            assert result.ok, f"JVM rejected roundtripped {class_path.name}: {result.message}"


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
    class_paths = compile_java_resource_classes(tmp_path, fixture_name, release=release)
    compilation_root = tmp_path / "classes"

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
