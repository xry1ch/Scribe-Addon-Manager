use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(test)]
use std::io::{Read, Write};
#[cfg(test)]
use std::net::TcpListener;
#[cfg(test)]
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Context};
use directories::ProjectDirs;
use reqwest::header::{ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED};
use reqwest::{Client, StatusCode, Url};
use serde::{Deserialize, Serialize};

pub const STALE_CACHE_WARNING: &str = "Showing cached data. Could not refresh from server.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CachePolicy {
    Use,
    Refresh,
    NetworkOnly,
}

impl CachePolicy {
    fn allows_stale_fallback(self) -> bool {
        matches!(self, Self::Use | Self::Refresh)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    GlobalConfig,
    GameConfig,
    FileList,
    CategoryList,
    FileDetails,
    Image,
    CategorySprite,
}

impl ResourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GlobalConfig => "globalconfig",
            Self::GameConfig => "gameconfig",
            Self::FileList => "filelist",
            Self::CategoryList => "categorylist",
            Self::FileDetails => "filedetails",
            Self::Image => "images",
            Self::CategorySprite => "category-sprite",
        }
    }

    pub fn ttl(self) -> Duration {
        match self {
            Self::GlobalConfig => Duration::from_secs(24 * 60 * 60),
            Self::GameConfig => Duration::from_secs(24 * 60 * 60),
            Self::FileList => Duration::from_secs(30 * 60),
            Self::CategoryList => Duration::from_secs(24 * 60 * 60),
            Self::FileDetails => Duration::from_secs(10 * 60),
            Self::Image => Duration::from_secs(7 * 24 * 60 * 60),
            Self::CategorySprite => Duration::from_secs(30 * 24 * 60 * 60),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CachedHttpResponse {
    pub bytes: Vec<u8>,
    pub from_cache: bool,
    pub stale: bool,
    pub content_type: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HttpCacheStats {
    pub cache_dir: PathBuf,
    pub entry_count: usize,
    pub byte_size: u64,
}

#[derive(Debug, Clone)]
pub struct HttpCache {
    root: PathBuf,
}

#[derive(Debug, Clone)]
struct EntryPaths {
    body: PathBuf,
    metadata: PathBuf,
}

#[derive(Debug, Clone)]
struct CachedEntry {
    metadata: CacheMetadata,
    body: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheMetadata {
    version: u8,
    kind: String,
    url: String,
    fetched_at: i64,
    expires_at: i64,
    content_type: Option<String>,
    etag: Option<String>,
    last_modified: Option<String>,
    byte_size: u64,
    body_sha256: String,
}

impl HttpCache {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            root: default_cache_dir(),
        })
    }

    pub fn with_root(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub async fn fetch(
        &self,
        http: &Client,
        url: &str,
        kind: ResourceKind,
        policy: CachePolicy,
    ) -> anyhow::Result<CachedHttpResponse> {
        let normalized_url = normalize_url(url)?;
        let paths = self.entry_paths(kind, &normalized_url);
        let cached = read_cached_entry(&paths);
        let now = now_timestamp();

        if policy == CachePolicy::Use {
            if let Some(entry) = cached
                .as_ref()
                .filter(|entry| entry.metadata.expires_at > now)
            {
                return Ok(CachedHttpResponse {
                    bytes: entry.body.clone(),
                    from_cache: true,
                    stale: false,
                    content_type: entry.metadata.content_type.clone(),
                });
            }
        }

        match self
            .fetch_network(http, &normalized_url, kind, cached.as_ref())
            .await
        {
            Ok(response) => Ok(response),
            Err(error) if policy.allows_stale_fallback() => {
                if let Some(entry) = cached {
                    Ok(CachedHttpResponse {
                        bytes: entry.body,
                        from_cache: true,
                        stale: true,
                        content_type: entry.metadata.content_type,
                    })
                } else {
                    Err(error)
                }
            }
            Err(error) => Err(error),
        }
    }

    async fn fetch_network(
        &self,
        http: &Client,
        normalized_url: &str,
        kind: ResourceKind,
        cached: Option<&CachedEntry>,
    ) -> anyhow::Result<CachedHttpResponse> {
        let has_validators = cached.is_some_and(|entry| {
            entry.metadata.etag.is_some() || entry.metadata.last_modified.is_some()
        });

        if has_validators {
            match send_get(http, normalized_url, cached).await {
                Ok(NetworkResponse::NotModified(headers)) => {
                    let entry = cached.expect("validators require cached entry");
                    let metadata = refreshed_metadata(&entry.metadata, kind, headers);
                    write_cache_entry(
                        &self.entry_paths(kind, normalized_url),
                        &metadata,
                        &entry.body,
                    )?;
                    return Ok(CachedHttpResponse {
                        bytes: entry.body.clone(),
                        from_cache: true,
                        stale: false,
                        content_type: metadata.content_type,
                    });
                }
                Ok(NetworkResponse::Body { bytes, headers }) => {
                    return self.store_response(kind, normalized_url, bytes, headers);
                }
                Err(_) => {}
            }
        }

        match send_get(http, normalized_url, None).await? {
            NetworkResponse::NotModified(_) => {
                let entry = cached.ok_or_else(|| anyhow!("server returned 304 without cache"))?;
                Ok(CachedHttpResponse {
                    bytes: entry.body.clone(),
                    from_cache: true,
                    stale: false,
                    content_type: entry.metadata.content_type.clone(),
                })
            }
            NetworkResponse::Body { bytes, headers } => {
                self.store_response(kind, normalized_url, bytes, headers)
            }
        }
    }

    fn store_response(
        &self,
        kind: ResourceKind,
        normalized_url: &str,
        bytes: Vec<u8>,
        headers: ResponseHeaders,
    ) -> anyhow::Result<CachedHttpResponse> {
        let metadata = new_metadata(kind, normalized_url, &bytes, headers.clone());
        write_cache_entry(&self.entry_paths(kind, normalized_url), &metadata, &bytes)?;
        Ok(CachedHttpResponse {
            bytes,
            from_cache: false,
            stale: false,
            content_type: metadata.content_type,
        })
    }

    pub fn clear(&self) -> anyhow::Result<()> {
        if self.root.exists() {
            fs::remove_dir_all(&self.root)
                .with_context(|| format!("failed to remove {}", self.root.display()))?;
        }
        fs::create_dir_all(&self.root)
            .with_context(|| format!("failed to create {}", self.root.display()))?;
        Ok(())
    }

    pub fn stats(&self) -> anyhow::Result<HttpCacheStats> {
        let mut stats = HttpCacheStats {
            cache_dir: self.root.clone(),
            entry_count: 0,
            byte_size: 0,
        };

        if !self.root.exists() {
            return Ok(stats);
        }

        collect_stats(&self.root, &mut stats)?;
        Ok(stats)
    }

    fn entry_paths(&self, kind: ResourceKind, normalized_url: &str) -> EntryPaths {
        let key = cache_key(kind, normalized_url);
        let dir = self.root.join(kind.as_str());
        EntryPaths {
            body: dir.join(format!("{key}.body")),
            metadata: dir.join(format!("{key}.json")),
        }
    }
}

fn default_cache_dir() -> PathBuf {
    ProjectDirs::from("dev", "eso-addon-manager", "Scribe Addon Manager")
        .map(|dirs| dirs.cache_dir().join("http-cache"))
        .unwrap_or_else(|| PathBuf::from("Scribe Addon Manager").join("http-cache"))
}

fn collect_stats(path: &Path, stats: &mut HttpCacheStats) -> anyhow::Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        let path = entry.path();
        if metadata.is_dir() {
            collect_stats(&path, stats)?;
            continue;
        }

        stats.byte_size += metadata.len();
        if path.extension().and_then(|value| value.to_str()) == Some("json") {
            stats.entry_count += 1;
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct ResponseHeaders {
    content_type: Option<String>,
    etag: Option<String>,
    last_modified: Option<String>,
}

enum NetworkResponse {
    Body {
        bytes: Vec<u8>,
        headers: ResponseHeaders,
    },
    NotModified(ResponseHeaders),
}

async fn send_get(
    http: &Client,
    normalized_url: &str,
    cached: Option<&CachedEntry>,
) -> anyhow::Result<NetworkResponse> {
    let mut request = http.get(normalized_url);
    if let Some(metadata) = cached.map(|entry| &entry.metadata) {
        if let Some(etag) = metadata.etag.as_deref() {
            request = request.header(IF_NONE_MATCH, etag);
        }
        if let Some(last_modified) = metadata.last_modified.as_deref() {
            request = request.header(IF_MODIFIED_SINCE, last_modified);
        }
    }

    let response = request.send().await?;
    let status = response.status();
    let headers = response_headers(response.headers());

    if status == StatusCode::NOT_MODIFIED {
        return Ok(NetworkResponse::NotModified(headers));
    }
    if !status.is_success() {
        return Err(anyhow!("HTTP GET {normalized_url} failed with {status}"));
    }

    let expected_len = response.content_length();
    let bytes = response.bytes().await?.to_vec();
    if let Some(expected_len) = expected_len {
        if expected_len != bytes.len() as u64 {
            return Err(anyhow!(
                "HTTP GET {normalized_url} returned {} bytes, expected {expected_len}",
                bytes.len()
            ));
        }
    }

    Ok(NetworkResponse::Body { bytes, headers })
}

fn response_headers(headers: &reqwest::header::HeaderMap) -> ResponseHeaders {
    ResponseHeaders {
        content_type: headers
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned),
        etag: headers
            .get(ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned),
        last_modified: headers
            .get(LAST_MODIFIED)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned),
    }
}

fn refreshed_metadata(
    existing: &CacheMetadata,
    kind: ResourceKind,
    headers: ResponseHeaders,
) -> CacheMetadata {
    let now = now_timestamp();
    CacheMetadata {
        fetched_at: now,
        expires_at: now + duration_secs_i64(kind.ttl()),
        content_type: headers
            .content_type
            .or_else(|| existing.content_type.clone()),
        etag: headers.etag.or_else(|| existing.etag.clone()),
        last_modified: headers
            .last_modified
            .or_else(|| existing.last_modified.clone()),
        ..existing.clone()
    }
}

fn new_metadata(
    kind: ResourceKind,
    normalized_url: &str,
    bytes: &[u8],
    headers: ResponseHeaders,
) -> CacheMetadata {
    let now = now_timestamp();
    CacheMetadata {
        version: 1,
        kind: kind.as_str().to_owned(),
        url: normalized_url.to_owned(),
        fetched_at: now,
        expires_at: now + duration_secs_i64(kind.ttl()),
        content_type: headers.content_type,
        etag: headers.etag,
        last_modified: headers.last_modified,
        byte_size: bytes.len() as u64,
        body_sha256: sha256_hex(bytes),
    }
}

fn write_cache_entry(
    paths: &EntryPaths,
    metadata: &CacheMetadata,
    body: &[u8],
) -> anyhow::Result<()> {
    let parent = paths
        .body
        .parent()
        .ok_or_else(|| anyhow!("cache body path has no parent"))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("failed to create cache directory {}", parent.display()))?;

    let body_tmp = paths.body.with_extension("body.tmp");
    let metadata_tmp = paths.metadata.with_extension("json.tmp");
    fs::write(&body_tmp, body)?;
    fs::write(&metadata_tmp, serde_json::to_vec_pretty(metadata)?)?;
    replace_file(&body_tmp, &paths.body)?;
    replace_file(&metadata_tmp, &paths.metadata)?;
    Ok(())
}

