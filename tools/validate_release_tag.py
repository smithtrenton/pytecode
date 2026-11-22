"""Validate that a release tag matches ``project.version`` in ``pyproject.toml``."""

from __future__ import annotations

import argparse
import re
import sys
from collections.abc import Sequence
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_PYPROJECT_PATH = REPO_ROOT / "pyproject.toml"
SECTION_PATTERN = re.compile(r"^\[(?P<section>[A-Za-z0-9_.-]+)\]\s*(?:#.*)?$")
VERSION_PATTERN = re.compile(r'^version\s*=\s*"(?P<version>[^"]+)"\s*(?:#.*)?$')


def read_project_version(pyproject_path: Path = DEFAULT_PYPROJECT_PATH) -> str:
    """Return the static ``project.version`` value from ``pyproject.toml``."""
    current_section: str | None = None
    for raw_line in pyproject_path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue

        section_match = SECTION_PATTERN.match(line)
        if section_match is not None:
            current_section = section_match.group("section")
            continue

        if current_section != "project":
            continue

        version_match = VERSION_PATTERN.match(line)
        if version_match is not None:
            return version_match.group("version")

    raise ValueError(f"{pyproject_path} is missing a non-empty project.version value in [project].")


def read_crate_versions(repo_root: Path) -> dict[str, str]:
    """Return mapping of workspace crate directory names to ``[package] version``."""
    crates_dir = repo_root / "crates"
    if not crates_dir.is_dir():
        return {}
    versions: dict[str, str] = {}
    for manifest in sorted(crates_dir.glob("*/Cargo.toml")):
        current_section: str | None = None
        found: str | None = None
        for raw_line in manifest.read_text(encoding="utf-8").splitlines():
            line = raw_line.strip()
            if not line or line.startswith("#"):
                continue

            section_match = SECTION_PATTERN.match(line)
            if section_match is not None:
                current_section = section_match.group("section")
                continue

            if current_section != "package":
                continue

            version_match = VERSION_PATTERN.match(line)
            if version_match is not None:
                found = version_match.group("version")
                break

        if found is None:
            raise ValueError(f"{manifest} is missing a non-empty package.version value in [package].")
        versions[manifest.parent.name] = found
    return versions


def extract_tag_version(tag: str) -> str:
    """Strip and validate the version portion of a release tag."""
    if not tag:
        raise ValueError("Release tag must not be empty.")
    if not tag.startswith("v"):
        raise ValueError("Release tag must start with 'v'.")

    version = tag.removeprefix("v")
    if not version:
        raise ValueError("Release tag must include a version after 'v'.")

    return version


def validate_release_tag(tag: str, expected_version: str) -> str:
    """Validate that ``tag`` points at ``expected_version``."""
    tag_version = extract_tag_version(tag)
    if tag_version != expected_version:
        raise ValueError(f"Release tag {tag!r} does not match project.version {expected_version!r}.")
    return tag_version


def validate_workspace_versions(pyproject_path: Path, expected_version: str) -> None:
    """Validate that every workspace crate version matches ``expected_version`` when ``crates/`` exists."""
    repo_root = pyproject_path.resolve().parent
    crate_versions = read_crate_versions(repo_root)
    mismatched = {name: version for name, version in crate_versions.items() if version != expected_version}
    if mismatched:
        details = ", ".join(f"{name}={version!r}" for name, version in sorted(mismatched.items()))
        raise ValueError(f"Workspace crate versions ({details}) do not match project.version {expected_version!r}.")


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    """Parse command-line arguments for release-tag validation."""
    parser = argparse.ArgumentParser(
        description="Validate that a release tag matches project.version in pyproject.toml.",
    )
    parser.add_argument(
        "--tag",
        required=True,
        help="Release tag to validate, including the leading 'v' (for example: v0.1.0).",
    )
    parser.add_argument(
        "--pyproject",
        type=Path,
        default=DEFAULT_PYPROJECT_PATH,
        help="Path to the pyproject.toml file to validate against.",
    )
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    """Run the release-tag validation command."""
    args = parse_args(argv)

    try:
        expected_version = read_project_version(args.pyproject)
        validate_release_tag(args.tag, expected_version)
        validate_workspace_versions(args.pyproject, expected_version)
    except ValueError as exc:
        print(f"Release validation failed: {exc}", file=sys.stderr)
        return 1

    print(f"Release tag {args.tag} matches project.version {expected_version}.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
