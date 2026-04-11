//! Criterion benchmarks for each pipeline stage: ClassParse, ModelLift, ModelLower, ClassWrite.
//!
//! Run with: `cargo bench -p pytecode-engine --bench pipeline`
//!
//! The JarRead stage lives in pytecode-archive (see `jar_read` bench there).

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use pytecode_engine::fixtures::default_benchmark_jar;
use pytecode_engine::model::ClassModel;
use pytecode_engine::raw::{ClassFile, RawClassStub};
use pytecode_engine::{parse_class, write_class};
use std::fs;
use std::io::{self, Read};
use std::path::Path;

/// Read all `.class` entries from a JAR and return (name, bytes) pairs.
fn read_class_entries(jar_path: &Path) -> io::Result<Vec<RawClassStub>> {
    let file = fs::File::open(jar_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut entries = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        if entry.name().ends_with(".class") {
            let mut bytes = Vec::with_capacity(entry.size() as usize);
            entry.read_to_end(&mut bytes)?;
            entries.push(RawClassStub {
                entry_name: entry.name().to_owned(),
                bytes,
            });
        }
    }
    Ok(entries)
}

fn bench_class_parse(c: &mut Criterion) {
    let jar_path = default_benchmark_jar().expect("benchmark jar must exist");
    let entries = read_class_entries(&jar_path).expect("failed to read jar");
    let total_bytes: u64 = entries.iter().map(|e| e.bytes.len() as u64).sum();

    let mut group = c.benchmark_group("class-parse");
    group.throughput(Throughput::Bytes(total_bytes));
    group.bench_function(BenchmarkId::new("all-classes", entries.len()), |b| {
        b.iter(|| {
            for entry in &entries {
                parse_class(&entry.bytes).expect("parse failed");
            }
        });
    });
    group.finish();
}

fn bench_model_lift(c: &mut Criterion) {
    let jar_path = default_benchmark_jar().expect("benchmark jar must exist");
    let entries = read_class_entries(&jar_path).expect("failed to read jar");
    let total_bytes: u64 = entries.iter().map(|e| e.bytes.len() as u64).sum();

    let classfiles: Vec<ClassFile> = entries
        .iter()
        .map(|e| parse_class(&e.bytes).expect("parse failed"))
        .collect();

    let mut group = c.benchmark_group("model-lift");
    group.throughput(Throughput::Bytes(total_bytes));
    group.bench_function(BenchmarkId::new("all-classes", classfiles.len()), |b| {
        b.iter(|| {
            for cf in &classfiles {
                ClassModel::from_classfile(cf).expect("lift failed");
            }
        });
    });
    group.finish();
}

fn bench_model_lower(c: &mut Criterion) {
    let jar_path = default_benchmark_jar().expect("benchmark jar must exist");
    let entries = read_class_entries(&jar_path).expect("failed to read jar");
    let total_bytes: u64 = entries.iter().map(|e| e.bytes.len() as u64).sum();

    let classfiles: Vec<ClassFile> = entries
        .iter()
        .map(|e| parse_class(&e.bytes).expect("parse failed"))
        .collect();

    let models: Vec<ClassModel> = classfiles
        .iter()
        .map(|cf| ClassModel::from_classfile(cf).expect("lift failed"))
        .collect();

    let mut group = c.benchmark_group("model-lower");
    group.throughput(Throughput::Bytes(total_bytes));
    group.bench_function(BenchmarkId::new("all-classes", models.len()), |b| {
        b.iter(|| {
            for model in &models {
                model.to_classfile().expect("lower failed");
            }
        });
    });
    group.finish();
}

fn bench_class_write(c: &mut Criterion) {
    let jar_path = default_benchmark_jar().expect("benchmark jar must exist");
    let entries = read_class_entries(&jar_path).expect("failed to read jar");

    let classfiles: Vec<ClassFile> = entries
        .iter()
        .map(|e| parse_class(&e.bytes).expect("parse failed"))
        .collect();

    let models: Vec<ClassModel> = classfiles
        .iter()
        .map(|cf| ClassModel::from_classfile(cf).expect("lift failed"))
        .collect();

    let lowered: Vec<ClassFile> = models
        .iter()
        .map(|m| m.to_classfile().expect("lower failed"))
        .collect();

    let total_bytes: u64 = lowered
        .iter()
        .map(|cf| write_class(cf).expect("write").len() as u64)
        .sum();

    let mut group = c.benchmark_group("class-write");
    group.throughput(Throughput::Bytes(total_bytes));
    group.bench_function(BenchmarkId::new("all-classes", lowered.len()), |b| {
        b.iter(|| {
            for cf in &lowered {
                write_class(cf).expect("write failed");
            }
        });
    });
    group.finish();
}

fn bench_full_roundtrip(c: &mut Criterion) {
    let jar_path = default_benchmark_jar().expect("benchmark jar must exist");
    let entries = read_class_entries(&jar_path).expect("failed to read jar");
    let total_bytes: u64 = entries.iter().map(|e| e.bytes.len() as u64).sum();

    let mut group = c.benchmark_group("full-roundtrip");
    group.throughput(Throughput::Bytes(total_bytes));
    group.bench_function(
        BenchmarkId::new("parse-lift-lower-write", entries.len()),
        |b| {
            b.iter(|| {
                for entry in &entries {
                    let cf = parse_class(&entry.bytes).expect("parse failed");
                    let model = ClassModel::from_classfile(&cf).expect("lift failed");
                    let lowered = model.to_classfile().expect("lower failed");
                    write_class(&lowered).expect("write failed");
                }
            });
        },
    );
    group.finish();
}

criterion_group!(
    benches,
    bench_class_parse,
    bench_model_lift,
    bench_model_lower,
    bench_class_write,
    bench_full_roundtrip,
);
criterion_main!(benches);
