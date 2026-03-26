from __future__ import annotations

from pytecode.instructions import (
    ArrayType,
    Branch,
    BranchW,
    ByteValue,
    ConstPoolIndex,
    IInc,
    IIncW,
    InsnInfo,
    InsnInfoType,
    InvokeDynamic,
    InvokeInterface,
    LocalIndex,
    LocalIndexW,
    LookupSwitch,
    MultiANewArray,
    NewArray,
    ShortValue,
    TableSwitch,
)
from tests.helpers import class_reader_for_insns, i1, i2, i4, u1, u2, u4

# ---------------------------------------------------------------------------
# No-operand instructions (InsnInfo base class)
# ---------------------------------------------------------------------------


def test_insn_info_type_preserves_integer_value():
    assert int(InsnInfoType.IFEQ) == 0x99
    assert int(InsnInfoType.GOTO_W) == 0xC8


def test_nop():
    reader = class_reader_for_insns(u1(0x00))
    insn = reader.read_instruction(0)
    assert isinstance(insn, InsnInfo)
    assert insn.type == InsnInfoType.NOP
    assert insn.bytecode_offset == 0


def test_aconst_null():
    reader = class_reader_for_insns(u1(0x01))
    insn = reader.read_instruction(0)
    assert isinstance(insn, InsnInfo)
    assert insn.type == InsnInfoType.ACONST_NULL
    assert insn.bytecode_offset == 0


def test_iconst_0():
    reader = class_reader_for_insns(u1(0x03))
    insn = reader.read_instruction(0)
    assert isinstance(insn, InsnInfo)
    assert insn.type == InsnInfoType.ICONST_0
    assert insn.bytecode_offset == 0


def test_return():
    reader = class_reader_for_insns(u1(0xB1))
    insn = reader.read_instruction(0)
    assert isinstance(insn, InsnInfo)
    assert insn.type == InsnInfoType.RETURN
    assert insn.bytecode_offset == 0


def test_areturn():
    reader = class_reader_for_insns(u1(0xB0))
    insn = reader.read_instruction(0)
    assert isinstance(insn, InsnInfo)
    assert insn.type == InsnInfoType.ARETURN
    assert insn.bytecode_offset == 0


# ---------------------------------------------------------------------------
# LocalIndex — u1 local variable index
# ---------------------------------------------------------------------------


def test_iload():
    reader = class_reader_for_insns(u1(0x15) + u1(3))
    insn = reader.read_instruction(0)
    assert isinstance(insn, LocalIndex)
    assert insn.type == InsnInfoType.ILOAD
    assert insn.index == 3
    assert insn.bytecode_offset == 0


def test_aload():
    reader = class_reader_for_insns(u1(0x19) + u1(0))
    insn = reader.read_instruction(0)
    assert isinstance(insn, LocalIndex)
    assert insn.type == InsnInfoType.ALOAD
    assert insn.index == 0


def test_istore():
    reader = class_reader_for_insns(u1(0x36) + u1(255))
    insn = reader.read_instruction(0)
    assert isinstance(insn, LocalIndex)
    assert insn.type == InsnInfoType.ISTORE
    assert insn.index == 255


def test_astore():
    reader = class_reader_for_insns(u1(0x3A) + u1(1))
    insn = reader.read_instruction(0)
    assert isinstance(insn, LocalIndex)
    assert insn.type == InsnInfoType.ASTORE
    assert insn.index == 1


# ---------------------------------------------------------------------------
# ConstPoolIndex — u2 constant pool index
# ---------------------------------------------------------------------------


def test_ldc_w():
    reader = class_reader_for_insns(u1(0x13) + u2(0x00FF))
    insn = reader.read_instruction(0)
    assert isinstance(insn, ConstPoolIndex)
    assert insn.type == InsnInfoType.LDC_W
    assert insn.index == 255


def test_ldc2_w():
    reader = class_reader_for_insns(u1(0x14) + u2(10))
    insn = reader.read_instruction(0)
    assert isinstance(insn, ConstPoolIndex)
    assert insn.type == InsnInfoType.LDC2_W
    assert insn.index == 10


