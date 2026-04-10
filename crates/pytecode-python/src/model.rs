use pyo3::exceptions::{PyIndexError, PyRuntimeError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};
use pytecode_engine::analysis::MappingClassResolver;
use pytecode_engine::model::{
    ClassModel, CodeItem, CodeModel, ConstantPoolBuilder, DebugInfoPolicy, DebugInfoState,
    ExceptionHandler, FieldModel, FrameComputationMode, Label, LdcValue, MethodModel,
};
use pytecode_engine::write_class;
use std::cell::RefCell;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

// ---------------------------------------------------------------------------
// Helper: convert EngineError → PyErr
// ---------------------------------------------------------------------------

fn analysis_error_to_py(e: pytecode_engine::analysis::AnalysisError) -> PyErr {
    PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string())
}

type SharedClassState = Rc<RefCell<ClassModelState>>;

struct ClassModelState {
    inner: Option<ClassModel>,
    interfaces_generation: u64,
    fields_generation: u64,
    methods_generation: u64,
}

impl ClassModelState {
    fn new(inner: ClassModel) -> Self {
        Self {
            inner: Some(inner),
            interfaces_generation: 0,
            fields_generation: 0,
            methods_generation: 0,
        }
    }
}

fn dead_model_err() -> PyErr {
    PyErr::new::<PyRuntimeError, _>("RustClassModel is no longer live")
}

fn stale_ref_err(kind: &str) -> PyErr {
    PyErr::new::<PyRuntimeError, _>(format!("{kind} is stale after structural mutation"))
}

fn index_err(index: isize, len: usize) -> PyErr {
    PyErr::new::<PyIndexError, _>(format!("index {index} out of range for length {len}"))
}

fn normalize_index(index: isize, len: usize) -> PyResult<usize> {
    let normalized = if index < 0 {
        len as isize + index
    } else {
        index
    };
    if normalized < 0 || normalized >= len as isize {
        return Err(index_err(index, len));
    }
    Ok(normalized as usize)
}

fn with_live_model<R>(
    state: &SharedClassState,
    f: impl FnOnce(&ClassModelState) -> PyResult<R>,
) -> PyResult<R> {
    let borrowed = state.borrow();
    if borrowed.inner.is_none() {
        return Err(dead_model_err());
    }
    f(&borrowed)
}

fn with_live_model_mut<R>(
    state: &SharedClassState,
    f: impl FnOnce(&mut ClassModelState) -> PyResult<R>,
) -> PyResult<R> {
    let mut borrowed = state.borrow_mut();
    if borrowed.inner.is_none() {
        return Err(dead_model_err());
    }
    f(&mut borrowed)
}

fn wrap_line_number(py: Python<'_>, label: &Label, line_number: u16) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    dict.set_item("label", wrap_label(py, label)?)?;
    dict.set_item("line_number", line_number)?;
    Ok(dict.into_any().unbind())
}

fn wrap_local_variable(
    py: Python<'_>,
    start: &Label,
    end: &Label,
    name: &str,
    descriptor: &str,
    index: u16,
) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    dict.set_item("start", wrap_label(py, start)?)?;
    dict.set_item("end", wrap_label(py, end)?)?;
    dict.set_item("name", name)?;
    dict.set_item("descriptor", descriptor)?;
    dict.set_item("index", index)?;
    Ok(dict.into_any().unbind())
}

fn wrap_local_variable_type(
    py: Python<'_>,
    start: &Label,
    end: &Label,
    name: &str,
    signature: &str,
    index: u16,
) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    dict.set_item("start", wrap_label(py, start)?)?;
    dict.set_item("end", wrap_label(py, end)?)?;
    dict.set_item("name", name)?;
    dict.set_item("signature", signature)?;
    dict.set_item("index", index)?;
    Ok(dict.into_any().unbind())
}

// ---------------------------------------------------------------------------
// PyLabel
// ---------------------------------------------------------------------------

#[pyclass(name = "RustLabel")]
#[derive(Clone)]
pub struct PyLabel {
    inner: Label,
}

impl From<Label> for PyLabel {
    fn from(inner: Label) -> Self {
        Self { inner }
    }
}

impl From<PyLabel> for Label {
    fn from(py: PyLabel) -> Self {
        py.inner
    }
}

#[pymethods]
impl PyLabel {
    #[new]
    fn new() -> Self {
        Self {
            inner: Label::new(),
        }
    }

    #[staticmethod]
    fn named(name: &str) -> Self {
        Self {
            inner: Label::named(name),
        }
    }

    #[getter]
    fn name(&self) -> Option<String> {
        self.inner.name.clone()
    }

    fn __repr__(&self) -> String {
        match &self.inner.name {
            Some(name) => format!("RustLabel(name={name:?})"),
            None => "RustLabel()".to_owned(),
        }
    }

    fn __eq__(&self, other: &PyLabel) -> bool {
        self.inner == other.inner
    }

    fn __hash__(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.inner.hash(&mut hasher);
        hasher.finish()
    }
}

// ---------------------------------------------------------------------------
// PyModelExceptionHandler
// ---------------------------------------------------------------------------

#[pyclass(name = "RustExceptionHandler")]
#[derive(Clone)]
pub struct PyModelExceptionHandler {
    inner: ExceptionHandler,
}

#[pymethods]
impl PyModelExceptionHandler {
    #[getter]
    fn start(&self) -> PyLabel {
        PyLabel::from(self.inner.start.clone())
    }

    #[getter]
    fn end(&self) -> PyLabel {
        PyLabel::from(self.inner.end.clone())
    }

    #[getter]
    fn handler(&self) -> PyLabel {
        PyLabel::from(self.inner.handler.clone())
    }

    #[getter]
    fn catch_type(&self) -> Option<String> {
        self.inner.catch_type.clone()
    }
}

// ---------------------------------------------------------------------------
// CodeItem → Python dict helper
// ---------------------------------------------------------------------------

fn wrap_label(py: Python<'_>, label: &Label) -> PyResult<PyObject> {
    Ok(Py::new(py, PyLabel::from(label.clone()))?.into_any())
}

