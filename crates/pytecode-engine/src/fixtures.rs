use crate::stages::stage_names;
use serde::{Deserialize, Serialize};
use serde_json::from_slice;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

const INFRASTRUCTURE_FIXTURES: &[&str] = &["VerifierHarness.java"];
const FOCUSED_BENCHMARK_JAR: &str = "byte-buddy-1.17.5.jar";
const FIXTURE_CACHE_SCHEMA_VERSION: u8 = 1;
const FIXTURE_COMPILE_CACHE_DIR: &str = "pytecode-rust-javac";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompatibilityManifest {
    pub max_release: u8,
    pub java_resources: Vec<String>,
    pub benchmark_jars: Vec<String>,
    pub benchmark_stages: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct FixtureCompileManifest {
    schema_version: u8,
    resource: String,
    release: u8,
    javac: String,
    source_hash: String,
    class_files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FixtureManifestInputs {
    resource: String,
    release: u8,
    javac: String,
    source_hash: String,
}

pub fn engine_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

pub fn repo_root() -> &'static Path {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(|| {
        engine_root()
            .parent()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .expect("pytecode-engine crate should live under crates/")
    })
}

pub fn rust_fixtures_dir() -> PathBuf {
    engine_root().join("fixtures")
}

pub fn java_fixtures_dir() -> PathBuf {
    rust_fixtures_dir().join("java")
}

pub fn compiled_fixture_cache_dir() -> PathBuf {
    repo_root().join("target").join(FIXTURE_COMPILE_CACHE_DIR)
}

pub fn benchmark_jars_dir() -> PathBuf {
    rust_fixtures_dir().join("jars")
}

pub fn list_java_resources(max_release: u8) -> io::Result<Vec<String>> {
    let resources_dir = java_fixtures_dir();
    let mut fixtures = Vec::new();

    for entry in WalkDir::new(&resources_dir) {
        let entry = entry.map_err(io::Error::other)?;
        let path = entry.path();
        if !entry.file_type().is_file()
            || path.extension().and_then(|ext| ext.to_str()) != Some("java")
        {
            continue;
        }
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("fixture path contains invalid UTF-8: {path:?}"),
                )
            })?;
        if INFRASTRUCTURE_FIXTURES.contains(&file_name)
            || fixture_min_release(file_name) > max_release
        {
            continue;
        }
        fixtures.push(relative_posix(path, &resources_dir)?);
    }

    fixtures.sort();
    Ok(fixtures)
}

pub fn compiled_fixture_paths(max_release: u8) -> io::Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for resource in list_java_resources(max_release)? {
        paths.extend(compiled_fixture_paths_for(&resource)?);
    }
    paths.sort();
    Ok(paths)
}

pub fn compiled_fixture_paths_for(resource_name: &str) -> io::Result<Vec<PathBuf>> {
    let resource_path = fixture_source_path(resource_name)?;
    let release = fixture_min_release(
        resource_path
            .file_name()
            .and_then(OsStr::to_str)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("fixture resource must have a valid UTF-8 file name: {resource_name}"),
                )
            })?,
    );
    let classes_dir = ensure_compiled_source_cached(
        &compiled_fixture_cache_dir(),
        resource_name,
        &resource_path,
        release,
    )?;
    compiled_class_paths(&classes_dir)
}

pub fn list_benchmark_jars() -> io::Result<Vec<String>> {
    let mut jars = Vec::new();
    for entry in fs::read_dir(benchmark_jars_dir())? {
        let entry = entry?;
        let path = entry.path();
        if !entry.file_type()?.is_file() {
            continue;
        }
        let Some(extension) = path.extension().and_then(|ext| ext.to_str()) else {
            continue;
        };
        if !extension.eq_ignore_ascii_case("jar") {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("jar path contains invalid UTF-8: {path:?}"),
                )
            })?;
        jars.push(name.to_owned());
    }
    jars.sort();
    Ok(jars)
}

