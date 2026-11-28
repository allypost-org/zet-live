//! Builds the live vehicles and live stop times from one realtime feed.

use std::collections::HashMap;

use super::{
    _entity::vehicle::Vehicle,
    next_stop::{self, NextStop},
    schedule::{compute_base_midnight, try_infer_base_midnight},
    snapshot::LiveStopTimeInfo,
    vehicle_tracker::{Located, Observation, Progress, VehicleTracker},
};
use crate::{
    database::schedule_offsets::ScheduleOffsets,
    proto::{
        gtfs_realtime::data::transit_realtime::{
            FeedMessage, TripUpdate, VehiclePosition, trip_update::StopTimeUpdate,
        },
        gtfs_schedule::data::{
            geo::{Bearing, LatLng, Metres},
            trip_key,
            trip_path::{PathStop, TripPath},
        },
    },
};

/// A smaller move is position noise, so the vehicle keeps its last position.
const MIN_MOVE_METRES: f64 = 5.0;
/// A vehicle with a larger estimated delay does not run this trip yet,
/// for example because it waits for an earlier trip to end.
const MAX_LOCATED_DELAY_SECS: u64 = 30 * 60;

/// The last position and bearing that clients got for a vehicle.
#[derive(Debug, Clone, Copy)]
pub struct PreviousPosition {
    pub position: LatLng,
    pub bearing: Option<f64>,
}

pub struct LiveVehicles {
    pub vehicles: Vec<Vehicle>,
    /// Live stop times by trip id.
    pub stop_times: HashMap<String, Vec<LiveStopTimeInfo>>,
    pub base_midnight: i64,
    /// Replaces the previous progress after the snapshot commits.
    pub progress: Progress,
}

/// The shared inputs for the vehicles of one feed.
pub struct FeedContext<'a> {
    pub previous_positions: &'a HashMap<String, PreviousPosition>,
    pub offsets: &'a ScheduleOffsets,
    pub now: i64,
}

pub fn build(
    feed: &FeedMessage,
    tracker: &VehicleTracker,
    context: &FeedContext<'_>,
) -> LiveVehicles {
    let trip_updates = feed
        .entity
        .iter()
        .filter_map(|entity| entity.trip_update.as_ref())
        .filter(|update| !update.trip.trip_id().is_empty())
        .map(|update| (update.trip.trip_id(), update))
        .collect::<HashMap<_, _>>();

    let base_midnight = compute_base_midnight(trip_updates.values().flat_map(|update| {
        let key = trip_key(update.trip.trip_id());
        update.stop_time_update.iter().map(move |stu| {
            let arrival = stu.arrival.as_ref();
            (
                arrival.and_then(|a| a.time),
                arrival.and_then(|a| a.delay.map(i64::from)),
                stu.stop_sequence
                    .and_then(|seq| context.offsets.get(&key, i64::from(seq))),
            )
        })
    }));

    let mut stop_times = trip_updates
        .iter()
        .map(|(trip_id, update)| ((*trip_id).to_owned(), live_stop_times(update)))
        .collect::<HashMap<_, _>>();

    let mut vehicles = Vec::new();
    let mut progress = Progress::default();
    for position in feed
        .entity
        .iter()
        .filter_map(|entity| entity.vehicle.as_ref())
    {
        let Ok(mut vehicle) = Vehicle::try_from(position) else {
            continue;
        };
        let key = trip_key(&vehicle.trip_id);
        let update = VehicleUpdate {
            position,
            trip_update: trip_updates.get(vehicle.trip_id.as_str()).copied(),
            trip_key: &key,
            previous: context.previous_positions.get(&vehicle.id).copied(),
            previous_progress: &tracker.progress,
            base_midnight,
            context,
        };
        let path = tracker.paths.get(&key);
        if let Some(replaced) = update.apply(&mut vehicle, path, &mut progress) {
            replaced.apply_to(stop_times.entry(vehicle.trip_id.clone()).or_default());
        }
        vehicles.push(vehicle);
    }
    LiveVehicles {
        vehicles,
        stop_times,
        base_midnight,
        progress,
    }
}

fn live_stop_times(update: &TripUpdate) -> Vec<LiveStopTimeInfo> {
    update
        .stop_time_update
        .iter()
        .filter_map(|stu| {
            Some(LiveStopTimeInfo {
                stop_id: stu.stop_id().to_owned(),
                stop_sequence: stu.stop_sequence?.into(),
                arrival_time: stu.arrival.as_ref().and_then(|a| a.time),
                arrival_delay: stu.arrival.as_ref().and_then(|a| a.delay.map(Into::into)),
            })
        })
        .collect()
}

