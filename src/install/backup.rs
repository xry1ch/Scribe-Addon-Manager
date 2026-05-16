use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Datelike, Timelike, Utc};
use thiserror::Error;
use tracing::debug;

#[derive(Debug, Clone)]
pub struct ManualBackupResult {
    pub backup_path: PathBuf,
    pub copied_addons: bool,
    pub copied_saved_variables: bool,
    pub saved_variables_missing: bool,
    pub total_files: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Error)]
pub enum ManualBackupError {
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

    #[error("refusing to follow symlink: {0}")]
    Symlink(PathBuf),

    #[error("filesystem error: {0}")]
    Io(#[from] io::Error),
}

#[derive(Debug, Clone, Copy, Default)]
struct CopyStats {
    files: u64,
    bytes: u64,
}

impl CopyStats {
    fn add(&mut self, other: CopyStats) {
        self.files += other.files;
        self.bytes += other.bytes;
    }
}

pub fn create_manual_backup(
    addons_dir: &Path,
    backup_dir: &Path,
    include_saved_variables: bool,
) -> Result<ManualBackupResult, ManualBackupError> {
    let timestamp = timestamp_name(SystemTime::now());
    create_manual_backup_with_timestamp(addons_dir, backup_dir, include_saved_variables, &timestamp)
}

fn create_manual_backup_with_timestamp(
    addons_dir: &Path,
    backup_dir: &Path,
    include_saved_variables: bool,
    timestamp: &str,
) -> Result<ManualBackupResult, ManualBackupError> {
    validate_backup_dir_location(backup_dir, addons_dir)?;
    let addons_dir = prepare_addons_dir(addons_dir)?;
    validate_backup_dir_location(backup_dir, &addons_dir)?;
    prepare_backup_dir(backup_dir)?;
    let backup_dir = fs::canonicalize(backup_dir)?;
    validate_backup_dir_location(&backup_dir, &addons_dir)?;

    let backup_path = create_unique_backup_folder(&backup_dir, timestamp)?;
    let mut stats = CopyStats::default();

    debug!("creating manual AddOns backup at {:?}", backup_path);
    let addon_stats = copy_dir_no_symlinks(&addons_dir, &backup_path.join("AddOns"))?;
    stats.add(addon_stats);

    let saved_variables_dir = saved_variables_dir_for_addons(&addons_dir);
    let mut copied_saved_variables = false;
    let mut saved_variables_missing = include_saved_variables;

    if include_saved_variables {
        match source_dir_status(&saved_variables_dir)? {
            SourceDirStatus::Directory => {
                debug!(
                    "copying SavedVariables into manual backup {:?}",
                    backup_path
                );
                let saved_variables_stats = copy_dir_no_symlinks(
                    &saved_variables_dir,
                    &backup_path.join("SavedVariables"),
                )?;
                stats.add(saved_variables_stats);
                copied_saved_variables = true;
                saved_variables_missing = false;
            }
            SourceDirStatus::Missing => {
                saved_variables_missing = true;
            }
        }
    }

    Ok(ManualBackupResult {
        backup_path,
        copied_addons: true,
        copied_saved_variables,
        saved_variables_missing,
        total_files: stats.files,
        total_bytes: stats.bytes,
    })
}

fn prepare_addons_dir(addons_dir: &Path) -> Result<PathBuf, ManualBackupError> {
    match fs::symlink_metadata(addons_dir) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(ManualBackupError::Symlink(addons_dir.to_path_buf()))
        }
        Ok(metadata) if metadata.is_dir() => Ok(fs::canonicalize(addons_dir)?),
        Ok(_) => Err(ManualBackupError::AddonsPathNotDirectory(
            addons_dir.to_path_buf(),
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Err(
            ManualBackupError::MissingAddonsDir(addons_dir.to_path_buf()),
        ),
        Err(error) => Err(ManualBackupError::Io(error)),
    }
}

fn prepare_backup_dir(backup_dir: &Path) -> Result<(), ManualBackupError> {
    match fs::symlink_metadata(backup_dir) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(ManualBackupError::Symlink(backup_dir.to_path_buf()))
        }
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(ManualBackupError::BackupDirNotDirectory(
            backup_dir.to_path_buf(),
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir_all(backup_dir)?;
            Ok(())
        }
        Err(error) => Err(ManualBackupError::Io(error)),
    }
}

