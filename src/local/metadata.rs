use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::warn;

use crate::api::models::AddonDetails;
use crate::install::apply::{InstallActionPerformed, InstallResult, InstalledItem};
use crate::install::dependencies::InstalledRemoteAddon;
use crate::install::plan::{InstallPlan, InstallPlanItem};
use crate::install::remote;

pub const INSTALLED_METADATA_SCHEMA_VERSION: u32 = 1;
pub const INSTALLED_BY_REMOTE_INSTALL: &str = "remote-install";
pub const INSTALLED_BY_REMOTE_UPDATE: &str = "remote-update";
pub const INSTALLED_BY_DEPENDENCY_INSTALL: &str = "dependency-install";
pub const INSTALLED_BY_ZIP_INSTALL: &str = "zip-install";
pub const INSTALLED_BY_IMPORTED_CURRENT: &str = "imported-current";
pub const INSTALLED_BY_FIRST_RUN_IMPORT: &str = "first-run-import";
pub const INSTALLED_BY_LINKED_EXISTING: &str = "linked-existing";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct InstalledMetadata {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub addons: BTreeMap<String, InstalledAddonMetadata>,
    #[serde(skip_serializing_if = "is_false")]
    pub first_run_baseline_complete: bool,
}

impl Default for InstalledMetadata {
    fn default() -> Self {
        Self {
            schema_version: INSTALLED_METADATA_SCHEMA_VERSION,
            addons: BTreeMap::new(),
            first_run_baseline_complete: false,
        }
    }
}

impl InstalledMetadata {
    pub fn addon_for_folder(&self, folder_name: &str) -> Option<&InstalledAddonMetadata> {
        self.addons.get(folder_name).or_else(|| {
            self.addons
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(folder_name))
                .map(|(_, value)| value)
        })
    }

    pub fn normalize(mut self) -> Self {
        if self.schema_version == 0 {
            self.schema_version = INSTALLED_METADATA_SCHEMA_VERSION;
        }

        let keys = self.addons.keys().cloned().collect::<Vec<_>>();
        for key in keys {
            if let Some(addon) = self.addons.get_mut(&key) {
                addon.normalize(&key);
            }
        }

        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct InstalledAddonMetadata {
    pub folder_name: String,
    pub remote_uid: Option<String>,
    pub remote_name: Option<String>,
    pub remote_version: Option<String>,
    #[serde(alias = "remote_date")]
    pub remote_updated_date: Option<i64>,
    pub remote_info_url: Option<String>,
    pub remote_download_url: Option<String>,
    #[serde(alias = "downloaded_filename")]
    pub file_name: Option<String>,
    pub md5: Option<String>,
    #[serde(alias = "installed_timestamp")]
    pub installed_at: String,
    pub linked_at: Option<String>,
    #[serde(alias = "source")]
    pub installed_by: String,
    pub local_title: Option<String>,
    pub local_version: Option<String>,
    pub source_addon_uid: Option<String>,
    pub match_confidence: Option<String>,
}

impl InstalledAddonMetadata {
    pub fn normalize(&mut self, folder_name: &str) {
        if self.folder_name.trim().is_empty() {
            self.folder_name = folder_name.to_owned();
        }

        self.remote_uid = normalize_optional_string(self.remote_uid.take());
        self.remote_name = normalize_optional_string(self.remote_name.take());
        self.remote_version = normalize_optional_string(self.remote_version.take());
        self.remote_info_url = normalize_optional_string(self.remote_info_url.take());
        self.remote_download_url = normalize_optional_string(self.remote_download_url.take());
        self.file_name = normalize_optional_string(self.file_name.take());
        self.md5 = normalize_optional_string(self.md5.take());
        self.linked_at = normalize_optional_string(self.linked_at.take());
        self.local_title = normalize_optional_string(self.local_title.take());
        self.local_version = normalize_optional_string(self.local_version.take());
        self.source_addon_uid = normalize_optional_string(self.source_addon_uid.take());
        self.match_confidence = normalize_optional_string(self.match_confidence.take());

        if self.installed_by.trim().is_empty() {
            self.installed_by = if self.remote_uid.is_some() {
                INSTALLED_BY_REMOTE_INSTALL.to_owned()
            } else {
                INSTALLED_BY_ZIP_INSTALL.to_owned()
            };
        }
        if self.installed_at.trim().is_empty() {
            self.installed_at = current_timestamp_string();
        }
    }
}

#[derive(Debug, Error)]
pub enum InstalledMetadataError {
    #[error("filesystem error: {0}")]
    Io(#[from] io::Error),

    #[error("metadata JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Copy)]
pub struct RemoteInstallMetadata<'a> {
    pub details: &'a AddonDetails,
    pub remote_uid: &'a str,
    pub installed_by: &'a str,
    pub source_addon_uid: Option<&'a str>,
}

pub fn installed_metadata_path(addons_dir: &Path) -> PathBuf {
    addons_dir
        .parent()
        .unwrap_or(addons_dir)
        .join(".scribe-addon-manager")
        .join("installed.json")
}

pub fn load_installed_metadata(
    addons_dir: &Path,
) -> Result<InstalledMetadata, InstalledMetadataError> {
    let path = installed_metadata_path(addons_dir);
    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(InstalledMetadata::default());
        }
        Err(error) => return Err(error.into()),
    };

    Ok(serde_json::from_str::<InstalledMetadata>(&contents)?.normalize())
}

