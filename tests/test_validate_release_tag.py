"""Tests for release-tag validation helpers."""

from __future__ import annotations

from pathlib import Path

import pytest

from tools.validate_release_tag import extract_tag_version, main, read_project_version, validate_release_tag


def write_pyproject(path: Path, version: str) -> Path:
    """Create a minimal ``pyproject.toml`` for release-tag tests."""
    path.write_text(
        f'[project]\nname = "example-project"\nversion = "{version}"\n',
        encoding="utf-8",
    )
    return path


def test_read_project_version_returns_version(tmp_path: Path) -> None:
    pyproject_path = write_pyproject(tmp_path / "pyproject.toml", "1.2.3")
    assert read_project_version(pyproject_path) == "1.2.3"


def test_read_project_version_requires_value(tmp_path: Path) -> None:
    pyproject_path = tmp_path / "pyproject.toml"
    pyproject_path.write_text('[project]\nname = "example-project"\n', encoding="utf-8")

    with pytest.raises(ValueError, match="project.version"):
        read_project_version(pyproject_path)


def test_extract_tag_version_requires_v_prefix() -> None:
    with pytest.raises(ValueError, match="start with 'v'"):
        extract_tag_version("1.2.3")


def test_validate_release_tag_accepts_matching_version() -> None:
    assert validate_release_tag("v1.2.3", "1.2.3") == "1.2.3"


def test_validate_release_tag_rejects_mismatch() -> None:
    with pytest.raises(ValueError, match="does not match"):
        validate_release_tag("v1.2.4", "1.2.3")


def test_main_reports_success(tmp_path: Path, capsys: pytest.CaptureFixture[str]) -> None:
    pyproject_path = write_pyproject(tmp_path / "pyproject.toml", "1.2.3")

    assert main(["--tag", "v1.2.3", "--pyproject", str(pyproject_path)]) == 0

    captured = capsys.readouterr()
    assert captured.err == ""
    assert captured.out.strip() == "Release tag v1.2.3 matches project.version 1.2.3."


def test_main_reports_failure(tmp_path: Path, capsys: pytest.CaptureFixture[str]) -> None:
    pyproject_path = write_pyproject(tmp_path / "pyproject.toml", "1.2.3")

    assert main(["--tag", "v1.2.4", "--pyproject", str(pyproject_path)]) == 1

    captured = capsys.readouterr()
    assert captured.out == ""
    assert "Release validation failed" in captured.err
    assert "does not match project.version '1.2.3'" in captured.err
