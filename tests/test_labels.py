from __future__ import annotations

from pathlib import Path

import pytest

from pytecode.attributes import (
    CodeAttr,
    ExceptionInfo,
    LineNumberInfo,
    LineNumberTableAttr,
    LocalVariableInfo,
    LocalVariableTableAttr,
    LocalVariableTypeInfo,
    LocalVariableTypeTableAttr,
)
from pytecode.class_reader import ClassReader
from pytecode.constant_pool_builder import ConstantPoolBuilder
from pytecode.constants import ClassAccessFlag, MethodAccessFlag
from pytecode.info import ClassFile, MethodInfo
from pytecode.instructions import Branch, BranchW, InsnInfo, InsnInfoType
from pytecode.labels import (
    BranchInsn,
    ExceptionHandler,
    Label,
    LineNumberEntry,
    LocalVariableEntry,
    LocalVariableTypeEntry,
    LookupSwitchInsn,
    TableSwitchInsn,
    lower_code,
    resolve_labels,
)
from pytecode.model import ClassModel, CodeModel, MethodModel
from tests.helpers import compile_java_resource


@pytest.fixture
def control_flow_class(tmp_path: Path) -> Path:
    return compile_java_resource(tmp_path, "ControlFlowExample.java")


def _find_method(model: ClassModel, name: str) -> MethodModel:
    for method in model.methods:
        if method.name == name:
            return method
    raise AssertionError(f"Method {name!r} not found")


def _find_raw_code(method: MethodInfo) -> CodeAttr:
    for attribute in method.attributes:
        if isinstance(attribute, CodeAttr):
            return attribute
    raise AssertionError("CodeAttr not found")


def _make_debug_classfile() -> ClassFile:
    cp = ConstantPoolBuilder()
    this_class = cp.add_class("DebugFixture")
    super_class = cp.add_class("java/lang/Object")
    code_name = cp.add_utf8("Code")
    line_number_name = cp.add_utf8("LineNumberTable")
    local_variable_name = cp.add_utf8("LocalVariableTable")
    local_variable_type_name = cp.add_utf8("LocalVariableTypeTable")
    method_name = cp.add_utf8("demo")
    method_descriptor = cp.add_utf8("()V")
    variable_name = cp.add_utf8("value")
    variable_descriptor = cp.add_utf8("Ljava/util/List;")
    variable_signature = cp.add_utf8("Ljava/util/List<Ljava/lang/String;>;")

    code_attr = CodeAttr(
        attribute_name_index=code_name,
        attribute_length=62,
        max_stacks=1,
        max_locals=1,
        code_length=2,
        code=[
            InsnInfo(InsnInfoType.NOP, 0),
            InsnInfo(InsnInfoType.RETURN, 1),
        ],
        exception_table_length=0,
        exception_table=[],
        attributes_count=3,
        attributes=[
            LineNumberTableAttr(
                attribute_name_index=line_number_name,
                attribute_length=6,
                line_number_table_length=1,
                line_number_table=[LineNumberInfo(0, 123)],
            ),
            LocalVariableTableAttr(
                attribute_name_index=local_variable_name,
                attribute_length=12,
                local_variable_table_length=1,
                local_variable_table=[
                    LocalVariableInfo(
                        start_pc=0,
                        length=2,
                        name_index=variable_name,
                        descriptor_index=variable_descriptor,
                        index=0,
                    )
                ],
            ),
            LocalVariableTypeTableAttr(
                attribute_name_index=local_variable_type_name,
                attribute_length=12,
                local_variable_type_table_length=1,
                local_variable_type_table=[
                    LocalVariableTypeInfo(
                        start_pc=0,
                        length=2,
                        name_index=variable_name,
                        signature_index=variable_signature,
                        index=0,
                    )
                ],
            ),
        ],
    )

    method = MethodInfo(
        access_flags=MethodAccessFlag.PUBLIC | MethodAccessFlag.STATIC,
        name_index=method_name,
        descriptor_index=method_descriptor,
        attributes_count=1,
        attributes=[code_attr],
    )

    return ClassFile(
        magic=0xCAFEBABE,
        minor_version=0,
        major_version=52,
        constant_pool_count=cp.count,
        constant_pool=cp.build(),
        access_flags=ClassAccessFlag.PUBLIC | ClassAccessFlag.SUPER,
        this_class=this_class,
        super_class=super_class,
        interfaces_count=0,
        interfaces=[],
        fields_count=0,
        fields=[],
        methods_count=1,
        methods=[method],
        attributes_count=0,
        attributes=[],
    )


