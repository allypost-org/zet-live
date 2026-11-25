use std::{
    string::ToString,
    sync::{
        Arc, LazyLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use reqwest::{
    StatusCode,
    header::{HeaderValue, IF_MODIFIED_SINCE, IF_NONE_MATCH},
};
use tokio::sync::Notify;
use tracing::{debug, trace, warn};

use crate::{
    admin,
    cli::Config,
    database::{Database, schedule_offsets},
    proto::gtfs_schedule::data::GtfsSchedule,
};

static DATA_NOTIFICATION: LazyLock<Arc<Notify>> = LazyLock::new(|| Arc::new(Notify::new()));
static FORCE_SYNC: LazyLock<Arc<Notify>> = LazyLock::new(|| Arc::new(Notify::new()));
static FORCE_FLAG: AtomicBool = AtomicBool::new(false);

const METADATA_NAME: &str = "gtfs_static_fetch";

pub fn force_sync() {
    FORCE_FLAG.store(true, Ordering::Relaxed);
    FORCE_SYNC.notify_one();
}

pub async fn wait_for_schedule_update() {
    DATA_NOTIFICATION.notified().await;
}

#[tracing::instrument(name = "schedule_fetcher")]
pub fn spawn_schedule_fetcher() {
    debug!("Spawning schedule fetcher");

    tokio::spawn(async move {
        let interval = Config::global()
            .global
            .data_fetcher
            .schedule_fetch_interval
            .to_duration(&jiff::Zoned::now())
            .expect("schedule_fetch_interval should be convertible to a duration")
            .unsigned_abs();

        trace!(interval = ?interval, "Starting schedule fetcher");
        loop {
            let forced = FORCE_FLAG.swap(false, Ordering::Relaxed);
            let paused = admin::ADMIN_SETTINGS
                .read()
                .await
                .static_paused
                .unwrap_or(false);

            if paused && !forced {
                trace!("Static schedule fetching paused, skipping");
                admin::metadata::write_metadata(
                    METADATA_NAME,
                    &admin::metadata::MetadataEntry::paused(),
                )
                .await;
            } else if let Err(e) = fetch_and_update_schedule(forced).await {
                warn!(error = %e, "Failed to fetch and update schedule");
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
    #[error("Failed to fetch metadata: {0:?}")]
    Fetch(reqwest::Error),
    #[error("Failed to open zip file: {0:?}")]
    Zip(#[from] zip::result::ZipError),
    #[error("Failed to spawn blocking task: {0:?}")]
    JoinBlocking(#[from] tokio::task::JoinError),
    #[error("Failed to parse file: {0:?}")]
    Parse(#[from] super::data::FileDataError),
    #[error("Got database error: {0:?}")]
    Database(#[from] sqlx::Error),
}

async fn fetch_and_update_schedule(forced: bool) -> Result<(), FetcherError> {
    admin::metadata::write_metadata(
        METADATA_NAME,
        &admin::metadata::MetadataEntry::in_progress(),
    )
    .await;

    let start = Instant::now();
    let has_updates = match fetch_newer_schedule(forced).await {
        Err(e) => {
            admin::metadata::write_metadata(
                METADATA_NAME,
                &admin::metadata::MetadataEntry::error()
                    .with_error_message(e.to_string())
                    .with_duration(start.elapsed()),
            )
            .await;

            return Err(e);
        }
        Ok(x) => x.is_some(),
    };

    if has_updates {
        admin::metadata::write_metadata(
            METADATA_NAME,
            &admin::metadata::MetadataEntry::success().with_duration(start.elapsed()),
        )
        .await;
    } else {
        admin::metadata::write_metadata(
            METADATA_NAME,
            &admin::metadata::MetadataEntry::skipped().with_duration(start.elapsed()),
        )
        .await;
    }

    trace!("Got newer schedule");
    DATA_NOTIFICATION.notify_waiters();
    Ok(())
}

/// Adds `If-None-Match` and `If-Modified-Since` from the last imported
/// schedule. The server then sends 304 without a body when the schedule did
/// not change.
async fn add_conditional_headers(
    mut request: reqwest::RequestBuilder,
) -> Result<reqwest::RequestBuilder, FetcherError> {
    let Some(last) = Database::logged(
        "schedule_meta_latest",
        sqlx::query!(
            "SELECT etag, last_modified FROM gtfs_schedule_meta ORDER BY fetched_at DESC LIMIT 1"
        )
        .fetch_optional(&Database::pool()),
    )
    .await
    .map_err(FetcherError::Database)?
    else {
        return Ok(request);
    };

    if let Some(etag) = last.etag.and_then(|x| HeaderValue::from_str(&x).ok()) {
        request = request.header(IF_NONE_MATCH, etag);
    }

    if let Some(last_modified) = last.last_modified {
        let date = jiff::fmt::rfc2822::DateTimePrinter::new()
            .timestamp_to_rfc9110_string(&crate::database::time::to_jiff(last_modified));
        if let Ok(date) = date {
            request = request.header(IF_MODIFIED_SINCE, date);
        }
    }

    Ok(request)
}

#[allow(clippy::too_many_lines)]
#[tracing::instrument(skip_all)]
async fn fetch_newer_schedule(forced: bool) -> Result<Option<()>, FetcherError> {
    let url = {
        let settings = admin::ADMIN_SETTINGS.read().await;
        settings.static_url.clone().unwrap_or_else(|| {
            Config::global()
                .global
                .data_fetcher
                .schedule_fetch_endpoint
                .clone()
        })
    };

    debug!(url = ?url.as_str(), "Fetching metadata");

    let response = crate::http_client::send_with_backoff({
        let mut request = crate::http_client::HTTP_CLIENT
            .get(url)
            .timeout(Duration::from_mins(1));

        if !forced {
            request = add_conditional_headers(request).await?;
        }

        request
    })
    .await
    .map_err(FetcherError::Fetch)?;

    if let Err(e) = response.error_for_status_ref() {
        return Err(FetcherError::Fetch(e));
    }

    trace!(status = ?response.status(), headers = ?response.headers(), "Got metadata response.");

    if response.status() == StatusCode::NOT_MODIFIED {
        trace!("Schedule is not modified, skipping download");
        crate::database::schedule_metadata::reload().await?;
        return Ok(None);
    }

    let modified = {
        let ts = response
            .headers()
            .get("last-modified")
            .and_then(|x| x.to_str().ok())
            .and_then(|x| jiff::fmt::rfc2822::parse(x).ok())
            .map_or_else(jiff::Timestamp::now, |zdt| zdt.timestamp());

        crate::database::time::from_jiff(ts)
    };

    let etag = response
        .headers()
        .get("etag")
        .and_then(|x| x.to_str().ok())
        .map(ToString::to_string);

    trace!(?modified, ?etag, "Got schedule metadata");

    let etag_param = etag.clone();
    let res = Database::logged(
        "schedule_meta_check",
        sqlx::query!(
            "SELECT * FROM gtfs_schedule_meta WHERE last_modified >= $1 OR etag = $2 LIMIT 1",
            modified,
            etag_param,
        )
        .fetch_optional(&Database::pool()),
    )
    .await
    .map_err(FetcherError::Database)?
    .is_some();

    trace!(forced, have_data = ?res, "Checking schedule metadata");

    if !forced && res {
        trace!("Schedule is up to date");
        // A previous forced import can have kept the same fetch metadata while
        // its enrichment refresh failed. Retrying here also covers that case.
        crate::database::schedule_metadata::reload().await?;
        return Ok(None);
    }

    trace!(forced, "Schedule is newer");

    let zip_body = response.bytes().await.map_err(FetcherError::Fetch)?;

    trace!(len = ?zip_body.len(), "Got zip body");

    let parse_result = GtfsSchedule::read_from_zip_bytes(zip_body).await;

    match parse_result {
        Ok(()) => {
            schedule_offsets::reload().await;
            crate::database::schedule_metadata::reload().await?;
            crate::database::service_days::reload().await;

            // Publish fetch metadata only after enrichment loads successfully,
            // so a failed cache refresh is retried on the next schedule fetch.
            debug!("Schedule read to database, committing metadata");

            Database::logged(
                "schedule_meta_insert",
                sqlx::query!(
                    "INSERT INTO gtfs_schedule_meta (last_modified, etag) VALUES ($1, $2)",
                    modified,
                    etag,
                )
                .execute(&Database::pool()),
            )
            .await
            .map_err(FetcherError::Database)?;

            debug!("Schedule updated");

            Ok(Some(()))
        }
        Err(e) => Err(FetcherError::Parse(e)),
    }
}
