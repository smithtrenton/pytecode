use super::{AnalysisError, ClassResolver, build_cfg, recompute_frames};
use crate::constants::{
    ClassAccessFlags, FieldAccessFlags, MAGIC, MethodAccessFlags,
    class_version_supported_by_java_se_25,
};
use crate::descriptors::{is_valid_field_descriptor, is_valid_method_descriptor};
use crate::error::EngineErrorKind;
use crate::model::{ClassModel, CodeModel};
use crate::raw::{AttributeInfo, ClassFile, ConstantPoolEntry};
use crate::signatures::{parse_class_signature, parse_field_signature, parse_method_signature};
use std::collections::BTreeMap;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AttributeOwner {
    Class,
    Field,
    Method,
    Code,
    RecordComponent,
}

impl AttributeOwner {
    const fn placement_name(self) -> &'static str {
        match self {
            Self::Class => "classes",
            Self::Field => "fields",
            Self::Method => "methods",
            Self::Code => "code attributes",
            Self::RecordComponent => "record components",
        }
    }

    const fn duplicate_subject(self) -> &'static str {
        match self {
            Self::Class => "class",
            Self::Field => "field",
            Self::Method => "method",
            Self::Code => "Code attribute",
            Self::RecordComponent => "record component",
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
    if !class_version_supported_by_java_se_25(classfile.major_version, classfile.minor_version) {
        diagnostics.push(Diagnostic::error(
            Category::Version,
            format!(
                "invalid class version {}/{}",
                classfile.major_version, classfile.minor_version
            ),
            class_location.clone(),
        ));
    }
    diagnostics.extend(verify_constant_pool(classfile, class_name.as_deref()));
    diagnostics.extend(verify_class_structure(classfile, class_name.as_deref()));
    diagnostics.extend(verify_class_access_flags(classfile, class_location.clone()));
    diagnostics.extend(verify_class_attributes(
        classfile,
        class_name.as_deref(),
        class_location.clone(),
    ));
    if classfile.this_class == 0 {
        diagnostics.push(Diagnostic::error(
            Category::ClassStructure,
            "this_class must be non-zero",
            class_location.clone(),
        ));
    }
    for field in &classfile.fields {
        let name = cp_utf8(classfile, field.name_index).ok();
        let descriptor = cp_utf8(classfile, field.descriptor_index).ok();
        let location = Location {
            class_name: class_name.clone(),
            field_name: name.clone(),
            ..Location::default()
        };
        if name.is_none() {
            diagnostics.push(Diagnostic::error(
                Category::ConstantPool,
                "field name_index must reference Utf8",
                location.clone(),
            ));
        }
        if descriptor
            .as_deref()
            .is_none_or(|descriptor| !is_valid_field_descriptor(descriptor))
        {
            diagnostics.push(Diagnostic::error(
                Category::Descriptor,
                "invalid field descriptor",
                location.clone(),
            ));
        }
        if field_visibility_count(field.access_flags) > 1 {
            diagnostics.push(Diagnostic::error(
                Category::AccessFlags,
                "field must not be both public/private/protected",
                location.clone(),
            ));
        }
        if field.access_flags.contains(FieldAccessFlags::FINAL)
            && field.access_flags.contains(FieldAccessFlags::VOLATILE)
        {
            diagnostics.push(Diagnostic::error(
                Category::AccessFlags,
                "field must not be both final and volatile",
                location.clone(),
            ));
        }
        diagnostics.extend(verify_attributes(
            classfile,
            &field.attributes,
            AttributeOwner::Field,
            &location,
        ));
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
        if name.is_none() {
            diagnostics.push(Diagnostic::error(
                Category::ConstantPool,
                "method name_index must reference Utf8",
                location.clone(),
            ));
        }
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
        if method_visibility_count(method.access_flags) > 1 {
            diagnostics.push(Diagnostic::error(
                Category::AccessFlags,
                "method must not be both public/private/protected",
                location.clone(),
            ));
        }
        diagnostics.extend(verify_attributes(
            classfile,
            &method.attributes,
            AttributeOwner::Method,
            &location,
        ));
        let code_attributes = method
            .attributes
            .iter()
            .filter_map(|attribute| match attribute {
                AttributeInfo::Code(code) => Some(code),
                _ => None,
            })
            .collect::<Vec<_>>();
        if method.access_flags.contains(MethodAccessFlags::ABSTRACT) && !code_attributes.is_empty()
        {
            diagnostics.push(Diagnostic::error(
                Category::Method,
                "abstract method must not carry Code",
                location.clone(),
            ));
        }
        if method.access_flags.contains(MethodAccessFlags::NATIVE) && !code_attributes.is_empty() {
            diagnostics.push(Diagnostic::error(
                Category::Method,
                "native method must not carry Code",
                location.clone(),
            ));
        }
        if !method.access_flags.contains(MethodAccessFlags::ABSTRACT)
            && !method.access_flags.contains(MethodAccessFlags::NATIVE)
            && code_attributes.is_empty()
        {
            diagnostics.push(Diagnostic::error(
                Category::Method,
                "concrete method must carry Code",
                location.clone(),
            ));
        }
        if let Some(code) = code_attributes.first() {
            diagnostics.extend(verify_attributes(
                classfile,
                &code.attributes,
                AttributeOwner::Code,
                &location,
            ));
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

fn verify_constant_pool(classfile: &ClassFile, class_name: Option<&str>) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    if classfile.constant_pool.is_empty() {
        diagnostics.push(Diagnostic::error(
            Category::ConstantPool,
            "constant pool must include slot 0",
            Location {
                class_name: class_name.map(str::to_owned),
                ..Location::default()
            },
        ));
        return diagnostics;
    }
    if classfile.constant_pool[0].is_some() {
        diagnostics.push(Diagnostic::error(
            Category::ConstantPool,
            "constant pool slot 0 must be empty",
            Location {
                class_name: class_name.map(str::to_owned),
                cp_index: Some(0),
                ..Location::default()
            },
        ));
    }

    let mut expect_gap = false;
    for (index, entry) in classfile.constant_pool.iter().enumerate().skip(1) {
        let location = Location {
            class_name: class_name.map(str::to_owned),
            cp_index: Some(index as u16),
            ..Location::default()
        };
        if expect_gap {
            if entry.is_some() {
                diagnostics.push(Diagnostic::error(
                    Category::ConstantPool,
                    "long/double constant-pool entry must be followed by an empty gap slot",
                    location,
                ));
            }
            expect_gap = false;
            continue;
        }
        let Some(entry) = entry else {
            diagnostics.push(Diagnostic::error(
                Category::ConstantPool,
                "unexpected empty constant-pool slot",
                location,
            ));
            continue;
        };
        expect_gap = entry.is_wide();
        match entry {
            ConstantPoolEntry::Class(info) => {
                if cp_utf8(classfile, info.name_index).is_err() {
                    diagnostics.push(Diagnostic::error(
                        Category::ConstantPool,
                        "class entry references invalid Utf8 name",
                        location,
                    ));
                }
            }
            ConstantPoolEntry::String(info) => {
                if cp_utf8(classfile, info.string_index).is_err() {
                    diagnostics.push(Diagnostic::error(
                        Category::ConstantPool,
                        "string entry references invalid Utf8 payload",
                        location,
                    ));
                }
            }
            ConstantPoolEntry::FieldRef(info) => {
                if cp_class_name(classfile, info.class_index).is_err() {
                    diagnostics.push(Diagnostic::error(
                        Category::ConstantPool,
                        "member reference has invalid class_index",
                        location.clone(),
                    ));
                }
                if cp_name_and_type(classfile, info.name_and_type_index).is_err() {
                    diagnostics.push(Diagnostic::error(
                        Category::ConstantPool,
                        "member reference has invalid name_and_type_index",
                        location.clone(),
                    ));
                } else if cp_name_and_type(classfile, info.name_and_type_index)
                    .is_ok_and(|(_, descriptor)| !is_valid_field_descriptor(&descriptor))
                {
                    diagnostics.push(Diagnostic::error(
                        Category::ConstantPool,
                        "field reference descriptor must be a field descriptor",
                        location,
                    ));
                }
            }
            ConstantPoolEntry::MethodRef(info) => {
                if cp_class_name(classfile, info.class_index).is_err() {
                    diagnostics.push(Diagnostic::error(
                        Category::ConstantPool,
                        "member reference has invalid class_index",
                        location.clone(),
                    ));
                }
                if cp_name_and_type(classfile, info.name_and_type_index).is_err() {
                    diagnostics.push(Diagnostic::error(
                        Category::ConstantPool,
                        "member reference has invalid name_and_type_index",
                        location.clone(),
                    ));
                } else if cp_name_and_type(classfile, info.name_and_type_index)
                    .is_ok_and(|(_, descriptor)| !is_valid_method_descriptor(&descriptor))
                {
                    diagnostics.push(Diagnostic::error(
                        Category::ConstantPool,
                        "method reference descriptor must be a method descriptor",
                        location,
                    ));
                }
            }
            ConstantPoolEntry::InterfaceMethodRef(info) => {
                if cp_class_name(classfile, info.class_index).is_err() {
                    diagnostics.push(Diagnostic::error(
                        Category::ConstantPool,
                        "member reference has invalid class_index",
                        location.clone(),
                    ));
                }
                if cp_name_and_type(classfile, info.name_and_type_index).is_err() {
                    diagnostics.push(Diagnostic::error(
                        Category::ConstantPool,
                        "member reference has invalid name_and_type_index",
                        location.clone(),
                    ));
                } else if cp_name_and_type(classfile, info.name_and_type_index)
                    .is_ok_and(|(_, descriptor)| !is_valid_method_descriptor(&descriptor))
                {
                    diagnostics.push(Diagnostic::error(
                        Category::ConstantPool,
                        "interface method reference descriptor must be a method descriptor",
                        location,
                    ));
                }
            }
            ConstantPoolEntry::NameAndType(info) => {
                if cp_utf8(classfile, info.name_index).is_err() {
                    diagnostics.push(Diagnostic::error(
                        Category::ConstantPool,
                        "name_and_type entry references invalid Utf8 name",
                        location.clone(),
                    ));
                }
                if cp_utf8(classfile, info.descriptor_index).is_err() {
                    diagnostics.push(Diagnostic::error(
                        Category::ConstantPool,
                        "name_and_type entry references invalid Utf8 descriptor",
                        location,
                    ));
                }
            }
            ConstantPoolEntry::MethodType(info) => {
                if cp_utf8(classfile, info.descriptor_index).is_err() {
                    diagnostics.push(Diagnostic::error(
                        Category::ConstantPool,
                        "method type entry references invalid Utf8 descriptor",
                        location.clone(),
                    ));
                } else if cp_utf8(classfile, info.descriptor_index)
                    .is_ok_and(|descriptor| !is_valid_method_descriptor(&descriptor))
                {
                    diagnostics.push(Diagnostic::error(
                        Category::ConstantPool,
                        "method type descriptor must be a method descriptor",
                        location,
                    ));
                }
            }
            ConstantPoolEntry::MethodHandle(info) => {
                diagnostics.extend(verify_method_handle(classfile, info, &location));
            }
            ConstantPoolEntry::Dynamic(info) => {
                if cp_name_and_type(classfile, info.name_and_type_index).is_err() {
                    diagnostics.push(Diagnostic::error(
                        Category::ConstantPool,
                        "dynamic entry has invalid name_and_type_index",
                        location.clone(),
                    ));
                } else if cp_name_and_type(classfile, info.name_and_type_index)
                    .is_ok_and(|(_, descriptor)| !is_valid_field_descriptor(&descriptor))
                {
                    diagnostics.push(Diagnostic::error(
                        Category::ConstantPool,
                        "dynamic entry descriptor must be a field descriptor",
                        location.clone(),
                    ));
                }
                if !bootstrap_method_index_valid(classfile, info.bootstrap_method_attr_index) {
                    diagnostics.push(Diagnostic::error(
                        Category::ConstantPool,
                        "dynamic entry bootstrap_method_attr_index is out of range",
                        location,
                    ));
                }
            }
            ConstantPoolEntry::InvokeDynamic(info) => {
                if cp_name_and_type(classfile, info.name_and_type_index).is_err() {
                    diagnostics.push(Diagnostic::error(
                        Category::ConstantPool,
                        "dynamic entry has invalid name_and_type_index",
                        location.clone(),
                    ));
                } else if cp_name_and_type(classfile, info.name_and_type_index)
                    .is_ok_and(|(_, descriptor)| !is_valid_method_descriptor(&descriptor))
                {
                    diagnostics.push(Diagnostic::error(
                        Category::ConstantPool,
                        "invokedynamic entry descriptor must be a method descriptor",
                        location.clone(),
                    ));
                }
                if !bootstrap_method_index_valid(classfile, info.bootstrap_method_attr_index) {
                    diagnostics.push(Diagnostic::error(
                        Category::ConstantPool,
                        "invokedynamic entry bootstrap_method_attr_index is out of range",
                        location,
                    ));
                }
            }
            ConstantPoolEntry::Module(info) => {
                if cp_utf8(classfile, info.name_index).is_err() {
                    diagnostics.push(Diagnostic::error(
                        Category::ConstantPool,
                        "module/package entry references invalid Utf8 name",
                        location,
                    ));
                }
            }
            ConstantPoolEntry::Package(info) => {
                if cp_utf8(classfile, info.name_index).is_err() {
                    diagnostics.push(Diagnostic::error(
                        Category::ConstantPool,
                        "module/package entry references invalid Utf8 name",
                        location,
                    ));
                }
            }
            _ => {}
        }
    }
    if expect_gap {
        diagnostics.push(Diagnostic::error(
            Category::ConstantPool,
            "long/double constant-pool entry is missing its trailing gap slot",
            Location {
                class_name: class_name.map(str::to_owned),
                ..Location::default()
            },
        ));
    }
    diagnostics
}

fn verify_class_structure(classfile: &ClassFile, class_name: Option<&str>) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let location = Location {
        class_name: class_name.map(str::to_owned),
        ..Location::default()
    };
    let is_module = classfile.access_flags.contains(ClassAccessFlags::MODULE);

    if classfile.this_class != 0 && cp_class_name(classfile, classfile.this_class).is_err() {
        diagnostics.push(Diagnostic::error(
            Category::ClassStructure,
            "this_class must reference CONSTANT_Class",
            location.clone(),
        ));
    }

    if is_module {
        if classfile.super_class != 0 {
            diagnostics.push(Diagnostic::error(
                Category::ClassStructure,
                "module class must have super_class == 0",
                location.clone(),
            ));
        }
        if !classfile.interfaces.is_empty() {
            diagnostics.push(Diagnostic::error(
                Category::ClassStructure,
                "module class must not declare interfaces",
                location.clone(),
            ));
        }
        if !classfile.fields.is_empty() {
            diagnostics.push(Diagnostic::error(
                Category::ClassStructure,
                "module class must not declare fields",
                location.clone(),
            ));
        }
        if !classfile.methods.is_empty() {
            diagnostics.push(Diagnostic::error(
                Category::ClassStructure,
                "module class must not declare methods",
                location.clone(),
            ));
        }
    } else if class_name == Some("java/lang/Object") {
        if classfile.super_class != 0 {
            diagnostics.push(Diagnostic::error(
                Category::ClassStructure,
                "java/lang/Object must have super_class == 0",
                location.clone(),
            ));
        }
    } else if classfile.super_class == 0 {
        diagnostics.push(Diagnostic::error(
            Category::ClassStructure,
            "non-root class must have non-zero super_class",
            location.clone(),
        ));
    } else if cp_class_name(classfile, classfile.super_class).is_err() {
        diagnostics.push(Diagnostic::error(
            Category::ClassStructure,
            "super_class must reference CONSTANT_Class",
            location.clone(),
        ));
    }

    for interface in &classfile.interfaces {
        if cp_class_name(classfile, *interface).is_err() {
            diagnostics.push(Diagnostic::error(
                Category::ClassStructure,
                "interface entry must reference CONSTANT_Class",
                location.clone(),
            ));
        }
    }
    diagnostics
}

fn verify_class_access_flags(classfile: &ClassFile, location: Location) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let flags = classfile.access_flags;
    if flags.contains(ClassAccessFlags::INTERFACE) && !flags.contains(ClassAccessFlags::ABSTRACT) {
        diagnostics.push(Diagnostic::error(
            Category::AccessFlags,
            "interface must also be abstract",
            location.clone(),
        ));
    }
    if flags.contains(ClassAccessFlags::ANNOTATION) && !flags.contains(ClassAccessFlags::INTERFACE)
    {
        diagnostics.push(Diagnostic::error(
            Category::AccessFlags,
            "annotation must also be an interface",
            location.clone(),
        ));
    }
    if flags.contains(ClassAccessFlags::FINAL) && flags.contains(ClassAccessFlags::ABSTRACT) {
        diagnostics.push(Diagnostic::error(
            Category::AccessFlags,
            "class must not be both final and abstract",
            location,
        ));
    }
    diagnostics
}

