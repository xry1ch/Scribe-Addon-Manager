use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use thiserror::Error;
use tracing::debug;

use crate::install::plan::{InstallPlan, InstallPlanAction, InstallPlanItem};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallActionPerformed {
    InstalledNew,
    ReplacedExisting,
    Skipped,
}

impl InstallActionPerformed {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InstalledNew => "installed-new",
            Self::ReplacedExisting => "replaced-existing",
            Self::Skipped => "skipped",
        }
    }
}

#[derive(Debug, Clone)]
pub struct InstalledItem {
    pub source_folder: Option<String>,
    pub target_folder: Option<PathBuf>,
    pub backup_folder: Option<PathBuf>,
    pub action: InstallActionPerformed,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct InstallResult {
    pub items: Vec<InstalledItem>,
    pub backup_dir: Option<PathBuf>,
    pub installed_new: usize,
    pub replaced: usize,
    pub skipped: usize,
}

#[derive(Debug, Error)]
pub enum InstallApplyError {
    #[error("install plan has no valid addon folders to install")]
    NoValidAddonFolders,

    #[error("planned item is missing source folder")]
    MissingSourceFolder,

    #[error("planned item is missing target folder")]
    MissingTargetFolder,

    #[error("source addon folder does not exist: {0}")]
    MissingSource(PathBuf),

    #[error("refusing to follow symlink: {0}")]
    Symlink(PathBuf),

    #[error("target already exists and was not planned as a replacement: {0}")]
    TargetExists(PathBuf),

    #[error("unsafe source folder name rejected: {0}")]
    UnsafeSourceFolder(String),

    #[error("planned target path escapes AddOns directory: {0}")]
    TargetEscapesAddonsDir(PathBuf),

    #[error("copy failed after backup; target may need manual restore from {backup}: {source}")]
    CopyFailedAfterBackup {
        backup: PathBuf,
        #[source]
        source: Box<InstallApplyError>,
    },

