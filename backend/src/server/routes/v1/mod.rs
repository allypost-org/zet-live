use std::{
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
    sync::{
        Arc, LazyLock, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

use _entity::{gbfs::GbfsStation, vehicle::Vehicle};
use axum::{
    Router,
    body::Bytes,
    extract::DefaultBodyLimit,
    routing::{delete, get, post},
};
use sqlx::AssertSqlSafe;
use tokio::sync::{RwLock, watch};
use tracing::{error, trace, warn};

use crate::{
    admin::settings::GlobalNotice,
    database::Database,
    entity::util::{mixed_value::MixedValue, versioned::Versioned},
    proto::{
        gbfs::fetcher::wait_for_gbfs_update,
        gtfs_realtime::{
            data::transit_realtime::FeedMessage,
            fetcher::{get_cached_feed, wait_for_feed_update},
        },
    },
};

mod _entity;
pub mod admin_notifications;
mod app;
pub mod auth;
mod capabilities;
mod feed;
mod feedback;
mod gbfs;
mod schedule;
mod settings;
mod vehicles;
pub mod ws;

pub fn create_v1_router() -> Router {
    let app_state = Arc::new(V1AppState::new());
    tokio::task::spawn(feed_listener(app_state.clone()));
    tokio::task::spawn(gbfs_listener(app_state.clone()));

    let _ = V1_APP_STATE.set(app_state.clone());

    Router::new()
        .route("/version", get(app::get_version))
        .route("/vehicles", get(vehicles::get_all))
        .route("/feed", get(feed::get_feed))
        .route("/ws", get(ws::websocket_handler))
        .route("/gbfs/stations", get(gbfs::get_stations))
        .route("/capabilities", get(capabilities::get_capabilities))
        .route("/auth/{provider}/start", get(auth::start))
        // NOTE: `/auth/{provider}/callback` is registered in
        // `routes::create_router` with a longer (30 s) timeout, since it makes
        // two sequential outbound HTTPS calls to the provider.
        .route("/auth/logout", post(auth::logout))
        .route("/auth/link-ticket", post(auth::link_ticket))
        .route("/auth/transfer", post(auth::transfer))
        .route("/auth/me", get(auth::me))
        .route("/auth/sessions", get(auth::list_sessions))
        .route("/auth/sessions/revoke-all", post(auth::revoke_all_sessions))
        .route("/auth/sessions/{id}", delete(auth::delete_session))
        .route("/auth/identities/{provider}", delete(auth::unlink))
        .route("/auth/account", delete(auth::delete_account))
        .route("/auth/facebook/deletion", post(auth::facebook_deletion))
        .route(
            "/auth/facebook/deletion/status/{code}",
            get(auth::facebook_deletion_status),
        )
        .route(
            "/settings",
            get(settings::get_settings)
                .put(settings::put_settings)
                // Settings are a handful of booleans/short strings; cap well
                // below axum's 2 MB default to prevent DB bloat (2 MB × N users).
                .layer(DefaultBodyLimit::max(64 * 1024)),
        )
        .route("/schedule/routes", get(schedule::get_routes))
        .route("/schedule/routes/{id}", get(schedule::get_route))
        .route("/schedule/stops", get(schedule::get_stops))
        .route("/schedule/stops/{id}", get(schedule::get_stop))
        .route("/schedule/simple-stops", get(schedule::get_simple_stops))
        .route("/schedule/stop-trips", get(schedule::get_stop_trips))
        .route("/schedule/trips", get(schedule::get_trips))
        .route("/schedule/trips/{id}", get(schedule::get_trip))
        .route("/schedule/shapes", get(schedule::get_shapes))
        .route("/schedule/shapes/{id}", get(schedule::get_shape))
        .route(
            "/schedule/shapes/for-trip/{id}",
            get(schedule::get_shape_for_trip),
        )
        .route(
            "/schedule/trip-info/{trip_id}",
            get(schedule::get_trip_info),
        )
        .route("/feedback", post(feedback::submit))
        .route("/feedback/mine", get(feedback::mine))
        .with_state(app_state)
}

type InitialStateData = prost::bytes::Bytes;
type InitialStateEntry = RwLock<InitialStateData>;
pub struct InitialState {
    vehicles: InitialStateEntry,
    active_stops: InitialStateEntry,
    notices: InitialStateEntry,
    gbfs_stations: InitialStateEntry,
}
#[allow(dead_code)]
impl InitialState {
    pub fn new() -> Self {
        Self {
            vehicles: RwLock::new(Bytes::new()),
            active_stops: RwLock::new(Bytes::new()),
            notices: RwLock::new(Bytes::new()),
            gbfs_stations: RwLock::new(Bytes::new()),
        }
    }

    pub async fn vehicles(&self) -> tokio::sync::RwLockReadGuard<'_, InitialStateData> {
        self.vehicles.read().await
    }

    pub async fn update_vehicles(&self, vehicles: InitialStateData) {
        *self.vehicles.write().await = vehicles;
    }

    pub async fn active_stops(&self) -> tokio::sync::RwLockReadGuard<'_, InitialStateData> {
        self.active_stops.read().await
    }

    pub async fn update_active_stops(&self, active_stops: InitialStateData) {
        *self.active_stops.write().await = active_stops;
    }

    pub async fn notices(&self) -> tokio::sync::RwLockReadGuard<'_, InitialStateData> {
        self.notices.read().await
    }

    pub async fn update_notices(&self, notices: InitialStateData) {
        *self.notices.write().await = notices;
    }

    pub async fn gbfs_stations(&self) -> tokio::sync::RwLockReadGuard<'_, InitialStateData> {
        self.gbfs_stations.read().await
    }

    pub async fn update_gbfs_stations(&self, gbfs_stations: InitialStateData) {
        *self.gbfs_stations.write().await = gbfs_stations;
    }
}

