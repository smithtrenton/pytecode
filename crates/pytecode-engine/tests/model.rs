use pytecode_engine::fixtures::compiled_fixture_paths_for;
use pytecode_engine::indexes::*;
use pytecode_engine::model::{
    BranchInsn, ClassModel, CodeItem, CodeModel, ConstantPoolBuilder, DebugInfoPolicy,
    DebugInfoState, Label, MethodModel, VarInsn, mark_class_debug_info_stale,
    mark_method_debug_info_stale,
};
use pytecode_engine::modified_utf8::decode_modified_utf8;
use pytecode_engine::raw::{AttributeInfo, ConstantPoolEntry, Instruction};
use pytecode_engine::transform::{insn_is_label, insn_opcode, insn_var_slot};
use pytecode_engine::{EngineErrorKind, parse_class};
use std::fs;

fn fixture_bytes(resource_name: &str, class_name: &str) -> Vec<u8> {
    let path = compiled_fixture_paths_for(resource_name)
        .expect("fixture paths should load")
        .into_iter()
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name == class_name)
                .unwrap_or(false)
        })
        .unwrap_or_else(|| panic!("fixture {class_name} not found for {resource_name}"));
    fs::read(path).expect("fixture bytes should read")
}

fn first_method_with_code(model: &mut ClassModel) -> &mut pytecode_engine::model::MethodModel {
    model
        .methods
        .iter_mut()
        .find(|method| method.code.is_some())
        .expect("fixture should contain method with code")
}

fn method_named<'a>(model: &'a mut ClassModel, name: &str) -> &'a mut MethodModel {
    model
        .methods
        .iter_mut()
        .find(|method| method.name == name)
        .unwrap_or_else(|| panic!("method {name} not found"))
}

fn first_method_with_code_index(model: &ClassModel) -> usize {
    model
        .methods
        .iter()
        .position(|method| method.code.is_some())
        .expect("fixture should contain method with code")
}

fn code_mut(method: &mut MethodModel) -> &mut CodeModel {
    method.code.as_mut().expect("method should have code")
}

fn strip_code_attr_named(code: &mut CodeModel, name: &str) {
    code.attributes
        .retain(|attribute| !attribute_named(attribute, name));
}

fn has_code_attr_named(code: &CodeModel, name: &str) -> bool {
    code.attributes
        .iter()
        .any(|attribute| attribute_named(attribute, name))
}

fn raw_code_attr_named(code: &pytecode_engine::raw::CodeAttribute, name: &str) -> bool {
    code.attributes
        .iter()
        .any(|attribute| attribute_named(attribute, name))
}

fn has_class_debug_info(attributes: &[AttributeInfo]) -> bool {
    attributes.iter().any(|attribute| {
        matches!(
            attribute,
            AttributeInfo::SourceFile(_) | AttributeInfo::SourceDebugExtension(_)
        )
    })
}

fn has_code_debug_info(attributes: &[AttributeInfo]) -> bool {
    attributes.iter().any(|attribute| {
        attribute_named(attribute, "LineNumberTable")
            || attribute_named(attribute, "LocalVariableTable")
            || attribute_named(attribute, "LocalVariableTypeTable")
    })
}

fn attribute_named(attribute: &AttributeInfo, name: &str) -> bool {
    match (attribute, name) {
        (AttributeInfo::StackMapTable(_), "StackMapTable")
        | (AttributeInfo::LineNumberTable(_), "LineNumberTable")
        | (AttributeInfo::LocalVariableTable(_), "LocalVariableTable")
        | (AttributeInfo::LocalVariableTypeTable(_), "LocalVariableTypeTable") => true,
        (AttributeInfo::Unknown(unknown), name) => unknown.name == name,
        _ => false,
    }
}