fn wrap_code_item(py: Python<'_>, item: &CodeItem) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    match item {
        CodeItem::Label(label) => {
            dict.set_item("type", "label")?;
            dict.set_item("label", wrap_label(py, label)?)?;
        }
        CodeItem::Raw(insn) => {
            dict.set_item("opcode", insn.opcode())?;
            match insn {
                pytecode_engine::raw::Instruction::Byte { value, .. } => {
                    dict.set_item("type", "byte")?;
                    dict.set_item("value", *value)?;
                }
                pytecode_engine::raw::Instruction::Short { value, .. } => {
                    dict.set_item("type", "short")?;
                    dict.set_item("value", *value)?;
                }
                pytecode_engine::raw::Instruction::NewArray(na) => {
                    dict.set_item("type", "newarray")?;
                    dict.set_item("atype", na.atype as u8)?;
                }
                _ => {
                    dict.set_item("type", "raw")?;
                }
            }
        }
        CodeItem::Field(f) => {
            dict.set_item("type", "field")?;
            dict.set_item("opcode", f.opcode)?;
            dict.set_item("owner", &f.owner)?;
            dict.set_item("name", &f.name)?;
            dict.set_item("descriptor", &f.descriptor)?;
        }
        CodeItem::Method(m) => {
            dict.set_item("type", "method")?;
            dict.set_item("opcode", m.opcode)?;
            dict.set_item("owner", &m.owner)?;
            dict.set_item("name", &m.name)?;
            dict.set_item("descriptor", &m.descriptor)?;
            dict.set_item("is_interface", m.is_interface)?;
        }
        CodeItem::InterfaceMethod(im) => {
            dict.set_item("type", "interface_method")?;
            dict.set_item("owner", &im.owner)?;
            dict.set_item("name", &im.name)?;
            dict.set_item("descriptor", &im.descriptor)?;
        }
        CodeItem::Type(t) => {
            dict.set_item("type", "type")?;
            dict.set_item("opcode", t.opcode)?;
            dict.set_item("descriptor", &t.descriptor)?;
        }
        CodeItem::Var(v) => {
            dict.set_item("type", "var")?;
            dict.set_item("opcode", v.opcode)?;
            dict.set_item("slot", v.slot)?;
        }
        CodeItem::IInc(ii) => {
            dict.set_item("type", "iinc")?;
            dict.set_item("slot", ii.slot)?;
            dict.set_item("value", ii.value)?;
        }
        CodeItem::Ldc(ldc) => {
            dict.set_item("type", "ldc")?;
            match &ldc.value {
                LdcValue::Int(v) => {
                    dict.set_item("value_type", "int")?;
                    dict.set_item("value", *v)?;
                }
                LdcValue::FloatBits(v) => {
                    dict.set_item("value_type", "float")?;
                    dict.set_item("value", *v)?;
                }
                LdcValue::Long(v) => {
                    dict.set_item("value_type", "long")?;
                    dict.set_item("value", *v)?;
                }
                LdcValue::DoubleBits(v) => {
                    dict.set_item("value_type", "double")?;
                    dict.set_item("value", *v)?;
                }
                LdcValue::String(v) => {
                    dict.set_item("value_type", "string")?;
                    dict.set_item("value", v.as_str())?;
                }
                LdcValue::Class(v) => {
                    dict.set_item("value_type", "class")?;
                    dict.set_item("value", v.as_str())?;
                }
                LdcValue::MethodType(v) => {
                    dict.set_item("value_type", "method_type")?;
                    dict.set_item("value", v.as_str())?;
                }
                LdcValue::MethodHandle(mh) => {
                    dict.set_item("value_type", "method_handle")?;
                    dict.set_item("reference_kind", mh.reference_kind)?;
                    dict.set_item("owner", &mh.owner)?;
                    dict.set_item("name", &mh.name)?;
                    dict.set_item("descriptor", &mh.descriptor)?;
                    dict.set_item("is_interface", mh.is_interface)?;
                }
                LdcValue::Dynamic(dv) => {
                    dict.set_item("value_type", "dynamic")?;
                    dict.set_item(
                        "bootstrap_method_attr_index",
                        dv.bootstrap_method_attr_index.value(),
                    )?;
                    dict.set_item("name", &dv.name)?;
                    dict.set_item("descriptor", &dv.descriptor)?;
                }
            }
        }
        CodeItem::InvokeDynamic(id) => {
            dict.set_item("type", "invokedynamic")?;
            dict.set_item(
                "bootstrap_method_attr_index",
                id.bootstrap_method_attr_index.value(),
            )?;
            dict.set_item("name", &id.name)?;
            dict.set_item("descriptor", &id.descriptor)?;
        }
        CodeItem::MultiANewArray(m) => {
            dict.set_item("type", "multianewarray")?;
            dict.set_item("descriptor", &m.descriptor)?;
            dict.set_item("dimensions", m.dimensions)?;
        }
        CodeItem::Branch(b) => {
            dict.set_item("type", "branch")?;
            dict.set_item("opcode", b.opcode)?;
            dict.set_item("target", wrap_label(py, &b.target)?)?;
        }
        CodeItem::LookupSwitch(ls) => {
            dict.set_item("type", "lookupswitch")?;
            dict.set_item("default_target", wrap_label(py, &ls.default_target)?)?;
            let pairs = ls
                .pairs
                .iter()
                .map(|(key, label)| -> PyResult<PyObject> {
                    let tuple = (key, wrap_label(py, label)?);
                    Ok(tuple.into_pyobject(py)?.into_any().unbind())
                })
                .collect::<PyResult<Vec<_>>>()?;
            dict.set_item("pairs", pairs)?;
        }
        CodeItem::TableSwitch(ts) => {
            dict.set_item("type", "tableswitch")?;
            dict.set_item("default_target", wrap_label(py, &ts.default_target)?)?;
            dict.set_item("low", ts.low)?;
            dict.set_item("high", ts.high)?;
            let targets = ts
                .targets
                .iter()
                .map(|label| wrap_label(py, label))
                .collect::<PyResult<Vec<_>>>()?;
            dict.set_item("targets", targets)?;
        }
    }
    Ok(dict.into_any().unbind())
}

// ---------------------------------------------------------------------------
// Shared bridge access
// ---------------------------------------------------------------------------

#[derive(Clone)]
enum FieldAccess {
    Shared {
        state: SharedClassState,
        index: usize,
        generation: u64,
    },
    Owned(Rc<RefCell<FieldModel>>),
}

#[derive(Clone)]
enum MethodAccess {
    Shared {
        state: SharedClassState,
        index: usize,
        generation: u64,
    },
    Owned(Rc<RefCell<MethodModel>>),
}

#[derive(Clone)]
enum CodeAccess {
    Shared {
        state: SharedClassState,
        method_index: usize,
        methods_generation: u64,
    },
    OwnedMethod(Rc<RefCell<MethodModel>>),
}

#[derive(Clone)]
enum ConstantPoolAccess {
    Owned(ConstantPoolBuilder),
    Shared(SharedClassState),
}

#[derive(Clone)]
enum AttributeOwner {
    Class(SharedClassState),
    Field {
        state: SharedClassState,
        index: usize,
        generation: u64,
    },
    Method {
        state: SharedClassState,
        index: usize,
        generation: u64,
    },
    Code {
        state: SharedClassState,
        method_index: usize,
        methods_generation: u64,
    },
    OwnedField(Rc<RefCell<FieldModel>>),
    OwnedMethod(Rc<RefCell<MethodModel>>),
}

