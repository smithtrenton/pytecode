use std::collections::{HashMap, HashSet};

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyModule, PyTuple, PyType};

const JAVA_LANG_OBJECT: &str = "java/lang/Object";

const ACC_PUBLIC: u32 = 0x0001;
const ACC_PRIVATE: u32 = 0x0002;
const ACC_PROTECTED: u32 = 0x0004;
const ACC_STATIC: u32 = 0x0008;
const ACC_FINAL: u32 = 0x0010;
const ACC_SUPER: u32 = 0x0020;

const NON_OVERRIDABLE_METHOD_FLAGS: u32 = ACC_PRIVATE | ACC_STATIC | ACC_FINAL;

struct PythonHierarchyTypes {
    resolved_class: Py<PyType>,
    resolved_method: Py<PyType>,
    inherited_method: Py<PyType>,
    unresolved_class_error: Py<PyType>,
    hierarchy_cycle_error: Py<PyType>,
    class_info: Py<PyType>,
    constant_pool_builder: Py<PyType>,
}

impl PythonHierarchyTypes {
    fn load(py: Python<'_>) -> PyResult<Self> {
        let hierarchy = py.import("pytecode.analysis.hierarchy")?;
        let constant_pool = py.import("pytecode.classfile.constant_pool")?;
        let builders = py.import("pytecode.edit.constant_pool_builder")?;
        Ok(Self {
            resolved_class: hierarchy
                .getattr("ResolvedClass")?
                .cast_into::<PyType>()?
                .unbind(),
            resolved_method: hierarchy
                .getattr("ResolvedMethod")?
                .cast_into::<PyType>()?
                .unbind(),
            inherited_method: hierarchy
                .getattr("InheritedMethod")?
                .cast_into::<PyType>()?
                .unbind(),
            unresolved_class_error: hierarchy
                .getattr("UnresolvedClassError")?
                .cast_into::<PyType>()?
                .unbind(),
            hierarchy_cycle_error: hierarchy
                .getattr("HierarchyCycleError")?
                .cast_into::<PyType>()?
                .unbind(),
            class_info: constant_pool
                .getattr("ClassInfo")?
                .cast_into::<PyType>()?
                .unbind(),
            constant_pool_builder: builders
                .getattr("ConstantPoolBuilder")?
                .cast_into::<PyType>()?
                .unbind(),
        })
    }
}

#[derive(Debug)]
struct ResolvedMethodSnapshot {
    name: String,
    descriptor: String,
    access_flags_bits: u32,
    access_flags_obj: Py<PyAny>,
}

impl ResolvedMethodSnapshot {
    fn clone_ref(&self, py: Python<'_>) -> Self {
        Self {
            name: self.name.clone(),
            descriptor: self.descriptor.clone(),
            access_flags_bits: self.access_flags_bits,
            access_flags_obj: self.access_flags_obj.clone_ref(py),
        }
    }
}

#[derive(Debug)]
struct ResolvedClassSnapshot {
    py_obj: Py<PyAny>,
    name: String,
    super_name: Option<String>,
    interfaces: Vec<String>,
    methods: Vec<ResolvedMethodSnapshot>,
}

impl ResolvedClassSnapshot {
    fn clone_ref(&self, py: Python<'_>) -> Self {
        Self {
            py_obj: self.py_obj.clone_ref(py),
            name: self.name.clone(),
            super_name: self.super_name.clone(),
            interfaces: self.interfaces.clone(),
            methods: self
                .methods
                .iter()
                .map(|method| method.clone_ref(py))
                .collect(),
        }
    }
}

fn extract_flag_bits(value: &Bound<'_, PyAny>) -> PyResult<u32> {
    value
        .extract::<u32>()
        .or_else(|_| value.call_method0("__int__")?.extract::<u32>())
}

fn snapshot_method(method: &Bound<'_, PyAny>) -> PyResult<ResolvedMethodSnapshot> {
    let access_flags = method.getattr("access_flags")?;
    Ok(ResolvedMethodSnapshot {
        name: method.getattr("name")?.extract()?,
        descriptor: method.getattr("descriptor")?.extract()?,
        access_flags_bits: extract_flag_bits(&access_flags)?,
        access_flags_obj: access_flags.unbind().into_any(),
    })
}

