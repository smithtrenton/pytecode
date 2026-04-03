"""Big-endian binary I/O primitives for JVM classfile parsing.

This module re-exports from either the Cython-accelerated implementation
(``_bytes_utils_cy``) or the pure-Python fallback (``_bytes_utils_py``)
depending on availability and the ``PYTECODE_BLOCK_CYTHON`` environment
variable.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pytecode._internal._bytes_utils_py import (
        BytesReader as BytesReader,
    )
    from pytecode._internal._bytes_utils_py import (
        BytesWriter as BytesWriter,
    )
    from pytecode._internal._bytes_utils_py import (
        _read_bytes as _read_bytes,
    )
    from pytecode._internal._bytes_utils_py import (
        _read_i1 as _read_i1,
    )
    from pytecode._internal._bytes_utils_py import (
        _read_i2 as _read_i2,
    )
    from pytecode._internal._bytes_utils_py import (
        _read_i4 as _read_i4,
    )
    from pytecode._internal._bytes_utils_py import (
        _read_u1 as _read_u1,
    )
    from pytecode._internal._bytes_utils_py import (
        _read_u2 as _read_u2,
    )
    from pytecode._internal._bytes_utils_py import (
        _read_u4 as _read_u4,
    )
    from pytecode._internal._bytes_utils_py import (
        _write_bytes as _write_bytes,
    )
    from pytecode._internal._bytes_utils_py import (
        _write_i1 as _write_i1,
    )
    from pytecode._internal._bytes_utils_py import (
        _write_i2 as _write_i2,
    )
    from pytecode._internal._bytes_utils_py import (
        _write_i4 as _write_i4,
    )
    from pytecode._internal._bytes_utils_py import (
        _write_u1 as _write_u1,
    )
    from pytecode._internal._bytes_utils_py import (
        _write_u2 as _write_u2,
    )
    from pytecode._internal._bytes_utils_py import (
        _write_u4 as _write_u4,
    )
else:
    from pytecode._internal.cython_import import import_cython_module

    _impl = import_cython_module(
        "pytecode._internal._bytes_utils_cy",
        "pytecode._internal._bytes_utils_py",
    )

    BytesReader = _impl.BytesReader
    BytesWriter = _impl.BytesWriter

    _read_u1 = _impl._read_u1
    _read_i1 = _impl._read_i1
    _read_u2 = _impl._read_u2
    _read_i2 = _impl._read_i2
    _read_u4 = _impl._read_u4
    _read_i4 = _impl._read_i4
    _read_bytes = _impl._read_bytes

    _write_u1 = _impl._write_u1
    _write_i1 = _impl._write_i1
    _write_u2 = _impl._write_u2
    _write_i2 = _impl._write_i2
    _write_u4 = _impl._write_u4
    _write_i4 = _impl._write_i4
    _write_bytes = _impl._write_bytes
