use std::fs;
use std::path::{Path, PathBuf};

use directories::UserDirs;

pub mod match_remote;
pub mod update_plan;
pub mod version;

#[derive(Debug, Clone)]
pub struct AddonPathCandidate {
    pub path: PathBuf,
    pub exists: bool,
    pub contains_addons: bool,
}

#[derive(Debug, Clone)]
pub struct LocalAddon {
    pub folder_name: String,
    pub folder_path: PathBuf,
    pub manifest_path: Option<PathBuf>,
    pub title: Option<String>,
    pub addon_version: Option<String>,
    pub version: Option<String>,
    pub api_versions: Vec<String>,
    pub depends_on: Vec<String>,
    pub optional_depends_on: Vec<String>,
    pub saved_variables: Vec<String>,
    pub saved_variables_per_character: Vec<String>,
    pub is_library: Option<bool>,
    pub author: Option<String>,
    pub description: Option<String>,
    pub valid_manifest: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ManifestInfo {
    pub title: Option<String>,
    pub addon_version: Option<String>,
    pub version: Option<String>,
    pub api_versions: Vec<String>,
    pub depends_on: Vec<String>,
    pub optional_depends_on: Vec<String>,
    pub saved_variables: Vec<String>,
    pub saved_variables_per_character: Vec<String>,
    pub is_library: Option<bool>,
    pub author: Option<String>,
    pub description: Option<String>,
    pub manifest_field_count: usize,
}

pub fn addon_path_candidates() -> Vec<AddonPathCandidate> {
    candidate_paths()
        .into_iter()
        .map(|path| AddonPathCandidate {
            exists: path.is_dir(),
            contains_addons: appears_to_contain_addons(&path),
            path,
        })
        .collect()
}

pub fn detect_best_addons_dir() -> Option<PathBuf> {
    let candidates = addon_path_candidates();

    candidates
        .iter()
        .find(|candidate| candidate.exists && candidate.contains_addons)
        .or_else(|| candidates.iter().find(|candidate| candidate.exists))
        .map(|candidate| candidate.path.clone())
}

pub fn scan_addons_dir(path: &Path) -> std::io::Result<Vec<LocalAddon>> {
    let mut addons = Vec::new();

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if !file_type.is_dir() {
            continue;
        }

        addons.push(scan_addon_folder(&entry.path())?);
    }

    addons.sort_by_key(|addon| addon.folder_name.to_lowercase());
    Ok(addons)
}

fn candidate_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Some(user_dirs) = UserDirs::new() {
        if let Some(documents) = user_dirs.document_dir() {
            paths.push(eso_addons_path(documents, "live"));
            paths.push(eso_addons_path(documents, "liveeu"));
        }
    }

    paths
}

fn eso_addons_path(documents: &Path, server_folder: &str) -> PathBuf {
    documents
        .join("Elder Scrolls Online")
        .join(server_folder)
        .join("AddOns")
}

fn appears_to_contain_addons(path: &Path) -> bool {
    scan_addons_dir(path)
        .map(|addons| addons.iter().any(|addon| addon.valid_manifest))
        .unwrap_or(false)
}

fn scan_addon_folder(folder_path: &Path) -> std::io::Result<LocalAddon> {
    let folder_name = folder_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| folder_path.display().to_string());

    let manifest_path = choose_manifest_path(folder_path, &folder_name)?;
    let parsed = match manifest_path.as_ref() {
        Some(path) => parse_manifest_file(path)?,
        None => ManifestInfo::default(),
    };

    Ok(LocalAddon {
        folder_name,
        folder_path: folder_path.to_path_buf(),
        manifest_path,
        title: parsed.title,
        addon_version: parsed.addon_version,
        version: parsed.version,
        api_versions: parsed.api_versions,
        depends_on: parsed.depends_on,
        optional_depends_on: parsed.optional_depends_on,
        saved_variables: parsed.saved_variables,
        saved_variables_per_character: parsed.saved_variables_per_character,
        is_library: parsed.is_library,
        author: parsed.author,
        description: parsed.description,
        valid_manifest: parsed.manifest_field_count > 0,
    })
}

fn choose_manifest_path(folder_path: &Path, folder_name: &str) -> std::io::Result<Option<PathBuf>> {
    for extension in ["txt", "addon"] {
        let preferred = folder_path.join(format!("{folder_name}.{extension}"));
        if preferred.is_file() {
            return Ok(Some(preferred));
        }
    }

    let mut best: Option<(PathBuf, usize)> = None;
    for entry in fs::read_dir(folder_path)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() || !has_manifest_extension(&path) {
            continue;
        }

        let parsed = parse_manifest_file(&path)?;
        let score = parsed.manifest_field_count;
        if score > best.as_ref().map(|(_, score)| *score).unwrap_or(0) {
            best = Some((path, score));
        }
    }

    Ok(best.map(|(path, _)| path))
}

fn parse_manifest_file(path: &Path) -> std::io::Result<ManifestInfo> {
    let bytes = fs::read(path)?;
    Ok(parse_manifest_bytes(&bytes))
}

fn has_manifest_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            extension.eq_ignore_ascii_case("txt") || extension.eq_ignore_ascii_case("addon")
        })
        .unwrap_or(false)
}

