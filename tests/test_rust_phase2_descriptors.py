from __future__ import annotations

import pytest

from pytecode.classfile import descriptors


def _unexpected_python_path(*_args: object, **_kwargs: object) -> object:
    raise AssertionError("expected the public descriptor wrapper to use the Rust-backed path")


@pytest.mark.skipif(
    not descriptors._RUST_DESCRIPTORS_AVAILABLE,
    reason="Rust descriptor backend is not installed in this environment",
)
def test_parse_field_descriptor_wrapper_uses_rust(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(descriptors, "_parse_field_descriptor_python", _unexpected_python_path)

    result = descriptors.parse_field_descriptor("Ljava/lang/String;")

    assert result == descriptors.ObjectType("java/lang/String")


@pytest.mark.skipif(
    not descriptors._RUST_DESCRIPTORS_AVAILABLE,
    reason="Rust descriptor backend is not installed in this environment",
)
def test_parse_method_signature_wrapper_uses_rust(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(descriptors, "_parse_method_signature_python", _unexpected_python_path)

    result = descriptors.parse_method_signature("<T:Ljava/lang/Object;>(TT;)TT;")

    assert len(result.type_parameters) == 1
    assert result.parameter_types[0] == descriptors.TypeVariable("T")


def test_descriptor_wrapper_falls_back_to_python(monkeypatch: pytest.MonkeyPatch) -> None:
    calls: list[str] = []
    original = descriptors._parse_field_descriptor_python

    def tracking_python_path(s: str) -> descriptors.FieldDescriptor:
        calls.append(s)
        return original(s)

    monkeypatch.setattr(descriptors, "_RUST_DESCRIPTORS_AVAILABLE", False)
    monkeypatch.setattr(descriptors, "_parse_field_descriptor_python", tracking_python_path)

    result = descriptors.parse_field_descriptor("I")

    assert calls == ["I"]
    assert result is descriptors.BaseType.INT
