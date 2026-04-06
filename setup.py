"""Opt-in setuptools entry point for pytecode's optional Cython modules."""

from __future__ import annotations

import os
import sys
from pathlib import Path

from setuptools import Extension, setup

REPO_ROOT = Path(__file__).parent.resolve()
PACKAGE_ROOT = REPO_ROOT / "pytecode"
CYTHON_PROFILE_ENV_VAR = "PYTECODE_CYTHON_PROFILE"


def _should_build_cython() -> bool:
    commands = {argument.partition("=")[0] for argument in sys.argv[1:] if argument and not argument.startswith("-")}
    return "build_ext" in commands


def _discover_cython_extensions() -> list[Extension]:
    extensions: list[Extension] = []
    for source_path in sorted(PACKAGE_ROOT.rglob("*_cy.pyx")):
        relative_path = source_path.relative_to(REPO_ROOT)
        module_name = ".".join(relative_path.with_suffix("").parts)
        extensions.append(Extension(module_name, [relative_path.as_posix()]))
    return extensions


def _compiler_directives() -> dict[str, bool]:
    directives: dict[str, bool] = {}
    if os.environ.get(CYTHON_PROFILE_ENV_VAR) == "1":
        directives["profile"] = True
    return directives


def _build_ext_modules() -> list[Extension]:
    if not _should_build_cython():
        return []

    try:
        from Cython.Build import cythonize
    except ImportError as exc:  # pragma: no cover - setup.py is exercised via build commands
        raise RuntimeError(
            "Cython is required to build pytecode's optional extensions. "
            "Install the dev dependencies first (for example, `uv sync --dev`)."
        ) from exc

    return cythonize(
        _discover_cython_extensions(),
        compiler_directives=_compiler_directives(),
        force=True,
    )


if __name__ == "__main__":
    setup(ext_modules=_build_ext_modules())