pub fn default_benchmark_jar() -> io::Result<PathBuf> {
    let focused = benchmark_jars_dir().join(FOCUSED_BENCHMARK_JAR);
    if focused.is_file() {
        return Ok(focused);
    }
    let first = list_benchmark_jars()?.into_iter().next().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "no checked-in benchmark jar found in Rust fixtures",
        )
    })?;
    Ok(benchmark_jars_dir().join(first))
}

pub fn compatibility_manifest(max_release: u8) -> io::Result<CompatibilityManifest> {
    Ok(CompatibilityManifest {
        max_release,
        java_resources: list_java_resources(max_release)?,
        benchmark_jars: list_benchmark_jars()?,
        benchmark_stages: stage_names(),
    })
}

fn fixture_min_release(file_name: &str) -> u8 {
    match file_name {
        "StaticInterfaceMethods.java" => 9,
        "StringConcat.java" => 9,
        "NestAccess.java" => 11,
        "SwitchExpressions.java" => 14,
        "RecordClass.java" => 16,
        "SealedHierarchy.java" => 17,
        "PatternMatching.java" => 21,
        "Java25Features.java" => 25,
        _ => 8,
    }
}

fn relative_posix(path: &Path, base: &Path) -> io::Result<String> {
    let relative = path.strip_prefix(base).map_err(io::Error::other)?;
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

fn fixture_source_path(resource_name: &str) -> io::Result<PathBuf> {
    let relative = validated_resource_path(resource_name)?;
    let source_path = java_fixtures_dir().join(relative);
    if !source_path.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("fixture source not found: {}", source_path.display()),
        ));
    }
    Ok(source_path)
}

fn ensure_compiled_source_cached(
    cache_root: &Path,
    resource_name: &str,
    source_path: &Path,
    release: u8,
) -> io::Result<PathBuf> {
    let entry_dir = cache_entry_dir(cache_root, resource_name, release)?;
    let expected = manifest_inputs(resource_name, source_path, release)?;
    if cache_entry_matches(&entry_dir, &expected)? {
        return Ok(cache_entry_classes_dir(&entry_dir));
    }
    if entry_dir.exists() {
        remove_path(&entry_dir)?;
    }

    fs::create_dir_all(cache_root)?;
    let staging_root = cache_root.join("staging");
    fs::create_dir_all(&staging_root)?;
    let staging_dir = create_staging_dir(&staging_root, resource_name)?;

    let build_result = (|| -> io::Result<PathBuf> {
        let classes_dir = cache_entry_classes_dir(&staging_dir);
        compile_java_source(&classes_dir, source_path, release)?;
        write_cache_manifest(&staging_dir, &expected)?;
        publish_cache_entry(&staging_dir, &entry_dir, &expected)?;
        Ok(cache_entry_classes_dir(&entry_dir))
    })();

    if build_result.is_err() && staging_dir.exists() {
        let _ = remove_path(&staging_dir);
    }
    build_result
}

fn validated_resource_path(resource_name: &str) -> io::Result<&Path> {
    let relative = Path::new(resource_name);
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                Component::Prefix(_) | Component::RootDir | Component::ParentDir
            )
        })
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "fixture resource must be a relative path under fixtures\\java: {resource_name}"
            ),
        ));
    }
    Ok(relative)
}

fn cache_entry_dir(cache_root: &Path, resource_name: &str, release: u8) -> io::Result<PathBuf> {
    let mut relative = validated_resource_path(resource_name)?.to_path_buf();
    relative.set_extension("");
    Ok(cache_root.join(format!("release-{release}")).join(relative))
}

fn cache_entry_manifest_path(entry_dir: &Path) -> PathBuf {
    entry_dir.join("manifest.json")
}

fn cache_entry_classes_dir(entry_dir: &Path) -> PathBuf {
    entry_dir.join("classes")
}

fn manifest_inputs(
    resource_name: &str,
    source_path: &Path,
    release: u8,
) -> io::Result<FixtureManifestInputs> {
    Ok(FixtureManifestInputs {
        resource: resource_name.replace('\\', "/"),
        release,
        javac: javac_identity()?,
        source_hash: source_hash(source_path)?,
    })
}

