"""Label-based bytecode instruction editing.

This module re-exports from either the Cython-accelerated implementation
or the pure-Python fallback depending on availability and the
``PYTECODE_BLOCK_CYTHON`` environment variable.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pytecode.edit._labels_py import (
        BranchInsn as BranchInsn,
    )
    from pytecode.edit._labels_py import (
        CodeItem as CodeItem,
    )
    from pytecode.edit._labels_py import (
        ExceptionHandler as ExceptionHandler,
    )
    from pytecode.edit._labels_py import (
        Label as Label,
    )
    from pytecode.edit._labels_py import (
        LabelResolution as LabelResolution,
    )
    from pytecode.edit._labels_py import (
        LineNumberEntry as LineNumberEntry,
    )
    from pytecode.edit._labels_py import (
        LocalVariableEntry as LocalVariableEntry,
    )
    from pytecode.edit._labels_py import (
        LocalVariableTypeEntry as LocalVariableTypeEntry,
    )
    from pytecode.edit._labels_py import (
        LookupSwitchInsn as LookupSwitchInsn,
    )
    from pytecode.edit._labels_py import (
        TableSwitchInsn as TableSwitchInsn,
    )
    from pytecode.edit._labels_py import (
        _build_ldc_index_cache as _build_ldc_index_cache,
    )
    from pytecode.edit._labels_py import (
        _resolve_labels_with_cache as _resolve_labels_with_cache,
    )
    from pytecode.edit._labels_py import (
        clone_raw_instruction as clone_raw_instruction,
    )
    from pytecode.edit._labels_py import (
        lower_code as lower_code,
    )
    from pytecode.edit._labels_py import (
        resolve_catch_type as resolve_catch_type,
    )
    from pytecode.edit._labels_py import (
        resolve_labels as resolve_labels,
    )
else:
    from pytecode._internal.cython_import import import_cython_module

    _impl = import_cython_module(
        "pytecode.edit._labels_cy",
        "pytecode.edit._labels_py",
    )

    BranchInsn = _impl.BranchInsn
    ExceptionHandler = _impl.ExceptionHandler
    Label = _impl.Label
    LabelResolution = _impl.LabelResolution
    LineNumberEntry = _impl.LineNumberEntry
    LocalVariableEntry = _impl.LocalVariableEntry
    LocalVariableTypeEntry = _impl.LocalVariableTypeEntry
    LookupSwitchInsn = _impl.LookupSwitchInsn
    TableSwitchInsn = _impl.TableSwitchInsn
    clone_raw_instruction = _impl.clone_raw_instruction
    lower_code = _impl.lower_code
    resolve_catch_type = _impl.resolve_catch_type
    resolve_labels = _impl.resolve_labels
    _build_ldc_index_cache = _impl._build_ldc_index_cache
    _resolve_labels_with_cache = _impl._resolve_labels_with_cache

    # CodeItem is a type alias (not available from Cython build)
    from pytecode.classfile.instructions import InsnInfo

    CodeItem = InsnInfo | Label

__all__ = [
    "BranchInsn",
    "CodeItem",
    "ExceptionHandler",
    "Label",
    "LabelResolution",
    "LineNumberEntry",
    "LocalVariableEntry",
    "LocalVariableTypeEntry",
    "LookupSwitchInsn",
    "TableSwitchInsn",
    "lower_code",
    "resolve_catch_type",
    "resolve_labels",
]
