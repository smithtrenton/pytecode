use pytecode_engine::analysis::ClassResolver;
use pytecode_engine::model::{DebugInfoPolicy, FrameComputationMode};
use pytecode_engine::raw::RawClassStub;
use pytecode_engine::transform::ApplyClassTransform;
use serde::{Deserialize, Serialize};
use std::fs;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceEntryStub {
    pub entry_name: String,
    pub byte_len: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JarInventory {
    pub class_entries: Vec<RawClassStub>,
    pub resource_entries: Vec<ResourceEntryStub>,
    pub total_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArchiveSupport {
    pub can_read: bool,
    pub can_rewrite: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JarEntryMetadata {
    pub compression_method: CompressionMethod,
    pub last_modified: DateTime,
    pub unix_mode: Option<u32>,
    pub is_dir: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JarInfo {
    pub filename: String,
    pub bytes: Vec<u8>,
    pub metadata: JarEntryMetadata,
    original_index: Option<usize>,
}

#[derive(Clone, Copy)]
pub struct RewriteOptions<'a> {
    pub frame_mode: FrameComputationMode,
    pub resolver: Option<&'a dyn ClassResolver>,
    pub debug_info: DebugInfoPolicy,
}

impl Default for RewriteOptions<'_> {
    fn default() -> Self {
        Self {
            frame_mode: FrameComputationMode::Preserve,
            resolver: None,
            debug_info: DebugInfoPolicy::Preserve,
        }
    }
}

#[derive(Debug, Error)]
pub enum ArchiveError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),
    #[error(transparent)]
    Engine(#[from] pytecode_engine::EngineError),
    #[error("archive entry filename must not be empty")]
    EmptyFilename,
    #[error("archive entry filename must be relative: {0}")]
    AbsolutePath(String),
    #[error("archive entry filename must not contain parent directory references: {0}")]
    ParentTraversal(String),
}

pub type Result<T> = std::result::Result<T, ArchiveError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JarFile {
    pub filename: PathBuf,
    pub entries: Vec<JarInfo>,
}

impl JarFile {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let filename = path.into();
        let entries = read_archive_entries(&filename)?;
        Ok(Self { filename, entries })
    }

    pub fn read(&mut self) -> Result<()> {
        self.entries = read_archive_entries(&self.filename)?;
        Ok(())
    }

    pub fn add_file(
        &mut self,
        filename: impl AsRef<str>,
        data: impl Into<Vec<u8>>,
    ) -> Result<&JarInfo> {
        let filename = normalize_filename(filename.as_ref(), false)?;
        let bytes = data.into();
        if let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.filename == filename)
        {
            let metadata = self.entries[index].metadata.clone();
            self.entries[index] = JarInfo {
                filename,
                bytes,
                metadata,
                original_index: None,
            };
            return Ok(&self.entries[index]);
        }
        self.entries.push(JarInfo {
            filename,
            bytes,
            metadata: JarEntryMetadata {
                compression_method: CompressionMethod::Stored,
                last_modified: DateTime::default(),
                unix_mode: None,
                is_dir: false,
            },
            original_index: None,
        });
        Ok(self.entries.last().expect("entry was just pushed"))
    }

    pub fn add_directory(&mut self, filename: impl AsRef<str>) -> Result<&JarInfo> {
        let filename = normalize_filename(filename.as_ref(), true)?;
        if let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.filename == filename)
        {
            self.entries[index].metadata.is_dir = true;
            self.entries[index].bytes.clear();
            self.entries[index].original_index = None;
            return Ok(&self.entries[index]);
        }
        self.entries.push(JarInfo {
            filename,
            bytes: Vec::new(),
            metadata: JarEntryMetadata {
                compression_method: CompressionMethod::Stored,
                last_modified: DateTime::default(),
                unix_mode: None,
                is_dir: true,
            },
            original_index: None,
        });
        Ok(self.entries.last().expect("entry was just pushed"))
    }

    pub fn remove_file(&mut self, filename: impl AsRef<str>) -> Result<JarInfo> {
        let filename = normalize_filename(filename.as_ref(), false)?;
        let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.filename == filename)
        else {
            return Err(ArchiveError::Io(io::Error::new(
                io::ErrorKind::NotFound,
                format!("archive entry not found: {filename}"),
            )));
        };
        Ok(self.entries.remove(index))
    }

    pub fn parse_classes(&self) -> (Vec<(JarInfo, RawClassStub)>, Vec<JarInfo>) {
        let mut classes = Vec::new();
        let mut others = Vec::new();
        for entry in &self.entries {
            if is_class_filename(entry) {
                classes.push((
                    entry.clone(),
                    RawClassStub {
                        entry_name: entry.filename.clone(),
                        bytes: entry.bytes.clone(),
                    },
                ));
            } else if !entry.metadata.is_dir {
                others.push(entry.clone());
            }
        }
        (classes, others)
    }

    pub fn rewrite(
        &mut self,
        output_path: Option<&Path>,
        mut transform: Option<&mut dyn ApplyClassTransform>,
        options: RewriteOptions<'_>,
    ) -> Result<PathBuf> {
        let destination = output_path
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.filename.clone());
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        let temp_path = temporary_archive_path(&destination);
        let mut source = ZipArchive::new(File::open(&self.filename)?)?;
        let no_transform = transform.is_none();

        {
            let file = File::create(&temp_path)?;
            let mut writer = ZipWriter::new(file);
            for entry in &self.entries {
                if let Some(index) = entry.original_index
                    && should_raw_copy_entry(entry, no_transform, options)
                {
                    let source_entry = source.by_index(index)?;
                    writer.raw_copy_file_rename(source_entry, archive_name(&entry.filename))?;
                    continue;
                }
                if let Some(transform_ref) = transform.as_deref_mut() {
                    write_entry(&mut writer, entry, Some(transform_ref), options)?;
                } else {
                    write_entry(&mut writer, entry, None, options)?;
                }
            }
            writer.finish()?;
        }

        let original_filename = self.filename.clone();
        let original_entries = self.entries.clone();
        fs::rename(&temp_path, &destination)?;
        match read_archive_entries(&destination) {
            Ok(entries) => {
                self.filename = destination.clone();
                self.entries = entries;
                Ok(destination)
            }
            Err(error) => {
                self.filename = original_filename;
                self.entries = original_entries;
                Err(error)
            }
        }
    }
}

