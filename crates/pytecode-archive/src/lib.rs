use pytecode_engine::analysis::ClassResolver;
use pytecode_engine::model::{DebugInfoPolicy, FrameComputationMode};
use pytecode_engine::raw::RawClassStub;
use pytecode_engine::transform::ApplyClassTransform;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashSet;
use std::fs;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;
use thiserror::Error;
use zip::read::HasZipMetadata;
use zip::write::FullFileOptions;
use zip::{CompressionMethod, DateTime, System, ZipArchive, ZipWriter};

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
    pub system: System,
    pub comment: Vec<u8>,
    pub extra_data: Vec<u8>,
    pub is_dir: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JarInfo {
    pub filename: String,
    pub bytes: Vec<u8>,
    pub metadata: JarEntryMetadata,
    original_index: Option<usize>,
}

impl JarInfo {
    pub fn new(
        filename: String,
        bytes: Vec<u8>,
        metadata: JarEntryMetadata,
        original_index: Option<usize>,
    ) -> Self {
        Self {
            filename,
            bytes,
            metadata,
            original_index,
        }
    }

    pub const fn original_index(&self) -> Option<usize> {
        self.original_index
    }
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
    #[error("archive entry timestamp is out of ZIP range for entry: {0}")]
    InvalidTimestamp(String),
    #[error("archive entry comment must be valid UTF-8 to rewrite natively: {0}")]
    NonUtf8Comment(String),
    #[error("duplicate normalized archive entry name: {0}")]
    DuplicateFilename(String),
    #[error("archive read limit exceeded: {0}")]
    ReadLimit(&'static str),
}

pub type Result<T> = std::result::Result<T, ArchiveError>;

/// Limits apply to declared sizes and actual decompressed bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArchiveReadLimits {
    pub max_entries: usize,
    pub max_entry_bytes: u64,
    pub max_total_bytes: u64,
}

impl Default for ArchiveReadLimits {
    fn default() -> Self {
        Self {
            max_entries: 100_000,
            max_entry_bytes: 256 * 1024 * 1024,
            max_total_bytes: 1024 * 1024 * 1024,
        }
    }
}

/// A validated rewrite. Dropping this value removes the temporary archive.
/// Commit atomically replaces the destination; it does not promise crash durability.
pub struct PreparedRewrite {
    temporary: NamedTempFile,
    destination: PathBuf,
    entries: Vec<JarInfo>,
}

impl PreparedRewrite {
    pub fn path(&self) -> &Path {
        self.temporary.path()
    }