fn cache_entry_matches(entry_dir: &Path, expected: &FixtureManifestInputs) -> io::Result<bool> {
    let Some(manifest) = read_cache_manifest(entry_dir)? else {
        return Ok(false);
    };
    if manifest.schema_version != FIXTURE_CACHE_SCHEMA_VERSION
        || manifest.resource != expected.resource
        || manifest.release != expected.release
        || manifest.javac != expected.javac
        || manifest.source_hash != expected.source_hash
    {
        return Ok(false);
    }
    let classes_dir = cache_entry_classes_dir(entry_dir);
    if !classes_dir.is_dir() || manifest.class_files.is_empty() {
        return Ok(false);
    }
    Ok(manifest
        .class_files
        .iter()
        .all(|class_file| classes_dir.join(class_file).is_file()))
}

fn read_cache_manifest(entry_dir: &Path) -> io::Result<Option<FixtureCompileManifest>> {
    let manifest_path = cache_entry_manifest_path(entry_dir);
    if !manifest_path.is_file() {
        return Ok(None);
    }
    let bytes = fs::read(manifest_path)?;
    match from_slice(&bytes) {
        Ok(manifest) => Ok(Some(manifest)),
        Err(_) => Ok(None),
    }
}

fn write_cache_manifest(entry_dir: &Path, expected: &FixtureManifestInputs) -> io::Result<()> {
    let classes_dir = cache_entry_classes_dir(entry_dir);
    let class_files = compiled_class_relative_paths(&classes_dir)?;
    let manifest = FixtureCompileManifest {
        schema_version: FIXTURE_CACHE_SCHEMA_VERSION,
        resource: expected.resource.clone(),
        release: expected.release,
        javac: expected.javac.clone(),
        source_hash: expected.source_hash.clone(),
        class_files,
    };
    let manifest_path = cache_entry_manifest_path(entry_dir);
    let manifest_text = serde_json::to_vec_pretty(&manifest).map_err(io::Error::other)?;
    fs::write(manifest_path, manifest_text)?;
    Ok(())
}

fn publish_cache_entry(
    staging_dir: &Path,
    entry_dir: &Path,
    expected: &FixtureManifestInputs,
) -> io::Result<()> {
    if let Some(parent) = entry_dir.parent() {
        fs::create_dir_all(parent)?;
    }
    match fs::rename(staging_dir, entry_dir) {
        Ok(()) => Ok(()),
        Err(rename_err) => {
            if cache_entry_matches(entry_dir, expected)? {
                remove_path(staging_dir)?;
                return Ok(());
            }
            if entry_dir.exists() {
                remove_path(entry_dir)?;
            }
            fs::rename(staging_dir, entry_dir).map_err(|retry_err| {
                io::Error::other(format!(
                    "failed to publish compiled fixture cache entry {}: {rename_err}; retry failed: {retry_err}",
                    entry_dir.display()
                ))
            })
        }
    }
}

fn create_staging_dir(staging_root: &Path, resource_name: &str) -> io::Result<PathBuf> {
    let stem = Path::new(resource_name)
        .file_stem()
        .and_then(OsStr::to_str)
        .unwrap_or("fixture");
    for attempt in 0..1000_u32 {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_nanos();
        let candidate = staging_root.join(format!("{stem}-{timestamp}-{attempt}"));
        match fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        }
    }
    Err(io::Error::other(format!(
        "failed to allocate staging dir under {}",
        staging_root.display()
    )))
}

fn compiled_class_paths(classes_dir: &Path) -> io::Result<Vec<PathBuf>> {
    let relative_paths = compiled_class_relative_paths(classes_dir)?;
    Ok(relative_paths
        .into_iter()
        .map(|relative| classes_dir.join(relative))
        .collect())
}

fn compiled_class_relative_paths(classes_dir: &Path) -> io::Result<Vec<String>> {
    let mut class_files = Vec::new();
    for entry in WalkDir::new(classes_dir) {
        let entry = entry.map_err(io::Error::other)?;
        let path = entry.path();
        if entry.file_type().is_file() && path.extension().and_then(OsStr::to_str) == Some("class")
        {
            class_files.push(relative_posix(path, classes_dir)?);
        }
    }
    class_files.sort();
    if class_files.is_empty() {
        return Err(io::Error::other(format!(
            "compiled fixture cache contains no class files: {}",
            classes_dir.display()
        )));
    }
    Ok(class_files)
}

