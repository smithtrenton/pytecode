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
