from __future__ import annotations

import copy
from collections.abc import Callable
from pathlib import Path
from typing import cast

import pytest

from pytecode.classfile.attributes import (
    ExceptionInfo,
    LineNumberInfo,
    LineNumberTableAttr,
    LocalVariableTableAttr,
    LocalVariableTypeTableAttr,
    SyntheticAttr,
)
from pytecode.classfile.instructions import (
    ArrayType,
    Branch,
    BranchW,
    ByteValue,
    ConstPoolIndex,
    IInc,
    InsnInfo,
    InsnInfoType,
    InvokeDynamic,
    InvokeInterface,
    LocalIndex,
    LocalIndexW,
    LookupSwitch,
    MatchOffsetPair,
    MultiANewArray,
    NewArray,
    ShortValue,
)
from pytecode.classfile.reader import ClassReader
from pytecode.edit.constant_pool_builder import ConstantPoolBuilder
from pytecode.edit.labels import (
    BranchInsn,
    CodeItem,
    ExceptionHandler,
    Label,
    LabelResolution,
    LineNumberEntry,
    LocalVariableEntry,
    LocalVariableTypeEntry,
    LookupSwitchInsn,
    TableSwitchInsn,
    _build_ldc_index_cache,
    _resolve_labels_with_cache,
    clone_raw_instruction,
    lower_code,
    resolve_labels,
)
from pytecode.edit.model import ClassModel, CodeModel
from pytecode.edit.operands import FieldInsn, LdcInsn, LdcInt, LdcString, MethodInsn, TypeInsn
from tests.helpers import (
    compile_java_resource,
    find_method_in_model,
    make_debug_fixture_classfile,
    require_raw_code_attr,
)


@pytest.fixture
def control_flow_class(tmp_path: Path) -> Path:
    return compile_java_resource(tmp_path, "ControlFlowExample.java")


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


def test_label_preserves_identity_hashing_repr_and_copy_helpers() -> None:
    named = Label("named")
    unnamed = Label()
    named_copy = copy.copy(named)
    named_deepcopy = copy.deepcopy(named)
    ctor, args = cast(tuple[Callable[..., object], tuple[object, ...]], named.__reduce__())
    rebuilt = cast(Label, ctor(*args))
    mapping = {named: "value"}

    assert named != Label("named")
    assert isinstance(hash(named), int)
    assert mapping[named] == "value"
    assert named_copy not in mapping
    assert repr(named) == "Label('named')"
    assert repr(unnamed).startswith("Label(id=0x")
    assert repr(unnamed).endswith(")")
    assert named_copy is not named
    assert named_copy.name == named.name
    assert named_copy != named
    assert named_deepcopy is not named
    assert named_deepcopy.name == named.name
    assert named_deepcopy != named
    assert rebuilt is not named
    assert rebuilt.name == named.name
    assert rebuilt != named


def test_migrated_label_record_objects_preserve_label_aware_value_semantics() -> None:
    start = Label("start")
    end = Label("end")
    handler = Label("handler")
    record_values = [
        (
            ExceptionHandler(start, end, handler, "java/lang/Exception"),
            "ExceptionHandler(start=Label('start'), end=Label('end'), handler=Label('handler'), "
            "catch_type='java/lang/Exception')",
            ("start", "end", "handler"),
        ),
        (
            LineNumberEntry(start, 42),
            "LineNumberEntry(label=Label('start'), line_number=42)",
            ("label",),
        ),
        (
            LocalVariableEntry(start, end, "value", "I", 1),
            "LocalVariableEntry(start=Label('start'), end=Label('end'), name='value', descriptor='I', slot=1)",
            ("start", "end"),
        ),
        (
            LocalVariableTypeEntry(start, end, "value", "TT;", 1),
            "LocalVariableTypeEntry(start=Label('start'), end=Label('end'), name='value', signature='TT;', slot=1)",
            ("start", "end"),
        ),
        (
            LabelResolution({start: 0}, [0, 1], 2),
            "LabelResolution(label_offsets={Label('start'): 0}, instruction_offsets=[0, 1], total_code_length=2)",
            (),
        ),
    ]

    for value, expected_repr, label_attrs in record_values:
        shallow = copy.copy(value)
        deep = copy.deepcopy(value)
        ctor, args = cast(tuple[Callable[..., object], tuple[object, ...]], value.__reduce__())
        rebuilt = ctor(*args)

        assert shallow == value
        assert shallow is not value
        assert deep is not value
        assert deep != value
        assert rebuilt == value
        assert rebuilt is not value
        assert repr(value) == expected_repr

        for attr_name in label_attrs:
            assert getattr(shallow, attr_name) is getattr(value, attr_name)
            assert getattr(deep, attr_name) is not getattr(value, attr_name)

        with pytest.raises(TypeError, match="unhashable type"):
            hash(value)