fn verify_class_attributes(
    classfile: &ClassFile,
    class_name: Option<&str>,
    base_location: Location,
) -> Vec<Diagnostic> {
    let mut diagnostics = verify_attributes(
        classfile,
        &classfile.attributes,
        AttributeOwner::Class,
        &base_location,
    );

    for attribute in &classfile.attributes {
        if let AttributeInfo::Record(record) = attribute {
            diagnostics.extend(verify_record_components(classfile, record, class_name));
        }
    }

    diagnostics
}

fn verify_record_components(
    classfile: &ClassFile,
    record: &crate::raw::RecordAttribute,
    class_name: Option<&str>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for component in &record.components {
        let name = cp_utf8(classfile, component.name_index).ok();
        let descriptor = cp_utf8(classfile, component.descriptor_index).ok();
        let location = Location {
            class_name: class_name.map(str::to_owned),
            field_name: name.clone(),
            attribute_name: Some("Record".to_owned()),
            ..Location::default()
        };

        if name.is_none() {
            diagnostics.push(Diagnostic::error(
                Category::ConstantPool,
                "record component name_index must reference Utf8",
                location.clone(),
            ));
        }
        if descriptor
            .as_deref()
            .is_none_or(|descriptor| !is_valid_field_descriptor(descriptor))
        {
            diagnostics.push(Diagnostic::error(
                Category::Descriptor,
                "invalid record component descriptor",
                location.clone(),
            ));
        }
        diagnostics.extend(verify_attributes(
            classfile,
            &component.attributes,
            AttributeOwner::RecordComponent,
            &location,
        ));
    }
    diagnostics
}

