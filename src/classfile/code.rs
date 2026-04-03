use crate::classfile::instructions::{
    ArrayType, Instruction, InstructionOperands, MatchOffsetPair, WIDE_PREFIX,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyList, PyModule};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OperandKind {
    None,
    LocalIndex,
    ConstPoolIndex,
    ByteValue,
    ShortValue,
    Branch,
    BranchWide,
    IInc,
    InvokeDynamic,
    InvokeInterface,
    MultiANewArray,
    NewArray,
    LookupSwitch,
    TableSwitch,
    Wide,
}

fn operand_kind(opcode: u16) -> OperandKind {
    match opcode {
        0x10 => OperandKind::ByteValue,
        0x11 => OperandKind::ShortValue,
        0x12 | 0x15..=0x19 | 0x36..=0x3A | 0xA9 => OperandKind::LocalIndex,
        0x13 | 0x14 | 0xB2..=0xB8 | 0xBB | 0xBD | 0xC0 | 0xC1 => OperandKind::ConstPoolIndex,
        0x84 => OperandKind::IInc,
        0x99..=0xA8 | 0xC6 | 0xC7 => OperandKind::Branch,
        0xAA => OperandKind::TableSwitch,
        0xAB => OperandKind::LookupSwitch,
        0xB9 => OperandKind::InvokeInterface,
        0xBA => OperandKind::InvokeDynamic,
        0xBC => OperandKind::NewArray,
        0xC4 => OperandKind::Wide,
        0xC5 => OperandKind::MultiANewArray,
        0xC8 | 0xC9 => OperandKind::BranchWide,
        _ => OperandKind::None,
    }
}

fn array_type_from_u8(value: u8) -> PyResult<ArrayType> {
    match value {
        4 => Ok(ArrayType::Boolean),
        5 => Ok(ArrayType::Char),
        6 => Ok(ArrayType::Float),
        7 => Ok(ArrayType::Double),
        8 => Ok(ArrayType::Byte),
        9 => Ok(ArrayType::Short),
        10 => Ok(ArrayType::Int),
        11 => Ok(ArrayType::Long),
        _ => Err(PyValueError::new_err(format!(
            "Unknown ArrayType value: {value}"
        ))),
    }
}

struct SliceReader<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> SliceReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    fn read_u1(&mut self) -> PyResult<u8> {
        if self.offset >= self.data.len() {
            return Err(PyValueError::new_err("read_u1: unexpected end of data"));
        }
        let value = self.data[self.offset];
        self.offset += 1;
        Ok(value)
    }

    fn read_u2(&mut self) -> PyResult<u16> {
        if self.offset + 2 > self.data.len() {
            return Err(PyValueError::new_err("read_u2: unexpected end of data"));
        }
        let value = u16::from_be_bytes([self.data[self.offset], self.data[self.offset + 1]]);
        self.offset += 2;
        Ok(value)
    }

    fn read_u4(&mut self) -> PyResult<u32> {
        if self.offset + 4 > self.data.len() {
            return Err(PyValueError::new_err("read_u4: unexpected end of data"));
        }
        let value = u32::from_be_bytes([
            self.data[self.offset],
            self.data[self.offset + 1],
            self.data[self.offset + 2],
            self.data[self.offset + 3],
        ]);
        self.offset += 4;
        Ok(value)
    }

    fn read_i1(&mut self) -> PyResult<i8> {
        Ok(self.read_u1()? as i8)
    }

    fn read_i2(&mut self) -> PyResult<i16> {
        Ok(self.read_u2()? as i16)
    }

    fn read_i4(&mut self) -> PyResult<i32> {
        Ok(self.read_u4()? as i32)
    }

    fn read_exact<const N: usize>(&mut self) -> PyResult<[u8; N]> {
        if self.offset + N > self.data.len() {
            return Err(PyValueError::new_err("read_exact: unexpected end of data"));
        }
        let mut bytes = [0_u8; N];
        bytes.copy_from_slice(&self.data[self.offset..self.offset + N]);
        self.offset += N;
        Ok(bytes)
    }

    fn align_to_four_after_opcode(&mut self, current_method_offset: u32) -> PyResult<()> {
        let align_bytes = (4 - ((current_method_offset + 1) % 4)) % 4;
        if self.offset + align_bytes as usize > self.data.len() {
            return Err(PyValueError::new_err("align: unexpected end of data"));
        }
        self.offset += align_bytes as usize;
        Ok(())
    }
}

