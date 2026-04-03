use std::collections::HashSet;

use pyo3::prelude::*;
use pyo3::types::{PyAny, PyList, PyModule};

const GETSTATIC: i64 = 0xB2;
const PUTSTATIC: i64 = 0xB3;
const GETFIELD: i64 = 0xB4;
const PUTFIELD: i64 = 0xB5;
const INVOKEVIRTUAL: i64 = 0xB6;
const INVOKESPECIAL: i64 = 0xB7;
const INVOKESTATIC: i64 = 0xB8;
const INVOKEINTERFACE: i64 = 0xB9;
const INVOKEDYNAMIC: i64 = 0xBA;
const NEW: i64 = 0xBB;
const LDC: i64 = 0x12;
const LDC_W: i64 = 0x13;
const LDC2_W: i64 = 0x14;
const ANEWARRAY: i64 = 0xBD;
const CHECKCAST: i64 = 0xC0;
const INSTANCEOF: i64 = 0xC1;
const MULTIANEWARRAY: i64 = 0xC5;

fn py_class_name(value: &Bound<'_, PyAny>) -> PyResult<String> {
    value.getattr("__class__")?.getattr("__name__")?.extract()
}

fn int_attr(value: &Bound<'_, PyAny>, name: &str) -> PyResult<i64> {
    value.getattr(name)?.extract()
}

fn cp_entry<'py>(cp: &Bound<'py, PyList>, index: i64) -> PyResult<Option<Bound<'py, PyAny>>> {
    if index < 1 || index >= cp.len() as i64 {
        return Ok(None);
    }

    let entry = cp.get_item(index as usize)?;
    if entry.is_none() {
        Ok(None)
    } else {
        Ok(Some(entry))
    }
}

fn add_diag(diags: &mut Vec<(String, Option<i64>)>, message: String, offset: Option<i64>) {
    diags.push((message, offset));
}

fn verify_ldc_entry(
    diags: &mut Vec<(String, Option<i64>)>,
    entry_type: &str,
    index: i64,
    major: u16,
    offset: i64,
) {
    match entry_type {
        "IntegerInfo" | "FloatInfo" | "StringInfo" => {}
        "ClassInfo" => {
            if major < 49 {
                add_diag(
                    diags,
                    format!("LDC CP#{index} ClassInfo requires version >= 49, got {major}"),
                    Some(offset),
                );
            }
        }
        "MethodHandleInfo" | "MethodTypeInfo" => {
            if major < 51 {
                add_diag(
                    diags,
                    format!("LDC CP#{index} {entry_type} requires version >= 51, got {major}"),
                    Some(offset),
                );
            }
        }
        "DynamicInfo" => {
            if major < 55 {
                add_diag(
                    diags,
                    format!("LDC CP#{index} DynamicInfo requires version >= 55, got {major}"),
                    Some(offset),
                );
            }
        }
        _ => add_diag(
            diags,
            format!("LDC CP#{index} has non-loadable type {entry_type}"),
            Some(offset),
        ),
    }
}

fn is_field_op(opcode: i64) -> bool {
    matches!(opcode, GETSTATIC | PUTSTATIC | GETFIELD | PUTFIELD)
}

fn is_method_op(opcode: i64) -> bool {
    matches!(opcode, INVOKEVIRTUAL | INVOKESPECIAL | INVOKESTATIC)
}

fn is_class_op(opcode: i64) -> bool {
    matches!(opcode, NEW | CHECKCAST | INSTANCEOF | ANEWARRAY)
}

