#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use eso_addon_manager::api::models::{AddonDetails, AddonSummary, CategorySummary};
use eso_addon_manager::api::ApiClient;
use eso_addon_manager::cache::{HttpCache, ResourceKind};
use eso_addon_manager::install::apply::{
    self, InstallActionPerformed, InstallResult, InstalledItem,
};
use eso_addon_manager::install::backup::{self, BackupInspection, BackupResult, RestoreResult};
use eso_addon_manager::install::dependencies::{self, DependencyPlan, DependencyStatus};
use eso_addon_manager::install::dependency_graph::{
    DependencyManifestSource, DEFAULT_MAX_DEPENDENCY_DEPTH,
};
use eso_addon_manager::install::plan::{self, InstallPlan, InstallPlanAction, InstallPlanItem};
use eso_addon_manager::install::remote;
use eso_addon_manager::install::remove::{self, ClearSavedVariablesResult, RemoveAddonResult};
use eso_addon_manager::install::update;
use eso_addon_manager::install::update_all;
use eso_addon_manager::install::zip_safety;
use eso_addon_manager::local::match_remote::{self, MatchResult, RemoteCandidate};
use eso_addon_manager::local::metadata::{
    self as manager_metadata, InstalledAddonMetadata, InstalledMetadata,
};
use eso_addon_manager::local::update_plan::{self, PlannedAddonAction};
use eso_addon_manager::local::version::{compare_versions, VersionComparison};
use eso_addon_manager::local::{self, AddonPathCandidate, LocalAddon};
use serde::Deserialize;
use serde::Serialize;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{Emitter, Manager};
use tauri_plugin_opener::OpenerExt;

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

#[derive(Debug, Clone, Serialize)]
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
    saved_variables: Vec<String>,
    saved_variables_per_character: Vec<String>,
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
struct BrowseRemoteAddonsResponse {
    mode: String,
    query: String,
    category_id: Option<String>,
    limit: usize,
    categories: Vec<RemoteCategoryDto>,
    category_warning: Option<String>,
    local_warning: Option<String>,
    cache_warning: Option<String>,
    results: Vec<AddonSummaryDto>,
}

#[derive(Debug, Clone, Serialize)]
struct RemoteCategoryDto {
    id: String,
    name: String,
    parent_id: Option<String>,
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
    is_library: bool,
    image_urls: Vec<String>,
    thumbnail_urls: Vec<String>,
    installed: bool,
    installed_local: Option<LocalAddonDto>,
    installed_match: Option<MatchResultDto>,
}

#[derive(Debug, Serialize)]
struct RemoteAddonDetailsWithLocalStateResponse {
    details: AddonDetailsDto,
    installed: bool,
    local: Option<LocalAddonDto>,
    match_result: Option<MatchResultDto>,
    local_warning: Option<String>,
    cache_warning: Option<String>,
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
    is_library: bool,
    image_urls: Vec<String>,
    thumbnail_urls: Vec<String>,
}

