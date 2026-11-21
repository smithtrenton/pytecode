//! Scratch diagnostic: per-method frame recomputation over a classfile,
//! printing the failing method and its instruction sequence.
//!
//! Not part of the shipped API; used for investigating RECOMPUTE failures on
//! real-world jars. Run with:
//!   cargo run -p pytecode-engine --example diag_frames -- <file.class>

use std::env;
use std::fs;

use pytecode_engine::analysis::recompute_frames;
use pytecode_engine::model::ClassModel;

fn main() {
    let path = env::args()
        .nth(1)
        .expect("usage: diag_frames <class file or directory of .class files>");
    let files = collect_files(&path);

    let mut ok = 0usize;
    let mut failed = 0usize;
    for file in &files {
        let Some(bytes) = fs::read(file).ok() else {
            eprintln!("skip (unreadable): {file}");
            continue;
        };
        let Ok(model) = ClassModel::from_bytes(&bytes) else {
            eprintln!("skip (unparseable): {file}");
            continue;
        };
        for method in &model.methods {
            let Some(code) = &method.code else {
                continue;
            };
            match recompute_frames(
                code,
                &model.name,
                &method.name,
                &method.descriptor,
                method.access_flags,
                None,
            ) {
                Ok(result) => {
                    ok += 1;
                    if verbose() {
                        println!(
                            "ok   {}.{}{} (max_stack={} max_locals={} frames={})",
                            model.name,
                            method.name,
                            method.descriptor,
                            result.max_stack,
                            result.max_locals,
                            result.frames.len()
                        );
                    }
                }
                Err(error) => {
                    failed += 1;
                    println!(
                        "FAIL {}.{}{}: {}",
                        model.name, method.name, method.descriptor, error
                    );
                    println!("  handlers: {:?}", code.exception_handlers);
                    if verbose() {
                        for (index, item) in code.instructions.iter().enumerate() {
                            let rendered = format!("{item:?}");
                            println!("  {index:4}: {}", truncate(&rendered, 140));
                        }
                    }
                    println!();
                }
            }
        }
    }
    println!("methods ok={ok} failed={failed} classes={}", files.len());
}

fn collect_files(path: &str) -> Vec<String> {
    let meta = fs::metadata(path).expect("path does not exist");
    if meta.is_dir() {
        let mut files: Vec<String> = fs::read_dir(path)
            .expect("failed to list directory")
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let name = entry.file_name().into_string().ok()?;
                name.ends_with(".class")
                    .then(|| entry.path().to_string_lossy().into_owned())
            })
            .collect();
        files.sort();
        files
    } else {
        vec![path.to_owned()]
    }
}

fn verbose() -> bool {
    env::args().any(|arg| arg == "--verbose")
}

fn truncate(value: &str, max: usize) -> &str {
    if value.len() <= max {
        value
    } else {
        &value[..max]
    }
}