def test_resolve_labels_allows_adjacent_and_terminal_labels() -> None:
    start = Label("start")
    alias = Label("alias")
    end = Label("end")

    resolution = resolve_labels(
        [
            start,
            alias,
            InsnInfo(InsnInfoType.NOP, -1),
            end,
        ]
    )

    assert resolution.label_offsets[start] == 0
    assert resolution.label_offsets[alias] == 0
    assert resolution.label_offsets[end] == 1
    assert resolution.total_code_length == 1


def test_lower_code_supports_self_branch() -> None:
    loop = Label("loop")
    code = CodeModel(
        max_stack=0,
        max_locals=0,
        instructions=[loop, BranchInsn(InsnInfoType.GOTO, loop)],
    )

    lowered = lower_code(code, ConstantPoolBuilder())

    assert len(lowered.code) == 1
    assert isinstance(lowered.code[0], Branch)
    assert lowered.code[0].type == InsnInfoType.GOTO
    assert lowered.code[0].offset == 0


def test_lower_code_recalculates_branch_offsets_after_edit() -> None:
    target = Label("target")
    code = CodeModel(
        max_stack=0,
        max_locals=0,
        instructions=[
            BranchInsn(InsnInfoType.GOTO, target),
            InsnInfo(InsnInfoType.NOP, -1),
            target,
            InsnInfo(InsnInfoType.RETURN, -1),
        ],
    )

    first = lower_code(code, ConstantPoolBuilder())
    assert isinstance(first.code[0], Branch)
    assert first.code[0].offset == 4

    code.instructions.insert(2, InsnInfo(InsnInfoType.NOP, -1))
    second = lower_code(code, ConstantPoolBuilder())

    assert isinstance(second.code[0], Branch)
    assert second.code[0].offset == 5


def test_lower_code_promotes_goto_to_goto_w() -> None:
    far_target = Label("far")
    code = CodeModel(
        max_stack=0,
        max_locals=0,
        instructions=[
            BranchInsn(InsnInfoType.GOTO, far_target),
            *[InsnInfo(InsnInfoType.NOP, -1) for _ in range(33000)],
            far_target,
            InsnInfo(InsnInfoType.RETURN, -1),
        ],
    )

    lowered = lower_code(code, ConstantPoolBuilder())

    assert isinstance(lowered.code[0], BranchW)
    assert lowered.code[0].type == InsnInfoType.GOTO_W
    assert lowered.code[0].offset > 32767


def test_lower_code_inverts_conditional_branch_overflow() -> None:
    far_target = Label("far")
    code = CodeModel(
        max_stack=0,
        max_locals=0,
        instructions=[
            BranchInsn(InsnInfoType.IFEQ, far_target),
            *[InsnInfo(InsnInfoType.NOP, -1) for _ in range(33000)],
            far_target,
            InsnInfo(InsnInfoType.RETURN, -1),
        ],
    )

    lowered = lower_code(code, ConstantPoolBuilder())

    assert len(lowered.code) >= 2
    assert isinstance(lowered.code[0], Branch)
    assert lowered.code[0].type == InsnInfoType.IFNE
    assert lowered.code[0].offset == 8
    assert isinstance(lowered.code[1], BranchW)
    assert lowered.code[1].type == InsnInfoType.GOTO_W
    assert lowered.code[1].offset > 32767


