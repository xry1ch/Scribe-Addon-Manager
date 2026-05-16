use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::io::{self, copy, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Datelike, SecondsFormat, Timelike, Utc};
use serde::{Deserialize, Serialize};
use tempfile::{Builder, NamedTempFile, TempDir};
use thiserror::Error;
use tracing::debug;
use zip::result::ZipError;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

#[derive(Debug, Clone)]
pub struct BackupResult {
    pub backup_zip_path: PathBuf,
    pub backup_created: bool,
    pub included_saved_variables: bool,
    pub file_count: u64,
    pub total_uncompressed_bytes: u64,
    pub skipped_files: Vec<SkippedBackupFile>,
    pub warnings: Vec<String>,
    pub backup_status: BackupStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkippedBackupFile {
    pub relative_path: String,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupStatus {
    Complete,
    CompletedWithWarnings,
}

impl BackupStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::CompletedWithWarnings => "completed_with_warnings",
        }
    }
}

#[derive(Debug, Clone)]
pub struct BackupInspection {
    pub valid: bool,
    pub backup_name: String,
    pub created_at: Option<String>,
    pub contains_addons: bool,
    pub contains_saved_variables: bool,
    pub file_count: u64,
    pub total_bytes: u64,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RestoreResult {
    pub restored_addons: bool,
    pub restored_saved_variables: bool,
    pub message: String,
    pub rollback_path: Option<PathBuf>,
}

#[derive(Debug, Error)]
pub enum BackupError {
    #[error("AddOns directory does not exist: {0}")]
    MissingAddonsDir(PathBuf),

    #[error("AddOns path is not a directory: {0}")]
    AddonsPathNotDirectory(PathBuf),

    #[error("backup folder is not a directory: {0}")]
    BackupDirNotDirectory(PathBuf),

    #[error("backup folder cannot be inside the AddOns directory: {0}")]
    BackupDirInsideAddons(PathBuf),

    #[error("backup folder cannot be inside SavedVariables: {0}")]
    BackupDirInsideSavedVariables(PathBuf),

    #[error("Only ZIP backups are supported.")]
    UnsupportedBackup,

    #[error("refusing to follow symlink: {0}")]
    Symlink(PathBuf),

    #[error("unsafe ZIP entry path rejected: {name}")]
    UnsafeZipEntry { name: String },

    #[error("unsupported backup ZIP entry: {name}")]
    UnsupportedZipEntry { name: String },

    #[error("duplicate ZIP entry rejected: {name}")]
    DuplicateZipEntry { name: String },

    #[error("invalid backup ZIP: {0}")]
    InvalidBackup(String),

    #[error("backup did not copy any files; source files may be locked or unreadable")]
    NoFilesCopied,

    #[error("restore requires at least one selected folder")]
    EmptyRestoreSelection,

    #[error("rollback failed after restore error. restore error: {restore_error}; rollback error: {rollback_error}")]
    RollbackFailed {
        restore_error: String,
        rollback_error: String,
    },

