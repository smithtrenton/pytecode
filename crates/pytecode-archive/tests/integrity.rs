use pytecode_archive::{ArchiveReadLimits, JarFile, RewriteOptions};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

fn make_archive(path: &Path) {
    let mut writer = ZipWriter::new(File::create(path).unwrap());
    writer
        .set_raw_comment(b"comment\xff".to_vec().into_boxed_slice())
        .unwrap();
    writer
        .start_file("entry", SimpleFileOptions::default())
        .unwrap();
    writer.write_all(b"original").unwrap();
    writer.finish().unwrap();
}

#[test]
fn public_mutations_are_not_hidden_by_original_index() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("input.jar");
    make_archive(&path);
    let mut jar = JarFile::open(&path).unwrap();
    jar.entries[0].bytes = b"modified".to_vec();
    jar.entries[0].filename = "renamed".to_owned();
    jar.entries[0].metadata.comment = b"entry comment".to_vec();
    jar.rewrite(None, None, RewriteOptions::default()).unwrap();
    let mut written = ZipArchive::new(File::open(&path).unwrap()).unwrap();
    assert_eq!(written.comment(), b"comment\xff");
    assert_eq!(
        written.by_name("renamed").unwrap().comment(),
        "entry comment"
    );
    assert_eq!(jar.entries[0].bytes, b"modified");
}

#[test]
fn dropping_prepared_archive_preserves_disk_and_cleans_temporary_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("input.jar");
    make_archive(&path);
    let before = fs::read(&path).unwrap();
    let mut jar = JarFile::open(&path).unwrap();
    jar.entries[0].bytes = b"modified".to_vec();
    let prepared = jar
        .prepare_rewrite(None, None, RewriteOptions::default())
        .unwrap();
    let staged = prepared.path().to_path_buf();
    assert!(staged.exists());
    assert_eq!(fs::read(&path).unwrap(), before);
    drop(prepared);
    assert!(!staged.exists());
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn limits_and_duplicate_mutations_fail_before_commit() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("input.jar");
    make_archive(&path);
    let before = fs::read(&path).unwrap();
    assert!(
        JarFile::open_with_limits(
            &path,
            ArchiveReadLimits {
                max_entry_bytes: 7,
                ..ArchiveReadLimits::default()
            }
        )
        .is_err()
    );
    let mut jar = JarFile::open(&path).unwrap();
    jar.entries.push(jar.entries[0].clone());
    assert!(jar.rewrite(None, None, RewriteOptions::default()).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn exact_duplicate_central_directory_names_are_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("input.jar");
    let mut writer = ZipWriter::new(File::create(&path).unwrap());
    for name in ["entry1", "entry2"] {
        writer
            .start_file(name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"payload").unwrap();
    }
    writer.finish().unwrap();
    // Keep lengths and offsets intact while creating an archive whose names
    // would otherwise be collapsed by the ZIP reader's internal name map.
    let mut bytes = fs::read(&path).unwrap();
    for index in 0..bytes.len() - 6 {
        if &bytes[index..index + 6] == b"entry2" {
            bytes[index + 5] = b'1';
        }
    }
    fs::write(&path, bytes).unwrap();
    assert!(
        JarFile::open(path)
            .unwrap_err()
            .to_string()
            .contains("duplicate")
    );
}
