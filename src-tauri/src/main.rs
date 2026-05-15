#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use eso_addon_manager::api::models::{AddonDetails, AddonSummary};
use eso_addon_manager::api::ApiClient;
use eso_addon_manager::install::apply::{
    self, InstallActionPerformed, InstallResult, InstalledItem,
};
use eso_addon_manager::install::plan::{self, InstallPlan, InstallPlanAction, InstallPlanItem};
use eso_addon_manager::install::remote;
use eso_addon_manager::install::update;
use eso_addon_manager::install::update_all;
use eso_addon_manager::install::zip_safety;
use eso_addon_manager::local::match_remote::{self, MatchResult, RemoteCandidate};
use eso_addon_manager::local::update_plan::{self, PlannedAddonAction};
use eso_addon_manager::local::version::{compare_versions, VersionComparison};
use eso_addon_manager::local::{self, AddonPathCandidate, LocalAddon};
use serde::Deserialize;
use serde::Serialize;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::Manager;

#[derive(Debug, Serialize)]
struct InstalledAddonsResponse {
    addons_dir: String,
    candidates: Vec<AddonPathCandidateDto>,
    addons: Vec<LocalAddonDto>,
}

#[derive(Debug, Serialize)]
struct AddonPathCandidateDto {
    path: String,
    exists: bool,
    contains_addons: bool,
}

#[derive(Debug, Serialize)]
struct LocalAddonDto {
    folder_name: String,
    folder_path: String,
    manifest_path: Option<String>,
    title: Option<String>,
    addon_version: Option<String>,
    version: Option<String>,
    display_version: Option<String>,
    api_versions: Vec<String>,
    depends_on: Vec<String>,
    optional_depends_on: Vec<String>,
    is_library: Option<bool>,
    author: Option<String>,
    description: Option<String>,
    valid_manifest: bool,
}

#[derive(Debug, Serialize)]
struct SearchResponse {
    query: String,
    limit: usize,
    results: Vec<AddonSummaryDto>,
}

#[derive(Debug, Serialize)]
struct AddonSummaryDto {
    uid: Option<String>,
    name: Option<String>,
    author_name: Option<String>,
    version: Option<String>,
    updated: Option<i64>,
    updated_display: Option<String>,
    file_info_url: Option<String>,
    summary: Option<String>,
    directories: Vec<String>,
    category_id: Option<String>,
    category_name: Option<String>,
    downloads: Option<i64>,
    monthly_downloads: Option<i64>,
}

#[derive(Debug, Serialize)]
struct AddonDetailsDto {
    uid: Option<String>,
    name: Option<String>,
    author_name: Option<String>,
    version: Option<String>,
    updated: Option<i64>,
    updated_display: Option<String>,
    file_name: Option<String>,
    md5: Option<String>,
    download_url: Option<String>,
    file_info_url: Option<String>,
    description: Option<String>,
    changelog: Option<String>,
    category_id: Option<String>,
    category_name: Option<String>,
    downloads: Option<i64>,
    monthly_downloads: Option<i64>,
}

#[derive(Debug, Serialize)]
struct CheckAddonsResponse {
    addons_dir: String,
    remote_addons_loaded: usize,
    matches: Vec<MatchResultDto>,
}

#[derive(Debug, Serialize)]
struct MatchResultDto {
    local: LocalAddonDto,
    status: String,
    update_confidence: String,
    update_reason: String,
    managed: bool,
    remote: Option<RemoteCandidateDto>,
    candidates: Vec<RemoteCandidateDto>,
    debug_candidates: Vec<RemoteCandidateDto>,
}

#[derive(Debug, Serialize)]
struct RemoteCandidateDto {
    uid: Option<String>,
    name: Option<String>,
    author_name: Option<String>,
    version: Option<String>,
    updated: Option<i64>,
    updated_display: Option<String>,
    file_info_url: Option<String>,
    summary: Option<String>,
    directories: Vec<String>,
    category_id: Option<String>,
    category_name: Option<String>,
    downloads: Option<i64>,
    monthly_downloads: Option<i64>,
    tier: u8,
    score: usize,
    reason: String,
}

#[derive(Debug, Serialize)]
struct PlanUpdatesResponse {
    addons_dir: String,
    remote_addons_loaded: usize,
    include_unknown: bool,
    matches: Vec<MatchResultDto>,
    actions: Vec<PlannedActionDto>,
    summary: UpdatePlanSummaryDto,
}

#[derive(Debug, Serialize)]
struct PlannedActionDto {
    local_folder: String,
    remote_name: Option<String>,
    remote_uid: Option<String>,
    local_version: Option<String>,
    remote_version: Option<String>,
    remote_date: Option<i64>,
    action: String,
    update_confidence: Option<String>,
    update_reason: Option<String>,
}

#[derive(Debug, Serialize)]
struct UpdatePlanSummaryDto {
    would_update: usize,
    current_skipped: usize,
    local_newer: usize,
    unknown: usize,
    no_match: usize,
    ambiguous: usize,
    libraries: usize,
}

#[derive(Debug, Serialize)]
struct PlanRemoteInstallResponse {
    dry_run: bool,
    applied: bool,
    remote: AddonDetailsDto,
    addons_dir: String,
    plan: InstallPlanDto,
}

#[derive(Debug, Serialize)]
struct InstallRemoteAddonResponse {
    applied: bool,
    installed_new: usize,
    replaced: usize,
    skipped: usize,
    backup_dir: Option<String>,
    remote: AddonDetailsDto,
    addons_dir: String,
    plan: InstallPlanDto,
    items: Vec<InstalledItemDto>,
}

#[derive(Debug, Serialize)]
struct SingleUpdatePlanResponse {
    dry_run: bool,
    applied: bool,
    target: String,
    local: LocalAddonDto,
    remote: Option<RemoteCandidateDto>,
    decision: String,
    should_install: bool,
    reason: Option<String>,
    remote_details: Option<AddonDetailsDto>,
    addons_dir: String,
    plan: Option<InstallPlanDto>,
}

#[derive(Debug, Serialize)]
struct SingleUpdateApplyResponse {
    applied: bool,
    target: String,
    local: LocalAddonDto,
    remote: Option<RemoteCandidateDto>,
    decision: String,
    reason: Option<String>,
    remote_details: Option<AddonDetailsDto>,
    addons_dir: String,
    plan: Option<InstallPlanDto>,
    installed_new: usize,
    replaced: usize,
    skipped: usize,
    backup_dir: Option<String>,
    items: Vec<InstalledItemDto>,
}