#[derive(Debug, Serialize)]
struct CheckAddonsResponse {
    addons_dir: String,
    remote_addons_loaded: usize,
    matches: Vec<MatchResultDto>,
    cache_warning: Option<String>,
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
    is_library: bool,
    image_urls: Vec<String>,
    thumbnail_urls: Vec<String>,
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
    cache_warning: Option<String>,
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
    dependency_plan: DependencyPlanDto,
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
    dependency_plan: DependencyPlanDto,
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
    dependency_plan: Option<DependencyPlanDto>,
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
    dependency_plan: Option<DependencyPlanDto>,
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
struct RemoveInstalledAddonResponse {
    removed_addon: bool,
    removed_saved_variables: bool,
    saved_variables_deleted_count: usize,
    saved_variables_deleted_files: Vec<String>,
    saved_variables_missing_files: Vec<String>,
    addon_folder: String,
    original_path: String,
    message: String,
}

#[derive(Debug, Serialize)]
struct ClearSavedVariablesResponse {
    addon_folder: String,
    saved_variables_dir: String,
    deleted_count: usize,
    deleted_files: Vec<String>,
    missing_files: Vec<String>,
    status: String,
    message: String,
}

#[derive(Debug, Serialize)]
struct BackupResultResponse {
    backup_zip_path: String,
    backup_created: bool,
    included_saved_variables: bool,
    file_count: u64,
    total_bytes: u64,
    skipped_files_count: usize,
    skipped_files: Vec<SkippedBackupFileResponse>,
    warnings: Vec<String>,
    backup_status: String,
}

#[derive(Debug, Serialize)]
struct SkippedBackupFileResponse {
    relative_path: String,
    reason: String,
}

#[derive(Debug, Serialize)]
struct BackupInspectionResponse {
    valid: bool,
    backup_name: String,
    created_at: Option<String>,
    contains_addons: bool,
    contains_saved_variables: bool,
    file_count: u64,
    total_bytes: u64,
    warnings: Vec<String>,
    target_addons_folder: String,
    target_saved_variables_folder: String,
}

#[derive(Debug, Serialize)]
struct RestoreResultResponse {
    restored_addons: bool,
    restored_saved_variables: bool,
    message: String,
    rollback_path: Option<String>,
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
struct DependencyPlanDto {
    main_addon: DependencyPlanMainDto,
    required_dependencies: Vec<DependencyPlanEntryDto>,
    optional_dependencies: Vec<DependencyPlanEntryDto>,
    install_items: Vec<DependencyInstallItemDto>,
    install_order: Vec<String>,
}

#[derive(Debug, Serialize)]
struct DependencyPlanMainDto {
    uid: String,
    name: Option<String>,
}

#[derive(Debug, Serialize)]
struct DependencyPlanEntryDto {
    name: String,
    constraint: Option<String>,
    raw: String,
    required: bool,
    relation: String,
    depth: usize,
    parent: Option<String>,
    status: String,
    remote_uid: Option<String>,
    remote_name: Option<String>,
    remote_version: Option<String>,
    installed_folder: Option<String>,
    installed_title: Option<String>,
    installed_version: Option<String>,
    bundled_folder: Option<String>,
}

#[derive(Debug, Serialize)]
struct InstalledAddonDependenciesResponse {
    required_dependencies: Vec<InstalledDependencyStatusDto>,
    optional_dependencies: Vec<InstalledDependencyStatusDto>,
    warning: Option<String>,
}

#[derive(Debug, Serialize)]
struct InstalledDependencyStatusDto {
    name: String,
    raw: String,
    constraint: Option<String>,
    required: bool,
    relation: String,
    depth: usize,
    parent: Option<String>,
    installed: bool,
    installed_folder: Option<String>,
    installed_title: Option<String>,
    installed_version: Option<String>,
    remote_uid: Option<String>,
    remote_name: Option<String>,
    remote_version: Option<String>,
    status: String,
}

#[derive(Debug, Serialize)]
struct DependencyInstallItemDto {
    role: String,
    name: String,
    remote_uid: Option<String>,
    remote_name: Option<String>,
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
    failure: Option<UpdateAllFailureDto>,
    results: Vec<UpdateAllResultDto>,
}

#[derive(Debug, Serialize)]
struct UpdateAllFailureDto {
    local_folder: String,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
struct UpdateAllProgressDto {
    index: usize,
    total: usize,
    local_folder: String,
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
    dependency_plan: DependencyPlanDto,
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
    hide_libraries_in_search: bool,
    hide_libraries_in_installed: bool,
}

#[derive(Debug, Serialize)]
struct AppStartupInfo {
    settings: AppSettings,
    settings_exists: bool,
    detected_addons_dir: Option<String>,
}

#[derive(Debug, Serialize)]
struct HttpCacheStatsResponse {
    cache_dir: String,
    entry_count: usize,
    byte_size: u64,
    size_display: String,
}

#[derive(Debug, Serialize)]
struct CachedImageResponse {
    url: String,
    data_url: String,
    content_type: String,
    from_cache: bool,
    stale: bool,
    cache_warning: Option<String>,
}

struct PreparedRemoteInstall {
    details: AddonDetails,
    main_plan: InstallPlan,
    dependency_plan: DependencyPlan,
    dependency_installs: Vec<PreparedDependencyInstall>,
}

struct PreparedDependencyInstall {
    details: AddonDetails,
    remote_uid: String,
    plan: InstallPlan,
}

const MAX_RECURSIVE_DEPENDENCY_PACKAGES: usize = 64;
const UPDATE_ALL_PROGRESS_EVENT: &str = "scribe-update-all-progress";

struct RemoteLocalState {
    local_addons: Vec<LocalAddon>,
    metadata: InstalledMetadata,
    matches: Vec<MatchResult>,
}

#[derive(Debug, Deserialize)]
struct AppSettingsInput {
    addons_dir_override: Option<String>,
    backup_dir_override: Option<String>,
    download_dir: Option<String>,
    keep_downloads_default: Option<bool>,
    include_unknown_updates_default: Option<bool>,
    hide_libraries_in_search: Option<bool>,
    hide_libraries_in_installed: Option<bool>,
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
async fn get_installed_addon_dependencies(
    folder_name: String,
    path: Option<String>,
) -> Result<InstalledAddonDependenciesResponse, String> {
    let addons_dir = resolve_addons_dir(path.as_deref()).map_err(to_string_error)?;
    let local_addons = local::scan_addons_dir(&addons_dir).map_err(to_string_error)?;
    let local_addon = local_addons
        .iter()
        .find(|addon| addon.folder_name.eq_ignore_ascii_case(folder_name.as_str()))
        .ok_or_else(|| format!("installed addon folder not found: {folder_name}"))?;
    let installed_metadata = load_installed_metadata(&addons_dir);
    let installed_remotes = installed_remote_addons(&installed_metadata);
    let (remote_addons, remote_warning) = load_dependency_remote_addons().await;
    let mut warnings = Vec::new();

    if !local_addon.valid_manifest {
        warnings.push("No valid addon manifest was found for this folder.".to_owned());
    }
    if let Some(warning) = remote_warning {
        warnings.push(warning);
    }

    let report = dependencies::resolve_manifest_dependencies(
        &local_addon.depends_on,
        &local_addon.optional_depends_on,
        &local_addons,
        remote_addons.as_deref(),
        &installed_remotes,
    );

    Ok(installed_addon_dependencies_response(
        &report,
        join_warning_messages(warnings),
    ))
}

#[tauri::command]
async fn remove_installed_addon(
    folder_name: String,
    path: Option<String>,
    remove_saved_variables: bool,
) -> Result<RemoveInstalledAddonResponse, String> {
    let addons_dir = resolve_addons_dir(path.as_deref()).map_err(to_string_error)?;
    let result = remove::remove_installed_addon(&addons_dir, &folder_name, remove_saved_variables)
        .map_err(to_string_error)?;
    remove_installed_metadata(&addons_dir, &result.addon_folder).map_err(to_string_error)?;

    Ok(remove_installed_addon_response(&result))
}

#[tauri::command]
async fn clear_saved_variables(
    folder_name: String,
    path: Option<String>,
) -> Result<ClearSavedVariablesResponse, String> {
    let addons_dir = resolve_addons_dir(path.as_deref()).map_err(to_string_error)?;
    let result =
        remove::clear_saved_variables(&addons_dir, &folder_name).map_err(to_string_error)?;

    Ok(clear_saved_variables_response(&result))
}

#[tauri::command]
async fn create_compressed_backup(
    addons_path: Option<String>,
    backup_dir: String,
    include_saved_variables: bool,
) -> Result<BackupResultResponse, String> {
    let addons_dir = resolve_addons_dir(addons_path.as_deref()).map_err(to_string_error)?;
    let backup_dir = required_path(&backup_dir, "backup folder").map_err(to_string_error)?;
    let result = backup::create_compressed_backup_with_app_version(
        &addons_dir,
        &backup_dir,
        include_saved_variables,
        Some(env!("CARGO_PKG_VERSION")),
    )
    .map_err(to_string_error)?;

    Ok(backup_result_response(&result))
}

#[tauri::command]
async fn inspect_backup_zip(
    zip_path: String,
    addons_path: Option<String>,
) -> Result<BackupInspectionResponse, String> {
    let addons_dir = resolve_addons_dir(addons_path.as_deref()).map_err(to_string_error)?;
    let zip_path = required_path(&zip_path, "backup ZIP").map_err(to_string_error)?;
    let target_saved_variables = saved_variables_dir_for_addons(&addons_dir);

    match backup::inspect_backup_zip(&zip_path) {
        Ok(inspection) => Ok(backup_inspection_response(
            &inspection,
            &addons_dir,
            &target_saved_variables,
        )),
        Err(error) => Ok(invalid_backup_inspection_response(
            &zip_path,
            &addons_dir,
            &target_saved_variables,
            error.to_string(),
        )),
    }
}

#[tauri::command]
async fn restore_backup_zip(
    zip_path: String,
    addons_path: Option<String>,
    restore_addons: bool,
    restore_saved_variables: bool,
) -> Result<RestoreResultResponse, String> {
    let addons_dir = resolve_addons_dir(addons_path.as_deref()).map_err(to_string_error)?;
    let zip_path = required_path(&zip_path, "backup ZIP").map_err(to_string_error)?;
    let result = backup::restore_backup_zip(
        &zip_path,
        &addons_dir,
        restore_addons,
        restore_saved_variables,
    )
    .map_err(to_string_error)?;

    Ok(restore_result_response(&result))
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
fn get_http_cache_stats() -> Result<HttpCacheStatsResponse, String> {
    let cache = HttpCache::new().map_err(to_string_error)?;
    let stats = cache.stats().map_err(to_string_error)?;
    Ok(HttpCacheStatsResponse {
        cache_dir: path_string(&stats.cache_dir),
        entry_count: stats.entry_count,
        byte_size: stats.byte_size,
        size_display: format_bytes(stats.byte_size),
    })
}

#[tauri::command]
fn clear_http_cache() -> Result<HttpCacheStatsResponse, String> {
    let cache = HttpCache::new().map_err(to_string_error)?;
    cache.clear().map_err(to_string_error)?;
    let stats = cache.stats().map_err(to_string_error)?;
    Ok(HttpCacheStatsResponse {
        cache_dir: path_string(&stats.cache_dir),
        entry_count: stats.entry_count,
        byte_size: stats.byte_size,
        size_display: format_bytes(stats.byte_size),
    })
}

#[tauri::command]
async fn cache_remote_image(url: String) -> Result<CachedImageResponse, String> {
    let client = ApiClient::new().map_err(to_string_error)?;
    let response = client
        .cached_bytes(&url, ResourceKind::Image)
        .await
        .map_err(to_string_error)?;
    let content_type = image_content_type(response.content_type.as_deref(), &url)
        .ok_or_else(|| "remote resource is not a supported image".to_owned())?;
    let data_url = format!(
        "data:{};base64,{}",
        content_type,
        base64_encode(&response.bytes)
    );

    Ok(CachedImageResponse {
        url,
        data_url,
        content_type,
        from_cache: response.from_cache,
        stale: response.stale,
        cache_warning: client.cache_warning_message(),
    })
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
    let metadata = load_installed_metadata(&addons_dir);
    let mut matches =
        match_remote::match_installed_addons_with_metadata(&local_addons, &remote_addons, &metadata);
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
async fn browse_remote_addons(
    mode: String,
    category_id: Option<String>,
    query: Option<String>,
    limit: Option<usize>,
    path: Option<String>,
    refresh: Option<bool>,
    hide_libraries: Option<bool>,
) -> Result<BrowseRemoteAddonsResponse, String> {
    let mode = BrowseMode::from_str(&mode).map_err(to_string_error)?;
    let limit = limit.unwrap_or(25).clamp(1, 100);
    let query = query.unwrap_or_default();
    let result_limit = if query.trim().is_empty() {
        Some(limit)
    } else {
        None
    };
    let category_id = normalize_optional_filter(category_id);
    let refresh = refresh.unwrap_or(false);
    let client = ApiClient::new().map_err(to_string_error)?;
    let addons = if refresh {
        client
            .eso_file_list_refresh()
            .await
            .map_err(to_string_error)?
    } else {
        client.eso_file_list().await.map_err(to_string_error)?
    };
    let (categories, category_warning) = match if refresh {
        client.eso_category_list_refresh().await
    } else {
        client.eso_category_list().await
    } {
        Ok(categories) => (category_dtos(&categories), None),
        Err(error) => (
            Vec::new(),
            Some(format!("Category list could not be loaded. {error}")),
        ),
    };
    let local_state = remote_local_state(path.as_deref(), &addons);
    let local_warning = local_state
        .as_ref()
        .err()
        .map(|error| format!("Installed addons could not be checked. {error}"));
    let local_state = local_state.ok();
    let results = browse_remote_results(
        &addons,
        &categories,
        local_state.as_ref(),
        mode,
        category_id.as_deref(),
        query.as_str(),
        result_limit,
        hide_libraries.unwrap_or(false),
    );

    Ok(BrowseRemoteAddonsResponse {
        mode: mode.as_str().to_owned(),
        query: query.trim().to_owned(),
        category_id,
        limit,
        categories,
        category_warning,
        local_warning,
        cache_warning: client.cache_warning_message(),
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
async fn get_remote_addon_details_with_local_state(
    addon_id: String,
    path: Option<String>,
) -> Result<RemoteAddonDetailsWithLocalStateResponse, String> {
    let client = ApiClient::new().map_err(to_string_error)?;
    let details = client
        .eso_file_details(&addon_id)
        .await
        .map_err(to_string_error)?;
    let remote_addons = client.eso_file_list().await.map_err(to_string_error)?;
    let local_state = remote_local_state(path.as_deref(), &remote_addons);
    let local_warning = local_state
        .as_ref()
        .err()
        .map(|error| format!("Installed addons could not be checked. {error}"));
    let match_result = local_state.ok().and_then(|state| {
        remote_addon_match(addon_id.as_str(), &remote_addons, &state)
            .map(|result| match_result_dto(&result, &state.metadata))
    });
    let local = match_result.as_ref().map(|result| result.local.clone());

    Ok(RemoteAddonDetailsWithLocalStateResponse {
        details: addon_details_dto(&details),
        installed: match_result.is_some(),
        local,
        match_result,
        local_warning,
        cache_warning: client.cache_warning_message(),
    })
}

#[tauri::command]
async fn open_external_url(app: tauri::AppHandle, url: String) -> Result<(), String> {
    let url = allowed_external_url(&url).map_err(to_string_error)?;
    app.opener()
        .open_url(url.as_str(), None::<&str>)
        .map_err(to_string_error)
}

#[tauri::command]
async fn open_addon_folder(
    app: tauri::AppHandle,
    folder_name: String,
    path: Option<String>,
) -> Result<(), String> {
    let addons_dir = resolve_addons_dir(path.as_deref()).map_err(to_string_error)?;
    let target =
        resolve_addon_folder_for_open(&addons_dir, &folder_name).map_err(to_string_error)?;
    app.opener()
        .open_path(path_string(&target), None::<&str>)
        .map_err(to_string_error)
}

#[tauri::command]
async fn open_folder(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let target = resolve_folder_for_open(&path).map_err(to_string_error)?;
    app.opener()
        .open_path(path_string(&target), None::<&str>)
        .map_err(to_string_error)
}

#[tauri::command]
async fn open_path_location(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let target = resolve_path_location_for_open(&path).map_err(to_string_error)?;
    app.opener()
        .open_path(path_string(&target), None::<&str>)
        .map_err(to_string_error)
}

#[tauri::command]
async fn check_addons(path: Option<String>) -> Result<CheckAddonsResponse, String> {
    let addons_dir = resolve_addons_dir(path.as_deref()).map_err(to_string_error)?;
    let local_addons = local::scan_addons_dir(&addons_dir).map_err(to_string_error)?;
    let client = ApiClient::new().map_err(to_string_error)?;
    let remote_addons = client.eso_file_list().await.map_err(to_string_error)?;
    let metadata = load_installed_metadata(&addons_dir);
    let mut matches =
        match_remote::match_installed_addons_with_metadata(&local_addons, &remote_addons, &metadata);
    matches.sort_by_key(|result| result.local.folder_name.to_lowercase());
    let metadata =
        ensure_first_run_baseline_complete(&addons_dir, &matches).map_err(to_string_error)?;

    Ok(CheckAddonsResponse {
        addons_dir: path_string(&addons_dir),
        remote_addons_loaded: remote_addons.len(),
        matches: matches
            .iter()
            .map(|result| match_result_dto(result, &metadata))
            .collect(),
        cache_warning: client.cache_warning_message(),
    })
}

#[tauri::command]
async fn plan_updates(
    path: Option<String>,
    include_unknown: Option<bool>,
    refresh: Option<bool>,
) -> Result<PlanUpdatesResponse, String> {
    let include_unknown = include_unknown.unwrap_or(false);
    let refresh = refresh.unwrap_or(false);
    let addons_dir = resolve_addons_dir(path.as_deref()).map_err(to_string_error)?;
    let local_addons = local::scan_addons_dir(&addons_dir).map_err(to_string_error)?;
    let client = ApiClient::new().map_err(to_string_error)?;
    let remote_addons = if refresh {
        client
            .eso_file_list_refresh()
            .await
            .map_err(to_string_error)?
    } else {
        client.eso_file_list().await.map_err(to_string_error)?
    };
    let metadata = load_installed_metadata(&addons_dir);
    let mut matches =
        match_remote::match_installed_addons_with_metadata(&local_addons, &remote_addons, &metadata);
    matches.sort_by_key(|result| result.local.folder_name.to_lowercase());
    let plan = update_plan::build_update_plan(&matches, include_unknown);
    let summary = plan.summary();
    let metadata =
        ensure_first_run_baseline_complete(&addons_dir, &matches).map_err(to_string_error)?;

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
        cache_warning: client.cache_warning_message(),
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
    app: tauri::AppHandle,
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
    let keep_download = keep_download.unwrap_or(false);
    let (addons_dir, remote_addons_loaded, mut plan) =
        build_update_all_plan_for_ui(path.as_deref(), include_unknown, limit).await?;
    apply_reliable_update_filter(&addons_dir, &mut plan);
    let client = ApiClient::new().map_err(to_string_error)?;
    let mut results = Vec::new();

    for (index, target) in plan.targets.iter().enumerate() {
        emit_update_all_progress(&app, index + 1, plan.targets.len(), &target.local_folder);
        match apply_update_all_target(
            &client,
            &addons_dir,
            target,
            backup_dir.as_deref(),
            keep_download,
            download_dir.as_deref(),
        )
        .await
        {
            Ok(result) => results.push(result),
            Err(message) => {
                return Ok(apply_update_all_response(
                    &addons_dir,
                    remote_addons_loaded,
                    include_unknown,
                    limit,
                    &plan,
                    Some(UpdateAllFailureDto {
                        local_folder: target.local_folder.clone(),
                        message,
                    }),
                    results,
                ));
            }
        }
    }

    Ok(apply_update_all_response(
        &addons_dir,
        remote_addons_loaded,
        include_unknown,
        limit,
        &plan,
        None,
        results,
    ))
}

async fn apply_update_all_target(
    client: &ApiClient,
    addons_dir: &Path,
    target: &PlannedAddonAction,
    backup_dir: Option<&Path>,
    keep_download: bool,
    download_dir: Option<&Path>,
) -> Result<UpdateAllResultDto, String> {
    let remote_uid = target.remote_uid.as_deref().ok_or_else(|| {
        format!(
            "planned addon {} has no clean remote UID",
            target.local_folder
        )
    })?;
    let prepared = prepare_remote_install_plan(client, remote_uid, addons_dir)
        .await
        .map_err(|error| {
            format!(
                "failed to plan update for {}: {}",
                target.local_folder, error
            )
        })?;
    validate_single_update_plan(&prepared.main_plan, &target.local_folder).map_err(|error| {
        format!(
            "failed to plan update for {}: {}",
            target.local_folder, error
        )
    })?;

    if keep_download {
        keep_remote_download(
            client,
            remote_uid,
            &prepared.details,
            download_dir,
        )
        .await
        .map_err(|error| format!("failed to update {}: {}", target.local_folder, error))?;
    }

    let result = apply_prepared_remote_install(
        addons_dir,
        &prepared,
        remote_uid,
        backup_dir,
        manager_metadata::INSTALLED_BY_REMOTE_UPDATE,
    )
    .map_err(|error| format!("failed to update {}: {}", target.local_folder, error))?;

    Ok(UpdateAllResultDto {
        target: planned_action_dto(target, &load_installed_metadata(addons_dir)),
        remote_details: addon_details_dto(&prepared.details),
        plan: install_plan_dto(&prepared.main_plan),
        dependency_plan: dependency_plan_dto(&prepared),
        installed_new: result.installed_new,
        replaced: result.replaced,
        skipped: result.skipped,
        backup_dir: result.backup_dir.as_ref().map(|path| path_string(path)),
        items: result.items.iter().map(installed_item_dto).collect(),
    })
}

fn emit_update_all_progress(
    app: &tauri::AppHandle,
    index: usize,
    total: usize,
    local_folder: &str,
) {
    let _ = app.emit(
        UPDATE_ALL_PROGRESS_EVENT,
        UpdateAllProgressDto {
            index,
            total,
            local_folder: local_folder.to_owned(),
        },
    );
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
        plan: install_plan_dto(&prepared.main_plan),
        dependency_plan: dependency_plan_dto(&prepared),
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

    let result = apply_prepared_remote_install(
        &addons_dir,
        &prepared,
        &addon_id,
        backup_dir.as_deref(),
        manager_metadata::INSTALLED_BY_REMOTE_INSTALL,
    )
    .map_err(to_string_error)?;
    Ok(install_remote_addon_response(
        &prepared.details,
        &addons_dir,
        &prepared,
        &result,
    ))
}

#[tauri::command]
async fn install_remote_addon_new_only(
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
    validate_new_only_prepared_install(&prepared)?;

    if keep_download.unwrap_or(false) {
        keep_remote_download(
            &client,
            &addon_id,
            &prepared.details,
            download_dir.as_deref(),
        )
        .await?;
    }

    let result = apply_prepared_remote_install(
        &addons_dir,
        &prepared,
        &addon_id,
        backup_dir.as_deref(),
        manager_metadata::INSTALLED_BY_REMOTE_INSTALL,
    )
    .map_err(to_string_error)?;
    Ok(install_remote_addon_response(
        &prepared.details,
        &addons_dir,
        &prepared,
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
    let metadata = load_installed_metadata(&addons_dir);
    let matches =
        match_remote::match_installed_addons_with_metadata(&local_addons, &remote_addons, &metadata);
    let selected = update::resolve_update_request(&matches, &target).map_err(to_string_error)?;
    let decision = update::update_decision(selected, force);

    Ok(single_update_plan_response(
        target,
        selected,
        &decision,
        &addons_dir,
    ))
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
    let metadata = load_installed_metadata(&addons_dir);
    let matches =
        match_remote::match_installed_addons_with_metadata(&local_addons, &remote_addons, &metadata);
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
            dependency_plan: None,
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
    let prepared = prepare_remote_install_plan(&client, remote_uid, &addons_dir)
        .await
        .map_err(|error| {
            format!(
                "failed to plan update for {}: {}",
                selected.local.folder_name, error
            )
        })?;
    validate_single_update_plan(&prepared.main_plan, &selected.local.folder_name)
        .map_err(|error| {
            format!(
                "failed to plan update for {}: {}",
                selected.local.folder_name, error
            )
        })?;

    if keep_download.unwrap_or(false) {
        keep_remote_download(
            &client,
            remote_uid,
            &prepared.details,
            download_dir.as_deref(),
        )
        .await
        .map_err(|error| format!("failed to update {}: {}", selected.local.folder_name, error))?;
    }

    let result = apply_prepared_remote_install(
        &addons_dir,
        &prepared,
        remote_uid,
        backup_dir.as_deref(),
        manager_metadata::INSTALLED_BY_REMOTE_UPDATE,
    )
    .map_err(|error| format!("failed to update {}: {}", selected.local.folder_name, error))?;
    Ok(single_update_apply_response(
        &target,
        selected,
        &decision,
        &prepared.details,
        &addons_dir,
        &prepared,
        &result,
    ))
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_startup_info,
            get_app_settings,
            save_app_settings,
            reset_app_settings,
            get_http_cache_stats,
            clear_http_cache,
            cache_remote_image,
            path_exists,
            import_existing_addons_as_current,
            get_installed_addons,
            get_installed_addon_dependencies,
            remove_installed_addon,
            clear_saved_variables,
            create_compressed_backup,
            inspect_backup_zip,
            restore_backup_zip,
            search_remote_addons,
            browse_remote_addons,
            get_remote_addon_details,
            get_remote_addon_details_with_local_state,
            open_external_url,
            open_addon_folder,
            open_folder,
            open_path_location,
            check_addons,
            plan_updates,
            plan_update_all,
            apply_update_all,
            plan_remote_install,
            install_remote_addon,
            install_remote_addon_new_only,
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
        hide_libraries_in_search: settings.hide_libraries_in_search.unwrap_or(false),
        hide_libraries_in_installed: settings.hide_libraries_in_installed.unwrap_or(false),
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

fn required_path(value: &str, label: &str) -> anyhow::Result<PathBuf> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(anyhow::anyhow!("{label} is required"))
    } else {
        Ok(PathBuf::from(trimmed))
    }
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
        saved_variables: addon.saved_variables.clone(),
        saved_variables_per_character: addon.saved_variables_per_character.clone(),
        is_library: addon.is_library,
        author: addon.author.clone(),
        description: addon.description.clone(),
        valid_manifest: addon.valid_manifest,
    }
}

async fn load_dependency_remote_addons() -> (Option<Vec<AddonSummary>>, Option<String>) {
    let client = match ApiClient::new() {
        Ok(client) => client,
        Err(error) => {
            return (
                None,
                Some(format!("Remote dependency lookup is unavailable. {error}")),
            );
        }
    };

    match client.eso_file_list().await {
        Ok(addons) => {
            let warning = client.cache_warning_message();
            (Some(addons), warning)
        }
        Err(error) => (
            None,
            Some(format!("Remote dependency lookup is unavailable. {error}")),
        ),
    }
}

fn join_warning_messages(warnings: Vec<String>) -> Option<String> {
    let warning = warnings
        .into_iter()
        .map(|warning| warning.trim().to_owned())
        .filter(|warning| !warning.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    if warning.is_empty() {
        None
    } else {
        Some(warning)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BrowseMode {
    MostDownloaded,
    Recent,
}

impl BrowseMode {
    fn from_str(value: &str) -> anyhow::Result<Self> {
        match value.trim() {
            "most_downloaded" => Ok(Self::MostDownloaded),
            "recent" => Ok(Self::Recent),
            other => Err(anyhow::anyhow!(
                "unsupported browse mode '{other}', expected 'most_downloaded' or 'recent'"
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::MostDownloaded => "most_downloaded",
            Self::Recent => "recent",
        }
    }
}

fn browse_remote_results(
    addons: &[AddonSummary],
    categories: &[RemoteCategoryDto],
    local_state: Option<&RemoteLocalState>,
    mode: BrowseMode,
    category_id: Option<&str>,
    query: &str,
    limit: Option<usize>,
    hide_libraries: bool,
) -> Vec<AddonSummaryDto> {
    let query = query.trim();
    let needle = query.trim().to_lowercase();
    let library_category_selected = selected_category_is_libraries(category_id, categories);
    let mut results = addons
        .iter()
        .filter(|addon| {
            addon_matches_category(addon, category_id, categories)
                && (needle.is_empty() || addon.searchable_text().contains(&needle))
                && addon_visible_after_library_filter(
                    addon,
                    local_state,
                    hide_libraries,
                    library_category_selected,
                    query,
                )
        })
        .collect::<Vec<_>>();

    results.sort_by(|left, right| compare_remote_addons(left, right, mode));
    let results = if let Some(limit) = limit {
        results.into_iter().take(limit).collect::<Vec<_>>()
    } else {
        results
    };

    results
        .into_iter()
        .map(|addon| addon_summary_dto_with_local_state(addon, addons, local_state))
        .collect()
}

fn addon_visible_after_library_filter(
    addon: &AddonSummary,
    local_state: Option<&RemoteLocalState>,
    hide_libraries: bool,
    library_category_selected: bool,
    query: &str,
) -> bool {
    if !hide_libraries || library_category_selected {
        return true;
    }

    if !browse_addon_is_library(addon, local_state) {
        return true;
    }

    exact_addon_query_matches(addon, query)
}

fn browse_addon_is_library(addon: &AddonSummary, local_state: Option<&RemoteLocalState>) -> bool {
    remote_summary_is_library(addon)
        || local_state
            .is_some_and(|local_state| local_manifest_marks_remote_library(addon, local_state))
}

fn local_manifest_marks_remote_library(
    addon: &AddonSummary,
    local_state: &RemoteLocalState,
) -> bool {
    let Some(uid) = addon.uid.as_deref() else {
        return false;
    };

    local_state.matches.iter().any(|result| {
        result.local.is_library == Some(true)
            && result
                .remote
                .as_ref()
                .and_then(|remote| remote.uid.as_deref())
                == Some(uid)
    }) || local_state
        .metadata
        .addons
        .iter()
        .any(|(folder_name, metadata)| {
            metadata.remote_uid.as_deref() == Some(uid)
                && local_state.local_addons.iter().any(|local| {
                    local.folder_name == *folder_name && local.is_library == Some(true)
                })
        })
}

fn compare_remote_addons(
    left: &AddonSummary,
    right: &AddonSummary,
    mode: BrowseMode,
) -> std::cmp::Ordering {
    let primary = match mode {
        BrowseMode::MostDownloaded => right
            .downloads()
            .unwrap_or(-1)
            .cmp(&left.downloads().unwrap_or(-1)),
        BrowseMode::Recent => right.date.unwrap_or(0).cmp(&left.date.unwrap_or(0)),
    };

    primary
        .then_with(|| right.date.unwrap_or(0).cmp(&left.date.unwrap_or(0)))
        .then_with(|| {
            right
                .downloads()
                .unwrap_or(-1)
                .cmp(&left.downloads().unwrap_or(-1))
        })
        .then_with(|| {
            display_remote_name(left)
                .to_lowercase()
                .cmp(&display_remote_name(right).to_lowercase())
        })
}

fn display_remote_name(addon: &AddonSummary) -> &str {
    addon.name.as_deref().unwrap_or("")
}

fn addon_matches_category(
    addon: &AddonSummary,
    category_id: Option<&str>,
    categories: &[RemoteCategoryDto],
) -> bool {
    let Some(category_id) = category_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return true;
    };

    if addon
        .category_id()
        .as_deref()
        .is_some_and(|id| id.trim() == category_id)
    {
        return true;
    }

    let Some(selected_name) = categories
        .iter()
        .find(|category| category.id == category_id)
        .map(|category| normalize_category_name(&category.name))
    else {
        return false;
    };

    addon
        .category_name()
        .as_deref()
        .is_some_and(|name| normalize_category_name(name) == selected_name)
}

fn selected_category_is_libraries(
    category_id: Option<&str>,
    categories: &[RemoteCategoryDto],
) -> bool {
    let Some(category_id) = category_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return false;
    };

    if category_is_libraries(Some(category_id), None) {
        return true;
    }

    categories
        .iter()
        .find(|category| category.id == category_id)
        .is_some_and(|category| category_is_libraries(Some(&category.id), Some(&category.name)))
}

fn remote_summary_is_library(addon: &AddonSummary) -> bool {
    let category_id = addon.category_id();
    let category_name = addon.category_name();
    if category_is_libraries(category_id.as_deref(), category_name.as_deref()) {
        return true;
    }

    if category_id.is_some() || category_name.is_some() {
        return false;
    }

    weak_library_name_signal(addon.name.as_deref(), &addon.directories)
}

fn remote_candidate_is_library(candidate: &RemoteCandidate) -> bool {
    if category_is_libraries(
        candidate.category_id.as_deref(),
        candidate.category_name.as_deref(),
    ) {
        return true;
    }

    if candidate.category_id.is_some() || candidate.category_name.is_some() {
        return false;
    }

    weak_library_name_signal(candidate.name.as_deref(), &candidate.directories)
}

fn remote_details_is_library(details: &AddonDetails) -> bool {
    let category_id = details.category_id();
    let category_name = details.category_name();
    if category_is_libraries(category_id.as_deref(), category_name.as_deref()) {
        return true;
    }

    if category_id.is_some() || category_name.is_some() {
        return false;
    }

    details.name.as_deref().is_some_and(starts_with_lib)
}

fn category_is_libraries(category_id: Option<&str>, category_name: Option<&str>) -> bool {
    category_id.is_some_and(|id| id.trim() == "53")
        || category_name.is_some_and(|name| normalize_category_name(name).contains("librar"))
}

fn weak_library_name_signal(name: Option<&str>, directories: &[String]) -> bool {
    name.is_some_and(starts_with_lib)
        || directories
            .iter()
            .any(|directory| starts_with_lib(directory))
}

fn starts_with_lib(value: &str) -> bool {
    value.trim_start().to_ascii_lowercase().starts_with("lib")
}

fn exact_addon_query_matches(addon: &AddonSummary, query: &str) -> bool {
    let query = normalize_addon_lookup_name(query);
    if query.is_empty() {
        return false;
    }

    addon
        .name
        .as_deref()
        .is_some_and(|name| normalize_addon_lookup_name(name) == query)
        || addon
            .directories
            .iter()
            .any(|directory| normalize_addon_lookup_name(directory) == query)
}

fn normalize_addon_lookup_name(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize_optional_filter(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn normalize_category_name(value: &str) -> String {
    value
        .to_lowercase()
        .replace('&', "and")
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn category_dtos(categories: &[CategorySummary]) -> Vec<RemoteCategoryDto> {
    let mut output = categories
        .iter()
        .filter_map(category_dto)
        .collect::<Vec<_>>();
    output.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.id.cmp(&right.id))
    });
    output
}

fn category_dto(category: &CategorySummary) -> Option<RemoteCategoryDto> {
    let id = category.id()?;
    let name = category.name()?;
    Some(RemoteCategoryDto {
        id,
        name,
        parent_id: category.parent_id(),
    })
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
        is_library: remote_summary_is_library(addon),
        image_urls: addon.image_urls(),
        thumbnail_urls: addon.thumbnail_urls(),
        installed: false,
        installed_local: None,
        installed_match: None,
    }
}

fn addon_summary_dto_with_local_state(
    addon: &AddonSummary,
    remote_addons: &[AddonSummary],
    local_state: Option<&RemoteLocalState>,
) -> AddonSummaryDto {
    let mut dto = addon_summary_dto(addon);
    let Some(addon_id) = addon.uid.as_deref() else {
        return dto;
    };
    let Some(local_state) = local_state else {
        return dto;
    };
    if let Some(result) = remote_addon_match(addon_id, remote_addons, local_state) {
        dto.installed = true;
        dto.is_library = dto.is_library || result.local.is_library == Some(true);
        dto.installed_local = Some(local_addon_dto(&result.local));
        dto.installed_match = Some(match_result_dto(&result, &local_state.metadata));
    }
    dto
}

fn remote_local_state(
    path: Option<&str>,
    remote_addons: &[AddonSummary],
) -> anyhow::Result<RemoteLocalState> {
    let addons_dir = resolve_addons_dir(path)?;
    let local_addons = local::scan_addons_dir(&addons_dir)?;
    let metadata = load_installed_metadata(&addons_dir);
    let matches =
        match_remote::match_installed_addons_with_metadata(&local_addons, remote_addons, &metadata);

    Ok(RemoteLocalState {
        local_addons,
        metadata,
        matches,
    })
}

fn remote_addon_match(
    addon_id: &str,
    remote_addons: &[AddonSummary],
    local_state: &RemoteLocalState,
) -> Option<MatchResult> {
    let remote = remote_addons
        .iter()
        .find(|remote| remote.uid.as_deref() == Some(addon_id))?;

    metadata_remote_match(addon_id, remote, local_state).or_else(|| {
        local_state
            .matches
            .iter()
            .find(|result| {
                result
                    .remote
                    .as_ref()
                    .and_then(|remote| remote.uid.as_deref())
                    == Some(addon_id)
            })
            .cloned()
    })
}

fn metadata_remote_match(
    addon_id: &str,
    remote: &AddonSummary,
    local_state: &RemoteLocalState,
) -> Option<MatchResult> {
    local_state
        .metadata
        .addons
        .iter()
        .find(|(_, metadata)| metadata.remote_uid.as_deref() == Some(addon_id))
        .and_then(|(folder_name, _)| {
            local_state
                .local_addons
                .iter()
                .find(|local| local.folder_name == *folder_name && local.valid_manifest)
        })
        .map(|local| MatchResult {
            local: local.clone(),
            status: match_status_for_remote(local, remote),
            remote: Some(remote_candidate_from_summary(
                remote,
                0,
                120,
                "metadata-remote-uid",
            )),
            candidates: Vec::new(),
            debug_candidates: Vec::new(),
        })
}

fn match_status_for_remote(local: &LocalAddon, remote: &AddonSummary) -> match_remote::MatchStatus {
    let local_version = local.addon_version.as_deref().or(local.version.as_deref());
    let remote_version = remote.version.as_deref();

    match compare_versions(local_version, remote_version) {
        VersionComparison::RemoteNewer => match_remote::MatchStatus::PossibleUpdate,
        VersionComparison::Same => match_remote::MatchStatus::Matched,
        VersionComparison::LocalNewer => match_remote::MatchStatus::LocalNewer,
        VersionComparison::Unknown => match_remote::MatchStatus::UnknownUpdate,
    }
}

fn remote_candidate_from_summary(
    remote: &AddonSummary,
    tier: u8,
    score: usize,
    reason: &str,
) -> RemoteCandidate {
    RemoteCandidate {
        uid: remote.uid.clone(),
        name: remote.name.clone(),
        author_name: remote.author_name.clone(),
        version: remote.version.clone(),
        updated: remote.date,
        file_info_url: remote.file_info_url.clone(),
        summary: remote.summary.clone(),
        directories: remote.directories.clone(),
        category_id: remote.category_id(),
        category_name: remote.category_name(),
        downloads: remote.downloads(),
        monthly_downloads: remote.monthly_downloads(),
        image_urls: remote.image_urls(),
        thumbnail_urls: remote.thumbnail_urls(),
        tier,
        score,
        reason: reason.to_owned(),
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
        is_library: remote_details_is_library(details),
        image_urls: details.image_urls(),
        thumbnail_urls: details.thumbnail_urls(),
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
        is_library: remote_candidate_is_library(candidate),
        image_urls: candidate.image_urls.clone(),
        thumbnail_urls: candidate.thumbnail_urls.clone(),
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
    let managed = metadata.addon_for_folder(&result.local.folder_name);

    let Some(remote) = result.remote.as_ref() else {
        if managed.is_some_and(|managed| managed.remote_uid.is_some()) {
            return UpdateConfidence {
                confidence: "unknown",
                reason: "Remote metadata unavailable",
                managed: true,
            };
        }

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
        if managed.remote_uid.is_none() {
            return confidence_from_versions(
                managed.remote_version.as_deref(),
                remote.version.as_deref(),
                managed.remote_updated_date,
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
        managed.remote_updated_date,
        remote.updated,
        true,
    )
}

fn update_confidence_for_action(
    action: &PlannedAddonAction,
    metadata: &InstalledMetadata,
) -> Option<UpdateConfidence> {
    let managed = metadata.addon_for_folder(&action.local_folder);

    match managed {
        Some(managed) if managed.remote_uid == action.remote_uid => Some(confidence_from_versions(
            managed.remote_version.as_deref(),
            action.remote_version.as_deref(),
            managed.remote_updated_date,
            action.remote_date,
            true,
        )),
        Some(managed) if managed.remote_uid.is_none() => Some(confidence_from_versions(
            managed.remote_version.as_deref(),
            action.remote_version.as_deref(),
            managed.remote_updated_date,
            action.remote_date,
            true,
        )),
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
    matches!(
        managed.installed_by.as_str(),
        manager_metadata::INSTALLED_BY_IMPORTED_CURRENT
            | manager_metadata::INSTALLED_BY_FIRST_RUN_IMPORT
    )
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
    failure: Option<UpdateAllFailureDto>,
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
        failure,
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

fn dependency_plan_dto(prepared: &PreparedRemoteInstall) -> DependencyPlanDto {
    DependencyPlanDto {
        main_addon: DependencyPlanMainDto {
            uid: prepared.dependency_plan.main_addon.uid.clone(),
            name: prepared.dependency_plan.main_addon.name.clone(),
        },
        required_dependencies: prepared
            .dependency_plan
            .required_dependencies
            .iter()
            .map(dependency_plan_entry_dto)
            .collect(),
        optional_dependencies: prepared
            .dependency_plan
            .optional_dependencies
            .iter()
            .map(dependency_plan_entry_dto)
            .collect(),
        install_items: dependency_install_items_dto(prepared),
        install_order: prepared.dependency_plan.install_order.clone(),
    }
}

fn dependency_plan_entry_dto(entry: &dependencies::DependencyPlanEntry) -> DependencyPlanEntryDto {
    DependencyPlanEntryDto {
        name: entry.name.clone(),
        constraint: entry.constraint.clone(),
        raw: entry.raw.clone(),
        required: entry.required,
        relation: entry.relation.as_str().to_owned(),
        depth: entry.depth,
        parent: entry.parent.clone(),
        status: entry.status.as_str().to_owned(),
        remote_uid: entry.remote_uid.clone(),
        remote_name: entry.remote_name.clone(),
        remote_version: entry.remote_version.clone(),
        installed_folder: entry.installed_folder.clone(),
        installed_title: entry.installed_title.clone(),
        installed_version: entry.installed_version.clone(),
        bundled_folder: entry.bundled_folder.clone(),
    }
}

fn installed_addon_dependencies_response(
    report: &dependencies::DependencyStatusReport,
    warning: Option<String>,
) -> InstalledAddonDependenciesResponse {
    InstalledAddonDependenciesResponse {
        required_dependencies: report
            .required_dependencies
            .iter()
            .map(installed_dependency_status_dto)
            .collect(),
        optional_dependencies: report
            .optional_dependencies
            .iter()
            .map(installed_dependency_status_dto)
            .collect(),
        warning,
    }
}

fn installed_dependency_status_dto(
    dependency: &dependencies::DependencyStatusEntry,
) -> InstalledDependencyStatusDto {
    InstalledDependencyStatusDto {
        name: dependency.name.clone(),
        raw: dependency.raw.clone(),
        constraint: dependency.constraint.clone(),
        required: dependency.required,
        relation: dependency.relation.as_str().to_owned(),
        depth: dependency.depth,
        parent: dependency.parent.clone(),
        installed: dependency.installed,
        installed_folder: dependency.installed_folder.clone(),
        installed_title: dependency.installed_title.clone(),
        installed_version: dependency.installed_version.clone(),
        remote_uid: dependency.remote_uid.clone(),
        remote_name: dependency.remote_name.clone(),
        remote_version: dependency.remote_version.clone(),
        status: dependency.status.as_str().to_owned(),
    }
}

fn dependency_install_items_dto(prepared: &PreparedRemoteInstall) -> Vec<DependencyInstallItemDto> {
    let mut items = prepared
        .dependency_installs
        .iter()
        .map(|dependency| DependencyInstallItemDto {
            role: "required-dependency".to_owned(),
            name: dependency
                .details
                .name
                .clone()
                .unwrap_or_else(|| dependency.remote_uid.clone()),
            remote_uid: Some(dependency.remote_uid.clone()),
            remote_name: dependency.details.name.clone(),
            action: install_plan_action_summary(&dependency.plan).to_owned(),
        })
        .collect::<Vec<_>>();

    items.push(DependencyInstallItemDto {
        role: "main-addon".to_owned(),
        name: prepared
            .details
            .name
            .clone()
            .unwrap_or_else(|| prepared.dependency_plan.main_addon.uid.clone()),
        remote_uid: Some(prepared.dependency_plan.main_addon.uid.clone()),
        remote_name: prepared.details.name.clone(),
        action: install_plan_action_summary(&prepared.main_plan).to_owned(),
    });

    items
}

fn install_plan_action_summary(plan: &InstallPlan) -> &'static str {
    if plan
        .items
        .iter()
        .any(|item| item.action == InstallPlanAction::WouldReplaceExisting)
    {
        "would-replace-existing"
    } else if plan
        .items
        .iter()
        .any(|item| item.action == InstallPlanAction::WouldInstallNew)
    {
        "would-install-new"
    } else if let Some(item) = plan.items.first() {
        item.action.as_str()
    } else {
        "empty"
    }
}

fn validate_new_only_prepared_install(prepared: &PreparedRemoteInstall) -> Result<(), String> {
    if prepared
        .dependency_plan
        .required_dependencies
        .iter()
        .any(|dependency| {
            matches!(
                dependency.status,
                DependencyStatus::Unresolved
                    | DependencyStatus::Ambiguous
                    | DependencyStatus::Circular
                    | DependencyStatus::MaxDepth
            )
        })
    {
        return Err("Some required dependencies could not be resolved.".to_owned());
    }

    for dependency in &prepared.dependency_installs {
        validate_new_only_install_plan(&dependency.plan)?;
    }

    validate_new_only_install_plan(&prepared.main_plan)
}

fn validate_new_only_install_plan(plan: &InstallPlan) -> Result<(), String> {
    if plan.items.is_empty() {
        return Err("install plan has no valid addon folders to install".to_owned());
    }

    if plan
        .items
        .iter()
        .all(|item| matches!(item.action, InstallPlanAction::WouldInstallNew))
    {
        return Ok(());
    }

    Err(
        "install preview requires confirmation because the package would replace existing folders or contains skipped/invalid addon folders"
            .to_owned(),
    )
}

async fn prepare_remote_install_plan(
    client: &ApiClient,
    addon_id: &str,
    addons_dir: &std::path::Path,
) -> Result<PreparedRemoteInstall, String> {
    let installed_addons =
        scan_installed_addons_for_install(addons_dir).map_err(to_string_error)?;
    let installed_metadata = load_installed_metadata(addons_dir);
    let installed_remotes = installed_remote_addons(&installed_metadata);
    let remote_addons = client.eso_file_list().await.map_err(to_string_error)?;
    let details = client
        .eso_file_details_fresh(addon_id)
        .await
        .map_err(to_string_error)?;
    let (install_plan, extracted) =
        prepare_remote_package(client, &details, addon_id, addons_dir, &installed_addons).await?;
    let main_source = DependencyManifestSource::from_extracted(&extracted);
    let main_addon = dependencies::RemoteAddonRef {
        uid: addon_id.to_owned(),
        name: details.name.clone(),
    };
    let mut remote_sources = BTreeMap::new();
    let mut prepared_dependency_packages = BTreeMap::new();
    let dependency_plan = loop {
        let dependency_plan = dependencies::build_dependency_plan_with_remote_sources(
            main_addon.clone(),
            &main_source,
            &installed_addons,
            &remote_addons,
            &installed_remotes,
            &remote_sources,
            DEFAULT_MAX_DEPENDENCY_DEPTH,
        );
        let dependency_uids = dependency_plan.required_remote_manifests_to_fetch(&remote_sources);
        if dependency_uids.is_empty() {
            break dependency_plan;
        }

        if remote_sources.len() + dependency_uids.len() > MAX_RECURSIVE_DEPENDENCY_PACKAGES {
            return Err(format!(
                "Recursive dependency resolution exceeded {MAX_RECURSIVE_DEPENDENCY_PACKAGES} packages."
            ));
        }

        for remote_uid in dependency_uids {
            if remote_sources.contains_key(&remote_uid) {
                continue;
            }
            let dependency_details = client
                .eso_file_details_fresh(&remote_uid)
                .await
                .map_err(to_string_error)?;
            let (plan, dependency_extracted) = prepare_remote_package(
                client,
                &dependency_details,
                &remote_uid,
                addons_dir,
                &installed_addons,
            )
            .await?;
            remote_sources.insert(
                remote_uid.clone(),
                DependencyManifestSource::from_extracted(&dependency_extracted),
            );
            prepared_dependency_packages.insert(
                remote_uid.clone(),
                PreparedDependencyInstall {
                    details: dependency_details,
                    remote_uid,
                    plan,
                },
            );
        }
    };

    let mut dependency_installs = Vec::new();
    for dependency in dependency_plan.required_dependencies_to_install() {
        let Some(remote_uid) = dependency.remote_uid.as_deref() else {
            continue;
        };
        if let Some(prepared) = prepared_dependency_packages.remove(remote_uid) {
            dependency_installs.push(prepared);
        }
    }

    Ok(PreparedRemoteInstall {
        details,
        main_plan: install_plan,
        dependency_plan,
        dependency_installs,
    })
}

async fn prepare_remote_package(
    client: &ApiClient,
    details: &AddonDetails,
    _addon_id: &str,
    addons_dir: &std::path::Path,
    installed_addons: &[LocalAddon],
) -> Result<(InstallPlan, zip_safety::ExtractedZip), String> {
    let download_url = remote::download_url(details).map_err(to_string_error)?;
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
        plan::plan_install(&extracted, addons_dir, installed_addons).map_err(to_string_error)?;

    Ok((install_plan, extracted))
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
    let metadata = load_installed_metadata(&addons_dir);
    let mut matches =
        match_remote::match_installed_addons_with_metadata(&local_addons, &remote_addons, &metadata);
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

fn load_installed_metadata(addons_dir: &std::path::Path) -> InstalledMetadata {
    manager_metadata::load_installed_metadata_or_default(addons_dir)
}

fn installed_remote_addons(
    metadata: &InstalledMetadata,
) -> Vec<dependencies::InstalledRemoteAddon> {
    manager_metadata::installed_remote_addons(metadata)
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
        if !is_importable_existing_match(result)
            || metadata.addons.contains_key(&result.local.folder_name)
        {
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
    Ok(manager_metadata::save_installed_metadata(
        addons_dir, metadata,
    )?)
}

fn remove_installed_metadata(
    addons_dir: &std::path::Path,
    folder_name: &str,
) -> anyhow::Result<()> {
    Ok(manager_metadata::remove_installed_metadata(
        addons_dir,
        folder_name,
    )?)
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
        folder_name: result.local.folder_name.clone(),
        remote_uid: remote.and_then(|remote| normalize_optional_path(remote.uid.clone())),
        remote_name: remote.and_then(|remote| normalize_optional_path(remote.name.clone())),
        remote_version: remote.and_then(|remote| normalize_optional_path(remote.version.clone())),
        remote_updated_date: remote.and_then(|remote| remote.updated),
        remote_info_url: remote.and_then(|remote| normalize_optional_path(remote.file_info_url.clone())),
        remote_download_url: None,
        file_name: None,
        md5: None,
        installed_at: installed_timestamp.to_owned(),
        installed_by: manager_metadata::INSTALLED_BY_IMPORTED_CURRENT.to_owned(),
        local_title: result.local.title.clone(),
        local_version: result
            .local
            .addon_version
            .clone()
            .or_else(|| result.local.version.clone()),
        source_addon_uid: None,
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

        match result.status {
            match_remote::MatchStatus::Ambiguous => {
                response.skipped_ambiguous += 1;
                continue;
            }
            match_remote::MatchStatus::NoMatch => {
                response.skipped_no_match += 1;
                continue;
            }
            match_remote::MatchStatus::Library => {
                response.skipped_libraries += 1;
                continue;
            }
            match_remote::MatchStatus::Matched
            | match_remote::MatchStatus::PossibleUpdate
            | match_remote::MatchStatus::LocalNewer
            | match_remote::MatchStatus::UnknownUpdate => {}
        }

        let Some(remote) = result.remote.as_ref() else {
            response.skipped_no_match += 1;
            continue;
        };

        if normalize_optional_path(remote.uid.clone()).is_none() {
            response.skipped_missing_remote_uid += 1;
            continue;
        }

        if normalize_optional_path(remote.version.clone()).is_none() {
            response.skipped_missing_remote_version += 1;
            continue;
        }

        metadata.addons.insert(
            result.local.folder_name.clone(),
            first_run_metadata_for_match(result, &installed_timestamp),
        );
        response.imported += 1;
    }

    metadata.first_run_baseline_complete = true;
    save_installed_metadata(addons_dir, &metadata)?;

    Ok(response)
}

fn is_importable_existing_match(result: &MatchResult) -> bool {
    if !result.local.valid_manifest {
        return false;
    }
    if matches!(
        result.status,
        match_remote::MatchStatus::Ambiguous
            | match_remote::MatchStatus::NoMatch
            | match_remote::MatchStatus::Library
    ) {
        return false;
    }

    result.remote.as_ref().is_some_and(|remote| {
        normalize_optional_path(remote.uid.clone()).is_some()
            && normalize_optional_path(remote.version.clone()).is_some()
    })
}

fn record_installed_metadata(
    addons_dir: &std::path::Path,
    plan: &InstallPlan,
    details: &AddonDetails,
    remote_uid: &str,
    result: &InstallResult,
    source: &str,
    source_addon_uid: Option<&str>,
) -> anyhow::Result<()> {
    Ok(manager_metadata::record_remote_install_metadata(
        addons_dir,
        plan,
        result,
        manager_metadata::RemoteInstallMetadata {
            details,
            remote_uid,
            installed_by: source,
            source_addon_uid,
        },
    )?)
}

fn apply_prepared_remote_install(
    addons_dir: &std::path::Path,
    prepared: &PreparedRemoteInstall,
    main_remote_uid: &str,
    backup_dir: Option<&std::path::Path>,
    source: &str,
) -> Result<InstallResult, String> {
    if prepared.dependency_plan.has_unresolved_required_dependencies() {
        return Err("Some required dependencies could not be resolved safely.".to_owned());
    }

    let mut aggregate = InstallResult::default();

    for dependency in &prepared.dependency_installs {
        let result =
            apply::apply_install_plan(&dependency.plan, backup_dir).map_err(to_string_error)?;
        record_installed_metadata(
            addons_dir,
            &dependency.plan,
            &dependency.details,
            &dependency.remote_uid,
            &result,
            manager_metadata::INSTALLED_BY_DEPENDENCY_INSTALL,
            Some(main_remote_uid),
        )
        .map_err(to_string_error)?;
        apply::merge_install_result(&mut aggregate, result);
    }

    let result =
        apply::apply_install_plan(&prepared.main_plan, backup_dir).map_err(to_string_error)?;
    record_installed_metadata(
        addons_dir,
        &prepared.main_plan,
        &prepared.details,
        main_remote_uid,
        &result,
        source,
        None,
    )
    .map_err(to_string_error)?;
    apply::merge_install_result(&mut aggregate, result);

    Ok(aggregate)
}

fn install_remote_addon_response(
    details: &AddonDetails,
    addons_dir: &std::path::Path,
    prepared: &PreparedRemoteInstall,
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
        plan: install_plan_dto(&prepared.main_plan),
        dependency_plan: dependency_plan_dto(prepared),
        items: result.items.iter().map(installed_item_dto).collect(),
    }
}

fn single_update_apply_response(
    target: &str,
    selected: &MatchResult,
    decision: &update::UpdateDecision,
    details: &AddonDetails,
    addons_dir: &std::path::Path,
    prepared: &PreparedRemoteInstall,
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
        plan: Some(install_plan_dto(&prepared.main_plan)),
        dependency_plan: Some(dependency_plan_dto(prepared)),
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

fn remove_installed_addon_response(result: &RemoveAddonResult) -> RemoveInstalledAddonResponse {
    RemoveInstalledAddonResponse {
        removed_addon: result.removed_addon,
        removed_saved_variables: result.removed_saved_variables,
        saved_variables_deleted_count: result.saved_variables_deleted_count,
        saved_variables_deleted_files: result.saved_variables_deleted_files.clone(),
        saved_variables_missing_files: result.saved_variables_missing_files.clone(),
        addon_folder: result.addon_folder.clone(),
        original_path: path_string(&result.original_path),
        message: result.message.clone(),
    }
}

fn clear_saved_variables_response(
    result: &ClearSavedVariablesResult,
) -> ClearSavedVariablesResponse {
    ClearSavedVariablesResponse {
        addon_folder: result.addon_folder.clone(),
        saved_variables_dir: path_string(&result.saved_variables_dir),
        deleted_count: result.deleted_count,
        deleted_files: result.deleted_files.clone(),
        missing_files: result.missing_files.clone(),
        status: result.status.as_str().to_owned(),
        message: if result.deleted_count > 0 {
            "SavedVariables cleared.".to_owned()
        } else {
            "No SavedVariables files were found.".to_owned()
        },
    }
}

fn backup_result_response(result: &BackupResult) -> BackupResultResponse {
    BackupResultResponse {
        backup_zip_path: path_string(&result.backup_zip_path),
        backup_created: result.backup_created,
        included_saved_variables: result.included_saved_variables,
        file_count: result.file_count,
        total_bytes: result.total_uncompressed_bytes,
        skipped_files_count: result.skipped_files.len(),
        skipped_files: result
            .skipped_files
            .iter()
            .map(|skipped| SkippedBackupFileResponse {
                relative_path: skipped.relative_path.clone(),
                reason: skipped.reason.clone(),
            })
            .collect(),
        warnings: result.warnings.clone(),
        backup_status: result.backup_status.as_str().to_owned(),
    }
}

fn backup_inspection_response(
    inspection: &BackupInspection,
    target_addons_folder: &Path,
    target_saved_variables_folder: &Path,
) -> BackupInspectionResponse {
    BackupInspectionResponse {
        valid: inspection.valid,
        backup_name: inspection.backup_name.clone(),
        created_at: inspection.created_at.clone(),
        contains_addons: inspection.contains_addons,
        contains_saved_variables: inspection.contains_saved_variables,
        file_count: inspection.file_count,
        total_bytes: inspection.total_bytes,
        warnings: inspection.warnings.clone(),
        target_addons_folder: path_string(target_addons_folder),
        target_saved_variables_folder: path_string(target_saved_variables_folder),
    }
}

fn invalid_backup_inspection_response(
    zip_path: &Path,
    target_addons_folder: &Path,
    target_saved_variables_folder: &Path,
    message: String,
) -> BackupInspectionResponse {
    BackupInspectionResponse {
        valid: false,
        backup_name: zip_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| path_string(zip_path)),
        created_at: None,
        contains_addons: false,
        contains_saved_variables: false,
        file_count: 0,
        total_bytes: 0,
        warnings: vec![message],
        target_addons_folder: path_string(target_addons_folder),
        target_saved_variables_folder: path_string(target_saved_variables_folder),
    }
}

fn restore_result_response(result: &RestoreResult) -> RestoreResultResponse {
    RestoreResultResponse {
        restored_addons: result.restored_addons,
        restored_saved_variables: result.restored_saved_variables,
        message: result.message.clone(),
        rollback_path: result.rollback_path.as_ref().map(|path| path_string(path)),
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

fn single_update_plan_response(
    target: String,
    selected: &MatchResult,
    decision: &update::UpdateDecision,
    addons_dir: &Path,
) -> SingleUpdatePlanResponse {
    SingleUpdatePlanResponse {
        dry_run: true,
        applied: false,
        target,
        local: local_addon_dto(&selected.local),
        remote: selected.remote.as_ref().map(remote_candidate_dto),
        decision: decision.as_str().to_owned(),
        should_install: decision.should_install(),
        reason: Some(update_skip_reason(decision).to_owned()),
        remote_details: None,
        addons_dir: path_string(addons_dir),
        plan: None,
        dependency_plan: None,
    }
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

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit_index = 0;
    while value >= 1024.0 && unit_index < UNITS.len() - 1 {
        value /= 1024.0;
        unit_index += 1;
    }
    if unit_index == 0 {
        format!("{bytes} {}", UNITS[unit_index])
    } else {
        format!("{value:.1} {}", UNITS[unit_index])
    }
}

fn image_content_type(content_type: Option<&str>, url: &str) -> Option<String> {
    if let Some(content_type) = content_type
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .filter(|value| value.starts_with("image/"))
    {
        return Some(content_type.to_owned());
    }

    let url_path = tauri::Url::parse(url).ok()?.path().to_lowercase();
    if url_path.ends_with(".png") {
        Some("image/png".to_owned())
    } else if url_path.ends_with(".jpg") || url_path.ends_with(".jpeg") {
        Some("image/jpeg".to_owned())
    } else if url_path.ends_with(".gif") {
        Some("image/gif".to_owned())
    } else if url_path.ends_with(".webp") {
        Some("image/webp".to_owned())
    } else {
        None
    }
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = *chunk.get(1).unwrap_or(&0);
        let third = *chunk.get(2).unwrap_or(&0);
        let combined = ((first as u32) << 16) | ((second as u32) << 8) | third as u32;
        output.push(TABLE[((combined >> 18) & 0x3f) as usize] as char);
        output.push(TABLE[((combined >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            output.push(TABLE[((combined >> 6) & 0x3f) as usize] as char);
        } else {
            output.push('=');
        }
        if chunk.len() > 2 {
            output.push(TABLE[(combined & 0x3f) as usize] as char);
        } else {
            output.push('=');
        }
    }
    output
}

fn path_string(path: &std::path::Path) -> String {
    path.display().to_string()
}

fn allowed_external_url(value: &str) -> anyhow::Result<String> {
    let parsed = tauri::Url::parse(value.trim())?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(anyhow::anyhow!(
            "only http:// and https:// URLs can be opened"
        ));
    }
    Ok(parsed.into())
}

fn resolve_addon_folder_for_open(addons_dir: &Path, folder_name: &str) -> anyhow::Result<PathBuf> {
    let addon = remove::resolve_installed_addon(addons_dir, folder_name)?;
    let target = addons_dir.join(&addon.folder_name);
    let addons_root = fs::canonicalize(addons_dir)?;
    let target = fs::canonicalize(&target)
        .map_err(|_| anyhow::anyhow!("addon folder is missing: {}", target.display()))?;

    if !target.starts_with(&addons_root) {
        return Err(anyhow::anyhow!(
            "addon target path escapes AddOns directory: {}",
            target.display()
        ));
    }
    if !target.is_dir() {
        return Err(anyhow::anyhow!(
            "addon folder is missing: {}",
            target.display()
        ));
    }

    Ok(target)
}

fn resolve_folder_for_open(path: &str) -> anyhow::Result<PathBuf> {
    let target = required_path(path, "folder")?;
    let target = fs::canonicalize(&target)
        .map_err(|_| anyhow::anyhow!("folder is missing: {}", target.display()))?;
    if !target.is_dir() {
        return Err(anyhow::anyhow!(
            "folder is not a directory: {}",
            target.display()
        ));
    }

    Ok(target)
}

fn resolve_path_location_for_open(path: &str) -> anyhow::Result<PathBuf> {
    let target = required_path(path, "path")?;
    let target = fs::canonicalize(&target)
        .map_err(|_| anyhow::anyhow!("path is missing: {}", target.display()))?;
    if target.is_dir() {
        return Ok(target);
    }
    if target.is_file() {
        return target
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| anyhow::anyhow!("file has no containing folder: {}", target.display()));
    }

    Err(anyhow::anyhow!(
        "path is not a file or directory: {}",
        target.display()
    ))
}

fn saved_variables_dir_for_addons(addons_dir: &Path) -> PathBuf {
    addons_dir
        .parent()
        .map(|parent| parent.join("SavedVariables"))
        .unwrap_or_else(|| PathBuf::from("SavedVariables"))
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
            hide_libraries_in_search: true,
            hide_libraries_in_installed: true,
        };

        save_app_settings_to_path(&path, &settings).unwrap();
        let loaded = load_app_settings_from_path(&path).unwrap();

        assert_eq!(loaded, settings);
    }

    #[test]
    fn app_settings_loads_missing_library_visibility_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = app_settings_path(dir.path());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            r#"{
  "addons_dir_override": "D:\\ESO\\AddOns",
  "keep_downloads_default": true,
  "include_unknown_updates_default": true
}"#,
        )
        .unwrap();

        let settings = load_app_settings_from_path(&path).unwrap();

        assert!(!settings.hide_libraries_in_search);
        assert!(!settings.hide_libraries_in_installed);
        assert!(settings.keep_downloads_default);
        assert!(settings.include_unknown_updates_default);
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
            hide_libraries_in_search: true,
            hide_libraries_in_installed: true,
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
            hide_libraries_in_search: None,
            hide_libraries_in_installed: None,
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
            hide_libraries_in_search: false,
            hide_libraries_in_installed: false,
        };

        save_app_settings_to_path(&path, &settings).unwrap();
        let content = fs::read_to_string(&path).unwrap();

        assert!(!content.contains("force"));
        assert!(!content.contains("reinstall"));
    }

    #[test]
    fn external_url_allows_only_http_and_https() {
        assert!(allowed_external_url("https://www.esoui.com/downloads/info7.html").is_ok());
        assert!(allowed_external_url("http://www.esoui.com/downloads/info7.html").is_ok());
        assert!(allowed_external_url("javascript:alert(1)").is_err());
        assert!(allowed_external_url("file:///C:/Windows/notepad.exe").is_err());
        assert!(allowed_external_url("mailto:someone@example.com").is_err());
    }

    #[test]
    fn open_addon_folder_resolves_installed_folder() {
        let dir = tempfile::tempdir().unwrap();
        let addons_dir = dir.path().join("AddOns");
        write_test_addon(&addons_dir, "ExampleAddon");

        let target = resolve_addon_folder_for_open(&addons_dir, "ExampleAddon").unwrap();

        assert_eq!(
            target,
            std::fs::canonicalize(addons_dir.join("ExampleAddon")).unwrap()
        );
    }

    #[test]
    fn open_addon_folder_validates_containment() {
        let dir = tempfile::tempdir().unwrap();
        let addons_dir = dir.path().join("AddOns");
        write_test_addon(&addons_dir, "ExampleAddon");

        let error = resolve_addon_folder_for_open(&addons_dir, "../ExampleAddon").unwrap_err();

        assert!(error.to_string().contains("unsafe addon folder name"));
    }

    #[test]
    fn open_folder_requires_a_directory() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("backup.txt");
        std::fs::write(&file, "not a folder").unwrap();

        let error = resolve_folder_for_open(&path_string(&file)).unwrap_err();

        assert!(error.to_string().contains("folder is not a directory"));
    }

    #[test]
    fn first_run_import_records_matched_addons_as_current_baseline() {
        let dir = tempfile::tempdir().unwrap();
        let addons_dir = dir.path().join("AddOns");
        fs::create_dir_all(&addons_dir).unwrap();
        let local = test_local_addon("ExampleAddon", true, false);
        let matches = vec![MatchResult {
            local,
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
        assert_eq!(imported.remote_name.as_deref(), Some("Example Addon"));
        assert_eq!(imported.remote_version.as_deref(), Some("2.0.0"));
        assert_eq!(imported.remote_updated_date, Some(1_700_000_000));
        assert_eq!(
            imported.remote_info_url.as_deref(),
            Some("https://www.esoui.com/downloads/info42.html")
        );
        assert_eq!(
            imported.installed_by,
            manager_metadata::INSTALLED_BY_IMPORTED_CURRENT
        );
        assert!(!imported.installed_at.is_empty());
        assert_eq!(imported.local_title.as_deref(), Some("ExampleAddon"));
        assert_eq!(imported.local_version.as_deref(), Some("1.0.0"));
        assert!(metadata.first_run_baseline_complete);
    }

    #[test]
    fn first_run_import_skips_matched_addons_without_remote_version() {
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

        assert_eq!(result.detected_addons, 1);
        assert_eq!(result.imported, 0);
        assert_eq!(result.skipped_missing_remote_version, 1);
        assert!(!metadata.addons.contains_key("ExampleAddon"));
        assert!(metadata.first_run_baseline_complete);
    }

    #[test]
    fn first_run_import_skips_unmatched_and_unresolved_library_addons() {
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
        assert_eq!(result.imported, 0);
        assert_eq!(result.skipped_no_match, 1);
        assert_eq!(result.skipped_libraries, 1);
        assert!(metadata.addons.is_empty());
        assert!(metadata.first_run_baseline_complete);
    }

    #[test]
    fn first_run_import_skips_ambiguous_addons() {
        let dir = tempfile::tempdir().unwrap();
        let addons_dir = dir.path().join("AddOns");
        fs::create_dir_all(&addons_dir).unwrap();
        let matches = vec![MatchResult {
            local: test_local_addon("AmbiguousAddon", true, false),
            status: match_remote::MatchStatus::Ambiguous,
            remote: None,
            candidates: vec![
                test_remote_candidate("42", "2.0.0"),
                test_remote_candidate("43", "2.0.0"),
            ],
            debug_candidates: Vec::new(),
        }];

        let result = import_existing_matches_as_current(&addons_dir, &matches).unwrap();
        let metadata = load_installed_metadata(&addons_dir);

        assert_eq!(result.detected_addons, 1);
        assert_eq!(result.imported, 0);
        assert_eq!(result.skipped_ambiguous, 1);
        assert!(!metadata.addons.contains_key("AmbiguousAddon"));
    }

    #[test]
    fn first_run_import_does_not_modify_addon_or_savedvariables_files() {
        let dir = tempfile::tempdir().unwrap();
        let live_dir = dir.path().join("live");
        let addons_dir = live_dir.join("AddOns");
        write_test_addon(&addons_dir, "ExampleAddon");
        let addon_manifest = addons_dir.join("ExampleAddon").join("ExampleAddon.txt");
        let saved_variables = live_dir.join("SavedVariables").join("ExampleAddon.lua");
        fs::create_dir_all(saved_variables.parent().unwrap()).unwrap();
        fs::write(&saved_variables, "saved = true").unwrap();
        let before_manifest = fs::read_to_string(&addon_manifest).unwrap();
        let before_saved_variables = fs::read_to_string(&saved_variables).unwrap();
        let matches = vec![MatchResult {
            local: test_local_addon("ExampleAddon", true, false),
            status: match_remote::MatchStatus::Matched,
            remote: Some(test_remote_candidate("42", "1.0.0")),
            candidates: Vec::new(),
            debug_candidates: Vec::new(),
        }];

        import_existing_matches_as_current(&addons_dir, &matches).unwrap();

        assert_eq!(fs::read_to_string(&addon_manifest).unwrap(), before_manifest);
        assert_eq!(
            fs::read_to_string(&saved_variables).unwrap(),
            before_saved_variables
        );
        assert!(manager_metadata::installed_metadata_path(&addons_dir).exists());
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
                status: match_remote::MatchStatus::PossibleUpdate,
                remote: Some(test_remote_candidate("99", "2.0.0")),
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
    fn first_run_import_does_not_repair_unmatched_addons() {
        let dir = tempfile::tempdir().unwrap();
        let addons_dir = dir.path().join("AddOns");
        fs::create_dir_all(&addons_dir).unwrap();
        let matches = vec![
            MatchResult {
                local: test_local_addon("AlreadyImported", true, false),
                status: match_remote::MatchStatus::PossibleUpdate,
                remote: Some(test_remote_candidate("42", "2.0.0")),
                candidates: Vec::new(),
                debug_candidates: Vec::new(),
            },
            MatchResult {
                local: test_local_addon("UnmatchedAddon", true, false),
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

        assert!(repaired.addons.contains_key("AlreadyImported"));
        assert!(!repaired.addons.contains_key("UnmatchedAddon"));
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

    #[test]
    fn single_update_preview_for_update_is_lightweight_and_direct() {
        let dir = tempfile::tempdir().unwrap();
        let selected = MatchResult {
            local: test_local_addon("ExampleAddon", true, false),
            status: match_remote::MatchStatus::PossibleUpdate,
            remote: Some(test_remote_candidate("42", "2.0.0")),
            candidates: Vec::new(),
            debug_candidates: Vec::new(),
        };

        let response = single_update_plan_response(
            "ExampleAddon".to_owned(),
            &selected,
            &update::UpdateDecision::WouldUpdate,
            dir.path(),
        );

        assert!(response.should_install);
        assert_eq!(response.decision, "would-update");
        assert_eq!(response.reason.as_deref(), Some("update can proceed"));
        assert!(response.plan.is_none());
        assert!(response.dependency_plan.is_none());
        assert!(response.remote_details.is_none());
    }

    #[test]
    fn single_update_preview_for_current_returns_clear_no_update_reason() {
        let dir = tempfile::tempdir().unwrap();
        let selected = MatchResult {
            local: test_local_addon("ExampleAddon", true, false),
            status: match_remote::MatchStatus::Matched,
            remote: Some(test_remote_candidate("42", "1.0.0")),
            candidates: Vec::new(),
            debug_candidates: Vec::new(),
        };

        let response = single_update_plan_response(
            "ExampleAddon".to_owned(),
            &selected,
            &update::UpdateDecision::SkippedCurrent,
            dir.path(),
        );

        assert!(!response.should_install);
        assert_eq!(response.decision, "skipped-current");
        assert_eq!(response.reason.as_deref(), Some("selected addon is current"));
        assert!(response.plan.is_none());
        assert!(response.dependency_plan.is_none());
    }

    #[test]
    fn browse_most_downloaded_sorts_descending_and_limits() {
        let addons = vec![
            test_addon_summary("1", "Low", "10", 100, "17", "Graphic UI Mods"),
            test_addon_summary("2", "High", "50", 90, "17", "Graphic UI Mods"),
            test_addon_summary("3", "Mid", "25", 110, "17", "Graphic UI Mods"),
        ];

        let results = browse_remote_results(
            &addons,
            &[],
            None,
            BrowseMode::MostDownloaded,
            None,
            "",
            Some(2),
            false,
        );

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].uid.as_deref(), Some("2"));
        assert_eq!(results[1].uid.as_deref(), Some("3"));
    }

    #[test]
    fn browse_recent_sorts_by_updated_date_and_filters_query() {
        let addons = vec![
            test_addon_summary_with_summary(
                "1",
                "Older",
                "10",
                100,
                "17",
                "Graphic UI Mods",
                "compass tools",
            ),
            test_addon_summary_with_summary(
                "2",
                "Newest",
                "50",
                300,
                "17",
                "Graphic UI Mods",
                "combat display",
            ),
            test_addon_summary_with_summary(
                "3",
                "Newer",
                "25",
                200,
                "17",
                "Graphic UI Mods",
                "combat feedback",
            ),
        ];

        let results = browse_remote_results(
            &addons,
            &[],
            None,
            BrowseMode::Recent,
            None,
            "combat",
            Some(25),
            false,
        );

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].uid.as_deref(), Some("2"));
        assert_eq!(results[1].uid.as_deref(), Some("3"));
    }

    #[test]
    fn browse_query_can_return_all_matches_without_limit() {
        let addons = vec![
            test_addon_summary_with_summary(
                "1",
                "Older",
                "10",
                100,
                "17",
                "Graphic UI Mods",
                "combat tools",
            ),
            test_addon_summary_with_summary(
                "2",
                "Newest",
                "50",
                300,
                "17",
                "Graphic UI Mods",
                "combat display",
            ),
            test_addon_summary_with_summary(
                "3",
                "Newer",
                "25",
                200,
                "17",
                "Graphic UI Mods",
                "combat feedback",
            ),
        ];

        let results = browse_remote_results(
            &addons,
            &[],
            None,
            BrowseMode::Recent,
            None,
            "combat",
            None,
            false,
        );

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].uid.as_deref(), Some("2"));
        assert_eq!(results[1].uid.as_deref(), Some("3"));
        assert_eq!(results[2].uid.as_deref(), Some("1"));
    }

    #[test]
    fn browse_category_filter_uses_id_and_name_fallback() {
        let categories = vec![RemoteCategoryDto {
            id: "17".to_owned(),
            name: "Graphic UI Mods".to_owned(),
            parent_id: None,
        }];
        let addons = vec![
            test_addon_summary("1", "Direct", "10", 100, "17", "Different name"),
            test_addon_summary("2", "Fallback", "20", 110, "", "Graphic UI Mods"),
            test_addon_summary("3", "Other", "30", 120, "25", "Combat Mods"),
        ];

        let results = browse_remote_results(
            &addons,
            &categories,
            None,
            BrowseMode::MostDownloaded,
            Some("17"),
            "",
            Some(25),
            false,
        );

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].uid.as_deref(), Some("2"));
        assert_eq!(results[1].uid.as_deref(), Some("1"));
    }

    #[test]
    fn browse_library_filter_hides_libraries_before_limit() {
        let addons = vec![
            test_addon_summary("1", "LibAddonMenu-2.0", "100", 100, "53", "Libraries"),
            test_addon_summary(
                "2",
                "Inventory Helper",
                "10",
                90,
                "20",
                "Bags, Bank, Inventory",
            ),
        ];

        let results = browse_remote_results(
            &addons,
            &[],
            None,
            BrowseMode::MostDownloaded,
            None,
            "",
            Some(1),
            true,
        );

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].uid.as_deref(), Some("2"));
        assert!(!results[0].is_library);
    }

    #[test]
    fn browse_library_filter_shows_selected_libraries_category() {
        let categories = vec![RemoteCategoryDto {
            id: "53".to_owned(),
            name: "Libraries".to_owned(),
            parent_id: None,
        }];
        let addons = vec![
            test_addon_summary("1", "LibAddonMenu-2.0", "100", 100, "53", "Libraries"),
            test_addon_summary(
                "2",
                "Inventory Helper",
                "10",
                90,
                "20",
                "Bags, Bank, Inventory",
            ),
        ];

        let results = browse_remote_results(
            &addons,
            &categories,
            None,
            BrowseMode::MostDownloaded,
            Some("53"),
            "",
            Some(25),
            true,
        );

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].uid.as_deref(), Some("1"));
        assert!(results[0].is_library);
    }

    #[test]
    fn browse_library_filter_shows_exact_library_query() {
        let addons = vec![
            test_addon_summary("1", "LibAddonMenu-2.0", "100", 100, "53", "Libraries"),
            test_addon_summary("2", "LibDialog", "50", 90, "53", "Libraries"),
        ];

        let results = browse_remote_results(
            &addons,
            &[],
            None,
            BrowseMode::MostDownloaded,
            None,
            "LibAddonMenu-2.0",
            None,
            true,
        );

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].uid.as_deref(), Some("1"));
        assert!(results[0].is_library);
    }

