use std::collections::{HashMap, HashSet};

use axum::{extract::Path, http::HeaderMap, response::IntoResponse};
use axum_extra::extract::Query;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::error;

use crate::{
    database::Database,
    entity::util::versioned::Versioned,
    proto::gtfs_schedule::data::{Route, Shape, SimpleStop, Trip, trip_key},
    server::{error::ApiError, request::JsonOrAccept},
};

mod predictions;

pub use predictions::compute_base_midnight;
use predictions::{
    LiveStopTime, LiveVehicleAnchor, ScheduledStop, predict_trip_stop_times,
    try_infer_base_midnight,
};

async fn get_base_midnight() -> i64 {
    Database::logged(
        "get_base_midnight",
        sqlx::query_scalar!("SELECT base_midnight FROM live_feed_metadata WHERE id = 0")
            .fetch_optional(&Database::pool()),
    )
    .await
    .ok()
    .flatten()
    .unwrap_or_default()
}

pub async fn get_routes(headers: HeaderMap) -> impl IntoResponse {
    let routes = Database::logged(
        "get_routes",
        sqlx::query!(
            "
            SELECT *
            FROM gtfs_routes
            "
        )
        .fetch_all(&Database::pool()),
    )
    .await
    .map(|x| {
        x.into_iter()
            .map(|x| Route {
                id: x.route_id,
                agency_id: x.agency_id,
                short_name: x.route_short_name,
                long_name: x.route_long_name,
                desc: x.route_desc,
                url: x.route_url.and_then(|u| url::Url::parse(&u).ok()),
                color: x.route_color.unwrap_or_else(Route::default_route_color),
                text_color: x
                    .route_text_color
                    .unwrap_or_else(Route::default_route_text_color),
                route_type: x.route_type.and_then(|t| t.try_into().ok()),
                continuous_pickup: Default::default(),
                continuous_drop_off: Default::default(),
                network_id: None,
                sort_order: None,
            })
            .collect::<Vec<_>>()
    })
    .unwrap_or_default();

    JsonOrAccept(Versioned::new(1, routes), headers).into_response()
}

pub async fn get_route(headers: HeaderMap, Path(id): Path<String>) -> impl IntoResponse {
    let route = Database::logged(
        "get_route",
        sqlx::query!(
            "
                SELECT *
                FROM gtfs_routes
                WHERE route_id = $1
                ",
            id
        )
        .fetch_optional(&Database::pool()),
    )
    .await;

    match route {
        Ok(Some(route)) => {
            let route = Route {
                id: route.route_id,
                agency_id: route.agency_id,
                short_name: route.route_short_name,
                long_name: route.route_long_name,
                desc: route.route_desc,
                url: route.route_url.and_then(|u| url::Url::parse(&u).ok()),
                color: route.route_color.unwrap_or_else(Route::default_route_color),
                text_color: route
                    .route_text_color
                    .unwrap_or_else(Route::default_route_text_color),
                route_type: route.route_type.and_then(|t| t.try_into().ok()),
                continuous_pickup: Default::default(),
                continuous_drop_off: Default::default(),
                network_id: None,
                sort_order: None,
            };

            JsonOrAccept(Versioned::new(1, route), headers).into_response()
        }
        Ok(None) => ApiError::not_found("Route not found").into_response(),
        Err(e) => {
            error!(%e, ?id, "Failed to get route");
            ApiError::internal("Failed to get route").into_response()
        }
    }
}

pub async fn get_stops(headers: HeaderMap) -> impl IntoResponse {
    let stops = Database::logged(
        "get_stops",
        sqlx::query!(
            "
            SELECT
                  stop_id
                , stop_name
                , latitude
                , longitude
            FROM gtfs_stops
            "
        )
        .fetch_all(&Database::pool()),
    )
    .await
    .map(|x| {
        x.into_iter()
            .map(|x| SimpleStop {
                id: x.stop_id,
                name: x.stop_name.unwrap_or_default(),
                latitude: x.latitude.unwrap_or_default(),
                longitude: x.longitude.unwrap_or_default(),
            })
            .collect::<Vec<_>>()
    })
    .unwrap_or_default();

    JsonOrAccept(Versioned::new(1, stops), headers).into_response()
}