def test_getstatic():
    reader = class_reader_for_insns(u1(0xB2) + u2(42))
    insn = reader.read_instruction(0)
    assert isinstance(insn, ConstPoolIndex)
    assert insn.type == InsnInfoType.GETSTATIC
    assert insn.index == 42


def test_putstatic():
    reader = class_reader_for_insns(u1(0xB3) + u2(7))
    insn = reader.read_instruction(0)
    assert isinstance(insn, ConstPoolIndex)
    assert insn.type == InsnInfoType.PUTSTATIC
    assert insn.index == 7


def test_getfield():
    reader = class_reader_for_insns(u1(0xB4) + u2(15))
    insn = reader.read_instruction(0)
    assert isinstance(insn, ConstPoolIndex)
    assert insn.type == InsnInfoType.GETFIELD
    assert insn.index == 15


def test_putfield():
    reader = class_reader_for_insns(u1(0xB5) + u2(20))
    insn = reader.read_instruction(0)
    assert isinstance(insn, ConstPoolIndex)
    assert insn.type == InsnInfoType.PUTFIELD
    assert insn.index == 20


def test_invokevirtual():
    reader = class_reader_for_insns(u1(0xB6) + u2(100))
    insn = reader.read_instruction(0)
    assert isinstance(insn, ConstPoolIndex)
    assert insn.type == InsnInfoType.INVOKEVIRTUAL
    assert insn.index == 100


def test_invokespecial():
    reader = class_reader_for_insns(u1(0xB7) + u2(200))
    insn = reader.read_instruction(0)
    assert isinstance(insn, ConstPoolIndex)
    assert insn.type == InsnInfoType.INVOKESPECIAL
    assert insn.index == 200


def test_invokestatic():
    reader = class_reader_for_insns(u1(0xB8) + u2(300))
    insn = reader.read_instruction(0)
    assert isinstance(insn, ConstPoolIndex)
    assert insn.type == InsnInfoType.INVOKESTATIC
    assert insn.index == 300


def test_new():
    reader = class_reader_for_insns(u1(0xBB) + u2(50))
    insn = reader.read_instruction(0)
    assert isinstance(insn, ConstPoolIndex)
    assert insn.type == InsnInfoType.NEW
    assert insn.index == 50


def test_anewarray():
    reader = class_reader_for_insns(u1(0xBD) + u2(8))
    insn = reader.read_instruction(0)
    assert isinstance(insn, ConstPoolIndex)
    assert insn.type == InsnInfoType.ANEWARRAY
    assert insn.index == 8


def test_checkcast():
    reader = class_reader_for_insns(u1(0xC0) + u2(99))
    insn = reader.read_instruction(0)
    assert isinstance(insn, ConstPoolIndex)
    assert insn.type == InsnInfoType.CHECKCAST
    assert insn.index == 99


def test_instanceof():
    reader = class_reader_for_insns(u1(0xC1) + u2(33))
    insn = reader.read_instruction(0)
    assert isinstance(insn, ConstPoolIndex)
    assert insn.type == InsnInfoType.INSTANCEOF
    assert insn.index == 33


# ---------------------------------------------------------------------------
# ByteValue — signed i1
# ---------------------------------------------------------------------------


def test_bipush_positive():
    reader = class_reader_for_insns(u1(0x10) + i1(42))
    insn = reader.read_instruction(0)
    assert isinstance(insn, ByteValue)
    assert insn.type == InsnInfoType.BIPUSH
    assert insn.value == 42


def test_bipush_negative():
    reader = class_reader_for_insns(u1(0x10) + i1(-128))
    insn = reader.read_instruction(0)
    assert isinstance(insn, ByteValue)
    assert insn.value == -128


def test_bipush_zero():
    reader = class_reader_for_insns(u1(0x10) + i1(0))
    insn = reader.read_instruction(0)
    assert isinstance(insn, ByteValue)
    assert insn.value == 0


# ---------------------------------------------------------------------------
# ShortValue — signed i2
# ---------------------------------------------------------------------------


