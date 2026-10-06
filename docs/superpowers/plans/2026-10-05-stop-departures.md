# Stop Departure Board Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the stop panel's live-only route chips with a time-sorted departure board (live + scheduled, with delay display), backed by GTFS calendar import and a new server-merged `stop-departures` endpoint, plus zoom-gated all-stops map display.

**Architecture:** Backend imports `calendar.txt`/`calendar_dates.txt` (tables already exist, currently dead), computes a date→services map in memory (the `schedule_offsets` cache pattern), and serves `GET /api/v1/schedule/stop-departures` which merges indexed schedule queries with the existing live-arrival prediction logic by `trip_key`. Frontend adds a `DeparturesBoard` (flag-gated via `stop_departures`), polls every ~20 s while a stop sheet is open, and swaps map stop data between active-only and all-stops sets on zoom-threshold crossing.

**Tech Stack:** Rust (edition 2024, axum, sqlx, jiff), React 19 + Vite + Tailwind v4, Zustand, zod, maplibre-gl. Spec: `docs/superpowers/specs/2026-10-05-stop-departures-design.md`.

## Global Constraints

- Work in the worktree `/home/allypost/Code/fun/zet-live-overview/stop-departures` (branch `stop-departures`, created from `main` in Task 1). All paths below are relative to that worktree unless absolute.
- Read `AGENTS.md` (root), `backend/AGENTS.md`, `frontend/AGENTS.md` before editing those components. Rust: 4-space indent; everything else 2-space.
- Backend formatting/lint/compile: **`just backend fmt-dev`** (never bare `just fmt`/`lint`). Tests: `just backend test`. Any new/changed sqlx query requires **`just backend sqlx-regenerate`** with the dev DB running, and the regenerated `backend/.sqlx/` files are committed.
- Frontend checks: **`just frontend check`** (typecheck + lint + format). No frontend tests exist; do not invent test commands.
- Postgres dev DB: `just backend dev-db` (docker, `zet-live-pg`, `127.0.0.1:5436`, user/pass/db `zet_live`); start with `docker start zet-live-pg` if it exists. `backend/.env` must set `DATABASE_URL=postgres://zet_live:zet_live@127.0.0.1:5436/zet_live`.
- Backend binary embeds `../frontend/dist` and `../frontend-admin/dist` at compile time: before the first backend build in the fresh worktree, run `just frontend build && just frontend-admin build` (or root `just build`).
- Both `frontend/dist` and `frontend-admin/dist` need the same env handling as the main checkout: copy `.env` files from the main worktree (`/home/allypost/Code/fun/zet-live-overview/main/backend/.env`, `frontend/.env`, `frontend-admin/.env`) if they exist.
- Default backend port: `9011` (`BIND_TO`). Frontend dev server: `5173`.
- E2E steps grep the dev-run log for `debug!`-level lines (`service_days cache reloaded`, `Schedule updated`): they only appear when `backend/.env`'s `LOG_LEVEL` enables `zet_live=debug` (or trace). If you had to create a fresh `.env` in Task 1, set `LOG_LEVEL=zet_live=debug,warn` in it.
- Repo testing policy: no TDD; unit tests only for subtle pure logic (service-day computation, window mapping, merge), inline `#[cfg(test)] mod tests` beside the code. E2E via `curl` against the real running server with captured responses as artifacts.
- Commit after every task (or logical step within a task). Never commit secrets.

---

### Task 1: Worktree setup and green baseline

**Files:**
- Modify: none (setup only)

**Interfaces:**
- Consumes: git repo at `/home/allypost/Code/fun/zet-live-overview/main` (HEAD includes spec commit `fed933f`)
- Produces: worktree `/home/allypost/Code/fun/zet-live-overview/stop-departures` on branch `stop-departures`, with all checks green before any feature work

- [ ] **Step 1: Create the worktree**

```bash
cd /home/allypost/Code/fun/zet-live-overview/main
/home/allypost/.scripts/git-worktree-create stop-departures
cd /home/allypost/Code/fun/zet-live-overview/stop-departures
```

Expected output ends with `Workdir created`. If `git fetch --all` fails (no network/remotes), the script still creates the worktree — proceed if the branch exists.

- [ ] **Step 2: Copy untracked env files from the main worktree**

```bash
for d in backend frontend frontend-admin; do
  if [ -f "/home/allypost/Code/fun/zet-live-overview/main/$d/.env" ]; then
    cp "/home/allypost/Code/fun/zet-live-overview/main/$d/.env" "$d/.env"
  fi
done
ls backend/.env frontend/.env frontend-admin/.env 2>/dev/null
```

Verify `backend/.env` contains `DATABASE_URL=postgres://...`. If it is missing, create `backend/.env` with:

```
DATABASE_URL=postgres://zet_live:zet_live@127.0.0.1:5436/zet_live
```

- [ ] **Step 3: Start the dev DB and run migrations**

```bash
docker start zet-live-pg 2>/dev/null || (cd backend && just dev-db &)
sleep 5
cd backend && just migrations-run && just migrations-list
```

Expected: all migrations applied (3 up migrations).

- [ ] **Step 4: Build frontends (needed for backend compile) and check both sides are green**

```bash
just frontend build
just frontend-admin build
just backend fmt-dev
just backend test
just frontend check
```

Expected: all pass (builds succeed, clippy clean, tests pass, frontend checks clean). If `fmt-dev` reports pre-existing clippy warnings on untouched code, capture them in the task notes and continue only if they are pre-existing on `main` too.

- [ ] **Step 5: Commit nothing (setup only).** Note in the final report which baseline warnings exist.

---

### Task 2: Import calendar.txt and calendar_dates.txt

**Files:**
- Modify: `backend/src/proto/gtfs_schedule/data/mod.rs` (inside `GtfsSchedule::read_from_zip_bytes`, after the `trips.txt` copy at ~line 110)

**Interfaces:**
- Consumes: existing `FileSpec` + `copy_csv` machinery; DB tables `gtfs_calendar` (`service_id TEXT PK, monday..sunday SMALLINT NOT NULL, start_date TEXT NOT NULL, end_date TEXT NOT NULL`) and `gtfs_calendar_dates` (`service_id TEXT, date TEXT, exception_type SMALLINT`), both currently empty.
- Produces: both tables populated on every schedule import. CSV column names map 1:1 to table columns (verified against the real ZET zip headers: `calendar.txt` → `service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date`; `calendar_dates.txt` → `service_id,date,exception_type`).

- [ ] **Step 1: Add the two FileSpec imports**

Insert between the `trips.txt` and `stop_times.txt` `copy_csv` calls (so the two small tables load before the big one):

```rust
        copy_csv(
            &mut tx,
            &zip_bytes,
            FileSpec {
                file: "calendar.txt",
                table: "gtfs_calendar",
                rebuild_indexes_around_copy: false,
                columns: &[
                    ("service_id", "service_id"),
                    ("monday", "monday"),
                    ("tuesday", "tuesday"),
                    ("wednesday", "wednesday"),
                    ("thursday", "thursday"),
                    ("friday", "friday"),
                    ("saturday", "saturday"),
                    ("sunday", "sunday"),
                    ("start_date", "start_date"),
                    ("end_date", "end_date"),
                ],
            },
        )
        .await?;

        copy_csv(
            &mut tx,
            &zip_bytes,
            FileSpec {
                file: "calendar_dates.txt",
                table: "gtfs_calendar_dates",
                rebuild_indexes_around_copy: false,
                columns: &[
                    ("service_id", "service_id"),
                    ("date", "date"),
                    ("exception_type", "exception_type"),
                ],
            },
        )
        .await?;
```

- [ ] **Step 2: Format and compile**

```bash
just backend fmt-dev
```

Expected: clean (no new warnings).

- [ ] **Step 3: E2E verify — run the server until it imports the schedule, then check the tables**

```bash
just backend dev-run &
sleep 45
docker exec zet-live-pg psql -U zet_live -d zet_live -c "SELECT count(*) FROM gtfs_calendar" -c "SELECT count(*) FROM gtfs_calendar_dates" -c "SELECT * FROM gtfs_calendar_dates LIMIT 3"
```

Expected: `gtfs_calendar` count = 7, `gtfs_calendar_dates` count ≈ 95, and the sample rows look like `0_30 | 20260928 | 1`. Save the output as the task artifact. Stop the server afterwards (`kill %1` or by PID).

- [ ] **Step 4: Commit**

```bash
git add backend/src/proto/gtfs_schedule/data/mod.rs
git commit -m "Import GTFS calendar and calendar_dates"
```