pub fn load_installed_metadata_or_default(addons_dir: &Path) -> InstalledMetadata {
    match load_installed_metadata(addons_dir) {
        Ok(metadata) => metadata,
        Err(error) => {
            warn!(
                "could not load installed addon metadata from {}: {}",
                installed_metadata_path(addons_dir).display(),
                error
            );
            InstalledMetadata::default()
        }
    }
}

pub fn save_installed_metadata(
    addons_dir: &Path,
    metadata: &InstalledMetadata,
) -> Result<(), InstalledMetadataError> {
    let path = installed_metadata_path(addons_dir);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let metadata = metadata.clone().normalize();
    fs::write(path, serde_json::to_string_pretty(&metadata)?)?;
    Ok(())
}

pub fn remove_installed_metadata(
    addons_dir: &Path,
    folder_name: &str,
) -> Result<(), InstalledMetadataError> {
    let mut metadata = load_installed_metadata(addons_dir)?;
    let matching_key = metadata
        .addons
        .keys()
        .find(|key| key.eq_ignore_ascii_case(folder_name))
        .cloned();

    if let Some(key) = matching_key {
        metadata.addons.remove(&key);
        save_installed_metadata(addons_dir, &metadata)?;
    }

    Ok(())
}

pub fn installed_remote_addons(metadata: &InstalledMetadata) -> Vec<InstalledRemoteAddon> {
    metadata
        .addons
        .values()
        .filter_map(|metadata| {
            metadata
                .remote_uid
                .as_ref()
                .map(|remote_uid| InstalledRemoteAddon {
                    folder_name: metadata.folder_name.clone(),
                    remote_uid: remote_uid.clone(),
                })
        })
        .collect()
}

pub fn record_remote_install_metadata(
    addons_dir: &Path,
    plan: &InstallPlan,
    result: &InstallResult,
    remote_metadata: RemoteInstallMetadata<'_>,
) -> Result<(), InstalledMetadataError> {
    if !install_result_applied(result) {
        return Ok(());
    }

    let mut metadata = load_installed_metadata(addons_dir)?;
    let installed_at = current_timestamp_string();

    for item in applied_items(result) {
        let Some(folder_name) = installed_folder_name(item) else {
            continue;
        };
        let plan_item = plan_item_for_installed_folder(plan, item, &folder_name);

        metadata.addons.insert(
            folder_name.clone(),
            remote_metadata_entry(&folder_name, plan_item, remote_metadata, &installed_at),
        );
    }

    save_installed_metadata(addons_dir, &metadata)
}

pub fn record_zip_install_metadata(
    addons_dir: &Path,
    plan: &InstallPlan,
    result: &InstallResult,
) -> Result<(), InstalledMetadataError> {
    if !install_result_applied(result) {
        return Ok(());
    }

    let mut metadata = load_installed_metadata(addons_dir)?;
    let installed_at = current_timestamp_string();

    for item in applied_items(result) {
        let Some(folder_name) = installed_folder_name(item) else {
            continue;
        };
        let plan_item = plan_item_for_installed_folder(plan, item, &folder_name);

        metadata.addons.insert(
            folder_name.clone(),
            zip_metadata_entry(&folder_name, plan_item, &installed_at),
        );
    }

    save_installed_metadata(addons_dir, &metadata)
}

