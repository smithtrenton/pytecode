from __future__ import annotations

from collections.abc import Buffer
from struct import Struct, pack, unpack_from

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


class BytesReader:
    def __init__(self, bytes_or_bytearray: bytes | bytearray, offset: int = 0) -> None:
        self.buffer: memoryview = memoryview(bytes_or_bytearray)
        self.offset: int = offset

    def rewind(self, distance: int | None = None) -> None:
        if distance is None:
            self.offset = 0
        else:
            self.offset = max(self.offset - distance, 0)

    def read_u1(self) -> int:
        res = _read_u1(self.buffer, self.offset)
        self.offset += 1
        return res

    def read_i1(self) -> int:
        res = _read_i1(self.buffer, self.offset)
        self.offset += 1
        return res

    def read_u2(self) -> int:
        res = _read_u2(self.buffer, self.offset)
        self.offset += 2
        return res

    def read_i2(self) -> int:
        res = _read_i2(self.buffer, self.offset)
        self.offset += 2
        return res

    def read_u4(self) -> int:
        res = _read_u4(self.buffer, self.offset)
        self.offset += 4
        return res

    def read_i4(self) -> int:
        res = _read_i4(self.buffer, self.offset)
        self.offset += 4
        return res

    def read_bytes(self, size: int) -> bytes:
        res = _read_bytes(self.buffer, size, self.offset)
        self.offset += size
        return res


# ---------------------------------------------------------------------------
# Write side
# ---------------------------------------------------------------------------


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


class BytesWriter:
    def __init__(self) -> None:
        self._buf: bytearray = bytearray()

    @property
    def position(self) -> int:
        return len(self._buf)

    def __len__(self) -> int:
        return len(self._buf)

    def to_bytes(self) -> bytes:
        return bytes(self._buf)

    def write_u1(self, value: int) -> None:
        self._buf += _write_u1(value)

    def write_i1(self, value: int) -> None:
        self._buf += _write_i1(value)

    def write_u2(self, value: int) -> None:
        self._buf += _write_u2(value)

    def write_i2(self, value: int) -> None:
        self._buf += _write_i2(value)

    def write_u4(self, value: int) -> None:
        self._buf += _write_u4(value)

    def write_i4(self, value: int) -> None:
        self._buf += _write_i4(value)

    def write_bytes(self, data: bytes | bytearray) -> None:
        self._buf += _write_bytes(data)

    def align(self, alignment: int) -> None:
        remainder = len(self._buf) % alignment
        if remainder != 0:
            self._buf += bytes(alignment - remainder)

    def reserve_u1(self) -> int:
        pos = len(self._buf)
        self._buf += b"\x00"
        return pos

    def reserve_i1(self) -> int:
        pos = len(self._buf)
        self._buf += b"\x00"
        return pos

    def reserve_u2(self) -> int:
        pos = len(self._buf)
        self._buf += b"\x00\x00"
        return pos

    def reserve_i2(self) -> int:
        pos = len(self._buf)
        self._buf += b"\x00\x00"
        return pos

    def reserve_u4(self) -> int:
        pos = len(self._buf)
        self._buf += b"\x00\x00\x00\x00"
        return pos

    def reserve_i4(self) -> int:
        pos = len(self._buf)
        self._buf += b"\x00\x00\x00\x00"
        return pos

    def patch_u1(self, position: int, value: int) -> None:
        self._buf[position : position + 1] = BE_U1.pack(value)

    def patch_i1(self, position: int, value: int) -> None:
        self._buf[position : position + 1] = BE_I1.pack(value)

    def patch_u2(self, position: int, value: int) -> None:
        self._buf[position : position + 2] = BE_U2.pack(value)

    def patch_i2(self, position: int, value: int) -> None:
        self._buf[position : position + 2] = BE_I2.pack(value)

    def patch_u4(self, position: int, value: int) -> None:
        self._buf[position : position + 4] = BE_U4.pack(value)

    def patch_i4(self, position: int, value: int) -> None:
        self._buf[position : position + 4] = BE_I4.pack(value)