fn cp_utf8(pool: &[Option<ConstantPoolEntry>], index: Utf8Index) -> String {
    let entry = pool[index.value() as usize]
        .as_ref()
        .expect("constant-pool entry should exist");
    match entry {
        ConstantPoolEntry::Utf8(info) => {
            decode_modified_utf8(&info.bytes).expect("modified utf8 should decode")
        }
        _ => panic!("constant-pool entry {index} is not Utf8"),
    }
}

fn raw_method_code(lowered_bytes: &[u8], method_name: &str) -> Vec<Instruction> {
    let classfile = parse_class(lowered_bytes).expect("lowered bytes should parse");
    classfile
        .methods
        .into_iter()
        .find(|method| cp_utf8(&classfile.constant_pool, method.name_index) == method_name)
        .and_then(|method| {
            method
                .attributes
                .into_iter()
                .find_map(|attribute| match attribute {
                    AttributeInfo::Code(code) => Some(code.code),
                    _ => None,
                })
        })
        .unwrap_or_else(|| panic!("raw method {method_name} not found"))
}

fn method_code_attr_named<'a>(
    classfile: &'a pytecode_engine::raw::ClassFile,
    method_name: &str,
) -> &'a pytecode_engine::raw::CodeAttribute {
    classfile
        .methods
        .iter()
        .find(|method| cp_utf8(&classfile.constant_pool, method.name_index) == method_name)
        .and_then(|method| {
            method
                .attributes
                .iter()
                .find_map(|attribute| match attribute {
                    AttributeInfo::Code(code) => Some(code),
                    _ => None,
                })
        })
        .unwrap_or_else(|| panic!("code attribute for method {method_name} not found"))
}

fn raw_nop() -> CodeItem {
    CodeItem::Raw(Instruction::Simple {
        opcode: 0x00,
        offset: 0,
    })
}

#[test]
fn code_model_search_helpers_find_and_count_matches() {
    let mut code = CodeModel::new(1, 1, DebugInfoState::Fresh);
    let label = Label::named("start");
    code.instructions = vec![
        CodeItem::Label(label),
        raw_nop(),
        CodeItem::Var(VarInsn {
            opcode: 0x15,
            slot: 2,
        }),
        raw_nop(),
    ];

    assert_eq!(code.find_insns(&insn_opcode(0x00)), vec![1, 3]);
    assert_eq!(code.find_insn(&insn_var_slot(2), 0), Some(2));
    assert!(code.contains_insn(&insn_is_label()));
    assert_eq!(code.count_insns(&insn_opcode(0x00)), 2);
    assert_eq!(
        code.find_sequences(&[insn_opcode(0x00), insn_var_slot(2)]),
        vec![1]
    );
}

#[test]
fn code_model_edit_helpers_replace_insert_and_remove() {
    let mut code = CodeModel::new(1, 1, DebugInfoState::Fresh);
    code.instructions = vec![raw_nop(), raw_nop()];

    assert_eq!(code.insert_before(&insn_opcode(0x00), &[raw_nop()]), 2);
    assert_eq!(code.instructions.len(), 4);
    assert_eq!(code.insert_after(&insn_opcode(0x00), &[raw_nop()]), 4);
    assert_eq!(code.instructions.len(), 8);
    assert_eq!(code.replace_insns(&insn_opcode(0x00), &[]), 8);
    assert!(code.instructions.is_empty());
}

#[test]
fn code_model_sequence_edit_helpers_use_non_overlapping_matches() {
    let mut code = CodeModel::new(1, 1, DebugInfoState::Fresh);
    code.instructions = vec![raw_nop(), raw_nop(), raw_nop(), raw_nop(), raw_nop()];

    assert_eq!(
        code.replace_sequences(
            &[insn_opcode(0x00), insn_opcode(0x00)],
            &[CodeItem::Var(VarInsn {
                opcode: 0x15,
                slot: 7,
            })]
        ),
        2
    );
    assert_eq!(code.instructions.len(), 3);
    assert_eq!(
        code.find_insns(&insn_var_slot(7)),
        vec![0, 1],
        "sequence replacement should skip overlaps"
    );
    assert_eq!(code.count_insns(&insn_opcode(0x00)), 1);

    assert_eq!(
        code.remove_sequences(&[insn_var_slot(7), insn_var_slot(7)]),
        1
    );
    assert_eq!(code.instructions.len(), 1);
    assert_eq!(code.count_insns(&insn_opcode(0x00)), 1);
}

