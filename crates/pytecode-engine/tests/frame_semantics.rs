use pytecode_engine::analysis::{
    FrameState, MappingClassResolver, VType, merge_vtypes, recompute_frames, simulate,
};
use pytecode_engine::constants::MethodAccessFlags;
use pytecode_engine::model::{CodeItem, CodeModel, DebugInfoState, VarInsn};

fn code(bytes: &[u8]) -> CodeModel {
    let mut code = CodeModel::new(0, 0, DebugInfoState::Fresh);
    code.instructions = pytecode_engine::parse_instructions(bytes)
        .unwrap()
        .into_iter()
        .map(CodeItem::Raw)
        .collect();
    code
}

#[test]
fn unary_negation_and_long_shifts_have_correct_stack_effects() {
    for (bytes, descriptor, expected_max) in [
        (vec![0x04, 0x74, 0xac], "()I", 1),
        (vec![0x09, 0x75, 0xad], "()J", 2),
        (vec![0x0b, 0x76, 0xae], "()F", 1),
        (vec![0x0e, 0x77, 0xaf], "()D", 2),
        (vec![0x09, 0x04, 0x79, 0xad], "()J", 3),
        (vec![0x09, 0x04, 0x7b, 0xad], "()J", 3),
        (vec![0x09, 0x04, 0x7d, 0xad], "()J", 3),
    ] {
        let result = recompute_frames(
            &code(&bytes),
            "Test",
            "run",
            descriptor,
            MethodAccessFlags::STATIC,
            None,
        )
        .unwrap_or_else(|error| panic!("{bytes:02x?}: {error}"));
        assert_eq!(result.max_stack, expected_max);
    }
}

#[test]
fn stack_operations_reject_splitting_category_two_values() {
    for bytes in [
        vec![0x09, 0x59, 0x57, 0x58, 0xb1],       // dup on long
        vec![0x09, 0x57, 0x57, 0xb1],             // pop half of long
        vec![0x09, 0x04, 0x5f, 0x57, 0x58, 0xb1], // swap long and int
    ] {
        assert!(
            recompute_frames(
                &code(&bytes),
                "Test",
                "run",
                "()V",
                MethodAccessFlags::STATIC,
                None,
            )
            .is_err(),
            "accepted {bytes:02x?}"
        );
    }
}

#[test]
fn primitive_load_requires_initialized_matching_local() {
    for (opcode, descriptor) in [(0x15, "()I"), (0x15, "(F)I"), (0x16, "(I)J")] {
        let mut code = code(&[if opcode == 0x16 { 0xad } else { 0xac }]);
        code.instructions
            .insert(0, CodeItem::Var(VarInsn { opcode, slot: 0 }));
        assert!(
            recompute_frames(
                &code,
                "Test",
                "run",
                descriptor,
                MethodAccessFlags::STATIC,
                None,
            )
            .is_err()
        );
    }
}

#[test]
fn overwriting_second_local_slot_invalidates_wide_value() {
    let initial = FrameState::default().set_local(0, VType::Long);
    let overwritten = initial.set_local(1, VType::Integer);
    assert!(overwritten.get_local(0).is_err());
    assert_eq!(overwritten.get_local(1).unwrap(), &VType::Integer);
}

#[test]
fn all_legal_dup_forms_preserve_complete_values() {
    use VType::{Double as D, Float as F, Integer as I, Long as L};
    for (inputs, opcode, expected) in [
        (vec![I], 0x59, vec![I, I]),
        (vec![F, I], 0x5a, vec![I, F, I]),
        (vec![F, F, I], 0x5b, vec![I, F, F, I]),
        (vec![D, I], 0x5b, vec![I, D, I]),
        (vec![I, F], 0x5c, vec![I, F, I, F]),
        (vec![L], 0x5c, vec![L, L]),
        (vec![I, F, I], 0x5d, vec![F, I, I, F, I]),
        (vec![I, L], 0x5d, vec![L, I, L]),
        (vec![I, F, I, F], 0x5e, vec![I, F, I, F, I, F]),
        (vec![I, F, L], 0x5e, vec![L, I, F, L]),
        (vec![L, I, F], 0x5e, vec![I, F, L, I, F]),
        (vec![D, L], 0x5e, vec![L, D, L]),
    ] {
        let mut bytes = inputs
            .iter()
            .map(|value| match value {
                I => 0x03,
                F => 0x0b,
                L => 0x09,
                D => 0x0e,
                _ => unreachable!(),
            })
            .collect::<Vec<_>>();
        bytes.extend([opcode, 0xb1]);
        let simulation = simulate(
            &code(&bytes),
            "Test",
            "run",
            "()V",
            MethodAccessFlags::STATIC,
            None,
        )
        .unwrap();
        let expected_stack = FrameState::default().push(expected).stack;
        assert_eq!(
            simulation
                .entry_frames
                .last()
                .unwrap()
                .as_ref()
                .unwrap()
                .stack,
            expected_stack,
            "opcode {opcode:02x}, inputs {inputs:?}"
        );
    }
}

