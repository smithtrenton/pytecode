pub mod matcher_spec;
pub mod pipeline_spec;
pub mod transform_spec;

use crate::Result;
use crate::constants::{ClassAccessFlags, FieldAccessFlags, MethodAccessFlags};
use crate::model::{ClassModel, CodeItem, CodeModel, FieldModel, LdcValue, MethodModel};
use crate::transform::matcher_spec::insn_matches_opcode;
use regex::Regex;
use std::fmt;
use std::ops::{BitAnd, BitOr, Not};
use std::sync::Arc;

pub trait ApplyClassTransform {
    fn apply(&mut self, model: &mut ClassModel) -> Result<()>;
}

impl<F> ApplyClassTransform for F
where
    F: FnMut(&mut ClassModel) -> Result<()>,
{
    fn apply(&mut self, model: &mut ClassModel) -> Result<()> {
        self(model)
    }
}

pub type BoxClassTransform = Box<dyn ApplyClassTransform + Send>;

#[derive(Default)]
pub struct Pipeline {
    pub transforms: Vec<BoxClassTransform>,
}

impl Pipeline {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn of(transform: impl ApplyClassTransform + Send + 'static) -> Self {
        Self {
            transforms: vec![Box::new(transform)],
        }
    }

    pub fn then(mut self, transform: impl ApplyClassTransform + Send + 'static) -> Self {
        self.transforms.push(Box::new(transform));
        self
    }

    pub fn apply(&mut self, model: &mut ClassModel) -> Result<()> {
        for transform in &mut self.transforms {
            transform.apply(model)?;
        }
        Ok(())
    }
}

impl ApplyClassTransform for Pipeline {
    fn apply(&mut self, model: &mut ClassModel) -> Result<()> {
        self.apply(model)
    }
}

impl fmt::Debug for Pipeline {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Pipeline")
            .field("transform_count", &self.transforms.len())
            .finish()
    }
}

#[macro_export]
macro_rules! pipeline {
    () => {
        $crate::transform::Pipeline::new()
    };
    ($first:expr $(, $rest:expr )* $(,)?) => {
        $crate::transform::Pipeline::of($first)$(.then($rest))*
    };
}

#[derive(Clone)]
pub struct Matcher<T> {
    predicate: Arc<dyn Fn(&T) -> bool + Send + Sync>,
    description: Arc<str>,
}

impl<T> Matcher<T> {
    pub fn of(
        predicate: impl Fn(&T) -> bool + Send + Sync + 'static,
        description: impl Into<String>,
    ) -> Self {
        Self {
            predicate: Arc::new(predicate),
            description: Arc::from(description.into()),
        }
    }

    pub fn matches(&self, value: &T) -> bool {
        (self.predicate)(value)
    }
}

impl<T> fmt::Debug for Matcher<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Matcher[{}]", self.description)
    }
}

impl<T: 'static> BitAnd for Matcher<T> {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        let lhs_predicate = self.predicate.clone();
        let rhs_predicate = rhs.predicate.clone();
        let description = format!(
            "({} & {})",
            self.description.as_ref(),
            rhs.description.as_ref()
        );
        Self::of(
            move |value| lhs_predicate(value) && rhs_predicate(value),
            description,
        )
    }
}

impl<T: 'static> BitOr for Matcher<T> {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        let lhs_predicate = self.predicate.clone();
        let rhs_predicate = rhs.predicate.clone();
        let description = format!(
            "({} | {})",
            self.description.as_ref(),
            rhs.description.as_ref()
        );
        Self::of(
            move |value| lhs_predicate(value) || rhs_predicate(value),
            description,
        )
    }
}

impl<T: 'static> Not for Matcher<T> {
    type Output = Self;

    fn not(self) -> Self::Output {
        let predicate = self.predicate.clone();
        let description = format!("~{}", parenthesize_description(&self.description));
        Self::of(move |value| !predicate(value), description)
    }
}

pub type ClassMatcher = Matcher<ClassModel>;
pub type FieldMatcher = Matcher<FieldModel>;
pub type MethodMatcher = Matcher<MethodModel>;
pub type InsnMatcher = Matcher<CodeItem>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassContext {
    pub version: (u16, u16),
    pub access_flags: ClassAccessFlags,
    pub name: String,
    pub super_name: Option<String>,
    pub interfaces: Vec<String>,
}

