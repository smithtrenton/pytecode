from __future__ import annotations

from collections.abc import Buffer
from struct import Struct, pack, unpack_from
from struct import error as StructError
from typing import Any

from .rust_import import import_optional_rust_module

BE_U1 = Struct(">B")
BE_I1 = Struct(">b")
BE_U2 = Struct(">H")
BE_I2 = Struct(">h")
BE_U4 = Struct(">I")
BE_I4 = Struct(">i")


def _read_u1(buffer: Buffer, offset: int = 0) -> int:
    return BE_U1.unpack_from(buffer, offset)[0]


def _read_i1(buffer: Buffer, offset: int = 0) -> int:
    return BE_I1.unpack_from(buffer, offset)[0]


def _read_u2(buffer: Buffer, offset: int = 0) -> int:
    return BE_U2.unpack_from(buffer, offset)[0]


def _read_i2(buffer: Buffer, offset: int = 0) -> int:
    return BE_I2.unpack_from(buffer, offset)[0]


def _read_u4(buffer: Buffer, offset: int = 0) -> int:
    return BE_U4.unpack_from(buffer, offset)[0]


def _read_i4(buffer: Buffer, offset: int = 0) -> int:
    return BE_I4.unpack_from(buffer, offset)[0]


def _read_bytes(buffer: Buffer, length: int, offset: int = 0) -> bytes:
    return unpack_from(f">{length}s", buffer, offset)[0]


def _write_u1(value: int) -> bytes:
    return BE_U1.pack(value)


def _write_i1(value: int) -> bytes:
    return BE_I1.pack(value)


def _write_u2(value: int) -> bytes:
    return BE_U2.pack(value)


def _write_i2(value: int) -> bytes:
    return BE_I2.pack(value)


def _write_u4(value: int) -> bytes:
    return BE_U4.pack(value)


def _write_i4(value: int) -> bytes:
    return BE_I4.pack(value)


def _write_bytes(data: bytes | bytearray) -> bytes:
    return pack(f">{len(data)}s", data)


def _packed_i1_as_u1(value: int) -> int:
    return _write_i1(value)[0]


def _packed_i2_as_u2(value: int) -> int:
    return int.from_bytes(_write_i2(value), "big", signed=False)


def _packed_i4_as_u4(value: int) -> int:
    return int.from_bytes(_write_i4(value), "big", signed=False)


try:
    _rust_binary_io = import_optional_rust_module("pytecode._rust.binary_io")
except ModuleNotFoundError:
    _RustBytesReader: Any | None = None
    _RustBytesWriter: Any | None = None
else:
    _RustBytesReader = _rust_binary_io.RustBytesReader
    _RustBytesWriter = _rust_binary_io.RustBytesWriter

_RUST_BYTES_UTILS_AVAILABLE = _RustBytesReader is not None and _RustBytesWriter is not None


class BytesReader:
    def __init__(self, bytes_or_bytearray: bytes | bytearray, offset: int = 0) -> None:
        self.buffer: memoryview = memoryview(bytes_or_bytearray)
        self._use_rust = _RUST_BYTES_UTILS_AVAILABLE
        self._offset = offset
        if self._use_rust:
            rust_reader_cls = _RustBytesReader
            assert rust_reader_cls is not None
            self._rust = rust_reader_cls(bytes_or_bytearray, offset)

    @property
    def offset(self) -> int:
        if self._use_rust:
            return int(self._rust.offset)
        return self._offset

    @offset.setter
    def offset(self, value: int) -> None:
        if self._use_rust:
            self._rust.offset = value
        self._offset = value

    def _call_rust(self, method_name: str, *args: Any) -> Any:
        try:
            return getattr(self._rust, method_name)(*args)
        except (OverflowError, ValueError) as exc:
            raise StructError(str(exc)) from exc

    def rewind(self, distance: int | None = None) -> None:
        if self._use_rust:
            self._rust.rewind(distance)
            return
        if distance is None:
            self._offset = 0
        else:
            self._offset = max(self._offset - distance, 0)

    def read_u1(self) -> int:
        if self._use_rust:
            return int(self._call_rust("read_u1"))
        res = _read_u1(self.buffer, self._offset)
        self._offset += 1
        return res

    def read_i1(self) -> int:
        if self._use_rust:
            return int(self._call_rust("read_i1"))
        res = _read_i1(self.buffer, self._offset)
        self._offset += 1
        return res

    def read_u2(self) -> int:
        if self._use_rust:
            return int(self._call_rust("read_u2"))
        res = _read_u2(self.buffer, self._offset)
        self._offset += 2
        return res

    def read_i2(self) -> int:
        if self._use_rust:
            return int(self._call_rust("read_i2"))
        res = _read_i2(self.buffer, self._offset)
        self._offset += 2
        return res

    def read_u4(self) -> int:
        if self._use_rust:
            return int(self._call_rust("read_u4"))
        res = _read_u4(self.buffer, self._offset)
        self._offset += 4
        return res

    def read_i4(self) -> int:
        if self._use_rust:
            return int(self._call_rust("read_i4"))
        res = _read_i4(self.buffer, self._offset)
        self._offset += 4
        return res

    def read_bytes(self, size: int) -> bytes:
        if self._use_rust:
            payload = self._call_rust("read_bytes", size)
            return payload if isinstance(payload, bytes) else bytes(payload)
        res = _read_bytes(self.buffer, size, self._offset)
        self._offset += size
        return res


