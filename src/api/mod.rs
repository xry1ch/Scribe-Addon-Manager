pub mod models;

use crate::config::{BASE_API_URL, GLOBAL_CONFIG_PATH, HTTP_TIMEOUT, USER_AGENT};
use crate::error::AppError;
use models::{AddonDetails, AddonSummary, GameConfig, GlobalConfig};
use reqwest::{Client, Url};

#[derive(Clone)]
pub struct ApiClient {
    http: Client,
}

impl ApiClient {
    pub fn new() -> Result<Self, reqwest::Error> {
        let http = Client::builder()
            .user_agent(USER_AGENT)
            .timeout(HTTP_TIMEOUT)
            .build()?;

        Ok(Self { http })
    }

    pub async fn global_config(&self) -> Result<GlobalConfig, reqwest::Error> {
        let url = format!("{BASE_API_URL}/{GLOBAL_CONFIG_PATH}");
        self.get_json(&url).await
    }

    pub async fn eso_game_config(&self) -> anyhow::Result<GameConfig> {
        let global = self.global_config().await?;
        let game = global
            .games
            .into_iter()
            .find(|game| game.game_id.as_deref() == Some("ESO"))
            .ok_or(AppError::EsoGameMissing)?;

        let url = game.game_config.ok_or(AppError::EsoGameConfigMissing)?;
        Ok(self.get_json(&url).await?)
    }

    pub async fn eso_file_list(&self) -> anyhow::Result<Vec<AddonSummary>> {
        let config = self.eso_game_config().await?;
        let url = config
            .api_feeds
            .and_then(|feeds| feeds.file_list)
            .ok_or(AppError::FeedMissing("FileList"))?;

        Ok(self.get_json(&url).await?)
    }

    pub async fn eso_file_details(&self, addon_id: &str) -> anyhow::Result<AddonDetails> {
        let config = self.eso_game_config().await?;
        let feed = config
            .api_feeds
            .and_then(|feeds| feeds.file_details)
            .ok_or(AppError::FeedMissing("FileDetails"))?;
        let url = details_url(&feed, addon_id)?;
        let details: Vec<AddonDetails> = self.get_json(url.as_str()).await?;

        Ok(details
            .into_iter()
            .next()
            .ok_or_else(|| AppError::AddonDetailsMissing(addon_id.to_owned()))?)
    }

    pub async fn download_bytes(&self, url: &str) -> Result<Vec<u8>, reqwest::Error> {
        Ok(self
            .http
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?
            .to_vec())
    }

    async fn get_json<T>(&self, url: &str) -> Result<T, reqwest::Error>
    where
        T: serde::de::DeserializeOwned,
    {
        self.http
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .json::<T>()
            .await
    }
}

fn details_url(feed_base: &str, addon_id: &str) -> anyhow::Result<Url> {
    let addon_id = addon_id.trim();
    if addon_id.is_empty()
        || addon_id.contains('/')
        || addon_id.contains('\\')
        || addon_id.contains('?')
        || addon_id.contains('#')
    {
        return Err(AppError::InvalidAddonId(addon_id.to_owned()).into());
    }

    let mut url = Url::parse(&format!("{}/", feed_base.trim_end_matches('/')))?;
    let file_name = if addon_id.ends_with(".json") {
        addon_id.to_owned()
    } else {
        format!("{addon_id}.json")
    };

    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("FileDetails feed URL cannot be used as a base URL"))?
        .pop_if_empty()
        .push(&file_name);

    Ok(url)
}
