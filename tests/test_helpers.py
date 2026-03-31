"""Tests for fixture-compilation helpers."""

from __future__ import annotations

from pathlib import Path

import pytest

from tests import helpers

_requires_javap = pytest.mark.skipif(not helpers.can_javac(), reason="javap not available")
_requires_java = pytest.mark.skipif(not (helpers.can_java() and helpers.can_javac()), reason="java/javac not available")


def _install_compile_tracker(monkeypatch: pytest.MonkeyPatch, cache_root: Path) -> dict[str, int]:
    call_counter = {"count": 0}
    real_compile = helpers._compile_java_sources_uncached

    def tracked_compile(
        classes_dir: Path,
        source_files: tuple[Path, ...],
        *,
        release: int = 8,
        classpath: tuple[Path, ...] = (),
    ) -> None:
        call_counter["count"] += 1
        real_compile(classes_dir, source_files, release=release, classpath=classpath)

    monkeypatch.setattr(helpers, "JAVA_COMPILE_CACHE_ROOT", cache_root)
    monkeypatch.setattr(helpers, "_compile_java_sources_uncached", tracked_compile)
    return call_counter


def _install_external_tool_cache(monkeypatch: pytest.MonkeyPatch, cache_root: Path) -> None:
    monkeypatch.setattr(helpers, "EXTERNAL_TOOL_CACHE_ROOT", cache_root)


def test_compile_java_resource_classes_reuses_cached_outputs(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    calls = _install_compile_tracker(monkeypatch, tmp_path / "cache")

    first = helpers.compile_java_resource_classes(tmp_path / "first", "Outer.java")
    second = helpers.compile_java_resource_classes(tmp_path / "second", "Outer.java")

    assert calls["count"] == 1
    assert sorted(path.name for path in first) == sorted(path.name for path in second)
    assert len(first) > 1
    assert all(path.is_file() for path in first)
    assert all(path.is_file() for path in second)
    assert all(path.is_relative_to(tmp_path / "first" / "classes") for path in first)
    assert all(path.is_relative_to(tmp_path / "second" / "classes") for path in second)


def test_compile_java_sources_recompiles_when_source_changes(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    calls = _install_compile_tracker(monkeypatch, tmp_path / "cache")
    source_path = tmp_path / "src" / "Demo.java"
    source_path.parent.mkdir(parents=True, exist_ok=True)
    source_path.write_text("public class Demo { public int value() { return 1; } }\n", encoding="utf-8")

    helpers.compile_java_sources(tmp_path / "first", [source_path])

    source_path.write_text("public class Demo { public int value() { return 2; } }\n", encoding="utf-8")
    helpers.compile_java_sources(tmp_path / "second", [source_path])

    assert calls["count"] == 2


def test_compile_java_sources_recompiles_when_release_changes(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    calls = _install_compile_tracker(monkeypatch, tmp_path / "cache")
    source_path = tmp_path / "src" / "Demo.java"
    source_path.parent.mkdir(parents=True, exist_ok=True)
    source_path.write_text("public class Demo { public int value() { return 1; } }\n", encoding="utf-8")

    helpers.compile_java_sources(tmp_path / "release8", [source_path], release=8)
    helpers.compile_java_sources(tmp_path / "release11", [source_path], release=11)

    assert calls["count"] == 2


def test_cached_java_resource_classes_returns_shared_cache_paths(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    calls = _install_compile_tracker(monkeypatch, tmp_path / "cache")

    first = helpers.cached_java_resource_classes("Outer.java")
    second = helpers.cached_java_resource_classes("Outer.java")

    assert calls["count"] == 1
    assert first == second
    assert len(first) > 1
    assert all(path.is_file() for path in first)
    assert all(path.is_relative_to(tmp_path / "cache") for path in first)


def test_cached_java_resource_classes_dir_returns_compilation_root(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    calls = _install_compile_tracker(monkeypatch, tmp_path / "cache")

    classes_dir = helpers.cached_java_resource_classes_dir("HierarchyFixture.java")
    class_files = helpers.cached_java_resource_classes("HierarchyFixture.java")

    assert calls["count"] == 1
    assert all(path.is_relative_to(classes_dir) for path in class_files)
    assert (classes_dir / "fixture" / "hierarchy" / "HierarchyFixture.class").is_file()


@_requires_javap
def test_run_javap_many_accepts_multiple_class_files() -> None:
    class_files = helpers.cached_java_resource_classes("HierarchyFixture.java")
    selected = class_files[:2]

    output = helpers.run_javap_many(selected)

    for class_file in selected:
        assert class_file.stem in output


@_requires_java
def test_run_verifier_harness_many_returns_ordered_results() -> None:
    classes_dir = helpers.cached_java_resource_classes_dir("HierarchyFixture.java")
    class_files = list(helpers.cached_java_resource_classes("HierarchyFixture.java"))[:3]

    payload = helpers.run_verifier_harness_many(class_files, extra_classpath=[classes_dir])

    assert [item["path"] for item in payload] == [str(path.resolve(strict=True)) for path in class_files]
    assert all(item["status"] == "VERIFY_OK" for item in payload)


def test_run_javap_many_memoizes_by_class_content(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    _install_external_tool_cache(monkeypatch, tmp_path / "external-cache")
    monkeypatch.setattr(helpers, "_get_javap_identity", lambda: "javap-test")

    first = tmp_path / "one.class"
    second = tmp_path / "two.class"
    first.write_bytes(b"same-bytes")
    second.write_bytes(b"same-bytes")

    calls = {"count": 0}

    def fake_run_javap_paths(class_paths: list[Path]) -> str:
        calls["count"] += 1
        return "\n".join(f"Classfile {helpers._format_javap_path(path)}" for path in class_paths) + "\n"

    monkeypatch.setattr(helpers, "_run_javap_paths", fake_run_javap_paths)

    first_output = helpers.run_javap_many([first])
    second_output = helpers.run_javap_many([second])

    assert calls["count"] == 1
    assert f"Classfile {helpers._format_javap_path(first)}" in first_output
    assert f"Classfile {helpers._format_javap_path(second)}" in second_output


def test_run_verifier_harness_many_memoizes_by_class_content(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    _install_external_tool_cache(monkeypatch, tmp_path / "external-cache")
    monkeypatch.setattr(helpers, "_get_java_identity", lambda: "java-test")

    first = tmp_path / "one.class"
    second = tmp_path / "two.class"
    first.write_bytes(b"same-bytes")
    second.write_bytes(b"same-bytes")

    calls = {"count": 0}

    def fake_run_verifier_harness_command(command: list[str], *, context: str) -> object:
        calls["count"] += 1
        batch_index = command.index("batch")
        return [
            {"path": str(Path(path).resolve(strict=True)), "status": "VERIFY_OK"} for path in command[batch_index + 1 :]
        ]

    monkeypatch.setattr(helpers, "_run_verifier_harness_command", fake_run_verifier_harness_command)

    first_payload = helpers.run_verifier_harness_many([first])
    second_payload = helpers.run_verifier_harness_many([second])

    assert calls["count"] == 1
    assert first_payload == [{"path": str(first.resolve(strict=True)), "status": "VERIFY_OK"}]
    assert second_payload == [{"path": str(second.resolve(strict=True)), "status": "VERIFY_OK"}]
