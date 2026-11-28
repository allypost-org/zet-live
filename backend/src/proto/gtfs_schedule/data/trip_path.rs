//! Trip geometry: the shape of a trip and the distance along it of each stop.
//!
//! The feed has no `shape_dist_traveled`, so stops are projected onto the shape.
//! A shape can pass the same place more than once, for example in a terminus
//! loop. Each stop takes the first pass after the previous stop.

use std::sync::Arc;

use super::geo::{Bearing, LatLng, Metres};

/// A stop further than this from the shape is not on the shape.
const MAX_STOP_DISTANCE_METRES: f64 = 50.0;
/// Two stops close together can project in the wrong order by this much.
const STOP_ORDER_TOLERANCE_METRES: f64 = 5.0;
/// Local minima closer than this along the shape are one pass, for example
/// both segments at a vertex.
const SAME_PASS_METRES: f64 = 1.0;
/// A vehicle keeps a stop as its next stop until it is this far past the stop.
const LEAVE_STOP_METRES: f64 = 20.0;
/// [`LEAVE_STOP_METRES`] for the first stop of a trip.
const LEAVE_FIRST_STOP_METRES: f64 = 100.0;
/// Consecutive shape points closer than this are one point.
const MIN_SEGMENT_METRES: f64 = 0.01;

/// A shape as a polyline in local metres, with the distance along it at each point.
#[derive(Debug)]
pub struct ShapeLine {
    origin: LatLng,
    points: Box<[Metres]>,
    along: Box<[f64]>,
}

/// The nearest point of one pass of a shape near a location.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapeProjection {
    /// Distance along the shape in metres.
    pub along: f64,
    /// Distance from the location to the shape in metres.
    pub distance: f64,
    /// Travel direction of the shape at the projection, as a unit vector `(east, north)`.
    pub direction: (f64, f64),
}

impl ShapeLine {
    /// Returns `None` if the shape has fewer than two distinct points.
    pub fn new(points: &[LatLng]) -> Option<Self> {
        let origin = *points.first()?;
        let mut metres = Vec::<Metres>::with_capacity(points.len());
        for point in points {
            let point = Metres::from_lat_lng(*point, origin);
            if metres
                .last()
                .is_none_or(|last| last.distance(point) >= MIN_SEGMENT_METRES)
            {
                metres.push(point);
            }
        }
        if metres.len() < 2 {
            return None;
        }
        let mut along = Vec::with_capacity(metres.len());
        let mut total = 0.0;
        along.push(total);
        for pair in metres.windows(2) {
            total += pair[0].distance(pair[1]);
            along.push(total);
        }
        Some(Self {
            origin,
            points: metres.into_boxed_slice(),
            along: along.into_boxed_slice(),
        })
    }

    /// Returns the nearest point of each pass of the shape within
    /// `max_distance` metres of `location`, in order along the shape.
    ///
    /// A pass is a local minimum of the distance from `location` to the shape,
    /// as a function of the distance along the shape. A trip that returns on
    /// the same road thus has two passes, also near the turn.
    pub fn passes_near(&self, location: LatLng, max_distance: f64) -> Vec<ShapeProjection> {
        let location = Metres::from_lat_lng(location, self.origin);
        let nearest = self
            .points
            .windows(2)
            .enumerate()
            .map(|(i, pair)| {
                let (a, b) = (pair[0], pair[1]);
                let length = self.along[i + 1] - self.along[i];
                let t = (location.east - a.east).mul_add(
                    b.east - a.east,
                    (location.north - a.north) * (b.north - a.north),
                ) / (length * length);
                let t = t.clamp(0.0, 1.0);
                let projection = ShapeProjection {
                    along: length.mul_add(t, self.along[i]),
                    distance: a.lerp(b, t).distance(location),
                    direction: a.direction_to(b).unwrap_or((0.0, 1.0)),
                };
                (t, projection)
            })
            .collect::<Vec<_>>();

        let mut passes = Vec::<ShapeProjection>::new();
        for (i, &(t, projection)) in nearest.iter().enumerate() {
            // The nearest point of a segment at its end is a local minimum
            // only if the next segment is nearest at the same point.
            let minimum = if t <= 0.0 {
                i == 0 || nearest[i - 1].0 >= 1.0
            } else if t >= 1.0 {
                nearest.get(i + 1).is_none_or(|&(next, _)| next <= 0.0)
            } else {
                true
            };
            if !minimum || projection.distance > max_distance {
                continue;
            }
            match passes.last_mut() {
                Some(last) if projection.along - last.along < SAME_PASS_METRES => {
                    if projection.distance < last.distance {
                        *last = projection;
                    }
                }
                _ => passes.push(projection),
            }
        }
        passes
    }