    #[test]
    fn remote_addon_match_prefers_stored_remote_uid() {
        let local_addons = vec![test_local_addon("DifferentFolder", true, false)];
        let remote_addons = vec![test_addon_summary(
            "42",
            "Different Remote Name",
            "10",
            100,
            "17",
            "Graphic UI Mods",
        )];
        let mut metadata = InstalledMetadata::default();
        metadata.addons.insert(
            "DifferentFolder".to_owned(),
            InstalledAddonMetadata {
                folder_name: "DifferentFolder".to_owned(),
                remote_uid: Some("42".to_owned()),
                remote_name: Some("Different Remote Name".to_owned()),
                remote_version: Some("1.0.0".to_owned()),
                remote_updated_date: Some(100),
                remote_info_url: None,
                remote_download_url: None,
                file_name: None,
                md5: None,
                installed_at: "2026-01-01T00:00:00Z".to_owned(),
                installed_by: manager_metadata::INSTALLED_BY_REMOTE_INSTALL.to_owned(),
                local_title: None,
                local_version: None,
                source_addon_uid: None,
            },
        );
        let state = test_remote_local_state(local_addons, metadata, &remote_addons);

        let result = remote_addon_match("42", &remote_addons, &state).expect("installed match");

        assert_eq!(result.local.folder_name, "DifferentFolder");
        assert_eq!(
            result.remote.as_ref().map(|remote| remote.reason.as_str()),
            Some("metadata-remote-uid")
        );
        assert_eq!(
            result.remote.as_ref().and_then(|remote| remote.name.as_deref()),
            Some("Different Remote Name")
        );
    }

