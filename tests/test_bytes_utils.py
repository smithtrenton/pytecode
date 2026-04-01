from __future__ import annotations

import struct

import pytest

from pytecode._internal.bytes_utils import (
    BytesReader,
    BytesWriter,
    _read_bytes,
    _read_i1,
    _read_i2,
    _read_i4,
    _read_u1,
    _read_u2,
    _read_u4,
    _write_bytes,
    _write_i1,
    _write_i2,
    _write_i4,
    _write_u1,
    _write_u2,
    _write_u4,
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
# Standalone write functions
# ---------------------------------------------------------------------------


def test_write_u1_zero():
    assert _write_u1(0) == b"\x00"


def test_write_u1_max():
    assert _write_u1(255) == b"\xff"


def test_write_u1_mid():
    assert _write_u1(127) == b"\x7f"


def test_write_i1_positive():
    assert _write_i1(127) == b"\x7f"


def test_write_i1_negative():
    assert _write_i1(-128) == b"\x80"


def test_write_i1_minus_one():
    assert _write_i1(-1) == b"\xff"


def test_write_u2_zero():
    assert _write_u2(0) == b"\x00\x00"


def test_write_u2_max():
    assert _write_u2(65535) == b"\xff\xff"


def test_write_u2_big_endian():
    assert _write_u2(255) == b"\x00\xff"
    assert _write_u2(256) == b"\x01\x00"


def test_write_i2_positive():
    assert _write_i2(32767) == b"\x7f\xff"


def test_write_i2_negative():
    assert _write_i2(-32768) == b"\x80\x00"


def test_write_i2_minus_one():
    assert _write_i2(-1) == b"\xff\xff"


def test_write_u4_zero():
    assert _write_u4(0) == b"\x00\x00\x00\x00"


def test_write_u4_max():
    assert _write_u4(4294967295) == b"\xff\xff\xff\xff"


def test_write_i4_positive():
    assert _write_i4(2147483647) == b"\x7f\xff\xff\xff"


def test_write_i4_negative():
    assert _write_i4(-2147483648) == b"\x80\x00\x00\x00"


def test_write_i4_minus_one():
    assert _write_i4(-1) == b"\xff\xff\xff\xff"


def test_write_bytes_exact():
    assert _write_bytes(b"\x01\x02\x03") == b"\x01\x02\x03"


def test_write_bytes_empty():
    assert _write_bytes(b"") == b""


def test_write_bytes_bytearray():
    assert _write_bytes(bytearray(b"\xaa\xbb")) == b"\xaa\xbb"


def test_write_u1_out_of_range():
    with pytest.raises(struct.error):
        _write_u1(256)


def test_write_i1_out_of_range():
    with pytest.raises(struct.error):
        _write_i1(128)


# ---------------------------------------------------------------------------
# Round-trip: write then read back
# ---------------------------------------------------------------------------


def test_roundtrip_u1():
    assert _read_u1(_write_u1(200)) == 200


def test_roundtrip_i1():
    assert _read_i1(_write_i1(-50)) == -50


def test_roundtrip_u2():
    assert _read_u2(_write_u2(1000)) == 1000


def test_roundtrip_i2():
    assert _read_i2(_write_i2(-1000)) == -1000


def test_roundtrip_u4():
    assert _read_u4(_write_u4(100000)) == 100000


def test_roundtrip_i4():
    assert _read_i4(_write_i4(-100000)) == -100000


def test_roundtrip_bytes():
    data = b"\xde\xad\xbe\xef"
    assert _read_bytes(_write_bytes(data), len(data)) == data


# ---------------------------------------------------------------------------
# BytesWriter: basic writes
# ---------------------------------------------------------------------------


def test_byteswriter_initial_state():
    w = BytesWriter()
    assert len(w) == 0
    assert w.position == 0
    assert w.to_bytes() == b""


def test_byteswriter_write_u1():
    w = BytesWriter()
    w.write_u1(0x42)
    assert w.to_bytes() == b"\x42"
    assert len(w) == 1


def test_byteswriter_write_i1():
    w = BytesWriter()
    w.write_i1(-1)
    assert w.to_bytes() == b"\xff"
    assert len(w) == 1


def test_byteswriter_write_u2():
    w = BytesWriter()
    w.write_u2(0x0102)
    assert w.to_bytes() == b"\x01\x02"
    assert len(w) == 2


def test_byteswriter_write_i2():
    w = BytesWriter()
    w.write_i2(-256)
    assert w.to_bytes() == b"\xff\x00"
    assert len(w) == 2


def test_byteswriter_write_u4():
    w = BytesWriter()
    w.write_u4(256)
    assert w.to_bytes() == b"\x00\x00\x01\x00"
    assert len(w) == 4


def test_byteswriter_write_i4():
    w = BytesWriter()
    w.write_i4(-256)
    assert w.to_bytes() == b"\xff\xff\xff\x00"
    assert len(w) == 4


def test_byteswriter_write_bytes():
    w = BytesWriter()
    w.write_bytes(b"\x0a\x0b\x0c")
    assert w.to_bytes() == b"\x0a\x0b\x0c"
    assert len(w) == 3


def test_byteswriter_write_bytes_bytearray():
    w = BytesWriter()
    w.write_bytes(bytearray(b"\x12\x34"))
    assert w.to_bytes() == b"\x12\x34"


def test_byteswriter_sequential_writes():
    w = BytesWriter()
    w.write_u1(0xAA)
    w.write_u2(0xBBCC)
    w.write_u1(0xDD)
    assert w.to_bytes() == b"\xaa\xbb\xcc\xdd"
    assert len(w) == 4


def test_byteswriter_position_advances():
    w = BytesWriter()
    assert w.position == 0
    w.write_u1(0)
    assert w.position == 1
    w.write_u4(0)
    assert w.position == 5


# ---------------------------------------------------------------------------
# BytesWriter: align
# ---------------------------------------------------------------------------


def test_byteswriter_align_already_aligned():
    w = BytesWriter()
    w.write_u4(0)  # 4 bytes — already aligned to 4
    w.align(4)
    assert len(w) == 4


def test_byteswriter_align_adds_padding():
    w = BytesWriter()
    w.write_u1(0)  # 1 byte — needs 3 padding bytes to reach alignment of 4
    w.align(4)
    assert len(w) == 4
    assert w.to_bytes() == b"\x00\x00\x00\x00"


def test_byteswriter_align_two_bytes():
    w = BytesWriter()
    w.write_u2(0)  # 2 bytes — needs 2 more to align to 4
    w.align(4)
    assert len(w) == 4


def test_byteswriter_align_three_bytes():
    w = BytesWriter()
    w.write_u2(0)
    w.write_u1(0)  # 3 bytes — needs 1 more to align to 4
    w.align(4)
    assert len(w) == 4


def test_byteswriter_align_pads_with_zeros():
    w = BytesWriter()
    w.write_u1(0xFF)
    w.align(4)
    assert w.to_bytes() == b"\xff\x00\x00\x00"


def test_byteswriter_align_empty():
    w = BytesWriter()
    w.align(4)
    assert len(w) == 0


# ---------------------------------------------------------------------------
# BytesWriter: reserve and patch
# ---------------------------------------------------------------------------


def test_byteswriter_reserve_u1():
    w = BytesWriter()
    pos = w.reserve_u1()
    assert pos == 0
    assert len(w) == 1
    assert w.to_bytes() == b"\x00"


def test_byteswriter_reserve_i1():
    w = BytesWriter()
    pos = w.reserve_i1()
    assert pos == 0
    assert len(w) == 1


def test_byteswriter_reserve_u2():
    w = BytesWriter()
    pos = w.reserve_u2()
    assert pos == 0
    assert len(w) == 2
    assert w.to_bytes() == b"\x00\x00"


def test_byteswriter_reserve_i2():
    w = BytesWriter()
    pos = w.reserve_i2()
    assert pos == 0
    assert len(w) == 2


def test_byteswriter_reserve_u4():
    w = BytesWriter()
    pos = w.reserve_u4()
    assert pos == 0
    assert len(w) == 4
    assert w.to_bytes() == b"\x00\x00\x00\x00"


def test_byteswriter_reserve_i4():
    w = BytesWriter()
    pos = w.reserve_i4()
    assert pos == 0
    assert len(w) == 4


def test_byteswriter_reserve_returns_correct_position():
    w = BytesWriter()
    w.write_u1(0xAA)
    pos = w.reserve_u2()
    assert pos == 1
    w.write_u1(0xBB)
    assert len(w) == 4


def test_byteswriter_patch_u1():
    w = BytesWriter()
    pos = w.reserve_u1()
    w.patch_u1(pos, 0x42)
    assert w.to_bytes() == b"\x42"


def test_byteswriter_patch_i1():
    w = BytesWriter()
    pos = w.reserve_i1()
    w.patch_i1(pos, -1)
    assert w.to_bytes() == b"\xff"


def test_byteswriter_patch_u2():
    w = BytesWriter()
    pos = w.reserve_u2()
    w.patch_u2(pos, 0x0102)
    assert w.to_bytes() == b"\x01\x02"


def test_byteswriter_patch_i2():
    w = BytesWriter()
    pos = w.reserve_i2()
    w.patch_i2(pos, -1)
    assert w.to_bytes() == b"\xff\xff"


def test_byteswriter_patch_u4():
    w = BytesWriter()
    pos = w.reserve_u4()
    w.patch_u4(pos, 0x01020304)
    assert w.to_bytes() == b"\x01\x02\x03\x04"


def test_byteswriter_patch_i4():
    w = BytesWriter()
    pos = w.reserve_i4()
    w.patch_i4(pos, -1)
    assert w.to_bytes() == b"\xff\xff\xff\xff"


def test_byteswriter_patch_does_not_change_length():
    w = BytesWriter()
    w.write_u1(0xAA)
    pos = w.reserve_u2()
    w.write_u1(0xBB)
    assert len(w) == 4
    w.patch_u2(pos, 0x1234)
    assert len(w) == 4
    assert w.to_bytes() == b"\xaa\x12\x34\xbb"


def test_byteswriter_patch_u2_surrounds_data():
    # Simulates writing a length-prefixed block: reserve, write content, patch length
    w = BytesWriter()
    length_pos = w.reserve_u2()
    w.write_bytes(b"\x01\x02\x03")
    w.patch_u2(length_pos, 3)
    assert w.to_bytes() == b"\x00\x03\x01\x02\x03"


# ---------------------------------------------------------------------------
# BytesWriter: round-trip with BytesReader
# ---------------------------------------------------------------------------


def test_byteswriter_roundtrip_with_bytesreader():
    w = BytesWriter()
    w.write_u1(0xAA)
    w.write_i1(-1)
    w.write_u2(0x1234)
    w.write_i2(-100)
    w.write_u4(0xDEADBEEF)
    w.write_i4(-1)
    w.write_bytes(b"\x01\x02")

    r = BytesReader(w.to_bytes())
    assert r.read_u1() == 0xAA
    assert r.read_i1() == -1
    assert r.read_u2() == 0x1234
    assert r.read_i2() == -100
    assert r.read_u4() == 0xDEADBEEF
    assert r.read_i4() == -1
    assert r.read_bytes(2) == b"\x01\x02"
