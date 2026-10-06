//! `stop-departures` endpoint: server-merged scheduled + live departures.
//!
//! Also hosts the shared live-arrival lookup extracted from `get_stop_trips`
//! so the departures merge and the legacy endpoint see identical predictions.

use std::collections::{HashMap, HashSet};

use jiff::{
    Span, Timestamp,
    civil::{Date, Time},
    tz::TimeZone,
};
use serde::Serialize;
use sqlx::PgPool;
use tracing::error;

use super::predictions::try_infer_base_midnight;
use crate::{
    database::{Database, service_days},
    entity::util::versioned::Versioned,
    feature_flags::{self, FeatureFlag},
    server::{error::ApiError, request::JsonOrAccept},
};

/// One live vehicle's predicted arrival at one of the requested stops.
pub struct LiveArrival {
    pub vehicle_id: String,
    pub trip_id: String,
    pub trip_key: String,
    pub route_id: String,
    pub stop_id: String,
    pub headsign: String,
    pub predicted: Option<i64>,
}

/// Result of the live-arrival lookup for a set of stops.
pub struct LiveArrivals {
    /// Every trip id whose live vehicle serves any requested stop (used for
    /// map vehicle highlighting) — includes vehicles already past the stop.
    pub trip_ids: HashSet<String>,
    /// One entry per vehicle (the row for this stop), predicted-sorted.
    pub arrivals: Vec<LiveArrival>,
}