fn verify_attributes(
    classfile: &ClassFile,
    attributes: &[AttributeInfo],
    owner: AttributeOwner,
    base_location: &Location,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut unique_counts = BTreeMap::<String, usize>::new();

    for attribute in attributes {
        let attribute_name = attribute_name(attribute).to_owned();
        let mut location = base_location.clone();
        location.attribute_name = Some(attribute_name.clone());

        if cp_utf8(classfile, attribute.attribute_name_index()).is_err() {
            diagnostics.push(Diagnostic::error(
                Category::ConstantPool,
                "attribute_name_index must reference Utf8",
                location.clone(),
            ));
        }
        if !attribute_allowed_on_owner(attribute, owner) {
            diagnostics.push(Diagnostic::error(
                Category::Attribute,
                format!(
                    "{attribute_name} attribute is not allowed on {}",
                    owner.placement_name()
                ),
                location.clone(),
            ));
        }
        if attribute_requires_module_class(attribute)
            && !classfile.access_flags.contains(ClassAccessFlags::MODULE)
        {
            diagnostics.push(Diagnostic::error(
                Category::Attribute,
                format!("{attribute_name} attribute is only allowed on module classes"),
                location.clone(),
            ));
        }
        if let Some(min_major) = attribute_minimum_major(attribute)
            && classfile.major_version < min_major
        {
            diagnostics.push(Diagnostic::error(
                Category::Attribute,
                format!(
                    "{attribute_name} attribute requires class file version {min_major}.0 or newer"
                ),
                location.clone(),
            ));
        }
        if attribute_must_be_unique(attribute) {
            *unique_counts.entry(attribute_name.clone()).or_default() += 1;
        }
        diagnostics.extend(verify_attribute_contents(
            classfile, attribute, owner, &location,
        ));
    }

    for (attribute_name, count) in unique_counts {
        if count > 1 {
            let mut location = base_location.clone();
            location.attribute_name = Some(attribute_name.clone());
            diagnostics.push(Diagnostic::error(
                Category::Attribute,
                format!(
                    "{} must not declare multiple {attribute_name} attributes",
                    owner.duplicate_subject()
                ),
                location,
            ));
        }
    }

    if owner == AttributeOwner::Class && classfile.access_flags.contains(ClassAccessFlags::MODULE) {
        let module_count = attributes
            .iter()
            .filter(|attribute| matches!(attribute, AttributeInfo::Module(_)))
            .count();
        if module_count != 1 {
            let mut location = base_location.clone();
            location.attribute_name = Some("Module".to_owned());
            diagnostics.push(Diagnostic::error(
                Category::Attribute,
                "module class must declare exactly one Module attribute",
                location,
            ));
        }
    }

    diagnostics
}

