use pytecode_cli::{analyze_deobfuscation, rewrite_deobfuscation};
use pytecode_engine::fixtures::compiled_fixture_paths_for;
use pytecode_engine::model::{BranchInsn, ClassModel, CodeItem, DebugInfoPolicy, Label};
use pytecode_engine::raw::Instruction;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

type TestResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn fixture_bytes(resource_name: &str, class_name: &str) -> Vec<u8> {
    let path = compiled_fixture_paths_for(resource_name)
        .expect("fixture paths should load")
        .into_iter()
        .find(|path| path.file_name().and_then(|name| name.to_str()) == Some(class_name))
        .unwrap_or_else(|| panic!("fixture {class_name} not found for {resource_name}"));
    fs::read(path).expect("fixture bytes should read")
}

fn fresh_temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("pytecode-cli-{label}-{nanos}"));
    fs::create_dir_all(&dir).expect("temp dir should create");
    dir
}

fn make_jar(path: &Path, entries: &[(&str, &[u8])]) -> TestResult<()> {
    let file = File::create(path)?;
    let mut writer = ZipWriter::new(file);
    for (name, bytes) in entries {
        writer.start_file(*name, SimpleFileOptions::default())?;
        writer.write_all(bytes)?;
    }
    writer.finish()?;
    Ok(())
}

fn instrumented_hello_world_bytes() -> TestResult<Vec<u8>> {
    let mut model = ClassModel::from_bytes(&fixture_bytes("HelloWorld.java", "HelloWorld.class"))?;
    let main = model
        .methods
        .iter_mut()
        .find(|method| method.name == "main")
        .expect("main method should exist");
    let code = main.code.as_mut().expect("main should have code");
    code.instructions.insert(
        0,
        CodeItem::Raw(Instruction::Simple {
            opcode: 0,
            offset: 0,
        }),
    );
    let skip = Label::named("skip");
    let return_index = code
        .instructions
        .iter()
        .position(|item| matches!(item, CodeItem::Raw(Instruction::Simple { opcode: 177, .. })))
        .expect("return should exist");
    code.instructions.insert(
        return_index,
        CodeItem::Branch(BranchInsn {
            opcode: 167,
            target: skip.clone(),
        }),
    );
    code.instructions
        .insert(return_index + 1, CodeItem::Label(skip));
    code.instructions.insert(
        return_index + 2,
        CodeItem::Raw(Instruction::Simple {
            opcode: 0,
            offset: 0,
        }),
    );
    Ok(model.to_bytes_with_recomputed_frames(DebugInfoPolicy::Preserve, None)?)
}

fn renamed_hello_world_bytes(class_name: &str) -> TestResult<Vec<u8>> {
    let mut model = ClassModel::from_bytes(&fixture_bytes("HelloWorld.java", "HelloWorld.class"))?;
    model.name = class_name.to_owned();
    Ok(model.to_bytes_with_recomputed_frames(DebugInfoPolicy::Preserve, None)?)
}

fn make_analysis_fixture_jar(path: &Path) -> TestResult<()> {
    let aa = renamed_hello_world_bytes("aa")?;
    let bb = renamed_hello_world_bytes("bb")?;
    let rl4 = renamed_hello_world_bytes("rl4")?;
    let normal = renamed_hello_world_bytes("pkg/Normal")?;
    let compiler_control = br#"
[
  {"match":["ed::ad([B)V"],"c2":{"exclude":true}},
  {"match":["keep::me()V"],"c2":{"exclude":false}}
]
"#;
    make_jar(
        path,
        &[
            ("aa.class", aa.as_slice()),
            ("bb.class", bb.as_slice()),
            ("rl4.class", rl4.as_slice()),
            ("pkg/Normal.class", normal.as_slice()),
            ("compilercontrol.json", compiler_control.as_ref()),
        ],
    )
}

#[test]
fn analyze_deobfuscation_reports_synthetic_patterns() -> TestResult<()> {
    let temp_dir = fresh_temp_dir("deobfuscate-analyze");
    let jar = temp_dir.join("input.jar");
    make_analysis_fixture_jar(&jar)?;
    let report = analyze_deobfuscation(&jar)?;
    assert_eq!(report.class_entries, 4);
    assert_eq!(report.resource_entries, 1);
    assert_eq!(report.suspicious_class_count, 2);
    assert_eq!(report.rl_class_count, 1);
    assert!(
        report
            .sample_suspicious_classes
            .iter()
            .any(|name| name == "aa")
    );
    assert!(
        report
            .sample_suspicious_classes
            .iter()
            .any(|name| name == "bb")
    );
    assert!(report.sample_rl_classes.iter().any(|name| name == "rl4"));
    assert!(
        report
            .compiler_control_excludes
            .iter()
            .any(|entry| entry == "ed::ad([B)V")
    );
    let top_package = report
        .top_packages
        .first()
        .expect("top package summary should exist");
    assert_eq!(top_package.package, "<root>");
    assert_eq!(top_package.class_count, 3);
    assert_eq!(top_package.suspicious_class_count, 2);
    Ok(())
}

#[test]
fn rewrite_deobfuscation_removes_nops_and_trivial_gotos() -> TestResult<()> {
    let temp_dir = fresh_temp_dir("deobfuscate-rewrite");
    let jar_path = temp_dir.join("input.jar");
    let out_path = temp_dir.join("output.jar");
    let class_bytes = instrumented_hello_world_bytes()?;
    make_jar(&jar_path, &[("HelloWorld.class", &class_bytes)])?;

    let report = rewrite_deobfuscation(&jar_path, &out_path)?;
    assert_eq!(report.classes_changed, 1);
    assert_eq!(report.nops_removed, 2);
    assert_eq!(report.noop_gotos_removed, 1);

    let rewritten = pytecode_archive::JarFile::open(&out_path)?;
    let class_entry = rewritten
        .entries
        .iter()
        .find(|entry| entry.filename == "HelloWorld.class")
        .expect("rewritten class should exist");
    let model = ClassModel::from_bytes(&class_entry.bytes)?;
    let main = model
        .methods
        .iter()
        .find(|method| method.name == "main")
        .expect("main method should exist");
    let code = main.code.as_ref().expect("main should have code");
    assert!(
        !code
            .instructions
            .iter()
            .any(|item| { matches!(item, CodeItem::Raw(Instruction::Simple { opcode: 0, .. })) })
    );
    assert!(
        !code
            .instructions
            .iter()
            .any(|item| { matches!(item, CodeItem::Branch(branch) if branch.opcode == 167) })
    );
    Ok(())
}
