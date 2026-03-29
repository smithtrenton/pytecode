"""Tests for ``pytecode.analysis`` — CFG construction and stack/local simulation."""

from __future__ import annotations

from collections.abc import Sequence
from dataclasses import dataclass
from pathlib import Path

import pytest

from pytecode import attributes, constants
from pytecode.analysis import (
    AnalysisError,
    ControlFlowGraph,
    FrameComputationResult,
    FrameState,
    InvalidLocalError,
    SimulationResult,
    StackUnderflowError,
    TypeMergeError,
    VDouble,
    VFloat,
    VInteger,
    VLong,
    VNull,
    VObject,
    VTop,
    VUninitialized,
    VUninitializedThis,
    build_cfg,
    initial_frame,
    is_category2,
    is_reference,
    merge_vtypes,
    simulate,
    vtype_from_descriptor,
    vtype_from_field_descriptor_str,
)
from pytecode.constant_pool import ClassInfo
from pytecode.constants import MethodAccessFlag
from pytecode.descriptors import ArrayType as DescArrayType
from pytecode.descriptors import BaseType, ObjectType
from pytecode.hierarchy import (
    MappingClassResolver,
    ResolvedClass,
)
from pytecode.instructions import InsnInfo, InsnInfoType
from pytecode.labels import (
    BranchInsn,
    ExceptionHandler,
    Label,
    LookupSwitchInsn,
    TableSwitchInsn,
)
from pytecode.model import ClassModel, CodeModel, MethodModel
from pytecode.operands import (
    FieldInsn,
    IIncInsn,
    InterfaceMethodInsn,
    InvokeDynamicInsn,
    LdcClass,
    LdcDouble,
    LdcFloat,
    LdcInsn,
    LdcInt,
    LdcLong,
    LdcMethodHandle,
    LdcMethodType,
    LdcString,
    MethodInsn,
    MultiANewArrayInsn,
    TypeInsn,
    VarInsn,
)
from tests.helpers import compile_java_resource

# ===================================================================
# Helper factories
# ===================================================================


def _code(*items: InsnInfo | Label, handlers: list[ExceptionHandler] | None = None) -> CodeModel:
    """Build a minimal CodeModel from an instruction stream."""
    return CodeModel(
        max_stack=100,
        max_locals=100,
        instructions=list(items),
        exception_handlers=handlers or [],
    )


def _method(
    name: str = "test",
    descriptor: str = "()V",
    access_flags: MethodAccessFlag = MethodAccessFlag(0),
    code: CodeModel | None = None,
) -> MethodModel:
    """Build a minimal MethodModel."""
    return MethodModel(
        access_flags=access_flags,
        name=name,
        descriptor=descriptor,
        code=code,
        attributes=[],
    )


def _static_method(
    name: str = "test",
    descriptor: str = "()V",
    code: CodeModel | None = None,
) -> MethodModel:
    return _method(name, descriptor, MethodAccessFlag.STATIC, code)


# ===================================================================
# Verification type tests
# ===================================================================


class TestVTypeFromDescriptor:
    def test_int(self) -> None:
        assert vtype_from_descriptor(BaseType.INT) == VInteger()

    def test_short(self) -> None:
        assert vtype_from_descriptor(BaseType.SHORT) == VInteger()

    def test_byte(self) -> None:
        assert vtype_from_descriptor(BaseType.BYTE) == VInteger()

    def test_char(self) -> None:
        assert vtype_from_descriptor(BaseType.CHAR) == VInteger()

    def test_boolean(self) -> None:
        assert vtype_from_descriptor(BaseType.BOOLEAN) == VInteger()

    def test_float(self) -> None:
        assert vtype_from_descriptor(BaseType.FLOAT) == VFloat()

    def test_long(self) -> None:
        assert vtype_from_descriptor(BaseType.LONG) == VLong()

    def test_double(self) -> None:
        assert vtype_from_descriptor(BaseType.DOUBLE) == VDouble()

    def test_object(self) -> None:
        assert vtype_from_descriptor(ObjectType("java/lang/String")) == VObject("java/lang/String")

    def test_array_of_int(self) -> None:
        result = vtype_from_descriptor(DescArrayType(BaseType.INT))
        assert isinstance(result, VObject)
        assert result.class_name == "[I"

    def test_array_of_object(self) -> None:
        result = vtype_from_descriptor(DescArrayType(ObjectType("java/lang/String")))
        assert isinstance(result, VObject)
        assert result.class_name == "[Ljava/lang/String;"

    def test_multidim_array(self) -> None:
        result = vtype_from_descriptor(DescArrayType(DescArrayType(BaseType.INT)))
        assert isinstance(result, VObject)
        assert result.class_name == "[[I"


class TestVTypeFromFieldDescriptorStr:
    def test_int(self) -> None:
        assert vtype_from_field_descriptor_str("I") == VInteger()

    def test_long(self) -> None:
        assert vtype_from_field_descriptor_str("J") == VLong()

    def test_object(self) -> None:
        assert vtype_from_field_descriptor_str("Ljava/lang/String;") == VObject("java/lang/String")

    def test_array(self) -> None:
        result = vtype_from_field_descriptor_str("[I")
        assert result == VObject("[I")


class TestIsCategory2:
    def test_long_is_cat2(self) -> None:
        assert is_category2(VLong()) is True

    def test_double_is_cat2(self) -> None:
        assert is_category2(VDouble()) is True

    def test_int_is_cat1(self) -> None:
        assert is_category2(VInteger()) is False

    def test_object_is_cat1(self) -> None:
        assert is_category2(VObject("java/lang/Object")) is False

    def test_null_is_cat1(self) -> None:
        assert is_category2(VNull()) is False

    def test_top_is_cat1(self) -> None:
        assert is_category2(VTop()) is False


class TestIsReference:
    def test_null(self) -> None:
        assert is_reference(VNull()) is True

    def test_object(self) -> None:
        assert is_reference(VObject("java/lang/Object")) is True

    def test_uninitialized_this(self) -> None:
        assert is_reference(VUninitializedThis()) is True

    def test_uninitialized(self) -> None:
        assert is_reference(VUninitialized(Label("test"))) is True

    def test_integer_not_ref(self) -> None:
        assert is_reference(VInteger()) is False

    def test_long_not_ref(self) -> None:
        assert is_reference(VLong()) is False

    def test_top_not_ref(self) -> None:
        assert is_reference(VTop()) is False


class TestMergeVTypes:
    def test_same_type(self) -> None:
        assert merge_vtypes(VInteger(), VInteger()) == VInteger()

    def test_same_object(self) -> None:
        assert merge_vtypes(VObject("java/lang/String"), VObject("java/lang/String")) == VObject("java/lang/String")

    def test_null_with_object(self) -> None:
        assert merge_vtypes(VNull(), VObject("java/lang/String")) == VObject("java/lang/String")

    def test_object_with_null(self) -> None:
        assert merge_vtypes(VObject("java/lang/String"), VNull()) == VObject("java/lang/String")

    def test_two_objects_no_resolver(self) -> None:
        result = merge_vtypes(VObject("java/lang/String"), VObject("java/lang/Integer"))
        assert result == VObject("java/lang/Object")

    def test_two_objects_with_resolver(self) -> None:
        from pytecode.constants import ClassAccessFlag as CAF

        resolver = MappingClassResolver(
            [
                ResolvedClass("A", "java/lang/Object", (), CAF(0)),
                ResolvedClass("B", "java/lang/Object", (), CAF(0)),
            ]
        )
        result = merge_vtypes(VObject("A"), VObject("B"), resolver)
        assert result == VObject("java/lang/Object")

    def test_incompatible_primitives(self) -> None:
        assert merge_vtypes(VInteger(), VFloat()) == VTop()

    def test_int_vs_long(self) -> None:
        assert merge_vtypes(VInteger(), VLong()) == VTop()

    def test_long_vs_double(self) -> None:
        assert merge_vtypes(VLong(), VDouble()) == VTop()

    def test_primitive_vs_object(self) -> None:
        assert merge_vtypes(VInteger(), VObject("java/lang/Object")) == VTop()

    def test_null_with_null(self) -> None:
        assert merge_vtypes(VNull(), VNull()) == VNull()

    def test_null_with_uninitialized_this(self) -> None:
        result = merge_vtypes(VNull(), VUninitializedThis())
        assert result == VUninitializedThis()


# ===================================================================
# Frame state tests
# ===================================================================


class TestFrameState:
    def test_push_integer(self) -> None:
        fs = FrameState((), ())
        fs2 = fs.push(VInteger())
        assert fs2.stack == (VInteger(),)

    def test_push_long_spans_two_slots(self) -> None:
        fs = FrameState((), ())
        fs2 = fs.push(VLong())
        assert fs2.stack == (VLong(), VTop())
        assert fs2.stack_depth == 2

    def test_push_double_spans_two_slots(self) -> None:
        fs = FrameState((), ())
        fs2 = fs.push(VDouble())
        assert fs2.stack == (VDouble(), VTop())

    def test_pop_integer(self) -> None:
        fs = FrameState((VInteger(),), ())
        fs2, popped = fs.pop(1)
        assert popped == (VInteger(),)
        assert fs2.stack == ()

    def test_pop_two_slots_returns_top_first(self) -> None:
        fs = FrameState((VInteger(), VFloat()), ())
        fs2, popped = fs.pop(2)
        assert popped == (VFloat(), VInteger())
        assert fs2.stack == ()

    def test_pop_underflow(self) -> None:
        fs = FrameState((), ())
        with pytest.raises(StackUnderflowError):
            fs.pop(1)

    def test_pop_zero(self) -> None:
        fs = FrameState((VInteger(),), ())
        fs2, popped = fs.pop(0)
        assert popped == ()
        assert fs2.stack == (VInteger(),)

    def test_peek(self) -> None:
        fs = FrameState((VInteger(), VFloat()), ())
        assert fs.peek(0) == VFloat()
        assert fs.peek(1) == VInteger()

    def test_peek_underflow(self) -> None:
        fs = FrameState((), ())
        with pytest.raises(StackUnderflowError):
            fs.peek(0)

    def test_set_local(self) -> None:
        fs = FrameState((), ())
        fs2 = fs.set_local(0, VInteger())
        assert fs2.locals == (VInteger(),)

    def test_set_local_extends(self) -> None:
        fs = FrameState((), ())
        fs2 = fs.set_local(2, VInteger())
        assert fs2.locals == (VTop(), VTop(), VInteger())

    def test_set_local_long(self) -> None:
        fs = FrameState((), ())
        fs2 = fs.set_local(0, VLong())
        assert fs2.locals == (VLong(), VTop())

    def test_get_local(self) -> None:
        fs = FrameState((), (VInteger(),))
        assert fs.get_local(0) == VInteger()

    def test_get_local_uninitialized(self) -> None:
        fs = FrameState((), (VTop(),))
        with pytest.raises(InvalidLocalError):
            fs.get_local(0)

    def test_get_local_out_of_range(self) -> None:
        fs = FrameState((), ())
        with pytest.raises(InvalidLocalError):
            fs.get_local(0)

    def test_stack_depth(self) -> None:
        fs = FrameState((VInteger(), VLong(), VTop()), ())
        assert fs.stack_depth == 3

    def test_max_local_index(self) -> None:
        fs = FrameState((), (VInteger(), VFloat()))
        assert fs.max_local_index == 1