fn validate_backup_dir_location(
    backup_dir: &Path,
    addons_dir: &Path,
) -> Result<(), ManualBackupError> {
    validate_backup_against_source(
        &absolute_normalized(backup_dir)?,
        &absolute_normalized(addons_dir)?,
    )?;

    validate_backup_against_source(
        &absolute_normalized_existing_prefix(backup_dir)?,
        &absolute_normalized_existing_prefix(addons_dir)?,
    )
}

fn validate_backup_against_source(
    backup_dir: &Path,
    addons_dir: &Path,
) -> Result<(), ManualBackupError> {
    if is_same_or_child(backup_dir, addons_dir) {
        return Err(ManualBackupError::BackupDirInsideAddons(
            backup_dir.to_path_buf(),
        ));
    }

    if let Some(live_dir) = addons_dir.parent() {
        let saved_variables_dir = live_dir.join("SavedVariables");
        if is_same_or_child(backup_dir, &saved_variables_dir) {
            return Err(ManualBackupError::BackupDirInsideSavedVariables(
                backup_dir.to_path_buf(),
            ));
        }
    }

    Ok(())
}

fn create_unique_backup_folder(
    backup_dir: &Path,
    timestamp: &str,
) -> Result<PathBuf, ManualBackupError> {
    let base_name = format!("Scribe-Backup-{timestamp}");
    for suffix in 0.. {
        let name = if suffix == 0 {
            base_name.clone()
        } else {
            format!("{base_name}-{suffix}")
        };
        let candidate = backup_dir.join(name);
        match fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(ManualBackupError::Io(error)),
        }
    }

    unreachable!("unbounded suffix search should eventually find a free path")
}

enum SourceDirStatus {
    Directory,
    Missing,
}

fn source_dir_status(path: &Path) -> Result<SourceDirStatus, ManualBackupError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(ManualBackupError::Symlink(path.to_path_buf()))
        }
        Ok(metadata) if metadata.is_dir() => Ok(SourceDirStatus::Directory),
        Ok(_) => Ok(SourceDirStatus::Missing),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(SourceDirStatus::Missing),
        Err(error) => Err(ManualBackupError::Io(error)),
    }
}

fn copy_dir_no_symlinks(source: &Path, target: &Path) -> Result<CopyStats, ManualBackupError> {
    match source_dir_status(source)? {
        SourceDirStatus::Directory => {}
        SourceDirStatus::Missing => {
            return Err(ManualBackupError::MissingAddonsDir(source.to_path_buf()));
        }
    }

    fs::create_dir_all(target)?;
    let mut stats = CopyStats::default();

    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        let metadata = fs::symlink_metadata(&source_path)?;

        if metadata.file_type().is_symlink() {
            return Err(ManualBackupError::Symlink(source_path));
        }

        if metadata.is_dir() {
            stats.add(copy_dir_no_symlinks(&source_path, &target_path)?);
        } else if metadata.is_file() {
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&source_path, &target_path)?;
            stats.files += 1;
            stats.bytes += metadata.len();
        }
    }

    Ok(stats)
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

fn absolute_normalized(path: &Path) -> Result<PathBuf, ManualBackupError> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    Ok(normalize_components(&absolute))
}

