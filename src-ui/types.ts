export interface LocalAddon {
  folder_name: string;
  title: string | null;
  display_version: string | null;
  api_versions: string[];
  depends_on: string[];
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
}

export interface SearchResponse {
  query: string;
  limit: number;
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
}

export interface RemoteCandidate {
  uid: string | null;
  name: string | null;
  version: string | null;
  updated_display: string | null;
}

export interface MatchResult {
  local: LocalAddon & { folder_name: string };
  status: string;
  remote: RemoteCandidate | null;
}

export interface CheckAddonsResponse {
  addons_dir: string;
  remote_addons_loaded: number;
  matches: MatchResult[];
}

export interface PlannedAction {
  local_folder: string;
  local_version: string | null;
  remote_name: string | null;
  remote_uid: string | null;
  remote_version: string | null;
  action: string;
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
  installed_new: number;
  replaced: number;
  skipped: number;
  backup_dir: string | null;
  items: InstallResultItem[];
}

export interface UpdateAllResult {
  target: PlannedAction;
  remote_details: AddonDetails;
  plan: {
    addons_dir: string;
    temp_dir: string;
    items: InstallPlanItem[];
  };
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
}

export interface AppSettingsInput {
  addons_dir_override: string | null;
  backup_dir_override: string | null;
  download_dir: string | null;
  keep_downloads_default: boolean;
  include_unknown_updates_default: boolean;
}
