use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize, Serialize)]
pub struct GlobalConfig {
    #[serde(rename = "GAMES", default, deserialize_with = "de::vec_from_value")]
    pub games: Vec<GameEntry>,

    #[serde(default, flatten)]
    pub _extra: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GameEntry {
    #[serde(rename = "GameID", default, deserialize_with = "de::optional_string")]
    pub game_id: Option<String>,

    #[serde(
        rename = "GameConfig",
        default,
        deserialize_with = "de::optional_string"
    )]
    pub game_config: Option<String>,

    #[serde(default, flatten)]
    pub _extra: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GameConfig {
    #[serde(
        rename = "WebsiteTitle",
        default,
        deserialize_with = "de::optional_string"
    )]
    pub website_title: Option<String>,

    #[serde(
        rename = "WebsiteURL",
        default,
        deserialize_with = "de::optional_string"
    )]
    pub website_url: Option<String>,

    #[serde(rename = "GameName", default, deserialize_with = "de::optional_string")]
    pub game_name: Option<String>,

    #[serde(
        rename = "APIFeeds",
        default,
        deserialize_with = "de::optional_from_value"
    )]
    pub api_feeds: Option<ApiFeeds>,

    #[serde(default, flatten)]
    pub _extra: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ApiFeeds {
    #[serde(rename = "FileList", default, deserialize_with = "de::optional_string")]
    pub file_list: Option<String>,

    #[serde(
        rename = "FileDetails",
        default,
        deserialize_with = "de::optional_string"
    )]
    pub file_details: Option<String>,

    #[serde(
        rename = "ListFiles",
        default,
        deserialize_with = "de::optional_string"
    )]
    pub list_files: Option<String>,

    #[serde(
        rename = "CategoryList",
        default,
        deserialize_with = "de::optional_string"
    )]
    pub category_list: Option<String>,

    #[serde(default, flatten)]
    pub _extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AddonSummary {
    #[serde(rename = "UID", default, deserialize_with = "de::optional_string")]
    pub uid: Option<String>,

    #[serde(rename = "UIName", default, deserialize_with = "de::optional_string")]
    pub name: Option<String>,

    #[serde(
        rename = "UIAuthorName",
        default,
        deserialize_with = "de::optional_string"
    )]
    pub author_name: Option<String>,

    #[serde(
        rename = "UIVersion",
        default,
        deserialize_with = "de::optional_string"
    )]
    pub version: Option<String>,

    #[serde(rename = "UIDate", default, deserialize_with = "de::optional_i64")]
    pub date: Option<i64>,

    #[serde(
        rename = "UIFileInfoURL",
        default,
        deserialize_with = "de::optional_string"
    )]
    pub file_info_url: Option<String>,

    #[serde(
        rename = "UIDescription",
        default,
        deserialize_with = "de::optional_string"
    )]
    pub description: Option<String>,

    #[serde(
        rename = "UISummary",
        default,
        deserialize_with = "de::optional_string"
    )]
    pub summary: Option<String>,

    #[serde(rename = "UIDir", default, deserialize_with = "de::string_vec")]
    pub directories: Vec<String>,

    #[serde(default, flatten)]
    pub _extra: BTreeMap<String, Value>,
}

impl AddonSummary {
    pub fn searchable_text(&self) -> String {
        [
            self.name.as_deref(),
            self.author_name.as_deref(),
            self.summary.as_deref(),
            self.description.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
    }

    pub fn category_id(&self) -> Option<String> {
        extra_string(
            &self._extra,
            &[
                "UICategoryID",
                "UICategoryId",
                "CategoryID",
                "CategoryId",
                "category_id",
            ],
        )
    }

    pub fn category_name(&self) -> Option<String> {
        extra_string(
            &self._extra,
            &[
                "UICategoryName",
                "CategoryName",
                "UICategory",
                "Category",
                "category_name",
            ],
        )
    }

    pub fn downloads(&self) -> Option<i64> {
        extra_i64(
            &self._extra,
            &[
                "UIDownloads",
                "UIDownloadCount",
                "UIDownloadTotal",
                "Downloads",
                "DownloadCount",
                "TotalDownloads",
            ],
        )
    }

    pub fn monthly_downloads(&self) -> Option<i64> {
        extra_i64(
            &self._extra,
            &[
                "UIMonthlyDownloads",
                "UIDownloadsMonthly",
                "MonthlyDownloads",
                "MonthlyDownloadCount",
            ],
        )
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AddonDetails {
    #[serde(rename = "UID", default, deserialize_with = "de::optional_string")]
    pub uid: Option<String>,

    #[serde(rename = "UIName", default, deserialize_with = "de::optional_string")]
    pub name: Option<String>,

    #[serde(
        rename = "UIAuthorName",
        default,
        deserialize_with = "de::optional_string"
    )]
    pub author_name: Option<String>,

    #[serde(
        rename = "UIVersion",
        default,
        deserialize_with = "de::optional_string"
    )]
    pub version: Option<String>,

    #[serde(rename = "UIDate", default, deserialize_with = "de::optional_i64")]
    pub date: Option<i64>,

    #[serde(
        rename = "UIFileName",
        default,
        deserialize_with = "de::optional_string"
    )]
    pub file_name: Option<String>,

    #[serde(rename = "UIMD5", default, deserialize_with = "de::optional_string")]
    pub md5: Option<String>,

    #[serde(
        rename = "UIDownload",
        default,
        deserialize_with = "de::optional_string"
    )]
    pub download_url: Option<String>,

    #[serde(
        rename = "UIFileInfoURL",
        default,
        deserialize_with = "de::optional_string"
    )]
    pub file_info_url: Option<String>,

    #[serde(
        rename = "UIDescription",
        default,
        deserialize_with = "de::optional_string"
    )]
    pub description: Option<String>,

    #[serde(
        rename = "UIChangeLog",
        default,
        deserialize_with = "de::optional_string"
    )]
    pub changelog: Option<String>,

    #[serde(default, flatten)]
    pub _extra: BTreeMap<String, Value>,
}