fn install_jsr_subroutine(code: &mut CodeModel) {
    let subroutine = Label::named("subroutine");
    code.instructions = vec![
        CodeItem::Branch(BranchInsn {
            opcode: 0xA8,
            target: subroutine.clone(),
        }),
        CodeItem::Var(VarInsn {
            opcode: 0x15,
            slot: 0,
        }),
        CodeItem::Raw(Instruction::Simple {
            opcode: 0xAC,
            offset: 0,
        }),
        CodeItem::Label(subroutine),
        CodeItem::Var(VarInsn {
            opcode: 0x3A,
            slot: 1,
        }),
        CodeItem::Var(VarInsn {
            opcode: 0xA9,
            slot: 1,
        }),
    ];
    code.exception_handlers.clear();
    code.line_numbers.clear();
    code.local_variables.clear();
    code.local_variable_types.clear();
    strip_code_attr_named(code, "StackMapTable");
}

fn first_conditional_branch_target(code: &CodeModel) -> BranchInsn {
    code.instructions
        .iter()
        .find_map(|item| match item {
            CodeItem::Branch(branch) if !matches!(branch.opcode, 0xA7 | 0xA8) => {
                Some(branch.clone())
            }
            _ => None,
        })
        .expect("expected conditional branch")
}

#[test]
fn constant_pool_builder_deduplicates_wide_entries() {
    let mut builder = ConstantPoolBuilder::new();
    let first = builder.add_long(42).expect("long entry should add");
    let second = builder
        .add_long(42)
        .expect("duplicate long entry should dedupe");
    assert_eq!(first, second);
    assert!(builder.peek(first.value() + 1).is_none());
    assert_eq!(builder.count(), first.value() + 2);
}

#[test]
fn unchanged_hello_world_model_roundtrip_preserves_bytes() {
    let bytes = fixture_bytes("HelloWorld.java", "HelloWorld.class");
    let model = ClassModel::from_bytes(&bytes).expect("fixture should lift");
    let lowered = model.to_bytes().expect("fixture should lower");
    assert_eq!(lowered, bytes);
}

#[test]
fn static_interface_methods_preserve_interface_ref_invocations() {
    let bytes = fixture_bytes(
        "StaticInterfaceMethods.java",
        "StaticInterfaceMethods.class",
    );
    let model = ClassModel::from_bytes(&bytes).expect("fixture should lift");
    let found = model.methods.iter().any(|method| {
        method.code.as_ref().is_some_and(|code| {
            code.instructions.iter().any(|item| {
                matches!(
                    item,
                    CodeItem::Method(insn) if insn.is_interface
                )
            })
        })
    });
    assert!(
        found,
        "expected at least one interface-backed non-invokeinterface call"
    );
    let lowered = model.to_bytes().expect("fixture should lower");
    let roundtripped = ClassModel::from_bytes(&lowered).expect("lowered bytes should still lift");
    let found_again = roundtripped.methods.iter().any(|method| {
        method.code.as_ref().is_some_and(|code| {
            code.instructions.iter().any(|item| {
                matches!(
                    item,
                    CodeItem::Method(insn) if insn.is_interface
                )
            })
        })
    });
    assert!(
        found_again,
        "lowered class should retain interface-backed invocations"
    );
}

