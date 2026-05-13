use std::cmp::Reverse;

use crate::api::models::AddonSummary;
use crate::local::LocalAddon;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchStatus {
    Matched,
    PossibleUpdate,
    UnknownUpdate,
    NoMatch,
    Library,
    Ambiguous,
}

impl MatchStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Matched => "matched",
            Self::PossibleUpdate => "possible-update",
            Self::UnknownUpdate => "unknown-update",
            Self::NoMatch => "no-match",
            Self::Library => "library",
            Self::Ambiguous => "ambiguous",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RemoteCandidate {
    pub uid: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
    pub updated: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct MatchResult {
    pub local: LocalAddon,
    pub status: MatchStatus,
    pub remote: Option<RemoteCandidate>,
    pub candidates: Vec<RemoteCandidate>,
}

pub fn match_installed_addons(
    local_addons: &[LocalAddon],
    remote_addons: &[AddonSummary],
) -> Vec<MatchResult> {
    local_addons
        .iter()
        .map(|local| match_one(local, remote_addons))
        .collect()
}

fn match_one(local: &LocalAddon, remote_addons: &[AddonSummary]) -> MatchResult {
    let mut matches = exact_folder_matches(local, remote_addons);
    if matches.is_empty() {
        matches = exact_title_matches(local, remote_addons);
    }
    if matches.is_empty() {
        matches = fuzzy_matches(local, remote_addons);
    }

    if matches.len() > 1 {
        matches.sort_by_key(|remote| Reverse(candidate_score(local, remote)));
        return MatchResult {
            local: local.clone(),
            status: MatchStatus::Ambiguous,
            remote: None,
            candidates: matches
                .into_iter()
                .take(5)
                .map(RemoteCandidate::from)
                .collect(),
        };
    }

    if let Some(remote) = matches.into_iter().next() {
        return MatchResult {
            local: local.clone(),
            status: version_status(local, remote),
            remote: Some(RemoteCandidate::from(remote)),
            candidates: Vec::new(),
        };
    }

    MatchResult {
        local: local.clone(),
        status: if local.is_library == Some(true) {
            MatchStatus::Library
        } else {
            MatchStatus::NoMatch
        },
        remote: None,
        candidates: Vec::new(),
    }
}

fn exact_folder_matches<'a>(
    local: &LocalAddon,
    remote_addons: &'a [AddonSummary],
) -> Vec<&'a AddonSummary> {
    let local_folder = normalize_identity(&local.folder_name);
    if local_folder.is_empty() {
        return Vec::new();
    }

    remote_addons
        .iter()
        .filter(|remote| {
            remote
                .directories
                .iter()
                .any(|directory| normalize_identity(directory) == local_folder)
        })
        .collect()
}

fn exact_title_matches<'a>(
    local: &LocalAddon,
    remote_addons: &'a [AddonSummary],
) -> Vec<&'a AddonSummary> {
    let Some(title) = local.title.as_deref() else {
        return Vec::new();
    };
    let title = normalize_identity(title);
    if title.is_empty() {
        return Vec::new();
    }

    remote_addons
        .iter()
        .filter(|remote| {
            remote.name.as_deref().map(normalize_identity).as_deref() == Some(title.as_str())
        })
        .collect()
}

fn fuzzy_matches<'a>(
    local: &LocalAddon,
    remote_addons: &'a [AddonSummary],
) -> Vec<&'a AddonSummary> {
    let local_folder = normalize_identity(&local.folder_name);
    let local_title = local.title.as_deref().map(normalize_identity);

    remote_addons
        .iter()
        .filter(|remote| {
            let remote_name = remote.name.as_deref().map(normalize_identity);
            let folder_match = remote.directories.iter().any(|directory| {
                let remote_dir = normalize_identity(directory);
                contains_either(&local_folder, &remote_dir)
            });
            let title_match = local_title
                .as_deref()
                .zip(remote_name.as_deref())
                .map(|(local_title, remote_name)| contains_either(local_title, remote_name))
                .unwrap_or(false);

            folder_match || title_match
        })
        .collect()
}

fn contains_either(left: &str, right: &str) -> bool {
    !left.is_empty()
        && !right.is_empty()
        && (left == right || left.contains(right) || right.contains(left))
}

fn candidate_score(local: &LocalAddon, remote: &AddonSummary) -> usize {
    let local_folder = normalize_identity(&local.folder_name);
    let local_title = local.title.as_deref().map(normalize_identity);
    let remote_name = remote.name.as_deref().map(normalize_identity);

    let folder_score = remote
        .directories
        .iter()
        .map(|directory| normalize_identity(directory))
        .map(|directory| {
            if directory == local_folder {
                100
            } else if contains_either(&directory, &local_folder) {
                50
            } else {
                0
            }
        })
        .max()
        .unwrap_or(0);

    let title_score = local_title
        .as_deref()
        .zip(remote_name.as_deref())
        .map(|(local_title, remote_name)| {
            if local_title == remote_name {
                90
            } else if contains_either(local_title, remote_name) {
                40
            } else {
                0
            }
        })
        .unwrap_or(0);

    folder_score + title_score
}

