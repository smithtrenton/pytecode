from __future__ import annotations

import pytest

from pytecode._internal import bytes_utils
from pytecode.classfile import modified_utf8

pytestmark = pytest.mark.skipif(
    not (bytes_utils._RUST_BYTES_UTILS_AVAILABLE and modified_utf8._RUST_MODIFIED_UTF8_AVAILABLE),
    reason="Rust backend is not installed in this environment",
)


def _unexpected_python_path(*_args: object, **_kwargs: object) -> object:
    raise AssertionError("expected the public wrapper to use the Rust-backed path")


def test_bytesreader_wrapper_uses_rust_for_success_path(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(bytes_utils, "_read_u4", _unexpected_python_path)

    reader = bytes_utils.BytesReader(b"\x00\x00\x01\x00")

    assert reader.read_u4() == 256
    assert reader.offset == 4


def test_modified_utf8_wrapper_uses_rust_for_success_path(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(modified_utf8, "_decode_modified_utf8_python", _unexpected_python_path)

    assert modified_utf8.decode_modified_utf8(b"Hello") == "Hello"


def test_modified_utf8_wrapper_falls_back_for_python_error_shape(monkeypatch: pytest.MonkeyPatch) -> None:
    def raise_value_error(_data: bytes) -> str:
        raise ValueError("rust backend rejected input")

    monkeypatch.setattr(modified_utf8, "_rust_decode_modified_utf8", raise_value_error)

    with pytest.raises(UnicodeDecodeError, match="NUL"):
        modified_utf8.decode_modified_utf8(b"\x00")