    #[error("ZIP error: {0}")]
    Zip(#[from] ZipError),

    #[error("metadata error: {0}")]
    Metadata(#[from] serde_json::Error),

    #[error("filesystem error: {0}")]
    Io(#[from] io::Error),
}

#[derive(Debug, Clone, Copy, Default)]
struct FileStats {
    files: u64,
    bytes: u64,
}

impl FileStats {
    fn add(&mut self, other: FileStats) {
        self.files += other.files;
        self.bytes += other.bytes;
    }
}

#[derive(Debug, Clone, Default)]
struct BackupWriteReport {
    stats: FileStats,
    skipped_files: Vec<SkippedBackupFile>,
}

impl BackupWriteReport {
    fn add(&mut self, other: BackupWriteReport) {
        self.stats.add(other.stats);
        self.skipped_files.extend(other.skipped_files);
    }

    fn skip(&mut self, relative_path: impl Into<String>, error: impl std::fmt::Display) {
        self.skipped_files.push(SkippedBackupFile {
            relative_path: relative_path.into(),
            reason: error.to_string(),
        });
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct BackupMetadata {
    created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    app_version: Option<String>,
    addons_path: String,
    included_saved_variables: bool,
    file_count: u64,
    total_uncompressed_bytes: u64,
    #[serde(default)]
    skipped_files: u64,
    #[serde(default)]
    warnings: Vec<String>,
    #[serde(default = "default_backup_status")]
    backup_status: BackupStatus,
}

fn default_backup_status() -> BackupStatus {
    BackupStatus::Complete
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BackupEntryKind {
    AddOns,
    SavedVariables,
    Metadata,
}

#[derive(Debug)]
struct RestoreRequest {
    name: &'static str,
    source: PathBuf,
    target: PathBuf,
}

#[derive(Debug)]
struct Replacement {
    target: PathBuf,
    rollback: Option<PathBuf>,
    restored: bool,
}

pub fn create_compressed_backup(
    addons_dir: &Path,
    backup_dir: &Path,
    include_saved_variables: bool,
) -> Result<BackupResult, BackupError> {
    create_compressed_backup_with_app_version(addons_dir, backup_dir, include_saved_variables, None)
}

pub fn create_compressed_backup_with_app_version(
    addons_dir: &Path,
    backup_dir: &Path,
    include_saved_variables: bool,
    app_version: Option<&str>,
) -> Result<BackupResult, BackupError> {
    create_compressed_backup_at_with_reader(
        addons_dir,
        backup_dir,
        include_saved_variables,
        app_version,
        SystemTime::now(),
        &read_file_bytes,
    )
}

#[cfg(test)]
fn create_compressed_backup_at(
    addons_dir: &Path,
    backup_dir: &Path,
    include_saved_variables: bool,
    app_version: Option<&str>,
    created_at: SystemTime,
) -> Result<BackupResult, BackupError> {
    create_compressed_backup_at_with_reader(
        addons_dir,
        backup_dir,
        include_saved_variables,
        app_version,
        created_at,
        &read_file_bytes,
    )
}

fn create_compressed_backup_at_with_reader(
    addons_dir: &Path,
    backup_dir: &Path,
    include_saved_variables: bool,
    app_version: Option<&str>,
    created_at: SystemTime,
    file_reader: &dyn Fn(&Path) -> io::Result<Vec<u8>>,
) -> Result<BackupResult, BackupError> {
    validate_backup_dir_location(backup_dir, addons_dir)?;
    let addons_dir = prepare_addons_dir(addons_dir)?;
    validate_backup_dir_location(backup_dir, &addons_dir)?;
    prepare_backup_dir(backup_dir)?;
    let backup_dir = fs::canonicalize(backup_dir)?;
    validate_backup_dir_location(&backup_dir, &addons_dir)?;

    let saved_variables_dir = saved_variables_dir_for_addons(&addons_dir);
    let mut include_saved_variables_in_zip = false;
    if include_saved_variables {
        include_saved_variables_in_zip = matches!(
            source_dir_status(&saved_variables_dir)?,
            SourceDirStatus::Directory
        );
    }

    let timestamp = timestamp_name(created_at);
    let backup_zip_path = create_unique_backup_zip_path(&backup_dir, &timestamp)?;
    validate_backup_zip_target(
        &backup_zip_path,
        &addons_dir,
        include_saved_variables_in_zip,
    )?;

    debug!("creating compressed AddOns backup at {:?}", backup_zip_path);
    let mut temp_file = Builder::new()
        .prefix(".scribe-backup-")
        .suffix(".zip.tmp")
        .tempfile_in(&backup_dir)?;

    let mut report = BackupWriteReport::default();
    let warnings;
    let backup_status;
    {
        let mut zip = ZipWriter::new(temp_file.as_file_mut());
        report.add(write_source_dir_to_zip(
            &mut zip,
            &addons_dir,
            "AddOns",
            file_reader,
        )?);

        if include_saved_variables_in_zip {
            report.add(write_source_dir_to_zip(
                &mut zip,
                &saved_variables_dir,
                "SavedVariables",
                file_reader,
            )?);
        }

        if report.stats.files == 0 {
            return Err(BackupError::NoFilesCopied);
        }

        warnings = backup_warnings(&report.skipped_files);
        backup_status = if report.skipped_files.is_empty() {
            BackupStatus::Complete
        } else {
            BackupStatus::CompletedWithWarnings
        };
        let metadata = BackupMetadata {
            created_at: metadata_timestamp(created_at),
            app_version: app_version
                .map(str::trim)
                .filter(|version| !version.is_empty())
                .map(ToOwned::to_owned),
            addons_path: path_string(&addons_dir),
            included_saved_variables: include_saved_variables_in_zip,
            file_count: report.stats.files,
            total_uncompressed_bytes: report.stats.bytes,
            skipped_files: report.skipped_files.len() as u64,
            warnings: warnings.clone(),
            backup_status,
        };
        zip.start_file("metadata.json", zip_file_options())?;
        zip.write_all(serde_json::to_vec_pretty(&metadata)?.as_slice())?;
        zip.finish()?;
    }

    persist_temp_file(temp_file, &backup_zip_path)?;

    Ok(BackupResult {
        backup_zip_path,
        backup_created: true,
        included_saved_variables: include_saved_variables_in_zip,
        file_count: report.stats.files,
        total_uncompressed_bytes: report.stats.bytes,
        skipped_files: report.skipped_files,
        warnings,
        backup_status,
    })
}

pub fn inspect_backup_zip(zip_path: &Path) -> Result<BackupInspection, BackupError> {
    ensure_zip_backup_path(zip_path)?;
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;
    inspect_backup_archive(zip_path, &mut archive)
}

pub fn restore_backup_zip(
    zip_path: &Path,
    addons_dir: &Path,
    restore_addons: bool,
    restore_saved_variables: bool,
) -> Result<RestoreResult, BackupError> {
    if !restore_addons && !restore_saved_variables {
        return Err(BackupError::EmptyRestoreSelection);
    }

    let addons_dir = prepare_addons_dir(addons_dir)?;
    let live_dir = addons_dir
        .parent()
        .ok_or_else(|| BackupError::InvalidBackup("AddOns folder has no parent".to_owned()))?
        .to_path_buf();
    let (extract_dir, inspection) = extract_backup_zip_to_temp(zip_path, &live_dir)?;

    if restore_addons && !inspection.contains_addons {
        return Err(BackupError::InvalidBackup(
            "backup does not contain AddOns".to_owned(),
        ));
    }
    if restore_saved_variables && !inspection.contains_saved_variables {
        return Err(BackupError::InvalidBackup(
            "backup does not contain SavedVariables".to_owned(),
        ));
    }

    let mut requests = Vec::new();
    if restore_addons {
        requests.push(RestoreRequest {
            name: "AddOns",
            source: extract_dir.path().join("AddOns"),
            target: addons_dir.clone(),
        });
    }
    if restore_saved_variables {
        requests.push(RestoreRequest {
            name: "SavedVariables",
            source: extract_dir.path().join("SavedVariables"),
            target: live_dir.join("SavedVariables"),
        });
    }

    for request in &requests {
        validate_restore_request(request, &live_dir)?;
    }

    let rollback_timestamp = timestamp_name(SystemTime::now());
    let mut rollback_root = None;
    let mut replacements = Vec::new();
    let restore_result = apply_restore_requests(
        &requests,
        &live_dir,
        &rollback_timestamp,
        &mut rollback_root,
        &mut replacements,
    );
    if let Err(error) = restore_result {
        if let Err(rollback_error) = rollback_replacements(&mut replacements) {
            return Err(BackupError::RollbackFailed {
                restore_error: error.to_string(),
                rollback_error: rollback_error.to_string(),
            });
        }
        return Err(error);
    }

    let restored_addons = restore_addons;
    let restored_saved_variables = restore_saved_variables;
    let message = match (restored_addons, restored_saved_variables) {
        (true, true) => "Backup restored. AddOns restored. SavedVariables restored.",
        (true, false) => "Backup restored. AddOns restored.",
        (false, true) => "Backup restored. SavedVariables restored.",
        (false, false) => unreachable!("empty restore selections are rejected"),
    }
    .to_owned();

    Ok(RestoreResult {
        restored_addons,
        restored_saved_variables,
        message,
        rollback_path: rollback_root,
    })
}

fn prepare_addons_dir(addons_dir: &Path) -> Result<PathBuf, BackupError> {
    match fs::symlink_metadata(addons_dir) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(BackupError::Symlink(addons_dir.to_path_buf()))
        }
        Ok(metadata) if metadata.is_dir() => Ok(fs::canonicalize(addons_dir)?),
        Ok(_) => Err(BackupError::AddonsPathNotDirectory(
            addons_dir.to_path_buf(),
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            Err(BackupError::MissingAddonsDir(addons_dir.to_path_buf()))
        }
        Err(error) => Err(BackupError::Io(error)),
    }
}

fn prepare_backup_dir(backup_dir: &Path) -> Result<(), BackupError> {
    match fs::symlink_metadata(backup_dir) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(BackupError::Symlink(backup_dir.to_path_buf()))
        }
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(BackupError::BackupDirNotDirectory(backup_dir.to_path_buf())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir_all(backup_dir)?;
            Ok(())
        }
        Err(error) => Err(BackupError::Io(error)),
    }
}

fn validate_backup_dir_location(backup_dir: &Path, addons_dir: &Path) -> Result<(), BackupError> {
    validate_backup_against_source(
        &absolute_normalized(backup_dir)?,
        &absolute_normalized(addons_dir)?,
    )?;

    validate_backup_against_source(
        &absolute_normalized_existing_prefix(backup_dir)?,
        &absolute_normalized_existing_prefix(addons_dir)?,
    )
}

fn validate_backup_zip_target(
    backup_zip_path: &Path,
    addons_dir: &Path,
    include_saved_variables: bool,
) -> Result<(), BackupError> {
    let backup_zip_path = absolute_normalized(backup_zip_path)?;
    let addons_dir = absolute_normalized(addons_dir)?;
    if is_same_or_child(&backup_zip_path, &addons_dir) {
        return Err(BackupError::BackupDirInsideAddons(backup_zip_path));
    }

    if include_saved_variables {
        if let Some(live_dir) = addons_dir.parent() {
            let saved_variables_dir = live_dir.join("SavedVariables");
            if is_same_or_child(&backup_zip_path, &saved_variables_dir) {
                return Err(BackupError::BackupDirInsideSavedVariables(backup_zip_path));
            }
        }
    }

    Ok(())
}

fn validate_backup_against_source(backup_dir: &Path, addons_dir: &Path) -> Result<(), BackupError> {
    if is_same_or_child(backup_dir, addons_dir) {
        return Err(BackupError::BackupDirInsideAddons(backup_dir.to_path_buf()));
    }

    if let Some(live_dir) = addons_dir.parent() {
        let saved_variables_dir = live_dir.join("SavedVariables");
        if is_same_or_child(backup_dir, &saved_variables_dir) {
            return Err(BackupError::BackupDirInsideSavedVariables(
                backup_dir.to_path_buf(),
            ));
        }
    }

    Ok(())
}

fn create_unique_backup_zip_path(
    backup_dir: &Path,
    timestamp: &str,
) -> Result<PathBuf, BackupError> {
    let base_name = format!("Scribe-Backup-{timestamp}");
    for suffix in 0.. {
        let name = if suffix == 0 {
            format!("{base_name}.zip")
        } else {
            format!("{base_name}-{suffix}.zip")
        };
        let candidate = backup_dir.join(name);
        match candidate.try_exists() {
            Ok(false) => return Ok(candidate),
            Ok(true) => continue,
            Err(error) => return Err(BackupError::Io(error)),
        }
    }

    unreachable!("unbounded suffix search should eventually find a free path")
}

enum SourceDirStatus {
    Directory,
    Missing,
}

fn source_dir_status(path: &Path) -> Result<SourceDirStatus, BackupError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(BackupError::Symlink(path.to_path_buf()))
        }
        Ok(metadata) if metadata.is_dir() => Ok(SourceDirStatus::Directory),
        Ok(_) => Ok(SourceDirStatus::Missing),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(SourceDirStatus::Missing),
        Err(error) => Err(BackupError::Io(error)),
    }
}

fn write_source_dir_to_zip<W: Write + io::Seek>(
    zip: &mut ZipWriter<W>,
    source: &Path,
    entry_prefix: &str,
    file_reader: &dyn Fn(&Path) -> io::Result<Vec<u8>>,
) -> Result<BackupWriteReport, BackupError> {
    let mut report = BackupWriteReport::default();
    match source_dir_status(source) {
        Ok(SourceDirStatus::Directory) => {}
        Ok(SourceDirStatus::Missing) => {
            return Err(BackupError::MissingAddonsDir(source.to_path_buf()))
        }
        Err(error) => {
            report.skip(entry_prefix.to_owned(), error);
            return Ok(report);
        }
    }

    zip.add_directory(format!("{entry_prefix}/"), zip_directory_options())?;

    let entries = match fs::read_dir(source) {
        Ok(entries) => entries,
        Err(error) => {
            report.skip(entry_prefix.to_owned(), error);
            return Ok(report);
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                report.skip(
                    entry_prefix.to_owned(),
                    format!("could not read directory entry: {error}"),
                );
                continue;
            }
        };
        let source_path = entry.path();
        let metadata = match fs::symlink_metadata(&source_path) {
            Ok(metadata) => metadata,
            Err(error) => {
                report.skip(
                    format!("{entry_prefix}/{}", entry.file_name().to_string_lossy()),
                    error,
                );
                continue;
            }
        };

        if metadata.file_type().is_symlink() {
            report.skip(
                format!("{entry_prefix}/{}", entry.file_name().to_string_lossy()),
                "refusing to follow symlink",
            );
            continue;
        }

        let component = match zip_component(entry.file_name().as_os_str(), &source_path) {
            Ok(component) => component,
            Err(error) => {
                report.skip(path_string(&source_path), error);
                continue;
            }
        };
        let entry_name = format!("{entry_prefix}/{component}");

        if metadata.is_dir() {
            report.add(write_source_dir_to_zip(
                zip,
                &source_path,
                &entry_name,
                file_reader,
            )?);
        } else if metadata.is_file() {
            let contents = match file_reader(&source_path) {
                Ok(contents) => contents,
                Err(error) => {
                    report.skip(entry_name, error);
                    continue;
                }
            };
            zip.start_file(entry_name, zip_file_options())?;
            zip.write_all(&contents)?;
            report.stats.files += 1;
            report.stats.bytes += contents.len() as u64;
        }
    }