pub static INITIAL_STATE: LazyLock<Arc<InitialState>> =
    LazyLock::new(|| Arc::new(InitialState::new()));

static V1_APP_STATE: OnceLock<Arc<V1AppState>> = OnceLock::new();

pub fn serialize_notices(notices: &[GlobalNotice]) -> Option<Vec<u8>> {
    let versioned = Versioned::new(1, Broadcast::Notices(notices.to_vec()));
    minicbor_serde::to_vec(&versioned).ok()
}

pub async fn broadcast_notices(notices: &[GlobalNotice]) {
    let Some(bytes) = serialize_notices(notices) else {
        return;
    };

    let bytes = Bytes::from(bytes);

    INITIAL_STATE
        .update_notices(if notices.is_empty() {
            Bytes::new()
        } else {
            bytes.clone()
        })
        .await;

    if let Some(state) = V1_APP_STATE.get() {
        state.send_transmission(Transmission::BroadcastToAll(bytes));
    }
}

async fn feed_listener(app_state: Arc<V1AppState>) {
    if let Some(feed) = get_cached_feed().await {
        process_feed(app_state.clone(), feed);
    }
    loop {
        let feed = wait_for_feed_update().await;
        trace!(?feed.header, "Got feed update on v1 router");
        process_feed(app_state.clone(), feed);
    }
}

async fn gbfs_listener(app_state: Arc<V1AppState>) {
    broadcast_gbfs_snapshot(&app_state).await;
    loop {
        wait_for_gbfs_update().await;
        trace!("Got GBFS update on v1 router");
        broadcast_gbfs_snapshot(&app_state).await;
    }
}

async fn broadcast_gbfs_snapshot(app_state: &Arc<V1AppState>) {
    let stations = match fetch_gbfs_stations().await {
        Ok(stations) => stations,
        Err(e) => {
            error!(?e, "Failed to fetch GBFS stations");
            return;
        }
    };

    trace!(stations = stations.len(), "Building GBFS snapshot");

    let stations_simple = stations
        .iter()
        .map(GbfsStation::to_simple)
        .collect::<Vec<_>>();

    let stations_bytes =
        match minicbor_serde::to_vec(Versioned::new(1, Broadcast::GbfsStations(stations_simple))) {
            Ok(bytes) => bytes,
            Err(e) => {
                error!(?e, "Failed to serialize GBFS stations");
                return;
            }
        };

    let hash = {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        stations_bytes.hash(&mut hasher);
        hasher.finish()
    };
    if hash == LAST_GBFS_HASH.swap(hash, Ordering::Relaxed) {
        trace!("GBFS snapshot unchanged, skipping broadcast");
        return;
    }

    let stations_bytes = Bytes::from(stations_bytes);

    INITIAL_STATE
        .update_gbfs_stations(stations_bytes.clone())
        .await;

    app_state.send_transmission(Transmission::BroadcastToAll(stations_bytes));
}

