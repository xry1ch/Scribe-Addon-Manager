use std::collections::{BTreeMap, BTreeSet};

use crate::api::models::AddonSummary;
use crate::install::zip_safety::ExtractedZip;
use crate::local::LocalAddon;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestDependency {
    pub name: String,
    pub constraint: Option<String>,
    pub raw: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencyStatus {
    AlreadyInstalled,
    WillInstall,
    NotInstalled,
    Unresolved,
    Ambiguous,
}

impl DependencyStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AlreadyInstalled => "already-installed",
            Self::WillInstall => "will-install",
            Self::NotInstalled => "not-installed",
            Self::Unresolved => "unresolved",
            Self::Ambiguous => "ambiguous",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyPlanEntry {
    pub name: String,
    pub constraint: Option<String>,
    pub raw: String,
    pub status: DependencyStatus,
    pub remote_uid: Option<String>,
    pub remote_name: Option<String>,
    pub installed_folder: Option<String>,
    pub bundled_folder: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencyInstallRole {
    MainAddon,
    RequiredDependency,
}

impl DependencyInstallRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MainAddon => "main-addon",
            Self::RequiredDependency => "required-dependency",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyInstallItem {
    pub role: DependencyInstallRole,
    pub name: String,
    pub remote_uid: Option<String>,
    pub remote_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteAddonRef {
    pub uid: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledRemoteAddon {
    pub folder_name: String,
    pub remote_uid: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyPlan {
    pub main_addon: RemoteAddonRef,
    pub required_dependencies: Vec<DependencyPlanEntry>,
    pub optional_dependencies: Vec<DependencyPlanEntry>,
    pub install_items: Vec<DependencyInstallItem>,
}

impl DependencyPlan {
    pub fn has_unresolved_required_dependencies(&self) -> bool {
        self.required_dependencies.iter().any(|dependency| {
            matches!(
                dependency.status,
                DependencyStatus::Unresolved | DependencyStatus::Ambiguous
            )
        })
    }

    pub fn required_dependencies_to_install(&self) -> Vec<&DependencyPlanEntry> {
        self.required_dependencies
            .iter()
            .filter(|dependency| {
                dependency.status == DependencyStatus::WillInstall
                    && dependency.remote_uid.is_some()
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DependencyRemoteCandidate {
    uid: String,
    name: Option<String>,
    library_category: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DependencyResolution {
    Resolved(DependencyRemoteCandidate),
    Unresolved,
    Ambiguous,
}

pub fn parse_dependency_values(value: &str) -> Vec<ManifestDependency> {
    let tokens = value
        .split(|ch: char| ch == ',' || ch == ';' || ch.is_whitespace())
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    let mut dependencies: Vec<ManifestDependency> = Vec::new();
    let mut index = 0;

    while index < tokens.len() {
        let token = tokens[index];

        if is_operator(token) {
            if let Some(last) = dependencies.last_mut() {
                let (constraint, raw_suffix, consumed) =
                    constraint_from_operator(token, &tokens, index);
                last.constraint = Some(constraint);
                last.raw = format!("{} {}", last.raw, raw_suffix);
                index += consumed;
                continue;
            }

            index += 1;
            continue;
        }

        let (name, constraint, mut raw, consumed) = dependency_from_token(token, &tokens, index);
        if !name.is_empty() {
            if let Some(constraint) = constraint.as_ref() {
                if raw == token && constraint == token {
                    raw = format!("{name}{constraint}");
                }
            }

            dependencies.push(ManifestDependency {
                name,
                constraint,
                raw,
            });
        }
        index += consumed;
    }

    dependencies
}

pub fn build_dependency_plan(
    main_addon: RemoteAddonRef,
    extracted: &ExtractedZip,
    installed_addons: &[LocalAddon],
    remote_addons: &[AddonSummary],
    installed_remotes: &[InstalledRemoteAddon],
) -> DependencyPlan {
    let required = collect_dependencies(
        extracted
            .detected_addons
            .iter()
            .filter(|addon| addon.valid_manifest)
            .flat_map(|addon| addon.depends_on.iter()),
    );
    let required_names = required
        .iter()
        .map(|dependency| normalize_key(&dependency.name))
        .collect::<BTreeSet<_>>();
    let optional = collect_dependencies(
        extracted
            .detected_addons
            .iter()
            .filter(|addon| addon.valid_manifest)
            .flat_map(|addon| addon.optional_depends_on.iter()),
    )
    .into_iter()
    .filter(|dependency| !required_names.contains(&normalize_key(&dependency.name)))
    .collect::<Vec<_>>();

    let mut install_items = Vec::new();
    let required_dependencies = required
        .into_iter()
        .map(|dependency| {
            let entry = plan_required_dependency(
                dependency,
                extracted,
                installed_addons,
                remote_addons,
                installed_remotes,
            );
            if entry.status == DependencyStatus::WillInstall && entry.remote_uid.is_some() {
                install_items.push(DependencyInstallItem {
                    role: DependencyInstallRole::RequiredDependency,
                    name: entry.name.clone(),
                    remote_uid: entry.remote_uid.clone(),
                    remote_name: entry.remote_name.clone(),
                });
            }
            entry
        })
        .collect::<Vec<_>>();

    install_items.push(DependencyInstallItem {
        role: DependencyInstallRole::MainAddon,
        name: main_addon
            .name
            .clone()
            .unwrap_or_else(|| main_addon.uid.clone()),
        remote_uid: Some(main_addon.uid.clone()),
        remote_name: main_addon.name.clone(),
    });

    let optional_dependencies = optional
        .into_iter()
        .map(|dependency| {
            plan_optional_dependency(
                dependency,
                installed_addons,
                remote_addons,
                installed_remotes,
            )
        })
        .collect();

    DependencyPlan {
        main_addon,
        required_dependencies,
        optional_dependencies,
        install_items,
    }
}

fn plan_required_dependency(
    dependency: ManifestDependency,
    extracted: &ExtractedZip,
    installed_addons: &[LocalAddon],
    remote_addons: &[AddonSummary],
    installed_remotes: &[InstalledRemoteAddon],
) -> DependencyPlanEntry {
    if let Some(folder) = find_matching_local_addon(&dependency.name, installed_addons) {
        return entry(
            dependency,
            DependencyStatus::AlreadyInstalled,
            None,
            None,
            Some(folder),
            None,
        );
    }

    if let Some(folder) = find_matching_local_addon(&dependency.name, &extracted.detected_addons) {
        return entry(
            dependency,
            DependencyStatus::WillInstall,
            None,
            Some("Bundled in package".to_owned()),
            None,
            Some(folder),
        );
    }

    match resolve_dependency(&dependency.name, remote_addons) {
        DependencyResolution::Resolved(remote) => {
            if let Some(folder) = find_installed_remote(&remote.uid, installed_remotes) {
                entry(
                    dependency,
                    DependencyStatus::AlreadyInstalled,
                    Some(remote.uid),
                    remote.name,
                    Some(folder),
                    None,
                )
            } else {
                entry(
                    dependency,
                    DependencyStatus::WillInstall,
                    Some(remote.uid),
                    remote.name,
                    None,
                    None,
                )
            }
        }
        DependencyResolution::Ambiguous => entry(
            dependency,
            DependencyStatus::Ambiguous,
            None,
            None,
            None,
            None,
        ),
        DependencyResolution::Unresolved => entry(
            dependency,
            DependencyStatus::Unresolved,
            None,
            None,
            None,
            None,
        ),
    }
}

fn plan_optional_dependency(
    dependency: ManifestDependency,
    installed_addons: &[LocalAddon],
    remote_addons: &[AddonSummary],
    installed_remotes: &[InstalledRemoteAddon],
) -> DependencyPlanEntry {
    if let Some(folder) = find_matching_local_addon(&dependency.name, installed_addons) {
        return entry(
            dependency,
            DependencyStatus::AlreadyInstalled,
            None,
            None,
            Some(folder),
            None,
        );
    }

    match resolve_dependency(&dependency.name, remote_addons) {
        DependencyResolution::Resolved(remote) => {
            if let Some(folder) = find_installed_remote(&remote.uid, installed_remotes) {
                entry(
                    dependency,
                    DependencyStatus::AlreadyInstalled,
                    Some(remote.uid),
                    remote.name,
                    Some(folder),
                    None,
                )
            } else {
                entry(
                    dependency,
                    DependencyStatus::NotInstalled,
                    Some(remote.uid),
                    remote.name,
                    None,
                    None,
                )
            }
        }
        DependencyResolution::Ambiguous | DependencyResolution::Unresolved => entry(
            dependency,
            DependencyStatus::Unresolved,
            None,
            None,
            None,
            None,
        ),
    }
}

fn entry(
    dependency: ManifestDependency,
    status: DependencyStatus,
    remote_uid: Option<String>,
    remote_name: Option<String>,
    installed_folder: Option<String>,
    bundled_folder: Option<String>,
) -> DependencyPlanEntry {
    DependencyPlanEntry {
        name: dependency.name,
        constraint: dependency.constraint,
        raw: dependency.raw,
        status,
        remote_uid,
        remote_name,
        installed_folder,
        bundled_folder,
    }
}

fn collect_dependencies<'a>(
    raw_values: impl Iterator<Item = &'a String>,
) -> Vec<ManifestDependency> {
    let mut seen = BTreeSet::new();
    let mut dependencies = Vec::new();

    for raw in raw_values {
        for dependency in parse_dependency_values(raw) {
            if seen.insert(normalize_key(&dependency.name)) {
                dependencies.push(dependency);
            }
        }
    }

    dependencies
}

fn resolve_dependency(name: &str, remote_addons: &[AddonSummary]) -> DependencyResolution {
    resolve_by(remote_addons, |addon| exact_ci(addon.name.as_deref(), name))
        .or_else(|| {
            resolve_by(remote_addons, |addon| {
                addon
                    .directories
                    .first()
                    .is_some_and(|directory| exact_ci(Some(directory.as_str()), name))
            })
        })
        .or_else(|| {
            resolve_by(remote_addons, |addon| {
                addon
                    .directories
                    .iter()
                    .any(|directory| exact_ci(Some(directory.as_str()), name))
            })
        })
        .or_else(|| {
            let normalized = normalize_identity(name);
            resolve_by(remote_addons, |addon| {
                addon
                    .name
                    .as_deref()
                    .is_some_and(|remote_name| normalize_identity(remote_name) == normalized)
                    || addon
                        .directories
                        .iter()
                        .any(|directory| normalize_identity(directory) == normalized)
            })
        })
        .unwrap_or(DependencyResolution::Unresolved)
}

fn resolve_by(
    remote_addons: &[AddonSummary],
    predicate: impl Fn(&AddonSummary) -> bool,
) -> Option<DependencyResolution> {
    let candidates = remote_addons
        .iter()
        .filter(|addon| predicate(addon))
        .filter_map(remote_candidate)
        .collect::<Vec<_>>();

    choose_candidate(candidates)
}

fn remote_candidate(addon: &AddonSummary) -> Option<DependencyRemoteCandidate> {
    Some(DependencyRemoteCandidate {
        uid: addon.uid.clone()?,
        name: addon.name.clone(),
        library_category: addon
            .category_name()
            .is_some_and(|name| name.to_lowercase().contains("librar")),
    })
}

fn choose_candidate(candidates: Vec<DependencyRemoteCandidate>) -> Option<DependencyResolution> {
    if candidates.is_empty() {
        return None;
    }

    let mut by_uid = BTreeMap::new();
    for candidate in candidates {
        by_uid.entry(candidate.uid.clone()).or_insert(candidate);
    }
    let mut candidates = by_uid.into_values().collect::<Vec<_>>();
    let library_candidates = candidates
        .iter()
        .filter(|candidate| candidate.library_category)
        .cloned()
        .collect::<Vec<_>>();
    if !library_candidates.is_empty() {
        candidates = library_candidates;
    }

    if candidates.len() == 1 {
        Some(DependencyResolution::Resolved(
            candidates.into_iter().next().expect("candidate"),
        ))
    } else {
        Some(DependencyResolution::Ambiguous)
    }
}

fn find_matching_local_addon(name: &str, addons: &[LocalAddon]) -> Option<String> {
    addons
        .iter()
        .find(|addon| {
            exact_ci(Some(addon.folder_name.as_str()), name)
                || addon
                    .title
                    .as_deref()
                    .is_some_and(|title| exact_ci(Some(title), name))
        })
        .map(|addon| addon.folder_name.clone())
}

fn find_installed_remote(uid: &str, installed_remotes: &[InstalledRemoteAddon]) -> Option<String> {
    installed_remotes
        .iter()
        .find(|installed| installed.remote_uid == uid)
        .map(|installed| installed.folder_name.clone())
}

fn dependency_from_token(
    token: &str,
    tokens: &[&str],
    index: usize,
) -> (String, Option<String>, String, usize) {
    if let Some((name, operator, version)) = split_inline_constraint(token) {
        let mut consumed = 1;
        let mut raw = token.to_owned();
        let constraint = if version.is_empty() && index + 1 < tokens.len() {
            consumed = 2;
            raw = format!("{token} {}", tokens[index + 1]);
            Some(format!("{} {}", operator, tokens[index + 1]))
        } else if version.is_empty() {
            Some(operator.to_owned())
        } else {
            Some(format!("{operator}{version}"))
        };
        return (name.to_owned(), constraint, raw, consumed);
    }

    (token.to_owned(), None, token.to_owned(), 1)
}

fn constraint_from_operator(
    operator: &str,
    tokens: &[&str],
    index: usize,
) -> (String, String, usize) {
    if index + 1 < tokens.len() && !is_operator(tokens[index + 1]) {
        (
            format!("{} {}", operator, tokens[index + 1]),
            format!("{} {}", operator, tokens[index + 1]),
            2,
        )
    } else {
        (operator.to_owned(), operator.to_owned(), 1)
    }
}

fn split_inline_constraint(token: &str) -> Option<(&str, &str, &str)> {
    for operator in [">=", "<=", "==", ">", "<", "="] {
        if let Some(index) = token.find(operator) {
            let name = token[..index].trim();
            let version = token[index + operator.len()..].trim();
            if !name.is_empty() {
                return Some((name, operator, version));
            }
        }
    }

    None
}

fn is_operator(token: &str) -> bool {
    matches!(token, ">=" | "<=" | "==" | ">" | "<" | "=")
}

fn exact_ci(value: Option<&str>, expected: &str) -> bool {
    value.is_some_and(|value| value.trim().eq_ignore_ascii_case(expected.trim()))
}

fn normalize_key(value: &str) -> String {
    value.trim().to_lowercase()
}

fn normalize_identity(value: &str) -> String {
    let mut normalized = String::new();
    let mut previous_was_space = false;

    for ch in value.chars().flat_map(char::to_lowercase) {
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use crate::api::models::AddonSummary;
    use crate::install::dependencies::{
        build_dependency_plan, parse_dependency_values, DependencyInstallRole, DependencyStatus,
        InstalledRemoteAddon, RemoteAddonRef,
    };
    use crate::install::zip_safety::{ExtractedZip, ZipInspection};
    use crate::local::LocalAddon;

    fn local(
        folder_name: &str,
        title: Option<&str>,
        depends_on: &[&str],
        optional: &[&str],
    ) -> LocalAddon {
        LocalAddon {
            folder_name: folder_name.to_owned(),
            folder_path: PathBuf::from(folder_name),
            manifest_path: None,
            title: title.map(ToOwned::to_owned),
            addon_version: Some("1".to_owned()),
            version: None,
            api_versions: Vec::new(),
            depends_on: depends_on.iter().map(|value| (*value).to_owned()).collect(),
            optional_depends_on: optional.iter().map(|value| (*value).to_owned()).collect(),
            saved_variables: Vec::new(),
            saved_variables_per_character: Vec::new(),
            is_library: None,
            author: None,
            description: None,
            valid_manifest: true,
        }
    }

    fn remote(uid: &str, name: &str, directories: &[&str]) -> AddonSummary {
        AddonSummary {
            uid: Some(uid.to_owned()),
            name: Some(name.to_owned()),
            author_name: None,
            version: Some("1".to_owned()),
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

    fn main_ref() -> RemoteAddonRef {
        RemoteAddonRef {
            uid: "main".to_owned(),
            name: Some("Main Addon".to_owned()),
        }
    }

    #[test]
    fn parses_dependson_plain_names() {
        let parsed = parse_dependency_values("LibAddonMenu-2.0");

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "LibAddonMenu-2.0");
        assert_eq!(parsed[0].constraint, None);
        assert_eq!(parsed[0].raw, "LibAddonMenu-2.0");
    }

    #[test]
    fn parses_dependson_version_constraints() {
        let parsed = parse_dependency_values("LibAddonMenu-2.0 >= 41 LibAsync>=1.0");

        assert_eq!(
            parsed
                .iter()
                .map(|dependency| (
                    dependency.name.as_str(),
                    dependency.constraint.as_deref(),
                    dependency.raw.as_str()
                ))
                .collect::<Vec<_>>(),
            vec![
                ("LibAddonMenu-2.0", Some(">= 41"), "LibAddonMenu-2.0 >= 41"),
                ("LibAsync", Some(">=1.0"), "LibAsync>=1.0"),
            ]
        );
    }

    #[test]
    fn parses_multiple_dependencies() {
        let parsed = parse_dependency_values("LibDebugLogger LibAddonMenu-2.0>=41");

        assert_eq!(
            parsed
                .iter()
                .map(|dependency| dependency.name.as_str())
                .collect::<Vec<_>>(),
            vec!["LibDebugLogger", "LibAddonMenu-2.0"]
        );
    }

    #[test]
    fn optional_dependencies_are_not_auto_installed() {
        let plan = build_dependency_plan(
            main_ref(),
            &extracted(vec![local(
                "MainAddon",
                Some("Main"),
                &[],
                &["LibOptional"],
            )]),
            &[],
            &[remote("7", "LibOptional", &["LibOptional"])],
            &[],
        );

        assert!(plan.required_dependencies.is_empty());
        assert_eq!(
            plan.optional_dependencies[0].status,
            DependencyStatus::NotInstalled
        );
        assert_eq!(plan.install_items.len(), 1);
        assert_eq!(plan.install_items[0].role, DependencyInstallRole::MainAddon);
    }

    #[test]
    fn installed_dependency_is_recognized() {
        let plan = build_dependency_plan(
            main_ref(),
            &extracted(vec![local(
                "MainAddon",
                Some("Main"),
                &["LibInstalled"],
                &[],
            )]),
            &[local("LibInstalled", Some("Different Title"), &[], &[])],
            &[remote("7", "LibInstalled", &["LibInstalled"])],
            &[],
        );

        assert_eq!(
            plan.required_dependencies[0].status,
            DependencyStatus::AlreadyInstalled
        );
        assert_eq!(
            plan.required_dependencies[0].installed_folder.as_deref(),
            Some("LibInstalled")
        );
        assert_eq!(plan.install_items.len(), 1);
    }

    #[test]
    fn installed_metadata_satisfies_resolved_dependency() {
        let plan = build_dependency_plan(
            main_ref(),
            &extracted(vec![local("MainAddon", Some("Main"), &["LibMapped"], &[])]),
            &[local("RenamedLibFolder", Some("Renamed"), &[], &[])],
            &[remote("7", "LibMapped", &["LibMapped"])],
            &[InstalledRemoteAddon {
                folder_name: "RenamedLibFolder".to_owned(),
                remote_uid: "7".to_owned(),
            }],
        );

        assert_eq!(
            plan.required_dependencies[0].status,
            DependencyStatus::AlreadyInstalled
        );
        assert_eq!(
            plan.required_dependencies[0].installed_folder.as_deref(),
            Some("RenamedLibFolder")
        );
    }

    #[test]
    fn exact_remote_dependency_resolves() {
        let plan = build_dependency_plan(
            main_ref(),
            &extracted(vec![local(
                "MainAddon",
                Some("Main"),
                &["LibAddonMenu-2.0"],
                &[],
            )]),
            &[],
            &[remote("7", "LibAddonMenu-2.0", &["LibAddonMenu-2.0"])],
            &[],
        );

        assert_eq!(
            plan.required_dependencies[0].status,
            DependencyStatus::WillInstall
        );
        assert_eq!(
            plan.required_dependencies[0].remote_uid.as_deref(),
            Some("7")
        );
        assert_eq!(
            plan.install_items[0].role,
            DependencyInstallRole::RequiredDependency
        );
    }

    #[test]
    fn ambiguous_dependency_does_not_auto_install() {
        let plan = build_dependency_plan(
            main_ref(),
            &extracted(vec![local("MainAddon", Some("Main"), &["LibFoo"], &[])]),
            &[],
            &[
                remote("7", "LibFoo", &["LibFoo"]),
                remote("8", "LibFoo", &["Other"]),
            ],
            &[],
        );

        assert_eq!(
            plan.required_dependencies[0].status,
            DependencyStatus::Ambiguous
        );
        assert_eq!(plan.install_items.len(), 1);
        assert_eq!(plan.install_items[0].role, DependencyInstallRole::MainAddon);
    }

    #[test]
    fn unresolved_dependency_requires_explicit_confirmation() {
        let plan = build_dependency_plan(
            main_ref(),
            &extracted(vec![local("MainAddon", Some("Main"), &["MissingLib"], &[])]),
            &[],
            &[],
            &[],
        );

        assert_eq!(
            plan.required_dependencies[0].status,
            DependencyStatus::Unresolved
        );
        assert!(plan.has_unresolved_required_dependencies());
    }
}
