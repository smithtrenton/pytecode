use pyo3::create_exception;
use pyo3::exceptions::PyOSError;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes, PyDict, PyModule, PyType};
use pytecode_engine::raw;
use pytecode_engine::raw::ClassFile;
use pytecode_engine::{parse_class, write_class};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

mod analysis;
mod archive;
mod attributes;
mod model;
mod transforms;

type PyObject = Py<PyAny>;

create_exception!(
    pytecode,
    MalformedClassException,
    pyo3::exceptions::PyException
);

pub(crate) fn engine_error_to_py(error: pytecode_engine::EngineError) -> PyErr {
    MalformedClassException::new_err(error.to_string())
}

macro_rules! wrap_pyclass {
    ($py:expr, $value:expr) => {
        Py::new($py, $value).map(|obj| obj.into_bound($py).into_any().unbind())
    };
}

macro_rules! call_attr_class {
    ($py:expr, $class_name:expr $(, $arg:expr )* $(,)?) => {{
        let module = PyModule::import($py, "pytecode.classfile.attributes")?;
        module
            .getattr($class_name)?
            .call1(($($arg,)*))
            .map(|obj| obj.unbind())
    }};
}

fn python_enum(
    py: Python<'_>,
    module_name: &str,
    class_name: &str,
    value: impl Into<i64>,
) -> PyResult<Py<PyAny>> {
    let module = PyModule::import(py, module_name)?;
    let enum_type = module.getattr(class_name)?;
    Ok(enum_type.call1((value.into(),))?.unbind())
}

fn wrap_class_access_flags(
    py: Python<'_>,
    flags: pytecode_engine::constants::ClassAccessFlags,
) -> PyResult<Py<PyAny>> {
    python_enum(
        py,
        "pytecode.classfile.constants",
        "ClassAccessFlag",
        i64::from(flags.bits()),
    )
}

fn wrap_field_access_flags(
    py: Python<'_>,
    flags: pytecode_engine::constants::FieldAccessFlags,
) -> PyResult<Py<PyAny>> {
    python_enum(
        py,
        "pytecode.classfile.constants",
        "FieldAccessFlag",
        i64::from(flags.bits()),
    )
}

fn wrap_method_access_flags(
    py: Python<'_>,
    flags: pytecode_engine::constants::MethodAccessFlags,
) -> PyResult<Py<PyAny>> {
    python_enum(
        py,
        "pytecode.classfile.constants",
        "MethodAccessFlag",
        i64::from(flags.bits()),
    )
}

fn wrap_array_type(py: Python<'_>, atype: raw::ArrayType) -> PyResult<Py<PyAny>> {
    python_enum(
        py,
        "pytecode.classfile",
        "ArrayType",
        i64::from(atype as u8),
    )
}

fn wrap_nested_class_access_flags(
    py: Python<'_>,
    flags: pytecode_engine::constants::NestedClassAccessFlag,
) -> PyResult<Py<PyAny>> {
    python_enum(
        py,
        "pytecode.classfile.constants",
        "NestedClassAccessFlag",
        i64::from(flags.bits()),
    )
}

fn wrap_method_parameter_access_flags(
    py: Python<'_>,
    flags: pytecode_engine::constants::MethodParameterAccessFlag,
) -> PyResult<Py<PyAny>> {
    python_enum(
        py,
        "pytecode.classfile.constants",
        "MethodParameterAccessFlag",
        i64::from(flags.bits()),
    )
}

fn wrap_module_access_flags(
    py: Python<'_>,
    flags: pytecode_engine::constants::ModuleAccessFlag,
) -> PyResult<Py<PyAny>> {
    python_enum(
        py,
        "pytecode.classfile.constants",
        "ModuleAccessFlag",
        i64::from(flags.bits()),
    )
}

fn wrap_module_requires_access_flags(
    py: Python<'_>,
    flags: pytecode_engine::constants::ModuleRequiresAccessFlag,
) -> PyResult<Py<PyAny>> {
    python_enum(
        py,
        "pytecode.classfile.constants",
        "ModuleRequiresAccessFlag",
        i64::from(flags.bits()),
    )
}

fn wrap_module_exports_access_flags(
    py: Python<'_>,
    flags: pytecode_engine::constants::ModuleExportsAccessFlag,
) -> PyResult<Py<PyAny>> {
    python_enum(
        py,
        "pytecode.classfile.constants",
        "ModuleExportsAccessFlag",
        i64::from(flags.bits()),
    )
}

fn wrap_module_opens_access_flags(
    py: Python<'_>,
    flags: pytecode_engine::constants::ModuleOpensAccessFlag,
) -> PyResult<Py<PyAny>> {
    python_enum(
        py,
        "pytecode.classfile.constants",
        "ModuleOpensAccessFlag",
        i64::from(flags.bits()),
    )
}

fn wrap_verification_type(
    py: Python<'_>,
    tag: pytecode_engine::constants::VerificationType,
) -> PyResult<Py<PyAny>> {
    python_enum(
        py,
        "pytecode.classfile.constants",
        "VerificationType",
        i64::from(tag as u8),
    )
}

fn wrap_target_type(
    py: Python<'_>,
    target_type: pytecode_engine::constants::TargetType,
) -> PyResult<Py<PyAny>> {
    python_enum(
        py,
        "pytecode.classfile.constants",
        "TargetType",
        i64::from(target_type as u8),
    )
}

fn wrap_type_path_kind(
    py: Python<'_>,
    kind: pytecode_engine::constants::TypePathKind,
) -> PyResult<Py<PyAny>> {
    python_enum(
        py,
        "pytecode.classfile.constants",
        "TypePathKind",
        i64::from(kind as u8),
    )
}

fn wrap_unimplemented_attr_type(py: Python<'_>) -> PyResult<Py<PyAny>> {
    Ok(PyModule::import(py, "pytecode.classfile.attributes")?
        .getattr("AttributeInfoType")?
        .getattr("UNIMPLEMENTED")?
        .unbind())
}

fn wrap_verification_type_info(
    py: Python<'_>,
    info: &raw::VerificationTypeInfo,
) -> PyResult<Py<PyAny>> {
    match info {
        raw::VerificationTypeInfo::Top => {
            let tag =
                wrap_verification_type(py, pytecode_engine::constants::VerificationType::Top)?;
            call_attr_class!(py, "TopVariableInfo", tag)
        }
        raw::VerificationTypeInfo::Integer => {
            let tag =
                wrap_verification_type(py, pytecode_engine::constants::VerificationType::Integer)?;
            call_attr_class!(py, "IntegerVariableInfo", tag)
        }
        raw::VerificationTypeInfo::Float => {
            let tag =
                wrap_verification_type(py, pytecode_engine::constants::VerificationType::Float)?;
            call_attr_class!(py, "FloatVariableInfo", tag)
        }
        raw::VerificationTypeInfo::Double => {
            let tag =
                wrap_verification_type(py, pytecode_engine::constants::VerificationType::Double)?;
            call_attr_class!(py, "DoubleVariableInfo", tag)
        }
        raw::VerificationTypeInfo::Long => {
            let tag =
                wrap_verification_type(py, pytecode_engine::constants::VerificationType::Long)?;
            call_attr_class!(py, "LongVariableInfo", tag)
        }
        raw::VerificationTypeInfo::Null => {
            let tag =
                wrap_verification_type(py, pytecode_engine::constants::VerificationType::Null)?;
            call_attr_class!(py, "NullVariableInfo", tag)
        }
        raw::VerificationTypeInfo::UninitializedThis => {
            let tag = wrap_verification_type(
                py,
                pytecode_engine::constants::VerificationType::UninitializedThis,
            )?;
            call_attr_class!(py, "UninitializedThisVariableInfo", tag)
        }
        raw::VerificationTypeInfo::Object { cpool_index } => {
            let tag =
                wrap_verification_type(py, pytecode_engine::constants::VerificationType::Object)?;
            call_attr_class!(py, "ObjectVariableInfo", tag, *cpool_index)
        }
        raw::VerificationTypeInfo::Uninitialized { offset } => {
            let tag = wrap_verification_type(
                py,
                pytecode_engine::constants::VerificationType::Uninitialized,
            )?;
            call_attr_class!(py, "UninitializedVariableInfo", tag, *offset)
        }
    }
}

