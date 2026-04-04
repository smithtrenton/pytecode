"""JVM class-file attribute data model.

This module re-exports from either the Cython-accelerated implementation
(``_attributes_cy``) or the pure-Python fallback (``_attributes_py``)
depending on availability and the ``PYTECODE_BLOCK_CYTHON`` environment
variable.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from pytecode.classfile._attributes_py import __all__ as __all__

if TYPE_CHECKING:
    from pytecode.classfile._attributes_py import *  # noqa: F403
else:
    from pytecode._internal.cython_import import import_cython_module

    _impl = import_cython_module(
        "pytecode.classfile._attributes_cy",
        "pytecode.classfile._attributes_py",
    )
    globals().update({name: getattr(_impl, name) for name in __all__})
