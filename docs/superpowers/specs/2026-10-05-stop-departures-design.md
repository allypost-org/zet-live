# Stop departure board

**Date:** 2026-10-05
**Status:** Approved (design) — pending implementation plan
**Scope:** `backend/` (GTFS calendar import, service-day computation, new `stop-departures` endpoint) and `frontend/` (`DeparturesBoard` sheet, zoom-gated stop display). `frontend-admin/` untouched.

---

## 1. Goal

Make the selected-stop panel useful at every stop, at every time of day:

1. Replace the current route-grouped chip list with a **departure board in two sections**: a "Live now" section (trips with a live vehicle) and a "Scheduled" section (timetable-only trips), each time-sorted, with headsigns. A section with no rows is hidden entirely — the distinction between actually-running and planned trips stays visible, and a live trip whose vehicle drops off the feed simply moves into the scheduled section instead of disappearing.
2. Show **scheduled departures** from the GTFS timetable, so stops without live vehicles are no longer empty and users can see that a line passes in e.g. 15 minutes.
3. Show **all stops on the map when zoomed in** (currently only stops with live vehicles are displayed), so rarely-used stops are discoverable without search.
4. Show **delay on live rows**: scheduled clock time struck through, predicted clock time beside it (amber late, blue early), countdown leading with matching color.

Performance matters on both ends: low-powered phones on mobile data. Keep payloads small and work per request low.

## 2. Decisions

