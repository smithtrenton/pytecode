from __future__ import annotations

from pathlib import Path

import pytest

from pytecode.analysis.verify import Severity, verify_classfile
from pytecode.classfile.attributes import (
    AttributeInfo,
    LineNumberTableAttr,
    LocalVariableTableAttr,
    LocalVariableTypeTableAttr,
    MethodParameterInfo,
    MethodParametersAttr,
    SourceDebugExtensionAttr,
    SourceFileAttr,
)
from pytecode.classfile.constants import MethodAccessFlag, MethodParameterAccessFlag
from pytecode.classfile.info import ClassFile
from pytecode.classfile.instructions import InsnInfo, InsnInfoType
from pytecode.classfile.reader import ClassReader
from pytecode.edit.constant_pool_builder import ConstantPoolBuilder
from pytecode.edit.debug_info import (
    DebugInfoPolicy,
    DebugInfoState,
    apply_debug_info_policy,
    mark_class_debug_info_stale,
    mark_code_debug_info_stale,
    strip_debug_info,
)
from pytecode.edit.labels import Label, LineNumberEntry, LocalVariableEntry, LocalVariableTypeEntry, lower_code
from pytecode.edit.model import ClassModel, CodeModel, MethodModel
from tests.helpers import (
    compile_java_resource,
    find_method_in_classfile,
    find_raw_code_attr,
    make_skip_debug_fixture_classfile,
    require_raw_code_attr,
)


def _first_method_with_code(model: ClassModel) -> MethodModel:
    for method in model.methods:
        if method.code is not None:
            return method
    raise AssertionError("Expected at least one method with code")


def _assert_valid_classfile(cf: ClassFile) -> None:
    errors = [diag for diag in verify_classfile(cf) if diag.severity is Severity.ERROR]
    assert errors == []


def _assert_no_debug_info_in_classfile(cf: ClassFile) -> None:
    assert not any(isinstance(attr, (SourceFileAttr, SourceDebugExtensionAttr)) for attr in cf.attributes)
    for method in cf.methods:
        code_attr = find_raw_code_attr(method)
        if code_attr is None:
            continue
        assert not any(
            isinstance(attr, (LineNumberTableAttr, LocalVariableTableAttr, LocalVariableTypeTableAttr))
            for attr in code_attr.attributes
        )


def _assert_has_class_debug_info_in_classfile(cf: ClassFile) -> None:
    assert any(isinstance(attr, (SourceFileAttr, SourceDebugExtensionAttr)) for attr in cf.attributes)