---

### Task 3: Service-day map (`service_days` module)

**Files:**
- Create: `backend/src/database/service_days.rs`
- Modify: `backend/src/database/mod.rs` (add `pub mod service_days;` next to `pub mod schedule_offsets;` at line 15, and call `service_days::init().await?;` after `schedule_offsets::init().await?;` at line 44)
- Modify: `backend/src/proto/gtfs_schedule/fetcher.rs` (~line 227, in the `Ok(())` branch after `GtfsSchedule::read_from_zip_bytes`): add `crate::database::service_days::reload().await;` right after `schedule_offsets::reload().await;`

**Interfaces:**
- Consumes: `gtfs_calendar` + `gtfs_calendar_dates` rows (Task 2); `Database::pool()`; jiff.
- Produces (used by Tasks 5–6):
  - `pub struct ServiceDays` with `pub fn services_on(&self, date: civil::Date) -> Vec<String>` and `pub fn feed_end(&self) -> Option<civil::Date>`
  - `pub async fn init() -> anyhow::Result<()>`, `pub async fn reload()`, `pub fn snapshot() -> Arc<ServiceDays>`
  - `pub const ZAGREB_TZ: &str = "Europe/Zagreb";`

- [ ] **Step 1: Write the failing tests first (pure computation)**

Create `backend/src/database/service_days.rs` with the module skeleton and tests. Full file:

```rust
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
    pub fn feed_end(&self) -> Option<Date> {
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
    row.weekdays[usize::from(weekday.to_monday_zero_offset())]
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
    CACHE.get().and_then(|rw| rw.read().ok()).map_or_else(
        || Arc::new(ServiceDays::default()),
        |guard| guard.clone(),
    )
}

fn parse_gtfs_date(s: &str) -> Option<Date> {
    Date::strptime("%Y%m%d", s).ok()
}

async fn load() -> anyhow::Result<ServiceDays> {
    let calendar_rows = sqlx::query!(
        "SELECT service_id, monday, tuesday, wednesday, thursday, friday, \
         saturday, sunday, start_date, end_date FROM gtfs_calendar"
    )
    .fetch_all(&Database::pool())
    .await?;

    let exception_rows = sqlx::query!(
        "SELECT service_id, date, exception_type FROM gtfs_calendar_dates"
    )
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
    use super::*;
    use jiff::civil::date;

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
            &[row("weekday_only", WEEKDAYS, date(2026, 10, 1), date(2026, 10, 31))],
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
```

- [ ] **Step 2: Wire the module in**

`backend/src/database/mod.rs`:

```rust
pub mod schedule_offsets;
pub mod service_days;
pub mod time;
```

and inside `Database::init`, after `schedule_offsets::init().await?;`:

```rust
        schedule_offsets::init().await?;
        service_days::init().await?;
```

`backend/src/proto/gtfs_schedule/fetcher.rs` (~line 227):

```rust
            schedule_offsets::reload().await;
            crate::database::service_days::reload().await;
```

- [ ] **Step 3: Regenerate the sqlx cache, format, and run tests**

```bash
just backend sqlx-regenerate
just backend fmt-dev
just backend test
```

Expected: tests pass including the five new `service_days::tests` cases. `git status` shows new `backend/.sqlx/` files for the two new queries.

- [ ] **Step 4: E2E verify startup loads the cache**

Note: Step 3's `sqlx-regenerate` **drops and recreates the dev database**, so the server boots with empty GTFS tables (`populated dates=0`), then the schedule fetcher re-imports (network + ~1.7M-row COPY, up to ~60 s). Poll for the post-import line instead of trusting a fixed sleep:

```bash
just backend dev-run > /tmp/zet-dev-run.log 2>&1 &
for i in $(seq 1 24); do
  grep -q "service_days cache reloaded" /tmp/zet-dev-run.log && break
  sleep 5
done
grep "service_days" /tmp/zet-dev-run.log
```

Expected: `service_days cache populated dates=0` at boot, then `service_days cache reloaded dates=95` (or close) after the import. Stop the server afterwards.

- [ ] **Step 5: Commit**

```bash
git add backend/src/database/service_days.rs backend/src/database/mod.rs \
  backend/src/proto/gtfs_schedule/fetcher.rs backend/.sqlx
git commit -m "Add service-day map from GTFS calendar"
```

---

### Task 4: Extract shared live-arrival lookup (behavior-preserving refactor)

**Files:**
- Create: `backend/src/server/routes/v1/schedule/departures.rs` (the shared `fetch_live_arrivals` fn; `mod departures;` is wired here)
- Modify: `backend/src/server/routes/v1/schedule/mod.rs` (declare `mod departures;`, replace the inline query+prediction in `get_stop_trips` with a call to the shared function)

**Interfaces:**
- Consumes: the SQL and prediction logic currently inline in `get_stop_trips` (`schedule/mod.rs` lines ~207–369); `predictions::try_infer_base_midnight`; `get_base_midnight`.
- Produces (used by Task 5 and by `get_stop_trips` itself):
  - `pub(crate) struct LiveArrival { pub vehicle_id: String, pub trip_id: String, pub route_id: String, pub predicted: Option<i64> }` (Task 5 extends it with `trip_key` + `headsign`)
  - `pub(crate) struct LiveArrivals { pub trip_ids: HashSet<String>, pub arrivals: Vec<LiveArrival> }` (`arrivals` sorted by predicted time, nulls last — same ordering the endpoint applies today)
  - `pub(crate) async fn fetch_live_arrivals(stop_ids: &[String], now: i64) -> Result<LiveArrivals, sqlx::Error>`

- [ ] **Step 1: Create `departures.rs` with the shared function**

The SQL is byte-identical to the current `get_stop_trips` query (Task 5 will add `lv.trip_key`/`lv.trip_headsign` when the merge actually needs them, so this task introduces no dead fields). Full initial file:

```rust
//! `stop-departures` endpoint: server-merged scheduled + live departures.
//!
//! Also hosts the shared live-arrival lookup extracted from `get_stop_trips`
//! so the departures merge and the legacy endpoint see identical predictions.

use std::collections::{HashMap, HashSet};

use crate::database::Database;

use super::predictions::try_infer_base_midnight;

/// One live vehicle's predicted arrival at one of the requested stops.
pub(crate) struct LiveArrival {
    pub vehicle_id: String,
    pub trip_id: String,
    pub route_id: String,
    pub predicted: Option<i64>,
}

/// Result of the live-arrival lookup for a set of stops.
pub(crate) struct LiveArrivals {
    /// Every trip id whose live vehicle serves any requested stop (used for
    /// map vehicle highlighting) — includes vehicles already past the stop.
    pub trip_ids: HashSet<String>,
    /// One entry per vehicle (the row for this stop), predicted-sorted.
    pub arrivals: Vec<LiveArrival>,
}

#[allow(clippy::too_many_lines)]
pub(crate) async fn fetch_live_arrivals(
    stop_ids: &[String],
    now: i64,
) -> Result<LiveArrivals, sqlx::Error> {
    let global_base_midnight = Database::logged(
        "get_base_midnight",
        sqlx::query_scalar!("SELECT base_midnight FROM live_feed_metadata WHERE id = 0")
            .fetch_optional(&Database::pool()),
    )
    .await
    .ok()
    .flatten()
    .unwrap_or_default();

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

    let rows = Database::logged(
        "fetch_live_arrivals",
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
            stop_ids,
        )
        .fetch_all(&Database::pool()),
    )
    .await?;

    let mut trip_base_midnight = HashMap::new();
    for row in &rows {
        if let (Some(live_time), Some(offset)) = (row.live_arrival_time, row.arrival_time_seconds)
        {
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

        arrivals.push(LiveArrival {
            vehicle_id: row.vehicle_id,
            trip_id: row.trip_id,
            route_id: row.route_id,
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
```

Wire the module in `backend/src/server/routes/v1/schedule/mod.rs` (next to `mod predictions;`):

```rust
mod departures;
mod predictions;
```

- [ ] **Step 2: Rewire `get_stop_trips` to use it**

In `schedule/mod.rs`, replace the body of `get_stop_trips` from the `let global_base_midnight = get_base_midnight().await;` line through the `arrival_times.sort_by(...)` block (i.e. everything between the empty-`stop` early return and the final `JsonOrAccept`) with:

```rust
    let now = jiff::Timestamp::now().as_second();
    let live = match departures::fetch_live_arrivals(&query.stop, now).await {
        Ok(live) => live,
        Err(e) => {
            error!(%e, "Failed to get stop trips");
            return ApiError::internal("Failed to get stop trips").into_response();
        }
    };

    let arrival_times = live
        .arrivals
        .into_iter()
        .map(|a| StopArrivalTime {
            trip_id: a.trip_id,
            vehicle_id: a.vehicle_id,
            route_id: a.route_id,
            stop_id: String::new(),
            arrival_time: a.predicted,
        })
        .collect::<Vec<_>>();

    let stop_trips = live.trip_ids.into_iter().collect::<Vec<_>>();
```

Then delete the now-unused `get_base_midnight` helper **only if** no other caller remains (check `get_trip_info` — it also calls `get_base_midnight`; keep the helper if so). The imports of `HashMap`, `HashSet`, and `predictions::{LiveStopTime, ...}` at the top of `schedule/mod.rs` may become partially unused — remove exactly the ones clippy/rustc reports.

**Behavioral change to know and accept:** `StopArrivalTime.stop_id` becomes `""` for the stop-trips response. Today the frontend `stopArrivalTimeSchema` parses `stopId` but no consumer reads it (the worker/board keys rows by vehicle). Confirm with `grep -rn "stopId" frontend/src | grep -i arrival` — expected: only the zod schema itself. If any consumer is found, instead extend `LiveArrival` with `stop_id` populated from the matched row and keep passing it through.

- [ ] **Step 3: Format, cache, tests**

```bash
just backend sqlx-regenerate
just backend fmt-dev
just backend test
```

Expected: clean. The `fetch_live_arrivals` query text is unchanged from the old inline one (it merely moved), so the `.sqlx` diff is one new entry for it.

- [ ] **Step 4: E2E verify `stop-trips` still responds identically in shape**

The Step 3 `sqlx-regenerate` wiped the dev DB — wait for the schedule import plus ~30 s of realtime fetcher before querying live data:

```bash
just backend dev-run > /tmp/zet-dev-run.log 2>&1 &
for i in $(seq 1 24); do
  grep -q "Schedule updated" /tmp/zet-dev-run.log && break
  sleep 5
done
sleep 30
STOP=$(docker exec zet-live-pg psql -U zet_live -d zet_live -tAc \
  "SELECT gst.stop_id FROM live_trips lt JOIN gtfs_stop_times gst ON gst.trip_key = lt.trip_key LIMIT 1")
curl -s "http://127.0.0.1:9011/api/v1/schedule/stop-trips?stop=$STOP" | head -c 600
```

Expected: JSON with `stopTrips` (non-empty during service hours) and `arrivalTimes` entries with `tripId`/`vehicleId`/`routeId`/`arrivalTime`. Save as artifact. Stop the server.

- [ ] **Step 5: Commit**

```bash
git add backend/src/server/routes/v1/schedule/mod.rs \
  backend/src/server/routes/v1/schedule/departures.rs backend/.sqlx
git commit -m "Extract live-arrival lookup shared with stop-departures"
```

---

### Task 5: Departures computation and endpoint

**Files:**
- Modify: `backend/src/server/routes/v1/schedule/departures.rs` (extend `LiveArrival` + its SQL, add windows, scheduled query, merge, handler)
- Modify: `backend/src/server/routes/v1/schedule/mod.rs` (`pub use departures::get_stop_departures;`)
- Modify: `backend/src/server/routes/v1/mod.rs` (route registration, next to the `stop-trips` route at line 93)
- Modify: `backend/src/feature_flags.rs` (registry entry)

**Interfaces:**
- Consumes: `service_days::{snapshot, ServiceDays, ZAGREB_TZ}` (Task 3), `fetch_live_arrivals` (Task 4), `JsonOrAccept`/`Versioned`/`ApiError`.
- Produces:
  - `GET /api/v1/schedule/stop-departures?stop=<id>&stop=<id>&limit=<n>` → `{v:1, d:{departures:[[routeId, headsign, scheduledTime, tripId, vehicleId, predictedTime], ...], scheduleEnd: <unix secs|null>}}`; live rows have non-empty `vehicleId` and non-null `predictedTime`.
  - Feature flag key `stop_departures` (default `disabled`, admin-CRUD controlled; endpoint itself is NOT flag-gated).

- [ ] **Step 1: Add window mapping + scheduled query + merge + handler with unit tests**

First extend the shared lookup from Task 4 — `LiveArrival` gains two fields, and `fetch_live_arrivals`'s SQL gains two columns (alias: `lv.trip_key` is nullable in the schema but never NULL through the `live_trips`-fed join, so force not-null; repo precedent in `v1/mod.rs` `AS "trip_key!"`):

```rust
pub(crate) struct LiveArrival {
    pub vehicle_id: String,
    pub trip_id: String,
    pub trip_key: String,
    pub route_id: String,
    pub headsign: String,
    pub predicted: Option<i64>,
}
```

In the `StopTripRow` struct add `trip_key: String,` and `trip_headsign: Option<String>,`; in the SQL select list add after `lv.trip_id`:

```sql
            , lv.trip_key AS "trip_key!"
            , lv.trip_headsign
```

(the `query_as!` string becomes `r#"..."#` to allow the quoted alias); and in the `arrivals.push(LiveArrival { ... })` add `trip_key: row.trip_key,` and `headsign: row.trip_headsign.unwrap_or_default(),`.

Then append the new code to `departures.rs`:

```rust
use jiff::{Timestamp, civil::Date, tz::TimeZone};
use serde::Serialize;
use sqlx::PgPool;
use tracing::error;

use crate::{
    database::service_days,
    entity::util::versioned::Versioned,
    server::{error::ApiError, request::JsonOrAccept},
};

/// Departure lookback/lookahead: a 2-minute grace for just-departed rows,
/// 6 hours ahead. Constant per the spec (§4.2).
const LOOKBACK_SECS: i64 = 120;
const LOOKAHEAD_SECS: i64 = 6 * 3600;
const DEFAULT_LIMIT: usize = 20;
const MAX_LIMIT: usize = 50;

/// The GTFS-seconds window for one service day that can contribute departures.
pub(crate) struct ServiceDayWindow {
    /// Unix seconds of this service day's local midnight.
    pub midnight: i64,
    pub services: Vec<String>,
    /// Bounds bind against the INTEGER `departure_time_seconds` column.
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

/// Service days that can contribute departures in `[now-lookback, now+lookahead]`:
/// yesterday's (for after-midnight GTFS times ≥ 24:00) and today's.
pub(crate) fn candidate_windows(now: Timestamp, days: &service_days::ServiceDays) -> Vec<ServiceDayWindow> {
    let tz = TimeZone::get(service_days::ZAGREB_TZ).expect("static time zone");
    let now_z = now.to_zoned(tz);
    let today = now_z.date();
    let now_secs = now.as_second();

    let mut out = Vec::new();
    for date in [today.yesterday().expect("today after Date::MIN"), today] {
        let services = days.services_on(date);
        if services.is_empty() {
            continue;
        }
        let midnight = midnight_zagreb(date);
        let lo = now_secs - LOOKBACK_SECS - midnight;
        let hi = now_secs + LOOKAHEAD_SECS - midnight;
        out.push(ServiceDayWindow {
            midnight,
            services,
            lo_secs: i32::try_from(lo).expect("window bound fits in i32"),
            hi_secs: i32::try_from(hi).expect("window bound fits in i32"),
        });
    }
    out
}

#[derive(Debug)]
pub(crate) struct ScheduledDeparture {
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
                  COALESCE(r.route_short_name, r.route_id) AS route
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
                departure: window.midnight + i64::from(secs),
                trip_id: row.trip_id,
                trip_key: row.trip_key,
            })
        })
        .collect())
}

/// A response row: positional `[routeId, headsign, scheduledTime, tripId,
/// vehicleId, predictedTime]` — live rows have non-empty vehicleId and
/// non-null predictedTime, scheduled rows `""`/`null`.
pub(crate) type DepartureRow = (String, String, i64, String, String, Option<i64>);

/// Merge scheduled departures with live arrivals by `trip_key`. `limit` caps
/// scheduled rows only; live rows are never dropped. A live row without a
/// predicted time and without a schedule match is dropped (nothing to sort by).
pub(crate) fn merge_departures(
    mut scheduled: Vec<ScheduledDeparture>,
    live: Vec<LiveArrival>,
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
                (s.route.clone(), s.headsign.clone(), s.departure, s.trip_id.clone(), String::new(), None),
            ));
        }
    }

    for l in &live {
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
    let mut scheduled = Vec::new();
    for window in &windows {
        match fetch_scheduled_departures(&pool, &query.stop, window).await {
            Ok(rows) => scheduled.extend(rows),
            Err(e) => {
                error!(%e, "Failed to get scheduled departures");
                return ApiError::internal("Failed to get stop departures").into_response();
            }
        }
    }

    let live = match fetch_live_arrivals(&query.stop, now.as_second()).await {
        Ok(live) => live,
        Err(e) => {
            error!(%e, "Failed to get live arrivals for departures");
            return ApiError::internal("Failed to get stop departures").into_response();
        }
    };

    let schedule_end = days
        .feed_end()
        .map(|end| midnight_zagreb(end.tomorrow().expect("feed_end below Date::MAX")));

    JsonOrAccept(
        Versioned::new(
            1,
            StopDepartures {
                departures: merge_departures(scheduled, live.arrivals, limit),
                schedule_end,
            },
        ),
        headers,
    )
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

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
            headsign: "Live Headsign".to_string(),
            predicted: Some(predicted),
        }
    }

    #[test]
    fn merge_live_replaces_scheduled_for_matching_trip_key() {
        let out = merge_departures(
            vec![scheduled(1000, "a", "6"), scheduled(2000, "b", "6")],
            vec![live("a", 1300, "v1")],
            20,
        );
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].0, "6");
        assert_eq!((out[0].3.clone(), out[0].4.clone()), ("0_40_a".to_string(), "v1".to_string()));
        assert_eq!(out[0].5, Some(1300)); // predicted, not scheduled 1000
        assert_eq!(out[1].4, ""); // scheduled row: no vehicle
        assert_eq!(out[1].5, None);
    }

    #[test]
    fn merge_live_only_row_inserted_in_time_order() {
        let out = merge_departures(
            vec![scheduled(2000, "b", "6")],
            vec![live("x", 1500, "v9")],
            20,
        );
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].2, 1500); // live-only row sorts before the 2000 scheduled
        assert_eq!(out[0].1, "Live Headsign");
        assert_eq!(out[1].2, 2000);
    }

    #[test]
    fn merge_limit_caps_scheduled_but_not_live() {
        let sched = (0..30).map(|i| scheduled(1000 + i, &format!("s{i}"), "6")).collect();
        let live_rows = (0..5).map(|i| live(&format!("s{i}"), 900 + i, &format!("v{i}"))).collect();
        let out = merge_departures(sched, live_rows, 10);
        // 5 scheduled rows became live (not counted against the limit) + 10 capped scheduled.
        assert_eq!(out.len(), 15);
    }

    #[test]
    fn merge_drops_live_without_prediction_or_schedule() {
        let mut l = live("x", 0, "v1");
        l.predicted = None;
        let out = merge_departures(vec![], vec![l], 20);
        assert!(out.is_empty());
    }

    #[test]
    fn merge_dedups_multi_platform_stop_groups() {
        // Same trip serving two stops of the selected group: one row only.
        let out = merge_departures(
            vec![scheduled(1000, "a", "6"), scheduled(1010, "a", "6")],
            vec![],
            20,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].2, 1000); // earliest kept
    }

    #[test]
    fn windows_cover_yesterday_for_after_midnight_trips() {
        // 2026-10-26 00:30 Europe/Zagreb (CET, offset +01:00) — the day after
        // DST ended (2026-10-25 03:00 CEST→CET), so yesterday's midnight offset
        // differs from a naive 24h subtraction by 3600 s.
        let now = "2026-10-25T23:30:00Z".parse::<Timestamp>().expect("valid ts");
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
        assert!(w.lo_secs <= 91_800 && 91_800 <= w.hi_secs, "{}..{}", w.lo_secs, w.hi_secs);
    }

    #[test]
    fn windows_skip_days_without_service() {
        // Sunday 2026-10-04 12:00 Zagreb (CEST).
        let now = "2026-10-04T10:00:00Z".parse::<Timestamp>().expect("valid ts");
        let days = service_days::compute(&[], &[]);
        assert!(candidate_windows(now, &days).is_empty());
    }
}
```

Cleanup: merge the two `use` blocks and re-run `fmt-dev` (rustfmt will reflow). jiff 0.2.28 facts used above (all verified): `Date::tomorrow()`/`yesterday()` return `Result<Date, _>`; `Date::to_zoned(tz) -> Result<Zoned, _>` is midnight-anchored (no separate `to_datetime()` step exists); `Zoned::timestamp() -> Timestamp`; `Timestamp` parses via `str::parse` (`"…Z".parse::<Timestamp>()` — no `FromStr` import needed with turbofish). `st.departure_time_seconds` is INTEGER in PG and `ServiceDayWindow` stores i32 bounds (converted with `i32::try_from(…).expect(…)` to satisfy pedantic `cast_possible_truncation` — the repo forbids new `#[allow]`s).

In `schedule/mod.rs` add the re-export next to `pub use predictions::compute_base_midnight;`:

```rust
pub use departures::get_stop_departures;
```

- [ ] **Step 2: Register the route and the feature flag**

`backend/src/server/routes/v1/mod.rs` (next to the stop-trips route, line ~93):

```rust
        .route("/schedule/stop-trips", get(schedule::get_stop_trips))
        .route(
            "/schedule/stop-departures",
            get(schedule::get_stop_departures),
        )
```

`backend/src/feature_flags.rs` — replace the placeholder comment block:

```rust
feature_flags! {
    // Gates the stop departure board (scheduled departures + zoom-gated
    // all-stops map) in the frontend; endpoint stays on regardless.
    stop_departures => "stop_departures",
}
```

- [ ] **Step 3: Regenerate sqlx cache, format, test**

```bash
just backend sqlx-regenerate
just backend fmt-dev
just backend test
```

Expected: all tests pass (new merge/window tests included). New `.sqlx` entry for `fetch_scheduled_departures`.

- [ ] **Step 4: E2E verify the endpoint**

Note: Step 3's `sqlx-regenerate` wiped the dev DB, so let the server re-import first (watch for `service_days cache reloaded` as the import signal, then give the realtime fetcher ~30 s to populate live tables).

```bash
just backend dev-run > /tmp/zet-dev-run.log 2>&1 &
for i in $(seq 1 24); do
  grep -q "service_days cache reloaded" /tmp/zet-dev-run.log && break
  sleep 5
done
sleep 30
# A busy daytime stop (main square)
SQUARE=$(docker exec zet-live-pg psql -U zet_live -d zet_live -tAc \
  "SELECT stop_id FROM gtfs_stops WHERE stop_name ILIKE '%Jela%' LIMIT 1")
curl -s "http://127.0.0.1:9011/api/v1/schedule/stop-departures?stop=$SQUARE" | python3 -m json.tool | head -50
curl -s "http://127.0.0.1:9011/api/v1/schedule/stop-departures?stop=$SQUARE&limit=3" | head -c 400
```

Expected: `departures` array with ~20 rows sorted ascending by time; daytime rows mostly scheduled (empty `vehicleId`) with any live vehicles merged in as `[...,"vNNN",secs]`; `scheduleEnd` non-null (~2027-01-01). Save both curls as artifacts. Stop the server.

- [ ] **Step 5: Commit**

```bash
git add backend/src/server/routes/v1/schedule/departures.rs \
  backend/src/server/routes/v1/schedule/mod.rs \
  backend/src/server/routes/v1/mod.rs backend/src/feature_flags.rs backend/.sqlx
git commit -m "Add stop-departures endpoint"
```

---

### Task 6: Frontend — schema, store, fetchers, flag plumbing

**Files:**
- Modify: `frontend/src/app/entity/v1/api.ts` (new schemas at the end)
- Modify: `frontend/src/store.ts` (`StopSelection` fields + `selectStop` initial state)
- Modify: `frontend/src/hooks/use-stops.ts` (`fetchStopDepartures` + `refreshStopDepartures`, flag-aware refresh in `processMessage`)
- Modify: `frontend/src/hooks/use-selection-fetcher.ts` (flag-aware selection fetch)
- Modify: `frontend/src/feature-flags-store.ts` (imperative helper)

**Interfaces:**
- Consumes: endpoint from Task 5; `apiFetch`, existing stale-guard pattern; `featureFlagsStore`.
- Produces (used by Tasks 7–8):
  - `export type Departure = { kind: "live"; routeId: string; headsign: string; scheduledTime: Date; tripId: string; vehicleId: string; predictedTime: Date } | { kind: "scheduled"; routeId: string; headsign: string; scheduledTime: Date; tripId: string }`
  - `export const stopDeparturesResponseSchema` (zod, infers `{ d: { departures: Departure[]; scheduleEnd: number | null } }`)
  - `StopSelection.departures: Departure[] | null`, `StopSelection.scheduleEnd: Date | null`
  - `export async function fetchStopDepartures(stopIds: string[]): Promise<void>`
  - `export function stopDeparturesEnabled(): boolean`