fn wrap_stack_map_frame_info(
    py: Python<'_>,
    frame: &raw::StackMapFrameInfo,
) -> PyResult<Py<PyAny>> {
    match frame {
        raw::StackMapFrameInfo::Same { frame_type } => {
            call_attr_class!(py, "SameFrameInfo", *frame_type)
        }
        raw::StackMapFrameInfo::SameLocals1StackItem { frame_type, stack } => {
            let stack = wrap_verification_type_info(py, stack)?;
            call_attr_class!(py, "SameLocals1StackItemFrameInfo", *frame_type, stack)
        }
        raw::StackMapFrameInfo::SameLocals1StackItemExtended {
            frame_type,
            offset_delta,
            stack,
        } => {
            let stack = wrap_verification_type_info(py, stack)?;
            call_attr_class!(
                py,
                "SameLocals1StackItemFrameExtendedInfo",
                *frame_type,
                *offset_delta,
                stack
            )
        }
        raw::StackMapFrameInfo::Chop {
            frame_type,
            offset_delta,
        } => call_attr_class!(py, "ChopFrameInfo", *frame_type, *offset_delta),
        raw::StackMapFrameInfo::SameExtended {
            frame_type,
            offset_delta,
        } => call_attr_class!(py, "SameFrameExtendedInfo", *frame_type, *offset_delta),
        raw::StackMapFrameInfo::Append {
            frame_type,
            offset_delta,
            locals,
        } => {
            let locals = locals
                .iter()
                .map(|value| wrap_verification_type_info(py, value))
                .collect::<PyResult<Vec<_>>>()?;
            call_attr_class!(py, "AppendFrameInfo", *frame_type, *offset_delta, locals)
        }
        raw::StackMapFrameInfo::Full {
            frame_type,
            offset_delta,
            locals,
            stack,
        } => {
            let locals = locals
                .iter()
                .map(|value| wrap_verification_type_info(py, value))
                .collect::<PyResult<Vec<_>>>()?;
            let stack = stack
                .iter()
                .map(|value| wrap_verification_type_info(py, value))
                .collect::<PyResult<Vec<_>>>()?;
            call_attr_class!(
                py,
                "FullFrameInfo",
                *frame_type,
                *offset_delta,
                locals.len(),
                locals,
                stack.len(),
                stack
            )
        }
    }
}

fn wrap_line_number_info(py: Python<'_>, info: &raw::LineNumberInfo) -> PyResult<Py<PyAny>> {
    call_attr_class!(py, "LineNumberInfo", info.start_pc, info.line_number)
}

fn wrap_local_variable_info(py: Python<'_>, info: &raw::LocalVariableInfo) -> PyResult<Py<PyAny>> {
    call_attr_class!(
        py,
        "LocalVariableInfo",
        info.start_pc,
        info.length,
        info.name_index,
        info.descriptor_index,
        info.index
    )
}

fn wrap_local_variable_type_info(
    py: Python<'_>,
    info: &raw::LocalVariableTypeInfo,
) -> PyResult<Py<PyAny>> {
    call_attr_class!(
        py,
        "LocalVariableTypeInfo",
        info.start_pc,
        info.length,
        info.name_index,
        info.signature_index,
        info.index
    )
}

fn wrap_inner_class_info(py: Python<'_>, info: &raw::InnerClassInfo) -> PyResult<Py<PyAny>> {
    let access_flags = wrap_nested_class_access_flags(py, info.inner_class_access_flags)?;
    call_attr_class!(
        py,
        "InnerClassInfo",
        info.inner_class_info_index,
        info.outer_class_info_index,
        info.inner_name_index,
        access_flags
    )
}

fn wrap_const_value_info(py: Python<'_>, const_value_index: u16) -> PyResult<Py<PyAny>> {
    call_attr_class!(py, "ConstValueInfo", const_value_index)
}

fn wrap_element_value_info(py: Python<'_>, value: &raw::ElementValueInfo) -> PyResult<Py<PyAny>> {
    match value {
        raw::ElementValueInfo::Const {
            tag,
            const_value_index,
        } => {
            let tag = (*tag as u8 as char).to_string();
            let value = wrap_const_value_info(py, (*const_value_index).into())?;
            call_attr_class!(py, "ElementValueInfo", tag, value)
        }
        raw::ElementValueInfo::Enum {
            type_name_index,
            const_name_index,
        } => {
            let value = call_attr_class!(
                py,
                "EnumConstantValueInfo",
                *type_name_index,
                *const_name_index
            )?;
            call_attr_class!(py, "ElementValueInfo", "e", value)
        }
        raw::ElementValueInfo::Class { class_info_index } => {
            let value = call_attr_class!(py, "ClassInfoValueInfo", *class_info_index)?;
            call_attr_class!(py, "ElementValueInfo", "c", value)
        }
        raw::ElementValueInfo::Annotation(annotation) => {
            let value = wrap_annotation_info(py, annotation)?;
            call_attr_class!(py, "ElementValueInfo", "@", value)
        }
        raw::ElementValueInfo::Array { values } => {
            let values = values
                .iter()
                .map(|value| wrap_element_value_info(py, value))
                .collect::<PyResult<Vec<_>>>()?;
            let value = call_attr_class!(py, "ArrayValueInfo", values.len(), values)?;
            call_attr_class!(py, "ElementValueInfo", "[", value)
        }
    }
}

fn wrap_element_value_pair_info(
    py: Python<'_>,
    pair: &raw::ElementValuePairInfo,
) -> PyResult<Py<PyAny>> {
    let element_value = wrap_element_value_info(py, &pair.element_value)?;
    call_attr_class!(
        py,
        "ElementValuePairInfo",
        pair.element_name_index,
        element_value
    )
}

fn wrap_annotation_info(py: Python<'_>, info: &raw::AnnotationInfo) -> PyResult<Py<PyAny>> {
    let pairs = info
        .element_value_pairs
        .iter()
        .map(|pair| wrap_element_value_pair_info(py, pair))
        .collect::<PyResult<Vec<_>>>()?;
    call_attr_class!(py, "AnnotationInfo", info.type_index, pairs.len(), pairs)
}

fn wrap_parameter_annotation_info(
    py: Python<'_>,
    info: &raw::ParameterAnnotationInfo,
) -> PyResult<Py<PyAny>> {
    let annotations = info
        .annotations
        .iter()
        .map(|annotation| wrap_annotation_info(py, annotation))
        .collect::<PyResult<Vec<_>>>()?;
    call_attr_class!(
        py,
        "ParameterAnnotationInfo",
        annotations.len(),
        annotations
    )
}

fn wrap_table_info(py: Python<'_>, info: &raw::TableInfo) -> PyResult<Py<PyAny>> {
    call_attr_class!(py, "TableInfo", info.start_pc, info.length, info.index)
}

fn wrap_target_info(py: Python<'_>, info: &raw::TargetInfo) -> PyResult<Py<PyAny>> {
    match info {
        raw::TargetInfo::TypeParameter {
            type_parameter_index,
        } => call_attr_class!(py, "TypeParameterTargetInfo", *type_parameter_index),
        raw::TargetInfo::Supertype { supertype_index } => {
            call_attr_class!(py, "SupertypeTargetInfo", *supertype_index)
        }
        raw::TargetInfo::TypeParameterBound {
            type_parameter_index,
            bound_index,
        } => call_attr_class!(
            py,
            "TypeParameterBoundTargetInfo",
            *type_parameter_index,
            *bound_index
        ),
        raw::TargetInfo::Empty => call_attr_class!(py, "EmptyTargetInfo"),
        raw::TargetInfo::FormalParameter {
            formal_parameter_index,
        } => call_attr_class!(py, "FormalParameterTargetInfo", *formal_parameter_index),
        raw::TargetInfo::Throws { throws_type_index } => {
            call_attr_class!(py, "ThrowsTargetInfo", *throws_type_index)
        }
        raw::TargetInfo::Localvar { table } => {
            let table = table
                .iter()
                .map(|entry| wrap_table_info(py, entry))
                .collect::<PyResult<Vec<_>>>()?;
            call_attr_class!(py, "LocalvarTargetInfo", table.len(), table)
        }
        raw::TargetInfo::Catch {
            exception_table_index,
        } => call_attr_class!(py, "CatchTargetInfo", *exception_table_index),
        raw::TargetInfo::Offset { offset } => call_attr_class!(py, "OffsetTargetInfo", *offset),
        raw::TargetInfo::TypeArgument {
            offset,
            type_argument_index,
        } => call_attr_class!(py, "TypeArgumentTargetInfo", *offset, *type_argument_index),
    }
}

fn wrap_path_info(py: Python<'_>, info: &raw::PathInfo) -> PyResult<Py<PyAny>> {
    let type_path_kind = wrap_type_path_kind(py, info.type_path_kind)?;
    call_attr_class!(py, "PathInfo", type_path_kind, info.type_argument_index)
}