    #[error("filesystem error: {0}")]
    Io(#[from] io::Error),
}

pub fn apply_install_plan(
    plan: &InstallPlan,
    backup_root: Option<&Path>,
) -> Result<InstallResult, InstallApplyError> {
    let actionable = plan
        .items
        .iter()
        .filter(|item| {
            matches!(
                item.action,
                InstallPlanAction::WouldInstallNew | InstallPlanAction::WouldReplaceExisting
            )
        })
        .collect::<Vec<_>>();

    if actionable.is_empty() {
        return Err(InstallApplyError::NoValidAddonFolders);
    }

    fs::create_dir_all(&plan.addons_dir)?;

    let backup_session_dir = if actionable
        .iter()
        .any(|item| matches!(item.action, InstallPlanAction::WouldReplaceExisting))
    {
        Some(create_backup_session_dir(&plan.addons_dir, backup_root)?)
    } else {
        None
    };

    let mut result = InstallResult {
        backup_dir: backup_session_dir.clone(),
        ..InstallResult::default()
    };

    for item in &plan.items {
        match item.action {
            InstallPlanAction::WouldInstallNew => {
                let installed = install_new(plan, item)?;
                result.installed_new += 1;
                result.items.push(installed);
            }
            InstallPlanAction::WouldReplaceExisting => {
                let backup_dir = backup_session_dir
                    .as_ref()
                    .expect("replacement actions create backup dir");
                let replaced = replace_existing(plan, item, backup_dir)?;
                result.replaced += 1;
                result.items.push(replaced);
            }
            _ => {
                result.skipped += 1;
                result.items.push(InstalledItem {
                    source_folder: item.source_folder.clone(),
                    target_folder: item.target_folder.clone(),
                    backup_folder: None,
                    action: InstallActionPerformed::Skipped,
                    message: Some(item.action.as_str().to_owned()),
                });
            }
        }
    }

    Ok(result)
}

fn install_new(
    plan: &InstallPlan,
    item: &InstallPlanItem,
) -> Result<InstalledItem, InstallApplyError> {
    let source_folder = item
        .source_folder
        .as_ref()
        .ok_or(InstallApplyError::MissingSourceFolder)?;
    validate_single_component(source_folder)
        .map_err(|_| InstallApplyError::UnsafeSourceFolder(source_folder.clone()))?;
    let source = plan.temp_dir.join(source_folder);
    let target = item
        .target_folder
        .as_ref()
        .ok_or(InstallApplyError::MissingTargetFolder)?;
    validate_target_in_addons_dir(&plan.addons_dir, target)?;

    if target.exists() {
        return Err(InstallApplyError::TargetExists(target.clone()));
    }

    debug!("copying new addon {:?} to {:?}", source, target);
    copy_dir_no_symlinks(&source, target)?;

    Ok(InstalledItem {
        source_folder: Some(source_folder.clone()),
        target_folder: Some(target.clone()),
        backup_folder: None,
        action: InstallActionPerformed::InstalledNew,
        message: None,
    })
}

fn replace_existing(
    plan: &InstallPlan,
    item: &InstallPlanItem,
    backup_session_dir: &Path,
) -> Result<InstalledItem, InstallApplyError> {
    let source_folder = item
        .source_folder
        .as_ref()
        .ok_or(InstallApplyError::MissingSourceFolder)?;
    validate_single_component(source_folder)
        .map_err(|_| InstallApplyError::UnsafeSourceFolder(source_folder.clone()))?;
    let source = plan.temp_dir.join(source_folder);
    let target = item
        .target_folder
        .as_ref()
        .ok_or(InstallApplyError::MissingTargetFolder)?;
    validate_target_in_addons_dir(&plan.addons_dir, target)?;
    let backup = unique_backup_path(backup_session_dir, source_folder);

    debug!("backing up existing addon {:?} to {:?}", target, backup);
    if let Some(parent) = backup.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(target, &backup)?;

    debug!("copying replacement addon {:?} to {:?}", source, target);
    if let Err(error) = copy_dir_no_symlinks(&source, target) {
        return Err(InstallApplyError::CopyFailedAfterBackup {
            backup,
            source: Box::new(error),
        });
    }

    Ok(InstalledItem {
        source_folder: Some(source_folder.clone()),
        target_folder: Some(target.clone()),
        backup_folder: Some(backup),
        action: InstallActionPerformed::ReplacedExisting,
        message: None,
    })
}

fn copy_dir_no_symlinks(source: &Path, target: &Path) -> Result<(), InstallApplyError> {
    let metadata = fs::symlink_metadata(source)?;
    if metadata.file_type().is_symlink() {
        return Err(InstallApplyError::Symlink(source.to_path_buf()));
    }
    if !metadata.is_dir() {
        return Err(InstallApplyError::MissingSource(source.to_path_buf()));
    }

    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        let metadata = fs::symlink_metadata(&source_path)?;
        let file_type = metadata.file_type();

        if file_type.is_symlink() {
            return Err(InstallApplyError::Symlink(source_path));
        }

        if metadata.is_dir() {
            copy_dir_no_symlinks(&source_path, &target_path)?;
        } else if metadata.is_file() {
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&source_path, &target_path)?;
        }
    }

    Ok(())
}

fn validate_target_in_addons_dir(
    addons_dir: &Path,
    target: &Path,
) -> Result<(), InstallApplyError> {
    if !target.starts_with(addons_dir) {
        return Err(InstallApplyError::TargetEscapesAddonsDir(
            target.to_path_buf(),
        ));
    }

    Ok(())
}

fn validate_single_component(value: &str) -> Result<(), ()> {
    let path = Path::new(value);
    let mut components = path.components();
    match components.next() {
        Some(Component::Normal(_)) if components.next().is_none() => Ok(()),
        _ => Err(()),
    }
}