fn verify_attribute_contents(
    classfile: &ClassFile,
    attribute: &AttributeInfo,
    owner: AttributeOwner,
    location: &Location,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    match attribute {
        AttributeInfo::Signature(attribute) => {
            let signature = cp_utf8(classfile, attribute.signature_index);
            if signature.is_err() {
                diagnostics.push(Diagnostic::error(
                    Category::ConstantPool,
                    "Signature attribute must reference a Utf8 signature",
                    location.clone(),
                ));
            } else if let Some(message) =
                validate_signature_attribute(owner, signature.as_deref().unwrap_or_default())
            {
                diagnostics.push(Diagnostic::error(
                    Category::Attribute,
                    message,
                    location.clone(),
                ));
            }
        }
        AttributeInfo::SourceFile(attribute) => {
            if cp_utf8(classfile, attribute.sourcefile_index).is_err() {
                diagnostics.push(Diagnostic::error(
                    Category::ConstantPool,
                    "SourceFile attribute must reference a Utf8 source file name",
                    location.clone(),
                ));
            }
        }
        AttributeInfo::Exceptions(attribute) => {
            for exception_index in &attribute.exception_index_table {
                if cp_class_name(classfile, *exception_index).is_err() {
                    diagnostics.push(Diagnostic::error(
                        Category::ConstantPool,
                        "Exceptions attribute entries must reference CONSTANT_Class",
                        location.clone(),
                    ));
                }
            }
        }
        AttributeInfo::EnclosingMethod(attribute) => {
            if cp_class_name(classfile, attribute.class_index).is_err() {
                diagnostics.push(Diagnostic::error(
                    Category::ConstantPool,
                    "EnclosingMethod attribute class_index must reference CONSTANT_Class",
                    location.clone(),
                ));
            }
            if attribute.method_index != 0
                && cp_name_and_type(classfile, attribute.method_index).is_err()
            {
                diagnostics.push(Diagnostic::error(
                    Category::ConstantPool,
                    "EnclosingMethod attribute method_index must reference CONSTANT_NameAndType or be zero",
                    location.clone(),
                ));
            }
        }
        AttributeInfo::MethodParameters(attribute) => {
            for parameter in &attribute.parameters {
                if parameter.name_index != 0 && cp_utf8(classfile, parameter.name_index).is_err() {
                    diagnostics.push(Diagnostic::error(
                        Category::ConstantPool,
                        "MethodParameters names must reference Utf8 or be zero",
                        location.clone(),
                    ));
                }
            }
        }
        AttributeInfo::BootstrapMethods(attribute) => {
            for bootstrap_method in &attribute.bootstrap_methods {
                if !matches!(
                    cp_entry(classfile, bootstrap_method.bootstrap_method_ref),
                    Ok(ConstantPoolEntry::MethodHandle(_))
                ) {
                    diagnostics.push(Diagnostic::error(
                        Category::ConstantPool,
                        "BootstrapMethods bootstrap_method_ref must reference CONSTANT_MethodHandle",
                        location.clone(),
                    ));
                }
                for argument in &bootstrap_method.bootstrap_arguments {
                    if !bootstrap_argument_entry_valid(classfile, *argument) {
                        diagnostics.push(Diagnostic::error(
                            Category::ConstantPool,
                            "BootstrapMethods arguments must reference loadable constants",
                            location.clone(),
                        ));
                    }
                }
            }
        }
        AttributeInfo::LocalVariableTypeTable(attribute) => {
            for entry in &attribute.local_variable_type_table {
                if cp_utf8(classfile, entry.name_index).is_err() {
                    diagnostics.push(Diagnostic::error(
                        Category::ConstantPool,
                        "LocalVariableTypeTable name_index must reference Utf8",
                        location.clone(),
                    ));
                }
                let signature = cp_utf8(classfile, entry.signature_index);
                if signature.is_err() {
                    diagnostics.push(Diagnostic::error(
                        Category::ConstantPool,
                        "LocalVariableTypeTable signature_index must reference Utf8",
                        location.clone(),
                    ));
                } else if let Err(error) =
                    parse_field_signature(signature.as_deref().unwrap_or_default())
                {
                    diagnostics.push(Diagnostic::error(
                        Category::Attribute,
                        format!(
                            "invalid local variable type signature: {}",
                            signature_reason(&error.kind)
                        ),
                        location.clone(),
                    ));
                }
            }
        }
        AttributeInfo::NestHost(attribute) => {
            if cp_class_name(classfile, attribute.host_class_index).is_err() {
                diagnostics.push(Diagnostic::error(
                    Category::ConstantPool,
                    "NestHost attribute must reference CONSTANT_Class",
                    location.clone(),
                ));
            }
        }
        AttributeInfo::NestMembers(attribute) => {
            for class_index in &attribute.classes {
                if cp_class_name(classfile, *class_index).is_err() {
                    diagnostics.push(Diagnostic::error(
                        Category::ConstantPool,
                        "NestMembers entries must reference CONSTANT_Class",
                        location.clone(),
                    ));
                }
            }
        }
        AttributeInfo::Module(attribute) => {
            if cp_module_name(classfile, attribute.module.module_name_index).is_err() {
                diagnostics.push(Diagnostic::error(
                    Category::ConstantPool,
                    "Module attribute must reference CONSTANT_Module for module_name_index",
                    location.clone(),
                ));
            }
            if attribute.module.module_version_index != 0
                && cp_utf8(classfile, attribute.module.module_version_index).is_err()
            {
                diagnostics.push(Diagnostic::error(
                    Category::ConstantPool,
                    "Module attribute module_version_index must reference Utf8 or be zero",
                    location.clone(),
                ));
            }
            for requires in &attribute.module.requires {
                if cp_module_name(classfile, requires.requires_index).is_err() {
                    diagnostics.push(Diagnostic::error(
                        Category::ConstantPool,
                        "Module requires entries must reference CONSTANT_Module",
                        location.clone(),
                    ));
                }
                if requires.requires_version_index != 0
                    && cp_utf8(classfile, requires.requires_version_index).is_err()
                {
                    diagnostics.push(Diagnostic::error(
                        Category::ConstantPool,
                        "Module requires version indices must reference Utf8 or be zero",
                        location.clone(),
                    ));
                }
            }
            for exports in &attribute.module.exports {
                if cp_package_name(classfile, exports.exports_index).is_err() {
                    diagnostics.push(Diagnostic::error(
                        Category::ConstantPool,
                        "Module exports entries must reference CONSTANT_Package",
                        location.clone(),
                    ));
                }
                for target in &exports.exports_to_index {
                    if cp_module_name(classfile, *target).is_err() {
                        diagnostics.push(Diagnostic::error(
                            Category::ConstantPool,
                            "Module exports targets must reference CONSTANT_Module",
                            location.clone(),
                        ));
                    }
                }
            }
            for opens in &attribute.module.opens {
                if cp_package_name(classfile, opens.opens_index).is_err() {
                    diagnostics.push(Diagnostic::error(
                        Category::ConstantPool,
                        "Module opens entries must reference CONSTANT_Package",
                        location.clone(),
                    ));
                }
                for target in &opens.opens_to_index {
                    if cp_module_name(classfile, *target).is_err() {
                        diagnostics.push(Diagnostic::error(
                            Category::ConstantPool,
                            "Module opens targets must reference CONSTANT_Module",
                            location.clone(),
                        ));
                    }
                }
            }
            for class_index in &attribute.module.uses_index {
                if cp_class_name(classfile, *class_index).is_err() {
                    diagnostics.push(Diagnostic::error(
                        Category::ConstantPool,
                        "Module uses entries must reference CONSTANT_Class",
                        location.clone(),
                    ));
                }
            }
            for provides in &attribute.module.provides {
                if cp_class_name(classfile, provides.provides_index).is_err() {
                    diagnostics.push(Diagnostic::error(
                        Category::ConstantPool,
                        "Module provides entries must reference CONSTANT_Class",
                        location.clone(),
                    ));
                }
                for implementation in &provides.provides_with_index {
                    if cp_class_name(classfile, *implementation).is_err() {
                        diagnostics.push(Diagnostic::error(
                            Category::ConstantPool,
                            "Module provides targets must reference CONSTANT_Class",
                            location.clone(),
                        ));
                    }
                }
            }
        }
        AttributeInfo::ModulePackages(attribute) => {
            for package_index in &attribute.package_index {
                if cp_package_name(classfile, *package_index).is_err() {
                    diagnostics.push(Diagnostic::error(
                        Category::ConstantPool,
                        "ModulePackages entries must reference CONSTANT_Package",
                        location.clone(),
                    ));
                }
            }
        }
        AttributeInfo::ModuleMainClass(attribute) => {
            if cp_class_name(classfile, attribute.main_class_index).is_err() {
                diagnostics.push(Diagnostic::error(
                    Category::ConstantPool,
                    "ModuleMainClass attribute must reference CONSTANT_Class",
                    location.clone(),
                ));
            }
        }
        AttributeInfo::PermittedSubclasses(attribute) => {
            for class_index in &attribute.classes {
                if cp_class_name(classfile, *class_index).is_err() {
                    diagnostics.push(Diagnostic::error(
                        Category::ConstantPool,
                        "PermittedSubclasses entries must reference CONSTANT_Class",
                        location.clone(),
                    ));
                }
            }
        }
        _ => {}
    }
    diagnostics
}