def test_migrated_symbolic_instruction_wrappers_preserve_instruction_style_semantics() -> None:
    default_target = Label("default")
    case_one = Label("case_one")
    case_two = Label("case_two")
    branch = BranchInsn(InsnInfoType.GOTO, case_one, 5)
    lookup = LookupSwitchInsn(default_target, [(1, case_one), (2, case_two)], 4)
    table = TableSwitchInsn(default_target, 0, 1, [case_one, case_two], 7)

    assert repr(branch) == "BranchInsn(type=<InsnInfoType.GOTO: 167>, bytecode_offset=5, target=Label('case_one'))"
    assert branch == BranchInsn(InsnInfoType.GOTO, case_one, 5)
    assert branch != BranchInsn(InsnInfoType.JSR, case_one, 5)
    assert branch != BranchInsn(InsnInfoType.GOTO, case_one, 6)

    shallow_branch = copy.copy(branch)
    deep_branch = copy.deepcopy(branch)
    branch_ctor, branch_args = cast(tuple[Callable[..., object], tuple[object, ...]], branch.__reduce__())
    rebuilt_branch = branch_ctor(*branch_args)
    assert shallow_branch == branch
    assert shallow_branch.target is branch.target
    assert deep_branch != branch
    assert deep_branch.target is not branch.target
    assert rebuilt_branch == branch
    assert branch_args == (InsnInfoType.GOTO, case_one, 5)

    assert repr(lookup) == (
        "LookupSwitchInsn(type=<InsnInfoType.LOOKUPSWITCH: 171>, bytecode_offset=4, "
        "default_target=Label('default'), pairs=[(1, Label('case_one')), (2, Label('case_two'))])"
    )
    assert lookup == LookupSwitchInsn(default_target, [(1, case_one), (2, case_two)], 4)
    assert lookup != LookupSwitchInsn(default_target, [(1, case_one), (2, case_two)], 5)

    shallow_lookup = copy.copy(lookup)
    deep_lookup = copy.deepcopy(lookup)
    lookup_ctor, lookup_args = cast(tuple[Callable[..., object], tuple[object, ...]], lookup.__reduce__())
    rebuilt_lookup = lookup_ctor(*lookup_args)
    assert shallow_lookup == lookup
    assert shallow_lookup.pairs is lookup.pairs
    assert deep_lookup != lookup
    assert deep_lookup.pairs is not lookup.pairs
    assert deep_lookup.pairs[0] is not lookup.pairs[0]
    assert deep_lookup.pairs[0][1] is not lookup.pairs[0][1]
    assert rebuilt_lookup == lookup
    assert lookup_args == (default_target, [(1, case_one), (2, case_two)], 4)

    assert repr(table) == (
        "TableSwitchInsn(type=<InsnInfoType.TABLESWITCH: 170>, bytecode_offset=7, "
        "default_target=Label('default'), low=0, high=1, targets=[Label('case_one'), Label('case_two')])"
    )
    assert table == TableSwitchInsn(default_target, 0, 1, [case_one, case_two], 7)
    assert table != TableSwitchInsn(default_target, 0, 1, [case_one, case_two], 8)

    shallow_table = copy.copy(table)
    deep_table = copy.deepcopy(table)
    table_ctor, table_args = cast(tuple[Callable[..., object], tuple[object, ...]], table.__reduce__())
    rebuilt_table = table_ctor(*table_args)
    assert shallow_table == table
    assert shallow_table.targets is table.targets
    assert deep_table != table
    assert deep_table.targets is not table.targets
    assert deep_table.targets[0] is not table.targets[0]
    assert rebuilt_table == table
    assert table_args == (default_target, 0, 1, [case_one, case_two], 7)

    for value in (branch, lookup, table):
        with pytest.raises(TypeError, match="unhashable type"):
            hash(value)


def test_symbolic_instruction_wrappers_preserve_constructor_validation() -> None:
    target = Label("target")

    with pytest.raises(ValueError, match="NOP is not a branch opcode"):
        BranchInsn(InsnInfoType.NOP, target)
    with pytest.raises(ValueError, match="tableswitch high must be >= low"):
        TableSwitchInsn(target, 2, 1, [target])
    with pytest.raises(ValueError, match="tableswitch range 0..1 requires 2 targets, got 1"):
        TableSwitchInsn(target, 0, 1, [target])


