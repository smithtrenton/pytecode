"""Tests for ``pytecode.analysis`` — CFG construction and stack/local simulation."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

import pytest

from pytecode.analysis import (
    AnalysisError,
    ControlFlowGraph,
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
