"""Tests for fixture-compilation helpers."""

from __future__ import annotations

import subprocess
from pathlib import Path
from typing import Any

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


def test_ensure_asm_jars_prefers_local_jars_and_caches_downloads(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    local_root = tmp_path / "local"
    download_root = tmp_path / "downloads"
    local_root.mkdir(parents=True, exist_ok=True)

    local_jar = local_root / helpers._asm_jar_name("asm")
    local_jar.write_bytes(b"local")
    downloads: list[tuple[str, Path]] = []

    def fake_download(url: str, destination: Path) -> None:
        downloads.append((url, destination))
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(b"downloaded")

    helpers.ensure_asm_jars.cache_clear()
    try:
        monkeypatch.setattr(helpers, "ORACLE_LIB_ROOT", local_root)
        monkeypatch.setattr(helpers, "ASM_DOWNLOAD_ROOT", download_root)
        monkeypatch.setattr(helpers, "ASM_ARTIFACTS", ("asm", "asm-tree"))
        monkeypatch.setattr(helpers, "_download_file", fake_download)

        first = helpers.ensure_asm_jars()
        second = helpers.ensure_asm_jars()
    finally:
        helpers.ensure_asm_jars.cache_clear()

    cached_jar = download_root / helpers._asm_jar_name("asm-tree")
    assert first == second
    assert first == (
        local_jar.resolve(strict=True),
        cached_jar.resolve(strict=True),
    )
    assert downloads == [(helpers._asm_jar_url("asm-tree"), cached_jar)]


def test_run_oracle_passes_method_name_and_returns_dict(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    class_file = tmp_path / "Demo.class"
    class_file.write_bytes(b"class-bytes")
    oracle_classes = tmp_path / "oracle-classes"
    oracle_classes.mkdir(parents=True, exist_ok=True)
    commands: list[list[str]] = []

    def fake_ensure_asm_jars() -> tuple[Path, ...]:
        return ()

    def fake_compile_oracle(_tmp_path: Path) -> Path:
        return oracle_classes

    def fake_jdk_tool(name: str) -> str:
        return f"fake-{name}"

    def fake_run(command: list[str], **_: object) -> subprocess.CompletedProcess[str]:
        commands.append(command)
        return subprocess.CompletedProcess(command, 0, '{"status":"ok"}', "")

    monkeypatch.setattr(helpers, "ensure_asm_jars", fake_ensure_asm_jars)
    monkeypatch.setattr(helpers, "compile_oracle", fake_compile_oracle)
    monkeypatch.setattr(helpers, "_jdk_tool", fake_jdk_tool)
    monkeypatch.setattr(helpers.subprocess, "run", fake_run)

    payload = helpers.run_oracle(class_file, "targetMethod")

    assert payload == {"status": "ok"}
    assert commands == [
        [
            "fake-java",
            "-cp",
            str(oracle_classes),
            "RecordingAnalyzer",
            str(class_file.resolve(strict=True)),
            "targetMethod",
        ]
    ]


@pytest.mark.parametrize(
    ("stdout", "message"),
    [
        ("[]", "Oracle output must be a JSON object"),
        ("{not-json", "Oracle returned invalid JSON"),
    ],
)
def test_run_oracle_validates_json_output(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    stdout: str,
    message: str,
) -> None:
    class_file = tmp_path / "Demo.class"
    class_file.write_bytes(b"class-bytes")
    oracle_classes = tmp_path / "oracle-classes"
    oracle_classes.mkdir(parents=True, exist_ok=True)

    def fake_ensure_asm_jars() -> tuple[Path, ...]:
        return ()

    def fake_compile_oracle(_tmp_path: Path) -> Path:
        return oracle_classes

    def fake_jdk_tool(name: str) -> str:
        return f"fake-{name}"

    def fake_run(command: list[str], **_: Any) -> subprocess.CompletedProcess[str]:
        return subprocess.CompletedProcess(command, 0, stdout, "")

    monkeypatch.setattr(helpers, "ensure_asm_jars", fake_ensure_asm_jars)
    monkeypatch.setattr(helpers, "compile_oracle", fake_compile_oracle)
    monkeypatch.setattr(helpers, "_jdk_tool", fake_jdk_tool)
    monkeypatch.setattr(helpers.subprocess, "run", fake_run)

    with pytest.raises(AssertionError, match=message):
        helpers.run_oracle(class_file)