fn compile_java_source(classes_dir: &Path, source_path: &Path, release: u8) -> io::Result<()> {
    fs::create_dir_all(classes_dir)?;
    let output = Command::new(jdk_tool("javac"))
        .arg("--release")
        .arg(release.to_string())
        .arg("-d")
        .arg(classes_dir)
        .arg(source_path)
        .output()?;
    if output.status.success() {
        return Ok(());
    }
    Err(io::Error::other(format!(
        "javac failed for {} (--release {}): {}",
        source_path.display(),
        release,
        command_output_text(&output)
    )))
}

fn javac_identity() -> io::Result<String> {
    static JAVAC_IDENTITY: OnceLock<std::result::Result<String, String>> = OnceLock::new();
    match JAVAC_IDENTITY
        .get_or_init(|| get_jdk_tool_identity("javac").map_err(|err| err.to_string()))
    {
        Ok(identity) => Ok(identity.clone()),
        Err(message) => Err(io::Error::other(message.clone())),
    }
}

fn get_jdk_tool_identity(name: &str) -> io::Result<String> {
    let output = Command::new(jdk_tool(name)).arg("-version").output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "{name} -version failed: {}",
            command_output_text(&output)
        )));
    }
    let text = command_output_text(&output);
    if text.is_empty() {
        return Err(io::Error::other(format!(
            "{name} -version produced no output"
        )));
    }
    Ok(text)
}

fn jdk_tool(name: &str) -> PathBuf {
    if let Some(java_home) = env::var_os("JAVA_HOME") {
        let bin_dir = PathBuf::from(java_home).join("bin");
        for suffix in ["", ".exe"] {
            let candidate = bin_dir.join(format!("{name}{suffix}"));
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    PathBuf::from(name)
}

fn command_output_text(output: &std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    match (stdout.is_empty(), stderr.is_empty()) {
        (false, false) => format!("{stdout}\n{stderr}"),
        (false, true) => stdout,
        (true, false) => stderr,
        (true, true) => format!("process exited with {}", output.status),
    }
}

fn source_hash(source_path: &Path) -> io::Result<String> {
    let bytes = fs::read(source_path)?;
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    Ok(format!("{hash:016x}"))
}

fn remove_path(path: &Path) -> io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

#[cfg(test)]
mod tests {
    use super::{cache_entry_classes_dir, cache_entry_dir, ensure_compiled_source_cached};
    use std::fs;
    use std::io;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(name: &str) -> io::Result<Self> {
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(io::Error::other)?
                .as_nanos();
            let path = std::env::temp_dir().join(format!("pytecode-{name}-{timestamp}"));
            fs::create_dir_all(&path)?;
            Ok(Self { path })
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn fixture_cache_recompiles_when_source_changes() -> io::Result<()> {
        let temp = TestDir::new("fixture-cache")?;
        let cache_root = temp.path().join("cache");
        let source_path = temp.path().join("CacheFixture.java");
        fs::write(
            &source_path,
            "public class CacheFixture { public static int value() { return 1; } }\n",
        )?;

        let classes_dir =
            ensure_compiled_source_cached(&cache_root, "CacheFixture.java", &source_path, 8)?;
        let class_path = classes_dir.join("CacheFixture.class");
        let first_bytes = fs::read(&class_path)?;

        fs::write(
            &source_path,
            "public class CacheFixture { public static int value() { return 2; } }\n",
        )?;
        let rebuilt_dir =
            ensure_compiled_source_cached(&cache_root, "CacheFixture.java", &source_path, 8)?;
        let rebuilt_bytes = fs::read(rebuilt_dir.join("CacheFixture.class"))?;

        assert_eq!(classes_dir, rebuilt_dir);
        assert_ne!(first_bytes, rebuilt_bytes);
        assert!(
            cache_entry_classes_dir(&cache_entry_dir(&cache_root, "CacheFixture.java", 8,)?)
                .is_dir()
        );
        Ok(())
    }
}
