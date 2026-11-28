//! Travel direction at each stop, from the GTFS shapes of the trips that serve it.
//!
//! The feed has no `shape_dist_traveled`, so each stop is projected onto the nearest
//! segment of each shape. The direction is measured over the last [`APPROACH_METRES`]
//! of the shape before the stop: routes often turn at an intersection just after a
//! stop, and the approach direction is the same for all of them.

use std::collections::HashMap;

use tokio::task::spawn_blocking;
use tracing::{debug, warn};

use super::geo::Metres;
pub use super::geo::{Bearing, LatLng};
use crate::database::Database;

/// Length of shape before the stop that sets its direction.
const APPROACH_METRES: f64 = 25.0;
/// A shape further than this from the stop does not pass through it.
const MAX_SHAPE_DISTANCE_METRES: f64 = 50.0;
/// Minimum length of the weighted mean direction vector (0 to 1). A lower value
/// means that the shapes through the stop go in different directions.
const MIN_AGREEMENT: f64 = 0.8;

/// A stop and the number of scheduled trips on one shape that serve it.
#[derive(Debug, Clone, PartialEq)]
pub struct StopShapeUse {
    pub stop_id: String,
    pub location: LatLng,
    pub shape_id: String,
    pub trips: i64,
}

/// Direction of `shape` where it passes `stop`, as a unit vector `(east, north)`.
/// Returns `None` if the shape does not pass within [`MAX_SHAPE_DISTANCE_METRES`].
fn shape_direction_at(stop: LatLng, shape: &[LatLng]) -> Option<(f64, f64)> {
    let points = shape
        .iter()
        .map(|&p| Metres::from_lat_lng(p, stop))
        .collect::<Vec<_>>();
    let origin = Metres::ORIGIN;

    let mut nearest: Option<(f64, usize, f64)> = None;
    for (i, pair) in points.windows(2).enumerate() {
        let (a, b) = (pair[0], pair[1]);
        let (de, dn) = (b.east - a.east, b.north - a.north);
        let length_sq = de.mul_add(de, dn * dn);
        if length_sq == 0.0 {
            continue;
        }
        let t = ((-a.east).mul_add(de, -a.north * dn) / length_sq).clamp(0.0, 1.0);
        let distance = a.lerp(b, t).distance(origin);
        if nearest.is_none_or(|(best, ..)| distance < best) {
            nearest = Some((distance, i, t));
        }
    }

    let (distance, segment, t) = nearest?;
    if distance > MAX_SHAPE_DISTANCE_METRES {
        return None;
    }

    let at_stop = points[segment].lerp(points[segment + 1], t);
    let mut behind = at_stop;
    let mut remaining = APPROACH_METRES;
    for &point in points[..=segment].iter().rev() {
        let back_length = behind.distance(point);
        if back_length >= remaining {
            behind = behind.lerp(point, remaining / back_length);
            remaining = 0.0;
            break;
        }
        remaining -= back_length;
        behind = point;
    }

    // The stop is at the start of the shape, so no shape comes before it.
    // Use the direction of the segment that leaves the stop instead.
    let (from, to) = if remaining >= APPROACH_METRES {
        (at_stop, points[segment + 1])
    } else {
        (behind, at_stop)
    };
    from.direction_to(to)
}

/// Computes the bearing for each stop. A stop has no bearing when no shape passes
/// near it, or when the shapes that pass it do not agree on a direction.
pub fn compute_stop_bearings(
    uses: &[StopShapeUse],
    shapes: &HashMap<String, Vec<LatLng>>,
) -> HashMap<String, Bearing> {
    // Weighted sum of unit vectors and total weight for each stop.
    let mut sums = HashMap::<&str, (f64, f64, f64)>::new();
    for stop_use in uses {
        let Some(shape) = shapes.get(&stop_use.shape_id) else {
            continue;
        };
        let Some((east, north)) = shape_direction_at(stop_use.location, shape) else {
            continue;
        };
        let weight = f64::from(i32::try_from(stop_use.trips).unwrap_or(i32::MAX));
        let sum = sums.entry(stop_use.stop_id.as_str()).or_default();
        sum.0 = east.mul_add(weight, sum.0);
        sum.1 = north.mul_add(weight, sum.1);
        sum.2 += weight;
    }

    sums.into_iter()
        .filter(|(_, (east, north, weight))| {
            *weight > 0.0 && east.hypot(*north) / weight >= MIN_AGREEMENT
        })
        .map(|(stop_id, (east, north, _))| (stop_id.to_string(), Bearing::from_vector(east, north)))
        .collect()
}