fn validate_signature_attribute(owner: AttributeOwner, signature: &str) -> Option<String> {
    let result = match owner {
        AttributeOwner::Class => parse_class_signature(signature).map(|_| ()),
        AttributeOwner::Field | AttributeOwner::RecordComponent => {
            parse_field_signature(signature).map(|_| ())
        }
        AttributeOwner::Method => parse_method_signature(signature).map(|_| ()),
        AttributeOwner::Code => return None,
    };
    result.err().map(|error| {
        format!(
            "invalid {} signature: {}",
            owner_signature_name(owner),
            signature_reason(&error.kind)
        )
    })
}

fn owner_signature_name(owner: AttributeOwner) -> &'static str {
    match owner {
        AttributeOwner::Class => "class",
        AttributeOwner::Field => "field",
        AttributeOwner::Method => "method",
        AttributeOwner::Code => "code",
        AttributeOwner::RecordComponent => "record component",
    }
}

fn signature_reason(kind: &EngineErrorKind) -> String {
    match kind {
        EngineErrorKind::InvalidSignature { reason } => reason.clone(),
        _ => kind.to_string(),
    }
}

fn verify_method_handle(
    classfile: &ClassFile,
    info: &crate::raw::MethodHandleInfo,
    location: &Location,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    if !(1..=9).contains(&info.reference_kind) {
        diagnostics.push(Diagnostic::error(
            Category::ConstantPool,
            "method handle reference_kind must be in 1..=9",
            location.clone(),
        ));
        return diagnostics;
    }

    let target_entry = cp_entry(classfile, info.reference_index);
    let valid_target = match info.reference_kind {
        1..=4 => matches!(target_entry, Ok(ConstantPoolEntry::FieldRef(_))),
        5 | 8 => matches!(target_entry, Ok(ConstantPoolEntry::MethodRef(_))),
        6 | 7 => {
            matches!(
                target_entry,
                Ok(ConstantPoolEntry::MethodRef(_))
                    | Ok(ConstantPoolEntry::InterfaceMethodRef(_))
                        if classfile.major_version >= 52
            ) || matches!(target_entry, Ok(ConstantPoolEntry::MethodRef(_)))
        }
        9 => matches!(target_entry, Ok(ConstantPoolEntry::InterfaceMethodRef(_))),
        _ => false,
    };
    if !valid_target {
        diagnostics.push(Diagnostic::error(
            Category::ConstantPool,
            method_handle_target_message(info.reference_kind),
            location.clone(),
        ));
        return diagnostics;
    }

    if let Ok((name, _)) = cp_member_name_and_type(classfile, info.reference_index) {
        match info.reference_kind {
            8 if name != "<init>" => diagnostics.push(Diagnostic::error(
                Category::ConstantPool,
                "REF_newInvokeSpecial method handle must reference <init>",
                location.clone(),
            )),
            5 | 6 | 7 | 9 if matches!(name.as_str(), "<init>" | "<clinit>") => {
                diagnostics.push(Diagnostic::error(
                    Category::ConstantPool,
                    "method handles of kind REF_invokeVirtual/Static/Special/InterfaceSpecial must not reference <init> or <clinit>",
                    location.clone(),
                ))
            }
            _ => {}
        }
    }

    diagnostics
}

