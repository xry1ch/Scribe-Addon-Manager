#[cfg(test)]
use std::cell::RefCell;
use std::env;
use std::io;
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;

use directories::BaseDirs;

pub const APP_DISPLAY_NAME: &str = "Crux Addon Manager";
pub const APP_SLUG: &str = "crux-addon-manager";
pub const APP_DATA_DIR_ENV: &str = "CRUX_ADDON_MANAGER_APP_DATA_DIR";
pub const APP_CACHE_DIR_ENV: &str = "CRUX_ADDON_MANAGER_APP_CACHE_DIR";

pub fn app_data_dir() -> io::Result<PathBuf> {
    #[cfg(test)]
    if let Some(path) = test_app_data_dir() {
        return Ok(path);
    }

    if let Some(path) = override_dir(APP_DATA_DIR_ENV) {
        return Ok(path);
    }

    let base = BaseDirs::new()
        .ok_or_else(|| missing_base_dir("app data"))?
        .data_dir()
        .to_path_buf();
    Ok(base.join(app_dir_name()))
}

pub fn app_cache_dir() -> io::Result<PathBuf> {
    #[cfg(test)]
    if let Some(path) = test_app_cache_dir() {
        return Ok(path);
    }

    if let Some(path) = override_dir(APP_CACHE_DIR_ENV) {
        return Ok(path);
    }

    let base = BaseDirs::new()
        .ok_or_else(|| missing_base_dir("cache"))?
        .cache_dir()
        .to_path_buf();
    let app_cache_root = base.join(app_dir_name());

    #[cfg(windows)]
    {
        Ok(app_cache_root.join("cache"))
    }

    #[cfg(not(windows))]
    {
        Ok(app_cache_root)
    }
}

pub fn settings_file_path() -> io::Result<PathBuf> {
    Ok(app_data_dir()?.join("settings.json"))
}

pub fn metadata_file_path() -> io::Result<PathBuf> {
    Ok(app_data_dir()?.join("metadata").join("installed.json"))
}

pub fn default_backup_dir() -> io::Result<PathBuf> {
    Ok(app_data_dir()?.join("backups"))
}

pub fn http_cache_dir() -> io::Result<PathBuf> {
    Ok(app_cache_dir()?.join("http-cache"))
}

pub fn download_cache_dir() -> io::Result<PathBuf> {
    Ok(app_cache_dir()?.join("downloads"))
}

fn override_dir(variable: &str) -> Option<PathBuf> {
    env::var_os(variable).and_then(|value| {
        let path = PathBuf::from(value);
        if path.as_os_str().is_empty() {
            None
        } else {
            Some(path)
        }
    })
}

fn missing_base_dir(label: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("could not determine OS {label} directory for {APP_DISPLAY_NAME}"),
    )
}

fn app_dir_name() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        APP_SLUG
    }

    #[cfg(not(target_os = "linux"))]
    {
        APP_DISPLAY_NAME
    }
}

#[cfg(test)]
thread_local! {
    static TEST_APP_DATA_DIR: RefCell<Option<PathBuf>> = RefCell::new(None);
    static TEST_APP_CACHE_DIR: RefCell<Option<PathBuf>> = RefCell::new(None);
}

#[cfg(test)]
pub fn with_app_data_dir_for_test<T>(path: &Path, test: impl FnOnce() -> T) -> T {
    TEST_APP_DATA_DIR.with(|slot| {
        let previous = slot.replace(Some(path.to_path_buf()));
        let result = test();
        slot.replace(previous);
        result
    })
}

#[cfg(test)]
pub fn with_app_cache_dir_for_test<T>(path: &Path, test: impl FnOnce() -> T) -> T {
    TEST_APP_CACHE_DIR.with(|slot| {
        let previous = slot.replace(Some(path.to_path_buf()));
        let result = test();
        slot.replace(previous);
        result
    })
}

#[cfg(test)]
fn test_app_data_dir() -> Option<PathBuf> {
    TEST_APP_DATA_DIR.with(|slot| slot.borrow().clone())
}

#[cfg(test)]
fn test_app_cache_dir() -> Option<PathBuf> {
    TEST_APP_CACHE_DIR.with(|slot| slot.borrow().clone())
}