def test_sipush_positive():
    reader = class_reader_for_insns(u1(0x11) + i2(1000))
    insn = reader.read_instruction(0)
    assert isinstance(insn, ShortValue)
    assert insn.type == InsnInfoType.SIPUSH
    assert insn.value == 1000


def test_sipush_negative():
    reader = class_reader_for_insns(u1(0x11) + i2(-32768))
    insn = reader.read_instruction(0)
    assert isinstance(insn, ShortValue)
    assert insn.value == -32768


def test_sipush_max():
    reader = class_reader_for_insns(u1(0x11) + i2(32767))
    insn = reader.read_instruction(0)
    assert isinstance(insn, ShortValue)
    assert insn.value == 32767


# ---------------------------------------------------------------------------
# Branch — signed i2 offset
# ---------------------------------------------------------------------------


def test_ifeq_forward():
    reader = class_reader_for_insns(u1(0x99) + i2(10))
    insn = reader.read_instruction(0)
    assert isinstance(insn, Branch)
    assert insn.type == InsnInfoType.IFEQ
    assert insn.offset == 10


def test_ifeq_backward():
    reader = class_reader_for_insns(u1(0x99) + i2(-4))
    insn = reader.read_instruction(0)
    assert isinstance(insn, Branch)
    assert insn.offset == -4


def test_goto_forward():
    reader = class_reader_for_insns(u1(0xA7) + i2(100))
    insn = reader.read_instruction(0)
    assert isinstance(insn, Branch)
    assert insn.type == InsnInfoType.GOTO
    assert insn.offset == 100


def test_goto_backward():
    reader = class_reader_for_insns(u1(0xA7) + i2(-200))
    insn = reader.read_instruction(0)
    assert isinstance(insn, Branch)
    assert insn.offset == -200


def test_if_icmpeq():
    reader = class_reader_for_insns(u1(0x9F) + i2(8))
    insn = reader.read_instruction(0)
    assert isinstance(insn, Branch)
    assert insn.type == InsnInfoType.IF_ICMPEQ
    assert insn.offset == 8


def test_ifnonnull():
    reader = class_reader_for_insns(u1(0xC7) + i2(16))
    insn = reader.read_instruction(0)
    assert isinstance(insn, Branch)
    assert insn.type == InsnInfoType.IFNONNULL
    assert insn.offset == 16


# ---------------------------------------------------------------------------
# BranchW — signed i4 offset
# ---------------------------------------------------------------------------


def test_goto_w():
    reader = class_reader_for_insns(u1(0xC8) + i4(100000))
    insn = reader.read_instruction(0)
    assert isinstance(insn, BranchW)
    assert insn.type == InsnInfoType.GOTO_W
    assert insn.offset == 100000


def test_jsr_w():
    reader = class_reader_for_insns(u1(0xC9) + i4(-100000))
    insn = reader.read_instruction(0)
    assert isinstance(insn, BranchW)
    assert insn.type == InsnInfoType.JSR_W
    assert insn.offset == -100000


# ---------------------------------------------------------------------------
# IInc — u1 index + i1 value
# ---------------------------------------------------------------------------


def test_iinc_positive():
    reader = class_reader_for_insns(u1(0x84) + u1(2) + i1(1))
    insn = reader.read_instruction(0)
    assert isinstance(insn, IInc)
    assert insn.type == InsnInfoType.IINC
    assert insn.index == 2
    assert insn.value == 1


def test_iinc_negative():
    reader = class_reader_for_insns(u1(0x84) + u1(0) + i1(-5))
    insn = reader.read_instruction(0)
    assert isinstance(insn, IInc)
    assert insn.index == 0
    assert insn.value == -5


def test_iinc_max_index():
    reader = class_reader_for_insns(u1(0x84) + u1(255) + i1(127))
    insn = reader.read_instruction(0)
    assert isinstance(insn, IInc)
    assert insn.index == 255
    assert insn.value == 127


# ---------------------------------------------------------------------------
# InvokeDynamic — u2 index + 2 unused bytes
# ---------------------------------------------------------------------------


