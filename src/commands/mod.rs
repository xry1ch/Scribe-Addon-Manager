use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use tokio::fs;

use crate::api::models::{AddonDetails, AddonSummary, ApiFeeds};
use crate::api::ApiClient;
use crate::error::AppError;
use crate::install::apply::{self, InstallResult};
use crate::install::plan::{self, InstallPlan, InstallPlanAction};
use crate::install::remote;
use crate::install::update;
use crate::install::update_all as update_all_core;
use crate::install::zip_safety::{self, ExtractedZip, ZipInspection};
use crate::local::match_remote::{self, MatchResult};
use crate::local::update_plan::{self, PlannedActionKind, PlannedAddonAction, UpdatePlan};
use crate::local::{self, LocalAddon};

#[derive(Debug, Parser)]
#[command(name = "eso-addon-manager")]
#[command(about = "Unofficial CLI-first Elder Scrolls Online addon manager")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Fetch global and ESO game configuration and print discovered feeds.
    FetchConfig,

    /// Fetch the ESO FileList feed and print the first 25 addons.
    List,

    /// Search addon metadata from the ESO FileList feed.
    Search {
        query: String,

        #[arg(long, default_value_t = 25)]
        limit: usize,
    },

    /// Fetch details for a single addon id.
    Details { addon_id: String },

    /// Download an addon ZIP without extracting it.
    Download {
        addon_id: String,

        #[arg(long)]
        output: PathBuf,
    },

    /// Print candidate ESO AddOns directories.
    LocalPaths,

    /// Scan a local ESO AddOns directory and print detected addons.
    Installed {
        #[arg(long)]
        path: Option<PathBuf>,
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
    },

    /// Validate and inspect a local addon ZIP without extracting it.
    InspectZip { zip_path: PathBuf },

    /// Validate and extract a local addon ZIP into a temporary directory only.
    ExtractTemp { zip_path: PathBuf },

    /// Print a dry-run plan for installing a ZIP into an ESO AddOns directory.
    PlanInstall {
        zip_path: PathBuf,

        #[arg(long)]
        path: Option<PathBuf>,
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
    },
}

