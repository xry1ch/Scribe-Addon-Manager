use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use thiserror::Error;
use tracing::debug;

use crate::local;

#[derive(Debug, Clone)]
pub struct RemoveAddonResult {
    pub removed_addon: bool,
    pub removed_saved_variables: bool,
    pub saved_variables_deleted_count: usize,
    pub saved_variables_deleted_files: Vec<String>,
    pub saved_variables_missing_files: Vec<String>,
    pub addon_folder: String,
    pub original_path: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct ClearSavedVariablesResult {
    pub addon_folder: String,
    pub saved_variables_dir: PathBuf,
    pub deleted_count: usize,
    pub deleted_files: Vec<String>,
    pub missing_files: Vec<String>,
    pub status: ClearSavedVariablesStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClearSavedVariablesStatus {
    Deleted,
    MissingSavedVariablesFolder,
    NoFilesFound,
}

impl ClearSavedVariablesStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Deleted => "deleted",
            Self::MissingSavedVariablesFolder => "missing_saved_variables_folder",
            Self::NoFilesFound => "no_files_found",
        }
    }
}

#[derive(Debug, Error)]
pub enum RemoveAddonError {
    #[error("unsafe addon folder name rejected: {0}")]
    UnsafeFolderName(String),

    #[error("no installed addon matched folder {0}")]
    NoMatch(String),

    #[error("multiple installed addons matched folder {folder_name}: {matches}")]
    Ambiguous {
        folder_name: String,
        matches: String,
    },

    #[error("addon folder is missing: {0}")]
    MissingTarget(PathBuf),

    #[error("addon target path escapes AddOns directory: {0}")]
    TargetEscapesAddonsDir(PathBuf),

    #[error("refusing to remove addon containing symlink: {0}")]
    Symlink(PathBuf),