    Ok(report)
}

fn read_file_bytes(path: &Path) -> io::Result<Vec<u8>> {
    let mut input = File::open(path)?;
    let mut contents = Vec::new();
    input.read_to_end(&mut contents)?;
    Ok(contents)
}

fn backup_warnings(skipped_files: &[SkippedBackupFile]) -> Vec<String> {
    if skipped_files.is_empty() {
        Vec::new()
    } else {
        vec!["Some files could not be copied because they were in use.".to_owned()]
    }
}

fn zip_component(value: &OsStr, path: &Path) -> Result<String, BackupError> {
    let value = value.to_str().ok_or_else(|| {
        BackupError::InvalidBackup(format!("path is not UTF-8: {}", path.display()))
    })?;
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
        || value.contains('\0')
    {
        return Err(BackupError::InvalidBackup(format!(
            "unsafe backup path: {}",
            path.display()
        )));
    }
    Ok(value.to_owned())
}

fn zip_file_options() -> SimpleFileOptions {
    SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644)
}

fn zip_directory_options() -> SimpleFileOptions {
    SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .unix_permissions(0o755)
}

fn persist_temp_file(temp_file: NamedTempFile, backup_zip_path: &Path) -> Result<(), BackupError> {
    temp_file
        .persist(backup_zip_path)
        .map(|_| ())
        .map_err(|error| BackupError::Io(error.error))
}

