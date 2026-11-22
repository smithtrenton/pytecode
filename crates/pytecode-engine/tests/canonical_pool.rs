//! Canonical constant-pool emission (`ConstantPoolPolicy::Canonical`).
//!
//! Classes with identical content but different pool histories must lower to
//! identical bytes, while the default policy keeps today's byte-for-byte
//! round trip.

use pytecode_engine::analysis::{Severity, verify_classfile};
use pytecode_engine::constants::{ClassAccessFlags, MethodAccessFlags};
use pytecode_engine::fixtures::{compiled_fixture_paths, compiled_fixture_paths_for};
use pytecode_engine::model::{
    BranchInsn, ClassModel, CodeItem, CodeModel, ConstantPoolPolicy, DebugInfoPolicy,
    DebugInfoState, FrameComputationMode, Label, LdcInsn, LdcValue, MethodModel, VarInsn,
    canonicalize_constant_pool,
};
use pytecode_engine::modified_utf8::JavaString;
use pytecode_engine::raw::{AttributeInfo, Instruction, UnknownAttribute};
use pytecode_engine::{parse_class, write_class};
use std::fs;

fn fixture_bytes(resource_name: &str, class_name: &str) -> Vec<u8> {
    let path = compiled_fixture_paths_for(resource_name)
        .expect("fixture paths should load")
        .into_iter()
        .find(|path| path.file_name().and_then(|name| name.to_str()) == Some(class_name))
        .unwrap_or_else(|| panic!("fixture {class_name} not found for {resource_name}"));
    fs::read(path).expect("fixture bytes should read")
}

fn canonical(model: &ClassModel) -> Vec<u8> {
    model
        .to_bytes_with_pool_policy(
            DebugInfoPolicy::Preserve,
            FrameComputationMode::Preserve,
            None,
            ConstantPoolPolicy::Canonical,
        )
        .expect("canonical lowering should succeed")
        .0
}

fn canonical_bytes(bytes: &[u8]) -> Vec<u8> {
    write_class(&canonicalize_constant_pool(&parse_class(bytes).unwrap()).unwrap()).unwrap()
}

fn assert_structurally_clean(bytes: &[u8], context: &str) {
    let errors = verify_classfile(&parse_class(bytes).unwrap())
        .into_iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .collect::<Vec<_>>();
    assert!(errors.is_empty(), "{context}: structural errors {errors:?}");
}

/// Lower, re-parse, and apply `edit`: one step of a class's edit history.
fn step(bytes: &[u8], edit: impl FnOnce(&mut ClassModel)) -> Vec<u8> {
    let mut model = ClassModel::from_bytes(bytes).unwrap();
    edit(&mut model);
    model.to_bytes().unwrap()
}

fn rename_method(from: &'static str, to: &'static str) -> impl FnOnce(&mut ClassModel) {
    move |model| {
        let method = model
            .methods
            .iter_mut()
            .find(|method| method.name == from)
            .unwrap_or_else(|| panic!("method {from} not found"));
        method.name = to.to_owned();
    }
}

#[test]
fn different_edit_histories_lower_to_identical_canonical_bytes() {
    let original = fixture_bytes("LambdaShowcase.java", "LambdaShowcase.class");

    // History A renames `prefixer` (via a temporary name) before
    // `longerThan`; history B renames them the other way round. Each step
    // round-trips through bytes, so the appended pool entries differ in order
    // and A keeps an orphaned "temporary" entry.
    let a = step(&original, rename_method("prefixer", "temporary"));
    let a = step(&a, rename_method("temporary", "first"));
    let a = step(&a, rename_method("longerThan", "second"));
    let b = step(&original, rename_method("longerThan", "second"));
    let b = step(&b, rename_method("prefixer", "first"));
    assert_ne!(a, b, "the preserving layout records the edit history");

    let canonical_a = canonical(&ClassModel::from_bytes(&a).unwrap());
    let canonical_b = canonical(&ClassModel::from_bytes(&b).unwrap());
    assert_eq!(canonical_a, canonical_b);
    assert_structurally_clean(&canonical_a, "canonical history");
    assert!(canonical_a.len() < a.len(), "orphaned entries are dropped");

    // The default policy is untouched: the original still round-trips.
    let model = ClassModel::from_bytes(&original).unwrap();
    assert_eq!(model.to_bytes().unwrap(), original);
    let (preserved, _) = model
        .to_bytes_with_pool_policy(
            DebugInfoPolicy::Preserve,
            FrameComputationMode::Preserve,
            None,
            ConstantPoolPolicy::Preserve,
        )
        .unwrap();
    assert_eq!(preserved, original);
}

fn simple(opcode: u8) -> CodeItem {
    CodeItem::Raw(Instruction::Simple { opcode, offset: 0 })
}

/// `static void count(int n) { for (int i = 0; i < n; i++) ldc "tick"; }`
/// with a loop, so the method carries StackMapTable frames after the `ldc`.
fn ldc_in_loop() -> ClassModel {
    let head = Label::new();
    let exit = Label::new();
    let mut code = CodeModel::new(2, 2, DebugInfoState::Fresh);
    code.instructions = vec![
        simple(0x03), // iconst_0
        CodeItem::Var(VarInsn {
            opcode: 0x36,
            slot: 1,
        }),
        CodeItem::Label(head.clone()),
        CodeItem::Var(VarInsn {
            opcode: 0x15,
            slot: 1,
        }),
        CodeItem::Var(VarInsn {
            opcode: 0x15,
            slot: 0,
        }),
        CodeItem::Branch(BranchInsn {
            opcode: 0xA2, // if_icmpge
            target: exit.clone(),
        }),
        CodeItem::Ldc(LdcInsn {
            value: LdcValue::String(JavaString::from("tick")),
        }),
        simple(0x57), // pop
        CodeItem::IInc(pytecode_engine::model::IIncInsn { slot: 1, value: 1 }),
        CodeItem::Branch(BranchInsn {
            opcode: 0xA7, // goto
            target: head,
        }),
        CodeItem::Label(exit),
        simple(0xB1), // return
    ];
    let mut model = ClassModel {
        name: "t/Ticker".to_owned(),
        super_name: Some("java/lang/Object".to_owned()),
        version: (52, 0),
        access_flags: ClassAccessFlags::PUBLIC | ClassAccessFlags::SUPER,
        ..ClassModel::default()
    };
    model.methods.push(MethodModel::new(
        MethodAccessFlags::PUBLIC | MethodAccessFlags::STATIC,
        "count".to_owned(),
        "(I)V".to_owned(),
        Some(code),
        Vec::new(),
    ));
    model
}