    #[error("filesystem error: {0}")]
    Io(#[from] io::Error),
}

pub fn remove_installed_addon(
    addons_dir: &Path,
    folder_name: &str,
    remove_saved_variables: bool,
) -> Result<RemoveAddonResult, RemoveAddonError> {
    let addon = resolve_installed_addon(addons_dir, folder_name)?;

    let target = contained_target_path(addons_dir, &addon.folder_name)?;
    let metadata = fs::symlink_metadata(&target)
        .map_err(|_| RemoveAddonError::MissingTarget(target.clone()))?;
    if metadata.file_type().is_symlink() {
        return Err(RemoveAddonError::Symlink(target));
    }
    if !metadata.is_dir() {
        return Err(RemoveAddonError::MissingTarget(target));
    }
    reject_symlinks_in_tree(&target)?;
    let saved_variables_plan = if remove_saved_variables {
        plan_saved_variables_removal(addons_dir, &addon)?
    } else {
        SavedVariablesRemovalPlan::default()
    };

    debug!("removing addon folder {:?}", target);
    fs::remove_dir_all(&target)?;
    let saved_variables_result = apply_saved_variables_removal(saved_variables_plan)?;

    Ok(RemoveAddonResult {
        removed_addon: true,
        removed_saved_variables: remove_saved_variables,
        saved_variables_deleted_count: saved_variables_result.deleted_files.len(),
        saved_variables_deleted_files: saved_variables_result.deleted_files,
        saved_variables_missing_files: saved_variables_result.missing_files,
        addon_folder: addon.folder_name.clone(),
        original_path: target,
        message: if remove_saved_variables {
            "Addon and SavedVariables removed.".to_owned()
        } else {
            "Addon uninstalled. SavedVariables were kept.".to_owned()
        },
    })
}

pub fn clear_saved_variables(
    addons_dir: &Path,
    folder_name: &str,
) -> Result<ClearSavedVariablesResult, RemoveAddonError> {
    let addon = resolve_installed_addon(addons_dir, folder_name)?;
    let plan = plan_saved_variables_removal(addons_dir, &addon)?;
    let saved_variables_dir = plan.saved_variables_dir.clone();
    let saved_variables_dir_missing = plan.saved_variables_dir_missing;
    let result = apply_saved_variables_removal(plan)?;
    let deleted_count = result.deleted_files.len();
    let status = if deleted_count > 0 {
        ClearSavedVariablesStatus::Deleted
    } else if saved_variables_dir_missing {
        ClearSavedVariablesStatus::MissingSavedVariablesFolder
    } else {
        ClearSavedVariablesStatus::NoFilesFound
    };

    Ok(ClearSavedVariablesResult {
        addon_folder: addon.folder_name,
        saved_variables_dir,
        deleted_count,
        deleted_files: result.deleted_files,
        missing_files: result.missing_files,
        status,
    })
}

pub fn resolve_installed_addon(
    addons_dir: &Path,
    folder_name: &str,
) -> Result<local::LocalAddon, RemoveAddonError> {
    let requested = folder_name.trim();
    validate_single_component(requested)
        .map_err(|_| RemoveAddonError::UnsafeFolderName(folder_name.to_owned()))?;

    let installed_addons = local::scan_addons_dir(addons_dir)?;
    let matches = installed_addons
        .into_iter()
        .filter(|addon| addon.folder_name.eq_ignore_ascii_case(requested))
        .collect::<Vec<_>>();

    match matches.as_slice() {
        [] => Err(RemoveAddonError::NoMatch(requested.to_owned())),
        [addon] => Ok(addon.clone()),
        matches => {
            let matches = matches
                .iter()
                .map(|addon| addon.folder_name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            Err(RemoveAddonError::Ambiguous {
                folder_name: requested.to_owned(),
                matches,
            })
        }
    }
}

fn reject_symlinks_in_tree(path: &Path) -> Result<(), RemoveAddonError> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let entry_path = entry.path();
        let metadata = fs::symlink_metadata(&entry_path)?;
        let file_type = metadata.file_type();

        if file_type.is_symlink() {
            return Err(RemoveAddonError::Symlink(entry_path));
        }

        if metadata.is_dir() {
            reject_symlinks_in_tree(&entry_path)?;
        }
    }

    Ok(())
}

fn contained_target_path(
    addons_dir: &Path,
    folder_name: &str,
) -> Result<PathBuf, RemoveAddonError> {
    validate_single_component(folder_name)
        .map_err(|_| RemoveAddonError::UnsafeFolderName(folder_name.to_owned()))?;
    let target = addons_dir.join(folder_name);
    if !target.starts_with(addons_dir) {
        return Err(RemoveAddonError::TargetEscapesAddonsDir(target));
    }
    Ok(target)
}

fn validate_single_component(value: &str) -> Result<(), ()> {
    let path = Path::new(value);
    let mut components = path.components();
    match components.next() {
        Some(Component::Normal(_)) if components.next().is_none() => Ok(()),
        _ => Err(()),
    }
}

#[derive(Debug, Default)]
struct SavedVariablesRemovalPlan {
    saved_variables_dir: PathBuf,
    saved_variables_dir_missing: bool,
    existing_files: Vec<(String, PathBuf)>,
    missing_files: Vec<String>,
}

#[derive(Debug, Default)]
struct SavedVariablesRemovalResult {
    deleted_files: Vec<String>,
    missing_files: Vec<String>,
}

