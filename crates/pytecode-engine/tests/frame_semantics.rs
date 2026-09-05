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