pub fn parse_manifest_bytes(bytes: &[u8]) -> ManifestInfo {
    let text = String::from_utf8_lossy(bytes);
    parse_manifest_text(&text)
}

pub fn parse_manifest_text(text: &str) -> ManifestInfo {
    let mut manifest = ManifestInfo::default();

    for line in text.lines() {
        let Some((key, value)) = parse_manifest_line(line) else {
            continue;
        };

        manifest.manifest_field_count += 1;
        match key.as_str() {
            "title" => manifest.title = Some(value),
            "addonversion" => manifest.addon_version = Some(value),
            "version" => manifest.version = Some(value),
            "apiversion" => manifest.api_versions.extend(split_values(&value)),
            "dependson" => manifest.depends_on.extend(split_dependency_values(&value)),
            "optionaldependson" => {
                manifest
                    .optional_depends_on
                    .extend(split_dependency_values(&value));
            }
            "savedvariables" => manifest.saved_variables.extend(split_values(&value)),
            "savedvariablespercharacter" => manifest
                .saved_variables_per_character
                .extend(split_values(&value)),
            "islibrary" => manifest.is_library = parse_bool(&value),
            "author" => manifest.author = Some(value),
            "description" => manifest.description = Some(value),
            _ => manifest.manifest_field_count -= 1,
        }
    }

    manifest
}

fn parse_manifest_line(line: &str) -> Option<(String, String)> {
    let line = line.trim_start();
    let line = line.strip_prefix("##")?.trim_start();
    let (key, value) = line.split_once(':')?;
    let key = key.trim().to_lowercase();
    let value = value.trim().to_owned();

    if key.is_empty() || value.is_empty() {
        return None;
    }

    Some((key, value))
}

fn split_values(value: &str) -> Vec<String> {
    value
        .split(|ch: char| ch == ',' || ch == ';' || ch.is_whitespace())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn split_dependency_values(value: &str) -> Vec<String> {
    let values: Vec<String> = value
        .split(|ch| ch == ',' || ch == ';')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect();

    if values.is_empty() && !value.trim().is_empty() {
        vec![value.trim().to_owned()]
    } else {
        values
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_lowercase().as_str() {
        "1" | "true" | "yes" => Some(true),
        "0" | "false" | "no" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::parse_manifest_text;

    #[test]
    fn parses_standard_manifest() {
        let manifest = parse_manifest_text(
            r#"
## Title: Sample Addon
## AddOnVersion: 42
## Version: 1.2.3
## APIVersion: 101044
## Author: An Author
## Description: Something useful
"#,
        );

        assert_eq!(manifest.title.as_deref(), Some("Sample Addon"));
        assert_eq!(manifest.addon_version.as_deref(), Some("42"));
        assert_eq!(manifest.version.as_deref(), Some("1.2.3"));
        assert_eq!(manifest.api_versions, vec!["101044"]);
        assert_eq!(manifest.author.as_deref(), Some("An Author"));
        assert_eq!(manifest.description.as_deref(), Some("Something useful"));
    }

    #[test]
    fn parses_no_space_after_hashes() {
        let manifest = parse_manifest_text(
            r#"
##Title: Tight Manifest
##Author: Someone
"#,
        );

        assert_eq!(manifest.title.as_deref(), Some("Tight Manifest"));
        assert_eq!(manifest.author.as_deref(), Some("Someone"));
    }

    #[test]
    fn keeps_dependency_version_constraints() {
        let manifest = parse_manifest_text("## DependsOn: LibAddonMenu-2.0 >= 28");

        assert_eq!(manifest.depends_on, vec!["LibAddonMenu-2.0 >= 28"]);
    }

    #[test]
    fn tolerates_missing_fields() {
        let manifest = parse_manifest_text(
            r#"
-- lua comment
Plain text
## Unknown: ignored
"#,
        );

        assert_eq!(manifest.title, None);
        assert_eq!(manifest.addon_version, None);
        assert_eq!(manifest.manifest_field_count, 0);
    }

    #[test]
    fn parses_multiple_api_versions() {
        let manifest = parse_manifest_text("## APIVersion: 101043 101044,101045; 101046");

        assert_eq!(
            manifest.api_versions,
            vec!["101043", "101044", "101045", "101046"]
        );
    }

    #[test]
    fn parses_is_library_true_false() {
        let true_manifest = parse_manifest_text("## IsLibrary: true");
        let false_manifest = parse_manifest_text("## IsLibrary: 0");

        assert_eq!(true_manifest.is_library, Some(true));
        assert_eq!(false_manifest.is_library, Some(false));
    }

    #[test]
    fn parses_saved_variables_fields() {
        let manifest = parse_manifest_text(
            r#"
## SavedVariables: AccountSettings OtherAccountSettings
## SavedVariablesPerCharacter: CharacterSettings, OtherCharacterSettings
"#,
        );

        assert_eq!(
            manifest.saved_variables,
            vec!["AccountSettings", "OtherAccountSettings"]
        );
        assert_eq!(
            manifest.saved_variables_per_character,
            vec!["CharacterSettings", "OtherCharacterSettings"]
        );
    }
}