#[derive(Debug, Serialize)]
struct InstalledItemDto {
    source_folder: Option<String>,
    target_folder: Option<String>,
    backup_folder: Option<String>,
    action: String,
    message: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct InstalledMetadata {
    addons: BTreeMap<String, InstalledAddonMetadata>,
    first_run_baseline_complete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InstalledAddonMetadata {
    remote_uid: Option<String>,
    remote_version: Option<String>,
    remote_date: Option<i64>,
    downloaded_filename: Option<String>,
    md5: Option<String>,
    installed_timestamp: String,
    source: String,
}

#[derive(Debug, Clone)]
struct UpdateConfidence {
    confidence: &'static str,
    reason: &'static str,
    managed: bool,
}

#[derive(Debug, Serialize)]
struct InstallPlanDto {
    addons_dir: String,
    temp_dir: String,
    items: Vec<InstallPlanItemDto>,
}

#[derive(Debug, Serialize)]
struct InstallPlanItemDto {
    source_folder: Option<String>,
    title: Option<String>,
    version: Option<String>,
    target_folder: Option<String>,
    action: String,
}

#[derive(Debug, Serialize)]
struct PlanUpdateAllResponse {
    dry_run: bool,
    applied: bool,
    addons_dir: String,
    remote_addons_loaded: usize,
    include_unknown: bool,
    limit: Option<usize>,
    actions: Vec<UpdateAllActionDto>,
    targets: Vec<PlannedActionDto>,
    summary: UpdateAllSummaryDto,
}

#[derive(Debug, Serialize)]
struct ApplyUpdateAllResponse {
    dry_run: bool,
    applied: bool,
    addons_dir: String,
    remote_addons_loaded: usize,
    include_unknown: bool,
    limit: Option<usize>,
    actions: Vec<UpdateAllActionDto>,
    targets: Vec<PlannedActionDto>,
    summary: UpdateAllSummaryDto,
    results: Vec<UpdateAllResultDto>,
}

#[derive(Debug, Serialize)]
struct UpdateAllActionDto {
    local_folder: String,
    remote_name: Option<String>,
    remote_uid: Option<String>,
    local_version: Option<String>,
    remote_version: Option<String>,
    remote_date: Option<i64>,
    action: String,
    update_confidence: Option<String>,
    update_reason: Option<String>,
    update_all_action: String,
}

#[derive(Debug, Serialize)]
struct UpdateAllSummaryDto {
    planned_updates: usize,
    skipped_current: usize,
    skipped_local_newer: usize,
    skipped_unknown: usize,
    skipped_no_match: usize,
    skipped_ambiguous: usize,
    skipped_libraries: usize,
}

#[derive(Debug, Serialize)]
struct UpdateAllResultDto {
    target: PlannedActionDto,
    remote_details: AddonDetailsDto,
    plan: InstallPlanDto,
    installed_new: usize,
    replaced: usize,
    skipped: usize,
    backup_dir: Option<String>,
    items: Vec<InstalledItemDto>,
}

#[derive(Debug, Serialize)]
struct ImportExistingAddonsResponse {
    addons_dir: String,
    detected_addons: usize,
    imported: usize,
    skipped_invalid_manifest: usize,
    skipped_libraries: usize,
    skipped_no_match: usize,
    skipped_ambiguous: usize,
    skipped_missing_remote_uid: usize,
    skipped_missing_remote_version: usize,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default, PartialEq, Eq)]
#[serde(default)]
struct AppSettings {
    addons_dir_override: Option<String>,
    backup_dir_override: Option<String>,
    download_dir: Option<String>,
    keep_downloads_default: bool,
    include_unknown_updates_default: bool,
}

#[derive(Debug, Serialize)]
struct AppStartupInfo {
    settings: AppSettings,
    settings_exists: bool,
    detected_addons_dir: Option<String>,
}

struct PreparedRemoteInstall {
    details: AddonDetails,
    plan: InstallPlan,
}

#[derive(Debug, Deserialize)]
struct AppSettingsInput {
    addons_dir_override: Option<String>,
    backup_dir_override: Option<String>,
    download_dir: Option<String>,
    keep_downloads_default: Option<bool>,
    include_unknown_updates_default: Option<bool>,
}

#[tauri::command]
async fn get_installed_addons(path: Option<String>) -> Result<InstalledAddonsResponse, String> {
    let addons_dir = resolve_addons_dir(path.as_deref()).map_err(to_string_error)?;
    let addons = local::scan_addons_dir(&addons_dir).map_err(to_string_error)?;
    let candidates = local::addon_path_candidates()
        .iter()
        .map(addon_path_candidate_dto)
        .collect();

    Ok(InstalledAddonsResponse {
        addons_dir: path_string(&addons_dir),
        candidates,
        addons: addons.iter().map(local_addon_dto).collect(),
    })
}

#[tauri::command]
async fn get_app_settings(app: tauri::AppHandle) -> Result<AppSettings, String> {
    load_app_settings(&app).map_err(to_string_error)
}

#[tauri::command]
async fn get_startup_info(app: tauri::AppHandle) -> Result<AppStartupInfo, String> {
    let path = settings_path(&app).map_err(to_string_error)?;
    let settings_exists = path.exists();
    let settings = load_app_settings_from_path(&path).map_err(to_string_error)?;
    let detected_addons_dir = local::detect_best_addons_dir().map(|path| path_string(&path));

    Ok(AppStartupInfo {
        settings,
        settings_exists,
        detected_addons_dir,
    })
}

#[tauri::command]
async fn save_app_settings(
    app: tauri::AppHandle,
    settings: AppSettingsInput,
) -> Result<AppSettings, String> {
    let saved = app_settings_from_input(settings);
    save_app_settings_to_disk(&app, &saved).map_err(to_string_error)?;
    Ok(saved)
}

#[tauri::command]
async fn reset_app_settings(app: tauri::AppHandle) -> Result<AppSettings, String> {
    let path = settings_path(&app).map_err(to_string_error)?;
    reset_app_settings_at_path(&path).map_err(to_string_error)
}

#[tauri::command]
async fn path_exists(path: Option<String>) -> Result<bool, String> {
    Ok(path
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(Path::new)
        .is_some_and(Path::exists))
}

#[tauri::command]
async fn import_existing_addons_as_current(
    path: Option<String>,
) -> Result<ImportExistingAddonsResponse, String> {
    let addons_dir = resolve_addons_dir(path.as_deref()).map_err(to_string_error)?;
    let local_addons = local::scan_addons_dir(&addons_dir).map_err(to_string_error)?;
    let client = ApiClient::new().map_err(to_string_error)?;
    let remote_addons = client.eso_file_list().await.map_err(to_string_error)?;
    let mut matches = match_remote::match_installed_addons(&local_addons, &remote_addons);
    matches.sort_by_key(|result| result.local.folder_name.to_lowercase());

    import_existing_matches_as_current(&addons_dir, &matches).map_err(to_string_error)
}

#[tauri::command]
async fn search_remote_addons(
    query: String,
    limit: Option<usize>,
) -> Result<SearchResponse, String> {
    let limit = limit.unwrap_or(25).clamp(1, 100);
    let needle = query.to_lowercase();
    let client = ApiClient::new().map_err(to_string_error)?;
    let addons = client.eso_file_list().await.map_err(to_string_error)?;
    let results = addons
        .iter()
        .filter(|addon| addon.searchable_text().contains(&needle))
        .take(limit)
        .map(addon_summary_dto)
        .collect();

    Ok(SearchResponse {
        query,
        limit,
        results,
    })
}

#[tauri::command]
async fn get_remote_addon_details(addon_id: String) -> Result<AddonDetailsDto, String> {
    let client = ApiClient::new().map_err(to_string_error)?;
    let details = client
        .eso_file_details(&addon_id)
        .await
        .map_err(to_string_error)?;
    Ok(addon_details_dto(&details))
}

#[tauri::command]
async fn check_addons(path: Option<String>) -> Result<CheckAddonsResponse, String> {
    let addons_dir = resolve_addons_dir(path.as_deref()).map_err(to_string_error)?;
    let local_addons = local::scan_addons_dir(&addons_dir).map_err(to_string_error)?;
    let client = ApiClient::new().map_err(to_string_error)?;
    let remote_addons = client.eso_file_list().await.map_err(to_string_error)?;
    let mut matches = match_remote::match_installed_addons(&local_addons, &remote_addons);
    matches.sort_by_key(|result| result.local.folder_name.to_lowercase());
    let metadata = ensure_first_run_baseline_complete(&addons_dir, &matches).map_err(to_string_error)?;

    Ok(CheckAddonsResponse {
        addons_dir: path_string(&addons_dir),
        remote_addons_loaded: remote_addons.len(),
        matches: matches
            .iter()
            .map(|result| match_result_dto(result, &metadata))
            .collect(),
    })
}

#[tauri::command]
async fn plan_updates(
    path: Option<String>,
    include_unknown: Option<bool>,
) -> Result<PlanUpdatesResponse, String> {
    let include_unknown = include_unknown.unwrap_or(false);
    let addons_dir = resolve_addons_dir(path.as_deref()).map_err(to_string_error)?;
    let local_addons = local::scan_addons_dir(&addons_dir).map_err(to_string_error)?;
    let client = ApiClient::new().map_err(to_string_error)?;
    let remote_addons = client.eso_file_list().await.map_err(to_string_error)?;
    let mut matches = match_remote::match_installed_addons(&local_addons, &remote_addons);
    matches.sort_by_key(|result| result.local.folder_name.to_lowercase());
    let plan = update_plan::build_update_plan(&matches, include_unknown);
    let summary = plan.summary();
    let metadata = ensure_first_run_baseline_complete(&addons_dir, &matches).map_err(to_string_error)?;

    Ok(PlanUpdatesResponse {
        addons_dir: path_string(&addons_dir),
        remote_addons_loaded: remote_addons.len(),
        include_unknown,
        matches: matches
            .iter()
            .map(|result| match_result_dto(result, &metadata))
            .collect(),
        actions: plan
            .actions
            .iter()
            .map(|action| planned_action_dto(action, &metadata))
            .collect(),
        summary: UpdatePlanSummaryDto {
            would_update: summary.would_update,
            current_skipped: summary.current_skipped,
            local_newer: summary.local_newer,
            unknown: summary.unknown,
            no_match: summary.no_match,
            ambiguous: summary.ambiguous,
            libraries: summary.libraries,
        },
    })
}

#[tauri::command]
async fn plan_update_all(
    path: Option<String>,
    include_unknown: Option<bool>,
    limit: Option<usize>,
) -> Result<PlanUpdateAllResponse, String> {
    let include_unknown = include_unknown.unwrap_or(false);
    let (addons_dir, remote_addons_loaded, mut plan) =
        build_update_all_plan_for_ui(path.as_deref(), include_unknown, limit).await?;
    apply_reliable_update_filter(&addons_dir, &mut plan);

    Ok(plan_update_all_response(
        &addons_dir,
        remote_addons_loaded,
        include_unknown,
        limit,
        &plan,
    ))
}

#[tauri::command]
async fn apply_update_all(
    path: Option<String>,
    backup_dir: Option<String>,
    keep_download: Option<bool>,
    download_dir: Option<String>,
    include_unknown: Option<bool>,
    limit: Option<usize>,
) -> Result<ApplyUpdateAllResponse, String> {
    let include_unknown = include_unknown.unwrap_or(false);
    let backup_dir = optional_path(backup_dir);
    let download_dir = optional_path(download_dir);
    let (addons_dir, remote_addons_loaded, mut plan) =
        build_update_all_plan_for_ui(path.as_deref(), include_unknown, limit).await?;
    apply_reliable_update_filter(&addons_dir, &mut plan);
    let client = ApiClient::new().map_err(to_string_error)?;
    let mut results = Vec::new();

    for target in &plan.targets {
        let remote_uid = target.remote_uid.as_deref().ok_or_else(|| {
            format!(
                "planned addon {} has no clean remote UID",
                target.local_folder
            )
        })?;
        let prepared = prepare_remote_install_plan(&client, remote_uid, &addons_dir).await?;
        validate_single_update_plan(&prepared.plan, &target.local_folder)
            .map_err(to_string_error)?;

        if keep_download.unwrap_or(false) {
            keep_remote_download(
                &client,
                remote_uid,
                &prepared.details,
                download_dir.as_deref(),
            )
            .await?;
        }

        let result = apply::apply_install_plan(&prepared.plan, backup_dir.as_deref())
            .map_err(|error| format!("failed to update {}: {}", target.local_folder, error))?;
        record_installed_metadata(
            &addons_dir,
            &prepared.details,
            remote_uid,
            &result,
            "update-all",
        )
        .map_err(to_string_error)?;

        results.push(UpdateAllResultDto {
            target: planned_action_dto(target, &load_installed_metadata(&addons_dir)),
            remote_details: addon_details_dto(&prepared.details),
            plan: install_plan_dto(&prepared.plan),
            installed_new: result.installed_new,
            replaced: result.replaced,
            skipped: result.skipped,
            backup_dir: result.backup_dir.as_ref().map(|path| path_string(path)),
            items: result.items.iter().map(installed_item_dto).collect(),
        });
    }

    Ok(apply_update_all_response(
        &addons_dir,
        remote_addons_loaded,
        include_unknown,
        limit,
        &plan,
        results,
    ))
}

#[tauri::command]
async fn plan_remote_install(
    addon_id: String,
    path: Option<String>,
) -> Result<PlanRemoteInstallResponse, String> {
    let addons_dir = resolve_addons_dir(path.as_deref()).map_err(to_string_error)?;
    let client = ApiClient::new().map_err(to_string_error)?;
    let prepared = prepare_remote_install_plan(&client, &addon_id, &addons_dir).await?;

    Ok(PlanRemoteInstallResponse {
        dry_run: true,
        applied: false,
        remote: addon_details_dto(&prepared.details),
        addons_dir: path_string(&addons_dir),
        plan: install_plan_dto(&prepared.plan),
    })
}

#[tauri::command]
async fn install_remote_addon(
    addon_id: String,
    path: Option<String>,
    backup_dir: Option<String>,
    keep_download: Option<bool>,
    download_dir: Option<String>,
) -> Result<InstallRemoteAddonResponse, String> {
    let addons_dir = resolve_addons_dir(path.as_deref()).map_err(to_string_error)?;
    let backup_dir = optional_path(backup_dir);
    let download_dir = optional_path(download_dir);
    let client = ApiClient::new().map_err(to_string_error)?;
    let prepared = prepare_remote_install_plan(&client, &addon_id, &addons_dir).await?;

    if keep_download.unwrap_or(false) {
        keep_remote_download(
            &client,
            &addon_id,
            &prepared.details,
            download_dir.as_deref(),
        )
        .await?;
    }

    let result = apply::apply_install_plan(&prepared.plan, backup_dir.as_deref())
        .map_err(to_string_error)?;
    record_installed_metadata(
        &addons_dir,
        &prepared.details,
        &addon_id,
        &result,
        "remote-install",
    )
    .map_err(to_string_error)?;
    Ok(install_remote_addon_response(
        &prepared.details,
        &addons_dir,
        &prepared.plan,
        &result,
    ))
}

#[tauri::command]
async fn plan_single_update(
    target: String,
    path: Option<String>,
    force: Option<bool>,
) -> Result<SingleUpdatePlanResponse, String> {
    let force = force.unwrap_or(false);
    let addons_dir = resolve_addons_dir(path.as_deref()).map_err(to_string_error)?;
    let local_addons = local::scan_addons_dir(&addons_dir).map_err(to_string_error)?;
    let client = ApiClient::new().map_err(to_string_error)?;
    let remote_addons = client.eso_file_list().await.map_err(to_string_error)?;
    let matches = match_remote::match_installed_addons(&local_addons, &remote_addons);
    let selected = update::resolve_update_request(&matches, &target).map_err(to_string_error)?;
    let decision = update::update_decision(selected, force);

    if !decision.should_install() {
        return Ok(SingleUpdatePlanResponse {
            dry_run: true,
            applied: false,
            target,
            local: local_addon_dto(&selected.local),
            remote: selected.remote.as_ref().map(remote_candidate_dto),
            decision: decision.as_str().to_owned(),
            should_install: false,
            reason: Some(update_skip_reason(&decision).to_owned()),
            remote_details: None,
            addons_dir: path_string(&addons_dir),
            plan: None,
        });
    }

    let remote_uid = selected
        .remote
        .as_ref()
        .and_then(|remote| remote.uid.as_deref())
        .ok_or_else(|| "selected addon has no clean remote UID".to_owned())?;
    let prepared = prepare_remote_install_plan(&client, remote_uid, &addons_dir).await?;
    validate_single_update_plan(&prepared.plan, &selected.local.folder_name)
        .map_err(to_string_error)?;

    Ok(SingleUpdatePlanResponse {
        dry_run: true,
        applied: false,
        target,
        local: local_addon_dto(&selected.local),
        remote: selected.remote.as_ref().map(remote_candidate_dto),
        decision: decision.as_str().to_owned(),
        should_install: true,
        reason: None,
        remote_details: Some(addon_details_dto(&prepared.details)),
        addons_dir: path_string(&addons_dir),
        plan: Some(install_plan_dto(&prepared.plan)),
    })
}

#[tauri::command]
async fn apply_single_update(
    target: String,
    path: Option<String>,
    backup_dir: Option<String>,
    keep_download: Option<bool>,
    download_dir: Option<String>,
    force: Option<bool>,
) -> Result<SingleUpdateApplyResponse, String> {
    let force = force.unwrap_or(false);
    let addons_dir = resolve_addons_dir(path.as_deref()).map_err(to_string_error)?;
    let backup_dir = optional_path(backup_dir);
    let download_dir = optional_path(download_dir);
    let local_addons = local::scan_addons_dir(&addons_dir).map_err(to_string_error)?;
    let client = ApiClient::new().map_err(to_string_error)?;
    let remote_addons = client.eso_file_list().await.map_err(to_string_error)?;
    let matches = match_remote::match_installed_addons(&local_addons, &remote_addons);
    let selected = update::resolve_update_request(&matches, &target).map_err(to_string_error)?;
    let decision = update::update_decision(selected, force);

    if !decision.should_install() {
        return Ok(SingleUpdateApplyResponse {
            applied: false,
            target,
            local: local_addon_dto(&selected.local),
            remote: selected.remote.as_ref().map(remote_candidate_dto),
            decision: decision.as_str().to_owned(),
            reason: Some(update_skip_reason(&decision).to_owned()),
            remote_details: None,
            addons_dir: path_string(&addons_dir),
            plan: None,
            installed_new: 0,
            replaced: 0,
            skipped: 0,
            backup_dir: None,
            items: Vec::new(),
        });
    }

    let remote_uid = selected
        .remote
        .as_ref()
        .and_then(|remote| remote.uid.as_deref())
        .ok_or_else(|| "selected addon has no clean remote UID".to_owned())?;
    let prepared = prepare_remote_install_plan(&client, remote_uid, &addons_dir).await?;
    validate_single_update_plan(&prepared.plan, &selected.local.folder_name)
        .map_err(to_string_error)?;

    if keep_download.unwrap_or(false) {
        keep_remote_download(
            &client,
            remote_uid,
            &prepared.details,
            download_dir.as_deref(),
        )
        .await?;
    }

    let result = apply::apply_install_plan(&prepared.plan, backup_dir.as_deref())
        .map_err(to_string_error)?;
    record_installed_metadata(
        &addons_dir,
        &prepared.details,
        remote_uid,
        &result,
        "update",
    )
    .map_err(to_string_error)?;
    Ok(single_update_apply_response(
        &target,
        selected,
        &decision,
        &prepared.details,
        &addons_dir,
        &prepared.plan,
        &result,
    ))
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            get_startup_info,
            get_app_settings,
            save_app_settings,
            reset_app_settings,
            path_exists,
            import_existing_addons_as_current,
            get_installed_addons,
            search_remote_addons,
            get_remote_addon_details,
            check_addons,
            plan_updates,
            plan_update_all,
            apply_update_all,
            plan_remote_install,
            install_remote_addon,
            plan_single_update,
            apply_single_update
        ])
        .run(tauri::generate_context!())
        .expect("error while running Scribe Addon Manager");
}

