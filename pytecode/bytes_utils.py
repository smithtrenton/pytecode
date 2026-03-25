from __future__ import annotations

from abc import ABC, abstractmethod
from collections.abc import Buffer
from struct import Struct, unpack_from

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


class ByteParser(ABC):
    @abstractmethod
    def parse(self, buffer: Buffer, offset: int) -> tuple[int | bytes, int]:
        pass


class U1(ByteParser):
    def parse(self, buffer: Buffer, offset: int) -> tuple[int, int]:
        return _read_u1(buffer, offset=offset), 1


class I1(ByteParser):
    def parse(self, buffer: Buffer, offset: int) -> tuple[int, int]:
        return _read_i1(buffer, offset=offset), 1


class U2(ByteParser):
    def parse(self, buffer: Buffer, offset: int) -> tuple[int, int]:
        return _read_u2(buffer, offset=offset), 2


class I2(ByteParser):
    def parse(self, buffer: Buffer, offset: int) -> tuple[int, int]:
        return _read_i2(buffer, offset=offset), 2


class U4(ByteParser):
    def parse(self, buffer: Buffer, offset: int) -> tuple[int, int]:
        return _read_u4(buffer, offset=offset), 4


class I4(ByteParser):
    def parse(self, buffer: Buffer, offset: int) -> tuple[int, int]:
        return _read_i4(buffer, offset=offset), 4


class Bytes(ByteParser):
    def __init__(self, size: int) -> None:
        self.size = size

    def parse(self, buffer: Buffer, offset: int) -> tuple[bytes, int]:
        return _read_bytes(buffer, self.size, offset=offset), self.size


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

    def read_byte_parser(self, byte_parser: ByteParser) -> tuple[int | bytes, int]:
        return byte_parser.parse(self.buffer, self.offset)
