//! GBFS auto-discovery (`gbfs.json`) resolver.
//!
//! Fetches the discovery document once, caches the `{feed_name -> url}` map for
//! the configured language, and resolves individual feed URLs on demand. The
//! cache is shared across all per-feed fetchers so `gbfs.json` is only fetched
//! once (lazily, single-flighted) regardless of how many feeds are polled.

use std::{collections::HashMap, sync::LazyLock, time::Duration};

use serde::Deserialize;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, trace, warn};
use url::Url;

use crate::{admin, cli::Config, http_client::HTTP_CLIENT};

#[derive(Debug, Deserialize)]
struct GbfsRoot {
    #[serde(default)]
    data: HashMap<String, LanguageFeeds>,
}

#[derive(Debug, Deserialize, Clone)]
struct LanguageFeeds {
    #[serde(default)]
    feeds: Vec<DiscoveredFeed>,
}

#[derive(Debug, Deserialize, Clone)]
struct DiscoveredFeed {
    name: String,
    url: String,
}

static MAP: LazyLock<RwLock<Option<HashMap<String, Url>>>> = LazyLock::new(|| RwLock::new(None));
static LOAD_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// Drop the cached `{feed_name -> url}` map so the next [`resolve_feed_url`]
/// call re-fetches `gbfs.json`. Used when the admin changes `gbfs_url`.
pub async fn invalidate() {
    *MAP.write().await = None;
}

/// Resolve the URL for a single feed by name (e.g. `station_status`).
///
/// Returns `None` if the feed is genuinely absent from `gbfs.json`, or if the
/// discovery document could not be loaded (transient failure). A transient load
/// failure is never cached, so the next call will retry.
pub async fn resolve_feed_url(name: &str) -> Option<Url> {
    trace!(?name, "Resolving feed URL");
    loop {
        {
            let read = MAP.read().await;
            if let Some(map) = read.as_ref() {
                trace!(?name, "Found feed URL in cache");
                return map.get(name).cloned();
            }
        }

        let _guard = LOAD_LOCK.lock().await;

        // Double-check after acquiring the lock: another task may have loaded it.
        {
            let read = MAP.read().await;
            if read.is_some() {
                continue;
            }
        }

        match load().await {
            Ok(map) => {
                debug!(feeds = ?map.keys().collect::<Vec<_>>(), "Loaded GBFS discovery map");
                *MAP.write().await = Some(map);
            }
            Err(()) => return None,
        }
    }
}

#[tracing::instrument(skip_all, fields(url = ?tracing::field::Empty, response_status = ?tracing::field::Empty))]
async fn load() -> Result<HashMap<String, Url>, ()> {
    let url = admin::ADMIN_SETTINGS
        .read()
        .await
        .gbfs_url
        .clone()
        .unwrap_or_else(|| {
            Config::global()
                .global
                .data_fetcher
                .gbfs_fetch_endpoint
                .clone()
        });
    tracing::Span::current().record("url", url.as_str());

    trace!("Fetching gbfs.json discovery document");

    let response = match HTTP_CLIENT
        .get(url)
        .timeout(Duration::from_secs(15))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            warn!(error = %e, "Failed to fetch gbfs.json");
            return Err(());
        }
    };

    trace!(status = ?response.status(), "Got gbfs.json response");

    if let Err(e) = response.error_for_status_ref() {
        warn!(error = %e, "gbfs.json returned an error status");
        return Err(());
    }

    let root = {
        let bytes = match super::fetch_bytes_capped(response, 30 * 1024 * 1024).await {
            Ok(bytes) => bytes,
            Err(e) => {
                warn!(error = %e, "Failed to read gbfs.json body");
                return Err(());
            }
        };

        match serde_json::from_slice::<GbfsRoot>(&bytes) {
            Ok(r) => r,
            Err(e) => {
                warn!(error = %e, "Failed to decode gbfs.json");
                return Err(());
            }
        }
    };

    trace!("Decoded gbfs.json");

    let desired_language = Config::global().global.data_fetcher.gbfs_language.clone();
    let available_languages = root.data.keys().cloned().collect::<Vec<_>>();

    let (feeds, used_fallback) = match root.data.get(&desired_language) {
        Some(feeds) => (Some(feeds.clone()), false),
        None => (root.data.into_values().next(), true),
    };

    if used_fallback {
        warn!(
            desired_language,
            available = ?available_languages,
            "Configured GBFS language not found, falling back to first available"
        );
    }

    let Some(feeds) = feeds else {
        warn!("gbfs.json contained no language feeds");
        return Err(());
    };

    let map = feeds
        .feeds
        .into_iter()
        .filter_map(|feed| match Url::parse(&feed.url) {
            Ok(url) => Some((feed.name, url)),
            Err(e) => {
                warn!(name = feed.name, url = feed.url, error = %e, "Skipping feed with invalid URL");
                None
            }
        })
        .collect::<HashMap<_, _>>();

    trace!(?map, "Built feed URL map");

    Ok(map)
}