fn method_handle_target_message(reference_kind: u8) -> &'static str {
    match reference_kind {
        1..=4 => "field-access method handles must reference CONSTANT_Fieldref",
        5 => "REF_invokeVirtual method handle must reference CONSTANT_Methodref",
        6 | 7 => {
            "REF_invokeStatic and REF_invokeSpecial method handles must reference CONSTANT_Methodref or CONSTANT_InterfaceMethodref"
        }
        8 => "REF_newInvokeSpecial method handle must reference CONSTANT_Methodref",
        9 => "REF_invokeInterface method handle must reference CONSTANT_InterfaceMethodref",
        _ => "method handle reference_kind must be in 1..=9",
    }
}

fn attribute_name(attribute: &AttributeInfo) -> &str {
    match attribute {
        AttributeInfo::ConstantValue(_) => "ConstantValue",
        AttributeInfo::Signature(_) => "Signature",
        AttributeInfo::SourceFile(_) => "SourceFile",
        AttributeInfo::SourceDebugExtension(_) => "SourceDebugExtension",
        AttributeInfo::Synthetic(_) => "Synthetic",
        AttributeInfo::Deprecated(_) => "Deprecated",
        AttributeInfo::StackMapTable(_) => "StackMapTable",
        AttributeInfo::Exceptions(_) => "Exceptions",
        AttributeInfo::InnerClasses(_) => "InnerClasses",
        AttributeInfo::EnclosingMethod(_) => "EnclosingMethod",
        AttributeInfo::Code(_) => "Code",
        AttributeInfo::LineNumberTable(_) => "LineNumberTable",
        AttributeInfo::LocalVariableTable(_) => "LocalVariableTable",
        AttributeInfo::LocalVariableTypeTable(_) => "LocalVariableTypeTable",
        AttributeInfo::MethodParameters(_) => "MethodParameters",
        AttributeInfo::NestHost(_) => "NestHost",
        AttributeInfo::NestMembers(_) => "NestMembers",
        AttributeInfo::RuntimeVisibleAnnotations(_) => "RuntimeVisibleAnnotations",
        AttributeInfo::RuntimeInvisibleAnnotations(_) => "RuntimeInvisibleAnnotations",
        AttributeInfo::RuntimeVisibleParameterAnnotations(_) => {
            "RuntimeVisibleParameterAnnotations"
        }
        AttributeInfo::RuntimeInvisibleParameterAnnotations(_) => {
            "RuntimeInvisibleParameterAnnotations"
        }
        AttributeInfo::RuntimeVisibleTypeAnnotations(_) => "RuntimeVisibleTypeAnnotations",
        AttributeInfo::RuntimeInvisibleTypeAnnotations(_) => "RuntimeInvisibleTypeAnnotations",
        AttributeInfo::AnnotationDefault(_) => "AnnotationDefault",
        AttributeInfo::BootstrapMethods(_) => "BootstrapMethods",
        AttributeInfo::Module(_) => "Module",
        AttributeInfo::ModulePackages(_) => "ModulePackages",
        AttributeInfo::ModuleMainClass(_) => "ModuleMainClass",
        AttributeInfo::Record(_) => "Record",
        AttributeInfo::PermittedSubclasses(_) => "PermittedSubclasses",
        AttributeInfo::Unknown(attribute) => attribute.name.as_str(),
    }
}