fn wrap_type_path_info(py: Python<'_>, info: &raw::TypePathInfo) -> PyResult<Py<PyAny>> {
    let path = info
        .path
        .iter()
        .map(|entry| wrap_path_info(py, entry))
        .collect::<PyResult<Vec<_>>>()?;
    call_attr_class!(py, "TypePathInfo", path.len(), path)
}

fn wrap_type_annotation_info(
    py: Python<'_>,
    info: &raw::TypeAnnotationInfo,
) -> PyResult<Py<PyAny>> {
    let target_type = wrap_target_type(py, info.target_type)?;
    let target_info = wrap_target_info(py, &info.target_info)?;
    let target_path = wrap_type_path_info(py, &info.target_path)?;
    let element_value_pairs = info
        .element_value_pairs
        .iter()
        .map(|pair| wrap_element_value_pair_info(py, pair))
        .collect::<PyResult<Vec<_>>>()?;
    call_attr_class!(
        py,
        "TypeAnnotationInfo",
        target_type,
        target_info,
        target_path,
        info.type_index,
        element_value_pairs.len(),
        element_value_pairs
    )
}

fn wrap_bootstrap_method_info(
    py: Python<'_>,
    info: &raw::BootstrapMethodInfo,
) -> PyResult<Py<PyAny>> {
    call_attr_class!(
        py,
        "BootstrapMethodInfo",
        info.bootstrap_method_ref,
        info.bootstrap_arguments.len(),
        info.bootstrap_arguments.clone()
    )
}

fn wrap_method_parameter_info(
    py: Python<'_>,
    info: &raw::MethodParameterInfo,
) -> PyResult<Py<PyAny>> {
    let access_flags = wrap_method_parameter_access_flags(py, info.access_flags)?;
    call_attr_class!(py, "MethodParameterInfo", info.name_index, access_flags)
}

fn wrap_requires_info(py: Python<'_>, info: &raw::RequiresInfo) -> PyResult<Py<PyAny>> {
    let requires_flag = wrap_module_requires_access_flags(py, info.requires_flags)?;
    call_attr_class!(
        py,
        "RequiresInfo",
        info.requires_index,
        requires_flag,
        info.requires_version_index
    )
}

fn wrap_export_info(py: Python<'_>, info: &raw::ExportInfo) -> PyResult<Py<PyAny>> {
    let exports_flags = wrap_module_exports_access_flags(py, info.exports_flags)?;
    call_attr_class!(
        py,
        "ExportInfo",
        info.exports_index,
        exports_flags,
        info.exports_to_index.len(),
        info.exports_to_index.clone()
    )
}

fn wrap_opens_info(py: Python<'_>, info: &raw::OpensInfo) -> PyResult<Py<PyAny>> {
    let opens_flags = wrap_module_opens_access_flags(py, info.opens_flags)?;
    call_attr_class!(
        py,
        "OpensInfo",
        info.opens_index,
        opens_flags,
        info.opens_to_index.len(),
        info.opens_to_index.clone()
    )
}

fn wrap_provides_info(py: Python<'_>, info: &raw::ProvidesInfo) -> PyResult<Py<PyAny>> {
    call_attr_class!(
        py,
        "ProvidesInfo",
        info.provides_index,
        info.provides_with_index.len(),
        info.provides_with_index.clone()
    )
}

fn wrap_unimplemented_attr(py: Python<'_>, info: &raw::UnknownAttribute) -> PyResult<Py<PyAny>> {
    let attr_type = wrap_unimplemented_attr_type(py)?;
    let info_bytes = PyBytes::new(py, &info.info).unbind();
    call_attr_class!(
        py,
        "UnimplementedAttr",
        info.attribute_name_index,
        info.attribute_length,
        info_bytes,
        attr_type
    )
}

fn wrap_record_component_attribute(
    py: Python<'_>,
    attribute: &raw::AttributeInfo,
) -> PyResult<Py<PyAny>> {
    match attribute {
        raw::AttributeInfo::Synthetic(inner) => call_attr_class!(
            py,
            "SyntheticAttr",
            inner.attribute_name_index,
            inner.attribute_length
        ),
        raw::AttributeInfo::Signature(inner) => call_attr_class!(
            py,
            "SignatureAttr",
            inner.attribute_name_index,
            inner.attribute_length,
            inner.signature_index
        ),
        raw::AttributeInfo::Deprecated(inner) => call_attr_class!(
            py,
            "DeprecatedAttr",
            inner.attribute_name_index,
            inner.attribute_length
        ),
        raw::AttributeInfo::RuntimeVisibleAnnotations(inner) => {
            let annotations = inner
                .annotations
                .iter()
                .map(|annotation| wrap_annotation_info(py, annotation))
                .collect::<PyResult<Vec<_>>>()?;
            call_attr_class!(
                py,
                "RuntimeVisibleAnnotationsAttr",
                inner.attribute_name_index,
                inner.attribute_length,
                annotations.len(),
                annotations
            )
        }
        raw::AttributeInfo::RuntimeInvisibleAnnotations(inner) => {
            let annotations = inner
                .annotations
                .iter()
                .map(|annotation| wrap_annotation_info(py, annotation))
                .collect::<PyResult<Vec<_>>>()?;
            call_attr_class!(
                py,
                "RuntimeInvisibleAnnotationsAttr",
                inner.attribute_name_index,
                inner.attribute_length,
                annotations.len(),
                annotations
            )
        }
        raw::AttributeInfo::RuntimeVisibleTypeAnnotations(inner) => {
            let annotations = inner
                .annotations
                .iter()
                .map(|annotation| wrap_type_annotation_info(py, annotation))
                .collect::<PyResult<Vec<_>>>()?;
            call_attr_class!(
                py,
                "RuntimeVisibleTypeAnnotationsAttr",
                inner.attribute_name_index,
                inner.attribute_length,
                annotations.len(),
                annotations
            )
        }
        raw::AttributeInfo::RuntimeInvisibleTypeAnnotations(inner) => {
            let annotations = inner
                .annotations
                .iter()
                .map(|annotation| wrap_type_annotation_info(py, annotation))
                .collect::<PyResult<Vec<_>>>()?;
            call_attr_class!(
                py,
                "RuntimeInvisibleTypeAnnotationsAttr",
                inner.attribute_name_index,
                inner.attribute_length,
                annotations.len(),
                annotations
            )
        }
        raw::AttributeInfo::Unknown(inner) => wrap_unimplemented_attr(py, inner),
        _ => Err(PyOSError::new_err(format!(
            "Unsupported nested record attribute for Rust bridge: {attribute:?}"
        ))),
    }
}

fn wrap_record_component_info(
    py: Python<'_>,
    info: &raw::RecordComponentInfo,
) -> PyResult<Py<PyAny>> {
    let attributes = info
        .attributes
        .iter()
        .map(|attribute| wrap_record_component_attribute(py, attribute))
        .collect::<PyResult<Vec<_>>>()?;
    call_attr_class!(
        py,
        "RecordComponentInfo",
        info.name_index,
        info.descriptor_index,
        attributes.len(),
        attributes
    )
}

fn instruction_type_value(instruction: &raw::Instruction) -> i64 {
    match instruction {
        raw::Instruction::Wide(inner) => i64::from(0xC4_u16 + u16::from(inner.opcode)),
        _ => i64::from(instruction.opcode()),
    }
}

fn wrap_instruction_type(py: Python<'_>, instruction: &raw::Instruction) -> PyResult<Py<PyAny>> {
    python_enum(
        py,
        "pytecode.classfile",
        "InsnInfoType",
        instruction_type_value(instruction),
    )
}

macro_rules! define_numeric_cp_wrapper {
    ($wrapper:ident, $pyname:literal, $inner:ty, $tag:expr, { $($field:ident : $ty:ty),* $(,)? }) => {
        #[pyclass(from_py_object, module = "pytecode._rust", name = $pyname)]
        #[derive(Clone)]
        pub struct $wrapper {
            pub(crate) index: usize,
            pub(crate) inner: $inner,
        }

        #[pymethods]
        impl $wrapper {
            #[getter]
            fn index(&self) -> usize {
                self.index
            }

            #[getter]
            fn offset(&self) -> Option<usize> {
                None
            }

            #[getter]
            fn tag(&self) -> u8 {
                $tag as u8
            }

            $(
                #[getter]
                fn $field(&self) -> $ty {
                    self.inner.$field.into()
                }
            )*
        }
    };
}

#[pyclass(from_py_object, module = "pytecode._rust", name = "Utf8Info")]
#[derive(Clone)]
pub struct PyUtf8Info {
    pub(crate) index: usize,
    pub(crate) inner: raw::Utf8Info,
}

