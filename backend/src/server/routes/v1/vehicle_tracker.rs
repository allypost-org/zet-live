//! Matches live vehicle positions to the shapes of their trips.
//!
//! A shape can pass the same place more than once, for example in a terminus
//! loop or on a road that the trip uses in both directions. The tracker keeps
//! the last distance along the shape of each vehicle and selects the pass that
//! continues from it. The travel direction helps choose between nearby passes.

use std::collections::{HashMap, HashSet};

use crate::proto::gtfs_schedule::data::{
    geo::{Bearing, LatLng, Metres},
    trip_path::{ShapeProjection, TripPath},
    trip_paths::TripPaths,
};

/// A vehicle further than this from its trip shape is not on the shape.
const MAX_SNAP_METRES: f64 = 30.0;
/// Position noise can move a vehicle back along the shape by this much. A
/// smaller move back keeps the vehicle at its last distance along the shape.
const MAX_BACKTRACK_METRES: f64 = 50.0;
/// A vehicle that moved less than this has no usable travel direction.
const MIN_HEADING_METRES: f64 = 5.0;
/// Passes this much further away than the nearest pass are about as near.
const SIMILAR_DISTANCE_METRES: f64 = 15.0;
/// Extra distance for turns and position noise when comparing passes.
const CONTINUITY_MARGIN_METRES: f64 = 50.0;
/// A longer move between two positions is a jump in the data, so it has no path.
const MAX_PATH_METRES: f64 = 3_000.0;
/// Maximum number of path points in one vehicle update. The worker uses the same limit.
const MAX_PATH_POINTS: usize = 256;

#[derive(Debug)]
struct TripProgress {
    trip_id: String,
    along: f64,
}

/// A vehicle position on the shape of its trip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Located {
    /// Distance along the shape in metres.
    pub along: f64,
    pub position: LatLng,
    pub bearing: Bearing,
}

/// A reported vehicle position.
#[derive(Debug, Clone, Copy)]
pub struct Observation {
    pub position: LatLng,
    /// The last position that clients got for the vehicle.
    pub previous: Option<LatLng>,
    /// The vehicle is at least this far along the shape, from the feed.
    pub min_along: Option<f64>,
}

#[derive(Debug, Default)]
pub struct VehicleTracker {
    pub paths: TripPaths,
    pub progress: Progress,
    schedule_generation: u64,
}

impl VehicleTracker {
    pub async fn update_paths(
        &mut self,
        trip_keys: &HashSet<String>,
        schedule_generation: u64,
    ) -> Result<(), sqlx::Error> {
        if self.schedule_generation != schedule_generation {
            *self = Self {
                schedule_generation,
                ..Self::default()
            };
        }
        self.paths.update(trip_keys).await
    }
}

/// The distance along the trip shape for each vehicle in a snapshot.
#[derive(Debug, Default)]
pub struct Progress(HashMap<String, TripProgress>);

impl Progress {
    pub fn previous_along(&self, vehicle_id: &str, trip_id: &str) -> Option<f64> {
        self.0
            .get(vehicle_id)
            .filter(|progress| progress.trip_id == trip_id)
            .map(|progress| progress.along)
    }

    /// Returns the vehicle position on `path`, or `None` if the vehicle is not
    /// near the shape.
    pub fn locate(
        &self,
        vehicle_id: &str,
        trip_id: &str,
        path: &TripPath,
        observation: Observation,
    ) -> Option<Located> {
        let passes = path
            .shape()
            .passes_near(observation.position, MAX_SNAP_METRES);
        let moved = observation
            .previous
            .map(|previous| Metres::from_lat_lng(observation.position, previous));
        let displacement = moved.map_or(0.0, |moved| moved.distance(Metres::ORIGIN));
        let heading = moved
            .filter(|_| displacement >= MIN_HEADING_METRES)
            .and_then(|moved| Metres::ORIGIN.direction_to(moved));
        let previous_along = self.previous_along(vehicle_id, trip_id);
        let min_along = previous_along
            .or(observation.min_along)
            .map(|along| along - MAX_BACKTRACK_METRES);
        // Try the part of the shape the vehicle could have reached first.
        // If it left that part, locate it again on the full shape.
        let pass = previous_along
            .and_then(|previous| {
                let max = displacement.mul_add(2.0, previous + CONTINUITY_MARGIN_METRES);
                let range = (previous - MAX_BACKTRACK_METRES)..=max;
                let nearby = passes
                    .iter()
                    .copied()
                    .filter(|pass| range.contains(&pass.along));
                choose_pass(nearby, heading, None)
            })
            .or_else(|| choose_pass(passes.iter().copied(), heading, min_along))?;

        let along = match previous_along {
            Some(previous) if (previous - MAX_BACKTRACK_METRES..previous).contains(&pass.along) => {
                previous
            }
            _ => pass.along,
        };
        let (position, bearing) = path.shape().point_at(along);
        Some(Located {
            along,
            position,
            bearing,
        })
    }