pub async fn get_stop(headers: HeaderMap, Path(id): Path<String>) -> impl IntoResponse {
    let stop = Database::logged(
        "get_stop",
        sqlx::query!(
            "
            SELECT
                  stop_id
                , stop_name
                , latitude
                , longitude
            FROM gtfs_stops
            WHERE stop_id = $1
            ",
            id
        )
        .fetch_optional(&Database::pool()),
    )
    .await;

    match stop {
        Ok(Some(stop)) => {
            let stop = SimpleStop {
                id: stop.stop_id,
                name: stop.stop_name.unwrap_or_default(),
                latitude: stop.latitude.unwrap_or_default(),
                longitude: stop.longitude.unwrap_or_default(),
            };

            JsonOrAccept(Versioned::new(1, stop), headers).into_response()
        }
        Ok(None) => ApiError::not_found("Stop not found").into_response(),
        Err(e) => {
            error!(%e, ?id, "Failed to get stop");
            ApiError::internal("Failed to get stop").into_response()
        }
    }
}

pub async fn get_simple_stops(headers: HeaderMap) -> impl IntoResponse {
    JsonOrAccept(
        Versioned::new(
            1,
            serde_json::json!({
                "simpleStops": *crate::server::routes::v1::SIMPLE_STOPS.read().await,
            }),
        ),
        headers,
    )
    .into_response()
}

#[derive(Deserialize)]
pub struct GetStopTripsQuery {
    #[serde(default)]
    pub stop: Vec<String>,
}
#[allow(clippy::too_many_lines)]
pub async fn get_stop_trips(
    headers: HeaderMap,
    Query(query): Query<GetStopTripsQuery>,
) -> impl IntoResponse {
    if query.stop.is_empty() {
        return JsonOrAccept(
            Versioned::new(
                1,
                serde_json::json!({
                    "stopTrips": Vec::<String>::new(),
                    "arrivalTimes": Vec::<StopArrivalTime>::new(),
                }),
            ),
            headers,
        )
        .into_response();
    }

    let global_base_midnight = get_base_midnight().await;

    let rows = {
        #[derive(Debug)]
        struct StopTripRow {
            vehicle_id: String,
            trip_id: String,
            route_id: String,
            stop_id: String,
            stop_sequence: i32,
            next_stop_sequence: Option<i32>,
            live_arrival_time: Option<i32>,
            live_arrival_delay: Option<i32>,
            arrival_time_seconds: Option<i32>,
            effective_delay: Option<i32>,
        }

        sqlx::query_as!(
            StopTripRow,
            "
        SELECT
              lv.vehicle_id
            , lv.trip_id
            , lv.route_id
            , gst.stop_id
            , gst.stop_sequence
            , lv.next_stop_sequence
            , lst.arrival_time  AS live_arrival_time
            , lst.arrival_delay AS live_arrival_delay
            , gst.arrival_time_seconds
            , (
                SELECT
                    lst2.arrival_delay
                FROM live_trip_stop_times lst2
                WHERE   lst2.trip_id = lv.trip_id
                    AND lst2.stop_sequence <= gst.stop_sequence
                    AND lst2.arrival_delay IS NOT NULL
                ORDER BY lst2.stop_sequence DESC LIMIT 1
            ) AS effective_delay
        FROM live_vehicles lv
        JOIN gtfs_stop_times gst ON gst.trip_key = lv.trip_key
        LEFT JOIN live_trip_stop_times lst
            ON  lst.trip_id = lv.trip_id
            AND lst.stop_sequence = gst.stop_sequence
        WHERE gst.stop_id = ANY($1)
        ORDER BY gst.stop_sequence
        ",
            &query.stop,
        )
    };
    let rows = match Database::logged("get_stop_trips", rows.fetch_all(&Database::pool())).await {
        Ok(rows) => rows,
        Err(e) => {
            error!(%e, "Failed to get stop trips");
            return ApiError::internal("Failed to get stop trips").into_response();
        }
    };

    let mut seen_vehicles = HashSet::new();
    let mut seen_trips = HashSet::new();
    let mut arrival_times = Vec::new();

    let now = jiff::Timestamp::now().as_second();

    let mut trip_base_midnight = HashMap::new();

    for row in &rows {
        if let (Some(live_time), Some(offset)) = (row.live_arrival_time, row.arrival_time_seconds) {
            let delay = row.live_arrival_delay.unwrap_or(0);
            if let Some(computed) = try_infer_base_midnight(
                i64::from(live_time),
                i64::from(delay),
                i64::from(offset),
                now,
            ) {
                trip_base_midnight
                    .entry(row.trip_id.clone())
                    .or_insert(computed);
            }
        }
    }

    for row in &rows {
        seen_trips.insert(row.trip_id.clone());

        if !seen_vehicles.insert(row.vehicle_id.clone()) {
            continue;
        }

        if let Some(next_seq) = row.next_stop_sequence
            && row.stop_sequence < next_seq
        {
            continue;
        }

        let base_midnight = trip_base_midnight
            .get(&row.trip_id)
            .copied()
            .unwrap_or(global_base_midnight);

        let predicted = if row.live_arrival_time.is_some() {
            row.live_arrival_time.map(i64::from)
        } else if let Some(offset) = row.arrival_time_seconds {
            let offset = i64::from(offset);
            row.live_arrival_delay.map_or_else(
                || {
                    row.effective_delay
                        .map(|delay| base_midnight + offset + i64::from(delay))
                },
                |delay| Some(base_midnight + offset + i64::from(delay)),
            )
        } else {
            None
        };

        arrival_times.push(StopArrivalTime {
            trip_id: row.trip_id.clone(),
            vehicle_id: row.vehicle_id.clone(),
            route_id: row.route_id.clone(),
            stop_id: row.stop_id.clone(),
            arrival_time: predicted,
        });
    }

    let stop_trips = seen_trips.into_iter().collect::<Vec<_>>();

    arrival_times.sort_by(|a, b| match (a.arrival_time, b.arrival_time) {
        (Some(a), Some(b)) => a.cmp(&b),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });

    JsonOrAccept(
        Versioned::new(
            1,
            serde_json::json!({
                "stopTrips": stop_trips,
                "arrivalTimes": arrival_times,
            }),
        ),
        headers,
    )
    .into_response()
}