fn replace_file(source: &Path, target: &Path) -> anyhow::Result<()> {
    if target.exists() {
        fs::remove_file(target)?;
    }
    fs::rename(source, target)?;
    Ok(())
}

fn read_cached_entry(paths: &EntryPaths) -> Option<CachedEntry> {
    let metadata = fs::read_to_string(&paths.metadata).ok()?;
    let metadata = serde_json::from_str::<CacheMetadata>(&metadata).ok()?;
    let body = fs::read(&paths.body).ok()?;

    if metadata.byte_size != body.len() as u64 {
        return None;
    }
    if metadata.body_sha256 != sha256_hex(&body) {
        return None;
    }

    Some(CachedEntry { metadata, body })
}

fn normalize_url(value: &str) -> anyhow::Result<String> {
    let mut url = Url::parse(value.trim())?;
    match url.scheme() {
        "http" | "https" => {}
        scheme => return Err(anyhow!("unsupported URL scheme for HTTP cache: {scheme}")),
    }
    url.set_fragment(None);
    Ok(url.to_string())
}

fn cache_key(kind: ResourceKind, normalized_url: &str) -> String {
    sha256_hex(format!("{}\n{normalized_url}", kind.as_str()).as_bytes())
}

fn duration_secs_i64(duration: Duration) -> i64 {
    i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
}