    /// Index of the segment that contains `along`.
    fn segment_at(&self, along: f64) -> usize {
        self.along
            .partition_point(|&d| d <= along)
            .saturating_sub(1)
            .min(self.points.len() - 2)
    }

    /// Location and travel bearing at `along` metres from the start.
    pub fn point_at(&self, along: f64) -> (LatLng, Bearing) {
        let i = self.segment_at(along);
        let (a, b) = (self.points[i], self.points[i + 1]);
        let t = ((along - self.along[i]) / (self.along[i + 1] - self.along[i])).clamp(0.0, 1.0);
        let (east, north) = a.direction_to(b).unwrap_or((0.0, 1.0));
        (
            a.lerp(b, t).to_lat_lng(self.origin),
            Bearing::from_vector(east, north),
        )
    }

    /// Shape points strictly between `from` and `to` metres along the shape.
    pub fn points_between(&self, from: f64, to: f64) -> impl ExactSizeIterator<Item = LatLng> + '_ {
        let start = self.along.partition_point(|&d| d <= from);
        let end = self.along.partition_point(|&d| d < to);
        self.points[start..end.max(start)]
            .iter()
            .map(|point| point.to_lat_lng(self.origin))
    }
}

/// A scheduled stop of a trip, as loaded from the schedule.
#[derive(Debug, Clone, PartialEq)]
pub struct ScheduledStop {
    pub stop_sequence: u32,
    pub stop_id: String,
    pub location: LatLng,
    /// Scheduled arrival in seconds after the service-day midnight.
    pub arrival_offset: Option<i32>,
}

/// A stop of a trip and its distance along the trip shape.
#[derive(Debug, Clone, PartialEq)]
pub struct PathStop {
    pub stop_sequence: u32,
    pub stop_id: String,
    pub along: f64,
    pub arrival_offset: Option<i32>,
}

#[derive(Debug)]
pub struct TripPath {
    shape: Arc<ShapeLine>,
    /// Stops on the shape, ascending by `stop_sequence` and by `along`.
    stops: Box<[PathStop]>,
}

impl TripPath {
    /// Projects `stops` onto `shape`. A stop that is not near the shape after
    /// the previous stop is left out.
    pub fn new(shape: Arc<ShapeLine>, mut stops: Vec<ScheduledStop>) -> Self {
        stops.sort_by_key(|stop| stop.stop_sequence);
        let mut path_stops = Vec::<PathStop>::with_capacity(stops.len());
        for stop in stops {
            let min_along = path_stops
                .last()
                .map_or(0.0, |last| last.along - STOP_ORDER_TOLERANCE_METRES);
            let Some(pass) = shape
                .passes_near(stop.location, MAX_STOP_DISTANCE_METRES)
                .into_iter()
                .find(|pass| pass.along >= min_along)
            else {
                continue;
            };
            let along = path_stops
                .last()
                .map_or(pass.along, |last| pass.along.max(last.along));
            path_stops.push(PathStop {
                stop_sequence: stop.stop_sequence,
                stop_id: stop.stop_id,
                along,
                arrival_offset: stop.arrival_offset,
            });
        }
        Self {
            shape,
            stops: path_stops.into_boxed_slice(),
        }
    }

    pub fn shape(&self) -> &ShapeLine {
        &self.shape
    }

    pub fn stops(&self) -> &[PathStop] {
        &self.stops
    }

    /// The stop that a vehicle at `along` goes to next. A vehicle at a stop
    /// keeps the stop as its next stop until it is [`LEAVE_STOP_METRES`] past
    /// it. Terminus stands are often after the first stop, so the first stop
    /// uses [`LEAVE_FIRST_STOP_METRES`]. After the last stop, this is the last
    /// stop.
    pub fn next_stop(&self, along: f64) -> Option<&PathStop> {
        self.stops
            .iter()
            .enumerate()
            .find(|(i, stop)| {
                let leave = if *i == 0 {
                    LEAVE_FIRST_STOP_METRES
                } else {
                    LEAVE_STOP_METRES
                };
                stop.along >= along - leave
            })
            .map(|(_, stop)| stop)
            .or_else(|| self.stops.last())
    }

