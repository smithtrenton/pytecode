use thiserror::Error;

pub type Result<T> = std::result::Result<T, EngineError>;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{kind} at byte offset {offset}")]
pub struct EngineError {
    pub offset: usize,
    pub kind: EngineErrorKind,
}

impl EngineError {
    pub fn new(offset: usize, kind: EngineErrorKind) -> Self {
        Self { offset, kind }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EngineErrorKind {
    #[error("unexpected end of input (needed {needed} bytes, {remaining} remaining)")]
    UnexpectedEof { needed: usize, remaining: usize },
    #[error("invalid magic number 0x{found:08x}, requires 0x{expected:08x}")]
    InvalidMagic { found: u32, expected: u32 },
    #[error("invalid class file version {major}/{minor}")]
    InvalidVersion { major: u16, minor: u16 },
    #[error("invalid constant-pool tag {tag}")]
    InvalidConstantPoolTag { tag: u8 },
    #[error("invalid constant-pool slot {index}")]
    InvalidConstantPoolIndex { index: u16 },
    #[error("constant-pool slot {index} must be empty after a long/double entry")]
    ConstantPoolGapViolation { index: usize },
    #[error("constant pool is missing the trailing gap slot for a long/double entry")]
    MissingTrailingConstantPoolGap,
    #[error("invalid opcode 0x{opcode:02x}")]
    InvalidOpcode { opcode: u8 },
    #[error("invalid wide opcode 0x{opcode:02x}")]
    InvalidWideOpcode { opcode: u8 },
    #[error("invalid array type tag {atype}")]
    InvalidArrayType { atype: u8 },
    #[error("invalid modified UTF-8: {reason}")]
    InvalidModifiedUtf8 { reason: String },
    #[error("invalid descriptor: {reason}")]
    InvalidDescriptor { reason: String },
    #[error("invalid signature: {reason}")]
    InvalidSignature { reason: String },
    #[error("invalid attribute: {reason}")]
    InvalidAttribute { reason: String },
    #[error("invalid symbolic model: {reason}")]
    InvalidModelState { reason: String },
    #[error("writer invariant failed: {reason}")]
    InvalidWriterState { reason: String },
}
