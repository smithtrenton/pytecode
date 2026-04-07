use super::AnalysisError;
use crate::constants::{ClassAccessFlags, MethodAccessFlags};
use crate::model::{ClassModel, MethodModel};
use crate::raw::ClassFile;
use crate::raw::ConstantPoolEntry;
use crate::{Result as EngineResult, modified_utf8::decode_modified_utf8};
use std::collections::{HashMap, HashSet, VecDeque};

pub const JAVA_LANG_OBJECT: &str = "java/lang/Object";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedMethod {
    pub name: String,
    pub descriptor: String,
    pub access_flags: MethodAccessFlags,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedClass {
    pub name: String,
    pub super_name: Option<String>,
    pub interfaces: Vec<String>,
    pub access_flags: ClassAccessFlags,
    pub methods: Vec<ResolvedMethod>,
}

impl ResolvedClass {
    pub fn is_interface(&self) -> bool {
        self.access_flags.contains(ClassAccessFlags::INTERFACE)
    }

    pub fn find_method(&self, name: &str, descriptor: &str) -> Option<&ResolvedMethod> {
        self.methods
            .iter()
            .find(|method| method.name == name && method.descriptor == descriptor)
    }

    pub fn from_model(model: &ClassModel) -> Self {
        Self {
            name: model.name.clone(),
            super_name: model.super_name.clone(),
            interfaces: model.interfaces.clone(),
            access_flags: model.access_flags,
            methods: model
                .methods
                .iter()
                .map(ResolvedMethod::from_model)
                .collect(),
        }
    }

    pub fn from_classfile(classfile: &ClassFile) -> EngineResult<Self> {
        let name = cp_class_name(classfile, classfile.this_class)?;
        let super_name = if classfile.super_class == 0 {
            None
        } else {
            Some(cp_class_name(classfile, classfile.super_class)?)
        };
        let interfaces = classfile
            .interfaces
            .iter()
            .map(|index| cp_class_name(classfile, *index))
            .collect::<EngineResult<Vec<_>>>()?;
        let methods = classfile
            .methods
            .iter()
            .map(|method| ResolvedMethod::from_classfile(classfile, method))
            .collect::<EngineResult<Vec<_>>>()?;
        Ok(Self {
            name,
            super_name,
            interfaces,
            access_flags: classfile.access_flags,
            methods,
        })
    }
}

impl ResolvedMethod {
    pub fn from_model(method: &MethodModel) -> Self {
        Self {
            name: method.name.clone(),
            descriptor: method.descriptor.clone(),
            access_flags: method.access_flags,
        }
    }

    fn from_classfile(
        classfile: &ClassFile,
        method: &crate::raw::MethodInfo,
    ) -> EngineResult<Self> {
        Ok(Self {
            name: cp_utf8(classfile, method.name_index)?,
            descriptor: cp_utf8(classfile, method.descriptor_index)?,
            access_flags: method.access_flags,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InheritedMethod {
    pub owner: String,
    pub name: String,
    pub descriptor: String,
    pub access_flags: MethodAccessFlags,
}

pub trait ClassResolver {
    fn resolve_class(&self, class_name: &str) -> Option<ResolvedClass>;
}

#[derive(Debug, Clone, Default)]
pub struct MappingClassResolver {
    classes: HashMap<String, ResolvedClass>,
}

impl MappingClassResolver {
    pub fn new(classes: impl IntoIterator<Item = ResolvedClass>) -> Result<Self, AnalysisError> {
        let mut mapping = HashMap::new();
        for resolved in classes {
            if mapping.insert(resolved.name.clone(), resolved).is_some() {
                return Err(AnalysisError::InvalidControlFlow {
                    reason: "duplicate resolved class name".to_owned(),
                });
            }
        }
        Ok(Self { classes: mapping })
    }

    pub fn from_models(
        models: impl IntoIterator<Item = ClassModel>,
    ) -> Result<Self, AnalysisError> {
        Self::new(
            models
                .into_iter()
                .map(|model| ResolvedClass::from_model(&model)),
        )
    }

    pub fn from_model_refs<'a>(
        models: impl IntoIterator<Item = &'a ClassModel>,
    ) -> Result<Self, AnalysisError> {
        Self::new(models.into_iter().map(ResolvedClass::from_model))
    }

    pub fn from_classfiles(
        classfiles: impl IntoIterator<Item = ClassFile>,
    ) -> Result<Self, AnalysisError> {
        let mut classes = Vec::new();
        for classfile in classfiles {
            classes.push(ResolvedClass::from_classfile(&classfile).map_err(|error| {
                AnalysisError::InvalidControlFlow {
                    reason: error.to_string(),
                }
            })?);
        }
        Self::new(classes)
    }

    pub fn from_classfile_refs<'a>(
        classfiles: impl IntoIterator<Item = &'a ClassFile>,
    ) -> Result<Self, AnalysisError> {
        let mut classes = Vec::new();
        for classfile in classfiles {
            classes.push(ResolvedClass::from_classfile(classfile).map_err(|error| {
                AnalysisError::InvalidControlFlow {
                    reason: error.to_string(),
                }
            })?);
        }
        Self::new(classes)
    }
}

impl ClassResolver for MappingClassResolver {
    fn resolve_class(&self, class_name: &str) -> Option<ResolvedClass> {
        if class_name == JAVA_LANG_OBJECT {
            return Some(ResolvedClass {
                name: JAVA_LANG_OBJECT.to_owned(),
                super_name: None,
                interfaces: Vec::new(),
                access_flags: ClassAccessFlags::PUBLIC | ClassAccessFlags::SUPER,
                methods: Vec::new(),
            });
        }
        self.classes.get(class_name).cloned()
    }
}

pub fn iter_superclasses(
    resolver: &dyn ClassResolver,
    class_name: &str,
) -> Result<Vec<ResolvedClass>, AnalysisError> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut current = resolve_class(resolver, class_name)?;
    while let Some(super_name) = current.super_name.clone() {
        if !seen.insert(super_name.clone()) {
            return Err(AnalysisError::HierarchyCycle {
                cycle: seen.into_iter().collect(),
            });
        }
        let resolved = resolve_class(resolver, &super_name)?;
        out.push(resolved.clone());
        current = resolved;
    }
    if current.name != JAVA_LANG_OBJECT && seen.insert(JAVA_LANG_OBJECT.to_owned()) {
        out.push(resolve_class(resolver, JAVA_LANG_OBJECT)?);
    }
    Ok(out)
}

pub fn iter_supertypes(
    resolver: &dyn ClassResolver,
    class_name: &str,
) -> Result<Vec<ResolvedClass>, AnalysisError> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut queue = VecDeque::new();
    let resolved = resolve_class(resolver, class_name)?;
    if let Some(super_name) = resolved.super_name {
        queue.push_back(super_name);
    }
    for interface in resolved.interfaces {
        queue.push_back(interface);
    }
    while let Some(name) = queue.pop_front() {
        if !seen.insert(name.clone()) {
            continue;
        }
        let resolved = resolve_class(resolver, &name)?;
        if let Some(super_name) = resolved.super_name.clone() {
            queue.push_back(super_name);
        }
        for interface in &resolved.interfaces {
            queue.push_back(interface.clone());
        }
        out.push(resolved);
    }
    Ok(out)
}

pub fn is_subtype(
    resolver: &dyn ClassResolver,
    child_name: &str,
    target_name: &str,
) -> Result<bool, AnalysisError> {
    if child_name == target_name {
        return Ok(true);
    }
    Ok(iter_supertypes(resolver, child_name)?
        .into_iter()
        .any(|resolved| resolved.name == target_name))
}

pub fn common_superclass(
    resolver: &dyn ClassResolver,
    left_name: &str,
    right_name: &str,
) -> Result<String, AnalysisError> {
    if left_name == right_name {
        return Ok(left_name.to_owned());
    }
    if left_name.starts_with('[') || right_name.starts_with('[') {
        return Ok(JAVA_LANG_OBJECT.to_owned());
    }
    let mut left_ancestors = vec![left_name.to_owned()];
    left_ancestors.extend(
        iter_superclasses(resolver, left_name)?
            .into_iter()
            .map(|resolved| resolved.name),
    );
    let right_ancestors = {
        let mut names = vec![right_name.to_owned()];
        names.extend(
            iter_superclasses(resolver, right_name)?
                .into_iter()
                .map(|resolved| resolved.name),
        );
        names
    };
    for name in left_ancestors {
        if right_ancestors.iter().any(|candidate| candidate == &name) {
            return Ok(name);
        }
    }
    Ok(JAVA_LANG_OBJECT.to_owned())
}

pub fn find_overridden_methods(
    resolver: &dyn ClassResolver,
    owner_name: &str,
    method: &ResolvedMethod,
) -> Result<Vec<InheritedMethod>, AnalysisError> {
    if method.name == "<init>"
        || method.name == "<clinit>"
        || method.access_flags.contains(MethodAccessFlags::PRIVATE)
        || method.access_flags.contains(MethodAccessFlags::STATIC)
    {
        return Ok(Vec::new());
    }
    let owner_package = package_name(owner_name);
    let mut matches = Vec::new();
    for resolved in iter_supertypes(resolver, owner_name)? {
        let Some(candidate) = resolved
            .find_method(&method.name, &method.descriptor)
            .cloned()
        else {
            continue;
        };
        if candidate.access_flags.contains(MethodAccessFlags::PRIVATE)
            || candidate.access_flags.contains(MethodAccessFlags::STATIC)
            || candidate.access_flags.contains(MethodAccessFlags::FINAL)
        {
            continue;
        }
        if !method_visible_from_subclass(&resolved.name, &candidate, owner_package) {
            continue;
        }
        matches.push(InheritedMethod {
            owner: resolved.name,
            name: candidate.name,
            descriptor: candidate.descriptor,
            access_flags: candidate.access_flags,
        });
    }
    Ok(matches)
}

fn method_visible_from_subclass(
    owner_name: &str,
    method: &ResolvedMethod,
    subclass_package: &str,
) -> bool {
    if method.access_flags.contains(MethodAccessFlags::PUBLIC)
        || method.access_flags.contains(MethodAccessFlags::PROTECTED)
    {
        return true;
    }
    package_name(owner_name) == subclass_package
}

fn package_name(class_name: &str) -> &str {
    class_name
        .rsplit_once('/')
        .map_or("", |(package, _)| package)
}

fn resolve_class(
    resolver: &dyn ClassResolver,
    class_name: &str,
) -> Result<ResolvedClass, AnalysisError> {
    resolver
        .resolve_class(class_name)
        .ok_or_else(|| AnalysisError::UnresolvedClass {
            class_name: class_name.to_owned(),
        })
}

fn cp_utf8(classfile: &ClassFile, index: u16) -> EngineResult<String> {
    let entry = classfile
        .constant_pool
        .get(index as usize)
        .and_then(Option::as_ref)
        .ok_or_else(|| {
            crate::EngineError::new(
                0,
                crate::EngineErrorKind::InvalidConstantPoolIndex { index },
            )
        })?;
    match entry {
        ConstantPoolEntry::Utf8(info) => decode_modified_utf8(&info.bytes),
        _ => Err(crate::EngineError::new(
            0,
            crate::EngineErrorKind::InvalidModelState {
                reason: format!("constant-pool entry {index} is not Utf8"),
            },
        )),
    }
}

fn cp_class_name(classfile: &ClassFile, index: u16) -> EngineResult<String> {
    let entry = classfile
        .constant_pool
        .get(index as usize)
        .and_then(Option::as_ref)
        .ok_or_else(|| {
            crate::EngineError::new(
                0,
                crate::EngineErrorKind::InvalidConstantPoolIndex { index },
            )
        })?;
    match entry {
        ConstantPoolEntry::Class(info) => cp_utf8(classfile, info.name_index),
        _ => Err(crate::EngineError::new(
            0,
            crate::EngineErrorKind::InvalidModelState {
                reason: format!("constant-pool entry {index} is not Class"),
            },
        )),
    }
}