    #[test]
    fn imported_metadata_prevents_ambiguous_installed_match() {
        let local_addons = vec![test_local_addon("Foo", true, false)];
        let remote_addons = vec![
            test_addon_summary_with_version(
                "1",
                "Foo",
                "1.0.0",
                "10",
                100,
                "17",
                "Graphic UI Mods",
                "",
            ),
            test_addon_summary_with_version(
                "2",
                "Foo",
                "1.0.0",
                "10",
                100,
                "17",
                "Graphic UI Mods",
                "",
            ),
        ];
        let mut metadata = InstalledMetadata::default();
        metadata.addons.insert(
            "Foo".to_owned(),
            InstalledAddonMetadata {
                folder_name: "Foo".to_owned(),
                remote_uid: Some("2".to_owned()),
                remote_name: Some("Foo".to_owned()),
                remote_version: Some("1.0.0".to_owned()),
                remote_updated_date: Some(100),
                remote_info_url: Some("https://www.esoui.com/downloads/info2.html".to_owned()),
                remote_download_url: None,
                file_name: None,
                md5: None,
                installed_at: "1".to_owned(),
                installed_by: manager_metadata::INSTALLED_BY_IMPORTED_CURRENT.to_owned(),
                local_title: Some("Foo".to_owned()),
                local_version: Some("1.0.0".to_owned()),
                source_addon_uid: None,
            },
        );

        let matches =
            match_remote::match_installed_addons_with_metadata(&local_addons, &remote_addons, &metadata);

        assert_ne!(matches[0].status, match_remote::MatchStatus::Ambiguous);
        assert_eq!(
            matches[0].remote.as_ref().and_then(|remote| remote.uid.as_deref()),
            Some("2")
        );
        assert_eq!(
            update_confidence_for_match(&matches[0], &metadata).confidence,
            "current"
        );
    }