def test_resolve_labels_requires_cp_for_single_slot_ldc() -> None:
    with pytest.raises(ValueError, match="ConstantPoolBuilder"):
        resolve_labels([LdcInsn(LdcInt(42))])


def test_resolve_labels_uses_cp_for_exact_single_slot_ldc_size() -> None:
    end = Label("end")
    resolution = resolve_labels([LdcInsn(LdcInt(42)), end], ConstantPoolBuilder())

    assert resolution.label_offsets[end] == 2
    assert resolution.total_code_length == 2


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


def test_lower_code_does_not_mutate_cp_on_failed_validation() -> None:
    cp = ConstantPoolBuilder()
    missing = Label("missing")
    code = CodeModel(
        max_stack=1,
        max_locals=0,
        instructions=[
            LdcInsn(LdcString("x")),
            BranchInsn(InsnInfoType.GOTO, missing),
        ],
    )
    before = cp.build()

    with pytest.raises(ValueError, match="not present"):
        lower_code(code, cp)

    assert cp.count == 1
    assert cp.build() == before


def test_lower_code_commits_constant_pool_on_success() -> None:
    cp = ConstantPoolBuilder()
    code = CodeModel(
        max_stack=1,
        max_locals=0,
        instructions=[
            LdcInsn(LdcString("shared")),
            LdcInsn(LdcString("shared")),
            InsnInfo(InsnInfoType.RETURN, -1),
        ],
    )

    lowered = lower_code(code, cp)

    shared_utf8 = cp.find_utf8("shared")
    assert shared_utf8 is not None
    string_index = cp.add_string("shared")
    assert cp.count > 1
    assert isinstance(lowered.code[0], (LocalIndex, ConstPoolIndex))
    assert isinstance(lowered.code[1], (LocalIndex, ConstPoolIndex))
    assert lowered.code[0].index == lowered.code[1].index == string_index


def test_lower_code_reuses_symbolic_cp_indexes_within_pass() -> None:
    cp = ConstantPoolBuilder()
    code = CodeModel(
        max_stack=2,
        max_locals=0,
        instructions=[
            FieldInsn(InsnInfoType.GETSTATIC, "java/lang/System", "out", "Ljava/io/PrintStream;"),
            FieldInsn(InsnInfoType.GETSTATIC, "java/lang/System", "out", "Ljava/io/PrintStream;"),
            MethodInsn(InsnInfoType.INVOKEVIRTUAL, "java/io/PrintStream", "println", "(I)V"),
            MethodInsn(InsnInfoType.INVOKEVIRTUAL, "java/io/PrintStream", "println", "(I)V"),
            TypeInsn(InsnInfoType.NEW, "java/lang/StringBuilder"),
            TypeInsn(InsnInfoType.NEW, "java/lang/StringBuilder"),
            LdcInsn(LdcString("shared")),
            LdcInsn(LdcString("shared")),
            InsnInfo(InsnInfoType.RETURN, -1),
        ],
    )

    lowered = lower_code(code, cp)

    assert isinstance(lowered.code[0], ConstPoolIndex)
    assert isinstance(lowered.code[1], ConstPoolIndex)
    assert lowered.code[0].index == lowered.code[1].index

    assert isinstance(lowered.code[2], ConstPoolIndex)
    assert isinstance(lowered.code[3], ConstPoolIndex)
    assert lowered.code[2].index == lowered.code[3].index

    assert isinstance(lowered.code[4], ConstPoolIndex)
    assert isinstance(lowered.code[5], ConstPoolIndex)
    assert lowered.code[4].index == lowered.code[5].index

    assert isinstance(lowered.code[6], (LocalIndex, ConstPoolIndex))
    assert isinstance(lowered.code[7], (LocalIndex, ConstPoolIndex))
    assert lowered.code[6].index == lowered.code[7].index


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


def test_lower_code_branch_promotion_does_not_mutate_original_instructions() -> None:
    far_target = Label("far")
    original_branch = BranchInsn(InsnInfoType.IFEQ, far_target)
    original_instructions: list[CodeItem] = [
        original_branch,
        *[InsnInfo(InsnInfoType.NOP, -1) for _ in range(33000)],
        far_target,
        InsnInfo(InsnInfoType.RETURN, -1),
    ]
    code = CodeModel(
        max_stack=0,
        max_locals=0,
        instructions=list(original_instructions),
    )

    lowered = lower_code(code, ConstantPoolBuilder())

    assert len(lowered.code) >= 2
    assert code.instructions == original_instructions
    assert code.instructions[0] is original_branch
    assert isinstance(code.instructions[0], BranchInsn)
    assert code.instructions[0].type == InsnInfoType.IFEQ


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