fn snapshot_resolved_class(resolved: &Bound<'_, PyAny>) -> PyResult<ResolvedClassSnapshot> {
    let methods = resolved
        .getattr("methods")?
        .try_iter()?
        .map(|method| snapshot_method(&method?))
        .collect::<PyResult<Vec<_>>>()?;
    Ok(ResolvedClassSnapshot {
        py_obj: resolved.clone().unbind().into_any(),
        name: resolved.getattr("name")?.extract()?,
        super_name: resolved.getattr("super_name")?.extract()?,
        interfaces: resolved.getattr("interfaces")?.extract()?,
        methods,
    })
}

fn build_implicit_object(
    py: Python<'_>,
    types: &PythonHierarchyTypes,
) -> PyResult<ResolvedClassSnapshot> {
    let interfaces = PyTuple::empty(py);
    let methods = PyTuple::empty(py);
    let resolved = types.resolved_class.bind(py).call1((
        JAVA_LANG_OBJECT,
        Option::<String>::None,
        interfaces,
        ACC_PUBLIC | ACC_SUPER,
        methods,
    ))?;
    Ok(ResolvedClassSnapshot {
        py_obj: resolved.unbind().into_any(),
        name: JAVA_LANG_OBJECT.to_string(),
        super_name: None,
        interfaces: Vec::new(),
        methods: Vec::new(),
    })
}

fn unresolved_class_error(
    py: Python<'_>,
    types: &PythonHierarchyTypes,
    class_name: &str,
) -> PyResult<PyErr> {
    Ok(PyErr::from_value(
        types
            .unresolved_class_error
            .bind(py)
            .call1((class_name,))?
            .into_any(),
    ))
}

fn hierarchy_cycle_error(
    py: Python<'_>,
    types: &PythonHierarchyTypes,
    cycle: &[String],
) -> PyResult<PyErr> {
    let cycle_tuple = PyTuple::new(py, cycle.iter())?;
    Ok(PyErr::from_value(
        types
            .hierarchy_cycle_error
            .bind(py)
            .call1((cycle_tuple,))?
            .into_any(),
    ))
}

fn resolve_required(
    py: Python<'_>,
    types: &PythonHierarchyTypes,
    resolver: &Bound<'_, PyAny>,
    cache: &mut HashMap<String, ResolvedClassSnapshot>,
    class_name: &str,
) -> PyResult<ResolvedClassSnapshot> {
    if let Some(snapshot) = cache.get(class_name) {
        return Ok(snapshot.clone_ref(py));
    }

    let resolved = resolver.call_method1("resolve_class", (class_name,))?;
    if !resolved.is_none() {
        let snapshot = snapshot_resolved_class(&resolved)?;
        cache.insert(class_name.to_string(), snapshot.clone_ref(py));
        return Ok(snapshot);
    }

    if class_name == JAVA_LANG_OBJECT {
        let snapshot = build_implicit_object(py, types)?;
        cache.insert(class_name.to_string(), snapshot.clone_ref(py));
        return Ok(snapshot);
    }

    Err(unresolved_class_error(py, types, class_name)?)
}

fn collect_superclasses(
    py: Python<'_>,
    types: &PythonHierarchyTypes,
    resolver: &Bound<'_, PyAny>,
    cache: &mut HashMap<String, ResolvedClassSnapshot>,
    class_name: &str,
    include_self: bool,
) -> PyResult<Vec<ResolvedClassSnapshot>> {
    let mut current_name = if include_self {
        Some(class_name.to_string())
    } else {
        resolve_required(py, types, resolver, cache, class_name)?.super_name
    };
    let mut seen = HashSet::new();
    let mut path = vec![class_name.to_string()];
    let mut result = Vec::new();

    while let Some(name) = current_name {
        if seen.contains(&name) {
            let cycle_start = path.iter().position(|entry| entry == &name).unwrap_or(0);
            let mut cycle = path[cycle_start..].to_vec();
            cycle.push(name);
            return Err(hierarchy_cycle_error(py, types, &cycle)?);
        }

        seen.insert(name.clone());
        let current = resolve_required(py, types, resolver, cache, &name)?;
        result.push(current.clone_ref(py));
        if path.last() != Some(&current.name) {
            path.push(current.name.clone());
        }
        current_name = current.super_name.clone();
    }

    Ok(result)
}