#[test]
fn stale_debug_info_flags_strip_lowered_debug_attributes_without_mutating_lists() {
    let bytes = fixture_bytes("HelloWorld.java", "HelloWorld.class");
    let mut model = ClassModel::from_bytes(&bytes).expect("fixture should lift");
    assert!(has_class_debug_info(&model.attributes));

    let method_index = first_method_with_code_index(&model);
    let (original_line_numbers, original_local_variables, original_local_variable_types) = {
        let method = &model.methods[method_index];
        let code = method.code.as_ref().expect("method should have code");
        assert!(!code.line_numbers.is_empty());
        (
            code.line_numbers.clone(),
            code.local_variables.clone(),
            code.local_variable_types.clone(),
        )
    };

    mark_class_debug_info_stale(&mut model);
    {
        let method = &mut model.methods[method_index];
        mark_method_debug_info_stale(method);
    }

    let lowered = model.to_classfile().expect("fixture should lower");
    assert!(!has_class_debug_info(&lowered.attributes));

    let lowered_code = lowered
        .methods
        .iter()
        .flat_map(|method| method.attributes.iter())
        .find_map(|attribute| match attribute {
            AttributeInfo::Code(code) => Some(code),
            _ => None,
        })
        .expect("lowered fixture should retain code attribute");
    assert!(!has_code_debug_info(&lowered_code.attributes));

    let method = first_method_with_code(&mut model);
    let code = method.code.as_ref().expect("method should still have code");
    assert_eq!(code.line_numbers, original_line_numbers);
    assert_eq!(code.local_variables, original_local_variables);
    assert_eq!(code.local_variable_types, original_local_variable_types);
}

#[test]
fn strip_policy_removes_debug_info_without_mutating_model() {
    let bytes = fixture_bytes("HelloWorld.java", "HelloWorld.class");
    let mut model = ClassModel::from_bytes(&bytes).expect("fixture should lift");
    assert!(has_class_debug_info(&model.attributes));

    let method = first_method_with_code(&mut model);
    let code = method.code.as_ref().expect("method should have code");
    assert!(!code.line_numbers.is_empty());
    let original_line_numbers = code.line_numbers.clone();

    let lowered = model
        .to_classfile_with_policy(DebugInfoPolicy::Strip)
        .expect("fixture should lower with strip policy");
    assert!(!has_class_debug_info(&lowered.attributes));

    let lowered_code = lowered
        .methods
        .iter()
        .flat_map(|method| method.attributes.iter())
        .find_map(|attribute| match attribute {
            AttributeInfo::Code(code) => Some(code),
            _ => None,
        })
        .expect("lowered fixture should retain code attribute");
    assert!(!has_code_debug_info(&lowered_code.attributes));

    let method = first_method_with_code(&mut model);
    let code = method.code.as_ref().expect("method should still have code");
    assert_eq!(code.line_numbers, original_line_numbers);
}

#[test]
fn conditional_branch_edits_widen_successfully_when_stackmap_is_removed() {
    let bytes = fixture_bytes("ControlFlowExample.java", "ControlFlowExample.class");
    let mut model = ClassModel::from_bytes(&bytes).expect("fixture should lift");
    let code = code_mut(method_named(&mut model, "branch"));
    strip_code_attr_named(code, "StackMapTable");

    let branch = first_conditional_branch_target(code);
    let target_index = code
        .instructions
        .iter()
        .position(|item| matches!(item, CodeItem::Label(label) if *label == branch.target))
        .expect("branch target label should exist");
    let padding = (0..40_000).map(|_| raw_nop()).collect::<Vec<_>>();
    code.instructions
        .splice(target_index..target_index, padding);

    let lowered = model.to_bytes().expect("edited method should lower");
    let raw_code = raw_method_code(&lowered, "branch");
    assert!(
        raw_code
            .iter()
            .any(|instruction| matches!(instruction, Instruction::BranchWide { opcode: 0xC8, .. }))
    );
    ClassModel::from_bytes(&lowered).expect("widened method should re-lift");
}