#[test]
fn array_joins_keep_reference_array_dimensions() {
    for (left, right, expected) in [
        (
            "[Ljava/lang/String;",
            "[Ljava/lang/Integer;",
            "[Ljava/lang/Object;",
        ),
        ("[[I", "[[D", "[Ljava/lang/Object;"),
        ("[[I", "[I", "java/lang/Object"),
        (
            "[[Ljava/lang/String;",
            "[[Ljava/lang/Object;",
            "[[Ljava/lang/Object;",
        ),
    ] {
        assert_eq!(
            merge_vtypes(
                &VType::Object(left.into()),
                &VType::Object(right.into()),
                None
            ),
            VType::Object(expected.into())
        );
    }
}

#[test]
fn provided_resolver_does_not_silently_hide_missing_classes_at_joins() {
    let resolver = MappingClassResolver::default();
    assert!(
        pytecode_engine::analysis::common_superclass(&resolver, "missing/One", "missing/Two")
            .is_err()
    );
}

#[test]
fn wide_local_beyond_u16_limit_is_rejected() {
    let mut code = code(&[0x09, 0xb1]);
    code.instructions.insert(
        1,
        CodeItem::Var(VarInsn {
            opcode: 0x37,
            slot: u16::MAX,
        }),
    );
    let error =
        recompute_frames(&code, "Test", "run", "()V", MethodAccessFlags::STATIC, None).unwrap_err();
    assert!(error.to_string().contains("max_locals exceeds"));
}

#[test]
fn constructor_initialization_survives_overwriting_local_zero() {
    let pool = pytecode_engine::model::ConstantPoolBuilder::new();
    let code = CodeModel::from_raw_code(1, 1, &[0x01, 0x4b, 0xb1], &pool).unwrap();
    let error = simulate(
        &code,
        "Test",
        "<init>",
        "()V",
        MethodAccessFlags::PUBLIC,
        None,
    )
    .unwrap_err();
    assert!(error.to_string().contains("before initializing this"));
}

#[test]
fn object_constructor_does_not_require_a_super_constructor() {
    simulate(
        &code(&[0xb1]),
        "java/lang/Object",
        "<init>",
        "()V",
        MethodAccessFlags::PUBLIC,
        None,
    )
    .unwrap();
}

#[test]
fn null_does_not_merge_with_uninitialized_references() {
    for uninitialized in [
        VType::UninitializedThis,
        VType::Uninitialized {
            code_index: 0,
            class_name: "Test".into(),
        },
    ] {
        assert_eq!(merge_vtypes(&VType::Null, &uninitialized, None), VType::Top);
        assert_eq!(merge_vtypes(&uninitialized, &VType::Null, None), VType::Top);
    }
}

#[test]
fn special_method_names_require_valid_invocation_forms() {
    use pytecode_engine::model::MethodInsn;
    for (opcode, name, descriptor) in [
        (0xb8, "<init>", "()V"),
        (0xb7, "<init>", "()I"),
        (0xb8, "<clinit>", "()V"),
    ] {
        let mut code = code(&[0xb1]);
        code.instructions.insert(
            0,
            CodeItem::Method(MethodInsn {
                opcode,
                owner: "Test".into(),
                name: name.into(),
                descriptor: descriptor.into(),
                is_interface: false,
            }),
        );
        assert!(simulate(&code, "Test", "run", "()V", MethodAccessFlags::STATIC, None).is_err());
    }
}