pub async fn fetch_gbfs_stations() -> Result<Vec<GbfsStation>, sqlx::Error> {
    let rows = sqlx::query!(
        "
        SELECT
              s.station_id AS \"station_id!\"
            , s.name
            , s.lat       AS \"lat!: f64\"
            , s.lon       AS \"lon!: f64\"
            , s.capacity
            , st.num_bikes_available
            , st.num_docks_available
            , st.is_renting
            , st.is_returning
        FROM gbfs_stations s
        LEFT JOIN gbfs_station_status st ON st.station_id = s.station_id
        WHERE st.is_installed = 1 OR st.is_installed IS NULL
        "
    )
    .fetch_all(&Database::pool())
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| GbfsStation {
            station_id: r.station_id,
            name: r.name,
            lat: r.lat,
            lon: r.lon,
            num_bikes_available: r.num_bikes_available,
            num_docks_available: r.num_docks_available,
            is_renting: r.is_renting.is_some_and(|v| v != 0),
            is_returning: r.is_returning.is_some_and(|v| v != 0),
            capacity: r.capacity,
        })
        .collect())
}

pub static SIMPLE_STOPS: LazyLock<RwLock<Vec<Vec<MixedValue>>>> =
    LazyLock::new(|| RwLock::new(Vec::new()));

static LAST_GBFS_HASH: AtomicU64 = AtomicU64::new(0);

