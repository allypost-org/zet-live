//! In-memory map of which GTFS service ids run on which calendar dates,
//! built from `gtfs_calendar` + `gtfs_calendar_dates` with standard GTFS
//! semantics. ZET publishes all-zero weekday flags and exactly one
//! `exception_type=1` row per date, but the general rule is implemented so a
//! feed-format change does not silently change meaning. Same lifecycle as
//! `schedule_offsets`: loaded after migrations, swapped on schedule import.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, OnceLock, RwLock},
};

use jiff::civil::{Date, Weekday};
use tracing::{debug, warn};

use crate::database::Database;

pub const ZAGREB_TZ: &str = "Europe/Zagreb";

static CACHE: OnceLock<RwLock<Arc<ServiceDays>>> = OnceLock::new();

#[derive(Debug, Default)]
pub struct ServiceDays {
    by_date: HashMap<Date, HashSet<String>>,
    feed_end: Option<Date>,
}

impl ServiceDays {
    /// Service ids active on `date` (empty when the feed has no service that day).
    #[must_use]
    pub fn services_on(&self, date: Date) -> Vec<String> {
        self.by_date
            .get(&date)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// The last date any service runs on; `None` when no calendar data exists.
    #[must_use]
    pub const fn feed_end(&self) -> Option<Date> {
        self.feed_end
    }
}

/// One `gtfs_calendar` row, parsed.
#[derive(Debug)]
pub struct CalendarRow {
    pub service_id: String,
    /// `[monday, tuesday, wednesday, thursday, friday, saturday, sunday]`
    pub weekdays: [bool; 7],
    pub start_date: Date,
    pub end_date: Date,
}

/// One `gtfs_calendar_dates` row, parsed.
#[derive(Debug)]
pub struct CalendarException {
    pub service_id: String,
    pub date: Date,
    pub exception_type: i16,
}

fn weekday_runs(row: &CalendarRow, weekday: Weekday) -> bool {
    row.weekdays[usize::from(weekday.to_monday_zero_offset().unsigned_abs())]
}

/// Pure service-day expansion. A service runs on a date when its calendar
/// weekday flag is set within `[start_date, end_date]`, plus `exception_type=1`
/// additions, minus `exception_type=2` removals.
pub fn compute(calendar: &[CalendarRow], exceptions: &[CalendarException]) -> ServiceDays {
    let mut by_date: HashMap<Date, HashSet<String>> = HashMap::new();

    for row in calendar {
        let mut date = row.start_date;
        while date <= row.end_date {
            if weekday_runs(row, date.weekday()) {
                by_date
                    .entry(date)
                    .or_default()
                    .insert(row.service_id.clone());
            }
            date = date.tomorrow().expect("calendar end_date below Date::MAX");
        }
    }

    for ex in exceptions {
        match ex.exception_type {
            1 => {
                by_date
                    .entry(ex.date)
                    .or_default()
                    .insert(ex.service_id.clone());
            }
            2 => {
                if let Some(set) = by_date.get_mut(&ex.date) {
                    set.remove(&ex.service_id);
                }
            }
            _ => {}
        }
    }

    by_date.retain(|_, set| !set.is_empty());

    ServiceDays {
        feed_end: by_date.keys().max().copied(),
        by_date,
    }
}

/// Populate the cache from the DB. Called once after migrations at startup.
pub async fn init() -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let map = load().await?;
    let dates = map.by_date.len();
    let _ = CACHE.set(RwLock::new(Arc::new(map)));
    debug!(dates, elapsed = ?started.elapsed(), "service_days cache populated");
    Ok(())
}

/// Re-populate the cache after the schedule fetcher commits new calendar data.
pub async fn reload() {
    match load().await {
        Ok(map) => {
            let dates = map.by_date.len();
            if let Some(rw) = CACHE.get() {
                *rw.write().expect("service_days lock poisoned") = Arc::new(map);
                debug!(dates, "service_days cache reloaded");
            } else {
                warn!("service_days reload before init; populating now");
                let _ = CACHE.set(RwLock::new(Arc::new(map)));
            }
        }
        Err(e) => warn!(error = ?e, "failed to reload service_days cache"),
    }
}

/// Cheap snapshot — clones only the `Arc`, not the map.
#[must_use]
pub fn snapshot() -> Arc<ServiceDays> {
    CACHE
        .get()
        .and_then(|rw| rw.read().ok())
        .map_or_else(|| Arc::new(ServiceDays::default()), |guard| guard.clone())
}

fn parse_gtfs_date(s: &str) -> Option<Date> {
    Date::strptime("%Y%m%d", s).ok()
}