def test_lower_code_respects_nested_attribute_layout_and_deduplicates_debug_tokens() -> None:
    start = Label("start")
    end = Label("end")
    marker = SyntheticAttr(7, 0)
    code = CodeModel(
        max_stack=1,
        max_locals=2,
        instructions=[
            start,
            InsnInfo(InsnInfoType.NOP, -1),
            end,
            InsnInfo(InsnInfoType.RETURN, -1),
        ],
        line_numbers=[LineNumberEntry(start, 10)],
        local_variables=[LocalVariableEntry(start, end, "value", "I", 1)],
        local_variable_types=[LocalVariableTypeEntry(start, end, "value", "TT;", 1)],
        attributes=[marker],
        _nested_attribute_layout=("line_numbers", "line_numbers", "other", "local_variables"),
    )

    lowered = lower_code(code, ConstantPoolBuilder())

    assert [type(attr) for attr in lowered.attributes] == [
        LineNumberTableAttr,
        SyntheticAttr,
        LocalVariableTableAttr,
        LocalVariableTypeTableAttr,
    ]


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
    model = ClassModel.from_classfile(make_debug_fixture_classfile())

    method = find_method_in_model(model, "demo")
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
    restored_code = require_raw_code_attr(restored.methods[0])

    assert any(isinstance(attr, LineNumberTableAttr) for attr in restored_code.attributes)
    assert any(isinstance(attr, LocalVariableTableAttr) for attr in restored_code.attributes)
    assert any(isinstance(attr, LocalVariableTypeTableAttr) for attr in restored_code.attributes)


def test_from_classfile_uses_symbolic_branch_and_switch_wrappers(control_flow_class: Path) -> None:
    model = ClassReader(control_flow_class.read_bytes()).class_info
    lifted = ClassModel.from_classfile(model)

    loop_sum = find_method_in_model(lifted, "loopSum")
    dense_switch = find_method_in_model(lifted, "denseSwitch")
    sparse_switch = find_method_in_model(lifted, "sparseSwitch")

    assert loop_sum.code is not None
    assert any(isinstance(item, BranchInsn) for item in loop_sum.code.instructions)
    assert any(isinstance(item, Label) for item in loop_sum.code.instructions)

    assert dense_switch.code is not None
    assert any(isinstance(item, TableSwitchInsn) for item in dense_switch.code.instructions)

    assert sparse_switch.code is not None
    assert any(isinstance(item, LookupSwitchInsn) for item in sparse_switch.code.instructions)


def test_lower_code_promotes_goto_backward_overflow() -> None:
    far_back = Label("far_back")
    code = CodeModel(
        max_stack=0,
        max_locals=0,
        instructions=[
            far_back,
            *[InsnInfo(InsnInfoType.NOP, -1) for _ in range(33000)],
            BranchInsn(InsnInfoType.GOTO, far_back),
        ],
    )

    lowered = lower_code(code, ConstantPoolBuilder())

    assert isinstance(lowered.code[-1], BranchW)
    assert lowered.code[-1].type == InsnInfoType.GOTO_W
    assert lowered.code[-1].offset < -32768


def test_lower_code_promotes_jsr_to_jsr_w() -> None:
    far_target = Label("far")
    code = CodeModel(
        max_stack=0,
        max_locals=0,
        instructions=[
            BranchInsn(InsnInfoType.JSR, far_target),
            *[InsnInfo(InsnInfoType.NOP, -1) for _ in range(33000)],
            far_target,
            InsnInfo(InsnInfoType.RETURN, -1),
        ],
    )

    lowered = lower_code(code, ConstantPoolBuilder())

    assert isinstance(lowered.code[0], BranchW)
    assert lowered.code[0].type == InsnInfoType.JSR_W
    assert lowered.code[0].offset > 32767