fn version_status(local: &LocalAddon, remote: &AddonSummary) -> MatchStatus {
    let local_version = local.addon_version.as_deref().or(local.version.as_deref());
    let remote_version = remote.version.as_deref();

    match (
        local_version.and_then(parse_numeric_version),
        remote_version.and_then(parse_numeric_version),
    ) {
        (Some(local), Some(remote)) if remote > local => MatchStatus::PossibleUpdate,
        (Some(_), Some(_)) => MatchStatus::Matched,
        _ => MatchStatus::UnknownUpdate,
    }
}

fn parse_numeric_version(value: &str) -> Option<u64> {
    let value = value.trim();
    if value.is_empty() || !value.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }

    value.parse().ok()
}

fn normalize_identity(value: &str) -> String {
    let without_color = strip_eso_color_codes(value);
    let mut normalized = String::new();
    let mut previous_was_space = false;

    for ch in without_color.chars().flat_map(char::to_lowercase) {
        if ch.is_alphanumeric() {
            normalized.push(ch);
            previous_was_space = false;
        } else if !previous_was_space {
            normalized.push(' ');
            previous_was_space = true;
        }
    }

    normalized.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn strip_eso_color_codes(value: &str) -> String {
    let mut stripped = String::new();
    let mut chars = value.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '|' {
            match chars.peek().copied() {
                Some('r') | Some('R') => {
                    chars.next();
                    continue;
                }
                Some('c') | Some('C') => {
                    chars.next();
                    for _ in 0..8 {
                        if chars.peek().is_some_and(|ch| ch.is_ascii_hexdigit()) {
                            chars.next();
                        }
                    }
                    continue;
                }
                _ => {}
            }
        }

        stripped.push(ch);
    }

    stripped
}

impl From<&AddonSummary> for RemoteCandidate {
    fn from(addon: &AddonSummary) -> Self {
        Self {
            uid: addon.uid.clone(),
            name: addon.name.clone(),
            version: addon.version.clone(),
            updated: addon.date,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use crate::api::models::AddonSummary;
    use crate::local::match_remote::{match_one, MatchStatus};
    use crate::local::LocalAddon;

    fn local(folder_name: &str, title: Option<&str>, addon_version: Option<&str>) -> LocalAddon {
        LocalAddon {
            folder_name: folder_name.to_owned(),
            folder_path: PathBuf::from(folder_name),
            manifest_path: None,
            title: title.map(ToOwned::to_owned),
            addon_version: addon_version.map(ToOwned::to_owned),
            version: None,
            api_versions: Vec::new(),
            depends_on: Vec::new(),
            optional_depends_on: Vec::new(),
            is_library: None,
            author: None,
            description: None,
            valid_manifest: true,
        }
    }

    fn remote(uid: &str, name: &str, version: &str, directories: &[&str]) -> AddonSummary {
        AddonSummary {
            uid: Some(uid.to_owned()),
            name: Some(name.to_owned()),
            author_name: None,
            version: Some(version.to_owned()),
            date: None,
            file_info_url: None,
            description: None,
            summary: None,
            directories: directories
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            _extra: BTreeMap::new(),
        }
    }

    #[test]
    fn exact_folder_match() {
        let result = match_one(
            &local("SkyShards", Some("Different"), Some("1")),
            &[remote("128", "SkyShards", "1", &["SkyShards"])],
        );

        assert_eq!(result.status, MatchStatus::Matched);
        assert_eq!(result.remote.unwrap().uid.as_deref(), Some("128"));
    }

    #[test]
    fn exact_title_match() {
        let result = match_one(
            &local("SomeFolder", Some("SkyShards"), Some("1")),
            &[remote("128", "SkyShards", "1", &["OtherFolder"])],
        );

        assert_eq!(result.status, MatchStatus::Matched);
        assert_eq!(result.remote.unwrap().uid.as_deref(), Some("128"));
    }

    #[test]
    fn title_with_eso_color_codes() {
        let result = match_one(
            &local("NirnsteelUI", Some("|cFFFF00NirnSteel UI|r"), Some("1")),
            &[remote("4574", "NirnSteel UI", "1", &["OtherFolder"])],
        );

        assert_eq!(result.status, MatchStatus::Matched);
        assert_eq!(result.remote.unwrap().uid.as_deref(), Some("4574"));
    }

    #[test]
    fn no_match() {
        let result = match_one(
            &local("Missing", Some("Missing"), Some("1")),
            &[remote("128", "SkyShards", "1", &["SkyShards"])],
        );

        assert_eq!(result.status, MatchStatus::NoMatch);
        assert!(result.remote.is_none());
    }

    #[test]
    fn ambiguous_matches() {
        let result = match_one(
            &local("Foo", Some("Foo"), Some("1")),
            &[
                remote("1", "Foo", "1", &["FooOne"]),
                remote("2", "Foo", "1", &["FooTwo"]),
            ],
        );

        assert_eq!(result.status, MatchStatus::Ambiguous);
        assert_eq!(result.candidates.len(), 2);
    }

    #[test]
    fn numeric_version_comparison() {
        let result = match_one(
            &local("Addon", Some("Addon"), Some("10")),
            &[remote("1", "Addon", "11", &["Addon"])],
        );

        assert_eq!(result.status, MatchStatus::PossibleUpdate);
    }

    #[test]
    fn non_numeric_version_unknown() {
        let result = match_one(
            &local("Addon", Some("Addon"), Some("1.0")),
            &[remote("1", "Addon", "1.1", &["Addon"])],
        );

        assert_eq!(result.status, MatchStatus::UnknownUpdate);
    }
}