#[derive(Clone, Copy)]
enum CodeListKind {
    Instructions,
    ExceptionHandlers,
    LineNumbers,
    LocalVariables,
    LocalVariableTypes,
}

// ---------------------------------------------------------------------------
// View objects
// ---------------------------------------------------------------------------

#[pyclass(name = "RustStringListView", unsendable)]
#[derive(Clone)]
pub struct PyStringListView {
    state: SharedClassState,
    generation: u64,
}

#[pymethods]
impl PyStringListView {
    fn __len__(&self) -> PyResult<usize> {
        with_live_model(&self.state, |model_state| {
            if model_state.interfaces_generation != self.generation {
                return Err(stale_ref_err("interface view"));
            }
            Ok(model_state.inner.as_ref().unwrap().interfaces.len())
        })
    }

    fn __getitem__(&self, index: isize) -> PyResult<String> {
        with_live_model(&self.state, |model_state| {
            if model_state.interfaces_generation != self.generation {
                return Err(stale_ref_err("interface view"));
            }
            let interfaces = &model_state.inner.as_ref().unwrap().interfaces;
            Ok(interfaces[normalize_index(index, interfaces.len())?].clone())
        })
    }

    fn to_list(&self) -> PyResult<Vec<String>> {
        with_live_model(&self.state, |model_state| {
            if model_state.interfaces_generation != self.generation {
                return Err(stale_ref_err("interface view"));
            }
            Ok(model_state.inner.as_ref().unwrap().interfaces.clone())
        })
    }
}

#[pyclass(name = "RustFieldListView", unsendable)]
#[derive(Clone)]
pub struct PyFieldListView {
    state: SharedClassState,
    generation: u64,
}

#[pymethods]
impl PyFieldListView {
    fn __len__(&self) -> PyResult<usize> {
        with_live_model(&self.state, |model_state| {
            if model_state.fields_generation != self.generation {
                return Err(stale_ref_err("field view"));
            }
            Ok(model_state.inner.as_ref().unwrap().fields.len())
        })
    }

    fn __getitem__(&self, index: isize) -> PyResult<PyFieldModel> {
        with_live_model(&self.state, |model_state| {
            if model_state.fields_generation != self.generation {
                return Err(stale_ref_err("field view"));
            }
            let fields = &model_state.inner.as_ref().unwrap().fields;
            Ok(PyFieldModel {
                access: FieldAccess::Shared {
                    state: self.state.clone(),
                    index: normalize_index(index, fields.len())?,
                    generation: self.generation,
                },
            })
        })
    }

    fn to_list(&self) -> PyResult<Vec<PyFieldModel>> {
        let len = self.__len__()?;
        (0..len)
            .map(|index| self.__getitem__(index as isize))
            .collect::<PyResult<Vec<_>>>()
    }
}

#[pyclass(name = "RustMethodListView", unsendable)]
#[derive(Clone)]
pub struct PyMethodListView {
    state: SharedClassState,
    generation: u64,
}

#[pymethods]
impl PyMethodListView {
    fn __len__(&self) -> PyResult<usize> {
        with_live_model(&self.state, |model_state| {
            if model_state.methods_generation != self.generation {
                return Err(stale_ref_err("method view"));
            }
            Ok(model_state.inner.as_ref().unwrap().methods.len())
        })
    }

    fn __getitem__(&self, index: isize) -> PyResult<PyMethodModel> {
        with_live_model(&self.state, |model_state| {
            if model_state.methods_generation != self.generation {
                return Err(stale_ref_err("method view"));
            }
            let methods = &model_state.inner.as_ref().unwrap().methods;
            Ok(PyMethodModel {
                access: MethodAccess::Shared {
                    state: self.state.clone(),
                    index: normalize_index(index, methods.len())?,
                    generation: self.generation,
                },
            })
        })
    }

    fn to_list(&self) -> PyResult<Vec<PyMethodModel>> {
        let len = self.__len__()?;
        (0..len)
            .map(|index| self.__getitem__(index as isize))
            .collect::<PyResult<Vec<_>>>()
    }
}

impl AttributeOwner {
    fn len(&self) -> PyResult<usize> {
        match self {
            AttributeOwner::Class(state) => with_live_model(state, |model_state| {
                Ok(model_state.inner.as_ref().unwrap().attributes.len())
            }),
            AttributeOwner::Field {
                state,
                index,
                generation,
            } => with_live_model(state, |model_state| {
                if model_state.fields_generation != *generation {
                    return Err(stale_ref_err("field attribute view"));
                }
                Ok(model_state.inner.as_ref().unwrap().fields[*index]
                    .attributes
                    .len())
            }),
            AttributeOwner::Method {
                state,
                index,
                generation,
            } => with_live_model(state, |model_state| {
                if model_state.methods_generation != *generation {
                    return Err(stale_ref_err("method attribute view"));
                }
                Ok(model_state.inner.as_ref().unwrap().methods[*index]
                    .attributes
                    .len())
            }),
            AttributeOwner::Code {
                state,
                method_index,
                methods_generation,
            } => with_live_model(state, |model_state| {
                if model_state.methods_generation != *methods_generation {
                    return Err(stale_ref_err("code attribute view"));
                }
                let method = &model_state.inner.as_ref().unwrap().methods[*method_index];
                let code = method
                    .code
                    .as_ref()
                    .ok_or_else(|| PyErr::new::<PyRuntimeError, _>("method has no code"))?;
                Ok(code.attributes.len())
            }),
            AttributeOwner::OwnedField(field) => Ok(field.borrow().attributes.len()),
            AttributeOwner::OwnedMethod(method) => Ok(method.borrow().attributes.len()),
        }
    }

