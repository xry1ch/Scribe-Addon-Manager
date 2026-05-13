use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{self, copy, Read};
use std::path::{Component, Path, PathBuf};

use tempfile::Builder;
use thiserror::Error;
use zip::result::ZipError;
use zip::ZipArchive;

use crate::local::{self, LocalAddon, ManifestInfo};

pub const MAX_UNCOMPRESSED_SIZE: u64 = 500 * 1024 * 1024;
pub const MAX_FILE_COUNT: usize = 10_000;

#[derive(Debug, Error)]
pub enum ZipSafetyError {
    #[error("failed to read ZIP: {0}")]
    Zip(#[from] ZipError),

    #[error("filesystem error: {0}")]
    Io(#[from] io::Error),

    #[error("unsafe ZIP entry path rejected: {name}")]
    UnsafePath { name: String },

    #[error("ZIP entry is a symlink and will not be extracted: {name}")]
    Symlink { name: String },

    #[error("ZIP has too many entries: {count} > {max}")]
    TooManyEntries { count: usize, max: usize },

    #[error("ZIP uncompressed size is too large: {size} > {max}")]
    TooLarge { size: u64, max: u64 },
}

#[derive(Debug, Clone)]
pub struct ZipInspection {
    pub zip_path: PathBuf,
    pub total_entries: usize,
    pub total_uncompressed_size: u64,
    pub top_level_items: Vec<String>,
    pub likely_addon_folders: Vec<ZipAddonFolder>,
}

#[derive(Debug, Clone)]
pub struct ZipAddonFolder {
    pub folder_name: String,
    pub has_manifest: bool,
    pub manifest_path: Option<String>,
    pub title: Option<String>,
    pub addon_version: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ExtractedZip {
    pub temp_dir: PathBuf,
    pub inspection: ZipInspection,
    pub detected_addons: Vec<LocalAddon>,
}

#[derive(Debug)]
struct ManifestCandidate {
    path: String,
    info: ManifestInfo,
}

pub fn inspect_zip(path: &Path) -> Result<ZipInspection, ZipSafetyError> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file)?;
    inspect_archive(path, &mut archive)
}

pub fn extract_zip_to_temp(path: &Path) -> Result<ExtractedZip, ZipSafetyError> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file)?;
    let inspection = inspect_archive(path, &mut archive)?;

    let temp_dir = Builder::new()
        .prefix("eso-addon-manager-")
        .tempdir()?
        .keep();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let safe_path = validate_entry_path(entry.name())?;
        if entry.is_dir() {
            fs::create_dir_all(temp_dir.join(&safe_path))?;
            continue;
        }

        let output_path = temp_dir.join(&safe_path);
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut output = File::create(&output_path)?;
        copy(&mut entry, &mut output)?;
    }

    let detected_addons = local::scan_addons_dir(&temp_dir)?;

    Ok(ExtractedZip {
        temp_dir,
        inspection,
        detected_addons,
    })
}

fn inspect_archive<R: Read + io::Seek>(
    zip_path: &Path,
    archive: &mut ZipArchive<R>,
) -> Result<ZipInspection, ZipSafetyError> {
    if archive.len() > MAX_FILE_COUNT {
        return Err(ZipSafetyError::TooManyEntries {
            count: archive.len(),
            max: MAX_FILE_COUNT,
        });
    }

    let mut total_uncompressed_size = 0_u64;
    let mut top_level_items = BTreeSet::new();
    let mut top_level_folders = BTreeSet::new();
    let mut manifests_by_folder: BTreeMap<String, Vec<ManifestCandidate>> = BTreeMap::new();

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let safe_path = validate_entry_path(entry.name())?;
        if entry.is_symlink() {
            return Err(ZipSafetyError::Symlink {
                name: entry.name().to_owned(),
            });
        }

        total_uncompressed_size =
            total_uncompressed_size
                .checked_add(entry.size())
                .ok_or(ZipSafetyError::TooLarge {
                    size: u64::MAX,
                    max: MAX_UNCOMPRESSED_SIZE,
                })?;
        if total_uncompressed_size > MAX_UNCOMPRESSED_SIZE {
            return Err(ZipSafetyError::TooLarge {
                size: total_uncompressed_size,
                max: MAX_UNCOMPRESSED_SIZE,
            });
        }

        if let Some(top_level) = top_level_component(&safe_path) {
            if has_more_than_one_component(&safe_path) || entry.is_dir() {
                top_level_folders.insert(top_level.clone());
            }
            top_level_items.insert(top_level);
        }

        if entry.is_file() && is_direct_manifest_child(&safe_path) {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            let info = local::parse_manifest_bytes(&bytes);
            if info.manifest_field_count > 0 {
                let folder_name = top_level_component(&safe_path).unwrap_or_default();
                manifests_by_folder
                    .entry(folder_name)
                    .or_default()
                    .push(ManifestCandidate {
                        path: safe_path.to_string_lossy().into_owned(),
                        info,
                    });
            }
        }
    }

    let likely_addon_folders = top_level_folders
        .into_iter()
        .map(|folder_name| {
            let manifests = manifests_by_folder.remove(&folder_name).unwrap_or_default();
            folder_from_manifests(folder_name, manifests)
        })
        .collect();

    Ok(ZipInspection {
        zip_path: zip_path.to_path_buf(),
        total_entries: archive.len(),
        total_uncompressed_size,
        top_level_items: top_level_items.into_iter().collect(),
        likely_addon_folders,
    })
}