fn settings_path(app: &tauri::AppHandle) -> anyhow::Result<PathBuf> {
    let config_dir = app.path().app_config_dir()?;
    Ok(app_settings_path(&config_dir))
}

fn load_app_settings(app: &tauri::AppHandle) -> anyhow::Result<AppSettings> {
    let path = settings_path(app)?;
    load_app_settings_from_path(&path)
}

fn save_app_settings_to_disk(app: &tauri::AppHandle, settings: &AppSettings) -> anyhow::Result<()> {
    let path = settings_path(app)?;
    save_app_settings_to_path(&path, settings)
}

fn app_settings_path(config_dir: &Path) -> PathBuf {
    config_dir.join("settings.json")
}

fn load_app_settings_from_path(path: &Path) -> anyhow::Result<AppSettings> {
    if !path.exists() {
        return Ok(AppSettings::default());
    }
    let content = fs::read_to_string(path)?;
    let settings = serde_json::from_str::<AppSettings>(&content)?;
    Ok(settings)
}

fn save_app_settings_to_path(path: &Path, settings: &AppSettings) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(settings)?;
    fs::write(path, content)?;
    Ok(())
}

fn reset_app_settings_at_path(path: &Path) -> anyhow::Result<AppSettings> {
    let settings = AppSettings::default();
    save_app_settings_to_path(path, &settings)?;
    Ok(settings)
}