    pub fn commit(self, jar: &mut JarFile) -> Result<PathBuf> {
        self.temporary
            .persist(&self.destination)
            .map_err(|error| error.error)?;
        jar.filename = self.destination;
        jar.entries = self.entries;
        Ok(jar.filename.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JarFile {
    pub filename: PathBuf,
    pub entries: Vec<JarInfo>,
    pub read_limits: ArchiveReadLimits,
}

impl JarFile {
    pub fn from_entries(path: impl Into<PathBuf>, entries: Vec<JarInfo>) -> Self {
        Self {
            filename: path.into(),
            entries,
            read_limits: ArchiveReadLimits::default(),
        }
    }

    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        Self::open_with_limits(path, ArchiveReadLimits::default())
    }

    pub fn open_with_limits(path: impl Into<PathBuf>, limits: ArchiveReadLimits) -> Result<Self> {
        let filename = path.into();
        let entries = read_archive_entries(&filename, limits)?;
        Ok(Self {
            filename,
            entries,
            read_limits: limits,
        })
    }

    pub fn read(&mut self) -> Result<()> {
        self.entries = read_archive_entries(&self.filename, self.read_limits)?;
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
                system: System::Unknown,
                comment: Vec::new(),
                extra_data: Vec::new(),
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
                system: System::Unknown,
                comment: Vec::new(),
                extra_data: Vec::new(),
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
        transform: Option<&mut dyn ApplyClassTransform>,
        options: RewriteOptions<'_>,
    ) -> Result<PathBuf> {
        self.prepare_rewrite(output_path, transform, options)?
            .commit(self)
    }

    pub fn prepare_rewrite(
        &self,
        output_path: Option<&Path>,
        mut transform: Option<&mut dyn ApplyClassTransform>,
        options: RewriteOptions<'_>,
    ) -> Result<PreparedRewrite> {
        let destination = output_path
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.filename.clone());
        let parent = destination
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;
        let temporary = NamedTempFile::new_in(parent)?;
        let mut source = ZipArchive::new(File::open(&self.filename)?)?;
        let no_transform = transform.is_none();
        let mut names = HashSet::new();

        {
            let file = temporary.reopen()?;
            let mut writer = ZipWriter::new(file);
            writer.set_raw_comment(source.comment().to_vec().into_boxed_slice())?;
            for entry in &self.entries {
                let normalized = normalize_filename(&entry.filename, entry.metadata.is_dir)?;
                if !names.insert(normalized.clone()) {
                    return Err(ArchiveError::DuplicateFilename(normalized));
                }
                if let Some(index) = entry.original_index
                    && should_raw_copy_entry(entry, no_transform, options)
                    && source_entry_matches(&mut source, index, entry)?
                {
                    let source_entry = source.by_index(index)?;
                    let archive_name = archive_name(&entry.filename);
                    if source_entry.name() == archive_name {
                        writer.raw_copy_file(source_entry)?;
                    } else {
                        writer.raw_copy_file_rename(source_entry, archive_name)?;
                    }
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

        drop(source);
        let entries = read_archive_entries(temporary.path(), self.read_limits)?;
        Ok(PreparedRewrite {
            temporary,
            destination,
            entries,
        })
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

fn read_archive_entries(path: &Path, limits: ArchiveReadLimits) -> Result<Vec<JarInfo>> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file)?;
    let mut entries = Vec::new();
    if archive.len() > limits.max_entries {
        return Err(ArchiveError::ReadLimit("entry count"));
    }
    check_central_directory_count(path, archive.central_directory_start(), archive.len())?;
    let mut names = HashSet::new();
    let mut total_bytes = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let is_dir = entry.is_dir();
        let filename = normalize_filename(entry.name(), is_dir)?;
        if !names.insert(filename.clone()) {
            return Err(ArchiveError::DuplicateFilename(filename));
        }
        let bound = limits
            .max_entry_bytes
            .min(limits.max_total_bytes.saturating_sub(total_bytes));
        if entry.size() > bound {
            return Err(ArchiveError::ReadLimit("uncompressed bytes"));
        }
        let mut bytes = Vec::new();
        (&mut entry)
            .take(bound.saturating_add(1))
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > bound {
            return Err(ArchiveError::ReadLimit("uncompressed bytes"));
        }
        total_bytes += bytes.len() as u64;
        entries.push(JarInfo {
            filename,
            bytes,
            metadata: JarEntryMetadata {
                compression_method: entry.compression(),
                last_modified: entry.last_modified().unwrap_or_default(),
                unix_mode: entry.unix_mode(),
                system: entry.get_metadata().system,
                comment: entry.comment().as_bytes().to_vec(),
                extra_data: entry
                    .extra_data()
                    .map_or_else(Vec::new, std::borrow::ToOwned::to_owned),
                is_dir,
            },
            original_index: Some(index),
        });
    }
    Ok(entries)
}

// zip stores entries in a name-keyed map and can discard exact duplicates before
// exposing them. Count the central headers at its validated directory offset so
// that such archives cannot silently lose entries. ZIP64 uses the same headers.
fn check_central_directory_count(path: &Path, start: u64, expected: usize) -> Result<()> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(start))?;
    for index in 0..=expected {
        let mut signature = [0; 4];
        file.read_exact(&mut signature)?;
        if signature != *b"PK\x01\x02" {
            return Ok(());
        }
        if index == expected {
            return Err(ArchiveError::DuplicateFilename(
                "central directory contains duplicate names".to_owned(),
            ));
        }
        let mut header = [0; 42];
        file.read_exact(&mut header)?;
        let variable_length: u64 = [24, 26, 28]
            .into_iter()
            .map(|offset| u64::from(u16::from_le_bytes([header[offset], header[offset + 1]])))
            .sum();
        file.seek(SeekFrom::Current(variable_length as i64))?;
    }
    Ok(())
}

fn should_raw_copy_entry(entry: &JarInfo, no_transform: bool, options: RewriteOptions<'_>) -> bool {
    no_transform
        && options.frame_mode == FrameComputationMode::Preserve
        && options.debug_info == DebugInfoPolicy::Preserve
        && options.resolver.is_none()
        && entry.original_index.is_some()
        // zip's raw-copy API does not retain arbitrary extra fields.
        && entry.metadata.extra_data.is_empty()
}

fn source_entry_matches(
    source: &mut ZipArchive<File>,
    index: usize,
    entry: &JarInfo,
) -> Result<bool> {
    if index >= source.len() {
        return Ok(false);
    }
    let mut original = source.by_index(index)?;
    if original.size() != entry.bytes.len() as u64
        || original.compression() != entry.metadata.compression_method
        || original.last_modified().unwrap_or_default() != entry.metadata.last_modified
        || original.unix_mode() != entry.metadata.unix_mode
        || original.get_metadata().system != entry.metadata.system
        || original.comment().as_bytes() != entry.metadata.comment
        || original.extra_data().unwrap_or_default() != entry.metadata.extra_data
        || original.is_dir() != entry.metadata.is_dir
    {
        return Ok(false);
    }
    // Compare bytes, not a caller-supplied index or a collision-prone fingerprint.
    let mut offset = 0;
    let mut buffer = [0_u8; 8192];
    loop {
        let count = original.read(&mut buffer)?;
        if count == 0 {
            return Ok(offset == entry.bytes.len());
        }
        if entry.bytes.get(offset..offset + count) != Some(&buffer[..count]) {
            return Ok(false);
        }
        offset += count;
    }
}

fn write_entry(
    writer: &mut ZipWriter<File>,
    entry: &JarInfo,
    transform: Option<&mut dyn ApplyClassTransform>,
    options: RewriteOptions<'_>,
) -> Result<()> {
    if entry.metadata.is_dir {
        writer.add_directory(archive_name(&entry.filename), file_options(entry)?)?;
        return Ok(());
    }

    let bytes: Cow<'_, [u8]> = if is_class_filename(entry) {
        let should_relower = transform.is_some()
            || options.frame_mode == FrameComputationMode::Recompute
            || options.debug_info != DebugInfoPolicy::Preserve
            || options.resolver.is_some();
        if should_relower {
            let mut model = pytecode_engine::model::ClassModel::from_bytes(&entry.bytes)?;
            if let Some(transform) = transform {
                transform.apply(&mut model)?;
            }
            let classfile = model.to_classfile_with_options(
                options.debug_info,
                options.frame_mode,
                options.resolver,
            )?;
            Cow::Owned(pytecode_engine::write_class(&classfile)?)
        } else {
            Cow::Borrowed(&entry.bytes)
        }
    } else {
        Cow::Borrowed(&entry.bytes)
    };

