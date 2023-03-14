from abc import ABC, abstractmethod
from struct import Struct, unpack_from

BE_U1 = Struct(">B")
BE_I1 = Struct(">b")
BE_U2 = Struct(">H")
BE_I2 = Struct(">h")
BE_U4 = Struct(">I")
BE_I4 = Struct(">i")


def _read_u1(buffer, offset=0):
    return BE_U1.unpack_from(buffer, offset)[0]


def _read_i1(buffer, offset=0):
    return BE_I1.unpack_from(buffer, offset)[0]


def _read_u2(buffer, offset=0):
    return BE_U2.unpack_from(buffer, offset)[0]


def _read_i2(buffer, offset=0):
    return BE_I2.unpack_from(buffer, offset)[0]


def _read_u4(buffer, offset=0):
    return BE_U4.unpack_from(buffer, offset)[0]


def _read_i4(buffer, offset=0):
    return BE_I4.unpack_from(buffer, offset)[0]


def _read_bytes(buffer, length, offset=0):
    return unpack_from(">%ds" % length, buffer, offset)[0]


class ByteParser(ABC):
    @abstractmethod
    def parse(self, buffer, offset):
        pass


class U1(ByteParser):
    def parse(self, buffer, offset):
        return _read_u1(buffer, offset=offset), 1


class I1(ByteParser):
    def parse(self, buffer, offset):
        return _read_i1(buffer, offset=offset), 1


class U2(ByteParser):
    def parse(self, buffer, offset):
        return _read_u2(buffer, offset=offset), 2


class I2(ByteParser):
    def parse(self, buffer, offset):
        return _read_i2(buffer, offset=offset), 2


class U4(ByteParser):
    def parse(self, buffer, offset):
        return _read_u4(buffer, offset=offset), 4


class I4(ByteParser):
    def parse(self, buffer, offset):
        return _read_i4(buffer, offset=offset), 4


class Bytes(ByteParser):
    def __init__(self, size):
        self.size = size

    def parse(self, buffer, offset):
        return _read_bytes(buffer, self.size, offset=offset), self.size


class BytesReader:
    def __init__(self, bytes_or_bytearray, offset=0):
        self.buffer = memoryview(bytes_or_bytearray)
        self.offset = offset

    def rewind(self, distance=None):
        if distance is None:
            self.offset = 0
        else:
            self.offset = max(self.offset - distance, 0)

    def read_u1(self):
        res = _read_u1(self.buffer, self.offset)
        self.offset += 1
        return res

    def read_i1(self):
        res = _read_i1(self.buffer, self.offset)
        self.offset += 1
        return res

    def read_u2(self):
        res = _read_u2(self.buffer, self.offset)
        self.offset += 2
        return res

    def read_i2(self):
        res = _read_i2(self.buffer, self.offset)
        self.offset += 2
        return res

    def read_u4(self):
        res = _read_u4(self.buffer, self.offset)
        self.offset += 4
        return res

    def read_i4(self):
        res = _read_i4(self.buffer, self.offset)
        self.offset += 4
        return res

    def read_bytes(self, size):
        res = _read_bytes(self.buffer, size, self.offset)
        self.offset += size
        return res

    def read_byte_parser(self, byte_parser):
        return byte_parser.parse(self.buffer, self.offset)