- [ ] **Step 1: Add the schema** — append to `api.ts`:

```ts
const departureTupleSchema = z.tuple([
  z.string(),
  z.string(),
  z.number(),
  z.string(),
  z.string(),
  z.number().nullable(),
]);

export type Departure =
  | {
      kind: "live";
      routeId: string;
      headsign: string;
      scheduledTime: Date;
      tripId: string;
      vehicleId: string;
      predictedTime: Date;
    }
  | {
      kind: "scheduled";
      routeId: string;
      headsign: string;
      scheduledTime: Date;
      tripId: string;
    };

export const departureSchema = departureTupleSchema.transform((row): Departure => {
  const [routeId, headsign, scheduledTime, tripId, vehicleId, predictedTime] = row;
  if (vehicleId !== "" && predictedTime !== null) {
    return {
      kind: "live",
      routeId,
      headsign,
      scheduledTime: new Date(scheduledTime * 1000),
      tripId,
      vehicleId,
      predictedTime: new Date(predictedTime * 1000),
    };
  }
  return { kind: "scheduled", routeId, headsign, scheduledTime: new Date(scheduledTime * 1000), tripId };
});

export const stopDeparturesResponseSchema = z.object({
  d: z.object({
    departures: z.array(departureSchema),
    scheduleEnd: z.number().nullable(),
  }),
});
```

- [ ] **Step 2: Extend the store** — in `store.ts`:

Import `type Departure` (add to the existing `api` import), extend `StopSelection`:

```ts
export type StopSelection = {
  name: string;
  routes: string[];
  tripIds: Set<string> | null;
  arrivalTimes: StopArrivalTime[] | null;
  departures: Departure[] | null;
  scheduleEnd: Date | null;
  fetchError: string | null;
};
```

and in `selectStop`, the initial object gains `departures: null, scheduleEnd: null`.

- [ ] **Step 3: Add the imperative flag helper** — append to `feature-flags-store.ts`:

```ts
const STOP_DEPARTURES_FLAG = "stop_departures";

/** Non-hook check for fetch paths (workers, store subscriptions). */
export function stopDeparturesEnabled(): boolean {
  return featureFlagsStore.getState().flags[STOP_DEPARTURES_FLAG] === true;
}
```

- [ ] **Step 4: Add fetchers and flag-aware refresh** — in `use-stops.ts`:

Add imports to the existing import block at the top of the file: `stopDeparturesResponseSchema` (in the existing `@/app/entity/v1/api` import), `type Departure` (also from `@/app/entity/v1/api`), and `stopDeparturesEnabled` from `@/feature-flags-store`. Add after `refreshStopArrivalTimes`:

```ts
let stopDeparturesAbort: AbortController | null = null;
let stopDeparturesRefreshAbort: AbortController | null = null;

export async function fetchStopDepartures(stopIds: string[], silent = false) {
  stopDeparturesAbort?.abort();
  stopDeparturesRefreshAbort?.abort();
  stopTripsAbort?.abort();
  stopTripsRefreshAbort?.abort();
  stopDeparturesAbort = new AbortController();
  const { signal } = stopDeparturesAbort;

  const isStale = () => {
    const sel = useStore.getState().selection;
    return sel?.type !== "stop" || !sameStopIds(sel.ids, stopIds);
  };

  const queryParams = new URLSearchParams();
  for (const stopId of stopIds) {
    queryParams.append("stop", stopId);
  }

  const result = await apiFetch(
    `${API_URL}/v1/schedule/stop-departures?${queryParams.toString()}`,
    stopDeparturesResponseSchema,
    { signal },
  );

  if (signal.aborted || isStale()) return;

  if (result.error) {
    patchStopSelection({ fetchError: result.error.error });
    if (!silent && result.error.status !== 404) {
      toast.error("Failed to load departures", { description: result.error.error });
    }
    return;
  }

  const liveTripIds = new Set(
    result.data.d.departures
      .filter((d): d is Extract<Departure, { kind: "live" }> => d.kind === "live")
      .map((d) => d.tripId),
  );

  patchStopSelection({
    departures: result.data.d.departures,
    scheduleEnd: result.data.d.scheduleEnd === null ? null : new Date(result.data.d.scheduleEnd * 1000),
    tripIds: liveTripIds,
    fetchError: null,
  });
}

async function refreshStopDepartures(stopIds: string[]) {
  stopDeparturesRefreshAbort?.abort();
  stopDeparturesRefreshAbort = new AbortController();
  const { signal } = stopDeparturesRefreshAbort;

  const isStale = () => {
    const sel = useStore.getState().selection;
    return sel?.type !== "stop" || !sameStopIds(sel.ids, stopIds);
  };

  const queryParams = new URLSearchParams();
  for (const stopId of stopIds) {
    queryParams.append("stop", stopId);
  }

  const result = await apiFetch(
    `${API_URL}/v1/schedule/stop-departures?${queryParams.toString()}`,
    stopDeparturesResponseSchema,
    { signal },
  );

  if (signal.aborted || isStale()) return;
  if (result.error) return; // stale board beats toast spam during background refreshes

  const liveTripIds = new Set(
    result.data.d.departures
      .filter((d): d is Extract<Departure, { kind: "live" }> => d.kind === "live")
      .map((d) => d.tripId),
  );

  patchStopSelection({
    departures: result.data.d.departures,
    tripIds: liveTripIds,
    fetchError: null,
  });
}
```

Give the departures path its own throttle timestamp so the vehicle-selection refresh cadence is untouched: keep `STOP_TIMES_REFRESH_INTERVAL = 15_000` as-is, and add next to it:

```ts
let lastStopDeparturesRefresh = 0;
const STOP_DEPARTURES_REFRESH_INTERVAL = 20_000;
```

Restructure the throttle block inside `processMessage` so the departures gate sits **beside** the stop-times gate, not nested inside it (nesting would make the effective departures cadence ~30 s, because the outer 15 s gate and inner 20 s gate only align at 30 s multiples). Replace the whole `const now = Date.now(); if (now - lastStopTimesRefresh >= STOP_TIMES_REFRESH_INTERVAL) { ... }` block with:

```ts
    const now = Date.now();
    const sel = useStore.getState().selection;

    if (sel?.type === "stop" && stopDeparturesEnabled()) {
      if (now - lastStopDeparturesRefresh >= STOP_DEPARTURES_REFRESH_INTERVAL) {
        lastStopDeparturesRefresh = now;
        void refreshStopDepartures(sel.ids);
      }
    } else if (now - lastStopTimesRefresh >= STOP_TIMES_REFRESH_INTERVAL) {
      lastStopTimesRefresh = now;

      if (sel?.type === "vehicle" && sel.tripId) {
        void refreshTripStopTimes(sel.tripId);
      } else if (sel?.type === "stop") {
        void refreshStopArrivalTimes(sel.ids);
      }
    }
```

- [ ] **Step 5: Flag-aware selection fetch (reactive to flag arrival)** — `use-selection-fetcher.ts`

Replace the `useStore.subscribe` implementation with a reactive effect that re-runs when either the selection **or** the flag changes. (Capabilities can arrive after a URL-restored selection already fired `selectStop`; with the subscribe-only version a flag-enabled deep link would fetch the legacy endpoint and never re-fetch.)

```ts
import { useEffect } from "react";
import { useStore } from "@/store";
import { fetchFollowingRoute, fetchStopDepartures, fetchStopTrips } from "@/hooks/use-stops";
import { useFeatureFlag } from "@/feature-flags-store";

/**
 * Reactively fetches the data needed for the current selection.
 * Re-runs when the departure-board flag changes so a deep-linked selection
 * re-fetches once flags arrive.
 */
export function useSelectionFetcher() {
  const selection = useStore((s) => s.selection);
  const stopBoard = useFeatureFlag("stop_departures");

  useEffect(() => {
    if (!selection) return;
    switch (selection.type) {
      case "vehicle":
        if (selection.tripId) void fetchFollowingRoute(selection.tripId);
        break;
      case "stop":
        if (stopBoard) void fetchStopDepartures(selection.ids);
        else void fetchStopTrips(selection.ids);
        break;
      case "gbfs-station":
        break;
    }
  }, [selection, stopBoard]);
}
```