    let file_options = file_options(entry)?;
    writer.start_file(archive_name(&entry.filename), file_options)?;
    writer.write_all(&bytes)?;
    Ok(())
}

fn file_options(entry: &JarInfo) -> Result<FullFileOptions<'static>> {
    let mut options = FullFileOptions::default()
        .compression_method(entry.metadata.compression_method)
        .last_modified_time(entry.metadata.last_modified)
        .system(entry.metadata.system);
    if let Some(unix_mode) = entry.metadata.unix_mode {
        options = options.unix_permissions(unix_mode);
    }
    if !entry.metadata.comment.is_empty() {
        let comment = std::str::from_utf8(&entry.metadata.comment)
            .map_err(|_| ArchiveError::NonUtf8Comment(entry.filename.clone()))?;
        options = options.with_file_comment(comment.to_owned());
    }
    if !entry.metadata.extra_data.is_empty() {
        copy_extra_data(&mut options, &entry.metadata.extra_data)?;
    }
    Ok(options)
}

fn copy_extra_data(options: &mut FullFileOptions<'static>, extra_data: &[u8]) -> Result<()> {
    let mut offset = 0_usize;
    while offset < extra_data.len() {
        if extra_data.len().saturating_sub(offset) < 4 {
            return Err(ArchiveError::Io(io::Error::other(
                "archive entry extra data is truncated",
            )));
        }
        let header_id = u16::from_le_bytes([extra_data[offset], extra_data[offset + 1]]);
        let data_len = usize::from(u16::from_le_bytes([
            extra_data[offset + 2],
            extra_data[offset + 3],
        ]));
        let value_start = offset + 4;
        let value_end = value_start + data_len;
        if value_end > extra_data.len() {
            return Err(ArchiveError::Io(io::Error::other(
                "archive entry extra data is truncated",
            )));
        }
        options
            .add_extra_data(header_id, &extra_data[value_start..value_end], false)
            .map_err(ArchiveError::Zip)?;
        offset = value_end;
    }
    Ok(())
}

fn normalize_filename(filename: &str, force_dir: bool) -> Result<String> {
    if filename.is_empty() {
        return Err(ArchiveError::EmptyFilename);
    }
    if Path::new(filename).is_absolute()
        || filename.starts_with('/')
        || filename.starts_with('\\')
        || filename.as_bytes().get(1) == Some(&b':')
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