impl ClassContext {
    pub fn from_model(model: &ClassModel) -> Self {
        Self {
            version: model.version,
            access_flags: model.access_flags,
            name: model.name.clone(),
            super_name: model.super_name.clone(),
            interfaces: model.interfaces.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodContext {
    pub access_flags: MethodAccessFlags,
    pub name: String,
    pub descriptor: String,
    pub has_code: bool,
}

impl MethodContext {
    pub fn from_method(method: &MethodModel) -> Self {
        Self {
            access_flags: method.access_flags,
            name: method.name.clone(),
            descriptor: method.descriptor.clone(),
            has_code: method.code.is_some(),
        }
    }
}

pub fn on_classes(
    mut transform: impl ApplyClassTransform + Send + 'static,
    where_matcher: Option<ClassMatcher>,
) -> impl ApplyClassTransform + Send {
    move |model: &mut ClassModel| {
        if where_matcher
            .as_ref()
            .is_some_and(|matcher| !matcher.matches(model))
        {
            return Ok(());
        }
        transform.apply(model)
    }
}

pub fn on_fields<F>(
    mut transform: F,
    where_matcher: Option<FieldMatcher>,
    owner_matcher: Option<ClassMatcher>,
) -> impl ApplyClassTransform + Send
where
    F: FnMut(&mut FieldModel, &ClassContext) -> Result<()> + Send + 'static,
{
    move |model: &mut ClassModel| {
        if owner_matcher
            .as_ref()
            .is_some_and(|matcher| !matcher.matches(model))
        {
            return Ok(());
        }
        let owner = ClassContext::from_model(model);
        let original_len = model.fields.len();
        for index in 0..original_len {
            let should_apply = {
                let field = &model.fields[index];
                where_matcher
                    .as_ref()
                    .is_none_or(|matcher| matcher.matches(field))
            };
            if should_apply {
                transform(&mut model.fields[index], &owner)?;
            }
        }
        Ok(())
    }
}

pub fn on_methods<F>(
    mut transform: F,
    where_matcher: Option<MethodMatcher>,
    owner_matcher: Option<ClassMatcher>,
) -> impl ApplyClassTransform + Send
where
    F: FnMut(&mut MethodModel, &ClassContext) -> Result<()> + Send + 'static,
{
    move |model: &mut ClassModel| {
        if owner_matcher
            .as_ref()
            .is_some_and(|matcher| !matcher.matches(model))
        {
            return Ok(());
        }
        let owner = ClassContext::from_model(model);
        let original_len = model.methods.len();
        for index in 0..original_len {
            let should_apply = {
                let method = &model.methods[index];
                where_matcher
                    .as_ref()
                    .is_none_or(|matcher| matcher.matches(method))
            };
            if should_apply {
                transform(&mut model.methods[index], &owner)?;
            }
        }
        Ok(())
    }
}

pub fn on_code<F>(
    mut transform: F,
    where_matcher: Option<MethodMatcher>,
    owner_matcher: Option<ClassMatcher>,
) -> impl ApplyClassTransform + Send
where
    F: FnMut(&mut CodeModel, &MethodContext, &ClassContext) -> Result<()> + Send + 'static,
{
    move |model: &mut ClassModel| {
        if owner_matcher
            .as_ref()
            .is_some_and(|matcher| !matcher.matches(model))
        {
            return Ok(());
        }
        let owner = ClassContext::from_model(model);
        let original_len = model.methods.len();
        for index in 0..original_len {
            let method_snapshot = MethodContext::from_method(&model.methods[index]);
            let should_apply = where_matcher
                .as_ref()
                .is_none_or(|matcher| matcher.matches(&model.methods[index]));
            if !should_apply {
                continue;
            }
            let Some(code) = model.methods[index].code.as_mut() else {
                continue;
            };
            transform(code, &method_snapshot, &owner)?;
        }
        Ok(())
    }
}

pub fn all_of<T: 'static>(matchers: impl IntoIterator<Item = Matcher<T>>) -> Matcher<T> {
    let matchers = matchers.into_iter().collect::<Vec<_>>();
    let description = if matchers.is_empty() {
        "all_of()".to_owned()
    } else {
        matchers
            .iter()
            .map(|matcher| matcher.description.as_ref())
            .collect::<Vec<_>>()
            .join(" & ")
    };
    Matcher::of(
        move |value| matchers.iter().all(|matcher| matcher.matches(value)),
        description,
    )
}

pub fn any_of<T: 'static>(matchers: impl IntoIterator<Item = Matcher<T>>) -> Matcher<T> {
    let matchers = matchers.into_iter().collect::<Vec<_>>();
    let description = if matchers.is_empty() {
        "any_of()".to_owned()
    } else {
        matchers
            .iter()
            .map(|matcher| matcher.description.as_ref())
            .collect::<Vec<_>>()
            .join(" | ")
    };
    Matcher::of(
        move |value| matchers.iter().any(|matcher| matcher.matches(value)),
        description,
    )
}

pub fn not_<T: 'static>(matcher: Matcher<T>) -> Matcher<T> {
    !matcher
}