pub async fn fetch_config(client: &ApiClient) -> Result<()> {
    let config = client.eso_game_config().await?;
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

pub async fn list(client: &ApiClient) -> Result<()> {
    let addons = client.eso_file_list().await?;
    print_addons(addons.iter().take(25));
    Ok(())
}

pub async fn search(client: &ApiClient, query: &str, limit: usize) -> Result<()> {
    let needle = query.to_lowercase();
    let addons = client.eso_file_list().await?;
    let matches = addons
        .iter()
        .filter(|addon| addon.searchable_text().contains(&needle))
        .take(limit);

    print_addons(matches);
    Ok(())
}

pub async fn details(client: &ApiClient, addon_id: &str) -> Result<()> {
    let details = client.eso_file_details(addon_id).await?;
    print_details(&details);
    Ok(())
}

pub async fn download(client: &ApiClient, addon_id: &str, output: &Path) -> Result<()> {
    let details = client.eso_file_details(addon_id).await?;
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

pub fn local_paths() -> Result<()> {
    let candidates = local::addon_path_candidates();

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

pub fn installed(path: Option<&Path>) -> Result<()> {
    let path = match path {
        Some(path) => path.to_path_buf(),
        None => local::detect_best_addons_dir()
            .ok_or_else(|| anyhow!("could not auto-detect an ESO AddOns directory"))?,
    };

    let addons = local::scan_addons_dir(&path)
        .with_context(|| format!("failed to scan AddOns directory {}", path.display()))?;

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
    let remote_addons = client.eso_file_list().await?;
    let mut results = match_remote::match_installed_addons(&local_addons, &remote_addons);

    results.sort_by_key(|result| result.local.folder_name.to_lowercase());
    if let Some(limit) = limit {
        results.truncate(limit);
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
    let remote_addons = client.eso_file_list().await?;
    let mut matches = match_remote::match_installed_addons(&local_addons, &remote_addons);
    matches.sort_by_key(|result| result.local.folder_name.to_lowercase());

    if let Some(limit) = limit {
        matches.truncate(limit);
    }

    let detail_uids = update_plan::detail_request_uids_for(&matches, include_unknown);
    let mut plan = update_plan::build_update_plan(&matches, include_unknown);
    for uid in detail_uids {
        let details = client.eso_file_details(&uid).await?;
        plan.attach_details(&uid, details);
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
    let remote_addons = client.eso_file_list().await?;
    let mut matches = match_remote::match_installed_addons(&local_addons, &remote_addons);
    matches.sort_by_key(|result| result.local.folder_name.to_lowercase());

    if let Some(limit) = limit {
        matches.truncate(limit);
    }

    let plan = update_all_core::build_update_all_plan(&matches, include_unknown);

    println!("AddOns directory: {}", addons_dir.display());
    println!("Remote addons loaded: {}", remote_addons.len());
    println!("Dry run by default: no files will be downloaded, extracted, modified, or deleted without --yes.");
    println!();
    print_update_all_plan(&plan);

    if !yes {
        println!();
        println!("No changes made. Re-run with --yes to update all planned addons.");
        return Ok(());
    }

    if plan.targets.is_empty() {
        println!();
        println!("No planned addons to update.");
        return Ok(());
    }

    println!();
    println!("Applying planned updates sequentially:");

    for target in &plan.targets {
        let remote_uid = target.remote_uid.as_deref().ok_or_else(|| {
            anyhow!(
                "planned addon {} has no clean remote UID",
                target.local_folder
            )
        })?;

        println!();
        println!(
            "Updating {} -> {} ({})",
            target.local_folder,
            target.remote_name.as_deref().unwrap_or("-"),
            remote_uid
        );

        let result = match update_all_one(
            client,
            target,
            remote_uid,
            &addons_dir,
            &local_addons,
            backup_dir,
            keep_download,
            download_dir,
        )
        .await
        {
            Ok(result) => result,
            Err(error) => {
                println!("failed: {} ({})", target.local_folder, error);
                return Err(error)
                    .with_context(|| format!("failed to update {}", target.local_folder));
            }
        };

        print_update_all_item_result(target, &result);
        local_addons = local::scan_addons_dir(&addons_dir).with_context(|| {
            format!(
                "failed to rescan AddOns directory {} after updating {}",
                addons_dir.display(),
                target.local_folder
            )
        })?;
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
) -> Result<InstallResult> {
    let details = client.eso_file_details(remote_uid).await?;
    let install_plan = prepare_remote_install_plan(
        client,
        &details,
        remote_uid,
        addons_dir,
        local_addons,
        keep_download,
        download_dir,
    )
    .await?;
    validate_single_update_plan(&install_plan, &target.local_folder)?;

    Ok(apply::apply_install_plan(&install_plan, backup_dir)?)
}

pub fn inspect_zip(zip_path: &Path) -> Result<()> {
    let inspection = zip_safety::inspect_zip(zip_path)
        .with_context(|| format!("failed to inspect ZIP {}", zip_path.display()))?;
    print_zip_inspection(&inspection);
    Ok(())
}

pub fn extract_temp(zip_path: &Path) -> Result<()> {
    let extracted = zip_safety::extract_zip_to_temp(zip_path)
        .with_context(|| format!("failed to extract ZIP {}", zip_path.display()))?;
    print_extracted_zip(&extracted);
    Ok(())
}

pub fn plan_install(zip_path: &Path, path: Option<&Path>) -> Result<()> {
    let addons_dir = match path {
        Some(path) => path.to_path_buf(),
        None => local::detect_best_addons_dir()
            .ok_or_else(|| anyhow!("could not auto-detect an ESO AddOns directory"))?,
    };
    let installed_addons = scan_installed_addons_for_install(&addons_dir)?;
    let extracted = zip_safety::extract_zip_to_temp(zip_path)
        .with_context(|| format!("failed to validate and extract ZIP {}", zip_path.display()))?;
    let plan = plan::plan_install(&extracted, &addons_dir, &installed_addons)?;

    print_install_plan(&plan, true);
    Ok(())
}

pub fn install_zip(
    zip_path: &Path,
    path: Option<&Path>,
    yes: bool,
    backup_dir: Option<&Path>,
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
        print_install_plan(&plan, true);
        println!();
        println!("No changes made. Re-run with --yes to install.");
        return Ok(());
    }

    print_install_plan(&plan, false);
    let result = apply::apply_install_plan(&plan, backup_dir)?;
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
) -> Result<()> {
    let details = client.eso_file_details(addon_id).await?;
    print_remote_install_metadata(&details);

    let download_url = remote::download_url(&details)?;
    let file_name = remote::download_file_name(&details, addon_id);
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
        println!("Saved ZIP: {}", path.display());
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

    let addons_dir = match path {
        Some(path) => path.to_path_buf(),
        None => local::detect_best_addons_dir()
            .ok_or_else(|| anyhow!("could not auto-detect an ESO AddOns directory"))?,
    };
    let installed_addons = scan_installed_addons_for_install(&addons_dir)?;
    let extracted = zip_safety::extract_zip_to_temp(&zip_path)
        .with_context(|| format!("failed to validate and extract ZIP {}", zip_path.display()))?;
    let plan = plan::plan_install(&extracted, &addons_dir, &installed_addons)?;

    if !yes {
        print_install_plan(&plan, true);
        println!();
        println!("No changes made. Re-run with --yes to install.");
        return Ok(());
    }

    print_install_plan(&plan, false);
    let result = apply::apply_install_plan(&plan, backup_dir)?;
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
) -> Result<()> {
    let addons_dir = match path {
        Some(path) => path.to_path_buf(),
        None => local::detect_best_addons_dir()
            .ok_or_else(|| anyhow!("could not auto-detect an ESO AddOns directory"))?,
    };
    let local_addons = local::scan_addons_dir(&addons_dir)
        .with_context(|| format!("failed to scan AddOns directory {}", addons_dir.display()))?;
    let remote_addons = client.eso_file_list().await?;
    let matches = match_remote::match_installed_addons(&local_addons, &remote_addons);
    let selected = update::resolve_update_request(&matches, request)?;
    let decision = update::update_decision(selected, force);

    print_update_selection(selected, &decision);

    if !decision.should_install() {
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
    let details = client.eso_file_details(remote_uid).await?;
    print_remote_install_metadata(&details);

    let plan = prepare_remote_install_plan(
        client,
        &details,
        remote_uid,
        &addons_dir,
        &local_addons,
        keep_download,
        download_dir,
    )
    .await?;
    validate_single_update_plan(&plan, &selected.local.folder_name)?;

    if !yes {
        print_install_plan(&plan, true);
        println!();
        println!("No changes made. Re-run with --yes to update.");
        return Ok(());
    }

    print_install_plan(&plan, false);
    let result = apply::apply_install_plan(&plan, backup_dir)?;
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
) -> Result<InstallPlan> {
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
        println!("Saved ZIP: {}", path.display());
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
    Ok(plan::plan_install(
        &extracted,
        addons_dir,
        installed_addons,
    )?)
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
