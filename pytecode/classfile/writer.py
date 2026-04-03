"""Serialize a ClassFile tree into JVM ``.class`` file bytes (JVMS §4).

This module re-exports from either the Cython-accelerated implementation
or the pure-Python fallback depending on availability and the
``PYTECODE_BLOCK_CYTHON`` environment variable.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pytecode.classfile._writer_py import ClassWriter as ClassWriter
else:
    from pytecode._internal.cython_import import import_cython_module

    _impl = import_cython_module(
        "pytecode.classfile._writer_cy",
        "pytecode.classfile._writer_py",
    )

    ClassWriter = _impl.ClassWriter

__all__ = ["ClassWriter"]
