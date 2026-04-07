use pytecode_archive::{JarFile, RewriteOptions};
use pytecode_engine::constants::{MAGIC, MethodAccessFlags};
use pytecode_engine::fixtures::compiled_fixture_paths_for;
use pytecode_engine::model::{ClassModel, CodeItem, FrameComputationMode};
use pytecode_engine::modified_utf8::decode_modified_utf8;
use pytecode_engine::parse_class;
use pytecode_engine::raw::{AttributeInfo, ConstantPoolEntry, Instruction};
use pytecode_engine::transform::{Pipeline, class_named, method_named, on_code, on_methods};
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
    let dir = std::env::temp_dir().join(format!("pytecode-{label}-{nanos}"));
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

fn cp_utf8(classfile: &pytecode_engine::raw::ClassFile, index: u16) -> String {
    match classfile.constant_pool[index as usize]
        .as_ref()
        .expect("cp entry should exist")
    {
        ConstantPoolEntry::Utf8(info) => {
            decode_modified_utf8(&info.bytes).expect("utf8 should decode")
        }
        _ => panic!("cp entry {index} should be Utf8"),
    }
}

fn method_flags(class_bytes: &[u8], method_name: &str) -> MethodAccessFlags {
    let classfile = parse_class(class_bytes).expect("class should parse");
    classfile
        .methods
        .iter()
        .find(|method| cp_utf8(&classfile, method.name_index) == method_name)
        .map(|method| method.access_flags)
        .unwrap_or_else(|| panic!("method {method_name} not found"))
}

fn has_code_attr_named(class_bytes: &[u8], method_name: &str, attr_name: &str) -> bool {
    let classfile = parse_class(class_bytes).expect("class should parse");
    classfile
        .methods
        .iter()
        .find(|method| cp_utf8(&classfile, method.name_index) == method_name)
        .and_then(|method| {
            method
                .attributes
                .iter()
                .find_map(|attribute| match attribute {
                    AttributeInfo::Code(code) => Some(code),
                    _ => None,
                })
        })
        .map(|code| {
            code.attributes
                .iter()
                .any(|attribute| match (attribute, attr_name) {
                    (AttributeInfo::StackMapTable(_), "StackMapTable") => true,
                    (AttributeInfo::LineNumberTable(_), "LineNumberTable") => true,
                    (AttributeInfo::LocalVariableTable(_), "LocalVariableTable") => true,
                    (AttributeInfo::LocalVariableTypeTable(_), "LocalVariableTypeTable") => true,
                    (AttributeInfo::Synthetic(_), "Synthetic") => true,
                    (AttributeInfo::Deprecated(_), "Deprecated") => true,
                    (AttributeInfo::Unknown(unknown), _) if unknown.name == attr_name => true,
                    _ => false,
                })
        })
        .unwrap_or(false)
}

#[test]
fn rewrite_applies_method_transform_and_preserves_resource() -> TestResult<()> {
    let temp_dir = fresh_temp_dir("archive-transform");
    let jar_path = temp_dir.join("input.jar");
    let out_path = temp_dir.join("output.jar");
    let class_bytes = fixture_bytes("HelloWorld.java", "HelloWorld.class");
    make_jar(
        &jar_path,
        &[
            ("HelloWorld.class", &class_bytes),
            ("README.txt", b"fixture"),
        ],
    )?;

    let mut jar = JarFile::open(&jar_path)?;
    let mut transform = Pipeline::of(on_methods(
        |method, _owner| {
            method.access_flags |= MethodAccessFlags::FINAL;
            Ok(())
        },
        Some(method_named("main")),
        Some(class_named("HelloWorld")),
    ));
    jar.rewrite(
        Some(&out_path),
        Some(&mut transform),
        RewriteOptions::default(),
    )?;

    let rewritten = JarFile::open(&out_path)?;
    assert_eq!(
        rewritten
            .entries
            .iter()
            .map(|entry| entry.filename.as_str())
            .collect::<Vec<_>>(),
        vec!["HelloWorld.class", "README.txt"]
    );
    let rewritten_class = rewritten
        .entries
        .iter()
        .find(|entry| entry.filename == "HelloWorld.class")
        .expect("rewritten class should exist");
    assert!(method_flags(&rewritten_class.bytes, "main").contains(MethodAccessFlags::FINAL));
    let readme = rewritten
        .entries
        .iter()
        .find(|entry| entry.filename == "README.txt")
        .expect("resource should exist");
    assert_eq!(readme.bytes, b"fixture");
    Ok(())
}

#[test]
fn rewrite_can_add_and_remove_entries() -> TestResult<()> {
    let temp_dir = fresh_temp_dir("archive-add-remove");
    let jar_path = temp_dir.join("input.jar");
    let out_path = temp_dir.join("output.jar");
    let class_bytes = fixture_bytes("HelloWorld.java", "HelloWorld.class");
    make_jar(
        &jar_path,
        &[
            ("HelloWorld.class", &class_bytes),
            ("README.txt", b"fixture"),
        ],
    )?;

    let mut jar = JarFile::open(&jar_path)?;
    let removed = jar.remove_file("README.txt")?;
    assert_eq!(removed.bytes, b"fixture");
    jar.add_file("extra/info.txt", b"added".to_vec())?;
    jar.rewrite(Some(&out_path), None, RewriteOptions::default())?;

    let rewritten = JarFile::open(&out_path)?;
    assert!(
        rewritten
            .entries
            .iter()
            .any(|entry| entry.filename == "HelloWorld.class")
    );
    let extra_name = PathBuf::from("extra")
        .join("info.txt")
        .to_string_lossy()
        .into_owned();
    assert!(
        rewritten
            .entries
            .iter()
            .any(|entry| entry.filename == extra_name)
    );
    assert!(
        !rewritten
            .entries
            .iter()
            .any(|entry| entry.filename == "README.txt")
    );
    Ok(())
}

#[test]
fn rewrite_requires_recompute_for_code_shape_changes() -> TestResult<()> {
    let temp_dir = fresh_temp_dir("archive-recompute");
    let jar_path = temp_dir.join("input.jar");
    let out_path = temp_dir.join("output.jar");
    let class_bytes = fixture_bytes("ControlFlowExample.java", "ControlFlowExample.class");
    make_jar(&jar_path, &[("ControlFlowExample.class", &class_bytes)])?;

    let mut jar = JarFile::open(&jar_path)?;
    let mut transform = Pipeline::of(on_code(
        |code, _method, _owner| {
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

    let error = jar
        .rewrite(
            Some(&out_path),
            Some(&mut transform),
            RewriteOptions::default(),
        )
        .expect_err("rewrite should fail without recompute");
    assert!(error.to_string().contains("StackMapTable") || error.to_string().contains("Phase 4"));

    let mut jar = JarFile::open(&jar_path)?;
    let mut transform = Pipeline::of(on_code(
        |code, _method, _owner| {
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
    let options = RewriteOptions {
        frame_mode: FrameComputationMode::Recompute,
        ..RewriteOptions::default()
    };
    jar.rewrite(Some(&out_path), Some(&mut transform), options)?;

    let rewritten = JarFile::open(&out_path)?;
    let rewritten_class = rewritten
        .entries
        .iter()
        .find(|entry| entry.filename == "ControlFlowExample.class")
        .expect("rewritten class should exist");
    assert_eq!(parse_class(&rewritten_class.bytes)?.magic, MAGIC);
    assert!(has_code_attr_named(
        &rewritten_class.bytes,
        "branch",
        "StackMapTable"
    ));
    ClassModel::from_bytes(&rewritten_class.bytes)?;
    Ok(())
}