/// Loads the schedule and computes the bearing of each stop.
pub async fn load_stop_bearings() -> Result<HashMap<String, Bearing>, sqlx::Error> {
    let uses = Database::logged(
        "stop_bearing_uses",
        sqlx::query!(
            r#"SELECT u.stop_id AS "stop_id!", u.shape_id AS "shape_id!", u.trips AS "trips!",
                      s.latitude AS "latitude!", s.longitude AS "longitude!"
               FROM (
                   SELECT st.stop_id, t.shape_id, COUNT(*) AS trips
                   FROM gtfs_stop_times st
                   JOIN gtfs_trips t ON t.trip_id = st.trip_id
                   WHERE t.shape_id IS NOT NULL
                   GROUP BY st.stop_id, t.shape_id
               ) u
               JOIN gtfs_stops s ON s.stop_id = u.stop_id
               WHERE s.latitude IS NOT NULL AND s.longitude IS NOT NULL"#
        )
        .fetch_all(&Database::pool()),
    )
    .await?
    .into_iter()
    .map(|row| StopShapeUse {
        stop_id: row.stop_id,
        location: LatLng {
            lat: row.latitude,
            lng: row.longitude,
        },
        shape_id: row.shape_id,
        trips: row.trips,
    })
    .collect::<Vec<_>>();

    let shape_rows = Database::logged(
        "stop_bearing_shapes",
        sqlx::query!(
            "SELECT shape_id, shape_pt_lat, shape_pt_lon
             FROM gtfs_shapes
             ORDER BY shape_id, shape_pt_sequence"
        )
        .fetch_all(&Database::pool()),
    )
    .await?;
    let mut shapes = HashMap::<String, Vec<LatLng>>::new();
    for row in shape_rows {
        shapes.entry(row.shape_id).or_default().push(LatLng {
            lat: row.shape_pt_lat,
            lng: row.shape_pt_lon,
        });
    }

    let bearings = spawn_blocking(move || compute_stop_bearings(&uses, &shapes))
        .await
        .unwrap_or_else(|e| {
            warn!(?e, "Stop bearing computation failed");
            HashMap::new()
        });
    debug!(stops = bearings.len(), "Computed stop bearings");
    Ok(bearings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::gtfs_schedule::data::geo::METRES_PER_DEGREE;

    /// Returns a point `east` and `north` metres from the origin stop.
    fn offset(east: f64, north: f64) -> LatLng {
        let origin = stop();
        LatLng {
            lat: origin.lat + north / METRES_PER_DEGREE,
            lng: origin.lng + east / (METRES_PER_DEGREE * origin.lat.to_radians().cos()),
        }
    }

    const fn stop() -> LatLng {
        LatLng {
            lat: 45.8,
            lng: 16.0,
        }
    }

    fn bearings(shapes: &[(&str, Vec<LatLng>, i64)]) -> Option<Bearing> {
        let uses = shapes
            .iter()
            .map(|(id, _, trips)| StopShapeUse {
                stop_id: "1_1".into(),
                location: stop(),
                shape_id: (*id).into(),
                trips: *trips,
            })
            .collect::<Vec<_>>();
        let shapes = shapes
            .iter()
            .map(|(id, points, _)| ((*id).to_string(), points.clone()))
            .collect();
        compute_stop_bearings(&uses, &shapes).remove("1_1")
    }

    #[test]
    fn stop_beside_straight_road() {
        let east = vec![offset(-200.0, 5.0), offset(200.0, 5.0)];
        assert_eq!(bearings(&[("a", east, 1)]), Some(Bearing(90.0)));
    }

    #[test]
    fn turn_after_stop_uses_approach_direction() {
        // The road goes north 4 m east of the stop. Just after the stop, the route
        // turns west across the front of it, so the west segment is the nearest one.
        let turn = vec![offset(4.0, -200.0), offset(4.0, 1.0), offset(-200.0, 1.0)];
        let degrees = bearings(&[("turn", turn, 1)]).map(Bearing::degrees);
        // The window includes the start of the turn, so the result is near north.
        assert!(
            degrees.is_some_and(|d| !(15.0..=345.0).contains(&d)),
            "{degrees:?}"
        );
    }

    #[test]
    fn stop_at_shape_start_uses_departure_direction() {
        let west = vec![offset(0.0, 0.0), offset(-200.0, 0.0)];
        assert_eq!(bearings(&[("a", west, 1)]), Some(Bearing(270.0)));
    }

    #[test]
    fn opposite_directions_give_no_bearing() {
        let north = vec![offset(0.0, -200.0), offset(0.0, 200.0)];
        let south = vec![offset(0.0, 200.0), offset(0.0, -200.0)];
        assert_eq!(bearings(&[("n", north, 3), ("s", south, 3)]), None);
    }

    #[test]
    fn distant_shape_is_ignored() {
        let far = vec![offset(-200.0, 100.0), offset(200.0, 100.0)];
        assert_eq!(bearings(&[("a", far, 1)]), None);
    }
}