pub async fn get_trips(headers: HeaderMap) -> impl IntoResponse {
    let trips = Database::logged(
        "get_trips",
        sqlx::query!(
            "
            SELECT
                trip_id
                , route_id
                , service_id
                , trip_headsign
                , trip_short_name
                , direction_id
                , block_id
                , shape_id
                , wheelchair_boarding
                , bikes_allowed
            FROM gtfs_trips
            "
        )
        .fetch_all(&Database::pool()),
    )
    .await
    .map(|rows| {
        rows.into_iter()
            .filter_map(|row| {
                Some(Trip {
                    id: row.trip_id,
                    route_id: row.route_id?,
                    service_id: row.service_id?,
                    headsign: row.trip_headsign,
                    short_name: row.trip_short_name,
                    direction_id: row.direction_id.and_then(|d| d.try_into().ok()),
                    block_id: row.block_id,
                    shape_id: row.shape_id,
                    wheelchair_boarding: row
                        .wheelchair_boarding
                        .and_then(|d| d.try_into().ok())
                        .unwrap_or_default(),
                    bikes_allowed: row
                        .bikes_allowed
                        .and_then(|d| d.try_into().ok())
                        .unwrap_or_default(),
                    stop_ids: vec![],
                })
            })
            .collect::<Vec<_>>()
    })
    .unwrap_or_default();

    JsonOrAccept(Versioned::new(1, trips), headers).into_response()
}

