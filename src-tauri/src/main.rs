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
use eso_addon_manager::install::zip_safety;
use eso_addon_manager::local::match_remote::{self, MatchResult, RemoteCandidate};
use eso_addon_manager::local::update_plan::{self, PlannedAddonAction};
use eso_addon_manager::local::{self, AddonPathCandidate, LocalAddon};
use serde::Serialize;

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

struct PreparedRemoteInstall {
    details: AddonDetails,
    plan: InstallPlan,
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
    let backup_dir = backup_dir
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from);
    let download_dir = download_dir
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from);
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
    let backup_dir = backup_dir
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from);
    let download_dir = download_dir
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from);
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
            get_installed_addons,
            search_remote_addons,
            get_remote_addon_details,
            check_addons,
            plan_updates,
            plan_remote_install,
            install_remote_addon,
            plan_single_update,
            apply_single_update
        ])
        .run(tauri::generate_context!())
        .expect("error while running Scribe Addon Manager");
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