#[allow(clippy::too_many_lines)]
pub async fn fetch_live_arrivals(
    stop_ids: &[String],
    now: i64,
) -> Result<LiveArrivals, sqlx::Error> {
    #[derive(Debug)]
    struct StopTripRow {
        vehicle_id: String,
        trip_id: String,
        trip_key: String,
        trip_headsign: Option<String>,
        route_id: String,
        stop_id: String,
        stop_sequence: i32,
        next_stop_sequence: Option<i32>,
        live_arrival_time: Option<i32>,
        live_arrival_delay: Option<i32>,
        arrival_time_seconds: Option<i32>,
        effective_delay: Option<i32>,
    }

    let global_base_midnight = Database::logged(
        "get_base_midnight",
        sqlx::query_scalar!("SELECT base_midnight FROM live_feed_metadata WHERE id = 0")
            .fetch_optional(&Database::pool()),
    )
    .await
    .ok()
    .flatten()
    .unwrap_or_default();

    let rows = Database::logged(
        "fetch_live_arrivals",
        sqlx::query_as!(
            StopTripRow,
            r#"
        SELECT
              lv.vehicle_id
            , lv.trip_id
            , lv.trip_key AS "trip_key!"
            , lv.trip_headsign
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
        "#,
            stop_ids,
        )
        .fetch_all(&Database::pool()),
    )
    .await?;

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

    let mut trip_ids = HashSet::new();
    let mut seen_vehicles = HashSet::new();
    let mut arrivals = Vec::new();

    for row in rows {
        trip_ids.insert(row.trip_id.clone());

        if let Some(next_seq) = row.next_stop_sequence
            && row.stop_sequence < next_seq
        {
            continue;
        }

        if !seen_vehicles.insert(row.vehicle_id.clone()) {
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

        arrivals.push(LiveArrival {
            vehicle_id: row.vehicle_id,
            trip_id: row.trip_id,
            trip_key: row.trip_key,
            route_id: row.route_id,
            stop_id: row.stop_id,
            headsign: row.trip_headsign.unwrap_or_default(),
            predicted,
        });
    }

    arrivals.sort_by(|a, b| match (a.predicted, b.predicted) {
        (Some(a), Some(b)) => a.cmp(&b),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });

    Ok(LiveArrivals { trip_ids, arrivals })
}

/// Departure lookback/lookahead: a 2-minute grace for just-departed rows,
/// 6 hours ahead. Constant per the spec (§4.2).
const LOOKBACK_SECS: i64 = 120;
const LOOKAHEAD_SECS: i64 = 6 * 3600;
const DEFAULT_LIMIT: usize = 20;
const MAX_LIMIT: usize = 50;
/// No GTFS feed schedules service beyond ~30h after service-day midnight
/// (ZET: ~30.2h). A candidate window whose lower bound exceeds this span can
/// only match nothing — skip it instead of paying a wasted query round trip.
const MAX_SERVICE_DAY_SPAN_SECS: i32 = 36 * 3600;

/// The GTFS-seconds window for one service day that can contribute departures.
pub struct ServiceDayWindow {
    /// The civil date of this service day.
    pub date: Date,
    pub services: Vec<String>,
    /// Elapsed seconds from the GTFS reference, directly comparable with the INTEGER
    /// `departure_time_seconds` column.
    pub lo_secs: i32,
    pub hi_secs: i32,
}

fn midnight_zagreb(date: Date) -> i64 {
    let tz = TimeZone::get(service_days::ZAGREB_TZ).expect("static time zone");
    date.to_zoned(tz)
        .expect("midnight in Zagreb is never ambiguous or in a gap")
        .timestamp()
        .as_second()
}

/// GTFS times use local noon minus 12 elapsed hours, which can differ from
/// local midnight on a DST transition day.
fn gtfs_reference(date: Date) -> Timestamp {
    let tz = TimeZone::get(service_days::ZAGREB_TZ).expect("static time zone");
    date.to_datetime(Time::new(12, 0, 0, 0).expect("valid noon"))
        .to_zoned(tz)
        .expect("noon in Zagreb is never ambiguous or in a gap")
        .timestamp()
        - Span::new().hours(12)
}

fn gtfs_secs_to_timestamp(date: Date, secs: i64) -> Timestamp {
    gtfs_reference(date) + Span::new().seconds(secs)
}

/// Service days that can contribute departures in `[now-lookback, now+lookahead]`:
/// yesterday's (for after-midnight GTFS times ≥ 24:00) and today's.
pub fn candidate_windows(
    now: Timestamp,
    days: &service_days::ServiceDays,
) -> Vec<ServiceDayWindow> {
    let tz = TimeZone::get(service_days::ZAGREB_TZ).expect("static time zone");
    let today = now.to_zoned(tz).date();
    let lower = now.as_second() - LOOKBACK_SECS;
    let upper = now.as_second() + LOOKAHEAD_SECS;

    let mut out = Vec::new();
    for date in [today.yesterday().expect("today after Date::MIN"), today] {
        let services = days.services_on(date);
        if services.is_empty() {
            continue;
        }
        let reference = gtfs_reference(date).as_second();
        let lo_secs = i32::try_from(lower - reference).expect("window bound fits in i32");
        let hi_secs = i32::try_from(upper - reference).expect("window bound fits in i32");
        if lo_secs > MAX_SERVICE_DAY_SPAN_SECS {
            continue;
        }
        out.push(ServiceDayWindow {
            date,
            services,
            lo_secs,
            hi_secs,
        });
    }
    out
}

#[derive(Debug)]
pub struct ScheduledDeparture {
    pub route: String,
    pub headsign: String,
    /// Absolute unix seconds.
    pub departure: i64,
    pub trip_id: String,
    pub trip_key: String,
}

async fn fetch_scheduled_departures(
    pool: &PgPool,
    stop_ids: &[String],
    window: &ServiceDayWindow,
) -> Result<Vec<ScheduledDeparture>, sqlx::Error> {
    #[derive(Debug)]
    struct Row {
        route: String,
        headsign: Option<String>,
        departure_time_seconds: Option<i32>,
        trip_id: String,
        trip_key: String,
    }

    let rows = Database::logged(
        "fetch_scheduled_departures",
        sqlx::query_as!(
            Row,
            r#"
            SELECT
                  COALESCE(r.route_short_name, r.route_id) AS "route!"
                , COALESCE(st.stop_headsign, t.trip_headsign) AS headsign
                , st.departure_time_seconds
                , t.trip_id
                , t.trip_key AS "trip_key!"
            FROM gtfs_stop_times st
            JOIN gtfs_trips t ON t.trip_id = st.trip_id
            JOIN gtfs_routes r ON r.route_id = t.route_id
            WHERE st.stop_id = ANY($1)
              AND t.service_id = ANY($2)
              AND st.departure_time_seconds BETWEEN $3 AND $4
            ORDER BY st.departure_time_seconds
            "#,
            stop_ids,
            &window.services,
            window.lo_secs,
            window.hi_secs,
        )
        .fetch_all(pool),
    )
    .await?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let secs = row.departure_time_seconds?;
            Some(ScheduledDeparture {
                route: row.route,
                headsign: row.headsign.unwrap_or_default(),
                departure: gtfs_secs_to_timestamp(window.date, i64::from(secs)).as_second(),
                trip_id: row.trip_id,
                trip_key: row.trip_key,
            })
        })
        .collect())
}

/// A response row: positional `[routeId, headsign, scheduledTime, tripId,
/// vehicleId, predictedTime]` — live rows have non-empty vehicleId and
/// non-null predictedTime, scheduled rows `""`/`null`.
pub type DepartureRow = (String, String, i64, String, String, Option<i64>);