    fn get(&self, py: Python<'_>, index: isize) -> PyResult<PyObject> {
        match self {
            AttributeOwner::Class(state) => with_live_model(state, |model_state| {
                let attrs = &model_state.inner.as_ref().unwrap().attributes;
                crate::wrap_attribute(py, &attrs[normalize_index(index, attrs.len())?])
            }),
            AttributeOwner::Field {
                state,
                index: field_index,
                generation,
            } => with_live_model(state, |model_state| {
                if model_state.fields_generation != *generation {
                    return Err(stale_ref_err("field attribute view"));
                }
                let attrs = &model_state.inner.as_ref().unwrap().fields[*field_index].attributes;
                crate::wrap_attribute(py, &attrs[normalize_index(index, attrs.len())?])
            }),
            AttributeOwner::Method {
                state,
                index: method_index,
                generation,
            } => with_live_model(state, |model_state| {
                if model_state.methods_generation != *generation {
                    return Err(stale_ref_err("method attribute view"));
                }
                let attrs = &model_state.inner.as_ref().unwrap().methods[*method_index].attributes;
                crate::wrap_attribute(py, &attrs[normalize_index(index, attrs.len())?])
            }),
            AttributeOwner::Code {
                state,
                method_index,
                methods_generation,
            } => with_live_model(state, |model_state| {
                if model_state.methods_generation != *methods_generation {
                    return Err(stale_ref_err("code attribute view"));
                }
                let method = &model_state.inner.as_ref().unwrap().methods[*method_index];
                let code = method
                    .code
                    .as_ref()
                    .ok_or_else(|| PyErr::new::<PyRuntimeError, _>("method has no code"))?;
                let attrs = &code.attributes;
                crate::wrap_attribute(py, &attrs[normalize_index(index, attrs.len())?])
            }),
            AttributeOwner::OwnedField(field) => {
                let borrowed = field.borrow();
                let attrs = &borrowed.attributes;
                crate::wrap_attribute(py, &attrs[normalize_index(index, attrs.len())?])
            }
            AttributeOwner::OwnedMethod(method) => {
                let borrowed = method.borrow();
                let attrs = &borrowed.attributes;
                crate::wrap_attribute(py, &attrs[normalize_index(index, attrs.len())?])
            }
        }
    }
}

#[pyclass(name = "RustAttributeListView", unsendable)]
#[derive(Clone)]
pub struct PyAttributeListView {
    owner: AttributeOwner,
}

#[pymethods]
impl PyAttributeListView {
    fn __len__(&self) -> PyResult<usize> {
        self.owner.len()
    }

    fn __getitem__(&self, py: Python<'_>, index: isize) -> PyResult<PyObject> {
        self.owner.get(py, index)
    }

    fn to_list(&self, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        let len = self.__len__()?;
        (0..len)
            .map(|index| self.__getitem__(py, index as isize))
            .collect::<PyResult<Vec<_>>>()
    }
}

#[pyclass(name = "RustCodeListView", unsendable)]
#[derive(Clone)]
pub struct PyCodeListView {
    access: CodeAccess,
    kind: CodeListKind,
}

#[pymethods]
impl PyCodeListView {
    fn __len__(&self) -> PyResult<usize> {
        self.with_code(|code| {
            Ok(match self.kind {
                CodeListKind::Instructions => code.instructions.len(),
                CodeListKind::ExceptionHandlers => code.exception_handlers.len(),
                CodeListKind::LineNumbers => code.line_numbers.len(),
                CodeListKind::LocalVariables => code.local_variables.len(),
                CodeListKind::LocalVariableTypes => code.local_variable_types.len(),
            })
        })
    }

    fn __getitem__(&self, py: Python<'_>, index: isize) -> PyResult<PyObject> {
        self.with_code(|code| match self.kind {
            CodeListKind::Instructions => wrap_code_item(
                py,
                &code.instructions[normalize_index(index, code.instructions.len())?],
            ),
            CodeListKind::ExceptionHandlers => {
                let item = &code.exception_handlers
                    [normalize_index(index, code.exception_handlers.len())?];
                Ok(Py::new(
                    py,
                    PyModelExceptionHandler {
                        inner: item.clone(),
                    },
                )?
                .into_any())
            }
            CodeListKind::LineNumbers => {
                let item = &code.line_numbers[normalize_index(index, code.line_numbers.len())?];
                wrap_line_number(py, &item.label, item.line_number)
            }
            CodeListKind::LocalVariables => {
                let item =
                    &code.local_variables[normalize_index(index, code.local_variables.len())?];
                wrap_local_variable(
                    py,
                    &item.start,
                    &item.end,
                    &item.name,
                    &item.descriptor,
                    item.index,
                )
            }
            CodeListKind::LocalVariableTypes => {
                let item = &code.local_variable_types
                    [normalize_index(index, code.local_variable_types.len())?];
                wrap_local_variable_type(
                    py,
                    &item.start,
                    &item.end,
                    &item.name,
                    &item.signature,
                    item.index,
                )
            }
        })
    }

    fn to_list(&self, py: Python<'_>) -> PyResult<Vec<PyObject>> {
        let len = self.__len__()?;
        (0..len)
            .map(|index| self.__getitem__(py, index as isize))
            .collect::<PyResult<Vec<_>>>()
    }
}

