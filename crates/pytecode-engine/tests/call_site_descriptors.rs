//! Editing `invokedynamic` call-site and dynamic-constant descriptors.
//!
//! A call site's descriptor lives in its `CONSTANT_InvokeDynamic` entry, not
//! in any method, so a rename that rewrites method descriptors has to rewrite
//! the call site too or the class fails to link at the call site's first
//! execution.

use pytecode_engine::analysis::{Severity, verify_classfile};
use pytecode_engine::constants::{ClassAccessFlags, MethodAccessFlags};
use pytecode_engine::fixtures::compiled_fixture_paths_for;
use pytecode_engine::model::{
    BootstrapMethodModel, ClassModel, CodeItem, CodeModel, ConstantPoolPolicy, DebugInfoPolicy,
    DebugInfoState, DynamicValue, FrameComputationMode, InvokeDynamicInsn, LdcInsn, LdcValue,
    MethodHandleValue, MethodModel,
};
use pytecode_engine::parse_class;
use pytecode_engine::raw::{AttributeInfo, ConstantPoolEntry, Instruction};
use std::fs;

fn fixture_bytes(resource_name: &str, class_name: &str) -> Vec<u8> {
    let path = compiled_fixture_paths_for(resource_name)
        .expect("fixture paths should load")
        .into_iter()
        .find(|path| path.file_name().and_then(|name| name.to_str()) == Some(class_name))
        .unwrap_or_else(|| panic!("fixture {class_name} not found for {resource_name}"));
    fs::read(path).expect("fixture bytes should read")
}

fn call_sites(model: &ClassModel) -> Vec<InvokeDynamicInsn> {
    model
        .methods
        .iter()
        .filter_map(MethodModel::code)
        .flat_map(|code| code.instructions.iter())
        .filter_map(|item| match item {
            CodeItem::InvokeDynamic(insn) => Some(insn.clone()),
            _ => None,
        })
        .collect()
}

fn rewrite_call_sites(model: &mut ClassModel, rewrite: impl Fn(&str) -> String) {
    for method in &mut model.methods {
        let Some(code) = method.code_mut() else {
            continue;
        };
        for item in &mut code.instructions {
            if let CodeItem::InvokeDynamic(insn) = item {
                insn.descriptor = rewrite(&insn.descriptor);
            }
        }
    }
}

fn assert_structurally_clean(bytes: &[u8]) {
    let classfile = parse_class(bytes).expect("lowered class should parse");
    let errors = verify_classfile(&classfile)
        .into_iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .collect::<Vec<_>>();
    assert!(errors.is_empty(), "structural errors: {errors:?}");
}

fn utf8_entries(bytes: &[u8]) -> Vec<String> {
    parse_class(bytes)
        .unwrap()
        .constant_pool
        .iter()
        .flatten()
        .filter_map(|entry| match entry {
            ConstantPoolEntry::Utf8(info) => {
                Some(String::from_utf8_lossy(&info.bytes).into_owned())
            }
            _ => None,
        })
        .collect()
}

/// Descriptors of the `CONSTANT_InvokeDynamic` entries that code refers to.
fn linked_call_site_descriptors(bytes: &[u8]) -> Vec<String> {
    let classfile = parse_class(bytes).unwrap();
    let pool = &classfile.constant_pool;
    let entry = |index: u16| pool[usize::from(index)].as_ref().unwrap();
    let mut descriptors = Vec::new();
    for method in &classfile.methods {
        for attribute in &method.attributes {
            let AttributeInfo::Code(code) = attribute else {
                continue;
            };
            for instruction in &code.code {
                let Instruction::InvokeDynamic(insn) = instruction else {
                    continue;
                };
                let ConstantPoolEntry::InvokeDynamic(site) = entry(insn.index.value()) else {
                    panic!("invokedynamic operand is not CONSTANT_InvokeDynamic");
                };
                let ConstantPoolEntry::NameAndType(nat) = entry(site.name_and_type_index.value())
                else {
                    panic!("call site does not reference a NameAndType");
                };
                let ConstantPoolEntry::Utf8(utf8) = entry(nat.descriptor_index.value()) else {
                    panic!("call-site descriptor is not Utf8");
                };
                descriptors.push(String::from_utf8_lossy(&utf8.bytes).into_owned());
            }
        }
    }
    descriptors
}

const THROWING_CONSUMER: &str = "LambdaShowcase$ThrowingConsumer";
const RENAMED_CONSUMER: &str = "LambdaShowcase$RenamedConsumer";

