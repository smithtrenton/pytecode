"""Import helper for optional Cython-accelerated modules.

When the environment variable ``PYTECODE_BLOCK_CYTHON`` is set to ``1``,
the compiled Cython extension is bypassed and the pure-Python fallback
is used instead.  This mirrors the Rust-branch pattern and allows tests
to validate both code paths.
"""

from __future__ import annotations

import importlib
import os
from types import ModuleType


def import_cython_module(cython_module_name: str, fallback_module_name: str) -> ModuleType:
    """Try to import a Cython extension module, falling back to pure Python.

    Args:
        cython_module_name: Fully-qualified name of the compiled Cython module
            (e.g. ``pytecode._internal._bytes_utils_cy``).
        fallback_module_name: Fully-qualified name of the pure-Python module
            (e.g. ``pytecode._internal._bytes_utils_py``).

    Returns:
        The Cython module if available and not blocked, otherwise the fallback.
    """
    if os.environ.get("PYTECODE_BLOCK_CYTHON") == "1":
        return importlib.import_module(fallback_module_name)

    try:
        return importlib.import_module(cython_module_name)
    except ImportError:
        return importlib.import_module(fallback_module_name)