#[test]
fn table_switch_padding_recomputes_after_code_motion() {
    let bytes = fixture_bytes("CfgEdgeCaseFixture.java", "CfgEdgeCaseFixture.class");
    let mut model = ClassModel::from_bytes(&bytes).expect("fixture should lift");
    let code = code_mut(method_named(&mut model, "largeTableSwitch"));
    strip_code_attr_named(code, "StackMapTable");

    let switch_index = code
        .instructions
        .iter()
        .position(|item| matches!(item, CodeItem::TableSwitch(_)))
        .expect("expected table switch");
    code.instructions.insert(switch_index, raw_nop());

    let lowered = model.to_bytes().expect("edited switch should lower");
    let raw_code = raw_method_code(&lowered, "largeTableSwitch");
    assert!(
        raw_code
            .iter()
            .any(|instruction| matches!(instruction, Instruction::TableSwitch(_)))
    );
    ClassModel::from_bytes(&lowered).expect("edited switch should re-lift");
}

#[test]
fn legacy_jsr_ret_methods_roundtrip_with_recomputed_frames_on_old_versions() {
    let bytes = fixture_bytes("ControlFlowExample.java", "ControlFlowExample.class");
    let mut model = ClassModel::from_bytes(&bytes).expect("fixture should lift");
    model.version = (49, 0);
    let code = code_mut(method_named(&mut model, "branch"));
    install_jsr_subroutine(code);

    let lowered = model
        .to_bytes_with_recomputed_frames(DebugInfoPolicy::Preserve, None)
        .expect("legacy jsr/ret method should lower with recomputed frames");
    let parsed = parse_class(&lowered).expect("lowered bytes should parse");
    let branch_code = method_code_attr_named(&parsed, "branch");
    assert!(!raw_code_attr_named(branch_code, "StackMapTable"));
    assert!(branch_code.code.iter().any(|instruction| {
        matches!(
            instruction,
            Instruction::Branch(pytecode_engine::raw::Branch { opcode: 0xA8, .. })
                | Instruction::BranchWide { opcode: 0xC9, .. }
        )
    }));
    assert!(branch_code.code.iter().any(|instruction| {
        matches!(
            instruction,
            Instruction::LocalIndex { opcode: 0xA9, .. }
                | Instruction::Wide(pytecode_engine::raw::WideInstruction { opcode: 0xA9, .. })
        )
    }));

    let roundtripped = ClassModel::from_bytes(&lowered).expect("lowered bytes should re-lift");
    let lifted_code = roundtripped
        .methods
        .iter()
        .find(|method| method.name == "branch")
        .and_then(|method| method.code.as_ref())
        .expect("re-lifted branch method should have code");
    assert!(
        lifted_code
            .instructions
            .iter()
            .any(|item| { matches!(item, CodeItem::Branch(BranchInsn { opcode: 0xA8, .. })) })
    );
    assert!(
        lifted_code
            .instructions
            .iter()
            .any(|item| { matches!(item, CodeItem::Var(VarInsn { opcode: 0xA9, .. })) })
    );
}

#[test]
fn edited_methods_with_stackmaptable_fail_explicitly_before_phase4() {
    let bytes = fixture_bytes("ControlFlowExample.java", "ControlFlowExample.class");
    let mut model = ClassModel::from_bytes(&bytes).expect("fixture should lift");
    let code = code_mut(method_named(&mut model, "branch"));
    assert!(has_code_attr_named(code, "StackMapTable"));

    let insert_at = code
        .instructions
        .iter()
        .position(|item| !matches!(item, CodeItem::Label(_)))
        .expect("method should contain instruction");
    code.instructions.insert(insert_at, raw_nop());

    let error = model
        .to_bytes()
        .expect_err("edited method with StackMapTable should fail");
    match error.kind {
        EngineErrorKind::InvalidModelState { reason } => {
            assert!(reason.contains("StackMapTable"));
            assert!(reason.contains("Phase 4") || reason.contains("recompute"));
        }
        other => panic!("unexpected error kind: {other:?}"),
    }
}
