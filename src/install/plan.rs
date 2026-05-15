use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use thiserror::Error;

use crate::install::zip_safety::ExtractedZip;
use crate::local::LocalAddon;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallPlanAction {
    WouldInstallNew,
    WouldReplaceExisting,
    WouldSkipInvalidManifest,
    WouldSkipNoAddonFolders,
    WouldWarnMultipleAddons,
}

impl InstallPlanAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::WouldInstallNew => "would-install-new",
            Self::WouldReplaceExisting => "would-replace-existing",
            Self::WouldSkipInvalidManifest => "would-skip-invalid-manifest",
            Self::WouldSkipNoAddonFolders => "would-skip-no-addon-folders",
            Self::WouldWarnMultipleAddons => "would-warn-multiple-addons",
        }
    }
}

#[derive(Debug, Clone)]
pub struct InstallPlanItem {
    pub source_folder: Option<String>,
    pub title: Option<String>,
    pub version: Option<String>,
    pub target_folder: Option<PathBuf>,
    pub action: InstallPlanAction,
}

#[derive(Debug, Clone)]
pub struct InstallPlan {
    pub addons_dir: PathBuf,
    pub temp_dir: PathBuf,
    pub items: Vec<InstallPlanItem>,
}

#[derive(Debug, Error)]
pub enum InstallPlanError {
    #[error("unsafe target folder name rejected: {0}")]
    UnsafeTargetFolder(String),

    #[error("planned target path escapes AddOns directory: {0}")]
    TargetEscapesAddonsDir(String),
}

pub fn plan_install(
    extracted: &ExtractedZip,
    addons_dir: &Path,
    installed_addons: &[LocalAddon],
) -> Result<InstallPlan, InstallPlanError> {
    let mut items = Vec::new();
    let installed_folders = installed_addons
        .iter()
        .map(|addon| addon.folder_name.to_lowercase())
        .collect::<BTreeSet<_>>();

    if extracted.detected_addons.is_empty() {
        items.push(InstallPlanItem {
            source_folder: None,
            title: None,
            version: None,
            target_folder: None,
            action: InstallPlanAction::WouldSkipNoAddonFolders,
        });
    } else {
        if extracted.detected_addons.len() > 1 {
            items.push(InstallPlanItem {
                source_folder: None,
                title: None,
                version: None,
                target_folder: None,
                action: InstallPlanAction::WouldWarnMultipleAddons,
            });
        }

        for addon in &extracted.detected_addons {
            let target_folder = contained_target_path(addons_dir, &addon.folder_name)?;
            let version = addon
                .addon_version
                .clone()
                .or_else(|| addon.version.clone());
            let action = if !addon.valid_manifest {
                InstallPlanAction::WouldSkipInvalidManifest
            } else if installed_folders.contains(&addon.folder_name.to_lowercase()) {
                InstallPlanAction::WouldReplaceExisting
            } else {
                InstallPlanAction::WouldInstallNew
            };

            items.push(InstallPlanItem {
                source_folder: Some(addon.folder_name.clone()),
                title: addon.title.clone(),
                version,
                target_folder: Some(target_folder),
                action,
            });
        }
    }

    Ok(InstallPlan {
        addons_dir: addons_dir.to_path_buf(),
        temp_dir: extracted.temp_dir.clone(),
        items,
    })
}

