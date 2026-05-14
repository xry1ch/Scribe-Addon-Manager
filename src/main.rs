mod api;
mod commands;
mod config;
mod error;
mod install;
mod local;

use anyhow::Result;
use clap::Parser;
use commands::{Cli, Commands};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let client = api::ApiClient::new()?;

    match cli.command {
        Commands::FetchConfig => commands::fetch_config(&client).await,
        Commands::List => commands::list(&client).await,
        Commands::Search { query, limit } => commands::search(&client, &query, limit).await,
        Commands::Details { addon_id } => commands::details(&client, &addon_id).await,
        Commands::Download { addon_id, output } => {
            commands::download(&client, &addon_id, &output).await
        }
        Commands::LocalPaths => commands::local_paths(),
        Commands::Installed { path } => commands::installed(path.as_deref()),
        Commands::Check {
            path,
            refresh,
            limit,
            verbose,
        } => commands::check(&client, path.as_deref(), refresh, limit, verbose).await,
        Commands::PlanUpdate {
            path,
            refresh,
            limit,
            include_unknown,
        } => commands::plan_update(&client, path.as_deref(), refresh, limit, include_unknown).await,
        Commands::InspectZip { zip_path } => commands::inspect_zip(&zip_path),
        Commands::ExtractTemp { zip_path } => commands::extract_temp(&zip_path),
        Commands::PlanInstall { zip_path, path } => {
            commands::plan_install(&zip_path, path.as_deref())
        }
        Commands::InstallZip {
            zip_path,
            path,
            yes,
            backup_dir,
        } => commands::install_zip(&zip_path, path.as_deref(), yes, backup_dir.as_deref()),
        Commands::Install {
            addon_id,
            path,
            yes,
            backup_dir,
            keep_download,
            download_dir,
        } => {
            commands::install_remote(
                &client,
                &addon_id,
                path.as_deref(),
                yes,
                backup_dir.as_deref(),
                keep_download,
                download_dir.as_deref(),
            )
            .await
        }
        Commands::Update {
            local_folder_or_uid,
            path,
            yes,
            backup_dir,
            keep_download,
            download_dir,
            force,
        } => {
            commands::update_one(
                &client,
                &local_folder_or_uid,
                path.as_deref(),
                yes,
                backup_dir.as_deref(),
                keep_download,
                download_dir.as_deref(),
                force,
            )
            .await
        }
        Commands::UpdateAll {
            path,
            refresh,
            yes,
            backup_dir,
            keep_download,
            download_dir,
            include_unknown,
            limit,
        } => {
            commands::update_all(
                &client,
                path.as_deref(),
                refresh,
                yes,
                backup_dir.as_deref(),
                keep_download,
                download_dir.as_deref(),
                include_unknown,
                limit,
            )
            .await
        }
    }
}