pub fn class_named(name: impl Into<String>) -> ClassMatcher {
    let name = name.into();
    let expected = name.clone();
    Matcher::of(
        move |model: &ClassModel| model.name == expected,
        format!("class_named({name:?})"),
    )
}

pub fn class_name_matches(pattern: &str) -> std::result::Result<ClassMatcher, regex::Error> {
    let regex = Regex::new(pattern)?;
    let description = format!("class_name_matches({pattern:?})");
    Ok(Matcher::of(
        move |model: &ClassModel| regex.is_match(&model.name),
        description,
    ))
}

pub fn class_access(flags: ClassAccessFlags) -> ClassMatcher {
    Matcher::of(
        move |model: &ClassModel| model.access_flags.contains(flags),
        format!("class_access({flags:?})"),
    )
}

pub fn class_access_any(flags: ClassAccessFlags) -> ClassMatcher {
    Matcher::of(
        move |model: &ClassModel| model.access_flags.intersects(flags),
        format!("class_access_any({flags:?})"),
    )
}

pub fn class_is_public() -> ClassMatcher {
    class_access(ClassAccessFlags::PUBLIC)
}

pub fn class_is_final() -> ClassMatcher {
    class_access(ClassAccessFlags::FINAL)
}

pub fn class_is_interface() -> ClassMatcher {
    class_access(ClassAccessFlags::INTERFACE)
}

pub fn class_is_abstract() -> ClassMatcher {
    class_access(ClassAccessFlags::ABSTRACT)
}

pub fn extends(name: impl Into<String>) -> ClassMatcher {
    let name = name.into();
    let expected = name.clone();
    Matcher::of(
        move |model: &ClassModel| model.super_name.as_deref() == Some(expected.as_str()),
        format!("extends({name:?})"),
    )
}

pub fn implements(name: impl Into<String>) -> ClassMatcher {
    let name = name.into();
    let expected = name.clone();
    Matcher::of(
        move |model: &ClassModel| model.interfaces.iter().any(|entry| entry == &expected),
        format!("implements({name:?})"),
    )
}

pub fn field_named(name: impl Into<String>) -> FieldMatcher {
    let name = name.into();
    let expected = name.clone();
    Matcher::of(
        move |field: &FieldModel| field.name == expected,
        format!("field_named({name:?})"),
    )
}

pub fn field_name_matches(pattern: &str) -> std::result::Result<FieldMatcher, regex::Error> {
    let regex = Regex::new(pattern)?;
    let description = format!("field_name_matches({pattern:?})");
    Ok(Matcher::of(
        move |field: &FieldModel| regex.is_match(&field.name),
        description,
    ))
}

pub fn field_descriptor(descriptor: impl Into<String>) -> FieldMatcher {
    let descriptor = descriptor.into();
    let expected = descriptor.clone();
    Matcher::of(
        move |field: &FieldModel| field.descriptor == expected,
        format!("field_descriptor({descriptor:?})"),
    )
}

pub fn field_access(flags: FieldAccessFlags) -> FieldMatcher {
    Matcher::of(
        move |field: &FieldModel| field.access_flags.contains(flags),
        format!("field_access({flags:?})"),
    )
}

pub fn field_access_any(flags: FieldAccessFlags) -> FieldMatcher {
    Matcher::of(
        move |field: &FieldModel| field.access_flags.intersects(flags),
        format!("field_access_any({flags:?})"),
    )
}