def test_lower_code_no_promotion_needed() -> None:
    near_target = Label("near")
    code = CodeModel(
        max_stack=0,
        max_locals=0,
        instructions=[
            BranchInsn(InsnInfoType.GOTO, near_target),
            *[InsnInfo(InsnInfoType.NOP, -1) for _ in range(10)],
            near_target,
            InsnInfo(InsnInfoType.RETURN, -1),
        ],
    )

    lowered = lower_code(code, ConstantPoolBuilder())

    assert isinstance(lowered.code[0], Branch)
    assert lowered.code[0].type == InsnInfoType.GOTO
    assert len(lowered.code) == 12  # GOTO + 10 NOPs + RETURN


_CONDITIONAL_INVERSION_PAIRS = [
    (InsnInfoType.IFEQ, InsnInfoType.IFNE),
    (InsnInfoType.IFNE, InsnInfoType.IFEQ),
    (InsnInfoType.IFLT, InsnInfoType.IFGE),
    (InsnInfoType.IFGE, InsnInfoType.IFLT),
    (InsnInfoType.IFGT, InsnInfoType.IFLE),
    (InsnInfoType.IFLE, InsnInfoType.IFGT),
    (InsnInfoType.IF_ICMPEQ, InsnInfoType.IF_ICMPNE),
    (InsnInfoType.IF_ICMPNE, InsnInfoType.IF_ICMPEQ),
    (InsnInfoType.IF_ICMPLT, InsnInfoType.IF_ICMPGE),
    (InsnInfoType.IF_ICMPGE, InsnInfoType.IF_ICMPLT),
    (InsnInfoType.IF_ICMPGT, InsnInfoType.IF_ICMPLE),
    (InsnInfoType.IF_ICMPLE, InsnInfoType.IF_ICMPGT),
    (InsnInfoType.IF_ACMPEQ, InsnInfoType.IF_ACMPNE),
    (InsnInfoType.IF_ACMPNE, InsnInfoType.IF_ACMPEQ),
    (InsnInfoType.IFNULL, InsnInfoType.IFNONNULL),
    (InsnInfoType.IFNONNULL, InsnInfoType.IFNULL),
]


@pytest.mark.parametrize(
    "branch_type,inverted_type",
    _CONDITIONAL_INVERSION_PAIRS,
    ids=lambda t: t.name,
)
def test_lower_code_inverts_all_conditional_branches(branch_type: InsnInfoType, inverted_type: InsnInfoType) -> None:
    far_target = Label("far")
    code = CodeModel(
        max_stack=0,
        max_locals=0,
        instructions=[
            BranchInsn(branch_type, far_target),
            *[InsnInfo(InsnInfoType.NOP, -1) for _ in range(33000)],
            far_target,
            InsnInfo(InsnInfoType.RETURN, -1),
        ],
    )

    lowered = lower_code(code, ConstantPoolBuilder())

    assert isinstance(lowered.code[0], Branch)
    assert lowered.code[0].type == inverted_type
    assert isinstance(lowered.code[1], BranchW)
    assert lowered.code[1].type == InsnInfoType.GOTO_W


def test_lower_code_cascading_promotion() -> None:
    label_a = Label("label_a")
    label_b = Label("label_b")
    # Initial layout:
    #   GOTO_A at 0 (3 bytes) → label_a at 32766  →  offset 32766 (fits i2: ≤ 32767)
    #   GOTO_B at 3 (3 bytes) → label_b at 32771  →  offset 32768 (overflows i2)
    # After round 1 GOTO_B is widened to GOTO_W (+2 bytes), label_a shifts to 32768.
    # Now GOTO_A's offset becomes 32768 > 32767, triggering a second promotion.
    code = CodeModel(
        max_stack=0,
        max_locals=0,
        instructions=[
            BranchInsn(InsnInfoType.GOTO, label_a),
            BranchInsn(InsnInfoType.GOTO, label_b),
            *[InsnInfo(InsnInfoType.NOP, -1) for _ in range(32760)],
            label_a,
            *[InsnInfo(InsnInfoType.NOP, -1) for _ in range(5)],
            label_b,
            InsnInfo(InsnInfoType.RETURN, -1),
        ],
    )

    lowered = lower_code(code, ConstantPoolBuilder())

    assert isinstance(lowered.code[0], BranchW)
    assert lowered.code[0].type == InsnInfoType.GOTO_W
    assert isinstance(lowered.code[1], BranchW)
    assert lowered.code[1].type == InsnInfoType.GOTO_W