fn remote_metadata_entry(
    folder_name: &str,
    plan_item: Option<&InstallPlanItem>,
    remote_metadata: RemoteInstallMetadata<'_>,
    installed_at: &str,
) -> InstalledAddonMetadata {
    InstalledAddonMetadata {
        folder_name: folder_name.to_owned(),
        remote_uid: remote_metadata
            .details
            .uid
            .clone()
            .and_then(normalize_string)
            .or_else(|| Some(remote_metadata.remote_uid.to_owned())),
        remote_name: normalize_optional_string(remote_metadata.details.name.clone()),
        remote_version: normalize_optional_string(remote_metadata.details.version.clone()),
        remote_updated_date: remote_metadata.details.date,
        remote_info_url: normalize_optional_string(remote_metadata.details.file_info_url.clone()),
        remote_download_url: normalize_optional_string(
            remote_metadata.details.download_url.clone(),
        ),
        file_name: normalize_optional_string(remote_metadata.details.file_name.clone()).or_else(
            || {
                Some(remote::download_file_name(
                    remote_metadata.details,
                    remote_metadata.remote_uid,
                ))
            },
        ),
        md5: normalize_optional_string(remote_metadata.details.md5.clone()),
        installed_at: installed_at.to_owned(),
        linked_at: None,
        installed_by: remote_metadata.installed_by.to_owned(),
        local_title: plan_item.and_then(|item| normalize_optional_string(item.title.clone())),
        local_version: plan_item.and_then(|item| normalize_optional_string(item.version.clone())),
        source_addon_uid: remote_metadata
            .source_addon_uid
            .map(ToOwned::to_owned)
            .and_then(normalize_string),
        match_confidence: None,
    }
}

fn zip_metadata_entry(
    folder_name: &str,
    plan_item: Option<&InstallPlanItem>,
    installed_at: &str,
) -> InstalledAddonMetadata {
    InstalledAddonMetadata {
        folder_name: folder_name.to_owned(),
        remote_uid: None,
        remote_name: None,
        remote_version: None,
        remote_updated_date: None,
        remote_info_url: None,
        remote_download_url: None,
        file_name: None,
        md5: None,
        installed_at: installed_at.to_owned(),
        linked_at: None,
        installed_by: INSTALLED_BY_ZIP_INSTALL.to_owned(),
        local_title: plan_item.and_then(|item| normalize_optional_string(item.title.clone())),
        local_version: plan_item.and_then(|item| normalize_optional_string(item.version.clone())),
        source_addon_uid: None,
        match_confidence: None,
    }
}

fn applied_items(result: &InstallResult) -> impl Iterator<Item = &InstalledItem> {
    result
        .items
        .iter()
        .filter(|item| !matches!(item.action, InstallActionPerformed::Skipped))
}

fn installed_folder_name(item: &InstalledItem) -> Option<String> {
    item.target_folder
        .as_ref()
        .and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy().to_string())
        .and_then(normalize_string)
}

fn plan_item_for_installed_folder<'a>(
    plan: &'a InstallPlan,
    item: &InstalledItem,
    folder_name: &str,
) -> Option<&'a InstallPlanItem> {
    plan.items
        .iter()
        .find(|plan_item| {
            plan_item
                .target_folder
                .as_ref()
                .and_then(|path| path.file_name())
                .is_some_and(|target| target.to_string_lossy().eq_ignore_ascii_case(folder_name))
        })
        .or_else(|| {
            let source_folder = item.source_folder.as_deref()?;
            plan.items.iter().find(|plan_item| {
                plan_item
                    .source_folder
                    .as_deref()
                    .is_some_and(|source| source.eq_ignore_ascii_case(source_folder))
            })
        })
}

fn install_result_applied(result: &InstallResult) -> bool {
    result.installed_new > 0 || result.replaced > 0
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(normalize_string)
}