    /// Shape points between the last committed position of the vehicle and
    /// `along`. Empty if the vehicle has no committed position on this trip,
    /// did not move forward, jumped, or passed too many points.
    pub fn path_since_previous(
        &self,
        vehicle_id: &str,
        trip_id: &str,
        path: &TripPath,
        along: f64,
    ) -> Vec<LatLng> {
        match self.previous_along(vehicle_id, trip_id) {
            Some(previous) if along > previous && along - previous <= MAX_PATH_METRES => {
                let points = path.shape().points_between(previous, along);
                if points.len() > MAX_PATH_POINTS {
                    return Vec::new();
                }
                points.collect()
            }
            _ => Vec::new(),
        }
    }

    pub fn record(&mut self, vehicle_id: &str, trip_id: &str, along: f64) {
        self.0.insert(
            vehicle_id.to_owned(),
            TripProgress {
                trip_id: trip_id.to_owned(),
                along,
            },
        );
    }
}

/// Selects the pass of the shape that the vehicle is on. `passes` are in order
/// along the shape. Passes against the `heading` are ignored, if any pass
/// agrees with it. Of the passes at or after `min_along`, the first one that is
/// almost as near as the nearest one is selected. If there is none, the
/// nearest pass is selected.
///
/// The first pass is preferred because a vehicle that waits at the start of a
/// circular trip is also at the end of the shape.
fn choose_pass(
    passes: impl Iterator<Item = ShapeProjection> + Clone,
    heading: Option<(f64, f64)>,
    min_along: Option<f64>,
) -> Option<ShapeProjection> {
    let agrees = |pass: &ShapeProjection| {
        heading.is_none_or(|(east, north)| {
            east.mul_add(pass.direction.0, north * pass.direction.1) >= 0.0
        })
    };
    let has_agreement = passes.clone().any(|pass| agrees(&pass));
    let candidates = passes.filter(|pass| !has_agreement || agrees(pass));
    let by_distance = |a: &ShapeProjection, b: &ShapeProjection| a.distance.total_cmp(&b.distance);
    let mut ahead = candidates
        .clone()
        .filter(|pass| min_along.is_none_or(|min| pass.along >= min));
    let Some(best) = ahead.clone().min_by(by_distance) else {
        return candidates.min_by(by_distance);
    };
    ahead.find(|pass| pass.distance <= best.distance + SIMILAR_DISTANCE_METRES)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::proto::gtfs_schedule::data::trip_path::{ScheduledStop, ShapeLine};

    const ORIGIN: LatLng = LatLng {
        lat: 45.8,
        lng: 16.0,
    };

    fn offset(east: f64, north: f64) -> LatLng {
        Metres { east, north }.to_lat_lng(ORIGIN)
    }

    /// 1 km east, then back west on a parallel line 4 m to the north.
    fn out_and_back() -> TripPath {
        let shape = ShapeLine::new(&[
            offset(0.0, 0.0),
            offset(1_000.0, 0.0),
            offset(1_000.0, 4.0),
            offset(0.0, 4.0),
        ])
        .map(Arc::new)
        .expect("valid shape");
        TripPath::new(
            shape,
            vec![ScheduledStop {
                stop_sequence: 1,
                stop_id: "s1".into(),
                location: offset(0.0, 0.0),
                arrival_offset: Some(0),
            }],
        )
    }

    fn observe(position: LatLng, previous: Option<LatLng>) -> Observation {
        Observation {
            position,
            previous,
            min_along: None,
        }
    }

    fn along(progress: &Progress, path: &TripPath, observation: Observation) -> Option<f64> {
        progress
            .locate("v", "t", path, observation)
            .map(|located| located.along.round())
    }

    #[test]
    fn travel_direction_selects_return_pass() {
        let path = out_and_back();
        let progress = Progress::default();

        let westbound = observe(offset(500.0, 2.0), Some(offset(600.0, 2.0)));
        let eastbound = observe(offset(500.0, 2.0), Some(offset(400.0, 2.0)));

        assert_eq!(along(&progress, &path, westbound), Some(1_504.0));
        assert_eq!(along(&progress, &path, eastbound), Some(500.0));
    }

    #[test]
    fn progress_selects_pass_without_travel_direction() {
        let path = out_and_back();
        let mut progress = Progress::default();
        let stopped = observe(offset(500.0, 2.0), None);

        progress.record("v", "t", 1_450.0);
        assert_eq!(along(&progress, &path, stopped), Some(1_504.0));

        progress.record("v", "t", 450.0);
        assert_eq!(along(&progress, &path, stopped), Some(500.0));
    }

    #[tokio::test]
    async fn new_schedule_discards_progress_from_the_old_shape() {
        let path = out_and_back();
        let mut tracker = VehicleTracker::default();
        tracker.progress.record("v", "t", 1_450.0);
        let stopped = observe(offset(500.0, 2.0), None);
        assert_eq!(along(&tracker.progress, &path, stopped), Some(1_504.0));

        tracker
            .update_paths(&HashSet::new(), 1)
            .await
            .expect("no paths to load");

        assert_eq!(along(&tracker.progress, &path, stopped), Some(500.0));
    }

    #[test]
    fn small_move_back_keeps_last_along() {
        let path = out_and_back();
        let mut progress = Progress::default();
        progress.record("v", "t", 520.0);

        assert_eq!(
            along(&progress, &path, observe(offset(500.0, 0.0), None)),
            Some(520.0)
        );
    }

    #[test]
    fn backward_gps_noise_does_not_switch_to_return_pass() {
        let path = out_and_back();
        let mut progress = Progress::default();
        progress.record("v", "t", 500.0);

        let observation = observe(offset(494.0, 0.0), Some(offset(500.0, 0.0)));
        assert_eq!(along(&progress, &path, observation), Some(500.0));
    }

    #[test]
    fn travel_around_turn_continues_on_return_pass() {
        let path = out_and_back();
        let mut progress = Progress::default();
        progress.record("v", "t", 990.0);

        let observation = observe(offset(980.0, 4.0), Some(offset(990.0, 0.0)));
        assert_eq!(along(&progress, &path, observation), Some(1_024.0));
    }

    #[test]
    fn vehicle_off_shape_is_not_located() {
        let path = out_and_back();
        let progress = Progress::default();

        assert_eq!(
            along(&progress, &path, observe(offset(500.0, 100.0), None)),
            None
        );
    }

    #[test]
    fn path_since_previous_follows_the_turn() {
        let path = out_and_back();
        let mut progress = Progress::default();
        progress.record("v", "t", 900.0);

        let turn = progress
            .path_since_previous("v", "t", &path, 1_100.0)
            .into_iter()
            .map(|point| {
                let m = Metres::from_lat_lng(point, ORIGIN);
                (m.east.round(), m.north.round())
            })
            .collect::<Vec<_>>();

        assert_eq!(turn, vec![(1_000.0, 0.0), (1_000.0, 4.0)]);
        assert!(
            progress
                .path_since_previous("v", "other trip", &path, 1_100.0)
                .is_empty()
        );
    }

    #[test]
    fn dense_shape_does_not_produce_an_oversized_path() {
        let points = (0..400)
            .map(|east| offset(f64::from(east), 0.0))
            .collect::<Vec<_>>();
        let shape = Arc::new(ShapeLine::new(&points).expect("valid shape"));
        let path = TripPath::new(shape, vec![]);
        let mut progress = Progress::default();
        progress.record("v", "t", 0.0);

        assert_eq!(
            progress.path_since_previous("v", "t", &path, 256.5).len(),
            256
        );
        assert!(
            progress
                .path_since_previous("v", "t", &path, 350.0)
                .is_empty()
        );
    }
}