fn plan_saved_variables_removal(
    addons_dir: &Path,
    addon: &local::LocalAddon,
) -> Result<SavedVariablesRemovalPlan, RemoveAddonError> {
    let saved_variables_dir = addons_dir
        .parent()
        .map(|parent| parent.join("SavedVariables"))
        .unwrap_or_else(|| PathBuf::from("SavedVariables"));
    let file_names = saved_variables_file_names(addon);
    let mut plan = SavedVariablesRemovalPlan {
        saved_variables_dir: saved_variables_dir.clone(),
        ..SavedVariablesRemovalPlan::default()
    };

    if !saved_variables_dir.exists() {
        plan.saved_variables_dir_missing = true;
        plan.missing_files = file_names.into_iter().collect();
        return Ok(plan);
    }

    let metadata = fs::symlink_metadata(&saved_variables_dir)?;
    if metadata.file_type().is_symlink() {
        return Err(RemoveAddonError::Symlink(saved_variables_dir));
    }
    if !metadata.is_dir() {
        plan.missing_files = file_names.into_iter().collect();
        return Ok(plan);
    }

    for file_name in file_names {
        let target = contained_saved_variables_file(&saved_variables_dir, &file_name)?;
        let Ok(metadata) = fs::symlink_metadata(&target) else {
            plan.missing_files.push(file_name);
            continue;
        };

        if metadata.is_file() {
            plan.existing_files.push((file_name, target));
        } else {
            plan.missing_files.push(file_name);
        }
    }

    Ok(plan)
}

fn apply_saved_variables_removal(
    plan: SavedVariablesRemovalPlan,
) -> Result<SavedVariablesRemovalResult, RemoveAddonError> {
    let mut result = SavedVariablesRemovalResult {
        missing_files: plan.missing_files,
        ..SavedVariablesRemovalResult::default()
    };

    for (file_name, path) in plan.existing_files {
        fs::remove_file(path)?;
        result.deleted_files.push(file_name);
    }

    Ok(result)
}

fn saved_variables_file_names(addon: &local::LocalAddon) -> BTreeSet<String> {
    let mut names = BTreeSet::new();

    add_saved_variable_file_names(&mut names, &addon.folder_name);
    for value in addon
        .saved_variables
        .iter()
        .chain(addon.saved_variables_per_character.iter())
    {
        add_saved_variable_file_names(&mut names, value);
    }

    names
}

fn add_saved_variable_file_names(names: &mut BTreeSet<String>, value: &str) {
    let value = value.trim();
    if validate_single_component(value).is_err() {
        return;
    }
    names.insert(format!("{value}.lua"));
    names.insert(format!("{value}.lua.bak"));
}

