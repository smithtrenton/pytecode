from dataclasses import dataclass
from enum import IntEnum
from typing import List


@dataclass
class InsnInfo:
    type: "InsnInfoType"
    bytecode_offset: int


@dataclass
class LocalIndex(InsnInfo):
    index: int


@dataclass
class LocalIndexW(InsnInfo):
    index: int


@dataclass
class ConstPoolIndex(InsnInfo):
    index: int


@dataclass
class ByteValue(InsnInfo):
    value: int


@dataclass
class ShortValue(InsnInfo):
    value: int


@dataclass
class Branch(InsnInfo):
    offset: int


@dataclass
class BranchW(InsnInfo):
    offset: int


@dataclass
class IInc(InsnInfo):
    index: int
    value: int


@dataclass
class IIncW(InsnInfo):
    index: int
    value: int


@dataclass
class InvokeDynamic(InsnInfo):
    index: int
    unused: bytes


@dataclass
class InvokeInterface(InsnInfo):
    index: int
    count: int
    unused: bytes


@dataclass
class NewArray(InsnInfo):
    atype: "ArrayType"


@dataclass
class MultiANewArray(InsnInfo):
    index: int
    dimensions: int


@dataclass
class MatchOffsetPair:
    match: int
    offset: int


@dataclass
class LookupSwitch(InsnInfo):
    default: int
    npairs: int
    pairs: List[MatchOffsetPair]


@dataclass
class TableSwitch(InsnInfo):
    default: int
    low: int
    high: int
    offsets: List[int]