pub fn validate_entry_path(name: &str) -> Result<PathBuf, ZipSafetyError> {
    let name = name.trim();
    if name.is_empty() || name.contains('\0') {
        return Err(ZipSafetyError::UnsafePath {
            name: name.to_owned(),
        });
    }

    let normalized = name.replace('\\', "/");
    if normalized.starts_with('/') || normalized.starts_with("//") {
        return Err(ZipSafetyError::UnsafePath {
            name: name.to_owned(),
        });
    }

    if normalized
        .split('/')
        .next()
        .is_some_and(has_windows_drive_prefix)
    {
        return Err(ZipSafetyError::UnsafePath {
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
                return Err(ZipSafetyError::UnsafePath {
                    name: name.to_owned(),
                });
            }
        }
    }

    if !has_normal_component {
        return Err(ZipSafetyError::UnsafePath {
            name: name.to_owned(),
        });
    }

    Ok(path)
}

fn has_windows_drive_prefix(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn top_level_component(path: &Path) -> Option<String> {
    path.components().find_map(|component| match component {
        Component::Normal(value) => Some(value.to_string_lossy().into_owned()),
        _ => None,
    })
}

fn is_direct_manifest_child(path: &Path) -> bool {
    let components = path.components().collect::<Vec<_>>();
    components.len() == 2
        && path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                extension.eq_ignore_ascii_case("txt") || extension.eq_ignore_ascii_case("addon")
            })
}

fn has_more_than_one_component(path: &Path) -> bool {
    path.components().take(2).count() > 1
}

fn folder_from_manifests(folder_name: String, manifests: Vec<ManifestCandidate>) -> ZipAddonFolder {
    let preferred = manifests
        .iter()
        .find(|candidate| {
            Path::new(&candidate.path)
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case(&format!("{folder_name}.txt")))
        })
        .or_else(|| {
            manifests
                .iter()
                .max_by_key(|candidate| candidate.info.manifest_field_count)
        });

    if let Some(candidate) = preferred {
        ZipAddonFolder {
            folder_name,
            has_manifest: true,
            manifest_path: Some(candidate.path.clone()),
            title: candidate.info.title.clone(),
            addon_version: candidate.info.addon_version.clone(),
            version: candidate.info.version.clone(),
        }
    } else {
        ZipAddonFolder {
            folder_name,
            has_manifest: false,
            manifest_path: None,
            title: None,
            addon_version: None,
            version: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Write;
    use std::path::Path;

    use tempfile::tempdir;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    use super::{
        extract_zip_to_temp, inspect_zip, validate_entry_path, ZipSafetyError, MAX_FILE_COUNT,
    };

    #[test]
    fn safe_normal_zip_passes() {
        let dir = tempdir().unwrap();
        let zip_path = dir.path().join("safe.zip");
        create_zip(
            &zip_path,
            &[(
                "Addon/Addon.txt",
                "## Title: Safe Addon\n## AddOnVersion: 2\n",
            )],
        );

        let inspection = inspect_zip(&zip_path).unwrap();

        assert_eq!(inspection.total_entries, 1);
        assert_eq!(inspection.likely_addon_folders.len(), 1);
        assert_eq!(
            inspection.likely_addon_folders[0].title.as_deref(),
            Some("Safe Addon")
        );
    }

    #[test]
    fn zip_with_parent_traversal_is_rejected() {
        let dir = tempdir().unwrap();
        let zip_path = dir.path().join("traversal.zip");
        create_zip(&zip_path, &[("../evil.txt", "oops")]);

        assert!(matches!(
            inspect_zip(&zip_path),
            Err(ZipSafetyError::UnsafePath { .. })
        ));
    }

    #[test]
    fn zip_with_absolute_path_is_rejected() {
        let dir = tempdir().unwrap();
        let zip_path = dir.path().join("absolute.zip");
        create_zip(&zip_path, &[("/tmp/evil.txt", "oops")]);

        assert!(matches!(
            inspect_zip(&zip_path),
            Err(ZipSafetyError::UnsafePath { .. })
        ));
    }

    #[test]
    fn zip_with_windows_drive_prefix_is_rejected() {
        let dir = tempdir().unwrap();
        let zip_path = dir.path().join("drive.zip");
        create_zip(&zip_path, &[("C:/evil.txt", "oops")]);

        assert!(matches!(
            inspect_zip(&zip_path),
            Err(ZipSafetyError::UnsafePath { .. })
        ));
    }

    #[test]
    fn backslash_traversal_is_rejected() {
        assert!(matches!(
            validate_entry_path("Addon\\..\\evil.txt"),
            Err(ZipSafetyError::UnsafePath { .. })
        ));
    }

    #[test]
    fn file_count_limit_is_rejected() {
        let dir = tempdir().unwrap();
        let zip_path = dir.path().join("many.zip");
        let file = File::create(&zip_path).unwrap();
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        for index in 0..=MAX_FILE_COUNT {
            zip.start_file(format!("Addon/file{index}.txt"), options)
                .unwrap();
            zip.write_all(b"").unwrap();
        }
        zip.finish().unwrap();

        assert!(matches!(
            inspect_zip(&zip_path),
            Err(ZipSafetyError::TooManyEntries { .. })
        ));
    }

    #[test]
    fn manifest_detection_works_from_extracted_temp_contents() {
        let dir = tempdir().unwrap();
        let zip_path = dir.path().join("extract.zip");
        create_zip(
            &zip_path,
            &[(
                "Addon/Addon.txt",
                "## Title: Extracted Addon\n## Version: 1.0\n",
            )],
        );

        let extracted = extract_zip_to_temp(&zip_path).unwrap();

        assert!(extracted.temp_dir.is_dir());
        assert_eq!(extracted.detected_addons.len(), 1);
        assert_eq!(
            extracted.detected_addons[0].title.as_deref(),
            Some("Extracted Addon")
        );
        let _ = std::fs::remove_dir_all(&extracted.temp_dir);
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