fn now_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn sha256_hex(input: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h = [
        0x6a09e667u32,
        0xbb67ae85,
        0x3c6ef372,
        0xa54ff53a,
        0x510e527f,
        0x9b05688c,
        0x1f83d9ab,
        0x5be0cd19,
    ];

    let bit_len = (input.len() as u64) * 8;
    let mut message = input.to_vec();
    message.push(0x80);
    while (message.len() % 64) != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in message.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (index, word) in w.iter_mut().take(16).enumerate() {
            let offset = index * 4;
            *word = u32::from_be_bytes([
                chunk[offset],
                chunk[offset + 1],
                chunk[offset + 2],
                chunk[offset + 3],
            ]);
        }
        for index in 16..64 {
            let s0 = w[index - 15].rotate_right(7)
                ^ w[index - 15].rotate_right(18)
                ^ (w[index - 15] >> 3);
            let s1 = w[index - 2].rotate_right(17)
                ^ w[index - 2].rotate_right(19)
                ^ (w[index - 2] >> 10);
            w[index] = w[index - 16]
                .wrapping_add(s0)
                .wrapping_add(w[index - 7])
                .wrapping_add(s1);
        }

        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut hh = h[7];

        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[index])
                .wrapping_add(w[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    h.iter().map(|word| format!("{word:08x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cache_miss_fetches_and_stores_response() {
        let server = TestServer::new(vec![TestResponse::ok(
            b"{\"ok\":true}".to_vec(),
            &[("content-type", "application/json")],
        )]);
        let cache = temp_cache();
        let http = Client::new();

        let response = cache
            .fetch(
                &http,
                &server.url(),
                ResourceKind::FileList,
                CachePolicy::Use,
            )
            .await
            .expect("fetch");

        assert!(!response.from_cache);
        assert_eq!(response.bytes, b"{\"ok\":true}");
        assert_eq!(cache.stats().expect("stats").entry_count, 1);
        assert_eq!(server.requests().len(), 1);
    }

    #[tokio::test]
    async fn fresh_cache_hit_returns_cached_response_without_network() {
        let server = TestServer::new(vec![TestResponse::ok(b"fresh".to_vec(), &[])]);
        let cache = temp_cache();
        let http = Client::new();

        cache
            .fetch(
                &http,
                &server.url(),
                ResourceKind::FileList,
                CachePolicy::Use,
            )
            .await
            .expect("first fetch");
        let response = cache
            .fetch(
                &http,
                &server.url(),
                ResourceKind::FileList,
                CachePolicy::Use,
            )
            .await
            .expect("cached fetch");

        assert!(response.from_cache);
        assert!(!response.stale);
        assert_eq!(response.bytes, b"fresh");
        assert_eq!(server.requests().len(), 1);
    }

    #[tokio::test]
    async fn expired_cache_attempts_network_refresh() {
        let server = TestServer::new(vec![
            TestResponse::ok(b"old".to_vec(), &[]),
            TestResponse::ok(b"new".to_vec(), &[]),
        ]);
        let cache = temp_cache();
        let http = Client::new();
        let url = server.url();

        cache
            .fetch(&http, &url, ResourceKind::FileDetails, CachePolicy::Use)
            .await
            .expect("first fetch");
        expire(&cache, ResourceKind::FileDetails, &url);
        let response = cache
            .fetch(&http, &url, ResourceKind::FileDetails, CachePolicy::Use)
            .await
            .expect("refresh");

        assert!(!response.from_cache);
        assert_eq!(response.bytes, b"new");
        assert_eq!(server.requests().len(), 2);
    }

    #[tokio::test]
    async fn not_modified_response_reuses_cached_body() {
        let server = TestServer::new(vec![
            TestResponse::ok(b"cached".to_vec(), &[("etag", "\"one\"")]),
            TestResponse::status(304, Vec::new(), &[("etag", "\"one\"")]),
        ]);
        let cache = temp_cache();
        let http = Client::new();
        let url = server.url();

        cache
            .fetch(&http, &url, ResourceKind::FileDetails, CachePolicy::Use)
            .await
            .expect("first fetch");
        expire(&cache, ResourceKind::FileDetails, &url);
        let response = cache
            .fetch(&http, &url, ResourceKind::FileDetails, CachePolicy::Use)
            .await
            .expect("304 fetch");

        assert!(response.from_cache);
        assert!(!response.stale);
        assert_eq!(response.bytes, b"cached");
        assert!(
            server.requests()[1].contains("if-none-match: \"one\""),
            "conditional header missing: {}",
            server.requests()[1]
        );
    }

    #[tokio::test]
    async fn network_failure_with_stale_cache_returns_stale_body() {
        let server = TestServer::new(vec![TestResponse::ok(b"stale".to_vec(), &[])]);
        let cache = temp_cache();
        let http = Client::new();
        let url = server.url();

        cache
            .fetch(&http, &url, ResourceKind::FileDetails, CachePolicy::Use)
            .await
            .expect("first fetch");
        expire(&cache, ResourceKind::FileDetails, &url);
        server.join();

        let response = cache
            .fetch(&http, &url, ResourceKind::FileDetails, CachePolicy::Use)
            .await
            .expect("stale fallback");

        assert!(response.from_cache);
        assert!(response.stale);
        assert_eq!(response.bytes, b"stale");
    }

    #[tokio::test]
    async fn network_failure_without_cache_errors() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let url = format!("http://{}/missing", listener.local_addr().expect("addr"));
        drop(listener);
        let cache = temp_cache();
        let http = Client::new();

        let result = cache
            .fetch(&http, &url, ResourceKind::FileDetails, CachePolicy::Use)
            .await;

        assert!(result.is_err());
    }

    #[test]
    fn clear_cache_removes_cache_files() {
        let cache = temp_cache();
        let kind = ResourceKind::FileList;
        let url = "https://example.com/list.json";
        let paths = cache.entry_paths(kind, &normalize_url(url).expect("url"));
        write_cache_entry(
            &paths,
            &new_metadata(
                kind,
                url,
                b"body",
                ResponseHeaders {
                    content_type: None,
                    etag: None,
                    last_modified: None,
                },
            ),
            b"body",
        )
        .expect("write");

        assert_eq!(cache.stats().expect("stats").entry_count, 1);
        cache.clear().expect("clear");
        assert_eq!(cache.stats().expect("stats").entry_count, 0);
    }

    #[test]
    fn cache_key_is_stable_and_url_safe() {
        let key = cache_key(
            ResourceKind::FileList,
            &normalize_url("https://example.com/a%20b.json#frag").expect("url"),
        );

        assert_eq!(key.len(), 64);
        assert!(key.chars().all(|ch| ch.is_ascii_hexdigit()));
        assert_eq!(
            key,
            cache_key(
                ResourceKind::FileList,
                &normalize_url("https://example.com/a%20b.json").expect("url")
            )
        );
    }

    #[test]
    fn ttl_selection_matches_resource_kind() {
        assert_eq!(ResourceKind::FileList.ttl(), Duration::from_secs(30 * 60));
        assert_eq!(
            ResourceKind::FileDetails.ttl(),
            Duration::from_secs(10 * 60)
        );
        assert_eq!(
            ResourceKind::Image.ttl(),
            Duration::from_secs(7 * 24 * 60 * 60)
        );
    }

    #[test]
    fn sha256_matches_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    fn temp_cache() -> HttpCache {
        #[allow(deprecated)]
        let root = tempfile::tempdir()
            .expect("tempdir")
            .into_path()
            .join("http-cache");
        HttpCache::with_root(root)
    }

    fn expire(cache: &HttpCache, kind: ResourceKind, url: &str) {
        let normalized_url = normalize_url(url).expect("url");
        let paths = cache.entry_paths(kind, &normalized_url);
        let content = fs::read_to_string(&paths.metadata).expect("metadata");
        let mut metadata: CacheMetadata = serde_json::from_str(&content).expect("json");
        metadata.expires_at = 0;
        fs::write(
            &paths.metadata,
            serde_json::to_vec_pretty(&metadata).expect("json"),
        )
        .expect("write metadata");
    }

    struct TestResponse {
        status: u16,
        body: Vec<u8>,
        headers: Vec<(String, String)>,
    }

    impl TestResponse {
        fn ok(body: Vec<u8>, headers: &[(&str, &str)]) -> Self {
            Self::status(200, body, headers)
        }

        fn status(status: u16, body: Vec<u8>, headers: &[(&str, &str)]) -> Self {
            Self {
                status,
                body,
                headers: headers
                    .iter()
                    .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                    .collect(),
            }
        }
    }

    struct TestServer {
        url: String,
        requests: Arc<Mutex<Vec<String>>>,
        handle: Option<std::thread::JoinHandle<()>>,
    }

    impl TestServer {
        fn new(responses: Vec<TestResponse>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
            let address = listener.local_addr().expect("addr");
            let requests = Arc::new(Mutex::new(Vec::new()));
            let request_log = Arc::clone(&requests);
            let handle = std::thread::spawn(move || {
                for response in responses {
                    let Ok((mut stream, _)) = listener.accept() else {
                        break;
                    };
                    let mut buffer = [0u8; 4096];
                    let read = stream.read(&mut buffer).unwrap_or(0);
                    let request = String::from_utf8_lossy(&buffer[..read]).to_lowercase();
                    request_log.lock().expect("requests").push(request);

                    let reason = if response.status == 304 {
                        "Not Modified"
                    } else {
                        "OK"
                    };
                    let mut header_text = format!(
                        "HTTP/1.1 {} {reason}\r\ncontent-length: {}\r\nconnection: close\r\n",
                        response.status,
                        response.body.len()
                    );
                    for (name, value) in response.headers {
                        header_text.push_str(&format!("{name}: {value}\r\n"));
                    }
                    header_text.push_str("\r\n");
                    stream.write_all(header_text.as_bytes()).expect("headers");
                    stream.write_all(&response.body).expect("body");
                }
            });

            Self {
                url: format!("http://{address}/resource.json"),
                requests,
                handle: Some(handle),
            }
        }

        fn url(&self) -> String {
            self.url.clone()
        }

        fn requests(&self) -> Vec<String> {
            self.requests.lock().expect("requests").clone()
        }

        fn join(mut self) {
            if let Some(handle) = self.handle.take() {
                handle.join().expect("server thread");
            }
        }
    }

    impl Drop for TestServer {
        fn drop(&mut self) {
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
        }
    }
}
