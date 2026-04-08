use pytecode_engine::analysis::{
    ClassResolver, JAVA_LANG_OBJECT, MappingClassResolver, build_cfg, common_superclass,
    find_overridden_methods, is_reference, is_subtype, merge_vtypes, recompute_frames,
    vtype_from_field_descriptor_str,
};
use pytecode_engine::constants::MethodAccessFlags;
use pytecode_engine::model::{
    BranchInsn, ClassModel, CodeItem, CodeModel, DebugInfoPolicy, Label, VarInsn,
};
use pytecode_engine::parse_class;
use pytecode_engine::raw::{AttributeInfo, ConstantPoolEntry, Instruction};
use std::fs;

type TestResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn fixture_bytes(resource_name: &str, class_name: &str) -> Vec<u8> {
    let path = pytecode_engine::fixtures::compiled_fixture_paths_for(resource_name)
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

fn parse_fixture_classes(resource_name: &str) -> Vec<pytecode_engine::raw::ClassFile> {
    let mut classes = Vec::new();
    for path in pytecode_engine::fixtures::compiled_fixture_paths_for(resource_name)
        .expect("fixture paths should load")
    {
        classes.push(
            parse_class(&fs::read(path).expect("fixture bytes should read"))
                .expect("class should parse"),
        );
    }
    classes
}

fn method_named<'a>(
    model: &'a mut ClassModel,
    name: &str,
) -> &'a mut pytecode_engine::model::MethodModel {
    model
        .methods
        .iter_mut()
        .find(|method| method.name == name)
        .unwrap_or_else(|| panic!("method {name} not found"))
}

fn method_code_named<'a>(
    model: &'a ClassModel,
    name: &str,
) -> &'a pytecode_engine::model::CodeModel {
    model
        .methods
        .iter()
        .find(|method| method.name == name)
        .and_then(|method| method.code.as_ref())
        .unwrap_or_else(|| panic!("method {name} not found"))
}

fn code_mut(method: &mut pytecode_engine::model::MethodModel) -> &mut CodeModel {
    method.code.as_mut().expect("method should have code")
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
    code.attributes
        .retain(|attribute| !matches!(attribute, AttributeInfo::StackMapTable(_)));
}

fn has_code_attr_named(code: &pytecode_engine::raw::CodeAttribute, name: &str) -> bool {
    code.attributes
        .iter()
        .any(|attribute| match (attribute, name) {
            (AttributeInfo::StackMapTable(_), "StackMapTable") => true,
            (AttributeInfo::Unknown(unknown), name) => unknown.name == name,
            _ => false,
        })
}

fn cp_utf8(classfile: &pytecode_engine::raw::ClassFile, index: u16) -> String {
    match classfile.constant_pool[index as usize]
        .as_ref()
        .expect("cp entry should exist")
    {
        ConstantPoolEntry::Utf8(info) => {
            pytecode_engine::modified_utf8::decode_modified_utf8(&info.bytes)
                .expect("utf8 should decode")
        }
        _ => panic!("cp entry {index} should be Utf8"),
    }
}

fn method_code_attr_named<'a>(
    classfile: &'a pytecode_engine::raw::ClassFile,
    name: &str,
) -> &'a pytecode_engine::raw::CodeAttribute {
    classfile
        .methods
        .iter()
        .find(|method| cp_utf8(classfile, method.name_index) == name)
        .and_then(|method| {
            method
                .attributes
                .iter()
                .find_map(|attribute| match attribute {
                    AttributeInfo::Code(code) => Some(code),
                    _ => None,
                })
        })
        .unwrap_or_else(|| panic!("code attribute for method {name} not found"))
}

#[test]
fn hierarchy_queries_work_for_fixture_classes() -> TestResult<()> {
    let classfiles = parse_fixture_classes("HierarchyFixture.java");
    let resolver = MappingClassResolver::from_classfile_refs(classfiles.iter())?;
    let fixture_name = "fixture/hierarchy/HierarchyFixture";

    assert!(is_subtype(
        &resolver,
        fixture_name,
        "fixture/hierarchy/Mammal"
    )?);
    assert!(is_subtype(
        &resolver,
        fixture_name,
        "fixture/hierarchy/Trainable"
    )?);
    assert_eq!(
        common_superclass(&resolver, fixture_name, "fixture/hierarchy/Pet")?,
        JAVA_LANG_OBJECT
    );

    let resolved = resolver
        .resolve_class(fixture_name)
        .expect("fixture should resolve");
    let train = resolved
        .find_method("train", "()V")
        .expect("train method should resolve")
        .clone();
    let owners = find_overridden_methods(&resolver, fixture_name, &train)?
        .into_iter()
        .map(|method| method.owner)
        .collect::<Vec<_>>();
    assert!(
        owners
            .iter()
            .any(|owner| owner == "fixture/hierarchy/Mammal")
    );
    assert!(
        owners
            .iter()
            .any(|owner| owner == "fixture/hierarchy/Trainable")
    );
    Ok(())
}