fn read_instruction(
    reader: &mut SliceReader<'_>,
    current_method_offset: u32,
) -> PyResult<Instruction> {
    let opcode = reader.read_u1()? as u16;
    let operands = match operand_kind(opcode) {
        OperandKind::None => InstructionOperands::None,
        OperandKind::LocalIndex => InstructionOperands::LocalIndex(reader.read_u1()? as u16),
        OperandKind::ConstPoolIndex => InstructionOperands::ConstPoolIndex(reader.read_u2()?),
        OperandKind::ByteValue => InstructionOperands::ByteValue(reader.read_i1()?),
        OperandKind::ShortValue => InstructionOperands::ShortValue(reader.read_i2()?),
        OperandKind::Branch => InstructionOperands::Branch(reader.read_i2()?),
        OperandKind::BranchWide => InstructionOperands::BranchWide(reader.read_i4()?),
        OperandKind::IInc => InstructionOperands::IInc {
            index: reader.read_u1()? as u16,
            value: reader.read_i1()? as i16,
        },
        OperandKind::InvokeDynamic => InstructionOperands::InvokeDynamic {
            index: reader.read_u2()?,
            unused: reader.read_exact::<2>()?,
        },
        OperandKind::InvokeInterface => InstructionOperands::InvokeInterface {
            index: reader.read_u2()?,
            count: reader.read_u1()?,
            unused: reader.read_u1()?,
        },
        OperandKind::MultiANewArray => InstructionOperands::MultiANewArray {
            index: reader.read_u2()?,
            dimensions: reader.read_u1()?,
        },
        OperandKind::NewArray => {
            InstructionOperands::NewArray(array_type_from_u8(reader.read_u1()?)?)
        }
        OperandKind::LookupSwitch => {
            reader.align_to_four_after_opcode(current_method_offset)?;
            let default = reader.read_i4()?;
            let npairs = reader.read_u4()? as usize;
            let mut pairs = Vec::with_capacity(npairs);
            for _ in 0..npairs {
                pairs.push(MatchOffsetPair {
                    match_value: reader.read_i4()?,
                    offset: reader.read_i4()?,
                });
            }
            InstructionOperands::LookupSwitch { default, pairs }
        }
        OperandKind::TableSwitch => {
            reader.align_to_four_after_opcode(current_method_offset)?;
            let default = reader.read_i4()?;
            let low = reader.read_i4()?;
            let high = reader.read_i4()?;
            if high < low {
                return Err(PyValueError::new_err(format!(
                    "tableswitch high must be >= low, got low={low} high={high}"
                )));
            }
            let count = (high - low + 1) as usize;
            let mut offsets = Vec::with_capacity(count);
            for _ in 0..count {
                offsets.push(reader.read_i4()?);
            }
            InstructionOperands::TableSwitch {
                default,
                low,
                high,
                offsets,
            }
        }
        OperandKind::Wide => {
            let wide_opcode = reader.read_u1()? as u16;
            let synthetic_opcode = WIDE_PREFIX as u16 + wide_opcode;
            let operands = match wide_opcode {
                0x15..=0x19 | 0x36..=0x3A | 0xA9 => {
                    InstructionOperands::LocalIndexWide(reader.read_u2()?)
                }
                0x84 => InstructionOperands::IInc {
                    index: reader.read_u2()?,
                    value: reader.read_i2()?,
                },
                _ => {
                    return Err(PyValueError::new_err(format!(
                        "Invalid wide opcode: {wide_opcode:#x}"
                    )));
                }
            };
            return Ok(Instruction {
                opcode: synthetic_opcode,
                bytecode_offset: current_method_offset,
                operands,
            });
        }
    };
    Ok(Instruction {
        opcode,
        bytecode_offset: current_method_offset,
        operands,
    })
}

fn read_code_instructions(data: &[u8]) -> PyResult<Vec<Instruction>> {
    let mut reader = SliceReader::new(data);
    let mut instructions = Vec::new();
    while reader.offset < data.len() {
        let current_method_offset = reader.offset as u32;
        instructions.push(read_instruction(&mut reader, current_method_offset)?);
    }
    Ok(instructions)
}

struct VecWriter {
    data: Vec<u8>,
}

impl VecWriter {
    fn new() -> Self {
        Self { data: Vec::new() }
    }

    fn write_u1(&mut self, value: u8) {
        self.data.push(value);
    }

    fn write_u2(&mut self, value: u16) {
        self.data.extend_from_slice(&value.to_be_bytes());
    }

