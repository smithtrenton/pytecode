use super::*;

pub fn simulate(
    code: &CodeModel,
    class_name: &str,
    method_name: &str,
    descriptor: &str,
    access_flags: MethodAccessFlags,
    resolver: Option<&dyn ClassResolver>,
) -> Result<SimulationResult, AnalysisError> {
    Ok(simulate_in_class(
        code,
        ClassContext {
            name: class_name,
            model: None,
        },
        method_name,
        descriptor,
        access_flags,
        resolver,
    )?
    .0)
}

#[derive(Clone, Copy)]
pub(super) struct ClassContext<'a> {
    pub(super) name: &'a str,
    pub(super) model: Option<&'a ClassModel>,
}

// JVMS flagThisUninit is independent of the locals: astore can remove its last
// alias without initializing the receiver. Keep it private to preserve FrameState.
#[derive(Clone)]
struct PendingFrame {
    frame: FrameState,
    this_uninitialized: bool,
}

fn simulate_in_class(
    code: &CodeModel,
    class: ClassContext<'_>,
    method_name: &str,
    descriptor: &str,
    access_flags: MethodAccessFlags,
    resolver: Option<&dyn ClassResolver>,
) -> Result<(SimulationResult, Vec<bool>), AnalysisError> {
    let class_name = class.name;
    let cfg = build_cfg(code)?;
    let return_type = parse_method_descriptor(descriptor)
        .map_err(|error| type_error(error.to_string()))?
        .return_type;
    if method_name == "<init>"
        && (return_type != ReturnType::Void || access_flags.contains(MethodAccessFlags::STATIC))
    {
        return Err(type_error(
            "constructor must be an instance method returning void",
        ));
    }
    let mut entry_frames = vec![None; cfg.nodes.len()];
    let mut worklist = VecDeque::new();
    let initial = initial_frame(class_name, method_name, descriptor, access_flags)?;
    entry_frames[cfg.entry_node] = Some(PendingFrame {
        this_uninitialized: initial.locals.contains(&VType::UninitializedThis),
        frame: initial,
    });
    worklist.push_back(cfg.entry_node);
    let mut max_stack = 0_usize;
    let mut max_locals = 0_usize;

    while let Some(node_index) = worklist.pop_front() {
        let pending =
            entry_frames[node_index]
                .clone()
                .ok_or_else(|| AnalysisError::InvalidControlFlow {
                    reason: "worklist node missing entry frame".to_owned(),
                })?;
        let state = &pending.frame;
        max_stack = max_stack.max(state.stack_depth());
        max_locals = max_locals.max(state.locals.len());
        let code_index = cfg.nodes[node_index].code_index;
        let item = &code.instructions[code_index];
        if let CodeItem::Raw(raw) = item {
            let expected = match &return_type {
                ReturnType::Void => 0xb1,
                ReturnType::Field(field) => match vtype_from_descriptor(field) {
                    VType::Long => 0xad,
                    VType::Float => 0xae,
                    VType::Double => 0xaf,
                    VType::Object(_) => 0xb0,
                    _ => 0xac,
                },
            };
            if (0xac..=0xb1).contains(&raw.opcode()) {
                if raw.opcode() != expected {
                    return Err(type_error(format!(
                        "return opcode does not match {descriptor} at instruction {code_index}"
                    )));
                }
                if pending.this_uninitialized {
                    return Err(type_error("constructor returns before initializing this"));
                }
            }
        }

        let next_state = simulate_item(
            item,
            state,
            class,
            code_index,
            (node_index + 1 < cfg.nodes.len()).then_some(node_index + 1),
        )
        .map_err(|error| {
            type_error(format!(
                "{class_name}.{method_name}{descriptor} instruction {code_index}: {error}"
            ))
        })?;
        let initialized = initializing_receiver(item, state)?;

        for exception_edge in &cfg.nodes[node_index].exception_successors {
            let stack = vec![match &exception_edge.catch_type {
                Some(catch_type) => VType::Object(catch_type.clone()),
                None => VType::Object("java/lang/Throwable".to_owned()),
            }];
            let handler_state = FrameState {
                stack,
                // A failed <init> call makes every saved receiver alias unusable.
                locals: state
                    .locals
                    .iter()
                    .map(|value| {
                        if initialized.as_ref() == Some(value) {
                            VType::Top
                        } else {
                            value.clone()
                        }
                    })
                    .collect(),
            };
            propagate(
                exception_edge.target,
                PendingFrame {
                    frame: handler_state,
                    this_uninitialized: pending.this_uninitialized,
                },
                &mut entry_frames,
                &mut worklist,
                resolver,
            )?;
        }

        max_stack = max_stack.max(next_state.stack_depth());
        max_locals = max_locals.max(next_state.locals.len());
        let normal_successors = dynamic_successors(item, state)
            .unwrap_or_else(|| cfg.nodes[node_index].normal_successors.clone());
        for successor in &normal_successors {
            propagate(
                *successor,
                PendingFrame {
                    frame: next_state.clone(),
                    this_uninitialized: pending.this_uninitialized
                        && initialized != Some(VType::UninitializedThis),
                },
                &mut entry_frames,
                &mut worklist,
                resolver,
            )?;
        }
    }

    let flags = entry_frames
        .iter()
        .map(|frame| frame.as_ref().is_some_and(|frame| frame.this_uninitialized))
        .collect();
    Ok((
        SimulationResult {
            cfg,
            entry_frames: entry_frames
                .into_iter()
                .map(|frame| frame.map(|frame| frame.frame))
                .collect(),
            max_stack: u16::try_from(max_stack)
                .map_err(|_| type_error("max_stack exceeds 65535"))?,
            max_locals: u16::try_from(max_locals)
                .map_err(|_| type_error("max_locals exceeds 65535"))?,
        },
        flags,
    ))
}

