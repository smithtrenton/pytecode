use pytecode_engine::constants::{ClassAccessFlags, MethodAccessFlags};
use pytecode_engine::fixtures::compiled_fixture_paths_for;
use pytecode_engine::model::{ClassModel, CodeItem};
use pytecode_engine::raw::Instruction;
use pytecode_engine::transform::{
    Pipeline, class_named, method_is_public, method_is_static, method_name_matches, method_named,
    on_code, on_methods,
};
use std::fs;
use std::sync::{Arc, Mutex};

type TestResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn fixture_bytes(resource_name: &str, class_name: &str) -> Vec<u8> {
    let path = compiled_fixture_paths_for(resource_name)
        .expect("fixture paths should load")
        .into_iter()
        .find(|path| path.file_name().and_then(|name| name.to_str()) == Some(class_name))
        .unwrap_or_else(|| panic!("fixture {class_name} not found for {resource_name}"));
    fs::read(path).expect("fixture bytes should read")
}

fn method_named_mut<'a>(
    model: &'a mut ClassModel,
    name: &str,
) -> &'a mut pytecode_engine::model::MethodModel {
    model
        .methods
        .iter_mut()
        .find(|method| method.name == name)
        .unwrap_or_else(|| panic!("method {name} not found"))
}

#[test]
fn pipeline_applies_transforms_in_order() -> TestResult<()> {
    let bytes = fixture_bytes("HelloWorld.java", "HelloWorld.class");
    let mut model = ClassModel::from_bytes(&bytes)?;
    let events = Arc::new(Mutex::new(Vec::new()));
    let rename_events = Arc::clone(&events);
    let retarget_events = Arc::clone(&events);

    let mut pipeline = Pipeline::of(move |model: &mut ClassModel| {
        rename_events
            .lock()
            .expect("events mutex")
            .push("rename".to_owned());
        model.name = "example/Renamed".to_owned();
        Ok(())
    })
    .then(move |model: &mut ClassModel| {
        retarget_events
            .lock()
            .expect("events mutex")
            .push(model.name.clone());
        model.access_flags |= ClassAccessFlags::FINAL;
        Ok(())
    });

    pipeline.apply(&mut model)?;

    assert_eq!(
        *events.lock().expect("events mutex"),
        vec!["rename".to_owned(), "example/Renamed".to_owned()]
    );
    assert!(model.access_flags.contains(ClassAccessFlags::FINAL));
    Ok(())
}

#[test]
fn on_methods_uses_owner_and_method_matchers() -> TestResult<()> {
    let bytes = fixture_bytes("HelloWorld.java", "HelloWorld.class");
    let mut model = ClassModel::from_bytes(&bytes)?;
    let matcher = method_name_matches("^main$")? & method_is_public() & method_is_static();
    let owner = class_named("HelloWorld");
    let mut transform = Pipeline::of(on_methods(
        |method, _owner| {
            method.access_flags |= MethodAccessFlags::FINAL;
            Ok(())
        },
        Some(matcher),
        Some(owner),
    ));

    transform.apply(&mut model)?;

    let main = method_named_mut(&mut model, "main");
    assert!(main.access_flags.contains(MethodAccessFlags::FINAL));
    let init = method_named_mut(&mut model, "<init>");
    assert!(!init.access_flags.contains(MethodAccessFlags::FINAL));
    Ok(())
}

#[test]
fn on_code_mutates_only_matching_method_code() -> TestResult<()> {
    let bytes = fixture_bytes("ControlFlowExample.java", "ControlFlowExample.class");
    let mut model = ClassModel::from_bytes(&bytes)?;
    let original_other_len = method_named_mut(&mut model, "denseSwitch")
        .code
        .as_ref()
        .expect("method should have code")
        .instructions
        .len();

    let mut transform = Pipeline::of(on_code(
        |code, method, owner| {
            assert_eq!(owner.name, "ControlFlowExample");
            assert_eq!(method.name, "branch");
            let insert_at = code
                .instructions
                .iter()
                .position(|item| !matches!(item, CodeItem::Label(_)))
                .expect("code should contain instruction");
            code.instructions.insert(
                insert_at,
                CodeItem::Raw(Instruction::Simple {
                    opcode: 0x00,
                    offset: 0,
                }),
            );
            Ok(())
        },
        Some(method_named("branch")),
        Some(class_named("ControlFlowExample")),
    ));

    transform.apply(&mut model)?;

    let branch = method_named_mut(&mut model, "branch");
    let branch_code = branch.code.as_ref().expect("branch should have code");
    assert!(matches!(
        branch_code
            .instructions
            .iter()
            .find(|item| !matches!(item, CodeItem::Label(_))),
        Some(CodeItem::Raw(Instruction::Simple { opcode: 0x00, .. }))
    ));
    let other_len = method_named_mut(&mut model, "denseSwitch")
        .code
        .as_ref()
        .expect("method should have code")
        .instructions
        .len();
    assert_eq!(other_len, original_other_len);
    Ok(())
}

#[test]
fn pipeline_macro_builds_composed_pipeline() -> TestResult<()> {
    let bytes = fixture_bytes("HelloWorld.java", "HelloWorld.class");
    let mut model = ClassModel::from_bytes(&bytes)?;
    let mut transform = pytecode_engine::pipeline![
        |model: &mut ClassModel| {
            model.access_flags |= ClassAccessFlags::FINAL;
            Ok(())
        },
        on_methods(
            |method, _owner| {
                if method.name == "main" {
                    method.access_flags |= MethodAccessFlags::FINAL;
                }
                Ok(())
            },
            None,
            None,
        )
    ];

    transform.apply(&mut model)?;

    assert!(model.access_flags.contains(ClassAccessFlags::FINAL));
    assert!(
        method_named_mut(&mut model, "main")
            .access_flags
            .contains(MethodAccessFlags::FINAL)
    );
    Ok(())
}
