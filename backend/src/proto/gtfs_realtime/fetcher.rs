use std::{
    sync::{
        Arc, LazyLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use prost::Message;
use tokio::sync::{Notify, watch};
use tracing::{debug, trace, warn};

use super::data::transit_realtime::FeedMessage;
use crate::{admin, cli::Config, http_client::HTTP_CLIENT};

static FEED: LazyLock<watch::Sender<Option<Arc<FeedMessage>>>> =
    LazyLock::new(|| watch::channel(None).0);
static FORCE_SYNC: LazyLock<Arc<Notify>> = LazyLock::new(|| Arc::new(Notify::new()));
static FORCE_FLAG: AtomicBool = AtomicBool::new(false);

const METADATA_NAME: &str = "gtfs_realtime_fetch";

pub fn force_sync() {
    FORCE_FLAG.store(true, Ordering::Relaxed);
    FORCE_SYNC.notify_one();
}

pub async fn fetch_feed() -> Result<FeedMessage, FetcherError> {
    let url = admin::ADMIN_SETTINGS
        .read()
        .await
        .realtime_url
        .clone()
        .unwrap_or_else(|| {
            Config::global()
                .global
                .data_fetcher
                .data_fetch_endpoint
                .clone()
        });

    debug!(url = ?url.as_str(), "Fetching feed");

    let start = Instant::now();
    let response = HTTP_CLIENT
        .get(url)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(FetcherError::FetchError)?;

    if let Err(e) = response.error_for_status_ref() {
        return Err(FetcherError::FetchError(e));
    }

    trace!(took = ?start.elapsed(), "Got feed response");

    let start_read = Instant::now();
    let body = response.bytes().await.map_err(FetcherError::FetchError)?;
    trace!(took = ?start_read.elapsed(), "Read feed body");

    let start_decode = Instant::now();
    let data = FeedMessage::decode(body).map_err(FetcherError::DecodeError)?;
    trace!(?data.header, took = ?start_decode.elapsed(), "Feed decoded successfully");

    trace!(took = ?start.elapsed(), "Feed fetched successfully");

    Ok(data)
}

pub async fn get_cached_feed() -> Option<Arc<FeedMessage>> {
    FEED.borrow().clone()
}

pub fn subscribe_feed() -> watch::Receiver<Option<Arc<FeedMessage>>> {
    FEED.subscribe()
}

pub async fn wait_for_feed_update() -> Arc<FeedMessage> {
    let mut updates = subscribe_feed();
    loop {
        let feed = updates.borrow_and_update().clone();
        if let Some(feed) = feed {
            return feed;
        }
        updates.changed().await.expect("feed sender remains alive");
    }
}

async fn fetch_and_update_feed(after_timestamp: u64, forced: bool) -> Option<u64> {
    let start = Instant::now();

    admin::metadata::write_metadata(
        METADATA_NAME,
        &admin::metadata::MetadataEntry::in_progress(),
    )
    .await;

    let feed = match fetch_feed().await {
        Ok(feed) => feed,
        Err(e) => {
            warn!(error = %e, "Failed to fetch and update feed");
            admin::metadata::write_metadata(
                METADATA_NAME,
                &admin::metadata::MetadataEntry::error()
                    .with_error_message(e.to_string())
                    .with_duration(start.elapsed()),
            )
            .await;
            return None;
        }
    };

    let timestamp = feed.header.timestamp();
    if !forced && timestamp <= after_timestamp {
        admin::metadata::write_metadata(
            METADATA_NAME,
            &admin::metadata::MetadataEntry::skipped().with_duration(start.elapsed()),
        )
        .await;
        return None;
    }

    trace!(forced, timestamp = ?timestamp, "Got newer feed");

    let entity_count = feed.entity.len();
    FEED.send_replace(Some(Arc::new(feed)));

    admin::metadata::write_metadata(
        METADATA_NAME,
        &admin::metadata::MetadataEntry::success()
            .with_duration(start.elapsed())
            .with_records_processed(entity_count as u64),
    )
    .await;

    Some(timestamp)
}

#[tracing::instrument(name = "feed_fetcher")]
pub fn spawn_feed_fetcher() {
    debug!("Spawning feed fetcher");

    tokio::task::spawn(async {
        let interval = Config::global()
            .global
            .data_fetcher
            .data_fetch_interval
            .to_duration(&jiff::Zoned::now())
            .expect("data_fetch_interval should be convertible to a duration")
            .unsigned_abs();
        trace!(interval = ?interval, "Starting feed fetcher");
        let mut previous_timestamp = 0;
        loop {
            let forced = FORCE_FLAG.swap(false, Ordering::Relaxed);
            let paused = admin::ADMIN_SETTINGS
                .read()
                .await
                .realtime_paused
                .unwrap_or(false);
            if paused && !forced {
                trace!("Realtime fetching paused, skipping");
                admin::metadata::write_metadata(
                    METADATA_NAME,
                    &admin::metadata::MetadataEntry::paused(),
                )
                .await;
            } else if let Some(new_timestamp) =
                fetch_and_update_feed(previous_timestamp, forced).await
            {
                previous_timestamp = new_timestamp;
                debug!(ts = ?new_timestamp, "Got newer feed");
            }

            tokio::select! {
                () = tokio::time::sleep(interval) => {},
                () = FORCE_SYNC.notified() => {
                    trace!("Force sync triggered");
                },
            }
        }
    });
}

#[derive(Debug, thiserror::Error)]
pub enum FetcherError {
    #[error("Failed to fetch feed: {0:?}")]
    FetchError(reqwest::Error),

    #[error("Failed to decode feed: {0:?}")]
    DecodeError(prost::DecodeError),
}