def test_invokedynamic():
    reader = class_reader_for_insns(u1(0xBA) + u2(7) + b"\x00\x00")
    insn = reader.read_instruction(0)
    assert isinstance(insn, InvokeDynamic)
    assert insn.type == InsnInfoType.INVOKEDYNAMIC
    assert insn.index == 7
    assert insn.unused == b"\x00\x00"


# ---------------------------------------------------------------------------
# InvokeInterface — u2 index + u1 count + 1 unused byte
# ---------------------------------------------------------------------------


def test_invokeinterface():
    reader = class_reader_for_insns(u1(0xB9) + u2(5) + u1(2) + b"\x00")
    insn = reader.read_instruction(0)
    assert isinstance(insn, InvokeInterface)
    assert insn.type == InsnInfoType.INVOKEINTERFACE
    assert insn.index == 5
    assert insn.count == 2


# ---------------------------------------------------------------------------
# NewArray — u1 atype mapped to ArrayType enum
# ---------------------------------------------------------------------------


def test_newarray_boolean():
    reader = class_reader_for_insns(u1(0xBC) + u1(4))
    insn = reader.read_instruction(0)
    assert isinstance(insn, NewArray)
    assert insn.type == InsnInfoType.NEWARRAY
    assert insn.atype == ArrayType.BOOLEAN


def test_newarray_char():
    reader = class_reader_for_insns(u1(0xBC) + u1(5))
    insn = reader.read_instruction(0)
    assert isinstance(insn, NewArray)
    assert insn.atype == ArrayType.CHAR


def test_newarray_int():
    reader = class_reader_for_insns(u1(0xBC) + u1(10))
    insn = reader.read_instruction(0)
    assert isinstance(insn, NewArray)
    assert insn.atype == ArrayType.INT


def test_newarray_long():
    reader = class_reader_for_insns(u1(0xBC) + u1(11))
    insn = reader.read_instruction(0)
    assert isinstance(insn, NewArray)
    assert insn.atype == ArrayType.LONG


# ---------------------------------------------------------------------------
# MultiANewArray — u2 index + u1 dimensions
# ---------------------------------------------------------------------------


def test_multianewarray():
    reader = class_reader_for_insns(u1(0xC5) + u2(10) + u1(3))
    insn = reader.read_instruction(0)
    assert isinstance(insn, MultiANewArray)
    assert insn.type == InsnInfoType.MULTIANEWARRAY
    assert insn.index == 10
    assert insn.dimensions == 3


# ---------------------------------------------------------------------------
# LookupSwitch — 4-byte aligned, i4 default, u4 npairs, [i4 match, i4 offset]*
#
# Padding: align_bytes = (4 - (current_method_offset+1) % 4) % 4
# At current_method_offset=0: (4 - 1%4) % 4 = 3 padding bytes
# At current_method_offset=3: (4 - 4%4) % 4 = 0 padding bytes
# ---------------------------------------------------------------------------


def test_lookupswitch_no_pairs():
    data = (
        u1(0xAB)  # LOOKUPSWITCH opcode
        + b"\x00\x00\x00"  # 3 padding bytes (align at offset 0)
        + i4(-1)  # default
        + u4(0)  # npairs=0
    )
    reader = class_reader_for_insns(data)
    insn = reader.read_instruction(0)
    assert isinstance(insn, LookupSwitch)
    assert insn.type == InsnInfoType.LOOKUPSWITCH
    assert insn.default == -1
    assert insn.npairs == 0
    assert insn.pairs == []


def test_lookupswitch_two_pairs():
    data = (
        u1(0xAB)  # LOOKUPSWITCH opcode
        + b"\x00\x00\x00"  # 3 padding bytes
        + i4(100)  # default=100
        + u4(2)  # npairs=2
        + i4(-1)
        + u4(5)  # pair 0: match=-1, offset=5
        + i4(0)
        + u4(10)  # pair 1: match=0, offset=10
    )
    reader = class_reader_for_insns(data)
    insn = reader.read_instruction(0)
    assert isinstance(insn, LookupSwitch)
    assert insn.default == 100
    assert insn.npairs == 2
    assert len(insn.pairs) == 2
    assert insn.pairs[0].match == -1
    assert insn.pairs[0].offset == 5
    assert insn.pairs[1].match == 0
    assert insn.pairs[1].offset == 10


