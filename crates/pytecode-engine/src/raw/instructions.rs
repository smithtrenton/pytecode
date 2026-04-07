use crate::error::{EngineError, EngineErrorKind, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ArrayType {
    Boolean = 4,
    Char = 5,
    Float = 6,
    Double = 7,
    Byte = 8,
    Short = 9,
    Int = 10,
    Long = 11,
}

impl TryFrom<u8> for ArrayType {
    type Error = EngineErrorKind;

    fn try_from(value: u8) -> std::result::Result<Self, Self::Error> {
        match value {
            4 => Ok(Self::Boolean),
            5 => Ok(Self::Char),
            6 => Ok(Self::Float),
            7 => Ok(Self::Double),
            8 => Ok(Self::Byte),
            9 => Ok(Self::Short),
            10 => Ok(Self::Int),
            11 => Ok(Self::Long),
            atype => Err(EngineErrorKind::InvalidArrayType { atype }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstantPoolIndexWide {
    pub opcode: u8,
    pub offset: u32,
    pub index: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    pub opcode: u8,
    pub offset: u32,
    pub branch_offset: i16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvokeDynamicInsn {
    pub offset: u32,
    pub index: u16,
    pub reserved: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvokeInterfaceInsn {
    pub offset: u32,
    pub index: u16,
    pub count: u8,
    pub reserved: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewArrayInsn {
    pub offset: u32,
    pub atype: ArrayType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchOffsetPair {
    pub match_value: i32,
    pub offset: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LookupSwitchInsn {
    pub offset: u32,
    pub default_offset: i32,
    pub pairs: Vec<MatchOffsetPair>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableSwitchInsn {
    pub offset: u32,
    pub default_offset: i32,
    pub low: i32,
    pub high: i32,
    pub offsets: Vec<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WideInstruction {
    pub offset: u32,
    pub opcode: u8,
    pub index: u16,
    pub value: Option<i16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Instruction {
    Simple {
        opcode: u8,
        offset: u32,
    },
    LocalIndex {
        opcode: u8,
        offset: u32,
        index: u8,
    },
    ConstantPoolIndex1 {
        opcode: u8,
        offset: u32,
        index: u8,
    },
    ConstantPoolIndexWide(ConstantPoolIndexWide),
    Byte {
        opcode: u8,
        offset: u32,
        value: i8,
    },
    Short {
        opcode: u8,
        offset: u32,
        value: i16,
    },
    Branch(Branch),
    BranchWide {
        opcode: u8,
        offset: u32,
        branch_offset: i32,
    },
    IInc {
        offset: u32,
        index: u8,
        value: i8,
    },
    InvokeDynamic(InvokeDynamicInsn),
    InvokeInterface(InvokeInterfaceInsn),
    NewArray(NewArrayInsn),
    MultiANewArray {
        offset: u32,
        index: u16,
        dimensions: u8,
    },
    LookupSwitch(LookupSwitchInsn),
    TableSwitch(TableSwitchInsn),
    Wide(WideInstruction),
}

impl Instruction {
    pub fn opcode(&self) -> u8 {
        match self {
            Self::Simple { opcode, .. }
            | Self::LocalIndex { opcode, .. }
            | Self::ConstantPoolIndex1 { opcode, .. }
            | Self::Byte { opcode, .. }
            | Self::Short { opcode, .. }
            | Self::Branch(Branch { opcode, .. })
            | Self::BranchWide { opcode, .. } => *opcode,
            Self::ConstantPoolIndexWide(insn) => insn.opcode,
            Self::IInc { .. } => 0x84,
            Self::InvokeDynamic(_) => 0xBA,
            Self::InvokeInterface(_) => 0xB9,
            Self::NewArray(_) => 0xBC,
            Self::MultiANewArray { .. } => 0xC5,
            Self::LookupSwitch(_) => 0xAB,
            Self::TableSwitch(_) => 0xAA,
            Self::Wide(_) => 0xC4,
        }
    }

    pub fn offset(&self) -> u32 {
        match self {
            Self::Simple { offset, .. }
            | Self::LocalIndex { offset, .. }
            | Self::ConstantPoolIndex1 { offset, .. }
            | Self::Byte { offset, .. }
            | Self::Short { offset, .. }
            | Self::Branch(Branch { offset, .. })
            | Self::BranchWide { offset, .. } => *offset,
            Self::ConstantPoolIndexWide(insn) => insn.offset,
            Self::IInc { offset, .. } => *offset,
            Self::InvokeDynamic(insn) => insn.offset,
            Self::InvokeInterface(insn) => insn.offset,
            Self::NewArray(insn) => insn.offset,
            Self::MultiANewArray { offset, .. } => *offset,
            Self::LookupSwitch(insn) => insn.offset,
            Self::TableSwitch(insn) => insn.offset,
            Self::Wide(insn) => insn.offset,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OperandKind {
    Simple,
    LocalIndex,
    ConstantPoolIndex1,
    ConstantPoolIndexWide,
    Byte,
    Short,
    Branch,
    BranchWide,
    IInc,
    InvokeDynamic,
    InvokeInterface,
    NewArray,
    MultiANewArray,
    LookupSwitch,
    TableSwitch,
    Wide,
}

pub(crate) fn operand_kind(opcode: u8) -> Result<OperandKind> {
    let kind = match opcode {
        0x10 => OperandKind::Byte,
        0x11 => OperandKind::Short,
        0x12 => OperandKind::ConstantPoolIndex1,
        0x13 | 0x14 | 0xB2..=0xB8 | 0xBB | 0xBD | 0xC0 | 0xC1 => OperandKind::ConstantPoolIndexWide,
        0x15..=0x19 | 0x36..=0x3A | 0xA9 => OperandKind::LocalIndex,
        0x84 => OperandKind::IInc,
        0x99..=0xA8 | 0xC6 | 0xC7 => OperandKind::Branch,
        0xC8 | 0xC9 => OperandKind::BranchWide,
        0xB9 => OperandKind::InvokeInterface,
        0xBA => OperandKind::InvokeDynamic,
        0xBC => OperandKind::NewArray,
        0xC4 => OperandKind::Wide,
        0xC5 => OperandKind::MultiANewArray,
        0xAA => OperandKind::TableSwitch,
        0xAB => OperandKind::LookupSwitch,
        0x00..=0x0F | 0x1A..=0x35 | 0x3B..=0x83 | 0x85..=0x98 | 0xAC..=0xB1 | 0xBE..=0xC3 => {
            OperandKind::Simple
        }
        _ => {
            return Err(EngineError::new(
                0,
                EngineErrorKind::InvalidOpcode { opcode },
            ));
        }
    };
    Ok(kind)
}

pub(crate) fn validate_wide_opcode(opcode: u8, offset: usize) -> Result<()> {
    match opcode {
        0x15..=0x19 | 0x36..=0x3A | 0x84 | 0xA9 => Ok(()),
        _ => Err(EngineError::new(
            offset,
            EngineErrorKind::InvalidWideOpcode { opcode },
        )),
    }
}