class TestInitialFrame:
    def test_static_void_method(self) -> None:
        method = _static_method(descriptor="()V")
        fs = initial_frame(method, "MyClass")
        assert fs.stack == ()
        assert fs.locals == ()

    def test_instance_void_method(self) -> None:
        method = _method(descriptor="()V")
        fs = initial_frame(method, "MyClass")
        assert fs.stack == ()
        assert fs.locals == (VObject("MyClass"),)

    def test_init_method(self) -> None:
        method = _method(name="<init>", descriptor="()V")
        fs = initial_frame(method, "MyClass")
        assert fs.stack == ()
        assert fs.locals == (VUninitializedThis(),)

    def test_static_with_int_param(self) -> None:
        method = _static_method(descriptor="(I)V")
        fs = initial_frame(method, "MyClass")
        assert fs.locals == (VInteger(),)

    def test_instance_with_int_param(self) -> None:
        method = _method(descriptor="(I)V")
        fs = initial_frame(method, "MyClass")
        assert fs.locals == (VObject("MyClass"), VInteger())

    def test_long_param_spans_two_slots(self) -> None:
        method = _static_method(descriptor="(J)V")
        fs = initial_frame(method, "MyClass")
        assert fs.locals == (VLong(), VTop())

    def test_double_param_spans_two_slots(self) -> None:
        method = _static_method(descriptor="(D)V")
        fs = initial_frame(method, "MyClass")
        assert fs.locals == (VDouble(), VTop())

    def test_mixed_params(self) -> None:
        method = _method(descriptor="(IJLjava/lang/String;D)V")
        fs = initial_frame(method, "MyClass")
        assert fs.locals == (
            VObject("MyClass"),  # this
            VInteger(),          # int
            VLong(),             # long (slot 2)
            VTop(),              # long high word (slot 3)
            VObject("java/lang/String"),  # String (slot 4)
            VDouble(),           # double (slot 5)
            VTop(),              # double high word (slot 6)
        )


# ===================================================================
# CFG construction tests
# ===================================================================


class TestBuildCfgBasic:
    def test_empty_code(self) -> None:
        code = _code()
        cfg = build_cfg(code)
        assert len(cfg.blocks) == 1
        assert cfg.blocks[0].successor_ids == []

    def test_single_return(self) -> None:
        code = _code(InsnInfo(InsnInfoType.RETURN, 0))
        cfg = build_cfg(code)
        assert len(cfg.blocks) == 1
        assert cfg.blocks[0].successor_ids == []

    def test_straight_line(self) -> None:
        code = _code(
            InsnInfo(InsnInfoType.ICONST_0, 0),
            InsnInfo(InsnInfoType.ISTORE_1, 1),
            InsnInfo(InsnInfoType.RETURN, 2),
        )
        cfg = build_cfg(code)
        assert len(cfg.blocks) == 1
        assert len(cfg.blocks[0].instructions) == 3


class TestBuildCfgBranching:
    def test_if_else(self) -> None:
        l_true = Label("true")
        l_end = Label("end")

        code = _code(
            InsnInfo(InsnInfoType.ILOAD_0, 0),
            BranchInsn(InsnInfoType.IFNE, l_true),
            # false branch
            InsnInfo(InsnInfoType.ICONST_0, 3),
            BranchInsn(InsnInfoType.GOTO, l_end),
            # true branch
            l_true,
            InsnInfo(InsnInfoType.ICONST_1, 7),
            # merge
            l_end,
            InsnInfo(InsnInfoType.IRETURN, 8),
        )
        cfg = build_cfg(code)
        assert len(cfg.blocks) >= 3

        # Entry block has the IFNE — should have 2 successors (true branch + fall-through).
        entry = cfg.blocks[0]
        assert len(entry.successor_ids) == 2

    def test_unconditional_goto_no_fallthrough(self) -> None:
        l_target = Label("target")
        code = _code(
            BranchInsn(InsnInfoType.GOTO, l_target),
            l_target,
            InsnInfo(InsnInfoType.RETURN, 3),
        )
        cfg = build_cfg(code)
        entry = cfg.blocks[0]
        # GOTO is unconditional — only 1 successor (the target), no fall-through.
        assert len(entry.successor_ids) == 1

    def test_non_target_mid_block_label_maps_to_current_block(self) -> None:
        helper = Label("helper")
        target = Label("target")
        code = _code(
            InsnInfo(InsnInfoType.ICONST_0, 0),
            helper,
            InsnInfo(InsnInfoType.POP, 1),
            BranchInsn(InsnInfoType.GOTO, target),
            target,
            InsnInfo(InsnInfoType.RETURN, 4),
        )
        cfg = build_cfg(code)
        assert cfg.label_to_block[helper] is cfg.blocks[0]
        assert cfg.label_to_block[target] is cfg.blocks[1]


class TestBuildCfgSwitch:
    def test_tableswitch(self) -> None:
        l_case0 = Label("case0")
        l_case1 = Label("case1")
        l_default = Label("default")

        code = _code(
            InsnInfo(InsnInfoType.ILOAD_0, 0),
            TableSwitchInsn(l_default, 0, 1, [l_case0, l_case1]),
            l_case0,
            InsnInfo(InsnInfoType.ICONST_0, 10),
            InsnInfo(InsnInfoType.IRETURN, 11),
            l_case1,
            InsnInfo(InsnInfoType.ICONST_1, 15),
            InsnInfo(InsnInfoType.IRETURN, 16),
            l_default,
            InsnInfo(InsnInfoType.ICONST_M1, 20),
            InsnInfo(InsnInfoType.IRETURN, 21),
        )
        cfg = build_cfg(code)
        # Entry block ends with tableswitch — has 3 successors (default + 2 cases).
        entry = cfg.blocks[0]
        assert len(entry.successor_ids) == 3

    def test_lookupswitch(self) -> None:
        l_case100 = Label("case100")
        l_case200 = Label("case200")
        l_default = Label("default")

        code = _code(
            InsnInfo(InsnInfoType.ILOAD_0, 0),
            LookupSwitchInsn(l_default, [(100, l_case100), (200, l_case200)]),
            l_case100,
            InsnInfo(InsnInfoType.ICONST_0, 20),
            InsnInfo(InsnInfoType.IRETURN, 21),
            l_case200,
            InsnInfo(InsnInfoType.ICONST_1, 25),
            InsnInfo(InsnInfoType.IRETURN, 26),
            l_default,
            InsnInfo(InsnInfoType.ICONST_M1, 30),
            InsnInfo(InsnInfoType.IRETURN, 31),
        )
        cfg = build_cfg(code)
        entry = cfg.blocks[0]
        assert len(entry.successor_ids) == 3


class TestBuildCfgExceptionHandlers:
    def test_try_catch_creates_exception_edges(self) -> None:
        l_try_start = Label("try_start")
        l_try_end = Label("try_end")
        l_handler = Label("handler")

        code = _code(
            l_try_start,
            InsnInfo(InsnInfoType.ILOAD_0, 0),
            InsnInfo(InsnInfoType.ILOAD_1, 1),
            InsnInfo(InsnInfoType.IDIV, 2),
            InsnInfo(InsnInfoType.IRETURN, 3),
            l_try_end,
            l_handler,
            InsnInfo(InsnInfoType.POP, 5),
            InsnInfo(InsnInfoType.ICONST_M1, 6),
            InsnInfo(InsnInfoType.IRETURN, 7),
            handlers=[ExceptionHandler(l_try_start, l_try_end, l_handler, "java/lang/ArithmeticException")],
        )
        cfg = build_cfg(code)

        # Find the block in the try range — it should have exception handler edges.
        try_block = cfg.label_to_block.get(l_try_start)
        assert try_block is not None
        assert len(try_block.exception_handler_ids) > 0

        # The handler edge should point to the handler block.
        handler_block = cfg.label_to_block.get(l_handler)
        assert handler_block is not None
        handler_ids = [h[0] for h in try_block.exception_handler_ids]
        assert handler_block.id in handler_ids


class TestBuildCfgTerminal:
    def test_return_is_terminal(self) -> None:
        l_after = Label("after")
        code = _code(
            InsnInfo(InsnInfoType.RETURN, 0),
            l_after,
            InsnInfo(InsnInfoType.ICONST_0, 1),
            InsnInfo(InsnInfoType.IRETURN, 2),
        )
        cfg = build_cfg(code)
        entry = cfg.blocks[0]
        assert entry.successor_ids == []  # RETURN has no successors.

    def test_athrow_is_terminal(self) -> None:
        code = _code(
            InsnInfo(InsnInfoType.ALOAD_0, 0),
            InsnInfo(InsnInfoType.ATHROW, 1),
        )
        cfg = build_cfg(code)
        entry = cfg.blocks[0]
        assert entry.successor_ids == []


class TestBuildCfgLoop:
    def test_loop_has_backedge(self) -> None:
        l_top = Label("top")
        l_end = Label("end")
        code = _code(
            l_top,
            InsnInfo(InsnInfoType.ILOAD_0, 0),
            BranchInsn(InsnInfoType.IFEQ, l_end),
            InsnInfo(InsnInfoType.IINC, 3),
            BranchInsn(InsnInfoType.GOTO, l_top),
            l_end,
            InsnInfo(InsnInfoType.RETURN, 8),
        )
        cfg = build_cfg(code)
        # There should be a block that has l_top as a successor (backedge).
        top_block = cfg.label_to_block.get(l_top)
        assert top_block is not None
        has_backedge = any(
            top_block.id in b.successor_ids
            for b in cfg.blocks
            if b.id != top_block.id
        )
        assert has_backedge


# ===================================================================
# Stack simulation tests — unit-level
# ===================================================================


