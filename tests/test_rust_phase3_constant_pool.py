from __future__ import annotations

import pytest

import pytecode.classfile.constant_pool as constant_pool
import pytecode.classfile.reader as reader_module
from tests.helpers import long_entry_bytes, minimal_classfile, utf8_entry_bytes


def _unexpected_python_path(*_args: object, **_kwargs: object) -> object:
    raise AssertionError("expected ClassReader to use the Rust constant-pool path")


@pytest.mark.skipif(
    not reader_module._RUST_CONSTANT_POOL_AVAILABLE,
    reason="Rust constant-pool backend is not installed in this environment",
)
def test_classreader_uses_rust_constant_pool_path(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(reader_module.ClassReader, "_read_constant_pool_python", _unexpected_python_path)

    cf = reader_module.ClassReader.from_bytes(minimal_classfile()).class_info

    assert isinstance(cf.constant_pool[1], constant_pool.Utf8Info)
    assert cf.constant_pool[1].str_bytes == b"TestClass"


@pytest.mark.skipif(
    not reader_module._RUST_CONSTANT_POOL_AVAILABLE,
    reason="Rust constant-pool backend is not installed in this environment",
)
def test_rust_constant_pool_matches_python_path(monkeypatch: pytest.MonkeyPatch) -> None:
    data = minimal_classfile(
        extra_cp_bytes=long_entry_bytes(0xDEAD, 0xBEEF) + utf8_entry_bytes("after"),
        extra_cp_count=3,
    )

    rust_cf = reader_module.ClassReader.from_bytes(data).class_info

    monkeypatch.setattr(reader_module, "_RUST_CONSTANT_POOL_AVAILABLE", False)
    python_cf = reader_module.ClassReader.from_bytes(data).class_info

    assert rust_cf.constant_pool == python_cf.constant_pool


def test_classreader_falls_back_to_python_constant_pool(monkeypatch: pytest.MonkeyPatch) -> None:
    calls: list[int] = []
    original = reader_module.ClassReader._read_constant_pool_python

    def tracking_python_path(
        self: reader_module.ClassReader, cp_count: int
    ) -> list[constant_pool.ConstantPoolInfo | None]:
        calls.append(cp_count)
        return original(self, cp_count)

    monkeypatch.setattr(reader_module, "_RUST_CONSTANT_POOL_AVAILABLE", False)
    monkeypatch.setattr(reader_module.ClassReader, "_read_constant_pool_python", tracking_python_path)

    cf = reader_module.ClassReader.from_bytes(minimal_classfile()).class_info

    assert calls == [5]
    assert isinstance(cf.constant_pool[2], constant_pool.ClassInfo)