fn app_settings_from_input(settings: AppSettingsInput) -> AppSettings {
    AppSettings {
        addons_dir_override: normalize_optional_path(settings.addons_dir_override),
        backup_dir_override: normalize_optional_path(settings.backup_dir_override),
        download_dir: normalize_optional_path(settings.download_dir),
        keep_downloads_default: settings.keep_downloads_default.unwrap_or(false),
        include_unknown_updates_default: settings.include_unknown_updates_default.unwrap_or(false),
    }
}

fn normalize_optional_path(value: Option<String>) -> Option<String> {
    value.and_then(|path| {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        }
    })
}

fn optional_path(value: Option<String>) -> Option<PathBuf> {
    normalize_optional_path(value).map(PathBuf::from)
}

fn resolve_addons_dir(path: Option<&str>) -> anyhow::Result<PathBuf> {
    match path.map(str::trim).filter(|path| !path.is_empty()) {
        Some(path) => Ok(PathBuf::from(path)),
        None => local::detect_best_addons_dir()
            .ok_or_else(|| anyhow::anyhow!("could not auto-detect an ESO AddOns directory")),
    }
}

fn addon_path_candidate_dto(candidate: &AddonPathCandidate) -> AddonPathCandidateDto {
    AddonPathCandidateDto {
        path: path_string(&candidate.path),
        exists: candidate.exists,
        contains_addons: candidate.contains_addons,
    }
}

fn local_addon_dto(addon: &LocalAddon) -> LocalAddonDto {
    LocalAddonDto {
        folder_name: addon.folder_name.clone(),
        folder_path: path_string(&addon.folder_path),
        manifest_path: addon.manifest_path.as_ref().map(|path| path_string(path)),
        title: addon.title.clone(),
        addon_version: addon.addon_version.clone(),
        version: addon.version.clone(),
        display_version: addon
            .addon_version
            .clone()
            .or_else(|| addon.version.clone()),
        api_versions: addon.api_versions.clone(),
        depends_on: addon.depends_on.clone(),
        optional_depends_on: addon.optional_depends_on.clone(),
        is_library: addon.is_library,
        author: addon.author.clone(),
        description: addon.description.clone(),
        valid_manifest: addon.valid_manifest,
    }
}

fn addon_summary_dto(addon: &AddonSummary) -> AddonSummaryDto {
    AddonSummaryDto {
        uid: addon.uid.clone(),
        name: addon.name.clone(),
        author_name: addon.author_name.clone(),
        version: addon.version.clone(),
        updated: addon.date,
        updated_display: addon.date.map(format_mmoui_date),
        file_info_url: addon.file_info_url.clone(),
        summary: addon.summary.clone(),
        directories: addon.directories.clone(),
        category_id: addon.category_id(),
        category_name: addon.category_name(),
        downloads: addon.downloads(),
        monthly_downloads: addon.monthly_downloads(),
    }
}

fn addon_details_dto(details: &AddonDetails) -> AddonDetailsDto {
    AddonDetailsDto {
        uid: details.uid.clone(),
        name: details.name.clone(),
        author_name: details.author_name.clone(),
        version: details.version.clone(),
        updated: details.date,
        updated_display: details.date.map(format_mmoui_date),
        file_name: details.file_name.clone(),
        md5: details.md5.clone(),
        download_url: details.download_url.clone(),
        file_info_url: details.file_info_url.clone(),
        description: details.description.clone(),
        changelog: details.changelog.clone(),
        category_id: details.category_id(),
        category_name: details.category_name(),
        downloads: details.downloads(),
        monthly_downloads: details.monthly_downloads(),
    }
}

fn match_result_dto(result: &MatchResult, metadata: &InstalledMetadata) -> MatchResultDto {
    let confidence = update_confidence_for_match(result, metadata);
    MatchResultDto {
        local: local_addon_dto(&result.local),
        status: result.status.as_str().to_owned(),
        update_confidence: confidence.confidence.to_owned(),
        update_reason: confidence.reason.to_owned(),
        managed: confidence.managed,
        remote: result.remote.as_ref().map(remote_candidate_dto),
        candidates: result.candidates.iter().map(remote_candidate_dto).collect(),
        debug_candidates: result
            .debug_candidates
            .iter()
            .map(remote_candidate_dto)
            .collect(),
    }
}

fn remote_candidate_dto(candidate: &RemoteCandidate) -> RemoteCandidateDto {
    RemoteCandidateDto {
        uid: candidate.uid.clone(),
        name: candidate.name.clone(),
        author_name: candidate.author_name.clone(),
        version: candidate.version.clone(),
        updated: candidate.updated,
        updated_display: candidate.updated.map(format_mmoui_date),
        file_info_url: candidate.file_info_url.clone(),
        summary: candidate.summary.clone(),
        directories: candidate.directories.clone(),
        category_id: candidate.category_id.clone(),
        category_name: candidate.category_name.clone(),
        downloads: candidate.downloads,
        monthly_downloads: candidate.monthly_downloads,
        tier: candidate.tier,
        score: candidate.score,
        reason: candidate.reason.clone(),
    }
}

