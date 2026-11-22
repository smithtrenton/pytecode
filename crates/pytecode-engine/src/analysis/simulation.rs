use super::*;
use crate::model::MethodModel;
use std::borrow::Cow;

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

struct Worklist {
    queue: VecDeque<usize>,
    queued: Vec<bool>,
}

impl Worklist {
    fn new(nodes: usize) -> Self {
        Self {
            queue: VecDeque::new(),
            queued: vec![false; nodes],
        }
    }

    fn push_back(&mut self, node: usize) {
        if !self.queued[node] {
            self.queued[node] = true;
            self.queue.push_back(node);
        }
    }

    fn pop_front(&mut self) -> Option<usize> {
        let node = self.queue.pop_front()?;
        self.queued[node] = false;
        Some(node)
    }
}

fn check_jsr_allowed(code: &CodeModel, class: ClassContext<'_>) -> Result<(), AnalysisError> {
    if class.model.is_some_and(|model| model.version.0 >= 51)
        && code.instructions.iter().any(|item| {
            matches!(
                item,
                CodeItem::Branch(b) if matches!(b.opcode, 0xA8 | 0xC9)
            ) || matches!(item, CodeItem::Var(v) if v.opcode == 0xA9)
                || matches!(
                    item,
                    CodeItem::Raw(raw) if matches!(raw.opcode(), 0xA8 | 0xC9 | 0xA9)
                )
        })
    {
        return Err(type_error(
            "jsr/ret subroutine instructions are not permitted in class file version 51.0 or newer",
        ));
    }
    Ok(())
}

fn item_cannot_fall_off_end(item: &CodeItem, at_last_node: bool) -> bool {
    if !at_last_node {
        return false;
    }
    !is_terminal_item(item)
        && !matches!(item, CodeItem::Branch(branch) if is_unconditional_branch(branch.opcode))
        && !matches!(item, CodeItem::LookupSwitch(_) | CodeItem::TableSwitch(_))
}

struct MethodSimContext<'a> {
    class: ClassContext<'a>,
    method_name: &'a str,
    descriptor: &'a str,
    return_type: &'a ReturnType,
    expected_return: Option<&'a VType>,
    resolver: Option<&'a dyn ClassResolver>,
}

