from __future__ import annotations

import zipfile
from pathlib import Path

import pytest

import pytecode.archive as archive_module
from pytecode.analysis.hierarchy import ClassResolver
from pytecode.archive import JarFile
from pytecode.edit.debug_info import DebugInfoPolicy
from pytecode.transforms import ClassTransform


def _make_jar(path: Path) -> JarFile:
    with zipfile.ZipFile(path, "w") as zf:
        zf.writestr("Hello.class", b"\xca\xfe\xba\xbe")
        zf.writestr("README.txt", b"docs")
    return JarFile(path)


def test_rewrite_wrapper_uses_rust_when_available(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> None:
    jar = _make_jar(tmp_path / "in.jar")
    out_path = tmp_path / "out.jar"
    calls: list[tuple[str, tuple[str, ...], tuple[str, ...], bool]] = []

    def unexpected_python_path(*_args: object, **_kwargs: object) -> None:
        raise AssertionError("expected JarFile.rewrite() to dispatch to the Rust-backed archive path")

    def fake_rust_write(
        source_path: str,
        entries: list[archive_module.JarInfo],
        output_path: str,
        raw_copy_filenames: list[str],
        should_rewrite_classes: bool,
        transform: ClassTransform | None,
        recompute_frames: bool,
        resolver: ClassResolver | None,
        debug_policy: DebugInfoPolicy,
        skip_debug: bool,
    ) -> None:
        calls.append(
            (
                source_path,
                tuple(jar_info.filename for jar_info in entries),
                tuple(raw_copy_filenames),
                should_rewrite_classes,
            )
        )
        with zipfile.ZipFile(output_path, "w") as zf:
            for jar_info in entries:
                zf.writestr(archive_module._clone_zipinfo(jar_info.zipinfo, filename=jar_info.filename), jar_info.bytes)

    def rust_supported(_entries: list[archive_module.JarInfo], *, raw_copy_filenames: list[str]) -> bool:
        return True

    monkeypatch.setattr(archive_module, "_rewrite_archive_python", unexpected_python_path)
    monkeypatch.setattr(archive_module, "_rust_write_archive", fake_rust_write)
    monkeypatch.setattr(archive_module, "_can_use_rust_archive_write", rust_supported)

    jar.rewrite(out_path)

    assert len(calls) == 1
    assert calls[0][0] == str(tmp_path / "in.jar")
    assert calls[0][1] == ("Hello.class", "README.txt")
    assert calls[0][3] is False
    assert Path(jar.filename) == out_path


def test_rewrite_wrapper_falls_back_to_python_when_rust_cannot_preserve_metadata(
    monkeypatch: pytest.MonkeyPatch,
    tmp_path: Path,
) -> None:
    jar = _make_jar(tmp_path / "in.jar")
    out_path = tmp_path / "out.jar"
    calls: list[tuple[str, ...]] = []
    original = archive_module._rewrite_archive_python

    def wrapped_python_path(
        entries: list[archive_module.JarInfo],
        temp_path: Path,
        *,
        should_rewrite_classes: bool,
        transform: ClassTransform | None,
        recompute_frames: bool,
        resolver: ClassResolver | None,
        debug_policy: DebugInfoPolicy,
        skip_debug: bool,
    ) -> None:
        calls.append(tuple(jar_info.filename for jar_info in entries))
        original(
            entries,
            temp_path,
            should_rewrite_classes=should_rewrite_classes,
            transform=transform,
            recompute_frames=recompute_frames,
            resolver=resolver,
            debug_policy=debug_policy,
            skip_debug=skip_debug,
        )

    def unexpected_rust_path(*_args: object, **_kwargs: object) -> None:
        raise AssertionError("expected JarFile.rewrite() to fall back to the Python archive path")

    def rust_unsupported(_entries: list[archive_module.JarInfo], *, raw_copy_filenames: list[str]) -> bool:
        return False

    monkeypatch.setattr(archive_module, "_rewrite_archive_python", wrapped_python_path)
    monkeypatch.setattr(archive_module, "_rust_write_archive", unexpected_rust_path)
    monkeypatch.setattr(archive_module, "_can_use_rust_archive_write", rust_unsupported)

    jar.rewrite(out_path)

    assert calls == [("Hello.class", "README.txt")]