pub fn field_is_public() -> FieldMatcher {
    field_access(FieldAccessFlags::PUBLIC)
}

pub fn field_is_private() -> FieldMatcher {
    field_access(FieldAccessFlags::PRIVATE)
}

pub fn field_is_protected() -> FieldMatcher {
    field_access(FieldAccessFlags::PROTECTED)
}

pub fn field_is_static() -> FieldMatcher {
    field_access(FieldAccessFlags::STATIC)
}

pub fn field_is_final() -> FieldMatcher {
    field_access(FieldAccessFlags::FINAL)
}

pub fn method_named(name: impl Into<String>) -> MethodMatcher {
    let name = name.into();
    let expected = name.clone();
    Matcher::of(
        move |method: &MethodModel| method.name == expected,
        format!("method_named({name:?})"),
    )
}

pub fn method_name_matches(pattern: &str) -> std::result::Result<MethodMatcher, regex::Error> {
    let regex = Regex::new(pattern)?;
    let description = format!("method_name_matches({pattern:?})");
    Ok(Matcher::of(
        move |method: &MethodModel| regex.is_match(&method.name),
        description,
    ))
}

pub fn method_descriptor(descriptor: impl Into<String>) -> MethodMatcher {
    let descriptor = descriptor.into();
    let expected = descriptor.clone();
    Matcher::of(
        move |method: &MethodModel| method.descriptor == expected,
        format!("method_descriptor({descriptor:?})"),
    )
}

pub fn method_access(flags: MethodAccessFlags) -> MethodMatcher {
    Matcher::of(
        move |method: &MethodModel| method.access_flags.contains(flags),
        format!("method_access({flags:?})"),
    )
}

pub fn method_access_any(flags: MethodAccessFlags) -> MethodMatcher {
    Matcher::of(
        move |method: &MethodModel| method.access_flags.intersects(flags),
        format!("method_access_any({flags:?})"),
    )
}

pub fn method_is_public() -> MethodMatcher {
    method_access(MethodAccessFlags::PUBLIC)
}

pub fn method_is_private() -> MethodMatcher {
    method_access(MethodAccessFlags::PRIVATE)
}

pub fn method_is_protected() -> MethodMatcher {
    method_access(MethodAccessFlags::PROTECTED)
}

pub fn method_is_static() -> MethodMatcher {
    method_access(MethodAccessFlags::STATIC)
}

pub fn method_is_final() -> MethodMatcher {
    method_access(MethodAccessFlags::FINAL)
}

pub fn method_is_synchronized() -> MethodMatcher {
    method_access(MethodAccessFlags::SYNCHRONIZED)
}

pub fn method_is_bridge() -> MethodMatcher {
    method_access(MethodAccessFlags::BRIDGE)
}

pub fn method_is_varargs() -> MethodMatcher {
    method_access(MethodAccessFlags::VARARGS)
}

pub fn method_is_native() -> MethodMatcher {
    method_access(MethodAccessFlags::NATIVE)
}

pub fn method_is_abstract() -> MethodMatcher {
    method_access(MethodAccessFlags::ABSTRACT)
}

pub fn method_is_strict() -> MethodMatcher {
    method_access(MethodAccessFlags::STRICT)
}

pub fn method_is_synthetic() -> MethodMatcher {
    method_access(MethodAccessFlags::SYNTHETIC)
}

pub fn has_code() -> MethodMatcher {
    Matcher::of(|method: &MethodModel| method.code.is_some(), "has_code()")
}

pub fn is_constructor() -> MethodMatcher {
    method_named("<init>")
}

pub fn is_static_initializer() -> MethodMatcher {
    method_named("<clinit>")
}

pub fn insn_opcode(opcode: u8) -> InsnMatcher {
    Matcher::of(
        move |item: &CodeItem| insn_matches_opcode(item, opcode),
        format!("insn_opcode(0x{opcode:02X})"),
    )
}

pub fn insn_opcode_any(opcodes: &[u8]) -> InsnMatcher {
    let opcodes = opcodes.to_vec();
    let description = format!("insn_opcode_any({opcodes:?})");
    Matcher::of(
        move |item: &CodeItem| {
            opcodes
                .iter()
                .any(|opcode| insn_matches_opcode(item, *opcode))
        },
        description,
    )
}