/// The arrival at a stop, estimated from the vehicle position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EstimatedArrival {
    time: i64,
    delay: i64,
}

/// The located next stop replaced the feed next stop. The feed updates before
/// it are stale, so they are removed. The estimated arrival at the located stop
/// takes their place, so that later stops use its delay.
#[derive(Debug)]
struct ReplacedStopTimes {
    from_sequence: u64,
    estimate: Option<LiveStopTimeInfo>,
}

impl ReplacedStopTimes {
    fn apply_to(self, times: &mut Vec<LiveStopTimeInfo>) {
        times.retain(|time| time.stop_sequence >= self.from_sequence);
        if let Some(estimate) = self.estimate
            && !times
                .iter()
                .any(|time| time.stop_sequence == estimate.stop_sequence)
        {
            times.insert(0, estimate);
        }
    }
}

struct VehicleUpdate<'a> {
    position: &'a VehiclePosition,
    trip_update: Option<&'a TripUpdate>,
    trip_key: &'a str,
    previous: Option<PreviousPosition>,
    previous_progress: &'a Progress,
    base_midnight: i64,
    context: &'a FeedContext<'a>,
}

impl VehicleUpdate<'_> {
    fn stop_time_updates(&self) -> &[StopTimeUpdate] {
        self.trip_update
            .map_or(&[], |update| update.stop_time_update.as_slice())
    }

    /// Sets the position, bearing, path, and next stop of `vehicle`. Returns
    /// the change to the live stop times of the trip, if any.
    fn apply(
        &self,
        vehicle: &mut Vehicle,
        path: Option<&TripPath>,
        progress: &mut Progress,
    ) -> Option<ReplacedStopTimes> {
        let feed_next = next_stop::select_next_stop(
            self.stop_time_updates(),
            self.position.current_stop_sequence,
            self.context.now,
        );
        let located = path.and_then(|path| self.locate(vehicle, path, feed_next));
        let path = path.zip(located);
        if let Some((path, located)) = path {
            if let Some(along) = self.apply_located_position(vehicle, path, located) {
                progress.record(&vehicle.id, &vehicle.trip_id, along);
            }
        } else {
            self.apply_raw_position(vehicle);
        }

        let located_next =
            path.and_then(|(path, located)| self.located_next_stop(path, located, feed_next));
        let next = next_stop::combine(
            feed_next,
            self.stop_time_updates(),
            located_next.map(|(stop, _)| stop),
        )?;
        let estimate = located_next
            .and_then(|(_, estimate)| estimate)
            .filter(|_| matches!(next, NextStop::Located(_)));
        match next {
            NextStop::Feed(update) => {
                vehicle.next_stop_id = Some(update.stop_id().to_owned());
                vehicle.next_stop_arrival_delay =
                    update.arrival.as_ref().and_then(|a| a.delay.map(i64::from));
                vehicle.next_stop_arrival_time = update.arrival.as_ref().and_then(|a| a.time);
            }
            NextStop::Located(stop) => {
                vehicle.next_stop_id = Some(stop.stop_id.clone());
                vehicle.next_stop_arrival_delay = estimate.map(|e| e.delay);
                vehicle.next_stop_arrival_time = estimate.map(|e| e.time);
            }
        }
        let sequence = next.stop_sequence()?;
        vehicle.next_stop_sequence = Some(sequence.into());

        let feed_sequence = feed_next.and_then(|update| update.stop_sequence);
        feed_sequence
            .is_none_or(|feed| feed < sequence)
            .then(|| ReplacedStopTimes {
                from_sequence: sequence.into(),
                estimate: estimate.map(|estimate| LiveStopTimeInfo {
                    stop_id: vehicle.next_stop_id.clone().unwrap_or_default(),
                    stop_sequence: sequence.into(),
                    arrival_time: Some(estimate.time),
                    arrival_delay: Some(estimate.delay),
                }),
            })
    }

    /// The next stop from the vehicle position, and the estimated arrival at
    /// it. Returns `None` if the position does not fit the trip.
    fn located_next_stop<'p>(
        &self,
        path: &'p TripPath,
        located: Located,
        feed_next: Option<&StopTimeUpdate>,
    ) -> Option<(&'p PathStop, Option<EstimatedArrival>)> {
        let stop = path.next_stop(located.along)?;
        // A vehicle that waits at the end of a circular trip is also at the
        // end of the shape of its next trip.
        let not_started = feed_next.and_then(|update| update.stop_sequence)
            == path.stops().first().map(|first| first.stop_sequence);
        if not_started && path.stops().last() == Some(stop) {
            return None;
        }
        let estimate = self.estimate(path, located, stop);
        if estimate.is_some_and(|estimate| estimate.delay.unsigned_abs() > MAX_LOCATED_DELAY_SECS) {
            return None;
        }
        Some((stop, estimate))
    }

    fn locate(
        &self,
        vehicle: &Vehicle,
        path: &TripPath,
        feed_next: Option<&StopTimeUpdate>,
    ) -> Option<Located> {
        // The vehicle passed the stop before the feed next stop. A stale feed
        // can only be behind the vehicle, so this is a safe lower limit.
        let min_along = feed_next
            .and_then(|update| update.stop_sequence)
            .and_then(|sequence| {
                path.stops()
                    .iter()
                    .rev()
                    .find(|stop| stop.stop_sequence < sequence)
            })
            .map(|stop| stop.along);
        self.previous_progress.locate(
            &vehicle.id,
            &vehicle.trip_id,
            path,
            Observation {
                position: LatLng {
                    lat: vehicle.latitude,
                    lng: vehicle.longitude,
                },
                previous: self.previous.map(|previous| previous.position),
                min_along,
            },
        )
    }

    /// Returns the previous position if the vehicle moved less than
    /// [`MIN_MOVE_METRES`] from it to `position`. Otherwise sets the previous
    /// position of `vehicle` and returns `None`.
    fn hold_or_move(&self, vehicle: &mut Vehicle, position: LatLng) -> Option<PreviousPosition> {
        let previous = self.previous?;
        let moved = Metres::from_lat_lng(position, previous.position);
        if moved.distance(Metres::ORIGIN) < MIN_MOVE_METRES {
            vehicle.latitude = previous.position.lat;
            vehicle.longitude = previous.position.lng;
            vehicle.bearing = previous.bearing;
            return Some(previous);
        }
        vehicle.prev_latitude = Some(previous.position.lat);
        vehicle.prev_longitude = Some(previous.position.lng);
        None
    }

    /// Snaps `vehicle` to its shape and sets the shape points that it passed.
    /// Returns the distance along the shape at the position sent to clients.
    fn apply_located_position(
        &self,
        vehicle: &mut Vehicle,
        path: &TripPath,
        located: Located,
    ) -> Option<f64> {
        if self.hold_or_move(vehicle, located.position).is_some() {
            return self
                .previous_progress
                .previous_along(&vehicle.id, &vehicle.trip_id);
        }
        vehicle.latitude = located.position.lat;
        vehicle.longitude = located.position.lng;
        vehicle.bearing = Some(located.bearing.degrees());
        if self.previous.is_some() {
            vehicle.path = self.previous_progress.path_since_previous(
                &vehicle.id,
                &vehicle.trip_id,
                path,
                located.along,
            );
        }
        Some(located.along)
    }

    /// Keeps the reported position of a vehicle that is not on its shape.
    fn apply_raw_position(&self, vehicle: &mut Vehicle) {
        let position = LatLng {
            lat: vehicle.latitude,
            lng: vehicle.longitude,
        };
        if self.hold_or_move(vehicle, position).is_some() {
            return;
        }
        if let Some(previous) = self.previous {
            let moved = Metres::from_lat_lng(position, previous.position);
            vehicle.bearing = Some(Bearing::from_vector(moved.east, moved.north).degrees());
        }
    }

    /// Estimates the arrival at `stop` from the scheduled time at the vehicle
    /// position. Returns `None` before the first stop, where the vehicle can
    /// wait for its departure time.
    fn estimate(
        &self,
        path: &TripPath,
        located: Located,
        stop: &PathStop,
    ) -> Option<EstimatedArrival> {
        let scheduled_here = path.scheduled_offset_at(located.along)?;
        let scheduled_here = jiff::SignedDuration::try_from_secs_f64(scheduled_here.round())
            .ok()?
            .as_secs();
        let base = self
            .trip_base_midnight()
            .or_else(|| (self.base_midnight != 0).then_some(self.base_midnight))?;
        let observed = self
            .position
            .timestamp
            .and_then(|time| i64::try_from(time).ok())
            .unwrap_or(self.context.now);
        let stop_offset = i64::from(stop.arrival_offset?);
        let remaining = stop_offset.checked_sub(scheduled_here)?.max(0);
        let time = observed.checked_add(remaining)?;
        let delay = time.checked_sub(base.checked_add(stop_offset)?)?;
        Some(EstimatedArrival { time, delay })
    }

    /// The service-day midnight of the trip, from its feed updates.
    fn trip_base_midnight(&self) -> Option<i64> {
        self.stop_time_updates().iter().find_map(|stu| {
            let arrival = stu.arrival.as_ref()?;
            let offset = self
                .context
                .offsets
                .get(self.trip_key, i64::from(stu.stop_sequence?))?;
            try_infer_base_midnight(
                arrival.time?,
                arrival.delay.map_or(0, i64::from),
                offset,
                self.context.now,
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::proto::{
        gtfs_realtime::data::transit_realtime::{
            FeedEntity, FeedHeader, Position, TripDescriptor, VehicleDescriptor,
            trip_update::StopTimeEvent,
        },
        gtfs_schedule::data::trip_path::{ScheduledStop, ShapeLine},
    };

    const TRIP_ID: &str = "0_11_1_1_1";
    const BASE_MIDNIGHT: i64 = 1_795_737_600;
    const STOP_SPACING_METRES: f64 = 500.0;

    /// The reported position of the vehicle. A `f32` literal, as in the feed.
    fn vehicle_location() -> LatLng {
        LatLng {
            lat: f64::from(45.8_f32),
            lng: f64::from(16.0_f32),
        }
    }

    /// A 2 km straight trip 2 m south of the vehicle, with five stops 500 m
    /// apart and 60 s apart. The vehicle is `vehicle_along` metres from the start.
    fn tracker(vehicle_along: f64) -> VehicleTracker {
        let at = |east: f64| {
            Metres {
                east: east - vehicle_along,
                north: -2.0,
            }
            .to_lat_lng(vehicle_location())
        };
        let shape = ShapeLine::new(&[at(0.0), at(4.0 * STOP_SPACING_METRES)])
            .map(Arc::new)
            .expect("valid shape");
        let stops = (0..5_u32)
            .map(|i| ScheduledStop {
                stop_sequence: i + 1,
                stop_id: format!("s{}", i + 1),
                location: at(f64::from(i) * STOP_SPACING_METRES),
                arrival_offset: Some(i32::try_from(i * 60).expect("small")),
            })
            .collect();
        let mut tracker = VehicleTracker::default();
        tracker
            .paths
            .insert(trip_key(TRIP_ID), TripPath::new(shape, stops));
        tracker
    }

    fn offsets() -> ScheduleOffsets {
        ScheduleOffsets::from_trips([(
            trip_key(TRIP_ID),
            (1..=5).map(|seq| (seq, (seq - 1) * 60)).collect(),
        )])
    }

    /// One vehicle on the trip and one trip update with arrivals at `stops`,
    /// as `(stop_sequence, delay)`.
    fn feed(now: i64, stops: &[(u32, i32)]) -> FeedMessage {
        let trip = TripDescriptor {
            trip_id: Some(TRIP_ID.into()),
            route_id: Some("1".into()),
            ..Default::default()
        };
        let updates = stops
            .iter()
            .map(|&(seq, delay)| StopTimeUpdate {
                stop_sequence: Some(seq),
                stop_id: Some(format!("s{seq}")),
                arrival: Some(StopTimeEvent {
                    delay: Some(delay),
                    time: Some(BASE_MIDNIGHT + (i64::from(seq) - 1) * 60 + i64::from(delay)),
                    ..Default::default()
                }),
                ..Default::default()
            })
            .collect();
        let location = vehicle_location();
        let position = Position {
            latitude: 45.8,
            longitude: 16.0,
            ..Default::default()
        };
        assert_eq!(f64::from(position.latitude), location.lat);
        FeedMessage {
            header: FeedHeader::default(),
            entity: vec![
                FeedEntity {
                    id: "trip".into(),
                    trip_update: Some(TripUpdate {
                        trip: trip.clone(),
                        stop_time_update: updates,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                FeedEntity {
                    id: "vehicle".into(),
                    vehicle: Some(VehiclePosition {
                        trip: Some(trip),
                        vehicle: Some(VehicleDescriptor {
                            id: Some("v".into()),
                            ..Default::default()
                        }),
                        position: Some(position),
                        timestamp: u64::try_from(now).ok(),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            ],
        }
    }

    fn run(vehicle_along: f64, now: i64, stops: &[(u32, i32)]) -> LiveVehicles {
        build(
            &feed(now, stops),
            &tracker(vehicle_along),
            &FeedContext {
                previous_positions: &HashMap::new(),
                offsets: &offsets(),
                now,
            },
        )
    }

    fn next_stop(live: &LiveVehicles) -> (Option<u64>, Option<i64>, Option<i64>) {
        let vehicle = &live.vehicles[0];
        (
            vehicle.next_stop_sequence,
            vehicle.next_stop_arrival_time,
            vehicle.next_stop_arrival_delay,
        )
    }

    fn stop_times(live: &LiveVehicles) -> Vec<(u64, Option<i64>, Option<i64>)> {
        live.stop_times[TRIP_ID]
            .iter()
            .map(|time| (time.stop_sequence, time.arrival_time, time.arrival_delay))
            .collect()
    }

    // The feed keeps a stale update for seq 2 with a 15 minute delay. The
    // vehicle is 210 m past seq 3. The schedule has it there at 145 s, and
    // it is there at 200 s, so it is 55 s late.
    #[test]
    fn stale_feed_stop_is_replaced_by_located_stop() {
        let now = BASE_MIDNIGHT + 200;
        let live = run(1_210.0, now, &[(2, 900)]);

        let (time, delay) = (Some(BASE_MIDNIGHT + 180 + 55), Some(55));
        assert_eq!(next_stop(&live), (Some(4), time, delay));
        assert_eq!(stop_times(&live), vec![(4, time, delay)]);

        let vehicle = &live.vehicles[0];
        let snapped = Metres::from_lat_lng(
            LatLng {
                lat: vehicle.latitude,
                lng: vehicle.longitude,
            },
            vehicle_location(),
        );
        assert!((snapped.north + 2.0).abs() < 0.01, "{snapped:?}");
    }

    #[test]
    fn vehicle_waiting_for_trip_start_at_shape_end_keeps_feed_stop() {
        let now = BASE_MIDNIGHT - 120;
        let live = run(2_000.0, now, &[(1, 0)]);

        assert_eq!(next_stop(&live), (Some(1), Some(BASE_MIDNIGHT), Some(0)));
        assert_eq!(stop_times(&live), vec![(1, Some(BASE_MIDNIGHT), Some(0))]);
    }

    #[test]
    fn located_stop_with_implausible_delay_keeps_feed_stop() {
        let now = BASE_MIDNIGHT + 200 + 3_600;
        let live = run(1_210.0, now, &[(2, 900)]);

        assert_eq!(next_stop(&live).0, Some(2));
        assert_eq!(stop_times(&live).len(), 1);
    }

    #[test]
    fn clamped_estimate_preserves_the_service_day_base() {
        let now = BASE_MIDNIGHT + 200;
        let live = run(1_015.0, now, &[(2, 900)]);

        let estimate = (3, Some(now), Some(now - BASE_MIDNIGHT - 120));
        assert_eq!(next_stop(&live), (Some(estimate.0), estimate.1, estimate.2));
        assert_eq!(stop_times(&live), vec![estimate]);
    }

    #[test]
    fn discarded_snapshot_does_not_advance_the_path_start() {
        let lat = vehicle_location().lat;
        let point = |lng: f32| LatLng {
            lat,
            lng: f64::from(lng),
        };
        let first_corner = point(16.001);
        let second_corner = point(16.003);
        let shape = ShapeLine::new(&[point(16.0), first_corner, second_corner, point(16.004)])
            .map(Arc::new)
            .expect("valid shape");
        let mut tracker = VehicleTracker::default();
        tracker
            .paths
            .insert(trip_key(TRIP_ID), TripPath::new(shape, vec![]));
        tracker.progress.record("v", TRIP_ID, 0.0);
        let previous_positions = HashMap::from([(
            "v".to_owned(),
            PreviousPosition {
                position: point(16.0),
                bearing: None,
            },
        )]);
        let context = FeedContext {
            previous_positions: &previous_positions,
            offsets: &offsets(),
            now: BASE_MIDNIGHT,
        };
        let observation = |longitude| {
            let mut message = feed(BASE_MIDNIGHT, &[]);
            message.entity[1]
                .vehicle
                .as_mut()
                .expect("vehicle")
                .position = Some(Position {
                latitude: 45.8,
                longitude,
                ..Default::default()
            });
            message
        };

        let pending = build(&observation(16.002), &tracker, &context);
        assert_eq!(pending.vehicles[0].path.len(), 1);

        let next = build(&observation(16.004), &tracker, &context);
        let path = &next.vehicles[0].path;
        assert_eq!(path.len(), 2);
        assert!((path[0].lng - first_corner.lng).abs() < 1e-9);
        assert!((path[1].lng - second_corner.lng).abs() < 1e-9);

        tracker.progress = pending.progress;
        let published_positions = HashMap::from([(
            "v".to_owned(),
            PreviousPosition {
                position: point(16.002),
                bearing: None,
            },
        )]);
        let context = FeedContext {
            previous_positions: &published_positions,
            ..context
        };
        let next = build(&observation(16.004), &tracker, &context);
        let path = &next.vehicles[0].path;
        assert_eq!(path.len(), 1);
        assert!((path[0].lng - second_corner.lng).abs() < 1e-9);
    }
}