    #[test]
    fn installed_details_lookup_uses_imported_metadata_uid() {
        let local_addons = vec![test_local_addon("Foo", true, false)];
        let remote_addons = vec![
            test_addon_summary_with_version(
                "1",
                "Foo",
                "1.0.0",
                "10",
                100,
                "17",
                "Graphic UI Mods",
                "",
            ),
            test_addon_summary_with_version(
                "2",
                "Foo",
                "1.0.0",
                "10",
                100,
                "17",
                "Graphic UI Mods",
                "",
            ),
        ];
        let mut metadata = InstalledMetadata::default();
        metadata.addons.insert(
            "Foo".to_owned(),
            InstalledAddonMetadata {
                folder_name: "Foo".to_owned(),
                remote_uid: Some("2".to_owned()),
                installed_at: "1".to_owned(),
                installed_by: manager_metadata::INSTALLED_BY_IMPORTED_CURRENT.to_owned(),
                ..InstalledAddonMetadata::default()
            },
        );
        let state = test_remote_local_state(local_addons, metadata, &remote_addons);

        let result = remote_addon_match("2", &remote_addons, &state).expect("installed match");

        assert_eq!(result.local.folder_name, "Foo");
        assert_eq!(
            result.remote.as_ref().and_then(|remote| remote.uid.as_deref()),
            Some("2")
        );
        assert_eq!(
            result.remote.as_ref().map(|remote| remote.reason.as_str()),
            Some("metadata-remote-uid")
        );
    }