#[pymethods]
impl PyUtf8Info {
    #[getter]
    fn index(&self) -> usize {
        self.index
    }

    #[getter]
    fn offset(&self) -> Option<usize> {
        None
    }

    #[getter]
    fn tag(&self) -> u8 {
        raw::ConstantPoolTag::Utf8 as u8
    }

    #[getter]
    fn length(&self) -> usize {
        self.inner.bytes.len()
    }

    #[getter]
    fn str_bytes<'py>(&self, py: Python<'py>) -> Py<PyBytes> {
        PyBytes::new(py, &self.inner.bytes).unbind()
    }

    fn __repr__(&self) -> String {
        format!(
            "Utf8Info(index={}, length={})",
            self.index,
            self.inner.bytes.len()
        )
    }
}

define_numeric_cp_wrapper!(
    PyIntegerInfo,
    "IntegerInfo",
    raw::IntegerInfo,
    raw::ConstantPoolTag::Integer,
    { value_bytes: u32 }
);
define_numeric_cp_wrapper!(
    PyFloatInfo,
    "FloatInfo",
    raw::FloatInfo,
    raw::ConstantPoolTag::Float,
    { value_bytes: u32 }
);
define_numeric_cp_wrapper!(
    PyLongInfo,
    "LongInfo",
    raw::LongInfo,
    raw::ConstantPoolTag::Long,
    { high_bytes: u32, low_bytes: u32 }
);
define_numeric_cp_wrapper!(
    PyDoubleInfo,
    "DoubleInfo",
    raw::DoubleInfo,
    raw::ConstantPoolTag::Double,
    { high_bytes: u32, low_bytes: u32 }
);
define_numeric_cp_wrapper!(
    PyConstantPoolClassInfo,
    "ClassInfo",
    raw::ClassInfo,
    raw::ConstantPoolTag::Class,
    { name_index: u16 }
);
define_numeric_cp_wrapper!(
    PyStringInfo,
    "StringInfo",
    raw::StringInfo,
    raw::ConstantPoolTag::String,
    { string_index: u16 }
);
define_numeric_cp_wrapper!(
    PyFieldrefInfo,
    "FieldrefInfo",
    raw::FieldRefInfo,
    raw::ConstantPoolTag::FieldRef,
    { class_index: u16, name_and_type_index: u16 }
);
define_numeric_cp_wrapper!(
    PyMethodrefInfo,
    "MethodrefInfo",
    raw::MethodRefInfo,
    raw::ConstantPoolTag::MethodRef,
    { class_index: u16, name_and_type_index: u16 }
);
define_numeric_cp_wrapper!(
    PyInterfaceMethodrefInfo,
    "InterfaceMethodrefInfo",
    raw::InterfaceMethodRefInfo,
    raw::ConstantPoolTag::InterfaceMethodRef,
    { class_index: u16, name_and_type_index: u16 }
);
define_numeric_cp_wrapper!(
    PyNameAndTypeInfo,
    "NameAndTypeInfo",
    raw::NameAndTypeInfo,
    raw::ConstantPoolTag::NameAndType,
    { name_index: u16, descriptor_index: u16 }
);
define_numeric_cp_wrapper!(
    PyMethodHandleInfo,
    "MethodHandleInfo",
    raw::MethodHandleInfo,
    raw::ConstantPoolTag::MethodHandle,
    { reference_kind: u8, reference_index: u16 }
);
define_numeric_cp_wrapper!(
    PyMethodTypeInfo,
    "MethodTypeInfo",
    raw::MethodTypeInfo,
    raw::ConstantPoolTag::MethodType,
    { descriptor_index: u16 }
);
define_numeric_cp_wrapper!(
    PyDynamicInfo,
    "DynamicInfo",
    raw::DynamicInfo,
    raw::ConstantPoolTag::Dynamic,
    { bootstrap_method_attr_index: u16, name_and_type_index: u16 }
);
define_numeric_cp_wrapper!(
    PyInvokeDynamicInfo,
    "InvokeDynamicInfo",
    raw::InvokeDynamicInfo,
    raw::ConstantPoolTag::InvokeDynamic,
    { bootstrap_method_attr_index: u16, name_and_type_index: u16 }
);
define_numeric_cp_wrapper!(
    PyModuleInfo,
    "ModuleInfo",
    raw::ModuleInfo,
    raw::ConstantPoolTag::Module,
    { name_index: u16 }
);
define_numeric_cp_wrapper!(
    PyPackageInfo,
    "PackageInfo",
    raw::PackageInfo,
    raw::ConstantPoolTag::Package,
    { name_index: u16 }
);

pub(crate) fn constant_pool_entry_to_pyobject(
    py: Python<'_>,
    index: usize,
    entry: &raw::ConstantPoolEntry,
) -> PyResult<PyObject> {
    use raw::ConstantPoolEntry::*;
    let obj: PyObject = match entry {
        Utf8(info) => Py::new(
            py,
            PyUtf8Info {
                index,
                inner: info.clone(),
            },
        )?
        .into_bound(py)
        .into_any()
        .unbind(),
        Integer(info) => Py::new(
            py,
            PyIntegerInfo {
                index,
                inner: info.clone(),
            },
        )?
        .into_bound(py)
        .into_any()
        .unbind(),
        Float(info) => Py::new(
            py,
            PyFloatInfo {
                index,
                inner: info.clone(),
            },
        )?
        .into_bound(py)
        .into_any()
        .unbind(),
        Long(info) => Py::new(
            py,
            PyLongInfo {
                index,
                inner: info.clone(),
            },
        )?
        .into_bound(py)
        .into_any()
        .unbind(),
        Double(info) => Py::new(
            py,
            PyDoubleInfo {
                index,
                inner: info.clone(),
            },
        )?
        .into_bound(py)
        .into_any()
        .unbind(),
        Class(info) => Py::new(
            py,
            PyConstantPoolClassInfo {
                index,
                inner: info.clone(),
            },
        )?
        .into_bound(py)
        .into_any()
        .unbind(),
        String(info) => Py::new(
            py,
            PyStringInfo {
                index,
                inner: info.clone(),
            },
        )?
        .into_bound(py)
        .into_any()
        .unbind(),
        FieldRef(info) => Py::new(
            py,
            PyFieldrefInfo {
                index,
                inner: info.clone(),
            },
        )?
        .into_bound(py)
        .into_any()
        .unbind(),
        MethodRef(info) => Py::new(
            py,
            PyMethodrefInfo {
                index,
                inner: info.clone(),
            },
        )?
        .into_bound(py)
        .into_any()
        .unbind(),
        InterfaceMethodRef(info) => Py::new(
            py,
            PyInterfaceMethodrefInfo {
                index,
                inner: info.clone(),
            },
        )?
        .into_bound(py)
        .into_any()
        .unbind(),
        NameAndType(info) => Py::new(
            py,
            PyNameAndTypeInfo {
                index,
                inner: info.clone(),
            },
        )?
        .into_bound(py)
        .into_any()
        .unbind(),
        MethodHandle(info) => Py::new(
            py,
            PyMethodHandleInfo {
                index,
                inner: info.clone(),
            },
        )?
        .into_bound(py)
        .into_any()
        .unbind(),
        MethodType(info) => Py::new(
            py,
            PyMethodTypeInfo {
                index,
                inner: info.clone(),
            },
        )?
        .into_bound(py)
        .into_any()
        .unbind(),
        Dynamic(info) => Py::new(
            py,
            PyDynamicInfo {
                index,
                inner: info.clone(),
            },
        )?
        .into_bound(py)
        .into_any()
        .unbind(),
        InvokeDynamic(info) => Py::new(
            py,
            PyInvokeDynamicInfo {
                index,
                inner: info.clone(),
            },
        )?
        .into_bound(py)
        .into_any()
        .unbind(),
        Module(info) => Py::new(
            py,
            PyModuleInfo {
                index,
                inner: info.clone(),
            },
        )?
        .into_bound(py)
        .into_any()
        .unbind(),
        Package(info) => Py::new(
            py,
            PyPackageInfo {
                index,
                inner: info.clone(),
            },
        )?
        .into_bound(py)
        .into_any()
        .unbind(),
    };
    Ok(obj)
}

#[pyclass(from_py_object, module = "pytecode._rust", name = "MatchOffsetPair")]
#[derive(Clone)]
pub struct PyMatchOffsetPair {
    inner: raw::MatchOffsetPair,
}

#[pymethods]
impl PyMatchOffsetPair {
    #[getter]
    fn r#match(&self) -> i32 {
        self.inner.match_value
    }

    #[getter]
    fn offset(&self) -> i32 {
        self.inner.offset
    }
}