@pytest.mark.parametrize(
    "insn, prefix_nops, expected_size",
    [
        # InsnInfo (NOP) → 1 byte
        (InsnInfo(InsnInfoType.NOP, -1), 0, 1),
        # LocalIndex (ILOAD) → 2 bytes
        (LocalIndex(InsnInfoType.ILOAD, -1, 0), 0, 2),
        # LocalIndexW (WIDE ILOAD) → 4 bytes
        (LocalIndexW(InsnInfoType.ILOADW, -1, 0), 0, 4),
        # ConstPoolIndex (LDC_W) → 3 bytes
        (ConstPoolIndex(InsnInfoType.LDC_W, -1, 0), 0, 3),
        # ByteValue (BIPUSH) → 2 bytes
        (ByteValue(InsnInfoType.BIPUSH, -1, 0), 0, 2),
        # ShortValue (SIPUSH) → 3 bytes
        (ShortValue(InsnInfoType.SIPUSH, -1, 0), 0, 3),
        # BranchInsn targeting a Branch opcode (GOTO) → 3 bytes
        (BranchInsn(InsnInfoType.GOTO, Label("target")), 0, 3),
        # BranchInsn targeting a BranchW opcode (GOTO_W) → 5 bytes
        (BranchInsn(InsnInfoType.GOTO_W, Label("target")), 0, 5),
        # IInc (IINC) → 3 bytes
        (IInc(InsnInfoType.IINC, -1, 0, 1), 0, 3),
        # InvokeDynamic → 5 bytes
        (InvokeDynamic(InsnInfoType.INVOKEDYNAMIC, -1, 0, b"\x00\x00"), 0, 5),
        # InvokeInterface → 5 bytes
        (InvokeInterface(InsnInfoType.INVOKEINTERFACE, -1, 0, 1, b"\x00"), 0, 5),
        # NewArray → 2 bytes
        (NewArray(InsnInfoType.NEWARRAY, -1, ArrayType.INT), 0, 2),
        # MultiANewArray → 4 bytes
        (MultiANewArray(InsnInfoType.MULTIANEWARRAY, -1, 0, 2), 0, 4),
        # LookupSwitchInsn at offset 0 (padding=3), 2 pairs → 1+3+8+16 = 28 bytes
        (LookupSwitchInsn(Label("d"), [(1, Label("c1")), (2, Label("c2"))]), 0, 28),
        # LookupSwitchInsn at offset 1 (padding=2), 2 pairs → 1+2+8+16 = 27 bytes
        (LookupSwitchInsn(Label("d"), [(1, Label("c1")), (2, Label("c2"))]), 1, 27),
        # LookupSwitchInsn at offset 2 (padding=1), 2 pairs → 1+1+8+16 = 26 bytes
        (LookupSwitchInsn(Label("d"), [(1, Label("c1")), (2, Label("c2"))]), 2, 26),
        # LookupSwitchInsn at offset 3 (padding=0), 2 pairs → 1+0+8+16 = 25 bytes
        (LookupSwitchInsn(Label("d"), [(1, Label("c1")), (2, Label("c2"))]), 3, 25),
        # TableSwitchInsn at offset 0 (padding=3), low=0 high=2 → 1+3+12+12 = 28 bytes
        (TableSwitchInsn(Label("d"), 0, 2, [Label("t0"), Label("t1"), Label("t2")]), 0, 28),
        # TableSwitchInsn at offset 1 (padding=2), low=0 high=2 → 1+2+12+12 = 27 bytes
        (TableSwitchInsn(Label("d"), 0, 2, [Label("t0"), Label("t1"), Label("t2")]), 1, 27),
    ],
)
def test_instruction_byte_size(insn: InsnInfo, prefix_nops: int, expected_size: int) -> None:
    items: list[InsnInfo | Label] = [*[InsnInfo(InsnInfoType.NOP, -1) for _ in range(prefix_nops)], insn]
    resolution = resolve_labels(items)
    assert resolution.total_code_length == prefix_nops + expected_size


def test_lower_code_remove_instruction_updates_offset() -> None:
    target_label = Label("target")
    code = CodeModel(
        max_stack=0,
        max_locals=0,
        instructions=[
            BranchInsn(InsnInfoType.GOTO, target_label),
            InsnInfo(InsnInfoType.NOP, -1),
            InsnInfo(InsnInfoType.NOP, -1),
            InsnInfo(InsnInfoType.NOP, -1),
            target_label,
            InsnInfo(InsnInfoType.RETURN, -1),
        ],
    )

    # GOTO(3) + NOP + NOP + NOP = 6 bytes; target at offset 6
    first = lower_code(code, ConstantPoolBuilder())
    assert isinstance(first.code[0], Branch)
    assert first.code[0].offset == 6

    # Remove one NOP; target shifts to offset 5
    code.instructions.pop(3)
    second = lower_code(code, ConstantPoolBuilder())
    assert isinstance(second.code[0], Branch)
    assert second.code[0].offset == 5


