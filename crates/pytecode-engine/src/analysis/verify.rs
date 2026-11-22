use super::{AnalysisError, ClassResolver, build_cfg, recompute_frames_for_class};
use crate::constants::{
    ClassAccessFlags, FieldAccessFlags, MAGIC, MethodAccessFlags, class_version_supported,
};
use crate::descriptors::{
    is_valid_field_descriptor, is_valid_method_descriptor, parameter_slot_count,
    parse_method_descriptor,
};
use crate::error::EngineErrorKind;
use crate::indexes::{ClassIndex, CpIndex, ModuleIndex, NameAndTypeIndex, PackageIndex, Utf8Index};
use crate::model::{
    ClassModel, CodeItem, CodeModel, CodeTypeAnnotationTarget, DebugInfoState, FieldModel, Label,
    LocalvarTargetEntry, MethodBody, MethodModel,
};
use crate::raw::{
    AttributeInfo, ClassFile, ConstantPoolEntry, StackMapFrameInfo, VerificationTypeInfo,
};
use crate::signatures::{parse_class_signature, parse_field_signature, parse_method_signature};
use std::collections::{BTreeMap, HashSet};

fn valid_declared_method_descriptor(descriptor: &str, access_flags: MethodAccessFlags) -> bool {
    parse_method_descriptor(descriptor).is_ok_and(|parsed| {
        parameter_slot_count(&parsed)
            + usize::from(!access_flags.contains(MethodAccessFlags::STATIC))
            <= 255
    })
}

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

    fn warning(category: Category, message: impl Into<String>, location: Location) -> Self {
        Self {
            severity: Severity::Warning,
            category,
            message: message.into(),
            location,
        }
    }
}

/// Returned when `fail_fast` is enabled and the first ERROR-severity
/// diagnostic is encountered.
#[derive(Debug)]
pub struct FailFastError(pub Diagnostic);

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

// ---------------------------------------------------------------------------
// Name validation helpers (shared between classfile and classmodel verifiers)
// ---------------------------------------------------------------------------

use crate::names::{is_valid_internal_name, is_valid_method_name, is_valid_unqualified_name};

// ---------------------------------------------------------------------------
// Diagnostic collector with optional fail-fast support
// ---------------------------------------------------------------------------

struct DiagnosticCollector {
    diagnostics: Vec<Diagnostic>,
    fail_fast: bool,
    had_error: bool,
}

impl DiagnosticCollector {
    fn new(fail_fast: bool) -> Self {
        Self {
            diagnostics: Vec::new(),
            fail_fast,
            had_error: false,
        }
    }

    /// Push a diagnostic. Returns `Err(())` if fail-fast is enabled and this
    /// was the first ERROR, signalling the caller to stop immediately.
    fn push(&mut self, d: Diagnostic) -> Result<(), ()> {
        let is_error = d.severity == Severity::Error;
        self.diagnostics.push(d);
        if is_error {
            self.had_error = true;
            if self.fail_fast {
                return Err(());
            }
        }
        Ok(())
    }

    fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
    }
}

pub fn verify_classfile(classfile: &ClassFile) -> Vec<Diagnostic> {
    verify_classfile_inner(classfile, false)
}

pub fn verify_classfile_with_options(classfile: &ClassFile, fail_fast: bool) -> Vec<Diagnostic> {
    verify_classfile_inner(classfile, fail_fast)
}

fn verify_classfile_inner(classfile: &ClassFile, fail_fast: bool) -> Vec<Diagnostic> {
    let mut diagnostics = DiagnosticCollector::new(fail_fast);
    let _ = collect_classfile_diagnostics(classfile, &mut diagnostics);
    diagnostics.into_diagnostics()
}

fn collect_classfile_diagnostics(
    classfile: &ClassFile,
    diagnostics: &mut DiagnosticCollector,
) -> Result<(), ()> {
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
        ))?;
    }
    if !class_version_supported(classfile.major_version, classfile.minor_version) {
        diagnostics.push(Diagnostic::error(
            Category::Version,
            format!(
                "invalid class version {}/{}",
                classfile.major_version, classfile.minor_version
            ),
            class_location.clone(),
        ))?;
    }
    verify_constant_pool(classfile, class_name.as_deref(), diagnostics)?;
    verify_class_structure(classfile, class_name.as_deref(), diagnostics)?;
    verify_class_access_flags(classfile, class_location.clone(), diagnostics)?;
    verify_class_attributes(
        classfile,
        class_name.as_deref(),
        class_location.clone(),
        diagnostics,
    )?;
    if classfile.this_class.value() == 0 {
        diagnostics.push(Diagnostic::error(
            Category::ClassStructure,
            "this_class must be non-zero",
            class_location.clone(),
        ))?;
    }
    let mut seen_fields: HashSet<(String, String)> = HashSet::new();
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
            ))?;
        }
        if descriptor
            .as_deref()
            .is_none_or(|descriptor| !is_valid_field_descriptor(descriptor))
        {
            diagnostics.push(Diagnostic::error(
                Category::Descriptor,
                "invalid field descriptor",
                location.clone(),
            ))?;
        }
        if let (Some(field_name), Some(field_desc)) = (&name, &descriptor)
            && !seen_fields.insert((field_name.clone(), field_desc.clone()))
        {
            diagnostics.push(Diagnostic::error(
                Category::ClassStructure,
                format!("duplicate field: {field_name} {field_desc}"),
                location.clone(),
            ))?;
        }
        if field_visibility_count(field.access_flags) > 1 {
            diagnostics.push(Diagnostic::error(
                Category::AccessFlags,
                "field must not be both public/private/protected",
                location.clone(),
            ))?;
        }
        if field.access_flags.contains(FieldAccessFlags::FINAL)
            && field.access_flags.contains(FieldAccessFlags::VOLATILE)
        {
            diagnostics.push(Diagnostic::error(
                Category::AccessFlags,
                "field must not be both final and volatile",
                location.clone(),
            ))?;
        }
        verify_attributes(
            classfile,
            &field.attributes,
            AttributeOwner::Field,
            &location,
            diagnostics,
        )?;
        if let Some(field_desc) = descriptor.as_deref() {
            for attr in &field.attributes {
                if let AttributeInfo::ConstantValue(cv) = attr {
                    validate_constant_value_descriptor(
                        classfile.constant_pool.as_slice(),
                        cv.constantvalue_index.value(),
                        field_desc,
                        &location,
                        diagnostics,
                    )?;
                }
            }
        }
    }
    let mut seen_methods: HashSet<(String, String)> = HashSet::new();
    let is_interface = classfile.access_flags.contains(ClassAccessFlags::INTERFACE);
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
            ))?;
        }
        if descriptor.as_deref().is_none_or(|descriptor| {
            !valid_declared_method_descriptor(descriptor, method.access_flags)
        }) {
            diagnostics.push(Diagnostic::error(
                Category::Descriptor,
                "invalid method descriptor",
                location.clone(),
            ))?;
        }
        if let (Some(method_name), Some(method_desc)) = (&name, &descriptor)
            && !seen_methods.insert((method_name.clone(), method_desc.clone()))
        {
            diagnostics.push(Diagnostic::error(
                Category::ClassStructure,
                format!("duplicate method: {method_name}{method_desc}"),
                location.clone(),
            ))?;
        }
        if name.as_deref() == Some("<init>") {
            if is_interface {
                diagnostics.push(Diagnostic::error(
                    Category::Method,
                    "interface must not declare <init>",
                    location.clone(),
                ))?;
            }
            if descriptor
                .as_deref()
                .is_some_and(|desc| !desc.ends_with(")V"))
            {
                diagnostics.push(Diagnostic::error(
                    Category::Method,
                    "<init> must have void return type",
                    location.clone(),
                ))?;
            }
        }
        if method_visibility_count(method.access_flags) > 1 {
            diagnostics.push(Diagnostic::error(
                Category::AccessFlags,
                "method must not be both public/private/protected",
                location.clone(),
            ))?;
        }
        verify_attributes(
            classfile,
            &method.attributes,
            AttributeOwner::Method,
            &location,
            diagnostics,
        )?;
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
            ))?;
        }
        if method.access_flags.contains(MethodAccessFlags::NATIVE) && !code_attributes.is_empty() {
            diagnostics.push(Diagnostic::error(
                Category::Method,
                "native method must not carry Code",
                location.clone(),
            ))?;
        }
        if !method.access_flags.contains(MethodAccessFlags::ABSTRACT)
            && !method.access_flags.contains(MethodAccessFlags::NATIVE)
            && code_attributes.is_empty()
        {
            diagnostics.push(Diagnostic::error(
                Category::Method,
                "concrete method must carry Code",
                location.clone(),
            ))?;
        }
        if let Some(code) = code_attributes.first() {
            verify_attributes(
                classfile,
                &code.attributes,
                AttributeOwner::Code,
                &location,
                diagnostics,
            )?;
            if code.code_length == 0 || code.code_length >= 65536 {
                diagnostics.push(Diagnostic::error(
                    Category::Code,
                    "Code attribute code_length must be in 1..=65535",
                    location.clone(),
                ))?;
            }
            for handler in &code.exception_table {
                let start = u32::from(handler.start_pc);
                let end = u32::from(handler.end_pc);
                let handler_pc = u32::from(handler.handler_pc);
                if start >= end || end > code.code_length {
                    diagnostics.push(Diagnostic::error(
                        Category::Code,
                        "exception handler range is invalid",
                        location.clone(),
                    ))?;
                }
                if handler_pc >= code.code_length {
                    diagnostics.push(Diagnostic::error(
                        Category::Code,
                        "exception handler_pc exceeds code_length",
                        location.clone(),
                    ))?;
                }
                if handler.catch_type.value() != 0
                    && cp_class_name(classfile, handler.catch_type).is_err()
                {
                    diagnostics.push(Diagnostic::error(
                        Category::ConstantPool,
                        "exception handler catch_type must reference CONSTANT_Class or be zero",
                        location.clone(),
                    ))?;
                }
            }
            verify_stack_map_table_offsets(code, &location, diagnostics)?;
        }
    }
    Ok(())
}

pub fn verify_classmodel(
    model: &ClassModel,
    resolver: Option<&dyn ClassResolver>,
) -> Vec<Diagnostic> {
    verify_classmodel_inner(model, resolver, false)
}

pub fn verify_classmodel_with_options(
    model: &ClassModel,
    resolver: Option<&dyn ClassResolver>,
    fail_fast: bool,
) -> Vec<Diagnostic> {
    verify_classmodel_inner(model, resolver, fail_fast)
}

