use crate::api::models::AddonSummary;
use crate::local::version::{compare_versions, VersionComparison};
use crate::local::LocalAddon;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchStatus {
    Matched,
    PossibleUpdate,
    LocalNewer,
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
            Self::LocalNewer => "local-newer",
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
    pub tier: u8,
    pub score: usize,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct MatchResult {
    pub local: LocalAddon,
    pub status: MatchStatus,
    pub remote: Option<RemoteCandidate>,
    pub candidates: Vec<RemoteCandidate>,
    pub debug_candidates: Vec<RemoteCandidate>,
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
    let mut candidates = scored_candidates(local, remote_addons);
    candidates.sort_by(|left, right| {
        left.tier
            .cmp(&right.tier)
            .then_with(|| right.score.cmp(&left.score))
            .then_with(|| left.name.cmp(&right.name))
    });
    let debug_candidates = candidates.iter().take(8).cloned().collect::<Vec<_>>();

    let Some(best) = candidates.first() else {
        return MatchResult {
            local: local.clone(),
            status: if local.is_library == Some(true) {
                MatchStatus::Library
            } else {
                MatchStatus::NoMatch
            },
            remote: None,
            candidates: Vec::new(),
            debug_candidates,
        };
    };

    let best_tier = best.tier;
    let best_score = best.score;
    let best_candidates = candidates
        .iter()
        .filter(|candidate| candidate.tier == best_tier && candidate.score == best_score)
        .cloned()
        .collect::<Vec<_>>();

    if best_candidates.len() > 1 {
        return MatchResult {
            local: local.clone(),
            status: MatchStatus::Ambiguous,
            remote: None,
            candidates: best_candidates,
            debug_candidates,
        };
    }

    let best = best_candidates.into_iter().next().expect("best candidate");
    let Some(remote) = remote_addons
        .iter()
        .find(|remote| remote.uid == best.uid && remote.name == best.name)
    else {
        return MatchResult {
            local: local.clone(),
            status: MatchStatus::NoMatch,
            remote: None,
            candidates: Vec::new(),
            debug_candidates,
        };
    };

    MatchResult {
        local: local.clone(),
        status: version_status(local, remote),
        remote: Some(best),
        candidates: Vec::new(),
        debug_candidates,
    }
}

fn scored_candidates(local: &LocalAddon, remote_addons: &[AddonSummary]) -> Vec<RemoteCandidate> {
    let local_folder = normalize_identity(&local.folder_name);
    let local_title = local.title.as_deref().map(normalize_identity);

    remote_addons
        .iter()
        .filter_map(|remote| {
            candidate_match(
                remote,
                &local_folder,
                local_title.as_deref().filter(|title| !title.is_empty()),
            )
        })
        .collect()
}

fn candidate_match(
    remote: &AddonSummary,
    local_folder: &str,
    local_title: Option<&str>,
) -> Option<RemoteCandidate> {
    let remote_name = remote.name.as_deref().map(normalize_identity);
    let primary_directory = remote
        .directories
        .first()
        .map(|directory| normalize_identity(directory));
    let bundled_directories = remote
        .directories
        .iter()
        .skip(1)
        .map(|directory| normalize_identity(directory))
        .collect::<Vec<_>>();

    let (tier, score, reason) = if remote_name.as_deref() == Some(local_folder) {
        (1, 100, "exact-folder-ui-name")
    } else if local_title
        .zip(remote_name.as_deref())
        .is_some_and(|(local_title, remote_name)| local_title == remote_name)
    {
        (1, 95, "exact-title-ui-name")
    } else if primary_directory.as_deref() == Some(local_folder) {
        (2, 80, "primary-directory")
    } else if bundled_directories
        .iter()
        .any(|directory| directory == local_folder)
    {
        (3, 60, "bundled-directory")
    } else if remote_name
        .as_deref()
        .is_some_and(|remote_name| contains_either(local_folder, remote_name))
        || local_title
            .zip(remote_name.as_deref())
            .is_some_and(|(local_title, remote_name)| contains_either(local_title, remote_name))
        || primary_directory
            .as_deref()
            .is_some_and(|primary_directory| contains_either(local_folder, primary_directory))
        || bundled_directories
            .iter()
            .any(|directory| contains_either(local_folder, directory))
    {
        (4, 30, "loose-normalized")
    } else {
        return None;
    };

    Some(RemoteCandidate {
        uid: remote.uid.clone(),
        name: remote.name.clone(),
        version: remote.version.clone(),
        updated: remote.date,
        tier,
        score,
        reason: reason.to_owned(),
    })
}

fn contains_either(left: &str, right: &str) -> bool {
    !left.is_empty()
        && !right.is_empty()
        && (left == right || left.contains(right) || right.contains(left))
}

fn version_status(local: &LocalAddon, remote: &AddonSummary) -> MatchStatus {
    let local_version = local.addon_version.as_deref().or(local.version.as_deref());
    let remote_version = remote.version.as_deref();

    match compare_versions(local_version, remote_version) {
        VersionComparison::RemoteNewer => MatchStatus::PossibleUpdate,
        VersionComparison::Same => MatchStatus::Matched,
        VersionComparison::LocalNewer => MatchStatus::LocalNewer,
        VersionComparison::Unknown => MatchStatus::UnknownUpdate,
    }
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
    fn exact_ui_name_beats_bundled_library_directory() {
        let result = match_one(
            &local("LibAddonMenu-2.0", Some("LibAddonMenu-2.0"), Some("43")),
            &[
                remote(
                    "1135",
                    "Provision's TeamFormation : Teammate Radar",
                    "1",
                    &["TeamFormation", "LibAddonMenu-2.0"],
                ),
                remote("7", "LibAddonMenu-2.0", "43", &["LibAddonMenu-2.0"]),
            ],
        );

        assert_ne!(result.status, MatchStatus::Ambiguous);
        assert_eq!(result.remote.unwrap().uid.as_deref(), Some("7"));
    }

    #[test]
    fn exact_title_ui_name_beats_bundled_directory_match() {
        let result = match_one(
            &local("SomeFolder", Some("LibAddonMenu-2.0"), Some("43")),
            &[
                remote(
                    "1135",
                    "Provision's TeamFormation : Teammate Radar",
                    "1",
                    &["TeamFormation", "SomeFolder"],
                ),
                remote("7", "LibAddonMenu-2.0", "43", &["OtherFolder"]),
            ],
        );

        assert_eq!(result.remote.unwrap().uid.as_deref(), Some("7"));
    }

    #[test]
    fn exact_folder_ui_name_beats_bundled_directory_match() {
        let result = match_one(
            &local("LibAddonMenu-2.0", Some("Other Title"), Some("43")),
            &[
                remote(
                    "1135",
                    "Provision's TeamFormation : Teammate Radar",
                    "1",
                    &["TeamFormation", "LibAddonMenu-2.0"],
                ),
                remote("7", "LibAddonMenu-2.0", "43", &["OtherFolder"]),
            ],
        );

        assert_eq!(result.remote.unwrap().uid.as_deref(), Some("7"));
    }

    #[test]
    fn bundled_only_match_still_matches_as_lower_tier() {
        let result = match_one(
            &local("LibAddonMenu-2.0", Some("Other Title"), Some("43")),
            &[remote(
                "1135",
                "Provision's TeamFormation : Teammate Radar",
                "1",
                &["TeamFormation", "LibAddonMenu-2.0"],
            )],
        );

        let remote = result.remote.unwrap();
        assert_eq!(remote.uid.as_deref(), Some("1135"));
        assert_eq!(remote.tier, 3);
        assert_eq!(remote.reason, "bundled-directory");
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
    fn dotted_version_comparison() {
        let result = match_one(
            &local("Addon", Some("Addon"), Some("1.0")),
            &[remote("1", "Addon", "1.1", &["Addon"])],
        );

        assert_eq!(result.status, MatchStatus::PossibleUpdate);
    }

    #[test]
    fn addon_version_matches_remote_release_marker() {
        let result = match_one(
            &local("LibAddonMenu-2.0", Some("LibAddonMenu-2.0"), Some("43")),
            &[remote(
                "7",
                "LibAddonMenu-2.0",
                "2.0 r43",
                &["LibAddonMenu-2.0"],
            )],
        );

        assert_eq!(result.status, MatchStatus::Matched);
    }

    #[test]
    fn local_newer_version_status() {
        let result = match_one(
            &local("Addon", Some("Addon"), Some("44")),
            &[remote("1", "Addon", "2.0 r43", &["Addon"])],
        );

        assert_eq!(result.status, MatchStatus::LocalNewer);
    }
}