    fn write_u4(&mut self, value: u32) {
        self.data.extend_from_slice(&value.to_be_bytes());
    }

    fn write_i1(&mut self, value: i8) {
        self.write_u1(value as u8);
    }

    fn write_i2(&mut self, value: i16) {
        self.write_u2(value as u16);
    }

    fn write_i4(&mut self, value: i32) {
        self.write_u4(value as u32);
    }

    fn write_bytes(&mut self, bytes: &[u8]) {
        self.data.extend_from_slice(bytes);
    }

    fn align_to_four(&mut self) {
        while !self.data.len().is_multiple_of(4) {
            self.data.push(0);
        }
    }

    fn finish(self) -> Vec<u8> {
        self.data
    }
}

fn write_instruction(writer: &mut VecWriter, instruction: &Instruction) -> PyResult<()> {
    let opcode = if instruction.is_wide() {
        writer.write_u1(WIDE_PREFIX);
        let wide_opcode = instruction
            .opcode
            .checked_sub(WIDE_PREFIX as u16)
            .ok_or_else(|| {
                PyValueError::new_err(format!(
                    "Invalid synthetic wide opcode: {}",
                    instruction.opcode
                ))
            })?;
        if wide_opcode > u8::MAX as u16 {
            return Err(PyValueError::new_err(format!(
                "Synthetic wide opcode out of range: {}",
                instruction.opcode
            )));
        }
        writer.write_u1(wide_opcode as u8);
        wide_opcode
    } else {
        if instruction.opcode > u8::MAX as u16 {
            return Err(PyValueError::new_err(format!(
                "Opcode {} requires wide prefix but instruction was not marked wide",
                instruction.opcode
            )));
        }
        writer.write_u1(instruction.opcode as u8);
        instruction.opcode
    };

    match &instruction.operands {
        InstructionOperands::None => {}
        InstructionOperands::LocalIndex(index) => writer.write_u1(*index as u8),
        InstructionOperands::LocalIndexWide(index) => writer.write_u2(*index),
        InstructionOperands::ConstPoolIndex(index) => {
            if opcode == 0x12 {
                writer.write_u1(*index as u8);
            } else {
                writer.write_u2(*index);
            }
        }
        InstructionOperands::ByteValue(value) => writer.write_i1(*value),
        InstructionOperands::ShortValue(value) => writer.write_i2(*value),
        InstructionOperands::Branch(offset) => writer.write_i2(*offset),
        InstructionOperands::BranchWide(offset) => writer.write_i4(*offset),
        InstructionOperands::IInc { index, value } => {
            if instruction.is_wide() {
                writer.write_u2(*index);
                writer.write_i2(*value);
            } else {
                writer.write_u1(*index as u8);
                writer.write_i1(*value as i8);
            }
        }
        InstructionOperands::InvokeDynamic { index, unused } => {
            writer.write_u2(*index);
            writer.write_bytes(unused);
        }
        InstructionOperands::InvokeInterface {
            index,
            count,
            unused,
        } => {
            writer.write_u2(*index);
            writer.write_u1(*count);
            writer.write_u1(*unused);
        }
        InstructionOperands::NewArray(array_type) => writer.write_u1(*array_type as u8),
        InstructionOperands::MultiANewArray { index, dimensions } => {
            writer.write_u2(*index);
            writer.write_u1(*dimensions);
        }
        InstructionOperands::LookupSwitch { default, pairs } => {
            writer.align_to_four();
            writer.write_i4(*default);
            writer.write_u4(pairs.len() as u32);
            for pair in pairs {
                writer.write_i4(pair.match_value);
                writer.write_i4(pair.offset);
            }
        }
        InstructionOperands::TableSwitch {
            default,
            low,
            high,
            offsets,
        } => {
            writer.align_to_four();
            writer.write_i4(*default);
            writer.write_i4(*low);
            writer.write_i4(*high);
            for offset in offsets {
                writer.write_i4(*offset);
            }
        }
    }
    Ok(())
}

fn write_code_instructions(instructions: &[Instruction]) -> PyResult<Vec<u8>> {
    let mut writer = VecWriter::new();
    for instruction in instructions {
        write_instruction(&mut writer, instruction)?;
    }
    Ok(writer.finish())
}