#[allow(clippy::too_many_arguments)]
fn visit_supertypes(
    py: Python<'_>,
    types: &PythonHierarchyTypes,
    resolver: &Bound<'_, PyAny>,
    cache: &mut HashMap<String, ResolvedClassSnapshot>,
    name: String,
    stack: &mut Vec<String>,
    seen: &mut HashSet<String>,
    result: &mut Vec<ResolvedClassSnapshot>,
) -> PyResult<()> {
    if let Some(cycle_start) = stack.iter().position(|entry| entry == &name) {
        let mut cycle = stack[cycle_start..].to_vec();
        cycle.push(name);
        return Err(hierarchy_cycle_error(py, types, &cycle)?);
    }
    if seen.contains(&name) {
        return Ok(());
    }

    let resolved = resolve_required(py, types, resolver, cache, &name)?;
    seen.insert(name.clone());
    result.push(resolved.clone_ref(py));

    stack.push(name);
    if let Some(super_name) = resolved.super_name.clone() {
        visit_supertypes(py, types, resolver, cache, super_name, stack, seen, result)?;
    }
    for interface_name in &resolved.interfaces {
        visit_supertypes(
            py,
            types,
            resolver,
            cache,
            interface_name.clone(),
            stack,
            seen,
            result,
        )?;
    }
    stack.pop();
    Ok(())
}

fn collect_supertypes(
    py: Python<'_>,
    types: &PythonHierarchyTypes,
    resolver: &Bound<'_, PyAny>,
    cache: &mut HashMap<String, ResolvedClassSnapshot>,
    class_name: &str,
    include_self: bool,
) -> PyResult<Vec<ResolvedClassSnapshot>> {
    let root = resolve_required(py, types, resolver, cache, class_name)?;
    let mut seen = HashSet::new();
    let mut result = Vec::new();

    if include_self {
        visit_supertypes(
            py,
            types,
            resolver,
            cache,
            root.name.clone(),
            &mut Vec::new(),
            &mut seen,
            &mut result,
        )?;
        return Ok(result);
    }

    let mut stack = vec![root.name.clone()];
    if let Some(super_name) = root.super_name.clone() {
        visit_supertypes(
            py,
            types,
            resolver,
            cache,
            super_name,
            &mut stack,
            &mut seen,
            &mut result,
        )?;
    }
    for interface_name in &root.interfaces {
        visit_supertypes(
            py,
            types,
            resolver,
            cache,
            interface_name.clone(),
            &mut stack,
            &mut seen,
            &mut result,
        )?;
    }

    Ok(result)
}

fn package_name(class_name: &str) -> &str {
    match class_name.rfind('/') {
        Some(index) => &class_name[..index],
        None => "",
    }
}

fn can_override(
    declaring_owner: &str,
    inherited_owner: &str,
    inherited_method: &ResolvedMethodSnapshot,
) -> bool {
    if matches!(inherited_method.name.as_str(), "<init>" | "<clinit>") {
        return false;
    }
    if inherited_method.access_flags_bits & NON_OVERRIDABLE_METHOD_FLAGS != 0 {
        return false;
    }
    if inherited_method.access_flags_bits & (ACC_PUBLIC | ACC_PROTECTED) != 0 {
        return true;
    }
    package_name(declaring_owner) == package_name(inherited_owner)
}

fn find_method<'a>(
    methods: &'a [ResolvedMethodSnapshot],
    name: &str,
    descriptor: &str,
) -> Option<&'a ResolvedMethodSnapshot> {
    methods
        .iter()
        .find(|method| method.name == name && method.descriptor == descriptor)
}