pub fn insn_is_field_access() -> InsnMatcher {
    Matcher::of(
        |item: &CodeItem| matches!(item, CodeItem::Field(_)),
        "insn_is_field_access()",
    )
}

pub fn insn_is_method_call() -> InsnMatcher {
    Matcher::of(
        |item: &CodeItem| {
            matches!(
                item,
                CodeItem::Method(_) | CodeItem::InterfaceMethod(_) | CodeItem::InvokeDynamic(_)
            )
        },
        "insn_is_method_call()",
    )
}

pub fn insn_is_branch() -> InsnMatcher {
    Matcher::of(
        |item: &CodeItem| {
            matches!(
                item,
                CodeItem::Branch(_) | CodeItem::LookupSwitch(_) | CodeItem::TableSwitch(_)
            )
        },
        "insn_is_branch()",
    )
}

pub fn insn_is_return() -> InsnMatcher {
    Matcher::of(
        |item: &CodeItem| match item {
            CodeItem::Raw(insn) => matches!(insn.opcode(), 0xAC..=0xB1),
            _ => false,
        },
        "insn_is_return()",
    )
}

pub fn insn_is_label() -> InsnMatcher {
    Matcher::of(
        |item: &CodeItem| matches!(item, CodeItem::Label(_)),
        "insn_is_label()",
    )
}

pub fn insn_is_ldc() -> InsnMatcher {
    Matcher::of(
        |item: &CodeItem| matches!(item, CodeItem::Ldc(_)),
        "insn_is_ldc()",
    )
}

pub fn insn_is_var() -> InsnMatcher {
    Matcher::of(
        |item: &CodeItem| matches!(item, CodeItem::Var(_)),
        "insn_is_var()",
    )
}

pub fn insn_field_owner(owner: &str) -> InsnMatcher {
    let owner = owner.to_owned();
    let expected = owner.clone();
    Matcher::of(
        move |item: &CodeItem| matches!(item, CodeItem::Field(insn) if insn.owner == expected),
        format!("insn_field_owner({owner:?})"),
    )
}

pub fn insn_field_named(name: &str) -> InsnMatcher {
    let name = name.to_owned();
    let expected = name.clone();
    Matcher::of(
        move |item: &CodeItem| matches!(item, CodeItem::Field(insn) if insn.name == expected),
        format!("insn_field_named({name:?})"),
    )
}

pub fn insn_field_descriptor(descriptor: &str) -> InsnMatcher {
    let descriptor = descriptor.to_owned();
    let expected = descriptor.clone();
    Matcher::of(
        move |item: &CodeItem| matches!(item, CodeItem::Field(insn) if insn.descriptor == expected),
        format!("insn_field_descriptor({descriptor:?})"),
    )
}

pub fn insn_field(owner: &str, name: &str, descriptor: &str) -> InsnMatcher {
    let owner = owner.to_owned();
    let name = name.to_owned();
    let descriptor = descriptor.to_owned();
    let expected_owner = owner.clone();
    let expected_name = name.clone();
    let expected_descriptor = descriptor.clone();
    Matcher::of(
        move |item: &CodeItem| {
            matches!(
                item,
                CodeItem::Field(insn)
                    if insn.owner == expected_owner
                        && insn.name == expected_name
                        && insn.descriptor == expected_descriptor
            )
        },
        format!("insn_field({owner:?}, {name:?}, {descriptor:?})"),
    )
}

pub fn insn_method_owner(owner: &str) -> InsnMatcher {
    let owner = owner.to_owned();
    let expected = owner.clone();
    Matcher::of(
        move |item: &CodeItem| match item {
            CodeItem::Method(insn) => insn.owner == expected,
            CodeItem::InterfaceMethod(insn) => insn.owner == expected,
            _ => false,
        },
        format!("insn_method_owner({owner:?})"),
    )
}

pub fn insn_method_named(name: &str) -> InsnMatcher {
    let name = name.to_owned();
    let expected = name.clone();
    Matcher::of(
        move |item: &CodeItem| match item {
            CodeItem::Method(insn) => insn.name == expected,
            CodeItem::InterfaceMethod(insn) => insn.name == expected,
            CodeItem::InvokeDynamic(insn) => insn.name == expected,
            _ => false,
        },
        format!("insn_method_named({name:?})"),
    )
}