pub async fn get_trip(headers: HeaderMap, Path(id): Path<String>) -> impl IntoResponse {
    let trip = Database::logged(
        "get_trip",
        sqlx::query!(
            "
            SELECT
                trip_id
                , route_id
                , service_id
                , trip_headsign
                , trip_short_name
                , direction_id
                , block_id
                , shape_id
                , wheelchair_boarding
                , bikes_allowed
            FROM gtfs_trips
            WHERE trip_id = $1
            ",
            id
        )
        .fetch_optional(&Database::pool()),
    )
    .await;

    let trip = match trip {
        Ok(trip) => trip,
        Err(e) => {
            error!(%e, ?id, "Failed to get trip");
            return ApiError::internal("Failed to get trip").into_response();
        }
    };

    let trip = trip.and_then(|trip| {
        Some(Trip {
            id: trip.trip_id,
            route_id: trip.route_id?,
            service_id: trip.service_id?,
            headsign: trip.trip_headsign,
            short_name: trip.trip_short_name,
            direction_id: trip.direction_id.and_then(|d| d.try_into().ok()),
            block_id: trip.block_id,
            shape_id: trip.shape_id,
            wheelchair_boarding: trip
                .wheelchair_boarding
                .and_then(|d| d.try_into().ok())
                .unwrap_or_default(),
            bikes_allowed: trip
                .bikes_allowed
                .and_then(|d| d.try_into().ok())
                .unwrap_or_default(),
            stop_ids: vec![],
        })
    });

    JsonOrAccept(Versioned::new(1, trip), headers).into_response()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TripStopTime {
    pub stop_id: String,
    pub stop_sequence: i64,
    pub stop_name: String,
    pub arrival_time: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TripInfo {
    pub stop_ids: Vec<String>,
    pub route: Vec<(f64, f64)>,
    pub stop_times: Vec<TripStopTime>,
    /// True when the trip has no static schedule entry and the response is
    /// built from realtime data only (missing shape + full stop list).
    pub partial: bool,
}

struct TripShapeData {
    route: Vec<(f64, f64)>,
}

fn build_route_from_shapes(
    shape_rows: &[(Option<f64>, Option<f64>, Option<i64>)],
    scheduled: &[ScheduledStop],
) -> TripShapeData {
    let mut shape_points: Vec<_> = shape_rows
        .iter()
        .filter_map(|(lat, lon, sequence)| {
            Some(Coord {
                latitude: (*lat)?,
                longitude: (*lon)?,
                sequence: (*sequence)?,
            })
        })
        .collect();

    if shape_points.is_empty() {
        let route = scheduled
            .iter()
            .filter_map(|s| {
                Some(
                    Coord {
                        latitude: s.latitude?,
                        longitude: s.longitude?,
                        sequence: s.stop_sequence,
                    }
                    .as_tuple(),
                )
            })
            .collect();

        return TripShapeData { route };
    }

    shape_points.sort_by_key(|p| p.sequence);

    TripShapeData {
        route: shape_points.iter().map(Coord::as_tuple).collect(),
    }
}

async fn fetch_trip_shapes(
    trip_id: &str,
    pool: &PgPool,
) -> Result<Vec<(Option<f64>, Option<f64>, Option<i64>)>, sqlx::Error> {
    let rows = Database::logged(
        "get_trip_info_trip_shapes",
        sqlx::query!(
            "
            SELECT
                  gs.shape_pt_lat  AS \"lat!: Option<f64>\"
                , gs.shape_pt_lon  AS \"lon!: Option<f64>\"
                , gs.shape_pt_sequence AS \"sequence!: Option<i32>\"
            FROM gtfs_trips t
            LEFT JOIN gtfs_shapes gs ON gs.shape_id = t.shape_id
            WHERE t.trip_id = $1
            ORDER BY gs.shape_pt_sequence
            ",
            trip_id
        )
        .fetch_all(pool),
    )
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| (row.lat, row.lon, row.sequence.map(i64::from)))
        .collect())
}

async fn fetch_scheduled_stops(
    trip_id: &str,
    pool: &PgPool,
) -> Result<Vec<ScheduledStop>, sqlx::Error> {
    let rows = Database::logged(
        "get_trip_info_scheduled",
        sqlx::query!(
            "
            SELECT
                  st.stop_id
                , st.stop_sequence
                , st.arrival_time_seconds
                , s.stop_name
                , s.latitude
                , s.longitude
            FROM gtfs_stop_times st
            LEFT JOIN gtfs_stops s ON s.stop_id = st.stop_id
            WHERE st.trip_id = $1
            ORDER BY st.stop_sequence
            ",
            trip_id
        )
        .fetch_all(pool),
    )
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| ScheduledStop {
            stop_id: row.stop_id,
            stop_sequence: i64::from(row.stop_sequence),
            stop_name: row.stop_name.unwrap_or_default(),
            arrival_time_seconds: row.arrival_time_seconds.map(i64::from),
            latitude: row.latitude,
            longitude: row.longitude,
        })
        .collect())
}

struct LiveTripData {
    live: Vec<LiveStopTime>,
    vehicle: Option<LiveVehicleAnchor>,
}