fn attribute_allowed_on_owner(attribute: &AttributeInfo, owner: AttributeOwner) -> bool {
    match owner {
        AttributeOwner::Class => matches!(
            attribute,
            AttributeInfo::Signature(_)
                | AttributeInfo::SourceFile(_)
                | AttributeInfo::SourceDebugExtension(_)
                | AttributeInfo::Synthetic(_)
                | AttributeInfo::Deprecated(_)
                | AttributeInfo::InnerClasses(_)
                | AttributeInfo::EnclosingMethod(_)
                | AttributeInfo::NestHost(_)
                | AttributeInfo::NestMembers(_)
                | AttributeInfo::RuntimeVisibleAnnotations(_)
                | AttributeInfo::RuntimeInvisibleAnnotations(_)
                | AttributeInfo::RuntimeVisibleTypeAnnotations(_)
                | AttributeInfo::RuntimeInvisibleTypeAnnotations(_)
                | AttributeInfo::BootstrapMethods(_)
                | AttributeInfo::Module(_)
                | AttributeInfo::ModulePackages(_)
                | AttributeInfo::ModuleMainClass(_)
                | AttributeInfo::Record(_)
                | AttributeInfo::PermittedSubclasses(_)
                | AttributeInfo::Unknown(_)
        ),
        AttributeOwner::Field => matches!(
            attribute,
            AttributeInfo::ConstantValue(_)
                | AttributeInfo::Signature(_)
                | AttributeInfo::Synthetic(_)
                | AttributeInfo::Deprecated(_)
                | AttributeInfo::RuntimeVisibleAnnotations(_)
                | AttributeInfo::RuntimeInvisibleAnnotations(_)
                | AttributeInfo::RuntimeVisibleTypeAnnotations(_)
                | AttributeInfo::RuntimeInvisibleTypeAnnotations(_)
                | AttributeInfo::Unknown(_)
        ),
        AttributeOwner::Method => matches!(
            attribute,
            AttributeInfo::Signature(_)
                | AttributeInfo::Synthetic(_)
                | AttributeInfo::Deprecated(_)
                | AttributeInfo::Exceptions(_)
                | AttributeInfo::Code(_)
                | AttributeInfo::MethodParameters(_)
                | AttributeInfo::AnnotationDefault(_)
                | AttributeInfo::RuntimeVisibleAnnotations(_)
                | AttributeInfo::RuntimeInvisibleAnnotations(_)
                | AttributeInfo::RuntimeVisibleParameterAnnotations(_)
                | AttributeInfo::RuntimeInvisibleParameterAnnotations(_)
                | AttributeInfo::RuntimeVisibleTypeAnnotations(_)
                | AttributeInfo::RuntimeInvisibleTypeAnnotations(_)
                | AttributeInfo::Unknown(_)
        ),
        AttributeOwner::Code => matches!(
            attribute,
            AttributeInfo::StackMapTable(_)
                | AttributeInfo::LineNumberTable(_)
                | AttributeInfo::LocalVariableTable(_)
                | AttributeInfo::LocalVariableTypeTable(_)
                | AttributeInfo::RuntimeVisibleTypeAnnotations(_)
                | AttributeInfo::RuntimeInvisibleTypeAnnotations(_)
                | AttributeInfo::Unknown(_)
        ),
        AttributeOwner::RecordComponent => matches!(
            attribute,
            AttributeInfo::Signature(_)
                | AttributeInfo::RuntimeVisibleAnnotations(_)
                | AttributeInfo::RuntimeInvisibleAnnotations(_)
                | AttributeInfo::RuntimeVisibleTypeAnnotations(_)
                | AttributeInfo::RuntimeInvisibleTypeAnnotations(_)
                | AttributeInfo::Unknown(_)
        ),
    }
}