fn verify_classmodel_inner(
    model: &ClassModel,
    resolver: Option<&dyn ClassResolver>,
    fail_fast: bool,
) -> Vec<Diagnostic> {
    let mut dc = DiagnosticCollector::new(fail_fast);
    let _ = collect_classmodel_diagnostics(model, resolver, &mut dc);
    dc.into_diagnostics()
}

fn collect_classmodel_diagnostics(
    model: &ClassModel,
    resolver: Option<&dyn ClassResolver>,
    dc: &mut DiagnosticCollector,
) -> Result<(), ()> {
    let class_location = Location {
        class_name: Some(model.name.clone()),
        ..Location::default()
    };
    let is_interface = model.access_flags.contains(ClassAccessFlags::INTERFACE);

    if !class_version_supported(model.version.0, model.version.1) {
        dc.push(Diagnostic::error(
            Category::Version,
            format!(
                "invalid class version {}/{}",
                model.version.0, model.version.1
            ),
            class_location.clone(),
        ))?;
    }

    // ── Name validation ──────────────────────────────────────────────────
    verify_model_names(model, &class_location, dc)?;

    // ── Class-level access flags ─────────────────────────────────────────
    verify_model_class_flags(model.access_flags, &class_location, dc)?;

    // ── Module structural checks ─────────────────────────────────────────
    verify_model_module_class(model, &class_location, dc)?;

    // ── Class-level attribute checks ─────────────────────────────────────
    verify_model_attributes(
        model,
        &model.attributes,
        AttributeOwner::Class,
        &class_location,
        dc,
    )?;
    let has_nest_host = model
        .attributes
        .iter()
        .any(|attr| matches!(attr, AttributeInfo::NestHost(_)));
    let has_nest_members = model
        .attributes
        .iter()
        .any(|attr| matches!(attr, AttributeInfo::NestMembers(_)));
    if has_nest_host && has_nest_members {
        dc.push(Diagnostic::error(
            Category::Attribute,
            "class must not declare both NestHost and NestMembers",
            class_location.clone(),
        ))?;
    }
    if model
        .attributes
        .iter()
        .any(|attr| matches!(attr, AttributeInfo::PermittedSubclasses(_)))
        && model.access_flags.contains(ClassAccessFlags::FINAL)
    {
        dc.push(Diagnostic::error(
            Category::Attribute,
            "final class must not declare PermittedSubclasses",
            class_location.clone(),
        ))?;
    }

    // ── Debug info staleness (class-level) ───────────────────────────────
    if model.debug_info_state == DebugInfoState::Stale {
        dc.push(Diagnostic::warning(
            Category::Attribute,
            "Class debug metadata is marked stale and will be stripped during lowering",
            class_location.clone(),
        ))?;
    }

    // ── Duplicate detection ──────────────────────────────────────────────
    verify_model_duplicates(model, &class_location, dc)?;

    // ── Field validation ─────────────────────────────────────────────────
    for field in &model.fields {
        verify_model_field(field, model, is_interface, dc)?;
    }

    // ── Method validation ────────────────────────────────────────────────
    for method in &model.methods {
        verify_model_method(method, model, is_interface, resolver, dc)?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Model name validation
// ---------------------------------------------------------------------------

fn verify_model_names(
    model: &ClassModel,
    loc: &Location,
    dc: &mut DiagnosticCollector,
) -> Result<(), ()> {
    if !is_valid_internal_name(&model.name) {
        dc.push(Diagnostic::error(
            Category::ClassStructure,
            format!("Invalid class name: {:?}", model.name),
            loc.clone(),
        ))?;
    }

    let is_module = model.access_flags.contains(ClassAccessFlags::MODULE);
    let is_interface = model.access_flags.contains(ClassAccessFlags::INTERFACE);

    if let Some(ref super_name) = model.super_name {
        if !is_valid_internal_name(super_name) {
            dc.push(Diagnostic::error(
                Category::ClassStructure,
                format!("Invalid super class name: {:?}", super_name),
                loc.clone(),
            ))?;
        }
        if model.name == "java/lang/Object" {
            dc.push(Diagnostic::error(
                Category::ClassStructure,
                "java/lang/Object must not have a superclass",
                loc.clone(),
            ))?;
        } else if is_module {
            dc.push(Diagnostic::error(
                Category::ClassStructure,
                "MODULE class must not have a superclass",
                loc.clone(),
            ))?;
        } else if is_interface && super_name != "java/lang/Object" {
            dc.push(Diagnostic::error(
                Category::ClassStructure,
                format!(
                    "INTERFACE class must have java/lang/Object as superclass, got {:?}",
                    super_name
                ),
                loc.clone(),
            ))?;
        }
    } else if model.name != "java/lang/Object" && !is_module {
        dc.push(Diagnostic::error(
            Category::ClassStructure,
            "No superclass (only valid for java/lang/Object or MODULE classes)",
            loc.clone(),
        ))?;
    }

    let mut seen = HashSet::new();
    for iface in &model.interfaces {
        if !is_valid_internal_name(iface) {
            dc.push(Diagnostic::error(
                Category::ClassStructure,
                format!("Invalid interface name: {:?}", iface),
                loc.clone(),
            ))?;
        }
        if !seen.insert(iface.as_str()) {
            dc.push(Diagnostic::error(
                Category::ClassStructure,
                format!("Duplicate interface: {:?}", iface),
                loc.clone(),
            ))?;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Model class access-flag & module validation
// ---------------------------------------------------------------------------

fn verify_model_class_flags(
    flags: ClassAccessFlags,
    loc: &Location,
    dc: &mut DiagnosticCollector,
) -> Result<(), ()> {
    if flags.contains(ClassAccessFlags::INTERFACE) {
        if !flags.contains(ClassAccessFlags::ABSTRACT) {
            dc.push(Diagnostic::error(
                Category::AccessFlags,
                "INTERFACE class must also be ABSTRACT",
                loc.clone(),
            ))?;
        }
        if flags.contains(ClassAccessFlags::FINAL) {
            dc.push(Diagnostic::error(
                Category::AccessFlags,
                "INTERFACE class must not be FINAL",
                loc.clone(),
            ))?;
        }
        if flags.contains(ClassAccessFlags::SUPER) {
            dc.push(Diagnostic::error(
                Category::AccessFlags,
                "INTERFACE class must not be SUPER",
                loc.clone(),
            ))?;
        }
        if flags.contains(ClassAccessFlags::ENUM) {
            dc.push(Diagnostic::error(
                Category::AccessFlags,
                "INTERFACE class must not be ENUM",
                loc.clone(),
            ))?;
        }
    }

    if flags.contains(ClassAccessFlags::ANNOTATION) && !flags.contains(ClassAccessFlags::INTERFACE)
    {
        dc.push(Diagnostic::error(
            Category::AccessFlags,
            "ANNOTATION class must also be INTERFACE",
            loc.clone(),
        ))?;
    }

    if flags.contains(ClassAccessFlags::MODULE) {
        let non_module = flags.bits() & !ClassAccessFlags::MODULE.bits();
        if non_module != 0 {
            dc.push(Diagnostic::error(
                Category::AccessFlags,
                format!("MODULE class has unexpected flags: 0x{non_module:04X}"),
                loc.clone(),
            ))?;
        }
    }

    if flags.contains(ClassAccessFlags::FINAL)
        && flags.contains(ClassAccessFlags::ABSTRACT)
        && !flags.contains(ClassAccessFlags::INTERFACE)
    {
        dc.push(Diagnostic::error(
            Category::AccessFlags,
            "Class cannot be both FINAL and ABSTRACT",
            loc.clone(),
        ))?;
    }

    Ok(())
}

fn verify_model_module_class(
    model: &ClassModel,
    loc: &Location,
    dc: &mut DiagnosticCollector,
) -> Result<(), ()> {
    if !model.access_flags.contains(ClassAccessFlags::MODULE) {
        return Ok(());
    }
    if model.version.0 < 53 {
        dc.push(Diagnostic::error(
            Category::Version,
            "MODULE class requires class file version 53.0 or newer",
            loc.clone(),
        ))?;
    }
    if !model.interfaces.is_empty() {
        dc.push(Diagnostic::error(
            Category::ClassStructure,
            "module class must not declare interfaces",
            loc.clone(),
        ))?;
    }
    if !model.fields.is_empty() {
        dc.push(Diagnostic::error(
            Category::ClassStructure,
            "module class must not declare fields",
            loc.clone(),
        ))?;
    }
    if !model.methods.is_empty() {
        dc.push(Diagnostic::error(
            Category::ClassStructure,
            "module class must not declare methods",
            loc.clone(),
        ))?;
    }
    let module_count = model
        .attributes
        .iter()
        .filter(|attr| matches!(attr, AttributeInfo::Module(_)))
        .count();
    if module_count != 1 {
        let mut attr_loc = loc.clone();
        attr_loc.attribute_name = Some("Module".to_owned());
        dc.push(Diagnostic::error(
            Category::Attribute,
            "module class must declare exactly one Module attribute",
            attr_loc,
        ))?;
    }
    Ok(())
}

fn verify_model_attributes(
    model: &ClassModel,
    attributes: &[AttributeInfo],
    owner: AttributeOwner,
    base_location: &Location,
    dc: &mut DiagnosticCollector,
) -> Result<(), ()> {
    let mut unique_counts = BTreeMap::<String, usize>::new();
    for attribute in attributes {
        let attr_name = attribute_name(attribute).to_owned();
        let mut location = base_location.clone();
        location.attribute_name = Some(attr_name.clone());

        if !attribute_allowed_on_owner(attribute, owner) {
            dc.push(Diagnostic::error(
                Category::Attribute,
                format!(
                    "{attr_name} attribute is not allowed on {}",
                    owner.placement_name()
                ),
                location.clone(),
            ))?;
        }
        if attribute_requires_module_class(attribute)
            && !model.access_flags.contains(ClassAccessFlags::MODULE)
        {
            dc.push(Diagnostic::error(
                Category::Attribute,
                format!("{attr_name} attribute is only allowed on module classes"),
                location.clone(),
            ))?;
        }
        if let Some(min_major) =
            attribute_version_supported(attribute, model.version.0, model.version.1)
        {
            dc.push(Diagnostic::error(
                Category::Attribute,
                format!("{attr_name} attribute requires class file version {min_major}.0 or newer"),
                location.clone(),
            ))?;
        }
        if attribute_must_be_unique(attribute) {
            *unique_counts.entry(attr_name).or_default() += 1;
        }
    }
    for (attr_name, count) in unique_counts {
        if count > 1 {
            let mut location = base_location.clone();
            location.attribute_name = Some(attr_name.clone());
            dc.push(Diagnostic::error(
                Category::Attribute,
                format!(
                    "{} must not declare multiple {attr_name} attributes",
                    owner.duplicate_subject()
                ),
                location,
            ))?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Duplicate field/method detection
// ---------------------------------------------------------------------------

fn verify_model_duplicates(
    model: &ClassModel,
    loc: &Location,
    dc: &mut DiagnosticCollector,
) -> Result<(), ()> {
    let mut field_sigs: HashSet<(&str, &str)> = HashSet::new();
    for field in &model.fields {
        let key = (field.name.as_str(), field.descriptor.as_str());
        if !field_sigs.insert(key) {
            dc.push(Diagnostic::error(
                Category::ClassStructure,
                format!("Duplicate field: {} {}", field.name, field.descriptor),
                loc.clone(),
            ))?;
        }
    }

    let mut method_sigs: HashSet<(&str, &str)> = HashSet::new();
    for method in &model.methods {
        let key = (method.name.as_str(), method.descriptor.as_str());
        if !method_sigs.insert(key) {
            dc.push(Diagnostic::error(
                Category::ClassStructure,
                format!("Duplicate method: {}{}", method.name, method.descriptor),
                loc.clone(),
            ))?;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Per-field validation
// ---------------------------------------------------------------------------

fn verify_model_field(
    field: &FieldModel,
    model: &ClassModel,
    is_interface: bool,
    dc: &mut DiagnosticCollector,
) -> Result<(), ()> {
    let loc = Location {
        class_name: Some(model.name.clone()),
        field_name: Some(field.name.clone()),
        ..Location::default()
    };

    if !is_valid_unqualified_name(&field.name) {
        dc.push(Diagnostic::error(
            Category::Field,
            format!("Invalid field name: {:?}", field.name),
            loc.clone(),
        ))?;
    }

    if !is_valid_field_descriptor(&field.descriptor) {
        dc.push(Diagnostic::error(
            Category::Descriptor,
            format!("Invalid field descriptor: {:?}", field.descriptor),
            loc.clone(),
        ))?;
    }

    if field_visibility_count(field.access_flags) > 1 {
        dc.push(Diagnostic::error(
            Category::AccessFlags,
            format!("Field {:?} has multiple visibility modifiers", field.name),
            loc.clone(),
        ))?;
    }

    if field.access_flags.contains(FieldAccessFlags::FINAL)
        && field.access_flags.contains(FieldAccessFlags::VOLATILE)
    {
        dc.push(Diagnostic::error(
            Category::AccessFlags,
            format!("Field {:?} cannot be both FINAL and VOLATILE", field.name),
            loc.clone(),
        ))?;
    }

    if is_interface {
        let required =
            FieldAccessFlags::PUBLIC | FieldAccessFlags::STATIC | FieldAccessFlags::FINAL;
        if !field.access_flags.contains(required) {
            dc.push(Diagnostic::error(
                Category::AccessFlags,
                format!(
                    "Interface field {:?} must be PUBLIC STATIC FINAL",
                    field.name
                ),
                loc.clone(),
            ))?;
        }
        let forbidden = FieldAccessFlags::PRIVATE
            | FieldAccessFlags::PROTECTED
            | FieldAccessFlags::VOLATILE
            | FieldAccessFlags::TRANSIENT
            | FieldAccessFlags::ENUM;
        let bad = field.access_flags & forbidden;
        if !bad.is_empty() {
            dc.push(Diagnostic::error(
                Category::AccessFlags,
                format!(
                    "Interface field {:?} has illegal flags: 0x{:04X}",
                    field.name,
                    bad.bits()
                ),
                loc.clone(),
            ))?;
        }
    }

    verify_model_attributes(model, &field.attributes, AttributeOwner::Field, &loc, dc)?;
    for attr in &field.attributes {
        if let AttributeInfo::ConstantValue(cv) = attr
            && !model.constant_pool.is_empty()
        {
            validate_constant_value_descriptor(
                model.constant_pool.entries(),
                cv.constantvalue_index.value(),
                &field.descriptor,
                &loc,
                dc,
            )?;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Per-method validation
// ---------------------------------------------------------------------------

fn verify_model_method(
    method: &MethodModel,
    model: &ClassModel,
    is_interface: bool,
    resolver: Option<&dyn ClassResolver>,
    dc: &mut DiagnosticCollector,
) -> Result<(), ()> {
    let loc = Location {
        class_name: Some(model.name.clone()),
        method_name: Some(method.name.clone()),
        method_descriptor: Some(method.descriptor.clone()),
        ..Location::default()
    };

    // Name validation
    if !is_valid_method_name(&method.name) {
        dc.push(Diagnostic::error(
            Category::Method,
            format!("Invalid method name: {:?}", method.name),
            loc.clone(),
        ))?;
    }

    // Descriptor validation
    if !valid_declared_method_descriptor(&method.descriptor, method.access_flags) {
        dc.push(Diagnostic::error(
            Category::Descriptor,
            format!("Invalid method descriptor: {:?}", method.descriptor),
            loc.clone(),
        ))?;
    }

    // Method access flags
    verify_model_method_flags(
        method.access_flags,
        &method.name,
        is_interface,
        model.version.0,
        &loc,
        dc,
    )?;

    // <init> checks
    if method.name == "<init>" {
        if is_interface {
            dc.push(Diagnostic::error(
                Category::Method,
                "Interface method cannot be <init>",
                loc.clone(),
            ))?;
        }
        if !method.descriptor.ends_with(")V") {
            dc.push(Diagnostic::error(
                Category::Method,
                format!(
                    "<init> must have void return type, got {:?}",
                    method.descriptor
                ),
                loc.clone(),
            ))?;
        }
    }

    // <clinit> descriptor check
    if method.name == "<clinit>" && method.descriptor != "()V" {
        dc.push(Diagnostic::error(
            Category::Method,
            format!(
                "<clinit> must have descriptor ()V, got {:?}",
                method.descriptor
            ),
            loc.clone(),
        ))?;
    }

    verify_model_attributes(model, &method.attributes, AttributeOwner::Method, &loc, dc)?;
    if method
        .attributes
        .iter()
        .any(|attr| matches!(attr, AttributeInfo::Code(_)))
    {
        dc.push(Diagnostic::error(
            Category::Attribute,
            "MethodModel must store Code via body rather than raw Code attributes",
            loc.clone(),
        ))?;
    }

    // Code presence / absence
    let is_abstract = method.access_flags.contains(MethodAccessFlags::ABSTRACT);
    let is_native = method.access_flags.contains(MethodAccessFlags::NATIVE);

    if is_abstract || is_native {
        if method.has_code() {
            let label = if is_abstract { "ABSTRACT" } else { "NATIVE" };
            dc.push(Diagnostic::error(
                Category::Method,
                format!("{} method {:?} must not have code", label, method.name),
                loc.clone(),
            ))?;
        }
    } else if !method.has_code() {
        dc.push(Diagnostic::error(
            Category::Method,
            format!("Method {:?} must have code", method.name),
            loc.clone(),
        ))?;
    }

    // Code body validation
    match &method.body {
        Some(MethodBody::Symbolic(code)) => {
            verify_code_model(code, model, method, resolver, dc)?;
        }
        Some(MethodBody::Prebuilt(bytes)) => {
            verify_prebuilt_code_body(bytes, model, method, &loc, dc)?;
        }
        None => {}
    }

    Ok(())
}

fn verify_prebuilt_code_body(
    bytes: &[u8],
    model: &ClassModel,
    method: &MethodModel,
    location: &Location,
    dc: &mut DiagnosticCollector,
) -> Result<(), ()> {
    let code_attr = match crate::reader::parse_code_attribute_payload(
        0.into(),
        bytes,
        model.constant_pool.entries(),
    ) {
        Ok(attr) => attr,
        Err(error) => {
            dc.push(Diagnostic::error(
                Category::Code,
                format!(
                    "Prebuilt Code payload for {:?} is invalid: {error}",
                    method.name
                ),
                location.clone(),
            ))?;
            return Ok(());
        }
    };
    if code_attr.code_length == 0 || code_attr.code_length >= 65536 {
        dc.push(Diagnostic::error(
            Category::Code,
            "Prebuilt Code attribute code_length must be in 1..=65535",
            location.clone(),
        ))?;
    }
    for handler in &code_attr.exception_table {
        let start = u32::from(handler.start_pc);
        let end = u32::from(handler.end_pc);
        let handler_pc = u32::from(handler.handler_pc);
        if start >= end || end > code_attr.code_length {
            dc.push(Diagnostic::error(
                Category::Code,
                "prebuilt exception handler range is invalid",
                location.clone(),
            ))?;
        }
        if handler_pc >= code_attr.code_length {
            dc.push(Diagnostic::error(
                Category::Code,
                "prebuilt exception handler_pc exceeds code_length",
                location.clone(),
            ))?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Method access-flag validation
// ---------------------------------------------------------------------------

fn verify_model_method_flags(
    flags: MethodAccessFlags,
    name: &str,
    is_interface: bool,
    major: u16,
    loc: &Location,
    dc: &mut DiagnosticCollector,
) -> Result<(), ()> {
    if method_visibility_count(flags) > 1 {
        dc.push(Diagnostic::error(
            Category::AccessFlags,
            format!("Method {:?} has multiple visibility modifiers", name),
            loc.clone(),
        ))?;
    }

    if flags.contains(MethodAccessFlags::ABSTRACT) {
        let forbidden = MethodAccessFlags::PRIVATE
            | MethodAccessFlags::STATIC
            | MethodAccessFlags::FINAL
            | MethodAccessFlags::SYNCHRONIZED
            | MethodAccessFlags::NATIVE
            | MethodAccessFlags::STRICT;
        let bad = flags & forbidden;
        if !bad.is_empty() {
            dc.push(Diagnostic::error(
                Category::AccessFlags,
                format!(
                    "ABSTRACT method {:?} has illegal flags: 0x{:04X}",
                    name,
                    bad.bits()
                ),
                loc.clone(),
            ))?;
        }
    }

    if is_interface && name != "<clinit>" {
        let forbidden_iface = MethodAccessFlags::PROTECTED
            | MethodAccessFlags::FINAL
            | MethodAccessFlags::SYNCHRONIZED
            | MethodAccessFlags::NATIVE;
        let bad_iface = flags & forbidden_iface;
        if !bad_iface.is_empty() {
            dc.push(Diagnostic::error(
                Category::AccessFlags,
                format!(
                    "Interface method {:?} has illegal flags: 0x{:04X}",
                    name,
                    bad_iface.bits()
                ),
                loc.clone(),
            ))?;
        }
        if major < 52 {
            if (!flags.contains(MethodAccessFlags::PUBLIC)
                || !flags.contains(MethodAccessFlags::ABSTRACT))
                && name != "<init>"
            {
                dc.push(Diagnostic::error(
                    Category::AccessFlags,
                    format!(
                        "Interface method {:?} must be PUBLIC ABSTRACT (pre-Java 8)",
                        name
                    ),
                    loc.clone(),
                ))?;
            }
        } else if !flags.contains(MethodAccessFlags::PUBLIC)
            && name != "<init>"
            && (major < 53 || !flags.contains(MethodAccessFlags::PRIVATE))
        {
            dc.push(Diagnostic::error(
                Category::AccessFlags,
                format!(
                    "Interface method {:?} must be PUBLIC (or PRIVATE for Java 9+)",
                    name
                ),
                loc.clone(),
            ))?;
        }
    }

    if name == "<init>" {
        let forbidden_init = MethodAccessFlags::STATIC
            | MethodAccessFlags::FINAL
            | MethodAccessFlags::SYNCHRONIZED
            | MethodAccessFlags::NATIVE
            | MethodAccessFlags::ABSTRACT
            | MethodAccessFlags::BRIDGE;
        let bad = flags & forbidden_init;
        if !bad.is_empty() {
            dc.push(Diagnostic::error(
                Category::Method,
                format!("<init> has illegal flags: 0x{:04X}", bad.bits()),
                loc.clone(),
            ))?;
        }
    }

    if name == "<clinit>" && major >= 51 && !flags.contains(MethodAccessFlags::STATIC) {
        dc.push(Diagnostic::error(
            Category::Method,
            "<clinit> must be STATIC in class file version 51.0 or newer",
            loc.clone(),
        ))?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Code model validation
// ---------------------------------------------------------------------------

fn verify_code_model(
    code: &CodeModel,
    model: &ClassModel,
    method: &MethodModel,
    resolver: Option<&dyn ClassResolver>,
    dc: &mut DiagnosticCollector,
) -> Result<(), ()> {
    let location = Location {
        class_name: Some(model.name.clone()),
        method_name: Some(method.name.clone()),
        method_descriptor: Some(method.descriptor.clone()),
        ..Location::default()
    };

    verify_model_attributes(model, &code.attributes, AttributeOwner::Code, &location, dc)?;

    // Debug info staleness
    if code.debug_info_state == DebugInfoState::Stale {
        dc.push(Diagnostic::warning(
            Category::Code,
            "Code debug metadata is marked stale and will be stripped during lowering",
            location.clone(),
        ))?;
    }

    // Empty instruction list
    if code.instructions.is_empty() {
        dc.push(Diagnostic::error(
            Category::Code,
            "Code has empty instruction list",
            location.clone(),
        ))?;
        return Ok(());
    }

    // Collect label positions (as instruction ordinals) and detect duplicate labels
    let mut label_ordinals: BTreeMap<u64, usize> = BTreeMap::new();
    let mut insn_count = 0usize;
    for item in &code.instructions {
        match item {
            CodeItem::Label(label) => {
                if label_ordinals.insert(label.id(), insn_count).is_some() {
                    dc.push(Diagnostic::error(
                        Category::Code,
                        "Code contains duplicate label in instruction stream",
                        location.clone(),
                    ))?;
                }
            }
            _ => {
                insn_count += 1;
            }
        }
    }

    if insn_count == 0 {
        dc.push(Diagnostic::error(
            Category::Code,
            "Code contains only labels and no instructions",
            location.clone(),
        ))?;
        return Ok(());
    }

    let check_label =
        |label: &Label, context: &str, dc: &mut DiagnosticCollector| -> Result<Option<usize>, ()> {
            match label_ordinals.get(&label.id()) {
                Some(&ord) => Ok(Some(ord)),
                None => {
                    dc.push(Diagnostic::error(
                        Category::Code,
                        format!("{context} references label not in instruction stream"),
                        location.clone(),
                    ))?;
                    Ok(None)
                }
            }
        };

    // Exception handler labels and bounds
    for eh in &code.exception_handlers {
        let start_ord = check_label(&eh.start, "Exception handler start", dc)?;
        let end_ord = check_label(&eh.end, "Exception handler end", dc)?;
        let handler_ord = check_label(&eh.handler, "Exception handler handler", dc)?;
        if let (Some(s), Some(e)) = (start_ord, end_ord)
            && s >= e
        {
            dc.push(Diagnostic::error(
                Category::Code,
                "Exception handler start must precede end with non-empty range",
                location.clone(),
            ))?;
        }
        if let Some(h) = handler_ord
            && h >= insn_count
        {
            dc.push(Diagnostic::error(
                Category::Code,
                "Exception handler target label points past end of instructions",
                location.clone(),
            ))?;
        }
        if let Some(catch_type) = &eh.catch_type
            && !is_valid_internal_name(catch_type)
        {
            dc.push(Diagnostic::error(
                Category::Code,
                format!("Invalid exception catch_type name: {catch_type:?}"),
                location.clone(),
            ))?;
        }
    }

    // Line number entry labels
    for ln in &code.line_numbers {
        if let Some(ord) = check_label(&ln.label, "Line number entry", dc)?
            && ord >= insn_count
        {
            dc.push(Diagnostic::error(
                Category::Code,
                "Line number entry label points past end of instructions",
                location.clone(),
            ))?;
        }
    }

    // Local variable validation
    for lv in &code.local_variables {
        let start_ord = check_label(
            &lv.start,
            &format!("Local variable '{}' start", lv.name),
            dc,
        )?;
        let end_ord = check_label(&lv.end, &format!("Local variable '{}' end", lv.name), dc)?;
        if let (Some(s), Some(e)) = (start_ord, end_ord)
            && e < s
        {
            dc.push(Diagnostic::error(
                Category::Code,
                format!("Local variable '{}' end precedes start", lv.name),
                location.clone(),
            ))?;
        }
        if !is_valid_unqualified_name(&lv.name) {
            dc.push(Diagnostic::error(
                Category::Code,
                format!("Invalid local variable name: {:?}", lv.name),
                location.clone(),
            ))?;
        }
        if !is_valid_field_descriptor(&lv.descriptor) {
            dc.push(Diagnostic::error(
                Category::Descriptor,
                format!("Invalid local variable descriptor: {:?}", lv.descriptor),
                location.clone(),
            ))?;
        } else {
            let width = u32::from(matches!(lv.descriptor.as_str(), "J" | "D") as u16 + 1);
            if u32::from(lv.index) + width > u32::from(code.max_locals) {
                dc.push(Diagnostic::error(
                    Category::Code,
                    format!(
                        "Local variable '{}' slot {} exceeds max_locals {}",
                        lv.name, lv.index, code.max_locals
                    ),
                    location.clone(),
                ))?;
            }
        }
    }

    // Local variable type validation
    for lvt in &code.local_variable_types {
        let start_ord = check_label(
            &lvt.start,
            &format!("Local variable type '{}' start", lvt.name),
            dc,
        )?;
        let end_ord = check_label(
            &lvt.end,
            &format!("Local variable type '{}' end", lvt.name),
            dc,
        )?;
        if let (Some(s), Some(e)) = (start_ord, end_ord)
            && e < s
        {
            dc.push(Diagnostic::error(
                Category::Code,
                format!("Local variable type '{}' end precedes start", lvt.name),
                location.clone(),
            ))?;
        }
        if !is_valid_unqualified_name(&lvt.name) {
            dc.push(Diagnostic::error(
                Category::Code,
                format!("Invalid local variable type name: {:?}", lvt.name),
                location.clone(),
            ))?;
        }
        if parse_field_signature(&lvt.signature).is_err() {
            dc.push(Diagnostic::error(
                Category::Attribute,
                format!("Invalid local variable type signature: {:?}", lvt.signature),
                location.clone(),
            ))?;
        }
        if lvt.index >= code.max_locals {
            dc.push(Diagnostic::error(
                Category::Code,
                format!(
                    "Local variable type '{}' slot {} exceeds max_locals {}",
                    lvt.name, lvt.index, code.max_locals
                ),
                location.clone(),
            ))?;
        }
    }

    // Code-level type annotation label & catch bounds validation
    for ta in code
        .visible_type_annotations
        .iter()
        .chain(&code.invisible_type_annotations)
    {
        match &ta.target {
            CodeTypeAnnotationTarget::Localvar { table } => {
                for row in table {
                    if let LocalvarTargetEntry::Range { start, end, index } = row {
                        let s = check_label(start, "type-annotation localvar start", dc)?;
                        let e = check_label(end, "type-annotation localvar end", dc)?;
                        if let (Some(s), Some(e)) = (s, e)
                            && e < s
                        {
                            dc.push(Diagnostic::error(
                                Category::Code,
                                "type-annotation localvar end precedes start",
                                location.clone(),
                            ))?;
                        }
                        if *index >= code.max_locals {
                            dc.push(Diagnostic::error(
                                Category::Code,
                                format!(
                                    "type-annotation localvar slot {index} exceeds max_locals {}",
                                    code.max_locals
                                ),
                                location.clone(),
                            ))?;
                        }
                    }
                }
            }
            CodeTypeAnnotationTarget::Catch {
                exception_table_index,
            } => {
                if *exception_table_index >= code.exception_handlers.len() {
                    dc.push(Diagnostic::error(
                        Category::Code,
                        format!(
                            "type-annotation catch index {exception_table_index} exceeds exception handler count {}",
                            code.exception_handlers.len()
                        ),
                        location.clone(),
                    ))?;
                }
            }
            CodeTypeAnnotationTarget::Offset { target }
            | CodeTypeAnnotationTarget::TypeArgument { target, .. } => {
                if let Some(ord) = check_label(target, "type-annotation target", dc)?
                    && ord >= insn_count
                {
                    dc.push(Diagnostic::error(
                        Category::Code,
                        "type-annotation target label points past end of instructions",
                        location.clone(),
                    ))?;
                }
            }
            CodeTypeAnnotationTarget::Other(_) => {}
        }
    }

    // Branch / switch target labels & switch invariants
    for item in &code.instructions {
        match item {
            CodeItem::Branch(branch) => {
                if let Some(ord) = check_label(&branch.target, "Branch target", dc)?
                    && ord >= insn_count
                {
                    dc.push(Diagnostic::error(
                        Category::Code,
                        "Branch target label points past end of instructions",
                        location.clone(),
                    ))?;
                }
            }
            CodeItem::LookupSwitch(ls) => {
                if let Some(ord) = check_label(&ls.default_target, "lookupswitch default", dc)?
                    && ord >= insn_count
                {
                    dc.push(Diagnostic::error(
                        Category::Code,
                        "lookupswitch default label points past end of instructions",
                        location.clone(),
                    ))?;
                }
                for pair in ls.pairs.windows(2) {
                    if pair[0].0 >= pair[1].0 {
                        dc.push(Diagnostic::error(
                            Category::Code,
                            "lookupswitch keys must be strictly sorted in ascending order",
                            location.clone(),
                        ))?;
                        break;
                    }
                }
                for (match_val, label) in &ls.pairs {
                    if let Some(ord) =
                        check_label(label, &format!("lookupswitch case {match_val}"), dc)?
                        && ord >= insn_count
                    {
                        dc.push(Diagnostic::error(
                            Category::Code,
                            format!(
                                "lookupswitch case {match_val} label points past end of instructions"
                            ),
                            location.clone(),
                        ))?;
                    }
                }
            }
            CodeItem::TableSwitch(ts) => {
                if let Some(ord) = check_label(&ts.default_target, "tableswitch default", dc)?
                    && ord >= insn_count
                {
                    dc.push(Diagnostic::error(
                        Category::Code,
                        "tableswitch default label points past end of instructions",
                        location.clone(),
                    ))?;
                }
                if ts.low > ts.high {
                    dc.push(Diagnostic::error(
                        Category::Code,
                        "tableswitch low exceeds high",
                        location.clone(),
                    ))?;
                } else {
                    let expected = i64::from(ts.high) - i64::from(ts.low) + 1;
                    if usize::try_from(expected).ok() != Some(ts.targets.len()) {
                        dc.push(Diagnostic::error(
                            Category::Code,
                            format!(
                                "tableswitch target count {} does not match {}..={}",
                                ts.targets.len(),
                                ts.low,
                                ts.high
                            ),
                            location.clone(),
                        ))?;
                    }
                }
                for label in &ts.targets {
                    if let Some(ord) = check_label(label, "tableswitch case", dc)?
                        && ord >= insn_count
                    {
                        dc.push(Diagnostic::error(
                            Category::Code,
                            "tableswitch case label points past end of instructions",
                            location.clone(),
                        ))?;
                    }
                }
            }
            _ => {}
        }
    }

    // Existing CFG + frame validation
    if let Err(error) = build_cfg(code) {
        dc.push(Diagnostic::error(
            Category::Code,
            analysis_error_message(&error),
            location.clone(),
        ))?;
        return Ok(());
    }
    match recompute_frames_for_class(code, model, method, resolver) {
        Err(error) => {
            dc.push(Diagnostic::error(
                Category::Code,
                analysis_error_message(&error),
                location,
            ))?;
        }
        Ok(result) => {
            if code.max_stack < result.max_stack {
                dc.push(Diagnostic::error(
                    Category::Code,
                    format!(
                        "max_stack {} is smaller than required {}",
                        code.max_stack, result.max_stack
                    ),
                    location.clone(),
                ))?;
            }
            if code.max_locals < result.max_locals {
                dc.push(Diagnostic::error(
                    Category::Code,
                    format!(
                        "max_locals {} is smaller than required {}",
                        code.max_locals, result.max_locals
                    ),
                    location,
                ))?;
            }
        }
    }

    Ok(())
}

fn analysis_error_message(error: &AnalysisError) -> String {
    error.to_string()
}

fn verify_constant_pool(
    classfile: &ClassFile,
    class_name: Option<&str>,
    dc: &mut DiagnosticCollector,
) -> Result<(), ()> {
    if classfile.constant_pool.is_empty() {
        dc.push(Diagnostic::error(
            Category::ConstantPool,
            "constant pool must include slot 0",
            Location {
                class_name: class_name.map(str::to_owned),
                ..Location::default()
            },
        ))?;
        return Ok(());
    }
    if classfile.constant_pool[0].is_some() {
        dc.push(Diagnostic::error(
            Category::ConstantPool,
            "constant pool slot 0 must be empty",
            Location {
                class_name: class_name.map(str::to_owned),
                cp_index: Some(0),
                ..Location::default()
            },
        ))?;
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
                dc.push(Diagnostic::error(
                    Category::ConstantPool,
                    "long/double constant-pool entry must be followed by an empty gap slot",
                    location,
                ))?;
            }
            expect_gap = false;
            continue;
        }
        let Some(entry) = entry else {
            dc.push(Diagnostic::error(
                Category::ConstantPool,
                "unexpected empty constant-pool slot",
                location,
            ))?;
            continue;
        };
        expect_gap = entry.is_wide();
        match entry {
            ConstantPoolEntry::Class(info) if cp_utf8(classfile, info.name_index).is_err() => {
                dc.push(Diagnostic::error(
                    Category::ConstantPool,
                    "class entry references invalid Utf8 name",
                    location,
                ))?;
            }
            ConstantPoolEntry::String(info)
                if !matches!(
                    classfile.constant_pool.get(usize::from(info.string_index.value())),
                    Some(Some(ConstantPoolEntry::Utf8(value))) if crate::modified_utf8::JavaString::from_modified_utf8(&value.bytes).is_ok()
                ) =>
            {
                dc.push(Diagnostic::error(
                    Category::ConstantPool,
                    "string entry references invalid Utf8 payload",
                    location,
                ))?;
            }
            ConstantPoolEntry::FieldRef(info) => {
                if cp_class_name(classfile, info.class_index).is_err() {
                    dc.push(Diagnostic::error(
                        Category::ConstantPool,
                        "member reference has invalid class_index",
                        location.clone(),
                    ))?;
                }
                if cp_name_and_type(classfile, info.name_and_type_index).is_err() {
                    dc.push(Diagnostic::error(
                        Category::ConstantPool,
                        "member reference has invalid name_and_type_index",
                        location.clone(),
                    ))?;
                } else if cp_name_and_type(classfile, info.name_and_type_index)
                    .is_ok_and(|(_, descriptor)| !is_valid_field_descriptor(&descriptor))
                {
                    dc.push(Diagnostic::error(
                        Category::ConstantPool,
                        "field reference descriptor must be a field descriptor",
                        location,
                    ))?;
                }
            }
            ConstantPoolEntry::MethodRef(info) => {
                if cp_class_name(classfile, info.class_index).is_err() {
                    dc.push(Diagnostic::error(
                        Category::ConstantPool,
                        "member reference has invalid class_index",
                        location.clone(),
                    ))?;
                }
                if cp_name_and_type(classfile, info.name_and_type_index).is_err() {
                    dc.push(Diagnostic::error(
                        Category::ConstantPool,
                        "member reference has invalid name_and_type_index",
                        location.clone(),
                    ))?;
                } else if cp_name_and_type(classfile, info.name_and_type_index)
                    .is_ok_and(|(_, descriptor)| !is_valid_method_descriptor(&descriptor))
                {
                    dc.push(Diagnostic::error(
                        Category::ConstantPool,
                        "method reference descriptor must be a method descriptor",
                        location,
                    ))?;
                }
            }
            ConstantPoolEntry::InterfaceMethodRef(info) => {
                if cp_class_name(classfile, info.class_index).is_err() {
                    dc.push(Diagnostic::error(
                        Category::ConstantPool,
                        "member reference has invalid class_index",
                        location.clone(),
                    ))?;
                }
                if cp_name_and_type(classfile, info.name_and_type_index).is_err() {
                    dc.push(Diagnostic::error(
                        Category::ConstantPool,
                        "member reference has invalid name_and_type_index",
                        location.clone(),
                    ))?;
                } else if cp_name_and_type(classfile, info.name_and_type_index)
                    .is_ok_and(|(_, descriptor)| !is_valid_method_descriptor(&descriptor))
                {
                    dc.push(Diagnostic::error(
                        Category::ConstantPool,
                        "interface method reference descriptor must be a method descriptor",
                        location,
                    ))?;
                }
            }
            ConstantPoolEntry::NameAndType(info) => {
                if cp_utf8(classfile, info.name_index).is_err() {
                    dc.push(Diagnostic::error(
                        Category::ConstantPool,
                        "name_and_type entry references invalid Utf8 name",
                        location.clone(),
                    ))?;
                }
                if cp_utf8(classfile, info.descriptor_index).is_err() {
                    dc.push(Diagnostic::error(
                        Category::ConstantPool,
                        "name_and_type entry references invalid Utf8 descriptor",
                        location,
                    ))?;
                }
            }
            ConstantPoolEntry::MethodType(info) => {
                if cp_utf8(classfile, info.descriptor_index).is_err() {
                    dc.push(Diagnostic::error(
                        Category::ConstantPool,
                        "method type entry references invalid Utf8 descriptor",
                        location.clone(),
                    ))?;
                } else if cp_utf8(classfile, info.descriptor_index)
                    .is_ok_and(|descriptor| !is_valid_method_descriptor(&descriptor))
                {
                    dc.push(Diagnostic::error(
                        Category::ConstantPool,
                        "method type descriptor must be a method descriptor",
                        location,
                    ))?;
                }
            }
            ConstantPoolEntry::MethodHandle(info) => {
                verify_method_handle(classfile, info, &location, dc)?;
            }
            ConstantPoolEntry::Dynamic(info) => {
                if cp_name_and_type(classfile, info.name_and_type_index).is_err() {
                    dc.push(Diagnostic::error(
                        Category::ConstantPool,
                        "dynamic entry has invalid name_and_type_index",
                        location.clone(),
                    ))?;
                } else if cp_name_and_type(classfile, info.name_and_type_index)
                    .is_ok_and(|(_, descriptor)| !is_valid_field_descriptor(&descriptor))
                {
                    dc.push(Diagnostic::error(
                        Category::ConstantPool,
                        "dynamic entry descriptor must be a field descriptor",
                        location.clone(),
                    ))?;
                }
                if !bootstrap_method_index_valid(
                    classfile,
                    info.bootstrap_method_attr_index.value(),
                ) {
                    dc.push(Diagnostic::error(
                        Category::ConstantPool,
                        "dynamic entry bootstrap_method_attr_index is out of range",
                        location,
                    ))?;
                }
            }
            ConstantPoolEntry::InvokeDynamic(info) => {
                if cp_name_and_type(classfile, info.name_and_type_index).is_err() {
                    dc.push(Diagnostic::error(
                        Category::ConstantPool,
                        "dynamic entry has invalid name_and_type_index",
                        location.clone(),
                    ))?;
                } else if cp_name_and_type(classfile, info.name_and_type_index)
                    .is_ok_and(|(_, descriptor)| !is_valid_method_descriptor(&descriptor))
                {
                    dc.push(Diagnostic::error(
                        Category::ConstantPool,
                        "invokedynamic entry descriptor must be a method descriptor",
                        location.clone(),
                    ))?;
                }
                if !bootstrap_method_index_valid(
                    classfile,
                    info.bootstrap_method_attr_index.value(),
                ) {
                    dc.push(Diagnostic::error(
                        Category::ConstantPool,
                        "invokedynamic entry bootstrap_method_attr_index is out of range",
                        location,
                    ))?;
                }
            }
            ConstantPoolEntry::Module(info) if cp_utf8(classfile, info.name_index).is_err() => {
                dc.push(Diagnostic::error(
                    Category::ConstantPool,
                    "module/package entry references invalid Utf8 name",
                    location,
                ))?;
            }
            ConstantPoolEntry::Package(info) if cp_utf8(classfile, info.name_index).is_err() => {
                dc.push(Diagnostic::error(
                    Category::ConstantPool,
                    "module/package entry references invalid Utf8 name",
                    location,
                ))?;
            }
            _ => {}
        }
    }
    if expect_gap {
        dc.push(Diagnostic::error(
            Category::ConstantPool,
            "long/double constant-pool entry is missing its trailing gap slot",
            Location {
                class_name: class_name.map(str::to_owned),
                ..Location::default()
            },
        ))?;
    }
    Ok(())
}

fn verify_class_structure(
    classfile: &ClassFile,
    class_name: Option<&str>,
    dc: &mut DiagnosticCollector,
) -> Result<(), ()> {
    let location = Location {
        class_name: class_name.map(str::to_owned),
        ..Location::default()
    };
    let is_module = classfile.access_flags.contains(ClassAccessFlags::MODULE);
    let is_interface = classfile.access_flags.contains(ClassAccessFlags::INTERFACE);

    if classfile.this_class.value() != 0 && cp_class_name(classfile, classfile.this_class).is_err()
    {
        dc.push(Diagnostic::error(
            Category::ClassStructure,
            "this_class must reference CONSTANT_Class",
            location.clone(),
        ))?;
    }

    if is_module {
        if classfile.major_version < 53 {
            dc.push(Diagnostic::error(
                Category::Version,
                "module class requires class file version 53.0 or newer",
                location.clone(),
            ))?;
        }
        if classfile.super_class.value() != 0 {
            dc.push(Diagnostic::error(
                Category::ClassStructure,
                "module class must have super_class == 0",
                location.clone(),
            ))?;
        }
        if !classfile.interfaces.is_empty() {
            dc.push(Diagnostic::error(
                Category::ClassStructure,
                "module class must not declare interfaces",
                location.clone(),
            ))?;
        }
        if !classfile.fields.is_empty() {
            dc.push(Diagnostic::error(
                Category::ClassStructure,
                "module class must not declare fields",
                location.clone(),
            ))?;
        }
        if !classfile.methods.is_empty() {
            dc.push(Diagnostic::error(
                Category::ClassStructure,
                "module class must not declare methods",
                location.clone(),
            ))?;
        }
    } else if class_name == Some("java/lang/Object") {
        if classfile.super_class.value() != 0 {
            dc.push(Diagnostic::error(
                Category::ClassStructure,
                "java/lang/Object must have super_class == 0",
                location.clone(),
            ))?;
        }
    } else if classfile.super_class.value() == 0 {
        dc.push(Diagnostic::error(
            Category::ClassStructure,
            "non-root class must have non-zero super_class",
            location.clone(),
        ))?;
    } else {
        match cp_class_name(classfile, classfile.super_class) {
            Err(()) => {
                dc.push(Diagnostic::error(
                    Category::ClassStructure,
                    "super_class must reference CONSTANT_Class",
                    location.clone(),
                ))?;
            }
            Ok(super_name) if is_interface && super_name != "java/lang/Object" => {
                dc.push(Diagnostic::error(
                    Category::ClassStructure,
                    format!(
                        "interface must have java/lang/Object as super_class, got {super_name:?}"
                    ),
                    location.clone(),
                ))?;
            }
            Ok(_) => {}
        }
    }

    let mut seen_interfaces = HashSet::new();
    for interface in &classfile.interfaces {
        match cp_class_name(classfile, *interface) {
            Err(()) => {
                dc.push(Diagnostic::error(
                    Category::ClassStructure,
                    "interface entry must reference CONSTANT_Class",
                    location.clone(),
                ))?;
            }
            Ok(iface_name) => {
                if !seen_interfaces.insert(iface_name.clone()) {
                    dc.push(Diagnostic::error(
                        Category::ClassStructure,
                        format!("duplicate interface: {iface_name:?}"),
                        location.clone(),
                    ))?;
                }
            }
        }
    }
    Ok(())
}

fn verify_class_access_flags(
    classfile: &ClassFile,
    location: Location,
    dc: &mut DiagnosticCollector,
) -> Result<(), ()> {
    let flags = classfile.access_flags;
    if flags.contains(ClassAccessFlags::INTERFACE) {
        if !flags.contains(ClassAccessFlags::ABSTRACT) {
            dc.push(Diagnostic::error(
                Category::AccessFlags,
                "interface must also be abstract",
                location.clone(),
            ))?;
        }
        if flags.contains(ClassAccessFlags::FINAL) {
            dc.push(Diagnostic::error(
                Category::AccessFlags,
                "interface must not be final",
                location.clone(),
            ))?;
        }
        if flags.contains(ClassAccessFlags::SUPER) {
            dc.push(Diagnostic::error(
                Category::AccessFlags,
                "interface must not be super",
                location.clone(),
            ))?;
        }
        if flags.contains(ClassAccessFlags::ENUM) {
            dc.push(Diagnostic::error(
                Category::AccessFlags,
                "interface must not be enum",
                location.clone(),
            ))?;
        }
    }
    if flags.contains(ClassAccessFlags::ANNOTATION) && !flags.contains(ClassAccessFlags::INTERFACE)
    {
        dc.push(Diagnostic::error(
            Category::AccessFlags,
            "annotation must also be an interface",
            location.clone(),
        ))?;
    }
    if flags.contains(ClassAccessFlags::MODULE) {
        let non_module = flags.bits() & !ClassAccessFlags::MODULE.bits();
        if non_module != 0 {
            dc.push(Diagnostic::error(
                Category::AccessFlags,
                format!("module class has unexpected flags: 0x{non_module:04X}"),
                location.clone(),
            ))?;
        }
    }
    if flags.contains(ClassAccessFlags::FINAL)
        && flags.contains(ClassAccessFlags::ABSTRACT)
        && !flags.contains(ClassAccessFlags::INTERFACE)
    {
        dc.push(Diagnostic::error(
            Category::AccessFlags,
            "class must not be both final and abstract",
            location,
        ))?;
    }
    Ok(())
}

fn verify_class_attributes(
    classfile: &ClassFile,
    class_name: Option<&str>,
    base_location: Location,
    dc: &mut DiagnosticCollector,
) -> Result<(), ()> {
    verify_attributes(
        classfile,
        &classfile.attributes,
        AttributeOwner::Class,
        &base_location,
        dc,
    )?;

    let has_nest_host = classfile
        .attributes
        .iter()
        .any(|attr| matches!(attr, AttributeInfo::NestHost(_)));
    let has_nest_members = classfile
        .attributes
        .iter()
        .any(|attr| matches!(attr, AttributeInfo::NestMembers(_)));
    if has_nest_host && has_nest_members {
        dc.push(Diagnostic::error(
            Category::Attribute,
            "class must not declare both NestHost and NestMembers",
            base_location.clone(),
        ))?;
    }
    if classfile
        .attributes
        .iter()
        .any(|attr| matches!(attr, AttributeInfo::PermittedSubclasses(_)))
        && classfile.access_flags.contains(ClassAccessFlags::FINAL)
    {
        dc.push(Diagnostic::error(
            Category::Attribute,
            "final class must not declare PermittedSubclasses",
            base_location.clone(),
        ))?;
    }

    for attribute in &classfile.attributes {
        if let AttributeInfo::Record(record) = attribute {
            verify_record_components(classfile, record, class_name, dc)?;
        }
    }

    Ok(())
}

fn verify_record_components(
    classfile: &ClassFile,
    record: &crate::raw::RecordAttribute,
    class_name: Option<&str>,
    dc: &mut DiagnosticCollector,
) -> Result<(), ()> {
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
            dc.push(Diagnostic::error(
                Category::ConstantPool,
                "record component name_index must reference Utf8",
                location.clone(),
            ))?;
        }
        if descriptor
            .as_deref()
            .is_none_or(|descriptor| !is_valid_field_descriptor(descriptor))
        {
            dc.push(Diagnostic::error(
                Category::Descriptor,
                "invalid record component descriptor",
                location.clone(),
            ))?;
        }
        verify_attributes(
            classfile,
            &component.attributes,
            AttributeOwner::RecordComponent,
            &location,
            dc,
        )?;
    }
    Ok(())
}

fn verify_attributes(
    classfile: &ClassFile,
    attributes: &[AttributeInfo],
    owner: AttributeOwner,
    base_location: &Location,
    dc: &mut DiagnosticCollector,
) -> Result<(), ()> {
    let mut unique_counts = BTreeMap::<String, usize>::new();

    for attribute in attributes {
        let attribute_name = attribute_name(attribute).to_owned();
        let mut location = base_location.clone();
        location.attribute_name = Some(attribute_name.clone());

        match cp_utf8(classfile, attribute.attribute_name_index()) {
            Ok(cp_name) => {
                if !matches!(attribute, AttributeInfo::Unknown(_)) && cp_name != attribute_name {
                    dc.push(Diagnostic::error(
                        Category::Attribute,
                        format!(
                            "attribute_name_index resolves to {cp_name:?}, expected {attribute_name:?}"
                        ),
                        location.clone(),
                    ))?;
                }
            }
            Err(()) => {
                dc.push(Diagnostic::error(
                    Category::ConstantPool,
                    "attribute_name_index must reference Utf8",
                    location.clone(),
                ))?;
            }
        }
        if !attribute_allowed_on_owner(attribute, owner) {
            dc.push(Diagnostic::error(
                Category::Attribute,
                format!(
                    "{attribute_name} attribute is not allowed on {}",
                    owner.placement_name()
                ),
                location.clone(),
            ))?;
        }
        if attribute_requires_module_class(attribute)
            && !classfile.access_flags.contains(ClassAccessFlags::MODULE)
        {
            dc.push(Diagnostic::error(
                Category::Attribute,
                format!("{attribute_name} attribute is only allowed on module classes"),
                location.clone(),
            ))?;
        }
        if let Some(min_major) =
            attribute_version_supported(attribute, classfile.major_version, classfile.minor_version)
        {
            dc.push(Diagnostic::error(
                Category::Attribute,
                format!(
                    "{attribute_name} attribute requires class file version {min_major}.0 or newer"
                ),
                location.clone(),
            ))?;
        }
        if attribute_must_be_unique(attribute) {
            *unique_counts.entry(attribute_name.clone()).or_default() += 1;
        }
        verify_attribute_contents(classfile, attribute, owner, &location, dc)?;
    }

    for (attribute_name, count) in unique_counts {
        if count > 1 {
            let mut location = base_location.clone();
            location.attribute_name = Some(attribute_name.clone());
            dc.push(Diagnostic::error(
                Category::Attribute,
                format!(
                    "{} must not declare multiple {attribute_name} attributes",
                    owner.duplicate_subject()
                ),
                location,
            ))?;
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
            dc.push(Diagnostic::error(
                Category::Attribute,
                "module class must declare exactly one Module attribute",
                location,
            ))?;
        }
    }

    Ok(())
}

fn verify_attribute_contents(
    classfile: &ClassFile,
    attribute: &AttributeInfo,
    owner: AttributeOwner,
    location: &Location,
    dc: &mut DiagnosticCollector,
) -> Result<(), ()> {
    match attribute {
        AttributeInfo::ConstantValue(attribute) => {
            if !matches!(
                cp_entry(classfile, attribute.constantvalue_index.value()),
                Ok(ConstantPoolEntry::Integer(_)
                    | ConstantPoolEntry::Float(_)
                    | ConstantPoolEntry::Long(_)
                    | ConstantPoolEntry::Double(_)
                    | ConstantPoolEntry::String(_))
            ) {
                dc.push(Diagnostic::error(
                    Category::ConstantPool,
                    "ConstantValue constantvalue_index must reference Integer, Float, Long, Double, or String",
                    location.clone(),
                ))?;
            }
        }
        AttributeInfo::Signature(attribute) => {
            let signature = cp_utf8(classfile, attribute.signature_index);
            if signature.is_err() {
                dc.push(Diagnostic::error(
                    Category::ConstantPool,
                    "Signature attribute must reference a Utf8 signature",
                    location.clone(),
                ))?;
            } else if let Some(message) =
                validate_signature_attribute(owner, signature.as_deref().unwrap_or_default())
            {
                dc.push(Diagnostic::error(
                    Category::Attribute,
                    message,
                    location.clone(),
                ))?;
            }
        }
        AttributeInfo::SourceFile(attribute)
            if cp_utf8(classfile, attribute.sourcefile_index).is_err() =>
        {
            dc.push(Diagnostic::error(
                Category::ConstantPool,
                "SourceFile attribute must reference a Utf8 source file name",
                location.clone(),
            ))?;
        }
        AttributeInfo::InnerClasses(attribute) => {
            for inner in &attribute.classes {
                if cp_class_name(classfile, inner.inner_class_info_index).is_err() {
                    dc.push(Diagnostic::error(
                        Category::ConstantPool,
                        "InnerClasses inner_class_info_index must reference CONSTANT_Class",
                        location.clone(),
                    ))?;
                }
                if inner.outer_class_info_index.value() != 0
                    && cp_class_name(classfile, inner.outer_class_info_index).is_err()
                {
                    dc.push(Diagnostic::error(
                        Category::ConstantPool,
                        "InnerClasses outer_class_info_index must reference CONSTANT_Class or be zero",
                        location.clone(),
                    ))?;
                }
                if inner.inner_name_index.value() != 0
                    && cp_utf8(classfile, inner.inner_name_index).is_err()
                {
                    dc.push(Diagnostic::error(
                        Category::ConstantPool,
                        "InnerClasses inner_name_index must reference Utf8 or be zero",
                        location.clone(),
                    ))?;
                }
            }
        }
        AttributeInfo::Exceptions(attribute) => {
            for exception_index in &attribute.exception_index_table {
                if cp_class_name(classfile, *exception_index).is_err() {
                    dc.push(Diagnostic::error(
                        Category::ConstantPool,
                        "Exceptions attribute entries must reference CONSTANT_Class",
                        location.clone(),
                    ))?;
                }
            }
        }
        AttributeInfo::EnclosingMethod(attribute) => {
            if cp_class_name(classfile, attribute.class_index).is_err() {
                dc.push(Diagnostic::error(
                    Category::ConstantPool,
                    "EnclosingMethod attribute class_index must reference CONSTANT_Class",
                    location.clone(),
                ))?;
            }
            if attribute.method_index.value() != 0
                && cp_name_and_type(classfile, attribute.method_index).is_err()
            {
                dc.push(Diagnostic::error(
                    Category::ConstantPool,
                    "EnclosingMethod attribute method_index must reference CONSTANT_NameAndType or be zero",
                    location.clone(),
                ))?;
            }
        }
        AttributeInfo::MethodParameters(attribute) => {
            for parameter in &attribute.parameters {
                if parameter.name_index.value() != 0
                    && cp_utf8(classfile, parameter.name_index).is_err()
                {
                    dc.push(Diagnostic::error(
                        Category::ConstantPool,
                        "MethodParameters names must reference Utf8 or be zero",
                        location.clone(),
                    ))?;
                }
            }
        }
        AttributeInfo::BootstrapMethods(attribute) => {
            for bootstrap_method in &attribute.bootstrap_methods {
                if !matches!(
                    cp_entry(classfile, bootstrap_method.bootstrap_method_ref.value()),
                    Ok(ConstantPoolEntry::MethodHandle(_))
                ) {
                    dc.push(Diagnostic::error(
                        Category::ConstantPool,
                        "BootstrapMethods bootstrap_method_ref must reference CONSTANT_MethodHandle",
                        location.clone(),
                    ))?;
                }
                for argument in &bootstrap_method.bootstrap_arguments {
                    if !bootstrap_argument_entry_valid(classfile, argument.value()) {
                        dc.push(Diagnostic::error(
                            Category::ConstantPool,
                            "BootstrapMethods arguments must reference loadable constants",
                            location.clone(),
                        ))?;
                    }
                }
            }
        }
        AttributeInfo::StackMapTable(attribute) => {
            let mut verify_types = Vec::new();
            for frame in &attribute.entries {
                match frame {
                    StackMapFrameInfo::Same { .. }
                    | StackMapFrameInfo::Chop { .. }
                    | StackMapFrameInfo::SameExtended { .. } => {}
                    StackMapFrameInfo::SameLocals1StackItem { stack, .. }
                    | StackMapFrameInfo::SameLocals1StackItemExtended { stack, .. } => {
                        verify_types.push(stack);
                    }
                    StackMapFrameInfo::Append { locals, .. } => {
                        verify_types.extend(locals.iter());
                    }
                    StackMapFrameInfo::Full { locals, stack, .. } => {
                        verify_types.extend(locals.iter());
                        verify_types.extend(stack.iter());
                    }
                }
            }
            for vt in verify_types {
                if let VerificationTypeInfo::Object { cpool_index } = vt
                    && cp_class_name(classfile, *cpool_index).is_err()
                {
                    dc.push(Diagnostic::error(
                        Category::ConstantPool,
                        "StackMapTable Object verification type must reference CONSTANT_Class",
                        location.clone(),
                    ))?;
                }
            }
        }
        AttributeInfo::LocalVariableTypeTable(attribute) => {
            for entry in &attribute.local_variable_type_table {
                if cp_utf8(classfile, entry.name_index).is_err() {
                    dc.push(Diagnostic::error(
                        Category::ConstantPool,
                        "LocalVariableTypeTable name_index must reference Utf8",
                        location.clone(),
                    ))?;
                }
                let signature = cp_utf8(classfile, entry.signature_index);
                if signature.is_err() {
                    dc.push(Diagnostic::error(
                        Category::ConstantPool,
                        "LocalVariableTypeTable signature_index must reference Utf8",
                        location.clone(),
                    ))?;
                } else if let Err(error) =
                    parse_field_signature(signature.as_deref().unwrap_or_default())
                {
                    dc.push(Diagnostic::error(
                        Category::Attribute,
                        format!(
                            "invalid local variable type signature: {}",
                            signature_reason(&error.kind)
                        ),
                        location.clone(),
                    ))?;
                }
            }
        }
        AttributeInfo::NestHost(attribute)
            if cp_class_name(classfile, attribute.host_class_index).is_err() =>
        {
            dc.push(Diagnostic::error(
                Category::ConstantPool,
                "NestHost attribute must reference CONSTANT_Class",
                location.clone(),
            ))?;
        }
        AttributeInfo::NestMembers(attribute) => {
            for class_index in &attribute.classes {
                if cp_class_name(classfile, *class_index).is_err() {
                    dc.push(Diagnostic::error(
                        Category::ConstantPool,
                        "NestMembers entries must reference CONSTANT_Class",
                        location.clone(),
                    ))?;
                }
            }
        }
        AttributeInfo::Module(attribute) => {
            if cp_module_name(classfile, attribute.module.module_name_index).is_err() {
                dc.push(Diagnostic::error(
                    Category::ConstantPool,
                    "Module attribute must reference CONSTANT_Module for module_name_index",
                    location.clone(),
                ))?;
            }
            if attribute.module.module_version_index.value() != 0
                && cp_utf8(classfile, attribute.module.module_version_index).is_err()
            {
                dc.push(Diagnostic::error(
                    Category::ConstantPool,
                    "Module attribute module_version_index must reference Utf8 or be zero",
                    location.clone(),
                ))?;
            }
            for requires in &attribute.module.requires {
                if cp_module_name(classfile, requires.requires_index).is_err() {
                    dc.push(Diagnostic::error(
                        Category::ConstantPool,
                        "Module requires entries must reference CONSTANT_Module",
                        location.clone(),
                    ))?;
                }
                if requires.requires_version_index.value() != 0
                    && cp_utf8(classfile, requires.requires_version_index).is_err()
                {
                    dc.push(Diagnostic::error(
                        Category::ConstantPool,
                        "Module requires version indices must reference Utf8 or be zero",
                        location.clone(),
                    ))?;
                }
            }
            for exports in &attribute.module.exports {
                if cp_package_name(classfile, exports.exports_index).is_err() {
                    dc.push(Diagnostic::error(
                        Category::ConstantPool,
                        "Module exports entries must reference CONSTANT_Package",
                        location.clone(),
                    ))?;
                }
                for target in &exports.exports_to_index {
                    if cp_module_name(classfile, *target).is_err() {
                        dc.push(Diagnostic::error(
                            Category::ConstantPool,
                            "Module exports targets must reference CONSTANT_Module",
                            location.clone(),
                        ))?;
                    }
                }
            }
            for opens in &attribute.module.opens {
                if cp_package_name(classfile, opens.opens_index).is_err() {
                    dc.push(Diagnostic::error(
                        Category::ConstantPool,
                        "Module opens entries must reference CONSTANT_Package",
                        location.clone(),
                    ))?;
                }
                for target in &opens.opens_to_index {
                    if cp_module_name(classfile, *target).is_err() {
                        dc.push(Diagnostic::error(
                            Category::ConstantPool,
                            "Module opens targets must reference CONSTANT_Module",
                            location.clone(),
                        ))?;
                    }
                }
            }
            for class_index in &attribute.module.uses_index {
                if cp_class_name(classfile, *class_index).is_err() {
                    dc.push(Diagnostic::error(
                        Category::ConstantPool,
                        "Module uses entries must reference CONSTANT_Class",
                        location.clone(),
                    ))?;
                }
            }
            for provides in &attribute.module.provides {
                if cp_class_name(classfile, provides.provides_index).is_err() {
                    dc.push(Diagnostic::error(
                        Category::ConstantPool,
                        "Module provides entries must reference CONSTANT_Class",
                        location.clone(),
                    ))?;
                }
                for implementation in &provides.provides_with_index {
                    if cp_class_name(classfile, *implementation).is_err() {
                        dc.push(Diagnostic::error(
                            Category::ConstantPool,
                            "Module provides targets must reference CONSTANT_Class",
                            location.clone(),
                        ))?;
                    }
                }
            }
        }
        AttributeInfo::ModulePackages(attribute) => {
            for package_index in &attribute.package_index {
                if cp_package_name(classfile, *package_index).is_err() {
                    dc.push(Diagnostic::error(
                        Category::ConstantPool,
                        "ModulePackages entries must reference CONSTANT_Package",
                        location.clone(),
                    ))?;
                }
            }
        }
        AttributeInfo::ModuleMainClass(attribute)
            if cp_class_name(classfile, attribute.main_class_index).is_err() =>
        {
            dc.push(Diagnostic::error(
                Category::ConstantPool,
                "ModuleMainClass attribute must reference CONSTANT_Class",
                location.clone(),
            ))?;
        }
        AttributeInfo::PermittedSubclasses(attribute) => {
            for class_index in &attribute.classes {
                if cp_class_name(classfile, *class_index).is_err() {
                    dc.push(Diagnostic::error(
                        Category::ConstantPool,
                        "PermittedSubclasses entries must reference CONSTANT_Class",
                        location.clone(),
                    ))?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn validate_constant_value_descriptor(
    constant_pool: &[Option<ConstantPoolEntry>],
    cp_index: u16,
    field_descriptor: &str,
    location: &Location,
    dc: &mut DiagnosticCollector,
) -> Result<(), ()> {
    let entry = constant_pool
        .get(usize::from(cp_index))
        .and_then(Option::as_ref);
    let matches_descriptor = match field_descriptor {
        "B" | "C" | "I" | "S" | "Z" => matches!(entry, Some(ConstantPoolEntry::Integer(_))),
        "F" => matches!(entry, Some(ConstantPoolEntry::Float(_))),
        "J" => matches!(entry, Some(ConstantPoolEntry::Long(_))),
        "D" => matches!(entry, Some(ConstantPoolEntry::Double(_))),
        "Ljava/lang/String;" => matches!(entry, Some(ConstantPoolEntry::String(_))),
        _ => false,
    };
    if !matches_descriptor {
        dc.push(Diagnostic::error(
            Category::Attribute,
            format!("ConstantValue attribute does not match field descriptor {field_descriptor:?}"),
            location.clone(),
        ))?;
    }
    Ok(())
}

fn verify_stack_map_table_offsets(
    code: &crate::raw::CodeAttribute,
    location: &Location,
    dc: &mut DiagnosticCollector,
) -> Result<(), ()> {
    for attr in &code.attributes {
        if let AttributeInfo::StackMapTable(smt) = attr {
            for frame in &smt.entries {
                match frame {
                    StackMapFrameInfo::Same { .. }
                    | StackMapFrameInfo::Chop { .. }
                    | StackMapFrameInfo::SameExtended { .. } => {}
                    StackMapFrameInfo::SameLocals1StackItem { stack, .. }
                    | StackMapFrameInfo::SameLocals1StackItemExtended { stack, .. } => {
                        if let VerificationTypeInfo::Uninitialized { offset } = stack
                            && u32::from(*offset) >= code.code_length
                        {
                            dc.push(Diagnostic::error(
                                Category::Code,
                                "StackMapTable Uninitialized offset exceeds code_length",
                                location.clone(),
                            ))?;
                        }
                    }
                    StackMapFrameInfo::Append { locals, .. } => {
                        for vt in locals {
                            if let VerificationTypeInfo::Uninitialized { offset } = vt
                                && u32::from(*offset) >= code.code_length
                            {
                                dc.push(Diagnostic::error(
                                    Category::Code,
                                    "StackMapTable Uninitialized offset exceeds code_length",
                                    location.clone(),
                                ))?;
                            }
                        }
                    }
                    StackMapFrameInfo::Full { locals, stack, .. } => {
                        for vt in locals.iter().chain(stack.iter()) {
                            if let VerificationTypeInfo::Uninitialized { offset } = vt
                                && u32::from(*offset) >= code.code_length
                            {
                                dc.push(Diagnostic::error(
                                    Category::Code,
                                    "StackMapTable Uninitialized offset exceeds code_length",
                                    location.clone(),
                                ))?;
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
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
    dc: &mut DiagnosticCollector,
) -> Result<(), ()> {
    if !(1..=9).contains(&info.reference_kind) {
        dc.push(Diagnostic::error(
            Category::ConstantPool,
            "method handle reference_kind must be in 1..=9",
            location.clone(),
        ))?;
        return Ok(());
    }

    let target_entry = cp_entry(classfile, info.reference_index.value());
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
        dc.push(Diagnostic::error(
            Category::ConstantPool,
            method_handle_target_message(info.reference_kind),
            location.clone(),
        ))?;
        return Ok(());
    }

    if let Ok((name, _)) = cp_member_name_and_type(classfile, info.reference_index) {
        match info.reference_kind {
            8 if name != "<init>" => {
                dc.push(Diagnostic::error(
                    Category::ConstantPool,
                    "REF_newInvokeSpecial method handle must reference <init>",
                    location.clone(),
                ))?;
            }
            5 | 6 | 7 | 9 if matches!(name.as_str(), "<init>" | "<clinit>") => {
                dc.push(Diagnostic::error(
                    Category::ConstantPool,
                    "method handles of kind REF_invokeVirtual/Static/Special/Interface must not reference <init> or <clinit>",
                    location.clone(),
                ))?;
            }
            _ => {}
        }
    }

    Ok(())
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
    !matches!(
        attribute,
        AttributeInfo::LineNumberTable(_)
            | AttributeInfo::LocalVariableTable(_)
            | AttributeInfo::LocalVariableTypeTable(_)
            | AttributeInfo::Unknown(_)
    )
}

fn attribute_version_supported(attribute: &AttributeInfo, major: u16, minor: u16) -> Option<u16> {
    match attribute {
        AttributeInfo::Record(_) if (major == 58 || major == 59) && minor == u16::MAX => None,
        AttributeInfo::PermittedSubclasses(_)
            if (major == 59 || major == 60) && minor == u16::MAX =>
        {
            None
        }
        _ => attribute_minimum_major(attribute).filter(|&min_major| major < min_major),
    }
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

fn cp_utf8(classfile: &ClassFile, index: Utf8Index) -> Result<String, ()> {
    let entry = classfile
        .constant_pool
        .get(index.value() as usize)
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

fn cp_class_name(classfile: &ClassFile, index: ClassIndex) -> Result<String, ()> {
    let entry = cp_entry(classfile, index.value())?;
    match entry {
        ConstantPoolEntry::Class(info) => cp_utf8(classfile, info.name_index),
        _ => Err(()),
    }
}

fn cp_module_name(classfile: &ClassFile, index: ModuleIndex) -> Result<String, ()> {
    let entry = cp_entry(classfile, index.value())?;
    match entry {
        ConstantPoolEntry::Module(info) => cp_utf8(classfile, info.name_index),
        _ => Err(()),
    }
}

fn cp_package_name(classfile: &ClassFile, index: PackageIndex) -> Result<String, ()> {
    let entry = cp_entry(classfile, index.value())?;
    match entry {
        ConstantPoolEntry::Package(info) => cp_utf8(classfile, info.name_index),
        _ => Err(()),
    }
}

fn cp_name_and_type(
    classfile: &ClassFile,
    index: NameAndTypeIndex,
) -> Result<(String, String), ()> {
    let entry = cp_entry(classfile, index.value())?;
    match entry {
        ConstantPoolEntry::NameAndType(info) => Ok((
            cp_utf8(classfile, info.name_index)?,
            cp_utf8(classfile, info.descriptor_index)?,
        )),
        _ => Err(()),
    }
}

fn cp_member_name_and_type(classfile: &ClassFile, index: CpIndex) -> Result<(String, String), ()> {
    match cp_entry(classfile, index.value())? {
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