fn inspect_backup_archive<R: Read + io::Seek>(
    zip_path: &Path,
    archive: &mut ZipArchive<R>,
) -> Result<BackupInspection, BackupError> {
    let mut warnings = Vec::new();
    let mut seen_entries = BTreeSet::new();
    let mut contains_addons = false;
    let mut contains_saved_variables = false;
    let mut file_count = 0_u64;
    let mut total_bytes = 0_u64;
    let mut metadata = None;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        if entry.is_symlink() {
            return Err(BackupError::UnsafeZipEntry {
                name: entry.name().to_owned(),
            });
        }

        let safe_path = validate_backup_entry_path(entry.name(), entry.is_dir())?;
        if !seen_entries.insert(safe_path.clone()) {
            return Err(BackupError::DuplicateZipEntry {
                name: entry.name().to_owned(),
            });
        }

        match classify_backup_entry(&safe_path, entry.is_dir(), entry.name())? {
            BackupEntryKind::AddOns => {
                contains_addons = true;
                if entry.is_file() {
                    file_count += 1;
                    total_bytes = total_bytes.checked_add(entry.size()).ok_or_else(|| {
                        BackupError::InvalidBackup("backup size overflowed".to_owned())
                    })?;
                }
            }
            BackupEntryKind::SavedVariables => {
                contains_saved_variables = true;
                if entry.is_file() {
                    file_count += 1;
                    total_bytes = total_bytes.checked_add(entry.size()).ok_or_else(|| {
                        BackupError::InvalidBackup("backup size overflowed".to_owned())
                    })?;
                }
            }
            BackupEntryKind::Metadata => {
                let mut contents = String::new();
                entry.read_to_string(&mut contents)?;
                match serde_json::from_str::<BackupMetadata>(&contents) {
                    Ok(parsed) => metadata = Some(parsed),
                    Err(error) => {
                        warnings.push(format!("metadata.json could not be read: {error}"))
                    }
                }
            }
        }
    }

    if !contains_addons {
        warnings.push("backup does not contain AddOns".to_owned());
    }

    if let Some(metadata) = metadata.as_ref() {
        if metadata.file_count != file_count {
            warnings.push("metadata file count does not match ZIP contents".to_owned());
        }
        if metadata.total_uncompressed_bytes != total_bytes {
            warnings.push("metadata size does not match ZIP contents".to_owned());
        }
        if metadata.included_saved_variables != contains_saved_variables {
            warnings.push("metadata SavedVariables flag does not match ZIP contents".to_owned());
        }
        if metadata.skipped_files > 0 {
            warnings.push(format!(
                "{} file{} skipped when this backup was created",
                metadata.skipped_files,
                if metadata.skipped_files == 1 { "" } else { "s" }
            ));
        }
        warnings.extend(metadata.warnings.iter().cloned());
        if metadata.backup_status == BackupStatus::CompletedWithWarnings
            && metadata.warnings.is_empty()
        {
            warnings.push("backup completed with warnings".to_owned());
        }
    } else {
        warnings.push("metadata.json is missing".to_owned());
    }

    Ok(BackupInspection {
        valid: true,
        backup_name: backup_name(zip_path),
        created_at: metadata.map(|metadata| metadata.created_at),
        contains_addons,
        contains_saved_variables,
        file_count,
        total_bytes,
        warnings,
    })
}

fn extract_backup_zip_to_temp(
    zip_path: &Path,
    live_dir: &Path,
) -> Result<(TempDir, BackupInspection), BackupError> {
    ensure_zip_backup_path(zip_path)?;
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;
    let inspection = inspect_backup_archive(zip_path, &mut archive)?;
    let temp_dir = Builder::new()
        .prefix(".scribe-restore-extract-")
        .tempdir_in(live_dir)?;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        if entry.is_symlink() {
            return Err(BackupError::UnsafeZipEntry {
                name: entry.name().to_owned(),
            });
        }
        let safe_path = validate_backup_entry_path(entry.name(), entry.is_dir())?;
        if matches!(
            classify_backup_entry(&safe_path, entry.is_dir(), entry.name())?,
            BackupEntryKind::Metadata
        ) {
            continue;
        }

        let output_path = temp_dir.path().join(&safe_path);
        ensure_extracted_path_stays_in_temp(temp_dir.path(), &output_path)?;
        if entry.is_dir() {
            fs::create_dir_all(&output_path)?;
            continue;
        }

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = File::create(&output_path)?;
        copy(&mut entry, &mut output)?;
    }

    Ok((temp_dir, inspection))
}

