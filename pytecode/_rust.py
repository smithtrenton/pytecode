"""Source-tree loader for the local Rust extension during Phase 7 bring-up."""

from __future__ import annotations

import importlib.machinery
import importlib.util
import sys
from pathlib import Path
from types import ModuleType


def _extension_candidates() -> tuple[Path, ...]:
    repo_root = Path(__file__).resolve().parent.parent
    return (
        repo_root / "target" / "maturin" / "_rust.dll",
        repo_root / "target" / "debug" / "_rust.dll",
    )


def _load_extension() -> ModuleType:
    for candidate in _extension_candidates():
        if not candidate.is_file():
            continue
        loader = importlib.machinery.ExtensionFileLoader(__name__, str(candidate))
        spec = importlib.util.spec_from_file_location(__name__, candidate, loader=loader)
        if spec is None or spec.loader is None:
            continue
        module = importlib.util.module_from_spec(spec)
        sys.modules[__name__] = module
        spec.loader.exec_module(module)
        return module
    raise ModuleNotFoundError(
        "pytecode._rust extension not built; run `uv run maturin develop -m crates\\pytecode-python\\Cargo.toml` first"
    )


_module = _load_extension()

globals().update(_module.__dict__)
