use pytecode_engine::fixtures::{compatibility_manifest, default_benchmark_jar};

#[test]
fn compatibility_manifest_loads_rust_owned_fixtures() {
    let manifest =
        compatibility_manifest(25).expect("manifest should load from Rust-owned fixtures");

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
    assert_eq!(
        manifest.benchmark_stages,
        vec![
            "jar-read",
            "class-parse",
            "model-lift",
            "model-lower",
            "class-write"
        ]
    );
}

#[test]
fn default_benchmark_jar_prefers_focused_fixture() {
    let path = default_benchmark_jar().expect("focused benchmark jar should exist");

    assert!(path.ends_with("byte-buddy-1.17.5.jar"));
}