(The fetchers abort stale requests, so the flag-flip re-run is safe. `fetchFollowingRoute`/`fetchStopTrips`/`fetchStopDepartures` are idempotent re-fetches.)

- [ ] **Step 6: Check and commit**

```bash
just frontend check
```

Expected: clean. (`fetchStopTrips` still used by the flag-off path — no unused-export warnings.)

```bash
git add frontend/src/app/entity/v1/api.ts frontend/src/store.ts \
  frontend/src/hooks/use-stops.ts frontend/src/hooks/use-selection-fetcher.ts \
  frontend/src/feature-flags-store.ts
git commit -m "Add stop departures fetch and flag plumbing"
```

---

### Task 7: Frontend — LineBadge, DeparturesBoard, app wiring

**Files:**
- Create: `frontend/src/components/line-badge.tsx`
- Create: `frontend/src/components/departures-board.tsx`
- Modify: `frontend/src/app.tsx` (sheet body + minimized body + vehicle title badge)
- Modify: `frontend/src/utils/time.ts` (add `formatClockTime`)
- Modify: `frontend/src/app.css` (add `--warn` token)
- NOT modified: `frontend/src/components/stop-sheet.tsx` (the flag-off path keeps rendering it) and `frontend/src/components/search-bar.tsx` (its badge is the `-container` visual variant, which intentionally stays inline — do not force-unify different variants)

**Interfaces:**
- Consumes: `Departure` type + `stopSelection` fields (Task 6), `formatMinutesFromNow`.
- Produces:
  - `export function LineBadge({ routeId, className }: { routeId: string; className?: string })`
  - `export function DeparturesBoard({ departures, scheduleEnd, fetchError, onVehicleClick }: { departures: Departure[] | null; scheduleEnd: Date | null; fetchError: string | null; onVehicleClick: (vehicleId: string, tripId: string) => void })`
  - `export function departureSummary(d: Departure, now: number): string` (used by the minimized sheet body)

- [ ] **Step 1: `LineBadge` component** — `frontend/src/components/line-badge.tsx`:

```tsx
type Props = {
  routeId: string;
  className?: string;
};

export function LineBadge({ routeId, className }: Props) {
  const isBus = routeId.length > 2;
  return (
    <span
      className={`text-on-primary inline-flex shrink-0 items-center rounded px-1.5 py-0.5 text-xs font-bold ${isBus ? "bg-primary" : "bg-danger"} ${className ?? ""}`}
    >
      {routeId}
    </span>
  );
}
```

- [ ] **Step 2: Clock-time helper** — append to `frontend/src/utils/time.ts`:

```ts
export function formatClockTime(date: Date): string {
  return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}
```

- [ ] **Step 3: `DeparturesBoard` component** — `frontend/src/components/departures-board.tsx`:

```tsx
import { useEffect, useState } from "react";
import type { Departure } from "@/app/entity/v1/api";
import { LineBadge } from "@/components/line-badge";
import { formatClockTime, formatMinutesFromNow } from "@/utils/time";

type Props = {
  departures: Departure[] | null;
  scheduleEnd: Date | null;
  fetchError: string | null;
  onVehicleClick: (vehicleId: string, tripId: string) => void;
};

/** Countdown/delay threshold: below this many seconds of |delay| a live row
 * counts as on time and shows no strike-through. */
const DELAY_THRESHOLD_MS = 60_000;
/** Scheduled rows further away than this show a clock time instead of a countdown. */
const COUNTDOWN_HORIZON_MS = 60 * 60_000;

function useNow(): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 15_000);
    const onVisible = () => {
      if (document.visibilityState === "visible") setNow(Date.now());
    };
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      clearInterval(id);
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, []);
  return now;
}

/** Short label for the minimized sheet body: "6 · Črnomerec · 3 min". */
export function departureSummary(d: Departure, now: number): string {
  const time = d.kind === "live" ? d.predictedTime : d.scheduledTime;
  const minutes = Math.round((time.getTime() - now) / 60_000);
  const eta = minutes <= 0 ? "now" : minutes === 1 ? "1 min" : `${minutes} min`;
  return `${d.routeId} · ${d.headsign} · ${eta}`;
}

function LiveDelay({ scheduledTime, predictedTime }: { scheduledTime: Date; predictedTime: Date }) {
  const delayMs = predictedTime.getTime() - scheduledTime.getTime();
  if (Math.abs(delayMs) < DELAY_THRESHOLD_MS) return null;
  const late = delayMs > 0;
  return (
    <span className={`text-xs tabular-nums ${late ? "text-warn" : "text-primary"}`}>
      <s className="text-on-surface-faint">{formatClockTime(scheduledTime)}</s>
      {" "}
      {formatClockTime(predictedTime)}
    </span>
  );
}

function Row({ d, now, onVehicleClick }: { d: Departure; now: number; onVehicleClick: Props["onVehicleClick"] }) {
  const isLive = d.kind === "live";
  const time = isLive ? d.predictedTime : d.scheduledTime;
  const showCountdown = time.getTime() - now < COUNTDOWN_HORIZON_MS;

  if (isLive) {
    const delayMs = d.predictedTime.getTime() - d.scheduledTime.getTime();
    const delayed = Math.abs(delayMs) >= DELAY_THRESHOLD_MS;
    const etaClass = delayed ? (delayMs > 0 ? "text-warn" : "text-primary") : "text-success";
    return (
      <button
        type="button"
        className="bg-surface-dim active:bg-surface-hover flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left"
        onClick={() => onVehicleClick(d.vehicleId, d.tripId)}
      >
        <LineBadge routeId={d.routeId} />
        <span className="bg-success h-1.5 w-1.5 shrink-0 animate-pulse rounded-full" />
        <span className="text-on-surface min-w-0 flex-1 truncate text-sm">{d.headsign}</span>
        {delayed ? <LiveDelay scheduledTime={d.scheduledTime} predictedTime={d.predictedTime} /> : null}
        <span className={`text-sm font-semibold tabular-nums ${etaClass}`}>
          {formatMinutesFromNow(time)}
        </span>
      </button>
    );
  }

  return (
    <div className="flex items-center gap-2 px-2 py-1.5">
      <LineBadge routeId={d.routeId} />
      <span className="text-on-surface min-w-0 flex-1 truncate text-sm">{d.headsign}</span>
      <span className="text-on-surface-muted text-sm font-medium tabular-nums">
        {showCountdown ? formatMinutesFromNow(time) : formatClockTime(time)}
      </span>
    </div>
  );
}

export function DeparturesBoard({ departures, scheduleEnd, fetchError, onVehicleClick }: Props) {
  const now = useNow();

  if (departures === null && fetchError === null) {
    return (
      <div className="space-y-2 px-4 pb-3">
        {[0, 1, 2].map((i) => (
          <div key={i} className="bg-surface-dim h-8 animate-pulse rounded-lg" />
        ))}
      </div>
    );
  }

  if (fetchError !== null && (departures === null || departures.length === 0)) {
    return (
      <div className="px-4 pb-3">
        <span className="text-on-surface-faint text-xs italic">{fetchError}</span>
      </div>
    );
  }

  if (departures !== null && departures.length === 0) {
    const scheduleGone = scheduleEnd !== null && now > scheduleEnd.getTime();
    return (
      <div className="px-4 pb-3">
        <span className="text-on-surface-faint text-xs italic">
          {scheduleGone ? "Schedule unavailable" : "No more departures today"}
        </span>
      </div>
    );
  }

  return (
    <div className="space-y-0.5 px-2 pb-3">
      {departures?.map((d) => (
        <Row
          key={`${d.kind === "live" ? "v" : "t"}-${d.tripId}-${d.scheduledTime.getTime()}`}
          d={d}
          now={now}
          onVehicleClick={onVehicleClick}
        />
      ))}
    </div>
  );
}
```

(The composite key is defensive: the backend dedupes per trip per response (Task 5), but a live and its predecessor scheduled row share a `tripId` across refreshes.)

**Color tokens:** the code above uses one new token, `text-warn` (amber, for late), and existing `text-primary` (early), `text-success` (live on time / dot), `bg-success` (pulse dot). Add `--warn` to both `:root` and `.dark` blocks in `frontend/src/app.css` next to `--success`:

`--warn: #d97706;` in `:root` and `--warn: #fbbf24;` in `.dark`, plus `--color-warn: var(--warn);` in the `@theme` block (mirroring the `--color-success` line). Verify `text-success`/`bg-success` already exist in `@theme` (grep `gbfs-station-sheet.tsx` for `bg-success-container` as evidence of the token family); if `text-success` is missing, add `--color-success` mapping the same way.