struct PythonInstructions<'py> {
    insn_info_type: Bound<'py, PyAny>,
    array_type: Bound<'py, PyAny>,
    insn_info: Bound<'py, PyAny>,
    local_index: Bound<'py, PyAny>,
    local_index_w: Bound<'py, PyAny>,
    const_pool_index: Bound<'py, PyAny>,
    byte_value: Bound<'py, PyAny>,
    short_value: Bound<'py, PyAny>,
    branch: Bound<'py, PyAny>,
    branch_w: Bound<'py, PyAny>,
    iinc: Bound<'py, PyAny>,
    iinc_w: Bound<'py, PyAny>,
    invoke_dynamic: Bound<'py, PyAny>,
    invoke_interface: Bound<'py, PyAny>,
    multi_anew_array: Bound<'py, PyAny>,
    new_array: Bound<'py, PyAny>,
    lookup_switch: Bound<'py, PyAny>,
    table_switch: Bound<'py, PyAny>,
    match_offset_pair: Bound<'py, PyAny>,
}

impl<'py> PythonInstructions<'py> {
    fn load(py: Python<'py>) -> PyResult<Self> {
        let module = py.import("pytecode.classfile.instructions")?;
        Ok(Self {
            insn_info_type: module.getattr("InsnInfoType")?,
            array_type: module.getattr("ArrayType")?,
            insn_info: module.getattr("InsnInfo")?,
            local_index: module.getattr("LocalIndex")?,
            local_index_w: module.getattr("LocalIndexW")?,
            const_pool_index: module.getattr("ConstPoolIndex")?,
            byte_value: module.getattr("ByteValue")?,
            short_value: module.getattr("ShortValue")?,
            branch: module.getattr("Branch")?,
            branch_w: module.getattr("BranchW")?,
            iinc: module.getattr("IInc")?,
            iinc_w: module.getattr("IIncW")?,
            invoke_dynamic: module.getattr("InvokeDynamic")?,
            invoke_interface: module.getattr("InvokeInterface")?,
            multi_anew_array: module.getattr("MultiANewArray")?,
            new_array: module.getattr("NewArray")?,
            lookup_switch: module.getattr("LookupSwitch")?,
            table_switch: module.getattr("TableSwitch")?,
            match_offset_pair: module.getattr("MatchOffsetPair")?,
        })
    }