fn step_node(
    node_index: usize,
    pending: &PendingFrame,
    code: &CodeModel,
    cfg: &ControlFlowGraph,
    ctx: &MethodSimContext<'_>,
) -> Result<(FrameState, Option<VType>, Vec<usize>), AnalysisError> {
    let state = &pending.frame;
    let code_index = cfg.nodes[node_index].code_index;
    let item = &code.instructions[code_index];
    if let CodeItem::Raw(raw) = item {
        let expected = match ctx.return_type {
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
                    "return opcode does not match {} at instruction {code_index}",
                    ctx.descriptor
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
        ctx.class,
        code_index,
        (node_index + 1 < cfg.nodes.len()).then_some(node_index + 1),
        ctx.expected_return,
        ctx.resolver,
    )
    .map_err(|error| {
        type_error(format!(
            "{}.{}{} instruction {code_index}: {error}",
            ctx.class.name, ctx.method_name, ctx.descriptor
        ))
    })?;

    if item_cannot_fall_off_end(item, node_index + 1 == cfg.nodes.len()) {
        return Err(type_error(format!(
            "{}.{}{} instruction {code_index}: control flow falls off end of code",
            ctx.class.name, ctx.method_name, ctx.descriptor
        )));
    }

    let initialized = initializing_receiver(item, state)?;
    let normal_successors = dynamic_successors(item, &next_state)
        .unwrap_or_else(|| cfg.nodes[node_index].normal_successors.clone());
    Ok((next_state, initialized, normal_successors))
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
    check_jsr_allowed(code, class)?;
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
    let expected_return = match &return_type {
        ReturnType::Void => None,
        ReturnType::Field(field) => Some(vtype_from_descriptor(field)),
    };
    let sim_ctx = MethodSimContext {
        class,
        method_name,
        descriptor,
        return_type: &return_type,
        expected_return: expected_return.as_ref(),
        resolver,
    };
    let initial = initial_frame(class_name, method_name, descriptor, access_flags)?;
    let has_jsr = code.instructions.iter().any(|item| {
        matches!(
            item,
            CodeItem::Branch(branch) if matches!(branch.opcode, 0xA8 | 0xC9)
        )
    });

    if has_jsr {
        return simulate_polymorphic_jsr(code, cfg, &sim_ctx, initial);
    }

    let mut entry_frames = vec![None; cfg.nodes.len()];
    let mut worklist = Worklist::new(cfg.nodes.len());
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
        let (next_state, initialized, normal_successors) =
            step_node(node_index, &pending, code, &cfg, &sim_ctx)?;

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

fn simulate_polymorphic_jsr(
    code: &CodeModel,
    cfg: ControlFlowGraph,
    ctx: &MethodSimContext<'_>,
    initial: FrameState,
) -> Result<(SimulationResult, Vec<bool>), AnalysisError> {
    type CallStack = Vec<(usize, usize)>;
    type StateKey = (usize, CallStack);

    let max_states = cfg.nodes.len() * 64 + 1024;
    let mut frames_by_key: HashMap<StateKey, PendingFrame> = HashMap::new();
    let mut queue: VecDeque<StateKey> = VecDeque::new();
    let mut queued: HashSet<StateKey> = HashSet::new();

    let start_key: StateKey = (cfg.entry_node, Vec::new());
    frames_by_key.insert(
        start_key.clone(),
        PendingFrame {
            this_uninitialized: initial.locals.contains(&VType::UninitializedThis),
            frame: initial,
        },
    );
    queue.push_back(start_key.clone());
    queued.insert(start_key);

    let mut max_stack = 0_usize;
    let mut max_locals = 0_usize;

    let enqueue = |key: StateKey,
                   candidate: PendingFrame,
                   frames_by_key: &mut HashMap<StateKey, PendingFrame>,
                   queue: &mut VecDeque<StateKey>,
                   queued: &mut HashSet<StateKey>|
     -> Result<(), AnalysisError> {
        let changed = match frames_by_key.get(&key) {
            Some(existing) => {
                let merged = merge_frames(&existing.frame, &candidate.frame, ctx.resolver)?;
                let flag = existing.this_uninitialized || candidate.this_uninitialized;
                if merged != existing.frame || flag != existing.this_uninitialized {
                    frames_by_key.insert(
                        key.clone(),
                        PendingFrame {
                            frame: merged,
                            this_uninitialized: flag,
                        },
                    );
                    true
                } else {
                    false
                }
            }
            None => {
                if frames_by_key.len() >= max_states {
                    return Err(type_error(
                        "subroutine frame simulation exceeded context state budget",
                    ));
                }
                frames_by_key.insert(key.clone(), candidate);
                true
            }
        };
        if changed && queued.insert(key.clone()) {
            queue.push_back(key);
        }
        Ok(())
    };

    while let Some(key) = queue.pop_front() {
        queued.remove(&key);
        let (node_index, call_stack) = key.clone();
        let pending =
            frames_by_key
                .get(&key)
                .cloned()
                .ok_or_else(|| AnalysisError::InvalidControlFlow {
                    reason: "worklist node missing entry frame".to_owned(),
                })?;
        let state = &pending.frame;
        max_stack = max_stack.max(state.stack_depth());
        max_locals = max_locals.max(state.locals.len());

        let (next_state, initialized, normal_successors) =
            step_node(node_index, &pending, code, &cfg, ctx)?;
        let item = &code.instructions[cfg.nodes[node_index].code_index];

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
            enqueue(
                (exception_edge.target, call_stack.clone()),
                PendingFrame {
                    frame: handler_state,
                    this_uninitialized: pending.this_uninitialized,
                },
                &mut frames_by_key,
                &mut queue,
                &mut queued,
            )?;
        }

        max_stack = max_stack.max(next_state.stack_depth());
        max_locals = max_locals.max(next_state.locals.len());
        let next_this_uninit =
            pending.this_uninitialized && initialized != Some(VType::UninitializedThis);

        if let CodeItem::Branch(branch) = item
            && matches!(branch.opcode, 0xA8 | 0xC9)
        {
            let sub_entry = normal_successors[0];
            let return_target = node_index + 1;
            if call_stack.iter().any(|&(entry, _)| entry == sub_entry) {
                return Err(type_error("recursive jsr subroutine call is not allowed"));
            }
            let mut next_stack = call_stack.clone();
            next_stack.push((sub_entry, return_target));
            enqueue(
                (sub_entry, next_stack),
                PendingFrame {
                    frame: next_state.clone(),
                    this_uninitialized: next_this_uninit,
                },
                &mut frames_by_key,
                &mut queue,
                &mut queued,
            )?;
        } else if let CodeItem::Var(var) = item
            && var.opcode == 0xA9
        {
            for target in &normal_successors {
                let next_stack = if let Some(pos) = call_stack
                    .iter()
                    .rposition(|&(_, ret_target)| ret_target == *target)
                {
                    call_stack[..pos].to_vec()
                } else {
                    call_stack.clone()
                };
                enqueue(
                    (*target, next_stack),
                    PendingFrame {
                        frame: next_state.clone(),
                        this_uninitialized: next_this_uninit,
                    },
                    &mut frames_by_key,
                    &mut queue,
                    &mut queued,
                )?;
            }
        } else {
            for successor in &normal_successors {
                enqueue(
                    (*successor, call_stack.clone()),
                    PendingFrame {
                        frame: next_state.clone(),
                        this_uninitialized: next_this_uninit,
                    },
                    &mut frames_by_key,
                    &mut queue,
                    &mut queued,
                )?;
            }
        }
    }

    let mut per_node: Vec<Vec<(CallStack, PendingFrame)>> = vec![Vec::new(); cfg.nodes.len()];
    for ((node_index, call_stack), frame) in frames_by_key {
        per_node[node_index].push((call_stack, frame));
    }

    let mut collapsed: Vec<Option<PendingFrame>> = Vec::with_capacity(cfg.nodes.len());
    for entries in &mut per_node {
        if entries.is_empty() {
            collapsed.push(None);
            continue;
        }
        entries.sort_by(|(left, _), (right, _)| left.cmp(right));
        let mut acc = entries[0].1.clone();
        for (_, candidate) in entries.iter().skip(1) {
            if let Ok(merged) = merge_frames(&acc.frame, &candidate.frame, ctx.resolver) {
                acc = PendingFrame {
                    frame: merged,
                    this_uninitialized: acc.this_uninitialized || candidate.this_uninitialized,
                };
            } else {
                acc.this_uninitialized |= candidate.this_uninitialized;
            }
        }
        collapsed.push(Some(acc));
    }

    let flags = collapsed
        .iter()
        .map(|frame| frame.as_ref().is_some_and(|frame| frame.this_uninitialized))
        .collect();
    Ok((
        SimulationResult {
            cfg,
            entry_frames: collapsed
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
    method: &MethodModel,
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

type NormalizedCodeResult<'a> = (
    Cow<'a, CodeModel>,
    FrameComputationResult,
    Option<Vec<Option<usize>>>,
);

pub(crate) fn recompute_and_normalize_code<'a>(
    code: &'a CodeModel,
    class: &ClassModel,
    method: &MethodModel,
    resolver: Option<&dyn ClassResolver>,
) -> Result<NormalizedCodeResult<'a>, AnalysisError> {
    let class_ctx = ClassContext {
        name: &class.name,
        model: Some(class),
    };
    let (simulation, flags) = simulate_in_class(
        code,
        class_ctx,
        &method.name,
        &method.descriptor,
        method.access_flags,
        resolver,
    )?;
    let mut frames_result = frames_from_simulation(&simulation, &flags, Some(class))?;

    let dead_nodes: Vec<bool> = simulation
        .entry_frames
        .iter()
        .map(Option::is_none)
        .collect();
    if dead_nodes.iter().all(|&is_dead| !is_dead) {
        return Ok((Cow::Borrowed(code), frames_result, None));
    }

    let mut normalized = code.clone();
    let mut label_positions = HashMap::new();
    for (idx, item) in code.instructions.iter().enumerate() {
        if let CodeItem::Label(label) = item {
            label_positions.insert(label.clone(), idx);
        }
    }

    let mut kept_handlers = Vec::with_capacity(code.exception_handlers.len());
    let mut catch_remap = Vec::with_capacity(code.exception_handlers.len());
    for handler in &code.exception_handlers {
        let start_pos = label_positions[&handler.start];
        let end_pos = label_positions[&handler.end];
        let covers_reachable_node = simulation.cfg.nodes.iter().any(|node| {
            node.code_index >= start_pos
                && node.code_index < end_pos
                && !dead_nodes[node.node_index]
        });
        if covers_reachable_node {
            let new_idx = kept_handlers.len();
            kept_handlers.push(handler.clone());
            catch_remap.push(Some(new_idx));
        } else {
            catch_remap.push(None);
        }
    }
    normalized.exception_handlers = kept_handlers;

    let mut node_index = 0_usize;
    while node_index < dead_nodes.len() {
        if !dead_nodes[node_index] {
            node_index += 1;
            continue;
        }
        let run_start = node_index;
        while node_index + 1 < dead_nodes.len() && dead_nodes[node_index + 1] {
            node_index += 1;
        }
        let run_end = node_index;
        for node in &simulation.cfg.nodes[run_start..run_end] {
            normalized.instructions[node.code_index] =
                CodeItem::Raw(crate::raw::Instruction::Simple {
                    opcode: 0x00,
                    offset: 0,
                });
        }
        normalized.instructions[simulation.cfg.nodes[run_end].code_index] =
            CodeItem::Raw(crate::raw::Instruction::Simple {
                opcode: 0xBF,
                offset: 0,
            });
        frames_result.frames.push(StackMapFrameState {
            code_index: simulation.cfg.nodes[run_start].code_index,
            locals: Vec::new(),
            stack: vec![VType::Object("java/lang/Throwable".to_owned())],
        });
        node_index = run_end + 1;
    }

    frames_result.frames.sort_by_key(|frame| frame.code_index);
    frames_result.frames.dedup_by_key(|frame| frame.code_index);
    frames_result.max_stack = frames_result.max_stack.max(1);

    Ok((Cow::Owned(normalized), frames_result, Some(catch_remap)))
}

fn frames_from_simulation(
    simulation: &SimulationResult,
    flags: &[bool],
    class_model: Option<&ClassModel>,
) -> Result<FrameComputationResult, AnalysisError> {
    let mut frames = Vec::new();
    for node in &simulation.cfg.nodes {
        if (node.node_index == simulation.cfg.entry_node && !node.is_jump_target)
            || !node.is_block_start
        {
            continue;
        }
        if let Some(frame) = &simulation.entry_frames[node.node_index] {
            if class_model.is_none_or(|model| model.version.0 >= 50)
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
    frames_from_simulation(&simulation, &flags, class.model)
}

pub(crate) fn initial_method_locals(
    class_name: &str,
    method_name: &str,
    descriptor: &str,
    access_flags: MethodAccessFlags,
) -> Result<Vec<VType>, AnalysisError> {
    initial_frame(class_name, method_name, descriptor, access_flags).map(|frame| frame.locals)
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
    worklist: &mut Worklist,
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
    if changed {
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