fn resolve_class_name(
    py: Python<'_>,
    types: &PythonHierarchyTypes,
    cp: &Bound<'_, PyAny>,
    index: usize,
) -> PyResult<String> {
    let entry = cp.call_method1("peek", (index,))?;
    if !entry.is_instance(types.class_info.bind(py))? {
        return Err(PyValueError::new_err(format!(
            "CP index {index} is not a CONSTANT_Class: {}",
            entry.get_type().name()?
        )));
    }
    let name_index: usize = entry.getattr("name_index")?.extract()?;
    cp.call_method1("resolve_utf8", (name_index,))?.extract()
}

fn resolved_class_from_classfile_impl(
    py: Python<'_>,
    types: &PythonHierarchyTypes,
    classfile: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let constant_pool = classfile.getattr("constant_pool")?;
    let cp = types
        .constant_pool_builder
        .bind(py)
        .call_method1("from_pool", (constant_pool,))?;

    let methods = classfile
        .getattr("methods")?
        .try_iter()?
        .map(|method| {
            let method = method?;
            let name_index: usize = method.getattr("name_index")?.extract()?;
            let descriptor_index: usize = method.getattr("descriptor_index")?.extract()?;
            let access_flags = method.getattr("access_flags")?;
            types.resolved_method.bind(py).call1((
                cp.call_method1("resolve_utf8", (name_index,))?
                    .extract::<String>()?,
                cp.call_method1("resolve_utf8", (descriptor_index,))?
                    .extract::<String>()?,
                access_flags,
            ))
        })
        .collect::<PyResult<Vec<_>>>()?;
    let methods = PyTuple::new(py, methods)?;

    let interfaces = classfile
        .getattr("interfaces")?
        .try_iter()?
        .map(|index| resolve_class_name(py, types, &cp, index?.extract()?))
        .collect::<PyResult<Vec<_>>>()?;
    let interfaces = PyTuple::new(py, interfaces)?;

    let this_class: usize = classfile.getattr("this_class")?.extract()?;
    let super_class: usize = classfile.getattr("super_class")?.extract()?;
    let super_name = if super_class == 0 {
        None
    } else {
        Some(resolve_class_name(py, types, &cp, super_class)?)
    };
    let access_flags = classfile.getattr("access_flags")?;

    Ok(types
        .resolved_class
        .bind(py)
        .call1((
            resolve_class_name(py, types, &cp, this_class)?,
            super_name,
            interfaces,
            access_flags,
            methods,
        ))?
        .unbind()
        .into_any())
}

#[pyfunction]
fn resolved_class_from_classfile(
    py: Python<'_>,
    classfile: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let types = PythonHierarchyTypes::load(py)?;
    resolved_class_from_classfile_impl(py, &types, classfile)
}

#[pyfunction]
fn resolved_classes_from_classfiles(
    py: Python<'_>,
    classfiles: &Bound<'_, PyAny>,
) -> PyResult<Py<PyTuple>> {
    let types = PythonHierarchyTypes::load(py)?;
    let resolved = classfiles
        .try_iter()?
        .map(|classfile| resolved_class_from_classfile_impl(py, &types, &classfile?))
        .collect::<PyResult<Vec<_>>>()?;
    Ok(PyTuple::new(py, resolved)?.unbind())
}

#[pyfunction(signature = (resolver, class_name, *, include_self = false))]
fn iter_superclasses(
    py: Python<'_>,
    resolver: &Bound<'_, PyAny>,
    class_name: &str,
    include_self: bool,
) -> PyResult<Py<PyTuple>> {
    let types = PythonHierarchyTypes::load(py)?;
    let mut cache = HashMap::new();
    let resolved =
        collect_superclasses(py, &types, resolver, &mut cache, class_name, include_self)?
            .into_iter()
            .map(|snapshot| snapshot.py_obj)
            .collect::<Vec<_>>();
    Ok(PyTuple::new(py, resolved)?.unbind())
}

