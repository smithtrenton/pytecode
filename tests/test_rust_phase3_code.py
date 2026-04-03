from __future__ import annotations

import pytest

import pytecode.classfile.attributes as attributes
import pytecode.classfile.instructions as instructions
import pytecode.classfile.reader as reader_module
import pytecode.classfile.writer as writer_module
from pytecode import ClassReader, ClassWriter
from tests.helpers import class_reader_for_insns, minimal_classfile, u1, u2, u4, utf8_entry_bytes


def _unexpected_python_path(*_args: object, **_kwargs: object) -> object:
    raise AssertionError("expected pytecode to use the Rust code path")


def _classfile_with_code_attr(code_bytes: bytes) -> bytes:
    code_payload = u2(1) + u2(1) + u4(len(code_bytes)) + code_bytes + u2(0) + u2(0)
    method_attr = u2(7) + u4(len(code_payload)) + code_payload
    method_bytes = u2(0x0001) + u2(5) + u2(6) + u2(1) + method_attr
    return minimal_classfile(
        extra_cp_bytes=utf8_entry_bytes("demo") + utf8_entry_bytes("()V") + utf8_entry_bytes("Code"),
        extra_cp_count=3,
        methods_count=1,
        methods_bytes=method_bytes,
    )


@pytest.mark.skipif(
    not reader_module._RUST_CODE_AVAILABLE,
    reason="Rust code backend is not installed in this environment",
)
def test_classreader_uses_rust_code_path(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(reader_module.ClassReader, "_read_code_bytes_python", _unexpected_python_path)

    cf = ClassReader.from_bytes(_classfile_with_code_attr(u1(0xB1))).class_info

    code_attr = cf.methods[0].attributes[0]
    assert isinstance(code_attr, attributes.CodeAttr)
    assert [insn.type for insn in code_attr.code] == [instructions.InsnInfoType.RETURN]


@pytest.mark.skipif(
    not reader_module._RUST_CODE_AVAILABLE,
    reason="Rust code backend is not installed in this environment",
)
def test_rust_read_code_matches_python_path(monkeypatch: pytest.MonkeyPatch) -> None:
    code_bytes = u1(0x10) + u1(42) + u1(0xC4) + u1(0x15) + u2(0x0100) + u1(0xB1)

    rust_reader = class_reader_for_insns(code_bytes)
    rust_insns = rust_reader.read_code_bytes(len(code_bytes))

    monkeypatch.setattr(reader_module, "_RUST_CODE_AVAILABLE", False)
    python_reader = class_reader_for_insns(code_bytes)
    python_insns = python_reader.read_code_bytes(len(code_bytes))

    assert rust_insns == python_insns


@pytest.mark.skipif(
    not writer_module._RUST_CODE_AVAILABLE,
    reason="Rust code backend is not installed in this environment",
)
def test_classwriter_uses_rust_code_path(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(writer_module, "_write_code_bytes_python", _unexpected_python_path)

    parsed = ClassReader.from_bytes(_classfile_with_code_attr(u1(0xB1))).class_info
    emitted = ClassWriter.write(parsed)

    assert emitted == _classfile_with_code_attr(u1(0xB1))


@pytest.mark.skipif(
    not writer_module._RUST_CODE_AVAILABLE,
    reason="Rust code backend is not installed in this environment",
)
def test_rust_write_code_matches_python_path(monkeypatch: pytest.MonkeyPatch) -> None:
    parsed = ClassReader.from_bytes(_classfile_with_code_attr(u1(0x10) + u1(7) + u1(0xB1))).class_info

    rust_bytes = writer_module._write_code_bytes_rust(parsed.methods[0].attributes[0].code)  # type: ignore[reportAttributeAccessIssue]

    monkeypatch.setattr(writer_module, "_RUST_CODE_AVAILABLE", False)
    python_bytes = writer_module._write_code_bytes_python(parsed.methods[0].attributes[0].code)  # type: ignore[reportAttributeAccessIssue]

    assert rust_bytes == python_bytes
