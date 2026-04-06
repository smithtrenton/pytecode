"""Parse JVM ``.class`` file bytes into a :class:`ClassFile` tree.

This module re-exports from either the Cython-accelerated implementation
or the pure-Python fallback depending on availability and the
``PYTECODE_BLOCK_CYTHON`` environment variable.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pytecode.classfile._reader_py import (
        ClassReader as ClassReader,
    )
    from pytecode.classfile._reader_py import (
        MalformedClassException as MalformedClassException,
    )
else:
    from pytecode._internal.cython_import import import_cython_module

    _impl = import_cython_module(
        "pytecode.classfile._reader_cy",
        "pytecode.classfile._reader_py",
    )

    ClassReader = _impl.ClassReader
    MalformedClassException = _impl.MalformedClassException

__all__ = ["ClassReader", "MalformedClassException"]