fn validate_backup_entry_path(name: &str, is_dir: bool) -> Result<PathBuf, BackupError> {
    let safe_path = validate_raw_zip_entry_path(name)?;
    classify_backup_entry(&safe_path, is_dir, name)?;
    Ok(safe_path)
}

fn validate_raw_zip_entry_path(name: &str) -> Result<PathBuf, BackupError> {
    let name = name.trim();
    if name.is_empty() || name.contains('\0') {
        return Err(BackupError::UnsafeZipEntry {
            name: name.to_owned(),
        });
    }

    let normalized = name.replace('\\', "/");
    if normalized.starts_with('/') || normalized.starts_with("//") {
        return Err(BackupError::UnsafeZipEntry {
            name: name.to_owned(),
        });
    }

    if normalized
        .split('/')
        .next()
        .is_some_and(has_windows_drive_prefix)
    {
        return Err(BackupError::UnsafeZipEntry {
            name: name.to_owned(),
        });
    }

    let path = PathBuf::from(&normalized);
    let mut has_normal_component = false;
    for component in path.components() {
        match component {
            Component::Normal(_) => has_normal_component = true,
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(BackupError::UnsafeZipEntry {
                    name: name.to_owned(),
                });
            }
        }
    }

    if !has_normal_component {
        return Err(BackupError::UnsafeZipEntry {
            name: name.to_owned(),
        });
    }

    Ok(path)
}

fn classify_backup_entry(
    path: &Path,
    is_dir: bool,
    original_name: &str,
) -> Result<BackupEntryKind, BackupError> {
    let components = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>();

    if components.len() == 1 && components[0] == "metadata.json" {
        if is_dir {
            return Err(BackupError::UnsupportedZipEntry {
                name: original_name.to_owned(),
            });
        }
        return Ok(BackupEntryKind::Metadata);
    }

    let Some(root) = components.first() else {
        return Err(BackupError::UnsafeZipEntry {
            name: original_name.to_owned(),
        });
    };

    let kind = match root.as_str() {
        "AddOns" => BackupEntryKind::AddOns,
        "SavedVariables" => BackupEntryKind::SavedVariables,
        _ => {
            return Err(BackupError::UnsupportedZipEntry {
                name: original_name.to_owned(),
            })
        }
    };

    if components.len() == 1 && !is_dir {
        return Err(BackupError::UnsupportedZipEntry {
            name: original_name.to_owned(),
        });
    }

    Ok(kind)
}

fn validate_restore_request(request: &RestoreRequest, live_dir: &Path) -> Result<(), BackupError> {
    if !request.source.is_dir() {
        return Err(BackupError::InvalidBackup(format!(
            "backup does not contain {}",
            request.name
        )));
    }

    validate_restore_target(&request.target, live_dir, request.name)
}

fn validate_restore_target(
    target: &Path,
    live_dir: &Path,
    folder_name: &str,
) -> Result<(), BackupError> {
    let target = absolute_normalized(target)?;
    let live_dir = absolute_normalized(live_dir)?;
    if target.parent() != Some(live_dir.as_path()) {
        return Err(BackupError::InvalidBackup(format!(
            "restore target escapes ESO live folder: {}",
            target.display()
        )));
    }
    if target.file_name().and_then(|name| name.to_str()) != Some(folder_name) {
        return Err(BackupError::InvalidBackup(format!(
            "unexpected restore target: {}",
            target.display()
        )));
    }
    Ok(())
}

fn apply_restore_requests(
    requests: &[RestoreRequest],
    live_dir: &Path,
    rollback_timestamp: &str,
    rollback_root: &mut Option<PathBuf>,
    replacements: &mut Vec<Replacement>,
) -> Result<(), BackupError> {
    for request in requests {
        let rollback = prepare_target_for_restore(
            &request.target,
            request.name,
            live_dir,
            rollback_timestamp,
            rollback_root,
        )?;
        replacements.push(Replacement {
            target: request.target.clone(),
            rollback,
            restored: false,
        });

        fs::rename(&request.source, &request.target)?;
        if let Some(replacement) = replacements.last_mut() {
            replacement.restored = true;
        }
    }

    Ok(())
}

fn prepare_target_for_restore(
    target: &Path,
    folder_name: &str,
    live_dir: &Path,
    rollback_timestamp: &str,
    rollback_root: &mut Option<PathBuf>,
) -> Result<Option<PathBuf>, BackupError> {
    match fs::symlink_metadata(target) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(BackupError::Symlink(target.to_path_buf()));
        }
        Ok(metadata) if !metadata.is_dir() => {
            return Err(BackupError::InvalidBackup(format!(
                "restore target is not a directory: {}",
                target.display()
            )));
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(BackupError::Io(error)),
    }

    let rollback_root = ensure_rollback_root(live_dir, rollback_timestamp, rollback_root)?;
    let rollback_target = rollback_root.join(folder_name);
    fs::rename(target, &rollback_target)?;
    Ok(Some(rollback_target))
}

fn ensure_rollback_root(
    live_dir: &Path,
    timestamp: &str,
    rollback_root: &mut Option<PathBuf>,
) -> Result<PathBuf, BackupError> {
    if let Some(path) = rollback_root.as_ref() {
        return Ok(path.clone());
    }

    let path = create_unique_rollback_folder(live_dir, timestamp)?;
    *rollback_root = Some(path.clone());
    Ok(path)
}

fn create_unique_rollback_folder(live_dir: &Path, timestamp: &str) -> Result<PathBuf, BackupError> {
    let base_name = format!(".scribe-restore-rollback-{timestamp}");
    for suffix in 0.. {
        let name = if suffix == 0 {
            base_name.clone()
        } else {
            format!("{base_name}-{suffix}")
        };
        let candidate = live_dir.join(name);
        match fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(BackupError::Io(error)),
        }
    }

    unreachable!("unbounded suffix search should eventually find a free path")
}