fn attribute_must_be_unique(attribute: &AttributeInfo) -> bool {
    !matches!(attribute, AttributeInfo::Unknown(_))
}

fn attribute_requires_module_class(attribute: &AttributeInfo) -> bool {
    matches!(
        attribute,
        AttributeInfo::Module(_)
            | AttributeInfo::ModulePackages(_)
            | AttributeInfo::ModuleMainClass(_)
    )
}

fn attribute_minimum_major(attribute: &AttributeInfo) -> Option<u16> {
    match attribute {
        AttributeInfo::StackMapTable(_) => Some(50),
        AttributeInfo::BootstrapMethods(_) => Some(51),
        AttributeInfo::MethodParameters(_) => Some(52),
        AttributeInfo::Module(_)
        | AttributeInfo::ModulePackages(_)
        | AttributeInfo::ModuleMainClass(_) => Some(53),
        AttributeInfo::NestHost(_) | AttributeInfo::NestMembers(_) => Some(55),
        AttributeInfo::Record(_) => Some(60),
        AttributeInfo::PermittedSubclasses(_) => Some(61),
        _ => None,
    }
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

fn cp_entry(classfile: &ClassFile, index: u16) -> Result<&ConstantPoolEntry, ()> {
    classfile
        .constant_pool
        .get(index as usize)
        .and_then(Option::as_ref)
        .ok_or(())
}

fn cp_class_name(classfile: &ClassFile, index: u16) -> Result<String, ()> {
    let entry = cp_entry(classfile, index)?;
    match entry {
        ConstantPoolEntry::Class(info) => cp_utf8(classfile, info.name_index),
        _ => Err(()),
    }
}

fn cp_module_name(classfile: &ClassFile, index: u16) -> Result<String, ()> {
    let entry = cp_entry(classfile, index)?;
    match entry {
        ConstantPoolEntry::Module(info) => cp_utf8(classfile, info.name_index),
        _ => Err(()),
    }
}

fn cp_package_name(classfile: &ClassFile, index: u16) -> Result<String, ()> {
    let entry = cp_entry(classfile, index)?;
    match entry {
        ConstantPoolEntry::Package(info) => cp_utf8(classfile, info.name_index),
        _ => Err(()),
    }
}

fn cp_name_and_type(classfile: &ClassFile, index: u16) -> Result<(String, String), ()> {
    let entry = cp_entry(classfile, index)?;
    match entry {
        ConstantPoolEntry::NameAndType(info) => Ok((
            cp_utf8(classfile, info.name_index)?,
            cp_utf8(classfile, info.descriptor_index)?,
        )),
        _ => Err(()),
    }
}

fn cp_member_name_and_type(classfile: &ClassFile, index: u16) -> Result<(String, String), ()> {
    match cp_entry(classfile, index)? {
        ConstantPoolEntry::FieldRef(info) => cp_name_and_type(classfile, info.name_and_type_index),
        ConstantPoolEntry::MethodRef(info) => cp_name_and_type(classfile, info.name_and_type_index),
        ConstantPoolEntry::InterfaceMethodRef(info) => {
            cp_name_and_type(classfile, info.name_and_type_index)
        }
        _ => Err(()),
    }
}

fn bootstrap_methods(classfile: &ClassFile) -> Option<&crate::raw::BootstrapMethodsAttribute> {
    classfile
        .attributes
        .iter()
        .find_map(|attribute| match attribute {
            AttributeInfo::BootstrapMethods(attribute) => Some(attribute),
            _ => None,
        })
}

fn bootstrap_method_index_valid(classfile: &ClassFile, index: u16) -> bool {
    bootstrap_methods(classfile)
        .is_some_and(|attribute| usize::from(index) < attribute.bootstrap_methods.len())
}

fn bootstrap_argument_entry_valid(classfile: &ClassFile, index: u16) -> bool {
    matches!(
        cp_entry(classfile, index),
        Ok(ConstantPoolEntry::Integer(_)
            | ConstantPoolEntry::Float(_)
            | ConstantPoolEntry::Long(_)
            | ConstantPoolEntry::Double(_)
            | ConstantPoolEntry::String(_)
            | ConstantPoolEntry::Class(_)
            | ConstantPoolEntry::MethodHandle(_)
            | ConstantPoolEntry::MethodType(_)
            | ConstantPoolEntry::Dynamic(_))
    )
}

fn field_visibility_count(flags: FieldAccessFlags) -> usize {
    usize::from(flags.contains(FieldAccessFlags::PUBLIC))
        + usize::from(flags.contains(FieldAccessFlags::PRIVATE))
        + usize::from(flags.contains(FieldAccessFlags::PROTECTED))
}

fn method_visibility_count(flags: MethodAccessFlags) -> usize {
    usize::from(flags.contains(MethodAccessFlags::PUBLIC))
        + usize::from(flags.contains(MethodAccessFlags::PRIVATE))
        + usize::from(flags.contains(MethodAccessFlags::PROTECTED))
}
