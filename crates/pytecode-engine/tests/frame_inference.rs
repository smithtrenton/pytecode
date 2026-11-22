//! Frame recomputation with incomplete hierarchy information.
//!
//! These cover merges whose common supertype needs classes the resolver
//! cannot provide (or no resolver at all): partial superclass chains, the
//! facts deduced from typed uses of merged values, the warnings left when
//! neither suffices, and frame placement.

use pytecode_engine::analysis::{
    DuplicateClassPolicy, FrameComputationResult, MappingClassResolver, ResolvedClass, VType,
    recompute_frames,
};
use pytecode_engine::constants::{ClassAccessFlags, MethodAccessFlags};
use pytecode_engine::model::{
    BootstrapMethodModel, BranchInsn, ClassModel, CodeItem, CodeModel, DebugInfoState, Label,
    LdcValue, MethodHandleValue, MethodInsn, VarInsn,
};

fn class(name: &str, super_name: Option<&str>) -> ResolvedClass {
    ResolvedClass {
        name: name.to_owned(),
        super_name: super_name.map(ToOwned::to_owned),
        interfaces: Vec::new(),
        access_flags: ClassAccessFlags::PUBLIC | ClassAccessFlags::SUPER,
        methods: Vec::new(),
    }
}

fn resolver(classes: &[(&str, Option<&str>)]) -> MappingClassResolver {
    MappingClassResolver::new(classes.iter().map(|(name, parent)| class(name, *parent))).unwrap()
}

/// `static void run(int flag, A a, B b) { sink(flag != 0 ? a : b); }`
/// where `sink` takes `required`.
fn join_then_use(required: &str) -> CodeModel {
    let else_label = Label::new();
    let join_label = Label::new();
    let mut code = CodeModel::new(0, 0, DebugInfoState::Fresh);
    code.instructions = vec![
        CodeItem::Var(VarInsn {
            opcode: 0x15, // iload
            slot: 0,
        }),
        CodeItem::Branch(BranchInsn {
            opcode: 0x99, // ifeq
            target: else_label.clone(),
        }),
        CodeItem::Var(VarInsn {
            opcode: 0x19, // aload
            slot: 1,
        }),
        CodeItem::Branch(BranchInsn {
            opcode: 0xA7, // goto
            target: join_label.clone(),
        }),
        CodeItem::Label(else_label),
        CodeItem::Var(VarInsn {
            opcode: 0x19,
            slot: 2,
        }),
        CodeItem::Label(join_label),
        CodeItem::Method(MethodInsn {
            opcode: 0xB8, // invokestatic
            owner: "t/Sink".to_owned(),
            name: "sink".to_owned(),
            descriptor: format!("(L{required};)V"),
            is_interface: false,
        }),
        CodeItem::Raw(pytecode_engine::raw::Instruction::Simple {
            opcode: 0xB1, // return
            offset: 0,
        }),
    ];
    code
}

fn recompute(required: &str, resolver: Option<&MappingClassResolver>) -> FrameComputationResult {
    recompute_frames(
        &join_then_use(required),
        "t/Owner",
        "run",
        "(ILa/A;Lb/B;)V",
        MethodAccessFlags::STATIC,
        resolver.map(|resolver| resolver as &dyn pytecode_engine::analysis::ClassResolver),
    )
    .unwrap()
}

fn join_stack(result: &FrameComputationResult) -> &[VType] {
    &result.frames.last().unwrap().stack
}

#[test]
fn frames_are_placed_only_at_branch_targets() {
    let result = recompute("java/lang/Object", None);
    // The else branch and the join; the fall-through after `ifeq` needs none.
    let indices = result
        .frames
        .iter()
        .map(|frame| frame.code_index)
        .collect::<Vec<_>>();
    assert_eq!(indices, vec![5, 7]);
}