pub fn read_jar_bytes(path: &Path) -> io::Result<Vec<u8>> {
    fs::read(path)
}

pub fn inventory_jar(path: &Path) -> io::Result<JarInventory> {
    let jar = JarFile::open(path).map_err(io::Error::other)?;
    let mut class_entries = Vec::new();
    let mut resource_entries = Vec::new();
    let mut total_bytes = 0_usize;
    for entry in jar.entries {
        total_bytes += entry.bytes.len();
        if is_class_filename(&entry) {
            class_entries.push(RawClassStub {
                entry_name: entry.filename,
                bytes: entry.bytes,
            });
        } else if !entry.metadata.is_dir {
            resource_entries.push(ResourceEntryStub {
                entry_name: entry.filename,
                byte_len: entry.bytes.len(),
            });
        }
    }
    Ok(JarInventory {
        class_entries,
        resource_entries,
        total_bytes,
    })
}

pub fn parse_jar_classes(path: &Path) -> io::Result<Vec<RawClassStub>> {
    Ok(inventory_jar(path)?.class_entries)
}

pub const fn phase5_support() -> ArchiveSupport {
    ArchiveSupport {
        can_read: true,
        can_rewrite: true,
    }
}

pub const fn phase0_support() -> ArchiveSupport {
    phase5_support()
}

