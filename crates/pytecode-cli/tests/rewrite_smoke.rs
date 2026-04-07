use pytecode_archive::JarFile;
use pytecode_cli::rewrite_smoke;
use pytecode_engine::constants::MethodAccessFlags;
use pytecode_engine::fixtures::compiled_fixture_paths_for;
use pytecode_engine::modified_utf8::decode_modified_utf8;
use pytecode_engine::parse_class;
use pytecode_engine::raw::ConstantPoolEntry;
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

fn main_flags(class_bytes: &[u8]) -> MethodAccessFlags {
    let classfile = parse_class(class_bytes).expect("class should parse");
    classfile
        .methods
        .iter()
        .find(
            |method| match classfile.constant_pool[method.name_index as usize].as_ref() {
                Some(ConstantPoolEntry::Utf8(info)) => {
                    decode_modified_utf8(&info.bytes).expect("utf8 should decode") == "main"
                }
                _ => false,
            },
        )
        .map(|method| method.access_flags)
        .expect("main method should exist")
}

#[test]
fn rewrite_smoke_rewrites_target_jar() -> TestResult<()> {
    let temp_dir = fresh_temp_dir("rewrite-smoke");
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

    let report = rewrite_smoke(&jar_path, &out_path, "HelloWorld")?;
    assert!(report.output_jar.ends_with("output.jar"));

    let rewritten = JarFile::open(&out_path)?;
    let class_entry = rewritten
        .entries
        .iter()
        .find(|entry| entry.filename == "HelloWorld.class")
        .expect("rewritten class should exist");
    assert!(main_flags(&class_entry.bytes).contains(MethodAccessFlags::FINAL));
    assert!(
        rewritten
            .entries
            .iter()
            .any(|entry| entry.filename == "README.txt")
    );
    Ok(())
}