#[test]
fn failed_initialization_poisoned_alias_cannot_be_retried() {
    use pytecode_engine::model::{ExceptionHandler, Label, MethodInsn, TypeInsn};
    let start = Label::new();
    let end = Label::new();
    let handler = Label::new();
    let init = CodeItem::Method(MethodInsn {
        opcode: 0xb7,
        owner: "Test".into(),
        name: "<init>".into(),
        descriptor: "()V".into(),
        is_interface: false,
    });
    let mut code = code(&[0xb1]);
    let return_insn = code.instructions[0].clone();
    let pop = self::code(&[0x57]).instructions[0].clone();
    code.instructions = vec![
        CodeItem::Type(TypeInsn {
            opcode: 0xbb,
            descriptor: "Test".into(),
        }),
        CodeItem::Var(VarInsn {
            opcode: 0x3a,
            slot: 0,
        }),
        CodeItem::Var(VarInsn {
            opcode: 0x19,
            slot: 0,
        }),
        CodeItem::Label(start.clone()),
        init.clone(),
        CodeItem::Label(end.clone()),
        return_insn.clone(),
        CodeItem::Label(handler.clone()),
        pop,
        CodeItem::Var(VarInsn {
            opcode: 0x19,
            slot: 0,
        }),
        init,
        return_insn,
    ];
    code.exception_handlers.push(ExceptionHandler {
        start,
        end,
        handler,
        catch_type: None,
    });
    let error = simulate(&code, "Test", "run", "()V", MethodAccessFlags::STATIC, None).unwrap_err();
    assert!(
        error.to_string().contains("slot is not initialized"),
        "{error}"
    );
    // A handler that only rethrows is valid; its saved allocation alias is Top.
    code.instructions.truncate(8);
    code.instructions.extend(self::code(&[0xbf]).instructions);
    let result =
        recompute_frames(&code, "Test", "run", "()V", MethodAccessFlags::STATIC, None).unwrap();
    assert_eq!(result.frames.last().unwrap().locals, vec![VType::Top]);
}