def test_lookupswitch_negative_match():
    data = (
        u1(0xAB)  # LOOKUPSWITCH opcode
        + b"\x00\x00\x00"  # 3 padding bytes
        + i4(0)  # default=0
        + u4(1)  # npairs=1
        + i4(-2147483648)
        + u4(99)  # pair: match=INT_MIN, offset=99
    )
    reader = class_reader_for_insns(data)
    insn = reader.read_instruction(0)
    assert isinstance(insn, LookupSwitch)
    assert len(insn.pairs) == 1
    assert insn.pairs[0].match == -2147483648
    assert insn.pairs[0].offset == 99


def test_lookupswitch_negative_offset():
    data = (
        u1(0xAB)  # LOOKUPSWITCH opcode
        + b"\x00\x00\x00"  # 3 padding bytes
        + i4(0)  # default=0
        + u4(1)  # npairs=1
        + i4(7)
        + i4(-12)  # pair: match=7, offset=-12
    )
    reader = class_reader_for_insns(data)
    insn = reader.read_instruction(0)
    assert isinstance(insn, LookupSwitch)
    assert len(insn.pairs) == 1
    assert insn.pairs[0].match == 7
    assert insn.pairs[0].offset == -12


def test_lookupswitch_aligned_at_offset_3():
    # current_method_offset=3: (4 - (3+1)%4) % 4 = (4-0)%4 = 0 padding bytes
    data = (
        u1(0xAB)  # LOOKUPSWITCH opcode at offset 3
        + i4(50)  # default=50 (no padding)
        + u4(1)  # npairs=1
        + i4(10)
        + u4(20)  # pair: match=10, offset=20
    )
    reader = class_reader_for_insns(data)
    insn = reader.read_instruction(3)
    assert isinstance(insn, LookupSwitch)
    assert insn.default == 50
    assert insn.npairs == 1
    assert insn.pairs[0].match == 10
    assert insn.pairs[0].offset == 20


# ---------------------------------------------------------------------------
# TableSwitch — 4-byte aligned, i4 default, i4 low, i4 high, [i4]*(high-low+1)
# At current_method_offset=0: 3 padding bytes needed
# ---------------------------------------------------------------------------


def test_tableswitch_single_case():
    # low==high, 1 offset
    data = (
        u1(0xAA)  # TABLESWITCH opcode
        + b"\x00\x00\x00"  # 3 padding bytes
        + i4(0)  # default=0
        + i4(5)  # low=5
        + i4(5)  # high=5
        + i4(100)  # offsets[0]=100
    )
    reader = class_reader_for_insns(data)
    insn = reader.read_instruction(0)
    assert isinstance(insn, TableSwitch)
    assert insn.type == InsnInfoType.TABLESWITCH
    assert insn.low == insn.high
    assert insn.low == 5
    assert len(insn.offsets) == 1
    assert insn.offsets[0] == 100


def test_tableswitch_range():
    # low=1, high=3, 3 offsets
    data = (
        u1(0xAA)
        + b"\x00\x00\x00"  # 3 padding bytes
        + i4(0)  # default
        + i4(1)  # low=1
        + i4(3)  # high=3
        + i4(10)
        + i4(20)
        + i4(30)  # offsets
    )
    reader = class_reader_for_insns(data)
    insn = reader.read_instruction(0)
    assert isinstance(insn, TableSwitch)
    assert insn.low == 1
    assert insn.high == 3
    assert len(insn.offsets) == 3
    assert insn.offsets == [10, 20, 30]


def test_tableswitch_negative_range():
    # low=-2, high=0, 3 offsets
    data = (
        u1(0xAA)
        + b"\x00\x00\x00"  # 3 padding bytes
        + i4(99)  # default=99
        + i4(-2)  # low=-2
        + i4(0)  # high=0
        + i4(10)
        + i4(20)
        + i4(30)
    )
    reader = class_reader_for_insns(data)
    insn = reader.read_instruction(0)
    assert isinstance(insn, TableSwitch)
    assert insn.default == 99
    assert insn.low == -2
    assert insn.high == 0
    assert len(insn.offsets) == 3