#[pyfunction]
fn verify_code(
    code: &Bound<'_, PyAny>,
    cp: &Bound<'_, PyList>,
    major: u16,
) -> PyResult<Vec<(String, Option<i64>)>> {
    let mut diags: Vec<(String, Option<i64>)> = Vec::new();

    let code_length = int_attr(code, "code_length")?;
    if code_length <= 0 {
        add_diag(&mut diags, "code_length must be > 0".to_string(), None);
    }
    if code_length > 65_535 {
        add_diag(
            &mut diags,
            format!("code_length {code_length} exceeds 65535"),
            None,
        );
    }

    let max_stacks = int_attr(code, "max_stacks")?;
    if !(0..=65_535).contains(&max_stacks) {
        add_diag(
            &mut diags,
            format!("max_stack {max_stacks} out of range [0, 65535]"),
            None,
        );
    }

    let max_locals = int_attr(code, "max_locals")?;
    if !(0..=65_535).contains(&max_locals) {
        add_diag(
            &mut diags,
            format!("max_locals {max_locals} out of range [0, 65535]"),
            None,
        );
    }

    let code_items = code.getattr("code")?.cast_into::<PyList>()?;
    if code_items.is_empty() {
        return Ok(diags);
    }

    let mut valid_offsets = HashSet::new();
    for insn in code_items.iter() {
        valid_offsets.insert(int_attr(&insn, "bytecode_offset")?);
    }

    for insn in code_items.iter() {
        let insn_offset = int_attr(&insn, "bytecode_offset")?;
        match py_class_name(&insn)?.as_str() {
            "Branch" => {
                let target = insn_offset + int_attr(&insn, "offset")?;
                if !valid_offsets.contains(&target) {
                    add_diag(
                        &mut diags,
                        format!("Branch at offset {insn_offset} targets invalid offset {target}"),
                        Some(insn_offset),
                    );
                }
            }
            "BranchW" => {
                let target = insn_offset + int_attr(&insn, "offset")?;
                if !valid_offsets.contains(&target) {
                    add_diag(
                        &mut diags,
                        format!(
                            "Wide branch at offset {insn_offset} targets invalid offset {target}"
                        ),
                        Some(insn_offset),
                    );
                }
            }
            "LookupSwitch" => {
                let default_target = insn_offset + int_attr(&insn, "default")?;
                if !valid_offsets.contains(&default_target) {
                    add_diag(
                        &mut diags,
                        format!("lookupswitch default targets invalid offset {default_target}"),
                        Some(insn_offset),
                    );
                }

                let pairs = insn.getattr("pairs")?.cast_into::<PyList>()?;
                for pair in pairs.iter() {
                    let target = insn_offset + int_attr(&pair, "offset")?;
                    if !valid_offsets.contains(&target) {
                        add_diag(
                            &mut diags,
                            format!(
                                "lookupswitch case {} targets invalid offset {target}",
                                int_attr(&pair, "match")?
                            ),
                            Some(insn_offset),
                        );
                    }
                }
            }
            "TableSwitch" => {
                let default_target = insn_offset + int_attr(&insn, "default")?;
                if !valid_offsets.contains(&default_target) {
                    add_diag(
                        &mut diags,
                        format!("tableswitch default targets invalid offset {default_target}"),
                        Some(insn_offset),
                    );
                }

                let low = int_attr(&insn, "low")?;
                let offsets = insn.getattr("offsets")?.cast_into::<PyList>()?;
                for (index, offset) in offsets.iter().enumerate() {
                    let target = insn_offset + offset.extract::<i64>()?;
                    if !valid_offsets.contains(&target) {
                        add_diag(
                            &mut diags,
                            format!(
                                "tableswitch case {} targets invalid offset {target}",
                                low + index as i64
                            ),
                            Some(insn_offset),
                        );
                    }
                }
            }
            _ => {}
        }
    }

    let exception_table = code.getattr("exception_table")?.cast_into::<PyList>()?;
    for handler in exception_table.iter() {
        let start_pc = int_attr(&handler, "start_pc")?;
        if !valid_offsets.contains(&start_pc) {
            add_diag(
                &mut diags,
                format!("Exception handler start_pc {start_pc} is not a valid instruction offset"),
                None,
            );
        }

        let end_pc = int_attr(&handler, "end_pc")?;
        if !valid_offsets.contains(&end_pc) && end_pc != code_length {
            add_diag(
                &mut diags,
                format!("Exception handler end_pc {end_pc} is not a valid offset or code_length"),
                None,
            );
        }
        if start_pc >= end_pc {
            add_diag(
                &mut diags,
                format!("Exception handler start_pc ({start_pc}) must be < end_pc ({end_pc})"),
                None,
            );
        }

        let handler_pc = int_attr(&handler, "handler_pc")?;
        if !valid_offsets.contains(&handler_pc) {
            add_diag(
                &mut diags,
                format!(
                    "Exception handler handler_pc {handler_pc} is not a valid instruction offset"
                ),
                None,
            );
        }

        let catch_type = int_attr(&handler, "catch_type")?;
        if catch_type != 0 {
            let catch_entry_type = cp_entry(cp, catch_type)?
                .map(|entry| py_class_name(&entry))
                .transpose()?;
            if catch_entry_type.as_deref() != Some("ClassInfo") {
                add_diag(
                    &mut diags,
                    format!(
                        "Exception handler catch_type {catch_type} does not point to CONSTANT_Class"
                    ),
                    None,
                );
            }
        }
    }

    for insn in code_items.iter() {
        let insn_offset = int_attr(&insn, "bytecode_offset")?;
        let insn_class = py_class_name(&insn)?;
        let insn_type = insn.getattr("type")?;
        let opcode = insn_type.extract::<i64>()?;
        let opcode_name = insn_type.getattr("name")?.extract::<String>()?;

        if insn_class == "ConstPoolIndex" {
            let index = int_attr(&insn, "index")?;
            let Some(entry) = cp_entry(cp, index)? else {
                add_diag(
                    &mut diags,
                    format!("{opcode_name} references invalid CP index {index}"),
                    Some(insn_offset),
                );
                continue;
            };
            let entry_type = py_class_name(&entry)?;

            if is_field_op(opcode) {
                if entry_type != "FieldrefInfo" {
                    add_diag(
                        &mut diags,
                        format!("{opcode_name} CP#{index} expected FieldrefInfo, got {entry_type}"),
                        Some(insn_offset),
                    );
                }
            } else if is_method_op(opcode) {
                if major >= 52 {
                    if entry_type != "MethodrefInfo" && entry_type != "InterfaceMethodrefInfo" {
                        add_diag(
                            &mut diags,
                            format!(
                                "{opcode_name} CP#{index} expected Methodref/InterfaceMethodref, got {entry_type}"
                            ),
                            Some(insn_offset),
                        );
                    }
                } else if entry_type != "MethodrefInfo" {
                    add_diag(
                        &mut diags,
                        format!(
                            "{opcode_name} CP#{index} expected MethodrefInfo, got {entry_type}"
                        ),
                        Some(insn_offset),
                    );
                }
            } else if is_class_op(opcode) {
                if entry_type != "ClassInfo" {
                    add_diag(
                        &mut diags,
                        format!("{opcode_name} CP#{index} expected ClassInfo, got {entry_type}"),
                        Some(insn_offset),
                    );
                }
            } else if opcode == LDC_W {
                verify_ldc_entry(&mut diags, &entry_type, index, major, insn_offset);
            } else if opcode == LDC2_W && entry_type != "LongInfo" && entry_type != "DoubleInfo" {
                add_diag(
                    &mut diags,
                    format!("LDC2_W CP#{index} expected Long/Double, got {entry_type}"),
                    Some(insn_offset),
                );
            }
        } else if insn_class == "LocalIndex" && opcode == LDC {
            let index = int_attr(&insn, "index")?;
            let Some(entry) = cp_entry(cp, index)? else {
                add_diag(
                    &mut diags,
                    format!("LDC references invalid CP index {index}"),
                    Some(insn_offset),
                );
                continue;
            };
            verify_ldc_entry(
                &mut diags,
                &py_class_name(&entry)?,
                index,
                major,
                insn_offset,
            );
        } else if opcode == INVOKEINTERFACE {
            let index = int_attr(&insn, "index")?;
            let Some(entry) = cp_entry(cp, index)? else {
                add_diag(
                    &mut diags,
                    format!("INVOKEINTERFACE references invalid CP index {index}"),
                    Some(insn_offset),
                );
                continue;
            };
            let entry_type = py_class_name(&entry)?;
            if entry_type != "InterfaceMethodrefInfo" {
                add_diag(
                    &mut diags,
                    format!(
                        "INVOKEINTERFACE CP#{index} expected InterfaceMethodrefInfo, got {entry_type}"
                    ),
                    Some(insn_offset),
                );
            }
        } else if opcode == INVOKEDYNAMIC {
            let index = int_attr(&insn, "index")?;
            let Some(entry) = cp_entry(cp, index)? else {
                add_diag(
                    &mut diags,
                    format!("INVOKEDYNAMIC references invalid CP index {index}"),
                    Some(insn_offset),
                );
                continue;
            };
            let entry_type = py_class_name(&entry)?;
            if entry_type != "InvokeDynamicInfo" {
                add_diag(
                    &mut diags,
                    format!(
                        "INVOKEDYNAMIC CP#{index} expected InvokeDynamicInfo, got {entry_type}"
                    ),
                    Some(insn_offset),
                );
            }
        } else if opcode == MULTIANEWARRAY {
            let index = int_attr(&insn, "index")?;
            let entry_type = cp_entry(cp, index)?
                .map(|entry| py_class_name(&entry))
                .transpose()?;
            if entry_type.is_none() {
                add_diag(
                    &mut diags,
                    format!("MULTIANEWARRAY references invalid CP index {index}"),
                    Some(insn_offset),
                );
            } else if entry_type.as_deref() != Some("ClassInfo") {
                add_diag(
                    &mut diags,
                    format!(
                        "MULTIANEWARRAY CP#{index} expected ClassInfo, got {}",
                        entry_type.unwrap_or_default()
                    ),
                    Some(insn_offset),
                );
            }

            let dimensions = int_attr(&insn, "dimensions")?;
            if dimensions < 1 {
                add_diag(
                    &mut diags,
                    format!("MULTIANEWARRAY dimensions must be >= 1, got {dimensions}"),
                    Some(insn_offset),
                );
            }
        }
    }

    Ok(diags)
}

pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent.py(), "verify")?;
    m.add_function(wrap_pyfunction!(verify_code, &m)?)?;
    crate::register_submodule(parent, &m, "pytecode._rust.analysis.verify")?;
    Ok(())
}
