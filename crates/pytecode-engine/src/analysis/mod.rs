mod hierarchy;
mod simulation;
mod verify;

use simulation::ClassContext;
pub(crate) use simulation::{
    initial_method_locals, recompute_and_normalize_code, recompute_frames_for_class,
};
pub use simulation::{recompute_frames, simulate};

pub use hierarchy::{
    ClassResolver, InheritedMethod, JAVA_LANG_OBJECT, MappingClassResolver, ResolvedClass,
    ResolvedMethod, common_superclass, find_overridden_methods, is_subtype, iter_superclasses,
    iter_supertypes,
};
pub use verify::{
    Category, Diagnostic, FailFastError, Location, Severity, verify_classfile,
    verify_classfile_with_options, verify_classmodel, verify_classmodel_with_options,
};

use crate::constants::{FieldAccessFlags, MethodAccessFlags};
use crate::descriptors::{
    BaseType, FieldDescriptor, ReturnType, parameter_slot_count, parse_field_descriptor,
    parse_method_descriptor,
};
use crate::model::{
    BranchInsn, ClassModel, CodeItem, CodeModel, FieldInsn, IIncInsn, InterfaceMethodInsn,
    InvokeDynamicInsn, LdcInsn, LdcValue, MethodInsn, MultiANewArrayInsn, TypeInsn, VarInsn,
};
use crate::names::is_valid_internal_name;
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
            let category2 = is_category2(&value);
            stack.push(value);
            if category2 {
                stack.push(VType::Top);
            }
        }
        Self {
            stack,
            locals: self.locals.clone(),
        }
    }

    pub fn pop(&self, slots: usize) -> Result<(Self, Vec<VType>), AnalysisError> {
        self.check_stack_groups(&[slots])?;
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
        let needed = depth.checked_add(1).ok_or(AnalysisError::StackUnderflow {
            needed: usize::MAX,
            available: self.stack.len(),
        })?;
        let index = self
            .stack
            .len()
            .checked_sub(needed)
            .ok_or(AnalysisError::StackUnderflow {
                needed,
                available: self.stack.len(),
            })?;
        Ok(&self.stack[index])
    }

    pub fn set_local(&self, index: usize, value: VType) -> Self {
        let width = if is_category2(&value) { 2 } else { 1 };
        let needed = index
            .checked_add(width)
            .expect("local slot index overflows usize");
        let mut locals = self.locals.clone();
        if locals.len() < needed {
            locals.resize(needed, VType::Top);
        }
        if index > 0 && is_category2(&locals[index - 1]) {
            locals[index - 1] = VType::Top;
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

    // Groups are expressed in slots, from the top of the stack down. Every
    // boundary must fall between complete values, never inside a long/double.
    fn check_stack_groups(&self, groups: &[usize]) -> Result<(), AnalysisError> {
        let needed: usize = groups.iter().sum();
        if self.stack.len() < needed {
            return Err(AnalysisError::StackUnderflow {
                needed,
                available: self.stack.len(),
            });
        }
        let mut end = self.stack.len();
        for &group in groups {
            let start = end - group;
            while end > start {
                match &self.stack[end - 1] {
                    VType::Top if end >= start + 2 && is_category2(&self.stack[end - 2]) => {
                        end -= 2
                    }
                    VType::Top | VType::Long | VType::Double => {
                        return Err(type_error("stack operation splits a category-2 value"));
                    }
                    _ => end -= 1,
                }
            }
        }
        Ok(())
    }

    fn pop_reference(&self) -> Result<(Self, VType), AnalysisError> {
        self.check_stack_groups(&[1])?;
        let value = self.stack[self.stack.len() - 1].clone();
        if !is_reference(&value) {
            return Err(type_error(format!(
                "expected reference type, found {value:?}"
            )));
        }
        Ok((
            Self {
                stack: self.stack[..self.stack.len() - 1].to_vec(),
                locals: self.locals.clone(),
            },
            value,
        ))
    }

    fn pop_typed(
        &self,
        expected: &VType,
        resolver: Option<&dyn ClassResolver>,
    ) -> Result<Self, AnalysisError> {
        let width = if is_category2(expected) { 2 } else { 1 };
        self.check_stack_groups(&[width])?;
        let actual = &self.stack[self.stack.len() - width];
        require_type(actual, expected, resolver)?;
        Ok(Self {
            stack: self.stack[..self.stack.len() - width].to_vec(),
            locals: self.locals.clone(),
        })
    }
}

fn type_error(reason: impl Into<String>) -> AnalysisError {
    AnalysisError::InvalidControlFlow {
        reason: reason.into(),
    }
}

fn require_type(
    actual: &VType,
    expected: &VType,
    resolver: Option<&dyn ClassResolver>,
) -> Result<(), AnalysisError> {
    if actual == expected {
        return Ok(());
    }
    match (actual, expected) {
        (VType::Null, VType::Object(_)) => Ok(()),
        (VType::Object(actual_name), VType::Object(expected_name)) => {
            if expected_name == JAVA_LANG_OBJECT {
                return Ok(());
            }
            if actual_name.starts_with('[') {
                if expected_name.starts_with('[') {
                    let fallback = MappingClassResolver::default();
                    let effective = resolver.unwrap_or(&fallback);
                    return match hierarchy::is_subtype(effective, actual_name, expected_name) {
                        Ok(true) | Err(AnalysisError::UnresolvedClass { .. }) => Ok(()),
                        Ok(false) => Err(type_error(format!(
                            "expected {expected:?}, found {actual:?}"
                        ))),
                        Err(error) => Err(error),
                    };
                }
                if matches!(
                    expected_name.as_str(),
                    "java/lang/Cloneable" | "java/io/Serializable"
                ) {
                    return Ok(());
                }
                return Err(type_error(format!(
                    "expected {expected:?}, found {actual:?}"
                )));
            }
            if expected_name.starts_with('[') {
                return Err(type_error(format!(
                    "expected {expected:?}, found {actual:?}"
                )));
            }
            let Some(resolver) = resolver else {
                return Ok(());
            };
            if let Some(resolved_expected) = resolver.resolve_class(expected_name)
                && resolved_expected.is_interface()
            {
                return Ok(());
            }
            match hierarchy::is_subtype(resolver, actual_name, expected_name) {
                Ok(true) | Err(AnalysisError::UnresolvedClass { .. }) => Ok(()),
                Ok(false) => Err(type_error(format!(
                    "expected {expected:?}, found {actual:?}"
                ))),
                Err(error) => Err(error),
            }
        }
        _ => Err(type_error(format!(
            "expected {expected:?}, found {actual:?}"
        ))),
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
    if matches!(left, VType::Null) && matches!(right, VType::Object(_)) {
        return right.clone();
    }
    if matches!(right, VType::Null) && matches!(left, VType::Object(_)) {
        return left.clone();
    }
    match (left, right) {
        (VType::Object(left_name), VType::Object(right_name)) => VType::Object(
            hierarchy::common_reference_type(resolver, left_name, right_name)
                .unwrap_or_else(|_| JAVA_LANG_OBJECT.to_owned()),
        ),
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
                if label_positions.insert(label.clone(), code_index).is_some() {
                    return Err(AnalysisError::InvalidControlFlow {
                        reason: "duplicate label in CodeModel instructions".to_owned(),
                    });
                }
                pending_labels.push(label.clone());
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
                let mut seen_targets = HashSet::new();
                if seen_targets.insert(*default_target) {
                    nodes[node_index].normal_successors.push(*default_target);
                }
                for (_, label) in &switch.pairs {
                    let Some(Some(target)) = label_targets.get(label) else {
                        return Err(AnalysisError::InvalidControlFlow {
                            reason: "switch target label does not point to an instruction"
                                .to_owned(),
                        });
                    };
                    if seen_targets.insert(*target) {
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
                let mut seen_targets = HashSet::new();
                if seen_targets.insert(*default_target) {
                    nodes[node_index].normal_successors.push(*default_target);
                }
                for label in &switch.targets {
                    let Some(Some(target)) = label_targets.get(label) else {
                        return Err(AnalysisError::InvalidControlFlow {
                            reason: "switch target label does not point to an instruction"
                                .to_owned(),
                        });
                    };
                    if seen_targets.insert(*target) {
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
        if start >= end {
            return Err(AnalysisError::InvalidControlFlow {
                reason: "exception handler start label must precede end label".to_owned(),
            });
        }
        let Some(Some(target)) = label_targets.get(&handler.handler) else {
            return Err(AnalysisError::InvalidControlFlow {
                reason: "exception handler label does not point to an instruction".to_owned(),
            });
        };
        let first_node = executable_indices.partition_point(|&code_index| code_index < start);
        let stop_node = executable_indices.partition_point(|&code_index| code_index < end);
        if first_node >= stop_node {
            return Err(AnalysisError::InvalidControlFlow {
                reason: "exception handler range covers no executable instructions".to_owned(),
            });
        }
        for node in &mut nodes[first_node..stop_node] {
            let edge = ExceptionSuccessor {
                target: *target,
                catch_type: handler.catch_type.clone(),
            };
            if !node.exception_successors.contains(&edge) {
                node.exception_successors.push(edge);
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

fn simulate_item(
    item: &CodeItem,
    state: &FrameState,
    class: ClassContext<'_>,
    code_index: usize,
    next_node: Option<usize>,
    expected_return: Option<&VType>,
    resolver: Option<&dyn ClassResolver>,
) -> Result<FrameState, AnalysisError> {
    match item {
        CodeItem::Raw(raw) => simulate_raw_opcode(
            raw.opcode(),
            state,
            raw,
            code_index,
            expected_return,
            resolver,
        ),
        CodeItem::Var(var) => simulate_var(var, state, resolver),
        CodeItem::IInc(iinc) => simulate_iinc(iinc, state),
        CodeItem::Field(field) => simulate_field(field, state, class, resolver),
        CodeItem::Method(method) => simulate_method(method, state, class, resolver),
        CodeItem::InterfaceMethod(method) => simulate_interface_method(method, state, resolver),
        CodeItem::InvokeDynamic(insn) => simulate_invokedynamic(insn, state, resolver),
        CodeItem::Type(insn) => simulate_type(insn, state, code_index, resolver),
        CodeItem::Ldc(insn) => simulate_ldc(insn, state),
        CodeItem::MultiANewArray(insn) => simulate_multianewarray(insn, state, resolver),
        CodeItem::Branch(branch) => simulate_branch(branch, state, next_node, resolver),
        CodeItem::LookupSwitch(_) | CodeItem::TableSwitch(_) => {
            state.pop_typed(&VType::Integer, resolver)
        }
        CodeItem::Label(_) => Err(AnalysisError::InvalidControlFlow {
            reason: "labels do not execute".to_owned(),
        }),
    }
}

fn simulate_var(
    var: &VarInsn,
    state: &FrameState,
    resolver: Option<&dyn ClassResolver>,
) -> Result<FrameState, AnalysisError> {
    let primitive = match var.opcode {
        0x15 | 0x36 => Some(VType::Integer),
        0x16 | 0x37 => Some(VType::Long),
        0x17 | 0x38 => Some(VType::Float),
        0x18 | 0x39 => Some(VType::Double),
        _ => None,
    };
    if let Some(expected) = primitive {
        if var.opcode <= 0x18 {
            let actual = state.get_local(var.slot as usize)?;
            require_type(actual, &expected, resolver)?;
            if is_category2(actual) && state.locals.get(var.slot as usize + 1) != Some(&VType::Top)
            {
                return Err(type_error("category-2 local is missing its second slot"));
            }
            return Ok(state.push([expected]));
        }
        return Ok(state
            .pop_typed(&expected, resolver)?
            .set_local(var.slot as usize, expected));
    }
    match var.opcode {
        0x19 => {
            let value = state.get_local(var.slot as usize)?;
            if !is_reference(value) {
                return Err(type_error("aload requires a reference local"));
            }
            Ok(state.push([value.clone()]))
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
            if !is_reference(&value) && !matches!(value, VType::ReturnAddress(_)) {
                return Err(type_error("astore requires a reference or returnAddress"));
            }
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

fn simulate_field(
    field: &FieldInsn,
    state: &FrameState,
    class: ClassContext<'_>,
    resolver: Option<&dyn ClassResolver>,
) -> Result<FrameState, AnalysisError> {
    let descriptor = parse_field_descriptor(&field.descriptor).map_err(|error| {
        AnalysisError::InvalidControlFlow {
            reason: error.to_string(),
        }
    })?;
    let field_type = vtype_from_descriptor(&descriptor);
    match field.opcode {
        0xB2 => Ok(state.push([field_type])),
        0xB3 => state.pop_typed(&field_type, resolver),
        0xB4 => {
            let next = state.pop_typed(&VType::Object(field.owner.clone()), resolver)?;
            Ok(next.push([field_type]))
        }
        0xB5 => {
            let next = state.pop_typed(&field_type, resolver)?;
            // A constructor may assign its own instance fields before invoking super.
            if next.peek(0)? == &VType::UninitializedThis {
                if field.owner != class.name
                    || class.model.is_some_and(|model| {
                        !model.fields.iter().any(|declared| {
                            declared.name == field.name
                                && declared.descriptor == field.descriptor
                                && !declared.access_flags.contains(FieldAccessFlags::STATIC)
                        })
                    })
                {
                    return Err(type_error(
                        "putfield before initialization requires an instance field declared by the current class",
                    ));
                }
                Ok(next.pop(1)?.0)
            } else {
                next.pop_typed(&VType::Object(field.owner.clone()), resolver)
            }
        }
        opcode => Err(AnalysisError::UnsupportedInstruction { opcode }),
    }
}

fn simulate_method(
    method: &MethodInsn,
    state: &FrameState,
    class: ClassContext<'_>,
    resolver: Option<&dyn ClassResolver>,
) -> Result<FrameState, AnalysisError> {
    if !matches!(method.opcode, 0xB6..=0xB8) {
        return Err(AnalysisError::UnsupportedInstruction {
            opcode: method.opcode,
        });
    }
    if method.opcode == 0xB6 && method.is_interface {
        return Err(type_error(
            "invokevirtual (0xb6) cannot target an InterfaceMethodref",
        ));
    }
    let parsed = parse_method_descriptor(&method.descriptor).map_err(|error| {
        AnalysisError::InvalidControlFlow {
            reason: error.to_string(),
        }
    })?;
    if method.name == "<clinit>"
        || (method.name == "<init>"
            && (method.opcode != 0xb7
                || method.is_interface
                || parsed.return_type != ReturnType::Void))
    {
        return Err(type_error(
            "constructor invocation requires non-interface invokespecial and a void descriptor; <clinit> cannot be invoked",
        ));
    }
    let max_param_slots = if method.opcode == 0xB8 { 255 } else { 254 };
    if parameter_slot_count(&parsed) > max_param_slots {
        return Err(type_error(
            "method invocation parameter slots exceed JVMS maximum",
        ));
    }
    let after_args = pop_arguments(state, &parsed.parameter_types, resolver)?;
    let receiver_state = if method.opcode == 0xB8 {
        after_args
    } else {
        let (after_receiver, receiver) = after_args.pop(1)?;
        if method.opcode == 0xB7 && method.name == "<init>" {
            let replacement = match receiver.first() {
                Some(VType::UninitializedThis) => {
                    if method.owner != class.name
                        && class
                            .model
                            .is_some_and(|model| model.super_name.as_deref() != Some(&method.owner))
                    {
                        return Err(type_error(
                            "constructor receiver requires the current class or its direct superclass",
                        ));
                    }
                    VType::Object(class.name.to_owned())
                }
                Some(VType::Uninitialized { class_name, .. }) if *class_name == method.owner => {
                    VType::Object(class_name.clone())
                }
                _ => {
                    return Err(type_error(
                        "invokespecial <init> requires a matching uninitialized receiver",
                    ));
                }
            };
            initialize_receiver(&after_receiver, receiver.first().cloned(), replacement)
        } else {
            require_type(&receiver[0], &VType::Object(method.owner.clone()), resolver)?;
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
    resolver: Option<&dyn ClassResolver>,
) -> Result<FrameState, AnalysisError> {
    if matches!(method.name.as_str(), "<init>" | "<clinit>") {
        return Err(type_error(
            "invokeinterface cannot invoke <init> or <clinit>",
        ));
    }
    let parsed = parse_method_descriptor(&method.descriptor).map_err(|error| {
        AnalysisError::InvalidControlFlow {
            reason: error.to_string(),
        }
    })?;
    if parameter_slot_count(&parsed) > 254 {
        return Err(type_error(
            "invokeinterface parameter slots exceed 254 (count limit 255)",
        ));
    }
    let after_args = pop_arguments(state, &parsed.parameter_types, resolver)?;
    let after_receiver = after_args.pop_typed(&VType::Object(method.owner.clone()), resolver)?;
    match &parsed.return_type {
        ReturnType::Void => Ok(after_receiver),
        ReturnType::Field(field) => Ok(after_receiver.push([vtype_from_descriptor(field)])),
    }
}

fn simulate_invokedynamic(
    insn: &InvokeDynamicInsn,
    state: &FrameState,
    resolver: Option<&dyn ClassResolver>,
) -> Result<FrameState, AnalysisError> {
    if matches!(insn.name.as_str(), "<init>" | "<clinit>") {
        return Err(type_error("invokedynamic cannot invoke <init> or <clinit>"));
    }
    let parsed = parse_method_descriptor(&insn.descriptor).map_err(|error| {
        AnalysisError::InvalidControlFlow {
            reason: error.to_string(),
        }
    })?;
    if parameter_slot_count(&parsed) > 255 {
        return Err(type_error("invokedynamic parameter slots exceed 255"));
    }
    let after_args = pop_arguments(state, &parsed.parameter_types, resolver)?;
    match &parsed.return_type {
        ReturnType::Void => Ok(after_args),
        ReturnType::Field(field) => Ok(after_args.push([vtype_from_descriptor(field)])),
    }
}

fn pop_arguments(
    state: &FrameState,
    parameters: &[FieldDescriptor],
    resolver: Option<&dyn ClassResolver>,
) -> Result<FrameState, AnalysisError> {
    let mut next = state.clone();
    for parameter in parameters.iter().rev() {
        next = next.pop_typed(&vtype_from_descriptor(parameter), resolver)?;
    }
    Ok(next)
}

fn simulate_type(
    insn: &TypeInsn,
    state: &FrameState,
    code_index: usize,
    resolver: Option<&dyn ClassResolver>,
) -> Result<FrameState, AnalysisError> {
    match insn.opcode {
        0xBB => {
            if insn.descriptor.starts_with('[') || !is_valid_internal_name(&insn.descriptor) {
                return Err(type_error(format!(
                    "new requires a non-array internal class name, got {}",
                    insn.descriptor
                )));
            }
            if state.stack.iter().any(|value| {
                matches!(
                    value,
                    VType::Uninitialized { code_index: existing, .. } if *existing == code_index
                )
            }) {
                return Err(type_error(
                    "uninitialized reference from the same new instruction already exists on stack",
                ));
            }
            let sanitized_locals = state
                .locals
                .iter()
                .map(|value| match value {
                    VType::Uninitialized {
                        code_index: existing,
                        ..
                    } if *existing == code_index => VType::Top,
                    other => other.clone(),
                })
                .collect();
            let next_state = FrameState {
                stack: state.stack.clone(),
                locals: sanitized_locals,
            };
            Ok(next_state.push([VType::Uninitialized {
                code_index,
                class_name: insn.descriptor.clone(),
            }]))
        }
        0xBD => {
            let next = state.pop_typed(&VType::Integer, resolver)?;
            let array_descriptor = anewarray_descriptor(&insn.descriptor);
            let array_dims = array_descriptor.bytes().take_while(|&b| b == b'[').count();
            if array_dims > 255 || parse_field_descriptor(&array_descriptor).is_err() {
                return Err(type_error(format!(
                    "anewarray descriptor is invalid or exceeds 255 dimensions: {array_descriptor}"
                )));
            }
            Ok(next.push([VType::Object(array_descriptor)]))
        }
        0xC0 => {
            let next = state.pop_typed(&VType::Object(JAVA_LANG_OBJECT.to_owned()), resolver)?;
            Ok(next.push([VType::Object(insn.descriptor.clone())]))
        }
        0xC1 => {
            let next = state.pop_typed(&VType::Object(JAVA_LANG_OBJECT.to_owned()), resolver)?;
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
    resolver: Option<&dyn ClassResolver>,
) -> Result<FrameState, AnalysisError> {
    if insn.dimensions == 0 {
        return Err(type_error("multianewarray dimensions must be at least 1"));
    }
    let array_dims = insn.descriptor.bytes().take_while(|&b| b == b'[').count();
    if array_dims < usize::from(insn.dimensions)
        || array_dims > 255
        || parse_field_descriptor(&insn.descriptor).is_err()
    {
        return Err(type_error(format!(
            "multianewarray descriptor {} does not have at least {} valid dimensions",
            insn.descriptor, insn.dimensions
        )));
    }
    let mut next = state.clone();
    for _ in 0..insn.dimensions {
        next = next.pop_typed(&VType::Integer, resolver)?;
    }
    Ok(next.push([VType::Object(insn.descriptor.clone())]))
}

fn simulate_branch(
    branch: &BranchInsn,
    state: &FrameState,
    next_node: Option<usize>,
    resolver: Option<&dyn ClassResolver>,
) -> Result<FrameState, AnalysisError> {
    match branch.opcode {
        0x99..=0x9E => state.pop_typed(&VType::Integer, resolver),
        0xC6 | 0xC7 => Ok(state.pop_reference()?.0),
        0x9F..=0xA4 => state
            .pop_typed(&VType::Integer, resolver)?
            .pop_typed(&VType::Integer, resolver),
        0xA5 | 0xA6 => Ok(state.pop_reference()?.0.pop_reference()?.0),
        0xA7 | 0xC8 => Ok(state.clone()),
        0xA8 | 0xC9 => simulate_jsr(state, next_node),
        opcode => Err(AnalysisError::UnsupportedInstruction { opcode }),
    }
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
    expected_return: Option<&VType>,
    resolver: Option<&dyn ClassResolver>,
) -> Result<FrameState, AnalysisError> {
    match opcode {
        0x59 => state.check_stack_groups(&[1])?,
        0x5a | 0x5f => state.check_stack_groups(&[1, 1])?,
        0x5b => state.check_stack_groups(&[1, 2])?,
        0x5c => state.check_stack_groups(&[2])?,
        0x5d => state.check_stack_groups(&[2, 1])?,
        0x5e => state.check_stack_groups(&[2, 2])?,
        _ => {}
    }
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
        0x74 => Ok(state
            .pop_typed(&VType::Integer, resolver)?
            .push([VType::Integer])),
        0x75 => Ok(state.pop_typed(&VType::Long, resolver)?.push([VType::Long])),
        0x76 => Ok(state
            .pop_typed(&VType::Float, resolver)?
            .push([VType::Float])),
        0x77 => Ok(state
            .pop_typed(&VType::Double, resolver)?
            .push([VType::Double])),
        0x79 | 0x7b | 0x7d => Ok(state
            .pop_typed(&VType::Integer, resolver)?
            .pop_typed(&VType::Long, resolver)?
            .push([VType::Long])),
        0x60 | 0x64 | 0x68 | 0x6C | 0x70 | 0x78 | 0x7A | 0x7C | 0x7E | 0x80 | 0x82 => Ok(state
            .pop_typed(&VType::Integer, resolver)?
            .pop_typed(&VType::Integer, resolver)?
            .push([VType::Integer])),
        0x61 | 0x65 | 0x69 | 0x6D | 0x71 | 0x7F | 0x81 | 0x83 => Ok(state
            .pop_typed(&VType::Long, resolver)?
            .pop_typed(&VType::Long, resolver)?
            .push([VType::Long])),
        0x62 | 0x66 | 0x6A | 0x6E | 0x72 => Ok(state
            .pop_typed(&VType::Float, resolver)?
            .pop_typed(&VType::Float, resolver)?
            .push([VType::Float])),
        0x63 | 0x67 | 0x6B | 0x6F | 0x73 => Ok(state
            .pop_typed(&VType::Double, resolver)?
            .pop_typed(&VType::Double, resolver)?
            .push([VType::Double])),
        0x85 => Ok(state
            .pop_typed(&VType::Integer, resolver)?
            .push([VType::Long])),
        0x86 => Ok(state
            .pop_typed(&VType::Integer, resolver)?
            .push([VType::Float])),
        0x87 => Ok(state
            .pop_typed(&VType::Integer, resolver)?
            .push([VType::Double])),
        0x88 => Ok(state
            .pop_typed(&VType::Long, resolver)?
            .push([VType::Integer])),
        0x89 => Ok(state
            .pop_typed(&VType::Long, resolver)?
            .push([VType::Float])),
        0x8A => Ok(state
            .pop_typed(&VType::Long, resolver)?
            .push([VType::Double])),
        0x8B => Ok(state
            .pop_typed(&VType::Float, resolver)?
            .push([VType::Integer])),
        0x91..=0x93 => Ok(state
            .pop_typed(&VType::Integer, resolver)?
            .push([VType::Integer])),
        0x8C => Ok(state
            .pop_typed(&VType::Float, resolver)?
            .push([VType::Long])),
        0x8D => Ok(state
            .pop_typed(&VType::Float, resolver)?
            .push([VType::Double])),
        0x8E => Ok(state
            .pop_typed(&VType::Double, resolver)?
            .push([VType::Integer])),
        0x8F => Ok(state
            .pop_typed(&VType::Double, resolver)?
            .push([VType::Long])),
        0x90 => Ok(state
            .pop_typed(&VType::Double, resolver)?
            .push([VType::Float])),
        0x94..=0x98 => {
            let expected = match opcode {
                0x94 => VType::Long,
                0x95 | 0x96 => VType::Float,
                _ => VType::Double,
            };
            Ok(state
                .pop_typed(&expected, resolver)?
                .pop_typed(&expected, resolver)?
                .push([VType::Integer]))
        }
        0x2e..=0x35 => {
            let next = state.pop_typed(&VType::Integer, resolver)?;
            let (next, array) = pop_array(&next)?;
            Ok(next.push([array_element_type(opcode, &array)?]))
        }
        0x4f..=0x56 => {
            let expected = match opcode {
                0x50 => VType::Long,
                0x51 => VType::Float,
                0x52 => VType::Double,
                0x53 => VType::Object(JAVA_LANG_OBJECT.to_owned()),
                _ => VType::Integer,
            };
            let next = state
                .pop_typed(&expected, resolver)?
                .pop_typed(&VType::Integer, resolver)?;
            let (next, array) = pop_array(&next)?;
            array_element_type(opcode - 0x21, &array)?;
            Ok(next)
        }
        0xAC => state.pop_typed(&VType::Integer, resolver),
        0xAD => state.pop_typed(&VType::Long, resolver),
        0xAE => state.pop_typed(&VType::Float, resolver),
        0xAF => state.pop_typed(&VType::Double, resolver),
        0xB0 => state.pop_typed(
            expected_return.unwrap_or(&VType::Object(JAVA_LANG_OBJECT.to_owned())),
            resolver,
        ),
        0xBF => state.pop_typed(&VType::Object("java/lang/Throwable".to_owned()), resolver),
        0xC2 | 0xC3 => Ok(state.pop_reference()?.0),
        0xBE => Ok(pop_array(state)?.0.push([VType::Integer])),
        0xB1 => Ok(state.clone()),
        0xBC => {
            let next = state.pop_typed(&VType::Integer, resolver)?;
            let descriptor = match raw {
                crate::raw::Instruction::NewArray(insn) => newarray_descriptor(insn.atype),
                _ => return Err(AnalysisError::UnsupportedInstruction { opcode }),
            };
            Ok(next.push([VType::Object(descriptor)]))
        }
        opcode => Err(AnalysisError::UnsupportedInstruction { opcode }),
    }
}

fn pop_array(state: &FrameState) -> Result<(FrameState, VType), AnalysisError> {
    let value = state.peek(0)?.clone();
    if !matches!(&value, VType::Null)
        && !matches!(&value, VType::Object(name) if name.starts_with('['))
    {
        return Err(type_error("array instruction requires an array reference"));
    }
    Ok((state.pop(1)?.0, value))
}

fn array_element_type(opcode: u8, array: &VType) -> Result<VType, AnalysisError> {
    let (descriptors, value): (&[&str], _) = match opcode {
        0x2e => (&["[I"], VType::Integer),
        0x2f => (&["[J"], VType::Long),
        0x30 => (&["[F"], VType::Float),
        0x31 => (&["[D"], VType::Double),
        0x32 => (&[], aaload_type(array)),
        0x33 => (&["[B", "[Z"], VType::Integer),
        0x34 => (&["[C"], VType::Integer),
        0x35 => (&["[S"], VType::Integer),
        _ => return Err(AnalysisError::UnsupportedInstruction { opcode }),
    };
    if let VType::Object(name) = array {
        let valid = if opcode == 0x32 {
            name.starts_with("[L") || name.starts_with("[[")
        } else {
            descriptors.contains(&name.as_str())
        };
        if !valid {
            return Err(type_error(format!(
                "array type {name} does not match opcode {opcode:02x}"
            )));
        }
    }
    Ok(value)
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
    matches!(opcode, 0xA7 | 0xA8 | 0xC8 | 0xC9)
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
    if matches!(array, VType::Null) {
        return VType::Null;
    }
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