    #[test]
    fn newer_remote_version_after_import_is_reliable_update() {
        let dir = tempfile::tempdir().unwrap();
        let addons_dir = dir.path().join("AddOns");
        fs::create_dir_all(&addons_dir).unwrap();
        let local_addons = vec![test_local_addon("ExampleAddon", true, false)];
        let imported_matches = vec![MatchResult {
            local: local_addons[0].clone(),
            status: match_remote::MatchStatus::PossibleUpdate,
            remote: Some(test_remote_candidate("42", "2.0.0")),
            candidates: Vec::new(),
            debug_candidates: Vec::new(),
        }];
        import_existing_matches_as_current(&addons_dir, &imported_matches).unwrap();
        let metadata = load_installed_metadata(&addons_dir);
        let remote_addons = vec![test_addon_summary_with_version(
            "42",
            "Example Addon",
            "3.0.0",
            "10",
            1_800_000_000,
            "17",
            "Graphic UI Mods",
            "",
        )];

        let matches =
            match_remote::match_installed_addons_with_metadata(&local_addons, &remote_addons, &metadata);

        assert_eq!(
            matches[0].remote.as_ref().and_then(|remote| remote.uid.as_deref()),
            Some("42")
        );
        assert_eq!(
            update_confidence_for_match(&matches[0], &metadata).confidence,
            "reliable-update"
        );
    }