#[test]
fn dead_code_inside_exception_ranges_trims_splits_and_drops_handlers() {
    use pytecode_engine::analysis::{Severity, verify_classfile, verify_classmodel};
    use pytecode_engine::constants::ClassAccessFlags;
    use pytecode_engine::model::{
        BranchInsn, ClassModel, DebugInfoPolicy, ExceptionHandler, Label, MethodModel,
    };
    use pytecode_engine::parse_class;
    use pytecode_engine::raw::{AttributeInfo, Instruction};

    let raw = |opcode: u8| CodeItem::Raw(Instruction::Simple { opcode, offset: 0 });
    let aload0 = CodeItem::Var(VarInsn {
        opcode: 0x19,
        slot: 0,
    });

    let build_and_lower = |instructions: Vec<CodeItem>, handlers: Vec<ExceptionHandler>| {
        let mut body = CodeModel::new(2, 2, DebugInfoState::Fresh);
        body.instructions = instructions;
        body.exception_handlers = handlers;
        let mut model = ClassModel {
            version: (52, 0),
            access_flags: ClassAccessFlags::PUBLIC | ClassAccessFlags::SUPER,
            name: "DeadTry".to_owned(),
            super_name: Some("java/lang/Object".to_owned()),
            ..ClassModel::default()
        };
        model.methods.push(MethodModel::new(
            MethodAccessFlags::PUBLIC,
            "run".to_owned(),
            "(I)V".to_owned(),
            Some(body),
            Vec::new(),
        ));
        let bytes = model
            .to_bytes_with_recomputed_frames(DebugInfoPolicy::Preserve, None)
            .expect("lowering with recomputed frames should succeed");
        let parsed = parse_class(&bytes).expect("emitted class should parse");
        let cf_errors: Vec<_> = verify_classfile(&parsed)
            .into_iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(
            cf_errors.is_empty(),
            "verify_classfile failed: {cf_errors:?}"
        );
        let relifted = ClassModel::from_bytes(&bytes).expect("emitted class should re-lift");
        let cm_errors: Vec<_> = verify_classmodel(&relifted, None)
            .into_iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(
            cm_errors.is_empty(),
            "verify_classmodel failed: {cm_errors:?}"
        );
        parsed.methods[0]
            .attributes
            .iter()
            .find_map(|attr| match attr {
                AttributeInfo::Code(code) => Some(code.clone()),
                _ => None,
            })
            .expect("method should have Code")
    };

    // 1. Dead block at tail of try range: [LIVE, DEAD] -> trimmed to [LIVE].
    {
        let try_start = Label::named("try_start");
        let try_end = Label::named("try_end");
        let handler = Label::named("handler");
        let code_attr = build_and_lower(
            vec![
                CodeItem::Label(try_start.clone()),
                raw(0x01), // aconst_null
                raw(0xBF), // athrow
                raw(0x00), // dead nop
                raw(0xB1), // dead return
                CodeItem::Label(try_end.clone()),
                CodeItem::Label(handler.clone()),
                raw(0x57), // pop
                aload0.clone(),
                raw(0x57), // pop (uses `this` so handler frame requires local 0)
                raw(0xB1), // return
            ],
            vec![ExceptionHandler {
                start: try_start,
                end: try_end,
                handler,
                catch_type: Some("java/lang/RuntimeException".to_owned()),
            }],
        );
        assert_eq!(code_attr.exception_table.len(), 1);
        let entry = &code_attr.exception_table[0];
        // Live instructions: offset 0 (aconst_null, 1B), offset 1 (athrow, 1B) -> end_pc = 2.
        // Dead instructions at offsets 2 (nop), 3 (athrow) must lie outside [start_pc, end_pc).
        assert_eq!((entry.start_pc, entry.end_pc), (0, 2));
        assert!(entry.handler_pc >= 4);
    }

    // 2. Dead block at head of try range: [DEAD, LIVE] -> trimmed to [LIVE].
    {
        let try_start = Label::named("try_start");
        let live_mid = Label::named("live_mid");
        let try_end = Label::named("try_end");
        let handler = Label::named("handler");
        let code_attr = build_and_lower(
            vec![
                CodeItem::Branch(BranchInsn {
                    opcode: 0xA7,
                    target: live_mid.clone(),
                }),
                CodeItem::Label(try_start.clone()),
                raw(0x00), // dead
                raw(0xB1), // dead
                CodeItem::Label(live_mid),
                raw(0x01), // aconst_null
                raw(0xBF), // athrow
                CodeItem::Label(try_end.clone()),
                CodeItem::Label(handler.clone()),
                raw(0x57),
                aload0.clone(),
                raw(0x57),
                raw(0xB1),
            ],
            vec![ExceptionHandler {
                start: try_start,
                end: try_end,
                handler,
                catch_type: None,
            }],
        );
        assert_eq!(code_attr.exception_table.len(), 1);
        let entry = &code_attr.exception_table[0];
        // goto (3B) -> dead nop (1B, off 3) + dead athrow (1B, off 4) -> live starts at off 5.
        assert_eq!((entry.start_pc, entry.end_pc), (5, 7));
    }

    // 3. Dead block in middle of try range: [LIVE1, DEAD, LIVE2] -> split into [LIVE1] and [LIVE2].
    {
        let try_start = Label::named("try_start");
        let live2 = Label::named("live2");
        let try_end = Label::named("try_end");
        let handler = Label::named("handler");
        let code_attr = build_and_lower(
            vec![
                CodeItem::Label(try_start.clone()),
                CodeItem::Var(VarInsn {
                    opcode: 0x15,
                    slot: 1,
                }),
                CodeItem::Branch(BranchInsn {
                    opcode: 0x99, // ifeq live2
                    target: live2.clone(),
                }),
                raw(0x01),
                raw(0xBF), // athrow
                raw(0x00), // dead
                raw(0x00), // dead
                CodeItem::Label(live2),
                raw(0x01),
                raw(0xBF), // athrow
                CodeItem::Label(try_end.clone()),
                CodeItem::Label(handler.clone()),
                raw(0x57),
                aload0.clone(),
                raw(0x57),
                raw(0xB1),
            ],
            vec![ExceptionHandler {
                start: try_start,
                end: try_end,
                handler,
                catch_type: Some("java/lang/Exception".to_owned()),
            }],
        );
        assert_eq!(code_attr.exception_table.len(), 2);
        let first = &code_attr.exception_table[0];
        let second = &code_attr.exception_table[1];
        assert!(first.start_pc < first.end_pc);
        assert!(first.end_pc < second.start_pc);
        assert!(second.start_pc < second.end_pc);
        assert_eq!(first.handler_pc, second.handler_pc);
        assert_eq!(first.catch_type, second.catch_type);
        // The gap [first.end_pc, second.start_pc) is exactly the normalized dead `nop; athrow`.
        assert_eq!(second.start_pc - first.end_pc, 2);
    }

    // 4. Wholly dead try range: [DEAD] -> handler dropped.
    {
        let try_start = Label::named("try_start");
        let try_end = Label::named("try_end");
        let handler = Label::named("handler");
        let code_attr = build_and_lower(
            vec![
                raw(0xB1), // return
                CodeItem::Label(try_start.clone()),
                raw(0x00), // dead
                CodeItem::Label(try_end.clone()),
                CodeItem::Label(handler.clone()),
                raw(0x57),
                raw(0xB1),
            ],
            vec![ExceptionHandler {
                start: try_start,
                end: try_end,
                handler,
                catch_type: None,
            }],
        );
        assert!(code_attr.exception_table.is_empty());
    }
}

