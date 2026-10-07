use std::{collections::HashMap, num::TryFromIntError};

use sqlx::{Postgres, QueryBuilder, Transaction};

use super::_entity::vehicle::Vehicle;

// Fifteen bindings per vehicle keep each batch below PostgreSQL's parameter limit.
const BATCH_SIZE: usize = 2_048;

pub(super) struct LiveStopTimeInfo {
    pub stop_id: String,
    pub stop_sequence: u64,
    pub arrival_time: Option<i64>,
    pub arrival_delay: Option<i64>,
}

pub(super) async fn insert_vehicles(
    tx: &mut Transaction<'_, Postgres>,
    vehicles: &[Vehicle],
) -> anyhow::Result<()> {
    for batch in vehicles.chunks(BATCH_SIZE) {
        let rows = batch
            .iter()
            .map(|vehicle| {
                Ok::<_, TryFromIntError>((
                    vehicle,
                    crate::proto::gtfs_schedule::data::trip_key(&vehicle.trip_id),
                    vehicle.next_stop_sequence.map(i32::try_from).transpose()?,
                    vehicle
                        .next_stop_arrival_delay
                        .map(i32::try_from)
                        .transpose()?,
                    vehicle
                        .next_stop_arrival_time
                        .map(i32::try_from)
                        .transpose()?,
                ))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut query = QueryBuilder::<Postgres>::new(
            "INSERT INTO live_vehicles
             (vehicle_id, route_id, trip_id, trip_key, route_long_name, trip_headsign,
              latitude, longitude, prev_latitude, prev_longitude, bearing, next_stop_id,
              next_stop_sequence, next_stop_arrival_delay, next_stop_arrival_time) ",
        );
        query.push_values(&rows, |mut row, (vehicle, key, sequence, delay, time)| {
            row.push_bind(&vehicle.id)
                .push_bind(&vehicle.route_id)
                .push_bind(&vehicle.trip_id)
                .push_bind(key)
                .push_bind(&vehicle.route_long_name)
                .push_bind(&vehicle.trip_headsign)
                .push_bind(vehicle.latitude)
                .push_bind(vehicle.longitude)
                .push_bind(vehicle.prev_latitude)
                .push_bind(vehicle.prev_longitude)
                .push_bind(vehicle.bearing)
                .push_bind(&vehicle.next_stop_id)
                .push_bind(sequence)
                .push_bind(delay)
                .push_bind(time);
        });
        query.build().persistent(false).execute(&mut **tx).await?;
    }
    Ok(())
}

pub(super) async fn insert_stop_times(
    tx: &mut Transaction<'_, Postgres>,
    stop_times: &HashMap<String, Vec<LiveStopTimeInfo>>,
) -> anyhow::Result<()> {
    let rows = stop_times
        .iter()
        .flat_map(|(trip, stops)| stops.iter().map(move |stop| (trip, stop)))
        .map(|(trip, stop)| {
            Ok::<_, TryFromIntError>((
                trip,
                stop,
                i32::try_from(stop.stop_sequence)?,
                stop.arrival_time.map(i32::try_from).transpose()?,
                stop.arrival_delay.map(i32::try_from).transpose()?,
            ))
        })
        .collect::<Result<Vec<_>, _>>()?;
    for batch in rows.chunks(BATCH_SIZE) {
        let mut query = QueryBuilder::<Postgres>::new(
            "INSERT INTO live_trip_stop_times
             (trip_id, stop_id, stop_sequence, arrival_time, arrival_delay) ",
        );
        query.push_values(batch, |mut row, (trip, stop, sequence, time, delay)| {
            row.push_bind(trip.as_str())
                .push_bind(&stop.stop_id)
                .push_bind(sequence)
                .push_bind(time)
                .push_bind(delay);
        });
        query.build().persistent(false).execute(&mut **tx).await?;
    }
    Ok(())
}