    #[test]
    fn remote_addon_match_falls_back_to_manifest_title_and_folder() {
        let local_addons = vec![test_local_addon("HARDCORE", true, false)];
        let remote_addons = vec![test_addon_summary(
            "9001",
            "HARDCORE",
            "10",
            100,
            "25",
            "Combat Mods",
        )];
        let state =
            test_remote_local_state(local_addons, InstalledMetadata::default(), &remote_addons);

        let result = remote_addon_match("9001", &remote_addons, &state).expect("installed match");

        assert_eq!(result.local.folder_name, "HARDCORE");
        assert_eq!(
            result
                .remote
                .as_ref()
                .and_then(|remote| remote.uid.as_deref()),
            Some("9001")
        );
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
            saved_variables: Vec::new(),
            saved_variables_per_character: Vec::new(),
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
            file_info_url: Some(format!("https://www.esoui.com/downloads/info{uid}.html")),
            summary: None,
            directories: vec!["ExampleAddon".to_owned()],
            category_id: None,
            category_name: None,
            downloads: None,
            monthly_downloads: None,
            image_urls: Vec::new(),
            thumbnail_urls: Vec::new(),
            tier: 1,
            score: 100,
            reason: "test".to_owned(),
        }
    }

    fn test_addon_summary(
        uid: &str,
        name: &str,
        downloads: &str,
        date: i64,
        category_id: &str,
        category_name: &str,
    ) -> AddonSummary {
        test_addon_summary_with_version(
            uid,
            name,
            "1.0.0",
            downloads,
            date,
            category_id,
            category_name,
            "",
        )
    }