#[test]
fn cfg_tracks_branch_and_exception_edges() -> TestResult<()> {
    let branch_model =
        ClassModel::from_bytes(&fixture_bytes("CfgFixture.java", "CfgFixture.class"))?;
    let branch_cfg = build_cfg(method_code_named(&branch_model, "ifElse"))?;
    assert!(
        branch_cfg
            .nodes
            .iter()
            .any(|node| node.normal_successors.len() == 2)
    );
    assert!(branch_cfg.nodes.iter().any(|node| node.is_jump_target));

    let try_model = ClassModel::from_bytes(&fixture_bytes(
        "TryCatchExample.java",
        "TryCatchExample.class",
    ))?;
    let try_cfg = build_cfg(method_code_named(&try_model, "safeDivide"))?;
    assert!(
        try_cfg
            .nodes
            .iter()
            .any(|node| !node.exception_successors.is_empty())
    );
    Ok(())
}

#[test]
fn recompute_frames_allows_phase3_edit_that_used_to_fail() -> TestResult<()> {
    let bytes = fixture_bytes("ControlFlowExample.java", "ControlFlowExample.class");
    let mut model = ClassModel::from_bytes(&bytes)?;
    let class_name = model.name.clone();
    let code = code_mut(method_named(&mut model, "branch"));
    let insert_at = code
        .instructions
        .iter()
        .position(|item| !matches!(item, CodeItem::Label(_)))
        .expect("method should contain instruction");
    code.instructions.insert(
        insert_at,
        CodeItem::Raw(Instruction::Simple {
            opcode: 0x00,
            offset: 0,
        }),
    );

    let frame_result = recompute_frames(
        code,
        &class_name,
        "branch",
        "(I)I",
        MethodAccessFlags::PUBLIC | MethodAccessFlags::STATIC,
        None,
    )?;
    assert!(!frame_result.frames.is_empty());

    let lowered = model.to_bytes_with_recomputed_frames(DebugInfoPolicy::Preserve, None)?;
    let parsed = parse_class(&lowered)?;
    let branch_code = method_code_attr_named(&parsed, "branch");
    assert!(has_code_attr_named(branch_code, "StackMapTable"));
    ClassModel::from_bytes(&lowered)?;
    Ok(())
}

#[test]
fn recompute_frames_supports_legacy_jsr_ret_subroutines() -> TestResult<()> {
    let bytes = fixture_bytes("ControlFlowExample.java", "ControlFlowExample.class");
    let mut model = ClassModel::from_bytes(&bytes)?;
    let class_name = model.name.clone();
    let code = code_mut(method_named(&mut model, "branch"));
    install_jsr_subroutine(code);

    let cfg = build_cfg(code)?;
    assert!(cfg.nodes.iter().any(|node| matches!(
        code.instructions[node.code_index],
        CodeItem::Branch(BranchInsn { opcode: 0xA8, .. })
    )));

    let frame_result = recompute_frames(
        code,
        &class_name,
        "branch",
        "(I)I",
        MethodAccessFlags::PUBLIC | MethodAccessFlags::STATIC,
        None,
    )?;
    assert_eq!(frame_result.max_stack, 1);
    assert!(frame_result.max_locals >= 2);
    assert!(!frame_result.frames.is_empty());
    Ok(())
}

#[test]
fn vtype_helpers_cover_references_and_object_merges() -> TestResult<()> {
    let int_type = vtype_from_field_descriptor_str("I")?;
    let string_type = vtype_from_field_descriptor_str("Ljava/lang/String;")?;
    assert!(!is_reference(&int_type));
    assert!(is_reference(&string_type));
    assert_eq!(
        merge_vtypes(&pytecode_engine::analysis::VType::Null, &string_type, None),
        string_type
    );
    Ok(())
}