#[test]
fn call_site_descriptor_rewrite_reencodes_and_survives_reparse() {
    let original = fixture_bytes("LambdaShowcase.java", "LambdaShowcase.class");
    let mut model = ClassModel::from_bytes(&original).unwrap();
    let before = call_sites(&model);
    assert!(
        before
            .iter()
            .any(|site| site.descriptor.contains(THROWING_CONSUMER)),
        "fixture should have a call site naming {THROWING_CONSUMER}"
    );

    rewrite_call_sites(&mut model, |descriptor| {
        descriptor.replace(THROWING_CONSUMER, RENAMED_CONSUMER)
    });

    for frame_mode in [
        FrameComputationMode::Preserve,
        FrameComputationMode::Recompute,
    ] {
        let bytes = model
            .to_bytes_with_options(DebugInfoPolicy::Preserve, frame_mode, None)
            .unwrap_or_else(|error| panic!("{frame_mode:?} lowering failed: {error}"));
        assert_structurally_clean(&bytes);

        let after = call_sites(&ClassModel::from_bytes(&bytes).unwrap());
        assert_eq!(after.len(), before.len());
        for (old, new) in before.iter().zip(&after) {
            assert_eq!(new.name, old.name);
            assert_eq!(
                new.bootstrap_method_attr_index,
                old.bootstrap_method_attr_index
            );
            assert_eq!(
                new.descriptor,
                old.descriptor.replace(THROWING_CONSUMER, RENAMED_CONSUMER)
            );
        }
    }
}

#[test]
fn reassigning_the_same_call_site_descriptor_round_trips_byte_for_byte() {
    let original = fixture_bytes("LambdaShowcase.java", "LambdaShowcase.class");
    let mut model = ClassModel::from_bytes(&original).unwrap();
    rewrite_call_sites(&mut model, str::to_owned);
    let bytes = model.to_bytes().unwrap();
    assert_eq!(bytes, original);
    assert_eq!(
        parse_class(&bytes).unwrap().constant_pool.len(),
        parse_class(&original).unwrap().constant_pool.len(),
        "unchanged call sites reuse their pool entries"
    );
}

const BOOTSTRAP_DESCRIPTOR: &str = "(Ljava/lang/invoke/MethodHandles$Lookup;Ljava/lang/String;Ljava/lang/invoke/MethodType;)Ljava/lang/invoke/CallSite;";

fn simple(opcode: u8) -> CodeItem {
    CodeItem::Raw(Instruction::Simple { opcode, offset: 0 })
}

/// `t/Host.run()` pushes `null` into an `invokedynamic` whose call-site
/// descriptor is the only place `t/Old` appears in the class.
fn indy_only_host() -> ClassModel {
    let mut model = ClassModel {
        name: "t/Host".to_owned(),
        super_name: Some("java/lang/Object".to_owned()),
        version: (55, 0),
        access_flags: ClassAccessFlags::PUBLIC | ClassAccessFlags::SUPER,
        ..ClassModel::default()
    };
    model
        .set_bootstrap_methods(&[BootstrapMethodModel {
            method: MethodHandleValue {
                reference_kind: 6,
                owner: "t/Bootstraps".to_owned(),
                name: "link".to_owned(),
                descriptor: BOOTSTRAP_DESCRIPTOR.to_owned(),
                is_interface: false,
            },
            arguments: Vec::new(),
        }])
        .unwrap();
    let mut code = CodeModel::new(1, 0, DebugInfoState::Fresh);
    code.instructions = vec![
        simple(0x01), // aconst_null
        CodeItem::InvokeDynamic(InvokeDynamicInsn {
            bootstrap_method_attr_index: 0.into(),
            name: "make".to_owned(),
            descriptor: "(Lt/Old;)Ljava/lang/Runnable;".to_owned(),
        }),
        simple(0x57), // pop
        simple(0xB1), // return
    ];
    model.methods.push(MethodModel::new(
        MethodAccessFlags::PUBLIC | MethodAccessFlags::STATIC,
        "run".to_owned(),
        "()V".to_owned(),
        Some(code),
        Vec::new(),
    ));
    model
}