def test_lower_code_insert_triggers_goto_w_promotion() -> None:
    target = Label("target")
    code = CodeModel(
        max_stack=0,
        max_locals=0,
        instructions=[
            BranchInsn(InsnInfoType.GOTO, target),
            *[InsnInfo(InsnInfoType.NOP, -1) for _ in range(32764)],
            target,
            InsnInfo(InsnInfoType.RETURN, -1),
        ],
    )

    # GOTO(3) + 32764 NOPs = target at offset 32767; just fits in i2
    first = lower_code(code, ConstantPoolBuilder())
    assert isinstance(first.code[0], Branch)
    assert first.code[0].offset == 32767

    # One extra NOP pushes the target past the i2 boundary
    code.instructions.insert(1, InsnInfo(InsnInfoType.NOP, -1))
    second = lower_code(code, ConstantPoolBuilder())
    assert isinstance(second.code[0], BranchW)
    assert second.code[0].type == InsnInfoType.GOTO_W
    assert second.code[0].offset > 32767


def test_lower_code_add_exception_handler_dynamically() -> None:
    start = Label("start")
    end = Label("end")
    handler = Label("handler")
    code = CodeModel(
        max_stack=0,
        max_locals=0,
        instructions=[
            start,
            InsnInfo(InsnInfoType.NOP, -1),
            end,
            handler,
            InsnInfo(InsnInfoType.RETURN, -1),
        ],
    )

    first = lower_code(code, ConstantPoolBuilder())
    assert first.exception_table == []

    code.exception_handlers.append(ExceptionHandler(start, end, handler, None))
    second = lower_code(code, ConstantPoolBuilder())

    assert len(second.exception_table) == 1
    assert second.exception_table[0].start_pc == 0
    assert second.exception_table[0].end_pc == 1
    assert second.exception_table[0].handler_pc == 1


def test_lower_code_add_line_number_entry() -> None:
    start = Label("start")
    code = CodeModel(
        max_stack=0,
        max_locals=0,
        instructions=[
            start,
            InsnInfo(InsnInfoType.NOP, -1),
            InsnInfo(InsnInfoType.RETURN, -1),
        ],
    )

    first = lower_code(code, ConstantPoolBuilder())
    assert not any(isinstance(attr, LineNumberTableAttr) for attr in first.attributes)

    code.line_numbers.append(LineNumberEntry(start, 42))
    second = lower_code(code, ConstantPoolBuilder())

    line_table = next(attr for attr in second.attributes if isinstance(attr, LineNumberTableAttr))
    assert line_table.line_number_table[0].line_number == 42


def test_lower_code_dangling_label_allowed() -> None:
    dangling = Label("dangling")
    code = CodeModel(
        max_stack=0,
        max_locals=0,
        instructions=[
            dangling,
            InsnInfo(InsnInfoType.NOP, -1),
            InsnInfo(InsnInfoType.RETURN, -1),
        ],
    )

    lowered = lower_code(code, ConstantPoolBuilder())

    assert lowered.code_length == 2


def test_lower_code_large_method_near_limit() -> None:
    code = CodeModel(
        max_stack=0,
        max_locals=0,
        instructions=[InsnInfo(InsnInfoType.NOP, -1) for _ in range(65535)],
    )

    lowered = lower_code(code, ConstantPoolBuilder())

    assert lowered.code_length == 65535


def test_lower_code_exceeds_code_length_limit() -> None:
    code = CodeModel(
        max_stack=0,
        max_locals=0,
        instructions=[InsnInfo(InsnInfoType.NOP, -1) for _ in range(65536)],
    )

    with pytest.raises(ValueError, match="exceeds JVM maximum of 65535 bytes"):
        lower_code(code, ConstantPoolBuilder())


def test_resolve_labels_linear_code() -> None:
    items: list[InsnInfo | Label] = [
        InsnInfo(InsnInfoType.NOP, -1),
        InsnInfo(InsnInfoType.NOP, -1),
        InsnInfo(InsnInfoType.NOP, -1),
    ]

    resolution = resolve_labels(items)

    assert resolution.instruction_offsets == [0, 1, 2]
    assert resolution.total_code_length == 3
    assert resolution.label_offsets == {}