impl AddonDetails {
    pub fn category_id(&self) -> Option<String> {
        extra_string(
            &self._extra,
            &[
                "UICategoryID",
                "UICategoryId",
                "CategoryID",
                "CategoryId",
                "category_id",
            ],
        )
    }

    pub fn category_name(&self) -> Option<String> {
        extra_string(
            &self._extra,
            &[
                "UICategoryName",
                "CategoryName",
                "UICategory",
                "Category",
                "category_name",
            ],
        )
    }

    pub fn downloads(&self) -> Option<i64> {
        extra_i64(
            &self._extra,
            &[
                "UIDownloads",
                "UIDownloadCount",
                "UIDownloadTotal",
                "Downloads",
                "DownloadCount",
                "TotalDownloads",
            ],
        )
    }

    pub fn monthly_downloads(&self) -> Option<i64> {
        extra_i64(
            &self._extra,
            &[
                "UIMonthlyDownloads",
                "UIDownloadsMonthly",
                "MonthlyDownloads",
                "MonthlyDownloadCount",
            ],
        )
    }
}

fn extra_string(extra: &BTreeMap<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        extra.get(*key).and_then(|value| match value {
            Value::String(value) if !value.trim().is_empty() => Some(value.clone()),
            Value::Number(value) => Some(value.to_string()),
            Value::Bool(value) => Some(value.to_string()),
            _ => None,
        })
    })
}

fn extra_i64(extra: &BTreeMap<String, Value>, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|key| {
        extra.get(*key).and_then(|value| match value {
            Value::Number(value) => value
                .as_i64()
                .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok())),
            Value::String(value) => value.trim().parse::<i64>().ok(),
            _ => None,
        })
    })
}

mod de {
    use serde::de::DeserializeOwned;
    use serde::{Deserialize, Deserializer};
    use serde_json::Value;

    pub fn optional_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Option::<Value>::deserialize(deserializer)?;
        Ok(match value {
            None | Some(Value::Null) => None,
            Some(Value::String(value)) if value.trim().is_empty() => None,
            Some(Value::String(value)) => Some(value),
            Some(Value::Number(value)) => Some(value.to_string()),
            Some(Value::Bool(value)) => Some(value.to_string()),
            Some(_) => None,
        })
    }

    pub fn optional_i64<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Option::<Value>::deserialize(deserializer)?;
        Ok(match value {
            None | Some(Value::Null) => None,
            Some(Value::Number(value)) => value
                .as_i64()
                .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok())),
            Some(Value::String(value)) => value.trim().parse::<i64>().ok(),
            Some(_) => None,
        })
    }

    pub fn optional_from_value<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
    where
        D: Deserializer<'de>,
        T: DeserializeOwned,
    {
        let value = Option::<Value>::deserialize(deserializer)?;
        Ok(value.and_then(|value| serde_json::from_value(value).ok()))
    }

    pub fn vec_from_value<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
    where
        D: Deserializer<'de>,
        T: DeserializeOwned,
    {
        let value = Option::<Value>::deserialize(deserializer)?;
        Ok(match value {
            Some(Value::Array(items)) => items
                .into_iter()
                .filter_map(|item| serde_json::from_value(item).ok())
                .collect(),
            Some(value) => serde_json::from_value(value)
                .map(|item| vec![item])
                .unwrap_or_default(),
            None => Vec::new(),
        })
    }

    pub fn string_vec<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Option::<Value>::deserialize(deserializer)?;
        Ok(match value {
            Some(Value::Array(items)) => items.into_iter().filter_map(value_to_string).collect(),
            Some(value) => value_to_string(value).into_iter().collect(),
            None => Vec::new(),
        })
    }

    fn value_to_string(value: Value) -> Option<String> {
        match value {
            Value::Null => None,
            Value::String(value) if value.trim().is_empty() => None,
            Value::String(value) => Some(value),
            Value::Number(value) => Some(value.to_string()),
            Value::Bool(value) => Some(value.to_string()),
            _ => None,
        }
    }
}