class InsnInfoType(IntEnum):
    AALOAD = 0x32, InsnInfo
    AASTORE = 0x53, InsnInfo
    ACONST_NULL = 0x01, InsnInfo
    ALOAD = 0x19, LocalIndex
    ALOAD_0 = 0x2A, InsnInfo
    ALOAD_1 = 0x2B, InsnInfo
    ALOAD_2 = 0x2C, InsnInfo
    ALOAD_3 = 0x2D, InsnInfo
    ANEWARRAY = 0xBD, ConstPoolIndex
    ARETURN = 0xB0, InsnInfo
    ARRAYLENGTH = 0xBE, InsnInfo
    ASTORE = 0x3A, LocalIndex
    ASTORE_0 = 0x4B, InsnInfo
    ASTORE_1 = 0x4C, InsnInfo
    ASTORE_2 = 0x4D, InsnInfo
    ASTORE_3 = 0x4E, InsnInfo
    ATHROW = 0xBF, InsnInfo
    BALOAD = 0x33, InsnInfo
    BASTORE = 0x54, InsnInfo
    BIPUSH = 0x10, ByteValue
    BREAKPOINT = 0xCA, InsnInfo
    CALOAD = 0x34, InsnInfo
    CASTORE = 0x55, InsnInfo
    CHECKCAST = 0xC0, ConstPoolIndex
    D2F = 0x90, InsnInfo
    D2I = 0x8E, InsnInfo
    D2L = 0x8F, InsnInfo
    DADD = 0x63, InsnInfo
    DALOAD = 0x31, InsnInfo
    DASTORE = 0x52, InsnInfo
    DCMPG = 0x98, InsnInfo
    DCMPL = 0x97, InsnInfo
    DCONST_0 = 0x0E, InsnInfo
    DCONST_1 = 0x0F, InsnInfo
    DDIV = 0x6F, InsnInfo
    DLOAD = 0x18, LocalIndex
    DLOAD_0 = 0x26, InsnInfo
    DLOAD_1 = 0x27, InsnInfo
    DLOAD_2 = 0x28, InsnInfo
    DLOAD_3 = 0x29, InsnInfo
    DMUL = 0x6B, InsnInfo
    DNEG = 0x77, InsnInfo
    DREM = 0x73, InsnInfo
    DRETURN = 0xAF, InsnInfo
    DSTORE = 0x39, LocalIndex
    DSTORE_0 = 0x47, InsnInfo
    DSTORE_1 = 0x48, InsnInfo
    DSTORE_2 = 0x49, InsnInfo
    DSTORE_3 = 0x4A, InsnInfo
    DSUB = 0x67, InsnInfo
    DUP = 0x59, InsnInfo
    DUP_X1 = 0x5A, InsnInfo
    DUP_X2 = 0x5B, InsnInfo
    DUP2 = 0x5C, InsnInfo
    DUP2_X1 = 0x5D, InsnInfo
    DUP2_X2 = 0x5E, InsnInfo
    F2D = 0x8D, InsnInfo
    F2I = 0x8B, InsnInfo
    F2L = 0x8C, InsnInfo
    FADD = 0x62, InsnInfo
    FALOAD = 0x30, InsnInfo
    FASTORE = 0x51, InsnInfo
    FCMPG = 0x96, InsnInfo
    FCMPL = 0x95, InsnInfo
    FCONST_0 = 0x0B, InsnInfo
    FCONST_1 = 0x0C, InsnInfo
    FCONST_2 = 0x0D, InsnInfo
    FDIV = 0x6E, InsnInfo
    FLOAD = 0x17, LocalIndex
    FLOAD_0 = 0x22, InsnInfo
    FLOAD_1 = 0x23, InsnInfo
    FLOAD_2 = 0x24, InsnInfo
    FLOAD_3 = 0x25, InsnInfo
    FMUL = 0x6A, InsnInfo
    FNEG = 0x76, InsnInfo
    FREM = 0x72, InsnInfo
    FRETURN = 0xAE, InsnInfo
    FSTORE = 0x38, LocalIndex
    FSTORE_0 = 0x43, InsnInfo
    FSTORE_1 = 0x44, InsnInfo
    FSTORE_2 = 0x45, InsnInfo
    FSTORE_3 = 0x46, InsnInfo
    FSUB = 0x66, InsnInfo
    GETFIELD = 0xB4, ConstPoolIndex
    GETSTATIC = 0xB2, ConstPoolIndex
    GOTO = 0xA7, Branch
    GOTO_W = 0xC8, BranchW
    I2B = 0x91, InsnInfo
    I2C = 0x92, InsnInfo
    I2D = 0x87, InsnInfo
    I2F = 0x86, InsnInfo
    I2L = 0x85, InsnInfo
    I2S = 0x93, InsnInfo
    IADD = 0x60, InsnInfo
    IALOAD = 0x2E, InsnInfo
    IAND = 0x7E, InsnInfo
    IASTORE = 0x4F, InsnInfo
    ICONST_M1 = 0x02, InsnInfo
    ICONST_0 = 0x03, InsnInfo
    ICONST_1 = 0x04, InsnInfo
    ICONST_2 = 0x05, InsnInfo
    ICONST_3 = 0x06, InsnInfo
    ICONST_4 = 0x07, InsnInfo
    ICONST_5 = 0x08, InsnInfo
    IDIV = 0x6C, InsnInfo
    IF_ACMPEQ = 0xA5, Branch
    IF_ACMPNE = 0xA6, Branch
    IF_ICMPEQ = 0x9F, Branch
    IF_ICMPGE = 0xA2, Branch
    IF_ICMPGT = 0xA3, Branch
    IF_ICMPLE = 0xA4, Branch
    IF_ICMPLT = 0xA1, Branch
    IF_ICMPNE = 0xA0, Branch
    IFEQ = 0x99, Branch
    IFGE = 0x9C, Branch
    IFGT = 0x9D, Branch
    IFLE = 0x9E, Branch
    IFLT = 0x9B, Branch
    IFNE = 0x9A, Branch
    IFNONNULL = 0xC7, Branch
    IFNULL = 0xC6, Branch
    IINC = 0x84, IInc
    ILOAD = 0x15, LocalIndex
    ILOAD_0 = 0x1A, InsnInfo
    ILOAD_1 = 0x1B, InsnInfo
    ILOAD_2 = 0x1C, InsnInfo
    ILOAD_3 = 0x1D, InsnInfo
    IMPDEP1 = 0xFE, InsnInfo
    IMPDEP2 = 0xFF, InsnInfo
    IMUL = 0x68, InsnInfo
    INEG = 0x74, InsnInfo
    INSTANCEOF = 0xC1, ConstPoolIndex
    INVOKEDYNAMIC = 0xBA, InvokeDynamic
    INVOKEINTERFACE = 0xB9, InvokeInterface
    INVOKESPECIAL = 0xB7, ConstPoolIndex
    INVOKESTATIC = 0xB8, ConstPoolIndex
    INVOKEVIRTUAL = 0xB6, ConstPoolIndex
    IOR = 0x80, InsnInfo
    IREM = 0x70, InsnInfo
    IRETURN = 0xAC, InsnInfo
    ISHL = 0x78, InsnInfo
    ISHR = 0x7A, InsnInfo
    ISTORE = 0x36, LocalIndex
    ISTORE_0 = 0x3B, InsnInfo
    ISTORE_1 = 0x3C, InsnInfo
    ISTORE_2 = 0x3D, InsnInfo
    ISTORE_3 = 0x3E, InsnInfo
    ISUB = 0x64, InsnInfo
    IUSHR = 0x7C, InsnInfo
    IXOR = 0x82, InsnInfo
    JSR = 0xA8, Branch
    JSR_W = 0xC9, BranchW
    L2D = 0x8A, InsnInfo
    L2F = 0x89, InsnInfo
    L2I = 0x88, InsnInfo
    LADD = 0x61, InsnInfo
    LALOAD = 0x2F, InsnInfo
    LAND = 0x7F, InsnInfo
    LASTORE = 0x50, InsnInfo
    LCMP = 0x94, InsnInfo
    LCONST_0 = 0x09, InsnInfo
    LCONST_1 = 0x0A, InsnInfo
    LDC = 0x12, LocalIndex
    LDC_W = 0x13, ConstPoolIndex
    LDC2_W = 0x14, ConstPoolIndex
    LDIV = 0x6D, InsnInfo
    LLOAD = 0x16, LocalIndex
    LLOAD_0 = 0x1E, InsnInfo
    LLOAD_1 = 0x1F, InsnInfo
    LLOAD_2 = 0x20, InsnInfo
    LLOAD_3 = 0x21, InsnInfo
    LMUL = 0x69, InsnInfo
    LNEG = 0x75, InsnInfo
    LOOKUPSWITCH = 0xAB, LookupSwitch
    LOR = 0x81, InsnInfo
    LREM = 0x71, InsnInfo
    LRETURN = 0xAD, InsnInfo
    LSHL = 0x79, InsnInfo
    LSHR = 0x7B, InsnInfo
    LSTORE = 0x37, LocalIndex
    LSTORE_0 = 0x3F, InsnInfo
    LSTORE_1 = 0x40, InsnInfo
    LSTORE_2 = 0x41, InsnInfo
    LSTORE_3 = 0x42, InsnInfo
    LSUB = 0x65, InsnInfo
    LUSHR = 0x7D, InsnInfo
    LXOR = 0x83, InsnInfo
    MONITORENTER = 0xC2, InsnInfo
    MONITOREXIT = 0xC3, InsnInfo
    MULTIANEWARRAY = 0xC5, MultiANewArray
    NEW = 0xBB, ConstPoolIndex
    NEWARRAY = 0xBC, NewArray
    NOP = 0x00, InsnInfo
    POP = 0x57, InsnInfo
    POP2 = 0x58, InsnInfo
    PUTFIELD = 0xB5, ConstPoolIndex
    PUTSTATIC = 0xB3, ConstPoolIndex
    RET = 0xA9, LocalIndex
    RETURN = 0xB1, InsnInfo
    SALOAD = 0x35, InsnInfo
    SASTORE = 0x56, InsnInfo
    SIPUSH = 0x11, ShortValue
    SWAP = 0x5F, InsnInfo
    TABLESWITCH = 0xAA, TableSwitch
    WIDE = 0xC4, InsnInfo
    ALOADW = (WIDE[0] + ALOAD[0]), LocalIndexW
    ASTOREW = (WIDE[0] + ASTORE[0]), LocalIndexW
    DLOADW = (WIDE[0] + DLOAD[0]), LocalIndexW
    DSTOREW = (WIDE[0] + DSTORE[0]), LocalIndexW
    FLOADW = (WIDE[0] + FLOAD[0]), LocalIndexW
    FSTOREW = (WIDE[0] + FSTORE[0]), LocalIndexW
    ILOADW = (WIDE[0] + ILOAD[0]), LocalIndexW
    ISTOREW = (WIDE[0] + ISTORE[0]), LocalIndexW
    LLOADW = (WIDE[0] + LLOAD[0]), LocalIndexW
    LSTOREW = (WIDE[0] + LSTORE[0]), LocalIndexW
    RETW = (WIDE[0] + RET[0]), LocalIndexW
    IINCW = (WIDE[0] + IINC[0]), IIncW

    def __new__(cls, value, instinfo):
        obj = int.__new__(cls)
        obj._value_ = value
        obj.instinfo = instinfo
        return obj


class ArrayType(IntEnum):
    BOOLEAN = 4
    CHAR = 5
    FLOAT = 6
    DOUBLE = 7
    BYTE = 8
    SHORT = 9
    INT = 10
    LONG = 11
