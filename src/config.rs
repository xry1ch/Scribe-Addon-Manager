use std::time::Duration;

pub const BASE_API_URL: &str = "https://api.mmoui.com/v3";
pub const GLOBAL_CONFIG_PATH: &str = "globalconfig.json";
pub const USER_AGENT: &str = "eso-addon-manager-dev/0.1 (+local development)";
pub const HTTP_TIMEOUT: Duration = Duration::from_secs(30);