async fn fetch_live_trip_data(trip_id: &str, pool: &PgPool) -> Result<LiveTripData, sqlx::Error> {
    let rows = Database::logged(
        "get_trip_info_live",
        sqlx::query!(
            "
            SELECT
                  lst.stop_sequence
                , lst.arrival_time
                , lst.arrival_delay
                , lv.next_stop_sequence
                , lv.next_stop_arrival_time
            FROM live_trip_stop_times lst
            LEFT JOIN live_vehicles lv ON lv.trip_id = lst.trip_id
            WHERE lst.trip_id = $1
            ORDER BY lst.stop_sequence
            ",
            trip_id
        )
        .fetch_all(pool),
    )
    .await?;

    if rows.is_empty() {
        let vehicle = Database::logged(
            "get_trip_info_live_vehicle",
            sqlx::query!(
                "SELECT next_stop_sequence, next_stop_arrival_time
                 FROM live_vehicles WHERE trip_id = $1 LIMIT 1",
                trip_id
            )
            .fetch_optional(pool),
        )
        .await?;

        return Ok(LiveTripData {
            live: Vec::new(),
            vehicle: vehicle.and_then(|row| {
                Some(LiveVehicleAnchor {
                    next_stop_sequence: row.next_stop_sequence?.into(),
                    next_stop_arrival_time: row.next_stop_arrival_time.map(i64::from),
                })
            }),
        });
    }

    let vehicle = rows.iter().find_map(|row| {
        Some(LiveVehicleAnchor {
            next_stop_sequence: row.next_stop_sequence?.into(),
            next_stop_arrival_time: row.next_stop_arrival_time.map(i64::from),
        })
    });

    let live = rows
        .into_iter()
        .map(|row| LiveStopTime {
            stop_sequence: row.stop_sequence.into(),
            arrival_time: row.arrival_time.map(i64::from),
            arrival_delay: row.arrival_delay.map(i64::from),
        })
        .collect();

    Ok(LiveTripData { live, vehicle })
}

pub async fn get_trip_info(headers: HeaderMap, Path(trip_id): Path<String>) -> impl IntoResponse {
    let pool = Database::pool();

    let canonical = match resolve_static_trip_id(&trip_key(&trip_id), &pool).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            return match build_live_only_trip_info(&trip_id, &pool).await {
                Ok(Some(info)) => JsonOrAccept(Versioned::new(1, info), headers).into_response(),
                Ok(None) => ApiError::not_found("Trip not found").into_response(),
                Err(e) => {
                    error!(%e, ?trip_id, "Failed to build live-only trip info");
                    ApiError::internal("Failed to get trip").into_response()
                }
            };
        }
        Err(e) => {
            error!(%e, ?trip_id, "Failed to resolve trip_key");
            return ApiError::internal("Failed to get trip").into_response();
        }
    };

    let (trip_shapes, scheduled, live_data, global_base_midnight) = tokio::join!(
        fetch_trip_shapes(&canonical, &pool),
        fetch_scheduled_stops(&canonical, &pool),
        fetch_live_trip_data(&trip_id, &pool),
        get_base_midnight(),
    );

    let trip_shapes = match trip_shapes {
        Ok(rows) => rows,
        Err(e) => {
            error!(%e, ?trip_id, "Failed to get trip shapes");
            return ApiError::internal("Failed to get trip").into_response();
        }
    };

    if trip_shapes.is_empty() {
        return ApiError::not_found("Trip not found").into_response();
    }

    let scheduled = match scheduled {
        Ok(stops) => stops,
        Err(e) => {
            error!(%e, ?trip_id, "Failed to get scheduled stop times");
            return ApiError::internal("Failed to get scheduled stop times").into_response();
        }
    };

    let live_data = match live_data {
        Ok(data) => data,
        Err(e) => {
            error!(%e, ?trip_id, "Failed to get live trip data");
            return ApiError::internal("Failed to get live trip data").into_response();
        }
    };

    let TripShapeData { route } = build_route_from_shapes(&trip_shapes, &scheduled);
    let stop_ids: Vec<String> = scheduled.iter().map(|s| s.stop_id.clone()).collect();

    let stop_times = predict_trip_stop_times(
        scheduled,
        &live_data.live,
        live_data.vehicle,
        global_base_midnight,
        &trip_id,
    );

    JsonOrAccept(
        Versioned::new(
            1,
            TripInfo {
                stop_ids,
                route,
                stop_times,
                partial: false,
            },
        ),
        headers,
    )
    .into_response()
}

async fn resolve_static_trip_id(
    trip_key: &str,
    pool: &PgPool,
) -> Result<Option<String>, sqlx::Error> {
    let row = Database::logged(
        "resolve_static_trip_id",
        sqlx::query!(
            "SELECT trip_id FROM gtfs_trips WHERE trip_key = $1 LIMIT 1",
            trip_key
        )
        .fetch_optional(pool),
    )
    .await?;

    Ok(row.map(|r| r.trip_id))
}

