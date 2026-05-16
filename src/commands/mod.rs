use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use serde::Serialize;
use serde_json::{json, Value};
use tokio::fs;

use crate::api::models::{AddonDetails, AddonSummary, ApiFeeds};
use crate::api::ApiClient;
use crate::error::AppError;
use crate::install::apply::{self, InstallResult};
use crate::install::dependencies::{self, DependencyPlan, DependencyStatus, RemoteAddonRef};
use crate::install::dependency_graph::{DependencyManifestSource, DEFAULT_MAX_DEPENDENCY_DEPTH};
use crate::install::plan::{self, InstallPlan, InstallPlanAction};
use crate::install::remote;
use crate::install::update;
use crate::install::update_all as update_all_core;
use crate::install::zip_safety::{self, ExtractedZip, ZipInspection};
use crate::local::match_remote::{self, MatchResult};
use crate::local::metadata as manager_metadata;
use crate::local::update_plan::{self, PlannedActionKind, PlannedAddonAction, UpdatePlan};
use crate::local::{self, LocalAddon};

#[derive(Debug, Parser)]
#[command(name = "eso-addon-manager")]
#[command(about = "Unofficial CLI-first Elder Scrolls Online addon manager")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

struct PreparedRemoteInstall {
    details: AddonDetails,
    download_path: PathBuf,
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

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Fetch global and ESO game configuration and print discovered feeds.
    FetchConfig {
        #[arg(long)]
        json: bool,
    },

    /// Fetch the ESO FileList feed and print the first 25 addons.
    List {
        #[arg(long)]
        json: bool,
    },

    /// Search addon metadata from the ESO FileList feed.
    Search {
        query: String,

        #[arg(long, default_value_t = 25)]
        limit: usize,

        #[arg(long)]
        json: bool,
    },

    /// Fetch details for a single addon id.
    Details {
        addon_id: String,

        #[arg(long)]
        json: bool,
    },

    /// Download an addon ZIP without extracting it.
    Download {
        addon_id: String,

        #[arg(long)]
        output: PathBuf,
    },

    /// Print candidate ESO AddOns directories.
    LocalPaths {
        #[arg(long)]
        json: bool,
    },

    /// Scan a local ESO AddOns directory and print detected addons.
    Installed {
        #[arg(long)]
        path: Option<PathBuf>,

        #[arg(long)]
        json: bool,
    },

    /// Compare installed local addons with the remote ESOUI FileList feed.
    Check {
        #[arg(long)]
        path: Option<PathBuf>,

        #[arg(long)]
        refresh: bool,

        #[arg(long)]
        limit: Option<usize>,

        #[arg(long)]
        verbose: bool,

        #[arg(long)]
        json: bool,
    },

    /// Print a read-only dry-run update plan.
    PlanUpdate {
        #[arg(long)]
        path: Option<PathBuf>,

        #[arg(long)]
        refresh: bool,

        #[arg(long)]
        limit: Option<usize>,

        #[arg(long)]
        include_unknown: bool,

        #[arg(long)]
        json: bool,
    },

    /// Validate and inspect a local addon ZIP without extracting it.
    InspectZip {
        zip_path: PathBuf,

        #[arg(long)]
        json: bool,
    },

    /// Validate and extract a local addon ZIP into a temporary directory only.
    ExtractTemp { zip_path: PathBuf },

    /// Print a dry-run plan for installing a ZIP into an ESO AddOns directory.
    PlanInstall {
        zip_path: PathBuf,

        #[arg(long)]
        path: Option<PathBuf>,

        #[arg(long)]
        json: bool,
    },

    /// Install addon folders from a validated ZIP, backing up replacements first.
    InstallZip {
        zip_path: PathBuf,

        #[arg(long)]
        path: Option<PathBuf>,

        #[arg(long)]
        yes: bool,

        #[arg(long)]
        backup_dir: Option<PathBuf>,

        #[arg(long)]
        json: bool,
    },

    /// Download and install an addon from ESOUI by MMOUI addon id.
    Install {
        addon_id: String,

        #[arg(long)]
        path: Option<PathBuf>,

        #[arg(long)]
        yes: bool,

        #[arg(long)]
        backup_dir: Option<PathBuf>,

        #[arg(long)]
        keep_download: bool,

        #[arg(long)]
        download_dir: Option<PathBuf>,

        #[arg(long)]
        json: bool,
    },

    /// Update exactly one installed addon by local folder name or remote UID.
    Update {
        local_folder_or_uid: String,

        #[arg(long)]
        path: Option<PathBuf>,

        #[arg(long)]
        yes: bool,

        #[arg(long)]
        backup_dir: Option<PathBuf>,

        #[arg(long)]
        keep_download: bool,

        #[arg(long)]
        download_dir: Option<PathBuf>,

        #[arg(long)]
        force: bool,

        #[arg(long)]
        json: bool,
    },

    /// Update all installed addons with reliable update candidates.
    UpdateAll {
        #[arg(long)]
        path: Option<PathBuf>,

        #[arg(long)]
        refresh: bool,

        #[arg(long)]
        yes: bool,

        #[arg(long)]
        backup_dir: Option<PathBuf>,

        #[arg(long)]
        keep_download: bool,

        #[arg(long)]
        download_dir: Option<PathBuf>,

        #[arg(long)]
        include_unknown: bool,

        #[arg(long)]
        limit: Option<usize>,

        #[arg(long)]
        json: bool,
    },
}

pub async fn fetch_config(client: &ApiClient, json_output: bool) -> Result<()> {
    let config = client.eso_game_config().await?;
    if json_output {
        return print_json(&config);
    }
    println!(
        "{}",
        config
            .game_name
            .as_deref()
            .unwrap_or("Elder Scrolls Online")
    );
    if let Some(title) = config.website_title.as_deref() {
        println!("Website: {title}");
    }
    if let Some(url) = config.website_url.as_deref() {
        println!("URL: {url}");
    }
    println!();
    print_feeds(config.api_feeds.as_ref())?;
    Ok(())
}

pub async fn list(client: &ApiClient, json_output: bool) -> Result<()> {
    let addons = client.eso_file_list().await?;
    if json_output {
        return print_json_value(json!({
            "addons": addons.iter().take(25).collect::<Vec<_>>(),
            "limit": 25,
            "total_remote": addons.len(),
        }));
    }
    print_addons(addons.iter().take(25));
    Ok(())
}

pub async fn search(
    client: &ApiClient,
    query: &str,
    limit: usize,
    json_output: bool,
) -> Result<()> {
    let needle = query.to_lowercase();
    let addons = client.eso_file_list().await?;
    let matches = addons
        .iter()
        .filter(|addon| addon.searchable_text().contains(&needle))
        .take(limit)
        .collect::<Vec<_>>();

    if json_output {
        return print_json_value(json!({
            "query": query,
            "limit": limit,
            "addons": matches,
        }));
    }

    print_addons(matches.into_iter());
    Ok(())
}

pub async fn details(client: &ApiClient, addon_id: &str, json_output: bool) -> Result<()> {
    let details = client.eso_file_details(addon_id).await?;
    if json_output {
        return print_json(&details);
    }
    print_details(&details);
    Ok(())
}

