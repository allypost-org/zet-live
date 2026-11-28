//! Trip paths of the trips in the live feed, loaded from the schedule.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use super::{
    geo::LatLng,
    trip_path::{ScheduledStop, ShapeLine, TripPath},
};
use crate::database::Database;

/// Trip paths by `trip_key`, for the trips in the live feed.
#[derive(Debug, Default)]
pub struct TripPaths {
    shapes: HashMap<String, Option<Arc<ShapeLine>>>,
    /// `None` when the trip has no usable shape, so that it is not loaded again.
    trips: HashMap<String, Option<Arc<TripPath>>>,
}

impl TripPaths {
    pub fn get(&self, trip_key: &str) -> Option<&TripPath> {
        self.trips.get(trip_key)?.as_deref()
    }

    #[cfg(test)]
    pub fn insert(&mut self, trip_key: String, path: TripPath) {
        self.trips.insert(trip_key, Some(Arc::new(path)));
    }

    /// Loads the paths of `trip_keys` that are not loaded yet and forgets the
    /// paths of other trips.
    pub async fn update(&mut self, trip_keys: &HashSet<String>) -> Result<(), sqlx::Error> {
        self.trips.retain(|key, _| trip_keys.contains(key));
        let missing = trip_keys
            .iter()
            .filter(|key| !self.trips.contains_key(*key))
            .cloned()
            .collect::<Vec<_>>();
        if missing.is_empty() {
            return Ok(());
        }

        let trip_shapes = Database::logged(
            "trip_path_shapes",
            sqlx::query!(
                r#"SELECT DISTINCT ON (trip_key) trip_key AS "trip_key!", shape_id AS "shape_id!"
                   FROM gtfs_trips
                   WHERE trip_key = ANY($1) AND shape_id IS NOT NULL
                   ORDER BY trip_key, trip_id"#,
                &missing,
            )
            .fetch_all(&Database::pool()),
        )
        .await?
        .into_iter()
        .map(|row| (row.trip_key, row.shape_id))
        .collect::<HashMap<_, _>>();

        self.load_shapes(trip_shapes.values()).await?;

        let stop_rows = Database::logged(
            "trip_path_stops",
            sqlx::query!(
                r#"SELECT DISTINCT ON (st.trip_key, st.stop_sequence)
                          st.trip_key AS "trip_key!", st.stop_sequence, st.stop_id,
                          st.arrival_time_seconds,
                          s.latitude AS "latitude!", s.longitude AS "longitude!"
                   FROM gtfs_stop_times st
                   JOIN gtfs_stops s ON s.stop_id = st.stop_id
                   WHERE st.trip_key = ANY($1)
                     AND s.latitude IS NOT NULL AND s.longitude IS NOT NULL
                   ORDER BY st.trip_key, st.stop_sequence, st.trip_id"#,
                &missing,
            )
            .fetch_all(&Database::pool()),
        )
        .await?;
        let mut stops = HashMap::<String, Vec<ScheduledStop>>::new();
        for row in stop_rows {
            let Ok(stop_sequence) = u32::try_from(row.stop_sequence) else {
                continue;
            };
            stops.entry(row.trip_key).or_default().push(ScheduledStop {
                stop_sequence,
                stop_id: row.stop_id,
                location: LatLng {
                    lat: row.latitude,
                    lng: row.longitude,
                },
                arrival_offset: row.arrival_time_seconds,
            });
        }

        for key in missing {
            let shape = trip_shapes
                .get(&key)
                .and_then(|shape_id| self.shapes.get(shape_id)?.clone());
            let path = shape
                .zip(stops.remove(&key))
                .map(|(shape, stops)| Arc::new(TripPath::new(shape, stops)));
            self.trips.insert(key, path);
        }
        Ok(())
    }

    async fn load_shapes(
        &mut self,
        shape_ids: impl Iterator<Item = &String>,
    ) -> Result<(), sqlx::Error> {
        let missing = shape_ids
            .filter(|id| !self.shapes.contains_key(*id))
            .cloned()
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        if missing.is_empty() {
            return Ok(());
        }
        let rows = Database::logged(
            "trip_path_shape_points",
            sqlx::query!(
                "SELECT shape_id, shape_pt_lat, shape_pt_lon
                 FROM gtfs_shapes
                 WHERE shape_id = ANY($1)
                 ORDER BY shape_id, shape_pt_sequence",
                &missing,
            )
            .fetch_all(&Database::pool()),
        )
        .await?;
        let mut points = HashMap::<String, Vec<LatLng>>::new();
        for row in rows {
            points.entry(row.shape_id).or_default().push(LatLng {
                lat: row.shape_pt_lat,
                lng: row.shape_pt_lon,
            });
        }
        for id in missing {
            let shape = points
                .get(&id)
                .and_then(|points| ShapeLine::new(points))
                .map(Arc::new);
            self.shapes.insert(id, shape);
        }
        Ok(())
    }
}
