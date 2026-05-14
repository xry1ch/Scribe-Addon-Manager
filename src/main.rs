use anyhow::Result;
use clap::Parser;
use eso_addon_manager::api;
use eso_addon_manager::commands::{self, Cli, Commands};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let client = api::ApiClient::new()?;

    match cli.command {
        Commands::FetchConfig { json } => commands::fetch_config(&client, json).await,
        Commands::List { json } => commands::list(&client, json).await,
        Commands::Search { query, limit, json } => {
            commands::search(&client, &query, limit, json).await
        }
        Commands::Details { addon_id, json } => commands::details(&client, &addon_id, json).await,
        Commands::Download { addon_id, output } => {
            commands::download(&client, &addon_id, &output).await
        }
        Commands::LocalPaths { json } => commands::local_paths(json),
        Commands::Installed { path, json } => commands::installed(path.as_deref(), json),
        Commands::Check {
            path,
            refresh,
            limit,
            verbose,
            json,
        } => commands::check(&client, path.as_deref(), refresh, limit, verbose, json).await,
        Commands::PlanUpdate {
            path,
            refresh,
            limit,
            include_unknown,
            json,
        } => {
            commands::plan_update(
                &client,
                path.as_deref(),
                refresh,
                limit,
                include_unknown,
                json,
            )
            .await
        }
        Commands::InspectZip { zip_path, json } => commands::inspect_zip(&zip_path, json),
        Commands::ExtractTemp { zip_path } => commands::extract_temp(&zip_path),
        Commands::PlanInstall {
            zip_path,
            path,
            json,
        } => commands::plan_install(&zip_path, path.as_deref(), json),
        Commands::InstallZip {
            zip_path,
            path,
            yes,
            backup_dir,
            json,
        } => commands::install_zip(&zip_path, path.as_deref(), yes, backup_dir.as_deref(), json),
        Commands::Install {
            addon_id,
            path,
            yes,
            backup_dir,
            keep_download,
            download_dir,
            json,
        } => {
            commands::install_remote(
                &client,
                &addon_id,
                path.as_deref(),
                yes,
                backup_dir.as_deref(),
                keep_download,
                download_dir.as_deref(),
                json,
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
            json,
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
                json,
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
            json,
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
                json,
            )
            .await
        }
    }
}
