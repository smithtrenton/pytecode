"""JVM bytecode instruction operand data model.

This module re-exports from either the Cython-accelerated implementation
(``_instructions_cy``) or the pure-Python fallback (``_instructions_py``)
depending on availability and the ``PYTECODE_BLOCK_CYTHON`` environment
variable.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from pytecode.classfile._instructions_py import __all__ as __all__

if TYPE_CHECKING:
    from pytecode.classfile._instructions_py import *  # noqa: F403
else:
    from pytecode._internal.cython_import import import_cython_module

    _impl = import_cython_module(
        "pytecode.classfile._instructions_cy",
        "pytecode.classfile._instructions_py",
    )
    globals().update({name: getattr(_impl, name) for name in __all__})
