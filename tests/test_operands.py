"""Tests for pytecode.edit.operands — symbolic instruction wrapper types."""

from __future__ import annotations

from copy import copy, deepcopy
from dataclasses import FrozenInstanceError
from pathlib import Path
from pickle import dumps, loads
from typing import Any

import pytest

from pytecode.classfile.instructions import (
    ConstPoolIndex,
    IInc,
    IIncW,
    InsnInfo,
    InsnInfoType,
    InvokeInterface,
    LocalIndex,
    LocalIndexW,
    MultiANewArray,
)
from pytecode.edit.constant_pool_builder import ConstantPoolBuilder
from pytecode.edit.labels import (
    CodeItem,
    lower_code,
)
from pytecode.edit.model import ClassModel, CodeModel
from pytecode.edit.operands import (
    _BASE_TO_WIDE,
    _IMPLICIT_VAR_SLOTS,
    _VAR_SHORTCUTS,
    _WIDE_TO_BASE,
    FieldInsn,
    IIncInsn,
    InterfaceMethodInsn,
    InvokeDynamicInsn,
    LdcClass,
    LdcDouble,
    LdcDynamic,
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
from tests.helpers import compile_java_resource, find_method_in_model

# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------


@pytest.fixture
def showcase_class(tmp_path: Path) -> Path:
    return compile_java_resource(tmp_path, "InstructionShowcase.java")


@pytest.fixture
def showcase_model(showcase_class: Path) -> ClassModel:
    return ClassModel.from_bytes(showcase_class.read_bytes())


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
def _code_items(model: ClassModel, method_name: str) -> list[CodeItem]:
    """Return the code item list for the named method."""
    method = find_method_in_model(model, method_name)
    assert method.code is not None
    return method.code.instructions


def _insns(items: list[CodeItem]) -> list[InsnInfo]:
    """Filter Labels out, returning only InsnInfo items."""
    return [item for item in items if isinstance(item, InsnInfo)]


def _lower_simple(instructions: list[CodeItem]) -> list[InsnInfo]:
    """Lower a bare instruction list to raw InsnInfo objects."""
    cp = ConstantPoolBuilder()
    code = CodeModel(
        max_stack=10,
        max_locals=10,
        instructions=instructions,
    )
    attr = lower_code(code, cp)
    return attr.code


# ---------------------------------------------------------------------------
# Dataclass-like semantics
# ---------------------------------------------------------------------------


class TestDataclassLikeSemantics:
    def test_frozen_ldc_values_are_hashable_and_immutable(self) -> None:
        value = LdcMethodHandle(6, "Owner", "name", "()V", is_interface=True)

        assert repr(value) == (
            "LdcMethodHandle("
            "reference_kind=6, owner='Owner', name='name', descriptor='()V', is_interface=True)"
        )
        assert value == LdcMethodHandle(6, "Owner", "name", "()V", is_interface=True)
        assert hash(value) == hash(LdcMethodHandle(6, "Owner", "name", "()V", is_interface=True))

        with pytest.raises(FrozenInstanceError, match="owner"):
            value.owner = "Other"

    def test_frozen_ldc_values_copy_deepcopy_and_pickle_roundtrip(self) -> None:
        value = LdcDynamic(2, "name", "I")

        copied = copy(value)
        deepcopied = deepcopy(value)
        restored = loads(dumps(value))

        assert copied == value
        assert copied is not value
        assert deepcopied == value
        assert deepcopied is not value
        assert restored == value
        assert restored is not value

    def test_wrapper_repr_and_equality_include_base_insn_fields(self) -> None:
        field_get = FieldInsn(InsnInfoType.GETFIELD, "Owner", "field", "I")
        field_put = FieldInsn(InsnInfoType.PUTFIELD, "Owner", "field", "I")
        field_offset = FieldInsn(InsnInfoType.GETFIELD, "Owner", "field", "I", 5)

        assert repr(field_get) == (
            "FieldInsn(type=<InsnInfoType.GETFIELD: 180>, bytecode_offset=-1, "
            "owner='Owner', name='field', descriptor='I')"
        )
        assert field_get != field_put
        assert field_get != field_offset
        assert field_get == FieldInsn(InsnInfoType.GETFIELD, "Owner", "field", "I")

        with pytest.raises(TypeError, match="FieldInsn"):
            hash(field_get)

    def test_wrapper_copy_deepcopy_and_pickle_preserve_base_insn_fields(self) -> None:
        field = FieldInsn(InsnInfoType.PUTSTATIC, "Owner", "field", "I", 7)
        ldc = LdcInsn(LdcInt(42), 9)

        field_copies = (copy(field), deepcopy(field), loads(dumps(field)))
        ldc_copies = (copy(ldc), deepcopy(ldc), loads(dumps(ldc)))

        for clone in field_copies:
            assert clone == field
            assert clone is not field
            assert clone.type == InsnInfoType.PUTSTATIC
            assert clone.bytecode_offset == 7

        assert repr(ldc) == "LdcInsn(type=<InsnInfoType.LDC_W: 19>, bytecode_offset=9, value=LdcInt(value=42))"
        assert ldc != LdcInsn(LdcInt(42), 1)
        assert ldc == LdcInsn(LdcInt(42), 9)
        for clone in ldc_copies:
            assert clone == ldc
            assert clone is not ldc
            assert clone.type == InsnInfoType.LDC_W
            assert clone.bytecode_offset == 9
            assert clone.value == LdcInt(42)


# ---------------------------------------------------------------------------
# 1. Constructor validation
# ---------------------------------------------------------------------------


class TestConstructorValidation:
    def test_field_insn_rejects_bad_opcode(self) -> None:
        with pytest.raises(ValueError, match="INVOKEVIRTUAL"):
            FieldInsn(InsnInfoType.INVOKEVIRTUAL, "Owner", "field", "I")

    def test_method_insn_rejects_invokeinterface(self) -> None:
        with pytest.raises(ValueError, match="INVOKEINTERFACE"):
            MethodInsn(InsnInfoType.INVOKEINTERFACE, "Owner", "m", "()V")

    def test_method_insn_rejects_field_opcode(self) -> None:
        with pytest.raises(ValueError, match="GETFIELD"):
            MethodInsn(InsnInfoType.GETFIELD, "Owner", "m", "()V")

    def test_type_insn_rejects_field_opcode(self) -> None:
        with pytest.raises(ValueError, match="PUTFIELD"):
            TypeInsn(InsnInfoType.PUTFIELD, "java/lang/String")

    def test_var_insn_rejects_field_opcode(self) -> None:
        with pytest.raises(ValueError, match="GETFIELD"):
            VarInsn(InsnInfoType.GETFIELD, 0)

    def test_var_insn_rejects_slot_over_u2(self) -> None:
        with pytest.raises(ValueError, match="65535"):
            VarInsn(InsnInfoType.ILOAD, 65536)

    def test_multi_anewarray_rejects_zero_dimensions(self) -> None:
        with pytest.raises(ValueError):
            MultiANewArrayInsn("[[I", 0)

    def test_multi_anewarray_rejects_negative_dimensions(self) -> None:
        with pytest.raises(ValueError):
            MultiANewArrayInsn("[[I", -1)

    def test_multi_anewarray_rejects_dimensions_over_u1(self) -> None:
        with pytest.raises(ValueError, match="255"):
            MultiANewArrayInsn("[[I", 256)

    def test_field_insn_accepts_all_field_opcodes(self) -> None:
        for opcode in (
            InsnInfoType.GETFIELD,
            InsnInfoType.PUTFIELD,
            InsnInfoType.GETSTATIC,
            InsnInfoType.PUTSTATIC,
        ):
            insn = FieldInsn(opcode, "Owner", "f", "I")
            assert insn.type == opcode

    def test_type_insn_accepts_all_type_opcodes(self) -> None:
        for opcode in (
            InsnInfoType.NEW,
            InsnInfoType.CHECKCAST,
            InsnInfoType.INSTANCEOF,
            InsnInfoType.ANEWARRAY,
        ):
            insn = TypeInsn(opcode, "java/lang/Object")
            assert insn.type == opcode

    def test_interface_method_insn_fixed_opcode(self) -> None:
        insn = InterfaceMethodInsn("Owner", "m", "()V")
        assert insn.type == InsnInfoType.INVOKEINTERFACE

    def test_invoke_dynamic_insn_fixed_opcode(self) -> None:
        insn = InvokeDynamicInsn(0, "run", "()Ljava/lang/Runnable;")
        assert insn.type == InsnInfoType.INVOKEDYNAMIC

    def test_invoke_dynamic_rejects_negative_bootstrap_index(self) -> None:
        with pytest.raises(ValueError, match="bootstrap_method_attr_index"):
            InvokeDynamicInsn(-1, "run", "()Ljava/lang/Runnable;")

    def test_invoke_dynamic_rejects_bootstrap_index_over_u2(self) -> None:
        with pytest.raises(ValueError, match="65535"):
            InvokeDynamicInsn(65536, "run", "()Ljava/lang/Runnable;")

    def test_iinc_insn_fixed_opcode(self) -> None:
        insn = IIncInsn(0, 1)
        assert insn.type == InsnInfoType.IINC

    def test_iinc_insn_rejects_slot_over_u2(self) -> None:
        with pytest.raises(ValueError, match="65535"):
            IIncInsn(65536, 1)

    def test_iinc_insn_rejects_increment_over_i2(self) -> None:
        with pytest.raises(ValueError, match="32767"):
            IIncInsn(0, 32768)

    def test_iinc_insn_rejects_increment_under_i2(self) -> None:
        with pytest.raises(ValueError, match="-32768"):
            IIncInsn(0, -32769)

    def test_ldc_insn_fixed_opcode(self) -> None:
        insn = LdcInsn(LdcInt(42))
        assert insn.type == InsnInfoType.LDC_W

    def test_ldc_method_handle_rejects_bad_reference_kind(self) -> None:
        with pytest.raises(ValueError, match="reference_kind"):
            LdcMethodHandle(0, "Owner", "f", "I")

    def test_ldc_dynamic_rejects_negative_bootstrap_index(self) -> None:
        with pytest.raises(ValueError, match="bootstrap_method_attr_index"):
            LdcDynamic(-1, "name", "I")

    def test_ldc_dynamic_rejects_bootstrap_index_over_u2(self) -> None:
        with pytest.raises(ValueError, match="65535"):
            LdcDynamic(65536, "name", "I")


# ---------------------------------------------------------------------------
# 2. Mapping table sanity checks
# ---------------------------------------------------------------------------


class TestMappingTables:
    def test_implicit_var_slots_count(self) -> None:
        # 8 base opcodes × 4 slots (0–3) = 32 entries
        # Plus RET has no implicit forms → actually 5 types × 4 = 20 + 3 types × 4 = 32
        # (ILOAD, LLOAD, FLOAD, DLOAD, ALOAD, ISTORE, LSTORE, FSTORE, DSTORE, ASTORE) × 4 = 40
        assert len(_IMPLICIT_VAR_SLOTS) == 40

    def test_implicit_var_slots_iload_0(self) -> None:
        base, slot = _IMPLICIT_VAR_SLOTS[InsnInfoType.ILOAD_0]
        assert base == InsnInfoType.ILOAD
        assert slot == 0

    def test_implicit_var_slots_astore_3(self) -> None:
        base, slot = _IMPLICIT_VAR_SLOTS[InsnInfoType.ASTORE_3]
        assert base == InsnInfoType.ASTORE
        assert slot == 3

    def test_var_shortcuts_roundtrip(self) -> None:
        for (base, slot), shortcut in _VAR_SHORTCUTS.items():
            restored_base, restored_slot = _IMPLICIT_VAR_SLOTS[shortcut]
            assert restored_base == base
            assert restored_slot == slot

    def test_wide_to_base_roundtrip(self) -> None:
        for wide, base in _WIDE_TO_BASE.items():
            assert _BASE_TO_WIDE[base] == wide


# ---------------------------------------------------------------------------
# 3. Lifting: FieldInsn
# ---------------------------------------------------------------------------


class TestFieldInsnLifting:
    def test_read_field_is_getfield(self, showcase_model: ClassModel) -> None:
        items = _insns(_code_items(showcase_model, "readField"))
        field_insns = [i for i in items if isinstance(i, FieldInsn)]
        assert len(field_insns) >= 1
        gf = next(i for i in field_insns if i.type == InsnInfoType.GETFIELD)
        assert gf.name == "intField"
        assert gf.descriptor == "I"

    def test_write_field_is_putfield(self, showcase_model: ClassModel) -> None:
        items = _insns(_code_items(showcase_model, "writeField"))
        pf = next(i for i in items if isinstance(i, FieldInsn) and i.type == InsnInfoType.PUTFIELD)
        assert pf.name == "intField"
        assert pf.descriptor == "I"

    def test_read_static_is_getstatic(self, showcase_model: ClassModel) -> None:
        items = _insns(_code_items(showcase_model, "readStatic"))
        gs = next(i for i in items if isinstance(i, FieldInsn) and i.type == InsnInfoType.GETSTATIC)
        assert gs.name == "counter"
        assert gs.descriptor == "J"

    def test_write_static_is_putstatic(self, showcase_model: ClassModel) -> None:
        items = _insns(_code_items(showcase_model, "writeStatic"))
        ps = next(i for i in items if isinstance(i, FieldInsn) and i.type == InsnInfoType.PUTSTATIC)
        assert ps.name == "counter"
        assert ps.descriptor == "J"

    def test_field_insn_has_correct_owner(self, showcase_model: ClassModel) -> None:
        items = _insns(_code_items(showcase_model, "readField"))
        gf = next(i for i in items if isinstance(i, FieldInsn))
        assert gf.owner == "InstructionShowcase"


# ---------------------------------------------------------------------------
# 4. Lifting: MethodInsn
# ---------------------------------------------------------------------------


class TestMethodInsnLifting:
    def test_build_string_has_invokespecial_and_invokevirtual(self, showcase_model: ClassModel) -> None:
        items = _insns(_code_items(showcase_model, "buildString"))
        method_insns = [i for i in items if isinstance(i, MethodInsn)]
        opcodes = {i.type for i in method_insns}
        assert InsnInfoType.INVOKESPECIAL in opcodes
        assert InsnInfoType.INVOKEVIRTUAL in opcodes

    def test_static_helper_has_invokestatic(self, showcase_model: ClassModel) -> None:
        items = _insns(_code_items(showcase_model, "staticHelper"))
        statics = [i for i in items if isinstance(i, MethodInsn) and i.type == InsnInfoType.INVOKESTATIC]
        assert len(statics) >= 1

    def test_method_insn_has_correct_fields(self, showcase_model: ClassModel) -> None:
        items = _insns(_code_items(showcase_model, "buildString"))
        init = next(
            i
            for i in items
            if isinstance(i, MethodInsn) and i.type == InsnInfoType.INVOKESPECIAL and i.name == "<init>"
        )
        assert "StringBuilder" in init.owner
        assert init.descriptor == "()V"


# ---------------------------------------------------------------------------
# 5. Lifting: InterfaceMethodInsn
# ---------------------------------------------------------------------------


class TestInterfaceMethodInsnLifting:
    def test_compare_via_interface_has_invokeinterface(self, showcase_model: ClassModel) -> None:
        items = _insns(_code_items(showcase_model, "compareViaInterface"))
        iface_insns = [i for i in items if isinstance(i, InterfaceMethodInsn)]
        assert len(iface_insns) >= 1

    def test_invokeinterface_owner_and_descriptor(self, showcase_model: ClassModel) -> None:
        items = _insns(_code_items(showcase_model, "compareViaInterface"))
        size_call = next(i for i in items if isinstance(i, InterfaceMethodInsn) and i.name == "size")
        assert "List" in size_call.owner
        assert size_call.descriptor == "()I"


# ---------------------------------------------------------------------------
# 6. Lifting: TypeInsn
# ---------------------------------------------------------------------------


class TestTypeInsnLifting:
    def test_type_ops_has_new(self, showcase_model: ClassModel) -> None:
        items = _insns(_code_items(showcase_model, "typeOps"))
        new_insns = [i for i in items if isinstance(i, TypeInsn) and i.type == InsnInfoType.NEW]
        assert len(new_insns) >= 1
        assert any("ArrayList" in i.class_name for i in new_insns)

    def test_type_ops_has_checkcast(self, showcase_model: ClassModel) -> None:
        items = _insns(_code_items(showcase_model, "typeOps"))
        casts = [i for i in items if isinstance(i, TypeInsn) and i.type == InsnInfoType.CHECKCAST]
        assert len(casts) >= 1

    def test_type_ops_has_instanceof(self, showcase_model: ClassModel) -> None:
        items = _insns(_code_items(showcase_model, "typeOps"))
        checks = [i for i in items if isinstance(i, TypeInsn) and i.type == InsnInfoType.INSTANCEOF]
        assert len(checks) >= 1
        assert any("String" in i.class_name for i in checks)

    def test_type_ops_has_anewarray(self, showcase_model: ClassModel) -> None:
        items = _insns(_code_items(showcase_model, "typeOps"))
        arrays = [i for i in items if isinstance(i, TypeInsn) and i.type == InsnInfoType.ANEWARRAY]
        assert len(arrays) >= 1
        assert any("String" in i.class_name for i in arrays)


# ---------------------------------------------------------------------------
# 7. Lifting: LdcInsn
# ---------------------------------------------------------------------------


class TestLdcInsnLifting:
    def _load_values(self, showcase_model: ClassModel) -> list[LdcInsn]:
        items = _insns(_code_items(showcase_model, "loadConstants"))
        return [i for i in items if isinstance(i, LdcInsn)]

    def test_has_ldc_int(self, showcase_model: ClassModel) -> None:
        ldc_insns = self._load_values(showcase_model)
        # 100_000 exceeds SIPUSH range so javac emits LDC/LDC_W for it
        assert any(isinstance(v.value, LdcInt) and v.value.value == 100_000 for v in ldc_insns)

    def test_has_ldc_float(self, showcase_model: ClassModel) -> None:
        ldc_insns = self._load_values(showcase_model)
        assert any(isinstance(i.value, LdcFloat) for i in ldc_insns)

    def test_has_ldc_long(self, showcase_model: ClassModel) -> None:
        ldc_insns = self._load_values(showcase_model)
        assert any(isinstance(v.value, LdcLong) and v.value.value == 1234567890123 for v in ldc_insns)

    def test_has_ldc_double(self, showcase_model: ClassModel) -> None:
        ldc_insns = self._load_values(showcase_model)
        assert any(isinstance(i.value, LdcDouble) for i in ldc_insns)

    def test_has_ldc_string(self, showcase_model: ClassModel) -> None:
        ldc_insns = self._load_values(showcase_model)
        assert any(isinstance(v.value, LdcString) and v.value.value == "hello pytecode" for v in ldc_insns)

    def test_has_ldc_class(self, showcase_model: ClassModel) -> None:
        ldc_insns = self._load_values(showcase_model)
        assert any(isinstance(v.value, LdcClass) and "String" in v.value.name for v in ldc_insns)


# ---------------------------------------------------------------------------
# 8. Lifting: MultiANewArrayInsn
# ---------------------------------------------------------------------------


class TestMultiANewArrayInsnLifting:
    def test_multi_array_is_multianewarray(self, showcase_model: ClassModel) -> None:
        items = _insns(_code_items(showcase_model, "multiArray"))
        multi = [i for i in items if isinstance(i, MultiANewArrayInsn)]
        assert len(multi) == 1
        assert multi[0].class_name == "[[I"
        assert multi[0].dimensions == 2


# ---------------------------------------------------------------------------
# 9. Lifting: VarInsn normalisation
# ---------------------------------------------------------------------------


class TestVarInsnLifting:
    def test_implicit_slots_normalized(self, showcase_model: ClassModel) -> None:
        """All implicit ILOAD_0–ASTORE_3 opcodes must become VarInsn."""
        items = _insns(_code_items(showcase_model, "readField"))
        # readField() loads `this` (slot 0 → ALOAD_0 implicit)
        var_insns = [i for i in items if isinstance(i, VarInsn)]
        assert any(i.type == InsnInfoType.ALOAD and i.slot == 0 for i in var_insns)

    def test_high_slot_iload(self, showcase_model: ClassModel) -> None:
        """manyLocals uses slots >= 7; these use explicit LocalIndex forms in raw bytecode."""
        items = _insns(_code_items(showcase_model, "manyLocals"))
        var_insns = [i for i in items if isinstance(i, VarInsn)]
        # Slots 7+ must appear (explicit encoding beyond implicit range)
        high_slots = [i for i in var_insns if i.slot > 3]
        assert len(high_slots) >= 2

    def test_no_implicit_opcode_survives_lifting(self, showcase_model: ClassModel) -> None:
        """After lifting, no raw ILOAD_0-through-ASTORE_3 opcodes should remain."""
        for method in showcase_model.methods:
            if method.code is None:
                continue
            for item in method.code.instructions:
                if isinstance(item, InsnInfo):
                    assert item.type not in _IMPLICIT_VAR_SLOTS, (
                        f"Implicit opcode {item.type.name} survived lifting in {method.name}"
                    )


# ---------------------------------------------------------------------------
# 10. Lifting: IIncInsn
# ---------------------------------------------------------------------------


class TestIIncInsnLifting:
    def test_iinc_demo_has_iinc_insn(self, showcase_model: ClassModel) -> None:
        items = _insns(_code_items(showcase_model, "iincDemo"))
        iinc_insns = [i for i in items if isinstance(i, IIncInsn)]
        assert len(iinc_insns) >= 3  # start++, start--, start += 100

    def test_iinc_values(self, showcase_model: ClassModel) -> None:
        items = _insns(_code_items(showcase_model, "iincDemo"))
        iinc_insns = [i for i in items if isinstance(i, IIncInsn)]
        increments = {i.increment for i in iinc_insns}
        assert 1 in increments
        assert -1 in increments
        assert 100 in increments


# ---------------------------------------------------------------------------
# 11. Lifting: InvokeDynamicInsn
# ---------------------------------------------------------------------------


class TestInvokeDynamicInsnLifting:
    def test_make_lambda_has_invokedynamic(self, showcase_model: ClassModel) -> None:
        items = _insns(_code_items(showcase_model, "makeLambda"))
        dyn_insns = [i for i in items if isinstance(i, InvokeDynamicInsn)]
        assert len(dyn_insns) >= 1

    def test_invokedynamic_has_descriptor(self, showcase_model: ClassModel) -> None:
        items = _insns(_code_items(showcase_model, "makeLambda"))
        dyn = next(i for i in items if isinstance(i, InvokeDynamicInsn))
        assert dyn.descriptor  # non-empty descriptor
        assert "Supplier" in dyn.descriptor or "get" in dyn.name.lower()


# ---------------------------------------------------------------------------
# 12. Lowering: VarInsn encoding selection
# ---------------------------------------------------------------------------


class TestVarInsnLowering:
    def _lower_var(self, opcode: InsnInfoType, slot: int) -> InsnInfo:
        items: list[CodeItem] = [VarInsn(opcode, slot)]
        lowered = _lower_simple(items)
        assert len(lowered) == 1
        return lowered[0]

    def test_slot_0_emits_implicit(self) -> None:
        raw = self._lower_var(InsnInfoType.ILOAD, 0)
        assert raw.type == InsnInfoType.ILOAD_0
        assert isinstance(raw, InsnInfo) and not isinstance(raw, LocalIndex)

    def test_slot_3_emits_implicit(self) -> None:
        raw = self._lower_var(InsnInfoType.ASTORE, 3)
        assert raw.type == InsnInfoType.ASTORE_3

    def test_slot_4_emits_local_index(self) -> None:
        raw = self._lower_var(InsnInfoType.ILOAD, 4)
        assert raw.type == InsnInfoType.ILOAD
        assert isinstance(raw, LocalIndex)
        assert raw.index == 4

    def test_slot_255_emits_local_index(self) -> None:
        raw = self._lower_var(InsnInfoType.ILOAD, 255)
        assert isinstance(raw, LocalIndex)
        assert raw.index == 255

    def test_slot_256_emits_wide(self) -> None:
        raw = self._lower_var(InsnInfoType.ILOAD, 256)
        assert raw.type == InsnInfoType.ILOADW
        assert isinstance(raw, LocalIndexW)
        assert raw.index == 256

    def test_ret_no_implicit_form(self) -> None:
        # RET has no _0 variant, always uses LocalIndex
        raw = self._lower_var(InsnInfoType.RET, 0)
        assert raw.type == InsnInfoType.RET
        assert isinstance(raw, LocalIndex)

    def test_aload_0_roundtrips(self) -> None:
        """ALOAD slot=0 → lower → ALOAD_0 → lift → VarInsn(ALOAD, 0)."""
        items: list[CodeItem] = [VarInsn(InsnInfoType.ALOAD, 0)]
        lowered = _lower_simple(items)
        assert lowered[0].type == InsnInfoType.ALOAD_0

    def test_mutated_slot_over_u2_is_rejected_on_lower(self) -> None:
        insn = VarInsn(InsnInfoType.ILOAD, 0)
        insn.slot = 65536
        with pytest.raises(ValueError, match="65535"):
            _lower_simple([insn])


# ---------------------------------------------------------------------------
# 13. Lowering: IIncInsn narrow/wide selection
# ---------------------------------------------------------------------------


class TestIIncInsnLowering:
    def _lower_iinc(self, slot: int, increment: int) -> InsnInfo:
        items: list[CodeItem] = [IIncInsn(slot, increment)]
        lowered = _lower_simple(items)
        assert len(lowered) == 1
        return lowered[0]

    def test_narrow_emits_iinc(self) -> None:
        raw = self._lower_iinc(0, 1)
        assert isinstance(raw, IInc)
        assert raw.index == 0
        assert raw.value == 1

    def test_slot_255_increment_127_emits_iinc(self) -> None:
        raw = self._lower_iinc(255, 127)
        assert isinstance(raw, IInc)

    def test_slot_255_increment_m128_emits_iinc(self) -> None:
        raw = self._lower_iinc(255, -128)
        assert isinstance(raw, IInc)

    def test_slot_256_emits_iincw(self) -> None:
        raw = self._lower_iinc(256, 1)
        assert isinstance(raw, IIncW)

    def test_increment_128_emits_iincw(self) -> None:
        raw = self._lower_iinc(0, 128)
        assert isinstance(raw, IIncW)

    def test_increment_m129_emits_iincw(self) -> None:
        raw = self._lower_iinc(0, -129)
        assert isinstance(raw, IIncW)

    def test_wide_preserves_slot_and_increment(self) -> None:
        raw = self._lower_iinc(300, 200)
        assert isinstance(raw, IIncW)
        assert raw.index == 300
        assert raw.value == 200

    def test_mutated_increment_over_i2_is_rejected_on_lower(self) -> None:
        insn = IIncInsn(0, 1)
        insn.increment = 32768
        with pytest.raises(ValueError, match="32767"):
            _lower_simple([insn])


# ---------------------------------------------------------------------------
# 14. Lowering: LdcInsn encoding selection
# ---------------------------------------------------------------------------


class TestLdcInsnLowering:
    def _lower_ldc(self, value: Any, pre_fill: int = 0) -> tuple[InsnInfo, int]:
        """Return (raw_insn, cp_index).  ``pre_fill`` adds that many unique
        integer entries to the CP first, forcing later entries to higher indices.
        """
        cp = ConstantPoolBuilder()
        for i in range(pre_fill):
            cp.add_integer(i)
        items: list[CodeItem] = [LdcInsn(value)]
        from pytecode.edit.model import CodeModel as _CodeModel

        code = _CodeModel(max_stack=2, max_locals=1, instructions=items)
        attr = lower_code(code, cp)
        assert len(attr.code) == 1
        raw = attr.code[0]
        assert isinstance(raw, (LocalIndex, ConstPoolIndex))
        return raw, raw.index

    def test_ldc_int_emits_ldc_for_small_index(self) -> None:
        raw, _ = self._lower_ldc(LdcInt(42))
        assert raw.type == InsnInfoType.LDC

    def test_ldc_float_emits_ldc_for_small_index(self) -> None:
        raw, _ = self._lower_ldc(LdcFloat(0x4048F5C3))  # ~3.14
        assert raw.type == InsnInfoType.LDC

    def test_ldc_string_emits_ldc_for_small_index(self) -> None:
        raw, _ = self._lower_ldc(LdcString("hello"))
        assert raw.type == InsnInfoType.LDC

    def test_ldc_class_emits_ldc_for_small_index(self) -> None:
        raw, _ = self._lower_ldc(LdcClass("java/lang/String"))
        assert raw.type == InsnInfoType.LDC

    def test_ldc_long_emits_ldc2_w(self) -> None:
        raw, _ = self._lower_ldc(LdcLong(1234567890123))
        assert raw.type == InsnInfoType.LDC2_W

    def test_ldc_double_emits_ldc2_w(self) -> None:
        # π as double (IEEE 754 bits: 0x400921FB54442D18)
        pi_bits = 0x400921FB54442D18
        raw, _ = self._lower_ldc(LdcDouble(pi_bits >> 32, pi_bits & 0xFFFFFFFF))
        assert raw.type == InsnInfoType.LDC2_W

    def test_ldc_method_type_emits_ldc_for_small_index(self) -> None:
        raw, _ = self._lower_ldc(LdcMethodType("()V"))
        assert raw.type == InsnInfoType.LDC

    def test_ldc_method_handle_emits_ldc_for_small_index(self) -> None:
        raw, _ = self._lower_ldc(LdcMethodHandle(1, "Owner", "f", "I"))
        assert raw.type == InsnInfoType.LDC

    def test_ldc_dynamic_emits_ldc_for_small_index(self) -> None:
        raw, _ = self._lower_ldc(LdcDynamic(0, "CONST", "I"))
        assert raw.type == InsnInfoType.LDC

    def test_ldc_emits_ldc_w_for_large_index(self) -> None:
        # Fill 255 slots (indices 1..255) so the next new entry lands at 256.
        raw, idx = self._lower_ldc(LdcInt(1_000_000), pre_fill=255)
        assert idx > 255
        assert raw.type == InsnInfoType.LDC_W


# ---------------------------------------------------------------------------
# 15. Lowering: FieldInsn / MethodInsn / InterfaceMethodInsn
# ---------------------------------------------------------------------------


class TestMemberInsnLowering:
    def test_field_insn_emits_const_pool_index(self) -> None:
        items: list[CodeItem] = [FieldInsn(InsnInfoType.GETFIELD, "Owner", "f", "I")]
        lowered = _lower_simple(items)
        assert len(lowered) == 1
        raw = lowered[0]
        assert isinstance(raw, ConstPoolIndex)
        assert raw.type == InsnInfoType.GETFIELD

    def test_method_insn_interface_uses_interface_methodref(self) -> None:
        """MethodInsn(is_interface=True) must add an InterfaceMethodref CP entry."""
        cp = ConstantPoolBuilder()
        items: list[CodeItem] = [
            MethodInsn(
                InsnInfoType.INVOKESTATIC,
                "java/util/Comparator",
                "naturalOrder",
                "()Ljava/util/Comparator;",
                is_interface=True,
            )
        ]
        from pytecode.edit.model import CodeModel as _CodeModel

        code = _CodeModel(max_stack=2, max_locals=1, instructions=items)
        attr = lower_code(code, cp)
        raw = attr.code[0]
        assert isinstance(raw, ConstPoolIndex)
        # The CP entry should be an InterfaceMethodref
        import pytecode.classfile.constant_pool as _cp

        entry = cp.get(raw.index)
        assert isinstance(entry, _cp.InterfaceMethodrefInfo)

    def test_interface_method_insn_emits_invokeinterface(self) -> None:
        items: list[CodeItem] = [InterfaceMethodInsn("java/util/List", "size", "()I")]
        lowered = _lower_simple(items)
        assert len(lowered) == 1
        raw = lowered[0]
        assert isinstance(raw, InvokeInterface)
        assert raw.count == 1  # no params + 1 for object ref

    def test_invokeinterface_count_includes_params(self) -> None:
        # void add(Object) → 1 param (object = 1 slot) + 1 for receiver = 2
        items: list[CodeItem] = [InterfaceMethodInsn("java/util/List", "add", "(Ljava/lang/Object;)Z")]
        lowered = _lower_simple(items)
        raw = lowered[0]
        assert isinstance(raw, InvokeInterface)
        assert raw.count == 2

    def test_multianewarray_insn_emits_multianewarray(self) -> None:
        items: list[CodeItem] = [MultiANewArrayInsn("[[I", 2)]
        lowered = _lower_simple(items)
        assert len(lowered) == 1
        raw = lowered[0]
        assert isinstance(raw, MultiANewArray)
        assert raw.dimensions == 2

    def test_mutated_invokedynamic_bootstrap_index_is_rejected_on_lower(self) -> None:
        insn = InvokeDynamicInsn(0, "run", "()V")
        insn.bootstrap_method_attr_index = -1
        with pytest.raises(ValueError, match="bootstrap_method_attr_index"):
            _lower_simple([insn])

    def test_mutated_multianewarray_dimensions_is_rejected_on_lower(self) -> None:
        insn = MultiANewArrayInsn("[[I", 2)
        insn.dimensions = 256
        with pytest.raises(ValueError, match="255"):
            _lower_simple([insn])


# ---------------------------------------------------------------------------
# 16. Edit tests: create symbolic instructions from scratch, lower, verify
# ---------------------------------------------------------------------------


class TestEditFromScratch:
    def test_create_field_insn_and_lower(self) -> None:
        """Build a minimal method that does GETSTATIC and verify the CP entry."""
        cp = ConstantPoolBuilder()
        items: list[CodeItem] = [
            FieldInsn(InsnInfoType.GETSTATIC, "java/lang/System", "out", "Ljava/io/PrintStream;"),
            InsnInfo(InsnInfoType.RETURN, -1),
        ]
        from pytecode.edit.model import CodeModel as _CodeModel

        code = _CodeModel(max_stack=1, max_locals=1, instructions=items)
        attr = lower_code(code, cp)

        getstatic_raw = attr.code[0]
        assert isinstance(getstatic_raw, ConstPoolIndex)
        assert getstatic_raw.type == InsnInfoType.GETSTATIC

        import pytecode.classfile.constant_pool as _cp

        cp_entry = cp.get(getstatic_raw.index)
        assert isinstance(cp_entry, _cp.FieldrefInfo)

    def test_mutate_var_insn_slot_changes_encoding(self) -> None:
        """Change VarInsn slot from 0 to 256 and verify encoding switches."""
        items: list[CodeItem] = [VarInsn(InsnInfoType.ILOAD, 0)]
        lowered = _lower_simple(items)
        assert lowered[0].type == InsnInfoType.ILOAD_0

        items2: list[CodeItem] = [VarInsn(InsnInfoType.ILOAD, 256)]
        lowered2 = _lower_simple(items2)
        assert isinstance(lowered2[0], LocalIndexW)
        assert lowered2[0].type == InsnInfoType.ILOADW

    def test_create_ldc_insn_deduplication(self) -> None:
        """Two LdcInsn(LdcString("x")) should reuse the same CP index."""
        cp = ConstantPoolBuilder()
        items: list[CodeItem] = [
            LdcInsn(LdcString("shared")),
            LdcInsn(LdcString("shared")),
        ]
        from pytecode.edit.model import CodeModel as _CodeModel

        code = _CodeModel(max_stack=2, max_locals=1, instructions=items)
        attr = lower_code(code, cp)
        assert len(attr.code) == 2
        idx0 = attr.code[0]
        idx1 = attr.code[1]
        assert isinstance(idx0, (LocalIndex, ConstPoolIndex))
        assert isinstance(idx1, (LocalIndex, ConstPoolIndex))
        assert idx0.index == idx1.index  # same CP slot (dedup)

    def test_mixed_symbolic_and_raw_instructions(self) -> None:
        """Mix symbolic wrappers and plain InsnInfo in one code list."""
        items: list[CodeItem] = [
            InsnInfo(InsnInfoType.ICONST_0, -1),
            VarInsn(InsnInfoType.ISTORE, 5),  # slot 5 has no implicit ISTORE form
            IIncInsn(5, 5),
            FieldInsn(InsnInfoType.PUTSTATIC, "Holder", "value", "I"),
            InsnInfo(InsnInfoType.RETURN, -1),
        ]
        lowered = _lower_simple(items)
        assert len(lowered) == 5
        # ICONST_0 pass-through
        assert lowered[0].type == InsnInfoType.ICONST_0
        # ISTORE 5 → explicit LocalIndex (slot 5 has no shortcut)
        assert isinstance(lowered[1], LocalIndex)
        assert lowered[1].type == InsnInfoType.ISTORE
        assert lowered[1].index == 5
        # IInc narrow
        assert isinstance(lowered[2], IInc)
        # PUTSTATIC via FieldInsn
        assert isinstance(lowered[3], ConstPoolIndex)
        assert lowered[3].type == InsnInfoType.PUTSTATIC


# ---------------------------------------------------------------------------
# 17. Round-trip: InstructionShowcase
# ---------------------------------------------------------------------------


class TestInstructionShowcaseRoundTrip:
    def _assert_roundtrip(self, path: Path) -> None:
        from pytecode.classfile.attributes import CodeAttr
        from pytecode.classfile.reader import ClassReader

        original = ClassReader(path.read_bytes()).class_info
        model = ClassModel.from_classfile(original)
        restored = model.to_classfile()

        assert len(restored.methods) == len(original.methods)
        for orig_m, rest_m in zip(original.methods, restored.methods, strict=True):
            orig_code = next((a for a in orig_m.attributes if isinstance(a, CodeAttr)), None)
            rest_code = next((a for a in rest_m.attributes if isinstance(a, CodeAttr)), None)
            if orig_code is None:
                assert rest_code is None
                continue
            assert rest_code is not None
            assert rest_code.max_stacks == orig_code.max_stacks
            assert rest_code.max_locals == orig_code.max_locals
            assert rest_code.code == orig_code.code
            assert rest_code.exception_table == orig_code.exception_table

    def test_roundtrip_instruction_showcase(self, showcase_class: Path) -> None:
        self._assert_roundtrip(showcase_class)

    def test_all_symbolic_types_present(self, showcase_model: ClassModel) -> None:
        """At least one of each symbolic wrapper type must appear in the model."""
        all_insns: list[InsnInfo] = []
        for method in showcase_model.methods:
            if method.code:
                all_insns.extend(_insns(method.code.instructions))

        wrapper_types = (
            FieldInsn,
            MethodInsn,
            InterfaceMethodInsn,
            TypeInsn,
            LdcInsn,
            MultiANewArrayInsn,
            VarInsn,
            IIncInsn,
            InvokeDynamicInsn,
        )
        for wt in wrapper_types:
            assert any(isinstance(i, wt) for i in all_insns), f"No {wt.__name__} found in InstructionShowcase"
