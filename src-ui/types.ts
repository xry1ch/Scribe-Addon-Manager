export interface LocalAddon {
  folder_name: string;
  folder_path: string;
  title: string | null;
  author: string | null;
  display_version: string | null;
  api_versions: string[];
  depends_on: string[];
  optional_depends_on: string[];
  saved_variables: string[];
  saved_variables_per_character: string[];
  description: string | null;
  is_library: boolean | null;
  valid_manifest: boolean;
}

export interface InstalledAddonsResponse {
  addons_dir: string;
  addons: LocalAddon[];
}

export interface AddonSummary {
  uid: string | null;
  name: string | null;
  author_name: string | null;
  version: string | null;
  updated_display: string | null;
  file_info_url: string | null;
  summary: string | null;
  directories: string[];
  category_id: string | null;
  category_name: string | null;
  downloads: number | null;
  monthly_downloads: number | null;
  is_library: boolean;
  image_urls: string[];
  thumbnail_urls: string[];
  installed: boolean;
  installed_local: LocalAddon | null;
  installed_match: MatchResult | null;
}

export interface SearchResponse {
  query: string;
  limit: number;
  results: AddonSummary[];
}

export interface RemoteCategory {
  id: string;
  name: string;
  parent_id: string | null;
}

export interface BrowseRemoteAddonsResponse {
  mode: string;
  query: string;
  category_id: string | null;
  limit: number;
  categories: RemoteCategory[];
  category_warning: string | null;
  local_warning: string | null;
  cache_warning: string | null;
  results: AddonSummary[];
}

export interface AddonDetails {
  uid: string | null;
  name: string | null;
  author_name: string | null;
  version: string | null;
  updated_display: string | null;
  file_name: string | null;
  md5: string | null;
  download_url: string | null;
  file_info_url: string | null;
  description: string | null;
  changelog: string | null;
  category_id: string | null;
  category_name: string | null;
  downloads: number | null;
  monthly_downloads: number | null;
  is_library: boolean;
  image_urls: string[];
  thumbnail_urls: string[];
}

export interface RemoteAddonDetailsWithLocalStateResponse {
  details: AddonDetails;
  installed: boolean;
  local: LocalAddon | null;
  match_result: MatchResult | null;
  local_warning: string | null;
  cache_warning: string | null;
}

export interface RemoteCandidate {
  uid: string | null;
  name: string | null;
  author_name: string | null;
  version: string | null;
  updated_display: string | null;
  file_info_url: string | null;
  summary: string | null;
  directories: string[];
  category_id: string | null;
  category_name: string | null;
  downloads: number | null;
  monthly_downloads: number | null;
  is_library: boolean;
  image_urls: string[];
  thumbnail_urls: string[];
}

export interface MatchResult {
  local: LocalAddon & { folder_name: string };
  status: string;
  update_confidence: string;
  update_reason: string;
  managed: boolean;
  remote: RemoteCandidate | null;
}

export interface CheckAddonsResponse {
  addons_dir: string;
  remote_addons_loaded: number;
  matches: MatchResult[];
  cache_warning: string | null;
}

export interface PlannedAction {
  local_folder: string;
  local_version: string | null;
  remote_name: string | null;
  remote_uid: string | null;
  remote_version: string | null;
  remote_date?: number | null;
  action: string;
  update_confidence?: string | null;
  update_reason?: string | null;
}

export interface PlanUpdatesResponse {
  addons_dir: string;
  remote_addons_loaded: number;
  include_unknown: boolean;
  matches: MatchResult[];
  actions: PlannedAction[];
  summary: {
    would_update: number;
    current_skipped: number;
    local_newer: number;
    unknown: number;
    no_match: number;
    ambiguous: number;
    libraries: number;
  };
  cache_warning: string | null;
}

export interface UpdateAllAction extends PlannedAction {
  update_all_action: string;
}

export interface UpdateAllSummary {
  planned_updates: number;
  skipped_current: number;
  skipped_local_newer: number;
  skipped_unknown: number;
  skipped_no_match: number;
  skipped_ambiguous: number;
  skipped_libraries: number;
}

export interface PlanUpdateAllResponse {
  dry_run: boolean;
  applied: boolean;
  addons_dir: string;
  remote_addons_loaded: number;
  include_unknown: boolean;
  limit: number | null;
  actions: UpdateAllAction[];
  targets: PlannedAction[];
  summary: UpdateAllSummary;
}

export interface InstallPlanItem {
  source_folder: string | null;
  title: string | null;
  version: string | null;
  target_folder: string | null;
  action: string;
}

export interface DependencyPlanEntry {
  name: string;
  constraint: string | null;
  raw: string;
  required: boolean;
  relation: "required" | "optional" | string;
  depth: number;
  parent: string | null;
  status: "already-installed" | "will-install" | "not-installed" | "unresolved" | "ambiguous" | string;
  remote_uid: string | null;
  remote_name: string | null;
  remote_version: string | null;
  installed_folder: string | null;
  installed_title: string | null;
  installed_version: string | null;
  bundled_folder: string | null;
}

export interface DependencyInstallItem {
  role: "main-addon" | "required-dependency" | string;
  name: string;
  remote_uid: string | null;
  remote_name: string | null;
  action: string;
}

export interface DependencyPlan {
  main_addon: {
    uid: string;
    name: string | null;
  };
  required_dependencies: DependencyPlanEntry[];
  optional_dependencies: DependencyPlanEntry[];
  install_items: DependencyInstallItem[];
  install_order: string[];
}