    fn instruction_type(&self, opcode: u16) -> PyResult<Bound<'py, PyAny>> {
        self.insn_info_type.call1((opcode,))
    }

    fn array_type_value(&self, atype: ArrayType) -> PyResult<Bound<'py, PyAny>> {
        self.array_type.call1((atype as u8,))
    }

    fn to_python(&self, instruction: &Instruction) -> PyResult<Py<PyAny>> {
        let bytecode_offset = instruction.bytecode_offset as usize;
        let object = match &instruction.operands {
            InstructionOperands::None => self
                .insn_info
                .call1((self.instruction_type(instruction.opcode)?, bytecode_offset))?,
            InstructionOperands::LocalIndex(index) => self.local_index.call1((
                self.instruction_type(instruction.opcode)?,
                bytecode_offset,
                *index,
            ))?,
            InstructionOperands::LocalIndexWide(index) => self.local_index_w.call1((
                self.instruction_type(instruction.opcode)?,
                bytecode_offset,
                *index,
            ))?,
            InstructionOperands::ConstPoolIndex(index) => self.const_pool_index.call1((
                self.instruction_type(instruction.opcode)?,
                bytecode_offset,
                *index,
            ))?,
            InstructionOperands::ByteValue(value) => self.byte_value.call1((
                self.instruction_type(instruction.opcode)?,
                bytecode_offset,
                *value,
            ))?,
            InstructionOperands::ShortValue(value) => self.short_value.call1((
                self.instruction_type(instruction.opcode)?,
                bytecode_offset,
                *value,
            ))?,
            InstructionOperands::Branch(offset) => self.branch.call1((
                self.instruction_type(instruction.opcode)?,
                bytecode_offset,
                *offset,
            ))?,
            InstructionOperands::BranchWide(offset) => self.branch_w.call1((
                self.instruction_type(instruction.opcode)?,
                bytecode_offset,
                *offset,
            ))?,
            InstructionOperands::IInc { index, value } => {
                if instruction.is_wide() {
                    self.iinc_w.call1((
                        self.instruction_type(instruction.opcode)?,
                        bytecode_offset,
                        *index,
                        *value,
                    ))?
                } else {
                    self.iinc.call1((
                        self.instruction_type(instruction.opcode)?,
                        bytecode_offset,
                        *index,
                        *value,
                    ))?
                }
            }
            InstructionOperands::InvokeDynamic { index, unused } => self.invoke_dynamic.call1((
                self.instruction_type(instruction.opcode)?,
                bytecode_offset,
                *index,
                PyBytes::new(self.insn_info_type.py(), unused),
            ))?,
            InstructionOperands::InvokeInterface {
                index,
                count,
                unused,
            } => self.invoke_interface.call1((
                self.instruction_type(instruction.opcode)?,
                bytecode_offset,
                *index,
                *count,
                PyBytes::new(self.insn_info_type.py(), &[*unused]),
            ))?,
            InstructionOperands::MultiANewArray { index, dimensions } => {
                self.multi_anew_array.call1((
                    self.instruction_type(instruction.opcode)?,
                    bytecode_offset,
                    *index,
                    *dimensions,
                ))?
            }
            InstructionOperands::NewArray(atype) => self.new_array.call1((
                self.instruction_type(instruction.opcode)?,
                bytecode_offset,
                self.array_type_value(*atype)?,
            ))?,
            InstructionOperands::LookupSwitch { default, pairs } => {
                let py = self.insn_info_type.py();
                let py_pairs = PyList::empty(py);
                for pair in pairs {
                    py_pairs.append(
                        self.match_offset_pair
                            .call1((pair.match_value, pair.offset))?,
                    )?;
                }
                self.lookup_switch.call1((
                    self.instruction_type(instruction.opcode)?,
                    bytecode_offset,
                    *default,
                    pairs.len(),
                    py_pairs,
                ))?
            }
            InstructionOperands::TableSwitch {
                default,
                low,
                high,
                offsets,
            } => self.table_switch.call1((
                self.instruction_type(instruction.opcode)?,
                bytecode_offset,
                *default,
                *low,
                *high,
                offsets.clone(),
            ))?,
        };
        Ok(object.unbind())
    }

    fn instruction_from_python(&self, item: &Bound<'py, PyAny>) -> PyResult<Instruction> {
        let opcode = item.getattr("type")?.extract::<u16>()?;
        let bytecode_offset = item.getattr("bytecode_offset")?.extract::<u32>()?;
        let type_name = item.get_type().name()?.to_str()?.to_owned();
        let operands = match type_name.as_str() {
            "InsnInfo" => InstructionOperands::None,
            "LocalIndex" => {
                InstructionOperands::LocalIndex(item.getattr("index")?.extract::<u16>()?)
            }
            "LocalIndexW" => {
                InstructionOperands::LocalIndexWide(item.getattr("index")?.extract::<u16>()?)
            }
            "ConstPoolIndex" => {
                InstructionOperands::ConstPoolIndex(item.getattr("index")?.extract::<u16>()?)
            }
            "ByteValue" => InstructionOperands::ByteValue(item.getattr("value")?.extract::<i8>()?),
            "ShortValue" => {
                InstructionOperands::ShortValue(item.getattr("value")?.extract::<i16>()?)
            }
            "Branch" => InstructionOperands::Branch(item.getattr("offset")?.extract::<i16>()?),
            "BranchW" => InstructionOperands::BranchWide(item.getattr("offset")?.extract::<i32>()?),
            "IInc" | "IIncW" => InstructionOperands::IInc {
                index: item.getattr("index")?.extract::<u16>()?,
                value: item.getattr("value")?.extract::<i16>()?,
            },
            "InvokeDynamic" => {
                let unused = item.getattr("unused")?.extract::<Vec<u8>>()?;
                let unused: [u8; 2] = unused.try_into().map_err(|_| {
                    PyValueError::new_err("InvokeDynamic unused bytes must be exactly 2 bytes")
                })?;
                InstructionOperands::InvokeDynamic {
                    index: item.getattr("index")?.extract::<u16>()?,
                    unused,
                }
            }
            "InvokeInterface" => {
                let unused = item.getattr("unused")?.extract::<Vec<u8>>()?;
                let unused: [u8; 1] = unused.try_into().map_err(|_| {
                    PyValueError::new_err("InvokeInterface unused bytes must be exactly 1 byte")
                })?;
                InstructionOperands::InvokeInterface {
                    index: item.getattr("index")?.extract::<u16>()?,
                    count: item.getattr("count")?.extract::<u8>()?,
                    unused: unused[0],
                }
            }
            "MultiANewArray" => InstructionOperands::MultiANewArray {
                index: item.getattr("index")?.extract::<u16>()?,
                dimensions: item.getattr("dimensions")?.extract::<u8>()?,
            },
            "NewArray" => InstructionOperands::NewArray(array_type_from_u8(
                item.getattr("atype")?.extract::<u8>()?,
            )?),
            "LookupSwitch" => {
                let pair_items = item.getattr("pairs")?;
                let mut pairs = Vec::new();
                for pair in pair_items.try_iter()? {
                    let pair = pair?;
                    pairs.push(MatchOffsetPair {
                        match_value: pair.getattr("match")?.extract::<i32>()?,
                        offset: pair.getattr("offset")?.extract::<i32>()?,
                    });
                }
                InstructionOperands::LookupSwitch {
                    default: item.getattr("default")?.extract::<i32>()?,
                    pairs,
                }
            }
            "TableSwitch" => InstructionOperands::TableSwitch {
                default: item.getattr("default")?.extract::<i32>()?,
                low: item.getattr("low")?.extract::<i32>()?,
                high: item.getattr("high")?.extract::<i32>()?,
                offsets: item.getattr("offsets")?.extract::<Vec<i32>>()?,
            },
            _ => {
                return Err(PyValueError::new_err(format!(
                    "Unsupported instruction type: {type_name}"
                )));
            }
        };
        Ok(Instruction {
            opcode,
            bytecode_offset,
            operands,
        })
    }
}

