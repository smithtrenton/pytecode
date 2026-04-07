use pytecode_cli::{export_class_summary, run_smoke_benchmark};
use pytecode_engine::fixtures::{
    compatibility_manifest, compiled_fixture_paths_for, default_benchmark_jar,
};

type TestResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[test]
fn compat_manifest_lists_rust_owned_fixtures() -> TestResult<()> {
    let manifest = compatibility_manifest(25)?;
    assert!(
        manifest
            .java_resources
            .iter()
            .any(|path| path == "HelloWorld.java")
    );
    assert!(
        manifest
            .benchmark_jars
            .iter()
            .any(|path| path == "byte-buddy-1.17.5.jar")
    );
    Ok(())
}

#[test]
fn bench_smoke_runs_against_focused_jar() -> TestResult<()> {
    let report = run_smoke_benchmark(Some(default_benchmark_jar()?), 1)?;
    assert_eq!(report.iterations, 1);
    assert_eq!(report.stage_reports.len(), 5);
    assert!(report.stage_reports.iter().any(|stage| {
        stage.stage.to_string() == "class-parse"
            && stage.units > 0
            && stage.samples_milliseconds.len() == 1
            && stage.median_milliseconds == stage.samples_milliseconds[0]
            && stage.spread_milliseconds == 0
    }));
    Ok(())
}

#[test]
fn class_summary_reads_hello_world_fixture() -> TestResult<()> {
    let class_path = compiled_fixture_paths_for("HelloWorld.java")?
        .into_iter()
        .find(|path| path.file_name().and_then(|name| name.to_str()) == Some("HelloWorld.class"))
        .ok_or("HelloWorld.class not found")?;

    let summary = export_class_summary(&class_path)?;
    assert_eq!(summary.major, 52);
    assert!(
        summary
            .class_attr_names
            .iter()
            .any(|name| name == "SourceFile")
    );
    assert!(summary.methods.iter().any(|method| {
        method.name == "main"
            && method.descriptor == "([Ljava/lang/String;)V"
            && method.opcodes.as_ref() == Some(&vec![0xB2, 0x12, 0xB6, 0xB1])
    }));
    Ok(())
}
