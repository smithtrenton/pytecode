"""Encode and decode JVM Modified UTF-8 strings (§4.4.7).

Modified UTF-8 is the encoding used by the JVM for ``CONSTANT_Utf8_info``
entries in class files.  It differs from standard UTF-8 in two ways:

* The null character (U+0000) is encoded as the two-byte sequence
  ``0xC0 0x80`` rather than a single ``0x00`` byte.
* Supplementary characters (U+10000–U+10FFFF) are represented as
  surrogate pairs, each encoded independently as three bytes.
"""

from __future__ import annotations

from typing import Never

_ONE_BYTE_MAX = 0x7F
_TWO_BYTE_MAX = 0x7FF
_THREE_BYTE_MAX = 0xFFFF
_MAX_UNICODE = 0x10FFFF


def _decode_error(data: bytes, start: int, end: int, reason: str) -> Never:
    raise UnicodeDecodeError("modified-utf-8", data, start, end, reason)


def _encode_code_unit(code_unit: int, out: bytearray) -> None:
    if code_unit == 0:
        out.extend((0xC0, 0x80))
        return

    if code_unit <= _ONE_BYTE_MAX:
        out.append(code_unit)
        return

    if code_unit <= _TWO_BYTE_MAX:
        out.extend((0xC0 | (code_unit >> 6), 0x80 | (code_unit & 0x3F)))
        return

    out.extend(
        (
            0xE0 | (code_unit >> 12),
            0x80 | ((code_unit >> 6) & 0x3F),
            0x80 | (code_unit & 0x3F),
        )
    )


def encode_modified_utf8(value: str) -> bytes:
    """Encode a Python string to JVM Modified UTF-8 bytes (§4.4.7).

    Supplementary characters (U+10000–U+10FFFF) are split into surrogate
    pairs, each encoded as a three-byte sequence.  The null character
    (U+0000) is encoded as ``0xC0 0x80``.

    Args:
        value: The Python string to encode.

    Returns:
        The Modified UTF-8 encoded byte string.

    Raises:
        ValueError: If any code point exceeds U+10FFFF.
    """

    out = bytearray()
    for char in value:
        code_point = ord(char)
        if code_point <= _THREE_BYTE_MAX:
            _encode_code_unit(code_point, out)
            continue

        if code_point > _MAX_UNICODE:
            raise ValueError(f"code point out of range: U+{code_point:06X}")

        supplemental = code_point - 0x10000
        high_surrogate = 0xD800 | (supplemental >> 10)
        low_surrogate = 0xDC00 | (supplemental & 0x3FF)
        _encode_code_unit(high_surrogate, out)
        _encode_code_unit(low_surrogate, out)

    return bytes(out)


def decode_modified_utf8(data: bytes) -> str:
    """Decode JVM Modified UTF-8 bytes into a Python string (§4.4.7).

    Interprets one-, two-, and three-byte Modified UTF-8 sequences,
    reassembling surrogate pairs into supplementary characters.  Bare
    ``0x00`` bytes and standard four-byte UTF-8 sequences are rejected.

    Args:
        data: The Modified UTF-8 encoded bytes to decode.

    Returns:
        The decoded Python string.

    Raises:
        UnicodeDecodeError: If *data* contains invalid, truncated, or
            overlong sequences.
    """

    utf16_bytes = bytearray()
    index = 0

    while index < len(data):
        first = data[index]

        if first == 0:
            _decode_error(data, index, index + 1, "NUL must use the modified UTF-8 two-byte form")

        if first <= _ONE_BYTE_MAX:
            code_unit = first
            index += 1
        elif 0xC0 <= first <= 0xDF:
            if index + 1 >= len(data):
                _decode_error(data, index, len(data), "truncated two-byte sequence")
            second = data[index + 1]
            if second & 0xC0 != 0x80:
                _decode_error(data, index + 1, index + 2, "invalid continuation byte")
            code_unit = ((first & 0x1F) << 6) | (second & 0x3F)
            if code_unit < 0x80 and code_unit != 0:
                _decode_error(data, index, index + 2, "overlong two-byte sequence")
            index += 2
        elif 0xE0 <= first <= 0xEF:
            if index + 2 >= len(data):
                _decode_error(data, index, len(data), "truncated three-byte sequence")
            second = data[index + 1]
            third = data[index + 2]
            if second & 0xC0 != 0x80:
                _decode_error(data, index + 1, index + 2, "invalid continuation byte")
            if third & 0xC0 != 0x80:
                _decode_error(data, index + 2, index + 3, "invalid continuation byte")
            code_unit = ((first & 0x0F) << 12) | ((second & 0x3F) << 6) | (third & 0x3F)
            if code_unit < 0x800:
                _decode_error(data, index, index + 3, "overlong three-byte sequence")
            index += 3
        else:
            _decode_error(data, index, index + 1, "modified UTF-8 does not permit four-byte sequences")

        utf16_bytes.extend((code_unit >> 8, code_unit & 0xFF))

    return utf16_bytes.decode("utf-16-be", errors="surrogatepass")


__all__ = ["decode_modified_utf8", "encode_modified_utf8"]