def test_lower_code_rebuilds_exception_handlers_and_debug_attrs() -> None:
    start = Label("start")
    end = Label("end")
    handler = Label("handler")
    code = CodeModel(
        max_stack=1,
        max_locals=2,
        instructions=[
            start,
            InsnInfo(InsnInfoType.NOP, -1),
            end,
            InsnInfo(InsnInfoType.RETURN, -1),
            handler,
            InsnInfo(InsnInfoType.RETURN, -1),
        ],
        exception_handlers=[ExceptionHandler(start, end, handler, None)],
        line_numbers=[LineNumberEntry(start, 10), LineNumberEntry(end, 20)],
        local_variables=[LocalVariableEntry(start, end, "value", "I", 1)],
        local_variable_types=[LocalVariableTypeEntry(start, end, "value", "TT;", 1)],
    )

    lowered = lower_code(code, ConstantPoolBuilder())

    assert lowered.exception_table == [ExceptionInfo(0, 1, 2, 0)]

    line_numbers = next(attr for attr in lowered.attributes if isinstance(attr, LineNumberTableAttr))
    assert line_numbers.line_number_table == [LineNumberInfo(0, 10), LineNumberInfo(1, 20)]

    local_variables = next(attr for attr in lowered.attributes if isinstance(attr, LocalVariableTableAttr))
    assert local_variables.local_variable_table[0].start_pc == 0
    assert local_variables.local_variable_table[0].length == 1
    assert local_variables.local_variable_table[0].index == 1

    local_variable_types = next(attr for attr in lowered.attributes if isinstance(attr, LocalVariableTypeTableAttr))
    assert local_variable_types.local_variable_type_table[0].start_pc == 0
    assert local_variable_types.local_variable_type_table[0].length == 1
    assert local_variable_types.local_variable_type_table[0].index == 1


def test_lower_code_rejects_empty_exception_handler_range() -> None:
    label = Label("same")
    code = CodeModel(
        max_stack=0,
        max_locals=0,
        instructions=[label, InsnInfo(InsnInfoType.RETURN, -1)],
        exception_handlers=[ExceptionHandler(label, label, label, None)],
    )

    with pytest.raises(ValueError, match="strictly before end"):
        lower_code(code, ConstantPoolBuilder())


def test_from_classfile_lifts_debug_attributes_from_raw_code() -> None:
    model = ClassModel.from_classfile(_make_debug_classfile())

    method = _find_method(model, "demo")
    assert method.code is not None
    start_label = method.code.instructions[0]
    end_label = method.code.instructions[-1]
    assert isinstance(start_label, Label)
    assert isinstance(end_label, Label)
    assert method.code.line_numbers == [LineNumberEntry(start_label, 123)]
    assert method.code.local_variables[0].name == "value"
    assert method.code.local_variables[0].descriptor == "Ljava/util/List;"
    assert method.code.local_variable_types[0].signature == "Ljava/util/List<Ljava/lang/String;>;"

    restored = model.to_classfile()
    restored_code = _find_raw_code(restored.methods[0])

    assert any(isinstance(attr, LineNumberTableAttr) for attr in restored_code.attributes)
    assert any(isinstance(attr, LocalVariableTableAttr) for attr in restored_code.attributes)
    assert any(isinstance(attr, LocalVariableTypeTableAttr) for attr in restored_code.attributes)


def test_from_classfile_uses_symbolic_branch_and_switch_wrappers(control_flow_class: Path) -> None:
    model = ClassReader(control_flow_class.read_bytes()).class_info
    lifted = ClassModel.from_classfile(model)

    loop_sum = _find_method(lifted, "loopSum")
    dense_switch = _find_method(lifted, "denseSwitch")
    sparse_switch = _find_method(lifted, "sparseSwitch")

    assert loop_sum.code is not None
    assert any(isinstance(item, BranchInsn) for item in loop_sum.code.instructions)
    assert any(isinstance(item, Label) for item in loop_sum.code.instructions)

    assert dense_switch.code is not None
    assert any(isinstance(item, TableSwitchInsn) for item in dense_switch.code.instructions)

    assert sparse_switch.code is not None
    assert any(isinstance(item, LookupSwitchInsn) for item in sparse_switch.code.instructions)