impl PyCodeListView {
    fn with_code<R>(&self, f: impl FnOnce(&CodeModel) -> PyResult<R>) -> PyResult<R> {
        match &self.access {
            CodeAccess::Shared {
                state,
                method_index,
                methods_generation,
            } => with_live_model(state, |model_state| {
                if model_state.methods_generation != *methods_generation {
                    return Err(stale_ref_err("code view"));
                }
                let method = &model_state.inner.as_ref().unwrap().methods[*method_index];
                let code = method
                    .code
                    .as_ref()
                    .ok_or_else(|| PyErr::new::<PyRuntimeError, _>("method has no code"))?;
                f(code)
            }),
            CodeAccess::OwnedMethod(method) => {
                let borrowed = method.borrow();
                let code = borrowed
                    .code
                    .as_ref()
                    .ok_or_else(|| PyErr::new::<PyRuntimeError, _>("method has no code"))?;
                f(code)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// PyCodeModel
// ---------------------------------------------------------------------------

#[pyclass(name = "RustCodeModel", unsendable)]
#[derive(Clone)]
pub struct PyCodeModel {
    access: CodeAccess,
}

impl PyCodeModel {
    fn with_code<R>(&self, f: impl FnOnce(&CodeModel) -> PyResult<R>) -> PyResult<R> {
        match &self.access {
            CodeAccess::Shared {
                state,
                method_index,
                methods_generation,
            } => with_live_model(state, |model_state| {
                if model_state.methods_generation != *methods_generation {
                    return Err(stale_ref_err("code ref"));
                }
                let method = &model_state.inner.as_ref().unwrap().methods[*method_index];
                let code = method
                    .code
                    .as_ref()
                    .ok_or_else(|| PyErr::new::<PyRuntimeError, _>("method has no code"))?;
                f(code)
            }),
            CodeAccess::OwnedMethod(method) => {
                let borrowed = method.borrow();
                let code = borrowed
                    .code
                    .as_ref()
                    .ok_or_else(|| PyErr::new::<PyRuntimeError, _>("method has no code"))?;
                f(code)
            }
        }
    }
}

#[pymethods]
impl PyCodeModel {
    #[getter]
    fn max_stack(&self) -> PyResult<u16> {
        self.with_code(|code| Ok(code.max_stack))
    }

    #[getter]
    fn max_locals(&self) -> PyResult<u16> {
        self.with_code(|code| Ok(code.max_locals))
    }

    #[getter]
    fn instructions(&self) -> PyCodeListView {
        PyCodeListView {
            access: self.access.clone(),
            kind: CodeListKind::Instructions,
        }
    }

    #[getter]
    fn exception_handlers(&self) -> PyCodeListView {
        PyCodeListView {
            access: self.access.clone(),
            kind: CodeListKind::ExceptionHandlers,
        }
    }

    #[getter]
    fn line_numbers(&self) -> PyCodeListView {
        PyCodeListView {
            access: self.access.clone(),
            kind: CodeListKind::LineNumbers,
        }
    }

    #[getter]
    fn local_variables(&self) -> PyCodeListView {
        PyCodeListView {
            access: self.access.clone(),
            kind: CodeListKind::LocalVariables,
        }
    }

    #[getter]
    fn local_variable_types(&self) -> PyCodeListView {
        PyCodeListView {
            access: self.access.clone(),
            kind: CodeListKind::LocalVariableTypes,
        }
    }

    #[getter]
    fn attributes(&self) -> PyAttributeListView {
        let owner = match &self.access {
            CodeAccess::Shared {
                state,
                method_index,
                methods_generation,
            } => AttributeOwner::Code {
                state: state.clone(),
                method_index: *method_index,
                methods_generation: *methods_generation,
            },
            CodeAccess::OwnedMethod(method) => AttributeOwner::OwnedMethod(method.clone()),
        };
        PyAttributeListView { owner }
    }

    #[getter]
    fn debug_info_state(&self) -> PyResult<&'static str> {
        self.with_code(|code| {
            Ok(match code.debug_info_state {
                DebugInfoState::Fresh => "fresh",
                DebugInfoState::Stale => "stale",
            })
        })
    }

    #[getter]
    fn nested_attribute_layout(&self) -> PyResult<Vec<String>> {
        self.with_code(|code| {
            Ok(code
                .nested_attribute_layout()
                .iter()
                .map(|s| s.to_string())
                .collect())
        })
    }
}

// ---------------------------------------------------------------------------
// PyFieldModel
// ---------------------------------------------------------------------------

#[pyclass(name = "RustFieldModel", unsendable)]
#[derive(Clone)]
pub struct PyFieldModel {
    access: FieldAccess,
}

impl PyFieldModel {
    fn with_field<R>(&self, f: impl FnOnce(&FieldModel) -> PyResult<R>) -> PyResult<R> {
        match &self.access {
            FieldAccess::Shared {
                state,
                index,
                generation,
            } => with_live_model(state, |model_state| {
                if model_state.fields_generation != *generation {
                    return Err(stale_ref_err("field ref"));
                }
                f(&model_state.inner.as_ref().unwrap().fields[*index])
            }),
            FieldAccess::Owned(field) => f(&field.borrow()),
        }
    }

    fn with_field_mut<R>(&self, f: impl FnOnce(&mut FieldModel) -> PyResult<R>) -> PyResult<R> {
        match &self.access {
            FieldAccess::Shared {
                state,
                index,
                generation,
            } => {
                let (state, index, generation) = (state.clone(), *index, *generation);
                with_live_model_mut(&state, |model_state| {
                    if model_state.fields_generation != generation {
                        return Err(stale_ref_err("field ref"));
                    }
                    f(&mut model_state.inner.as_mut().unwrap().fields[index])
                })
            }
            FieldAccess::Owned(field) => {
                let mut borrowed = field.borrow_mut();
                f(&mut borrowed)
            }
        }
    }

    fn clone_inner(&self) -> PyResult<FieldModel> {
        self.with_field(|field| Ok(field.clone()))
    }
}

#[pymethods]
impl PyFieldModel {
    #[new]
    fn new(name: String, descriptor: String, access_flags: u16) -> Self {
        let field = FieldModel {
            access_flags: pytecode_engine::constants::FieldAccessFlags::from_bits_truncate(
                access_flags,
            ),
            name,
            descriptor,
            attributes: Vec::new(),
        };
        Self {
            access: FieldAccess::Owned(Rc::new(RefCell::new(field))),
        }
    }

    #[getter]
    fn access_flags(&self) -> PyResult<u16> {
        self.with_field(|field| Ok(field.access_flags.bits()))
    }

    #[setter]
    fn set_access_flags(&mut self, value: u16) -> PyResult<()> {
        self.with_field_mut(|field| {
            field.access_flags =
                pytecode_engine::constants::FieldAccessFlags::from_bits_truncate(value);
            Ok(())
        })
    }

    #[getter]
    fn name(&self) -> PyResult<String> {
        self.with_field(|field| Ok(field.name.clone()))
    }

    #[setter]
    fn set_name(&mut self, value: String) -> PyResult<()> {
        self.with_field_mut(|field| {
            field.name = value;
            Ok(())
        })
    }

    #[getter]
    fn descriptor(&self) -> PyResult<String> {
        self.with_field(|field| Ok(field.descriptor.clone()))
    }

    #[setter]
    fn set_descriptor(&mut self, value: String) -> PyResult<()> {
        self.with_field_mut(|field| {
            field.descriptor = value;
            Ok(())
        })
    }

    #[getter]
    fn attributes(&self) -> PyAttributeListView {
        let owner = match &self.access {
            FieldAccess::Shared {
                state,
                index,
                generation,
            } => AttributeOwner::Field {
                state: state.clone(),
                index: *index,
                generation: *generation,
            },
            FieldAccess::Owned(field) => AttributeOwner::OwnedField(field.clone()),
        };
        PyAttributeListView { owner }
    }
}

// ---------------------------------------------------------------------------
// PyMethodModel
// ---------------------------------------------------------------------------

#[pyclass(name = "RustMethodModel", unsendable)]
#[derive(Clone)]
pub struct PyMethodModel {
    access: MethodAccess,
}

impl PyMethodModel {
    fn with_method<R>(&self, f: impl FnOnce(&MethodModel) -> PyResult<R>) -> PyResult<R> {
        match &self.access {
            MethodAccess::Shared {
                state,
                index,
                generation,
            } => with_live_model(state, |model_state| {
                if model_state.methods_generation != *generation {
                    return Err(stale_ref_err("method ref"));
                }
                f(&model_state.inner.as_ref().unwrap().methods[*index])
            }),
            MethodAccess::Owned(method) => f(&method.borrow()),
        }
    }

    fn with_method_mut<R>(&self, f: impl FnOnce(&mut MethodModel) -> PyResult<R>) -> PyResult<R> {
        match &self.access {
            MethodAccess::Shared {
                state,
                index,
                generation,
            } => {
                let (state, index, generation) = (state.clone(), *index, *generation);
                with_live_model_mut(&state, |model_state| {
                    if model_state.methods_generation != generation {
                        return Err(stale_ref_err("method ref"));
                    }
                    f(&mut model_state.inner.as_mut().unwrap().methods[index])
                })
            }
            MethodAccess::Owned(method) => {
                let mut borrowed = method.borrow_mut();
                f(&mut borrowed)
            }
        }
    }

    fn clone_inner(&self) -> PyResult<MethodModel> {
        self.with_method(|method| Ok(method.clone()))
    }
}

#[pymethods]
impl PyMethodModel {
    #[new]
    fn new(name: String, descriptor: String, access_flags: u16) -> Self {
        let method = MethodModel::new(
            pytecode_engine::constants::MethodAccessFlags::from_bits_truncate(access_flags),
            name,
            descriptor,
            None,
            Vec::new(),
        );
        Self {
            access: MethodAccess::Owned(Rc::new(RefCell::new(method))),
        }
    }

    #[getter]
    fn access_flags(&self) -> PyResult<u16> {
        self.with_method(|method| Ok(method.access_flags.bits()))
    }

    #[setter]
    fn set_access_flags(&mut self, value: u16) -> PyResult<()> {
        self.with_method_mut(|method| {
            method.access_flags =
                pytecode_engine::constants::MethodAccessFlags::from_bits_truncate(value);
            Ok(())
        })
    }

    #[getter]
    fn name(&self) -> PyResult<String> {
        self.with_method(|method| Ok(method.name.clone()))
    }

    #[setter]
    fn set_name(&mut self, value: String) -> PyResult<()> {
        self.with_method_mut(|method| {
            method.name = value;
            Ok(())
        })
    }

    #[getter]
    fn descriptor(&self) -> PyResult<String> {
        self.with_method(|method| Ok(method.descriptor.clone()))
    }

    #[setter]
    fn set_descriptor(&mut self, value: String) -> PyResult<()> {
        self.with_method_mut(|method| {
            method.descriptor = value;
            Ok(())
        })
    }

    #[getter]
    fn code(&self) -> PyResult<Option<PyCodeModel>> {
        match &self.access {
            MethodAccess::Shared {
                state,
                index,
                generation,
            } => with_live_model(state, |model_state| {
                if model_state.methods_generation != *generation {
                    return Err(stale_ref_err("method ref"));
                }
                let method = &model_state.inner.as_ref().unwrap().methods[*index];
                Ok(method.code.as_ref().map(|_| PyCodeModel {
                    access: CodeAccess::Shared {
                        state: state.clone(),
                        method_index: *index,
                        methods_generation: *generation,
                    },
                }))
            }),
            MethodAccess::Owned(method) => {
                let has_code = method.borrow().code.is_some();
                Ok(has_code.then(|| PyCodeModel {
                    access: CodeAccess::OwnedMethod(method.clone()),
                }))
            }
        }
    }

    fn set_raw_code(&mut self, max_stack: u16, max_locals: u16, code_bytes: &[u8]) -> PyResult<()> {
        let instructions = pytecode_engine::parse_instructions(code_bytes)
            .map_err(|e| PyErr::new::<PyRuntimeError, _>(e.to_string()))?;
        let mut code_model = CodeModel::new(max_stack, max_locals, DebugInfoState::Fresh);
        code_model.instructions = instructions.into_iter().map(CodeItem::Raw).collect();
        self.with_method_mut(|method| {
            method.code = Some(code_model);
            Ok(())
        })
    }

    #[getter]
    fn attributes(&self) -> PyAttributeListView {
        let owner = match &self.access {
            MethodAccess::Shared {
                state,
                index,
                generation,
            } => AttributeOwner::Method {
                state: state.clone(),
                index: *index,
                generation: *generation,
            },
            MethodAccess::Owned(method) => AttributeOwner::OwnedMethod(method.clone()),
        };
        PyAttributeListView { owner }
    }
}

// ---------------------------------------------------------------------------
// PyConstantPoolBuilder
// ---------------------------------------------------------------------------

#[pyclass(name = "RustConstantPoolBuilder", unsendable)]
#[derive(Clone)]
pub struct PyConstantPoolBuilder {
    access: ConstantPoolAccess,
}

impl PyConstantPoolBuilder {
    fn from_shared(state: SharedClassState) -> Self {
        Self {
            access: ConstantPoolAccess::Shared(state),
        }
    }

    fn with_builder<R>(&self, f: impl FnOnce(&ConstantPoolBuilder) -> PyResult<R>) -> PyResult<R> {
        match &self.access {
            ConstantPoolAccess::Owned(builder) => f(builder),
            ConstantPoolAccess::Shared(state) => with_live_model(state, |model_state| {
                f(&model_state.inner.as_ref().unwrap().constant_pool)
            }),
        }
    }

    fn with_builder_mut<R>(
        &mut self,
        f: impl FnOnce(&mut ConstantPoolBuilder) -> PyResult<R>,
    ) -> PyResult<R> {
        match &mut self.access {
            ConstantPoolAccess::Owned(builder) => f(builder),
            ConstantPoolAccess::Shared(state) => with_live_model_mut(state, |model_state| {
                f(&mut model_state.inner.as_mut().unwrap().constant_pool)
            }),
        }
    }
}

#[pymethods]
impl PyConstantPoolBuilder {
    #[new]
    fn new() -> Self {
        Self {
            access: ConstantPoolAccess::Owned(ConstantPoolBuilder::new()),
        }
    }

    fn add_utf8(&mut self, value: &str) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_utf8(value)
                .map(|idx| idx.into())
                .map_err(crate::engine_error_to_py)
        })
    }