pub async fn download(client: &ApiClient, addon_id: &str, output: &Path) -> Result<()> {
    let details = client.eso_file_details_fresh(addon_id).await?;
    let download_url = details
        .download_url
        .as_deref()
        .ok_or_else(|| anyhow!("addon {addon_id} has no UIDownload URL"))?;
    let file_name = details
        .file_name
        .as_deref()
        .and_then(|name| Path::new(name).file_name())
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("addon.zip");

    let final_path = resolve_output_path(output, file_name).await?;
    if let Some(parent) = final_path.parent() {
        fs::create_dir_all(parent)
            .await
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let bytes = client.download_bytes(download_url).await?;
    fs::write(&final_path, &bytes)
        .await
        .with_context(|| format!("failed to write {}", final_path.display()))?;

    if let Some(expected) = details.md5.as_deref().filter(|value| !value.is_empty()) {
        let actual = format!("{:x}", md5::compute(&bytes));
        if !expected.eq_ignore_ascii_case(&actual) {
            return Err(AppError::Md5Mismatch {
                path: final_path.display().to_string(),
                expected: expected.to_owned(),
                actual,
            }
            .into());
        }
    }

    println!("{}", final_path.display());
    Ok(())
}

pub fn local_paths(json_output: bool) -> Result<()> {
    let candidates = local::addon_path_candidates();

    if json_output {
        return print_json_value(json!({
            "candidates": candidates.iter().map(addon_path_candidate_json).collect::<Vec<_>>(),
        }));
    }

    println!("{:<8} {:<8} Path", "Exists", "Addons");
    println!("{}", "-".repeat(96));
    for candidate in candidates {
        println!(
            "{:<8} {:<8} {}",
            yes_no(candidate.exists),
            yes_no(candidate.contains_addons),
            candidate.path.display()
        );
    }

    Ok(())
}

pub fn installed(path: Option<&Path>, json_output: bool) -> Result<()> {
    let path = match path {
        Some(path) => path.to_path_buf(),
        None => local::detect_best_addons_dir()
            .ok_or_else(|| anyhow!("could not auto-detect an ESO AddOns directory"))?,
    };

    let addons = local::scan_addons_dir(&path)
        .with_context(|| format!("failed to scan AddOns directory {}", path.display()))?;

    if json_output {
        return print_json_value(json!({
            "addons_dir": path_string(&path),
            "addons": addons.iter().map(local_addon_json).collect::<Vec<_>>(),
        }));
    }

    println!("AddOns directory: {}", path.display());
    println!();
    print_installed_addons(&addons);
    Ok(())
}

pub async fn check(
    client: &ApiClient,
    path: Option<&Path>,
    refresh: bool,
    limit: Option<usize>,
    verbose: bool,
    json_output: bool,
) -> Result<()> {
    if refresh {
        tracing::debug!("refresh requested; fetching live remote FileList");
    }

    let path = match path {
        Some(path) => path.to_path_buf(),
        None => local::detect_best_addons_dir()
            .ok_or_else(|| anyhow!("could not auto-detect an ESO AddOns directory"))?,
    };

    let local_addons = local::scan_addons_dir(&path)
        .with_context(|| format!("failed to scan AddOns directory {}", path.display()))?;
    let remote_addons = if refresh {
        client.eso_file_list_refresh().await?
    } else {
        client.eso_file_list().await?
    };
    let metadata = manager_metadata::load_installed_metadata_or_default(&path);
    let mut results = match_remote::match_installed_addons_with_metadata(
        &local_addons,
        &remote_addons,
        &metadata,
    );

    results.sort_by_key(|result| result.local.folder_name.to_lowercase());
    if let Some(limit) = limit {
        results.truncate(limit);
    }

    if json_output {
        return print_json_value(json!({
            "addons_dir": path_string(&path),
            "remote_addons_loaded": remote_addons.len(),
            "limit": limit,
            "verbose": verbose,
            "matches": results.iter().map(|result| match_result_json(result, verbose)).collect::<Vec<_>>(),
        }));
    }

    println!("AddOns directory: {}", path.display());
    println!("Remote addons loaded: {}", remote_addons.len());
    println!();
    print_check_results(&results, verbose);
    Ok(())
}

pub async fn plan_update(
    client: &ApiClient,
    path: Option<&Path>,
    refresh: bool,
    limit: Option<usize>,
    include_unknown: bool,
    json_output: bool,
) -> Result<()> {
    if refresh {
        tracing::debug!("refresh requested; fetching live remote FileList");
    }

    let path = match path {
        Some(path) => path.to_path_buf(),
        None => local::detect_best_addons_dir()
            .ok_or_else(|| anyhow!("could not auto-detect an ESO AddOns directory"))?,
    };

    let local_addons = local::scan_addons_dir(&path)
        .with_context(|| format!("failed to scan AddOns directory {}", path.display()))?;
    let remote_addons = if refresh {
        client.eso_file_list_refresh().await?
    } else {
        client.eso_file_list().await?
    };
    let metadata = manager_metadata::load_installed_metadata_or_default(&path);
    let mut matches = match_remote::match_installed_addons_with_metadata(
        &local_addons,
        &remote_addons,
        &metadata,
    );
    matches.sort_by_key(|result| result.local.folder_name.to_lowercase());

    if let Some(limit) = limit {
        matches.truncate(limit);
    }

    let detail_uids = update_plan::detail_request_uids_for(&matches, include_unknown);
    let mut plan = update_plan::build_update_plan(&matches, include_unknown);
    for uid in detail_uids {
        let details = if refresh {
            client.eso_file_details_fresh(&uid).await?
        } else {
            client.eso_file_details(&uid).await?
        };
        plan.attach_details(&uid, details);
    }

    if json_output {
        return print_json_value(json!({
            "addons_dir": path_string(&path),
            "remote_addons_loaded": remote_addons.len(),
            "dry_run": true,
            "applied": false,
            "include_unknown": include_unknown,
            "limit": limit,
            "plan": update_plan_json(&plan),
        }));
    }

    println!("AddOns directory: {}", path.display());
    println!("Remote addons loaded: {}", remote_addons.len());
    println!("Dry run only: no files will be downloaded, extracted, modified, or deleted.");
    println!();
    print_update_plan(&plan);
    Ok(())
}

pub async fn update_all(
    client: &ApiClient,
    path: Option<&Path>,
    refresh: bool,
    yes: bool,
    backup_dir: Option<&Path>,
    keep_download: bool,
    download_dir: Option<&Path>,
    include_unknown: bool,
    limit: Option<usize>,
    json_output: bool,
) -> Result<()> {
    if refresh {
        tracing::debug!("refresh requested; fetching live remote FileList");
    }

    let addons_dir = match path {
        Some(path) => path.to_path_buf(),
        None => local::detect_best_addons_dir()
            .ok_or_else(|| anyhow!("could not auto-detect an ESO AddOns directory"))?,
    };

    let mut local_addons = local::scan_addons_dir(&addons_dir)
        .with_context(|| format!("failed to scan AddOns directory {}", addons_dir.display()))?;
    let remote_addons = if refresh {
        client.eso_file_list_refresh().await?
    } else {
        client.eso_file_list().await?
    };
    let metadata = manager_metadata::load_installed_metadata_or_default(&addons_dir);
    let mut matches = match_remote::match_installed_addons_with_metadata(
        &local_addons,
        &remote_addons,
        &metadata,
    );
    matches.sort_by_key(|result| result.local.folder_name.to_lowercase());

    if let Some(limit) = limit {
        matches.truncate(limit);
    }

    let plan = update_all_core::build_update_all_plan(&matches, include_unknown);

    if json_output && !yes {
        return print_json_value(json!({
            "addons_dir": path_string(&addons_dir),
            "remote_addons_loaded": remote_addons.len(),
            "dry_run": true,
            "applied": false,
            "include_unknown": include_unknown,
            "limit": limit,
            "plan": update_all_plan_json(&plan),
            "results": [],
        }));
    }

    if !json_output {
        println!("AddOns directory: {}", addons_dir.display());
        println!("Remote addons loaded: {}", remote_addons.len());
        println!("Dry run by default: no files will be downloaded, extracted, modified, or deleted without --yes.");
        println!();
        print_update_all_plan(&plan);
    }

    if !yes {
        if !json_output {
            println!();
            println!("No changes made. Re-run with --yes to update all planned addons.");
        }
        return Ok(());
    }

    if plan.targets.is_empty() {
        if json_output {
            return print_json_value(json!({
                "addons_dir": path_string(&addons_dir),
                "remote_addons_loaded": remote_addons.len(),
                "dry_run": false,
                "applied": false,
                "include_unknown": include_unknown,
                "limit": limit,
                "plan": update_all_plan_json(&plan),
                "results": [],
            }));
        }
        println!();
        println!("No planned addons to update.");
        return Ok(());
    }

    let mut applied_results = Vec::new();

    if !json_output {
        println!();
        println!("Applying planned updates sequentially:");
    }

    for target in &plan.targets {
        let remote_uid = target.remote_uid.as_deref().ok_or_else(|| {
            anyhow!(
                "planned addon {} has no clean remote UID",
                target.local_folder
            )
        })?;

        if !json_output {
            println!();
            println!(
                "Updating {} -> {} ({})",
                target.local_folder,
                target.remote_name.as_deref().unwrap_or("-"),
                remote_uid
            );
        }

        let result = match update_all_one(
            client,
            target,
            remote_uid,
            &addons_dir,
            &local_addons,
            backup_dir,
            keep_download,
            download_dir,
            json_output,
        )
        .await
        {
            Ok(result) => result,
            Err(error) => {
                if !json_output {
                    println!("failed: {} ({})", target.local_folder, error);
                }
                return Err(error)
                    .with_context(|| format!("failed to update {}", target.local_folder));
            }
        };

        if json_output {
            applied_results.push(json!({
                "local_folder": target.local_folder,
                "remote_uid": target.remote_uid,
                "remote_name": target.remote_name,
                "status": "updated",
                "result": install_result_json(&result),
            }));
        } else {
            print_update_all_item_result(target, &result);
        }
        local_addons = local::scan_addons_dir(&addons_dir).with_context(|| {
            format!(
                "failed to rescan AddOns directory {} after updating {}",
                addons_dir.display(),
                target.local_folder
            )
        })?;
    }

    if json_output {
        return print_json_value(json!({
            "addons_dir": path_string(&addons_dir),
            "remote_addons_loaded": remote_addons.len(),
            "dry_run": false,
            "applied": !applied_results.is_empty(),
            "include_unknown": include_unknown,
            "limit": limit,
            "plan": update_all_plan_json(&plan),
            "results": applied_results,
        }));
    }

    println!();
    println!("Update-all complete: {} updated.", plan.targets.len());
    Ok(())
}

async fn update_all_one(
    client: &ApiClient,
    target: &PlannedAddonAction,
    remote_uid: &str,
    addons_dir: &Path,
    local_addons: &[LocalAddon],
    backup_dir: Option<&Path>,
    keep_download: bool,
    download_dir: Option<&Path>,
    quiet: bool,
) -> Result<InstallResult> {
    let details = client.eso_file_details_fresh(remote_uid).await?;
    let prepared = prepare_remote_install_plan(
        client,
        &details,
        remote_uid,
        addons_dir,
        local_addons,
        keep_download,
        download_dir,
        quiet,
    )
    .await?;
    validate_single_update_plan(&prepared.main_plan, &target.local_folder)?;

    Ok(apply_prepared_install(
        &prepared,
        backup_dir,
        manager_metadata::INSTALLED_BY_REMOTE_UPDATE,
    )?)
}

pub fn inspect_zip(zip_path: &Path, json_output: bool) -> Result<()> {
    let inspection = zip_safety::inspect_zip(zip_path)
        .with_context(|| format!("failed to inspect ZIP {}", zip_path.display()))?;
    if json_output {
        return print_json_value(zip_inspection_json(&inspection));
    }
    print_zip_inspection(&inspection);
    Ok(())
}

pub fn extract_temp(zip_path: &Path) -> Result<()> {
    let extracted = zip_safety::extract_zip_to_temp(zip_path)
        .with_context(|| format!("failed to extract ZIP {}", zip_path.display()))?;
    print_extracted_zip(&extracted);
    Ok(())
}

pub fn plan_install(zip_path: &Path, path: Option<&Path>, json_output: bool) -> Result<()> {
    let addons_dir = match path {
        Some(path) => path.to_path_buf(),
        None => local::detect_best_addons_dir()
            .ok_or_else(|| anyhow!("could not auto-detect an ESO AddOns directory"))?,
    };
    let installed_addons = scan_installed_addons_for_install(&addons_dir)?;
    let extracted = zip_safety::extract_zip_to_temp(zip_path)
        .with_context(|| format!("failed to validate and extract ZIP {}", zip_path.display()))?;
    let plan = plan::plan_install(&extracted, &addons_dir, &installed_addons)?;

    if json_output {
        return print_json_value(json!({
            "zip_path": path_string(zip_path),
            "addons_dir": path_string(&addons_dir),
            "dry_run": true,
            "applied": false,
            "plan": install_plan_json(&plan),
        }));
    }

    print_install_plan(&plan, true);
    Ok(())
}

pub fn install_zip(
    zip_path: &Path,
    path: Option<&Path>,
    yes: bool,
    backup_dir: Option<&Path>,
    json_output: bool,
) -> Result<()> {
    let addons_dir = match path {
        Some(path) => path.to_path_buf(),
        None => local::detect_best_addons_dir()
            .ok_or_else(|| anyhow!("could not auto-detect an ESO AddOns directory"))?,
    };
    let installed_addons = scan_installed_addons_for_install(&addons_dir)?;
    let extracted = zip_safety::extract_zip_to_temp(zip_path)
        .with_context(|| format!("failed to validate and extract ZIP {}", zip_path.display()))?;
    let plan = plan::plan_install(&extracted, &addons_dir, &installed_addons)?;

    if !yes {
        if json_output {
            return print_json_value(json!({
                "zip_path": path_string(zip_path),
                "addons_dir": path_string(&addons_dir),
                "dry_run": true,
                "applied": false,
                "plan": install_plan_json(&plan),
                "result": Value::Null,
            }));
        }
        print_install_plan(&plan, true);
        println!();
        println!("No changes made. Re-run with --yes to install.");
        return Ok(());
    }

    if json_output {
        let result = apply::apply_install_plan(&plan, backup_dir)?;
        manager_metadata::record_zip_install_metadata(&addons_dir, &plan, &result)?;
        return print_json_value(json!({
            "zip_path": path_string(zip_path),
            "addons_dir": path_string(&addons_dir),
            "dry_run": false,
            "applied": install_result_applied(&result),
            "plan": install_plan_json(&plan),
            "result": install_result_json(&result),
        }));
    }

    print_install_plan(&plan, false);
    let result = apply::apply_install_plan(&plan, backup_dir)?;
    manager_metadata::record_zip_install_metadata(&addons_dir, &plan, &result)?;
    println!();
    print_install_result(&result);
    Ok(())
}

pub async fn install_remote(
    client: &ApiClient,
    addon_id: &str,
    path: Option<&Path>,
    yes: bool,
    backup_dir: Option<&Path>,
    keep_download: bool,
    download_dir: Option<&Path>,
    json_output: bool,
) -> Result<()> {
    let details = client.eso_file_details_fresh(addon_id).await?;
    if !json_output {
        print_remote_install_metadata(&details);
    }

    let addons_dir = match path {
        Some(path) => path.to_path_buf(),
        None => local::detect_best_addons_dir()
            .ok_or_else(|| anyhow!("could not auto-detect an ESO AddOns directory"))?,
    };
    let installed_addons = scan_installed_addons_for_install(&addons_dir)?;
    let prepared = prepare_remote_install_plan(
        client,
        &details,
        addon_id,
        &addons_dir,
        &installed_addons,
        keep_download,
        download_dir,
        json_output,
    )
    .await?;

    if !yes {
        if json_output {
            return print_json_value(json!({
                "addon_id": addon_id,
                "remote": prepared.details,
                "addons_dir": path_string(&addons_dir),
                "download_path": path_string(&prepared.download_path),
                "dry_run": true,
                "applied": false,
                "plan": install_plan_json(&prepared.main_plan),
                "dependency_plan": dependency_plan_json(&prepared),
                "result": Value::Null,
            }));
        }
        print_dependency_plan(&prepared.dependency_plan);
        println!();
        print_install_plan(&prepared.main_plan, true);
        println!();
        println!("No changes made. Re-run with --yes to install.");
        return Ok(());
    }

    if json_output {
        let result = apply_prepared_install(
            &prepared,
            backup_dir,
            manager_metadata::INSTALLED_BY_REMOTE_INSTALL,
        )?;
        return print_json_value(json!({
            "addon_id": addon_id,
            "remote": prepared.details,
            "addons_dir": path_string(&addons_dir),
            "download_path": path_string(&prepared.download_path),
            "dry_run": false,
            "applied": install_result_applied(&result),
            "plan": install_plan_json(&prepared.main_plan),
            "dependency_plan": dependency_plan_json(&prepared),
            "result": install_result_json(&result),
        }));
    }

    print_dependency_plan(&prepared.dependency_plan);
    println!();
    print_install_plan(&prepared.main_plan, false);
    let result = apply_prepared_install(
        &prepared,
        backup_dir,
        manager_metadata::INSTALLED_BY_REMOTE_INSTALL,
    )?;
    println!();
    print_install_result(&result);
    Ok(())
}

pub async fn update_one(
    client: &ApiClient,
    request: &str,
    path: Option<&Path>,
    yes: bool,
    backup_dir: Option<&Path>,
    keep_download: bool,
    download_dir: Option<&Path>,
    force: bool,
    json_output: bool,
) -> Result<()> {
    let addons_dir = match path {
        Some(path) => path.to_path_buf(),
        None => local::detect_best_addons_dir()
            .ok_or_else(|| anyhow!("could not auto-detect an ESO AddOns directory"))?,
    };
    let local_addons = local::scan_addons_dir(&addons_dir)
        .with_context(|| format!("failed to scan AddOns directory {}", addons_dir.display()))?;
    let remote_addons = client.eso_file_list().await?;
    let metadata = manager_metadata::load_installed_metadata_or_default(&addons_dir);
    let matches = match_remote::match_installed_addons_with_metadata(
        &local_addons,
        &remote_addons,
        &metadata,
    );
    let selected = update::resolve_update_request(&matches, request)?;
    let decision = update::update_decision(selected, force);

    if !json_output {
        print_update_selection(selected, &decision);
    }

    if !decision.should_install() {
        if json_output {
            return print_json_value(json!({
                "request": request,
                "addons_dir": path_string(&addons_dir),
                "dry_run": true,
                "applied": false,
                "selection": update_selection_json(selected, &decision),
                "plan": Value::Null,
                "result": Value::Null,
            }));
        }
        match decision {
            update::UpdateDecision::SkippedCurrent
            | update::UpdateDecision::SkippedLocalNewer
            | update::UpdateDecision::SkippedUnknownUseForce => {
                println!("Use --force to reinstall despite unknown/current/local-newer version.");
            }
            update::UpdateDecision::SkippedNoMatch => {
                println!("No remote match found; update cannot continue.");
            }
            update::UpdateDecision::SkippedAmbiguous => {
                println!("Remote match is ambiguous; update cannot continue.");
            }
            _ => {}
        }
        println!("No changes made.");
        return Ok(());
    }

    let remote_uid = selected
        .remote
        .as_ref()
        .and_then(|remote| remote.uid.as_deref())
        .ok_or_else(|| anyhow!("selected addon has no clean remote UID"))?;
    let details = client.eso_file_details_fresh(remote_uid).await?;
    if !json_output {
        print_remote_install_metadata(&details);
    }

    let prepared = prepare_remote_install_plan(
        client,
        &details,
        remote_uid,
        &addons_dir,
        &local_addons,
        keep_download,
        download_dir,
        json_output,
    )
    .await?;
    validate_single_update_plan(&prepared.main_plan, &selected.local.folder_name)?;

    if !yes {
        if json_output {
            return print_json_value(json!({
                "request": request,
                "addons_dir": path_string(&addons_dir),
                "dry_run": true,
                "applied": false,
                "selection": update_selection_json(selected, &decision),
                "remote": prepared.details,
                "plan": install_plan_json(&prepared.main_plan),
                "dependency_plan": dependency_plan_json(&prepared),
                "result": Value::Null,
            }));
        }
        print_dependency_plan(&prepared.dependency_plan);
        println!();
        print_install_plan(&prepared.main_plan, true);
        println!();
        println!("No changes made. Re-run with --yes to update.");
        return Ok(());
    }

    if json_output {
        let result = apply_prepared_install(
            &prepared,
            backup_dir,
            manager_metadata::INSTALLED_BY_REMOTE_UPDATE,
        )?;
        return print_json_value(json!({
            "request": request,
            "addons_dir": path_string(&addons_dir),
            "dry_run": false,
            "applied": install_result_applied(&result),
            "selection": update_selection_json(selected, &decision),
            "remote": prepared.details,
            "plan": install_plan_json(&prepared.main_plan),
            "dependency_plan": dependency_plan_json(&prepared),
            "result": install_result_json(&result),
        }));
    }

    print_dependency_plan(&prepared.dependency_plan);
    println!();
    print_install_plan(&prepared.main_plan, false);
    let result = apply_prepared_install(
        &prepared,
        backup_dir,
        manager_metadata::INSTALLED_BY_REMOTE_UPDATE,
    )?;
    println!();
    print_install_result(&result);
    Ok(())
}

async fn prepare_remote_install_plan(
    client: &ApiClient,
    details: &AddonDetails,
    addon_id: &str,
    addons_dir: &Path,
    installed_addons: &[LocalAddon],
    keep_download: bool,
    download_dir: Option<&Path>,
    quiet: bool,
) -> Result<PreparedRemoteInstall> {
    let remote_addons = client.eso_file_list().await?;
    let installed_metadata = manager_metadata::load_installed_metadata_or_default(addons_dir);
    let installed_remotes = manager_metadata::installed_remote_addons(&installed_metadata);
    let (main_plan, extracted, download_path) = prepare_remote_package(
        client,
        details,
        addon_id,
        addons_dir,
        installed_addons,
        keep_download,
        download_dir,
        quiet,
    )
    .await?;
    let main_source = DependencyManifestSource::from_extracted(&extracted);
    let main_addon = RemoteAddonRef {
        uid: addon_id.to_owned(),
        name: details.name.clone(),
    };
    let mut remote_sources = BTreeMap::new();
    let mut prepared_dependency_packages = BTreeMap::new();
    let dependency_plan = loop {
        let dependency_plan = dependencies::build_dependency_plan_with_remote_sources(
            main_addon.clone(),
            &main_source,
            installed_addons,
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
            return Err(anyhow!(
                "recursive dependency resolution exceeded {MAX_RECURSIVE_DEPENDENCY_PACKAGES} packages"
            ));
        }

        for remote_uid in dependency_uids {
            if remote_sources.contains_key(&remote_uid) {
                continue;
            }
            let dependency_details = client.eso_file_details_fresh(&remote_uid).await?;
            let (plan, dependency_extracted, _) = prepare_remote_package(
                client,
                &dependency_details,
                &remote_uid,
                addons_dir,
                installed_addons,
                keep_download,
                download_dir,
                quiet,
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
        details: details.clone(),
        download_path,
        main_plan,
        dependency_plan,
        dependency_installs,
    })
}

async fn prepare_remote_package(
    client: &ApiClient,
    details: &AddonDetails,
    addon_id: &str,
    addons_dir: &Path,
    installed_addons: &[LocalAddon],
    keep_download: bool,
    download_dir: Option<&Path>,
    quiet: bool,
) -> Result<(InstallPlan, ExtractedZip, PathBuf)> {
    let download_url = remote::download_url(details)?;
    let file_name = remote::download_file_name(details, addon_id);
    tracing::debug!(
        "downloading remote addon {} from {}",
        addon_id,
        download_url
    );
    let bytes = client.download_bytes(download_url).await?;
    remote::verify_md5(&bytes, details.md5.as_deref())?;

    let temp_file;
    let zip_path = if keep_download {
        let path = remote::keep_download_path(download_dir, &file_name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::write(&path, &bytes)
            .await
            .with_context(|| format!("failed to write {}", path.display()))?;
        if !quiet {
            println!("Saved ZIP: {}", path.display());
        }
        path
    } else {
        temp_file = tempfile::Builder::new()
            .prefix("eso-addon-manager-download-")
            .suffix(".zip")
            .tempfile()?;
        fs::write(temp_file.path(), &bytes)
            .await
            .with_context(|| format!("failed to write {}", temp_file.path().display()))?;
        temp_file.path().to_path_buf()
    };

    let extracted = zip_safety::extract_zip_to_temp(&zip_path)
        .with_context(|| format!("failed to validate and extract ZIP {}", zip_path.display()))?;
    let install_plan = plan::plan_install(&extracted, addons_dir, installed_addons)?;

    Ok((install_plan, extracted, zip_path))
}

fn apply_prepared_install(
    prepared: &PreparedRemoteInstall,
    backup_dir: Option<&Path>,
    installed_by: &str,
) -> Result<InstallResult> {
    if prepared
        .dependency_plan
        .has_unresolved_required_dependencies()
    {
        return Err(anyhow!(
            "some required dependencies could not be resolved safely"
        ));
    }

    let mut aggregate = InstallResult::default();

    for dependency in &prepared.dependency_installs {
        let result = apply::apply_install_plan(&dependency.plan, backup_dir)?;
        manager_metadata::record_remote_install_metadata(
            &dependency.plan.addons_dir,
            &dependency.plan,
            &result,
            manager_metadata::RemoteInstallMetadata {
                details: &dependency.details,
                remote_uid: &dependency.remote_uid,
                installed_by: manager_metadata::INSTALLED_BY_DEPENDENCY_INSTALL,
                source_addon_uid: Some(&prepared.dependency_plan.main_addon.uid),
            },
        )?;
        apply::merge_install_result(&mut aggregate, result);
    }

    let result = apply::apply_install_plan(&prepared.main_plan, backup_dir)?;
    manager_metadata::record_remote_install_metadata(
        &prepared.main_plan.addons_dir,
        &prepared.main_plan,
        &result,
        manager_metadata::RemoteInstallMetadata {
            details: &prepared.details,
            remote_uid: &prepared.dependency_plan.main_addon.uid,
            installed_by,
            source_addon_uid: None,
        },
    )?;
    apply::merge_install_result(&mut aggregate, result);

    Ok(aggregate)
}

fn print_json<T: Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn print_json_value(value: Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

fn path_string(path: &Path) -> String {
    path.display().to_string()
}

fn addon_path_candidate_json(candidate: &local::AddonPathCandidate) -> Value {
    json!({
        "path": path_string(&candidate.path),
        "exists": candidate.exists,
        "contains_addons": candidate.contains_addons,
    })
}

fn local_addon_json(addon: &LocalAddon) -> Value {
    json!({
        "folder_name": addon.folder_name,
        "folder_path": path_string(&addon.folder_path),
        "manifest_path": addon.manifest_path.as_ref().map(|path| path_string(path)),
        "title": addon.title,
        "addon_version": addon.addon_version,
        "version": addon.version,
        "display_version": addon.addon_version.as_deref().or(addon.version.as_deref()),
        "api_versions": addon.api_versions,
        "depends_on": addon.depends_on,
        "optional_depends_on": addon.optional_depends_on,
        "saved_variables": addon.saved_variables,
        "saved_variables_per_character": addon.saved_variables_per_character,
        "is_library": addon.is_library,
        "author": addon.author,
        "description": addon.description,
        "valid_manifest": addon.valid_manifest,
    })
}

fn remote_candidate_json(candidate: &match_remote::RemoteCandidate) -> Value {
    json!({
        "uid": candidate.uid,
        "name": candidate.name,
        "version": candidate.version,
        "updated": candidate.updated,
        "updated_display": candidate.updated.map(format_mmoui_date),
        "tier": candidate.tier,
        "score": candidate.score,
        "reason": candidate.reason,
    })
}

fn match_result_json(result: &MatchResult, verbose: bool) -> Value {
    json!({
        "local": local_addon_json(&result.local),
        "status": result.status.as_str(),
        "remote": result.remote.as_ref().map(remote_candidate_json),
        "candidates": result.candidates.iter().map(remote_candidate_json).collect::<Vec<_>>(),
        "debug_candidates": if verbose {
            result.debug_candidates.iter().map(remote_candidate_json).collect::<Vec<_>>()
        } else {
            Vec::new()
        },
    })
}

fn planned_action_json(action: &PlannedAddonAction) -> Value {
    json!({
        "local_folder": action.local_folder,
        "remote_name": action.remote_name,
        "remote_uid": action.remote_uid,
        "local_version": action.local_version,
        "remote_version": action.remote_version,
        "action": action.kind.as_str(),
        "details": action.details.as_ref().map(|details| {
            json!({
                "file_name": details.file_name,
                "md5": details.md5,
                "download_url": details.download_url,
            })
        }),
    })
}

fn update_plan_json(plan: &UpdatePlan) -> Value {
    let summary = plan.summary();
    json!({
        "actions": plan.actions.iter().map(planned_action_json).collect::<Vec<_>>(),
        "summary": {
            "would_update": summary.would_update,
            "current_skipped": summary.current_skipped,
            "local_newer": summary.local_newer,
            "unknown": summary.unknown,
            "no_match": summary.no_match,
            "ambiguous": summary.ambiguous,
            "libraries": summary.libraries,
        }
    })
}

fn update_all_plan_json(plan: &update_all_core::UpdateAllPlan) -> Value {
    let display_actions = plan
        .display_plan
        .actions
        .iter()
        .map(|action| {
            let mut value = planned_action_json(action);
            if let Some(object) = value.as_object_mut() {
                object.insert(
                    "update_all_action".to_owned(),
                    Value::String(update_all_action_label(action, &plan.targets).to_owned()),
                );
            }
            value
        })
        .collect::<Vec<_>>();

    json!({
        "actions": display_actions,
        "targets": plan.targets.iter().map(planned_action_json).collect::<Vec<_>>(),
        "summary": update_all_summary_json(plan),
    })
}

fn update_all_summary_json(plan: &update_all_core::UpdateAllPlan) -> Value {
    let mut skipped_current = 0;
    let mut skipped_local_newer = 0;
    let mut skipped_unknown = 0;
    let mut skipped_no_match = 0;
    let mut skipped_ambiguous = 0;
    let mut skipped_libraries = 0;

    for action in &plan.display_plan.actions {
        if update_all_targets_contain(&plan.targets, action) {
            continue;
        }

        match action.kind {
            PlannedActionKind::WouldUpdate => {}
            PlannedActionKind::WouldSkipCurrent => skipped_current += 1,
            PlannedActionKind::WouldSkipLocalNewer => skipped_local_newer += 1,
            PlannedActionKind::WouldSkipUnknownVersion => skipped_unknown += 1,
            PlannedActionKind::WouldSkipNoMatch => skipped_no_match += 1,
            PlannedActionKind::WouldSkipAmbiguous => skipped_ambiguous += 1,
            PlannedActionKind::WouldSkipLibrary => skipped_libraries += 1,
        }
    }

    json!({
        "planned_updates": plan.targets.len(),
        "skipped_current": skipped_current,
        "skipped_local_newer": skipped_local_newer,
        "skipped_unknown": skipped_unknown,
        "skipped_no_match": skipped_no_match,
        "skipped_ambiguous": skipped_ambiguous,
        "skipped_libraries": skipped_libraries,
    })
}

fn install_plan_json(plan: &InstallPlan) -> Value {
    json!({
        "addons_dir": path_string(&plan.addons_dir),
        "temp_dir": path_string(&plan.temp_dir),
        "items": plan.items.iter().map(|item| {
            json!({
                "source_folder": item.source_folder,
                "title": item.title,
                "version": item.version,
                "target_folder": item.target_folder.as_ref().map(|path| path_string(path)),
                "action": item.action.as_str(),
            })
        }).collect::<Vec<_>>(),
    })
}

fn dependency_plan_json(prepared: &PreparedRemoteInstall) -> Value {
    json!({
        "main_addon": {
            "uid": prepared.dependency_plan.main_addon.uid,
            "name": prepared.dependency_plan.main_addon.name,
        },
        "required_dependencies": prepared.dependency_plan.required_dependencies.iter().map(dependency_entry_json).collect::<Vec<_>>(),
        "optional_dependencies": prepared.dependency_plan.optional_dependencies.iter().map(dependency_entry_json).collect::<Vec<_>>(),
        "install_items": dependency_install_items_json(prepared),
        "install_order": prepared.dependency_plan.install_order,
    })
}

fn dependency_entry_json(entry: &dependencies::DependencyPlanEntry) -> Value {
    json!({
        "name": entry.name,
        "constraint": entry.constraint,
        "raw": entry.raw,
        "required": entry.required,
        "relation": entry.relation.as_str(),
        "depth": entry.depth,
        "parent": entry.parent,
        "status": entry.status.as_str(),
        "remote_uid": entry.remote_uid,
        "remote_name": entry.remote_name,
        "remote_version": entry.remote_version,
        "installed_folder": entry.installed_folder,
        "installed_title": entry.installed_title,
        "installed_version": entry.installed_version,
        "bundled_folder": entry.bundled_folder,
    })
}

fn dependency_install_items_json(prepared: &PreparedRemoteInstall) -> Vec<Value> {
    let mut items = prepared
        .dependency_installs
        .iter()
        .map(|dependency| {
            json!({
                "role": "required-dependency",
                "name": dependency.details.name.as_deref().unwrap_or(&dependency.remote_uid),
                "remote_uid": dependency.remote_uid,
                "remote_name": dependency.details.name,
                "action": install_plan_action_summary(&dependency.plan),
            })
        })
        .collect::<Vec<_>>();

    items.push(json!({
        "role": "main-addon",
        "name": prepared.details.name.as_deref().unwrap_or(&prepared.dependency_plan.main_addon.uid),
        "remote_uid": prepared.dependency_plan.main_addon.uid,
        "remote_name": prepared.details.name,
        "action": install_plan_action_summary(&prepared.main_plan),
    }));

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

fn install_result_json(result: &InstallResult) -> Value {
    json!({
        "backup_dir": result.backup_dir.as_ref().map(|path| path_string(path)),
        "installed_new": result.installed_new,
        "replaced": result.replaced,
        "skipped": result.skipped,
        "items": result.items.iter().map(|item| {
            json!({
                "source_folder": item.source_folder,
                "target_folder": item.target_folder.as_ref().map(|path| path_string(path)),
                "backup_folder": item.backup_folder.as_ref().map(|path| path_string(path)),
                "action": item.action.as_str(),
                "message": item.message,
            })
        }).collect::<Vec<_>>(),
    })
}

fn install_result_applied(result: &InstallResult) -> bool {
    result.installed_new > 0 || result.replaced > 0
}

fn update_selection_json(result: &MatchResult, decision: &update::UpdateDecision) -> Value {
    json!({
        "match": match_result_json(result, true),
        "decision": decision.as_str(),
        "should_install": decision.should_install(),
    })
}

fn zip_inspection_json(inspection: &ZipInspection) -> Value {
    json!({
        "zip_path": path_string(&inspection.zip_path),
        "total_entries": inspection.total_entries,
        "total_uncompressed_size": inspection.total_uncompressed_size,
        "top_level_items": inspection.top_level_items,
        "likely_addon_folders": inspection.likely_addon_folders.iter().map(|folder| {
            json!({
                "folder_name": folder.folder_name,
                "has_manifest": folder.has_manifest,
                "manifest_path": folder.manifest_path,
                "title": folder.title,
                "addon_version": folder.addon_version,
                "version": folder.version,
            })
        }).collect::<Vec<_>>(),
    })
}

fn print_feeds(feeds: Option<&ApiFeeds>) -> Result<()> {
    let feeds = feeds.ok_or(AppError::FeedMissing("APIFeeds"))?;
    println!("Discovered feeds:");
    println!("  FileList:     {}", display_opt(&feeds.file_list));
    println!("  FileDetails:  {}", display_opt(&feeds.file_details));
    println!("  ListFiles:    {}", display_opt(&feeds.list_files));
    println!("  CategoryList: {}", display_opt(&feeds.category_list));
    Ok(())
}

fn print_addons<'a>(addons: impl Iterator<Item = &'a AddonSummary>) {
    println!(
        "{:<8} {:<42} {:<22} {:<16} {:<14} URL",
        "ID", "Name", "Author", "Version", "Updated"
    );
    println!("{}", "-".repeat(128));

    for addon in addons {
        println!(
            "{:<8} {:<42} {:<22} {:<16} {:<14} {}",
            truncate(addon.uid.as_deref().unwrap_or("-"), 8),
            truncate(addon.name.as_deref().unwrap_or("-"), 42),
            truncate(addon.author_name.as_deref().unwrap_or("-"), 22),
            truncate(addon.version.as_deref().unwrap_or("-"), 16),
            addon
                .date
                .map(format_mmoui_date)
                .unwrap_or_else(|| "-".into()),
            addon.file_info_url.as_deref().unwrap_or("-")
        );
    }
}

fn print_details(details: &AddonDetails) {
    println!("ID:           {}", display_opt(&details.uid));
    println!("Name:         {}", display_opt(&details.name));
    println!("Author:       {}", display_opt(&details.author_name));
    println!("Version:      {}", display_opt(&details.version));
    println!(
        "Date:         {}",
        details
            .date
            .map(format_mmoui_date)
            .unwrap_or_else(|| "-".into())
    );
    println!("File name:    {}", display_opt(&details.file_name));
    println!("MD5:          {}", display_opt(&details.md5));
    println!("Download URL: {}", display_opt(&details.download_url));
    println!("Info URL:     {}", display_opt(&details.file_info_url));

    if let Some(description) = details.description.as_deref() {
        println!();
        println!("Description:");
        println!("{}", description.trim());
    }

    if let Some(changelog) = details.changelog.as_deref() {
        println!();
        println!("Changelog:");
        println!("{}", changelog.trim());
    }
}

fn print_remote_install_metadata(details: &AddonDetails) {
    println!("Remote addon:");
    println!("  UID:          {}", display_opt(&details.uid));
    println!("  Name:         {}", display_opt(&details.name));
    println!("  Author:       {}", display_opt(&details.author_name));
    println!("  Version:      {}", display_opt(&details.version));
    println!("  File name:    {}", display_opt(&details.file_name));
    println!("  MD5:          {}", display_opt(&details.md5));
    println!("  Download URL: {}", display_opt(&details.download_url));
    println!();
}

fn print_update_selection(result: &MatchResult, decision: &update::UpdateDecision) {
    let local_version = result
        .local
        .addon_version
        .as_deref()
        .or(result.local.version.as_deref())
        .unwrap_or("-");
    println!("Local addon:");
    println!("  Folder:        {}", result.local.folder_name);
    println!("  Title:         {}", display_opt(&result.local.title));
    println!("  Local version: {local_version}");
    println!();

    println!("Remote match:");
    if let Some(remote) = result.remote.as_ref() {
        println!("  UID:            {}", remote.uid.as_deref().unwrap_or("-"));
        println!(
            "  Name:           {}",
            remote.name.as_deref().unwrap_or("-")
        );
        println!(
            "  Remote version: {}",
            remote.version.as_deref().unwrap_or("-")
        );
        println!(
            "  Updated:        {}",
            remote
                .updated
                .map(format_mmoui_date)
                .unwrap_or_else(|| "-".to_owned())
        );
    } else if !result.candidates.is_empty() {
        println!("  Ambiguous candidates:");
        for candidate in &result.candidates {
            println!(
                "    {} ({})",
                candidate.name.as_deref().unwrap_or("-"),
                candidate.uid.as_deref().unwrap_or("-")
            );
        }
    } else {
        println!("  -");
    }
    println!();
    println!("Decision: {}", decision.as_str());
    println!();
}

fn validate_single_update_plan(plan: &InstallPlan, local_folder: &str) -> Result<()> {
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
        return Err(anyhow!(
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
        return Err(anyhow!(
            "remote package would target folder {target_name}, not selected local folder {local_folder}; refusing conservative update"
        ));
    }

    Ok(())
}

fn print_installed_addons(addons: &[LocalAddon]) {
    println!(
        "{:<30} {:<36} {:<14} {:<18} {:<8} Dependencies",
        "Folder", "Title", "Version", "API", "Valid"
    );
    println!("{}", "-".repeat(128));

    for addon in addons {
        let version = addon
            .addon_version
            .as_deref()
            .or(addon.version.as_deref())
            .unwrap_or("-");
        let title = addon.title.as_deref().unwrap_or("-");
        let api_versions = join_or_dash(&addon.api_versions);
        let dependencies = join_or_dash(&addon.depends_on);

        println!(
            "{:<30} {:<36} {:<14} {:<18} {:<8} {}",
            truncate(&addon.folder_name, 30),
            truncate(title, 36),
            truncate(version, 14),
            truncate(&api_versions, 18),
            yes_no(addon.valid_manifest),
            dependencies
        );

        if !addon.optional_depends_on.is_empty() {
            println!(
                "{:<30} {:<36} {:<14} {:<18} {:<8} Optional: {}",
                "",
                "",
                "",
                "",
                "",
                addon.optional_depends_on.join(", ")
            );
        }

        if let Some(is_library) = addon.is_library {
            println!(
                "{:<30} {:<36} {:<14} {:<18} {:<8} IsLibrary: {}",
                "",
                "",
                "",
                "",
                "",
                yes_no(is_library)
            );
        }

        if let Some(author) = addon.author.as_deref() {
            println!(
                "{:<30} {:<36} {:<14} {:<18} {:<8} Author: {}",
                "", "", "", "", "", author
            );
        }

        if let Some(description) = addon.description.as_deref() {
            println!(
                "{:<30} {:<36} {:<14} {:<18} {:<8} Description: {}",
                "",
                "",
                "",
                "",
                "",
                truncate(description, 80)
            );
        }

        let manifest = addon
            .manifest_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "-".to_owned());
        println!(
            "{:<30} {:<36} {:<14} {:<18} {:<8} Manifest: {}",
            "", "", "", "", "", manifest
        );
        println!(
            "{:<30} {:<36} {:<14} {:<18} {:<8} Folder path: {}",
            "",
            "",
            "",
            "",
            "",
            addon.folder_path.display()
        );
    }
}

fn print_check_results(results: &[MatchResult], verbose: bool) {
    println!(
        "{:<26} {:<30} {:<14} {:<32} {:<8} {:<14} {:<12} Status",
        "Local folder",
        "Local title",
        "Local version",
        "Remote name",
        "UID",
        "Remote version",
        "Updated"
    );
    println!("{}", "-".repeat(150));

    for result in results {
        let local_version = result
            .local
            .addon_version
            .as_deref()
            .or(result.local.version.as_deref())
            .unwrap_or("-");
        let local_title = result.local.title.as_deref().unwrap_or("-");
        let remote_name = result
            .remote
            .as_ref()
            .and_then(|remote| remote.name.as_deref())
            .unwrap_or("-");
        let remote_uid = result
            .remote
            .as_ref()
            .and_then(|remote| remote.uid.as_deref())
            .unwrap_or("-");
        let remote_version = result
            .remote
            .as_ref()
            .and_then(|remote| remote.version.as_deref())
            .unwrap_or("-");
        let remote_updated = result
            .remote
            .as_ref()
            .and_then(|remote| remote.updated)
            .map(format_mmoui_date)
            .unwrap_or_else(|| "-".to_owned());

        println!(
            "{:<26} {:<30} {:<14} {:<32} {:<8} {:<14} {:<12} {}",
            truncate(&result.local.folder_name, 26),
            truncate(local_title, 30),
            truncate(local_version, 14),
            truncate(remote_name, 32),
            truncate(remote_uid, 8),
            truncate(remote_version, 14),
            remote_updated,
            result.status.as_str()
        );

        if !result.candidates.is_empty() {
            let candidates = result
                .candidates
                .iter()
                .map(|candidate| {
                    format!(
                        "{} ({})",
                        candidate.name.as_deref().unwrap_or("-"),
                        candidate.uid.as_deref().unwrap_or("-")
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            println!(
                "{:<26} {:<30} {:<14} {:<32} {:<8} {:<14} {:<12} candidates: {}",
                "", "", "", "", "", "", "", candidates
            );
        }

        if verbose && !result.debug_candidates.is_empty() {
            println!("{:<26} match candidates:", "");
            for candidate in &result.debug_candidates {
                println!(
                    "{:<26} tier={} score={} reason={} {} ({})",
                    "",
                    candidate.tier,
                    candidate.score,
                    candidate.reason,
                    candidate.name.as_deref().unwrap_or("-"),
                    candidate.uid.as_deref().unwrap_or("-")
                );
            }
        }
    }
}

fn print_update_plan(plan: &UpdatePlan) {
    println!(
        "{:<26} {:<32} {:<8} {:<14} {:<14} Action",
        "Local folder", "Remote name", "UID", "Local version", "Remote version"
    );
    println!("{}", "-".repeat(120));

    for action in &plan.actions {
        println!(
            "{:<26} {:<32} {:<8} {:<14} {:<14} {}",
            truncate(&action.local_folder, 26),
            truncate(action.remote_name.as_deref().unwrap_or("-"), 32),
            truncate(action.remote_uid.as_deref().unwrap_or("-"), 8),
            truncate(action.local_version.as_deref().unwrap_or("-"), 14),
            truncate(action.remote_version.as_deref().unwrap_or("-"), 14),
            action.kind.as_str()
        );

        if matches!(
            action.kind,
            PlannedActionKind::WouldUpdate | PlannedActionKind::WouldSkipUnknownVersion
        ) {
            if let Some(details) = action.details.as_ref() {
                println!("{:<26} filename: {}", "", display_opt(&details.file_name));
                println!("{:<26} md5: {}", "", display_opt(&details.md5));
                println!(
                    "{:<26} download: {}",
                    "",
                    display_opt(&details.download_url)
                );
            }
        }
    }

    let summary = plan.summary();
    println!();
    println!(
        "Summary: would update: {}, current/skipped: {}, local-newer: {}, unknown: {}, no match: {}, ambiguous: {}, libraries: {}",
        summary.would_update,
        summary.current_skipped,
        summary.local_newer,
        summary.unknown,
        summary.no_match,
        summary.ambiguous,
        summary.libraries
    );
}

fn print_update_all_plan(plan: &update_all_core::UpdateAllPlan) {
    println!(
        "{:<26} {:<32} {:<8} {:<14} {:<14} Action",
        "Local folder", "Remote name", "UID", "Local version", "Remote version"
    );
    println!("{}", "-".repeat(120));

    for action in &plan.display_plan.actions {
        println!(
            "{:<26} {:<32} {:<8} {:<14} {:<14} {}",
            truncate(&action.local_folder, 26),
            truncate(action.remote_name.as_deref().unwrap_or("-"), 32),
            truncate(action.remote_uid.as_deref().unwrap_or("-"), 8),
            truncate(action.local_version.as_deref().unwrap_or("-"), 14),
            truncate(action.remote_version.as_deref().unwrap_or("-"), 14),
            update_all_action_label(action, &plan.targets)
        );
    }

    let mut skipped_current = 0;
    let mut skipped_local_newer = 0;
    let mut skipped_unknown = 0;
    let mut skipped_no_match = 0;
    let mut skipped_ambiguous = 0;
    let mut skipped_libraries = 0;

    for action in &plan.display_plan.actions {
        if update_all_targets_contain(&plan.targets, action) {
            continue;
        }

        match action.kind {
            PlannedActionKind::WouldUpdate => {}
            PlannedActionKind::WouldSkipCurrent => skipped_current += 1,
            PlannedActionKind::WouldSkipLocalNewer => skipped_local_newer += 1,
            PlannedActionKind::WouldSkipUnknownVersion => skipped_unknown += 1,
            PlannedActionKind::WouldSkipNoMatch => skipped_no_match += 1,
            PlannedActionKind::WouldSkipAmbiguous => skipped_ambiguous += 1,
            PlannedActionKind::WouldSkipLibrary => skipped_libraries += 1,
        }
    }

    println!();
    println!(
        "Summary: planned updates: {}, skipped current: {}, skipped local-newer: {}, skipped unknown: {}, skipped no-match: {}, skipped ambiguous: {}, skipped libraries: {}",
        plan.targets.len(),
        skipped_current,
        skipped_local_newer,
        skipped_unknown,
        skipped_no_match,
        skipped_ambiguous,
        skipped_libraries
    );
}

fn update_all_action_label(
    action: &PlannedAddonAction,
    targets: &[PlannedAddonAction],
) -> &'static str {
    if update_all_targets_contain(targets, action) {
        "would-update"
    } else {
        action.kind.as_str()
    }
}

fn update_all_targets_contain(targets: &[PlannedAddonAction], action: &PlannedAddonAction) -> bool {
    targets.iter().any(|target| {
        target.local_folder == action.local_folder && target.remote_uid == action.remote_uid
    })
}

fn print_update_all_item_result(target: &PlannedAddonAction, result: &InstallResult) {
    let status = if result.replaced > 0 || result.installed_new > 0 {
        "updated"
    } else if result.skipped > 0 {
        "skipped"
    } else {
        "updated"
    };

    println!(
        "{}: {} ({})",
        status,
        target.local_folder,
        target.remote_uid.as_deref().unwrap_or("-")
    );
    println!(
        "  backup path: {}",
        result
            .backup_dir
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "-".to_owned())
    );

    for item in &result.items {
        println!(
            "  {} -> {}: {}",
            item.source_folder.as_deref().unwrap_or("-"),
            item.target_folder
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "-".to_owned()),
            item.action.as_str()
        );
    }
}

fn print_zip_inspection(inspection: &ZipInspection) {
    println!("ZIP: {}", inspection.zip_path.display());
    println!("Entries: {}", inspection.total_entries);
    println!(
        "Total uncompressed size: {} bytes",
        inspection.total_uncompressed_size
    );
    println!();
    println!("Top-level items:");
    for item in &inspection.top_level_items {
        println!("  {item}");
    }

    println!();
    println!(
        "{:<30} {:<9} {:<36} {:<14} Manifest",
        "Likely addon folder", "Manifest", "Title", "Version"
    );
    println!("{}", "-".repeat(110));
    for folder in &inspection.likely_addon_folders {
        let version = folder
            .addon_version
            .as_deref()
            .or(folder.version.as_deref())
            .unwrap_or("-");
        println!(
            "{:<30} {:<9} {:<36} {:<14} {}",
            truncate(&folder.folder_name, 30),
            yes_no(folder.has_manifest),
            truncate(folder.title.as_deref().unwrap_or("-"), 36),
            truncate(version, 14),
            folder.manifest_path.as_deref().unwrap_or("-")
        );
    }
}

fn print_extracted_zip(extracted: &ExtractedZip) {
    println!("Temporary directory: {}", extracted.temp_dir.display());
    println!("Entries extracted: {}", extracted.inspection.total_entries);
    println!();
    println!("Detected addon folders:");
    print_installed_addons(&extracted.detected_addons);
}

fn print_install_plan(plan: &InstallPlan, dry_run: bool) {
    println!("AddOns directory: {}", plan.addons_dir.display());
    println!(
        "Temporary extraction directory: {}",
        plan.temp_dir.display()
    );
    if dry_run {
        println!("Dry run only: no installed addon files will be created, modified, or deleted.");
    } else {
        println!("Plan preview: applying this plan may create or replace addon folders.");
    }
    println!();
    println!(
        "{:<28} {:<34} {:<14} {:<48} Action",
        "Source folder", "Title", "Version", "Target folder"
    );
    println!("{}", "-".repeat(150));

    for item in &plan.items {
        println!(
            "{:<28} {:<34} {:<14} {:<48} {}",
            truncate(item.source_folder.as_deref().unwrap_or("-"), 28),
            truncate(item.title.as_deref().unwrap_or("-"), 34),
            truncate(item.version.as_deref().unwrap_or("-"), 14),
            truncate(
                &item
                    .target_folder
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "-".to_owned()),
                48
            ),
            item.action.as_str()
        );

        if matches!(item.action, InstallPlanAction::WouldWarnMultipleAddons) {
            println!(
                "{:<28} {}",
                "",
                "Archive contains multiple addon folders; review all planned targets before real install support is added."
            );
        }
    }
}

fn print_dependency_plan(plan: &DependencyPlan) {
    if plan.required_dependencies.is_empty() && plan.optional_dependencies.is_empty() {
        return;
    }

    println!("Required libraries:");
    if plan.required_dependencies.is_empty() {
        println!("  -");
    } else {
        for dependency in &plan.required_dependencies {
            let indent = "  ".repeat(dependency.depth.saturating_sub(1));
            println!(
                "  {}{:<28} {:<18} {}",
                indent,
                truncate(&dependency.name, 28),
                dependency.status.as_str(),
                dependency_detail(dependency)
            );
        }
    }

    if plan.has_unresolved_required_dependencies() {
        println!();
        println!("Some required dependencies could not be resolved.");
    }

    if !plan.optional_dependencies.is_empty() {
        println!();
        println!("Optional dependencies (not installed automatically):");
        for dependency in &plan.optional_dependencies {
            let indent = "  ".repeat(dependency.depth.saturating_sub(1));
            println!(
                "  {}{:<28} {:<18} {}",
                indent,
                truncate(&dependency.name, 28),
                dependency.status.as_str(),
                dependency_detail(dependency)
            );
        }
    }

    if plan.install_order.len() > 1 {
        println!();
        println!("Install order: {}", plan.install_order.join(" -> "));
    }
}

fn dependency_detail(dependency: &dependencies::DependencyPlanEntry) -> String {
    if let Some(folder) = dependency.installed_folder.as_deref() {
        return format!("installed folder: {folder}");
    }
    if let Some(folder) = dependency.bundled_folder.as_deref() {
        return format!("bundled folder: {folder}");
    }
    if let Some(uid) = dependency.remote_uid.as_deref() {
        return format!(
            "{} ({uid})",
            dependency.remote_name.as_deref().unwrap_or("-")
        );
    }
    if dependency.status == DependencyStatus::Ambiguous {
        "multiple remote matches; not installed automatically".to_owned()
    } else if dependency.status == DependencyStatus::Circular {
        "circular dependency; not installed automatically".to_owned()
    } else if dependency.status == DependencyStatus::MaxDepth {
        "max dependency depth reached; not installed automatically".to_owned()
    } else {
        "no remote match; not installed automatically".to_owned()
    }
}

fn print_install_result(result: &InstallResult) {
    println!("Install summary:");
    println!("  installed new: {}", result.installed_new);
    println!("  replaced: {}", result.replaced);
    println!("  skipped: {}", result.skipped);
    println!(
        "  backup location: {}",
        result
            .backup_dir
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "-".to_owned())
    );

    if !result.items.is_empty() {
        println!();
        println!(
            "{:<26} {:<20} {:<48} Backup",
            "Source folder", "Performed", "Target"
        );
        println!("{}", "-".repeat(120));
        for item in &result.items {
            println!(
                "{:<26} {:<20} {:<48} {}",
                truncate(item.source_folder.as_deref().unwrap_or("-"), 26),
                item.action.as_str(),
                truncate(
                    &item
                        .target_folder
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "-".to_owned()),
                    48
                ),
                item.backup_folder
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .or_else(|| item.message.clone())
                    .unwrap_or_else(|| "-".to_owned())
            );
        }
    }
}

fn scan_installed_addons_for_install(addons_dir: &Path) -> Result<Vec<LocalAddon>> {
    if !addons_dir.exists() {
        return Ok(Vec::new());
    }

    local::scan_addons_dir(addons_dir)
        .with_context(|| format!("failed to scan AddOns directory {}", addons_dir.display()))
}

async fn resolve_output_path(output: &Path, remote_file_name: &str) -> Result<PathBuf> {
    match fs::metadata(output).await {
        Ok(metadata) if metadata.is_dir() => Ok(output.join(remote_file_name)),
        Ok(_) => Ok(output.to_path_buf()),
        Err(_) if output.extension().is_none() => Ok(output.join(remote_file_name)),
        Err(_) => Ok(output.to_path_buf()),
    }
}

fn display_opt(value: &Option<String>) -> &str {
    value.as_deref().unwrap_or("-")
}

fn join_or_dash(values: &[String]) -> String {
    if values.is_empty() {
        "-".to_owned()
    } else {
        values.join(", ")
    }
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_owned();
    }

    if max_chars <= 3 {
        return ".".repeat(max_chars);
    }

    let mut chars = value.chars();
    let mut truncated = String::new();
    for _ in 0..(max_chars - 3) {
        if let Some(ch) = chars.next() {
            truncated.push(ch);
        }
    }

    truncated.push_str("...");
    truncated
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde_json::Value;

    use crate::install::apply::{InstallActionPerformed, InstallResult, InstalledItem};
    use crate::install::plan::{InstallPlan, InstallPlanAction, InstallPlanItem};
    use crate::local::match_remote::{MatchResult, MatchStatus, RemoteCandidate};
    use crate::local::LocalAddon;

    use super::{install_plan_json, install_result_json, match_result_json};

    fn local(folder_name: &str) -> LocalAddon {
        LocalAddon {
            folder_name: folder_name.to_owned(),
            folder_path: PathBuf::from(folder_name),
            manifest_path: Some(PathBuf::from(format!("{folder_name}/{folder_name}.txt"))),
            title: Some(folder_name.to_owned()),
            addon_version: Some("1".to_owned()),
            version: None,
            api_versions: Vec::new(),
            depends_on: Vec::new(),
            optional_depends_on: Vec::new(),
            saved_variables: Vec::new(),
            saved_variables_per_character: Vec::new(),
            is_library: None,
            author: None,
            description: None,
            valid_manifest: true,
        }
    }

    #[test]
    fn check_json_shape_serializes_status_and_candidates() {
        let result = MatchResult {
            local: local("Addon"),
            status: MatchStatus::PossibleUpdate,
            remote: Some(RemoteCandidate {
                uid: Some("1".to_owned()),
                name: Some("Addon".to_owned()),
                author_name: None,
                version: Some("2".to_owned()),
                updated: None,
                file_info_url: None,
                summary: None,
                directories: Vec::new(),
                category_id: None,
                category_name: None,
                downloads: None,
                monthly_downloads: None,
                image_urls: Vec::new(),
                thumbnail_urls: Vec::new(),
                tier: 1,
                score: 100,
                reason: "test".to_owned(),
            }),
            candidates: Vec::new(),
            debug_candidates: vec![RemoteCandidate {
                uid: Some("1".to_owned()),
                name: Some("Addon".to_owned()),
                author_name: None,
                version: Some("2".to_owned()),
                updated: None,
                file_info_url: None,
                summary: None,
                directories: Vec::new(),
                category_id: None,
                category_name: None,
                downloads: None,
                monthly_downloads: None,
                image_urls: Vec::new(),
                thumbnail_urls: Vec::new(),
                tier: 1,
                score: 100,
                reason: "test".to_owned(),
            }],
        };

        let value = match_result_json(&result, true);
        let reparsed: Value =
            serde_json::from_str(&serde_json::to_string(&value).unwrap()).unwrap();

        assert_eq!(reparsed["status"], "possible-update");
        assert_eq!(reparsed["remote"]["uid"], "1");
        assert_eq!(reparsed["debug_candidates"][0]["reason"], "test");
    }

    #[test]
    fn dry_run_install_json_can_report_applied_false() {
        let plan = InstallPlan {
            addons_dir: PathBuf::from("/tmp/AddOns"),
            temp_dir: PathBuf::from("/tmp/extracted"),
            items: vec![InstallPlanItem {
                source_folder: Some("Addon".to_owned()),
                title: Some("Addon".to_owned()),
                version: Some("1".to_owned()),
                target_folder: Some(PathBuf::from("/tmp/AddOns/Addon")),
                action: InstallPlanAction::WouldReplaceExisting,
            }],
        };
        let value = serde_json::json!({
            "dry_run": true,
            "applied": false,
            "plan": install_plan_json(&plan),
        });

        assert_eq!(value["dry_run"], true);
        assert_eq!(value["applied"], false);
        assert_eq!(
            value["plan"]["items"][0]["action"],
            "would-replace-existing"
        );
    }

    #[test]
    fn applied_install_json_includes_backup_paths() {
        let result = InstallResult {
            items: vec![InstalledItem {
                source_folder: Some("Addon".to_owned()),
                target_folder: Some(PathBuf::from("/tmp/AddOns/Addon")),
                backup_folder: Some(PathBuf::from("/tmp/backup/Addon")),
                action: InstallActionPerformed::ReplacedExisting,
                message: None,
            }],
            backup_dir: Some(PathBuf::from("/tmp/backup")),
            installed_new: 0,
            replaced: 1,
            skipped: 0,
        };

        let value = install_result_json(&result);

        assert_eq!(value["replaced"], 1);
        assert_eq!(value["backup_dir"], "/tmp/backup");
        assert_eq!(value["items"][0]["backup_folder"], "/tmp/backup/Addon");
    }
}
