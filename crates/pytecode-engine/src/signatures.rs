use crate::descriptors::BaseType;
use crate::error::{EngineError, EngineErrorKind, Result};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeSignature {
    Base(BaseType),
    Reference(ReferenceTypeSignature),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ReferenceTypeSignature {
    Class(ClassTypeSignature),
    TypeVariable(TypeVariableSignature),
    Array(ArrayTypeSignature),
}

pub type FieldSignature = ReferenceTypeSignature;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ArrayTypeSignature {
    pub component_type: Box<TypeSignature>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeVariableSignature {
    pub identifier: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClassTypeSignature {
    pub package_specifier: Vec<String>,
    pub simple_class: SimpleClassTypeSignature,
    pub suffixes: Vec<SimpleClassTypeSignature>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SimpleClassTypeSignature {
    pub identifier: String,
    pub type_arguments: Vec<TypeArgument>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeArgument {
    Any,
    Exact(ReferenceTypeSignature),
    Extends(ReferenceTypeSignature),
    Super(ReferenceTypeSignature),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeParameter {
    pub identifier: String,
    pub class_bound: Option<ReferenceTypeSignature>,
    pub interface_bounds: Vec<ReferenceTypeSignature>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClassSignature {
    pub type_parameters: Vec<TypeParameter>,
    pub superclass_signature: ClassTypeSignature,
    pub superinterface_signatures: Vec<ClassTypeSignature>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MethodSignature {
    pub type_parameters: Vec<TypeParameter>,
    pub parameter_types: Vec<TypeSignature>,
    pub result: ResultSignature,
    pub throws_signatures: Vec<ThrowsSignature>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ResultSignature {
    Void,
    Type(TypeSignature),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ThrowsSignature {
    Class(ClassTypeSignature),
    TypeVariable(TypeVariableSignature),
}

pub fn parse_type_signature(signature: &str) -> Result<TypeSignature> {
    let bytes = signature.as_bytes();
    let mut index = 0_usize;
    let parsed = parse_type_signature_at(bytes, &mut index)?;
    ensure_consumed(bytes, index)?;
    Ok(parsed)
}

pub fn parse_reference_type_signature(signature: &str) -> Result<ReferenceTypeSignature> {
    let bytes = signature.as_bytes();
    let mut index = 0_usize;
    let parsed = parse_reference_type_signature_at(bytes, &mut index)?;
    ensure_consumed(bytes, index)?;
    Ok(parsed)
}

pub fn parse_field_signature(signature: &str) -> Result<FieldSignature> {
    parse_reference_type_signature(signature)
}

pub fn parse_class_signature(signature: &str) -> Result<ClassSignature> {
    let bytes = signature.as_bytes();
    let mut index = 0_usize;
    let type_parameters = parse_optional_type_parameters_at(bytes, &mut index)?;
    let superclass_signature = parse_class_type_signature_at(bytes, &mut index)?;
    let mut superinterface_signatures = Vec::new();
    while index < bytes.len() {
        superinterface_signatures.push(parse_class_type_signature_at(bytes, &mut index)?);
    }
    Ok(ClassSignature {
        type_parameters,
        superclass_signature,
        superinterface_signatures,
    })
}

pub fn parse_method_signature(signature: &str) -> Result<MethodSignature> {
    let bytes = signature.as_bytes();
    let mut index = 0_usize;
    let type_parameters = parse_optional_type_parameters_at(bytes, &mut index)?;
    expect_byte(bytes, &mut index, b'(')?;
    let mut parameter_types = Vec::new();
    while peek(bytes, index) != Some(b')') {
        parameter_types.push(parse_type_signature_at(bytes, &mut index)?);
    }
    expect_byte(bytes, &mut index, b')')?;
    let result = if peek(bytes, index) == Some(b'V') {
        index += 1;
        ResultSignature::Void
    } else {
        ResultSignature::Type(parse_type_signature_at(bytes, &mut index)?)
    };
    let mut throws_signatures = Vec::new();
    while peek(bytes, index) == Some(b'^') {
        index += 1;
        throws_signatures.push(parse_throws_signature_at(bytes, &mut index)?);
    }
    ensure_consumed(bytes, index)?;
    Ok(MethodSignature {
        type_parameters,
        parameter_types,
        result,
        throws_signatures,
    })
}

pub fn is_valid_type_signature(signature: &str) -> bool {
    parse_type_signature(signature).is_ok()
}

pub fn is_valid_reference_type_signature(signature: &str) -> bool {
    parse_reference_type_signature(signature).is_ok()
}

pub fn is_valid_field_signature(signature: &str) -> bool {
    parse_field_signature(signature).is_ok()
}

pub fn is_valid_class_signature(signature: &str) -> bool {
    parse_class_signature(signature).is_ok()
}

pub fn is_valid_method_signature(signature: &str) -> bool {
    parse_method_signature(signature).is_ok()
}

fn parse_optional_type_parameters_at(
    bytes: &[u8],
    index: &mut usize,
) -> Result<Vec<TypeParameter>> {
    if peek(bytes, *index) != Some(b'<') {
        return Ok(Vec::new());
    }
    parse_type_parameters_at(bytes, index)
}

fn parse_type_parameters_at(bytes: &[u8], index: &mut usize) -> Result<Vec<TypeParameter>> {
    expect_byte(bytes, index, b'<')?;
    let mut parameters = Vec::new();
    while peek(bytes, *index) != Some(b'>') {
        parameters.push(parse_type_parameter_at(bytes, index)?);
    }
    if parameters.is_empty() {
        return Err(signature_error("type parameter list must not be empty"));
    }
    expect_byte(bytes, index, b'>')?;
    Ok(parameters)
}

fn parse_type_parameter_at(bytes: &[u8], index: &mut usize) -> Result<TypeParameter> {
    let identifier = parse_identifier_at(bytes, index, &[b':'])?;
    expect_byte(bytes, index, b':')?;
    let class_bound = if starts_reference_type_signature(peek(bytes, *index)) {
        Some(parse_reference_type_signature_at(bytes, index)?)
    } else {
        None
    };
    let mut interface_bounds = Vec::new();
    while peek(bytes, *index) == Some(b':') {
        *index += 1;
        interface_bounds.push(parse_reference_type_signature_at(bytes, index)?);
    }
    Ok(TypeParameter {
        identifier,
        class_bound,
        interface_bounds,
    })
}

fn parse_type_signature_at(bytes: &[u8], index: &mut usize) -> Result<TypeSignature> {
    if let Some(base_type) = parse_base_type(peek(bytes, *index)) {
        *index += 1;
        Ok(TypeSignature::Base(base_type))
    } else {
        Ok(TypeSignature::Reference(parse_reference_type_signature_at(
            bytes, index,
        )?))
    }
}

fn parse_reference_type_signature_at(
    bytes: &[u8],
    index: &mut usize,
) -> Result<ReferenceTypeSignature> {
    match peek(bytes, *index) {
        Some(b'L') => {
            parse_class_type_signature_at(bytes, index).map(ReferenceTypeSignature::Class)
        }
        Some(b'T') => {
            parse_type_variable_signature_at(bytes, index).map(ReferenceTypeSignature::TypeVariable)
        }
        Some(b'[') => {
            parse_array_type_signature_at(bytes, index).map(ReferenceTypeSignature::Array)
        }
        Some(value) => Err(signature_error(format!(
            "invalid reference type signature character '{}'",
            value as char
        ))),
        None => Err(signature_error("unexpected end")),
    }
}

fn parse_class_type_signature_at(bytes: &[u8], index: &mut usize) -> Result<ClassTypeSignature> {
    expect_byte(bytes, index, b'L')?;
    let mut package_specifier = Vec::new();
    let mut current = parse_identifier_at(bytes, index, &[b'/', b';', b'<', b'.'])?;
    while peek(bytes, *index) == Some(b'/') {
        package_specifier.push(current);
        *index += 1;
        current = parse_identifier_at(bytes, index, &[b'/', b';', b'<', b'.'])?;
    }
    let simple_class = SimpleClassTypeSignature {
        identifier: current,
        type_arguments: parse_optional_type_arguments_at(bytes, index)?,
    };
    let mut suffixes = Vec::new();
    while peek(bytes, *index) == Some(b'.') {
        *index += 1;
        suffixes.push(SimpleClassTypeSignature {
            identifier: parse_identifier_at(bytes, index, &[b';', b'<', b'.'])?,
            type_arguments: parse_optional_type_arguments_at(bytes, index)?,
        });
    }
    expect_byte(bytes, index, b';')?;
    Ok(ClassTypeSignature {
        package_specifier,
        simple_class,
        suffixes,
    })
}

fn parse_optional_type_arguments_at(bytes: &[u8], index: &mut usize) -> Result<Vec<TypeArgument>> {
    if peek(bytes, *index) != Some(b'<') {
        return Ok(Vec::new());
    }
    parse_type_arguments_at(bytes, index)
}

fn parse_type_arguments_at(bytes: &[u8], index: &mut usize) -> Result<Vec<TypeArgument>> {
    expect_byte(bytes, index, b'<')?;
    let mut arguments = Vec::new();
    while peek(bytes, *index) != Some(b'>') {
        arguments.push(parse_type_argument_at(bytes, index)?);
    }
    if arguments.is_empty() {
        return Err(signature_error("type argument list must not be empty"));
    }
    expect_byte(bytes, index, b'>')?;
    Ok(arguments)
}

fn parse_type_argument_at(bytes: &[u8], index: &mut usize) -> Result<TypeArgument> {
    match peek(bytes, *index) {
        Some(b'*') => {
            *index += 1;
            Ok(TypeArgument::Any)
        }
        Some(b'+') => {
            *index += 1;
            Ok(TypeArgument::Extends(parse_reference_type_signature_at(
                bytes, index,
            )?))
        }
        Some(b'-') => {
            *index += 1;
            Ok(TypeArgument::Super(parse_reference_type_signature_at(
                bytes, index,
            )?))
        }
        _ => Ok(TypeArgument::Exact(parse_reference_type_signature_at(
            bytes, index,
        )?)),
    }
}

fn parse_type_variable_signature_at(
    bytes: &[u8],
    index: &mut usize,
) -> Result<TypeVariableSignature> {
    expect_byte(bytes, index, b'T')?;
    let identifier = parse_identifier_at(bytes, index, &[b';'])?;
    expect_byte(bytes, index, b';')?;
    Ok(TypeVariableSignature { identifier })
}

fn parse_array_type_signature_at(bytes: &[u8], index: &mut usize) -> Result<ArrayTypeSignature> {
    expect_byte(bytes, index, b'[')?;
    Ok(ArrayTypeSignature {
        component_type: Box::new(parse_type_signature_at(bytes, index)?),
    })
}

fn parse_throws_signature_at(bytes: &[u8], index: &mut usize) -> Result<ThrowsSignature> {
    match peek(bytes, *index) {
        Some(b'L') => parse_class_type_signature_at(bytes, index).map(ThrowsSignature::Class),
        Some(b'T') => {
            parse_type_variable_signature_at(bytes, index).map(ThrowsSignature::TypeVariable)
        }
        Some(value) => Err(signature_error(format!(
            "invalid throws signature character '{}'",
            value as char
        ))),
        None => Err(signature_error("unexpected end")),
    }
}

fn parse_identifier_at(bytes: &[u8], index: &mut usize, terminators: &[u8]) -> Result<String> {
    let start = *index;
    while let Some(current) = peek(bytes, *index) {
        if terminators.contains(&current) {
            break;
        }
        *index += 1;
    }
    if *index == start {
        return Err(signature_error("empty signature identifier"));
    }
    let raw = std::str::from_utf8(&bytes[start..*index])
        .map_err(|_| signature_error("signature must be valid UTF-8"))?;
    validate_identifier(raw)?;
    Ok(raw.to_owned())
}

fn validate_identifier(identifier: &str) -> Result<()> {
    if identifier.is_empty() {
        return Err(signature_error("empty signature identifier"));
    }
    if identifier
        .bytes()
        .any(|byte| matches!(byte, b'.' | b';' | b'[' | b'/' | b'<' | b'>' | b':'))
    {
        return Err(signature_error(format!(
            "invalid character in signature identifier '{identifier}'"
        )));
    }
    Ok(())
}

fn parse_base_type(value: Option<u8>) -> Option<BaseType> {
    match value {
        Some(b'B') => Some(BaseType::Byte),
        Some(b'C') => Some(BaseType::Char),
        Some(b'D') => Some(BaseType::Double),
        Some(b'F') => Some(BaseType::Float),
        Some(b'I') => Some(BaseType::Int),
        Some(b'J') => Some(BaseType::Long),
        Some(b'S') => Some(BaseType::Short),
        Some(b'Z') => Some(BaseType::Boolean),
        _ => None,
    }
}

fn starts_reference_type_signature(value: Option<u8>) -> bool {
    matches!(value, Some(b'L' | b'T' | b'['))
}

fn expect_byte(bytes: &[u8], index: &mut usize, expected: u8) -> Result<()> {
    let current = next_byte(bytes, index)?;
    if current != expected {
        return Err(signature_error(format!("expected '{}'", expected as char)));
    }
    Ok(())
}

fn ensure_consumed(bytes: &[u8], index: usize) -> Result<()> {
    if index == bytes.len() {
        Ok(())
    } else {
        Err(signature_error(format!(
            "trailing characters at byte index {index}"
        )))
    }
}

fn next_byte(bytes: &[u8], index: &mut usize) -> Result<u8> {
    let current = peek(bytes, *index).ok_or_else(|| signature_error("unexpected end"))?;
    *index += 1;
    Ok(current)
}

fn peek(bytes: &[u8], index: usize) -> Option<u8> {
    bytes.get(index).copied()
}

fn signature_error(reason: impl Into<String>) -> EngineError {
    EngineError::new(
        0,
        EngineErrorKind::InvalidSignature {
            reason: reason.into(),
        },
    )
}