class TestSimulateConstants:
    def test_iconst_0(self) -> None:
        code = _code(InsnInfo(InsnInfoType.ICONST_0, 0), InsnInfo(InsnInfoType.IRETURN, 1))
        method = _static_method(descriptor="()I", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        assert result.max_stack >= 1

    def test_lconst_0_pushes_2_slots(self) -> None:
        code = _code(InsnInfo(InsnInfoType.LCONST_0, 0), InsnInfo(InsnInfoType.LRETURN, 1))
        method = _static_method(descriptor="()J", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        assert result.max_stack >= 2

    def test_dconst_0_pushes_2_slots(self) -> None:
        code = _code(InsnInfo(InsnInfoType.DCONST_0, 0), InsnInfo(InsnInfoType.DRETURN, 1))
        method = _static_method(descriptor="()D", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        assert result.max_stack >= 2

    def test_aconst_null(self) -> None:
        code = _code(InsnInfo(InsnInfoType.ACONST_NULL, 0), InsnInfo(InsnInfoType.ARETURN, 1))
        method = _static_method(descriptor="()Ljava/lang/Object;", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        # After ARETURN the stack was popped; but the max_stack was 1 (the null).
        assert result.max_stack >= 1

    def test_bipush(self) -> None:
        code = _code(InsnInfo(InsnInfoType.BIPUSH, 0), InsnInfo(InsnInfoType.IRETURN, 2))
        method = _static_method(descriptor="()I", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        assert result.max_stack >= 1


class TestSimulateVarInsn:
    def test_iload_istore(self) -> None:
        code = _code(
            VarInsn(InsnInfoType.ILOAD, 0),
            VarInsn(InsnInfoType.ISTORE, 1),
            InsnInfo(InsnInfoType.RETURN, 4),
        )
        method = _static_method(descriptor="(I)V", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        exit_state = result.exit_states[cfg.entry.id]
        assert exit_state.locals[1] == VInteger()

    def test_lload_pushes_long(self) -> None:
        code = _code(
            VarInsn(InsnInfoType.LLOAD, 0),
            InsnInfo(InsnInfoType.LRETURN, 2),
        )
        method = _static_method(descriptor="(J)J", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        assert result.max_stack >= 2

    def test_aload_preserves_type(self) -> None:
        code = _code(
            VarInsn(InsnInfoType.ALOAD, 0),
            InsnInfo(InsnInfoType.ARETURN, 2),
        )
        method = _method(descriptor="()Ljava/lang/Object;", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        assert result.max_stack >= 1

    def test_dstore_spans_two_local_slots(self) -> None:
        code = _code(
            InsnInfo(InsnInfoType.DCONST_0, 0),
            VarInsn(InsnInfoType.DSTORE, 0),
            InsnInfo(InsnInfoType.RETURN, 3),
        )
        method = _static_method(descriptor="()V", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        exit_state = result.exit_states[cfg.entry.id]
        assert exit_state.locals[0] == VDouble()
        assert exit_state.locals[1] == VTop()


class TestSimulateArithmetic:
    def test_iadd(self) -> None:
        code = _code(
            InsnInfo(InsnInfoType.ICONST_1, 0),
            InsnInfo(InsnInfoType.ICONST_2, 1),
            InsnInfo(InsnInfoType.IADD, 2),
            InsnInfo(InsnInfoType.IRETURN, 3),
        )
        method = _static_method(descriptor="()I", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        assert result.max_stack >= 2

    def test_ladd(self) -> None:
        code = _code(
            InsnInfo(InsnInfoType.LCONST_1, 0),
            InsnInfo(InsnInfoType.LCONST_1, 1),
            InsnInfo(InsnInfoType.LADD, 2),
            InsnInfo(InsnInfoType.LRETURN, 3),
        )
        method = _static_method(descriptor="()J", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        assert result.max_stack >= 4  # Two longs = 4 slots before add.


class TestSimulateConversions:
    def test_i2l(self) -> None:
        code = _code(
            InsnInfo(InsnInfoType.ICONST_1, 0),
            InsnInfo(InsnInfoType.I2L, 1),
            InsnInfo(InsnInfoType.LRETURN, 2),
        )
        method = _static_method(descriptor="()J", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        assert result.max_stack >= 2

    def test_d2i(self) -> None:
        code = _code(
            InsnInfo(InsnInfoType.DCONST_1, 0),
            InsnInfo(InsnInfoType.D2I, 1),
            InsnInfo(InsnInfoType.IRETURN, 2),
        )
        method = _static_method(descriptor="()I", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        assert result.max_stack >= 2  # double on stack before conversion


class TestSimulateStackManipulation:
    def test_dup(self) -> None:
        code = _code(
            InsnInfo(InsnInfoType.ICONST_1, 0),
            InsnInfo(InsnInfoType.DUP, 1),
            InsnInfo(InsnInfoType.IADD, 2),
            InsnInfo(InsnInfoType.IRETURN, 3),
        )
        method = _static_method(descriptor="()I", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        assert result.max_stack >= 2

    def test_swap(self) -> None:
        code = _code(
            InsnInfo(InsnInfoType.ICONST_1, 0),
            InsnInfo(InsnInfoType.ICONST_2, 1),
            InsnInfo(InsnInfoType.SWAP, 2),
            InsnInfo(InsnInfoType.POP, 3),
            InsnInfo(InsnInfoType.IRETURN, 4),
        )
        method = _static_method(descriptor="()I", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        # After SWAP, top is 1 (was pushed first), then popped.
        assert result.max_stack >= 2

    def test_dup2_cat1(self) -> None:
        code = _code(
            InsnInfo(InsnInfoType.ICONST_1, 0),
            InsnInfo(InsnInfoType.ICONST_2, 1),
            InsnInfo(InsnInfoType.DUP2, 2),
            InsnInfo(InsnInfoType.POP2, 3),
            InsnInfo(InsnInfoType.POP2, 4),
            InsnInfo(InsnInfoType.RETURN, 5),
        )
        method = _static_method(descriptor="()V", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        assert result.max_stack >= 4

    def test_pop(self) -> None:
        code = _code(
            InsnInfo(InsnInfoType.ICONST_1, 0),
            InsnInfo(InsnInfoType.POP, 1),
            InsnInfo(InsnInfoType.RETURN, 2),
        )
        method = _static_method(descriptor="()V", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        exit_state = result.exit_states[cfg.entry.id]
        assert exit_state.stack_depth == 0


class TestSimulateFieldInsn:
    def test_getstatic(self) -> None:
        code = _code(
            FieldInsn(InsnInfoType.GETSTATIC, "java/lang/System", "out", "Ljava/io/PrintStream;"),
            InsnInfo(InsnInfoType.ARETURN, 3),
        )
        method = _static_method(descriptor="()Ljava/io/PrintStream;", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        assert result.max_stack >= 1

    def test_getfield(self) -> None:
        code = _code(
            VarInsn(InsnInfoType.ALOAD, 0),
            FieldInsn(InsnInfoType.GETFIELD, "MyClass", "value", "I"),
            InsnInfo(InsnInfoType.IRETURN, 4),
        )
        method = _method(descriptor="()I", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "MyClass")
        assert result.max_stack >= 1

    def test_putfield(self) -> None:
        code = _code(
            VarInsn(InsnInfoType.ALOAD, 0),
            InsnInfo(InsnInfoType.ICONST_5, 2),
            FieldInsn(InsnInfoType.PUTFIELD, "MyClass", "value", "I"),
            InsnInfo(InsnInfoType.RETURN, 6),
        )
        method = _method(descriptor="()V", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "MyClass")
        exit_state = result.exit_states[cfg.entry.id]
        assert exit_state.stack_depth == 0


class TestSimulateMethodInsn:
    def test_invokestatic_void(self) -> None:
        code = _code(
            MethodInsn(InsnInfoType.INVOKESTATIC, "Math", "doSomething", "()V"),
            InsnInfo(InsnInfoType.RETURN, 3),
        )
        method = _static_method(descriptor="()V", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        exit_state = result.exit_states[cfg.entry.id]
        assert exit_state.stack_depth == 0

    def test_invokestatic_returns_int(self) -> None:
        code = _code(
            MethodInsn(InsnInfoType.INVOKESTATIC, "Math", "getVal", "()I"),
            InsnInfo(InsnInfoType.IRETURN, 3),
        )
        method = _static_method(descriptor="()I", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        assert result.max_stack >= 1

    def test_invokevirtual(self) -> None:
        code = _code(
            VarInsn(InsnInfoType.ALOAD, 0),
            MethodInsn(InsnInfoType.INVOKEVIRTUAL, "java/lang/String", "length", "()I"),
            InsnInfo(InsnInfoType.IRETURN, 4),
        )
        method = _method(descriptor="()I", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "java/lang/String")
        assert result.max_stack >= 1

    def test_invokeinterface(self) -> None:
        code = _code(
            VarInsn(InsnInfoType.ALOAD, 0),
            InterfaceMethodInsn("java/util/List", "size", "()I"),
            InsnInfo(InsnInfoType.IRETURN, 5),
        )
        method = _method(descriptor="()I", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        assert result.max_stack >= 1

    def test_invokedynamic(self) -> None:
        code = _code(
            InvokeDynamicInsn(0, "apply", "()Ljava/util/function/Function;"),
            InsnInfo(InsnInfoType.ARETURN, 5),
        )
        method = _static_method(descriptor="()Ljava/util/function/Function;", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        assert result.max_stack >= 1


class TestSimulateTypeInsn:
    def test_checkcast(self) -> None:
        code = _code(
            VarInsn(InsnInfoType.ALOAD, 0),
            TypeInsn(InsnInfoType.CHECKCAST, "java/lang/String"),
            InsnInfo(InsnInfoType.ARETURN, 4),
        )
        method = _method(descriptor="()Ljava/lang/String;", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        assert result.max_stack >= 1

    def test_instanceof(self) -> None:
        code = _code(
            VarInsn(InsnInfoType.ALOAD, 0),
            TypeInsn(InsnInfoType.INSTANCEOF, "java/lang/String"),
            InsnInfo(InsnInfoType.IRETURN, 4),
        )
        method = _method(descriptor="()I", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        assert result.max_stack >= 1

    def test_anewarray(self) -> None:
        code = _code(
            InsnInfo(InsnInfoType.ICONST_5, 0),
            TypeInsn(InsnInfoType.ANEWARRAY, "java/lang/String"),
            InsnInfo(InsnInfoType.ARETURN, 4),
        )
        method = _static_method(descriptor="()[Ljava/lang/String;", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        assert result.max_stack >= 1


class TestSimulateNewInit:
    def test_new_invokespecial_init(self) -> None:
        l_new = Label("new")
        code = _code(
            l_new,
            TypeInsn(InsnInfoType.NEW, "java/lang/Object"),
            InsnInfo(InsnInfoType.DUP, 4),
            MethodInsn(InsnInfoType.INVOKESPECIAL, "java/lang/Object", "<init>", "()V"),
            InsnInfo(InsnInfoType.ARETURN, 8),
        )
        method = _static_method(descriptor="()Ljava/lang/Object;", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        assert result.max_stack >= 2  # NEW + DUP


class TestSimulateLdcInsn:
    def test_ldc_int(self) -> None:
        code = _code(LdcInsn(LdcInt(42)), InsnInfo(InsnInfoType.IRETURN, 2))
        method = _static_method(descriptor="()I", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        assert result.max_stack >= 1

    def test_ldc_float(self) -> None:
        code = _code(LdcInsn(LdcFloat(0x3F800000)), InsnInfo(InsnInfoType.FRETURN, 2))
        method = _static_method(descriptor="()F", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        assert result.max_stack >= 1

    def test_ldc_long(self) -> None:
        code = _code(LdcInsn(LdcLong(100)), InsnInfo(InsnInfoType.LRETURN, 3))
        method = _static_method(descriptor="()J", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        assert result.max_stack >= 2

    def test_ldc_double(self) -> None:
        code = _code(LdcInsn(LdcDouble(0x3FF00000, 0)), InsnInfo(InsnInfoType.DRETURN, 3))
        method = _static_method(descriptor="()D", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        assert result.max_stack >= 2

    def test_ldc_string(self) -> None:
        code = _code(LdcInsn(LdcString("hello")), InsnInfo(InsnInfoType.ARETURN, 2))
        method = _static_method(descriptor="()Ljava/lang/String;", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        assert result.max_stack >= 1

    def test_ldc_class(self) -> None:
        code = _code(LdcInsn(LdcClass("java/lang/String")), InsnInfo(InsnInfoType.ARETURN, 2))
        method = _static_method(descriptor="()Ljava/lang/Class;", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        assert result.max_stack >= 1

    def test_ldc_method_type(self) -> None:
        code = _code(
            LdcInsn(LdcMethodType("()V")),
            InsnInfo(InsnInfoType.ARETURN, 2),
        )
        method = _static_method(descriptor="()Ljava/lang/invoke/MethodType;", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        assert result.max_stack >= 1

    def test_ldc_method_handle(self) -> None:
        code = _code(
            LdcInsn(LdcMethodHandle(6, "java/lang/Object", "hashCode", "()I")),
            InsnInfo(InsnInfoType.ARETURN, 2),
        )
        method = _static_method(descriptor="()Ljava/lang/invoke/MethodHandle;", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        assert result.max_stack >= 1


class TestSimulateIInc:
    def test_iinc_no_stack_change(self) -> None:
        code = _code(
            VarInsn(InsnInfoType.ILOAD, 0),
            VarInsn(InsnInfoType.ISTORE, 1),
            IIncInsn(1, 5),
            VarInsn(InsnInfoType.ILOAD, 1),
            InsnInfo(InsnInfoType.IRETURN, 8),
        )
        method = _static_method(descriptor="(I)I", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        assert result.max_stack >= 1


class TestSimulateMultiANewArray:
    def test_multianewarray(self) -> None:
        code = _code(
            InsnInfo(InsnInfoType.ICONST_3, 0),
            InsnInfo(InsnInfoType.ICONST_4, 1),
            MultiANewArrayInsn("[[I", 2),
            InsnInfo(InsnInfoType.ARETURN, 5),
        )
        method = _static_method(descriptor="()[[I", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        assert result.max_stack >= 2


class TestSimulateArrayOps:
    def test_aaload(self) -> None:
        code = _code(
            VarInsn(InsnInfoType.ALOAD, 0),
            InsnInfo(InsnInfoType.ICONST_0, 2),
            InsnInfo(InsnInfoType.AALOAD, 3),
            VarInsn(InsnInfoType.ASTORE, 1),
            InsnInfo(InsnInfoType.RETURN, 4),
        )
        method = _static_method(descriptor="([Ljava/lang/String;)V", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        exit_state = result.exit_states[cfg.entry.id]
        assert result.max_stack >= 2
        assert exit_state.locals[1] == VObject("java/lang/String")

    def test_arraylength(self) -> None:
        code = _code(
            VarInsn(InsnInfoType.ALOAD, 0),
            InsnInfo(InsnInfoType.ARRAYLENGTH, 2),
            InsnInfo(InsnInfoType.IRETURN, 3),
        )
        method = _static_method(descriptor="([I)I", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        assert result.max_stack >= 1


class TestSimulateMonitor:
    def test_monitorenter_monitorexit(self) -> None:
        code = _code(
            VarInsn(InsnInfoType.ALOAD, 0),
            InsnInfo(InsnInfoType.DUP, 2),
            InsnInfo(InsnInfoType.MONITORENTER, 3),
            InsnInfo(InsnInfoType.MONITOREXIT, 4),
            InsnInfo(InsnInfoType.RETURN, 5),
        )
        method = _method(descriptor="()V", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        exit_state = result.exit_states[cfg.entry.id]
        assert exit_state.stack_depth == 0


class TestSimulateMaxLocals:
    def test_max_locals_with_cat2(self) -> None:
        code = _code(
            InsnInfo(InsnInfoType.DCONST_0, 0),
            VarInsn(InsnInfoType.DSTORE, 2),
            InsnInfo(InsnInfoType.RETURN, 3),
        )
        method = _static_method(descriptor="(I)V", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        # Locals: [0]=int(param), [1]=TOP, [2]=double, [3]=TOP
        assert result.max_locals >= 4


class TestSimulateExceptionHandler:
    def test_handler_entry_state(self) -> None:
        l_try_start = Label("try_start")
        l_try_end = Label("try_end")
        l_handler = Label("handler")

        code = _code(
            l_try_start,
            VarInsn(InsnInfoType.ALOAD, 0),
            FieldInsn(InsnInfoType.GETFIELD, "Test", "value", "I"),
            InsnInfo(InsnInfoType.IRETURN, 3),
            l_try_end,
            l_handler,
            InsnInfo(InsnInfoType.POP, 4),
            InsnInfo(InsnInfoType.ICONST_M1, 5),
            InsnInfo(InsnInfoType.IRETURN, 6),
            handlers=[ExceptionHandler(l_try_start, l_try_end, l_handler, "java/lang/Exception")],
        )
        method = _method(descriptor="()I", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        handler_block = cfg.label_to_block[l_handler]
        handler_entry = result.entry_states.get(handler_block.id)
        assert handler_entry is not None
        # Handler entry stack should have the exception type.
        assert handler_entry.stack_depth == 1

    def test_catch_all_handler_has_throwable(self) -> None:
        l_try_start = Label("try_start")
        l_try_end = Label("try_end")
        l_handler = Label("handler")

        code = _code(
            l_try_start,
            VarInsn(InsnInfoType.ALOAD, 0),
            FieldInsn(InsnInfoType.GETFIELD, "Test", "value", "I"),
            InsnInfo(InsnInfoType.POP, 3),
            InsnInfo(InsnInfoType.RETURN, 4),
            l_try_end,
            l_handler,
            InsnInfo(InsnInfoType.POP, 5),
            InsnInfo(InsnInfoType.RETURN, 6),
            handlers=[ExceptionHandler(l_try_start, l_try_end, l_handler, None)],
        )
        method = _method(descriptor="()V", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        handler_block = cfg.label_to_block[l_handler]
        handler_entry = result.entry_states.get(handler_block.id)
        assert handler_entry is not None
        assert handler_entry.stack[0] == VObject("java/lang/Throwable")

    def test_handler_uses_pre_instruction_locals(self) -> None:
        l_try_start = Label("try_start")
        l_try_end = Label("try_end")
        l_handler = Label("handler")

        code = _code(
            l_try_start,
            VarInsn(InsnInfoType.ALOAD, 0),
            FieldInsn(InsnInfoType.GETFIELD, "Test", "value", "I"),
            VarInsn(InsnInfoType.ISTORE, 1),
            InsnInfo(InsnInfoType.RETURN, 4),
            l_try_end,
            l_handler,
            InsnInfo(InsnInfoType.POP, 5),
            InsnInfo(InsnInfoType.RETURN, 6),
            handlers=[ExceptionHandler(l_try_start, l_try_end, l_handler, "java/lang/Exception")],
        )
        method = _method(descriptor="()V", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        handler_block = cfg.label_to_block[l_handler]
        handler_entry = result.entry_states[handler_block.id]
        with pytest.raises(InvalidLocalError):
            handler_entry.get_local(1)


class TestSimulateBranching:
    def test_if_else_type_merge(self) -> None:
        l_true = Label("true")
        l_end = Label("end")

        code = _code(
            VarInsn(InsnInfoType.ILOAD, 0),
            BranchInsn(InsnInfoType.IFNE, l_true),
            # false: push 0
            InsnInfo(InsnInfoType.ICONST_0, 3),
            BranchInsn(InsnInfoType.GOTO, l_end),
            # true: push 1
            l_true,
            InsnInfo(InsnInfoType.ICONST_1, 7),
            # merge
            l_end,
            InsnInfo(InsnInfoType.IRETURN, 8),
        )
        method = _static_method(descriptor="(I)I", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        # Both paths push an integer; they should merge cleanly.
        end_block = cfg.label_to_block[l_end]
        entry = result.entry_states.get(end_block.id)
        assert entry is not None
        assert entry.stack_depth >= 1

    def test_incompatible_stack_merge_raises(self) -> None:
        l_true = Label("true")
        l_end = Label("end")

        code = _code(
            InsnInfo(InsnInfoType.ICONST_0, 0),
            BranchInsn(InsnInfoType.IFEQ, l_true),
            InsnInfo(InsnInfoType.LCONST_0, 3),
            BranchInsn(InsnInfoType.GOTO, l_end),
            l_true,
            InsnInfo(InsnInfoType.ICONST_1, 6),
            l_end,
            InsnInfo(InsnInfoType.POP, 7),
            InsnInfo(InsnInfoType.RETURN, 8),
        )
        method = _static_method(descriptor="()V", code=code)
        cfg = build_cfg(code)
        with pytest.raises(TypeMergeError, match="Cannot merge incoming frame into block"):
            simulate(cfg, code, method, "Test")


class TestSimulateLoop:
    def test_loop_converges(self) -> None:
        l_top = Label("top")
        l_end = Label("end")

        code = _code(
            InsnInfo(InsnInfoType.ICONST_0, 0),
            VarInsn(InsnInfoType.ISTORE, 1),
            l_top,
            VarInsn(InsnInfoType.ILOAD, 0),
            BranchInsn(InsnInfoType.IFEQ, l_end),
            IIncInsn(1, 1),
            IIncInsn(0, -1),
            BranchInsn(InsnInfoType.GOTO, l_top),
            l_end,
            VarInsn(InsnInfoType.ILOAD, 1),
            InsnInfo(InsnInfoType.IRETURN, 20),
        )
        method = _static_method(descriptor="(I)I", code=code)
        cfg = build_cfg(code)
        result = simulate(cfg, code, method, "Test")
        # The loop should converge; we should have exit states for all reachable blocks.
        assert len(result.exit_states) > 0


# ===================================================================
# Integration tests — compile Java fixtures
# ===================================================================


@dataclass(frozen=True, slots=True)
class ExpectedCfgBlock:
    terminator: InsnInfoType
    successor_ids: frozenset[int]
    exception_handler_ids: frozenset[tuple[int, str | None]] = frozenset()


@pytest.fixture
def cfg_model(tmp_path: Path) -> ClassModel:
    """Compile CfgFixture.java and return its ClassModel."""
    class_path = compile_java_resource(tmp_path, "CfgFixture.java")
    return ClassModel.from_bytes(class_path.read_bytes())


class TestCfgFixtureIntegration:
    """Test CFG construction and simulation on compiled Java bytecode."""

    def _find_method(self, model: ClassModel, name: str) -> MethodModel:
        for m in model.methods:
            if m.name == name:
                return m
        raise AssertionError(f"Method {name!r} not found")

    def _assert_cfg_shape(self, cfg: ControlFlowGraph, expected_blocks: tuple[ExpectedCfgBlock, ...]) -> None:
        assert len(cfg.blocks) == len(expected_blocks)
        assert cfg.entry is cfg.blocks[0]
        for block, expected in zip(cfg.blocks, expected_blocks, strict=True):
            assert block.instructions
            assert block.instructions[-1].type is expected.terminator
            assert frozenset(block.successor_ids) == expected.successor_ids
            assert frozenset(block.exception_handler_ids) == expected.exception_handler_ids

    def test_straight_line_single_block(self, cfg_model: ClassModel) -> None:
        method = self._find_method(cfg_model, "straightLine")
        assert method.code is not None
        cfg = build_cfg(method.code)
        self._assert_cfg_shape(
            cfg,
            (
                ExpectedCfgBlock(InsnInfoType.IRETURN, frozenset()),
            ),
        )

    def test_empty_method_single_block(self, cfg_model: ClassModel) -> None:
        method = self._find_method(cfg_model, "emptyMethod")
        assert method.code is not None
        cfg = build_cfg(method.code)
        self._assert_cfg_shape(
            cfg,
            (
                ExpectedCfgBlock(InsnInfoType.RETURN, frozenset()),
            ),
        )

    def test_if_else_has_branch(self, cfg_model: ClassModel) -> None:
        method = self._find_method(cfg_model, "ifElse")
        assert method.code is not None
        cfg = build_cfg(method.code)
        self._assert_cfg_shape(
            cfg,
            (
                ExpectedCfgBlock(InsnInfoType.IFEQ, frozenset({1, 2})),
                ExpectedCfgBlock(InsnInfoType.IRETURN, frozenset()),
                ExpectedCfgBlock(InsnInfoType.IRETURN, frozenset()),
            ),
        )

    def test_if_no_else_has_fallthrough_merge(self, cfg_model: ClassModel) -> None:
        method = self._find_method(cfg_model, "ifNoElse")
        assert method.code is not None
        cfg = build_cfg(method.code)
        self._assert_cfg_shape(
            cfg,
            (
                ExpectedCfgBlock(InsnInfoType.IFLE, frozenset({1, 2})),
                ExpectedCfgBlock(InsnInfoType.ISTORE, frozenset({2})),
                ExpectedCfgBlock(InsnInfoType.IRETURN, frozenset()),
            ),
        )

    def test_for_loop_has_backedge(self, cfg_model: ClassModel) -> None:
        method = self._find_method(cfg_model, "forLoop")
        assert method.code is not None
        cfg = build_cfg(method.code)
        self._assert_cfg_shape(
            cfg,
            (
                ExpectedCfgBlock(InsnInfoType.ISTORE, frozenset({1})),
                ExpectedCfgBlock(InsnInfoType.IF_ICMPGE, frozenset({2, 3})),
                ExpectedCfgBlock(InsnInfoType.GOTO, frozenset({1})),
                ExpectedCfgBlock(InsnInfoType.IRETURN, frozenset()),
            ),
        )

    def test_while_loop_has_backedge(self, cfg_model: ClassModel) -> None:
        method = self._find_method(cfg_model, "whileLoop")
        assert method.code is not None
        cfg = build_cfg(method.code)
        self._assert_cfg_shape(
            cfg,
            (
                ExpectedCfgBlock(InsnInfoType.ISTORE, frozenset({1})),
                ExpectedCfgBlock(InsnInfoType.IFLE, frozenset({2, 3})),
                ExpectedCfgBlock(InsnInfoType.GOTO, frozenset({1})),
                ExpectedCfgBlock(InsnInfoType.IRETURN, frozenset()),
            ),
        )

    def test_dense_switch_has_cases(self, cfg_model: ClassModel) -> None:
        method = self._find_method(cfg_model, "denseSwitch")
        assert method.code is not None
        cfg = build_cfg(method.code)
        self._assert_cfg_shape(
            cfg,
            (
                ExpectedCfgBlock(InsnInfoType.TABLESWITCH, frozenset({1, 2, 3, 4, 5})),
                ExpectedCfgBlock(InsnInfoType.IRETURN, frozenset()),
                ExpectedCfgBlock(InsnInfoType.IRETURN, frozenset()),
                ExpectedCfgBlock(InsnInfoType.IRETURN, frozenset()),
                ExpectedCfgBlock(InsnInfoType.IRETURN, frozenset()),
                ExpectedCfgBlock(InsnInfoType.IRETURN, frozenset()),
            ),
        )

    def test_sparse_switch_has_cases(self, cfg_model: ClassModel) -> None:
        method = self._find_method(cfg_model, "sparseSwitch")
        assert method.code is not None
        cfg = build_cfg(method.code)
        self._assert_cfg_shape(
            cfg,
            (
                ExpectedCfgBlock(InsnInfoType.LOOKUPSWITCH, frozenset({1, 2, 3, 4, 5})),
                ExpectedCfgBlock(InsnInfoType.IRETURN, frozenset()),
                ExpectedCfgBlock(InsnInfoType.IRETURN, frozenset()),
                ExpectedCfgBlock(InsnInfoType.IRETURN, frozenset()),
                ExpectedCfgBlock(InsnInfoType.IRETURN, frozenset()),
                ExpectedCfgBlock(InsnInfoType.IRETURN, frozenset()),
            ),
        )

    def test_try_catch_has_handler_edges(self, cfg_model: ClassModel) -> None:
        method = self._find_method(cfg_model, "tryCatchSingle")
        assert method.code is not None
        cfg = build_cfg(method.code)
        self._assert_cfg_shape(
            cfg,
            (
                ExpectedCfgBlock(
                    InsnInfoType.IDIV,
                    frozenset({1}),
                    frozenset({(2, "java/lang/ArithmeticException")}),
                ),
                ExpectedCfgBlock(InsnInfoType.IRETURN, frozenset()),
                ExpectedCfgBlock(InsnInfoType.IRETURN, frozenset()),
            ),
        )

    def test_multiple_handlers(self, cfg_model: ClassModel) -> None:
        method = self._find_method(cfg_model, "tryCatchMultiple")
        assert method.code is not None
        cfg = build_cfg(method.code)
        self._assert_cfg_shape(
            cfg,
            (
                ExpectedCfgBlock(
                    InsnInfoType.INVOKEVIRTUAL,
                    frozenset({1}),
                    frozenset(
                        {
                            (2, "java/lang/NullPointerException"),
                            (3, "java/lang/RuntimeException"),
                        }
                    ),
                ),
                ExpectedCfgBlock(InsnInfoType.ARETURN, frozenset()),
                ExpectedCfgBlock(InsnInfoType.ARETURN, frozenset()),
                ExpectedCfgBlock(InsnInfoType.ARETURN, frozenset()),
            ),
        )

    def test_try_catch_finally(self, cfg_model: ClassModel) -> None:
        method = self._find_method(cfg_model, "tryCatchFinally")
        assert method.code is not None
        cfg = build_cfg(method.code)
        self._assert_cfg_shape(
            cfg,
            (
                ExpectedCfgBlock(
                    InsnInfoType.ISTORE,
                    frozenset({1}),
                    frozenset({(2, "java/lang/ArithmeticException"), (4, None)}),
                ),
                ExpectedCfgBlock(InsnInfoType.GOTO, frozenset({6})),
                ExpectedCfgBlock(
                    InsnInfoType.ISTORE,
                    frozenset({3}),
                    frozenset({(4, None)}),
                ),
                ExpectedCfgBlock(InsnInfoType.GOTO, frozenset({6})),
                ExpectedCfgBlock(
                    InsnInfoType.ASTORE,
                    frozenset({5}),
                    frozenset({(4, None)}),
                ),
                ExpectedCfgBlock(InsnInfoType.ATHROW, frozenset()),
                ExpectedCfgBlock(InsnInfoType.IRETURN, frozenset()),
            ),
        )

    def test_nested_try_catch(self, cfg_model: ClassModel) -> None:
        method = self._find_method(cfg_model, "nestedTryCatch")
        assert method.code is not None
        cfg = build_cfg(method.code)
        self._assert_cfg_shape(
            cfg,
            (
                ExpectedCfgBlock(
                    InsnInfoType.IDIV,
                    frozenset({1}),
                    frozenset(
                        {
                            (2, "java/lang/ArithmeticException"),
                            (4, "java/lang/ArithmeticException"),
                        }
                    ),
                ),
                ExpectedCfgBlock(InsnInfoType.IRETURN, frozenset()),
                ExpectedCfgBlock(
                    InsnInfoType.IDIV,
                    frozenset({3}),
                    frozenset({(4, "java/lang/ArithmeticException")}),
                ),
                ExpectedCfgBlock(InsnInfoType.IRETURN, frozenset()),
                ExpectedCfgBlock(InsnInfoType.IRETURN, frozenset()),
            ),
        )

    def test_multiple_returns(self, cfg_model: ClassModel) -> None:
        method = self._find_method(cfg_model, "multipleReturns")
        assert method.code is not None
        cfg = build_cfg(method.code)
        self._assert_cfg_shape(
            cfg,
            (
                ExpectedCfgBlock(InsnInfoType.IFGE, frozenset({1, 2})),
                ExpectedCfgBlock(InsnInfoType.IRETURN, frozenset()),
                ExpectedCfgBlock(InsnInfoType.IFNE, frozenset({3, 4})),
                ExpectedCfgBlock(InsnInfoType.IRETURN, frozenset()),
                ExpectedCfgBlock(InsnInfoType.IF_ICMPLE, frozenset({5, 6})),
                ExpectedCfgBlock(InsnInfoType.IRETURN, frozenset()),
                ExpectedCfgBlock(InsnInfoType.IRETURN, frozenset()),
            ),
        )

    def test_athrow_method(self, cfg_model: ClassModel) -> None:
        method = self._find_method(cfg_model, "throwException")
        assert method.code is not None
        cfg = build_cfg(method.code)
        self._assert_cfg_shape(
            cfg,
            (
                ExpectedCfgBlock(InsnInfoType.ATHROW, frozenset()),
            ),
        )


class TestSimulationFixtureIntegration:
    """Test stack simulation on compiled Java bytecode."""

    def _find_method(self, model: ClassModel, name: str) -> MethodModel:
        for m in model.methods:
            if m.name == name:
                return m
        raise AssertionError(f"Method {name!r} not found")

    def _simulate_method(
        self, model: ClassModel, method_name: str
    ) -> SimulationResult:
        method = self._find_method(model, method_name)
        assert method.code is not None
        cfg = build_cfg(method.code)
        return simulate(cfg, method.code, method, model.name)

    def test_straight_line(self, cfg_model: ClassModel) -> None:
        result = self._simulate_method(cfg_model, "straightLine")
        assert result.max_stack >= 1

    def test_empty_method(self, cfg_model: ClassModel) -> None:
        result = self._simulate_method(cfg_model, "emptyMethod")
        assert result.max_stack >= 0

    def test_if_else(self, cfg_model: ClassModel) -> None:
        result = self._simulate_method(cfg_model, "ifElse")
        assert result.max_stack >= 1

    def test_for_loop(self, cfg_model: ClassModel) -> None:
        result = self._simulate_method(cfg_model, "forLoop")
        assert result.max_stack >= 1

    def test_while_loop(self, cfg_model: ClassModel) -> None:
        result = self._simulate_method(cfg_model, "whileLoop")
        assert result.max_stack >= 1

    def test_nested_loops(self, cfg_model: ClassModel) -> None:
        result = self._simulate_method(cfg_model, "nestedLoops")
        assert result.max_stack >= 1

    def test_dense_switch(self, cfg_model: ClassModel) -> None:
        result = self._simulate_method(cfg_model, "denseSwitch")
        assert result.max_stack >= 1

    def test_sparse_switch(self, cfg_model: ClassModel) -> None:
        result = self._simulate_method(cfg_model, "sparseSwitch")
        assert result.max_stack >= 1

    def test_try_catch_single(self, cfg_model: ClassModel) -> None:
        result = self._simulate_method(cfg_model, "tryCatchSingle")
        assert result.max_stack >= 1

    def test_try_catch_multiple(self, cfg_model: ClassModel) -> None:
        result = self._simulate_method(cfg_model, "tryCatchMultiple")
        assert result.max_stack >= 1

    def test_try_catch_finally(self, cfg_model: ClassModel) -> None:
        result = self._simulate_method(cfg_model, "tryCatchFinally")
        assert result.max_stack >= 1

    def test_nested_try_catch(self, cfg_model: ClassModel) -> None:
        result = self._simulate_method(cfg_model, "nestedTryCatch")
        assert result.max_stack >= 1

    def test_long_arithmetic(self, cfg_model: ClassModel) -> None:
        result = self._simulate_method(cfg_model, "longArithmetic")
        assert result.max_stack >= 2  # At least one long on the stack.

    def test_double_arithmetic(self, cfg_model: ClassModel) -> None:
        result = self._simulate_method(cfg_model, "doubleArithmetic")
        assert result.max_stack >= 2

    def test_mixed_conversions(self, cfg_model: ClassModel) -> None:
        result = self._simulate_method(cfg_model, "mixedConversions")
        assert result.max_stack >= 2

    def test_create_object(self, cfg_model: ClassModel) -> None:
        result = self._simulate_method(cfg_model, "createObject")
        assert result.max_stack >= 2  # NEW + DUP

    def test_create_string(self, cfg_model: ClassModel) -> None:
        result = self._simulate_method(cfg_model, "createString")
        assert result.max_stack >= 2

    def test_create_int_array(self, cfg_model: ClassModel) -> None:
        result = self._simulate_method(cfg_model, "createIntArray")
        assert result.max_stack >= 1

    def test_create_string_array(self, cfg_model: ClassModel) -> None:
        result = self._simulate_method(cfg_model, "createStringArray")
        assert result.max_stack >= 1

    def test_array_access(self, cfg_model: ClassModel) -> None:
        result = self._simulate_method(cfg_model, "arrayAccess")
        assert result.max_stack >= 2  # arrayref + index

    def test_array_store(self, cfg_model: ClassModel) -> None:
        result = self._simulate_method(cfg_model, "arrayStore")
        assert result.max_stack >= 3  # arrayref + index + value

    def test_is_string(self, cfg_model: ClassModel) -> None:
        result = self._simulate_method(cfg_model, "isString")
        assert result.max_stack >= 1

    def test_cast_to_string(self, cfg_model: ClassModel) -> None:
        result = self._simulate_method(cfg_model, "castToString")
        assert result.max_stack >= 1

    def test_multiple_returns(self, cfg_model: ClassModel) -> None:
        result = self._simulate_method(cfg_model, "multipleReturns")
        assert result.max_stack >= 1

    def test_method_calls(self, cfg_model: ClassModel) -> None:
        result = self._simulate_method(cfg_model, "methodCalls")
        assert result.max_stack >= 1

    def test_synchronized_method(self, cfg_model: ClassModel) -> None:
        result = self._simulate_method(cfg_model, "synchronizedMethod")
        assert result.max_stack >= 1

    def test_null_check(self, cfg_model: ClassModel) -> None:
        result = self._simulate_method(cfg_model, "nullCheck")
        assert result.max_stack >= 1

    def test_complex_condition(self, cfg_model: ClassModel) -> None:
        result = self._simulate_method(cfg_model, "complexCondition")
        assert result.max_stack >= 1

    def test_create_2d_array(self, cfg_model: ClassModel) -> None:
        result = self._simulate_method(cfg_model, "create2DArray")
        assert result.max_stack >= 2

    def test_void_method(self, cfg_model: ClassModel) -> None:
        result = self._simulate_method(cfg_model, "voidMethod")
        assert result.max_stack >= 1

    def test_compare_longs(self, cfg_model: ClassModel) -> None:
        result = self._simulate_method(cfg_model, "compareLongs")
        assert result.max_stack >= 2

    def test_compare_doubles(self, cfg_model: ClassModel) -> None:
        result = self._simulate_method(cfg_model, "compareDoubles")
        assert result.max_stack >= 2

    def test_throw_exception(self, cfg_model: ClassModel) -> None:
        result = self._simulate_method(cfg_model, "throwException")
        assert result.max_stack >= 2  # NEW + DUP

    def test_static_initializer(self, cfg_model: ClassModel) -> None:
        result = self._simulate_method(cfg_model, "<clinit>")
        assert result.max_stack >= 0

    def test_ifno_else(self, cfg_model: ClassModel) -> None:
        result = self._simulate_method(cfg_model, "ifNoElse")
        assert result.max_stack >= 1


# ===================================================================
# Error condition tests
# ===================================================================


class TestAnalysisErrors:
    def test_stack_underflow_error_is_analysis_error(self) -> None:
        assert issubclass(StackUnderflowError, AnalysisError)

    def test_invalid_local_error_is_analysis_error(self) -> None:
        assert issubclass(InvalidLocalError, AnalysisError)

    def test_type_merge_error_is_analysis_error(self) -> None:
        assert issubclass(TypeMergeError, AnalysisError)


# ===================================================================
# VType → VerificationTypeInfo conversion tests
# ===================================================================


class TestVTypeToVerificationTypeInfo:
    """Tests for ``_vtype_to_vti`` — converting analysis VTypes to raw attribute types."""

    def setup_method(self) -> None:
        from pytecode.constant_pool_builder import ConstantPoolBuilder

        self.cp = ConstantPoolBuilder()
        self.label_offsets: dict[Label, int] = {}

    def _convert(
        self,
        vtype: VTop | VInteger | VFloat | VLong | VDouble | VNull | VObject | VUninitializedThis | VUninitialized,
    ) -> attributes.VerificationTypeInfo:
        from pytecode.analysis import _vtype_to_vti

        return _vtype_to_vti(vtype, self.cp, self.label_offsets)

    def test_top(self) -> None:
        result = self._convert(VTop())
        assert isinstance(result, attributes.TopVariableInfo)
        assert result.tag == constants.VerificationType.TOP

    def test_integer(self) -> None:
        result = self._convert(VInteger())
        assert isinstance(result, attributes.IntegerVariableInfo)
        assert result.tag == constants.VerificationType.INTEGER

    def test_float(self) -> None:
        result = self._convert(VFloat())
        assert isinstance(result, attributes.FloatVariableInfo)
        assert result.tag == constants.VerificationType.FLOAT

    def test_long(self) -> None:
        result = self._convert(VLong())
        assert isinstance(result, attributes.LongVariableInfo)
        assert result.tag == constants.VerificationType.LONG

    def test_double(self) -> None:
        result = self._convert(VDouble())
        assert isinstance(result, attributes.DoubleVariableInfo)
        assert result.tag == constants.VerificationType.DOUBLE

    def test_null(self) -> None:
        result = self._convert(VNull())
        assert isinstance(result, attributes.NullVariableInfo)
        assert result.tag == constants.VerificationType.NULL

    def test_uninitialized_this(self) -> None:
        result = self._convert(VUninitializedThis())
        assert isinstance(result, attributes.UninitializedThisVariableInfo)
        assert result.tag == constants.VerificationType.UNINITIALIZED_THIS

    def test_object(self) -> None:
        result = self._convert(VObject("java/lang/String"))
        assert isinstance(result, attributes.ObjectVariableInfo)
        assert result.tag == constants.VerificationType.OBJECT
        assert result.cpool_index > 0

    def test_object_allocates_class_entry(self) -> None:
        self._convert(VObject("com/example/Foo"))
        # Verify the CP entry was created
        pool = self.cp.build()
        found = False
        for entry in pool:
            if entry is not None and isinstance(entry, ClassInfo):
                name = self.cp.resolve_utf8(entry.name_index)
                if name == "com/example/Foo":
                    found = True
                    break
        assert found

    def test_uninitialized(self) -> None:
        label = Label()
        self.label_offsets[label] = 42
        result = self._convert(VUninitialized(label))
        assert isinstance(result, attributes.UninitializedVariableInfo)
        assert result.tag == constants.VerificationType.UNINITIALIZED
        assert result.offset == 42

    def test_uninitialized_missing_label_raises(self) -> None:
        label = Label()
        with pytest.raises(ValueError, match="missing bytecode offset"):
            self._convert(VUninitialized(label))


# ===================================================================
# Compact frame encoding selection tests
# ===================================================================


class TestSelectFrame:
    """Tests for ``_select_frame`` — compact StackMapTable encoding selection."""

    def _select(
        self,
        offset_delta: int,
        prev_locals: Sequence[attributes.VerificationTypeInfo],
        curr_locals: Sequence[attributes.VerificationTypeInfo],
        curr_stack: Sequence[attributes.VerificationTypeInfo] | None = None,
    ) -> attributes.StackMapFrameInfo:
        from pytecode.analysis import _select_frame

        return _select_frame(offset_delta, prev_locals, curr_locals, curr_stack or [])

    def test_same_frame_small_delta(self) -> None:
        locals_ = [attributes.IntegerVariableInfo(constants.VerificationType.INTEGER)]
        result = self._select(5, locals_, locals_)
        assert isinstance(result, attributes.SameFrameInfo)
        assert result.frame_type == 5

    def test_same_frame_zero_delta(self) -> None:
        locals_ = [attributes.IntegerVariableInfo(constants.VerificationType.INTEGER)]
        result = self._select(0, locals_, locals_)
        assert isinstance(result, attributes.SameFrameInfo)
        assert result.frame_type == 0

    def test_same_frame_max_small_delta(self) -> None:
        locals_: list[attributes.VerificationTypeInfo] = []
        result = self._select(63, locals_, locals_)
        assert isinstance(result, attributes.SameFrameInfo)
        assert result.frame_type == 63

    def test_same_frame_extended(self) -> None:
        locals_ = [attributes.IntegerVariableInfo(constants.VerificationType.INTEGER)]
        result = self._select(64, locals_, locals_)
        assert isinstance(result, attributes.SameFrameExtendedInfo)
        assert result.frame_type == 251
        assert result.offset_delta == 64

    def test_same_locals_1_stack_item_small_delta(self) -> None:
        locals_ = [attributes.IntegerVariableInfo(constants.VerificationType.INTEGER)]
        stack = [attributes.FloatVariableInfo(constants.VerificationType.FLOAT)]
        result = self._select(10, locals_, locals_, stack)
        assert isinstance(result, attributes.SameLocals1StackItemFrameInfo)
        assert result.frame_type == 74  # 64 + 10
        assert result.stack == stack[0]

    def test_same_locals_1_stack_item_extended(self) -> None:
        locals_ = [attributes.IntegerVariableInfo(constants.VerificationType.INTEGER)]
        stack = [attributes.FloatVariableInfo(constants.VerificationType.FLOAT)]
        result = self._select(100, locals_, locals_, stack)
        assert isinstance(result, attributes.SameLocals1StackItemFrameExtendedInfo)
        assert result.frame_type == 247
        assert result.offset_delta == 100

    def test_chop_1_local(self) -> None:
        prev = [
            attributes.IntegerVariableInfo(constants.VerificationType.INTEGER),
            attributes.FloatVariableInfo(constants.VerificationType.FLOAT),
        ]
        curr = [attributes.IntegerVariableInfo(constants.VerificationType.INTEGER)]
        result = self._select(5, prev, curr)
        assert isinstance(result, attributes.ChopFrameInfo)
        assert result.frame_type == 250  # 251 + (-1)
        assert result.offset_delta == 5

    def test_chop_3_locals(self) -> None:
        prev = [
            attributes.IntegerVariableInfo(constants.VerificationType.INTEGER),
            attributes.FloatVariableInfo(constants.VerificationType.FLOAT),
            attributes.DoubleVariableInfo(constants.VerificationType.DOUBLE),
            attributes.LongVariableInfo(constants.VerificationType.LONG),
        ]
        curr = [attributes.IntegerVariableInfo(constants.VerificationType.INTEGER)]
        result = self._select(5, prev, curr)
        assert isinstance(result, attributes.ChopFrameInfo)
        assert result.frame_type == 248  # 251 + (-3)

    def test_append_1_local(self) -> None:
        prev = [attributes.IntegerVariableInfo(constants.VerificationType.INTEGER)]
        curr = [
            attributes.IntegerVariableInfo(constants.VerificationType.INTEGER),
            attributes.FloatVariableInfo(constants.VerificationType.FLOAT),
        ]
        result = self._select(5, prev, curr)
        assert isinstance(result, attributes.AppendFrameInfo)
        assert result.frame_type == 252  # 251 + 1
        assert result.offset_delta == 5
        assert len(result.locals) == 1
        assert isinstance(result.locals[0], attributes.FloatVariableInfo)

    def test_append_3_locals(self) -> None:
        prev = [attributes.IntegerVariableInfo(constants.VerificationType.INTEGER)]
        curr = [
            attributes.IntegerVariableInfo(constants.VerificationType.INTEGER),
            attributes.FloatVariableInfo(constants.VerificationType.FLOAT),
            attributes.DoubleVariableInfo(constants.VerificationType.DOUBLE),
            attributes.LongVariableInfo(constants.VerificationType.LONG),
        ]
        result = self._select(5, prev, curr)
        assert isinstance(result, attributes.AppendFrameInfo)
        assert result.frame_type == 254  # 251 + 3

    def test_full_frame_different_locals_with_stack(self) -> None:
        prev = [attributes.IntegerVariableInfo(constants.VerificationType.INTEGER)]
        curr = [attributes.FloatVariableInfo(constants.VerificationType.FLOAT)]
        stack = [attributes.NullVariableInfo(constants.VerificationType.NULL)]
        result = self._select(5, prev, curr, stack)
        assert isinstance(result, attributes.FullFrameInfo)
        assert result.frame_type == 255
        assert result.number_of_locals == 1
        assert result.number_of_stack_items == 1

    def test_full_frame_multiple_stack_items(self) -> None:
        locals_ = [attributes.IntegerVariableInfo(constants.VerificationType.INTEGER)]
        stack = [
            attributes.IntegerVariableInfo(constants.VerificationType.INTEGER),
            attributes.FloatVariableInfo(constants.VerificationType.FLOAT),
        ]
        result = self._select(5, locals_, locals_, stack)
        assert isinstance(result, attributes.FullFrameInfo)
        assert result.frame_type == 255

    def test_full_frame_chop_more_than_3(self) -> None:
        """Chopping > 3 locals requires a full_frame."""
        prev = [
            attributes.IntegerVariableInfo(constants.VerificationType.INTEGER),
            attributes.FloatVariableInfo(constants.VerificationType.FLOAT),
            attributes.DoubleVariableInfo(constants.VerificationType.DOUBLE),
            attributes.LongVariableInfo(constants.VerificationType.LONG),
            attributes.NullVariableInfo(constants.VerificationType.NULL),
        ]
        curr = [attributes.IntegerVariableInfo(constants.VerificationType.INTEGER)]
        result = self._select(5, prev, curr)
        assert isinstance(result, attributes.FullFrameInfo)

    def test_full_frame_append_more_than_3(self) -> None:
        """Appending > 3 locals requires a full_frame."""
        prev = [attributes.IntegerVariableInfo(constants.VerificationType.INTEGER)]
        curr = [
            attributes.IntegerVariableInfo(constants.VerificationType.INTEGER),
            attributes.FloatVariableInfo(constants.VerificationType.FLOAT),
            attributes.DoubleVariableInfo(constants.VerificationType.DOUBLE),
            attributes.LongVariableInfo(constants.VerificationType.LONG),
            attributes.NullVariableInfo(constants.VerificationType.NULL),
        ]
        result = self._select(5, prev, curr)
        assert isinstance(result, attributes.FullFrameInfo)

    def test_chop_requires_prefix_match(self) -> None:
        """If the remaining locals differ, fall through to full_frame."""
        prev = [
            attributes.IntegerVariableInfo(constants.VerificationType.INTEGER),
            attributes.FloatVariableInfo(constants.VerificationType.FLOAT),
        ]
        curr = [attributes.DoubleVariableInfo(constants.VerificationType.DOUBLE)]
        result = self._select(5, prev, curr)
        assert isinstance(result, attributes.FullFrameInfo)

    def test_append_requires_prefix_match(self) -> None:
        """If the shared prefix doesn't match, fall through to full_frame."""
        prev = [attributes.IntegerVariableInfo(constants.VerificationType.INTEGER)]
        curr = [
            attributes.FloatVariableInfo(constants.VerificationType.FLOAT),
            attributes.DoubleVariableInfo(constants.VerificationType.DOUBLE),
        ]
        result = self._select(5, prev, curr)
        assert isinstance(result, attributes.FullFrameInfo)


# ===================================================================
# compute_maxs tests
# ===================================================================


class TestComputeMaxs:
    """Tests for ``compute_maxs`` — recomputing max_stack/max_locals."""

    def test_simple_return(self) -> None:
        from pytecode.analysis import compute_maxs

        code = _code(InsnInfo(InsnInfoType.RETURN, 0))
        method = _static_method(descriptor="()V", code=code)
        ms, ml = compute_maxs(code, method, "Test")
        assert ms == 0
        assert ml == 0

    def test_iconst_ireturn(self) -> None:
        from pytecode.analysis import compute_maxs

        code = _code(
            InsnInfo(InsnInfoType.ICONST_1, 0),
            InsnInfo(InsnInfoType.IRETURN, 1),
        )
        method = _static_method(descriptor="()I", code=code)
        ms, ml = compute_maxs(code, method, "Test")
        assert ms == 1
        assert ml == 0

    def test_instance_method_has_this_in_locals(self) -> None:
        from pytecode.analysis import compute_maxs

        code = _code(InsnInfo(InsnInfoType.RETURN, 0))
        method = _method(descriptor="()V", code=code)
        ms, ml = compute_maxs(code, method, "Test")
        assert ms == 0
        assert ml == 1  # slot 0 = this

    def test_method_with_params(self) -> None:
        from pytecode.analysis import compute_maxs

        code = _code(InsnInfo(InsnInfoType.RETURN, 0))
        method = _static_method(descriptor="(IJD)V", code=code)
        ms, ml = compute_maxs(code, method, "Test")
        assert ms == 0
        assert ml == 5  # int(1) + long(2) + double(2)

    def test_local_store_increases_max_locals(self) -> None:
        from pytecode.analysis import compute_maxs

        code = _code(
            InsnInfo(InsnInfoType.ICONST_0, 0),
            VarInsn(InsnInfoType.ISTORE, 5),
            InsnInfo(InsnInfoType.RETURN, 2),
        )
        method = _static_method(descriptor="()V", code=code)
        ms, ml = compute_maxs(code, method, "Test")
        assert ms == 1
        assert ml == 6  # slot 5 + 1

    def test_with_branch(self) -> None:
        from pytecode.analysis import compute_maxs

        label = Label()
        code = _code(
            InsnInfo(InsnInfoType.ICONST_0, 0),
            BranchInsn(InsnInfoType.IFEQ, label),
            InsnInfo(InsnInfoType.ICONST_1, 1),
            InsnInfo(InsnInfoType.IRETURN, 2),
            label,
            InsnInfo(InsnInfoType.ICONST_0, 3),
            InsnInfo(InsnInfoType.IRETURN, 4),
        )
        method = _static_method(descriptor="()I", code=code)
        ms, ml = compute_maxs(code, method, "Test")
        assert ms >= 1
        assert ml == 0


# ===================================================================
# compute_frames tests
# ===================================================================



class TestComputeFrames:
    """Tests for ``compute_frames`` — StackMapTable generation."""

    def setup_method(self) -> None:
        from pytecode.constant_pool_builder import ConstantPoolBuilder

        self.cp = ConstantPoolBuilder()

    def _compute(
        self,
        code: CodeModel,
        method: MethodModel,
        class_name: str = "Test",
    ) -> FrameComputationResult:
        from pytecode.analysis import compute_frames
        from pytecode.labels import resolve_labels

        items = list(code.instructions)
        resolution = resolve_labels(items, self.cp)
        return compute_frames(code, method, class_name, self.cp, resolution.label_offsets)

    def test_linear_code_no_frames(self) -> None:
        """A method with no branches should produce no StackMapTable."""
        code = _code(
            InsnInfo(InsnInfoType.ICONST_0, 0),
            InsnInfo(InsnInfoType.IRETURN, 1),
        )
        method = _static_method(descriptor="()I", code=code)
        result = self._compute(code, method)
        assert result.max_stack == 1
        assert result.max_locals == 0
        assert result.stack_map_table is None

    def test_if_else_branch(self) -> None:
        """An if/else should produce a frame at the else target."""
        else_label = Label()
        end_label = Label()
        code = _code(
            InsnInfo(InsnInfoType.ICONST_0, 0),
            BranchInsn(InsnInfoType.IFEQ, else_label),
            InsnInfo(InsnInfoType.ICONST_1, 1),
            BranchInsn(InsnInfoType.GOTO, end_label),
            else_label,
            InsnInfo(InsnInfoType.ICONST_0, 3),
            end_label,
            InsnInfo(InsnInfoType.IRETURN, 4),
        )
        method = _static_method(descriptor="()I", code=code)
        result = self._compute(code, method)
        assert result.stack_map_table is not None
        assert result.stack_map_table.number_of_entries >= 1

    def test_loop(self) -> None:
        """A loop should produce a frame at the loop header."""
        loop_label = Label()
        end_label = Label()
        code = _code(
            loop_label,
            InsnInfo(InsnInfoType.ICONST_0, 0),
            BranchInsn(InsnInfoType.IFNE, end_label),
            BranchInsn(InsnInfoType.GOTO, loop_label),
            end_label,
            InsnInfo(InsnInfoType.RETURN, 3),
        )
        method = _static_method(descriptor="()V", code=code)
        result = self._compute(code, method)
        assert result.stack_map_table is not None
        assert result.stack_map_table.number_of_entries >= 1

    def test_try_catch(self) -> None:
        """Exception handler should produce a frame at the handler entry."""
        start = Label()
        end = Label()
        handler = Label()
        end2 = Label()
        code = _code(
            start,
            # Method invocation can throw — needed for exception propagation
            MethodInsn(InsnInfoType.INVOKESTATIC, "Foo", "bar", "()V"),
            BranchInsn(InsnInfoType.GOTO, end2),
            end,
            handler,
            InsnInfo(InsnInfoType.POP, 10),
            InsnInfo(InsnInfoType.RETURN, 11),
            end2,
            InsnInfo(InsnInfoType.RETURN, 12),
            handlers=[
                ExceptionHandler(
                    start=start,
                    end=end,
                    handler=handler,
                    catch_type="java/lang/Exception",
                ),
            ],
        )
        method = _static_method(descriptor="()V", code=code)
        result = self._compute(code, method)
        assert result.stack_map_table is not None
        assert result.stack_map_table.number_of_entries >= 1

    def test_try_catch_null_type(self) -> None:
        """Exception handler with catch_type=None (finally) should produce frames."""
        start = Label()
        end = Label()
        handler = Label()
        end2 = Label()
        code = _code(
            start,
            MethodInsn(InsnInfoType.INVOKESTATIC, "Foo", "bar", "()V"),
            BranchInsn(InsnInfoType.GOTO, end2),
            end,
            handler,
            InsnInfo(InsnInfoType.POP, 10),
            InsnInfo(InsnInfoType.RETURN, 11),
            end2,
            InsnInfo(InsnInfoType.RETURN, 12),
            handlers=[
                ExceptionHandler(
                    start=start,
                    end=end,
                    handler=handler,
                    catch_type=None,
                ),
            ],
        )
        method = _static_method(descriptor="()V", code=code)
        result = self._compute(code, method)
        assert result.stack_map_table is not None

    def test_empty_code(self) -> None:
        """Empty code body should return no frames."""
        code = CodeModel(max_stack=0, max_locals=0, instructions=[])
        method = _static_method(descriptor="()V", code=code)
        result = self._compute(code, method)
        assert result.stack_map_table is None

    def test_max_values_correct(self) -> None:
        """Verify that compute_frames returns correct max_stack and max_locals."""
        code = _code(
            InsnInfo(InsnInfoType.ICONST_0, 0),
            InsnInfo(InsnInfoType.ICONST_1, 1),
            InsnInfo(InsnInfoType.IADD, 2),
            InsnInfo(InsnInfoType.IRETURN, 3),
        )
        method = _static_method(descriptor="()I", code=code)
        result = self._compute(code, method)
        assert result.max_stack == 2  # Two ints on stack before IADD
        assert result.max_locals == 0

    def test_switch_targets_get_frames(self) -> None:
        """Tableswitch targets should all get frames."""
        default_label = Label()
        case0_label = Label()
        case1_label = Label()
        code = _code(
            InsnInfo(InsnInfoType.ICONST_0, 0),
            TableSwitchInsn(default_label, 0, 1, [case0_label, case1_label]),
            default_label,
            InsnInfo(InsnInfoType.RETURN, 10),
            case0_label,
            InsnInfo(InsnInfoType.RETURN, 11),
            case1_label,
            InsnInfo(InsnInfoType.RETURN, 12),
        )
        method = _static_method(descriptor="()V", code=code)
        result = self._compute(code, method)
        assert result.stack_map_table is not None
        assert result.stack_map_table.number_of_entries >= 2

    def test_instance_method_init(self) -> None:
        """An <init> method should use VUninitializedThis for slot 0."""
        label = Label()
        code = _code(
            VarInsn(InsnInfoType.ALOAD, 0),
            MethodInsn(InsnInfoType.INVOKESPECIAL, "java/lang/Object", "<init>", "()V"),
            BranchInsn(InsnInfoType.GOTO, label),
            label,
            InsnInfo(InsnInfoType.RETURN, 10),
        )
        method = _method(name="<init>", descriptor="()V", code=code)
        result = self._compute(code, method, "MyClass")
        assert result.stack_map_table is not None
        frame = result.stack_map_table.entries[0]
        assert isinstance(frame, attributes.FullFrameInfo)
        obj = frame.locals[0]
        assert isinstance(obj, attributes.ObjectVariableInfo)
        entry = self.cp.get(obj.cpool_index)
        assert isinstance(entry, ClassInfo)
        assert self.cp.resolve_utf8(entry.name_index) == "MyClass"

    def test_unlabeled_new_branch_tracks_uninitialized_offset(self) -> None:
        """Frames should preserve unlabeled NEW sites as UNINITIALIZED offsets."""
        branch_label = Label()
        code = _code(
            TypeInsn(InsnInfoType.NEW, "java/lang/Object"),
            InsnInfo(InsnInfoType.DUP, 3),
            VarInsn(InsnInfoType.ASTORE, 0),
            InsnInfo(InsnInfoType.ICONST_0, 4),
            BranchInsn(InsnInfoType.IFEQ, branch_label),
            VarInsn(InsnInfoType.ALOAD, 0),
            MethodInsn(InsnInfoType.INVOKESPECIAL, "java/lang/Object", "<init>", "()V"),
            InsnInfo(InsnInfoType.RETURN, 10),
            branch_label,
            VarInsn(InsnInfoType.ALOAD, 0),
            MethodInsn(InsnInfoType.INVOKESPECIAL, "java/lang/Object", "<init>", "()V"),
            InsnInfo(InsnInfoType.RETURN, 20),
        )
        method = _static_method(descriptor="()V", code=code)
        result = self._compute(code, method)
        assert result.stack_map_table is not None

        uninitialized_offsets = [
            local.offset
            for frame in result.stack_map_table.entries
            for local in getattr(frame, "locals", [])
            if isinstance(local, attributes.UninitializedVariableInfo)
        ]
        assert uninitialized_offsets == [0]


# ===================================================================
# FrameComputationResult tests
# ===================================================================


class TestFrameComputationResult:
    def test_importable(self) -> None:
        from pytecode.analysis import FrameComputationResult

        assert FrameComputationResult is not None

    def test_fields(self) -> None:
        from pytecode.analysis import FrameComputationResult

        result = FrameComputationResult(max_stack=5, max_locals=3, stack_map_table=None)
        assert result.max_stack == 5
        assert result.max_locals == 3
        assert result.stack_map_table is None


# ===================================================================
# lower_code integration tests
# ===================================================================


class TestLowerCodeRecomputeFrames:
    """Tests for lower_code() with recompute_frames=True."""

    def test_backwards_compatible_no_params(self) -> None:
        """lower_code() without new params should work exactly as before."""
        from pytecode.constant_pool_builder import ConstantPoolBuilder
        from pytecode.labels import lower_code

        cp = ConstantPoolBuilder()
        code = _code(InsnInfo(InsnInfoType.RETURN, 0))
        result = lower_code(code, cp)
        assert result.max_stacks == 100  # Uses CodeModel's original value
        assert result.max_locals == 100

    def test_recompute_frames_updates_maxs(self) -> None:
        """recompute_frames=True should update max_stack and max_locals."""
        from pytecode.constant_pool_builder import ConstantPoolBuilder
        from pytecode.labels import lower_code

        cp = ConstantPoolBuilder()
        code = _code(
            InsnInfo(InsnInfoType.ICONST_0, 0),
            InsnInfo(InsnInfoType.IRETURN, 1),
        )
        method = _static_method(descriptor="()I", code=code)
        result = lower_code(
            code, cp,
            method=method,
            class_name="Test",
            recompute_frames=True,
        )
        assert result.max_stacks == 1  # Recomputed, not 100
        assert result.max_locals == 0

    def test_recompute_frames_adds_stack_map_table(self) -> None:
        """recompute_frames=True with branches should add StackMapTable."""
        from pytecode.constant_pool_builder import ConstantPoolBuilder
        from pytecode.labels import lower_code

        cp = ConstantPoolBuilder()
        label = Label()
        code = _code(
            InsnInfo(InsnInfoType.ICONST_0, 0),
            BranchInsn(InsnInfoType.IFEQ, label),
            InsnInfo(InsnInfoType.ICONST_1, 1),
            InsnInfo(InsnInfoType.IRETURN, 2),
            label,
            InsnInfo(InsnInfoType.ICONST_0, 3),
            InsnInfo(InsnInfoType.IRETURN, 4),
        )
        method = _static_method(descriptor="()I", code=code)
        result = lower_code(
            code, cp,
            method=method,
            class_name="Test",
            recompute_frames=True,
        )
        smt_attrs = [a for a in result.attributes if isinstance(a, attributes.StackMapTableAttr)]
        assert len(smt_attrs) == 1
        assert smt_attrs[0].attribute_length > 0
        assert result.attributes_count == 1
        assert result.attribute_length > 12 + result.code_length + (8 * result.exception_table_length)

    def test_recompute_frames_removes_stale_stack_map_table(self) -> None:
        """recompute_frames=True should remove stale StackMapTable attrs when none are needed."""
        from pytecode.constant_pool_builder import ConstantPoolBuilder
        from pytecode.labels import lower_code

        cp = ConstantPoolBuilder()
        stale = attributes.StackMapTableAttr(
            attribute_name_index=cp.add_utf8("StackMapTable"),
            attribute_length=2,
            number_of_entries=0,
            entries=[],
        )
        code = CodeModel(
            max_stack=100,
            max_locals=100,
            instructions=[InsnInfo(InsnInfoType.RETURN, 0)],
            attributes=[stale],
        )
        method = _static_method(descriptor="()V", code=code)
        result = lower_code(
            code, cp,
            method=method,
            class_name="Test",
            recompute_frames=True,
        )
        assert not any(isinstance(attr, attributes.StackMapTableAttr) for attr in result.attributes)
        assert result.attributes_count == 0
        assert result.attribute_length == 13

    def test_recompute_frames_requires_method_and_class_name(self) -> None:
        """recompute_frames=True without method/class_name should raise."""
        from pytecode.constant_pool_builder import ConstantPoolBuilder
        from pytecode.labels import lower_code

        cp = ConstantPoolBuilder()
        code = _code(InsnInfo(InsnInfoType.RETURN, 0))
        with pytest.raises(ValueError, match="method and class_name are required"):
            lower_code(code, cp, recompute_frames=True)


# ===================================================================
# to_classfile integration tests
# ===================================================================


class TestToClassfileRecomputeFrames:
    """Tests for ClassModel.to_classfile() with recompute_frames=True."""

    def test_default_behavior_unchanged(self, tmp_path: Path) -> None:
        """to_classfile() without recompute_frames should preserve originals."""
        class_path = compile_java_resource(tmp_path, "HelloWorld.java")
        cf = ClassModel.from_bytes(class_path.read_bytes())
        restored = cf.to_classfile()
        assert restored.magic == 0xCAFEBABE

    def test_recompute_frames_produces_valid_output(self, tmp_path: Path) -> None:
        """to_classfile(recompute_frames=True) should work end-to-end."""
        class_path = compile_java_resource(tmp_path, "HelloWorld.java")
        model = ClassModel.from_bytes(class_path.read_bytes())
        restored = model.to_classfile(recompute_frames=True)
        assert restored.magic == 0xCAFEBABE
        model2 = ClassModel.from_classfile(restored)
        assert model2.name == model.name


# ===================================================================
# Compiled fixture tests for frame computation
# ===================================================================


class TestComputeFramesWithFixtures:
    """Test frame computation against compiled Java class files."""

    def test_hello_world_frames(self, tmp_path: Path) -> None:
        """compute_frames on HelloWorld methods should succeed."""
        from pytecode.analysis import compute_frames
        from pytecode.constant_pool_builder import ConstantPoolBuilder
        from pytecode.labels import resolve_labels

        class_path = compile_java_resource(tmp_path, "HelloWorld.java")
        model = ClassModel.from_bytes(class_path.read_bytes())
        for mm in model.methods:
            if mm.code is None:
                continue
            cp = ConstantPoolBuilder()
            items = list(mm.code.instructions)
            resolution = resolve_labels(items, cp)
            result = compute_frames(
                mm.code, mm, model.name, cp, resolution.label_offsets,
            )
            assert result.max_stack >= 0
            assert result.max_locals >= 0

    def test_cfg_fixture_frames(self, tmp_path: Path) -> None:
        """compute_frames on CfgFixture methods should succeed."""
        from pytecode.analysis import compute_frames
        from pytecode.constant_pool_builder import ConstantPoolBuilder
        from pytecode.labels import resolve_labels

        class_path = compile_java_resource(tmp_path, "CfgFixture.java")
        model = ClassModel.from_bytes(class_path.read_bytes())
        for mm in model.methods:
            if mm.code is None:
                continue
            cp = ConstantPoolBuilder()
            items = list(mm.code.instructions)
            resolution = resolve_labels(items, cp)
            result = compute_frames(
                mm.code, mm, model.name, cp, resolution.label_offsets,
            )
            assert result.max_stack >= 0
            assert result.max_locals >= 0
