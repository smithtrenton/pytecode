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
    let parsed = parse_type_signature_at(bytes, &mut index, 0)?;
    ensure_consumed(bytes, index)?;
    Ok(parsed)
}

pub fn parse_reference_type_signature(signature: &str) -> Result<ReferenceTypeSignature> {
    let bytes = signature.as_bytes();
    let mut index = 0_usize;
    let parsed = parse_reference_type_signature_at(bytes, &mut index, 0)?;
    ensure_consumed(bytes, index)?;
    Ok(parsed)
}

pub fn parse_field_signature(signature: &str) -> Result<FieldSignature> {
    parse_reference_type_signature(signature)
}

pub fn parse_class_signature(signature: &str) -> Result<ClassSignature> {
    let bytes = signature.as_bytes();
    let mut index = 0_usize;
    let type_parameters = parse_optional_type_parameters_at(bytes, &mut index, 0)?;
    let superclass_signature = parse_class_type_signature_at(bytes, &mut index, 0)?;
    let mut superinterface_signatures = Vec::new();
    while index < bytes.len() {
        superinterface_signatures.push(parse_class_type_signature_at(bytes, &mut index, 0)?);
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
    let type_parameters = parse_optional_type_parameters_at(bytes, &mut index, 0)?;
    expect_byte(bytes, &mut index, b'(')?;
    let mut parameter_types = Vec::new();
    while peek(bytes, index) != Some(b')') {
        parameter_types.push(parse_type_signature_at(bytes, &mut index, 0)?);
    }
    expect_byte(bytes, &mut index, b')')?;
    let result = if peek(bytes, index) == Some(b'V') {
        index += 1;
        ResultSignature::Void
    } else {
        ResultSignature::Type(parse_type_signature_at(bytes, &mut index, 0)?)
    };
    let mut throws_signatures = Vec::new();
    while peek(bytes, index) == Some(b'^') {
        index += 1;
        throws_signatures.push(parse_throws_signature_at(bytes, &mut index, 0)?);
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
    depth: usize,
) -> Result<Vec<TypeParameter>> {
    if peek(bytes, *index) != Some(b'<') {
        return Ok(Vec::new());
    }
    parse_type_parameters_at(bytes, index, depth)
}

fn parse_type_parameters_at(
    bytes: &[u8],
    index: &mut usize,
    depth: usize,
) -> Result<Vec<TypeParameter>> {
    expect_byte(bytes, index, b'<')?;
    let mut parameters = Vec::new();
    while peek(bytes, *index) != Some(b'>') {
        parameters.push(parse_type_parameter_at(bytes, index, depth)?);
    }
    if parameters.is_empty() {
        return Err(signature_error(
            *index,
            "type parameter list must not be empty",
        ));
    }
    expect_byte(bytes, index, b'>')?;
    Ok(parameters)
}

fn parse_type_parameter_at(bytes: &[u8], index: &mut usize, depth: usize) -> Result<TypeParameter> {
    let identifier = parse_identifier_at(bytes, index, b":")?;
    expect_byte(bytes, index, b':')?;
    let class_bound = if starts_reference_type_signature(peek(bytes, *index)) {
        Some(parse_reference_type_signature_at(bytes, index, depth)?)
    } else {
        None
    };
    let mut interface_bounds = Vec::new();
    while peek(bytes, *index) == Some(b':') {
        *index += 1;
        interface_bounds.push(parse_reference_type_signature_at(bytes, index, depth)?);
    }
    Ok(TypeParameter {
        identifier,
        class_bound,
        interface_bounds,
    })
}

fn parse_type_signature_at(bytes: &[u8], index: &mut usize, depth: usize) -> Result<TypeSignature> {
    if let Some(base_type) = parse_base_type(peek(bytes, *index)) {
        *index += 1;
        Ok(TypeSignature::Base(base_type))
    } else {
        Ok(TypeSignature::Reference(parse_reference_type_signature_at(
            bytes, index, depth,
        )?))
    }
}

fn parse_reference_type_signature_at(
    bytes: &[u8],
    index: &mut usize,
    depth: usize,
) -> Result<ReferenceTypeSignature> {
    if depth >= 128 {
        return Err(signature_error(
            *index,
            "signature nesting exceeds limit of 128",
        ));
    }
    let depth = depth + 1;
    match peek(bytes, *index) {
        Some(b'L') => {
            parse_class_type_signature_at(bytes, index, depth).map(ReferenceTypeSignature::Class)
        }
        Some(b'T') => {
            parse_type_variable_signature_at(bytes, index).map(ReferenceTypeSignature::TypeVariable)
        }
        Some(b'[') => {
            parse_array_type_signature_at(bytes, index, depth).map(ReferenceTypeSignature::Array)
        }
        Some(value) => Err(signature_error(
            *index,
            if value.is_ascii_graphic() {
                format!(
                    "invalid reference type signature character '{}'",
                    value as char
                )
            } else {
                format!("invalid reference type signature byte 0x{value:02x}")
            },
        )),
        None => Err(signature_error(*index, "unexpected end")),
    }
}

fn parse_class_type_signature_at(
    bytes: &[u8],
    index: &mut usize,
    depth: usize,
) -> Result<ClassTypeSignature> {
    expect_byte(bytes, index, b'L')?;
    let mut package_specifier = Vec::new();
    let mut current = parse_identifier_at(bytes, index, b"/;<.")?;
    while peek(bytes, *index) == Some(b'/') {
        package_specifier.push(current);
        *index += 1;
        current = parse_identifier_at(bytes, index, b"/;<.")?;
    }
    let simple_class = SimpleClassTypeSignature {
        identifier: current,
        type_arguments: parse_optional_type_arguments_at(bytes, index, depth)?,
    };
    let mut suffixes = Vec::new();
    while peek(bytes, *index) == Some(b'.') {
        *index += 1;
        suffixes.push(SimpleClassTypeSignature {
            identifier: parse_identifier_at(bytes, index, b";<.")?,
            type_arguments: parse_optional_type_arguments_at(bytes, index, depth)?,
        });
    }
    expect_byte(bytes, index, b';')?;
    Ok(ClassTypeSignature {
        package_specifier,
        simple_class,
        suffixes,
    })
}

fn parse_optional_type_arguments_at(
    bytes: &[u8],
    index: &mut usize,
    depth: usize,
) -> Result<Vec<TypeArgument>> {
    if peek(bytes, *index) != Some(b'<') {
        return Ok(Vec::new());
    }
    parse_type_arguments_at(bytes, index, depth)
}

fn parse_type_arguments_at(
    bytes: &[u8],
    index: &mut usize,
    depth: usize,
) -> Result<Vec<TypeArgument>> {
    expect_byte(bytes, index, b'<')?;
    let mut arguments = Vec::new();
    while peek(bytes, *index) != Some(b'>') {
        arguments.push(parse_type_argument_at(bytes, index, depth)?);
    }
    if arguments.is_empty() {
        return Err(signature_error(
            *index,
            "type argument list must not be empty",
        ));
    }
    expect_byte(bytes, index, b'>')?;
    Ok(arguments)
}

fn parse_type_argument_at(bytes: &[u8], index: &mut usize, depth: usize) -> Result<TypeArgument> {
    match peek(bytes, *index) {
        Some(b'*') => {
            *index += 1;
            Ok(TypeArgument::Any)
        }
        Some(b'+') => {
            *index += 1;
            Ok(TypeArgument::Extends(parse_reference_type_signature_at(
                bytes, index, depth,
            )?))
        }
        Some(b'-') => {
            *index += 1;
            Ok(TypeArgument::Super(parse_reference_type_signature_at(
                bytes, index, depth,
            )?))
        }
        _ => Ok(TypeArgument::Exact(parse_reference_type_signature_at(
            bytes, index, depth,
        )?)),
    }
}

fn parse_type_variable_signature_at(
    bytes: &[u8],
    index: &mut usize,
) -> Result<TypeVariableSignature> {
    expect_byte(bytes, index, b'T')?;
    let identifier = parse_identifier_at(bytes, index, b";")?;
    expect_byte(bytes, index, b';')?;
    Ok(TypeVariableSignature { identifier })
}

fn parse_array_type_signature_at(
    bytes: &[u8],
    index: &mut usize,
    depth: usize,
) -> Result<ArrayTypeSignature> {
    expect_byte(bytes, index, b'[')?;
    let mut dimensions = 1;
    while peek(bytes, *index) == Some(b'[') {
        dimensions += 1;
        *index += 1;
        if dimensions > 255 {
            return Err(signature_error(
                *index,
                "array signature exceeds 255 dimensions",
            ));
        }
    }
    let mut component_type = parse_type_signature_at(bytes, index, depth)?;
    for _ in 1..dimensions {
        component_type =
            TypeSignature::Reference(ReferenceTypeSignature::Array(ArrayTypeSignature {
                component_type: Box::new(component_type),
            }));
    }
    Ok(ArrayTypeSignature {
        component_type: Box::new(component_type),
    })
}

fn parse_throws_signature_at(
    bytes: &[u8],
    index: &mut usize,
    depth: usize,
) -> Result<ThrowsSignature> {
    match peek(bytes, *index) {
        Some(b'L') => {
            parse_class_type_signature_at(bytes, index, depth).map(ThrowsSignature::Class)
        }
        Some(b'T') => {
            parse_type_variable_signature_at(bytes, index).map(ThrowsSignature::TypeVariable)
        }
        Some(value) => Err(signature_error(
            *index,
            if value.is_ascii_graphic() {
                format!("invalid throws signature character '{}'", value as char)
            } else {
                format!("invalid throws signature byte 0x{value:02x}")
            },
        )),
        None => Err(signature_error(*index, "unexpected end")),
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
        return Err(signature_error(start, "empty signature identifier"));
    }
    let raw = std::str::from_utf8(&bytes[start..*index])
        .map_err(|_| signature_error(start, "signature must be valid UTF-8"))?;
    validate_identifier(raw, start)?;
    Ok(raw.to_owned())
}

fn validate_identifier(identifier: &str, offset: usize) -> Result<()> {
    if identifier.is_empty() {
        return Err(signature_error(offset, "empty signature identifier"));
    }
    if identifier
        .bytes()
        .any(|byte| matches!(byte, b'.' | b';' | b'[' | b'/' | b'<' | b'>' | b':'))
    {
        return Err(signature_error(
            offset,
            format!("invalid character in signature identifier '{identifier}'"),
        ));
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
    let offset = *index;
    let current = next_byte(bytes, index)?;
    if current != expected {
        return Err(signature_error(
            offset,
            format!("expected '{}'", expected as char),
        ));
    }
    Ok(())
}

fn ensure_consumed(bytes: &[u8], index: usize) -> Result<()> {
    if index == bytes.len() {
        Ok(())
    } else {
        Err(signature_error(
            index,
            format!("trailing characters at byte index {index}"),
        ))
    }
}

fn next_byte(bytes: &[u8], index: &mut usize) -> Result<u8> {
    let current = peek(bytes, *index).ok_or_else(|| signature_error(*index, "unexpected end"))?;
    *index += 1;
    Ok(current)
}

fn peek(bytes: &[u8], index: usize) -> Option<u8> {
    bytes.get(index).copied()
}

fn signature_error(offset: usize, reason: impl Into<String>) -> EngineError {
    EngineError::new(
        offset,
        EngineErrorKind::InvalidSignature {
            reason: reason.into(),
        },
    )
}

pub fn to_signature_type(signature: &TypeSignature) -> String {
    signature.to_string()
}

pub fn to_signature_reference_type(signature: &ReferenceTypeSignature) -> String {
    signature.to_string()
}

pub fn to_signature_field(signature: &FieldSignature) -> String {
    signature.to_string()
}

pub fn to_signature_class(signature: &ClassSignature) -> String {
    signature.to_string()
}

pub fn to_signature_method(signature: &MethodSignature) -> String {
    signature.to_string()
}

fn write_type_parameters(out: &mut String, parameters: &[TypeParameter]) {
    if parameters.is_empty() {
        return;
    }
    out.push('<');
    for parameter in parameters {
        out.push_str(&parameter.identifier);
        out.push(':');
        if let Some(class_bound) = &parameter.class_bound {
            write_reference_type_signature(out, class_bound);
        }
        for interface_bound in &parameter.interface_bounds {
            out.push(':');
            write_reference_type_signature(out, interface_bound);
        }
    }
    out.push('>');
}

fn write_simple_class_type(out: &mut String, simple: &SimpleClassTypeSignature) {
    out.push_str(&simple.identifier);
    if !simple.type_arguments.is_empty() {
        out.push('<');
        for argument in &simple.type_arguments {
            match argument {
                TypeArgument::Any => out.push('*'),
                TypeArgument::Exact(sig) => write_reference_type_signature(out, sig),
                TypeArgument::Extends(sig) => {
                    out.push('+');
                    write_reference_type_signature(out, sig);
                }
                TypeArgument::Super(sig) => {
                    out.push('-');
                    write_reference_type_signature(out, sig);
                }
            }
        }
        out.push('>');
    }
}

fn write_class_type_signature(out: &mut String, signature: &ClassTypeSignature) {
    out.push('L');
    for pkg in &signature.package_specifier {
        out.push_str(pkg);
        out.push('/');
    }
    write_simple_class_type(out, &signature.simple_class);
    for suffix in &signature.suffixes {
        out.push('.');
        write_simple_class_type(out, suffix);
    }
    out.push(';');
}

fn write_reference_type_signature(out: &mut String, signature: &ReferenceTypeSignature) {
    match signature {
        ReferenceTypeSignature::Class(cls) => write_class_type_signature(out, cls),
        ReferenceTypeSignature::TypeVariable(tv) => {
            out.push('T');
            out.push_str(&tv.identifier);
            out.push(';');
        }
        ReferenceTypeSignature::Array(arr) => {
            out.push('[');
            write_type_signature(out, &arr.component_type);
        }
    }
}

fn write_type_signature(out: &mut String, signature: &TypeSignature) {
    match signature {
        TypeSignature::Base(base) => out.push(match base {
            BaseType::Boolean => 'Z',
            BaseType::Byte => 'B',
            BaseType::Char => 'C',
            BaseType::Short => 'S',
            BaseType::Int => 'I',
            BaseType::Long => 'J',
            BaseType::Float => 'F',
            BaseType::Double => 'D',
        }),
        TypeSignature::Reference(ref_sig) => write_reference_type_signature(out, ref_sig),
    }
}

impl std::fmt::Display for TypeSignature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut out = String::new();
        write_type_signature(&mut out, self);
        f.write_str(&out)
    }
}

impl std::fmt::Display for ReferenceTypeSignature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut out = String::new();
        write_reference_type_signature(&mut out, self);
        f.write_str(&out)
    }
}

impl std::fmt::Display for ClassTypeSignature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut out = String::new();
        write_class_type_signature(&mut out, self);
        f.write_str(&out)
    }
}

impl std::fmt::Display for ClassSignature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut out = String::new();
        write_type_parameters(&mut out, &self.type_parameters);
        write_class_type_signature(&mut out, &self.superclass_signature);
        for iface in &self.superinterface_signatures {
            write_class_type_signature(&mut out, iface);
        }
        f.write_str(&out)
    }
}

impl std::fmt::Display for MethodSignature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut out = String::new();
        write_type_parameters(&mut out, &self.type_parameters);
        out.push('(');
        for param in &self.parameter_types {
            write_type_signature(&mut out, param);
        }
        out.push(')');
        match &self.result {
            ResultSignature::Void => out.push('V'),
            ResultSignature::Type(ty) => write_type_signature(&mut out, ty),
        }
        for throws in &self.throws_signatures {
            out.push('^');
            match throws {
                ThrowsSignature::Class(cls) => write_class_type_signature(&mut out, cls),
                ThrowsSignature::TypeVariable(tv) => {
                    out.push('T');
                    out.push_str(&tv.identifier);
                    out.push(';');
                }
            }
        }
        f.write_str(&out)
    }
}