fn planned_action_dto(
    action: &PlannedAddonAction,
    metadata: &InstalledMetadata,
) -> PlannedActionDto {
    let confidence = update_confidence_for_action(action, metadata);
    PlannedActionDto {
        local_folder: action.local_folder.clone(),
        remote_name: action.remote_name.clone(),
        remote_uid: action.remote_uid.clone(),
        local_version: action.local_version.clone(),
        remote_version: action.remote_version.clone(),
        remote_date: action.remote_date,
        action: action.kind.as_str().to_owned(),
        update_confidence: confidence.as_ref().map(|value| value.confidence.to_owned()),
        update_reason: confidence.as_ref().map(|value| value.reason.to_owned()),
    }
}

fn update_all_action_dto(
    action: &PlannedAddonAction,
    targets: &[PlannedAddonAction],
    metadata: &InstalledMetadata,
) -> UpdateAllActionDto {
    let confidence = update_confidence_for_action(action, metadata);
    UpdateAllActionDto {
        local_folder: action.local_folder.clone(),
        remote_name: action.remote_name.clone(),
        remote_uid: action.remote_uid.clone(),
        local_version: action.local_version.clone(),
        remote_version: action.remote_version.clone(),
        remote_date: action.remote_date,
        action: action.kind.as_str().to_owned(),
        update_confidence: confidence.as_ref().map(|value| value.confidence.to_owned()),
        update_reason: confidence.as_ref().map(|value| value.reason.to_owned()),
        update_all_action: update_all_action_label(action, targets).to_owned(),
    }
}

fn update_confidence_for_match(
    result: &MatchResult,
    metadata: &InstalledMetadata,
) -> UpdateConfidence {
    let managed = metadata.addons.get(&result.local.folder_name);

    let Some(remote) = result.remote.as_ref() else {
        if let Some(managed) = managed.filter(|managed| is_first_run_baseline(managed)) {
            return first_run_baseline_confidence(managed);
        }

        return match result.status {
            match_remote::MatchStatus::Ambiguous => UpdateConfidence {
                confidence: "unknown",
                reason: "remote match is ambiguous",
                managed: false,
            },
            _ => UpdateConfidence {
                confidence: "unknown",
                reason: "remote match missing",
                managed: false,
            },
        };
    };

    let Some(managed) = managed else {
        return match result.status {
            match_remote::MatchStatus::Matched => UpdateConfidence {
                confidence: "current",
                reason: "versions match",
                managed: false,
            },
            match_remote::MatchStatus::LocalNewer => UpdateConfidence {
                confidence: "local-newer",
                reason: "local version source is manifest only",
                managed: false,
            },
            match_remote::MatchStatus::PossibleUpdate => UpdateConfidence {
                confidence: "possible-update",
                reason: "manager install metadata missing",
                managed: false,
            },
            _ => UpdateConfidence {
                confidence: "unknown",
                reason: "manager install metadata missing",
                managed: false,
            },
        };
    };

    if managed.remote_uid != remote.uid {
        if is_first_run_baseline(managed) && managed.remote_uid.is_none() {
            return confidence_from_versions(
                managed.remote_version.as_deref(),
                remote.version.as_deref(),
                managed.remote_date,
                remote.updated,
                true,
            );
        }

        return UpdateConfidence {
            confidence: "unknown",
            reason: "stored remote UID differs",
            managed: true,
        };
    }

    confidence_from_versions(
        managed.remote_version.as_deref(),
        remote.version.as_deref(),
        managed.remote_date,
        remote.updated,
        true,
    )
}

fn update_confidence_for_action(
    action: &PlannedAddonAction,
    metadata: &InstalledMetadata,
) -> Option<UpdateConfidence> {
    let managed = metadata.addons.get(&action.local_folder);

    match managed {
        Some(managed) if managed.remote_uid == action.remote_uid => Some(confidence_from_versions(
            managed.remote_version.as_deref(),
            action.remote_version.as_deref(),
            managed.remote_date,
            action.remote_date,
            true,
        )),
        Some(managed) if is_first_run_baseline(managed) && managed.remote_uid.is_none() => {
            Some(confidence_from_versions(
                managed.remote_version.as_deref(),
                action.remote_version.as_deref(),
                managed.remote_date,
                action.remote_date,
                true,
            ))
        }
        Some(_) => Some(UpdateConfidence {
            confidence: "unknown",
            reason: "stored remote UID differs",
            managed: true,
        }),
        None => match action.kind {
            update_plan::PlannedActionKind::WouldUpdate => Some(UpdateConfidence {
                confidence: "possible-update",
                reason: "manager install metadata missing",
                managed: false,
            }),
            update_plan::PlannedActionKind::WouldSkipCurrent => Some(UpdateConfidence {
                confidence: "current",
                reason: "versions match",
                managed: false,
            }),
            update_plan::PlannedActionKind::WouldSkipLocalNewer => Some(UpdateConfidence {
                confidence: "local-newer",
                reason: "local version source is manifest only",
                managed: false,
            }),
            _ => Some(UpdateConfidence {
                confidence: "unknown",
                reason: "manager install metadata missing",
                managed: false,
            }),
        },
    }
}

fn is_first_run_baseline(managed: &InstalledAddonMetadata) -> bool {
    managed.source == "first-run-import"
}

fn first_run_baseline_confidence(_managed: &InstalledAddonMetadata) -> UpdateConfidence {
    UpdateConfidence {
        confidence: "current",
        reason: "first-run import baseline",
        managed: true,
    }
}

fn confidence_from_versions(
    installed_remote_version: Option<&str>,
    current_remote_version: Option<&str>,
    installed_remote_date: Option<i64>,
    current_remote_date: Option<i64>,
    managed: bool,
) -> UpdateConfidence {
    match compare_versions(installed_remote_version, current_remote_version) {
        VersionComparison::RemoteNewer => UpdateConfidence {
            confidence: "reliable-update",
            reason: "remote version differs",
            managed,
        },
        VersionComparison::Same => UpdateConfidence {
            confidence: "current",
            reason: "versions match",
            managed,
        },
        VersionComparison::LocalNewer => UpdateConfidence {
            confidence: "local-newer",
            reason: "local version source is manager metadata",
            managed,
        },
        VersionComparison::Unknown => UpdateConfidence {
            confidence: date_confidence(installed_remote_date, current_remote_date),
            reason: "remote version unavailable; compared remote update date",
            managed,
        },
    }
}

fn date_confidence(
    installed_remote_date: Option<i64>,
    current_remote_date: Option<i64>,
) -> &'static str {
    match (installed_remote_date, current_remote_date) {
        (Some(installed), Some(current)) if current > installed => "reliable-update",
        (Some(installed), Some(current)) if current == installed => "current",
        (Some(installed), Some(current)) if current < installed => "local-newer",
        (None, None) => "current",
        _ => "unknown",
    }
}

fn update_all_action_label(
    action: &PlannedAddonAction,
    targets: &[PlannedAddonAction],
) -> &'static str {
    if targets
        .iter()
        .any(|target| target.local_folder == action.local_folder)
    {
        "would-update"
    } else {
        "would-skip"
    }
}

fn update_all_summary_dto(plan: &update_all::UpdateAllPlan) -> UpdateAllSummaryDto {
    let mut skipped_current = 0;
    let mut skipped_local_newer = 0;
    let mut skipped_unknown = 0;
    let mut skipped_no_match = 0;
    let mut skipped_ambiguous = 0;
    let mut skipped_libraries = 0;

    for action in &plan.display_plan.actions {
        if plan
            .targets
            .iter()
            .any(|target| target.local_folder == action.local_folder)
        {
            continue;
        }

        match action.kind {
            update_plan::PlannedActionKind::WouldUpdate => {}
            update_plan::PlannedActionKind::WouldSkipCurrent => skipped_current += 1,
            update_plan::PlannedActionKind::WouldSkipLocalNewer => skipped_local_newer += 1,
            update_plan::PlannedActionKind::WouldSkipUnknownVersion => skipped_unknown += 1,
            update_plan::PlannedActionKind::WouldSkipNoMatch => skipped_no_match += 1,
            update_plan::PlannedActionKind::WouldSkipAmbiguous => skipped_ambiguous += 1,
            update_plan::PlannedActionKind::WouldSkipLibrary => skipped_libraries += 1,
        }
    }

    UpdateAllSummaryDto {
        planned_updates: plan.targets.len(),
        skipped_current,
        skipped_local_newer,
        skipped_unknown,
        skipped_no_match,
        skipped_ambiguous,
        skipped_libraries,
    }
}