async fn build_live_only_trip_info(
    trip_id: &str,
    pool: &PgPool,
) -> Result<Option<TripInfo>, sqlx::Error> {
    let rows = Database::logged(
        "get_trip_info_live_only",
        sqlx::query!(
            "
            SELECT lst.stop_id, lst.stop_sequence, lst.arrival_time, s.stop_name
            FROM live_trip_stop_times lst
            LEFT JOIN gtfs_stops s ON s.stop_id = lst.stop_id
            WHERE lst.trip_id = $1
            ORDER BY lst.stop_sequence
            ",
            trip_id
        )
        .fetch_all(pool),
    )
    .await?;

    if rows.is_empty() {
        return Ok(None);
    }

    let stop_times = rows
        .into_iter()
        .map(|r| TripStopTime {
            stop_id: r.stop_id,
            stop_sequence: i64::from(r.stop_sequence),
            stop_name: r.stop_name.unwrap_or_default(),
            arrival_time: r.arrival_time.map(i64::from),
        })
        .collect::<Vec<_>>();

    let stop_ids = stop_times.iter().map(|s| s.stop_id.clone()).collect();

    Ok(Some(TripInfo {
        stop_ids,
        route: vec![],
        stop_times,
        partial: true,
    }))
}

struct Coord {
    latitude: f64,
    longitude: f64,
    sequence: i64,
}
impl Coord {
    pub const fn as_tuple(&self) -> (f64, f64) {
        (self.longitude, self.latitude)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StopArrivalTime {
    trip_id: String,
    vehicle_id: String,
    route_id: String,
    stop_id: String,
    arrival_time: Option<i64>,
}

pub async fn get_shapes(headers: HeaderMap) -> impl IntoResponse {
    let shapes = Database::logged(
        "get_shapes",
        sqlx::query_as!(
            Shape,
            r#"SELECT
                shape_id as "id",
                shape_pt_lat as "latitude",
                shape_pt_lon as "longitude",
                shape_pt_sequence as "sequence: i32",
                shape_dist_traveled as "distance"
            FROM gtfs_shapes"#
        )
        .fetch_all(&Database::pool()),
    )
    .await
    .unwrap_or_default();

    JsonOrAccept(Versioned::new(1, shapes), headers).into_response()
}

pub async fn get_shape(headers: HeaderMap, Path(id): Path<String>) -> impl IntoResponse {
    let shape = Database::logged(
        "get_shape",
        sqlx::query_as!(
            Shape,
            r#"SELECT
                shape_id as "id",
                shape_pt_lat as "latitude",
                shape_pt_lon as "longitude",
                shape_pt_sequence as "sequence: i32",
                shape_dist_traveled as "distance"
            FROM gtfs_shapes WHERE shape_id = $1"#,
            id
        )
        .fetch_optional(&Database::pool()),
    )
    .await;

    match shape {
        Ok(Some(shape)) => JsonOrAccept(Versioned::new(1, shape), headers).into_response(),
        Ok(None) => ApiError::not_found("Shape not found").into_response(),
        Err(e) => {
            error!(%e, "Failed to get shape");
            ApiError::internal("Failed to get shape").into_response()
        }
    }
}

pub async fn get_shape_for_trip(headers: HeaderMap, Path(id): Path<String>) -> impl IntoResponse {
    let trip = Database::logged(
        "get_shape_for_trip_trip",
        sqlx::query!("SELECT shape_id FROM gtfs_trips WHERE trip_id = $1", id)
            .fetch_optional(&Database::pool()),
    )
    .await;

    let trip = match trip {
        Ok(Some(trip)) => trip,
        Ok(None) => return ApiError::not_found("Trip not found").into_response(),
        Err(e) => {
            error!(%e, ?id, "Failed to get trip");
            return ApiError::internal("Failed to get trip").into_response();
        }
    };

    let Some(shape_id) = trip.shape_id else {
        return ApiError::not_found("Trip has no shape").into_response();
    };

    let shapes = Database::logged(
        "get_shape_for_trip_points",
        sqlx::query!(
            "
            SELECT
                shape_pt_lat
                , shape_pt_lon
            FROM gtfs_shapes
            WHERE shape_id = $1
            ",
            shape_id
        )
        .fetch_all(&Database::pool()),
    )
    .await;

    let shapes = match shapes {
        Ok(shapes) => shapes,
        Err(e) => {
            error!(%e, "Failed to get shape");
            return ApiError::internal("Failed to get shape").into_response();
        }
    };

    JsonOrAccept(
        Versioned::new(
            1,
            shapes
                .iter()
                .map(|x| (x.shape_pt_lon, x.shape_pt_lat))
                .collect::<Vec<_>>(),
        ),
        headers,
    )
    .into_response()
}