def test_resolve_labels_forward_branch() -> None:
    label = Label("target")
    items = [
        BranchInsn(InsnInfoType.GOTO, label),
        InsnInfo(InsnInfoType.NOP, -1),
        label,
        InsnInfo(InsnInfoType.RETURN, -1),
    ]

    resolution = resolve_labels(items)

    assert resolution.label_offsets[label] == 4
    assert resolution.total_code_length == 5


def test_resolve_labels_backward_branch() -> None:
    label = Label("loop")
    items = [
        label,
        InsnInfo(InsnInfoType.NOP, -1),
        BranchInsn(InsnInfoType.GOTO, label),
        InsnInfo(InsnInfoType.RETURN, -1),
    ]

    resolution = resolve_labels(items)

    assert resolution.label_offsets[label] == 0
    assert resolution.instruction_offsets[0] == 0  # label
    assert resolution.instruction_offsets[1] == 0  # NOP
    assert resolution.instruction_offsets[2] == 1  # GOTO
    assert resolution.instruction_offsets[3] == 4  # RETURN
    assert resolution.total_code_length == 5


def test_resolve_labels_multiple_branches_same_label() -> None:
    target = Label("target")
    items = [
        BranchInsn(InsnInfoType.GOTO, target),
        BranchInsn(InsnInfoType.GOTO, target),
        target,
        InsnInfo(InsnInfoType.RETURN, -1),
    ]

    resolution = resolve_labels(items)

    assert len(resolution.label_offsets) == 1
    assert resolution.label_offsets[target] == 6
    assert resolution.instruction_offsets[0] == 0
    assert resolution.instruction_offsets[1] == 3
    assert resolution.total_code_length == 7


def test_resolve_labels_with_cache_matches_public_resolver() -> None:
    end = Label("end")
    items: list[CodeItem] = [LdcInsn(LdcInt(42)), end]
    cp = ConstantPoolBuilder()

    expected = resolve_labels(items, cp)
    cached = _resolve_labels_with_cache(items, _build_ldc_index_cache(items, cp))

    assert cached == expected


def test_build_ldc_index_cache_reuses_imported_entries_without_cloning() -> None:
    cp = ConstantPoolBuilder()
    existing_index = cp.add_string("shared")
    items: list[CodeItem] = [LdcInsn(LdcString("shared"))]

    original_clone = ConstantPoolBuilder.clone

    def fail_clone(self: ConstantPoolBuilder) -> ConstantPoolBuilder:
        raise AssertionError("clone() should not be needed for pre-existing LDC entries")

    ConstantPoolBuilder.clone = fail_clone
    try:
        cache = _build_ldc_index_cache(items, cp)
    finally:
        ConstantPoolBuilder.clone = original_clone

    assert cache[id(items[0])] == existing_index


def test_build_ldc_index_cache_rolls_back_probe_allocations() -> None:
    cp = ConstantPoolBuilder()
    before = cp.build()
    items: list[CodeItem] = [LdcInsn(LdcString("new-value"))]

    cache = _build_ldc_index_cache(items, cp)

    assert cache[id(items[0])] is not None
    assert cp.build() == before


def test_clone_raw_instruction_copies_lookup_switch_pairs_and_offset() -> None:
    original = LookupSwitch(
        InsnInfoType.LOOKUPSWITCH,
        3,
        10,
        2,
        [MatchOffsetPair(1, 20), MatchOffsetPair(2, 30)],
    )

    cloned = clone_raw_instruction(original, bytecode_offset=7)

    assert isinstance(cloned, LookupSwitch)
    assert cloned.bytecode_offset == 7
    assert cloned.pairs == original.pairs
    assert cloned.pairs is not original.pairs


def test_clone_raw_instruction_copies_simple_raw_instruction_with_new_offset() -> None:
    original = LocalIndex(InsnInfoType.ILOAD, 4, 2)

    cloned = clone_raw_instruction(original, bytecode_offset=9)

    assert isinstance(cloned, LocalIndex)
    assert cloned is not original
    assert cloned.type == original.type
    assert cloned.index == original.index
    assert cloned.bytecode_offset == 9


def test_resolve_labels_empty_list() -> None:
    resolution = resolve_labels([])

    assert resolution.total_code_length == 0
    assert resolution.label_offsets == {}
    assert resolution.instruction_offsets == []


def test_resolve_labels_duplicate_label_raises() -> None:
    label = Label("dup")
    items = [
        label,
        InsnInfo(InsnInfoType.NOP, -1),
        label,
    ]

    with pytest.raises(ValueError):
        resolve_labels(items)