#[allow(clippy::too_many_lines)]
fn process_feed(app_state: Arc<V1AppState>, feed: Arc<FeedMessage>) {
    let active_stops_feed = feed.clone();
    let active_stops_app_state = app_state.clone();
    tokio::task::spawn(async move {
        let active_stops = {
            let start = Instant::now();
            let current_feed_trip_ids = active_stops_feed
                .entity
                .iter()
                .filter_map(|x| x.vehicle.as_ref())
                .filter_map(|x| x.trip.as_ref())
                .map(|x| x.trip_id().to_string())
                .collect::<HashSet<_>>();

            if current_feed_trip_ids.is_empty() {
                warn!(?active_stops_feed, "Got empty active trips");
                return;
            }

            trace!(current_feed_trips = ?current_feed_trip_ids.len(), "Updating active trips");
            let stmts_start = Instant::now();

            {
                let tx = Database::pool().begin().await;
                let mut tx = match tx {
                    Ok(tx) => tx,
                    Err(e) => {
                        error!(?e, "Failed to begin transaction for active trips");
                        return;
                    }
                };

                if let Err(e) = Database::logged(
                    "delete_live_trips",
                    sqlx::query!("DELETE FROM live_trips").execute(&mut *tx),
                )
                .await
                {
                    error!(?e, "Failed to delete live trips");
                    return;
                }

                for trip_id in &current_feed_trip_ids {
                    if let Err(e) =
                        sqlx::query!("INSERT INTO live_trips (trip_id) VALUES (?)", trip_id)
                            .execute(&mut *tx)
                            .await
                    {
                        error!(?e, "Failed to insert live trip");
                        return;
                    }
                }

                if let Err(e) = tx.commit().await {
                    error!(?e, "Failed to commit live trips");
                    return;
                }
            }

            Database::optimize().await;

            {
                let stops = Database::logged(
                    "active_stops_simple",
                    sqlx::query!(
                        "
                        SELECT DISTINCT
                            s.stop_id, s.stop_name, s.longitude, s.latitude
                        FROM live_trips lt
                        INNER JOIN gtfs_stop_times st on st.trip_id = lt.trip_id
                        INNER JOIN gtfs_stops s on s.stop_id = st.stop_id
                        ",
                    )
                    .fetch_all(&Database::pool()),
                )
                .await;

                match stops {
                    Ok(stops) => {
                        let stops = stops
                            .into_iter()
                            .filter_map(|x| {
                                Some(vec![
                                    x.stop_id.into(),
                                    x.stop_name?.into(),
                                    x.latitude?.into(),
                                    x.longitude?.into(),
                                ])
                            })
                            .collect::<Vec<_>>();

                        if !stops.is_empty() {
                            SIMPLE_STOPS.write().await.clone_from(&stops);
                        }
                    }
                    Err(e) => {
                        error!(?e, "Failed to query simple stops for active trips");
                    }
                }
            }

            trace!(took = ?stmts_start.elapsed(), "Updated active trips");

            let active_stop_ids: Vec<String> = {
                let rows = Database::logged(
                    "active_stop_ids",
                    sqlx::query_scalar!(
                        "
                        SELECT DISTINCT
                            stop_id
                        FROM live_trips lt
                        LEFT JOIN gtfs_stop_times gst ON lt.trip_id = gst.trip_id
                        "
                    )
                    .fetch_all(&Database::pool()),
                )
                .await;

                match rows {
                    Ok(rows) => rows.into_iter().flatten().collect(),
                    Err(e) => {
                        error!(?e, "Error getting active stops");
                        return;
                    }
                }
            };

            trace!(
                took = ?start.elapsed(),
                stops = ?active_stop_ids.len(),
                "Got active stops"
            );

            active_stop_ids
        };

        let active_stops = Versioned::new(1, Broadcast::ActiveStops(active_stops));
        let active_stops = minicbor_serde::to_vec(&active_stops);

        let active_stops = match active_stops {
            Ok(active_stops) => active_stops,
            Err(e) => {
                error!(?e, "Error serializing active stops");
                return;
            }
        };

        let active_stops = Bytes::from(active_stops);

        INITIAL_STATE
            .update_active_stops(active_stops.clone())
            .await;

        active_stops_app_state.send_transmission(Transmission::BroadcastToAll(active_stops));
    });

    let vehicles_feed = feed;
    let vehicles_app_state = app_state;
    tokio::task::spawn(async move {
        struct NextStopInfo {
            stop_id: String,
            stop_sequence: u64,
            arrival_delay: Option<i64>,
            arrival_time: Option<i64>,
        }

        struct LiveStopTimeInfo {
            stop_id: String,
            stop_sequence: u64,
            arrival_time: Option<i64>,
            arrival_delay: Option<i64>,
        }

        #[derive(sqlx::FromRow)]
        struct RouteLongNameRow {
            route_id: String,
            route_long_name: Option<String>,
        }

        #[derive(sqlx::FromRow)]
        struct TripHeadsignRow {
            trip_id: String,
            trip_headsign: Option<String>,
        }

        let current_stop_sequences = vehicles_feed
            .entity
            .iter()
            .filter_map(|x| x.vehicle.as_ref())
            .filter_map(|vp| {
                let trip_id = vp.trip.as_ref()?.trip_id();
                if trip_id.is_empty() {
                    return None;
                }
                vp.current_stop_sequence
                    .map(|seq| (trip_id.to_string(), seq))
            })
            .collect::<HashMap<_, _>>();

        let (trip_updates, all_stop_times): (HashMap<_, _>, HashMap<_, _>) = vehicles_feed
            .entity
            .iter()
            .filter_map(|x| x.trip_update.as_ref())
            .filter_map(|tu| {
                let trip_id = tu.trip.trip_id().to_string();
                if trip_id.is_empty() {
                    return None;
                }

                // The GTFS-RT feed's first StopTimeUpdate may be for a stop
                // the vehicle has already passed (feed lag). Skip any stops
                // whose predicted arrival is more than 30s in the past so the
                // frontend always highlights a genuinely upcoming stop.
                // Also skip stops before the vehicle's current_stop_sequence
                // from VehiclePosition, since those are definitely passed.
                let now_secs = jiff::Timestamp::now().as_second();
                let min_stop_seq = current_stop_sequences.get(&trip_id);
                let first_stu = tu
                    .stop_time_update
                    .iter()
                    .find(|stu| {
                        let seq_ok = min_stop_seq
                            .is_none_or(|&min| stu.stop_sequence.is_none_or(|seq| seq >= min));
                        seq_ok
                            && stu
                                .arrival
                                .as_ref()
                                .and_then(|a| a.time)
                                .is_none_or(|time| time >= now_secs - 30)
                    })
                    .or_else(|| tu.stop_time_update.last())?;
                let first_stop_sequence = first_stu.stop_sequence?;

                let next_stop = NextStopInfo {
                    stop_id: first_stu.stop_id().to_string(),
                    stop_sequence: u64::from(first_stop_sequence),
                    arrival_delay: first_stu
                        .arrival
                        .as_ref()
                        .and_then(|a| a.delay.map(Into::into)),
                    arrival_time: first_stu.arrival.as_ref().and_then(|a| a.time),
                };

                let all_stus = tu
                    .stop_time_update
                    .iter()
                    .filter_map(|stu| {
                        let stop_sequence = stu.stop_sequence?.into();
                        Some(LiveStopTimeInfo {
                            stop_id: stu.stop_id().to_string(),
                            stop_sequence,
                            arrival_time: stu.arrival.as_ref().and_then(|a| a.time),
                            arrival_delay: stu
                                .arrival
                                .as_ref()
                                .and_then(|a| a.delay.map(Into::into)),
                        })
                    })
                    .collect::<Vec<_>>();

                Some(((trip_id.clone(), next_stop), (trip_id, all_stus)))
            })
            .unzip();

        let vehicles = vehicles_feed
            .entity
            .iter()
            .filter_map(|x| x.vehicle.as_ref())
            .filter_map(|x| Vehicle::try_from(x).ok())
            .map(|mut v| {
                if let Some(next) = trip_updates.get(&v.trip_id) {
                    v.next_stop_id = Some(next.stop_id.clone());
                    v.next_stop_sequence = Some(next.stop_sequence);
                    v.next_stop_arrival_delay = next.arrival_delay;
                    v.next_stop_arrival_time = next.arrival_time;
                }
                v
            })
            .collect::<Vec<_>>();

        let route_long_names: HashMap<String, String> = {
            let route_ids = vehicles
                .iter()
                .map(|v| v.route_id.clone())
                .collect::<HashSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();

            if route_ids.is_empty() {
                HashMap::new()
            } else {
                let query = format!(
                    "
                    SELECT
                          route_id
                        , NULLIF(route_long_name, '') AS route_long_name
                    FROM gtfs_routes
                    WHERE route_id IN ({})
                    ",
                    route_ids.iter().map(|_| "?").collect::<Vec<_>>().join(", "),
                );
                let mut q = sqlx::query_as::<_, RouteLongNameRow>(AssertSqlSafe(query));
                for id in &route_ids {
                    q = q.bind(id);
                }
                let rows = Database::logged("route_long_names", q.fetch_all(&Database::pool()))
                    .await
                    .unwrap_or_default();

                rows.into_iter()
                    .filter_map(|row| row.route_long_name.map(|name| (row.route_id, name)))
                    .collect()
            }
        };

        trace!(current_vehicles = ?vehicles.len(), "Updating vehicles");

        let previous_positions = {
            let rows = Database::logged(
                "previous_positions",
                sqlx::query!(
                    "
                    SELECT
                          vehicle_id
                        , latitude
                        , longitude
                        , bearing
                    FROM live_vehicles
                    ",
                )
                .fetch_all(&Database::pool()),
            )
            .await;

            match rows {
                Ok(rows) => rows
                    .into_iter()
                    .map(|r| (r.vehicle_id, (r.latitude, r.longitude, r.bearing)))
                    .collect(),
                Err(e) => {
                    error!(?e, "Error fetching previous positions");
                    HashMap::new()
                }
            }
        };

        let trip_headsigns = {
            let trip_ids = vehicles
                .iter()
                .map(|v| v.trip_id.clone())
                .collect::<HashSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();

            if trip_ids.is_empty() {
                HashMap::new()
            } else {
                let query = format!(
                    "
                    SELECT
                          trip_id
                        , NULLIF(trip_headsign, '') AS trip_headsign
                    FROM gtfs_trips
                    WHERE trip_id IN ({})
                    ",
                    trip_ids.iter().map(|_| "?").collect::<Vec<_>>().join(", "),
                );
                let mut q = sqlx::query_as::<_, TripHeadsignRow>(AssertSqlSafe(query));
                for id in &trip_ids {
                    q = q.bind(id);
                }
                let rows = Database::logged("trip_headsigns", q.fetch_all(&Database::pool()))
                    .await
                    .unwrap_or_default();

                rows.into_iter()
                    .filter_map(|row| row.trip_headsign.map(|name| (row.trip_id, name)))
                    .collect()
            }
        };

        let vehicles = vehicles
            .into_iter()
            .map(|mut v| {
                v.route_long_name = route_long_names.get(&v.route_id).cloned();
                v.trip_headsign = trip_headsigns.get(&v.trip_id).cloned();
                if let Some((prev_lat, prev_lng, prev_bearing)) = previous_positions.get(&v.id) {
                    let dist = haversine_distance(*prev_lat, *prev_lng, v.latitude, v.longitude);
                    if dist < 5.0 {
                        v.latitude = *prev_lat;
                        v.longitude = *prev_lng;
                        v.bearing = *prev_bearing;
                    } else {
                        v.prev_latitude = Some(*prev_lat);
                        v.prev_longitude = Some(*prev_lng);
                        v.bearing = Some(
                            (v.longitude - *prev_lng)
                                .atan2(v.latitude - *prev_lat)
                                .to_degrees(),
                        );
                    }
                }
                v
            })
            .collect::<Vec<_>>();

        let stmts_start = Instant::now();

        {
            let schedule_offsets = {
                let trip_ids = all_stop_times.keys().cloned().collect::<Vec<_>>();
                if trip_ids.is_empty() {
                    HashMap::new()
                } else {
                    let sql = format!(
                        "
                        SELECT
                              trip_id
                            , stop_sequence
                            , arrival_time_seconds
                        FROM gtfs_stop_times
                        WHERE   trip_id IN ({})
                            AND arrival_time_seconds IS NOT NULL
                        ",
                        trip_ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ")
                    );
                    let mut map = HashMap::new();
                    let mut q = sqlx::query_as::<_, (String, i64, i64)>(AssertSqlSafe(sql));
                    for id in &trip_ids {
                        q = q.bind(id);
                    }
                    if let Ok(rows) =
                        Database::logged("schedule_offsets", q.fetch_all(&Database::pool())).await
                    {
                        for (trip_id, stop_sequence, offset) in rows {
                            map.insert((trip_id, stop_sequence), offset);
                        }
                    }
                    map
                }
            };

            let best_base = schedule::compute_base_midnight(all_stop_times.iter().flat_map(
                |(trip_id, stops)| {
                    stops.iter().map(|s| {
                        let offset = schedule_offsets
                            .get(&(trip_id.clone(), {
                                #[allow(clippy::cast_possible_wrap)]
                                {
                                    s.stop_sequence as i64
                                }
                            }))
                            .copied();
                        (s.arrival_time, s.arrival_delay, offset)
                    })
                },
            ));

            let mut tx = match Database::pool().begin().await {
                Ok(tx) => tx,
                Err(e) => {
                    error!(?e, "Failed to begin transaction for vehicles");
                    return;
                }
            };

            if let Err(e) = Database::logged(
                "delete_live_vehicles",
                sqlx::query!("DELETE FROM live_vehicles").execute(&mut *tx),
            )
            .await
            {
                error!(?e, "Failed to delete live vehicles");
                return;
            }

            if let Err(e) = Database::logged(
                "delete_live_trip_stop_times",
                sqlx::query!("DELETE FROM live_trip_stop_times").execute(&mut *tx),
            )
            .await
            {
                error!(?e, "Failed to delete live trip stop times");
                return;
            }

            for vehicle in &vehicles {
                let id = vehicle.id.as_str();
                let route_id = vehicle.route_id.as_str();
                let trip_id = vehicle.trip_id.as_str();
                let route_long_name = vehicle.route_long_name.as_deref();
                let trip_headsign = vehicle.trip_headsign.as_deref();
                let latitude = vehicle.latitude;
                let longitude = vehicle.longitude;
                let prev_latitude = vehicle.prev_latitude;
                let prev_longitude = vehicle.prev_longitude;
                let bearing = vehicle.bearing;
                let next_stop_id = vehicle.next_stop_id.as_deref();
                let next_stop_sequence = vehicle.next_stop_sequence.map(|v| {
                    #[allow(clippy::cast_possible_truncation)]
                    {
                        v as i32
                    }
                });
                let next_stop_arrival_delay = vehicle.next_stop_arrival_delay;
                let next_stop_arrival_time = vehicle.next_stop_arrival_time;

                let q = sqlx::query!(
                    "
                    INSERT INTO
                    live_vehicles
                        ( vehicle_id
                        , route_id
                        , trip_id
                        , route_long_name
                        , trip_headsign
                        , latitude
                        , longitude
                        , prev_latitude
                        , prev_longitude
                        , bearing
                        , next_stop_id
                        , next_stop_sequence
                        , next_stop_arrival_delay
                        , next_stop_arrival_time
                        )
                    VALUES
                        ( ?
                        , ?
                        , ?
                        , ?
                        , ?
                        , ?
                        , ?
                        , ?
                        , ?
                        , ?
                        , ?
                        , ?
                        , ?
                        , ?
                        )
                    ",
                    id,
                    route_id,
                    trip_id,
                    route_long_name,
                    trip_headsign,
                    latitude,
                    longitude,
                    prev_latitude,
                    prev_longitude,
                    bearing,
                    next_stop_id,
                    next_stop_sequence,
                    next_stop_arrival_delay,
                    next_stop_arrival_time,
                );

                if let Err(e) = q.execute(&mut *tx).await {
                    error!(?e, "Failed to insert live vehicle");
                    return;
                }
            }

            for (trip_id, stop_times) in &all_stop_times {
                for stu in stop_times {
                    let stop_id: &str = stu.stop_id.as_str();
                    #[allow(clippy::cast_possible_truncation)]
                    let stop_sequence = stu.stop_sequence as i32;

                    let q = sqlx::query!(
                        "
                        INSERT INTO
                        live_trip_stop_times
                            ( trip_id
                            , stop_id
                            , stop_sequence
                            , arrival_time
                            , arrival_delay
                            )
                        VALUES
                            ( ?
                            , ?
                            , ?
                            , ?
                            , ?
                            )
                        ",
                        trip_id,
                        stop_id,
                        stop_sequence,
                        stu.arrival_time,
                        stu.arrival_delay,
                    );

                    if let Err(e) = q.execute(&mut *tx).await {
                        error!(?e, "Failed to insert live trip stop time");
                        return;
                    }
                }
            }

            if let Err(e) = Database::logged(
                "update_base_midnight",
                sqlx::query!(
                    "UPDATE live_feed_metadata SET base_midnight = ? WHERE id = 0",
                    best_base
                )
                .execute(&mut *tx),
            )
            .await
            {
                error!(?e, "Failed to update base midnight");
                return;
            }

            if let Err(e) = tx.commit().await {
                error!(?e, "Failed to commit vehicles transaction");
                return;
            }
        }

        trace!(took = ?stmts_start.elapsed(), "Updated vehicles");

        Database::optimize().await;

        let vehicles = tokio::task::spawn_blocking(move || {
            let simple_vehicles_feed = vehicles
                .iter()
                .map(_entity::vehicle::Vehicle::to_simple)
                .collect::<Vec<_>>();

            let vehicles = Versioned::new(1, Broadcast::Vehicles(simple_vehicles_feed));

            minicbor_serde::to_vec(&vehicles)
        })
        .await;

        let vehicles = match vehicles {
            Ok(vehicles) => vehicles,
            Err(e) => {
                error!(?e, "Error joining thread");
                return;
            }
        };

        let vehicles = match vehicles {
            Ok(vehicles) => vehicles,
            Err(e) => {
                error!(?e, "Error serializing vehicles");
                return;
            }
        };

        let vehicles = Bytes::from(vehicles);

        INITIAL_STATE.update_vehicles(vehicles.clone()).await;

        vehicles_app_state.send_transmission(Transmission::BroadcastToAll(vehicles));
    });
}

