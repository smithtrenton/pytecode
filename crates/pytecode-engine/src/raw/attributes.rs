use crate::raw::instructions::Instruction;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstantValueAttribute {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
    pub constantvalue_index: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureAttribute {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
    pub signature_index: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFileAttribute {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
    pub sourcefile_index: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceDebugExtensionAttribute {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
    pub debug_extension: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExceptionsAttribute {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
    pub exception_index_table: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExceptionHandler {
    pub start_pc: u16,
    pub end_pc: u16,
    pub handler_pc: u16,
    pub catch_type: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeAttribute {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
    pub max_stack: u16,
    pub max_locals: u16,
    pub code_length: u32,
    pub code: Vec<Instruction>,
    pub exception_table: Vec<ExceptionHandler>,
    pub attributes: Vec<AttributeInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownAttribute {
    pub attribute_name_index: u16,
    pub attribute_length: u32,
    pub name: String,
    pub info: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttributeInfo {
    ConstantValue(ConstantValueAttribute),
    Signature(SignatureAttribute),
    SourceFile(SourceFileAttribute),
    SourceDebugExtension(SourceDebugExtensionAttribute),
    Exceptions(ExceptionsAttribute),
    Code(CodeAttribute),
    Unknown(UnknownAttribute),
}

impl AttributeInfo {
    pub fn attribute_name_index(&self) -> u16 {
        match self {
            Self::ConstantValue(attr) => attr.attribute_name_index,
            Self::Signature(attr) => attr.attribute_name_index,
            Self::SourceFile(attr) => attr.attribute_name_index,
            Self::SourceDebugExtension(attr) => attr.attribute_name_index,
            Self::Exceptions(attr) => attr.attribute_name_index,
            Self::Code(attr) => attr.attribute_name_index,
            Self::Unknown(attr) => attr.attribute_name_index,
        }
    }
}