fn absolute_normalized_existing_prefix(path: &Path) -> Result<PathBuf, ManualBackupError> {
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use tempfile::tempdir;

    use crate::install::backup::{create_manual_backup_with_timestamp, ManualBackupError};

    const TIMESTAMP: &str = "2026-05-16-10-11-12";

    #[test]
    fn backup_addons_only_copies_addons() {
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
            create_manual_backup_with_timestamp(&addons_dir, &backup_dir, false, TIMESTAMP)
                .unwrap();

        assert!(result.copied_addons);
        assert!(!result.copied_saved_variables);
        assert!(!result.saved_variables_missing);
        assert_eq!(result.total_files, 1);
        assert_eq!(
            fs::read_to_string(
                result
                    .backup_path
                    .join("AddOns/SampleAddon/SampleAddon.txt")
            )
            .unwrap(),
            "addon"
        );
        assert!(!result.backup_path.join("SavedVariables").exists());
    }

    #[test]
    fn backup_with_saved_variables_copies_both_when_present() {
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
            create_manual_backup_with_timestamp(&addons_dir, &backup_dir, true, TIMESTAMP).unwrap();

        assert!(result.copied_addons);
        assert!(result.copied_saved_variables);
        assert!(!result.saved_variables_missing);
        assert_eq!(result.total_files, 2);
        assert_eq!(
            fs::read_to_string(
                result
                    .backup_path
                    .join("AddOns/SampleAddon/SampleAddon.txt")
            )
            .unwrap(),
            "addon"
        );
        assert_eq!(
            fs::read_to_string(result.backup_path.join("SavedVariables/SampleAddon.lua")).unwrap(),
            "saved"
        );
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
            create_manual_backup_with_timestamp(&addons_dir, &backup_dir, true, TIMESTAMP).unwrap();

        assert!(result.copied_addons);
        assert!(!result.copied_saved_variables);
        assert!(result.saved_variables_missing);
        assert!(result.backup_path.join("AddOns").is_dir());
        assert!(!result.backup_path.join("SavedVariables").exists());
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
            create_manual_backup_with_timestamp(&addons_dir, &backup_dir, false, TIMESTAMP)
                .unwrap();

        assert!(backup_dir.is_dir());
        assert!(result.backup_path.is_dir());
    }

    #[test]
    fn timestamp_collision_gets_suffix() {
        let dir = tempdir().unwrap();
        let addons_dir = dir.path().join("AddOns");
        let backup_dir = dir.path().join("Backups");
        write_file(
            &addons_dir.join("SampleAddon").join("SampleAddon.txt"),
            "addon",
        );
        fs::create_dir_all(backup_dir.join(format!("Scribe-Backup-{TIMESTAMP}"))).unwrap();

        let result =
            create_manual_backup_with_timestamp(&addons_dir, &backup_dir, false, TIMESTAMP)
                .unwrap();

        assert_eq!(
            result.backup_path.file_name().unwrap().to_string_lossy(),
            format!("Scribe-Backup-{TIMESTAMP}-1")
        );
    }

    #[test]
    fn backup_target_inside_addons_is_refused() {
        let dir = tempdir().unwrap();
        let addons_dir = dir.path().join("AddOns");
        let backup_dir = addons_dir.join("Backups");
        write_file(
            &addons_dir.join("SampleAddon").join("SampleAddon.txt"),
            "addon",
        );

        let error = create_manual_backup_with_timestamp(&addons_dir, &backup_dir, false, TIMESTAMP)
            .unwrap_err();

        assert!(matches!(error, ManualBackupError::BackupDirInsideAddons(_)));
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

        let error = create_manual_backup_with_timestamp(&addons_dir, &backup_dir, false, TIMESTAMP)
            .unwrap_err();

        assert!(matches!(error, ManualBackupError::BackupDirInsideAddons(_)));
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

        create_manual_backup_with_timestamp(&addons_dir, &backup_dir, true, TIMESTAMP).unwrap();

        assert_eq!(
            fs::read_to_string(addons_dir.join("SampleAddon/SampleAddon.txt")).unwrap(),
            "addon"
        );
        assert_eq!(
            fs::read_to_string(saved_variables.join("SampleAddon.lua")).unwrap(),
            "saved"
        );
    }

    fn write_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }
}
