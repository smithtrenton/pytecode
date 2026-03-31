from __future__ import annotations

import pytest

from pytecode.modified_utf8 import decode_modified_utf8, encode_modified_utf8


@pytest.mark.parametrize(
    ("value", "expected"),
    [
        ("", b""),
        ("Hello", b"Hello"),
        ("\x00", b"\xc0\x80"),
        ("😀", b"\xed\xa0\xbd\xed\xb8\x80"),
    ],
)
def test_encode_modified_utf8(value: str, expected: bytes) -> None:
    assert encode_modified_utf8(value) == expected


@pytest.mark.parametrize("value", ["", "Hello", "café", "\x00", "😀", "a\x00😀b"])
def test_modified_utf8_round_trip(value: str) -> None:
    assert decode_modified_utf8(encode_modified_utf8(value)) == value


def test_decode_rejects_raw_nul_byte() -> None:
    with pytest.raises(UnicodeDecodeError, match="NUL"):
        decode_modified_utf8(b"\x00")


def test_decode_rejects_four_byte_sequence() -> None:
    with pytest.raises(UnicodeDecodeError, match="four-byte sequences"):
        decode_modified_utf8("😀".encode())
