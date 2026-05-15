use std::path::{Component, Path};

use thiserror::Error;

pub mod apply;
pub mod plan;
pub mod remote;
pub mod remove;
pub mod update;
pub mod update_all;
pub mod zip_safety;

#[allow(dead_code)]
#[derive(Debug, Error)]
pub enum InstallError {
    #[error("unsafe zip entry path rejected: {0}")]
    UnsafeZipPath(String),
}

#[allow(dead_code)]
pub fn validate_zip_entry_path(path: &Path) -> Result<(), InstallError> {
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(InstallError::UnsafeZipPath(path.display().to_string()));
    }

    Ok(())
}

// TODO: extract ZIPs into a tempfile::TempDir first, validate every entry with
// validate_zip_entry_path, then atomically replace installed addon directories.
// TODO: compare a local manifest against remote metadata before update operations.