/// Merge scheduled departures with live arrivals by `trip_key`. `limit` caps
/// scheduled rows only; live rows are never dropped. A live row without a
/// predicted time and without a schedule match is dropped (nothing to sort by).
pub fn merge_departures(
    mut scheduled: Vec<ScheduledDeparture>,
    live: &[LiveArrival],
    limit: usize,
) -> Vec<DepartureRow> {
    scheduled.sort_by_key(|s| s.departure);
    // A selected stop group usually contains several platform stop ids, and a
    // trip may serve two of them — keep one departure per trip (the earliest).
    // `HashSet<String>` (not `&str`): a set of `&str` is invariant, so
    // reborrowing from `retain`'s `&mut` element cannot compile (E0521).
    let mut seen_trips: HashSet<String> = HashSet::new();
    scheduled.retain(|s| seen_trips.insert(s.trip_key.clone()));

    let live_by_key: HashMap<&str, &LiveArrival> =
        live.iter().map(|l| (l.trip_key.as_str(), l)).collect();

    let mut merged: Vec<(i64, DepartureRow)> = Vec::new();
    let mut used_live: HashSet<&str> = HashSet::new();
    let mut scheduled_count = 0usize;

    for s in &scheduled {
        if let Some(l) = live_by_key.get(s.trip_key.as_str()) {
            used_live.insert(l.trip_key.as_str());
            let predicted = l.predicted.unwrap_or(s.departure);
            merged.push((
                predicted,
                (
                    s.route.clone(),
                    s.headsign.clone(),
                    s.departure,
                    l.trip_id.clone(),
                    l.vehicle_id.clone(),
                    Some(predicted),
                ),
            ));
        } else if scheduled_count < limit {
            scheduled_count += 1;
            merged.push((
                s.departure,
                (
                    s.route.clone(),
                    s.headsign.clone(),
                    s.departure,
                    s.trip_id.clone(),
                    String::new(),
                    None,
                ),
            ));
        }
    }

    for l in live {
        if used_live.contains(l.trip_key.as_str()) {
            continue;
        }
        let Some(predicted) = l.predicted else {
            continue;
        };
        merged.push((
            predicted,
            (
                l.route_id.clone(),
                l.headsign.clone(),
                predicted,
                l.trip_id.clone(),
                l.vehicle_id.clone(),
                Some(predicted),
            ),
        ));
    }

    merged.sort_by_key(|(t, _)| *t);
    merged.into_iter().map(|(_, row)| row).collect()
}

#[derive(serde::Deserialize)]
pub struct GetStopDeparturesQuery {
    #[serde(default)]
    pub stop: Vec<String>,
    pub limit: Option<usize>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StopDepartures {
    departures: Vec<DepartureRow>,
    schedule_end: Option<i64>,
}

pub async fn get_stop_departures(
    headers: axum::http::HeaderMap,
    axum_extra::extract::Query(query): axum_extra::extract::Query<GetStopDeparturesQuery>,
) -> impl axum::response::IntoResponse {
    use axum::response::IntoResponse as _;

    let user = crate::auth::resolve_current_user(&headers).await;
    let flag_user = user.as_ref().map(|r| r.user.id.as_str());
    if !feature_flags::is_enabled(FeatureFlag::stop_departures(), flag_user) {
        return ApiError::not_found("Stop departures are not available").into_response();
    }

    if query.stop.is_empty() {
        return JsonOrAccept(
            Versioned::new(
                1,
                StopDepartures {
                    departures: Vec::new(),
                    schedule_end: None,
                },
            ),
            headers,
        )
        .into_response();
    }

    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);

    let now = jiff::Timestamp::now();
    let days = service_days::snapshot();
    let windows = candidate_windows(now, &days);