export interface AddonDependencyStatus {
  name: string;
  raw: string;
  constraint: string | null;
  required: boolean;
  relation: "required" | "optional" | string;
  depth: number;
  parent: string | null;
  installed: boolean;
  installed_folder: string | null;
  installed_title: string | null;
  installed_version: string | null;
  remote_uid: string | null;
  remote_name: string | null;
  remote_version: string | null;
  status: "installed" | "missing" | "unknown" | "ambiguous" | string;
}

export interface InstalledAddonDependenciesResponse {
  required_dependencies: AddonDependencyStatus[];
  optional_dependencies: AddonDependencyStatus[];
  warning: string | null;
}

export interface PlanRemoteInstallResponse {
  dry_run: boolean;
  applied: boolean;
  remote: AddonDetails;
  addons_dir: string;
  plan: {
    addons_dir: string;
    temp_dir: string;
    items: InstallPlanItem[];
  };
  dependency_plan: DependencyPlan;
}

export interface InstallResultItem {
  source_folder: string | null;
  target_folder: string | null;
  backup_folder: string | null;
  action: string;
  message: string | null;
}

export interface InstallRemoteAddonResponse {
  applied: boolean;
  installed_new: number;
  replaced: number;
  skipped: number;
  backup_dir: string | null;
  remote: AddonDetails;
  addons_dir: string;
  plan: {
    addons_dir: string;
    temp_dir: string;
    items: InstallPlanItem[];
  };
  dependency_plan: DependencyPlan;
  items: InstallResultItem[];
}

export interface SingleUpdatePlanResponse {
  dry_run: boolean;
  applied: boolean;
  target: string;
  local: LocalAddon;
  remote: RemoteCandidate | null;
  decision: string;
  should_install: boolean;
  reason: string | null;
  remote_details: AddonDetails | null;
  addons_dir: string;
  plan: {
    addons_dir: string;
    temp_dir: string;
    items: InstallPlanItem[];
  } | null;
  dependency_plan: DependencyPlan | null;
}

export interface SingleUpdateApplyResponse {
  applied: boolean;
  target: string;
  local: LocalAddon;
  remote: RemoteCandidate | null;
  decision: string;
  reason: string | null;
  remote_details: AddonDetails | null;
  addons_dir: string;
  plan: {
    addons_dir: string;
    temp_dir: string;
    items: InstallPlanItem[];
  } | null;
  dependency_plan: DependencyPlan | null;
  installed_new: number;
  replaced: number;
  skipped: number;
  backup_dir: string | null;
  items: InstallResultItem[];
}

export interface RemoveInstalledAddonResponse {
  removed_addon: boolean;
  removed_saved_variables: boolean;
  saved_variables_deleted_count: number;
  saved_variables_deleted_files: string[];
  saved_variables_missing_files: string[];
  addon_folder: string;
  original_path: string;
  message: string;
}

export interface ClearSavedVariablesResponse {
  addon_folder: string;
  saved_variables_dir: string;
  deleted_count: number;
  deleted_files: string[];
  missing_files: string[];
  status: "deleted" | "missing_saved_variables_folder" | "no_files_found";
  message: string;
}

export interface ManualBackupResponse {
  backup_path: string;
  backup_name: string;
  copied_addons: boolean;
  copied_saved_variables: boolean;
  saved_variables_missing: boolean;
  total_files: number;
  total_bytes: number;
}

export interface UpdateAllResult {
  target: PlannedAction;
  remote_details: AddonDetails;
  plan: {
    addons_dir: string;
    temp_dir: string;
    items: InstallPlanItem[];
  };
  dependency_plan: DependencyPlan;
  installed_new: number;
  replaced: number;
  skipped: number;
  backup_dir: string | null;
  items: InstallResultItem[];
}

export interface ApplyUpdateAllResponse {
  dry_run: boolean;
  applied: boolean;
  addons_dir: string;
  remote_addons_loaded: number;
  include_unknown: boolean;
  limit: number | null;
  actions: UpdateAllAction[];
  targets: PlannedAction[];
  summary: UpdateAllSummary;
  results: UpdateAllResult[];
}

export interface AppSettings {
  addons_dir_override: string | null;
  backup_dir_override: string | null;
  download_dir: string | null;
  keep_downloads_default: boolean;
  include_unknown_updates_default: boolean;
  hide_libraries_in_search: boolean;
  hide_libraries_in_installed: boolean;
}

export interface AppStartupInfo {
  settings: AppSettings;
  settings_exists: boolean;
  detected_addons_dir: string | null;
}

export interface ImportExistingAddonsResponse {
  addons_dir: string;
  detected_addons: number;
  imported: number;
  skipped_invalid_manifest: number;
  skipped_libraries: number;
  skipped_no_match: number;
  skipped_ambiguous: number;
  skipped_missing_remote_uid: number;
  skipped_missing_remote_version: number;
}

export interface AppSettingsInput {
  addons_dir_override: string | null;
  backup_dir_override: string | null;
  download_dir: string | null;
  keep_downloads_default: boolean;
  include_unknown_updates_default: boolean;
  hide_libraries_in_search: boolean;
  hide_libraries_in_installed: boolean;
}

export interface HttpCacheStatsResponse {
  cache_dir: string;
  entry_count: number;
  byte_size: number;
  size_display: string;
}

export interface CachedImageResponse {
  url: string;
  data_url: string;
  content_type: string;
  from_cache: boolean;
  stale: boolean;
  cache_warning: string | null;
}
