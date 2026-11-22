use super::AnalysisError;
use crate::constants::{ClassAccessFlags, MethodAccessFlags};
use crate::indexes::{ClassIndex, Utf8Index};
use crate::model::{ClassModel, MethodModel};
use crate::raw::ClassFile;
use crate::raw::ConstantPoolEntry;
use crate::{Result as EngineResult, modified_utf8::decode_modified_utf8};
use std::collections::{HashMap, HashSet};

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
        let super_name = if classfile.super_class.value() == 0 {
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
        if let Some(resolved) = self.classes.get(class_name) {
            return Some(resolved.clone());
        }
        if class_name == JAVA_LANG_OBJECT {
            return Some(ResolvedClass {
                name: JAVA_LANG_OBJECT.to_owned(),
                super_name: None,
                interfaces: Vec::new(),
                access_flags: ClassAccessFlags::PUBLIC | ClassAccessFlags::SUPER,
                methods: Vec::new(),
            });
        }
        None
    }
}

pub fn iter_superclasses(
    resolver: &dyn ClassResolver,
    class_name: &str,
) -> Result<Vec<ResolvedClass>, AnalysisError> {
    let mut out = Vec::new();
    let mut seen = HashSet::from([class_name.to_owned()]);
    let mut path = vec![class_name.to_owned()];
    let mut current = resolve_class(resolver, class_name)?;
    while let Some(super_name) = current.super_name.clone() {
        if !seen.insert(super_name.clone()) {
            path.push(super_name);
            return Err(AnalysisError::HierarchyCycle { cycle: path });
        }
        path.push(super_name.clone());
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
    fn dfs_supertype(
        resolver: &dyn ClassResolver,
        name: &str,
        visiting: &mut HashSet<String>,
        visited: &mut HashSet<String>,
        path: &mut Vec<String>,
        out: &mut Vec<ResolvedClass>,
    ) -> Result<(), AnalysisError> {
        if visiting.contains(name) {
            let mut cycle = path.clone();
            cycle.push(name.to_owned());
            return Err(AnalysisError::HierarchyCycle { cycle });
        }
        if visited.contains(name) {
            return Ok(());
        }
        let resolved = resolve_class(resolver, name)?;
        visiting.insert(name.to_owned());
        path.push(name.to_owned());
        if let Some(super_name) = resolved.super_name.as_deref() {
            dfs_supertype(resolver, super_name, visiting, visited, path, out)?;
        } else if resolved.name != JAVA_LANG_OBJECT {
            dfs_supertype(resolver, JAVA_LANG_OBJECT, visiting, visited, path, out)?;
        }
        for interface in &resolved.interfaces {
            dfs_supertype(resolver, interface, visiting, visited, path, out)?;
        }
        path.pop();
        visiting.remove(name);
        visited.insert(name.to_owned());
        out.push(resolved);
        Ok(())
    }

    let root = resolve_class(resolver, class_name)?;
    let mut visiting = HashSet::from([class_name.to_owned()]);
    let mut visited = HashSet::new();
    let mut path = vec![class_name.to_owned()];
    let mut out = Vec::new();
    if let Some(super_name) = root.super_name.as_deref() {
        dfs_supertype(
            resolver,
            super_name,
            &mut visiting,
            &mut visited,
            &mut path,
            &mut out,
        )?;
    } else if root.name != JAVA_LANG_OBJECT {
        dfs_supertype(
            resolver,
            JAVA_LANG_OBJECT,
            &mut visiting,
            &mut visited,
            &mut path,
            &mut out,
        )?;
    }
    for interface in &root.interfaces {
        dfs_supertype(
            resolver,
            interface,
            &mut visiting,
            &mut visited,
            &mut path,
            &mut out,
        )?;
    }
    Ok(out)
}

fn peel_array_component_reference(component: &str) -> Option<&str> {
    if component.starts_with('[') {
        Some(component)
    } else {
        component
            .strip_prefix('L')
            .and_then(|name| name.strip_suffix(';'))
    }
}

pub fn is_subtype(
    resolver: &dyn ClassResolver,
    child_name: &str,
    target_name: &str,
) -> Result<bool, AnalysisError> {
    let mut child = child_name;
    let mut target = target_name;
    for _ in 0..=255 {
        if child == target {
            return Ok(true);
        }
        if let Some(child_component) = child.strip_prefix('[') {
            if matches!(
                target,
                JAVA_LANG_OBJECT | "java/lang/Cloneable" | "java/io/Serializable"
            ) {
                return Ok(true);
            }
            let Some(target_component) = target.strip_prefix('[') else {
                return Ok(false);
            };
            match (
                peel_array_component_reference(child_component),
                peel_array_component_reference(target_component),
            ) {
                (Some(next_child), Some(next_target)) => {
                    child = next_child;
                    target = next_target;
                    continue;
                }
                _ => return Ok(child_component == target_component),
            }
        }
        if target.starts_with('[') {
            return Ok(false);
        }
        if target != JAVA_LANG_OBJECT
            && resolver
                .resolve_class(target)
                .is_some_and(|resolved| !resolved.is_interface())
        {
            return Ok(iter_superclasses(resolver, child)?
                .into_iter()
                .any(|resolved| resolved.name == target));
        }
        return Ok(iter_supertypes(resolver, child)?
            .into_iter()
            .any(|resolved| resolved.name == target));
    }
    Err(AnalysisError::InvalidControlFlow {
        reason: "array subtype nesting exceeds 255 dimensions".to_owned(),
    })
}

pub fn common_superclass(
    resolver: &dyn ClassResolver,
    left_name: &str,
    right_name: &str,
) -> Result<String, AnalysisError> {
    common_reference_type(Some(resolver), left_name, right_name)
}

pub(crate) fn common_reference_type(
    resolver: Option<&dyn ClassResolver>,
    left_name: &str,
    right_name: &str,
) -> Result<String, AnalysisError> {
    let mut dims = 0_usize;
    let mut left = left_name;
    let mut right = right_name;
    let base_common = loop {
        if left == right {
            break left.to_owned();
        }
        if dims > 255 {
            return Err(AnalysisError::InvalidControlFlow {
                reason: "array nesting exceeds 255 dimensions".to_owned(),
            });
        }
        if let (Some(left_component), Some(right_component)) =
            (left.strip_prefix('['), right.strip_prefix('['))
        {
            if let (Some(next_left), Some(next_right)) = (
                peel_array_component_reference(left_component),
                peel_array_component_reference(right_component),
            ) {
                dims += 1;
                left = next_left;
                right = next_right;
                continue;
            }
            break JAVA_LANG_OBJECT.to_owned();
        }
        if left.starts_with('[') || right.starts_with('[') {
            let other = if left.starts_with('[') { right } else { left };
            if matches!(other, "java/lang/Cloneable" | "java/io/Serializable") {
                break other.to_owned();
            }
            break JAVA_LANG_OBJECT.to_owned();
        }
        if left == JAVA_LANG_OBJECT || right == JAVA_LANG_OBJECT {
            break JAVA_LANG_OBJECT.to_owned();
        }
        let Some(resolver) = resolver else {
            break JAVA_LANG_OBJECT.to_owned();
        };
        let left_resolved = resolve_class(resolver, left)?;
        let right_resolved = resolve_class(resolver, right)?;
        if left_resolved.is_interface() || right_resolved.is_interface() {
            break JAVA_LANG_OBJECT.to_owned();
        }
        let mut left_ancestors = vec![left.to_owned()];
        left_ancestors.extend(
            iter_superclasses(resolver, left)?
                .into_iter()
                .map(|resolved| resolved.name),
        );
        let right_ancestors = {
            let mut names = vec![right.to_owned()];
            names.extend(
                iter_superclasses(resolver, right)?
                    .into_iter()
                    .map(|resolved| resolved.name),
            );
            names
        };
        let mut found = JAVA_LANG_OBJECT.to_owned();
        for name in left_ancestors {
            if right_ancestors.iter().any(|candidate| candidate == &name) {
                found = name;
                break;
            }
        }
        break found;
    };
    if dims == 0 {
        Ok(base_common)
    } else if base_common.starts_with('[') {
        Ok(format!("{}{base_common}", "[".repeat(dims)))
    } else {
        Ok(format!("{}L{base_common};", "[".repeat(dims)))
    }
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
    let mut overriding_packages = HashSet::from([owner_package.to_owned()]);
    let mut visited_superclasses = HashSet::new();
    let mut matches = Vec::new();

    for resolved in iter_superclasses(resolver, owner_name)? {
        visited_superclasses.insert(resolved.name.clone());
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
        let resolved_package = package_name(&resolved.name);
        let visible = candidate.access_flags.contains(MethodAccessFlags::PUBLIC)
            || candidate
                .access_flags
                .contains(MethodAccessFlags::PROTECTED)
            || overriding_packages.contains(resolved_package);
        if !visible {
            continue;
        }
        overriding_packages.insert(resolved_package.to_owned());
        matches.push(InheritedMethod {
            owner: resolved.name,
            name: candidate.name,
            descriptor: candidate.descriptor,
            access_flags: candidate.access_flags,
        });
    }

    for resolved in iter_supertypes(resolver, owner_name)? {
        if visited_superclasses.contains(&resolved.name) {
            continue;
        }
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
        let visible = candidate.access_flags.contains(MethodAccessFlags::PUBLIC)
            || candidate
                .access_flags
                .contains(MethodAccessFlags::PROTECTED)
            || package_name(&resolved.name) == owner_package;
        if !visible {
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

fn cp_utf8(classfile: &ClassFile, index: Utf8Index) -> EngineResult<String> {
    let raw = index.value();
    let entry = classfile
        .constant_pool
        .get(raw as usize)
        .and_then(Option::as_ref)
        .ok_or_else(|| {
            crate::EngineError::new(
                0,
                crate::EngineErrorKind::InvalidConstantPoolIndex { index: raw },
            )
        })?;
    match entry {
        ConstantPoolEntry::Utf8(info) => decode_modified_utf8(&info.bytes),
        _ => Err(crate::EngineError::new(
            0,
            crate::EngineErrorKind::InvalidModelState {
                reason: format!("constant-pool entry {raw} is not Utf8"),
            },
        )),
    }
}

fn cp_class_name(classfile: &ClassFile, index: ClassIndex) -> EngineResult<String> {
    let raw = index.value();
    let entry = classfile
        .constant_pool
        .get(raw as usize)
        .and_then(Option::as_ref)
        .ok_or_else(|| {
            crate::EngineError::new(
                0,
                crate::EngineErrorKind::InvalidConstantPoolIndex { index: raw },
            )
        })?;
    match entry {
        ConstantPoolEntry::Class(info) => cp_utf8(classfile, info.name_index),
        _ => Err(crate::EngineError::new(
            0,
            crate::EngineErrorKind::InvalidModelState {
                reason: format!("constant-pool entry {raw} is not Class"),
            },
        )),
    }
}