fn create_backup_session_dir(
    addons_dir: &Path,
    backup_root: Option<&Path>,
) -> Result<PathBuf, InstallApplyError> {
    let root = backup_root.map(Path::to_path_buf).unwrap_or_else(|| {
        addons_dir
            .parent()
            .map(|parent| parent.join(".eso-addon-manager-backups"))
            .unwrap_or_else(|| PathBuf::from(".eso-addon-manager-backups"))
    });
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let session_dir = unique_path(&root.join(timestamp.to_string()));

    fs::create_dir_all(&session_dir)?;
    Ok(session_dir)
}

fn unique_backup_path(backup_session_dir: &Path, folder_name: &str) -> PathBuf {
    unique_path(&backup_session_dir.join(folder_name))
}

fn unique_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }

    for suffix in 1.. {
        let candidate = path.with_file_name(format!(
            "{}-{suffix}",
            path.file_name()
                .map(|name| name.to_string_lossy())
                .unwrap_or_default()
        ));
        if !candidate.exists() {
            return candidate;
        }
    }

    unreachable!("unbounded suffix search should eventually find a free path")
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use tempfile::tempdir;

    use crate::install::apply::{apply_install_plan, InstallActionPerformed, InstallApplyError};
    use crate::install::plan::{InstallPlan, InstallPlanAction, InstallPlanItem};

    fn plan(
        temp_dir: PathBuf,
        addons_dir: PathBuf,
        source_folder: &str,
        action: InstallPlanAction,
    ) -> InstallPlan {
        InstallPlan {
            addons_dir: addons_dir.clone(),
            temp_dir,
            items: vec![InstallPlanItem {
                source_folder: Some(source_folder.to_owned()),
                title: Some(source_folder.to_owned()),
                version: Some("1".to_owned()),
                target_folder: Some(addons_dir.join(source_folder)),
                action,
            }],
        }
    }

    fn write_addon(root: &Path, folder: &str, marker: &str) {
        let folder_path = root.join(folder);
        fs::create_dir_all(&folder_path).unwrap();
        fs::write(
            folder_path.join(format!("{folder}.txt")),
            "## Title: Test\n",
        )
        .unwrap();
        fs::write(folder_path.join("marker.txt"), marker).unwrap();
    }

    #[test]
    fn install_new_addon_into_temp_addons_directory() {
        let dir = tempdir().unwrap();
        let temp = dir.path().join("temp");
        let addons = dir.path().join("AddOns");
        write_addon(&temp, "NewAddon", "new");

        let result = apply_install_plan(
            &plan(
                temp.clone(),
                addons.clone(),
                "NewAddon",
                InstallPlanAction::WouldInstallNew,
            ),
            None,
        )
        .unwrap();

        assert_eq!(result.installed_new, 1);
        assert_eq!(
            fs::read_to_string(addons.join("NewAddon").join("marker.txt")).unwrap(),
            "new"
        );
    }

    #[test]
    fn replace_existing_addon_and_create_backup() {
        let dir = tempdir().unwrap();
        let temp = dir.path().join("temp");
        let addons = dir.path().join("AddOns");
        write_addon(&temp, "Addon", "new");
        write_addon(&addons, "Addon", "old");

        let result = apply_install_plan(
            &plan(
                temp.clone(),
                addons.clone(),
                "Addon",
                InstallPlanAction::WouldReplaceExisting,
            ),
            None,
        )
        .unwrap();

        assert_eq!(result.replaced, 1);
        assert_eq!(
            result.items[0].action,
            InstallActionPerformed::ReplacedExisting
        );
        assert_eq!(
            fs::read_to_string(addons.join("Addon").join("marker.txt")).unwrap(),
            "new"
        );
        let backup = result.items[0].backup_folder.as_ref().unwrap();
        assert_eq!(
            fs::read_to_string(backup.join("marker.txt")).unwrap(),
            "old"
        );
    }

    #[test]
    fn dry_confirmation_path_makes_no_changes() {
        let dir = tempdir().unwrap();
        let temp = dir.path().join("temp");
        let addons = dir.path().join("AddOns");
        write_addon(&temp, "NewAddon", "new");

        let _plan = plan(
            temp.clone(),
            addons.clone(),
            "NewAddon",
            InstallPlanAction::WouldInstallNew,
        );

        assert!(!addons.join("NewAddon").exists());
    }

    #[test]
    fn invalid_manifest_is_skipped_and_not_installed() {
        let dir = tempdir().unwrap();
        let temp = dir.path().join("temp");
        let addons = dir.path().join("AddOns");
        write_addon(&temp, "InvalidAddon", "new");
        let plan = plan(
            temp,
            addons.clone(),
            "InvalidAddon",
            InstallPlanAction::WouldSkipInvalidManifest,
        );

        assert!(matches!(
            apply_install_plan(&plan, None),
            Err(InstallApplyError::NoValidAddonFolders)
        ));
        assert!(!addons.join("InvalidAddon").exists());
    }

    #[test]
    fn no_valid_addon_folders_returns_error() {
        let plan = InstallPlan {
            addons_dir: PathBuf::from("AddOns"),
            temp_dir: PathBuf::from("temp"),
            items: vec![InstallPlanItem {
                source_folder: None,
                title: None,
                version: None,
                target_folder: None,
                action: InstallPlanAction::WouldSkipNoAddonFolders,
            }],
        };

        assert!(matches!(
            apply_install_plan(&plan, None),
            Err(InstallApplyError::NoValidAddonFolders)
        ));
    }

    #[test]
    fn backup_failure_aborts_replacement() {
        let dir = tempdir().unwrap();
        let temp = dir.path().join("temp");
        let addons = dir.path().join("AddOns");
        let backup_root = dir.path().join("backup-root-file");
        write_addon(&temp, "Addon", "new");
        write_addon(&addons, "Addon", "old");
        fs::write(&backup_root, "not a directory").unwrap();

        let result = apply_install_plan(
            &plan(
                temp,
                addons.clone(),
                "Addon",
                InstallPlanAction::WouldReplaceExisting,
            ),
            Some(&backup_root),
        );

        assert!(result.is_err());
        assert_eq!(
            fs::read_to_string(addons.join("Addon").join("marker.txt")).unwrap(),
            "old"
        );
    }

    #[test]
    fn target_containment_is_enforced() {
        let dir = tempdir().unwrap();
        let temp = dir.path().join("temp");
        let addons = dir.path().join("AddOns");
        write_addon(&temp, "Addon", "new");
        let plan = InstallPlan {
            addons_dir: addons.clone(),
            temp_dir: temp,
            items: vec![InstallPlanItem {
                source_folder: Some("Addon".to_owned()),
                title: None,
                version: None,
                target_folder: Some(dir.path().join("outside")),
                action: InstallPlanAction::WouldInstallNew,
            }],
        };

        let result = apply_install_plan(&plan, None);

        assert!(matches!(
            result,
            Err(InstallApplyError::TargetEscapesAddonsDir(_))
        ));
        assert!(!addons.join("Addon").exists());
        assert!(!dir.path().join("outside").exists());
    }

    #[test]
    fn no_unrelated_files_are_copied() {
        let dir = tempdir().unwrap();
        let temp = dir.path().join("temp");
        let addons = dir.path().join("AddOns");
        write_addon(&temp, "Addon", "new");
        fs::write(temp.join("root-file.txt"), "ignore").unwrap();

        apply_install_plan(
            &plan(
                temp,
                addons.clone(),
                "Addon",
                InstallPlanAction::WouldInstallNew,
            ),
            None,
        )
        .unwrap();

        assert!(addons.join("Addon").exists());
        assert!(!addons.join("root-file.txt").exists());
    }
}
