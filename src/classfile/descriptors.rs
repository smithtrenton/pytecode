use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyModule, PyTuple};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

impl BaseType {
    const fn from_byte(ch: u8) -> Option<Self> {
        match ch {
            b'Z' => Some(Self::Boolean),
            b'B' => Some(Self::Byte),
            b'C' => Some(Self::Char),
            b'S' => Some(Self::Short),
            b'I' => Some(Self::Int),
            b'J' => Some(Self::Long),
            b'F' => Some(Self::Float),
            b'D' => Some(Self::Double),
            _ => None,
        }
    }

    const fn code(self) -> u8 {
        match self {
            Self::Boolean => b'Z',
            Self::Byte => b'B',
            Self::Char => b'C',
            Self::Short => b'S',
            Self::Int => b'I',
            Self::Long => b'J',
            Self::Float => b'F',
            Self::Double => b'D',
        }
    }

    const fn python_name(self) -> &'static str {
        match self {
            Self::Boolean => "BOOLEAN",
            Self::Byte => "BYTE",
            Self::Char => "CHAR",
            Self::Short => "SHORT",
            Self::Int => "INT",
            Self::Long => "LONG",
            Self::Float => "FLOAT",
            Self::Double => "DOUBLE",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectType {
    pub class_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldDescriptor {
    Base(BaseType),
    Object(ObjectType),
    Array(Box<FieldDescriptor>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReturnType {
    Void,
    Field(FieldDescriptor),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MethodDescriptor {
    pub parameter_types: Vec<FieldDescriptor>,
    pub return_type: ReturnType,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeVariable {
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeArgument {
    pub wildcard: Option<char>,
    pub signature: Option<ReferenceTypeSignature>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InnerClassType {
    pub name: String,
    pub type_arguments: Vec<TypeArgument>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassTypeSignature {
    pub package: String,
    pub name: String,
    pub type_arguments: Vec<TypeArgument>,
    pub inner: Vec<InnerClassType>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArrayTypeSignature {
    pub component: Box<JavaTypeSignature>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReferenceTypeSignature {
    Class(ClassTypeSignature),
    TypeVariable(TypeVariable),
    Array(ArrayTypeSignature),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JavaTypeSignature {
    Base(BaseType),
    Reference(ReferenceTypeSignature),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeParameter {
    pub name: String,
    pub class_bound: Option<ReferenceTypeSignature>,
    pub interface_bounds: Vec<ReferenceTypeSignature>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassSignature {
    pub type_parameters: Vec<TypeParameter>,
    pub super_class: ClassTypeSignature,
    pub super_interfaces: Vec<ClassTypeSignature>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThrowsSignature {
    Class(ClassTypeSignature),
    TypeVariable(TypeVariable),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignatureReturnType {
    Void,
    Java(JavaTypeSignature),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MethodSignature {
    pub type_parameters: Vec<TypeParameter>,
    pub parameter_types: Vec<JavaTypeSignature>,
    pub return_type: SignatureReturnType,
    pub throws: Vec<ThrowsSignature>,
}

type ParseResult<T> = Result<T, String>;

const INVALID_UNQUALIFIED_NAME_CHARS: [u8; 7] = [b'.', b';', b'[', b'/', b'<', b'>', b':'];

struct Reader<'a> {
    s: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(s: &'a str) -> Self {
        Self {
            s: s.as_bytes(),
            pos: 0,
        }
    }

    const fn at_end(&self) -> bool {
        self.pos >= self.s.len()
    }

    fn original(&self) -> String {
        String::from_utf8_lossy(self.s).into_owned()
    }

    fn fail<T>(&self, msg: &str, pos: Option<usize>) -> ParseResult<T> {
        let p = pos.unwrap_or(self.pos);
        Err(format!(
            "{} at position {} in {:?}",
            msg,
            p,
            self.original()
        ))
    }

    fn peek(&self) -> ParseResult<u8> {
        if self.at_end() {
            return self.fail("unexpected end of string", None);
        }
        Ok(self.s[self.pos])
    }

    fn advance(&mut self) -> ParseResult<u8> {
        let ch = self.peek()?;
        self.pos += 1;
        Ok(ch)
    }

    fn expect(&mut self, ch: u8) -> ParseResult<()> {
        let actual = self.advance()?;
        if actual != ch {
            return self.fail(
                &format!("expected '{}', got '{}'", ch as char, actual as char),
                Some(self.pos - 1),
            );
        }
        Ok(())
    }
}

fn validate_unqualified_name(
    r: &Reader<'_>,
    name: &str,
    start: usize,
    context: &str,
) -> ParseResult<()> {
    if name.is_empty() {
        return r.fail(&format!("empty {context}"), Some(start));
    }
    for ch in name.bytes() {
        if INVALID_UNQUALIFIED_NAME_CHARS.contains(&ch) {
            return r.fail(
                &format!("invalid character '{}' in {context}", ch as char),
                Some(start),
            );
        }
    }
    Ok(())
}

fn validate_internal_class_name(r: &Reader<'_>, class_name: &str, start: usize) -> ParseResult<()> {
    if class_name.is_empty() {
        return r.fail("empty class name in object type", Some(start));
    }
    for segment in class_name.split('/') {
        if segment.is_empty() {
            return r.fail("empty class name segment in object type", Some(start));
        }
        validate_unqualified_name(r, segment, start, "class name")?;
    }
    Ok(())
}

fn split_class_type_identifier(
    r: &Reader<'_>,
    full_ident: &str,
    start: usize,
) -> ParseResult<(String, String)> {
    if full_ident.is_empty() {
        return r.fail("empty class name in class type signature", Some(start));
    }
    let segments: Vec<&str> = full_ident.split('/').collect();
    if segments.iter().any(|segment| segment.is_empty()) {
        return r.fail(
            "empty class name segment in class type signature",
            Some(start),
        );
    }
    for segment in &segments {
        validate_unqualified_name(r, segment, start, "class name")?;
    }
    if segments.len() == 1 {
        return Ok((String::new(), segments[0].to_owned()));
    }
    Ok((
        format!("{}/", segments[..segments.len() - 1].join("/")),
        segments[segments.len() - 1].to_owned(),
    ))
}

fn read_field_descriptor(r: &mut Reader<'_>) -> ParseResult<FieldDescriptor> {
    let ch = r.peek()?;
    if let Some(base_type) = BaseType::from_byte(ch) {
        r.advance()?;
        return Ok(FieldDescriptor::Base(base_type));
    }
    match ch {
        b'L' => read_object_type(r).map(FieldDescriptor::Object),
        b'[' => {
            r.advance()?;
            Ok(FieldDescriptor::Array(Box::new(read_field_descriptor(r)?)))
        }
        _ => r.fail(
            &format!("invalid descriptor character '{}'", ch as char),
            None,
        ),
    }
}

fn read_object_type(r: &mut Reader<'_>) -> ParseResult<ObjectType> {
    r.expect(b'L')?;
    let start = r.pos;
    while r.peek()? != b';' {
        r.advance()?;
    }
    let class_name = String::from_utf8_lossy(&r.s[start..r.pos]).into_owned();
    r.expect(b';')?;
    validate_internal_class_name(r, &class_name, start)?;
    Ok(ObjectType { class_name })
}

fn read_return_type(r: &mut Reader<'_>) -> ParseResult<ReturnType> {
    if r.peek()? == b'V' {
        r.advance()?;
        return Ok(ReturnType::Void);
    }
    Ok(ReturnType::Field(read_field_descriptor(r)?))
}

fn read_reference_type_signature(r: &mut Reader<'_>) -> ParseResult<ReferenceTypeSignature> {
    match r.peek()? {
        b'L' => read_class_type_signature(r).map(ReferenceTypeSignature::Class),
        b'T' => read_type_variable(r).map(ReferenceTypeSignature::TypeVariable),
        b'[' => read_array_type_signature(r).map(ReferenceTypeSignature::Array),
        ch => r.fail(
            &format!("expected reference type signature, got '{}'", ch as char),
            None,
        ),
    }
}

fn read_java_type_signature(r: &mut Reader<'_>) -> ParseResult<JavaTypeSignature> {
    let ch = r.peek()?;
    if let Some(base_type) = BaseType::from_byte(ch) {
        r.advance()?;
        return Ok(JavaTypeSignature::Base(base_type));
    }
    read_reference_type_signature(r).map(JavaTypeSignature::Reference)
}

fn read_type_variable(r: &mut Reader<'_>) -> ParseResult<TypeVariable> {
    r.expect(b'T')?;
    let start = r.pos;
    while r.peek()? != b';' {
        r.advance()?;
    }
    let name = String::from_utf8_lossy(&r.s[start..r.pos]).into_owned();
    r.expect(b';')?;
    validate_unqualified_name(r, &name, start, "type variable name")?;
    Ok(TypeVariable { name })
}

fn read_type_arguments(r: &mut Reader<'_>) -> ParseResult<Vec<TypeArgument>> {
    r.expect(b'<')?;
    let mut args = Vec::new();
    while r.peek()? != b'>' {
        args.push(read_type_argument(r)?);
    }
    r.expect(b'>')?;
    Ok(args)
}

fn read_type_argument(r: &mut Reader<'_>) -> ParseResult<TypeArgument> {
    let ch = r.peek()?;
    if ch == b'*' {
        r.advance()?;
        return Ok(TypeArgument {
            wildcard: None,
            signature: None,
        });
    }
    let wildcard = if ch == b'+' || ch == b'-' {
        r.advance()?;
        Some(ch as char)
    } else {
        None
    };
    Ok(TypeArgument {
        wildcard,
        signature: Some(read_reference_type_signature(r)?),
    })
}

fn read_class_type_signature(r: &mut Reader<'_>) -> ParseResult<ClassTypeSignature> {
    r.expect(b'L')?;
    let start = r.pos;
    let mut ident = Vec::new();
    while !matches!(r.peek()?, b'<' | b'.' | b';') {
        ident.push(r.advance()?);
    }
    let full_ident = String::from_utf8_lossy(&ident).into_owned();
    let (package, name) = split_class_type_identifier(r, &full_ident, start)?;
    let type_arguments = if !r.at_end() && r.peek()? == b'<' {
        read_type_arguments(r)?
    } else {
        Vec::new()
    };
    let mut inner = Vec::new();
    while !r.at_end() && r.peek()? == b'.' {
        r.advance()?;
        let inner_start = r.pos;
        let mut inner_name = Vec::new();
        while !matches!(r.peek()?, b'<' | b'.' | b';') {
            inner_name.push(r.advance()?);
        }
        let name = String::from_utf8_lossy(&inner_name).into_owned();
        validate_unqualified_name(r, &name, inner_start, "inner class name")?;
        let type_arguments = if !r.at_end() && r.peek()? == b'<' {
            read_type_arguments(r)?
        } else {
            Vec::new()
        };
        inner.push(InnerClassType {
            name,
            type_arguments,
        });
    }
    r.expect(b';')?;
    Ok(ClassTypeSignature {
        package,
        name,
        type_arguments,
        inner,
    })
}

fn read_array_type_signature(r: &mut Reader<'_>) -> ParseResult<ArrayTypeSignature> {
    r.expect(b'[')?;
    Ok(ArrayTypeSignature {
        component: Box::new(read_java_type_signature(r)?),
    })
}

fn read_type_parameters(r: &mut Reader<'_>) -> ParseResult<Vec<TypeParameter>> {
    r.expect(b'<')?;
    let mut params = Vec::new();
    while r.peek()? != b'>' {
        params.push(read_type_parameter(r)?);
    }
    r.expect(b'>')?;
    Ok(params)
}

fn read_type_parameter(r: &mut Reader<'_>) -> ParseResult<TypeParameter> {
    let start = r.pos;
    let mut name = Vec::new();
    while r.peek()? != b':' {
        name.push(r.advance()?);
    }
    let name = String::from_utf8_lossy(&name).into_owned();
    if name.is_empty() {
        return r.fail("empty type parameter name", Some(start));
    }
    r.expect(b':')?;
    let class_bound = if !r.at_end() && !matches!(r.peek()?, b':' | b'>') {
        Some(read_reference_type_signature(r)?)
    } else {
        None
    };
    let mut interface_bounds = Vec::new();
    while !r.at_end() && r.peek()? == b':' {
        r.advance()?;
        interface_bounds.push(read_reference_type_signature(r)?);
    }
    Ok(TypeParameter {
        name,
        class_bound,
        interface_bounds,
    })
}

fn read_return_type_signature(r: &mut Reader<'_>) -> ParseResult<SignatureReturnType> {
    if r.peek()? == b'V' {
        r.advance()?;
        return Ok(SignatureReturnType::Void);
    }
    Ok(SignatureReturnType::Java(read_java_type_signature(r)?))
}

fn read_throws_signature(r: &mut Reader<'_>) -> ParseResult<ThrowsSignature> {
    r.expect(b'^')?;
    match r.peek()? {
        b'L' => read_class_type_signature(r).map(ThrowsSignature::Class),
        b'T' => read_type_variable(r).map(ThrowsSignature::TypeVariable),
        ch => r.fail(
            &format!(
                "expected class type or type variable after '^', got '{}'",
                ch as char
            ),
            None,
        ),
    }
}

fn parse_field_descriptor_model(s: &str) -> ParseResult<FieldDescriptor> {
    let mut r = Reader::new(s);
    let result = read_field_descriptor(&mut r)?;
    if !r.at_end() {
        return r.fail("trailing characters after field descriptor", None);
    }
    Ok(result)
}

fn parse_method_descriptor_model(s: &str) -> ParseResult<MethodDescriptor> {
    let mut r = Reader::new(s);
    r.expect(b'(')?;
    let mut params = Vec::new();
    while r.peek()? != b')' {
        params.push(read_field_descriptor(&mut r)?);
    }
    r.expect(b')')?;
    let return_type = read_return_type(&mut r)?;
    if !r.at_end() {
        return r.fail("trailing characters after method descriptor", None);
    }
    Ok(MethodDescriptor {
        parameter_types: params,
        return_type,
    })
}

fn parse_class_signature_model(s: &str) -> ParseResult<ClassSignature> {
    let mut r = Reader::new(s);
    let type_parameters = if !r.at_end() && r.peek()? == b'<' {
        read_type_parameters(&mut r)?
    } else {
        Vec::new()
    };
    let super_class = read_class_type_signature(&mut r)?;
    let mut super_interfaces = Vec::new();
    while !r.at_end() {
        super_interfaces.push(read_class_type_signature(&mut r)?);
    }
    Ok(ClassSignature {
        type_parameters,
        super_class,
        super_interfaces,
    })
}

fn parse_method_signature_model(s: &str) -> ParseResult<MethodSignature> {
    let mut r = Reader::new(s);
    let type_parameters = if !r.at_end() && r.peek()? == b'<' {
        read_type_parameters(&mut r)?
    } else {
        Vec::new()
    };
    r.expect(b'(')?;
    let mut parameter_types = Vec::new();
    while r.peek()? != b')' {
        parameter_types.push(read_java_type_signature(&mut r)?);
    }
    r.expect(b')')?;
    let return_type = read_return_type_signature(&mut r)?;
    let mut throws = Vec::new();
    while !r.at_end() {
        throws.push(read_throws_signature(&mut r)?);
    }
    Ok(MethodSignature {
        type_parameters,
        parameter_types,
        return_type,
        throws,
    })
}

fn parse_field_signature_model(s: &str) -> ParseResult<ReferenceTypeSignature> {
    let mut r = Reader::new(s);
    let result = read_reference_type_signature(&mut r)?;
    if !r.at_end() {
        return r.fail("trailing characters after field signature", None);
    }
    Ok(result)
}

struct PythonTypes<'py> {
    module: Bound<'py, PyModule>,
}

impl<'py> PythonTypes<'py> {
    fn new(py: Python<'py>) -> PyResult<Self> {
        Ok(Self {
            module: PyModule::import(py, "pytecode.classfile.descriptors")?,
        })
    }

    fn class(&self, name: &str) -> PyResult<Bound<'py, PyAny>> {
        self.module.getattr(name)
    }

    fn void(&self) -> PyResult<Py<PyAny>> {
        Ok(self.module.getattr("VOID")?.unbind())
    }

    fn base_type(&self, base_type: BaseType) -> PyResult<Py<PyAny>> {
        Ok(self
            .class("BaseType")?
            .getattr(base_type.python_name())?
            .unbind())
    }

    fn field_descriptor(&self, descriptor: &FieldDescriptor) -> PyResult<Py<PyAny>> {
        match descriptor {
            FieldDescriptor::Base(base_type) => self.base_type(*base_type),
            FieldDescriptor::Object(obj) => Ok(self
                .class("ObjectType")?
                .call1((obj.class_name.clone(),))?
                .unbind()),
            FieldDescriptor::Array(component) => Ok(self
                .class("ArrayType")?
                .call1((self.field_descriptor(component)?,))?
                .unbind()),
        }
    }

    fn return_type(&self, return_type: &ReturnType) -> PyResult<Py<PyAny>> {
        match return_type {
            ReturnType::Void => self.void(),
            ReturnType::Field(field) => self.field_descriptor(field),
        }
    }

    fn method_descriptor(&self, descriptor: &MethodDescriptor) -> PyResult<Py<PyAny>> {
        let params: Vec<Py<PyAny>> = descriptor
            .parameter_types
            .iter()
            .map(|param| self.field_descriptor(param))
            .collect::<PyResult<_>>()?;
        let params = PyTuple::new(self.module.py(), params)?;
        Ok(self
            .class("MethodDescriptor")?
            .call1((params, self.return_type(&descriptor.return_type)?))?
            .unbind())
    }

    fn type_variable(&self, type_variable: &TypeVariable) -> PyResult<Py<PyAny>> {
        Ok(self
            .class("TypeVariable")?
            .call1((type_variable.name.clone(),))?
            .unbind())
    }

    fn type_argument(&self, argument: &TypeArgument) -> PyResult<Py<PyAny>> {
        let signature = match &argument.signature {
            Some(signature) => self.reference_type_signature(signature)?,
            None => self.module.py().None(),
        };
        Ok(self
            .class("TypeArgument")?
            .call1((argument.wildcard, signature))?
            .unbind())
    }

    fn inner_class_type(&self, inner: &InnerClassType) -> PyResult<Py<PyAny>> {
        let type_arguments: Vec<Py<PyAny>> = inner
            .type_arguments
            .iter()
            .map(|arg| self.type_argument(arg))
            .collect::<PyResult<_>>()?;
        let type_arguments = PyTuple::new(self.module.py(), type_arguments)?;
        Ok(self
            .class("InnerClassType")?
            .call1((inner.name.clone(), type_arguments))?
            .unbind())
    }

    fn class_type_signature(&self, signature: &ClassTypeSignature) -> PyResult<Py<PyAny>> {
        let type_arguments: Vec<Py<PyAny>> = signature
            .type_arguments
            .iter()
            .map(|arg| self.type_argument(arg))
            .collect::<PyResult<_>>()?;
        let inner: Vec<Py<PyAny>> = signature
            .inner
            .iter()
            .map(|item| self.inner_class_type(item))
            .collect::<PyResult<_>>()?;
        Ok(self
            .class("ClassTypeSignature")?
            .call1((
                signature.package.clone(),
                signature.name.clone(),
                PyTuple::new(self.module.py(), type_arguments)?,
                PyTuple::new(self.module.py(), inner)?,
            ))?
            .unbind())
    }

    fn array_type_signature(&self, signature: &ArrayTypeSignature) -> PyResult<Py<PyAny>> {
        Ok(self
            .class("ArrayTypeSignature")?
            .call1((self.java_type_signature(&signature.component)?,))?
            .unbind())
    }

    fn reference_type_signature(&self, signature: &ReferenceTypeSignature) -> PyResult<Py<PyAny>> {
        match signature {
            ReferenceTypeSignature::Class(class_type) => self.class_type_signature(class_type),
            ReferenceTypeSignature::TypeVariable(type_variable) => {
                self.type_variable(type_variable)
            }
            ReferenceTypeSignature::Array(array_type) => self.array_type_signature(array_type),
        }
    }

    fn java_type_signature(&self, signature: &JavaTypeSignature) -> PyResult<Py<PyAny>> {
        match signature {
            JavaTypeSignature::Base(base_type) => self.base_type(*base_type),
            JavaTypeSignature::Reference(reference) => self.reference_type_signature(reference),
        }
    }

    fn type_parameter(&self, parameter: &TypeParameter) -> PyResult<Py<PyAny>> {
        let class_bound = match &parameter.class_bound {
            Some(bound) => self.reference_type_signature(bound)?,
            None => self.module.py().None(),
        };
        let interface_bounds: Vec<Py<PyAny>> = parameter
            .interface_bounds
            .iter()
            .map(|bound| self.reference_type_signature(bound))
            .collect::<PyResult<_>>()?;
        Ok(self
            .class("TypeParameter")?
            .call1((
                parameter.name.clone(),
                class_bound,
                PyTuple::new(self.module.py(), interface_bounds)?,
            ))?
            .unbind())
    }

    fn class_signature(&self, signature: &ClassSignature) -> PyResult<Py<PyAny>> {
        let type_parameters: Vec<Py<PyAny>> = signature
            .type_parameters
            .iter()
            .map(|param| self.type_parameter(param))
            .collect::<PyResult<_>>()?;
        let super_interfaces: Vec<Py<PyAny>> = signature
            .super_interfaces
            .iter()
            .map(|iface| self.class_type_signature(iface))
            .collect::<PyResult<_>>()?;
        Ok(self
            .class("ClassSignature")?
            .call1((
                PyTuple::new(self.module.py(), type_parameters)?,
                self.class_type_signature(&signature.super_class)?,
                PyTuple::new(self.module.py(), super_interfaces)?,
            ))?
            .unbind())
    }

    fn throws_signature(&self, signature: &ThrowsSignature) -> PyResult<Py<PyAny>> {
        match signature {
            ThrowsSignature::Class(class_type) => self.class_type_signature(class_type),
            ThrowsSignature::TypeVariable(type_variable) => self.type_variable(type_variable),
        }
    }

    fn signature_return_type(&self, return_type: &SignatureReturnType) -> PyResult<Py<PyAny>> {
        match return_type {
            SignatureReturnType::Void => self.void(),
            SignatureReturnType::Java(signature) => self.java_type_signature(signature),
        }
    }

    fn method_signature(&self, signature: &MethodSignature) -> PyResult<Py<PyAny>> {
        let type_parameters: Vec<Py<PyAny>> = signature
            .type_parameters
            .iter()
            .map(|param| self.type_parameter(param))
            .collect::<PyResult<_>>()?;
        let parameter_types: Vec<Py<PyAny>> = signature
            .parameter_types
            .iter()
            .map(|param| self.java_type_signature(param))
            .collect::<PyResult<_>>()?;
        let throws: Vec<Py<PyAny>> = signature
            .throws
            .iter()
            .map(|item| self.throws_signature(item))
            .collect::<PyResult<_>>()?;
        Ok(self
            .class("MethodSignature")?
            .call1((
                PyTuple::new(self.module.py(), type_parameters)?,
                PyTuple::new(self.module.py(), parameter_types)?,
                self.signature_return_type(&signature.return_type)?,
                PyTuple::new(self.module.py(), throws)?,
            ))?
            .unbind())
    }
}

fn value_error(err: String) -> PyErr {
    PyValueError::new_err(err)
}

#[pyfunction]
pub fn parse_field_descriptor(py: Python<'_>, s: &str) -> PyResult<Py<PyAny>> {
    PythonTypes::new(py)?.field_descriptor(&parse_field_descriptor_model(s).map_err(value_error)?)
}

#[pyfunction]
pub fn parse_method_descriptor(py: Python<'_>, s: &str) -> PyResult<Py<PyAny>> {
    PythonTypes::new(py)?.method_descriptor(&parse_method_descriptor_model(s).map_err(value_error)?)
}

#[pyfunction]
pub fn parse_class_signature(py: Python<'_>, s: &str) -> PyResult<Py<PyAny>> {
    PythonTypes::new(py)?.class_signature(&parse_class_signature_model(s).map_err(value_error)?)
}

#[pyfunction]
pub fn parse_method_signature(py: Python<'_>, s: &str) -> PyResult<Py<PyAny>> {
    PythonTypes::new(py)?.method_signature(&parse_method_signature_model(s).map_err(value_error)?)
}

#[pyfunction]
pub fn parse_field_signature(py: Python<'_>, s: &str) -> PyResult<Py<PyAny>> {
    PythonTypes::new(py)?
        .reference_type_signature(&parse_field_signature_model(s).map_err(value_error)?)
}

#[pyfunction]
pub fn is_valid_field_descriptor(s: &str) -> bool {
    parse_field_descriptor_model(s).is_ok()
}

#[pyfunction]
pub fn is_valid_method_descriptor(s: &str) -> bool {
    parse_method_descriptor_model(s).is_ok()
}

pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent.py(), "descriptors")?;
    m.add_function(wrap_pyfunction!(parse_field_descriptor, &m)?)?;
    m.add_function(wrap_pyfunction!(parse_method_descriptor, &m)?)?;
    m.add_function(wrap_pyfunction!(parse_class_signature, &m)?)?;
    m.add_function(wrap_pyfunction!(parse_method_signature, &m)?)?;
    m.add_function(wrap_pyfunction!(parse_field_signature, &m)?)?;
    m.add_function(wrap_pyfunction!(is_valid_field_descriptor, &m)?)?;
    m.add_function(wrap_pyfunction!(is_valid_method_descriptor, &m)?)?;
    crate::register_submodule(parent, &m, "pytecode._rust.classfile.descriptors")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_method_descriptor_model() {
        let descriptor = parse_method_descriptor_model("(IDLjava/lang/String;)V").unwrap();
        assert_eq!(descriptor.parameter_types.len(), 3);
        assert!(matches!(descriptor.return_type, ReturnType::Void));
    }

    #[test]
    fn rejects_invalid_field_descriptor() {
        let err = parse_field_descriptor_model("V").unwrap_err();
        assert!(err.contains("invalid descriptor character"));
    }

    #[test]
    fn parses_method_signature_model() {
        let signature = parse_method_signature_model("<T:Ljava/lang/Object;>(TT;)TT;").unwrap();
        assert_eq!(signature.type_parameters.len(), 1);
        assert_eq!(signature.parameter_types.len(), 1);
        assert!(matches!(
            signature.return_type,
            SignatureReturnType::Java(_)
        ));
    }
}
