"""Mutable editing models for JVM class files.

This module re-exports from either the Cython-accelerated implementation
or the pure-Python fallback depending on availability and the
``PYTECODE_BLOCK_CYTHON`` environment variable.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pytecode.edit._model_py import (
        ClassModel as ClassModel,
    )
    from pytecode.edit._model_py import (
        CodeModel as CodeModel,
    )
    from pytecode.edit._model_py import (
        FieldModel as FieldModel,
    )
    from pytecode.edit._model_py import (
        MethodModel as MethodModel,
    )
else:
    from pytecode._internal.cython_import import import_cython_module

    _impl = import_cython_module(
        "pytecode.edit._model_cy",
        "pytecode.edit._model_py",
    )

    ClassModel = _impl.ClassModel
    CodeModel = _impl.CodeModel
    FieldModel = _impl.FieldModel
    MethodModel = _impl.MethodModel

__all__ = ["ClassModel", "CodeModel", "FieldModel", "MethodModel"]