# ---------------------------------------------------------------------------
# WIDE prefix (0xc4)
# Combined opcode = WIDE(0xC4) + sub-opcode
#   ILOADW  = 0xC4 + 0x15 = 0xD9
#   ALOADW  = 0xC4 + 0x19 = 0xDD
#   ISTOREW = 0xC4 + 0x36 = 0xFA
#   ASTOREW = 0xC4 + 0x3A = 0xFE
#   IINCW   = 0xC4 + 0x84 = 0x148
# ---------------------------------------------------------------------------


def test_wide_iload():
    reader = class_reader_for_insns(u1(0xC4) + u1(0x15) + u2(300))
    insn = reader.read_instruction(0)
    assert isinstance(insn, LocalIndexW)
    assert insn.type == InsnInfoType.ILOADW
    assert insn.index == 300


def test_wide_aload():
    reader = class_reader_for_insns(u1(0xC4) + u1(0x19) + u2(400))
    insn = reader.read_instruction(0)
    assert isinstance(insn, LocalIndexW)
    assert insn.type == InsnInfoType.ALOADW
    assert insn.index == 400


def test_wide_istore():
    reader = class_reader_for_insns(u1(0xC4) + u1(0x36) + u2(200))
    insn = reader.read_instruction(0)
    assert isinstance(insn, LocalIndexW)
    assert insn.type == InsnInfoType.ISTOREW
    assert insn.index == 200


def test_wide_astore():
    reader = class_reader_for_insns(u1(0xC4) + u1(0x3A) + u2(100))
    insn = reader.read_instruction(0)
    assert isinstance(insn, LocalIndexW)
    assert insn.type == InsnInfoType.ASTOREW
    assert insn.index == 100


def test_wide_iinc():
    reader = class_reader_for_insns(u1(0xC4) + u1(0x84) + u2(500) + i2(1000))
    insn = reader.read_instruction(0)
    assert isinstance(insn, IIncW)
    assert insn.type == InsnInfoType.IINCW
    assert insn.index == 500
    assert insn.value == 1000


# ---------------------------------------------------------------------------
# read_code_bytes sequence
# ---------------------------------------------------------------------------


def test_read_code_bytes_empty():
    reader = class_reader_for_insns(b"")
    result = reader.read_code_bytes(0)
    assert result == []


def test_read_code_bytes_single_insn():
    reader = class_reader_for_insns(u1(0x00))
    result = reader.read_code_bytes(1)
    assert len(result) == 1
    assert isinstance(result[0], InsnInfo)
    assert result[0].type == InsnInfoType.NOP


def test_read_code_bytes_multiple_insns():
    # NOP(1 byte) + BIPUSH 42(2 bytes) + RETURN(1 byte) = 4 bytes
    data = u1(0x00) + u1(0x10) + i1(42) + u1(0xB1)
    reader = class_reader_for_insns(data)
    result = reader.read_code_bytes(4)
    assert len(result) == 3
    assert result[0].type == InsnInfoType.NOP
    assert isinstance(result[1], ByteValue)
    assert result[1].type == InsnInfoType.BIPUSH
    assert result[1].value == 42
    assert result[2].type == InsnInfoType.RETURN


def test_read_code_bytes_tracks_offsets():
    # BIPUSH(1) at offset 0: 2 bytes
    # BIPUSH(2) at offset 2: 2 bytes
    # SIPUSH(3) at offset 4: 3 bytes
    # total = 7 bytes
    data = (
        u1(0x10)
        + i1(1)  # BIPUSH 1 at offset 0
        + u1(0x10)
        + i1(2)  # BIPUSH 2 at offset 2
        + u1(0x11)
        + i2(3)  # SIPUSH 3 at offset 4
    )
    reader = class_reader_for_insns(data)
    result = reader.read_code_bytes(7)
    assert len(result) == 3
    assert result[0].bytecode_offset == 0
    assert result[1].bytecode_offset == 2
    assert result[2].bytecode_offset == 4