pub fn insn_method_descriptor(descriptor: &str) -> InsnMatcher {
    let descriptor = descriptor.to_owned();
    let expected = descriptor.clone();
    Matcher::of(
        move |item: &CodeItem| match item {
            CodeItem::Method(insn) => insn.descriptor == expected,
            CodeItem::InterfaceMethod(insn) => insn.descriptor == expected,
            CodeItem::InvokeDynamic(insn) => insn.descriptor == expected,
            _ => false,
        },
        format!("insn_method_descriptor({descriptor:?})"),
    )
}

pub fn insn_method(owner: &str, name: &str, descriptor: &str) -> InsnMatcher {
    let owner = owner.to_owned();
    let name = name.to_owned();
    let descriptor = descriptor.to_owned();
    let expected_owner = owner.clone();
    let expected_name = name.clone();
    let expected_descriptor = descriptor.clone();
    Matcher::of(
        move |item: &CodeItem| match item {
            CodeItem::Method(insn) => {
                insn.owner == expected_owner
                    && insn.name == expected_name
                    && insn.descriptor == expected_descriptor
            }
            CodeItem::InterfaceMethod(insn) => {
                insn.owner == expected_owner
                    && insn.name == expected_name
                    && insn.descriptor == expected_descriptor
            }
            _ => false,
        },
        format!("insn_method({owner:?}, {name:?}, {descriptor:?})"),
    )
}

pub fn insn_type_descriptor(descriptor: &str) -> InsnMatcher {
    let descriptor = descriptor.to_owned();
    let expected = descriptor.clone();
    Matcher::of(
        move |item: &CodeItem| match item {
            CodeItem::Type(insn) => insn.descriptor == expected,
            CodeItem::MultiANewArray(insn) => insn.descriptor == expected,
            _ => false,
        },
        format!("insn_type_descriptor({descriptor:?})"),
    )
}

pub fn insn_ldc_string(value: &str) -> InsnMatcher {
    let value = value.to_owned();
    let expected = value.clone();
    Matcher::of(
        move |item: &CodeItem| matches!(item, CodeItem::Ldc(insn) if matches!(&insn.value, LdcValue::String(found) if found == &expected)),
        format!("insn_ldc_string({value:?})"),
    )
}

pub fn insn_var_slot(slot: u16) -> InsnMatcher {
    Matcher::of(
        move |item: &CodeItem| matches!(item, CodeItem::Var(insn) if insn.slot == slot),
        format!("insn_var_slot({slot})"),
    )
}

pub fn insn_field_owner_matches(pattern: &str) -> std::result::Result<InsnMatcher, regex::Error> {
    let regex = Regex::new(&format!("^(?:{pattern})$"))?;
    let description = format!("insn_field_owner_matches({pattern:?})");
    Ok(Matcher::of(
        move |item: &CodeItem| matches!(item, CodeItem::Field(insn) if regex.is_match(&insn.owner)),
        description,
    ))
}

pub fn insn_method_owner_matches(pattern: &str) -> std::result::Result<InsnMatcher, regex::Error> {
    let regex = Regex::new(&format!("^(?:{pattern})$"))?;
    let description = format!("insn_method_owner_matches({pattern:?})");
    Ok(Matcher::of(
        move |item: &CodeItem| match item {
            CodeItem::Method(insn) => regex.is_match(&insn.owner),
            CodeItem::InterfaceMethod(insn) => regex.is_match(&insn.owner),
            _ => false,
        },
        description,
    ))
}

pub fn insn_method_name_matches(pattern: &str) -> std::result::Result<InsnMatcher, regex::Error> {
    let regex = Regex::new(&format!("^(?:{pattern})$"))?;
    let description = format!("insn_method_name_matches({pattern:?})");
    Ok(Matcher::of(
        move |item: &CodeItem| match item {
            CodeItem::Method(insn) => regex.is_match(&insn.name),
            CodeItem::InterfaceMethod(insn) => regex.is_match(&insn.name),
            CodeItem::InvokeDynamic(insn) => regex.is_match(&insn.name),
            _ => false,
        },
        description,
    ))
}

fn parenthesize_description(description: &str) -> String {
    if description.starts_with('(') && description.ends_with(')') {
        description.to_owned()
    } else {
        format!("({description})")
    }
}