#[test]
fn partial_superclass_chains_give_exact_merges() {
    // Neither chain reaches java/lang/Object, but they meet at t/T.
    let resolver = resolver(&[
        ("a/A", Some("t/T")),
        ("b/B", Some("t/T")),
        ("t/T", Some("missing/Base")),
    ]);
    let result = recompute("t/T", Some(&resolver));
    assert_eq!(join_stack(&result), [VType::Object("t/T".to_owned())]);
    assert!(result.warnings.is_empty());
}

#[test]
fn unresolved_classes_do_not_abort_recomputation() {
    // Only the sink's parameter type is known; a/A and b/B are not.
    let resolver = resolver(&[("t/T", None)]);
    let result = recompute("t/T", Some(&resolver));
    assert!(result.warnings.is_empty());
    assert_eq!(join_stack(&result), [VType::Object("t/T".to_owned())]);
}

#[test]
fn typed_uses_of_merged_values_are_inferred_without_a_resolver() {
    let result = recompute("t/T", None);
    // a/A and b/B both flow into a use requiring t/T, so the join is t/T.
    assert_eq!(join_stack(&result), [VType::Object("t/T".to_owned())]);
    assert!(result.warnings.is_empty());
}

#[test]
fn imprecise_merges_used_only_as_object_stay_quiet() {
    let result = recompute("java/lang/Object", None);
    assert_eq!(
        join_stack(&result),
        [VType::Object("java/lang/Object".to_owned())]
    );
    assert!(result.warnings.is_empty());
}

#[test]
fn unknown_required_types_with_a_known_member_chain_are_interfaces() {
    // a/A's ancestry is fully known and lacks t/I, so t/I must be an
    // interface — which accepts any reference.
    let resolver = resolver(&[("a/A", None)]);
    let result = recompute("t/I", Some(&resolver));
    assert!(result.warnings.is_empty());
}

#[test]
fn merges_that_cannot_reach_a_required_class_are_reported() {
    // a/A is known to extend java/lang/Object directly, t/T is a known
    // class, and b/B is unknown: no frame type can satisfy the use.
    let resolver = resolver(&[("a/A", None), ("t/T", None)]);
    let result = recompute("t/T", Some(&resolver));
    let [warning] = result.warnings.as_slice() else {
        panic!("expected one warning, got {:?}", result.warnings);
    };
    assert_eq!(warning.method_name, "run");
    assert_eq!(warning.required_types, vec!["t/T".to_owned()]);
    assert_eq!(warning.code_indices, vec![7]);
    assert_eq!(warning.unresolved_classes, vec!["b/B".to_owned()]);
    assert!(warning.resolver_supplied);
    let message = warning.to_string();
    assert!(message.contains("t/Owner.run(ILa/A;Lb/B;)V"), "{message}");
    assert!(message.contains("b/B"), "{message}");
    // The internal form of an imprecise merge never escapes.
    assert_eq!(
        join_stack(&result),
        [VType::Object("java/lang/Object".to_owned())]
    );
}

#[test]
fn duplicate_class_policies() {
    let first = class("x/Dup", Some("x/First"));
    let last = class("x/Dup", Some("x/Last"));
    let error = MappingClassResolver::new([first.clone(), last.clone()]).unwrap_err();
    assert!(error.to_string().contains("x/Dup"), "{error}");

    let keep_first = MappingClassResolver::with_duplicate_policy(
        [first.clone(), last.clone()],
        DuplicateClassPolicy::KeepFirst,
    )
    .unwrap();
    let keep_last = MappingClassResolver::with_duplicate_policy(
        [first.clone(), last.clone()],
        DuplicateClassPolicy::KeepLast,
    )
    .unwrap();
    use pytecode_engine::analysis::ClassResolver;
    assert_eq!(
        keep_first
            .resolve_class("x/Dup")
            .unwrap()
            .super_name
            .as_deref(),
        Some("x/First")
    );
    assert_eq!(
        keep_last
            .resolve_class("x/Dup")
            .unwrap()
            .super_name
            .as_deref(),
        Some("x/Last")
    );
    assert_eq!(keep_first.len(), 1);
    assert!(keep_first.contains("x/Dup"));

    let other = MappingClassResolver::new([class("x/Other", None), last]).unwrap();
    let merged = keep_first
        .merged(&other, DuplicateClassPolicy::KeepFirst)
        .unwrap();
    assert_eq!(merged.len(), 2);
    assert_eq!(
        merged.resolve_class("x/Dup").unwrap().super_name.as_deref(),
        Some("x/First")
    );
    assert!(
        keep_first
            .merged(&other, DuplicateClassPolicy::Error)
            .is_err()
    );
}

