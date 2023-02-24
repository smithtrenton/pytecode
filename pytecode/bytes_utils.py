from abc import ABC, abstractmethod
from struct import unpack_from


def _read_u4(_bytes, offset=0):
    return unpack_from(">I", _bytes, offset)[0]


def _read_u2(_bytes, offset=0):
    return unpack_from(">H", _bytes, offset)[0]


def _read_u1(_bytes, offset=0):
    return unpack_from(">B", _bytes, offset)[0]


def _read_bytes(_bytes, length, offset=0):
    return unpack_from(">%ds" % length, _bytes, offset)[0]


class BytesType(ABC):
    @abstractmethod
    def read_from_bytes(self, _bytes, offset):
        pass


class U1(BytesType):
    def read_from_bytes(self, _bytes, offset):
        return _read_u1(_bytes, offset=offset), 1


class U2(BytesType):
    def read_from_bytes(self, _bytes, offset):
        return _read_u2(_bytes, offset=offset), 2


class U4(BytesType):
    def read_from_bytes(self, _bytes, offset):
        return _read_u4(_bytes, offset=offset), 4


class Bytes(BytesType):
    def __init__(self, size):
        self.size = size

    def read_from_bytes(self, _bytes, offset):
        return _read_bytes(_bytes, self.size, offset=offset), self.size


class BytesReader:
    def __init__(self, _bytes, offset=0):
        self.bytes = _bytes
        self.offset = offset

    def rewind(self, distance=None):
        if distance is None:
            self.offset = 0
        else:
            self.offset = max(self.offset - distance, 0)

    def read_u1(self):
        res = _read_u1(self.bytes, self.offset)
        self.offset += 1
        return res

    def read_u2(self):
        res = _read_u2(self.bytes, self.offset)
        self.offset += 2
        return res

    def read_u4(self):
        res = _read_u4(self.bytes, self.offset)
        self.offset += 4
        return res

    def read_bytes(self, size):
        res = _read_bytes(self.bytes, size, self.offset)
        self.offset += size
        return res