- [ ] **Step 4: Wire `app.tsx`**

1. Imports: **keep** `import { StopSheet } from "@/components/stop-sheet";` (flag-off path still renders it); add `import { DeparturesBoard, departureSummary } from "@/components/departures-board";`, `import { LineBadge } from "@/components/line-badge";`, and `import { useFeatureFlag } from "@/feature-flags-store";`.
2. In `App()`: `const stopBoardFlag = useFeatureFlag("stop_departures");` and read `const stopDepartures = stopSelection?.departures ?? null; const stopScheduleEnd = stopSelection?.scheduleEnd ?? null; const stopFetchError = stopSelection?.fetchError ?? null;`.
3. Vehicle sheet title: replace the inline badge span **and delete the now-unused `const isBus = selectedVehicle.routeId.length > 2;` line** with `<LineBadge routeId={selectedVehicle.routeId} />` (keep the wrapper div and long-name span).
4. Stop minimized body (replace the `firstArrival` block): when flag on, use the first departure:

```tsx
    if (stopBoardFlag && stopDepartures !== null && stopDepartures.length > 0) {
      minimizedBody = (
        <span className="text-on-surface-muted text-xs">
          {departureSummary(stopDepartures[0]!, Date.now())}
        </span>
      );
    } else if (!stopBoardFlag && stopArrivalTimes !== null) {
      const firstArrival = stopArrivalTimes.find((a) => a.arrivalTime !== null);
      if (firstArrival) {
        const secondsUntil = (firstArrival.arrivalTime!.getTime() - Date.now()) / 1000;
        const minutes = Math.round(secondsUntil / 60);
        const label = minutes <= 0 ? "now" : minutes === 1 ? "1 min" : `${minutes} min`;
        minimizedBody = (
          <span className="text-on-surface-muted text-xs">
            Route {firstArrival.routeId} in {label}
          </span>
        );
      }
    }
```

5. Sheet body — replace the `<StopSheet .../>` branch:

```tsx
          ) : selectedStop ? (
            stopBoardFlag ? (
              <DeparturesBoard
                departures={stopDepartures}
                scheduleEnd={stopScheduleEnd}
                fetchError={stopFetchError}
                onVehicleClick={(vehicleId, tripId) => {
                  selectVehicle(vehicleId, tripId, true);
                }}
              />
            ) : (
              <StopSheet
                stop={selectedStop}
                arrivals={stopArrivalTimes}
                onArrivalClick={(vehicleId, tripId) => {
                  selectVehicle(vehicleId, tripId, true);
                }}
              />
            )
          ) : selectedGbfsStation ? (
```

6. `search-bar.tsx`: verified during review — its badge is the `-container` visual variant and does not match `LineBadge`'s classes; leave it untouched.

- [ ] **Step 5: Keep `stop-sheet.tsx`, run checks**

The flag-off path still renders `StopSheet`, so `stop-sheet.tsx` stays untouched (deletion happens only when the flag is retired, per spec §6.3).

```bash
just frontend check
```

Expected: clean.

- [ ] **Step 6: Browser smoke test (flag on)**

```bash
docker exec zet-live-pg psql -U zet_live -d zet_live -c \
  "UPDATE feature_flags SET state='enabled' WHERE key='stop_departures'"
just backend dev-run &
(cd frontend && bun dev &) ; sleep 8
```

Open `http://localhost:5173`, click any stop dot: verify the departure board renders (badges, headsigns, live rows with countdown, scheduled with clock/grey countdown), tap a live row → vehicle sheet opens. Reset the flag afterwards:

```bash
docker exec zet-live-pg psql -U zet_live -d zet_live -c \
  "UPDATE feature_flags SET state='disabled' WHERE key='stop_departures'"
```

Verify flag-off shows the old `StopSheet` again after reload.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/components/line-badge.tsx frontend/src/components/departures-board.tsx \
  frontend/src/app.tsx frontend/src/utils/time.ts \
  frontend/src/app.css