fn plan_update_all_response(
    addons_dir: &std::path::Path,
    remote_addons_loaded: usize,
    include_unknown: bool,
    limit: Option<usize>,
    plan: &update_all::UpdateAllPlan,
) -> PlanUpdateAllResponse {
    let metadata = load_installed_metadata(addons_dir);
    PlanUpdateAllResponse {
        dry_run: true,
        applied: false,
        addons_dir: path_string(addons_dir),
        remote_addons_loaded,
        include_unknown,
        limit,
        actions: plan
            .display_plan
            .actions
            .iter()
            .map(|action| update_all_action_dto(action, &plan.targets, &metadata))
            .collect(),
        targets: plan
            .targets
            .iter()
            .map(|target| planned_action_dto(target, &metadata))
            .collect(),
        summary: update_all_summary_dto(plan),
    }
}

fn apply_update_all_response(
    addons_dir: &std::path::Path,
    remote_addons_loaded: usize,
    include_unknown: bool,
    limit: Option<usize>,
    plan: &update_all::UpdateAllPlan,
    results: Vec<UpdateAllResultDto>,
) -> ApplyUpdateAllResponse {
    let metadata = load_installed_metadata(addons_dir);
    ApplyUpdateAllResponse {
        dry_run: false,
        applied: !results.is_empty(),
        addons_dir: path_string(addons_dir),
        remote_addons_loaded,
        include_unknown,
        limit,
        actions: plan
            .display_plan
            .actions
            .iter()
            .map(|action| update_all_action_dto(action, &plan.targets, &metadata))
            .collect(),
        targets: plan
            .targets
            .iter()
            .map(|target| planned_action_dto(target, &metadata))
            .collect(),
        summary: update_all_summary_dto(plan),
        results,
    }
}

fn install_plan_dto(plan: &InstallPlan) -> InstallPlanDto {
    InstallPlanDto {
        addons_dir: path_string(&plan.addons_dir),
        temp_dir: path_string(&plan.temp_dir),
        items: plan.items.iter().map(install_plan_item_dto).collect(),
    }
}

fn install_plan_item_dto(item: &InstallPlanItem) -> InstallPlanItemDto {
    InstallPlanItemDto {
        source_folder: item.source_folder.clone(),
        title: item.title.clone(),
        version: item.version.clone(),
        target_folder: item.target_folder.as_ref().map(|path| path_string(path)),
        action: item.action.as_str().to_owned(),
    }
}

async fn prepare_remote_install_plan(
    client: &ApiClient,
    addon_id: &str,
    addons_dir: &std::path::Path,
) -> Result<PreparedRemoteInstall, String> {
    let installed_addons =
        scan_installed_addons_for_install(addons_dir).map_err(to_string_error)?;
    let details = client
        .eso_file_details(addon_id)
        .await
        .map_err(to_string_error)?;
    let download_url = remote::download_url(&details).map_err(to_string_error)?;
    let bytes = client
        .download_bytes(download_url)
        .await
        .map_err(to_string_error)?;
    remote::verify_md5(&bytes, details.md5.as_deref()).map_err(to_string_error)?;

    let temp_file = tempfile::Builder::new()
        .prefix("eso-addon-manager-ui-install-")
        .suffix(".zip")
        .tempfile()
        .map_err(to_string_error)?;
    std::fs::write(temp_file.path(), &bytes).map_err(to_string_error)?;
    let extracted = zip_safety::extract_zip_to_temp(temp_file.path()).map_err(to_string_error)?;
    let install_plan =
        plan::plan_install(&extracted, addons_dir, &installed_addons).map_err(to_string_error)?;

    Ok(PreparedRemoteInstall {
        details,
        plan: install_plan,
    })
}

async fn build_update_all_plan_for_ui(
    path: Option<&str>,
    include_unknown: bool,
    limit: Option<usize>,
) -> Result<(PathBuf, usize, update_all::UpdateAllPlan), String> {
    let addons_dir = resolve_addons_dir(path).map_err(to_string_error)?;
    let local_addons = local::scan_addons_dir(&addons_dir).map_err(to_string_error)?;
    let client = ApiClient::new().map_err(to_string_error)?;
    let remote_addons = client.eso_file_list().await.map_err(to_string_error)?;
    let mut matches = match_remote::match_installed_addons(&local_addons, &remote_addons);
    matches.sort_by_key(|result| result.local.folder_name.to_lowercase());

    if let Some(limit) = limit {
        matches.truncate(limit);
    }

    ensure_first_run_baseline_complete(&addons_dir, &matches).map_err(to_string_error)?;
    let plan = update_all::build_update_all_plan(&matches, include_unknown);
    Ok((addons_dir, remote_addons.len(), plan))
}

fn apply_reliable_update_filter(
    addons_dir: &std::path::Path,
    plan: &mut update_all::UpdateAllPlan,
) {
    let metadata = load_installed_metadata(addons_dir);
    plan.targets.retain(|target| {
        update_confidence_for_action(target, &metadata)
            .is_some_and(|confidence| confidence.confidence == "reliable-update")
    });
}

fn metadata_path(addons_dir: &std::path::Path) -> PathBuf {
    addons_dir
        .parent()
        .unwrap_or(addons_dir)
        .join(".scribe-addon-manager")
        .join("installed.json")
}

fn load_installed_metadata(addons_dir: &std::path::Path) -> InstalledMetadata {
    let path = metadata_path(addons_dir);
    fs::read_to_string(path)
        .ok()
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or_default()
}

fn ensure_first_run_baseline_complete(
    addons_dir: &std::path::Path,
    matches: &[MatchResult],
) -> anyhow::Result<InstalledMetadata> {
    let mut metadata = load_installed_metadata(addons_dir);
    if metadata.first_run_baseline_complete || !metadata_has_first_run_baseline(&metadata) {
        return Ok(metadata);
    }

    let installed_timestamp = current_timestamp_string();

    for result in matches {
        if !result.local.valid_manifest || metadata.addons.contains_key(&result.local.folder_name) {
            continue;
        }

        metadata.addons.insert(
            result.local.folder_name.clone(),
            first_run_metadata_for_match(result, &installed_timestamp),
        );
    }

    metadata.first_run_baseline_complete = true;
    save_installed_metadata(addons_dir, &metadata)?;

    Ok(metadata)
}

fn metadata_has_first_run_baseline(metadata: &InstalledMetadata) -> bool {
    metadata.addons.values().any(is_first_run_baseline)
}

fn save_installed_metadata(
    addons_dir: &std::path::Path,
    metadata: &InstalledMetadata,
) -> anyhow::Result<()> {
    let path = metadata_path(addons_dir);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(metadata)?)?;
    Ok(())
}

fn current_timestamp_string() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}

fn first_run_metadata_for_match(
    result: &MatchResult,
    installed_timestamp: &str,
) -> InstalledAddonMetadata {
    let remote = result.remote.as_ref();
    InstalledAddonMetadata {
        remote_uid: remote.and_then(|remote| normalize_optional_path(remote.uid.clone())),
        remote_version: remote.and_then(|remote| normalize_optional_path(remote.version.clone())),
        remote_date: remote.and_then(|remote| remote.updated),
        downloaded_filename: None,
        md5: None,
        installed_timestamp: installed_timestamp.to_owned(),
        source: "first-run-import".to_owned(),
    }
}

fn import_existing_matches_as_current(
    addons_dir: &std::path::Path,
    matches: &[MatchResult],
) -> anyhow::Result<ImportExistingAddonsResponse> {
    let mut metadata = load_installed_metadata(addons_dir);
    let installed_timestamp = current_timestamp_string();
    let mut response = ImportExistingAddonsResponse {
        addons_dir: path_string(addons_dir),
        detected_addons: 0,
        imported: 0,
        skipped_invalid_manifest: 0,
        skipped_libraries: 0,
        skipped_no_match: 0,
        skipped_ambiguous: 0,
        skipped_missing_remote_uid: 0,
        skipped_missing_remote_version: 0,
    };

    for result in matches {
        if !result.local.valid_manifest {
            response.skipped_invalid_manifest += 1;
            continue;
        }

        response.detected_addons += 1;

        metadata.addons.insert(
            result.local.folder_name.clone(),
            first_run_metadata_for_match(result, &installed_timestamp),
        );
        response.imported += 1;
    }

    metadata.first_run_baseline_complete = true;
    if response.imported > 0 {
        save_installed_metadata(addons_dir, &metadata)?;
    }

    Ok(response)
}

