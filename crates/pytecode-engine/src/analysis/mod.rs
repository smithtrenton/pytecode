mod hierarchy;
mod verify;

pub use hierarchy::{
    ClassResolver, InheritedMethod, JAVA_LANG_OBJECT, MappingClassResolver, ResolvedClass,
    ResolvedMethod, common_superclass, find_overridden_methods, is_subtype, iter_superclasses,
    iter_supertypes,
};
pub use verify::{
    Category, Diagnostic, FailFastError, Location, Severity, verify_classfile,
    verify_classfile_with_options, verify_classmodel, verify_classmodel_with_options,
};

use crate::constants::MethodAccessFlags;
use crate::descriptors::{
    BaseType, FieldDescriptor, ReturnType, parse_field_descriptor, parse_method_descriptor,
    slot_size,
};
use crate::model::{
    BranchInsn, CodeItem, CodeModel, FieldInsn, IIncInsn, InterfaceMethodInsn, InvokeDynamicInsn,
    LdcInsn, LdcValue, MethodInsn, MultiANewArrayInsn, TypeInsn, VarInsn,
};
use crate::raw::ArrayType;
use std::collections::{HashMap, HashSet, VecDeque};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AnalysisError {
    #[error("invalid control flow: {reason}")]
    InvalidControlFlow { reason: String },
    #[error("unsupported instruction opcode 0x{opcode:02x}")]
    UnsupportedInstruction { opcode: u8 },
    #[error("stack underflow: needed {needed} slots but only {available} available")]
    StackUnderflow { needed: usize, available: usize },
    #[error("invalid local slot {index}: {reason}")]
    InvalidLocal { index: usize, reason: String },
    #[error("type merge error: {reason}")]
    TypeMerge { reason: String },
    #[error("unresolved class {class_name}")]
    UnresolvedClass { class_name: String },
    #[error("hierarchy cycle detected: {cycle:?}")]
    HierarchyCycle { cycle: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum VType {
    Top,
    Integer,
    Float,
    Long,
    Double,
    Null,
    ReturnAddress(Vec<usize>),
    Object(String),
    UninitializedThis,
    Uninitialized {
        code_index: usize,
        class_name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FrameState {
    pub stack: Vec<VType>,
    pub locals: Vec<VType>,
}

impl FrameState {
    pub fn push(&self, types: impl IntoIterator<Item = VType>) -> Self {
        let mut stack = self.stack.clone();
        for value in types {
            stack.push(value.clone());
            if is_category2(&value) {
                stack.push(VType::Top);
            }
        }
        Self {
            stack,
            locals: self.locals.clone(),
        }
    }

    pub fn pop(&self, slots: usize) -> Result<(Self, Vec<VType>), AnalysisError> {
        if self.stack.len() < slots {
            return Err(AnalysisError::StackUnderflow {
                needed: slots,
                available: self.stack.len(),
            });
        }
        let remaining = self.stack[..self.stack.len() - slots].to_vec();
        let popped = self.stack[self.stack.len() - slots..]
            .iter()
            .rev()
            .cloned()
            .collect::<Vec<_>>();
        Ok((
            Self {
                stack: remaining,
                locals: self.locals.clone(),
            },
            popped,
        ))
    }

    pub fn peek(&self, depth: usize) -> Result<&VType, AnalysisError> {
        let index =
            self.stack
                .len()
                .checked_sub(depth + 1)
                .ok_or(AnalysisError::StackUnderflow {
                    needed: depth + 1,
                    available: self.stack.len(),
                })?;
        Ok(&self.stack[index])
    }

    pub fn set_local(&self, index: usize, value: VType) -> Self {
        let width = if is_category2(&value) { 2 } else { 1 };
        let mut locals = self.locals.clone();
        if locals.len() < index + width {
            locals.resize(index + width, VType::Top);
        }
        locals[index] = value.clone();
        if is_category2(&value) {
            locals[index + 1] = VType::Top;
        }
        Self {
            stack: self.stack.clone(),
            locals,
        }
    }

    pub fn get_local(&self, index: usize) -> Result<&VType, AnalysisError> {
        let value = self.locals.get(index).ok_or(AnalysisError::InvalidLocal {
            index,
            reason: "slot is out of range".to_owned(),
        })?;
        if matches!(value, VType::Top) {
            return Err(AnalysisError::InvalidLocal {
                index,
                reason: "slot is not initialized".to_owned(),
            });
        }
        Ok(value)
    }

    pub fn stack_depth(&self) -> usize {
        self.stack.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExceptionSuccessor {
    pub target: usize,
    pub catch_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlFlowNode {
    pub node_index: usize,
    pub code_index: usize,
    pub normal_successors: Vec<usize>,
    pub exception_successors: Vec<ExceptionSuccessor>,
    pub is_block_start: bool,
    pub is_jump_target: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlFlowGraph {
    pub entry_node: usize,
    pub nodes: Vec<ControlFlowNode>,
    pub code_index_to_node: HashMap<usize, usize>,
    pub label_targets: HashMap<crate::model::Label, Option<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulationResult {
    pub cfg: ControlFlowGraph,
    pub entry_frames: Vec<Option<FrameState>>,
    pub max_stack: u16,
    pub max_locals: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackMapFrameState {
    pub code_index: usize,
    pub locals: Vec<VType>,
    pub stack: Vec<VType>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameComputationResult {
    pub max_stack: u16,
    pub max_locals: u16,
    pub frames: Vec<StackMapFrameState>,
}

pub fn is_category2(value: &VType) -> bool {
    matches!(value, VType::Long | VType::Double)
}

pub fn is_reference(value: &VType) -> bool {
    matches!(
        value,
        VType::Null | VType::Object(_) | VType::UninitializedThis | VType::Uninitialized { .. }
    )
}

pub fn vtype_from_descriptor(descriptor: &FieldDescriptor) -> VType {
    match descriptor {
        FieldDescriptor::Base(
            BaseType::Boolean | BaseType::Byte | BaseType::Char | BaseType::Short | BaseType::Int,
        ) => VType::Integer,
        FieldDescriptor::Base(BaseType::Long) => VType::Long,
        FieldDescriptor::Base(BaseType::Float) => VType::Float,
        FieldDescriptor::Base(BaseType::Double) => VType::Double,
        FieldDescriptor::Object(object) => VType::Object(object.class_name.clone()),
        FieldDescriptor::Array(array) => VType::Object(format!(
            "[{}",
            descriptor_component(array.component_type.as_ref())
        )),
    }
}

pub fn vtype_from_field_descriptor_str(descriptor: &str) -> Result<VType, crate::EngineError> {
    Ok(vtype_from_descriptor(&parse_field_descriptor(descriptor)?))
}

pub fn merge_vtypes(left: &VType, right: &VType, resolver: Option<&dyn ClassResolver>) -> VType {
    if left == right {
        return left.clone();
    }
    if let (VType::ReturnAddress(left_targets), VType::ReturnAddress(right_targets)) = (left, right)
    {
        return VType::ReturnAddress(merge_return_targets(left_targets, right_targets));
    }
    if matches!(left, VType::Null) && is_reference(right) {
        return right.clone();
    }
    if matches!(right, VType::Null) && is_reference(left) {
        return left.clone();
    }
    match (left, right) {
        (VType::Object(left_name), VType::Object(right_name)) => {
            if let Some(resolver) = resolver {
                match common_superclass(resolver, left_name, right_name) {
                    Ok(name) => VType::Object(name),
                    Err(_) => VType::Object(JAVA_LANG_OBJECT.to_owned()),
                }
            } else {
                VType::Object(JAVA_LANG_OBJECT.to_owned())
            }
        }
        _ => VType::Top,
    }
}

pub fn build_cfg(code: &CodeModel) -> Result<ControlFlowGraph, AnalysisError> {
    let executable_indices = code
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(index, item)| (!matches!(item, CodeItem::Label(_))).then_some(index))
        .collect::<Vec<_>>();
    if executable_indices.is_empty() {
        return Err(AnalysisError::InvalidControlFlow {
            reason: "code model contains no executable instructions".to_owned(),
        });
    }

    let mut code_index_to_node = HashMap::new();
    for (node_index, code_index) in executable_indices.iter().copied().enumerate() {
        code_index_to_node.insert(code_index, node_index);
    }

    let mut label_targets = HashMap::new();
    let mut label_positions = HashMap::new();
    let mut pending_labels = Vec::new();
    for (code_index, item) in code.instructions.iter().enumerate() {
        match item {
            CodeItem::Label(label) => {
                pending_labels.push(label.clone());
                label_positions.insert(label.clone(), code_index);
            }
            _ => {
                let node_index = code_index_to_node[&code_index];
                for label in pending_labels.drain(..) {
                    label_targets.insert(label, Some(node_index));
                }
            }
        }
    }
    for label in pending_labels {
        label_targets.insert(label.clone(), None);
        label_positions.insert(label, code.instructions.len());
    }

    let mut jump_targets = HashSet::new();
    let mut leaders = HashSet::new();
    leaders.insert(0_usize);

    for (node_index, code_index) in executable_indices.iter().copied().enumerate() {
        let item = &code.instructions[code_index];
        for label in branch_targets(item) {
            let Some(Some(target)) = label_targets.get(&label) else {
                return Err(AnalysisError::InvalidControlFlow {
                    reason: "control-flow target label does not point to an instruction".to_owned(),
                });
            };
            jump_targets.insert(*target);
            leaders.insert(*target);
        }
        if terminates_block(item) && node_index + 1 < executable_indices.len() {
            leaders.insert(node_index + 1);
        }
    }

    for handler in &code.exception_handlers {
        let Some(Some(target)) = label_targets.get(&handler.handler) else {
            return Err(AnalysisError::InvalidControlFlow {
                reason: "exception handler target label does not point to an instruction"
                    .to_owned(),
            });
        };
        jump_targets.insert(*target);
        leaders.insert(*target);
    }

    let mut nodes = executable_indices
        .iter()
        .copied()
        .enumerate()
        .map(|(node_index, code_index)| ControlFlowNode {
            node_index,
            code_index,
            normal_successors: Vec::new(),
            exception_successors: Vec::new(),
            is_block_start: leaders.contains(&node_index),
            is_jump_target: jump_targets.contains(&node_index),
        })
        .collect::<Vec<_>>();

    for (node_index, code_index) in executable_indices.iter().copied().enumerate() {
        let item = &code.instructions[code_index];
        let next = (node_index + 1 < executable_indices.len()).then_some(node_index + 1);
        match item {
            CodeItem::Branch(branch) => {
                let Some(Some(target)) = label_targets.get(&branch.target) else {
                    return Err(AnalysisError::InvalidControlFlow {
                        reason: "branch target label does not point to an instruction".to_owned(),
                    });
                };
                nodes[node_index].normal_successors.push(*target);
                if !is_unconditional_branch(branch.opcode)
                    && let Some(next) = next
                {
                    nodes[node_index].normal_successors.push(next);
                }
            }
            CodeItem::LookupSwitch(switch) => {
                let Some(Some(default_target)) = label_targets.get(&switch.default_target) else {
                    return Err(AnalysisError::InvalidControlFlow {
                        reason: "switch default target label does not point to an instruction"
                            .to_owned(),
                    });
                };
                nodes[node_index].normal_successors.push(*default_target);
                for (_, label) in &switch.pairs {
                    let Some(Some(target)) = label_targets.get(label) else {
                        return Err(AnalysisError::InvalidControlFlow {
                            reason: "switch target label does not point to an instruction"
                                .to_owned(),
                        });
                    };
                    if !nodes[node_index].normal_successors.contains(target) {
                        nodes[node_index].normal_successors.push(*target);
                    }
                }
            }
            CodeItem::TableSwitch(switch) => {
                let Some(Some(default_target)) = label_targets.get(&switch.default_target) else {
                    return Err(AnalysisError::InvalidControlFlow {
                        reason: "switch default target label does not point to an instruction"
                            .to_owned(),
                    });
                };
                nodes[node_index].normal_successors.push(*default_target);
                for label in &switch.targets {
                    let Some(Some(target)) = label_targets.get(label) else {
                        return Err(AnalysisError::InvalidControlFlow {
                            reason: "switch target label does not point to an instruction"
                                .to_owned(),
                        });
                    };
                    if !nodes[node_index].normal_successors.contains(target) {
                        nodes[node_index].normal_successors.push(*target);
                    }
                }
            }
            _ if is_terminal_item(item) => {}
            _ => {
                if let Some(next) = next {
                    nodes[node_index].normal_successors.push(next);
                }
            }
        }
    }

    for handler in &code.exception_handlers {
        let start = label_positions
            .get(&handler.start)
            .copied()
            .ok_or_else(|| AnalysisError::InvalidControlFlow {
                reason: "exception start label missing".to_owned(),
            })?;
        let end = label_positions.get(&handler.end).copied().ok_or_else(|| {
            AnalysisError::InvalidControlFlow {
                reason: "exception end label missing".to_owned(),
            }
        })?;
        let Some(Some(target)) = label_targets.get(&handler.handler) else {
            return Err(AnalysisError::InvalidControlFlow {
                reason: "exception handler label does not point to an instruction".to_owned(),
            });
        };
        for (node_index, code_index) in executable_indices.iter().copied().enumerate() {
            if code_index < start || code_index >= end {
                continue;
            }
            let edge = ExceptionSuccessor {
                target: *target,
                catch_type: handler.catch_type.clone(),
            };
            if !nodes[node_index].exception_successors.contains(&edge) {
                nodes[node_index].exception_successors.push(edge);
            }
        }
    }

    Ok(ControlFlowGraph {
        entry_node: 0,
        nodes,
        code_index_to_node,
        label_targets,
    })
}

pub fn simulate(
    code: &CodeModel,
    class_name: &str,
    method_name: &str,
    descriptor: &str,
    access_flags: MethodAccessFlags,
    resolver: Option<&dyn ClassResolver>,
) -> Result<SimulationResult, AnalysisError> {
    let cfg = build_cfg(code)?;
    let mut entry_frames = vec![None; cfg.nodes.len()];
    let mut worklist = VecDeque::new();
    entry_frames[cfg.entry_node] = Some(initial_frame(
        class_name,
        method_name,
        descriptor,
        access_flags,
    )?);
    worklist.push_back(cfg.entry_node);
    let mut max_stack = 0_usize;
    let mut max_locals = 0_usize;

    while let Some(node_index) = worklist.pop_front() {
        let state =
            entry_frames[node_index]
                .clone()
                .ok_or_else(|| AnalysisError::InvalidControlFlow {
                    reason: "worklist node missing entry frame".to_owned(),
                })?;
        max_stack = max_stack.max(state.stack_depth());
        max_locals = max_locals.max(state.locals.len());
        let code_index = cfg.nodes[node_index].code_index;
        let item = &code.instructions[code_index];

        for exception_edge in &cfg.nodes[node_index].exception_successors {
            let stack = vec![match &exception_edge.catch_type {
                Some(catch_type) => VType::Object(catch_type.clone()),
                None => VType::Object("java/lang/Throwable".to_owned()),
            }];
            let handler_state = FrameState {
                stack,
                locals: state.locals.clone(),
            };
            propagate(
                exception_edge.target,
                handler_state,
                &mut entry_frames,
                &mut worklist,
                resolver,
            )?;
        }

        let next_state = simulate_item(
            item,
            &state,
            class_name,
            code_index,
            (node_index + 1 < cfg.nodes.len()).then_some(node_index + 1),
        )?;
        max_stack = max_stack.max(next_state.stack_depth());
        max_locals = max_locals.max(next_state.locals.len());
        let normal_successors = dynamic_successors(item, &state)
            .unwrap_or_else(|| cfg.nodes[node_index].normal_successors.clone());
        for successor in &normal_successors {
            propagate(
                *successor,
                next_state.clone(),
                &mut entry_frames,
                &mut worklist,
                resolver,
            )?;
        }
    }

    Ok(SimulationResult {
        cfg,
        entry_frames,
        max_stack: max_stack as u16,
        max_locals: max_locals as u16,
    })
}

pub fn recompute_frames(
    code: &CodeModel,
    class_name: &str,
    method_name: &str,
    descriptor: &str,
    access_flags: MethodAccessFlags,
    resolver: Option<&dyn ClassResolver>,
) -> Result<FrameComputationResult, AnalysisError> {
    let simulation = simulate(
        code,
        class_name,
        method_name,
        descriptor,
        access_flags,
        resolver,
    )?;
    let mut frames = Vec::new();
    for node in &simulation.cfg.nodes {
        if node.node_index == simulation.cfg.entry_node || !node.is_block_start {
            continue;
        }
        if let Some(frame) = &simulation.entry_frames[node.node_index] {
            frames.push(StackMapFrameState {
                code_index: node.code_index,
                locals: frame.locals.clone(),
                stack: frame.stack.clone(),
            });
        }
    }
    Ok(FrameComputationResult {
        max_stack: simulation.max_stack,
        max_locals: simulation.max_locals,
        frames,
    })
}

fn initial_frame(
    class_name: &str,
    method_name: &str,
    descriptor: &str,
    access_flags: MethodAccessFlags,
) -> Result<FrameState, AnalysisError> {
    let parsed =
        parse_method_descriptor(descriptor).map_err(|error| AnalysisError::InvalidControlFlow {
            reason: error.to_string(),
        })?;
    let mut locals = Vec::new();
    if !access_flags.contains(MethodAccessFlags::STATIC) {
        if method_name == "<init>" {
            locals.push(VType::UninitializedThis);
        } else {
            locals.push(VType::Object(class_name.to_owned()));
        }
    }
    for parameter in &parsed.parameter_types {
        let value = vtype_from_descriptor(parameter);
        locals.push(value.clone());
        if is_category2(&value) {
            locals.push(VType::Top);
        }
    }
    Ok(FrameState {
        stack: Vec::new(),
        locals,
    })
}

fn propagate(
    target: usize,
    candidate: FrameState,
    entry_frames: &mut [Option<FrameState>],
    worklist: &mut VecDeque<usize>,
    resolver: Option<&dyn ClassResolver>,
) -> Result<(), AnalysisError> {
    let changed = match &entry_frames[target] {
        Some(existing) => {
            let merged = merge_frames(existing, &candidate, resolver)?;
            if merged != *existing {
                entry_frames[target] = Some(merged);
                true
            } else {
                false
            }
        }
        None => {
            entry_frames[target] = Some(candidate);
            true
        }
    };
    if changed && !worklist.contains(&target) {
        worklist.push_back(target);
    }
    Ok(())
}

fn merge_frames(
    left: &FrameState,
    right: &FrameState,
    resolver: Option<&dyn ClassResolver>,
) -> Result<FrameState, AnalysisError> {
    if left.stack.len() != right.stack.len() {
        return Err(AnalysisError::TypeMerge {
            reason: format!(
                "stack depths differ at join point: {} vs {}",
                left.stack.len(),
                right.stack.len()
            ),
        });
    }
    let stack = left
        .stack
        .iter()
        .zip(&right.stack)
        .map(|(left, right)| merge_vtypes(left, right, resolver))
        .collect::<Vec<_>>();
    let max_locals = left.locals.len().max(right.locals.len());
    let mut locals = Vec::with_capacity(max_locals);
    for index in 0..max_locals {
        let left_value = left.locals.get(index).unwrap_or(&VType::Top);
        let right_value = right.locals.get(index).unwrap_or(&VType::Top);
        locals.push(merge_vtypes(left_value, right_value, resolver));
    }
    Ok(FrameState { stack, locals })
}

fn simulate_item(
    item: &CodeItem,
    state: &FrameState,
    class_name: &str,
    code_index: usize,
    next_node: Option<usize>,
) -> Result<FrameState, AnalysisError> {
    match item {
        CodeItem::Raw(raw) => simulate_raw_opcode(raw.opcode(), state, raw, code_index),
        CodeItem::Var(var) => simulate_var(var, state),
        CodeItem::IInc(iinc) => simulate_iinc(iinc, state),
        CodeItem::Field(field) => simulate_field(field, state),
        CodeItem::Method(method) => simulate_method(method, state, class_name),
        CodeItem::InterfaceMethod(method) => simulate_interface_method(method, state),
        CodeItem::InvokeDynamic(insn) => simulate_invokedynamic(insn, state),
        CodeItem::Type(insn) => simulate_type(insn, state, code_index),
        CodeItem::Ldc(insn) => simulate_ldc(insn, state),
        CodeItem::MultiANewArray(insn) => simulate_multianewarray(insn, state),
        CodeItem::Branch(branch) => simulate_branch(branch, state, next_node),
        CodeItem::LookupSwitch(_) | CodeItem::TableSwitch(_) => {
            let (next, _) = state.pop(1)?;
            Ok(next)
        }
        CodeItem::Label(_) => Err(AnalysisError::InvalidControlFlow {
            reason: "labels do not execute".to_owned(),
        }),
    }
}

fn simulate_var(var: &VarInsn, state: &FrameState) -> Result<FrameState, AnalysisError> {
    match var.opcode {
        0x15 => Ok(state.push([VType::Integer])),
        0x16 => Ok(state.push([VType::Long])),
        0x17 => Ok(state.push([VType::Float])),
        0x18 => Ok(state.push([VType::Double])),
        0x19 => Ok(state.push([state.get_local(var.slot as usize)?.clone()])),
        0x36 => {
            let (next, _) = state.pop(1)?;
            Ok(next.set_local(var.slot as usize, VType::Integer))
        }
        0x37 => {
            let (next, _) = state.pop(2)?;
            Ok(next.set_local(var.slot as usize, VType::Long))
        }
        0x38 => {
            let (next, _) = state.pop(1)?;
            Ok(next.set_local(var.slot as usize, VType::Float))
        }
        0x39 => {
            let (next, _) = state.pop(2)?;
            Ok(next.set_local(var.slot as usize, VType::Double))
        }
        0x3A => {
            let (next, popped) = state.pop(1)?;
            let value = popped
                .first()
                .cloned()
                .ok_or(AnalysisError::StackUnderflow {
                    needed: 1,
                    available: 0,
                })?;
            Ok(next.set_local(var.slot as usize, value))
        }
        0xA9 => {
            let value = state.get_local(var.slot as usize)?;
            if matches!(value, VType::ReturnAddress(targets) if !targets.is_empty()) {
                Ok(state.clone())
            } else {
                Err(AnalysisError::InvalidLocal {
                    index: var.slot as usize,
                    reason: "ret requires returnAddress local".to_owned(),
                })
            }
        }
        opcode => Err(AnalysisError::UnsupportedInstruction { opcode }),
    }
}

fn simulate_iinc(iinc: &IIncInsn, state: &FrameState) -> Result<FrameState, AnalysisError> {
    let value = state.get_local(iinc.slot as usize)?;
    if !matches!(value, VType::Integer) {
        return Err(AnalysisError::InvalidLocal {
            index: iinc.slot as usize,
            reason: "iinc requires integer local".to_owned(),
        });
    }
    Ok(state.clone())
}

fn simulate_field(field: &FieldInsn, state: &FrameState) -> Result<FrameState, AnalysisError> {
    let descriptor = parse_field_descriptor(&field.descriptor).map_err(|error| {
        AnalysisError::InvalidControlFlow {
            reason: error.to_string(),
        }
    })?;
    let field_type = vtype_from_descriptor(&descriptor);
    let field_slots = slot_size(&descriptor);
    match field.opcode {
        0xB2 => Ok(state.push([field_type])),
        0xB3 => {
            let (next, _) = state.pop(field_slots)?;
            Ok(next)
        }
        0xB4 => {
            let (next, _) = state.pop(1)?;
            Ok(next.push([field_type]))
        }
        0xB5 => {
            let (next, _) = state.pop(field_slots + 1)?;
            Ok(next)
        }
        opcode => Err(AnalysisError::UnsupportedInstruction { opcode }),
    }
}

fn simulate_method(
    method: &MethodInsn,
    state: &FrameState,
    _class_name: &str,
) -> Result<FrameState, AnalysisError> {
    let parsed = parse_method_descriptor(&method.descriptor).map_err(|error| {
        AnalysisError::InvalidControlFlow {
            reason: error.to_string(),
        }
    })?;
    let arg_slots = parsed.parameter_types.iter().map(slot_size).sum::<usize>();
    let (after_args, _) = state.pop(arg_slots)?;
    let receiver_state = if method.opcode == 0xB8 {
        after_args
    } else {
        let (after_receiver, receiver) = after_args.pop(1)?;
        if method.opcode == 0xB7 && method.name == "<init>" {
            let replacement = VType::Object(method.owner.clone());
            initialize_receiver(&after_receiver, receiver.first().cloned(), replacement)
        } else {
            after_receiver
        }
    };
    match &parsed.return_type {
        ReturnType::Void => Ok(receiver_state),
        ReturnType::Field(field) => Ok(receiver_state.push([vtype_from_descriptor(field)])),
    }
}

fn simulate_interface_method(
    method: &InterfaceMethodInsn,
    state: &FrameState,
) -> Result<FrameState, AnalysisError> {
    let parsed = parse_method_descriptor(&method.descriptor).map_err(|error| {
        AnalysisError::InvalidControlFlow {
            reason: error.to_string(),
        }
    })?;
    let arg_slots = parsed.parameter_types.iter().map(slot_size).sum::<usize>();
    let (after_args, _) = state.pop(arg_slots)?;
    let (after_receiver, _) = after_args.pop(1)?;
    match &parsed.return_type {
        ReturnType::Void => Ok(after_receiver),
        ReturnType::Field(field) => Ok(after_receiver.push([vtype_from_descriptor(field)])),
    }
}

fn simulate_invokedynamic(
    insn: &InvokeDynamicInsn,
    state: &FrameState,
) -> Result<FrameState, AnalysisError> {
    let parsed = parse_method_descriptor(&insn.descriptor).map_err(|error| {
        AnalysisError::InvalidControlFlow {
            reason: error.to_string(),
        }
    })?;
    let arg_slots = parsed.parameter_types.iter().map(slot_size).sum::<usize>();
    let (after_args, _) = state.pop(arg_slots)?;
    match &parsed.return_type {
        ReturnType::Void => Ok(after_args),
        ReturnType::Field(field) => Ok(after_args.push([vtype_from_descriptor(field)])),
    }
}

fn simulate_type(
    insn: &TypeInsn,
    state: &FrameState,
    code_index: usize,
) -> Result<FrameState, AnalysisError> {
    match insn.opcode {
        0xBB => Ok(state.push([VType::Uninitialized {
            code_index,
            class_name: insn.descriptor.clone(),
        }])),
        0xBD => {
            let (next, _) = state.pop(1)?;
            Ok(next.push([VType::Object(anewarray_descriptor(&insn.descriptor))]))
        }
        0xC0 => {
            let (next, _) = state.pop(1)?;
            Ok(next.push([VType::Object(insn.descriptor.clone())]))
        }
        0xC1 => {
            let (next, _) = state.pop(1)?;
            Ok(next.push([VType::Integer]))
        }
        opcode => Err(AnalysisError::UnsupportedInstruction { opcode }),
    }
}

fn simulate_ldc(insn: &LdcInsn, state: &FrameState) -> Result<FrameState, AnalysisError> {
    let value = match &insn.value {
        LdcValue::Int(_) => VType::Integer,
        LdcValue::FloatBits(_) => VType::Float,
        LdcValue::Long(_) => VType::Long,
        LdcValue::DoubleBits(_) => VType::Double,
        LdcValue::String(_) => VType::Object("java/lang/String".to_owned()),
        LdcValue::Class(_) => VType::Object("java/lang/Class".to_owned()),
        LdcValue::MethodType(_) => VType::Object("java/lang/invoke/MethodType".to_owned()),
        LdcValue::MethodHandle(_) => VType::Object("java/lang/invoke/MethodHandle".to_owned()),
        LdcValue::Dynamic(dynamic) => {
            let descriptor = parse_field_descriptor(&dynamic.descriptor).map_err(|error| {
                AnalysisError::InvalidControlFlow {
                    reason: error.to_string(),
                }
            })?;
            vtype_from_descriptor(&descriptor)
        }
    };
    Ok(state.push([value]))
}

fn simulate_multianewarray(
    insn: &MultiANewArrayInsn,
    state: &FrameState,
) -> Result<FrameState, AnalysisError> {
    let (next, _) = state.pop(insn.dimensions as usize)?;
    Ok(next.push([VType::Object(insn.descriptor.clone())]))
}

fn simulate_branch(
    branch: &BranchInsn,
    state: &FrameState,
    next_node: Option<usize>,
) -> Result<FrameState, AnalysisError> {
    let pops = match branch.opcode {
        0x99..=0x9E | 0xC6 | 0xC7 => 1,
        0x9F..=0xA6 => 2,
        0xA7 => 0,
        0xA8 => return simulate_jsr(state, next_node),
        opcode => return Err(AnalysisError::UnsupportedInstruction { opcode }),
    };
    Ok(state.pop(pops)?.0)
}

fn simulate_jsr(state: &FrameState, next_node: Option<usize>) -> Result<FrameState, AnalysisError> {
    let return_target = next_node.ok_or_else(|| AnalysisError::InvalidControlFlow {
        reason: "jsr/jsr_w requires a reachable continuation instruction".to_owned(),
    })?;
    Ok(state.push([VType::ReturnAddress(vec![return_target])]))
}

fn dynamic_successors(item: &CodeItem, state: &FrameState) -> Option<Vec<usize>> {
    match item {
        CodeItem::Var(var) if var.opcode == 0xA9 => match state.get_local(var.slot as usize) {
            Ok(VType::ReturnAddress(targets)) => Some(targets.clone()),
            _ => Some(Vec::new()),
        },
        _ => None,
    }
}

fn simulate_raw_opcode(
    opcode: u8,
    state: &FrameState,
    raw: &crate::raw::Instruction,
    _code_index: usize,
) -> Result<FrameState, AnalysisError> {
    match opcode {
        0x00 => Ok(state.clone()),
        0x01 => Ok(state.push([VType::Null])),
        0x02..=0x08 | 0x10 | 0x11 => Ok(state.push([VType::Integer])),
        0x09..=0x0A => Ok(state.push([VType::Long])),
        0x0B..=0x0D => Ok(state.push([VType::Float])),
        0x0E..=0x0F => Ok(state.push([VType::Double])),
        0x57 => Ok(state.pop(1)?.0),
        0x58 => Ok(state.pop(2)?.0),
        0x59 => {
            let value = state.peek(0)?.clone();
            Ok(FrameState {
                stack: [state.stack.clone(), vec![value]].concat(),
                locals: state.locals.clone(),
            })
        }
        0x5A => {
            let v1 = state.peek(0)?.clone();
            let v2 = state.peek(1)?.clone();
            Ok(FrameState {
                stack: [
                    state.stack[..state.stack.len() - 2].to_vec(),
                    vec![v1.clone(), v2, v1],
                ]
                .concat(),
                locals: state.locals.clone(),
            })
        }
        0x5B => {
            let v1 = state.peek(0)?.clone();
            let v2 = state.peek(1)?.clone();
            let v3 = state.peek(2)?.clone();
            Ok(FrameState {
                stack: [
                    state.stack[..state.stack.len() - 3].to_vec(),
                    vec![v1.clone(), v3, v2, v1],
                ]
                .concat(),
                locals: state.locals.clone(),
            })
        }
        0x5C => {
            let v1 = state.peek(0)?.clone();
            let v2 = state.peek(1)?.clone();
            Ok(FrameState {
                stack: [state.stack.clone(), vec![v2, v1]].concat(),
                locals: state.locals.clone(),
            })
        }
        0x5D => {
            let v1 = state.peek(0)?.clone();
            let v2 = state.peek(1)?.clone();
            let v3 = state.peek(2)?.clone();
            Ok(FrameState {
                stack: [
                    state.stack[..state.stack.len() - 3].to_vec(),
                    vec![v2.clone(), v1.clone(), v3, v2, v1],
                ]
                .concat(),
                locals: state.locals.clone(),
            })
        }
        0x5E => {
            let v1 = state.peek(0)?.clone();
            let v2 = state.peek(1)?.clone();
            let v3 = state.peek(2)?.clone();
            let v4 = state.peek(3)?.clone();
            Ok(FrameState {
                stack: [
                    state.stack[..state.stack.len() - 4].to_vec(),
                    vec![v2.clone(), v1.clone(), v4, v3, v2, v1],
                ]
                .concat(),
                locals: state.locals.clone(),
            })
        }
        0x5F => {
            let v1 = state.peek(0)?.clone();
            let v2 = state.peek(1)?.clone();
            Ok(FrameState {
                stack: [state.stack[..state.stack.len() - 2].to_vec(), vec![v1, v2]].concat(),
                locals: state.locals.clone(),
            })
        }
        0x60 | 0x64 | 0x68 | 0x6C | 0x70 | 0x74 | 0x78 | 0x7A | 0x7C | 0x7E | 0x80 | 0x82 => {
            Ok(state.pop(2)?.0.push([VType::Integer]))
        }
        0x61 | 0x65 | 0x69 | 0x6D | 0x71 | 0x75 | 0x79 | 0x7B | 0x7D | 0x7F | 0x81 | 0x83 => {
            Ok(state.pop(4)?.0.push([VType::Long]))
        }
        0x62 | 0x66 | 0x6A | 0x6E | 0x72 | 0x76 => Ok(state.pop(2)?.0.push([VType::Float])),
        0x63 | 0x67 | 0x6B | 0x6F | 0x73 | 0x77 => Ok(state.pop(4)?.0.push([VType::Double])),
        0x85 => Ok(state.pop(1)?.0.push([VType::Long])),
        0x86 => Ok(state.pop(1)?.0.push([VType::Float])),
        0x87 => Ok(state.pop(1)?.0.push([VType::Double])),
        0x88 => Ok(state.pop(2)?.0.push([VType::Integer])),
        0x89 => Ok(state.pop(2)?.0.push([VType::Float])),
        0x8A => Ok(state.pop(2)?.0.push([VType::Double])),
        0x8B | 0x91 | 0x92 | 0x93 => Ok(state.pop(1)?.0.push([VType::Integer])),
        0x8C => Ok(state.pop(1)?.0.push([VType::Long])),
        0x8D => Ok(state.pop(1)?.0.push([VType::Double])),
        0x8E => Ok(state.pop(2)?.0.push([VType::Integer])),
        0x8F => Ok(state.pop(2)?.0.push([VType::Long])),
        0x90 => Ok(state.pop(2)?.0.push([VType::Float])),
        0x94..=0x98 => {
            let pops = if opcode == 0x94 || opcode >= 0x97 {
                4
            } else {
                2
            };
            Ok(state.pop(pops)?.0.push([VType::Integer]))
        }
        0x2E | 0x33..=0x35 => Ok(state.pop(2)?.0.push([VType::Integer])),
        0x2F => Ok(state.pop(2)?.0.push([VType::Long])),
        0x30 => Ok(state.pop(2)?.0.push([VType::Float])),
        0x31 => Ok(state.pop(2)?.0.push([VType::Double])),
        0x32 => {
            let (next, popped) = state.pop(2)?;
            let array = popped
                .get(1)
                .cloned()
                .unwrap_or(VType::Object(JAVA_LANG_OBJECT.to_owned()));
            Ok(next.push([aaload_type(&array)]))
        }
        0x4F | 0x51 | 0x53..=0x56 => Ok(state.pop(3)?.0),
        0x50 | 0x52 => Ok(state.pop(4)?.0),
        0xAC | 0xAE | 0xB0 | 0xBE | 0xC2 | 0xC3 => {
            let push = if opcode == 0xBE {
                Some(VType::Integer)
            } else {
                None
            };
            let next = state.pop(1)?.0;
            Ok(match push {
                Some(value) => next.push([value]),
                None => next,
            })
        }
        0xAD | 0xAF => Ok(state.pop(2)?.0),
        0xB1 => Ok(state.clone()),
        0xBF => Ok(state.pop(1)?.0),
        0xBC => {
            let (next, _) = state.pop(1)?;
            let descriptor = match raw {
                crate::raw::Instruction::NewArray(insn) => newarray_descriptor(insn.atype),
                _ => "[I".to_owned(),
            };
            Ok(next.push([VType::Object(descriptor)]))
        }
        opcode => Err(AnalysisError::UnsupportedInstruction { opcode }),
    }
}

fn initialize_receiver(
    state: &FrameState,
    receiver: Option<VType>,
    replacement: VType,
) -> FrameState {
    let Some(receiver) = receiver else {
        return state.clone();
    };
    match receiver {
        VType::UninitializedThis | VType::Uninitialized { .. } => FrameState {
            stack: state
                .stack
                .iter()
                .map(|value| {
                    if value == &receiver {
                        replacement.clone()
                    } else {
                        value.clone()
                    }
                })
                .collect(),
            locals: state
                .locals
                .iter()
                .map(|value| {
                    if value == &receiver {
                        replacement.clone()
                    } else {
                        value.clone()
                    }
                })
                .collect(),
        },
        _ => state.clone(),
    }
}

fn branch_targets(item: &CodeItem) -> Vec<crate::model::Label> {
    match item {
        CodeItem::Branch(branch) => vec![branch.target.clone()],
        CodeItem::LookupSwitch(switch) => {
            let mut labels = vec![switch.default_target.clone()];
            labels.extend(switch.pairs.iter().map(|(_, label)| label.clone()));
            labels
        }
        CodeItem::TableSwitch(switch) => {
            let mut labels = vec![switch.default_target.clone()];
            labels.extend(switch.targets.iter().cloned());
            labels
        }
        _ => Vec::new(),
    }
}

fn terminates_block(item: &CodeItem) -> bool {
    matches!(
        item,
        CodeItem::Branch(_) | CodeItem::LookupSwitch(_) | CodeItem::TableSwitch(_)
    ) || is_terminal_item(item)
}

fn is_terminal_item(item: &CodeItem) -> bool {
    matches!(
        item,
        CodeItem::Var(VarInsn { opcode: 0xA9, .. })
            | CodeItem::Raw(crate::raw::Instruction::Simple {
                opcode: 0xAC..=0xB1 | 0xBF,
                ..
            })
    )
}

fn is_unconditional_branch(opcode: u8) -> bool {
    matches!(opcode, 0xA7 | 0xA8)
}

fn merge_return_targets(left: &[usize], right: &[usize]) -> Vec<usize> {
    let mut targets = left.to_vec();
    for target in right {
        if !targets.contains(target) {
            targets.push(*target);
        }
    }
    targets.sort_unstable();
    targets.dedup();
    targets
}

fn descriptor_component(descriptor: &FieldDescriptor) -> String {
    match descriptor {
        FieldDescriptor::Base(base) => match base {
            BaseType::Boolean => "Z".to_owned(),
            BaseType::Byte => "B".to_owned(),
            BaseType::Char => "C".to_owned(),
            BaseType::Short => "S".to_owned(),
            BaseType::Int => "I".to_owned(),
            BaseType::Long => "J".to_owned(),
            BaseType::Float => "F".to_owned(),
            BaseType::Double => "D".to_owned(),
        },
        FieldDescriptor::Object(object) => format!("L{};", object.class_name),
        FieldDescriptor::Array(array) => {
            format!("[{}", descriptor_component(array.component_type.as_ref()))
        }
    }
}

fn newarray_descriptor(atype: ArrayType) -> String {
    match atype {
        ArrayType::Boolean => "[Z".to_owned(),
        ArrayType::Char => "[C".to_owned(),
        ArrayType::Float => "[F".to_owned(),
        ArrayType::Double => "[D".to_owned(),
        ArrayType::Byte => "[B".to_owned(),
        ArrayType::Short => "[S".to_owned(),
        ArrayType::Int => "[I".to_owned(),
        ArrayType::Long => "[J".to_owned(),
    }
}

fn anewarray_descriptor(descriptor: &str) -> String {
    if descriptor.starts_with('[') {
        format!("[{descriptor}")
    } else {
        format!("[L{descriptor};")
    }
}

fn aaload_type(array: &VType) -> VType {
    let VType::Object(class_name) = array else {
        return VType::Object(JAVA_LANG_OBJECT.to_owned());
    };
    if !class_name.starts_with('[') {
        return VType::Object(JAVA_LANG_OBJECT.to_owned());
    }
    let component = &class_name[1..];
    if component.starts_with('L') && component.ends_with(';') {
        return VType::Object(component[1..component.len() - 1].to_owned());
    }
    if component.starts_with('[') {
        return VType::Object(component.to_owned());
    }
    VType::Object(JAVA_LANG_OBJECT.to_owned())
}
