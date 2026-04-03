"""Constant-pool builder for JVM class files.

This module re-exports from either the Cython-accelerated implementation
or the pure-Python fallback depending on availability and the
``PYTECODE_BLOCK_CYTHON`` environment variable.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pytecode.edit._constant_pool_builder_py import (
        ConstantPoolBuilder as ConstantPoolBuilder,
    )
else:
    from pytecode._internal.cython_import import import_cython_module

    _impl = import_cython_module(
        "pytecode.edit._constant_pool_builder_cy",
        "pytecode.edit._constant_pool_builder_py",
    )

    ConstantPoolBuilder = _impl.ConstantPoolBuilder

__all__ = ["ConstantPoolBuilder"]