#[pyfunction]
pub fn read_code_bytes<'py>(py: Python<'py>, data: &[u8]) -> PyResult<Py<PyList>> {
    let types = PythonInstructions::load(py)?;
    let instructions = read_code_instructions(data)?;
    let result = PyList::empty(py);
    for instruction in &instructions {
        result.append(types.to_python(instruction)?)?;
    }
    Ok(result.unbind())
}

#[pyfunction]
pub fn write_code_bytes<'py>(py: Python<'py>, code: &Bound<'py, PyAny>) -> PyResult<Py<PyBytes>> {
    let types = PythonInstructions::load(py)?;
    let mut instructions = Vec::new();
    for item in code.try_iter()? {
        instructions.push(types.instruction_from_python(&item?)?);
    }
    let bytes = write_code_instructions(&instructions)?;
    Ok(PyBytes::new(py, &bytes).unbind())
}

pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent.py(), "code")?;
    m.add_function(wrap_pyfunction!(read_code_bytes, &m)?)?;
    m.add_function(wrap_pyfunction!(write_code_bytes, &m)?)?;
    crate::register_submodule(parent, &m, "pytecode._rust.classfile.code")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_wide_and_return_instruction_stream() {
        let data = [WIDE_PREFIX, 0x15, 0x01, 0x00, 0xB1];
        let instructions = read_code_instructions(&data).expect("parse should succeed");
        assert_eq!(instructions.len(), 2);
        assert_eq!(instructions[0].opcode, WIDE_PREFIX as u16 + 0x15);
        assert_eq!(instructions[0].bytecode_offset, 0);
        assert_eq!(
            instructions[0].operands,
            InstructionOperands::LocalIndexWide(0x0100)
        );
        assert_eq!(instructions[1].bytecode_offset, 4);
    }

    #[test]
    fn reads_lookupswitch_with_alignment() {
        let data = [
            0xAB, 0x00, 0x00, 0x00, // opcode + padding
            0x00, 0x00, 0x00, 0x05, // default
            0x00, 0x00, 0x00, 0x01, // npairs
            0x00, 0x00, 0x00, 0x07, // match
            0x00, 0x00, 0x00, 0x09, // offset
        ];
        let instructions = read_code_instructions(&data).expect("parse should succeed");
        assert_eq!(instructions.len(), 1);
        assert_eq!(
            instructions[0].operands,
            InstructionOperands::LookupSwitch {
                default: 5,
                pairs: vec![MatchOffsetPair {
                    match_value: 7,
                    offset: 9
                }]
            }
        );
    }

    #[test]
    fn writes_tableswitch_with_alignment() {
        let bytes = write_code_instructions(&[Instruction {
            opcode: 0xAA,
            bytecode_offset: 0,
            operands: InstructionOperands::TableSwitch {
                default: 1,
                low: 2,
                high: 3,
                offsets: vec![4, 5],
            },
        }])
        .expect("write should succeed");

        assert_eq!(
            bytes,
            vec![
                0xAA, 0x00, 0x00, 0x00, // opcode + padding
                0x00, 0x00, 0x00, 0x01, // default
                0x00, 0x00, 0x00, 0x02, // low
                0x00, 0x00, 0x00, 0x03, // high
                0x00, 0x00, 0x00, 0x04, // offsets[0]
                0x00, 0x00, 0x00, 0x05, // offsets[1]
            ]
        );
    }
}
