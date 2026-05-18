use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::api::models::AddonDetails;

#[derive(Debug, Error)]
pub enum RemoteInstallError {
    #[error("remote FileDetails has no UIDownload URL")]
    MissingDownloadUrl,

    #[error("download MD5 mismatch: expected {expected}, got {actual}")]
    Md5Mismatch { expected: String, actual: String },
}

pub fn download_url(details: &AddonDetails) -> Result<&str, RemoteInstallError> {
    details
        .download_url
        .as_deref()
        .ok_or(RemoteInstallError::MissingDownloadUrl)
}

pub fn download_file_name(details: &AddonDetails, addon_id: &str) -> String {
    details
        .file_name
        .as_deref()
        .and_then(sanitize_remote_file_name)
        .unwrap_or_else(|| format!("addon-{addon_id}.zip"))
}

pub fn verify_md5(bytes: &[u8], expected: Option<&str>) -> Result<(), RemoteInstallError> {
    let Some(expected) = expected.filter(|value| !value.trim().is_empty()) else {
        return Ok(());
    };
    let actual = format!("{:x}", md5::compute(bytes));

    if expected.eq_ignore_ascii_case(&actual) {
        Ok(())
    } else {
        Err(RemoteInstallError::Md5Mismatch {
            expected: expected.to_owned(),
            actual,
        })
    }
}

pub fn keep_download_path(
    download_dir: Option<&Path>,
    file_name: &str,
) -> std::io::Result<PathBuf> {
    let dir = download_dir
        .map(Path::to_path_buf)
        .map(Ok)
        .unwrap_or_else(default_download_dir)?;
    Ok(unique_path(&dir.join(file_name)))
}

pub fn default_download_dir() -> std::io::Result<PathBuf> {
    crate::app_paths::download_cache_dir()
}

pub fn sanitize_remote_file_name(value: &str) -> Option<String> {
    let normalized = value.replace('\\', "/");
    let file_name = normalized.rsplit('/').next()?.trim();
    if file_name.is_empty()
        || file_name == "."
        || file_name == ".."
        || file_name.contains('\0')
        || has_windows_drive_prefix(file_name)
    {
        return None;
    }

    let sanitized = file_name
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            ch if ch.is_control() => '_',
            ch => ch,
        })
        .collect::<String>();

    if sanitized.is_empty() {
        None
    } else {
        Some(sanitized)
    }
}

fn unique_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }

    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy())
        .unwrap_or_default();
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy());

    for suffix in 1.. {
        let file_name = match extension.as_deref() {
            Some(extension) => format!("{stem}-{suffix}.{extension}"),
            None => format!("{stem}-{suffix}"),
        };
        let candidate = parent.join(file_name);
        if !candidate.exists() {
            return candidate;
        }
    }

    unreachable!("unbounded suffix search should eventually find a free path")
}

fn has_windows_drive_prefix(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use crate::install::remote::{
        keep_download_path, sanitize_remote_file_name, verify_md5, RemoteInstallError,
    };

    #[test]
    fn remote_filename_is_sanitized() {
        assert_eq!(
            sanitize_remote_file_name("../SomeAddon.zip").as_deref(),
            Some("SomeAddon.zip")
        );
        assert_eq!(
            sanitize_remote_file_name("C:\\temp\\Some:Addon.zip").as_deref(),
            Some("Some_Addon.zip")
        );
        assert_eq!(sanitize_remote_file_name(".."), None);
    }

    #[test]
    fn keep_download_path_avoids_overwrite() {
        let dir = tempdir().unwrap();
        let existing = dir.path().join("Addon.zip");
        std::fs::write(&existing, "already here").unwrap();

        let path = keep_download_path(Some(dir.path()), "Addon.zip").unwrap();

        assert_eq!(path.file_name().unwrap().to_string_lossy(), "Addon-1.zip");
    }

    #[test]
    fn md5_verification_failure_is_reported() {
        let result = verify_md5(b"hello", Some("00000000000000000000000000000000"));

        assert!(matches!(
            result,
            Err(RemoteInstallError::Md5Mismatch { .. })
        ));
    }
}