fn record_installed_metadata(
    addons_dir: &std::path::Path,
    details: &AddonDetails,
    remote_uid: &str,
    result: &InstallResult,
    source: &str,
) -> anyhow::Result<()> {
    if !install_result_applied(result) {
        return Ok(());
    }

    let mut metadata = load_installed_metadata(addons_dir);
    let installed_timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string();
    let downloaded_filename = Some(remote::download_file_name(details, remote_uid));

    for item in &result.items {
        if matches!(item.action, InstallActionPerformed::Skipped) {
            continue;
        }

        let Some(folder_name) = item
            .target_folder
            .as_ref()
            .and_then(|path| path.file_name())
        else {
            continue;
        };

        metadata.addons.insert(
            folder_name.to_string_lossy().to_string(),
            InstalledAddonMetadata {
                remote_uid: details.uid.clone().or_else(|| Some(remote_uid.to_owned())),
                remote_version: details.version.clone(),
                remote_date: details.date,
                downloaded_filename: downloaded_filename.clone(),
                md5: details.md5.clone(),
                installed_timestamp: installed_timestamp.clone(),
                source: source.to_owned(),
            },
        );
    }

    save_installed_metadata(addons_dir, &metadata)
}

fn install_remote_addon_response(
    details: &AddonDetails,
    addons_dir: &std::path::Path,
    plan: &InstallPlan,
    result: &InstallResult,
) -> InstallRemoteAddonResponse {
    InstallRemoteAddonResponse {
        applied: install_result_applied(result),
        installed_new: result.installed_new,
        replaced: result.replaced,
        skipped: result.skipped,
        backup_dir: result.backup_dir.as_ref().map(|path| path_string(path)),
        remote: addon_details_dto(details),
        addons_dir: path_string(addons_dir),
        plan: install_plan_dto(plan),
        items: result.items.iter().map(installed_item_dto).collect(),
    }
}

fn single_update_apply_response(
    target: &str,
    selected: &MatchResult,
    decision: &update::UpdateDecision,
    details: &AddonDetails,
    addons_dir: &std::path::Path,
    plan: &InstallPlan,
    result: &InstallResult,
) -> SingleUpdateApplyResponse {
    SingleUpdateApplyResponse {
        applied: install_result_applied(result),
        target: target.to_owned(),
        local: local_addon_dto(&selected.local),
        remote: selected.remote.as_ref().map(remote_candidate_dto),
        decision: decision.as_str().to_owned(),
        reason: None,
        remote_details: Some(addon_details_dto(details)),
        addons_dir: path_string(addons_dir),
        plan: Some(install_plan_dto(plan)),
        installed_new: result.installed_new,
        replaced: result.replaced,
        skipped: result.skipped,
        backup_dir: result.backup_dir.as_ref().map(|path| path_string(path)),
        items: result.items.iter().map(installed_item_dto).collect(),
    }
}

fn installed_item_dto(item: &InstalledItem) -> InstalledItemDto {
    InstalledItemDto {
        source_folder: item.source_folder.clone(),
        target_folder: item.target_folder.as_ref().map(|path| path_string(path)),
        backup_folder: item.backup_folder.as_ref().map(|path| path_string(path)),
        action: install_action_as_str(&item.action).to_owned(),
        message: item.message.clone(),
    }
}

async fn keep_remote_download(
    client: &ApiClient,
    addon_id: &str,
    details: &AddonDetails,
    download_dir: Option<&std::path::Path>,
) -> Result<(), String> {
    let download_url = remote::download_url(details).map_err(to_string_error)?;
    let file_name = remote::download_file_name(details, addon_id);
    let path = remote::keep_download_path(download_dir, &file_name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(to_string_error)?;
    }
    let bytes = client
        .download_bytes(download_url)
        .await
        .map_err(to_string_error)?;
    remote::verify_md5(&bytes, details.md5.as_deref()).map_err(to_string_error)?;
    std::fs::write(path, bytes).map_err(to_string_error)?;
    Ok(())
}

fn install_result_applied(result: &InstallResult) -> bool {
    result.installed_new > 0 || result.replaced > 0
}

fn install_action_as_str(action: &InstallActionPerformed) -> &'static str {
    match action {
        InstallActionPerformed::InstalledNew => "installed-new",
        InstallActionPerformed::ReplacedExisting => "replaced-existing",
        InstallActionPerformed::Skipped => "skipped",
    }
}

fn validate_single_update_plan(plan: &InstallPlan, local_folder: &str) -> anyhow::Result<()> {
    let actionable = plan
        .items
        .iter()
        .filter(|item| {
            matches!(
                item.action,
                InstallPlanAction::WouldInstallNew | InstallPlanAction::WouldReplaceExisting
            )
        })
        .collect::<Vec<_>>();

    if actionable.len() != 1 {
        return Err(anyhow::anyhow!(
            "update would affect {} addon folders; refusing to update more than one addon",
            actionable.len()
        ));
    }

    let target_name = actionable[0]
        .target_folder
        .as_ref()
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if !target_name.eq_ignore_ascii_case(local_folder) {
        return Err(anyhow::anyhow!(
            "remote package would target folder {target_name}, not selected local folder {local_folder}; refusing conservative update"
        ));
    }

    Ok(())
}

fn update_skip_reason(decision: &update::UpdateDecision) -> &'static str {
    match decision {
        update::UpdateDecision::SkippedCurrent => "selected addon is current",
        update::UpdateDecision::SkippedLocalNewer => "local addon version is newer",
        update::UpdateDecision::SkippedUnknownUseForce => {
            "version comparison is unknown; enable force to reinstall"
        }
        update::UpdateDecision::SkippedNoMatch => "no clean remote match is available",
        update::UpdateDecision::SkippedAmbiguous => "remote match is ambiguous",
        update::UpdateDecision::WouldUpdate | update::UpdateDecision::ForcedReinstall => {
            "update can proceed"
        }
    }
}

fn scan_installed_addons_for_install(
    addons_dir: &std::path::Path,
) -> anyhow::Result<Vec<LocalAddon>> {
    if !addons_dir.exists() {
        return Ok(Vec::new());
    }

    Ok(local::scan_addons_dir(addons_dir)?)
}

fn path_string(path: &std::path::Path) -> String {
    path.display().to_string()
}

fn format_mmoui_date(timestamp: i64) -> String {
    let seconds = if timestamp > 9_999_999_999 {
        timestamp / 1_000
    } else {
        timestamp
    };

    DateTime::<Utc>::from_timestamp(seconds, 0)
        .map(|date| date.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| seconds.to_string())
}