#[pyclass(from_py_object, module = "pytecode._rust", name = "InsnInfo")]
#[derive(Clone)]
pub struct PyInsnInfo {
    inner: raw::Instruction,
}

#[pymethods]
impl PyInsnInfo {
    #[getter]
    fn r#type(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        wrap_instruction_type(py, &self.inner)
    }

    #[getter]
    fn bytecode_offset(&self) -> u32 {
        self.inner.offset()
    }

    #[getter]
    fn opcode(&self) -> u8 {
        self.inner.opcode()
    }

    #[getter]
    fn index(&self) -> Option<u16> {
        match &self.inner {
            raw::Instruction::LocalIndex { index, .. }
            | raw::Instruction::ConstantPoolIndex1 { index, .. } => Some(u16::from(*index)),
            raw::Instruction::ConstantPoolIndexWide(inner) => Some(inner.index.into()),
            raw::Instruction::IInc { index, .. } => Some(u16::from(*index)),
            raw::Instruction::InvokeDynamic(inner) => Some(inner.index.into()),
            raw::Instruction::InvokeInterface(inner) => Some(inner.index.into()),
            raw::Instruction::MultiANewArray { index, .. } => Some((*index).into()),
            raw::Instruction::Wide(inner) => Some(inner.index),
            _ => None,
        }
    }

    #[getter]
    fn value(&self) -> Option<i16> {
        match &self.inner {
            raw::Instruction::Byte { value, .. } => Some(i16::from(*value)),
            raw::Instruction::Short { value, .. } => Some(*value),
            raw::Instruction::IInc { value, .. } => Some(i16::from(*value)),
            raw::Instruction::Wide(inner) => inner.value,
            _ => None,
        }
    }

    #[getter]
    fn branch_offset(&self) -> Option<i32> {
        match &self.inner {
            raw::Instruction::Branch(inner) => Some(i32::from(inner.branch_offset)),
            raw::Instruction::BranchWide { branch_offset, .. } => Some(*branch_offset),
            _ => None,
        }
    }

    #[getter]
    fn count(&self) -> Option<u8> {
        match &self.inner {
            raw::Instruction::InvokeInterface(inner) => Some(inner.count),
            _ => None,
        }
    }

    #[getter]
    fn reserved<'py>(&self, py: Python<'py>) -> Option<Py<PyBytes>> {
        match &self.inner {
            raw::Instruction::InvokeDynamic(inner) => {
                Some(PyBytes::new(py, &inner.reserved.to_be_bytes()).unbind())
            }
            raw::Instruction::InvokeInterface(inner) => {
                Some(PyBytes::new(py, &[inner.reserved]).unbind())
            }
            _ => None,
        }
    }

    #[getter]
    fn atype(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        match &self.inner {
            raw::Instruction::NewArray(inner) => wrap_array_type(py, inner.atype).map(Some),
            _ => Ok(None),
        }
    }

    #[getter]
    fn dimensions(&self) -> Option<u8> {
        match &self.inner {
            raw::Instruction::MultiANewArray { dimensions, .. } => Some(*dimensions),
            _ => None,
        }
    }

    #[getter]
    fn default(&self) -> Option<i32> {
        match &self.inner {
            raw::Instruction::LookupSwitch(inner) => Some(inner.default_offset),
            raw::Instruction::TableSwitch(inner) => Some(inner.default_offset),
            _ => None,
        }
    }

    #[getter]
    fn npairs(&self) -> Option<usize> {
        match &self.inner {
            raw::Instruction::LookupSwitch(inner) => Some(inner.pairs.len()),
            _ => None,
        }
    }

    #[getter]
    fn pairs(&self) -> Option<Vec<PyMatchOffsetPair>> {
        match &self.inner {
            raw::Instruction::LookupSwitch(inner) => Some(
                inner
                    .pairs
                    .iter()
                    .cloned()
                    .map(|pair| PyMatchOffsetPair { inner: pair })
                    .collect(),
            ),
            _ => None,
        }
    }

    #[getter]
    fn low(&self) -> Option<i32> {
        match &self.inner {
            raw::Instruction::TableSwitch(inner) => Some(inner.low),
            _ => None,
        }
    }

    #[getter]
    fn high(&self) -> Option<i32> {
        match &self.inner {
            raw::Instruction::TableSwitch(inner) => Some(inner.high),
            _ => None,
        }
    }

    #[getter]
    fn offsets(&self) -> Option<Vec<i32>> {
        match &self.inner {
            raw::Instruction::TableSwitch(inner) => Some(inner.offsets.clone()),
            _ => None,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "InsnInfo(type={}, bytecode_offset={}, opcode=0x{:02x})",
            instruction_type_value(&self.inner),
            self.inner.offset(),
            self.inner.opcode()
        )
    }
}

#[pyclass(from_py_object, module = "pytecode._rust", name = "ExceptionInfo")]
#[derive(Clone)]
pub struct PyExceptionInfo {
    inner: raw::ExceptionHandler,
}

#[pymethods]
impl PyExceptionInfo {
    #[getter]
    fn start_pc(&self) -> u16 {
        self.inner.start_pc
    }

    #[getter]
    fn end_pc(&self) -> u16 {
        self.inner.end_pc
    }

    #[getter]
    fn handler_pc(&self) -> u16 {
        self.inner.handler_pc
    }

    #[getter]
    fn catch_type(&self) -> u16 {
        self.inner.catch_type.into()
    }
}

#[pyclass(from_py_object, module = "pytecode._rust", name = "ConstantValueAttr")]
#[derive(Clone)]
pub struct PyConstantValueAttr {
    inner: raw::ConstantValueAttribute,
}

#[pymethods]
impl PyConstantValueAttr {
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index.into()
    }

    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    #[getter]
    fn constantvalue_index(&self) -> u16 {
        self.inner.constantvalue_index.into()
    }
}

#[pyclass(from_py_object, module = "pytecode._rust", name = "SignatureAttr")]
#[derive(Clone)]
pub struct PySignatureAttr {
    inner: raw::SignatureAttribute,
}

#[pymethods]
impl PySignatureAttr {
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index.into()
    }

    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    #[getter]
    fn signature_index(&self) -> u16 {
        self.inner.signature_index.into()
    }
}

#[pyclass(from_py_object, module = "pytecode._rust", name = "SourceFileAttr")]
#[derive(Clone)]
pub struct PySourceFileAttr {
    inner: raw::SourceFileAttribute,
}

#[pymethods]
impl PySourceFileAttr {
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index.into()
    }

    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    #[getter]
    fn sourcefile_index(&self) -> u16 {
        self.inner.sourcefile_index.into()
    }
}

#[pyclass(
    from_py_object,
    module = "pytecode._rust",
    name = "SourceDebugExtensionAttr"
)]
#[derive(Clone)]
pub struct PySourceDebugExtensionAttr {
    inner: raw::SourceDebugExtensionAttribute,
}

#[pymethods]
impl PySourceDebugExtensionAttr {
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index.into()
    }

    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    #[getter]
    fn debug_extension<'py>(&self, py: Python<'py>) -> Py<PyBytes> {
        PyBytes::new(py, &self.inner.debug_extension).unbind()
    }
}

#[pyclass(from_py_object, module = "pytecode._rust", name = "ExceptionsAttr")]
#[derive(Clone)]
pub struct PyExceptionsAttr {
    inner: raw::ExceptionsAttribute,
}

#[pymethods]
impl PyExceptionsAttr {
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index.into()
    }

    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    #[getter]
    fn number_of_exceptions(&self) -> usize {
        self.inner.exception_index_table.len()
    }

    #[getter]
    fn exception_index_table(&self) -> Vec<u16> {
        self.inner
            .exception_index_table
            .iter()
            .map(|x| (*x).into())
            .collect()
    }
}

#[pyclass(from_py_object, module = "pytecode._rust", name = "CodeAttr")]
#[derive(Clone)]
pub struct PyCodeAttr {
    inner: raw::CodeAttribute,
}

#[pymethods]
impl PyCodeAttr {
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index.into()
    }

    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    #[getter]
    fn max_stack(&self) -> u16 {
        self.inner.max_stack
    }

    #[getter]
    fn max_stacks(&self) -> u16 {
        self.inner.max_stack
    }

    #[getter]
    fn max_locals(&self) -> u16 {
        self.inner.max_locals
    }

    #[getter]
    fn code_length(&self) -> u32 {
        self.inner.code_length
    }

    #[getter]
    fn code(&self) -> Vec<PyInsnInfo> {
        self.inner
            .code
            .iter()
            .cloned()
            .map(|inner| PyInsnInfo { inner })
            .collect()
    }

    #[getter]
    fn exception_table_length(&self) -> usize {
        self.inner.exception_table.len()
    }

    #[getter]
    fn exception_table(&self) -> Vec<PyExceptionInfo> {
        self.inner
            .exception_table
            .iter()
            .cloned()
            .map(|inner| PyExceptionInfo { inner })
            .collect()
    }

    #[getter]
    fn attributes_count(&self) -> usize {
        self.inner.attributes.len()
    }

    #[getter]
    fn attributes(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .attributes
            .iter()
            .map(|attribute| wrap_attribute(py, attribute))
            .collect()
    }
}