    fn test_addon_summary_with_summary(
        uid: &str,
        name: &str,
        downloads: &str,
        date: i64,
        category_id: &str,
        category_name: &str,
        summary: &str,
    ) -> AddonSummary {
        test_addon_summary_with_version(
            uid,
            name,
            "1.0.0",
            downloads,
            date,
            category_id,
            category_name,
            summary,
        )
    }

    fn test_addon_summary_with_version(
        uid: &str,
        name: &str,
        version: &str,
        downloads: &str,
        date: i64,
        category_id: &str,
        category_name: &str,
        summary: &str,
    ) -> AddonSummary {
        serde_json::from_value(serde_json::json!({
            "UID": uid,
            "UIName": name,
            "UIAuthorName": "Author",
            "UIVersion": version,
            "UISummary": summary,
            "UIDate": date,
            "UIFileInfoURL": format!("https://www.esoui.com/downloads/info{uid}.html"),
            "UIDownloadTotal": downloads,
            "UICATID": category_id,
            "UICATTitle": category_name
        }))
        .expect("valid addon summary")
    }

    fn test_remote_local_state(
        local_addons: Vec<LocalAddon>,
        metadata: InstalledMetadata,
        remote_addons: &[AddonSummary],
    ) -> RemoteLocalState {
        let matches =
            match_remote::match_installed_addons_with_metadata(&local_addons, remote_addons, &metadata);
        RemoteLocalState {
            local_addons,
            metadata,
            matches,
        }
    }

    fn write_test_addon(addons_dir: &Path, folder_name: &str) {
        let folder = addons_dir.join(folder_name);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            folder.join(format!("{folder_name}.txt")),
            "## Title: Example Addon\n## Version: 1\n",
        )
        .unwrap();
    }
}
