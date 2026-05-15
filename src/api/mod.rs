pub mod models;

use crate::cache::{CachePolicy, HttpCache, ResourceKind, STALE_CACHE_WARNING};
use crate::config::{BASE_API_URL, GLOBAL_CONFIG_PATH, HTTP_TIMEOUT, USER_AGENT};
use crate::error::AppError;
use models::{AddonDetails, AddonSummary, CategorySummary, GameConfig, GlobalConfig};
use reqwest::{Client, Url};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct ApiClient {
    http: Client,
    cache: HttpCache,
    cache_events: Arc<Mutex<Vec<CacheEvent>>>,
}

#[derive(Debug, Clone)]
struct CacheEvent {
    stale: bool,
}

impl ApiClient {
    pub fn new() -> anyhow::Result<Self> {
        let http = Client::builder()
            .user_agent(USER_AGENT)
            .timeout(HTTP_TIMEOUT)
            .build()?;

        Ok(Self {
            http,
            cache: HttpCache::new()?,
            cache_events: Arc::new(Mutex::new(Vec::new())),
        })
    }

    pub async fn global_config(&self) -> anyhow::Result<GlobalConfig> {
        self.global_config_with_policy(CachePolicy::Use).await
    }

    pub async fn global_config_refresh(&self) -> anyhow::Result<GlobalConfig> {
        self.global_config_with_policy(CachePolicy::Refresh).await
    }

    async fn global_config_with_policy(&self, policy: CachePolicy) -> anyhow::Result<GlobalConfig> {
        let url = format!("{BASE_API_URL}/{GLOBAL_CONFIG_PATH}");
        self.get_json(&url, ResourceKind::GlobalConfig, policy)
            .await
    }

    pub async fn eso_game_config(&self) -> anyhow::Result<GameConfig> {
        self.eso_game_config_with_policy(CachePolicy::Use).await
    }

    pub async fn eso_game_config_refresh(&self) -> anyhow::Result<GameConfig> {
        self.eso_game_config_with_policy(CachePolicy::Refresh).await
    }

    async fn eso_game_config_with_policy(&self, policy: CachePolicy) -> anyhow::Result<GameConfig> {
        let global = self.global_config_with_policy(policy).await?;
        let game = global
            .games
            .into_iter()
            .find(|game| game.game_id.as_deref() == Some("ESO"))
            .ok_or(AppError::EsoGameMissing)?;

        let url = game.game_config.ok_or(AppError::EsoGameConfigMissing)?;
        self.get_json(&url, ResourceKind::GameConfig, policy).await
    }

    pub async fn eso_file_list(&self) -> anyhow::Result<Vec<AddonSummary>> {
        self.eso_file_list_with_policy(CachePolicy::Use).await
    }

    pub async fn eso_file_list_refresh(&self) -> anyhow::Result<Vec<AddonSummary>> {
        self.eso_file_list_with_policy(CachePolicy::Refresh).await
    }

    async fn eso_file_list_with_policy(
        &self,
        policy: CachePolicy,
    ) -> anyhow::Result<Vec<AddonSummary>> {
        let config = self.eso_game_config_with_policy(policy).await?;
        let url = config
            .api_feeds
            .and_then(|feeds| feeds.file_list)
            .ok_or(AppError::FeedMissing("FileList"))?;

        self.get_json(&url, ResourceKind::FileList, policy).await
    }

    pub async fn eso_category_list(&self) -> anyhow::Result<Vec<CategorySummary>> {
        self.eso_category_list_with_policy(CachePolicy::Use).await
    }

    pub async fn eso_category_list_refresh(&self) -> anyhow::Result<Vec<CategorySummary>> {
        self.eso_category_list_with_policy(CachePolicy::Refresh)
            .await
    }

    async fn eso_category_list_with_policy(
        &self,
        policy: CachePolicy,
    ) -> anyhow::Result<Vec<CategorySummary>> {
        let config = self.eso_game_config_with_policy(policy).await?;
        let url = config
            .api_feeds
            .and_then(|feeds| feeds.category_list)
            .ok_or(AppError::FeedMissing("CategoryList"))?;

        self.get_json(&url, ResourceKind::CategoryList, policy)
            .await
    }

    pub async fn eso_file_details(&self, addon_id: &str) -> anyhow::Result<AddonDetails> {
        self.eso_file_details_with_policy(addon_id, CachePolicy::Use)
            .await
    }

    pub async fn eso_file_details_fresh(&self, addon_id: &str) -> anyhow::Result<AddonDetails> {
        self.eso_file_details_with_policy(addon_id, CachePolicy::NetworkOnly)
            .await
    }

    async fn eso_file_details_with_policy(
        &self,
        addon_id: &str,
        policy: CachePolicy,
    ) -> anyhow::Result<AddonDetails> {
        let config = self.eso_game_config_with_policy(policy).await?;
        let feed = config
            .api_feeds
            .and_then(|feeds| feeds.file_details)
            .ok_or(AppError::FeedMissing("FileDetails"))?;
        let url = details_url(&feed, addon_id)?;
        let details: Vec<AddonDetails> = self
            .get_json(url.as_str(), ResourceKind::FileDetails, policy)
            .await?;

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

    pub async fn cached_bytes(
        &self,
        url: &str,
        kind: ResourceKind,
    ) -> anyhow::Result<crate::cache::CachedHttpResponse> {
        let response = self
            .cache
            .fetch(&self.http, url, kind, CachePolicy::Use)
            .await?;
        self.record_cache_event(response.stale);
        Ok(response)
    }

    pub fn cache_warning_message(&self) -> Option<String> {
        self.cache_events
            .lock()
            .ok()
            .filter(|events| events.iter().any(|event| event.stale))
            .map(|_| STALE_CACHE_WARNING.to_owned())
    }

    async fn get_json<T>(
        &self,
        url: &str,
        kind: ResourceKind,
        policy: CachePolicy,
    ) -> anyhow::Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        let response = self.cache.fetch(&self.http, url, kind, policy).await?;
        self.record_cache_event(response.stale);
        Ok(serde_json::from_slice::<T>(&response.bytes)?)
    }

    fn record_cache_event(&self, stale: bool) {
        if let Ok(mut events) = self.cache_events.lock() {
            events.push(CacheEvent { stale });
        }
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