    /// The scheduled time, in seconds after midnight, at which the vehicle is
    /// at `along`. Interpolates between the stops before and after `along`.
    /// Returns `None` before the first stop and after the last stop.
    pub fn scheduled_offset_at(&self, along: f64) -> Option<f64> {
        let after = self.stops.iter().position(|stop| stop.along > along)?;
        let before = self.stops[..after]
            .iter()
            .rev()
            .find(|stop| stop.arrival_offset.is_some())?;
        let after = self.stops[after..]
            .iter()
            .find(|stop| stop.arrival_offset.is_some())?;
        let from = f64::from(before.arrival_offset?);
        let to = f64::from(after.arrival_offset?);
        let span = after.along - before.along;
        if span <= 0.0 {
            return Some(to);
        }
        Some((to - from).mul_add((along - before.along) / span, from))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::gtfs_schedule::data::geo::METRES_PER_DEGREE;

    const ORIGIN: LatLng = LatLng {
        lat: 45.8,
        lng: 16.0,
    };

    fn offset(east: f64, north: f64) -> LatLng {
        Metres { east, north }.to_lat_lng(ORIGIN)
    }

    fn stop(stop_sequence: u32, location: LatLng, arrival_offset: i32) -> ScheduledStop {
        ScheduledStop {
            stop_sequence,
            stop_id: format!("s{stop_sequence}"),
            location,
            arrival_offset: Some(arrival_offset),
        }
    }

    fn alongs(path: &TripPath) -> Vec<(u32, f64)> {
        path.stops()
            .iter()
            .map(|stop| (stop.stop_sequence, stop.along.round()))
            .collect()
    }

    #[test]
    fn metres_round_trip() {
        let point = offset(1_234.0, -567.0);
        let back = Metres::from_lat_lng(point, ORIGIN);
        assert!((back.east - 1_234.0).abs() < 1e-6, "{back:?}");
        assert!((back.north + 567.0).abs() < 1e-6, "{back:?}");
        assert!((point.lat - ORIGIN.lat + 567.0 / METRES_PER_DEGREE).abs() < 1e-12);
    }

    // The trip goes 1 km east and returns on the same road. The stop at 200 m
    // is served in both directions. The return stop must not take the first pass.
    #[test]
    fn stop_served_twice_takes_pass_after_previous_stop() {
        let shape = ShapeLine::new(&[offset(0.0, 0.0), offset(1_000.0, 0.0), offset(0.0, 4.0)])
            .map(Arc::new)
            .expect("valid shape");
        let path = TripPath::new(
            shape,
            vec![
                stop(1, offset(200.0, 0.0), 0),
                stop(2, offset(900.0, 2.0), 60),
                stop(3, offset(200.0, 4.0), 120),
            ],
        );

        assert_eq!(alongs(&path), vec![(1, 200.0), (2, 900.0), (3, 1_800.0)]);
    }

    #[test]
    fn stop_far_from_shape_is_left_out() {
        let shape = ShapeLine::new(&[offset(0.0, 0.0), offset(1_000.0, 0.0)])
            .map(Arc::new)
            .expect("valid shape");
        let path = TripPath::new(
            shape,
            vec![
                stop(1, offset(0.0, 0.0), 0),
                stop(2, offset(500.0, 300.0), 60),
                stop(3, offset(1_000.0, 0.0), 120),
            ],
        );

        assert_eq!(alongs(&path), vec![(1, 0.0), (3, 1_000.0)]);
    }

    #[test]
    fn scheduled_offset_is_interpolated_between_stops() {
        let shape = ShapeLine::new(&[offset(0.0, 0.0), offset(1_000.0, 0.0)])
            .map(Arc::new)
            .expect("valid shape");
        let path = TripPath::new(
            shape,
            vec![
                stop(1, offset(100.0, 0.0), 100),
                stop(2, offset(900.0, 0.0), 300),
            ],
        );

        assert_eq!(path.scheduled_offset_at(300.0).map(f64::round), Some(150.0));
        assert_eq!(path.scheduled_offset_at(50.0), None);
        assert_eq!(path.scheduled_offset_at(950.0), None);
    }

    #[test]
    fn points_between_gives_corners_in_order() {
        let shape = ShapeLine::new(&[
            offset(0.0, 0.0),
            offset(100.0, 0.0),
            offset(100.0, 100.0),
            offset(200.0, 100.0),
        ])
        .expect("valid shape");

        let corners = shape
            .points_between(50.0, 250.0)
            .map(|point| {
                let m = Metres::from_lat_lng(point, ORIGIN);
                (m.east.round(), m.north.round())
            })
            .collect::<Vec<_>>();

        assert_eq!(corners, vec![(100.0, 0.0), (100.0, 100.0)]);
    }
}
