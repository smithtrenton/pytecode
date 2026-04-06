"""Encode and decode JVM Modified UTF-8 strings (§4.4.7).

This module re-exports from either the Cython-accelerated implementation
or the pure-Python fallback depending on availability and the
``PYTECODE_BLOCK_CYTHON`` environment variable.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pytecode.classfile._modified_utf8_py import (
        decode_modified_utf8 as decode_modified_utf8,
    )
    from pytecode.classfile._modified_utf8_py import (
        encode_modified_utf8 as encode_modified_utf8,
    )
else:
    from pytecode._internal.cython_import import import_cython_module

    _impl = import_cython_module(
        "pytecode.classfile._modified_utf8_cy",
        "pytecode.classfile._modified_utf8_py",
    )

    encode_modified_utf8 = _impl.encode_modified_utf8
    decode_modified_utf8 = _impl.decode_modified_utf8

__all__ = ["decode_modified_utf8", "encode_modified_utf8"]