def _assert_has_code_debug_info_in_classfile(cf: ClassFile) -> None:
    assert any(
        code_attr is not None
        and any(
            isinstance(attr, (LineNumberTableAttr, LocalVariableTableAttr, LocalVariableTypeTableAttr))
            for attr in code_attr.attributes
        )
        for method in cf.methods
        if (code_attr := find_raw_code_attr(method)) is not None
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
    mark_code_debug_info_stale(code)

    returned = strip_debug_info(code)

    assert returned is code
    assert code.line_numbers == []
    assert code.local_variables == []
    assert code.local_variable_types == []
    assert code.attributes == [AttributeInfo(77, 0)]
    assert code.debug_info_state is DebugInfoState.FRESH


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


def test_mark_class_and_code_debug_info_stale_mark_explicit_states(tmp_path: Path) -> None:
    class_path = compile_java_resource(tmp_path, "HelloWorld.java")
    model = ClassModel.from_bytes(class_path.read_bytes())
    method = _first_method_with_code(model)
    assert method.code is not None

    returned_class = mark_class_debug_info_stale(model)
    returned_code = mark_code_debug_info_stale(method)

    assert returned_class is model
    assert returned_code is method
    assert model.debug_info_state is DebugInfoState.STALE
    assert method.code.debug_info_state is DebugInfoState.STALE


def test_lower_code_stale_debug_info_omits_debug_attrs_but_preserves_other_order() -> None:
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
    mark_code_debug_info_stale(code)

    lowered = lower_code(code, ConstantPoolBuilder())

    assert lowered.attributes == [first_other, second_other]
    assert not any(
        isinstance(attr, (LineNumberTableAttr, LocalVariableTableAttr, LocalVariableTypeTableAttr))
        for attr in lowered.attributes
    )
    assert code.line_numbers == [LineNumberEntry(start, 10)]
    assert code.debug_info_state is DebugInfoState.STALE


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


def test_to_classfile_class_stale_debug_info_strips_only_class_attrs_without_mutating_model(tmp_path: Path) -> None:
    class_path = compile_java_resource(tmp_path, "HelloWorld.java")
    model = ClassModel.from_bytes(class_path.read_bytes())
    mark_class_debug_info_stale(model)

    stripped = model.to_classfile()

    assert any(isinstance(attr, SourceFileAttr) for attr in model.attributes)
    assert model.debug_info_state is DebugInfoState.STALE
    assert not any(isinstance(attr, (SourceFileAttr, SourceDebugExtensionAttr)) for attr in stripped.attributes)
    _assert_has_code_debug_info_in_classfile(stripped)
    _assert_valid_classfile(stripped)


def test_to_classfile_code_stale_debug_info_strips_only_code_attrs_without_mutating_model(tmp_path: Path) -> None:
    class_path = compile_java_resource(tmp_path, "HelloWorld.java")
    model = ClassModel.from_bytes(class_path.read_bytes())
    method = _first_method_with_code(model)
    assert method.code is not None
    original_line_numbers = list(method.code.line_numbers)
    mark_code_debug_info_stale(method)

    stripped = model.to_classfile()

    assert method.code.line_numbers == original_line_numbers
    assert method.code.debug_info_state is DebugInfoState.STALE
    _assert_has_class_debug_info_in_classfile(stripped)
    _assert_has_code_debug_info_in_classfile(stripped)
    raw_method = find_method_in_classfile(stripped, method.name, method.descriptor)
    code_attr = require_raw_code_attr(raw_method)
    assert not any(
        isinstance(attr, (LineNumberTableAttr, LocalVariableTableAttr, LocalVariableTypeTableAttr))
        for attr in code_attr.attributes
    )
    _assert_valid_classfile(stripped)


def test_strip_debug_info_mutates_class_model_and_to_bytes_emits_no_debug_attrs(tmp_path: Path) -> None:
    class_path = compile_java_resource(tmp_path, "HelloWorld.java")
    model = ClassModel.from_bytes(class_path.read_bytes())
    assert any(isinstance(attr, SourceFileAttr) for attr in model.attributes)
    mark_class_debug_info_stale(model)
    mark_code_debug_info_stale(model)

    returned = apply_debug_info_policy(model, DebugInfoPolicy.STRIP)

    assert returned is model
    assert not any(isinstance(attr, (SourceFileAttr, SourceDebugExtensionAttr)) for attr in model.attributes)
    assert model.debug_info_state is DebugInfoState.FRESH
    for method in model.methods:
        if method.code is None:
            continue
        assert method.code.line_numbers == []
        assert method.code.local_variables == []
        assert method.code.local_variable_types == []
        assert method.code.debug_info_state is DebugInfoState.FRESH

    stripped = ClassReader(model.to_bytes()).class_info
    _assert_no_debug_info_in_classfile(stripped)
    _assert_valid_classfile(stripped)


def test_strip_debug_info_does_not_remove_method_parameters_attrs() -> None:
    code = _sample_code_model()
    method = MethodModel(
        access_flags=MethodAccessFlag.PUBLIC,
        name="demo",
        descriptor="(I)V",
        code=code,
        attributes=[
            MethodParametersAttr(
                attribute_name_index=77,
                attribute_length=5,
                parameters_count=1,
                parameters=[
                    MethodParameterInfo(
                        name_index=78,
                        access_flags=MethodParameterAccessFlag(0),
                    )
                ],
            )
        ],
    )

    strip_debug_info(method)

    assert len(method.attributes) == 1
    assert isinstance(method.attributes[0], MethodParametersAttr)


def test_from_classfile_skip_debug_omits_selected_debug_metadata_before_lift() -> None:
    model = ClassModel.from_classfile(make_skip_debug_fixture_classfile(), skip_debug=True)
    method = _first_method_with_code(model)
    assert method.code is not None

    assert not any(isinstance(attr, (SourceFileAttr, SourceDebugExtensionAttr)) for attr in model.attributes)
    assert not any(isinstance(attr, MethodParametersAttr) for attr in method.attributes)
    assert method.code.line_numbers == []
    assert method.code.local_variables == []
    assert method.code.local_variable_types == []
    assert all(not isinstance(item, Label) for item in method.code.instructions)

    restored = model.to_classfile()

    _assert_no_debug_info_in_classfile(restored)
    assert not any(isinstance(attr, MethodParametersAttr) for method in restored.methods for attr in method.attributes)
    _assert_valid_classfile(restored)