    fn add_class(&mut self, name: &str) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_class(name)
                .map(|idx| idx.into())
                .map_err(crate::engine_error_to_py)
        })
    }

    fn add_string(&mut self, value: &str) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_string(value)
                .map(|idx| idx.into())
                .map_err(crate::engine_error_to_py)
        })
    }

    fn add_integer(&mut self, value: u32) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_integer(value)
                .map(|idx| idx.into())
                .map_err(crate::engine_error_to_py)
        })
    }

    fn add_long(&mut self, value: u64) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_long(value)
                .map(|idx| idx.into())
                .map_err(crate::engine_error_to_py)
        })
    }

    fn add_float_bits(&mut self, raw_bits: u32) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_float_bits(raw_bits)
                .map(|idx| idx.into())
                .map_err(crate::engine_error_to_py)
        })
    }

    fn add_double_bits(&mut self, raw_bits: u64) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_double_bits(raw_bits)
                .map(|idx| idx.into())
                .map_err(crate::engine_error_to_py)
        })
    }

    fn add_field_ref(&mut self, owner: &str, name: &str, descriptor: &str) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_field_ref(owner, name, descriptor)
                .map(|idx| idx.into())
                .map_err(crate::engine_error_to_py)
        })
    }

    fn add_method_ref(&mut self, owner: &str, name: &str, descriptor: &str) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_method_ref(owner, name, descriptor)
                .map(|idx| idx.into())
                .map_err(crate::engine_error_to_py)
        })
    }

    fn add_interface_method_ref(
        &mut self,
        owner: &str,
        name: &str,
        descriptor: &str,
    ) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_interface_method_ref(owner, name, descriptor)
                .map(|idx| idx.into())
                .map_err(crate::engine_error_to_py)
        })
    }

    fn add_method_type(&mut self, descriptor: &str) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_method_type(descriptor)
                .map(|idx| idx.into())
                .map_err(crate::engine_error_to_py)
        })
    }

    fn add_method_handle(&mut self, reference_kind: u8, reference_index: u16) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_method_handle(
                    reference_kind,
                    pytecode_engine::indexes::CpIndex::from(reference_index),
                )
                .map(|idx| idx.into())
                .map_err(crate::engine_error_to_py)
        })
    }

    fn add_invoke_dynamic(
        &mut self,
        bootstrap_idx: u16,
        name: &str,
        descriptor: &str,
    ) -> PyResult<u16> {
        self.with_builder_mut(|builder| {
            builder
                .add_invoke_dynamic(
                    pytecode_engine::indexes::BootstrapMethodIndex::from(bootstrap_idx),
                    name,
                    descriptor,
                )
                .map(|idx| idx.into())
                .map_err(crate::engine_error_to_py)
        })
    }

    fn resolve_utf8(&self, index: u16) -> PyResult<String> {
        self.with_builder(|builder| {
            builder
                .resolve_utf8(pytecode_engine::indexes::Utf8Index::from(index))
                .map_err(crate::engine_error_to_py)
        })
    }

    fn resolve_class_name(&self, index: u16) -> PyResult<String> {
        self.with_builder(|builder| {
            builder
                .resolve_class_name(pytecode_engine::indexes::ClassIndex::from(index))
                .map_err(crate::engine_error_to_py)
        })
    }

    fn count(&self) -> PyResult<u16> {
        self.with_builder(|builder| Ok(builder.count()))
    }

    fn len(&self) -> PyResult<usize> {
        self.with_builder(|builder| Ok(builder.len()))
    }

    fn raw_constant_pool(&self, py: Python<'_>) -> PyResult<Vec<Option<PyObject>>> {
        self.with_builder(|builder| {
            builder
                .entries()
                .iter()
                .enumerate()
                .map(|(index, entry)| match entry {
                    Some(e) => crate::constant_pool_entry_to_pyobject(py, index, e).map(Some),
                    None => Ok(None),
                })
                .collect()
        })
    }
}