git commit -m "Add departure board UI"
```

---

### Task 8: Frontend — zoom-gated all-stops map

**Files:**
- Modify: `frontend/src/scripts/worker.ts` (compute both stop sets, send `groupedAll`)
- Modify: `frontend/src/app/entity/shared.ts` (`StopsUpdateResponse.groupedAll`)
- Modify: `frontend/src/hooks/use-stops.ts` (`handleStopsUpdate` stores the all set)
- Modify: `frontend/src/store.ts` (`stopsGroupedAll` state)
- Modify: `frontend/src/components/map-container.tsx` (zoom threshold, base-stops selection, `active` property)
- Modify: `frontend/src/data/maps/style/flat.json`, `3d.json`, `3d.dark.json`, `satellite.json` (dim rendering for inactive dots)

**Interfaces:**
- Consumes: `useFeatureFlag("stop_departures")`, store stop sets.
- Produces: at zoom ≥ 15 with the flag on and nothing selected, the map shows all stops (inactive ones dimmed); below 15 or flag off, exactly today's behavior.

- [ ] **Step 1: Worker computes both sets and transports the active ids** — `worker.ts`:

Change `computeGroupedStops` signature to take an optional filter (pass `null` for no filtering):

```ts
function computeGroupedStops(
  stops: StopData[],
  activeStopIds: Set<string> | null,
): GroupedStop[] {
```

and its inner filter condition becomes:

```ts
      if (activeStopIds !== null && activeStopIds.size > 0 && !activeStopIds.has(stop.id)) {
        continue;
      }
```

The all-stops set depends only on the full stop list (not on active ids), so compute it once per stops fetch and cache it; active-id pushes re-send only the filtered set plus the id list. This keeps `stopsGroupedAll` identity stable between full stops refreshes, so the map re-uploads the ~2k-feature all-stops payload only when it actually changes (spec §7), not on every 30 s active-stops push:

```ts
let cachedGroupedAll: GroupedStop[] = [];

function handleProcessedStops(stops: StopData[]): StopsUpdateResponse {
  cachedStops = stops;
  const bounds = computeBounds(stops);
  const active = cachedActiveStopIds.size > 0 ? cachedActiveStopIds : null;
  cachedGroupedAll = computeGroupedStops(stops, null);
  return {
    type: "stops-update",
    stops,
    bounds,
    grouped: computeGroupedStops(stops, active),
    groupedAll: cachedGroupedAll,
    activeStopIds: [...cachedActiveStopIds],
  };
}

function handleActiveStopIds(activeStopIds: string[]): StopsUpdateResponse {
  cachedActiveStopIds = new Set(activeStopIds);
  const active = cachedActiveStopIds.size > 0 ? cachedActiveStopIds : null;
  return {
    type: "stops-update",
    grouped: computeGroupedStops(cachedStops, active),
    activeStopIds,
  };
}
```

`shared.ts`:

```ts
export type StopsUpdateResponse = {
  type: "stops-update";
  stops?: StopData[];
  bounds?: [[number, number], [number, number]];
  grouped: GroupedStop[];
  groupedAll?: GroupedStop[];
  activeStopIds?: string[];
};
```

`use-stops.ts` `handleStopsUpdate` — also populate the store's `activeStopIds` (it exists in the store but is currently never written; the map needs it for dimming):

```ts
  const state = useStore.getState();
  const useGrouped = state.selection === null;
  useStore.setState({
    stopsGrouped: response.grouped,
    ...(response.groupedAll ? { stopsGroupedAll: response.groupedAll } : {}),
    ...(response.activeStopIds
      ? { activeStopIds: new Set(response.activeStopIds) }
      : {}),
    displayedStops: useGrouped ? response.grouped : state.displayedStops,
  });
```

`store.ts`: add `stopsGroupedAll: GroupedStop[]` to `StoreState` (initial `[]`).

- [ ] **Step 2: Map zoom gating** — `map-container.tsx`:

Add near the top of the file:

```ts
/** Zoom at and above which all stops are shown (not just active ones). */
const ALL_STOPS_MIN_ZOOM = 15;
```

In `MapContainer()`:

```ts
  const stopsGroupedAll = useStore((s) => s.stopsGroupedAll);
  const activeStopIds = useStore((s) => s.activeStopIds);
  const stopBoardFlag = useFeatureFlag("stop_departures");
  const [zoom, setZoom] = useState(12);

  const showAllStops =
    stopBoardFlag && selection === null && zoom >= ALL_STOPS_MIN_ZOOM && stopsGroupedAll.length > 0;
  const baseStops = showAllStops ? stopsGroupedAll : displayedStops;
```

(import `useFeatureFlag` from `@/feature-flags-store`; `useState` is already imported.) On the `<MapGL>` element add:

```tsx
          onZoomEnd={(e) => setZoom(e.viewState.zoom)}
```

and initialize `zoom` from the real map once the style is ready (the map may restore a higher zoom from the URL hash before any zoomend fires):

```ts
  useEffect(() => {
    if (!styleReady) return;
    const z = mapRef.current?.getZoom();
    if (z !== undefined) setZoom(z);
  }, [styleReady]);
```

Change `routeStopsFeatures` to use `baseStops` and add the `active` property. Gate the computation on `showAllStops`: dimming is only meaningful while browsing all stops — a selected (possibly inactive) stop must render at full opacity (spec §6.5 "selection behavior unchanged"), and flag-off must never dim:

```ts
  const routeStopsFeatures = useMemo(() => {
    const filtered = searchMatchedStopIds
      ? baseStops.filter((s) => s.ids.some((id) => searchMatchedStopIds.has(id)))
      : baseStops;
    const hasActive = activeStopIds.size > 0;
    return {
      type: "FeatureCollection" as const,
      features: filtered.map((stop) => ({
        type: "Feature" as const,
        properties: {
          name: stop.name,
          ids: JSON.stringify(stop.ids),
          isNext: nextStopId !== null && stop.ids.includes(nextStopId),
          active:
            showAllStops && hasActive
              ? stop.ids.some((id) => activeStopIds.has(id))
              : true,
        },
        geometry: {
          type: "Point" as const,
          coordinates: [stop.lng, stop.lat],
        },
      })),
    };
  }, [baseStops, nextStopId, searchMatchedStopIds, activeStopIds, showAllStops]);
```

(The `hasActive` guard mirrors the worker's semantics for the night-time empty active set, when the worker's "active" grouping is already all stops.)

- [ ] **Step 3: Dim inactive dots in all four styles**

In each of `flat.json`, `3d.json`, `3d.dark.json`, `satellite.json`, find the `route-stop-dot` circle layer and change its `circle-opacity` (or add it if the style only sets `circle-color`) to a data-driven case, keeping each style's existing base value. For `flat.json` (base `0.67`):

```json
        "circle-opacity": [
          "case",
          ["boolean", ["get", "active"], true],
          0.67,
          0.22
        ]
```

Apply the same pattern in the other three styles using their existing base opacity (grep `circle-opacity` under `route-stop-dot`; if a style sets none, add the case expression with base `0.67`). The label layer stays untouched (maplibre declutters labels; hidden-by-filter features would not be clickable, and stop dots must stay clickable).

- [ ] **Step 4: Checks + browser verification**

```bash
just frontend check
```

Then with the flag enabled (see Task 7 Step 6 psql command), run `bun dev`, open the map, zoom in past 15: the dot count visibly increases and inactive dots render dim; zoom out: back to active-only; select a stop at high zoom: only the selected stop's group shows (existing behavior preserved). Reset flag to `disabled` after.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/scripts/worker.ts frontend/src/app/entity/shared.ts \
  frontend/src/hooks/use-stops.ts frontend/src/store.ts \
  frontend/src/components/map-container.tsx frontend/src/data/maps/style
git commit -m "Show all stops on the map when zoomed in"
```

---

### Task 9: Final E2E sweep and status report

**Files:**
- Modify: none (verification only; produces artifacts under `/tmp/zet-departures-e2e/`)

- [ ] **Step 1: Full backend + frontend check suite**

```bash
just backend fmt-dev && just backend test
just frontend check
```

- [ ] **Step 2: Run the stack and capture artifacts**

```bash
mkdir -p /tmp/zet-departures-e2e
docker exec zet-live-pg psql -U zet_live -d zet_live -c \
  "UPDATE feature_flags SET state='enabled' WHERE key='stop_departures'"
just backend dev-run &
(cd frontend && bun dev &)
sleep 15

SQUARE=$(docker exec zet-live-pg psql -U zet_live -d zet_live -tAc \
  "SELECT stop_id FROM gtfs_stops WHERE stop_name ILIKE '%Jela%' LIMIT 1")
# A rarely-served stop (fewest stop_times rows) — the "not empty anymore" case
QUIET=$(docker exec zet-live-pg psql -U zet_live -d zet_live -tAc \
  "SELECT stop_id FROM gtfs_stop_times GROUP BY stop_id ORDER BY count(*) ASC LIMIT 1")

curl -s "http://127.0.0.1:9011/api/v1/schedule/stop-departures?stop=$SQUARE" \
  > /tmp/zet-departures-e2e/square.json
curl -s "http://127.0.0.1:9011/api/v1/schedule/stop-departures?stop=$QUIET" \
  > /tmp/zet-departures-e2e/quiet.json
curl -s "http://127.0.0.1:9011/api/v1/schedule/stop-trips?stop=$SQUARE" \
  > /tmp/zet-departures-e2e/stop-trips-flag-off-path.json
python3 -c "
import json
for name in ('square', 'quiet'):
    d = json.load(open(f'/tmp/zet-departures-e2e/{name}.json'))['d']
    rows = d['departures']
    print(name, 'rows:', len(rows), 'live:', sum(1 for r in rows if r[4]), 'scheduleEnd:', d['scheduleEnd'])
"
```

Expected: `square` ≈ 20 rows with at least the scheduled rows sorted ascending (times strictly non-decreasing across `[2]` and `[5]` where present); `quiet` (rarely-used stop) has > 0 rows — the core feature promise.

- [ ] **Step 3: Browser pass (flag on, then off)**

Using the browser automation skill against `http://localhost:5173`:
1. Flag on: select the main-square stop → board renders, rows sorted, live rows tappable; minimize → summary line; zoom ≥ 15 → all stops appear (inactive dimmed, selected group full-opacity); zoom out → active only; flag off (psql + reload) → old `StopSheet` chips render.
2. Empty state: pick the rarely-served stop from Step 2 during a low-service window (or verify at night): empty board shows "No more departures today". "Schedule unavailable" cannot be forged against a live server (the endpoint reads the in-memory service-day cache, and any psql tampering is invisible until the fetcher reloads — which restores the tables), so verify it only as code (the `now > scheduleEnd` branch in `DeparturesBoard`) and note that in the report. Capture screenshots to `/tmp/zet-departures-e2e/` (`board-flag-on.png`, `map-zoomed-all-stops.png`, `board-flag-off.png`, `board-no-departures.png`).

- [ ] **Step 4: Stop background processes**

```bash
pkill -f "target/x86_64-unknown-linux-musl/debug/zet-live" || true
pkill -f "bun dev" || true
pkill -f vite || true
pgrep -af "vite|zet-live" || echo "all stopped"
docker exec zet-live-pg psql -U zet_live -d zet_live -c \
  "UPDATE feature_flags SET state='disabled' WHERE key='stop_departures'"
```

- [ ] **Step 5: Report** — summarize verified behavior, artifact paths, known limitations (see self-review notes below), and any deviations from the plan taken during implementation.

---

## Self-review notes (for implementers)

- Deliberate deviations from the spec, to note in the final report:
  - `arrivalTimes[].stopId` in `stop-trips` responses becomes `""` (Task 4 refactor); the frontend never reads it (only the zod schema names it).
  - Empty `stop` list on `stop-departures` returns a 200 with an empty `d`, mirroring `stop-trips`' existing convention (spec §4.3 loosely said "400"; the codebase convention wins).
  - `refreshStopDepartures` swallows errors silently (stale board beats toast spam on background refreshes); only the initial fetch toasts.
  - `scheduleEnd` is the exclusive midnight after the last service day (spec example showed the last inclusive second); the UI comparison `now > scheduleEnd` is consistent with the exclusive value.
- `departureSummary` and row rendering deliberately duplicate a tiny amount of formatting; the board owns its presentation.
- Do not run `just frontend build`/backend release builds repeatedly — use `dev-run`/`bun dev` for iteration; build once at the end if CI parity matters.
- Reviewer-verified environment facts this plan relies on: jiff 0.2.28 (`tomorrow()`/`yesterday()`/`to_zoned` return `Result`), zod 3.25.76, react-map-gl 8.1.1 (`onZoomEnd` → `e.viewState.zoom`), `stops-update` messages pass through no zod validation on the main thread, `--color-success`/`--color-primary` exist in `@theme`.
