use super::{AnalysisError, ClassResolver, build_cfg, recompute_frames};
use crate::constants::{ClassAccessFlags, MAGIC, MethodAccessFlags};
use crate::descriptors::{is_valid_field_descriptor, is_valid_method_descriptor};
use crate::model::{ClassModel, CodeModel};
use crate::raw::{AttributeInfo, ClassFile, ConstantPoolEntry};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Magic,
    Version,
    ConstantPool,
    AccessFlags,
    ClassStructure,
    Field,
    Method,
    Code,
    Attribute,
    Descriptor,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Location {
    pub class_name: Option<String>,
    pub field_name: Option<String>,
    pub method_name: Option<String>,
    pub method_descriptor: Option<String>,
    pub attribute_name: Option<String>,
    pub cp_index: Option<u16>,
    pub code_index: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub category: Category,
    pub message: String,
    pub location: Location,
}

impl Diagnostic {
    fn error(category: Category, message: impl Into<String>, location: Location) -> Self {
        Self {
            severity: Severity::Error,
            category,
            message: message.into(),
            location,
        }
    }
}

pub fn verify_classfile(classfile: &ClassFile) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let class_name = cp_class_name(classfile, classfile.this_class).ok();
    let class_location = Location {
        class_name: class_name.clone(),
        ..Location::default()
    };

    if classfile.magic != MAGIC {
        diagnostics.push(Diagnostic::error(
            Category::Magic,
            format!("invalid magic 0x{:08x}", classfile.magic),
            class_location.clone(),
        ));
    }
    if classfile.major_version < 45
        || (classfile.major_version >= 56
            && classfile.minor_version != 0
            && classfile.minor_version != u16::MAX)
    {
        diagnostics.push(Diagnostic::error(
            Category::Version,
            format!(
                "invalid class version {}/{}",
                classfile.major_version, classfile.minor_version
            ),
            class_location.clone(),
        ));
    }
    if classfile.this_class == 0 {
        diagnostics.push(Diagnostic::error(
            Category::ClassStructure,
            "this_class must be non-zero",
            class_location.clone(),
        ));
    }
    if classfile
        .constant_pool
        .iter()
        .enumerate()
        .skip(1)
        .any(|(_, entry)| matches!(entry, Some(ConstantPoolEntry::Utf8(_))))
        && classfile.constant_pool.is_empty()
    {
        diagnostics.push(Diagnostic::error(
            Category::ConstantPool,
            "constant pool must not be empty",
            class_location.clone(),
        ));
    }
    for (index, entry) in classfile.constant_pool.iter().enumerate().skip(1) {
        if let Some(ConstantPoolEntry::Class(info)) = entry
            && cp_utf8(classfile, info.name_index).is_err()
        {
            diagnostics.push(Diagnostic::error(
                Category::ConstantPool,
                "class entry references invalid Utf8 name",
                Location {
                    class_name: class_name.clone(),
                    cp_index: Some(index as u16),
                    ..Location::default()
                },
            ));
        }
    }
    for field in &classfile.fields {
        let name = cp_utf8(classfile, field.name_index).ok();
        let descriptor = cp_utf8(classfile, field.descriptor_index).ok();
        let location = Location {
            class_name: class_name.clone(),
            field_name: name.clone(),
            ..Location::default()
        };
        if descriptor
            .as_deref()
            .is_none_or(|descriptor| !is_valid_field_descriptor(descriptor))
        {
            diagnostics.push(Diagnostic::error(
                Category::Descriptor,
                "invalid field descriptor",
                location,
            ));
        }
    }
    for method in &classfile.methods {
        let name = cp_utf8(classfile, method.name_index).ok();
        let descriptor = cp_utf8(classfile, method.descriptor_index).ok();
        let location = Location {
            class_name: class_name.clone(),
            method_name: name.clone(),
            method_descriptor: descriptor.clone(),
            ..Location::default()
        };
        if descriptor
            .as_deref()
            .is_none_or(|descriptor| !is_valid_method_descriptor(descriptor))
        {
            diagnostics.push(Diagnostic::error(
                Category::Descriptor,
                "invalid method descriptor",
                location.clone(),
            ));
        }
        if method.access_flags.contains(MethodAccessFlags::ABSTRACT)
            && method
                .attributes
                .iter()
                .any(|attribute| matches!(attribute, AttributeInfo::Code(_)))
        {
            diagnostics.push(Diagnostic::error(
                Category::Method,
                "abstract method must not carry Code",
                location.clone(),
            ));
        }
        if let Some(code) = method
            .attributes
            .iter()
            .find_map(|attribute| match attribute {
                AttributeInfo::Code(code) => Some(code),
                _ => None,
            })
        {
            for handler in &code.exception_table {
                if handler.start_pc >= handler.end_pc || handler.end_pc > code.code_length as u16 {
                    diagnostics.push(Diagnostic::error(
                        Category::Code,
                        "exception handler range is invalid",
                        location.clone(),
                    ));
                }
            }
        }
    }
    diagnostics
}

