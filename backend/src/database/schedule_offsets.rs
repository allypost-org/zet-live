//! In-memory cache of `gtfs_stop_times.arrival_time_seconds` keyed by
//! `trip_key`, with each trip's `(stop_sequence, arrival_offset)` pairs packed
//! in a sorted `Box<[(i64, i64)]>`. The table is static between schedule
//! reloads (rewritten only by the schedule fetcher), but the predictions handler
//! hit it per request — decoding ~13K rows each call (~50 ms). This cache loads
//! the full table once at startup and swaps in a new snapshot whenever the
//! schedule fetcher commits, so request handlers pay only an `Arc` clone.
//!
//! Storage shape (`HashMap<String, Box<[…]>>` ≈ ~30 MB) is ~4.5× smaller than
//! the flat `HashMap<(String, i64), i64>` (~136 MB), because only ~32K distinct
//! `trip_keys` exist — each referenced ~54× by its stops.

use std::{
    collections::HashMap,
    sync::{Arc, OnceLock, RwLock},
};

use tracing::{debug, warn};

use crate::database::Database;

static CACHE: OnceLock<RwLock<Arc<ScheduleOffsets>>> = OnceLock::new();

#[derive(Debug, Default)]
pub struct ScheduleOffsets(HashMap<String, Box<[(i64, i64)]>>);

impl ScheduleOffsets {
    /// Look up `arrival_time_seconds` for a `(trip_key, stop_sequence)`.
    pub fn get(&self, trip_key: &str, stop_sequence: i64) -> Option<i64> {
        self.0.get(trip_key).and_then(|stops| {
            stops
                .binary_search_by_key(&stop_sequence, |&(seq, _)| seq)
                .ok()
                .map(|i| stops[i].1)
        })
    }
}

#[derive(Debug)]
struct OffsetRow {
    trip_key: Option<String>,
    stop_sequence: i32,
    arrival_time_seconds: Option<i32>,
}

/// Populate the cache from the DB. Called once after migrations at startup.
pub async fn init() -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let map = load().await?;
    let trips = map.0.len();
    let _ = CACHE.set(RwLock::new(Arc::new(map)));
    debug!(trips, elapsed = ?started.elapsed(), "schedule_offsets cache populated");
    Ok(())
}

/// Re-populate the cache after the schedule fetcher commits new
/// `gtfs_stop_times`. Readers keep using the old snapshot until the swap.
pub async fn reload() {
    match load().await {
        Ok(map) => {
            let trips = map.0.len();
            if let Some(rw) = CACHE.get() {
                *rw.write().expect("schedule_offsets lock poisoned") = Arc::new(map);
                debug!(trips, "schedule_offsets cache reloaded");
            } else {
                warn!("schedule_offsets reload before init; populating now");
                let _ = CACHE.set(RwLock::new(Arc::new(map)));
            }
        }
        Err(e) => warn!(error = ?e, "failed to reload schedule_offsets cache"),
    }
}

/// Cheap snapshot — clones only the `Arc`, not the map.
pub fn snapshot() -> Arc<ScheduleOffsets> {
    CACHE.get().and_then(|rw| rw.read().ok()).map_or_else(
        || Arc::new(ScheduleOffsets::default()),
        |guard| guard.clone(),
    )
}

async fn load() -> anyhow::Result<ScheduleOffsets> {
    let rows = sqlx::query_as!(
        OffsetRow,
        "
        SELECT trip_key, stop_sequence, arrival_time_seconds
        FROM gtfs_stop_times
        WHERE arrival_time_seconds IS NOT NULL
        ",
    )
    .fetch_all(&Database::pool())
    .await?;

    let mut by_trip: HashMap<String, Vec<(i64, i64)>> = HashMap::new();
    for row in rows {
        if let (Some(trip_key), Some(offset)) = (row.trip_key, row.arrival_time_seconds) {
            by_trip
                .entry(trip_key)
                .or_default()
                .push((i64::from(row.stop_sequence), i64::from(offset)));
        }
    }

    let mut map = HashMap::with_capacity(by_trip.len());
    for (trip_key, mut stops) in by_trip {
        stops.sort_by_key(|&(seq, _)| seq);
        map.insert(trip_key, stops.into_boxed_slice());
    }

    Ok(ScheduleOffsets(map))
}
