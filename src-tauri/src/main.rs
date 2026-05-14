#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

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
use eso_addon_manager::local::{self, AddonPathCandidate, LocalAddon};
use serde::Deserialize;
use serde::Serialize;
use std::fs;
use std::path::Path;
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
    remote: Option<RemoteCandidateDto>,
    candidates: Vec<RemoteCandidateDto>,
    debug_candidates: Vec<RemoteCandidateDto>,
}

#[derive(Debug, Serialize)]
struct RemoteCandidateDto {
    uid: Option<String>,
    name: Option<String>,
    version: Option<String>,
    updated: Option<i64>,
    updated_display: Option<String>,
    tier: u8,
    score: usize,
    reason: String,
}

#[derive(Debug, Serialize)]
struct PlanUpdatesResponse {
    addons_dir: String,
    remote_addons_loaded: usize,
    include_unknown: bool,
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
    action: String,
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
    action: String,
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

#[derive(Debug, Deserialize, Serialize, Clone, Default, PartialEq, Eq)]
#[serde(default)]
struct AppSettings {
    addons_dir_override: Option<String>,
    backup_dir_override: Option<String>,
    download_dir: Option<String>,
    keep_downloads_default: bool,
    include_unknown_updates_default: bool,
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

    Ok(CheckAddonsResponse {
        addons_dir: path_string(&addons_dir),
        remote_addons_loaded: remote_addons.len(),
        matches: matches.iter().map(match_result_dto).collect(),
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

    Ok(PlanUpdatesResponse {
        addons_dir: path_string(&addons_dir),
        remote_addons_loaded: remote_addons.len(),
        include_unknown,
        actions: plan.actions.iter().map(planned_action_dto).collect(),
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
    let (addons_dir, remote_addons_loaded, plan) =
        build_update_all_plan_for_ui(path.as_deref(), include_unknown, limit).await?;

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
    let (addons_dir, remote_addons_loaded, plan) =
        build_update_all_plan_for_ui(path.as_deref(), include_unknown, limit).await?;
    let client = ApiClient::new().map_err(to_string_error)?;
    let mut results = Vec::new();

    for target in &plan.targets {
        let remote_uid = target
            .remote_uid
            .as_deref()
            .ok_or_else(|| format!("planned addon {} has no clean remote UID", target.local_folder))?;
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

        results.push(UpdateAllResultDto {
            target: planned_action_dto(target),
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
        .invoke_handler(tauri::generate_handler![
            get_app_settings,
            save_app_settings,
            reset_app_settings,
            path_exists,
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
    }
}

fn match_result_dto(result: &MatchResult) -> MatchResultDto {
    MatchResultDto {
        local: local_addon_dto(&result.local),
        status: result.status.as_str().to_owned(),
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
        version: candidate.version.clone(),
        updated: candidate.updated,
        updated_display: candidate.updated.map(format_mmoui_date),
        tier: candidate.tier,
        score: candidate.score,
        reason: candidate.reason.clone(),
    }
}

fn planned_action_dto(action: &PlannedAddonAction) -> PlannedActionDto {
    PlannedActionDto {
        local_folder: action.local_folder.clone(),
        remote_name: action.remote_name.clone(),
        remote_uid: action.remote_uid.clone(),
        local_version: action.local_version.clone(),
        remote_version: action.remote_version.clone(),
        action: action.kind.as_str().to_owned(),
    }
}

fn update_all_action_dto(
    action: &PlannedAddonAction,
    targets: &[PlannedAddonAction],
) -> UpdateAllActionDto {
    UpdateAllActionDto {
        local_folder: action.local_folder.clone(),
        remote_name: action.remote_name.clone(),
        remote_uid: action.remote_uid.clone(),
        local_version: action.local_version.clone(),
        remote_version: action.remote_version.clone(),
        action: action.kind.as_str().to_owned(),
        update_all_action: update_all_action_label(action, targets).to_owned(),
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
            .map(|action| update_all_action_dto(action, &plan.targets))
            .collect(),
        targets: plan.targets.iter().map(planned_action_dto).collect(),
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
            .map(|action| update_all_action_dto(action, &plan.targets))
            .collect(),
        targets: plan.targets.iter().map(planned_action_dto).collect(),
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

    let plan = update_all::build_update_all_plan(&matches, include_unknown);
    Ok((addons_dir, remote_addons.len(), plan))
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
}