fn rollback_replacements(replacements: &mut [Replacement]) -> Result<(), BackupError> {
    for replacement in replacements.iter().rev() {
        if replacement.restored && replacement.target.exists() {
            remove_restored_target(&replacement.target)?;
        }
        if let Some(rollback) = replacement.rollback.as_ref() {
            if rollback.exists() {
                fs::rename(rollback, &replacement.target)?;
            }
        }
    }
    Ok(())
}

fn remove_restored_target(target: &Path) -> Result<(), BackupError> {
    let metadata = fs::symlink_metadata(target)?;
    if metadata.is_dir() {
        fs::remove_dir_all(target)?;
    } else {
        fs::remove_file(target)?;
    }
    Ok(())
}

fn ensure_extracted_path_stays_in_temp(
    temp_dir: &Path,
    output_path: &Path,
) -> Result<(), BackupError> {
    let normalized_temp = absolute_normalized(temp_dir)?;
    let normalized_output = absolute_normalized(output_path)?;
    if !normalized_output.starts_with(&normalized_temp) {
        return Err(BackupError::UnsafeZipEntry {
            name: output_path.display().to_string(),
        });
    }
    Ok(())
}

fn ensure_zip_backup_path(zip_path: &Path) -> Result<(), BackupError> {
    if !zip_path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
    {
        return Err(BackupError::UnsupportedBackup);
    }
    Ok(())
}

fn has_windows_drive_prefix(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn saved_variables_dir_for_addons(addons_dir: &Path) -> PathBuf {
    addons_dir
        .parent()
        .map(|parent| parent.join("SavedVariables"))
        .unwrap_or_else(|| PathBuf::from("SavedVariables"))
}

fn timestamp_name(time: SystemTime) -> String {
    let timestamp: DateTime<Utc> = time.into();
    format!(
        "{:04}-{:02}-{:02}-{:02}-{:02}-{:02}",
        timestamp.year(),
        timestamp.month(),
        timestamp.day(),
        timestamp.hour(),
        timestamp.minute(),
        timestamp.second()
    )
}

fn metadata_timestamp(time: SystemTime) -> String {
    let timestamp: DateTime<Utc> = time.into();
    timestamp.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn absolute_normalized(path: &Path) -> Result<PathBuf, BackupError> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    Ok(normalize_components(&absolute))
}

fn absolute_normalized_existing_prefix(path: &Path) -> Result<PathBuf, BackupError> {
    let absolute = absolute_normalized(path)?;
    let mut existing = absolute.as_path();
    let mut missing = Vec::<OsString>::new();

    while !existing.exists() {
        if let Some(name) = existing.file_name() {
            missing.push(name.to_os_string());
        }
        existing = existing.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("no existing parent found for {}", path.display()),
            )
        })?;
    }

    let mut resolved = fs::canonicalize(existing)?;
    for component in missing.iter().rev() {
        resolved.push(component);
    }

    Ok(normalize_components(&resolved))
}