pub fn contained_target_path(
    addons_dir: &Path,
    folder_name: &str,
) -> Result<PathBuf, InstallPlanError> {
    let folder_path = Path::new(folder_name);
    let mut components = folder_path.components();
    let Some(Component::Normal(_)) = components.next() else {
        return Err(InstallPlanError::UnsafeTargetFolder(folder_name.to_owned()));
    };
    if components.next().is_some() {
        return Err(InstallPlanError::UnsafeTargetFolder(folder_name.to_owned()));
    }

    let target = addons_dir.join(folder_path);
    if !target.starts_with(addons_dir) {
        return Err(InstallPlanError::TargetEscapesAddonsDir(
            target.display().to_string(),
        ));
    }

    Ok(target)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use tempfile::tempdir;

    use crate::install::plan::{
        contained_target_path, plan_install, InstallPlanAction, InstallPlanError,
    };
    use crate::install::zip_safety::{ExtractedZip, ZipInspection};
    use crate::local::LocalAddon;

    fn addon(folder_name: &str, valid_manifest: bool) -> LocalAddon {
        LocalAddon {
            folder_name: folder_name.to_owned(),
            folder_path: PathBuf::from(folder_name),
            manifest_path: None,
            title: if valid_manifest {
                Some(folder_name.to_owned())
            } else {
                None
            },
            addon_version: if valid_manifest {
                Some("1".to_owned())
            } else {
                None
            },
            version: None,
            api_versions: Vec::new(),
            depends_on: Vec::new(),
            optional_depends_on: Vec::new(),
            saved_variables: Vec::new(),
            saved_variables_per_character: Vec::new(),
            is_library: None,
            author: None,
            description: None,
            valid_manifest,
        }
    }

    fn extracted(addons: Vec<LocalAddon>) -> ExtractedZip {
        ExtractedZip {
            temp_dir: PathBuf::from("/tmp/extracted"),
            inspection: ZipInspection {
                zip_path: PathBuf::from("addon.zip"),
                total_entries: 1,
                total_uncompressed_size: 1,
                top_level_items: Vec::new(),
                likely_addon_folders: Vec::new(),
            },
            detected_addons: addons,
        }
    }

    #[test]
    fn new_addon_plans_would_install_new() {
        let plan = plan_install(
            &extracted(vec![addon("NewAddon", true)]),
            Path::new("AddOns"),
            &[],
        )
        .unwrap();

        assert_eq!(plan.items[0].action, InstallPlanAction::WouldInstallNew);
    }

    #[test]
    fn existing_addon_plans_would_replace_existing() {
        let installed = vec![addon("ExistingAddon", true)];
        let plan = plan_install(
            &extracted(vec![addon("ExistingAddon", true)]),
            Path::new("AddOns"),
            &installed,
        )
        .unwrap();

        assert_eq!(
            plan.items[0].action,
            InstallPlanAction::WouldReplaceExisting
        );
    }

    #[test]
    fn invalid_manifest_plans_would_skip_invalid_manifest() {
        let plan = plan_install(
            &extracted(vec![addon("InvalidAddon", false)]),
            Path::new("AddOns"),
            &[],
        )
        .unwrap();

        assert_eq!(
            plan.items[0].action,
            InstallPlanAction::WouldSkipInvalidManifest
        );
    }

    #[test]
    fn multiple_addon_folders_emit_warning() {
        let plan = plan_install(
            &extracted(vec![addon("One", true), addon("Two", true)]),
            Path::new("AddOns"),
            &[],
        )
        .unwrap();

        assert_eq!(
            plan.items[0].action,
            InstallPlanAction::WouldWarnMultipleAddons
        );
        assert_eq!(plan.items.len(), 3);
    }

    #[test]
    fn target_path_containment_is_enforced() {
        assert!(matches!(
            contained_target_path(Path::new("AddOns"), "../evil"),
            Err(InstallPlanError::UnsafeTargetFolder(_))
        ));
        assert!(matches!(
            contained_target_path(Path::new("AddOns"), "nested/evil"),
            Err(InstallPlanError::UnsafeTargetFolder(_))
        ));
    }

    #[test]
    fn no_real_addons_files_are_modified() {
        let dir = tempdir().unwrap();
        let addons_dir = dir.path().join("AddOns");
        let existing_dir = addons_dir.join("ExistingAddon");
        fs::create_dir_all(&existing_dir).unwrap();
        let sentinel = existing_dir.join("sentinel.txt");
        fs::write(&sentinel, "keep me").unwrap();

        let installed = vec![addon("ExistingAddon", true)];
        let plan = plan_install(
            &extracted(vec![addon("ExistingAddon", true)]),
            &addons_dir,
            &installed,
        )
        .unwrap();

        assert_eq!(
            plan.items[0].action,
            InstallPlanAction::WouldReplaceExisting
        );
        assert_eq!(fs::read_to_string(&sentinel).unwrap(), "keep me");
    }
}
