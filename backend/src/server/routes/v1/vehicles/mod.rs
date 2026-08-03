use axum::{http::HeaderMap, response::IntoResponse};

use super::_entity::vehicle::Vehicle;
use crate::{database::Database, server::request::JsonOrAccept};

pub async fn get_all(headers: HeaderMap) -> impl IntoResponse {
    let vehicles = Database::logged(
        "get_all_vehicles",
        sqlx::query!("SELECT * FROM live_vehicles").fetch_all(&Database::pool()),
    )
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|x| Vehicle {
        id: x.vehicle_id,
        route_id: x.route_id,
        trip_id: x.trip_id,
        route_long_name: x.route_long_name,
        trip_headsign: x.trip_headsign,
        latitude: x.latitude,
        longitude: x.longitude,
        bearing: x.bearing,
        prev_latitude: x.prev_latitude,
        prev_longitude: x.prev_longitude,
        next_stop_id: x.next_stop_id,
        next_stop_sequence: x.next_stop_sequence.map(|s| i64::from(s).cast_unsigned()),
        next_stop_arrival_delay: x.next_stop_arrival_delay.map(i64::from),
        next_stop_arrival_time: x.next_stop_arrival_time.map(i64::from),
    })
    .collect::<Vec<_>>();

    JsonOrAccept(vehicles, headers).into_response()
}