fn normalize_components(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(value) => normalized.push(value),
            Component::Prefix(_) | Component::RootDir => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn is_same_or_child(path: &Path, parent: &Path) -> bool {
    path == parent || path.starts_with(parent)
}

fn path_string(path: &Path) -> String {
    path.display().to_string()
}

fn backup_name(zip_path: &Path) -> String {
    zip_path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path_string(zip_path))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::fs::File;
    use std::io::{self, Read, Write};
    use std::path::Path;
    use std::time::{Duration, UNIX_EPOCH};

    use tempfile::tempdir;
    use zip::write::SimpleFileOptions;
    use zip::{ZipArchive, ZipWriter};

    use crate::install::backup::{
        create_compressed_backup_at, inspect_backup_zip, restore_backup_zip, BackupError,
    };
    use crate::install::backup::{
        create_compressed_backup_at_with_reader, read_file_bytes, BackupStatus,
    };

    const TIMESTAMP: &str = "2026-05-16-10-11-12";

    fn fixed_time() -> std::time::SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_778_926_272)
    }

    #[test]
    fn compressed_backup_creates_zip_with_addons_and_metadata() {
        let dir = tempdir().unwrap();
        let addons_dir = dir.path().join("AddOns");
        let backup_dir = dir.path().join("Backups");
        write_file(
            &addons_dir.join("SampleAddon").join("SampleAddon.txt"),
            "addon",
        );
        write_file(
            &dir.path().join("SavedVariables").join("SampleAddon.lua"),
            "saved",
        );

        let result = create_compressed_backup_at(
            &addons_dir,
            &backup_dir,
            false,
            Some("1.2.3"),
            fixed_time(),
        )
        .unwrap();

        assert_eq!(
            result
                .backup_zip_path
                .extension()
                .and_then(|ext| ext.to_str()),
            Some("zip")
        );
        assert_eq!(
            result
                .backup_zip_path
                .file_name()
                .unwrap()
                .to_string_lossy(),
            format!("Scribe-Backup-{TIMESTAMP}.zip")
        );
        assert!(!result.included_saved_variables);
        assert_eq!(result.file_count, 1);
        assert_eq!(result.total_uncompressed_bytes, 5);

        let entries = zip_entries(&result.backup_zip_path);
        assert!(entries.contains(&"AddOns/".to_owned()));
        assert!(entries.contains(&"AddOns/SampleAddon/SampleAddon.txt".to_owned()));
        assert!(entries.contains(&"metadata.json".to_owned()));
        assert!(!entries
            .iter()
            .any(|entry| entry.starts_with("SavedVariables/")));

        let metadata = zip_file_contents(&result.backup_zip_path, "metadata.json");
        assert!(metadata.contains("\"created_at\""));
        assert!(metadata.contains("\"app_version\": \"1.2.3\""));
        assert!(metadata.contains("\"included_saved_variables\": false"));
    }

    #[test]
    fn compressed_backup_includes_saved_variables_only_when_selected() {
        let dir = tempdir().unwrap();
        let addons_dir = dir.path().join("AddOns");
        let backup_dir = dir.path().join("Backups");
        write_file(
            &addons_dir.join("SampleAddon").join("SampleAddon.txt"),
            "addon",
        );
        write_file(
            &dir.path().join("SavedVariables").join("SampleAddon.lua"),
            "saved",
        );

        let result =
            create_compressed_backup_at(&addons_dir, &backup_dir, true, None, fixed_time())
                .unwrap();

        assert!(result.included_saved_variables);
        assert_eq!(result.file_count, 2);
        let entries = zip_entries(&result.backup_zip_path);
        assert!(entries.contains(&"AddOns/SampleAddon/SampleAddon.txt".to_owned()));
        assert!(entries.contains(&"SavedVariables/SampleAddon.lua".to_owned()));
        assert!(entries.contains(&"metadata.json".to_owned()));
    }

    #[test]
    fn include_saved_variables_true_handles_missing_saved_variables_gracefully() {
        let dir = tempdir().unwrap();
        let addons_dir = dir.path().join("AddOns");
        let backup_dir = dir.path().join("Backups");
        write_file(
            &addons_dir.join("SampleAddon").join("SampleAddon.txt"),
            "addon",
        );

        let result =
            create_compressed_backup_at(&addons_dir, &backup_dir, true, None, fixed_time())
                .unwrap();

        assert!(!result.included_saved_variables);
        let entries = zip_entries(&result.backup_zip_path);
        assert!(entries.contains(&"AddOns/".to_owned()));
        assert!(!entries
            .iter()
            .any(|entry| entry.starts_with("SavedVariables/")));
    }

    #[test]
    fn backup_folder_is_created_if_missing() {
        let dir = tempdir().unwrap();
        let addons_dir = dir.path().join("AddOns");
        let backup_dir = dir.path().join("Missing").join("Backups");
        write_file(
            &addons_dir.join("SampleAddon").join("SampleAddon.txt"),
            "addon",
        );

        let result =
            create_compressed_backup_at(&addons_dir, &backup_dir, false, None, fixed_time())
                .unwrap();

        assert!(backup_dir.is_dir());
        assert!(result.backup_zip_path.is_file());
    }

    #[test]
    fn timestamp_collision_gets_zip_suffix() {
        let dir = tempdir().unwrap();
        let addons_dir = dir.path().join("AddOns");
        let backup_dir = dir.path().join("Backups");
        write_file(
            &addons_dir.join("SampleAddon").join("SampleAddon.txt"),
            "addon",
        );
        fs::create_dir_all(&backup_dir).unwrap();
        fs::write(
            backup_dir.join(format!("Scribe-Backup-{TIMESTAMP}.zip")),
            "taken",
        )
        .unwrap();

        let result =
            create_compressed_backup_at(&addons_dir, &backup_dir, false, None, fixed_time())
                .unwrap();

        assert_eq!(
            result
                .backup_zip_path
                .file_name()
                .unwrap()
                .to_string_lossy(),
            format!("Scribe-Backup-{TIMESTAMP}-1.zip")
        );
    }

    #[test]
    fn backup_target_recursion_inside_addons_is_refused() {
        let dir = tempdir().unwrap();
        let addons_dir = dir.path().join("AddOns");
        let backup_dir = addons_dir.join("Backups");
        write_file(
            &addons_dir.join("SampleAddon").join("SampleAddon.txt"),
            "addon",
        );

        let error =
            create_compressed_backup_at(&addons_dir, &backup_dir, false, None, fixed_time())
                .unwrap_err();

        assert!(matches!(error, BackupError::BackupDirInsideAddons(_)));
        assert!(!backup_dir.exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_backup_parent_inside_addons_is_refused_before_create() {
        let dir = tempdir().unwrap();
        let addons_dir = dir.path().join("AddOns");
        let symlink = dir.path().join("AddOnsLink");
        let backup_dir = symlink.join("Backups");
        write_file(
            &addons_dir.join("SampleAddon").join("SampleAddon.txt"),
            "addon",
        );
        std::os::unix::fs::symlink(&addons_dir, &symlink).unwrap();

        let error =
            create_compressed_backup_at(&addons_dir, &backup_dir, false, None, fixed_time())
                .unwrap_err();

        assert!(matches!(error, BackupError::BackupDirInsideAddons(_)));
        assert!(!addons_dir.join("Backups").exists());
    }

    #[test]
    fn source_folders_are_not_modified() {
        let dir = tempdir().unwrap();
        let addons_dir = dir.path().join("AddOns");
        let saved_variables = dir.path().join("SavedVariables");
        let backup_dir = dir.path().join("Backups");
        write_file(
            &addons_dir.join("SampleAddon").join("SampleAddon.txt"),
            "addon",
        );
        write_file(&saved_variables.join("SampleAddon.lua"), "saved");

        create_compressed_backup_at(&addons_dir, &backup_dir, true, None, fixed_time()).unwrap();

        assert_eq!(
            fs::read_to_string(addons_dir.join("SampleAddon/SampleAddon.txt")).unwrap(),
            "addon"
        );
        assert_eq!(
            fs::read_to_string(saved_variables.join("SampleAddon.lua")).unwrap(),
            "saved"
        );
    }

    #[test]
    fn backup_skips_unreadable_file_and_records_warning_metadata() {
        let dir = tempdir().unwrap();
        let addons_dir = dir.path().join("AddOns");
        let backup_dir = dir.path().join("Backups");
        let good_file = addons_dir.join("SampleAddon").join("Good.txt");
        let locked_file = addons_dir.join("SampleAddon").join("Locked.txt");
        write_file(&good_file, "good");
        write_file(&locked_file, "locked");

        let result = create_compressed_backup_at_with_reader(
            &addons_dir,
            &backup_dir,
            false,
            None,
            fixed_time(),
            &|path| {
                if path.ends_with("Locked.txt") {
                    Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "simulated sharing violation",
                    ))
                } else {
                    read_file_bytes(path)
                }
            },
        )
        .unwrap();

        assert!(result.backup_created);
        assert_eq!(result.file_count, 1);
        assert_eq!(result.skipped_files.len(), 1);
        assert_eq!(
            result.skipped_files[0].relative_path,
            "AddOns/SampleAddon/Locked.txt"
        );
        assert!(result.skipped_files[0]
            .reason
            .contains("simulated sharing violation"));
        assert_eq!(result.backup_status, BackupStatus::CompletedWithWarnings);
        assert_eq!(
            result.warnings,
            vec!["Some files could not be copied because they were in use."]
        );

        let entries = zip_entries(&result.backup_zip_path);
        assert!(entries.contains(&"AddOns/SampleAddon/Good.txt".to_owned()));
        assert!(!entries.contains(&"AddOns/SampleAddon/Locked.txt".to_owned()));

        let metadata = zip_file_contents(&result.backup_zip_path, "metadata.json");
        let metadata: serde_json::Value = serde_json::from_str(&metadata).unwrap();
        assert_eq!(metadata["skipped_files"], 1);
        assert_eq!(metadata["backup_status"], "completed_with_warnings");
        assert_eq!(
            metadata["warnings"][0],
            "Some files could not be copied because they were in use."
        );
    }

    #[test]
    fn backup_errors_when_no_files_can_be_copied() {
        let dir = tempdir().unwrap();
        let addons_dir = dir.path().join("AddOns");
        let backup_dir = dir.path().join("Backups");
        write_file(&addons_dir.join("SampleAddon").join("Locked.txt"), "locked");

        let error = create_compressed_backup_at_with_reader(
            &addons_dir,
            &backup_dir,
            false,
            None,
            fixed_time(),
            &|_| {
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "simulated sharing violation",
                ))
            },
        )
        .unwrap_err();

        assert!(matches!(error, BackupError::NoFilesCopied));
        assert!(!fs::read_dir(&backup_dir).unwrap().any(|entry| entry
            .unwrap()
            .path()
            .extension()
            .is_some_and(|ext| ext == "zip")));
    }

    #[test]
    fn restore_inspection_rejects_unsafe_zip_path_traversal() {
        let dir = tempdir().unwrap();
        let zip_path = dir.path().join("unsafe.zip");
        create_zip(&zip_path, &[("../evil.txt", "oops")]);

        assert!(matches!(
            inspect_backup_zip(&zip_path),
            Err(BackupError::UnsafeZipEntry { .. })
        ));
    }

    #[test]
    fn restore_inspection_recognizes_addons_and_saved_variables() {
        let dir = tempdir().unwrap();
        let zip_path = dir.path().join("backup.zip");
        create_zip(
            &zip_path,
            &[
                ("AddOns/SampleAddon/SampleAddon.txt", "addon"),
                ("SavedVariables/SampleAddon.lua", "saved"),
                (
                    "metadata.json",
                    r#"{"created_at":"2026-05-16T10:11:12Z","addons_path":"x","included_saved_variables":true,"file_count":2,"total_uncompressed_bytes":10}"#,
                ),
            ],
        );

        let inspection = inspect_backup_zip(&zip_path).unwrap();

        assert!(inspection.valid);
        assert!(inspection.contains_addons);
        assert!(inspection.contains_saved_variables);
        assert_eq!(inspection.file_count, 2);
        assert_eq!(inspection.total_bytes, 10);
        assert_eq!(
            inspection.created_at.as_deref(),
            Some("2026-05-16T10:11:12Z")
        );
    }

    #[test]
    fn restore_addons_replaces_target_from_temp_extraction() {
        let dir = tempdir().unwrap();
        let live_dir = dir.path().join("live");
        let addons_dir = live_dir.join("AddOns");
        write_file(&addons_dir.join("OldAddon").join("OldAddon.txt"), "old");
        let zip_path = dir.path().join("backup.zip");
        create_zip(
            &zip_path,
            &[
                ("AddOns/NewAddon/NewAddon.txt", "new"),
                (
                    "metadata.json",
                    r#"{"created_at":"2026-05-16T10:11:12Z","addons_path":"x","included_saved_variables":false,"file_count":1,"total_uncompressed_bytes":3}"#,
                ),
            ],
        );

        let result = restore_backup_zip(&zip_path, &addons_dir, true, false).unwrap();

        assert!(result.restored_addons);
        assert!(!result.restored_saved_variables);
        assert_eq!(
            fs::read_to_string(addons_dir.join("NewAddon/NewAddon.txt")).unwrap(),
            "new"
        );
        assert!(!addons_dir.join("OldAddon").exists());
        assert!(result
            .rollback_path
            .as_ref()
            .unwrap()
            .join("AddOns/OldAddon/OldAddon.txt")
            .is_file());
    }

    #[test]
    fn restore_does_not_write_outside_target() {
        let dir = tempdir().unwrap();
        let live_dir = dir.path().join("live");
        let addons_dir = live_dir.join("AddOns");
        write_file(&addons_dir.join("OldAddon").join("OldAddon.txt"), "old");
        let zip_path = dir.path().join("unsafe.zip");
        create_zip(
            &zip_path,
            &[
                ("AddOns/../outside.txt", "outside"),
                ("AddOns/NewAddon/NewAddon.txt", "new"),
            ],
        );

        assert!(restore_backup_zip(&zip_path, &addons_dir, true, false).is_err());
        assert!(!live_dir.join("outside.txt").exists());
        assert_eq!(
            fs::read_to_string(addons_dir.join("OldAddon/OldAddon.txt")).unwrap(),
            "old"
        );
    }

    #[test]
    fn invalid_zip_returns_readable_error() {
        let dir = tempdir().unwrap();
        let zip_path = dir.path().join("invalid.zip");
        fs::write(&zip_path, "not a zip").unwrap();

        let error = inspect_backup_zip(&zip_path).unwrap_err();

        assert!(!error.to_string().is_empty());
    }

    fn write_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    fn zip_entries(path: &Path) -> Vec<String> {
        let file = File::open(path).unwrap();
        let mut archive = ZipArchive::new(file).unwrap();
        (0..archive.len())
            .map(|index| archive.by_index(index).unwrap().name().to_owned())
            .collect()
    }

    fn zip_file_contents(path: &Path, name: &str) -> String {
        let file = File::open(path).unwrap();
        let mut archive = ZipArchive::new(file).unwrap();
        let mut entry = archive.by_name(name).unwrap();
        let mut contents = String::new();
        entry.read_to_string(&mut contents).unwrap();
        contents
    }

    fn create_zip(path: &Path, files: &[(&str, &str)]) {
        let file = File::create(path).unwrap();
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        for (name, contents) in files {
            zip.start_file(*name, options).unwrap();
            zip.write_all(contents.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }
}