// ---------------------------------------------------------------------------
// PyMappingClassResolver
// ---------------------------------------------------------------------------

#[pyclass(name = "RustMappingClassResolver")]
#[derive(Clone)]
pub struct PyMappingClassResolver {
    pub(crate) inner: MappingClassResolver,
}

#[pymethods]
impl PyMappingClassResolver {
    #[new]
    fn new() -> PyResult<Self> {
        Ok(Self {
            inner: MappingClassResolver::new(std::iter::empty()).map_err(analysis_error_to_py)?,
        })
    }

    #[staticmethod]
    fn from_bytes(class_bytes_list: Vec<Vec<u8>>) -> PyResult<Self> {
        let models: Vec<ClassModel> = class_bytes_list
            .iter()
            .map(|bytes| ClassModel::from_bytes(bytes).map_err(crate::engine_error_to_py))
            .collect::<PyResult<Vec<_>>>()?;
        let resolver = MappingClassResolver::from_models(models).map_err(analysis_error_to_py)?;
        Ok(Self { inner: resolver })
    }

    fn resolve_class(&self, py: Python<'_>, name: &str) -> PyResult<Option<PyObject>> {
        use pytecode_engine::analysis::ClassResolver;
        match self.inner.resolve_class(name) {
            Some(rc) => {
                let dict = PyDict::new(py);
                dict.set_item("name", &rc.name)?;
                dict.set_item("super_name", &rc.super_name)?;
                dict.set_item("interfaces", &rc.interfaces)?;
                dict.set_item("access_flags", rc.access_flags.bits())?;
                Ok(Some(dict.into_any().unbind()))
            }
            None => Ok(None),
        }
    }
}

// ---------------------------------------------------------------------------
// PyClassModel
// ---------------------------------------------------------------------------

fn parse_debug_info_policy(s: &str) -> PyResult<DebugInfoPolicy> {
    match s {
        "preserve" => Ok(DebugInfoPolicy::Preserve),
        "strip" => Ok(DebugInfoPolicy::Strip),
        _ => Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
            "invalid debug info policy: {s:?}, expected \"preserve\" or \"strip\""
        ))),
    }
}

#[pyclass(name = "RustClassModel", unsendable)]
#[derive(Clone)]
pub struct PyClassModel {
    state: SharedClassState,
}

impl PyClassModel {
    pub(crate) fn from_model(inner: ClassModel) -> Self {
        Self {
            state: Rc::new(RefCell::new(ClassModelState::new(inner))),
        }
    }

    pub(crate) fn with_class_model<R>(
        &self,
        f: impl FnOnce(&ClassModel) -> PyResult<R>,
    ) -> PyResult<R> {
        with_live_model(&self.state, |model_state| {
            f(model_state.inner.as_ref().unwrap())
        })
    }

    pub(crate) fn with_class_model_mut<R>(
        &mut self,
        f: impl FnOnce(&mut ClassModel) -> PyResult<R>,
    ) -> PyResult<R> {
        with_live_model_mut(&self.state, |model_state| {
            f(model_state.inner.as_mut().unwrap())
        })
    }

    pub(crate) fn take_inner(&mut self) -> PyResult<ClassModel> {
        let mut borrowed = self.state.borrow_mut();
        borrowed.inner.take().ok_or_else(dead_model_err)
    }
}

#[pymethods]
impl PyClassModel {
    // -- constructors -------------------------------------------------------

    #[staticmethod]
    fn from_bytes(data: &[u8]) -> PyResult<Self> {
        let model = ClassModel::from_bytes(data).map_err(crate::engine_error_to_py)?;
        Ok(Self::from_model(model))
    }

    // -- serialisation ------------------------------------------------------