pub fn verify_classmodel(
    model: &ClassModel,
    resolver: Option<&dyn ClassResolver>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let class_location = Location {
        class_name: Some(model.name.clone()),
        ..Location::default()
    };
    if model.name.is_empty() {
        diagnostics.push(Diagnostic::error(
            Category::ClassStructure,
            "class name must not be empty",
            class_location.clone(),
        ));
    }
    if model.access_flags.contains(ClassAccessFlags::INTERFACE)
        && !model.access_flags.contains(ClassAccessFlags::ABSTRACT)
    {
        diagnostics.push(Diagnostic::error(
            Category::AccessFlags,
            "interface must also be abstract",
            class_location.clone(),
        ));
    }
    for field in &model.fields {
        if !is_valid_field_descriptor(&field.descriptor) {
            diagnostics.push(Diagnostic::error(
                Category::Descriptor,
                "invalid field descriptor",
                Location {
                    class_name: Some(model.name.clone()),
                    field_name: Some(field.name.clone()),
                    ..Location::default()
                },
            ));
        }
    }
    for method in &model.methods {
        let location = Location {
            class_name: Some(model.name.clone()),
            method_name: Some(method.name.clone()),
            method_descriptor: Some(method.descriptor.clone()),
            ..Location::default()
        };
        if !is_valid_method_descriptor(&method.descriptor) {
            diagnostics.push(Diagnostic::error(
                Category::Descriptor,
                "invalid method descriptor",
                location.clone(),
            ));
        }
        if method.access_flags.contains(MethodAccessFlags::ABSTRACT) && method.code.is_some() {
            diagnostics.push(Diagnostic::error(
                Category::Method,
                "abstract method must not carry code",
                location.clone(),
            ));
        }
        if let Some(code) = &method.code {
            diagnostics.extend(verify_code_model(code, model, method, resolver));
        }
    }
    diagnostics
}

fn verify_code_model(
    code: &CodeModel,
    model: &ClassModel,
    method: &crate::model::MethodModel,
    resolver: Option<&dyn ClassResolver>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let location = Location {
        class_name: Some(model.name.clone()),
        method_name: Some(method.name.clone()),
        method_descriptor: Some(method.descriptor.clone()),
        ..Location::default()
    };
    if build_cfg(code).is_err() {
        diagnostics.push(Diagnostic::error(
            Category::Code,
            "code contains unresolved control-flow labels",
            location.clone(),
        ));
        return diagnostics;
    }
    if let Err(error) = recompute_frames(
        code,
        &model.name,
        &method.name,
        &method.descriptor,
        method.access_flags,
        resolver,
    ) {
        diagnostics.push(Diagnostic::error(
            Category::Code,
            analysis_error_message(&error),
            location,
        ));
    }
    diagnostics
}

fn analysis_error_message(error: &AnalysisError) -> String {
    error.to_string()
}

fn cp_utf8(classfile: &ClassFile, index: u16) -> Result<String, ()> {
    let entry = classfile
        .constant_pool
        .get(index as usize)
        .and_then(Option::as_ref)
        .ok_or(())?;
    match entry {
        ConstantPoolEntry::Utf8(info) => {
            crate::modified_utf8::decode_modified_utf8(&info.bytes).map_err(|_| ())
        }
        _ => Err(()),
    }
}

fn cp_class_name(classfile: &ClassFile, index: u16) -> Result<String, ()> {
    let entry = classfile
        .constant_pool
        .get(index as usize)
        .and_then(Option::as_ref)
        .ok_or(())?;
    match entry {
        ConstantPoolEntry::Class(info) => cp_utf8(classfile, info.name_index),
        _ => Err(()),
    }
}