fn handle(owner: &str, name: &str) -> MethodHandleValue {
    MethodHandleValue {
        reference_kind: 6,
        owner: owner.to_owned(),
        name: name.to_owned(),
        descriptor: "()V".to_owned(),
        is_interface: false,
    }
}

#[test]
fn bootstrap_methods_can_be_read_and_rewritten() {
    let mut model = ClassModel {
        name: "t/Owner".to_owned(),
        super_name: Some("java/lang/Object".to_owned()),
        version: (55, 0),
        access_flags: ClassAccessFlags::PUBLIC | ClassAccessFlags::SUPER,
        ..ClassModel::default()
    };
    assert!(model.bootstrap_methods().unwrap().is_empty());

    let original = vec![BootstrapMethodModel {
        method: handle("java/lang/invoke/LambdaMetafactory", "metafactory"),
        arguments: vec![
            LdcValue::MethodType("()V".to_owned()),
            LdcValue::MethodHandle(handle("t/Old", "body")),
            LdcValue::Class("t/Old".to_owned()),
            LdcValue::Long(7),
        ],
    }];
    model.set_bootstrap_methods(&original).unwrap();
    assert_eq!(model.bootstrap_methods().unwrap(), original);

    let reparsed = ClassModel::from_bytes(&model.to_bytes().unwrap()).unwrap();
    assert_eq!(reparsed.bootstrap_methods().unwrap(), original);

    // Rewriting unchanged entries reuses every constant-pool entry.
    let mut identity = reparsed.clone();
    identity
        .set_bootstrap_methods(&reparsed.bootstrap_methods().unwrap())
        .unwrap();
    assert_eq!(identity.to_bytes().unwrap(), reparsed.to_bytes().unwrap());

    // Rename the class the arguments refer to; the entry stays in place.
    let mut renamed = original.clone();
    renamed[0].arguments[1] = LdcValue::MethodHandle(handle("t/New", "body"));
    renamed[0].arguments[2] = LdcValue::Class("t/New".to_owned());
    let mut edited = reparsed.clone();
    edited.set_bootstrap_methods(&renamed).unwrap();
    let edited = ClassModel::from_bytes(&edited.to_bytes().unwrap()).unwrap();
    assert_eq!(edited.bootstrap_methods().unwrap(), renamed);

    let mut cleared = edited.clone();
    cleared.set_bootstrap_methods(&[]).unwrap();
    assert!(cleared.bootstrap_methods().unwrap().is_empty());
    assert!(!cleared.attributes.iter().any(|attribute| matches!(
        attribute,
        pytecode_engine::raw::AttributeInfo::BootstrapMethods(_)
    )));
}

#[test]
fn invalid_bootstrap_methods_leave_the_model_unchanged() {
    let mut model = ClassModel {
        name: "t/Owner".to_owned(),
        super_name: Some("java/lang/Object".to_owned()),
        version: (55, 0),
        ..ClassModel::default()
    };
    let before = model.constant_pool.clone();
    let mut bad = handle("t/Owner", "bad");
    bad.reference_kind = 42;
    let error = model
        .set_bootstrap_methods(&[BootstrapMethodModel {
            method: handle("t/Owner", "bsm"),
            arguments: vec![LdcValue::MethodHandle(bad)],
        }])
        .unwrap_err();
    assert!(error.to_string().contains("reference kind"), "{error}");
    assert!(model.attributes.is_empty());
    assert_eq!(model.constant_pool.entries(), before.entries());
}