fn to_string_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_settings_loads_defaults_when_file_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = app_settings_path(dir.path());

        let settings = load_app_settings_from_path(&path).unwrap();

        assert_eq!(settings, AppSettings::default());
        assert!(!path.exists());
    }

    #[test]
    fn app_settings_saves_then_loads() {
        let dir = tempfile::tempdir().unwrap();
        let path = app_settings_path(dir.path());
        let settings = AppSettings {
            addons_dir_override: Some("D:\\ESO\\AddOns".to_owned()),
            backup_dir_override: Some("D:\\ESO\\Backups".to_owned()),
            download_dir: Some("D:\\ESO\\Downloads".to_owned()),
            keep_downloads_default: true,
            include_unknown_updates_default: true,
        };

        save_app_settings_to_path(&path, &settings).unwrap();
        let loaded = load_app_settings_from_path(&path).unwrap();

        assert_eq!(loaded, settings);
    }

    #[test]
    fn app_settings_reset_persists_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = app_settings_path(dir.path());
        let settings = AppSettings {
            addons_dir_override: Some("D:\\ESO\\AddOns".to_owned()),
            backup_dir_override: Some("D:\\ESO\\Backups".to_owned()),
            download_dir: Some("D:\\ESO\\Downloads".to_owned()),
            keep_downloads_default: true,
            include_unknown_updates_default: true,
        };
        save_app_settings_to_path(&path, &settings).unwrap();

        let reset = reset_app_settings_at_path(&path).unwrap();
        let loaded = load_app_settings_from_path(&path).unwrap();

        assert_eq!(reset, AppSettings::default());
        assert_eq!(loaded, AppSettings::default());
    }

    #[test]
    fn app_settings_empty_strings_are_saved_as_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = app_settings_path(dir.path());
        let settings = app_settings_from_input(AppSettingsInput {
            addons_dir_override: Some("".to_owned()),
            backup_dir_override: Some("   ".to_owned()),
            download_dir: Some("\t".to_owned()),
            keep_downloads_default: None,
            include_unknown_updates_default: None,
        });

        save_app_settings_to_path(&path, &settings).unwrap();
        let loaded = load_app_settings_from_path(&path).unwrap();

        assert_eq!(settings, AppSettings::default());
        assert_eq!(loaded, AppSettings::default());
    }

    #[test]
    fn app_settings_do_not_persist_force_or_reinstall_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = app_settings_path(dir.path());
        let settings = AppSettings {
            addons_dir_override: Some("D:\\ESO\\AddOns".to_owned()),
            backup_dir_override: None,
            download_dir: None,
            keep_downloads_default: true,
            include_unknown_updates_default: true,
        };

        save_app_settings_to_path(&path, &settings).unwrap();
        let content = fs::read_to_string(&path).unwrap();

        assert!(!content.contains("force"));
        assert!(!content.contains("reinstall"));
    }

    #[test]
    fn first_run_import_records_matched_addons_as_current_baseline() {
        let dir = tempfile::tempdir().unwrap();
        let addons_dir = dir.path().join("AddOns");
        fs::create_dir_all(&addons_dir).unwrap();
        let matches = vec![MatchResult {
            local: test_local_addon("ExampleAddon", true, false),
            status: match_remote::MatchStatus::PossibleUpdate,
            remote: Some(test_remote_candidate("42", "2.0.0")),
            candidates: Vec::new(),
            debug_candidates: Vec::new(),
        }];

        let result = import_existing_matches_as_current(&addons_dir, &matches).unwrap();
        let metadata = load_installed_metadata(&addons_dir);
        let imported = metadata.addons.get("ExampleAddon").unwrap();

        assert_eq!(result.detected_addons, 1);
        assert_eq!(result.imported, 1);
        assert_eq!(imported.remote_uid.as_deref(), Some("42"));
        assert_eq!(imported.remote_version.as_deref(), Some("2.0.0"));
        assert_eq!(imported.source, "first-run-import");
        assert!(metadata.first_run_baseline_complete);
    }

    #[test]
    fn first_run_import_records_matched_addons_without_remote_version() {
        let dir = tempfile::tempdir().unwrap();
        let addons_dir = dir.path().join("AddOns");
        fs::create_dir_all(&addons_dir).unwrap();
        let mut remote = test_remote_candidate("42", "2.0.0");
        remote.version = None;
        let matches = vec![MatchResult {
            local: test_local_addon("ExampleAddon", true, false),
            status: match_remote::MatchStatus::UnknownUpdate,
            remote: Some(remote),
            candidates: Vec::new(),
            debug_candidates: Vec::new(),
        }];

        let result = import_existing_matches_as_current(&addons_dir, &matches).unwrap();
        let metadata = load_installed_metadata(&addons_dir);
        let imported = metadata.addons.get("ExampleAddon").unwrap();

        assert_eq!(result.detected_addons, 1);
        assert_eq!(result.imported, 1);
        assert_eq!(result.skipped_missing_remote_version, 0);
        assert_eq!(imported.remote_uid.as_deref(), Some("42"));
        assert_eq!(imported.remote_version, None);
        assert_eq!(imported.remote_date, Some(1_700_000_000));
        assert!(metadata.first_run_baseline_complete);
    }

    #[test]
    fn first_run_import_records_unmatched_addons_as_current_baseline() {
        let dir = tempfile::tempdir().unwrap();
        let addons_dir = dir.path().join("AddOns");
        fs::create_dir_all(&addons_dir).unwrap();
        let matches = vec![
            MatchResult {
                local: test_local_addon("UnmatchedAddon", true, false),
                status: match_remote::MatchStatus::NoMatch,
                remote: None,
                candidates: Vec::new(),
                debug_candidates: Vec::new(),
            },
            MatchResult {
                local: test_local_addon("LibraryAddon", true, true),
                status: match_remote::MatchStatus::Library,
                remote: None,
                candidates: Vec::new(),
                debug_candidates: Vec::new(),
            },
        ];

        let result = import_existing_matches_as_current(&addons_dir, &matches).unwrap();
        let metadata = load_installed_metadata(&addons_dir);

        assert_eq!(result.detected_addons, 2);
        assert_eq!(result.imported, 2);
        assert_eq!(result.skipped_no_match, 0);
        assert_eq!(result.skipped_libraries, 0);
        assert!(metadata.first_run_baseline_complete);

        for result in &matches {
            let imported = metadata.addons.get(&result.local.folder_name).unwrap();
            let confidence = update_confidence_for_match(result, &metadata);

            assert_eq!(imported.remote_uid, None);
            assert_eq!(imported.source, "first-run-import");
            assert_eq!(confidence.confidence, "current");
        }
    }

    #[test]
    fn incomplete_first_run_baseline_is_repaired_on_refresh() {
        let dir = tempfile::tempdir().unwrap();
        let addons_dir = dir.path().join("AddOns");
        fs::create_dir_all(&addons_dir).unwrap();
        let matches = vec![
            MatchResult {
                local: test_local_addon("AlreadyImported", true, false),
                status: match_remote::MatchStatus::NoMatch,
                remote: None,
                candidates: Vec::new(),
                debug_candidates: Vec::new(),
            },
            MatchResult {
                local: test_local_addon("MissingFromOldImport", true, false),
                status: match_remote::MatchStatus::NoMatch,
                remote: None,
                candidates: Vec::new(),
                debug_candidates: Vec::new(),
            },
        ];
        let mut metadata = InstalledMetadata::default();
        metadata.addons.insert(
            "AlreadyImported".to_owned(),
            first_run_metadata_for_match(&matches[0], "1"),
        );
        save_installed_metadata(&addons_dir, &metadata).unwrap();

        let repaired = ensure_first_run_baseline_complete(&addons_dir, &matches).unwrap();

        assert!(repaired.first_run_baseline_complete);
        assert!(repaired.addons.contains_key("AlreadyImported"));
        assert!(repaired.addons.contains_key("MissingFromOldImport"));
        assert_eq!(
            update_confidence_for_match(&matches[1], &repaired).confidence,
            "current"
        );
    }

    #[test]
    fn confidence_uses_remote_date_when_version_is_unavailable() {
        let current =
            confidence_from_versions(None, None, Some(1_700_000_000), Some(1_700_000_000), true);
        let updated =
            confidence_from_versions(None, None, Some(1_700_000_000), Some(1_800_000_000), true);

        assert_eq!(current.confidence, "current");
        assert_eq!(updated.confidence, "reliable-update");
    }

    #[test]
    fn confidence_keeps_managed_baseline_current_without_version_or_date() {
        let current = confidence_from_versions(None, None, None, None, true);

        assert_eq!(current.confidence, "current");
    }

    fn test_local_addon(folder_name: &str, valid_manifest: bool, is_library: bool) -> LocalAddon {
        LocalAddon {
            folder_name: folder_name.to_owned(),
            folder_path: PathBuf::from(folder_name),
            manifest_path: None,
            title: Some(folder_name.to_owned()),
            addon_version: Some("1.0.0".to_owned()),
            version: None,
            api_versions: Vec::new(),
            depends_on: Vec::new(),
            optional_depends_on: Vec::new(),
            is_library: Some(is_library),
            author: None,
            description: None,
            valid_manifest,
        }
    }

    fn test_remote_candidate(uid: &str, version: &str) -> RemoteCandidate {
        RemoteCandidate {
            uid: Some(uid.to_owned()),
            name: Some("Example Addon".to_owned()),
            author_name: None,
            version: Some(version.to_owned()),
            updated: Some(1_700_000_000),
            file_info_url: None,
            summary: None,
            directories: vec!["ExampleAddon".to_owned()],
            category_id: None,
            category_name: None,
            downloads: None,
            monthly_downloads: None,
            tier: 1,
            score: 100,
            reason: "test".to_owned(),
        }
    }
}
