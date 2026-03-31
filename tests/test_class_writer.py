from __future__ import annotations

import pytest

from pytecode import ClassModel, ClassReader, ClassWriter
from pytecode.attributes import RuntimeInvisibleParameterAnnotationsAttr, RuntimeVisibleParameterAnnotationsAttr
from pytecode.constants import ClassAccessFlag, FieldAccessFlag, MethodAccessFlag
from pytecode.model import FieldModel, MethodModel
from pytecode.verify import Severity, verify_classfile
from tests.helpers import (
    cached_java_resource_classes,
    list_java_resources,
    long_entry_bytes,
    make_attribute_blob,
    minimal_classfile,
    utf8_entry_bytes,
)

ROUNDTRIP_JAVA_RESOURCES = list_java_resources()


def _assert_no_error_diagnostics(data: bytes) -> None:
    diags = verify_classfile(ClassReader(data).class_info)
    errors = [diag for diag in diags if diag.severity is Severity.ERROR]
    assert errors == []


@pytest.mark.parametrize("resource_name", ROUNDTRIP_JAVA_RESOURCES)
def test_writer_and_model_roundtrip_all_java_resources(resource_name: str) -> None:
    for class_path in cached_java_resource_classes(resource_name):
        original = class_path.read_bytes()
        parsed = ClassReader(original).class_info

        emitted = ClassWriter.write(parsed)
        assert emitted == original
        _assert_no_error_diagnostics(emitted)

        model = ClassModel.from_classfile(parsed)
        lowered = model.to_bytes()
        assert lowered == original
        _assert_no_error_diagnostics(lowered)


def test_writer_roundtrip_preserves_unknown_attribute_bytes() -> None:
    raw = minimal_classfile(
        extra_cp_bytes=utf8_entry_bytes("CustomAttr"),
        extra_cp_count=1,
        class_attrs_count=1,
        class_attrs_bytes=make_attribute_blob(5, b"\x01\x02\x03"),
    )

    parsed = ClassReader(raw).class_info
    assert ClassWriter.write(parsed) == raw


def test_writer_roundtrip_preserves_long_gap_slots() -> None:
    raw = minimal_classfile(
        extra_cp_bytes=long_entry_bytes(0x11111111, 0x22222222),
        extra_cp_count=2,
    )

    parsed = ClassReader(raw).class_info
    assert ClassWriter.write(parsed) == raw


def test_writer_roundtrip_preserves_parameter_annotation_attributes() -> None:
    class_paths = cached_java_resource_classes("ParameterAnnotations.java")
    class_path = next((path for path in class_paths if path.name == "ParameterAnnotations.class"), None)
    assert class_path is not None

    original = class_path.read_bytes()
    parsed = ClassReader(original).class_info

    parameter_annotation_attrs = [
        attr
        for method in parsed.methods
        for attr in method.attributes
        if isinstance(
            attr,
            (RuntimeVisibleParameterAnnotationsAttr, RuntimeInvisibleParameterAnnotationsAttr),
        )
    ]

    assert {type(attr) for attr in parameter_annotation_attrs} == {
        RuntimeVisibleParameterAnnotationsAttr,
        RuntimeInvisibleParameterAnnotationsAttr,
    }
    assert {attr.num_parameters for attr in parameter_annotation_attrs} == {2}
    assert all(len(attr.parameter_annotations) == 2 for attr in parameter_annotation_attrs)

    emitted = ClassWriter.write(parsed)
    assert emitted == original
    _assert_no_error_diagnostics(emitted)

    lowered = ClassModel.from_classfile(parsed).to_bytes()
    assert lowered == original
    _assert_no_error_diagnostics(lowered)


def test_model_to_bytes_from_scratch_is_deterministic_and_valid() -> None:
    model = ClassModel(
        version=(52, 0),
        access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.ABSTRACT | ClassAccessFlag.SUPER,
        name="com/example/Generated",
        super_name="java/lang/Object",
        interfaces=["java/io/Serializable"],
        fields=[
            FieldModel(
                access_flags=FieldAccessFlag.PRIVATE,
                name="value",
                descriptor="I",
                attributes=[],
            )
        ],
        methods=[
            MethodModel(
                access_flags=MethodAccessFlag.PUBLIC | MethodAccessFlag.ABSTRACT,
                name="doIt",
                descriptor="()V",
                code=None,
                attributes=[],
            )
        ],
        attributes=[],
    )

    first = model.to_bytes()
    second = model.to_bytes()

    assert first == second
    _assert_no_error_diagnostics(first)
    assert ClassModel.from_bytes(first).name == "com/example/Generated"