    fn to_bytes<'py>(&self, py: Python<'py>) -> PyResult<Py<PyBytes>> {
        let bytes = with_live_model(&self.state, |model_state| {
            model_state
                .inner
                .as_ref()
                .unwrap()
                .to_bytes()
                .map_err(crate::engine_error_to_py)
        })?;
        Ok(PyBytes::new(py, &bytes).unbind())
    }

    #[pyo3(signature = (recompute_frames = false, resolver = None, debug_info = "preserve"))]
    fn to_bytes_with_options<'py>(
        &self,
        py: Python<'py>,
        recompute_frames: bool,
        resolver: Option<&PyMappingClassResolver>,
        debug_info: &str,
    ) -> PyResult<Py<PyBytes>> {
        let policy = parse_debug_info_policy(debug_info)?;
        let frame_mode = if recompute_frames {
            FrameComputationMode::Recompute
        } else {
            FrameComputationMode::Preserve
        };
        let classfile = with_live_model(&self.state, |model_state| {
            model_state
                .inner
                .as_ref()
                .unwrap()
                .to_classfile_with_options(
                    policy,
                    frame_mode,
                    resolver.map(|r| &r.inner as &dyn pytecode_engine::analysis::ClassResolver),
                )
                .map_err(crate::engine_error_to_py)
        })?;
        let bytes = write_class(&classfile).map_err(crate::engine_error_to_py)?;
        Ok(PyBytes::new(py, &bytes).unbind())
    }

    // -- getters ------------------------------------------------------------

    #[getter]
    fn entry_name(&self) -> PyResult<String> {
        with_live_model(&self.state, |model_state| {
            Ok(model_state.inner.as_ref().unwrap().entry_name.clone())
        })
    }

    #[getter]
    fn original_byte_len(&self) -> PyResult<usize> {
        with_live_model(&self.state, |model_state| {
            Ok(model_state.inner.as_ref().unwrap().original_byte_len)
        })
    }

    #[getter]
    fn version(&self) -> PyResult<(u16, u16)> {
        with_live_model(&self.state, |model_state| {
            Ok(model_state.inner.as_ref().unwrap().version)
        })
    }

    #[getter]
    fn access_flags(&self) -> PyResult<u16> {
        with_live_model(&self.state, |model_state| {
            Ok(model_state.inner.as_ref().unwrap().access_flags.bits())
        })
    }

    #[getter]
    fn name(&self) -> PyResult<String> {
        with_live_model(&self.state, |model_state| {
            Ok(model_state.inner.as_ref().unwrap().name.clone())
        })
    }

    #[getter]
    fn super_name(&self) -> PyResult<Option<String>> {
        with_live_model(&self.state, |model_state| {
            Ok(model_state.inner.as_ref().unwrap().super_name.clone())
        })
    }

    #[getter]
    fn interfaces(&self) -> PyResult<PyStringListView> {
        with_live_model(&self.state, |model_state| {
            Ok(PyStringListView {
                state: self.state.clone(),
                generation: model_state.interfaces_generation,
            })
        })
    }

    #[getter]
    fn fields(&self) -> PyResult<PyFieldListView> {
        with_live_model(&self.state, |model_state| {
            Ok(PyFieldListView {
                state: self.state.clone(),
                generation: model_state.fields_generation,
            })
        })
    }

    #[getter]
    fn methods(&self) -> PyResult<PyMethodListView> {
        with_live_model(&self.state, |model_state| {
            Ok(PyMethodListView {
                state: self.state.clone(),
                generation: model_state.methods_generation,
            })
        })
    }

    #[getter]
    fn attributes(&self) -> PyAttributeListView {
        PyAttributeListView {
            owner: AttributeOwner::Class(self.state.clone()),
        }
    }

    #[getter]
    fn constant_pool(&self) -> PyConstantPoolBuilder {
        PyConstantPoolBuilder::from_shared(self.state.clone())
    }

    #[getter]
    fn debug_info_state(&self) -> PyResult<&'static str> {
        with_live_model(&self.state, |model_state| {
            Ok(match model_state.inner.as_ref().unwrap().debug_info_state {
                DebugInfoState::Fresh => "fresh",
                DebugInfoState::Stale => "stale",
            })
        })
    }

    // -- setters ------------------------------------------------------------

    #[setter]
    fn set_name(&mut self, value: String) -> PyResult<()> {
        with_live_model_mut(&self.state, |model_state| {
            model_state.inner.as_mut().unwrap().name = value;
            Ok(())
        })
    }

    #[setter]
    fn set_super_name(&mut self, value: Option<String>) -> PyResult<()> {
        with_live_model_mut(&self.state, |model_state| {
            model_state.inner.as_mut().unwrap().super_name = value;
            Ok(())
        })
    }

    #[setter]
    fn set_interfaces(&mut self, value: Vec<String>) -> PyResult<()> {
        with_live_model_mut(&self.state, |model_state| {
            model_state.inner.as_mut().unwrap().interfaces = value;
            model_state.interfaces_generation += 1;
            Ok(())
        })
    }

    #[setter]
    fn set_access_flags(&mut self, value: u16) -> PyResult<()> {
        with_live_model_mut(&self.state, |model_state| {
            model_state.inner.as_mut().unwrap().access_flags =
                pytecode_engine::constants::ClassAccessFlags::from_bits_truncate(value);
            Ok(())
        })
    }

    #[setter]
    fn set_version(&mut self, value: (u16, u16)) -> PyResult<()> {
        with_live_model_mut(&self.state, |model_state| {
            model_state.inner.as_mut().unwrap().version = value;
            Ok(())
        })
    }

    #[setter]
    fn set_fields(&mut self, value: Vec<PyFieldModel>) -> PyResult<()> {
        let fields = value
            .into_iter()
            .map(|field| field.clone_inner())
            .collect::<PyResult<Vec<_>>>()?;
        with_live_model_mut(&self.state, |model_state| {
            model_state.inner.as_mut().unwrap().fields = fields;
            model_state.fields_generation += 1;
            Ok(())
        })
    }

    #[setter]
    fn set_methods(&mut self, value: Vec<PyMethodModel>) -> PyResult<()> {
        let methods = value
            .into_iter()
            .map(|method| method.clone_inner())
            .collect::<PyResult<Vec<_>>>()?;
        with_live_model_mut(&self.state, |model_state| {
            model_state.inner.as_mut().unwrap().methods = methods;
            model_state.methods_generation += 1;
            Ok(())
        })
    }
}

// ---------------------------------------------------------------------------
// register
// ---------------------------------------------------------------------------

pub(crate) fn register(_py: Python<'_>, module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyClassModel>()?;
    module.add_class::<PyMethodModel>()?;
    module.add_class::<PyFieldModel>()?;
    module.add_class::<PyCodeModel>()?;
    module.add_class::<PyStringListView>()?;
    module.add_class::<PyFieldListView>()?;
    module.add_class::<PyMethodListView>()?;
    module.add_class::<PyAttributeListView>()?;
    module.add_class::<PyCodeListView>()?;
    module.add_class::<PyLabel>()?;
    module.add_class::<PyConstantPoolBuilder>()?;
    module.add_class::<PyMappingClassResolver>()?;
    module.add_class::<PyModelExceptionHandler>()?;
    Ok(())
}
