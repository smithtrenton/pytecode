#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchOffsetPair {
    pub match_value: i32,
    pub offset: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstructionOperands {
    None,
    LocalIndex(u16),
    LocalIndexWide(u16),
    ConstPoolIndex(u16),
    ByteValue(i8),
    ShortValue(i16),
    Branch(i16),
    BranchWide(i32),
    IInc {
        index: u16,
        value: i16,
    },
    InvokeDynamic {
        index: u16,
        unused: [u8; 2],
    },
    InvokeInterface {
        index: u16,
        count: u8,
        unused: u8,
    },
    NewArray(ArrayType),
    MultiANewArray {
        index: u16,
        dimensions: u8,
    },
    LookupSwitch {
        default: i32,
        pairs: Vec<MatchOffsetPair>,
    },
    TableSwitch {
        default: i32,
        low: i32,
        high: i32,
        offsets: Vec<i32>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instruction {
    pub opcode: u16,
    pub bytecode_offset: u32,
    pub operands: InstructionOperands,
}

pub const WIDE_PREFIX: u8 = 0xC4;

impl Instruction {
    pub const fn is_wide(&self) -> bool {
        if self.opcode > u8::MAX as u16 {
            return true;
        }
        match self.operands {
            InstructionOperands::LocalIndexWide(_) => true,
            InstructionOperands::IInc { index, value } => {
                index > u8::MAX as u16 || value < i8::MIN as i16 || value > i8::MAX as i16
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_wide_opcodes() {
        let insn = Instruction {
            opcode: 0xC4 + 0x15,
            bytecode_offset: 12,
            operands: InstructionOperands::LocalIndexWide(512),
        };
        assert!(insn.is_wide());
    }

    #[test]
    fn lookup_switch_pairs_are_preserved() {
        let pairs = vec![MatchOffsetPair {
            match_value: 7,
            offset: 24,
        }];
        let insn = Instruction {
            opcode: 0xAB,
            bytecode_offset: 3,
            operands: InstructionOperands::LookupSwitch { default: 1, pairs },
        };
        match insn.operands {
            InstructionOperands::LookupSwitch { default, pairs } => {
                assert_eq!(default, 1);
                assert_eq!(pairs.len(), 1);
            }
            _ => panic!("expected lookupswitch operands"),
        }
    }
}
