use pytecode_archive::JarFile;
use pytecode_cli::patch_jar;
use pytecode_engine::constants::MethodAccessFlags;
use pytecode_engine::fixtures::compiled_fixture_paths_for;
use pytecode_engine::model::{ClassModel, CodeItem, LdcValue};
use pytecode_engine::modified_utf8::decode_modified_utf8;
use pytecode_engine::parse_class;
use pytecode_engine::raw::ConstantPoolEntry;
use pytecode_engine::raw::Instruction;
use serde_json::json;
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
            |method| match classfile.constant_pool[method.name_index.value() as usize].as_ref() {
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
fn patch_jar_applies_method_rule_from_json() -> TestResult<()> {
    let temp_dir = fresh_temp_dir("patch-jar");
    let jar_path = temp_dir.join("input.jar");
    let out_path = temp_dir.join("output.jar");
    let rules_path = temp_dir.join("rules.json");
    let class_bytes = fixture_bytes("HelloWorld.java", "HelloWorld.class");
    make_jar(
        &jar_path,
        &[
            ("HelloWorld.class", &class_bytes),
            ("README.txt", b"fixture"),
        ],
    )?;
    fs::write(
        &rules_path,
        serde_json::to_vec_pretty(&json!({
            "rules": [
                {
                    "name": "finalize-main",
                    "kind": "method",
                    "owner": {
                        "name": "HelloWorld"
                    },
                    "matcher": {
                        "name": "main",
                        "access_all": ["public", "static"],
                        "has_code": true
                    },
                    "action": {
                        "type": "add-access-flags",
                        "flags": ["final"]
                    }
                }
            ]
        }))?,
    )?;

    let report = patch_jar(&jar_path, &out_path, &rules_path)?;
    assert!(report.output_jar.ends_with("output.jar"));
    assert_eq!(report.class_entries, 1);
    assert_eq!(report.resource_entries, 1);
    assert_eq!(report.rules.len(), 1);
    assert_eq!(report.rules[0].name, "finalize-main");
    assert_eq!(report.rules[0].kind, "method");
    assert_eq!(report.rules[0].matched_classes, 1);
    assert_eq!(report.rules[0].changed_classes, 1);
    assert_eq!(report.rules[0].matched_targets, 1);
    assert_eq!(report.rules[0].changed_targets, 1);

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

#[test]
fn patch_jar_rejects_invalid_regex_rules() -> TestResult<()> {
    let temp_dir = fresh_temp_dir("patch-jar-invalid");
    let jar_path = temp_dir.join("input.jar");
    let out_path = temp_dir.join("output.jar");
    let rules_path = temp_dir.join("rules.json");
    let class_bytes = fixture_bytes("HelloWorld.java", "HelloWorld.class");
    make_jar(&jar_path, &[("HelloWorld.class", &class_bytes)])?;
    fs::write(
        &rules_path,
        serde_json::to_vec_pretty(&json!({
            "rules": [
                {
                    "kind": "method",
                    "matcher": {
                        "name_matches": "("
                    },
                    "action": {
                        "type": "remove"
                    }
                }
            ]
        }))?,
    )?;

    let error = patch_jar(&jar_path, &out_path, &rules_path).expect_err("invalid regex must fail");
    assert!(error.to_string().contains("regex parse error"));
    Ok(())
}

#[test]
fn patch_jar_applies_code_actions_from_json() -> TestResult<()> {
    let temp_dir = fresh_temp_dir("patch-jar-code");
    let jar_path = temp_dir.join("input.jar");
    let out_path = temp_dir.join("output.jar");
    let rules_path = temp_dir.join("rules.json");
    let class_bytes = fixture_bytes("HelloWorld.java", "HelloWorld.class");
    make_jar(&jar_path, &[("HelloWorld.class", &class_bytes)])?;
    fs::write(
        &rules_path,
        serde_json::to_vec_pretty(&json!({
            "rules": [
                {
                    "name": "rewrite-main-code",
                    "kind": "method",
                    "owner": {
                        "name": "HelloWorld"
                    },
                    "matcher": {
                        "name": "main",
                        "has_code": true
                    },
                    "code_actions": [
                        {
                            "type": "replace-string",
                            "from": "Hello from fixture",
                            "to": "patched via code action"
                        },
                        {
                            "type": "redirect-method-call",
                            "from_owner": "java/io/PrintStream",
                            "from_name": "println",
                            "from_descriptor": "(Ljava/lang/String;)V",
                            "to_owner": "java/io/PrintStream",
                            "to_name": "print"
                        }
                    ]
                }
            ]
        }))?,
    )?;

    let report = patch_jar(&jar_path, &out_path, &rules_path)?;
    assert_eq!(report.rules.len(), 1);
    assert_eq!(report.rules[0].name, "rewrite-main-code");
    assert_eq!(report.rules[0].kind, "method");
    assert_eq!(report.rules[0].matched_classes, 1);
    assert_eq!(report.rules[0].changed_classes, 1);
    assert_eq!(report.rules[0].matched_targets, 2);
    assert_eq!(report.rules[0].changed_targets, 2);

    let rewritten = JarFile::open(&out_path)?;
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
    assert!(code.instructions.iter().any(|item| {
        matches!(
            item,
            CodeItem::Ldc(insn)
                if matches!(&insn.value, LdcValue::String(value) if value == "patched via code action")
        )
    }));
    assert!(code.instructions.iter().any(|item| {
        matches!(
            item,
            CodeItem::Method(insn)
                if insn.owner == "java/io/PrintStream"
                    && insn.name == "print"
                    && insn.descriptor == "(Ljava/lang/String;)V"
        )
    }));
    Ok(())
}

#[test]
fn patch_jar_applies_sequence_code_actions_from_json() -> TestResult<()> {
    let temp_dir = fresh_temp_dir("patch-jar-sequence");
    let jar_path = temp_dir.join("input.jar");
    let out_path = temp_dir.join("output.jar");
    let rules_path = temp_dir.join("rules.json");
    let class_bytes = fixture_bytes("HelloWorld.java", "HelloWorld.class");
    make_jar(&jar_path, &[("HelloWorld.class", &class_bytes)])?;
    fs::write(
        &rules_path,
        serde_json::to_vec_pretty(&json!({
            "rules": [
                {
                    "name": "replace-main-sequence",
                    "kind": "method",
                    "owner": {
                        "name": "HelloWorld"
                    },
                    "matcher": {
                        "name": "main",
                        "has_code": true
                    },
                    "code_actions": [
                        {
                            "type": "replace-sequence",
                            "pattern": [
                                {
                                    "ldc_string": "Hello from fixture"
                                },
                                {
                                    "method_owner": "java/io/PrintStream",
                                    "method_name": "println",
                                    "method_descriptor": "(Ljava/lang/String;)V"
                                }
                            ],
                            "replacement": [
                                {
                                    "type": "ldc-string",
                                    "value": "patched via sequence action"
                                },
                                {
                                    "type": "method",
                                    "opcode": 182,
                                    "owner": "java/io/PrintStream",
                                    "name": "print",
                                    "descriptor": "(Ljava/lang/String;)V"
                                }
                            ]
                        }
                    ]
                }
            ]
        }))?,
    )?;

    let report = patch_jar(&jar_path, &out_path, &rules_path)?;
    assert_eq!(report.rules.len(), 1);
    assert_eq!(report.rules[0].name, "replace-main-sequence");
    assert_eq!(report.rules[0].kind, "method");
    assert_eq!(report.rules[0].matched_classes, 1);
    assert_eq!(report.rules[0].changed_classes, 1);
    assert_eq!(report.rules[0].matched_targets, 1);
    assert_eq!(report.rules[0].changed_targets, 1);

    let rewritten = JarFile::open(&out_path)?;
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
    assert!(!code.instructions.iter().any(|item| {
        matches!(
            item,
            CodeItem::Ldc(insn)
                if matches!(&insn.value, LdcValue::String(value) if value == "Hello from fixture")
        )
    }));
    assert!(code.instructions.iter().any(|item| {
        matches!(
            item,
            CodeItem::Ldc(insn)
                if matches!(&insn.value, LdcValue::String(value) if value == "patched via sequence action")
        )
    }));
    assert!(!code.instructions.iter().any(|item| {
        matches!(
            item,
            CodeItem::Method(insn)
                if insn.owner == "java/io/PrintStream"
                    && insn.name == "println"
                    && insn.descriptor == "(Ljava/lang/String;)V"
        )
    }));
    assert!(code.instructions.iter().any(|item| {
        matches!(
            item,
            CodeItem::Method(insn)
                if insn.owner == "java/io/PrintStream"
                    && insn.name == "print"
                    && insn.descriptor == "(Ljava/lang/String;)V"
        )
    }));
    Ok(())
}

#[test]
fn patch_jar_applies_replace_insert_code_actions_from_json() -> TestResult<()> {
    let temp_dir = fresh_temp_dir("patch-jar-insert-replace");
    let jar_path = temp_dir.join("input.jar");
    let out_path = temp_dir.join("output.jar");
    let rules_path = temp_dir.join("rules.json");
    let class_bytes = fixture_bytes("HelloWorld.java", "HelloWorld.class");
    make_jar(&jar_path, &[("HelloWorld.class", &class_bytes)])?;
    fs::write(
        &rules_path,
        serde_json::to_vec_pretty(&json!({
            "rules": [
                {
                    "name": "replace-and-bracket-println",
                    "kind": "method",
                    "owner": {
                        "name": "HelloWorld"
                    },
                    "matcher": {
                        "name": "main",
                        "has_code": true
                    },
                    "code_actions": [
                        {
                            "type": "replace-insn",
                            "matcher": {
                                "ldc_string": "Hello from fixture"
                            },
                            "replacement": [
                                {
                                    "type": "ldc-string",
                                    "value": "patched via replace-insn"
                                }
                            ]
                        },
                        {
                            "type": "sequence",
                            "actions": [
                                {
                                    "type": "insert-before",
                                    "matcher": {
                                        "method_owner": "java/io/PrintStream",
                                        "method_name": "println",
                                        "method_descriptor": "(Ljava/lang/String;)V"
                                    },
                                    "items": [
                                        {
                                            "type": "raw",
                                            "opcode": 0
                                        }
                                    ]
                                },
                                {
                                    "type": "insert-after",
                                    "matcher": {
                                        "method_owner": "java/io/PrintStream",
                                        "method_name": "println",
                                        "method_descriptor": "(Ljava/lang/String;)V"
                                    },
                                    "items": [
                                        {
                                            "type": "raw",
                                            "opcode": 0
                                        }
                                    ]
                                }
                            ]
                        }
                    ]
                }
            ]
        }))?,
    )?;

    let report = patch_jar(&jar_path, &out_path, &rules_path)?;
    assert_eq!(report.rules.len(), 1);
    assert_eq!(report.rules[0].matched_targets, 3);
    assert_eq!(report.rules[0].changed_targets, 3);

    let rewritten = JarFile::open(&out_path)?;
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

    assert!(code.instructions.iter().any(|item| {
        matches!(
            item,
            CodeItem::Ldc(insn)
                if matches!(&insn.value, LdcValue::String(value) if value == "patched via replace-insn")
        )
    }));
    assert_eq!(
        code.instructions
            .iter()
            .filter(|item| { matches!(item, CodeItem::Raw(Instruction::Simple { opcode: 0, .. })) })
            .count(),
        2
    );
    Ok(())
}

#[test]
fn patch_jar_supports_control_flow_replacements_from_json() -> TestResult<()> {
    let temp_dir = fresh_temp_dir("patch-jar-control-flow");
    let jar_path = temp_dir.join("input.jar");
    let out_path = temp_dir.join("output.jar");
    let rules_path = temp_dir.join("rules.json");
    let class_bytes = fixture_bytes("HelloWorld.java", "HelloWorld.class");
    make_jar(&jar_path, &[("HelloWorld.class", &class_bytes)])?;
    fs::write(
        &rules_path,
        serde_json::to_vec_pretty(&json!({
            "rules": [
                {
                    "name": "wrap-return-with-goto",
                    "kind": "method",
                    "owner": {
                        "name": "HelloWorld"
                    },
                    "matcher": {
                        "name": "main",
                        "has_code": true
                    },
                    "code_actions": [
                        {
                            "type": "replace-insn",
                            "matcher": {
                                "opcode": 177
                            },
                            "replacement": [
                                {
                                    "type": "branch",
                                    "opcode": 167,
                                    "target": "done"
                                },
                                {
                                    "type": "raw",
                                    "opcode": 0
                                },
                                {
                                    "type": "label",
                                    "name": "done"
                                },
                                {
                                    "type": "raw",
                                    "opcode": 177
                                }
                            ]
                        }
                    ]
                }
            ]
        }))?,
    )?;

    let report = patch_jar(&jar_path, &out_path, &rules_path)?;
    assert_eq!(report.rules.len(), 1);
    assert_eq!(report.rules[0].matched_targets, 1);
    assert_eq!(report.rules[0].changed_targets, 1);

    let rewritten = JarFile::open(&out_path)?;
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

    let branch_target = code.instructions.iter().find_map(|item| match item {
        CodeItem::Branch(insn) if insn.opcode == 167 => Some(insn.target.clone()),
        _ => None,
    });
    let branch_target = branch_target.expect("goto target should exist");
    assert!(
        code.instructions
            .iter()
            .any(|item| { matches!(item, CodeItem::Label(label) if *label == branch_target) })
    );
    Ok(())
}

#[test]
fn patch_jar_rejects_missing_replacement_labels() -> TestResult<()> {
    let temp_dir = fresh_temp_dir("patch-jar-missing-label");
    let jar_path = temp_dir.join("input.jar");
    let out_path = temp_dir.join("output.jar");
    let rules_path = temp_dir.join("rules.json");
    let class_bytes = fixture_bytes("HelloWorld.java", "HelloWorld.class");
    make_jar(&jar_path, &[("HelloWorld.class", &class_bytes)])?;
    fs::write(
        &rules_path,
        serde_json::to_vec_pretty(&json!({
            "rules": [
                {
                    "kind": "method",
                    "owner": {
                        "name": "HelloWorld"
                    },
                    "matcher": {
                        "name": "main",
                        "has_code": true
                    },
                    "code_actions": [
                        {
                            "type": "replace-insn",
                            "matcher": {
                                "opcode": 177
                            },
                            "replacement": [
                                {
                                    "type": "branch",
                                    "opcode": 167,
                                    "target": "missing"
                                },
                                {
                                    "type": "raw",
                                    "opcode": 177
                                }
                            ]
                        }
                    ]
                }
            ]
        }))?,
    )?;

    let error = patch_jar(&jar_path, &out_path, &rules_path).expect_err("missing label must fail");
    assert!(
        error
            .to_string()
            .contains("replacement branch target 'missing' was not declared")
    );
    Ok(())
}
