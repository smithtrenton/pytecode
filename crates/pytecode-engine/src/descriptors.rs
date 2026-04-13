use crate::error::{EngineError, EngineErrorKind, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BaseType {
    Boolean,
    Byte,
    Char,
    Short,
    Int,
    Long,
    Float,
    Double,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ObjectType {
    pub class_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ArrayType {
    pub component_type: Box<FieldDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FieldDescriptor {
    Base(BaseType),
    Object(ObjectType),
    Array(ArrayType),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VoidType {
    Void,
}

pub const VOID: VoidType = VoidType::Void;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ReturnType {
    Void,
    Field(FieldDescriptor),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MethodDescriptor {
    pub parameter_types: Vec<FieldDescriptor>,
    pub return_type: ReturnType,
}

pub fn parse_field_descriptor(descriptor: &str) -> Result<FieldDescriptor> {
    let bytes = descriptor.as_bytes();
    let mut index = 0_usize;
    let parsed = parse_field_descriptor_at(bytes, &mut index)?;
    if index != bytes.len() {
        return Err(descriptor_error(format!(
            "trailing characters at byte index {index}"
        )));
    }
    Ok(parsed)
}

pub fn parse_method_descriptor(descriptor: &str) -> Result<MethodDescriptor> {
    let bytes = descriptor.as_bytes();
    let mut index = 0_usize;
    expect_byte(bytes, &mut index, b'(')?;
    let mut parameter_types = Vec::new();
    while peek(bytes, index) != Some(b')') {
        parameter_types.push(parse_field_descriptor_at(bytes, &mut index)?);
    }
    expect_byte(bytes, &mut index, b')')?;
    let return_type = if peek(bytes, index) == Some(b'V') {
        index += 1;
        ReturnType::Void
    } else {
        ReturnType::Field(parse_field_descriptor_at(bytes, &mut index)?)
    };
    if index != bytes.len() {
        return Err(descriptor_error(format!(
            "trailing characters at byte index {index}"
        )));
    }
    Ok(MethodDescriptor {
        parameter_types,
        return_type,
    })
}

pub fn is_valid_field_descriptor(descriptor: &str) -> bool {
    parse_field_descriptor(descriptor).is_ok()
}

pub fn is_valid_method_descriptor(descriptor: &str) -> bool {
    parse_method_descriptor(descriptor).is_ok()
}

pub fn slot_size(descriptor: &FieldDescriptor) -> usize {
    match descriptor {
        FieldDescriptor::Base(BaseType::Long | BaseType::Double) => 2,
        _ => 1,
    }
}

pub fn parameter_slot_count(descriptor: &MethodDescriptor) -> usize {
    descriptor.parameter_types.iter().map(slot_size).sum()
}

pub fn to_descriptor_field(descriptor: &FieldDescriptor) -> String {
    match descriptor {
        FieldDescriptor::Base(base) => match base {
            BaseType::Boolean => "Z".to_owned(),
            BaseType::Byte => "B".to_owned(),
            BaseType::Char => "C".to_owned(),
            BaseType::Short => "S".to_owned(),
            BaseType::Int => "I".to_owned(),
            BaseType::Long => "J".to_owned(),
            BaseType::Float => "F".to_owned(),
            BaseType::Double => "D".to_owned(),
        },
        FieldDescriptor::Object(object) => format!("L{};", object.class_name),
        FieldDescriptor::Array(array) => format!("[{}", to_descriptor_field(&array.component_type)),
    }
}

pub fn to_descriptor_method(descriptor: &MethodDescriptor) -> String {
    let mut out = String::from("(");
    for parameter in &descriptor.parameter_types {
        out.push_str(&to_descriptor_field(parameter));
    }
    out.push(')');
    match &descriptor.return_type {
        ReturnType::Void => out.push('V'),
        ReturnType::Field(field) => out.push_str(&to_descriptor_field(field)),
    }
    out
}

fn parse_field_descriptor_at(bytes: &[u8], index: &mut usize) -> Result<FieldDescriptor> {
    let current = next_byte(bytes, index)?;
    match current {
        b'Z' => Ok(FieldDescriptor::Base(BaseType::Boolean)),
        b'B' => Ok(FieldDescriptor::Base(BaseType::Byte)),
        b'C' => Ok(FieldDescriptor::Base(BaseType::Char)),
        b'S' => Ok(FieldDescriptor::Base(BaseType::Short)),
        b'I' => Ok(FieldDescriptor::Base(BaseType::Int)),
        b'J' => Ok(FieldDescriptor::Base(BaseType::Long)),
        b'F' => Ok(FieldDescriptor::Base(BaseType::Float)),
        b'D' => Ok(FieldDescriptor::Base(BaseType::Double)),
        b'L' => parse_object_type(bytes, index).map(FieldDescriptor::Object),
        b'[' => Ok(FieldDescriptor::Array(ArrayType {
            component_type: Box::new(parse_field_descriptor_at(bytes, index)?),
        })),
        value => Err(descriptor_error(format!(
            "invalid descriptor character '{}'",
            value as char
        ))),
    }
}

fn parse_object_type(bytes: &[u8], index: &mut usize) -> Result<ObjectType> {
    let start = *index;
    while let Some(current) = peek(bytes, *index) {
        if current == b';' {
            break;
        }
        *index += 1;
    }
    if peek(bytes, *index).is_none() {
        return Err(descriptor_error("unexpected end while parsing object type"));
    }
    if *index == start {
        return Err(descriptor_error("empty class name"));
    }
    let raw = std::str::from_utf8(&bytes[start..*index])
        .map_err(|_| descriptor_error("descriptor must be ASCII"))?;
    validate_internal_name(raw)?;
    *index += 1;
    Ok(ObjectType {
        class_name: raw.to_owned(),
    })
}

fn validate_internal_name(value: &str) -> Result<()> {
    if value.contains('.') {
        return Err(descriptor_error("invalid character '.' in class name"));
    }
    if value.split('/').any(str::is_empty) {
        return Err(descriptor_error("empty class name segment"));
    }
    Ok(())
}

fn next_byte(bytes: &[u8], index: &mut usize) -> Result<u8> {
    let current = peek(bytes, *index).ok_or_else(|| descriptor_error("unexpected end"))?;
    *index += 1;
    Ok(current)
}

fn expect_byte(bytes: &[u8], index: &mut usize, expected: u8) -> Result<()> {
    let current = next_byte(bytes, index)?;
    if current != expected {
        return Err(descriptor_error(format!("expected '{}'", expected as char)));
    }
    Ok(())
}

fn peek(bytes: &[u8], index: usize) -> Option<u8> {
    bytes.get(index).copied()
}

fn descriptor_error(reason: impl Into<String>) -> EngineError {
    EngineError::new(
        0,
        EngineErrorKind::InvalidDescriptor {
            reason: reason.into(),
        },
    )
}
