use std::{
    collections::HashMap,
    sync::{Arc, OnceLock, RwLock},
};

use tracing::{debug, warn};

use crate::database::Database;

static CACHE: OnceLock<RwLock<Arc<ScheduleMetadata>>> = OnceLock::new();

#[derive(Debug, Default)]
pub struct ScheduleMetadata {
    route_names: HashMap<String, String>,
    headsigns_by_trip: HashMap<String, String>,
    headsigns_by_key: HashMap<String, String>,
}

impl ScheduleMetadata {
    pub fn route_name(&self, route_id: &str) -> Option<&str> {
        self.route_names.get(route_id).map(String::as_str)
    }

    pub fn headsign(&self, trip_id: &str, trip_key: &str) -> Option<&str> {
        self.headsigns_by_trip
            .get(trip_id)
            .or_else(|| self.headsigns_by_key.get(trip_key))
            .map(String::as_str)
    }

    fn from_rows(routes: Vec<RouteRow>, mut trips: Vec<TripRow>) -> Self {
        let route_names = routes
            .into_iter()
            .filter_map(|row| {
                Some((
                    row.route_id,
                    row.route_name.filter(|name| !name.is_empty())?,
                ))
            })
            .collect();

        // A normalized key can span service variants. Use the first non-empty
        // headsign by exact trip id so database row order cannot pick a variant.
        trips.sort_unstable_by(|left, right| left.trip_id.cmp(&right.trip_id));
        let mut headsigns_by_trip = HashMap::new();
        let mut headsigns_by_key = HashMap::new();
        for trip in trips {
            let Some(headsign) = trip.headsign.filter(|headsign| !headsign.is_empty()) else {
                continue;
            };
            if let Some(key) = trip.trip_key {
                headsigns_by_key
                    .entry(key)
                    .or_insert_with(|| headsign.clone());
            }
            headsigns_by_trip.insert(trip.trip_id, headsign);
        }

        Self {
            route_names,
            headsigns_by_trip,
            headsigns_by_key,
        }
    }
}

#[derive(Debug)]
struct RouteRow {
    route_id: String,
    route_name: Option<String>,
}

#[derive(Debug, Clone)]
struct TripRow {
    trip_id: String,
    trip_key: Option<String>,
    headsign: Option<String>,
}

pub async fn init() -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let metadata = load().await?;
    let routes = metadata.route_names.len();
    let trips = metadata.headsigns_by_trip.len();
    let _ = CACHE.set(RwLock::new(Arc::new(metadata)));
    debug!(routes, trips, elapsed = ?started.elapsed(), "schedule_metadata cache populated");
    Ok(())
}

pub async fn reload() -> Result<(), sqlx::Error> {
    let metadata = load().await?;
    let routes = metadata.route_names.len();
    let trips = metadata.headsigns_by_trip.len();
    if let Some(rw) = CACHE.get() {
        *rw.write().expect("schedule_metadata lock poisoned") = Arc::new(metadata);
        debug!(routes, trips, "schedule_metadata cache reloaded");
    } else {
        warn!("schedule_metadata reload before init; populating now");
        let _ = CACHE.set(RwLock::new(Arc::new(metadata)));
    }
    Ok(())
}

pub fn snapshot() -> Arc<ScheduleMetadata> {
    CACHE.get().and_then(|rw| rw.read().ok()).map_or_else(
        || Arc::new(ScheduleMetadata::default()),
        |guard| guard.clone(),
    )
}

async fn load() -> Result<ScheduleMetadata, sqlx::Error> {
    let routes = sqlx::query_as!(
        RouteRow,
        "SELECT route_id, NULLIF(route_long_name, '') AS route_name FROM gtfs_routes"
    )
    .fetch_all(&Database::pool())
    .await?;

    let trips = sqlx::query_as!(
        TripRow,
        "SELECT trip_id, trip_key, NULLIF(trip_headsign, '') AS headsign FROM gtfs_trips"
    )
    .fetch_all(&Database::pool())
    .await?;

    Ok(ScheduleMetadata::from_rows(routes, trips))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trip(id: &str, key: Option<&str>, headsign: Option<&str>) -> TripRow {
        TripRow {
            trip_id: id.to_owned(),
            trip_key: key.map(str::to_owned),
            headsign: headsign.map(str::to_owned),
        }
    }

    #[test]
    fn exact_service_variant_takes_precedence() {
        let metadata = ScheduleMetadata::from_rows(
            Vec::new(),
            vec![
                trip("0_40_123", Some("0_123"), Some("First variant")),
                trip("0_50_123", Some("0_123"), Some("Selected variant")),
            ],
        );

        assert_eq!(
            metadata.headsign("0_50_123", "0_123"),
            Some("Selected variant")
        );
    }

    #[test]
    fn fallback_is_independent_of_database_row_order() {
        let trips = vec![
            trip("0_50_123", Some("0_123"), Some("Later variant")),
            trip("0_40_123", Some("0_123"), Some("Earlier variant")),
        ];
        let first = ScheduleMetadata::from_rows(Vec::new(), trips.clone());
        let reversed = ScheduleMetadata::from_rows(Vec::new(), trips.into_iter().rev().collect());

        assert_eq!(first.headsign("unknown", "0_123"), Some("Earlier variant"));
        assert_eq!(
            reversed.headsign("unknown", "0_123"),
            Some("Earlier variant")
        );
    }

    #[test]
    fn missing_metadata_does_not_mask_fallback() {
        let metadata = ScheduleMetadata::from_rows(
            vec![
                RouteRow {
                    route_id: "1".to_owned(),
                    route_name: None,
                },
                RouteRow {
                    route_id: "2".to_owned(),
                    route_name: Some(String::new()),
                },
            ],
            vec![
                trip("0_30_123", Some("0_123"), None),
                trip("0_40_123", Some("0_123"), Some("")),
                trip("0_50_123", Some("0_123"), Some("Valid fallback")),
                trip("exact_only", None, Some("Exact only")),
            ],
        );

        assert_eq!(
            metadata.headsign("0_30_123", "0_123"),
            Some("Valid fallback")
        );
        assert_eq!(
            metadata.headsign("0_40_123", "0_123"),
            Some("Valid fallback")
        );
        assert_eq!(metadata.headsign("unknown", "missing"), None);
        assert_eq!(
            metadata.headsign("exact_only", "missing"),
            Some("Exact only")
        );
        assert_eq!(metadata.route_name("1"), None);
        assert_eq!(metadata.route_name("2"), None);
    }
}