fn initializing_receiver(
    item: &CodeItem,
    state: &FrameState,
) -> Result<Option<VType>, AnalysisError> {
    if let CodeItem::Method(method) = item
        && method.opcode == 0xb7
        && method.name == "<init>"
    {
        let parsed = parse_method_descriptor(&method.descriptor)
            .map_err(|error| type_error(error.to_string()))?;
        return Ok(Some(state.peek(parameter_slot_count(&parsed))?.clone()));
    }
    Ok(None)
}

pub fn recompute_frames(
    code: &CodeModel,
    class_name: &str,
    method_name: &str,
    descriptor: &str,
    access_flags: MethodAccessFlags,
    resolver: Option<&dyn ClassResolver>,
) -> Result<FrameComputationResult, AnalysisError> {
    compute_frames(
        code,
        ClassContext {
            name: class_name,
            model: None,
        },
        method_name,
        descriptor,
        access_flags,
        resolver,
    )
}

pub(crate) fn recompute_frames_for_class(
    code: &CodeModel,
    class: &ClassModel,
    method: &crate::model::MethodModel,
    resolver: Option<&dyn ClassResolver>,
) -> Result<FrameComputationResult, AnalysisError> {
    compute_frames(
        code,
        ClassContext {
            name: &class.name,
            model: Some(class),
        },
        &method.name,
        &method.descriptor,
        method.access_flags,
        resolver,
    )
}

fn compute_frames(
    code: &CodeModel,
    class: ClassContext<'_>,
    method_name: &str,
    descriptor: &str,
    access_flags: MethodAccessFlags,
    resolver: Option<&dyn ClassResolver>,
) -> Result<FrameComputationResult, AnalysisError> {
    let (simulation, flags) =
        simulate_in_class(code, class, method_name, descriptor, access_flags, resolver)?;
    let mut frames = Vec::new();
    for node in &simulation.cfg.nodes {
        if node.node_index == simulation.cfg.entry_node || !node.is_block_start {
            continue;
        }
        if let Some(frame) = &simulation.entry_frames[node.node_index] {
            if class.model.is_none_or(|model| model.version.0 >= 50)
                && flags[node.node_index]
                && !frame.locals.contains(&VType::UninitializedThis)
            {
                return Err(type_error(format!(
                    "cannot encode uninitialized constructor state at instruction {} in StackMapTable",
                    node.code_index
                )));
            }
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
        if method_name == "<init>" && class_name != JAVA_LANG_OBJECT {
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
    candidate: PendingFrame,
    entry_frames: &mut [Option<PendingFrame>],
    worklist: &mut VecDeque<usize>,
    resolver: Option<&dyn ClassResolver>,
) -> Result<(), AnalysisError> {
    let changed = match &entry_frames[target] {
        Some(existing) => {
            let merged = merge_frames(&existing.frame, &candidate.frame, resolver)?;
            let flag = existing.this_uninitialized || candidate.this_uninitialized;
            if merged != existing.frame || flag != existing.this_uninitialized {
                entry_frames[target] = Some(PendingFrame {
                    frame: merged,
                    this_uninitialized: flag,
                });
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
    let merge = |left: &VType, right: &VType| -> Result<VType, AnalysisError> {
        if let (VType::Object(left), VType::Object(right)) = (left, right) {
            return hierarchy::common_reference_type(resolver, left, right).map(VType::Object);
        }
        Ok(merge_vtypes(left, right, resolver))
    };
    let stack = left
        .stack
        .iter()
        .zip(&right.stack)
        .map(|(left, right)| {
            let value = merge(left, right)?;
            if value == VType::Top && left != right {
                return Err(type_error("incompatible operand stack types at join"));
            }
            Ok(value)
        })
        .collect::<Result<Vec<_>, AnalysisError>>()?;
    let max_locals = left.locals.len().max(right.locals.len());
    let mut locals = Vec::with_capacity(max_locals);
    for index in 0..max_locals {
        let left_value = left.locals.get(index).unwrap_or(&VType::Top);
        let right_value = right.locals.get(index).unwrap_or(&VType::Top);
        locals.push(merge(left_value, right_value)?);
    }
    Ok(FrameState { stack, locals })
}