fn contained_saved_variables_file(
    saved_variables_dir: &Path,
    file_name: &str,
) -> Result<PathBuf, RemoveAddonError> {
    validate_single_component(file_name)
        .map_err(|_| RemoveAddonError::UnsafeFolderName(file_name.to_owned()))?;
    let target = saved_variables_dir.join(file_name);
    if !target.starts_with(saved_variables_dir) {
        return Err(RemoveAddonError::TargetEscapesAddonsDir(target));
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io;
    use std::path::Path;

    use tempfile::tempdir;

    use crate::install::remove::{
        clear_saved_variables, remove_installed_addon, ClearSavedVariablesStatus, RemoveAddonError,
    };

    fn write_addon(addons_dir: &Path, folder_name: &str) {
        write_addon_manifest(
            addons_dir,
            folder_name,
            &format!("## Title: {folder_name}\n## Version: 1\n"),
        );
    }

    fn write_addon_manifest(addons_dir: &Path, folder_name: &str, manifest: &str) {
        let folder = addons_dir.join(folder_name);
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join(format!("{folder_name}.txt")), manifest).unwrap();
        fs::write(folder.join("marker.txt"), "installed").unwrap();
    }

    #[test]
    fn remove_installed_addon_deletes_folder_from_addons() {
        let dir = tempdir().unwrap();
        let addons_dir = dir.path().join("AddOns");
        write_addon(&addons_dir, "SampleAddon");

        let result = remove_installed_addon(&addons_dir, "SampleAddon", false).unwrap();

        assert!(result.removed_addon);
        assert_eq!(result.addon_folder, "SampleAddon");
        assert!(!addons_dir.join("SampleAddon").exists());
        assert_eq!(result.original_path, addons_dir.join("SampleAddon"));
    }

    #[test]
    fn no_match_errors() {
        let dir = tempdir().unwrap();
        let addons_dir = dir.path().join("AddOns");
        fs::create_dir_all(&addons_dir).unwrap();

        let error = remove_installed_addon(&addons_dir, "MissingAddon", false).unwrap_err();

        assert!(matches!(error, RemoveAddonError::NoMatch(_)));
    }

    #[test]
    fn target_containment_is_enforced() {
        let dir = tempdir().unwrap();
        let addons_dir = dir.path().join("AddOns");
        write_addon(&addons_dir, "SampleAddon");

        let error = remove_installed_addon(&addons_dir, "../SampleAddon", false).unwrap_err();

        assert!(matches!(error, RemoveAddonError::UnsafeFolderName(_)));
        assert!(addons_dir.join("SampleAddon").exists());
    }

    #[test]
    fn no_backup_folder_is_created_by_remove() {
        let dir = tempdir().unwrap();
        let addons_dir = dir.path().join("AddOns");
        let backup_dir = dir.path().join("Backups");
        write_addon(&addons_dir, "SampleAddon");

        remove_installed_addon(&addons_dir, "SampleAddon", false).unwrap();

        assert!(!backup_dir.exists());
        assert!(!dir.path().join(".scribe-addon-manager").exists());
    }

    #[test]
    fn saved_variables_are_not_touched() {
        let dir = tempdir().unwrap();
        let addons_dir = dir.path().join("AddOns");
        let saved_variables = dir.path().join("SavedVariables");
        write_addon(&addons_dir, "SampleAddon");
        fs::create_dir_all(&saved_variables).unwrap();
        fs::write(saved_variables.join("SampleAddon.lua"), "saved").unwrap();

        let result = remove_installed_addon(&addons_dir, "SampleAddon", false).unwrap();

        assert!(!result.removed_saved_variables);
        assert_eq!(result.saved_variables_deleted_count, 0);
        assert_eq!(
            result.message,
            "Addon uninstalled. SavedVariables were kept."
        );
        assert_eq!(
            fs::read_to_string(saved_variables.join("SampleAddon.lua")).unwrap(),
            "saved"
        );
    }

    #[test]
    fn remove_saved_variables_true_deletes_exact_declared_folder_and_bak_files() {
        let dir = tempdir().unwrap();
        let addons_dir = dir.path().join("AddOns");
        let saved_variables = dir.path().join("SavedVariables");
        write_addon_manifest(
            &addons_dir,
            "SampleAddon",
            "## Title: Sample Addon\n## SavedVariables: AccountSettings OtherAccountSettings\n## SavedVariablesPerCharacter: CharacterSettings\n",
        );
        fs::create_dir_all(&saved_variables).unwrap();
        fs::write(saved_variables.join("SampleAddon.lua"), "folder").unwrap();
        fs::write(saved_variables.join("SampleAddon.lua.bak"), "folder backup").unwrap();
        fs::write(saved_variables.join("Sample_Addon.lua"), "title").unwrap();
        fs::write(saved_variables.join("AccountSettings.lua"), "account").unwrap();
        fs::write(
            saved_variables.join("AccountSettings.lua.bak"),
            "account backup",
        )
        .unwrap();
        fs::write(saved_variables.join("CharacterSettings.lua"), "character").unwrap();
        fs::write(saved_variables.join("Unrelated.lua"), "keep").unwrap();

        let result = remove_installed_addon(&addons_dir, "SampleAddon", true).unwrap();

        assert!(result.removed_addon);
        assert!(result.removed_saved_variables);
        assert_eq!(result.saved_variables_deleted_count, 5);
        assert!(!addons_dir.join("SampleAddon").exists());
        assert!(!saved_variables.join("SampleAddon.lua").exists());
        assert!(!saved_variables.join("SampleAddon.lua.bak").exists());
        assert!(saved_variables.join("Sample_Addon.lua").is_file());
        assert!(!saved_variables.join("AccountSettings.lua").exists());
        assert!(!saved_variables.join("AccountSettings.lua.bak").exists());
        assert!(!saved_variables.join("CharacterSettings.lua").exists());
        assert_eq!(
            fs::read_to_string(saved_variables.join("Unrelated.lua")).unwrap(),
            "keep"
        );
    }

    #[test]
    fn remove_saved_variables_true_does_not_delete_directories() {
        let dir = tempdir().unwrap();
        let addons_dir = dir.path().join("AddOns");
        let saved_variables = dir.path().join("SavedVariables");
        write_addon(&addons_dir, "SampleAddon");
        fs::create_dir_all(saved_variables.join("SampleAddon.lua")).unwrap();

        let result = remove_installed_addon(&addons_dir, "SampleAddon", true).unwrap();

        assert_eq!(result.saved_variables_deleted_count, 0);
        assert!(saved_variables.join("SampleAddon.lua").is_dir());
    }

    #[test]
    fn missing_saved_variables_folder_is_handled_gracefully() {
        let dir = tempdir().unwrap();
        let addons_dir = dir.path().join("AddOns");
        write_addon(&addons_dir, "SampleAddon");

        let result = remove_installed_addon(&addons_dir, "SampleAddon", true).unwrap();

        assert!(result.removed_addon);
        assert!(result.removed_saved_variables);
        assert_eq!(result.saved_variables_deleted_count, 0);
        assert!(result.saved_variables_deleted_files.is_empty());
        assert_eq!(result.message, "Addon and SavedVariables removed.");
        assert!(!addons_dir.join("SampleAddon").exists());
    }

    #[test]
    fn remove_saved_variables_true_rejects_manifest_path_components() {
        let dir = tempdir().unwrap();
        let addons_dir = dir.path().join("AddOns");
        let saved_variables = dir.path().join("SavedVariables");
        write_addon_manifest(
            &addons_dir,
            "SampleAddon",
            "## SavedVariables: ../Outside Nested/Bad AccountSettings\n",
        );
        fs::create_dir_all(&saved_variables).unwrap();
        fs::write(saved_variables.join("AccountSettings.lua"), "account").unwrap();
        fs::write(dir.path().join("Outside.lua"), "outside").unwrap();
        fs::create_dir_all(saved_variables.join("Nested")).unwrap();
        fs::write(saved_variables.join("Nested").join("Bad.lua"), "bad").unwrap();

        let result = remove_installed_addon(&addons_dir, "SampleAddon", true).unwrap();

        assert_eq!(result.saved_variables_deleted_count, 1);
        assert!(!saved_variables.join("AccountSettings.lua").exists());
        assert_eq!(
            fs::read_to_string(dir.path().join("Outside.lua")).unwrap(),
            "outside"
        );
        assert_eq!(
            fs::read_to_string(saved_variables.join("Nested").join("Bad.lua")).unwrap(),
            "bad"
        );
    }

    #[test]
    fn clear_saved_variables_deletes_declared_and_folder_files() {
        let dir = tempdir().unwrap();
        let addons_dir = dir.path().join("AddOns");
        let saved_variables = dir.path().join("SavedVariables");
        write_addon_manifest(
            &addons_dir,
            "SampleAddon",
            "## Title: Sample Addon\n## SavedVariables: AccountSettings OtherAccountSettings\n## SavedVariablesPerCharacter: CharacterSettings\n",
        );
        fs::create_dir_all(&saved_variables).unwrap();
        fs::write(saved_variables.join("SampleAddon.lua"), "folder").unwrap();
        fs::write(saved_variables.join("SampleAddon.lua.bak"), "folder backup").unwrap();
        fs::write(saved_variables.join("AccountSettings.lua"), "account").unwrap();
        fs::write(
            saved_variables.join("OtherAccountSettings.lua.bak"),
            "other account backup",
        )
        .unwrap();
        fs::write(saved_variables.join("CharacterSettings.lua"), "character").unwrap();

        let result = clear_saved_variables(&addons_dir, "SampleAddon").unwrap();

        assert_eq!(result.addon_folder, "SampleAddon");
        assert_eq!(result.status, ClearSavedVariablesStatus::Deleted);
        assert_eq!(result.deleted_count, 5);
        assert!(addons_dir.join("SampleAddon").exists());
        assert!(!saved_variables.join("SampleAddon.lua").exists());
        assert!(!saved_variables.join("SampleAddon.lua.bak").exists());
        assert!(!saved_variables.join("AccountSettings.lua").exists());
        assert!(!saved_variables
            .join("OtherAccountSettings.lua.bak")
            .exists());
        assert!(!saved_variables.join("CharacterSettings.lua").exists());
    }

    #[test]
    fn clear_saved_variables_does_not_delete_unrelated_files_or_directories() {
        let dir = tempdir().unwrap();
        let addons_dir = dir.path().join("AddOns");
        let saved_variables = dir.path().join("SavedVariables");
        write_addon_manifest(
            &addons_dir,
            "SampleAddon",
            "## Title: Sample Addon\n## SavedVariables: AccountSettings\n",
        );
        fs::create_dir_all(&saved_variables).unwrap();
        fs::write(saved_variables.join("AccountSettings.lua"), "account").unwrap();
        fs::write(saved_variables.join("Sample_Addon.lua"), "title").unwrap();
        fs::write(saved_variables.join("AccountSettings.lua.tmp"), "temp").unwrap();
        fs::write(saved_variables.join("Unrelated.lua"), "keep").unwrap();
        fs::create_dir_all(saved_variables.join("SampleAddon.lua")).unwrap();

        let result = clear_saved_variables(&addons_dir, "SampleAddon").unwrap();

        assert_eq!(result.deleted_count, 1);
        assert!(!saved_variables.join("AccountSettings.lua").exists());
        assert!(saved_variables.join("Sample_Addon.lua").is_file());
        assert!(saved_variables.join("AccountSettings.lua.tmp").is_file());
        assert!(saved_variables.join("Unrelated.lua").is_file());
        assert!(saved_variables.join("SampleAddon.lua").is_dir());
        assert!(addons_dir.join("SampleAddon").exists());
    }

    #[test]
    fn clear_saved_variables_handles_missing_saved_variables_folder() {
        let dir = tempdir().unwrap();
        let addons_dir = dir.path().join("AddOns");
        write_addon_manifest(
            &addons_dir,
            "SampleAddon",
            "## SavedVariables: AccountSettings\n",
        );

        let result = clear_saved_variables(&addons_dir, "SampleAddon").unwrap();

        assert_eq!(
            result.status,
            ClearSavedVariablesStatus::MissingSavedVariablesFolder
        );
        assert_eq!(result.deleted_count, 0);
        assert!(result.deleted_files.is_empty());
        assert!(addons_dir.join("SampleAddon").exists());
    }

    #[test]
    fn symlink_refusal_if_supported() {
        let dir = tempdir().unwrap();
        let addons_dir = dir.path().join("AddOns");
        write_addon(&addons_dir, "SampleAddon");
        fs::write(dir.path().join("outside.txt"), "outside").unwrap();
        if create_file_symlink(
            &dir.path().join("outside.txt"),
            &addons_dir.join("SampleAddon").join("linked.txt"),
        )
        .is_err()
        {
            return;
        }

        let error = remove_installed_addon(&addons_dir, "SampleAddon", false).unwrap_err();

        assert!(matches!(error, RemoveAddonError::Symlink(_)));
        assert!(addons_dir.join("SampleAddon").exists());
    }

    #[cfg(unix)]
    fn create_file_symlink(source: &Path, link: &Path) -> io::Result<()> {
        std::os::unix::fs::symlink(source, link)
    }

    #[cfg(windows)]
    fn create_file_symlink(source: &Path, link: &Path) -> io::Result<()> {
        std::os::windows::fs::symlink_file(source, link)
    }
}