#[test]
fn a_class_named_only_by_a_call_site_descriptor_can_be_renamed() {
    let built = indy_only_host().to_bytes().unwrap();
    let mut model = ClassModel::from_bytes(&built).unwrap();
    rewrite_call_sites(&mut model, |descriptor| {
        descriptor.replace("t/Old", "t/New")
    });

    let bytes = model
        .to_bytes_with_options(
            DebugInfoPolicy::Preserve,
            FrameComputationMode::Recompute,
            None,
        )
        .unwrap();
    assert_structurally_clean(&bytes);
    let renamed = ClassModel::from_bytes(&bytes).unwrap();
    let sites = call_sites(&renamed);
    assert_eq!(sites.len(), 1);
    assert_eq!(sites[0].descriptor, "(Lt/New;)Ljava/lang/Runnable;");
    assert_eq!(sites[0].name, "make");
    assert_eq!(sites[0].bootstrap_method_attr_index.value(), 0);

    // The call site's CONSTANT_InvokeDynamic now points at the new
    // descriptor. The default (preserving) pool layout keeps the old
    // descriptor as an unreferenced entry; nothing refers to it.
    assert_eq!(
        linked_call_site_descriptors(&bytes),
        ["(Lt/New;)Ljava/lang/Runnable;"]
    );
    assert!(
        utf8_entries(&bytes)
            .iter()
            .any(|value| value.contains("t/Old"))
    );

    // The canonical layout keeps referenced entries only: the old name is
    // gone from the class entirely.
    let (canonical, _) = model
        .to_bytes_with_pool_policy(
            DebugInfoPolicy::Preserve,
            FrameComputationMode::Recompute,
            None,
            ConstantPoolPolicy::Canonical,
        )
        .unwrap();
    assert_structurally_clean(&canonical);
    assert_eq!(
        linked_call_site_descriptors(&canonical),
        ["(Lt/New;)Ljava/lang/Runnable;"]
    );
    assert!(
        !utf8_entries(&canonical)
            .iter()
            .any(|value| value.contains("t/Old"))
    );
}

#[test]
fn dynamic_constant_descriptor_rewrite_reencodes() {
    let mut model = indy_only_host();
    let code = model.methods[0].code_mut().unwrap();
    code.max_stack = 1;
    code.instructions = vec![
        CodeItem::Ldc(LdcInsn {
            value: LdcValue::Dynamic(DynamicValue {
                bootstrap_method_attr_index: 0.into(),
                name: "value".to_owned(),
                descriptor: "Lt/Old;".to_owned(),
            }),
        }),
        simple(0x57), // pop
        simple(0xB1), // return
    ];
    let built = model.to_bytes().unwrap();
    let mut model = ClassModel::from_bytes(&built).unwrap();

    // Same descriptor: byte-identical.
    assert_eq!(model.to_bytes().unwrap(), built);

    let code = model.methods[0].code_mut().unwrap();
    let CodeItem::Ldc(LdcInsn {
        value: LdcValue::Dynamic(value),
    }) = &mut code.instructions[0]
    else {
        panic!("expected a dynamic ldc");
    };
    value.descriptor = "Lt/New;".to_owned();
    let bytes = model
        .to_bytes_with_options(
            DebugInfoPolicy::Preserve,
            FrameComputationMode::Recompute,
            None,
        )
        .unwrap();
    assert_structurally_clean(&bytes);
    let reparsed = ClassModel::from_bytes(&bytes).unwrap();
    let CodeItem::Ldc(LdcInsn {
        value: LdcValue::Dynamic(value),
    }) = &reparsed.methods[0].code().unwrap().instructions[0]
    else {
        panic!("expected a dynamic ldc");
    };
    assert_eq!(value.descriptor, "Lt/New;");
    assert_eq!(value.name, "value");
    assert_eq!(value.bootstrap_method_attr_index.value(), 0);
}

/// Instructions rebuilt outside the engine (as Python write-backs are) carry
/// no original offsets; that alone must not count as a code change.
#[test]
fn rebuilt_raw_instructions_keep_preserved_frames_valid() {
    let original = fixture_bytes("LambdaShowcase.java", "LambdaShowcase.class");
    let mut model = ClassModel::from_bytes(&original).unwrap();
    let mut rebased_with_frames = 0;
    for method in &mut model.methods {
        let Some(code) = method.code_mut() else {
            continue;
        };
        let has_frames = code
            .attributes
            .iter()
            .any(|attribute| matches!(attribute, AttributeInfo::StackMapTable(_)));
        for item in &mut code.instructions {
            if let CodeItem::Raw(
                Instruction::Simple { offset, .. }
                | Instruction::LocalIndex { offset, .. }
                | Instruction::Byte { offset, .. }
                | Instruction::Short { offset, .. },
            ) = item
                && *offset != 0
            {
                *offset = 0;
                rebased_with_frames += usize::from(has_frames);
            }
        }
    }
    assert!(
        rebased_with_frames > 0,
        "fixture should rebase raw items in a framed method"
    );
    assert_eq!(model.to_bytes().unwrap(), original);
}