fn normalize_string(value: String) -> Option<String> {
    let value = value.trim().to_owned();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn default_schema_version() -> u32 {
    INSTALLED_METADATA_SCHEMA_VERSION
}

fn is_false(value: &bool) -> bool {
    !*value
}

pub fn current_timestamp_string() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use tempfile::tempdir;

    use super::{
        installed_metadata_path, load_installed_metadata, load_installed_metadata_or_default,
        record_remote_install_metadata, record_zip_install_metadata, remove_installed_metadata,
        save_installed_metadata, InstalledAddonMetadata, InstalledMetadata, RemoteInstallMetadata,
        INSTALLED_BY_REMOTE_INSTALL, INSTALLED_BY_REMOTE_UPDATE, INSTALLED_BY_ZIP_INSTALL,
    };
    use crate::api::models::AddonDetails;
    use crate::install::apply::{InstallActionPerformed, InstallResult, InstalledItem};
    use crate::install::plan::{InstallPlan, InstallPlanAction, InstallPlanItem};

    fn plan(addons_dir: PathBuf, folder_name: &str, version: &str) -> InstallPlan {
        InstallPlan {
            addons_dir: addons_dir.clone(),
            temp_dir: PathBuf::from("temp"),
            items: vec![InstallPlanItem {
                source_folder: Some(folder_name.to_owned()),
                title: Some("Local Title".to_owned()),
                version: Some(version.to_owned()),
                target_folder: Some(addons_dir.join(folder_name)),
                action: InstallPlanAction::WouldInstallNew,
            }],
        }
    }

    fn result(addons_dir: &std::path::Path, folder_name: &str) -> InstallResult {
        InstallResult {
            items: vec![InstalledItem {
                source_folder: Some(folder_name.to_owned()),
                target_folder: Some(addons_dir.join(folder_name)),
                backup_folder: None,
                action: InstallActionPerformed::InstalledNew,
                message: None,
            }],
            installed_new: 1,
            ..InstallResult::default()
        }
    }

    fn details(uid: &str, name: &str, version: &str) -> AddonDetails {
        serde_json::from_value(serde_json::json!({
            "UID": uid,
            "UIName": name,
            "UIVersion": version,
            "UIDate": 1_700_000_000,
            "UIFileInfoURL": "https://www.esoui.com/downloads/info42.html",
            "UIDownload": "https://cdn.esoui.com/addons/example.zip",
            "UIFileName": "Example.zip",
            "UIMD5": "abc123"
        }))
        .expect("valid details")
    }

    #[test]
    fn metadata_path_is_derived_from_addons_parent() {
        let addons_dir = PathBuf::from("/Users/Unai/Documents/Elder Scrolls Online/live/AddOns");

        assert_eq!(
            installed_metadata_path(&addons_dir),
            PathBuf::from(
                "/Users/Unai/Documents/Elder Scrolls Online/live/.scribe-addon-manager/installed.json"
            )
        );
    }

    #[test]
    fn metadata_save_and_load_round_trip() {
        let dir = tempdir().unwrap();
        let addons_dir = dir.path().join("live").join("AddOns");
        let mut metadata = InstalledMetadata::default();
        metadata.addons.insert(
            "ExampleAddon".to_owned(),
            InstalledAddonMetadata {
                folder_name: "ExampleAddon".to_owned(),
                remote_uid: Some("42".to_owned()),
                installed_at: "1".to_owned(),
                installed_by: INSTALLED_BY_REMOTE_INSTALL.to_owned(),
                ..InstalledAddonMetadata::default()
            },
        );

        save_installed_metadata(&addons_dir, &metadata).unwrap();
        let loaded = load_installed_metadata(&addons_dir).unwrap();

        assert_eq!(loaded.schema_version, 1);
        assert_eq!(
            loaded
                .addons
                .get("ExampleAddon")
                .unwrap()
                .remote_uid
                .as_deref(),
            Some("42")
        );
    }

    #[test]
    fn missing_and_corrupt_metadata_load_as_empty_with_default_helper() {
        let dir = tempdir().unwrap();
        let addons_dir = dir.path().join("live").join("AddOns");

        assert!(load_installed_metadata_or_default(&addons_dir)
            .addons
            .is_empty());

        let path = installed_metadata_path(&addons_dir);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{not json").unwrap();

        assert!(load_installed_metadata_or_default(&addons_dir)
            .addons
            .is_empty());
    }

    #[test]
    fn install_writes_metadata_for_installed_folder() {
        let dir = tempdir().unwrap();
        let addons_dir = dir.path().join("live").join("AddOns");
        let plan = plan(addons_dir.clone(), "ExampleAddon", "1");
        let result = result(&addons_dir, "ExampleAddon");
        let details = details("42", "Remote Example", "1.0.0");

        record_remote_install_metadata(
            &addons_dir,
            &plan,
            &result,
            RemoteInstallMetadata {
                details: &details,
                remote_uid: "42",
                installed_by: INSTALLED_BY_REMOTE_INSTALL,
                source_addon_uid: None,
            },
        )
        .unwrap();

        let metadata = load_installed_metadata(&addons_dir).unwrap();
        let addon = metadata.addons.get("ExampleAddon").unwrap();
        assert_eq!(addon.folder_name, "ExampleAddon");
        assert_eq!(addon.remote_uid.as_deref(), Some("42"));
        assert_eq!(addon.remote_name.as_deref(), Some("Remote Example"));
        assert_eq!(addon.remote_version.as_deref(), Some("1.0.0"));
        assert_eq!(addon.remote_updated_date, Some(1_700_000_000));
        assert_eq!(
            addon.remote_info_url.as_deref(),
            Some("https://www.esoui.com/downloads/info42.html")
        );
        assert_eq!(
            addon.remote_download_url.as_deref(),
            Some("https://cdn.esoui.com/addons/example.zip")
        );
        assert_eq!(addon.file_name.as_deref(), Some("Example.zip"));
        assert_eq!(addon.md5.as_deref(), Some("abc123"));
        assert_eq!(addon.installed_by, INSTALLED_BY_REMOTE_INSTALL);
        assert_eq!(addon.local_title.as_deref(), Some("Local Title"));
        assert_eq!(addon.local_version.as_deref(), Some("1"));
    }

    #[test]
    fn update_overwrites_metadata() {
        let dir = tempdir().unwrap();
        let addons_dir = dir.path().join("live").join("AddOns");
        let plan = plan(addons_dir.clone(), "ExampleAddon", "2");
        let result = result(&addons_dir, "ExampleAddon");
        let old_details = details("42", "Remote Example", "1.0.0");
        let new_details = details("42", "Remote Example", "2.0.0");

        record_remote_install_metadata(
            &addons_dir,
            &plan,
            &result,
            RemoteInstallMetadata {
                details: &old_details,
                remote_uid: "42",
                installed_by: INSTALLED_BY_REMOTE_INSTALL,
                source_addon_uid: None,
            },
        )
        .unwrap();
        record_remote_install_metadata(
            &addons_dir,
            &plan,
            &result,
            RemoteInstallMetadata {
                details: &new_details,
                remote_uid: "42",
                installed_by: INSTALLED_BY_REMOTE_UPDATE,
                source_addon_uid: None,
            },
        )
        .unwrap();

        let metadata = load_installed_metadata(&addons_dir).unwrap();
        let addon = metadata.addons.get("ExampleAddon").unwrap();
        assert_eq!(addon.remote_version.as_deref(), Some("2.0.0"));
        assert_eq!(addon.installed_by, INSTALLED_BY_REMOTE_UPDATE);
        assert_eq!(addon.local_version.as_deref(), Some("2"));
    }

    #[test]
    fn remove_deletes_metadata_entry() {
        let dir = tempdir().unwrap();
        let addons_dir = dir.path().join("live").join("AddOns");
        let mut metadata = InstalledMetadata::default();
        metadata.addons.insert(
            "ExampleAddon".to_owned(),
            InstalledAddonMetadata {
                folder_name: "ExampleAddon".to_owned(),
                installed_at: "1".to_owned(),
                installed_by: INSTALLED_BY_ZIP_INSTALL.to_owned(),
                ..InstalledAddonMetadata::default()
            },
        );
        save_installed_metadata(&addons_dir, &metadata).unwrap();

        remove_installed_metadata(&addons_dir, "exampleaddon").unwrap();

        let metadata = load_installed_metadata(&addons_dir).unwrap();
        assert!(!metadata.addons.contains_key("ExampleAddon"));
    }

    #[test]
    fn zip_install_writes_local_metadata_without_remote_uid() {
        let dir = tempdir().unwrap();
        let addons_dir = dir.path().join("live").join("AddOns");
        let plan = plan(addons_dir.clone(), "ZipAddon", "1");
        let result = result(&addons_dir, "ZipAddon");

        record_zip_install_metadata(&addons_dir, &plan, &result).unwrap();

        let metadata = load_installed_metadata(&addons_dir).unwrap();
        let addon = metadata.addons.get("ZipAddon").unwrap();
        assert_eq!(addon.remote_uid, None);
        assert_eq!(addon.installed_by, INSTALLED_BY_ZIP_INSTALL);
    }
}