#[test]
fn splitting_handlers_preserves_order_uninitialized_offsets_and_catch_annotations() {
    use pytecode_engine::analysis::{Severity, verify_classfile};
    use pytecode_engine::constants::{ClassAccessFlags, TargetType};
    use pytecode_engine::model::{
        BranchInsn, ClassModel, CodeTypeAnnotation, CodeTypeAnnotationTarget, DebugInfoPolicy,
        ExceptionHandler, Label, MethodInsn, MethodModel, TypeInsn,
    };
    use pytecode_engine::parse_class;
    use pytecode_engine::raw::{
        AttributeInfo, Instruction, StackMapFrameInfo, TargetInfo, TypePathInfo,
        VerificationTypeInfo,
    };

    let raw = |opcode: u8| CodeItem::Raw(Instruction::Simple { opcode, offset: 0 });
    let try_start = Label::named("try_start");
    let live2 = Label::named("live2");
    let try_end = Label::named("try_end");
    let dead_try_start = Label::named("dead_try_start");
    let dead_try_end = Label::named("dead_try_end");
    let after_try = Label::named("after_try");
    let join_uninit = Label::named("join_uninit");
    let handler_io = Label::named("handler_io");
    let handler_ex = Label::named("handler_ex");

    let mut model = ClassModel {
        version: (52, 0),
        access_flags: ClassAccessFlags::PUBLIC | ClassAccessFlags::SUPER,
        name: "SplitMeta".to_owned(),
        super_name: Some("java/lang/Object".to_owned()),
        ..ClassModel::default()
    };
    let anno_type_idx = model
        .constant_pool
        .add_utf8("LTypeAnno;")
        .expect("utf8 should add");

    let mut body = CodeModel::new(4, 3, DebugInfoState::Fresh);
    body.instructions = vec![
        CodeItem::Label(try_start.clone()),
        CodeItem::Var(VarInsn {
            opcode: 0x15,
            slot: 1,
        }),
        CodeItem::Branch(BranchInsn {
            opcode: 0x99, // ifeq live2
            target: live2.clone(),
        }),
        raw(0x01),
        raw(0xBF), // athrow (no label before the dead instruction below -> forces fresh boundary label insertion)
        raw(0x00), // dead inside both nested handlers
        CodeItem::Label(live2),
        CodeItem::Branch(BranchInsn {
            opcode: 0xA7,
            target: after_try.clone(),
        }),
        CodeItem::Label(try_end.clone()),
        CodeItem::Label(dead_try_start.clone()),
        raw(0x00), // wholly dead handler range
        CodeItem::Label(dead_try_end.clone()),
        CodeItem::Label(after_try),
        // Allocate an object AFTER the split try range, then branch with Uninitialized live on stack.
        CodeItem::Type(TypeInsn {
            opcode: 0xBB,
            descriptor: "java/lang/Object".to_owned(),
        }),
        raw(0x59), // dup
        CodeItem::Var(VarInsn {
            opcode: 0x15,
            slot: 1,
        }),
        CodeItem::Branch(BranchInsn {
            opcode: 0x99,
            target: join_uninit.clone(),
        }),
        CodeItem::Label(join_uninit),
        CodeItem::Method(MethodInsn {
            opcode: 0xB7,
            owner: "java/lang/Object".to_owned(),
            name: "<init>".to_owned(),
            descriptor: "()V".to_owned(),
            is_interface: false,
        }),
        raw(0x57), // pop
        raw(0xB1), // return
        CodeItem::Label(handler_io.clone()),
        raw(0x57),
        raw(0xB1),
        CodeItem::Label(handler_ex.clone()),
        raw(0x57),
        raw(0xB1),
    ];
    body.exception_handlers = vec![
        ExceptionHandler {
            start: try_start.clone(),
            end: try_end.clone(),
            handler: handler_io.clone(),
            catch_type: Some("java/io/IOException".to_owned()),
        },
        ExceptionHandler {
            start: try_start,
            end: try_end,
            handler: handler_ex.clone(),
            catch_type: Some("java/lang/Exception".to_owned()),
        },
        ExceptionHandler {
            start: dead_try_start,
            end: dead_try_end,
            handler: handler_ex,
            catch_type: Some("java/lang/Error".to_owned()),
        },
    ];
    body.visible_type_annotations = vec![
        CodeTypeAnnotation {
            target_type: TargetType::TypeExceptionParameter,
            target: CodeTypeAnnotationTarget::Catch {
                exception_table_index: 0,
            },
            target_path: TypePathInfo { path: Vec::new() },
            type_index: anno_type_idx,
            element_value_pairs: Vec::new(),
        },
        CodeTypeAnnotation {
            target_type: TargetType::TypeExceptionParameter,
            target: CodeTypeAnnotationTarget::Catch {
                exception_table_index: 2,
            },
            target_path: TypePathInfo { path: Vec::new() },
            type_index: anno_type_idx,
            element_value_pairs: Vec::new(),
        },
    ];
    model.methods.push(MethodModel::new(
        MethodAccessFlags::PUBLIC,
        "run".to_owned(),
        "(I)V".to_owned(),
        Some(body),
        Vec::new(),
    ));

    let bytes = model
        .to_bytes_with_recomputed_frames(DebugInfoPolicy::Preserve, None)
        .expect("lowering should succeed");
    let parsed = parse_class(&bytes).expect("class should parse");
    let errors: Vec<_> = verify_classfile(&parsed)
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "verify_classfile failed: {errors:?}");

    let code_attr = parsed.methods[0]
        .attributes
        .iter()
        .find_map(|attr| match attr {
            AttributeInfo::Code(code) => Some(code),
            _ => None,
        })
        .expect("method should have Code");

    // Handler 0 split into indices 0, 1; handler 1 split into indices 2, 3; handler 2 dropped.
    assert_eq!(code_attr.exception_table.len(), 4);
    assert_eq!(
        code_attr.exception_table[0].catch_type,
        code_attr.exception_table[1].catch_type
    );
    assert_eq!(
        code_attr.exception_table[2].catch_type,
        code_attr.exception_table[3].catch_type
    );
    assert_ne!(
        code_attr.exception_table[0].catch_type,
        code_attr.exception_table[2].catch_type
    );

    // Catch type annotation for original handler 0 maps to [0, 1]; annotation for dropped handler 2 is removed.
    let catch_indices: Vec<u16> = code_attr
        .attributes
        .iter()
        .find_map(|attr| match attr {
            AttributeInfo::RuntimeVisibleTypeAnnotations(ta) => Some(
                ta.annotations
                    .iter()
                    .filter_map(|ann| match ann.target_info {
                        TargetInfo::Catch {
                            exception_table_index,
                        } => Some(exception_table_index),
                        _ => None,
                    })
                    .collect(),
            ),
            _ => None,
        })
        .expect("RuntimeVisibleTypeAnnotations should be present");
    assert_eq!(catch_indices, vec![0, 1]);

    // Verify Uninitialized offset in StackMapTable points at the actual `new` instruction offset!
    let new_offset = code_attr
        .code
        .iter()
        .find(|insn| insn.opcode() == 0xBB)
        .expect("new instruction should exist")
        .offset() as u16;
    let uninit_offsets: Vec<u16> = code_attr
        .attributes
        .iter()
        .find_map(|attr| match attr {
            AttributeInfo::StackMapTable(smt) => Some(
                smt.entries
                    .iter()
                    .flat_map(|frame| match frame {
                        StackMapFrameInfo::Full { stack, .. } => stack.as_slice(),
                        StackMapFrameInfo::SameLocals1StackItem { stack, .. }
                        | StackMapFrameInfo::SameLocals1StackItemExtended { stack, .. } => {
                            std::slice::from_ref(stack)
                        }
                        _ => &[],
                    })
                    .filter_map(|vt| match vt {
                        VerificationTypeInfo::Uninitialized { offset } => Some(*offset),
                        _ => None,
                    })
                    .collect(),
            ),
            _ => None,
        })
        .expect("StackMapTable should be present");
    assert!(!uninit_offsets.is_empty());
    assert!(
        uninit_offsets.iter().all(|&offset| offset == new_offset),
        "expected Uninitialized offset {new_offset}, got {uninit_offsets:?}"
    );
}
