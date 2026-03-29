from __future__ import annotations

from pathlib import Path

import pytest

from pytecode.attributes import (
    AttributeInfo,
    CodeAttr,
    LineNumberTableAttr,
    LocalVariableTableAttr,
    LocalVariableTypeTableAttr,
    SourceDebugExtensionAttr,
    SourceFileAttr,
)
from pytecode.class_reader import ClassReader
from pytecode.constant_pool_builder import ConstantPoolBuilder
from pytecode.debug_info import DebugInfoPolicy, apply_debug_info_policy, strip_debug_info
from pytecode.info import ClassFile, MethodInfo
from pytecode.instructions import InsnInfo, InsnInfoType
from pytecode.labels import Label, LineNumberEntry, LocalVariableEntry, LocalVariableTypeEntry, lower_code
from pytecode.model import ClassModel, CodeModel, MethodModel
from pytecode.verify import Severity, verify_classfile
from tests.helpers import compile_java_resource


def _first_method_with_code(model: ClassModel) -> MethodModel:
    for method in model.methods:
        if method.code is not None:
            return method
    raise AssertionError("Expected at least one method with code")


def _find_raw_code(method: MethodInfo) -> CodeAttr | None:
    return next((attribute for attribute in method.attributes if isinstance(attribute, CodeAttr)), None)


def _assert_valid_classfile(cf: ClassFile) -> None:
    errors = [diag for diag in verify_classfile(cf) if diag.severity is Severity.ERROR]
    assert errors == []


def _assert_no_debug_info_in_classfile(cf: ClassFile) -> None:
    assert not any(isinstance(attr, (SourceFileAttr, SourceDebugExtensionAttr)) for attr in cf.attributes)
    for method in cf.methods:
        code_attr = _find_raw_code(method)
        if code_attr is None:
            continue
        assert not any(
            isinstance(attr, (LineNumberTableAttr, LocalVariableTableAttr, LocalVariableTypeTableAttr))
            for attr in code_attr.attributes
        )


def _sample_code_model() -> CodeModel:
    start = Label("start")
    end = Label("end")
    return CodeModel(
        max_stack=1,
        max_locals=1,
        instructions=[
            start,
            InsnInfo(InsnInfoType.NOP, -1),
            end,
            InsnInfo(InsnInfoType.RETURN, -1),
        ],
        line_numbers=[LineNumberEntry(start, 10)],
        local_variables=[LocalVariableEntry(start, end, "value", "I", 0)],
        local_variable_types=[LocalVariableTypeEntry(start, end, "value", "TI;", 0)],
        attributes=[AttributeInfo(77, 0)],
    )


def test_apply_debug_info_policy_preserve_is_noop_for_code_model() -> None:
    code = _sample_code_model()
    line_numbers = list(code.line_numbers)
    local_variables = list(code.local_variables)
    local_variable_types = list(code.local_variable_types)
    attributes = list(code.attributes)

    returned = apply_debug_info_policy(code, DebugInfoPolicy.PRESERVE)

    assert returned is code
    assert code.line_numbers == line_numbers
    assert code.local_variables == local_variables
    assert code.local_variable_types == local_variable_types
    assert code.attributes == attributes


def test_strip_debug_info_clears_code_model_debug_lists_but_preserves_other_attrs() -> None:
    code = _sample_code_model()

    returned = strip_debug_info(code)

    assert returned is code
    assert code.line_numbers == []
    assert code.local_variables == []
    assert code.local_variable_types == []
    assert code.attributes == [AttributeInfo(77, 0)]


def test_apply_debug_info_policy_rejects_unknown_policy() -> None:
    with pytest.raises(ValueError, match="debug_info must be one of"):
        apply_debug_info_policy(_sample_code_model(), "discard")


def test_lower_code_debug_info_strip_omits_debug_attrs_but_preserves_other_order() -> None:
    start = Label("start")
    end = Label("end")
    first_other = AttributeInfo(90, 0)
    second_other = AttributeInfo(91, 1)
    code = CodeModel(
        max_stack=1,
        max_locals=1,
        instructions=[
            start,
            InsnInfo(InsnInfoType.NOP, -1),
            end,
            InsnInfo(InsnInfoType.RETURN, -1),
        ],
        line_numbers=[LineNumberEntry(start, 10)],
        local_variables=[LocalVariableEntry(start, end, "value", "I", 0)],
        local_variable_types=[LocalVariableTypeEntry(start, end, "value", "TI;", 0)],
        attributes=[first_other, second_other],
        _nested_attribute_layout=("other", "line_numbers", "other", "local_variables", "local_variable_types"),
    )

    lowered = lower_code(code, ConstantPoolBuilder(), debug_info="strip")

    assert lowered.attributes == [first_other, second_other]
    assert not any(
        isinstance(attr, (LineNumberTableAttr, LocalVariableTableAttr, LocalVariableTypeTableAttr))
        for attr in lowered.attributes
    )


def test_to_classfile_debug_info_strip_omits_debug_attrs_without_mutating_model(tmp_path: Path) -> None:
    class_path = compile_java_resource(tmp_path, "HelloWorld.java")
    model = ClassModel.from_bytes(class_path.read_bytes())
    method = _first_method_with_code(model)
    assert method.code is not None
    assert any(isinstance(attr, SourceFileAttr) for attr in model.attributes)
    assert any(mm.code is not None and mm.code.line_numbers for mm in model.methods)
    original_line_numbers = list(method.code.line_numbers)

    stripped = model.to_classfile(debug_info=DebugInfoPolicy.STRIP)

    assert any(isinstance(attr, SourceFileAttr) for attr in model.attributes)
    assert method.code.line_numbers == original_line_numbers
    _assert_no_debug_info_in_classfile(stripped)
    _assert_valid_classfile(stripped)


def test_strip_debug_info_mutates_class_model_and_to_bytes_emits_no_debug_attrs(tmp_path: Path) -> None:
    class_path = compile_java_resource(tmp_path, "HelloWorld.java")
    model = ClassModel.from_bytes(class_path.read_bytes())
    assert any(isinstance(attr, SourceFileAttr) for attr in model.attributes)

    returned = apply_debug_info_policy(model, DebugInfoPolicy.STRIP)

    assert returned is model
    assert not any(isinstance(attr, (SourceFileAttr, SourceDebugExtensionAttr)) for attr in model.attributes)
    for method in model.methods:
        if method.code is None:
            continue
        assert method.code.line_numbers == []
        assert method.code.local_variables == []
        assert method.code.local_variable_types == []

    stripped = ClassReader(model.to_bytes()).class_info
    _assert_no_debug_info_in_classfile(stripped)
    _assert_valid_classfile(stripped)