fn read_archive_entries(path: &Path) -> Result<Vec<JarInfo>> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file)?;
    let mut entries = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let is_dir = entry.is_dir();
        let filename = normalize_filename(entry.name(), is_dir)?;
        let mut bytes = Vec::with_capacity(usize::try_from(entry.size()).unwrap_or(0));
        if !is_dir {
            entry.read_to_end(&mut bytes)?;
        }
        entries.push(JarInfo {
            filename,
            bytes,
            metadata: JarEntryMetadata {
                compression_method: entry.compression(),
                last_modified: entry.last_modified().unwrap_or_default(),
                unix_mode: entry.unix_mode(),
                is_dir,
            },
            original_index: Some(index),
        });
    }
    Ok(entries)
}

fn should_raw_copy_entry(entry: &JarInfo, no_transform: bool, options: RewriteOptions<'_>) -> bool {
    no_transform
        && options.frame_mode == FrameComputationMode::Preserve
        && options.debug_info == DebugInfoPolicy::Preserve
        && options.resolver.is_none()
        && entry.original_index.is_some()
}

fn write_entry(
    writer: &mut ZipWriter<File>,
    entry: &JarInfo,
    transform: Option<&mut dyn ApplyClassTransform>,
    options: RewriteOptions<'_>,
) -> Result<()> {
    if entry.metadata.is_dir {
        writer.add_directory(archive_name(&entry.filename), file_options(entry))?;
        return Ok(());
    }

    let mut bytes = entry.bytes.clone();
    if is_class_filename(entry) {
        let should_relower = transform.is_some()
            || options.frame_mode == FrameComputationMode::Recompute
            || options.debug_info != DebugInfoPolicy::Preserve
            || options.resolver.is_some();
        if should_relower {
            let mut model = pytecode_engine::model::ClassModel::from_bytes(&bytes)?;
            if let Some(transform) = transform {
                transform.apply(&mut model)?;
            }
            let classfile = model.to_classfile_with_options(
                options.debug_info,
                options.frame_mode,
                options.resolver,
            )?;
            bytes = pytecode_engine::write_class(&classfile)?;
        }
    }

    writer.start_file(archive_name(&entry.filename), file_options(entry))?;
    writer.write_all(&bytes)?;
    Ok(())
}

fn file_options(entry: &JarInfo) -> SimpleFileOptions {
    let mut options = SimpleFileOptions::default()
        .compression_method(entry.metadata.compression_method)
        .last_modified_time(entry.metadata.last_modified);
    if let Some(unix_mode) = entry.metadata.unix_mode {
        options = options.unix_permissions(unix_mode);
    }
    options
}

fn normalize_filename(filename: &str, force_dir: bool) -> Result<String> {
    if filename.is_empty() {
        return Err(ArchiveError::EmptyFilename);
    }
    if Path::new(filename).is_absolute() || filename.starts_with('/') || filename.starts_with('\\')
    {
        return Err(ArchiveError::AbsolutePath(filename.to_owned()));
    }
    let posix = filename.replace('\\', "/");
    let mut parts = Vec::new();
    for part in posix.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            return Err(ArchiveError::ParentTraversal(filename.to_owned()));
        }
        parts.push(part);
    }
    if parts.is_empty() {
        return Err(ArchiveError::EmptyFilename);
    }
    let mut normalized = parts.join(std::path::MAIN_SEPARATOR_STR);
    if force_dir || filename.ends_with('/') || filename.ends_with('\\') {
        normalized.push(std::path::MAIN_SEPARATOR);
    }
    Ok(normalized)
}

fn archive_name(filename: &str) -> String {
    let is_dir = filename.ends_with(std::path::MAIN_SEPARATOR);
    let stripped = filename.trim_end_matches(std::path::MAIN_SEPARATOR);
    let archive_name = stripped.replace('\\', "/");
    if is_dir {
        format!("{archive_name}/")
    } else {
        archive_name
    }
}

fn is_class_filename(entry: &JarInfo) -> bool {
    !entry.metadata.is_dir && entry.filename.ends_with(".class")
}

fn temporary_archive_path(destination: &Path) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let file_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("archive.jar");
    destination
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!("{file_name}.{nanos}.tmp"))
}
