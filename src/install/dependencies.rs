use std::collections::{BTreeMap, BTreeSet};

use crate::api::models::AddonSummary;
use crate::install::dependency_graph::{
    build_dependency_graph, DependencyEdgeKind, DependencyGraph, DependencyGraphOptions,
    DependencyManifestSource, DependencyNode, DependencyResolutionStatus,
    DEFAULT_MAX_DEPENDENCY_DEPTH,
};
use crate::install::zip_safety::ExtractedZip;
use crate::local::LocalAddon;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestDependency {
    pub name: String,
    pub constraint: Option<String>,
    pub raw: String,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencyStatus {
    AlreadyInstalled,
    WillInstall,
    NotInstalled,
    Unresolved,
    Ambiguous,
    Circular,
    MaxDepth,
}

impl DependencyStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AlreadyInstalled => "already-installed",
            Self::WillInstall => "will-install",
            Self::NotInstalled => "not-installed",
            Self::Unresolved => "unresolved",
            Self::Ambiguous => "ambiguous",
            Self::Circular => "circular",
            Self::MaxDepth => "max-depth",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyPlanEntry {
    pub name: String,
    pub constraint: Option<String>,
    pub raw: String,
    pub required: bool,
    pub relation: DependencyEdgeKind,
    pub depth: usize,
    pub parent: Option<String>,
    pub status: DependencyStatus,
    pub remote_uid: Option<String>,
    pub remote_name: Option<String>,
    pub remote_version: Option<String>,
    pub installed_folder: Option<String>,
    pub installed_title: Option<String>,
    pub installed_version: Option<String>,
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
    pub install_order: Vec<String>,
    pub graph: DependencyGraph,
}

impl DependencyPlan {
    pub fn has_unresolved_required_dependencies(&self) -> bool {
        self.required_dependencies.iter().any(|dependency| {
            matches!(
                dependency.status,
                DependencyStatus::Unresolved
                    | DependencyStatus::Ambiguous
                    | DependencyStatus::Circular
                    | DependencyStatus::MaxDepth
            )
        })
    }

    pub fn required_dependencies_to_install(&self) -> Vec<&DependencyPlanEntry> {
        self.install_items
            .iter()
            .filter(|item| item.role == DependencyInstallRole::RequiredDependency)
            .filter_map(|item| {
                let remote_uid = item.remote_uid.as_deref()?;
                self.required_dependencies.iter().find(|dependency| {
                    dependency.status == DependencyStatus::WillInstall
                        && dependency.remote_uid.as_deref() == Some(remote_uid)
                })
            })
            .collect()
    }

    pub fn required_remote_manifests_to_fetch(
        &self,
        remote_sources: &BTreeMap<String, DependencyManifestSource>,
    ) -> Vec<String> {
        self.graph
            .required_remote_uids_missing_sources(remote_sources)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DependencyRemoteCandidate {
    pub(crate) uid: String,
    pub(crate) name: Option<String>,
    pub(crate) version: Option<String>,
    library_category: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DependencyResolution {
    Resolved(DependencyRemoteCandidate),
    Unresolved,
    Ambiguous,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DependencyAvailability {
    Installed,
    Missing,
    Unknown,
    Ambiguous,
    Circular,
    MaxDepth,
}

impl DependencyAvailability {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Installed => "installed",
            Self::Missing => "missing",
            Self::Unknown => "unknown",
            Self::Ambiguous => "ambiguous",
            Self::Circular => "circular",
            Self::MaxDepth => "max-depth",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyStatusEntry {
    pub name: String,
    pub raw: String,
    pub constraint: Option<String>,
    pub required: bool,
    pub relation: DependencyEdgeKind,
    pub depth: usize,
    pub parent: Option<String>,
    pub installed: bool,
    pub installed_folder: Option<String>,
    pub installed_title: Option<String>,
    pub installed_version: Option<String>,
    pub remote_uid: Option<String>,
    pub remote_name: Option<String>,
    pub remote_version: Option<String>,
    pub status: DependencyAvailability,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyStatusReport {
    pub required_dependencies: Vec<DependencyStatusEntry>,
    pub optional_dependencies: Vec<DependencyStatusEntry>,
}

pub fn parse_dependency_values(value: &str) -> Vec<ManifestDependency> {
    parse_dependency_values_with_required(value, true)
}

pub fn parse_required_dependency_values(value: &str) -> Vec<ManifestDependency> {
    parse_dependency_values_with_required(value, true)
}

pub fn parse_optional_dependency_values(value: &str) -> Vec<ManifestDependency> {
    parse_dependency_values_with_required(value, false)
}

fn parse_dependency_values_with_required(value: &str, required: bool) -> Vec<ManifestDependency> {
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
                required,
            });
        }
        index += consumed;
    }

    dependencies
}

pub fn resolve_manifest_dependencies(
    required_values: &[String],
    optional_values: &[String],
    installed_addons: &[LocalAddon],
    remote_addons: Option<&[AddonSummary]>,
    installed_remotes: &[InstalledRemoteAddon],
) -> DependencyStatusReport {
    build_dependency_status_report_from_source(
        RemoteAddonRef {
            uid: "installed-addon".to_owned(),
            name: Some("Selected addon".to_owned()),
        },
        &DependencyManifestSource::from_dependency_values(required_values, optional_values),
        installed_addons,
        remote_addons,
        installed_remotes,
    )
}

pub fn build_dependency_plan(
    main_addon: RemoteAddonRef,
    extracted: &ExtractedZip,
    installed_addons: &[LocalAddon],
    remote_addons: &[AddonSummary],
    installed_remotes: &[InstalledRemoteAddon],
) -> DependencyPlan {
    build_dependency_plan_with_remote_sources(
        main_addon,
        &DependencyManifestSource::from_extracted(extracted),
        installed_addons,
        remote_addons,
        installed_remotes,
        &BTreeMap::new(),
        DEFAULT_MAX_DEPENDENCY_DEPTH,
    )
}

pub fn build_dependency_plan_with_remote_sources(
    main_addon: RemoteAddonRef,
    main_source: &DependencyManifestSource,
    installed_addons: &[LocalAddon],
    remote_addons: &[AddonSummary],
    installed_remotes: &[InstalledRemoteAddon],
    remote_sources: &BTreeMap<String, DependencyManifestSource>,
    max_depth: usize,
) -> DependencyPlan {
    let graph = build_dependency_graph(
        &main_addon,
        main_source,
        installed_addons,
        remote_addons,
        installed_remotes,
        remote_sources,
        DependencyGraphOptions { max_depth },
    );
    dependency_plan_from_graph(main_addon, graph)
}

pub fn build_dependency_status_report_from_source(
    main_addon: RemoteAddonRef,
    main_source: &DependencyManifestSource,
    installed_addons: &[LocalAddon],
    remote_addons: Option<&[AddonSummary]>,
    installed_remotes: &[InstalledRemoteAddon],
) -> DependencyStatusReport {
    let remote_lookup_unavailable = remote_addons.is_none();
    let remote_addons = remote_addons.unwrap_or(&[]);
    let graph = build_dependency_graph(
        &main_addon,
        main_source,
        installed_addons,
        remote_addons,
        installed_remotes,
        &BTreeMap::new(),
        DependencyGraphOptions {
            max_depth: DEFAULT_MAX_DEPENDENCY_DEPTH,
        },
    );

    let required_dependencies = graph
        .nodes
        .iter()
        .filter(|node| node.required)
        .map(|node| dependency_status_entry_from_node(node, remote_lookup_unavailable))
        .collect();
    let optional_dependencies = graph
        .nodes
        .iter()
        .filter(|node| !node.required)
        .map(|node| dependency_status_entry_from_node(node, remote_lookup_unavailable))
        .collect();

    DependencyStatusReport {
        required_dependencies,
        optional_dependencies,
    }
}

fn dependency_plan_from_graph(
    main_addon: RemoteAddonRef,
    graph: DependencyGraph,
) -> DependencyPlan {
    let required_dependencies = graph
        .nodes
        .iter()
        .filter(|node| node.required)
        .map(dependency_plan_entry_from_node)
        .collect::<Vec<_>>();
    let optional_dependencies = graph
        .nodes
        .iter()
        .filter(|node| !node.required)
        .map(dependency_plan_entry_from_node)
        .collect::<Vec<_>>();
    let mut install_items = graph
        .required_install_order()
        .into_iter()
        .map(|node| DependencyInstallItem {
            role: DependencyInstallRole::RequiredDependency,
            name: node
                .remote_name
                .clone()
                .unwrap_or_else(|| node.name.clone()),
            remote_uid: node.remote_uid.clone(),
            remote_name: node.remote_name.clone(),
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
    let install_order = install_items
        .iter()
        .map(|item| item.name.clone())
        .collect::<Vec<_>>();

    DependencyPlan {
        main_addon,
        required_dependencies,
        optional_dependencies,
        install_items,
        install_order,
        graph,
    }
}

fn dependency_plan_entry_from_node(node: &DependencyNode) -> DependencyPlanEntry {
    DependencyPlanEntry {
        name: node.name.clone(),
        constraint: node.constraint.clone(),
        raw: node.raw.clone(),
        required: node.required,
        relation: node.relation,
        depth: node.depth,
        parent: node.parent.clone(),
        status: dependency_status_from_graph_status(node.status),
        remote_uid: node.remote_uid.clone(),
        remote_name: node.remote_name.clone(),
        remote_version: node.remote_version.clone(),
        installed_folder: node.installed_folder.clone(),
        installed_title: node.installed_title.clone(),
        installed_version: node.installed_version.clone(),
        bundled_folder: node.bundled_folder.clone(),
    }
}

fn dependency_status_from_graph_status(status: DependencyResolutionStatus) -> DependencyStatus {
    match status {
        DependencyResolutionStatus::Installed => DependencyStatus::AlreadyInstalled,
        DependencyResolutionStatus::Missing => DependencyStatus::NotInstalled,
        DependencyResolutionStatus::WillInstall => DependencyStatus::WillInstall,
        DependencyResolutionStatus::Unresolved => DependencyStatus::Unresolved,
        DependencyResolutionStatus::Ambiguous => DependencyStatus::Ambiguous,
        DependencyResolutionStatus::Circular => DependencyStatus::Circular,
        DependencyResolutionStatus::MaxDepth => DependencyStatus::MaxDepth,
    }
}

fn dependency_status_entry_from_node(
    node: &DependencyNode,
    remote_lookup_unavailable: bool,
) -> DependencyStatusEntry {
    let status = match node.status {
        DependencyResolutionStatus::Installed => DependencyAvailability::Installed,
        DependencyResolutionStatus::Ambiguous => DependencyAvailability::Ambiguous,
        DependencyResolutionStatus::Circular => DependencyAvailability::Circular,
        DependencyResolutionStatus::MaxDepth => DependencyAvailability::MaxDepth,
        _ if remote_lookup_unavailable && node.remote_uid.is_none() => {
            DependencyAvailability::Unknown
        }
        _ => DependencyAvailability::Missing,
    };

    DependencyStatusEntry {
        name: node.name.clone(),
        raw: node.raw.clone(),
        constraint: node.constraint.clone(),
        required: node.required,
        relation: node.relation,
        depth: node.depth,
        parent: node.parent.clone(),
        installed: status == DependencyAvailability::Installed,
        installed_folder: node.installed_folder.clone(),
        installed_title: node.installed_title.clone(),
        installed_version: node.installed_version.clone(),
        remote_uid: node.remote_uid.clone(),
        remote_name: node.remote_name.clone(),
        remote_version: node.remote_version.clone(),
        status,
    }
}

pub(crate) fn collect_dependencies<'a>(
    raw_values: impl Iterator<Item = &'a String>,
    required: bool,
) -> Vec<ManifestDependency> {
    let mut seen = BTreeSet::new();
    let mut dependencies = Vec::new();

    for raw in raw_values {
        for dependency in parse_dependency_values_with_required(raw, required) {
            if seen.insert(normalize_key(&dependency.name)) {
                dependencies.push(dependency);
            }
        }
    }

    dependencies
}

pub(crate) fn resolve_dependency(
    name: &str,
    remote_addons: &[AddonSummary],
) -> DependencyResolution {
    resolve_dependency_exact(name, remote_addons)
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

fn resolve_dependency_exact(
    name: &str,
    remote_addons: &[AddonSummary],
) -> Option<DependencyResolution> {
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
        version: addon.version.clone(),
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

pub(crate) fn find_matching_local_addon_details<'a>(
    name: &str,
    addons: &'a [LocalAddon],
) -> Option<&'a LocalAddon> {
    addons.iter().find(|addon| {
        exact_ci(Some(addon.folder_name.as_str()), name)
            || addon
                .title
                .as_deref()
                .is_some_and(|title| exact_ci(Some(title), name))
    })
}

pub(crate) fn find_installed_remote_addon<'a>(
    uid: &str,
    installed_remotes: &[InstalledRemoteAddon],
    installed_addons: &'a [LocalAddon],
) -> Option<&'a LocalAddon> {
    let installed = installed_remotes
        .iter()
        .find(|installed| installed.remote_uid == uid)?;
    installed_addons.iter().find(|addon| {
        addon
            .folder_name
            .eq_ignore_ascii_case(installed.folder_name.as_str())
    })
}

pub(crate) fn remote_for_installed_addon(
    addon: &LocalAddon,
    remote_addons: Option<&[AddonSummary]>,
    installed_remotes: &[InstalledRemoteAddon],
) -> Option<DependencyRemoteCandidate> {
    let remote_uid = installed_remotes
        .iter()
        .find(|installed| {
            installed
                .folder_name
                .eq_ignore_ascii_case(addon.folder_name.as_str())
        })?
        .remote_uid
        .as_str();

    remote_addons?
        .iter()
        .find(|remote| remote.uid.as_deref() == Some(remote_uid))
        .and_then(remote_candidate)
}

pub(crate) fn local_addon_display_version(addon: &LocalAddon) -> Option<String> {
    addon
        .addon_version
        .clone()
        .or_else(|| addon.version.clone())
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

pub(crate) fn normalize_key(value: &str) -> String {
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
        build_dependency_plan, build_dependency_plan_with_remote_sources, parse_dependency_values,
        parse_optional_dependency_values, resolve_manifest_dependencies, DependencyAvailability,
        DependencyInstallRole, DependencyStatus, InstalledRemoteAddon, RemoteAddonRef,
    };
    use crate::install::dependency_graph::{
        DependencyManifestSource, DEFAULT_MAX_DEPENDENCY_DEPTH,
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

    fn source(addons: Vec<LocalAddon>) -> DependencyManifestSource {
        DependencyManifestSource::from_addons(addons)
    }

    fn recursive_plan(
        main_addon: LocalAddon,
        remote_addons: &[AddonSummary],
        remote_sources: BTreeMap<String, DependencyManifestSource>,
        max_depth: usize,
    ) -> crate::install::dependencies::DependencyPlan {
        build_dependency_plan_with_remote_sources(
            main_ref(),
            &source(vec![main_addon]),
            &[],
            remote_addons,
            &[],
            &remote_sources,
            max_depth,
        )
    }

    #[test]
    fn parses_dependson_plain_names() {
        let parsed = parse_dependency_values("LibAddonMenu-2.0");

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "LibAddonMenu-2.0");
        assert_eq!(parsed[0].constraint, None);
        assert_eq!(parsed[0].raw, "LibAddonMenu-2.0");
        assert!(parsed[0].required);
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
    fn parses_optional_dependson() {
        let parsed = parse_optional_dependency_values("LibDebugLogger LibAddonMenu-2.0>=41");

        assert_eq!(parsed.len(), 2);
        assert!(parsed.iter().all(|dependency| !dependency.required));
        assert_eq!(parsed[1].constraint.as_deref(), Some(">=41"));
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
    fn status_report_detects_installed_dependency_by_folder() {
        let report = resolve_manifest_dependencies(
            &["LibInstalled".to_owned()],
            &[],
            &[local("LibInstalled", Some("Different Title"), &[], &[])],
            Some(&[remote("7", "LibInstalled", &["LibInstalled"])]),
            &[],
        );

        assert_eq!(
            report.required_dependencies[0].status,
            DependencyAvailability::Installed
        );
        assert_eq!(
            report.required_dependencies[0].installed_folder.as_deref(),
            Some("LibInstalled")
        );
    }

    #[test]
    fn status_report_detects_installed_dependency_by_title() {
        let report = resolve_manifest_dependencies(
            &["LibInstalled".to_owned()],
            &[],
            &[local("DifferentFolder", Some("LibInstalled"), &[], &[])],
            Some(&[remote("7", "LibInstalled", &["LibInstalled"])]),
            &[],
        );

        assert_eq!(
            report.required_dependencies[0].status,
            DependencyAvailability::Installed
        );
        assert_eq!(
            report.required_dependencies[0].installed_folder.as_deref(),
            Some("DifferentFolder")
        );
        assert_eq!(
            report.required_dependencies[0].installed_title.as_deref(),
            Some("LibInstalled")
        );
    }

    #[test]
    fn status_report_detects_installed_dependency_by_metadata_uid() {
        let report = resolve_manifest_dependencies(
            &["LibMapped".to_owned()],
            &[],
            &[local("RenamedLibFolder", Some("Renamed"), &[], &[])],
            Some(&[remote("7", "LibMapped", &["LibMapped"])]),
            &[InstalledRemoteAddon {
                folder_name: "RenamedLibFolder".to_owned(),
                remote_uid: "7".to_owned(),
            }],
        );

        assert_eq!(
            report.required_dependencies[0].status,
            DependencyAvailability::Installed
        );
        assert_eq!(
            report.required_dependencies[0].installed_folder.as_deref(),
            Some("RenamedLibFolder")
        );
        assert_eq!(
            report.required_dependencies[0].remote_uid.as_deref(),
            Some("7")
        );
    }

    #[test]
    fn status_report_marks_missing_dependency_as_missing() {
        let report =
            resolve_manifest_dependencies(&["MissingLib".to_owned()], &[], &[], Some(&[]), &[]);

        assert_eq!(
            report.required_dependencies[0].status,
            DependencyAvailability::Missing
        );
        assert!(!report.required_dependencies[0].installed);
    }

    #[test]
    fn status_report_keeps_hidden_installed_library_visible() {
        let mut hidden_library = local("LibAddonMenu-2.0", Some("LibAddonMenu-2.0"), &[], &[]);
        hidden_library.is_library = Some(true);
        let report = resolve_manifest_dependencies(
            &["LibAddonMenu-2.0".to_owned()],
            &[],
            &[hidden_library],
            Some(&[remote("7", "LibAddonMenu-2.0", &["LibAddonMenu-2.0"])]),
            &[],
        );

        assert_eq!(
            report.required_dependencies[0].status,
            DependencyAvailability::Installed
        );
        assert_eq!(
            report.required_dependencies[0].installed_folder.as_deref(),
            Some("LibAddonMenu-2.0")
        );
    }

    #[test]
    fn status_report_marks_ambiguous_remote_dependency() {
        let report = resolve_manifest_dependencies(
            &["LibFoo".to_owned()],
            &[],
            &[],
            Some(&[
                remote("7", "LibFoo", &["LibFoo"]),
                remote("8", "LibFoo", &["Other"]),
            ]),
            &[],
        );

        assert_eq!(
            report.required_dependencies[0].status,
            DependencyAvailability::Ambiguous
        );
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

    #[test]
    fn transitive_required_dependency_is_resolved() {
        let mut remote_sources = BTreeMap::new();
        remote_sources.insert(
            "a".to_owned(),
            source(vec![local("LibA", Some("LibA"), &["LibB"], &[])]),
        );

        let plan = recursive_plan(
            local("MainAddon", Some("Main"), &["LibA"], &[]),
            &[
                remote("a", "LibA", &["LibA"]),
                remote("b", "LibB", &["LibB"]),
            ],
            remote_sources,
            DEFAULT_MAX_DEPENDENCY_DEPTH,
        );

        let lib_b = plan
            .required_dependencies
            .iter()
            .find(|dependency| dependency.name == "LibB")
            .expect("LibB dependency");
        assert_eq!(lib_b.status, DependencyStatus::WillInstall);
        assert_eq!(lib_b.depth, 2);
        assert_eq!(lib_b.parent.as_deref(), Some("LibA"));
    }

    #[test]
    fn install_order_is_deepest_dependency_first() {
        let mut remote_sources = BTreeMap::new();
        remote_sources.insert(
            "a".to_owned(),
            source(vec![local("LibA", Some("LibA"), &["LibB"], &[])]),
        );

        let plan = recursive_plan(
            local("MainAddon", Some("Main"), &["LibA"], &[]),
            &[
                remote("a", "LibA", &["LibA"]),
                remote("b", "LibB", &["LibB"]),
            ],
            remote_sources,
            DEFAULT_MAX_DEPENDENCY_DEPTH,
        );

        assert_eq!(plan.install_order, vec!["LibB", "LibA", "Main Addon"]);
        assert_eq!(
            plan.required_dependencies_to_install()
                .iter()
                .map(|dependency| dependency.name.as_str())
                .collect::<Vec<_>>(),
            vec!["LibB", "LibA"]
        );
    }

    #[test]
    fn optional_dependency_of_required_dependency_is_not_auto_installed() {
        let mut remote_sources = BTreeMap::new();
        remote_sources.insert(
            "a".to_owned(),
            source(vec![local("LibA", Some("LibA"), &[], &["LibOptional"])]),
        );

        let plan = recursive_plan(
            local("MainAddon", Some("Main"), &["LibA"], &[]),
            &[
                remote("a", "LibA", &["LibA"]),
                remote("opt", "LibOptional", &["LibOptional"]),
            ],
            remote_sources,
            DEFAULT_MAX_DEPENDENCY_DEPTH,
        );

        assert_eq!(
            plan.optional_dependencies[0].status,
            DependencyStatus::NotInstalled
        );
        assert_eq!(
            plan.install_order,
            vec!["LibA".to_owned(), "Main Addon".to_owned()]
        );
    }

    #[test]
    fn required_dependency_of_required_dependency_is_auto_installed() {
        let mut remote_sources = BTreeMap::new();
        remote_sources.insert(
            "a".to_owned(),
            source(vec![local("LibA", Some("LibA"), &["LibB"], &[])]),
        );

        let plan = recursive_plan(
            local("MainAddon", Some("Main"), &["LibA"], &[]),
            &[
                remote("a", "LibA", &["LibA"]),
                remote("b", "LibB", &["LibB"]),
            ],
            remote_sources,
            DEFAULT_MAX_DEPENDENCY_DEPTH,
        );

        assert_eq!(
            plan.required_dependencies_to_install()
                .iter()
                .map(|dependency| dependency.remote_uid.as_deref())
                .collect::<Vec<_>>(),
            vec![Some("b"), Some("a")]
        );
    }

    #[test]
    fn circular_dependency_is_detected_and_stops_branch() {
        let mut remote_sources = BTreeMap::new();
        remote_sources.insert(
            "a".to_owned(),
            source(vec![local("LibA", Some("LibA"), &["LibB"], &[])]),
        );
        remote_sources.insert(
            "b".to_owned(),
            source(vec![local("LibB", Some("LibB"), &["LibA"], &[])]),
        );

        let plan = recursive_plan(
            local("MainAddon", Some("Main"), &["LibA"], &[]),
            &[
                remote("a", "LibA", &["LibA"]),
                remote("b", "LibB", &["LibB"]),
            ],
            remote_sources,
            DEFAULT_MAX_DEPENDENCY_DEPTH,
        );

        assert!(plan
            .required_dependencies
            .iter()
            .any(|dependency| dependency.status == DependencyStatus::Circular));
        assert!(plan.has_unresolved_required_dependencies());
    }

    #[test]
    fn max_depth_is_enforced() {
        let mut remote_sources = BTreeMap::new();
        remote_sources.insert(
            "a".to_owned(),
            source(vec![local("LibA", Some("LibA"), &["LibB"], &[])]),
        );

        let plan = recursive_plan(
            local("MainAddon", Some("Main"), &["LibA"], &[]),
            &[
                remote("a", "LibA", &["LibA"]),
                remote("b", "LibB", &["LibB"]),
            ],
            remote_sources,
            1,
        );

        assert!(plan
            .required_dependencies
            .iter()
            .any(|dependency| dependency.name == "LibB"
                && dependency.status == DependencyStatus::MaxDepth));
        assert!(plan.has_unresolved_required_dependencies());
    }

    #[test]
    fn transitive_ambiguous_dependency_blocks_auto_install() {
        let mut remote_sources = BTreeMap::new();
        remote_sources.insert(
            "a".to_owned(),
            source(vec![local("LibA", Some("LibA"), &["LibShared"], &[])]),
        );

        let plan = recursive_plan(
            local("MainAddon", Some("Main"), &["LibA"], &[]),
            &[
                remote("a", "LibA", &["LibA"]),
                remote("one", "LibShared", &["LibShared"]),
                remote("two", "LibShared", &["Other"]),
            ],
            remote_sources,
            DEFAULT_MAX_DEPENDENCY_DEPTH,
        );

        assert!(plan
            .required_dependencies
            .iter()
            .any(|dependency| dependency.name == "LibShared"
                && dependency.status == DependencyStatus::Ambiguous));
        assert!(plan.has_unresolved_required_dependencies());
    }

    #[test]
    fn transitive_unresolved_dependency_blocks_auto_install() {
        let mut remote_sources = BTreeMap::new();
        remote_sources.insert(
            "a".to_owned(),
            source(vec![local("LibA", Some("LibA"), &["MissingLib"], &[])]),
        );

        let plan = recursive_plan(
            local("MainAddon", Some("Main"), &["LibA"], &[]),
            &[remote("a", "LibA", &["LibA"])],
            remote_sources,
            DEFAULT_MAX_DEPENDENCY_DEPTH,
        );

        assert!(plan
            .required_dependencies
            .iter()
            .any(|dependency| dependency.name == "MissingLib"
                && dependency.status == DependencyStatus::Unresolved));
        assert!(plan.has_unresolved_required_dependencies());
    }

    #[test]
    fn already_installed_transitive_dependency_is_not_reinstalled() {
        let plan = build_dependency_plan_with_remote_sources(
            main_ref(),
            &source(vec![local("MainAddon", Some("Main"), &["LibA"], &[])]),
            &[local("LibA", Some("LibA"), &["LibB"], &[])],
            &[
                remote("a", "LibA", &["LibA"]),
                remote("b", "LibB", &["LibB"]),
            ],
            &[],
            &BTreeMap::new(),
            DEFAULT_MAX_DEPENDENCY_DEPTH,
        );

        assert_eq!(
            plan.required_dependencies
                .iter()
                .find(|dependency| dependency.name == "LibA")
                .map(|dependency| &dependency.status),
            Some(&DependencyStatus::AlreadyInstalled)
        );
        assert_eq!(
            plan.required_dependencies_to_install()
                .iter()
                .map(|dependency| dependency.name.as_str())
                .collect::<Vec<_>>(),
            vec!["LibB"]
        );
    }

    #[test]
    fn dependency_graph_does_not_duplicate_shared_dependencies() {
        let mut remote_sources = BTreeMap::new();
        remote_sources.insert(
            "a".to_owned(),
            source(vec![local("LibA", Some("LibA"), &["LibC"], &[])]),
        );
        remote_sources.insert(
            "b".to_owned(),
            source(vec![local("LibB", Some("LibB"), &["LibC"], &[])]),
        );

        let plan = recursive_plan(
            local("MainAddon", Some("Main"), &["LibA LibB"], &[]),
            &[
                remote("a", "LibA", &["LibA"]),
                remote("b", "LibB", &["LibB"]),
                remote("c", "LibC", &["LibC"]),
            ],
            remote_sources,
            DEFAULT_MAX_DEPENDENCY_DEPTH,
        );

        assert_eq!(
            plan.required_dependencies
                .iter()
                .filter(|dependency| dependency.name == "LibC")
                .count(),
            1
        );
        assert_eq!(
            plan.graph
                .edges
                .iter()
                .filter(|edge| edge.child_name == "LibC")
                .count(),
            2
        );
    }
}
