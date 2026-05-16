use std::collections::{BTreeMap, BTreeSet};

use crate::api::models::AddonSummary;
use crate::install::dependencies::{
    collect_dependencies, find_installed_remote_addon, find_matching_local_addon_details,
    local_addon_display_version, normalize_key, remote_for_installed_addon, resolve_dependency,
    DependencyResolution, InstalledRemoteAddon, ManifestDependency, RemoteAddonRef,
};
use crate::install::zip_safety::ExtractedZip;
use crate::local::LocalAddon;

pub const DEFAULT_MAX_DEPENDENCY_DEPTH: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyGraphOptions {
    pub max_depth: usize,
}

impl Default for DependencyGraphOptions {
    fn default() -> Self {
        Self {
            max_depth: DEFAULT_MAX_DEPENDENCY_DEPTH,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DependencyManifestSource {
    pub addons: Vec<LocalAddon>,
    pub required_values: Vec<String>,
    pub optional_values: Vec<String>,
}

impl DependencyManifestSource {
    pub fn from_extracted(extracted: &ExtractedZip) -> Self {
        Self::from_addons(extracted.detected_addons.clone())
    }

    pub fn from_addons(addons: Vec<LocalAddon>) -> Self {
        let required_values = addons
            .iter()
            .filter(|addon| addon.valid_manifest)
            .flat_map(|addon| addon.depends_on.iter().cloned())
            .collect();
        let optional_values = addons
            .iter()
            .filter(|addon| addon.valid_manifest)
            .flat_map(|addon| addon.optional_depends_on.iter().cloned())
            .collect();

        Self {
            addons,
            required_values,
            optional_values,
        }
    }

    pub fn from_dependency_values(required_values: &[String], optional_values: &[String]) -> Self {
        Self {
            addons: Vec::new(),
            required_values: required_values.to_vec(),
            optional_values: optional_values.to_vec(),
        }
    }

    fn dependencies(&self) -> (Vec<ManifestDependency>, Vec<ManifestDependency>) {
        let required = collect_dependencies(self.required_values.iter(), true);
        let required_names = required
            .iter()
            .map(|dependency| normalize_key(&dependency.name))
            .collect::<BTreeSet<_>>();
        let optional = collect_dependencies(self.optional_values.iter(), false)
            .into_iter()
            .filter(|dependency| !required_names.contains(&normalize_key(&dependency.name)))
            .collect::<Vec<_>>();

        (required, optional)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyEdgeKind {
    Required,
    Optional,
}

impl DependencyEdgeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Required => "required",
            Self::Optional => "optional",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyResolutionStatus {
    Installed,
    Missing,
    WillInstall,
    Unresolved,
    Ambiguous,
    Circular,
    MaxDepth,
}

impl DependencyResolutionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Installed => "installed",
            Self::Missing => "missing",
            Self::WillInstall => "will-install",
            Self::Unresolved => "unresolved",
            Self::Ambiguous => "ambiguous",
            Self::Circular => "circular",
            Self::MaxDepth => "max-depth",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyNode {
    pub id: usize,
    pub key: String,
    pub name: String,
    pub raw: String,
    pub constraint: Option<String>,
    pub required: bool,
    pub relation: DependencyEdgeKind,
    pub depth: usize,
    pub parent: Option<String>,
    pub status: DependencyResolutionStatus,
    pub remote_uid: Option<String>,
    pub remote_name: Option<String>,
    pub remote_version: Option<String>,
    pub installed_folder: Option<String>,
    pub installed_title: Option<String>,
    pub installed_version: Option<String>,
    pub bundled_folder: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyEdge {
    pub parent_node: Option<usize>,
    pub parent_name: String,
    pub child_node: usize,
    pub child_name: String,
    pub kind: DependencyEdgeKind,
    pub depth: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyGraph {
    pub nodes: Vec<DependencyNode>,
    pub edges: Vec<DependencyEdge>,
    pub max_depth: usize,
}

impl DependencyGraph {
    pub fn required_install_order(&self) -> Vec<&DependencyNode> {
        let mut children_by_parent: BTreeMap<Option<usize>, Vec<usize>> = BTreeMap::new();
        for edge in &self.edges {
            if edge.kind == DependencyEdgeKind::Required
                && self
                    .nodes
                    .get(edge.child_node)
                    .is_some_and(|node| node.required)
            {
                children_by_parent
                    .entry(edge.parent_node)
                    .or_default()
                    .push(edge.child_node);
            }
        }

        let mut visited = BTreeSet::new();
        let mut emitted = BTreeSet::new();
        let mut order = Vec::new();
        self.visit_install_order(
            None,
            &children_by_parent,
            &mut visited,
            &mut emitted,
            &mut order,
        );
        order
    }

    fn visit_install_order<'a>(
        &'a self,
        parent: Option<usize>,
        children_by_parent: &BTreeMap<Option<usize>, Vec<usize>>,
        visited: &mut BTreeSet<usize>,
        emitted: &mut BTreeSet<usize>,
        order: &mut Vec<&'a DependencyNode>,
    ) {
        let Some(children) = children_by_parent.get(&parent) else {
            return;
        };

        for child_id in children {
            if !visited.insert(*child_id) {
                continue;
            }

            self.visit_install_order(Some(*child_id), children_by_parent, visited, emitted, order);

            let node = &self.nodes[*child_id];
            if node.status == DependencyResolutionStatus::WillInstall
                && node.remote_uid.is_some()
                && emitted.insert(*child_id)
            {
                order.push(node);
            }
        }
    }

    pub fn required_remote_uids_missing_sources(
        &self,
        remote_sources: &BTreeMap<String, DependencyManifestSource>,
    ) -> Vec<String> {
        let mut seen = BTreeSet::new();
        let mut uids = Vec::new();

        for node in self.required_install_order() {
            let Some(uid) = node.remote_uid.as_deref() else {
                continue;
            };
            if remote_sources.contains_key(uid) {
                continue;
            }
            if seen.insert(uid.to_owned()) {
                uids.push(uid.to_owned());
            }
        }

        uids
    }
}

#[derive(Debug, Clone)]
struct ResolvedDependency {
    status: DependencyResolutionStatus,
    remote_uid: Option<String>,
    remote_name: Option<String>,
    remote_version: Option<String>,
    installed_folder: Option<String>,
    installed_title: Option<String>,
    installed_version: Option<String>,
    bundled_folder: Option<String>,
}

struct GraphBuilder<'a> {
    installed_addons: &'a [LocalAddon],
    remote_addons: &'a [AddonSummary],
    installed_remotes: &'a [InstalledRemoteAddon],
    remote_sources: &'a BTreeMap<String, DependencyManifestSource>,
    options: DependencyGraphOptions,
    nodes: Vec<DependencyNode>,
    edges: Vec<DependencyEdge>,
    node_ids: BTreeMap<String, usize>,
    expanded_required: BTreeSet<String>,
    expanded_optional: BTreeSet<String>,
    edge_keys: BTreeSet<(Option<usize>, usize, &'static str)>,
}

pub fn build_dependency_graph(
    main_addon: &RemoteAddonRef,
    main_source: &DependencyManifestSource,
    installed_addons: &[LocalAddon],
    remote_addons: &[AddonSummary],
    installed_remotes: &[InstalledRemoteAddon],
    remote_sources: &BTreeMap<String, DependencyManifestSource>,
    options: DependencyGraphOptions,
) -> DependencyGraph {
    let mut builder = GraphBuilder {
        installed_addons,
        remote_addons,
        installed_remotes,
        remote_sources,
        options,
        nodes: Vec::new(),
        edges: Vec::new(),
        node_ids: BTreeMap::new(),
        expanded_required: BTreeSet::new(),
        expanded_optional: BTreeSet::new(),
        edge_keys: BTreeSet::new(),
    };
    let mut path = vec![remote_key(&main_addon.uid)];
    builder.visit_source(None, root_name(main_addon), main_source, true, 0, &mut path);

    DependencyGraph {
        nodes: builder.nodes,
        edges: builder.edges,
        max_depth: builder.options.max_depth,
    }
}

impl<'a> GraphBuilder<'a> {
    fn visit_source(
        &mut self,
        parent_node: Option<usize>,
        parent_name: String,
        source: &DependencyManifestSource,
        parent_required: bool,
        parent_depth: usize,
        path: &mut Vec<String>,
    ) {
        let (required, optional) = source.dependencies();
        for dependency in required {
            self.visit_dependency(
                parent_node,
                parent_name.clone(),
                source,
                dependency,
                DependencyEdgeKind::Required,
                parent_required,
                parent_depth + 1,
                path,
            );
        }
        for dependency in optional {
            self.visit_dependency(
                parent_node,
                parent_name.clone(),
                source,
                dependency,
                DependencyEdgeKind::Optional,
                false,
                parent_depth + 1,
                path,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn visit_dependency(
        &mut self,
        parent_node: Option<usize>,
        parent_name: String,
        current_source: &DependencyManifestSource,
        dependency: ManifestDependency,
        relation: DependencyEdgeKind,
        effective_required: bool,
        depth: usize,
        path: &mut Vec<String>,
    ) {
        let mut resolved = self.resolve_dependency(&dependency, current_source, effective_required);
        let key = node_key(&dependency, &resolved);
        let circular = path.iter().any(|path_key| path_key == &key);
        if circular {
            resolved.status = DependencyResolutionStatus::Circular;
        } else if depth > self.options.max_depth {
            resolved.status = DependencyResolutionStatus::MaxDepth;
        }

        let node_id = self.upsert_node(
            key.clone(),
            dependency,
            resolved.clone(),
            effective_required,
            relation,
            depth,
            Some(parent_name.clone()),
        );
        self.add_edge(parent_node, parent_name, node_id, relation, depth);

        if circular || depth > self.options.max_depth {
            return;
        }

        let should_recurse = effective_required
            || resolved.status == DependencyResolutionStatus::Installed
            || resolved.bundled_folder.is_some();
        if !should_recurse {
            return;
        }

        let expanded = if effective_required {
            self.expanded_required.insert(key.clone())
        } else {
            self.expanded_optional.insert(key.clone())
        };
        if !expanded {
            return;
        }

        let Some(child_source) = self.source_for_resolved(&resolved, current_source) else {
            return;
        };

        path.push(key);
        let child_parent_name = self.node_display_name(node_id);
        self.visit_source(
            Some(node_id),
            child_parent_name,
            &child_source,
            effective_required,
            depth,
            path,
        );
        path.pop();
    }

    #[allow(clippy::too_many_arguments)]
    fn upsert_node(
        &mut self,
        key: String,
        dependency: ManifestDependency,
        resolved: ResolvedDependency,
        effective_required: bool,
        relation: DependencyEdgeKind,
        depth: usize,
        parent: Option<String>,
    ) -> usize {
        if let Some(id) = self.node_ids.get(&key).copied() {
            let node = &mut self.nodes[id];
            let was_required = node.required;
            node.required |= effective_required;
            if depth < node.depth {
                node.depth = depth;
            }
            if !was_required && effective_required {
                node.relation = relation;
                node.parent = parent;
            }
            node.status = merge_status(node.status, resolved.status, node.required);
            if node.remote_uid.is_none() {
                node.remote_uid = resolved.remote_uid;
            }
            if node.remote_name.is_none() {
                node.remote_name = resolved.remote_name;
            }
            if node.remote_version.is_none() {
                node.remote_version = resolved.remote_version;
            }
            if node.installed_folder.is_none() {
                node.installed_folder = resolved.installed_folder;
            }
            if node.installed_title.is_none() {
                node.installed_title = resolved.installed_title;
            }
            if node.installed_version.is_none() {
                node.installed_version = resolved.installed_version;
            }
            if node.bundled_folder.is_none() {
                node.bundled_folder = resolved.bundled_folder;
            }
            return id;
        }

        let id = self.nodes.len();
        self.node_ids.insert(key.clone(), id);
        self.nodes.push(DependencyNode {
            id,
            key,
            name: dependency.name,
            raw: dependency.raw,
            constraint: dependency.constraint,
            required: effective_required,
            relation,
            depth,
            parent,
            status: resolved.status,
            remote_uid: resolved.remote_uid,
            remote_name: resolved.remote_name,
            remote_version: resolved.remote_version,
            installed_folder: resolved.installed_folder,
            installed_title: resolved.installed_title,
            installed_version: resolved.installed_version,
            bundled_folder: resolved.bundled_folder,
        });
        id
    }

    fn add_edge(
        &mut self,
        parent_node: Option<usize>,
        parent_name: String,
        child_node: usize,
        kind: DependencyEdgeKind,
        depth: usize,
    ) {
        if !self
            .edge_keys
            .insert((parent_node, child_node, kind.as_str()))
        {
            return;
        }

        self.edges.push(DependencyEdge {
            parent_node,
            parent_name,
            child_node,
            child_name: self.node_display_name(child_node),
            kind,
            depth,
        });
    }

    fn resolve_dependency(
        &self,
        dependency: &ManifestDependency,
        current_source: &DependencyManifestSource,
        effective_required: bool,
    ) -> ResolvedDependency {
        if let Some(local) =
            find_matching_local_addon_details(&dependency.name, self.installed_addons)
        {
            let remote =
                remote_for_installed_addon(local, Some(self.remote_addons), self.installed_remotes);
            return ResolvedDependency {
                status: DependencyResolutionStatus::Installed,
                remote_uid: remote.as_ref().map(|remote| remote.uid.clone()),
                remote_name: remote.as_ref().and_then(|remote| remote.name.clone()),
                remote_version: remote.and_then(|remote| remote.version),
                installed_folder: Some(local.folder_name.clone()),
                installed_title: local.title.clone(),
                installed_version: local_addon_display_version(local),
                bundled_folder: None,
            };
        }

        if let Some(local) =
            find_matching_local_addon_details(&dependency.name, &current_source.addons)
        {
            return ResolvedDependency {
                status: DependencyResolutionStatus::WillInstall,
                remote_uid: None,
                remote_name: None,
                remote_version: None,
                installed_folder: None,
                installed_title: None,
                installed_version: None,
                bundled_folder: Some(local.folder_name.clone()),
            };
        }

        match resolve_dependency(&dependency.name, self.remote_addons) {
            DependencyResolution::Resolved(remote) => {
                if let Some(local) = find_installed_remote_addon(
                    &remote.uid,
                    self.installed_remotes,
                    self.installed_addons,
                ) {
                    ResolvedDependency {
                        status: DependencyResolutionStatus::Installed,
                        remote_uid: Some(remote.uid),
                        remote_name: remote.name,
                        remote_version: remote.version,
                        installed_folder: Some(local.folder_name.clone()),
                        installed_title: local.title.clone(),
                        installed_version: local_addon_display_version(local),
                        bundled_folder: None,
                    }
                } else {
                    ResolvedDependency {
                        status: if effective_required {
                            DependencyResolutionStatus::WillInstall
                        } else {
                            DependencyResolutionStatus::Missing
                        },
                        remote_uid: Some(remote.uid),
                        remote_name: remote.name,
                        remote_version: remote.version,
                        installed_folder: None,
                        installed_title: None,
                        installed_version: None,
                        bundled_folder: None,
                    }
                }
            }
            DependencyResolution::Ambiguous => ResolvedDependency {
                status: DependencyResolutionStatus::Ambiguous,
                remote_uid: None,
                remote_name: None,
                remote_version: None,
                installed_folder: None,
                installed_title: None,
                installed_version: None,
                bundled_folder: None,
            },
            DependencyResolution::Unresolved => ResolvedDependency {
                status: DependencyResolutionStatus::Unresolved,
                remote_uid: None,
                remote_name: None,
                remote_version: None,
                installed_folder: None,
                installed_title: None,
                installed_version: None,
                bundled_folder: None,
            },
        }
    }

    fn source_for_resolved(
        &self,
        resolved: &ResolvedDependency,
        current_source: &DependencyManifestSource,
    ) -> Option<DependencyManifestSource> {
        if let Some(folder) = resolved.installed_folder.as_deref() {
            return self
                .installed_addons
                .iter()
                .find(|addon| addon.folder_name.eq_ignore_ascii_case(folder))
                .cloned()
                .map(|addon| DependencyManifestSource::from_addons(vec![addon]));
        }

        if let Some(folder) = resolved.bundled_folder.as_deref() {
            return current_source
                .addons
                .iter()
                .find(|addon| addon.folder_name.eq_ignore_ascii_case(folder))
                .cloned()
                .map(|addon| DependencyManifestSource::from_addons(vec![addon]));
        }

        resolved
            .remote_uid
            .as_deref()
            .and_then(|uid| self.remote_sources.get(uid))
            .cloned()
    }

    fn node_display_name(&self, node_id: usize) -> String {
        let node = &self.nodes[node_id];
        node.remote_name
            .clone()
            .unwrap_or_else(|| node.name.clone())
    }
}

fn merge_status(
    current: DependencyResolutionStatus,
    next: DependencyResolutionStatus,
    required: bool,
) -> DependencyResolutionStatus {
    if matches!(
        next,
        DependencyResolutionStatus::Circular | DependencyResolutionStatus::MaxDepth
    ) {
        return next;
    }
    if matches!(
        current,
        DependencyResolutionStatus::Circular | DependencyResolutionStatus::MaxDepth
    ) {
        return current;
    }
    if required && current == DependencyResolutionStatus::Missing {
        return next;
    }
    if required && next == DependencyResolutionStatus::WillInstall {
        return next;
    }
    if current == DependencyResolutionStatus::Installed
        || next == DependencyResolutionStatus::Installed
    {
        return DependencyResolutionStatus::Installed;
    }
    if matches!(
        current,
        DependencyResolutionStatus::Ambiguous | DependencyResolutionStatus::Unresolved
    ) {
        return current;
    }
    next
}

fn node_key(dependency: &ManifestDependency, resolved: &ResolvedDependency) -> String {
    resolved
        .remote_uid
        .as_deref()
        .map(remote_key)
        .unwrap_or_else(|| format!("name:{}", normalize_key(&dependency.name)))
}

fn remote_key(uid: &str) -> String {
    format!("uid:{}", normalize_key(uid))
}

fn root_name(main_addon: &RemoteAddonRef) -> String {
    main_addon
        .name
        .clone()
        .unwrap_or_else(|| main_addon.uid.clone())
}