#[pyclass(from_py_object, module = "pytecode._rust", name = "UnimplementedAttr")]
#[derive(Clone)]
pub struct PyUnimplementedAttr {
    inner: raw::UnknownAttribute,
}

#[pymethods]
impl PyUnimplementedAttr {
    #[getter]
    fn attribute_name_index(&self) -> u16 {
        self.inner.attribute_name_index.into()
    }

    #[getter]
    fn attribute_length(&self) -> u32 {
        self.inner.attribute_length
    }

    #[getter]
    fn name(&self) -> String {
        self.inner.name.clone()
    }

    #[getter]
    fn info<'py>(&self, py: Python<'py>) -> Py<PyBytes> {
        PyBytes::new(py, &self.inner.info).unbind()
    }
}

fn wrap_constant_pool_entry(
    py: Python<'_>,
    index: usize,
    entry: &raw::ConstantPoolEntry,
) -> PyResult<Py<PyAny>> {
    match entry {
        raw::ConstantPoolEntry::Utf8(inner) => wrap_pyclass!(
            py,
            PyUtf8Info {
                index,
                inner: inner.clone(),
            }
        ),
        raw::ConstantPoolEntry::Integer(inner) => wrap_pyclass!(
            py,
            PyIntegerInfo {
                index,
                inner: inner.clone(),
            }
        ),
        raw::ConstantPoolEntry::Float(inner) => wrap_pyclass!(
            py,
            PyFloatInfo {
                index,
                inner: inner.clone(),
            }
        ),
        raw::ConstantPoolEntry::Long(inner) => wrap_pyclass!(
            py,
            PyLongInfo {
                index,
                inner: inner.clone(),
            }
        ),
        raw::ConstantPoolEntry::Double(inner) => wrap_pyclass!(
            py,
            PyDoubleInfo {
                index,
                inner: inner.clone(),
            }
        ),
        raw::ConstantPoolEntry::Class(inner) => wrap_pyclass!(
            py,
            PyConstantPoolClassInfo {
                index,
                inner: inner.clone(),
            }
        ),
        raw::ConstantPoolEntry::String(inner) => wrap_pyclass!(
            py,
            PyStringInfo {
                index,
                inner: inner.clone(),
            }
        ),
        raw::ConstantPoolEntry::FieldRef(inner) => wrap_pyclass!(
            py,
            PyFieldrefInfo {
                index,
                inner: inner.clone(),
            }
        ),
        raw::ConstantPoolEntry::MethodRef(inner) => wrap_pyclass!(
            py,
            PyMethodrefInfo {
                index,
                inner: inner.clone(),
            }
        ),
        raw::ConstantPoolEntry::InterfaceMethodRef(inner) => wrap_pyclass!(
            py,
            PyInterfaceMethodrefInfo {
                index,
                inner: inner.clone(),
            }
        ),
        raw::ConstantPoolEntry::NameAndType(inner) => wrap_pyclass!(
            py,
            PyNameAndTypeInfo {
                index,
                inner: inner.clone(),
            }
        ),
        raw::ConstantPoolEntry::MethodHandle(inner) => wrap_pyclass!(
            py,
            PyMethodHandleInfo {
                index,
                inner: inner.clone(),
            }
        ),
        raw::ConstantPoolEntry::MethodType(inner) => wrap_pyclass!(
            py,
            PyMethodTypeInfo {
                index,
                inner: inner.clone(),
            }
        ),
        raw::ConstantPoolEntry::Dynamic(inner) => wrap_pyclass!(
            py,
            PyDynamicInfo {
                index,
                inner: inner.clone(),
            }
        ),
        raw::ConstantPoolEntry::InvokeDynamic(inner) => wrap_pyclass!(
            py,
            PyInvokeDynamicInfo {
                index,
                inner: inner.clone(),
            }
        ),
        raw::ConstantPoolEntry::Module(inner) => wrap_pyclass!(
            py,
            PyModuleInfo {
                index,
                inner: inner.clone(),
            }
        ),
        raw::ConstantPoolEntry::Package(inner) => wrap_pyclass!(
            py,
            PyPackageInfo {
                index,
                inner: inner.clone(),
            }
        ),
    }
}

