//! Criterion benchmarks for JAR reading (the JarRead pipeline stage).
//!
//! Run with: `cargo bench -p pytecode-archive --bench jar_read`

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use pytecode_archive::JarFile;
use pytecode_engine::fixtures::default_benchmark_jar;

fn bench_jar_open(c: &mut Criterion) {
    let jar_path = default_benchmark_jar().expect("benchmark jar must exist");

    // Measure total JAR file size for throughput.
    let file_size = std::fs::metadata(&jar_path).expect("jar stat").len();

    let mut group = c.benchmark_group("jar-read");
    group.throughput(Throughput::Bytes(file_size));
    group.bench_function(
        BenchmarkId::new(
            "open-and-classify",
            jar_path.file_name().unwrap().to_str().unwrap(),
        ),
        |b| {
            b.iter(|| {
                let jar = JarFile::open(&jar_path).expect("open failed");
                let (classes, resources) = jar.parse_classes();
                assert!(!classes.is_empty());
                let _ = resources;
            });
        },
    );
    group.finish();
}

criterion_group!(benches, bench_jar_open);
criterion_main!(benches);