pub struct V1AppState {
    tx: watch::Sender<Arc<Transmission>>,
    pub rx: watch::Receiver<Arc<Transmission>>,
}
impl V1AppState {
    pub fn new() -> Self {
        let (tx, rx) = watch::channel(Arc::new(Transmission::Empty));

        Self { tx, rx }
    }

    pub fn send_transmission(&self, transmission: Transmission) {
        let _ = self.tx.send(Arc::new(transmission));
    }

    pub fn get_transmission_receiver(&self) -> watch::Receiver<Arc<Transmission>> {
        self.rx.clone()
    }

    pub async fn wait_for_transmission(
        &self,
        rx: &mut watch::Receiver<Arc<Transmission>>,
    ) -> Result<Arc<Transmission>, watch::error::RecvError> {
        rx.changed().await?;

        Ok(rx.borrow_and_update().clone())
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Broadcast {
    Vehicles(Vec<Vec<MixedValue>>),
    ActiveStops(Vec<String>),
    Notices(Vec<GlobalNotice>),
    /// Per-account notices (full replacement of that account's notice set).
    UserNotices(Vec<GlobalNotice>),
    Toast(ToastData),
    GbfsStations(Vec<Vec<MixedValue>>),
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToastData {
    pub message: String,
    #[serde(rename = "type")]
    pub toast_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<u32>,
}

pub enum Transmission {
    Empty,
    BroadcastToAll(Bytes),
    /// Per-account notice(s) for `user_id` (broadcast to all connection tasks,
    /// each filters by its own user). `bytes` is a serialized `Broadcast::UserNotices`.
    UserNotice {
        user_id: String,
        bytes: Bytes,
    },
}

/// Push a per-account notice update to a single account's connections.
pub fn send_user_notice(user_id: &str, notices: &[GlobalNotice]) {
    let versioned = Versioned::new(1, Broadcast::UserNotices(notices.to_vec()));
    let Ok(bytes) = minicbor_serde::to_vec(&versioned) else {
        return;
    };
    if let Some(state) = V1_APP_STATE.get() {
        state.send_transmission(Transmission::UserNotice {
            user_id: user_id.to_string(),
            bytes: Bytes::from(bytes),
        });
    }
}

/// Notify a specific session's WS connection that it was force-expired/revoked.
/// Only the connection whose `session_id` matches receives the message. Sent as
/// a JSON text frame (not CBOR binary) so it bypasses the worker's CBOR
/// pipeline and is handled directly by the main-thread WS handler.
pub fn notify_session_revoked(user_id: &str, session_id: &str) {
    let text = r#"{"v":1,"d":{"sessionRevoked":true}}"#.to_string();

    admin_notifications::ADMIN_NOTIFICATION_TX
        .send(std::sync::Arc::new(
            admin_notifications::AdminNotification::SessionRevoked {
                text,
                user_id: user_id.to_string(),
                session_id: session_id.to_string(),
            },
        ))
        .ok();
}

fn haversine_distance(lat1: f64, lng1: f64, lat2: f64, lng2: f64) -> f64 {
    let lat1_rad = lat1.to_radians();
    let lat2_rad = lat2.to_radians();
    let dlat = (lat2 - lat1).to_radians();
    let dlng = (lng2 - lng1).to_radians();

    let a = (lat1_rad.cos() * lat2_rad.cos())
        .mul_add((dlng / 2.0).sin().powi(2), (dlat / 2.0).sin().powi(2));
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());

    6_371_000.0 * c
}