pub(crate) fn wrap_attribute(
    py: Python<'_>,
    attribute: &raw::AttributeInfo,
) -> PyResult<Py<PyAny>> {
    match attribute {
        raw::AttributeInfo::ConstantValue(inner) => wrap_pyclass!(
            py,
            PyConstantValueAttr {
                inner: inner.clone(),
            }
        ),
        raw::AttributeInfo::Signature(inner) => wrap_pyclass!(
            py,
            PySignatureAttr {
                inner: inner.clone(),
            }
        ),
        raw::AttributeInfo::SourceFile(inner) => wrap_pyclass!(
            py,
            PySourceFileAttr {
                inner: inner.clone(),
            }
        ),
        raw::AttributeInfo::SourceDebugExtension(inner) => wrap_pyclass!(
            py,
            PySourceDebugExtensionAttr {
                inner: inner.clone(),
            }
        ),
        raw::AttributeInfo::Code(inner) => wrap_pyclass!(
            py,
            PyCodeAttr {
                inner: inner.clone(),
            }
        ),
        raw::AttributeInfo::StackMapTable(inner) => {
            let entries = inner
                .entries
                .iter()
                .map(|entry| wrap_stack_map_frame_info(py, entry))
                .collect::<PyResult<Vec<_>>>()?;
            call_attr_class!(
                py,
                "StackMapTableAttr",
                inner.attribute_name_index,
                inner.attribute_length,
                entries.len(),
                entries
            )
        }
        raw::AttributeInfo::Exceptions(inner) => wrap_pyclass!(
            py,
            PyExceptionsAttr {
                inner: inner.clone(),
            }
        ),
        raw::AttributeInfo::InnerClasses(inner) => {
            let classes = inner
                .classes
                .iter()
                .map(|class_info| wrap_inner_class_info(py, class_info))
                .collect::<PyResult<Vec<_>>>()?;
            call_attr_class!(
                py,
                "InnerClassesAttr",
                inner.attribute_name_index,
                inner.attribute_length,
                classes.len(),
                classes
            )
        }
        raw::AttributeInfo::EnclosingMethod(inner) => call_attr_class!(
            py,
            "EnclosingMethodAttr",
            inner.attribute_name_index,
            inner.attribute_length,
            inner.class_index,
            inner.method_index
        ),
        raw::AttributeInfo::Synthetic(inner) => call_attr_class!(
            py,
            "SyntheticAttr",
            inner.attribute_name_index,
            inner.attribute_length
        ),
        raw::AttributeInfo::Deprecated(inner) => call_attr_class!(
            py,
            "DeprecatedAttr",
            inner.attribute_name_index,
            inner.attribute_length
        ),
        raw::AttributeInfo::LineNumberTable(inner) => {
            let entries = inner
                .line_number_table
                .iter()
                .map(|entry| wrap_line_number_info(py, entry))
                .collect::<PyResult<Vec<_>>>()?;
            call_attr_class!(
                py,
                "LineNumberTableAttr",
                inner.attribute_name_index,
                inner.attribute_length,
                entries.len(),
                entries
            )
        }
        raw::AttributeInfo::LocalVariableTable(inner) => {
            let entries = inner
                .local_variable_table
                .iter()
                .map(|entry| wrap_local_variable_info(py, entry))
                .collect::<PyResult<Vec<_>>>()?;
            call_attr_class!(
                py,
                "LocalVariableTableAttr",
                inner.attribute_name_index,
                inner.attribute_length,
                entries.len(),
                entries
            )
        }
        raw::AttributeInfo::LocalVariableTypeTable(inner) => {
            let entries = inner
                .local_variable_type_table
                .iter()
                .map(|entry| wrap_local_variable_type_info(py, entry))
                .collect::<PyResult<Vec<_>>>()?;
            call_attr_class!(
                py,
                "LocalVariableTypeTableAttr",
                inner.attribute_name_index,
                inner.attribute_length,
                entries.len(),
                entries
            )
        }
        raw::AttributeInfo::RuntimeVisibleAnnotations(inner) => {
            let annotations = inner
                .annotations
                .iter()
                .map(|annotation| wrap_annotation_info(py, annotation))
                .collect::<PyResult<Vec<_>>>()?;
            call_attr_class!(
                py,
                "RuntimeVisibleAnnotationsAttr",
                inner.attribute_name_index,
                inner.attribute_length,
                annotations.len(),
                annotations
            )
        }
        raw::AttributeInfo::RuntimeInvisibleAnnotations(inner) => {
            let annotations = inner
                .annotations
                .iter()
                .map(|annotation| wrap_annotation_info(py, annotation))
                .collect::<PyResult<Vec<_>>>()?;
            call_attr_class!(
                py,
                "RuntimeInvisibleAnnotationsAttr",
                inner.attribute_name_index,
                inner.attribute_length,
                annotations.len(),
                annotations
            )
        }
        raw::AttributeInfo::RuntimeVisibleParameterAnnotations(inner) => {
            let annotations = inner
                .parameter_annotations
                .iter()
                .map(|annotation| wrap_parameter_annotation_info(py, annotation))
                .collect::<PyResult<Vec<_>>>()?;
            call_attr_class!(
                py,
                "RuntimeVisibleParameterAnnotationsAttr",
                inner.attribute_name_index,
                inner.attribute_length,
                annotations.len(),
                annotations
            )
        }
        raw::AttributeInfo::RuntimeInvisibleParameterAnnotations(inner) => {
            let annotations = inner
                .parameter_annotations
                .iter()
                .map(|annotation| wrap_parameter_annotation_info(py, annotation))
                .collect::<PyResult<Vec<_>>>()?;
            call_attr_class!(
                py,
                "RuntimeInvisibleParameterAnnotationsAttr",
                inner.attribute_name_index,
                inner.attribute_length,
                annotations.len(),
                annotations
            )
        }
        raw::AttributeInfo::RuntimeVisibleTypeAnnotations(inner) => {
            let annotations = inner
                .annotations
                .iter()
                .map(|annotation| wrap_type_annotation_info(py, annotation))
                .collect::<PyResult<Vec<_>>>()?;
            call_attr_class!(
                py,
                "RuntimeVisibleTypeAnnotationsAttr",
                inner.attribute_name_index,
                inner.attribute_length,
                annotations.len(),
                annotations
            )
        }
        raw::AttributeInfo::RuntimeInvisibleTypeAnnotations(inner) => {
            let annotations = inner
                .annotations
                .iter()
                .map(|annotation| wrap_type_annotation_info(py, annotation))
                .collect::<PyResult<Vec<_>>>()?;
            call_attr_class!(
                py,
                "RuntimeInvisibleTypeAnnotationsAttr",
                inner.attribute_name_index,
                inner.attribute_length,
                annotations.len(),
                annotations
            )
        }
        raw::AttributeInfo::AnnotationDefault(inner) => {
            let default_value = wrap_element_value_info(py, &inner.default_value)?;
            call_attr_class!(
                py,
                "AnnotationDefaultAttr",
                inner.attribute_name_index,
                inner.attribute_length,
                default_value
            )
        }
        raw::AttributeInfo::BootstrapMethods(inner) => {
            let methods = inner
                .bootstrap_methods
                .iter()
                .map(|method| wrap_bootstrap_method_info(py, method))
                .collect::<PyResult<Vec<_>>>()?;
            call_attr_class!(
                py,
                "BootstrapMethodsAttr",
                inner.attribute_name_index,
                inner.attribute_length,
                methods.len(),
                methods
            )
        }
        raw::AttributeInfo::MethodParameters(inner) => {
            let parameters = inner
                .parameters
                .iter()
                .map(|parameter| wrap_method_parameter_info(py, parameter))
                .collect::<PyResult<Vec<_>>>()?;
            call_attr_class!(
                py,
                "MethodParametersAttr",
                inner.attribute_name_index,
                inner.attribute_length,
                parameters.len(),
                parameters
            )
        }
        raw::AttributeInfo::Module(inner) => {
            let module_flags = wrap_module_access_flags(py, inner.module.module_flags)?;
            let requires = inner
                .module
                .requires
                .iter()
                .map(|info| wrap_requires_info(py, info))
                .collect::<PyResult<Vec<_>>>()?;
            let exports = inner
                .module
                .exports
                .iter()
                .map(|info| wrap_export_info(py, info))
                .collect::<PyResult<Vec<_>>>()?;
            let opens = inner
                .module
                .opens
                .iter()
                .map(|info| wrap_opens_info(py, info))
                .collect::<PyResult<Vec<_>>>()?;
            let provides = inner
                .module
                .provides
                .iter()
                .map(|info| wrap_provides_info(py, info))
                .collect::<PyResult<Vec<_>>>()?;
            let kwargs = PyDict::new(py);
            kwargs.set_item("attribute_name_index", inner.attribute_name_index)?;
            kwargs.set_item("attribute_length", inner.attribute_length)?;
            kwargs.set_item("module_name_index", inner.module.module_name_index)?;
            kwargs.set_item("module_flags", module_flags)?;
            kwargs.set_item("module_version_index", inner.module.module_version_index)?;
            kwargs.set_item("requires_count", requires.len())?;
            kwargs.set_item("requires", requires)?;
            kwargs.set_item("exports_count", exports.len())?;
            kwargs.set_item("exports", exports)?;
            kwargs.set_item("opens_count", opens.len())?;
            kwargs.set_item("opens", opens)?;
            kwargs.set_item("uses_count", inner.module.uses_index.len())?;
            kwargs.set_item("uses_index", inner.module.uses_index.clone())?;
            kwargs.set_item("provides_count", provides.len())?;
            kwargs.set_item("provides", provides)?;
            PyModule::import(py, "pytecode.classfile.attributes")?
                .getattr("ModuleAttr")?
                .call((), Some(&kwargs))
                .map(|obj| obj.unbind())
        }
        raw::AttributeInfo::ModulePackages(inner) => call_attr_class!(
            py,
            "ModulePackagesAttr",
            inner.attribute_name_index,
            inner.attribute_length,
            inner.package_index.len(),
            inner.package_index.clone()
        ),
        raw::AttributeInfo::ModuleMainClass(inner) => call_attr_class!(
            py,
            "ModuleMainClassAttr",
            inner.attribute_name_index,
            inner.attribute_length,
            inner.main_class_index
        ),
        raw::AttributeInfo::NestHost(inner) => call_attr_class!(
            py,
            "NestHostAttr",
            inner.attribute_name_index,
            inner.attribute_length,
            inner.host_class_index
        ),
        raw::AttributeInfo::NestMembers(inner) => call_attr_class!(
            py,
            "NestMembersAttr",
            inner.attribute_name_index,
            inner.attribute_length,
            inner.classes.len(),
            inner.classes.clone()
        ),
        raw::AttributeInfo::Record(inner) => {
            let components = inner
                .components
                .iter()
                .map(|component| wrap_record_component_info(py, component))
                .collect::<PyResult<Vec<_>>>()?;
            call_attr_class!(
                py,
                "RecordAttr",
                inner.attribute_name_index,
                inner.attribute_length,
                components.len(),
                components
            )
        }
        raw::AttributeInfo::PermittedSubclasses(inner) => call_attr_class!(
            py,
            "PermittedSubclassesAttr",
            inner.attribute_name_index,
            inner.attribute_length,
            inner.classes.len(),
            inner.classes.clone()
        ),
        raw::AttributeInfo::Unknown(inner) => wrap_pyclass!(
            py,
            PyUnimplementedAttr {
                inner: inner.clone(),
            }
        ),
    }
}

#[pyclass(from_py_object, module = "pytecode._rust", name = "FieldInfo")]
#[derive(Clone)]
pub struct PyFieldInfo {
    inner: raw::FieldInfo,
}

#[pymethods]
impl PyFieldInfo {
    #[getter]
    fn access_flags(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        wrap_field_access_flags(py, self.inner.access_flags)
    }

    #[getter]
    fn name_index(&self) -> u16 {
        self.inner.name_index.into()
    }

    #[getter]
    fn descriptor_index(&self) -> u16 {
        self.inner.descriptor_index.into()
    }

    #[getter]
    fn attributes_count(&self) -> usize {
        self.inner.attributes.len()
    }

    #[getter]
    fn attributes(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .attributes
            .iter()
            .map(|attribute| wrap_attribute(py, attribute))
            .collect()
    }
}

#[pyclass(from_py_object, module = "pytecode._rust", name = "MethodInfo")]
#[derive(Clone)]
pub struct PyMethodInfo {
    inner: raw::MethodInfo,
}

