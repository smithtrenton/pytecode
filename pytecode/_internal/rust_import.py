from __future__ import annotations

import os
from importlib import import_module
from types import ModuleType


def import_optional_rust_module(name: str) -> ModuleType:
    """Import an optional ``pytecode._rust`` module unless Rust is blocked."""
    if os.environ.get("PYTECODE_BLOCK_RUST"):
        raise ModuleNotFoundError(name)
    return import_module(name)
