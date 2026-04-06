"""Utilities for managing JVM debug information in class files.

This module re-exports from either the Cython-accelerated implementation
or the pure-Python fallback depending on availability and the
``PYTECODE_BLOCK_CYTHON`` environment variable.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pytecode.edit._debug_info_py import (
        DebugInfoPolicy as DebugInfoPolicy,
    )
    from pytecode.edit._debug_info_py import (
        DebugInfoState as DebugInfoState,
    )
    from pytecode.edit._debug_info_py import (
        apply_debug_info_policy as apply_debug_info_policy,
    )
    from pytecode.edit._debug_info_py import (
        has_class_debug_info as has_class_debug_info,
    )
    from pytecode.edit._debug_info_py import (
        has_code_debug_info as has_code_debug_info,
    )
    from pytecode.edit._debug_info_py import (
        is_class_debug_info_stale as is_class_debug_info_stale,
    )
    from pytecode.edit._debug_info_py import (
        is_code_debug_info_stale as is_code_debug_info_stale,
    )
    from pytecode.edit._debug_info_py import (
        mark_class_debug_info_stale as mark_class_debug_info_stale,
    )
    from pytecode.edit._debug_info_py import (
        mark_code_debug_info_stale as mark_code_debug_info_stale,
    )
    from pytecode.edit._debug_info_py import (
        normalize_debug_info_policy as normalize_debug_info_policy,
    )
    from pytecode.edit._debug_info_py import (
        skip_debug_method_attributes as skip_debug_method_attributes,
    )
    from pytecode.edit._debug_info_py import (
        strip_class_debug_attributes as strip_class_debug_attributes,
    )
    from pytecode.edit._debug_info_py import (
        strip_debug_info as strip_debug_info,
    )
else:
    from pytecode._internal.cython_import import import_cython_module

    _impl = import_cython_module(
        "pytecode.edit._debug_info_cy",
        "pytecode.edit._debug_info_py",
    )

    DebugInfoPolicy = _impl.DebugInfoPolicy
    DebugInfoState = _impl.DebugInfoState
    apply_debug_info_policy = _impl.apply_debug_info_policy
    has_class_debug_info = _impl.has_class_debug_info
    has_code_debug_info = _impl.has_code_debug_info
    is_class_debug_info_stale = _impl.is_class_debug_info_stale
    is_code_debug_info_stale = _impl.is_code_debug_info_stale
    mark_class_debug_info_stale = _impl.mark_class_debug_info_stale
    mark_code_debug_info_stale = _impl.mark_code_debug_info_stale
    normalize_debug_info_policy = _impl.normalize_debug_info_policy
    skip_debug_method_attributes = _impl.skip_debug_method_attributes
    strip_class_debug_attributes = _impl.strip_class_debug_attributes
    strip_debug_info = _impl.strip_debug_info

__all__ = [
    "DebugInfoPolicy",
    "DebugInfoState",
    "apply_debug_info_policy",
    "has_class_debug_info",
    "has_code_debug_info",
    "is_class_debug_info_stale",
    "is_code_debug_info_stale",
    "mark_class_debug_info_stale",
    "mark_code_debug_info_stale",
    "normalize_debug_info_policy",
    "skip_debug_method_attributes",
    "strip_class_debug_attributes",
    "strip_debug_info",
]
