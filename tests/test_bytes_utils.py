from __future__ import annotations

import struct

import pytest

from pytecode.bytes_utils import (
    I1,
    I2,
    I4,
    U1,
    U2,
    U4,
    Bytes,
    BytesReader,
    _read_bytes,
    _read_i1,
    _read_i2,
    _read_i4,
    _read_u1,
    _read_u2,
    _read_u4,
)

# ---------------------------------------------------------------------------
# Standalone read functions
# ---------------------------------------------------------------------------


def test_read_u1_zero():
    assert _read_u1(b"\x00") == 0


def test_read_u1_max():
    assert _read_u1(b"\xff") == 255


def test_read_u1_with_offset():
    assert _read_u1(b"\x00\x7f", offset=1) == 127


def test_read_i1_positive():
    assert _read_i1(b"\x7f") == 127


def test_read_i1_negative():
    assert _read_i1(b"\x80") == -128


def test_read_i1_minus_one():
    assert _read_i1(b"\xff") == -1


def test_read_u2_zero():
    assert _read_u2(b"\x00\x00") == 0


def test_read_u2_max():
    assert _read_u2(b"\xff\xff") == 65535


def test_read_u2_big_endian():
    assert _read_u2(b"\x00\xff") == 255
    assert _read_u2(b"\x01\x00") == 256


def test_read_i2_positive():
    assert _read_i2(b"\x7f\xff") == 32767


def test_read_i2_negative():
    assert _read_i2(b"\x80\x00") == -32768


def test_read_i2_minus_one():
    assert _read_i2(b"\xff\xff") == -1


def test_read_u4_zero():
    assert _read_u4(b"\x00\x00\x00\x00") == 0


def test_read_u4_max():
    assert _read_u4(b"\xff\xff\xff\xff") == 4294967295


def test_read_i4_positive():
    assert _read_i4(b"\x7f\xff\xff\xff") == 2147483647


def test_read_i4_negative():
    assert _read_i4(b"\x80\x00\x00\x00") == -2147483648


def test_read_i4_minus_one():
    assert _read_i4(b"\xff\xff\xff\xff") == -1


def test_read_bytes_exact():
    assert _read_bytes(b"\x01\x02\x03", 3) == b"\x01\x02\x03"


def test_read_bytes_with_offset():
    assert _read_bytes(b"\x00\x01\x02", 2, offset=1) == b"\x01\x02"


# ---------------------------------------------------------------------------
# BytesReader
# ---------------------------------------------------------------------------


def test_bytesreader_read_u1_advances():
    r = BytesReader(b"\x42")
    assert r.read_u1() == 0x42
    assert r.offset == 1


def test_bytesreader_read_i1_advances():
    r = BytesReader(b"\x80")
    assert r.read_i1() == -128
    assert r.offset == 1


def test_bytesreader_read_u2_advances():
    r = BytesReader(b"\x01\x02")
    assert r.read_u2() == 0x0102
    assert r.offset == 2


def test_bytesreader_read_i2_advances():
    r = BytesReader(b"\xff\x00")
    assert r.read_i2() == -256
    assert r.offset == 2


def test_bytesreader_read_u4_advances():
    r = BytesReader(b"\x00\x00\x01\x00")
    assert r.read_u4() == 256
    assert r.offset == 4


def test_bytesreader_read_i4_advances():
    r = BytesReader(b"\xff\xff\xff\x00")
    assert r.read_i4() == -256
    assert r.offset == 4


def test_bytesreader_read_bytes_advances():
    r = BytesReader(b"\x0a\x0b\x0c")
    assert r.read_bytes(3) == b"\x0a\x0b\x0c"
    assert r.offset == 3


def test_bytesreader_sequential_reads():
    # u1 + u2 + u1 from a 4-byte buffer: 0xAA, 0xBB, 0xCC, 0xDD
    r = BytesReader(b"\xaa\xbb\xcc\xdd")
    assert r.read_u1() == 0xAA
    assert r.read_u2() == 0xBBCC
    assert r.read_u1() == 0xDD


def test_bytesreader_initial_offset():
    r = BytesReader(b"\x00\x00\x42", offset=2)
    assert r.read_u1() == 0x42


def test_bytesreader_rewind_full():
    r = BytesReader(b"\x01\x02\x03")
    r.read_u1()
    r.read_u1()
    r.rewind()
    assert r.offset == 0


def test_bytesreader_rewind_partial():
    r = BytesReader(b"\x01\x02\x03\x04")
    r.read_u4()
    r.rewind(2)
    assert r.offset == 2


def test_bytesreader_rewind_floor():
    r = BytesReader(b"\x01\x02\x03")
    r.read_u1()
    r.read_u1()
    r.read_u1()
    assert r.offset == 3
    r.rewind(1000)
    assert r.offset == 0


def test_bytesreader_buffer_overrun():
    r = BytesReader(b"\x01")
    with pytest.raises(struct.error):
        r.read_u2()


def test_bytesreader_accepts_bytearray():
    r = BytesReader(bytearray(b"\x12\x34"))
    assert r.read_u2() == 0x1234


# ---------------------------------------------------------------------------
# ByteParser subclasses
# ---------------------------------------------------------------------------


def test_u1_parser():
    assert U1().parse(b"\xab", 0) == (0xAB, 1)


def test_i1_parser():
    assert I1().parse(b"\x80", 0) == (-128, 1)


def test_u2_parser():
    assert U2().parse(b"\x01\x00", 0) == (256, 2)


def test_i2_parser():
    assert I2().parse(b"\xff\x00", 0) == (-256, 2)


def test_u4_parser():
    assert U4().parse(b"\x00\x00\x01\x00", 0) == (256, 4)


def test_i4_parser():
    assert I4().parse(b"\xff\xff\xff\x00", 0) == (-256, 4)


def test_bytes_parser():
    assert Bytes(3).parse(b"\x01\x02\x03", 0) == (b"\x01\x02\x03", 3)


def test_bytes_parser_with_offset():
    assert Bytes(2).parse(b"\x00\x01\x02", 1) == (b"\x01\x02", 2)


def test_bytesreader_read_byte_parser():
    r = BytesReader(b"\x7f")
    assert r.read_byte_parser(U1()) == (127, 1)
    assert r.offset == 0  # read_byte_parser does NOT advance offset