async fn load() -> anyhow::Result<ServiceDays> {
    let calendar_rows = sqlx::query!(
        "SELECT service_id, monday, tuesday, wednesday, thursday, friday, saturday, sunday, \
         start_date, end_date FROM gtfs_calendar"
    )
    .fetch_all(&Database::pool())
    .await?;

    let exception_rows =
        sqlx::query!("SELECT service_id, date, exception_type FROM gtfs_calendar_dates")
            .fetch_all(&Database::pool())
            .await?;

    let mut calendar = Vec::with_capacity(calendar_rows.len());
    for row in calendar_rows {
        let Some(start) = parse_gtfs_date(&row.start_date) else {
            warn!(service_id = %row.service_id, "unparseable calendar start_date");
            continue;
        };
        let Some(end) = parse_gtfs_date(&row.end_date) else {
            warn!(service_id = %row.service_id, "unparseable calendar end_date");
            continue;
        };
        calendar.push(CalendarRow {
            service_id: row.service_id,
            weekdays: [
                row.monday != 0,
                row.tuesday != 0,
                row.wednesday != 0,
                row.thursday != 0,
                row.friday != 0,
                row.saturday != 0,
                row.sunday != 0,
            ],
            start_date: start,
            end_date: end,
        });
    }

    let mut exceptions = Vec::with_capacity(exception_rows.len());
    for row in exception_rows {
        let Some(date) = parse_gtfs_date(&row.date) else {
            warn!(service_id = %row.service_id, "unparseable calendar_dates date");
            continue;
        };
        exceptions.push(CalendarException {
            service_id: row.service_id,
            date,
            exception_type: row.exception_type,
        });
    }

    Ok(compute(&calendar, &exceptions))
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use super::*;

    fn row(service: &str, weekdays: [bool; 7], start: Date, end: Date) -> CalendarRow {
        CalendarRow {
            service_id: service.to_string(),
            weekdays,
            start_date: start,
            end_date: end,
        }
    }

    fn exception(service: &str, date: Date, kind: i16) -> CalendarException {
        CalendarException {
            service_id: service.to_string(),
            date,
            exception_type: kind,
        }
    }

    const ALL_WEEK: [bool; 7] = [true, true, true, true, true, true, true];
    const WEEKDAYS: [bool; 7] = [true, true, true, true, true, false, false];

    #[test]
    fn weekday_flags_within_range() {
        // 2026-10-05 is a Monday.
        let days = compute(
            &[row(
                "weekday_only",
                WEEKDAYS,
                date(2026, 10, 1),
                date(2026, 10, 31),
            )],
            &[],
        );
        assert_eq!(days.services_on(date(2026, 10, 5)), vec!["weekday_only"]);
        assert!(days.services_on(date(2026, 10, 10)).is_empty()); // Saturday
        assert!(days.services_on(date(2026, 9, 30)).is_empty()); // before range
    }

    #[test]
    fn zet_degenerate_case_one_added_service_per_date() {
        // All weekday flags zero; every date added exactly once via exception 1.
        let zeros = [false; 7];
        let days = compute(
            &[
                row("0_30", zeros, date(2026, 9, 28), date(2030, 12, 31)),
                row("0_33", zeros, date(2026, 9, 28), date(2030, 12, 31)),
            ],
            &[
                exception("0_30", date(2026, 10, 5), 1),
                exception("0_33", date(2026, 10, 10), 1),
            ],
        );
        assert_eq!(days.services_on(date(2026, 10, 5)), vec!["0_30"]);
        assert_eq!(days.services_on(date(2026, 10, 10)), vec!["0_33"]);
        assert!(days.services_on(date(2026, 10, 6)).is_empty());
        assert_eq!(days.feed_end(), Some(date(2026, 10, 10)));
    }

    #[test]
    fn exception_2_removes_calendar_service() {
        let days = compute(
            &[row("svc", ALL_WEEK, date(2026, 10, 1), date(2026, 10, 31))],
            &[exception("svc", date(2026, 10, 7), 2)],
        );
        assert!(days.services_on(date(2026, 10, 7)).is_empty());
        assert_eq!(days.services_on(date(2026, 10, 8)), vec!["svc"]);
    }

    #[test]
    fn feed_end_is_latest_date_with_service() {
        let days = compute(
            &[row("a", WEEKDAYS, date(2026, 10, 1), date(2026, 10, 10))],
            &[exception("late", date(2026, 12, 24), 1)],
        );
        assert_eq!(days.feed_end(), Some(date(2026, 12, 24)));
    }

    #[test]
    fn empty_inputs_yield_no_services() {
        let days = compute(&[], &[]);
        assert!(days.feed_end().is_none());
        assert!(days.services_on(date(2026, 10, 5)).is_empty());
    }
}