fn ldc_opcode(bytes: &[u8]) -> u8 {
    let classfile = parse_class(bytes).unwrap();
    classfile.methods[0]
        .attributes
        .iter()
        .find_map(|attribute| match attribute {
            AttributeInfo::Code(code) => code.code.iter().find_map(|insn| match insn {
                Instruction::ConstantPoolIndex1 { opcode, .. } => Some(*opcode),
                Instruction::ConstantPoolIndexWide(insn) => Some(insn.opcode),
                _ => None,
            }),
            _ => None,
        })
        .expect("method should load a constant")
}

#[test]
fn pool_order_that_widens_ldc_still_canonicalizes_with_preserved_frames() {
    // History A grows the pool past 255 entries before the string constant
    // is added, so the constant needs `ldc_w`; history B adds it early, so a
    // narrow `ldc` reaches it. The code lengths differ, and so do the
    // StackMapTable offsets after the `ldc`.
    let mut crowded = ldc_in_loop();
    for index in 0..300 {
        crowded
            .constant_pool
            .add_utf8(&format!("filler{index}"))
            .unwrap();
    }
    let a = crowded
        .to_bytes_with_options(
            DebugInfoPolicy::Preserve,
            FrameComputationMode::Recompute,
            None,
        )
        .unwrap();
    let b = ldc_in_loop()
        .to_bytes_with_options(
            DebugInfoPolicy::Preserve,
            FrameComputationMode::Recompute,
            None,
        )
        .unwrap();
    assert_eq!(ldc_opcode(&a), 0x13, "ldc_w");
    assert_eq!(ldc_opcode(&b), 0x12, "ldc");

    // Lowered from the lifted classes, frames are preserved, so canonical
    // emission has to move them to the new offsets.
    let canonical_a = canonical(&ClassModel::from_bytes(&a).unwrap());
    let canonical_b = canonical(&ClassModel::from_bytes(&b).unwrap());
    assert_eq!(canonical_a, canonical_b);
    assert_eq!(ldc_opcode(&canonical_a), 0x12);
    assert_structurally_clean(&canonical_a, "relocated frames");

    // The relocated frames are the ones recomputation produces.
    let recomputed = ClassModel::from_bytes(&canonical_a)
        .unwrap()
        .to_bytes_with_options(
            DebugInfoPolicy::Preserve,
            FrameComputationMode::Recompute,
            None,
        )
        .unwrap();
    assert_eq!(recomputed, canonical_a);
}

#[test]
fn canonical_emission_is_stable_across_every_fixture() {
    let mut checked = 0;
    for path in compiled_fixture_paths(25).expect("fixtures should compile") {
        let original = fs::read(&path).unwrap();
        let context = path.display().to_string();
        let first = canonical_bytes(&original);
        assert_structurally_clean(&first, &context);
        assert_eq!(canonical_bytes(&first), first, "{context}: not idempotent");

        // Same content through the model, with and without recomputed frames.
        let model = ClassModel::from_bytes(&original).unwrap();
        assert_eq!(canonical(&model), first, "{context}: model path differs");
        let (recomputed, _) = model
            .to_bytes_with_pool_policy(
                DebugInfoPolicy::Preserve,
                FrameComputationMode::Recompute,
                None,
                ConstantPoolPolicy::Canonical,
            )
            .unwrap_or_else(|error| panic!("{context}: {error}"));
        assert_structurally_clean(&recomputed, &context);

        // An orphaned entry is history, not content.
        let mut padded = model.clone();
        padded.constant_pool.add_utf8("orphaned history").unwrap();
        padded
            .constant_pool
            .add_long(0x0123_4567_89AB_CDEF)
            .unwrap();
        assert_ne!(padded.to_bytes().unwrap(), original);
        assert_eq!(canonical(&padded), first, "{context}: orphan survived");
        checked += 1;
    }
    assert!(checked > 20, "expected the full fixture set, saw {checked}");
}

#[test]
fn unparsed_attributes_are_rejected_rather_than_misremapped() {
    let mut model =
        ClassModel::from_bytes(&fixture_bytes("HelloWorld.java", "HelloWorld.class")).unwrap();
    let name = model.constant_pool.add_utf8("Opaque").unwrap();
    model
        .attributes
        .push(AttributeInfo::Unknown(UnknownAttribute {
            attribute_name_index: name,
            attribute_length: 2,
            name: "Opaque".to_owned(),
            info: vec![0, 1],
        }));
    assert!(model.to_bytes().is_ok(), "the default policy keeps it");
    let error = model
        .to_bytes_with_pool_policy(
            DebugInfoPolicy::Preserve,
            FrameComputationMode::Preserve,
            None,
            ConstantPoolPolicy::Canonical,
        )
        .unwrap_err();
    assert!(error.to_string().contains("Opaque"), "{error}");
}