    let pool = Database::pool();
    let valid_stops = Database::logged(
        "validate_departure_stops",
        sqlx::query_scalar!(
            "SELECT NOT EXISTS (SELECT 1 FROM unnest($1::text[]) AS requested(stop_id)
             WHERE NOT EXISTS (SELECT 1 FROM gtfs_stops s WHERE s.stop_id = requested.stop_id))",
            &query.stop,
        )
        .fetch_one(&pool),
    )
    .await;
    match valid_stops {
        Ok(Some(true)) => {}
        Ok(_) => {
            return ApiError::with_status(axum::http::StatusCode::BAD_REQUEST, "Unknown stop ID")
                .into_response();
        }
        Err(e) => {
            error!(%e, "Failed to validate departure stops");
            return ApiError::internal("Failed to get stop departures").into_response();
        }
    }
    let scheduled_fetch = async {
        let mut scheduled = Vec::new();
        for window in &windows {
            scheduled.extend(fetch_scheduled_departures(&pool, &query.stop, window).await?);
        }
        Ok::<Vec<ScheduledDeparture>, sqlx::Error>(scheduled)
    };

    let (scheduled, live) = tokio::join!(
        scheduled_fetch,
        fetch_live_arrivals(&query.stop, now.as_second())
    );

    let scheduled = match scheduled {
        Ok(rows) => rows,
        Err(e) => {
            error!(%e, "Failed to get scheduled departures");
            return ApiError::internal("Failed to get stop departures").into_response();
        }
    };
    let live = match live {
        Ok(live) => live,
        Err(e) => {
            error!(%e, "Failed to get live arrivals for departures");
            return ApiError::internal("Failed to get stop departures").into_response();
        }
    };

    let schedule_end = days
        .feed_end()
        .map(|end| midnight_zagreb(end.tomorrow().expect("feed_end below Date::MAX")));

    let response = StopDepartures {
        departures: merge_departures(scheduled, &live.arrivals, limit),
        schedule_end,
    };

    JsonOrAccept(Versioned::new(1, &response), headers).into_response()
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use super::*;

    fn scheduled(departure: i64, trip_key: &str, route: &str) -> ScheduledDeparture {
        ScheduledDeparture {
            route: route.to_string(),
            headsign: "Headsign".to_string(),
            departure,
            trip_id: format!("0_30_{trip_key}"),
            trip_key: trip_key.to_string(),
        }
    }

    fn live(trip_key: &str, predicted: i64, vehicle: &str) -> LiveArrival {
        LiveArrival {
            vehicle_id: vehicle.to_string(),
            trip_id: format!("0_40_{trip_key}"),
            trip_key: trip_key.to_string(),
            route_id: "1".to_string(),
            stop_id: "stop".to_string(),
            headsign: "Live Headsign".to_string(),
            predicted: Some(predicted),
        }
    }

    #[test]
    fn merge_live_replaces_scheduled_for_matching_trip_key() {
        let out = merge_departures(
            vec![scheduled(1000, "a", "6"), scheduled(2000, "b", "6")],
            &[live("a", 1300, "v1")],
            20,
        );
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].0, "6");
        assert_eq!(
            (out[0].3.clone(), out[0].4.clone()),
            ("0_40_a".to_string(), "v1".to_string())
        );
        assert_eq!(out[0].5, Some(1300)); // predicted, not scheduled 1000
        assert_eq!(out[1].4, ""); // scheduled row: no vehicle
        assert_eq!(out[1].5, None);
    }

    #[test]
    fn merge_live_only_row_inserted_in_time_order() {
        let out = merge_departures(
            vec![scheduled(2000, "b", "6")],
            &[live("x", 1500, "v9")],
            20,
        );
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].2, 1500); // live-only row sorts before the 2000 scheduled
        assert_eq!(out[0].1, "Live Headsign");
        assert_eq!(out[1].2, 2000);
    }

    #[test]
    fn merge_limit_caps_scheduled_but_not_live() {
        let sched = (0..30)
            .map(|i| scheduled(1000 + i, &format!("s{i}"), "6"))
            .collect();
        let live_rows = (0..5)
            .map(|i| live(&format!("s{i}"), 900 + i, &format!("v{i}")))
            .collect::<Vec<_>>();
        let out = merge_departures(sched, &live_rows, 10);
        // 5 scheduled rows became live (not counted against the limit) + 10 capped scheduled.
        assert_eq!(out.len(), 15);
    }

    #[test]
    fn merge_drops_live_without_prediction_or_schedule() {
        let mut l = live("x", 0, "v1");
        l.predicted = None;
        let out = merge_departures(vec![], &[l], 20);
        assert!(out.is_empty());
    }

    #[test]
    fn merge_dedups_multi_platform_stop_groups() {
        // Same trip serving two stops of the selected group: one row only.
        let out = merge_departures(
            vec![scheduled(1000, "a", "6"), scheduled(1010, "a", "6")],
            &[],
            20,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].2, 1000); // earliest kept
    }

    #[test]
    fn windows_skip_days_beyond_service_span() {
        // 20:00 local: yesterday's window would start at ~44h past its
        // midnight, beyond any scheduled service — only today is queried.
        let now = "2026-10-05T18:00:00Z"
            .parse::<Timestamp>()
            .expect("valid ts");
        let zeros = [false; 7];
        let days = service_days::compute(
            &[service_days::CalendarRow {
                service_id: "s".to_string(),
                weekdays: zeros,
                start_date: date(2026, 10, 1),
                end_date: date(2026, 12, 31),
            }],
            &[
                service_days::CalendarException {
                    service_id: "s".to_string(),
                    date: date(2026, 10, 4),
                    exception_type: 1,
                },
                service_days::CalendarException {
                    service_id: "s".to_string(),
                    date: date(2026, 10, 5),
                    exception_type: 1,
                },
            ],
        );
        let windows = candidate_windows(now, &days);
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].date, date(2026, 10, 5));
    }

    #[test]
    fn windows_cover_yesterday_for_after_midnight_trips() {
        // 2026-10-26 00:30 Europe/Zagreb, after the fall-back service day.
        let now = "2026-10-25T23:30:00Z"
            .parse::<Timestamp>()
            .expect("valid ts");
        // All-zero weekday flags; service added only on 2026-10-25 (ZET shape).
        let zeros = [false; 7];
        let days = service_days::compute(
            &[service_days::CalendarRow {
                service_id: "0_30".to_string(),
                weekdays: zeros,
                start_date: date(2026, 10, 1),
                end_date: date(2026, 12, 31),
            }],
            &[service_days::CalendarException {
                service_id: "0_30".to_string(),
                date: date(2026, 10, 25),
                exception_type: 1,
            }],
        );
        let windows = candidate_windows(now, &days);
        // Only 2026-10-25 has service; it must still be a candidate at 00:30.
        assert_eq!(windows.len(), 1);
        let w = &windows[0];
        // 25:30 = 91_800 GTFS-seconds must fall inside the window.
        assert!(
            w.lo_secs <= 91_800 && 91_800 <= w.hi_secs,
            "{}..{}",
            w.lo_secs,
            w.hi_secs
        );
    }

    #[test]
    fn windows_skip_days_without_service() {
        // Sunday 2026-10-04 12:00 Zagreb (CEST).
        let now = "2026-10-04T10:00:00Z"
            .parse::<Timestamp>()
            .expect("valid ts");
        let days = service_days::compute(&[], &[]);
        assert!(candidate_windows(now, &days).is_empty());
    }

    #[test]
    fn gtfs_secs_after_fall_back() {
        let ts = gtfs_secs_to_timestamp(date(2026, 10, 25), 28_800);
        assert_eq!(ts.to_string(), "2026-10-25T07:00:00Z");
    }

    #[test]
    fn gtfs_secs_during_fall_back() {
        let ts = gtfs_secs_to_timestamp(date(2026, 10, 25), 9_000);
        assert_eq!(ts.to_string(), "2026-10-25T01:30:00Z");
    }

    #[test]
    fn gtfs_secs_past_midnight_rollover_after_fall_back() {
        // GTFS 26:30 (95400 s) on the 25th is wall 02:30 on the 26th (CET).
        let ts = gtfs_secs_to_timestamp(date(2026, 10, 25), 95_400);
        assert_eq!(ts.to_string(), "2026-10-26T01:30:00Z");
    }

    #[test]
    fn gtfs_secs_after_spring_forward() {
        let ts = gtfs_secs_to_timestamp(date(2027, 3, 28), 28_800);
        assert_eq!(ts.to_string(), "2027-03-28T06:00:00Z");
    }

    #[test]
    fn gtfs_secs_before_dst_transitions_use_noon_reference() {
        assert_eq!(
            gtfs_secs_to_timestamp(date(2026, 10, 25), 5_400).to_string(),
            "2026-10-25T00:30:00Z"
        );
        assert_eq!(
            gtfs_secs_to_timestamp(date(2027, 3, 28), 5_400).to_string(),
            "2027-03-27T23:30:00Z"
        );
    }

    #[test]
    fn windows_preserve_elapsed_bounds_across_dst_transitions() {
        for (date, now, expected_lo, expected_hi) in [
            (date(2026, 10, 25), "2026-10-24T22:30:00Z", -1_920, 19_800),
            (date(2027, 3, 28), "2027-03-27T23:30:00Z", 5_280, 27_000),
        ] {
            let days = service_days::compute(
                &[],
                &[service_days::CalendarException {
                    service_id: "s".to_string(),
                    date,
                    exception_type: 1,
                }],
            );
            let now = now.parse::<Timestamp>().expect("valid timestamp");
            let windows = candidate_windows(now, &days);
            assert_eq!(windows.len(), 1);
            let window = &windows[0];
            assert_eq!((window.lo_secs, window.hi_secs), (expected_lo, expected_hi));
            assert_eq!(
                gtfs_secs_to_timestamp(date, i64::from(window.lo_secs)).as_second(),
                now.as_second() - 120
            );
            assert_eq!(
                gtfs_secs_to_timestamp(date, i64::from(window.hi_secs)).as_second(),
                now.as_second() + 21_600
            );
        }
    }
}
