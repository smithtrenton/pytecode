"""Internal helpers for cloning attribute dataclass trees.

This module re-exports from either the Cython-accelerated implementation
(``_attribute_clone_cy``) or the pure-Python fallback (``_attribute_clone_py``)
depending on availability and the ``PYTECODE_BLOCK_CYTHON`` environment
variable.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pytecode.edit._attribute_clone_py import (
        clone_attribute as clone_attribute,
    )
    from pytecode.edit._attribute_clone_py import (
        clone_attributes as clone_attributes,
    )
else:
    from pytecode._internal.cython_import import import_cython_module

    _impl = import_cython_module(
        "pytecode.edit._attribute_clone_cy",
        "pytecode.edit._attribute_clone_py",
    )

    clone_attribute = _impl.clone_attribute
    clone_attributes = _impl.clone_attributes
