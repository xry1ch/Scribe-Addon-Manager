use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("ESO game entry was not found in MMOUI globalconfig.json")]
    EsoGameMissing,

    #[error("ESO GameConfig URL was missing from MMOUI globalconfig.json")]
    EsoGameConfigMissing,

    #[error("required ESO feed URL was missing from gameconfig.json: {0}")]
    FeedMissing(&'static str),

    #[error("FileDetails feed returned no addon for id {0}")]
    AddonDetailsMissing(String),

    #[error("invalid addon id: {0}")]
    InvalidAddonId(String),

    #[error("download MD5 mismatch for {path}: expected {expected}, got {actual}")]
    Md5Mismatch {
        path: String,
        expected: String,
        actual: String,
    },
}