| Decision | Chosen option | Rejected alternatives |
|---|---|---|
| Departure list layout | **Time-sorted board** (live rows get pulsing dot + green countdown; scheduled rows get clock time or grey countdown) | Route-grouped chips (current layout — no room for headsigns, confusing when both directions serve a stop); separate "live" / "scheduled" sections (boundary moves, routes repeat) |
| Data flow | **Server-merged REST endpoint**, polled by the client | Client-side merge of two endpoints (duplicates tricky logic in every client); WS push per selected stop (needs per-connection subscriptions the broadcast WS lacks; see §9) |
| Service days | **Import `calendar.txt` + `calendar_dates.txt`, compute date → service set in memory** (standard GTFS rules) | Store nothing and guess from live feed (wrong: schedule must answer when no vehicle is live); full calendar expansion into stop_times rows (bloat) |
| Service-day computation | **General GTFS semantics** (weekday columns in date range, `exception_type=1` adds, `exception_type=2` removes) | ZET-specific shortcut "one service per date" (today's feed is degenerate this way, but the general rule is equally cheap and survives a feed change) |
| Rollout | **`stop_departures` feature flag** (registry slot already reserved); flag off = current behavior end-to-end | Big-bang switch |
| Old `stop-trips` endpoint | **Keep untouched** as the flag-off path | Remove (extra churn; it is the fallback during rollout) |
| DB schema | **No migration** — `gtfs_calendar` / `gtfs_calendar_dates` tables already exist (currently dead) | New tables |
| Payload | **Positional arrays** per departure row, `Versioned {v, d}` wrapper, `JsonOrAccept` (JSON or CBOR) like all v1 endpoints | Named-field objects (larger); a new envelope |
| Inactive stops on map | **Zoom-gated: all stops at z ≥ 15, active-only below**; inactive dots render dimmer via a style layer | All stops at every zoom (clutter + mobile cost); panel-only (rare stops stay undiscoverable) |

## 3. Backend: service days

### 3.1 Calendar import

Add `calendar.txt` → `gtfs_calendar` and `calendar_dates.txt` → `gtfs_calendar_dates` to the `FileSpec` list in `GtfsSchedule::read_from_zip_bytes` (`src/proto/gtfs_schedule/data/mod.rs`). The tables already exist with the right columns; no migration.

ZET feed facts (verified 2026-10-05, feed version 000396): 7 services (`0_30`–`0_36`), all weekday flags zero, `calendar_dates.txt` has exactly one `exception_type=1` row per date, 2026-09-28 → 2026-12-31, no `exception_type=2`. The general rule below handles this correctly without special-casing.

### 3.2 Service-day map

New module `src/database/service_days.rs`, following the `schedule_offsets` pattern:

- Type: `ServiceDays { by_date: HashMap<Date, HashSet<String>> }` (service ids active per local date) plus `feed_end: Option<Date>` (latest date with any active service).
- Build from both tables with standard GTFS semantics: a service runs on a date if (weekday flag set AND date within `[start_date, end_date]`) OR listed with `exception_type=1`, minus `exception_type=2`. Dates outside any service → empty set.
- Load at startup (after migrations, alongside `schedule_offsets::reload()`) and swap on every schedule import (`ArcSwap`).
- Timezone: `Europe/Zagreb` constant (jiff). Service-day midnight = local midnight of that date in that zone.

## 4. Backend: `GET /api/v1/schedule/stop-departures`

### 4.1 Request

- Query params: `stop` (repeatable, same convention as `stop-trips`), optional `limit` (default 20, max 50).
- Auth: same as other v1 schedule endpoints.

### 4.2 Computation

All functions take `now` explicitly (unit-testable without clock mocking):

1. **Window**: `[now − 2 min, now + 6 h]` absolute.
2. **Candidate service days**: the local dates of `now` and `now − 1 day`. A departure at GTFS second `s` on service day `D` is absolute `midnight(D) + s`; times may exceed 24 h (ZET runs to ~30 h), so yesterday's service day contributes today's after-midnight trips.
3. **Scheduled rows**: for each candidate day, one indexed query —
   `gtfs_stop_times st JOIN gtfs_trips t ON t.trip_id = st.trip_id JOIN gtfs_routes r ON r.route_id = t.route_id` with `st.stop_id = ANY($stops)`, `t.service_id = ANY($services_of_day)`, and `departure_time_seconds` mapped into the window (bounds computed per day). Return route short name, headsign (`st.stop_headsign` preferred, `t.trip_headsign` fallback), trip id + trip key, absolute departure. Sort by absolute time.
   The `stop_id` filter is the selective one (existing `(stop_id, trip_id)` index; a stop has on the order of 10² stop_times rows across services); no new index required.
4. **Live rows**: the same live-arrival lookup `get_stop_trips` uses today (live vehicles whose trip serves any requested stop, with predicted arrival at that stop from `live_trip_stop_times`).
5. **Merge by `trip_key`**:
   - Scheduled row whose trip key matches a live row → live departure: shows `vehicleId` + `predictedTime` (predicted replaces scheduled).
   - Live row with no scheduled match (RT-only trip) → live-only departure, inserted in time order.
   - Live rows are never dropped by `limit`; `limit` caps scheduled rows.

### 4.3 Response

`Versioned` wrapper, `d`:

```json
{
  "departures": [
    ["6", "Črnomerec", 1791210000, "0_40_20601_206_10157", "11042", 1791210180],
    ["13", "Žitnjak", 1791210600, "0_30_26820_268_10002", "", null]
  ],
  "scheduleEnd": 1798761599
}
```

Row fields, positional: `routeId`, `headsign`, `scheduledTime` (unix seconds), `tripId`, `vehicleId`, `predictedTime`. Live rows have non-empty `vehicleId` and non-null `predictedTime`; scheduled rows have `""` / `null`. `scheduleEnd` = end of the last service day known to the service-day map (unix seconds; lets the UI say "schedule unavailable" when the feed horizon passes).

Status codes match existing conventions (`400` bad stop ids, `500` on DB failure). Query goes through sqlx macros → regenerate `.sqlx/` cache.

### 4.4 Handler layout

- `src/server/routes/v1/schedule/departures.rs` — HTTP handler (params, response shape).
- Pure computation (service-day candidates, window mapping, merge) as free functions in the same module, `#[cfg(test)] mod tests` beside them (see §8).
- Feature flag: the endpoint **is** user-aware gated (`is_enabled` with the requesting user, fail-closed 404) and the frontend usage is gated (§6.4). Gating the endpoint also uses the registry constructor server-side, keeping the flag single-sourced.

## 5. Backend: feature flag

Add `stop_departures => "stop_departures"` to the `feature_flags!` registry (`src/feature_flags.rs`, slot already reserved by a placeholder comment). Default state: `disabled`. Enabled per-user via existing admin CRUD (`scoped`/`logged_in` states available for gradual rollout).

## 6. Frontend

### 6.1 Entities

In `src/app/entity/v1/api.ts`:

- `departureSchema`: zod tuple → discriminated union `{ kind: "live", routeId, headsign, scheduledTime: Date, tripId, vehicleId, predictedTime: Date } | { kind: "scheduled", routeId, headsign, scheduledTime: Date, tripId }` (empty-string/null encodings collapse in the transform).
- `stopDeparturesResponseSchema`: `{ d: { departures: Departure[], scheduleEnd: Date | null } }`.

### 6.2 Store & fetch

- `StopSelection` gains `departures: Departure[] | null` and `scheduleEnd: Date | null`; `arrivalTimes` and its refresh path are removed from the flag-on flow (flag off keeps the old path).
- `fetchStopDepartures(ids)` in `src/hooks/use-stops.ts` following the existing stale-guard/`AbortController` pattern; called by `use-selection-fetcher` on stop selection and on an interval (~20 s) while a stop is selected, replacing the 15 s `stop-trips` refresh when the flag is on.
- Map vehicle dimming (`selectedStopTripIds`) derives trip ids from live departure rows.

### 6.3 `DeparturesBoard` component

`src/components/departures-board.tsx` (replaces `StopSheet` in the sheet body; `stop-sheet.tsx` is deleted when the flag-on path lands as the only path):

- Rows render under two labeled sections, each hidden when it has no rows: **Live now** (live rows) and **Scheduled** (scheduled-only rows), each sorted by time. Row: `LineBadge` + headsign + right-aligned time. Live rows: pulsing green dot + green countdown, tap → `selectVehicle(vehicleId, tripId)`. Scheduled rows: grey countdown when ≤ 60 min away, `HH:MM` beyond.
- **Delay on live rows** (delay = `predictedTime − scheduledTime`, computed client-side; no response change): when |delay| ≥ 1 min, the row shows the struck-through scheduled clock time with the predicted clock time beside it — amber when late, blue when early — and the countdown takes the same color. On-time live rows and live-only rows (no schedule match) show the plain green countdown.
- New shared `LineBadge` component (`src/components/line-badge.tsx`) — the bus/tram badge currently copy-pasted in `stop-sheet.tsx`, `app.tsx`, and `search-bar.tsx`.
- Countdowns re-render on a ~15 s tick and on `visibilitychange`, not per second. Board re-sorts only on fetch; between fetches, countdowns tick locally.
- Empty states: skeleton rows while loading; "No more departures today" when the list is empty but schedule covers today; "Schedule unavailable" when `scheduleEnd` has passed. Fetch errors toast and retry on the next interval (existing pattern).
- Minimized sheet body: first live row ("6 · Črnomerec · 3 min") or first scheduled row.

### 6.4 Feature flag gating

- `capabilities` already delivers `featureFlags` to the client (plus WS push on change). A helper checks `stop_departures`.
- Flag on: sheet renders `DeparturesBoard`, fetches the new endpoint, and the map uses zoom-gated all-stops display (§6.5).
- Flag off: current `StopSheet` + `stop-trips` path and active-only map, unchanged.

### 6.5 Zoom-gated all stops

- `computeGroupedStops` (`src/scripts/worker.ts`) produces **two** sets: active-only (current output) and all grouped stops. Both flow through the existing `stops-update` message (add `groupedAll`; keep `grouped` semantics unchanged for flag-off).
- `map-container.tsx`: on zoom crossing threshold **15** (a constant; only evaluated on `zoomend`, data swapped through the rAF-batched setter), the `route-stops` source shows all stops; below it, active-only. No per-zoom-tick work. Applies only when the flag is on (§6.4).
- Stop features gain an `active` property; all four bundled map styles (`flat.json`, `3d.json`, `3d.dark.json`, `satellite.json`) render inactive dots dimmer than active ones.
- Selection, search, and URL restore behavior unchanged.

## 7. Performance notes

- Departures query: `stop_id`-indexed, ~10² candidate rows per stop, PK joins; target < 10 ms. No per-request caching in v1 — measure first (the query is per-selection, not per-broadcast).
- Payload: 20 rows ≈ ~1.5 KB JSON, less as CBOR. One poll every ~20 s while a stop sheet is open; no polling otherwise.
- All-stops map set: ~3.8 k stops → ~2 k grouped features; `setData` runs only on threshold crossings, not continuously.
- Board rendering: ~20 rows, one interval-driven re-render; no per-second timers.

## 8. Testing

Meaningful failure modes, per repo policy (no TDD; unit tests only for subtle logic; E2E through public interfaces):

- **Rust unit tests** (inline `#[cfg(test)]`):
  - Service-day computation: weekday-window rule, exceptions add/remove, ZET's all-zero + one-add-per-date case, dates outside the feed.
  - Window/service-day mapping: after-midnight trip (25:00 on yesterday's service day lands tomorrow 01:00), window edges, DST boundary (Europe/Zagreb oct/mar transitions).
  - Merge: live replaces scheduled for matching trip key; live-only row inserted; live rows survive `limit`.
- **E2E**: run the backend against real PG with a real ZET feed import; `curl` the endpoint for known stops (major daytime stop, night stop for after-midnight rows, feed-horizon stub via a forged service-day map) and save responses as artifacts; browser-check the board via the dev server (loading, live rows, scheduled rows, empty states, flag off/on).
- Frontend: `just frontend check`; backend: `just backend fmt-dev`, `just backend test`, `just sqlx-regenerate` (commit `.sqlx/`).

## 9. WebSocket migration (cost estimate, not in scope)

Migrating §4 to WS push later costs **~1–2 days**, moderate risk:

- Reuse: the departures computation and response encoding stay as-is; a WS frame would carry the same payload.
- New: a client→server `stop-departures-sub` message (the WS already accepts JSON text frames for auth); a per-connection subscription registry keyed by stop group; recompute on each feed cycle (~2 s) and send directly on the connection's sink; cleanup on disconnect.
- The one real complexity: the SharedWorker owns a single WS shared by all tabs, so per-tab subscriptions need ref-counted multiplexing in the worker.

## 10. Non-goals

- Day switching / "tomorrow's board" (a departure board shows the rest of today; the night edge shows after-midnight service).
- "Load more" beyond `limit`; per-route filtering; favorite stops; reminders.
- Applying a vehicle's delay to scheduled rows it does not serve.
- Changes to `frontend-admin/`.