#[pyfunction(signature = (resolver, class_name, *, include_self = false))]
fn iter_supertypes(
    py: Python<'_>,
    resolver: &Bound<'_, PyAny>,
    class_name: &str,
    include_self: bool,
) -> PyResult<Py<PyTuple>> {
    let types = PythonHierarchyTypes::load(py)?;
    let mut cache = HashMap::new();
    let resolved = collect_supertypes(py, &types, resolver, &mut cache, class_name, include_self)?
        .into_iter()
        .map(|snapshot| snapshot.py_obj)
        .collect::<Vec<_>>();
    Ok(PyTuple::new(py, resolved)?.unbind())
}

#[pyfunction]
fn is_subtype(
    py: Python<'_>,
    resolver: &Bound<'_, PyAny>,
    class_name: &str,
    super_name: &str,
) -> PyResult<bool> {
    let types = PythonHierarchyTypes::load(py)?;
    let mut cache = HashMap::new();
    let _ = resolve_required(py, &types, resolver, &mut cache, super_name)?;
    let supertypes = collect_supertypes(py, &types, resolver, &mut cache, class_name, true)?;
    Ok(supertypes
        .into_iter()
        .any(|resolved| resolved.name == super_name))
}

#[pyfunction]
fn common_superclass(
    py: Python<'_>,
    resolver: &Bound<'_, PyAny>,
    left: &str,
    right: &str,
) -> PyResult<String> {
    let types = PythonHierarchyTypes::load(py)?;
    let mut cache = HashMap::new();
    let left_chain = collect_superclasses(py, &types, resolver, &mut cache, left, true)?
        .into_iter()
        .map(|resolved| resolved.name)
        .collect::<HashSet<_>>();
    for resolved in collect_superclasses(py, &types, resolver, &mut cache, right, true)? {
        if left_chain.contains(&resolved.name) {
            return Ok(resolved.name);
        }
    }
    Ok(JAVA_LANG_OBJECT.to_string())
}

#[pyfunction]
fn find_overridden_methods(
    py: Python<'_>,
    resolver: &Bound<'_, PyAny>,
    class_name: &str,
    method: &Bound<'_, PyAny>,
) -> PyResult<Py<PyTuple>> {
    let types = PythonHierarchyTypes::load(py)?;
    let mut cache = HashMap::new();
    let method = snapshot_method(method)?;
    let _ = resolve_required(py, &types, resolver, &mut cache, class_name)?;

    if matches!(method.name.as_str(), "<init>" | "<clinit>") {
        return Ok(PyTuple::empty(py).unbind());
    }
    if method.access_flags_bits & (ACC_PRIVATE | ACC_STATIC) != 0 {
        return Ok(PyTuple::empty(py).unbind());
    }

    let mut matches = Vec::new();
    for supertype in collect_supertypes(py, &types, resolver, &mut cache, class_name, false)? {
        let Some(inherited) = find_method(&supertype.methods, &method.name, &method.descriptor)
        else {
            continue;
        };
        if !can_override(class_name, &supertype.name, inherited) {
            continue;
        }
        matches.push(types.inherited_method.bind(py).call1((
            supertype.name,
            inherited.name.as_str(),
            inherited.descriptor.as_str(),
            inherited.access_flags_obj.bind(py),
        ))?);
    }

    Ok(PyTuple::new(py, matches)?.unbind())
}

pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent.py(), "hierarchy")?;
    m.add_function(wrap_pyfunction!(resolved_class_from_classfile, &m)?)?;
    m.add_function(wrap_pyfunction!(resolved_classes_from_classfiles, &m)?)?;
    m.add_function(wrap_pyfunction!(iter_superclasses, &m)?)?;
    m.add_function(wrap_pyfunction!(iter_supertypes, &m)?)?;
    m.add_function(wrap_pyfunction!(is_subtype, &m)?)?;
    m.add_function(wrap_pyfunction!(common_superclass, &m)?)?;
    m.add_function(wrap_pyfunction!(find_overridden_methods, &m)?)?;
    crate::register_submodule(parent, &m, "pytecode._rust.analysis.hierarchy")?;
    Ok(())
}
