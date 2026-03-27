"""Tests for fixture-compilation helpers."""

from __future__ import annotations

from pathlib import Path

import pytest

from tests import helpers


def _install_compile_tracker(monkeypatch: pytest.MonkeyPatch, cache_root: Path) -> dict[str, int]:
    call_counter = {"count": 0}
    real_compile = helpers._compile_java_sources_uncached

    def tracked_compile(classes_dir: Path, source_files: tuple[Path, ...], *, release: int = 8) -> None:
        call_counter["count"] += 1
        real_compile(classes_dir, source_files, release=release)

    monkeypatch.setattr(helpers, "JAVA_COMPILE_CACHE_ROOT", cache_root)
    monkeypatch.setattr(helpers, "_compile_java_sources_uncached", tracked_compile)
    return call_counter


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
