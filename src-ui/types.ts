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