class BytesWriter:
    def __init__(self) -> None:
        self._use_rust = _RUST_BYTES_UTILS_AVAILABLE
        if self._use_rust:
            rust_writer_cls = _RustBytesWriter
            assert rust_writer_cls is not None
            self._rust = rust_writer_cls()
        else:
            self._buf: bytearray = bytearray()

    def _call_rust(self, method_name: str, *args: Any) -> Any:
        try:
            return getattr(self._rust, method_name)(*args)
        except (OverflowError, ValueError) as exc:
            raise StructError(str(exc)) from exc

    @property
    def position(self) -> int:
        if self._use_rust:
            return int(self._rust.position)
        return len(self._buf)

    def __len__(self) -> int:
        if self._use_rust:
            return int(self._rust.position)
        return len(self._buf)

    def to_bytes(self) -> bytes:
        if self._use_rust:
            payload = self._rust.to_bytes()
            return payload if isinstance(payload, bytes) else bytes(payload)
        return bytes(self._buf)

    def write_u1(self, value: int) -> None:
        if self._use_rust:
            self._call_rust("write_u1", value)
            return
        self._buf += _write_u1(value)

    def write_i1(self, value: int) -> None:
        if self._use_rust:
            self._call_rust("write_i1", value)
            return
        self._buf += _write_i1(value)

    def write_u2(self, value: int) -> None:
        if self._use_rust:
            self._call_rust("write_u2", value)
            return
        self._buf += _write_u2(value)

    def write_i2(self, value: int) -> None:
        if self._use_rust:
            self._call_rust("write_i2", value)
            return
        self._buf += _write_i2(value)

    def write_u4(self, value: int) -> None:
        if self._use_rust:
            self._call_rust("write_u4", value)
            return
        self._buf += _write_u4(value)

    def write_i4(self, value: int) -> None:
        if self._use_rust:
            self._call_rust("write_i4", value)
            return
        self._buf += _write_i4(value)

    def write_bytes(self, data: bytes | bytearray) -> None:
        if self._use_rust:
            self._rust.write_bytes(bytes(data))
            return
        self._buf += _write_bytes(data)

    def align(self, alignment: int) -> None:
        if self._use_rust:
            self._rust.align(alignment)
            return
        remainder = len(self._buf) % alignment
        if remainder != 0:
            self._buf += bytes(alignment - remainder)

    def reserve_u1(self) -> int:
        if self._use_rust:
            return int(self._rust.reserve_u1())
        pos = len(self._buf)
        self._buf += b"\x00"
        return pos

    def reserve_i1(self) -> int:
        return self.reserve_u1()

    def reserve_u2(self) -> int:
        if self._use_rust:
            return int(self._rust.reserve_u2())
        pos = len(self._buf)
        self._buf += b"\x00\x00"
        return pos

    def reserve_i2(self) -> int:
        return self.reserve_u2()

    def reserve_u4(self) -> int:
        if self._use_rust:
            return int(self._rust.reserve_u4())
        pos = len(self._buf)
        self._buf += b"\x00\x00\x00\x00"
        return pos

    def reserve_i4(self) -> int:
        return self.reserve_u4()

    def patch_u1(self, position: int, value: int) -> None:
        if self._use_rust:
            self._call_rust("patch_u1", position, value)
            return
        self._buf[position : position + 1] = BE_U1.pack(value)

    def patch_i1(self, position: int, value: int) -> None:
        if self._use_rust:
            self._call_rust("patch_u1", position, _packed_i1_as_u1(value))
            return
        self._buf[position : position + 1] = BE_I1.pack(value)

    def patch_u2(self, position: int, value: int) -> None:
        if self._use_rust:
            self._call_rust("patch_u2", position, value)
            return
        self._buf[position : position + 2] = BE_U2.pack(value)

    def patch_i2(self, position: int, value: int) -> None:
        if self._use_rust:
            self._call_rust("patch_u2", position, _packed_i2_as_u2(value))
            return
        self._buf[position : position + 2] = BE_I2.pack(value)

    def patch_u4(self, position: int, value: int) -> None:
        if self._use_rust:
            self._call_rust("patch_u4", position, value)
            return
        self._buf[position : position + 4] = BE_U4.pack(value)

    def patch_i4(self, position: int, value: int) -> None:
        if self._use_rust:
            self._call_rust("patch_u4", position, _packed_i4_as_u4(value))
            return
        self._buf[position : position + 4] = BE_I4.pack(value)
