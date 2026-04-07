use crate::descriptors::{parameter_slot_count, parse_method_descriptor};
use crate::{EngineError, EngineErrorKind, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldInsn {
    pub opcode: u8,
    pub owner: String,
    pub name: String,
    pub descriptor: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodInsn {
    pub opcode: u8,
    pub owner: String,
    pub name: String,
    pub descriptor: String,
    pub is_interface: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceMethodInsn {
    pub owner: String,
    pub name: String,
    pub descriptor: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeInsn {
    pub opcode: u8,
    pub descriptor: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarInsn {
    pub opcode: u8,
    pub slot: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IIncInsn {
    pub slot: u16,
    pub value: i16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiANewArrayInsn {
    pub descriptor: String,
    pub dimensions: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvokeDynamicInsn {
    pub bootstrap_method_attr_index: u16,
    pub name: String,
    pub descriptor: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LdcValue {
    Int(i32),
    FloatBits(u32),
    Long(i64),
    DoubleBits(u64),
    String(String),
    Class(String),
    MethodType(String),
    MethodHandle(MethodHandleValue),
    Dynamic(DynamicValue),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodHandleValue {
    pub reference_kind: u8,
    pub owner: String,
    pub name: String,
    pub descriptor: String,
    pub is_interface: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicValue {
    pub bootstrap_method_attr_index: u16,
    pub name: String,
    pub descriptor: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LdcInsn {
    pub value: LdcValue,
}

pub fn implicit_var_slot(opcode: u8) -> Option<(u8, u16)> {
    match opcode {
        0x1A => Some((0x15, 0)),
        0x1B => Some((0x15, 1)),
        0x1C => Some((0x15, 2)),
        0x1D => Some((0x15, 3)),
        0x1E => Some((0x16, 0)),
        0x1F => Some((0x16, 1)),
        0x20 => Some((0x16, 2)),
        0x21 => Some((0x16, 3)),
        0x22 => Some((0x17, 0)),
        0x23 => Some((0x17, 1)),
        0x24 => Some((0x17, 2)),
        0x25 => Some((0x17, 3)),
        0x26 => Some((0x18, 0)),
        0x27 => Some((0x18, 1)),
        0x28 => Some((0x18, 2)),
        0x29 => Some((0x18, 3)),
        0x2A => Some((0x19, 0)),
        0x2B => Some((0x19, 1)),
        0x2C => Some((0x19, 2)),
        0x2D => Some((0x19, 3)),
        0x3B => Some((0x36, 0)),
        0x3C => Some((0x36, 1)),
        0x3D => Some((0x36, 2)),
        0x3E => Some((0x36, 3)),
        0x3F => Some((0x37, 0)),
        0x40 => Some((0x37, 1)),
        0x41 => Some((0x37, 2)),
        0x42 => Some((0x37, 3)),
        0x43 => Some((0x38, 0)),
        0x44 => Some((0x38, 1)),
        0x45 => Some((0x38, 2)),
        0x46 => Some((0x38, 3)),
        0x47 => Some((0x39, 0)),
        0x48 => Some((0x39, 1)),
        0x49 => Some((0x39, 2)),
        0x4A => Some((0x39, 3)),
        0x4B => Some((0x3A, 0)),
        0x4C => Some((0x3A, 1)),
        0x4D => Some((0x3A, 2)),
        0x4E => Some((0x3A, 3)),
        _ => None,
    }
}

pub fn var_shortcut_opcode(opcode: u8, slot: u16) -> Option<u8> {
    match (opcode, slot) {
        (0x15, 0) => Some(0x1A),
        (0x15, 1) => Some(0x1B),
        (0x15, 2) => Some(0x1C),
        (0x15, 3) => Some(0x1D),
        (0x16, 0) => Some(0x1E),
        (0x16, 1) => Some(0x1F),
        (0x16, 2) => Some(0x20),
        (0x16, 3) => Some(0x21),
        (0x17, 0) => Some(0x22),
        (0x17, 1) => Some(0x23),
        (0x17, 2) => Some(0x24),
        (0x17, 3) => Some(0x25),
        (0x18, 0) => Some(0x26),
        (0x18, 1) => Some(0x27),
        (0x18, 2) => Some(0x28),
        (0x18, 3) => Some(0x29),
        (0x19, 0) => Some(0x2A),
        (0x19, 1) => Some(0x2B),
        (0x19, 2) => Some(0x2C),
        (0x19, 3) => Some(0x2D),
        (0x36, 0) => Some(0x3B),
        (0x36, 1) => Some(0x3C),
        (0x36, 2) => Some(0x3D),
        (0x36, 3) => Some(0x3E),
        (0x37, 0) => Some(0x3F),
        (0x37, 1) => Some(0x40),
        (0x37, 2) => Some(0x41),
        (0x37, 3) => Some(0x42),
        (0x38, 0) => Some(0x43),
        (0x38, 1) => Some(0x44),
        (0x38, 2) => Some(0x45),
        (0x38, 3) => Some(0x46),
        (0x39, 0) => Some(0x47),
        (0x39, 1) => Some(0x48),
        (0x39, 2) => Some(0x49),
        (0x39, 3) => Some(0x4A),
        (0x3A, 0) => Some(0x4B),
        (0x3A, 1) => Some(0x4C),
        (0x3A, 2) => Some(0x4D),
        (0x3A, 3) => Some(0x4E),
        _ => None,
    }
}

pub fn interface_method_count(descriptor: &str) -> Result<u8> {
    let parsed = parse_method_descriptor(descriptor)?;
    let slots = parameter_slot_count(&parsed) + 1;
    u8::try_from(slots).map_err(|_| {
        EngineError::new(
            0,
            EngineErrorKind::InvalidModelState {
                reason: format!("invokeinterface count exceeds u8 for descriptor {descriptor}"),
            },
        )
    })
}