#[pymethods]
impl PyMethodInfo {
    #[getter]
    fn access_flags(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        wrap_method_access_flags(py, self.inner.access_flags)
    }

    #[getter]
    fn name_index(&self) -> u16 {
        self.inner.name_index.into()
    }

    #[getter]
    fn descriptor_index(&self) -> u16 {
        self.inner.descriptor_index.into()
    }

    #[getter]
    fn attributes_count(&self) -> usize {
        self.inner.attributes.len()
    }

    #[getter]
    fn attributes(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .attributes
            .iter()
            .map(|attribute| wrap_attribute(py, attribute))
            .collect()
    }
}

#[pyclass(from_py_object, module = "pytecode._rust", name = "ClassFile")]
#[derive(Clone)]
pub struct PyClassFile {
    inner: Arc<ClassFile>,
}

#[pymethods]
impl PyClassFile {
    #[getter]
    fn magic(&self) -> u32 {
        self.inner.magic
    }

    #[getter]
    fn minor_version(&self) -> u16 {
        self.inner.minor_version
    }

    #[getter]
    fn major_version(&self) -> u16 {
        self.inner.major_version
    }

    #[getter]
    fn constant_pool_count(&self) -> usize {
        self.inner.constant_pool.len()
    }

    #[getter]
    fn constant_pool(&self, py: Python<'_>) -> PyResult<Vec<Option<Py<PyAny>>>> {
        self.inner
            .constant_pool
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                entry
                    .as_ref()
                    .map(|entry| wrap_constant_pool_entry(py, index, entry))
                    .transpose()
            })
            .collect()
    }

    #[getter]
    fn access_flags(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        wrap_class_access_flags(py, self.inner.access_flags)
    }

    #[getter]
    fn this_class(&self) -> u16 {
        self.inner.this_class.into()
    }

    #[getter]
    fn super_class(&self) -> u16 {
        self.inner.super_class.into()
    }

    #[getter]
    fn interfaces_count(&self) -> usize {
        self.inner.interfaces.len()
    }

    #[getter]
    fn interface_count(&self) -> usize {
        self.inner.interfaces.len()
    }

    #[getter]
    fn interfaces(&self) -> Vec<u16> {
        self.inner.interfaces.iter().map(|x| (*x).into()).collect()
    }

    #[getter]
    fn fields_count(&self) -> usize {
        self.inner.fields.len()
    }

    #[getter]
    fn field_count(&self) -> usize {
        self.inner.fields.len()
    }

    #[getter]
    fn fields(&self) -> Vec<PyFieldInfo> {
        self.inner
            .fields
            .iter()
            .cloned()
            .map(|inner| PyFieldInfo { inner })
            .collect()
    }

    #[getter]
    fn methods_count(&self) -> usize {
        self.inner.methods.len()
    }

    #[getter]
    fn method_count(&self) -> usize {
        self.inner.methods.len()
    }

    #[getter]
    fn methods(&self) -> Vec<PyMethodInfo> {
        self.inner
            .methods
            .iter()
            .cloned()
            .map(|inner| PyMethodInfo { inner })
            .collect()
    }

    #[getter]
    fn attributes_count(&self) -> usize {
        self.inner.attributes.len()
    }

    #[getter]
    fn attribute_count(&self) -> usize {
        self.inner.attributes.len()
    }

    #[getter]
    fn attributes(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        self.inner
            .attributes
            .iter()
            .map(|attribute| wrap_attribute(py, attribute))
            .collect()
    }

    fn to_bytes<'py>(&self, py: Python<'py>) -> PyResult<Py<PyBytes>> {
        let bytes = write_class(&self.inner).map_err(engine_error_to_py)?;
        Ok(PyBytes::new(py, &bytes).unbind())
    }

    fn __repr__(&self) -> String {
        format!(
            "ClassFile(major_version={}, minor_version={}, methods={}, fields={})",
            self.inner.major_version,
            self.inner.minor_version,
            self.inner.methods.len(),
            self.inner.fields.len()
        )
    }
}

#[pyclass(module = "pytecode._rust", name = "ClassReader")]
pub struct PyClassReader {
    class_info: Arc<ClassFile>,
}

#[pymethods]
impl PyClassReader {
    #[new]
    fn new(bytes_or_bytearray: &[u8]) -> PyResult<Self> {
        let class_info = parse_class(bytes_or_bytearray).map_err(engine_error_to_py)?;
        Ok(Self {
            class_info: Arc::new(class_info),
        })
    }

    #[classmethod]
    fn from_bytes(_cls: &Bound<'_, PyType>, bytes_or_bytearray: &[u8]) -> PyResult<Self> {
        Self::new(bytes_or_bytearray)
    }

    #[classmethod]
    fn from_file(_cls: &Bound<'_, PyType>, path: PathBuf) -> PyResult<Self> {
        let bytes = fs::read(&path).map_err(PyOSError::new_err)?;
        Self::new(&bytes)
    }

    #[getter]
    fn class_info(&self) -> PyClassFile {
        PyClassFile {
            inner: self.class_info.clone(),
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "ClassReader(major_version={}, minor_version={})",
            self.class_info.major_version, self.class_info.minor_version
        )
    }
}

#[pyclass(module = "pytecode._rust", name = "ClassWriter")]
pub struct PyClassWriter;

#[pymethods]
impl PyClassWriter {
    #[staticmethod]
    fn write<'py>(py: Python<'py>, classfile: &PyClassFile) -> PyResult<Py<PyBytes>> {
        let bytes = write_class(&classfile.inner).map_err(engine_error_to_py)?;
        Ok(PyBytes::new(py, &bytes).unbind())
    }
}

#[pymodule]
fn _rust(py: Python<'_>, module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add(
        "MalformedClassException",
        py.get_type::<MalformedClassException>(),
    )?;
    module.add_class::<PyUtf8Info>()?;
    module.add_class::<PyIntegerInfo>()?;
    module.add_class::<PyFloatInfo>()?;
    module.add_class::<PyLongInfo>()?;
    module.add_class::<PyDoubleInfo>()?;
    module.add_class::<PyConstantPoolClassInfo>()?;
    module.add_class::<PyStringInfo>()?;
    module.add_class::<PyFieldrefInfo>()?;
    module.add_class::<PyMethodrefInfo>()?;
    module.add_class::<PyInterfaceMethodrefInfo>()?;
    module.add_class::<PyNameAndTypeInfo>()?;
    module.add_class::<PyMethodHandleInfo>()?;
    module.add_class::<PyMethodTypeInfo>()?;
    module.add_class::<PyDynamicInfo>()?;
    module.add_class::<PyInvokeDynamicInfo>()?;
    module.add_class::<PyModuleInfo>()?;
    module.add_class::<PyPackageInfo>()?;
    module.add_class::<PyMatchOffsetPair>()?;
    module.add_class::<PyInsnInfo>()?;
    module.add_class::<PyExceptionInfo>()?;
    module.add_class::<PyConstantValueAttr>()?;
    module.add_class::<PySignatureAttr>()?;
    module.add_class::<PySourceFileAttr>()?;
    module.add_class::<PySourceDebugExtensionAttr>()?;
    module.add_class::<PyExceptionsAttr>()?;
    module.add_class::<PyCodeAttr>()?;
    module.add_class::<PyUnimplementedAttr>()?;
    module.add_class::<PyFieldInfo>()?;
    module.add_class::<PyMethodInfo>()?;
    module.add_class::<PyClassFile>()?;
    module.add_class::<PyClassReader>()?;
    module.add_class::<PyClassWriter>()?;
    module.add_class::<transforms::PyInsnMatcher>()?;
    module.add_class::<transforms::PyClassMatcher>()?;
    module.add_class::<transforms::PyFieldMatcher>()?;
    module.add_class::<transforms::PyMethodMatcher>()?;
    module.add_class::<transforms::PyCodeTransform>()?;
    module.add_class::<transforms::PyClassTransform>()?;
    module.add_class::<transforms::PyPipeline>()?;
    module.add_class::<transforms::PyCompiledPipeline>()?;
    model::register(py, module)?;
    analysis::register(py, module)?;
    archive::register(py, module)?;
    attributes::register(py, module)?;
    module.add(
        "__all__",
        vec![
            "MalformedClassException",
            "ClassFile",
            "ClassReader",
            "ClassWriter",
            "FieldInfo",
            "MethodInfo",
            "InsnInfo",
            "CodeAttr",
            "Utf8Info",
            "FieldrefInfo",
            "MethodrefInfo",
            "InterfaceMethodrefInfo",
        ],
    )?;
    Ok(())
}
