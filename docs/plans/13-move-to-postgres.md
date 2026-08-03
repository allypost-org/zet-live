# Move backend from SQLite to Postgres — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the SQLite persistence layer with PostgreSQL (Postgres-only, single fresh `init` migration, idiomatic PG types).

**Architecture:** Swap sqlx's `sqlite` feature for `postgres`, collapse the 6 existing migration pairs into one new `init`, delete the PRAGMA block, rewrite the `DatabaseUrl` parser as a validating newtype, and update ~25 Rust files for the new type bindings (booleans, `jsonb`, `numeric`, `timestamptz` via a `time`-crate bridge, `= ANY($1)` array binds for dynamic `IN`).

**Tech Stack:** Rust 2024, sqlx 0.9 (postgres + macros + `time` + `json` + `rust_decimal`), `time` 0.3 (jiff↔PG timestamptz bridge), `rust_decimal` 1 (currency), `jiff` 0.2 (application time, unchanged), PG ≥ 14 (generated columns need ≥12; `GENERATED … AS IDENTITY` needs ≥10).

**Spec:** [`docs/superpowers/specs/2026-08-03-move-to-postgres-design.md`](../superpowers/specs/2026-08-03-move-to-postgres-design.md). Read it before starting.

## Global Constraints

- **Working directory for all `just`/`cargo`/`sqlx` commands:** `backend/` (or prefix with `just backend ...` from repo root). All relative paths below are from repo root unless noted.
- **Formatting:** Always finish a code-changing task with `just fmt-dev` (clippy `--fix` + nightly `cargo fmt`). Do NOT run `just fmt`/`just lint` standalone.
- **No new `#[allow(...)]` / `#[expect(...)]`** and do not remove clippy lints from `Cargo.toml` without explicit user permission.
- **No tests exist** in this project (per `backend/AGENTS.md`). Verification per task is NOT unit tests — it's the cycle described in "Verification approach" below. Do not invent a test framework.
- **All Rust files: 4-space indent. All other files: 2-space indent. LF endings, trim trailing whitespace.**
- **sqlx offline cache:** the build runs with `SQLX_OFFLINE=true`. After any query change you must regenerate `backend/.sqlx/` against a live PG or the build breaks. The regen is gated into Task 8 (don't try to do it piecemeal earlier — intermediate states intentionally won't compile).
- **Postgres dev instance:** the user is running one at `DATABASE_URL=postgres://postgres:mysecretpassword@localhost:5435/postgres`. Use this URL for `sqlx` CLI commands and smoke tests.
- **`AGENTS.md` precedence:** read `backend/AGENTS.md` before editing backend code; it documents the `just` recipes, sqlx cache rules, and lint conventions in more detail.
- **Commits:** Each task ends with a commit on the `feat/move-to-postgres` branch. The per-task "Stage for commit" steps below are stage-AND-commit — append `&& git commit -m "<message>"`. Use the repo's commit style: capitalized imperative sentence, no conventional-commit prefix, no scope tags (see `git log` for examples). **Never push to origin** — the user pushes when ready. Each implementer commits its own task; the controller records the commit range in the ledger.

## Verification approach (read this — it replaces TDD for this plan)

This codebase has no test suite and the spec scopes out adding one. So the per-task verification cycle is one or more of:

- **`cargo check`** — incremental compile progress. Tasks 2–7 are *expected* to leave `cargo check` failing (the sqlx feature swap and stale `.sqlx` cache break everything at once). Each task notes the expected failure reason. Only Task 8 (cache regen) produces a clean `cargo check`. Use the *error count dropping* between tasks as the progress signal.
- **`DATABASE_URL=postgres://postgres:mysecretpassword@localhost:5435/postgres sqlx migrate run`** (run from `backend/`) — the migration applies cleanly to PG.
- **`cargo sqlx prepare --workspace`** (run from `backend/`, with `SQLX_OFFLINE=` unset and `DATABASE_URL` set) — the `.sqlx` cache regenerates. This is the big gate in Task 8.
- **Final task:** `just fmt-dev` + `just build` + boot the server + manual smoke checks.

Do not write Rust `#[test]`s. If you want a sanity check for a specific transformation (e.g. one `= ANY($1)` query), the verification is: the `cargo sqlx prepare` step in Task 8 succeeds for that query (macro type-checks it against the live PG). If `prepare` fails on a query, fix forward.

## File Structure

**Created:**
- `backend/migrations/<ts>_init.up.sql` — full PG schema (replaces 6 old pairs)
- `backend/migrations/<ts>_init.down.sql` — drop everything in reverse
- `backend/src/database/time.rs` — jiff↔`time::OffsetDateTime` bridge helpers

**Deleted:**
- `backend/migrations/20260605172220_init.{up,down}.sql`
- `backend/migrations/20260610234142_add_admin_settings.{up,down}.sql`
- `backend/migrations/20260616180000_gbfs.{up,down}.sql`
- `backend/migrations/20260617120000_add_feedback.{up,down}.sql`
- `backend/migrations/20260623170000_user_accounts.{up,down}.sql`
- `backend/migrations/20260803110000_add_trip_key.{up,down}.sql`
- `backend/src/database/sqlx_types.rs` (the `sqlx_int_enum_decode!` macro)
- All 112 files under `backend/.sqlx/` (regenerated in Task 8)

**Modified (Rust):**
- `backend/Cargo.toml` — sqlx feature swap, add `time` + `rust_decimal`
- `backend/src/database/mod.rs` — `Sqlite*` → `Pg*`, delete PRAGMAs + optimize task
- `backend/src/cli/mod.rs` — `DatabaseUrl` newtype, required, scheme validation
- `backend/src/proto/gtfs_schedule/data/{trip,route,stop}.rs` — `#[repr(u8)]`→`#[repr(i32)]`, drop macro
- `backend/src/auth/accounts.rs` — `GROUP_CONCAT`→`string_agg`; timestamp decode
- `backend/src/auth/{session,oauth,config}.rs` — timestamp binds
- `backend/src/admin/{feedback,user_notices,metadata,router,mod,settings}.rs` — booleans, jsonb, timestamps
- `backend/src/server/routes/v1/{mod,schedule/mod}.rs` — `= ANY($1)` conversions (7 sites), `AssertSqlSafe` removal, timestamps
- `backend/src/server/routes/v1/{settings,feedback/mod,auth/mod}.rs` — timestamps/jsonb
- `backend/src/proto/gtfs_schedule/{fetcher,data/mod}.rs` — drop `VACUUM`, epoch-float → timestamptz, `AssertSqlSafe` removal
- `backend/src/proto/gbfs/data/system_pricing_plans.rs` — `Option<f64>` → `Option<rust_decimal::Decimal>`
- Other `src/proto/gbfs/data/*.rs` — any `*_at`/timestamp sites (enumerate in Task 7)

**Modified (tooling/docs):**
- `backend/justfile` — `sqlx-regenerate`, `migrations-run`, `_migrations-ensure-db`
- `backend/AGENTS.md` — Database section, Environment section, sqlx cache note
- `AGENTS.md` (root) — Docker/deploy note

---

### Task 1: Replace migrations with a single Postgres `init`

**Files:**
- Delete: the 12 migration files listed in "File Structure → Deleted"
- Create: `backend/migrations/<ts>_init.up.sql`, `backend/migrations/<ts>_init.down.sql` (where `<ts>` is generated by `just migrations-add`)

**Interfaces:**
- Produces: a PG schema that every later task's `sqlx::query!` will be type-checked against during the cache regen (Task 8).

- [ ] **Step 1: Generate the new migration skeleton**

From `backend/`:
```bash
DATABASE_URL=postgres://postgres:mysecretpassword@localhost:5435/postgres just migrations-add init
```
This creates `backend/migrations/<ts>_init.{up,down}.sql` with placeholder comments. Note: the justfile's `_migrations-ensure-db` shim is SQLite-only — but `just migrations-add` only invokes `sqlx migrate add`, which doesn't touch the DB, so it works against the PG `DATABASE_URL`.

- [ ] **Step 2: Delete the 6 old migration pairs**

```bash
cd backend/migrations && rm \
  20260605172220_init.up.sql 20260605172220_init.down.sql \
  20260610234142_add_admin_settings.up.sql 20260610234142_add_admin_settings.down.sql \
  20260616180000_gbfs.up.sql 20260616180000_gbfs.down.sql \
  20260617120000_add_feedback.up.sql 20260617120000_add_feedback.down.sql \
  20260623170000_user_accounts.up.sql 20260623170000_user_accounts.down.sql \
  20260803110000_add_trip_key.up.sql 20260803110000_add_trip_key.down.sql
```

- [ ] **Step 3: Write `init.up.sql`**

Replace the placeholder contents of `backend/migrations/<ts>_init.up.sql` with the full schema below. This applies the §3 type mapping to the union of the 6 old migrations (including the `add_trip_key` columns folded in from the start, since the schema is built fresh).

```sql
-- ZET Live schema for PostgreSQL.
-- Mirrors the SQLite schema's final form with idiomatic PG types; see
-- docs/superpowers/specs/2026-08-03-move-to-postgres-design.md §3.

-- Reference data enums (values inserted by the GTFS loader, not auto-generated)
CREATE TABLE gtfs_location_types (
  location_type SMALLINT PRIMARY KEY,
  description   TEXT
);
CREATE TABLE gtfs_route_types (
  route_type   SMALLINT PRIMARY KEY,
  description  TEXT
);
CREATE TABLE gtfs_directions (
  direction_id SMALLINT PRIMARY KEY,
  description  TEXT
);
CREATE TABLE gtfs_pickup_dropoff_types (
  type_id      SMALLINT PRIMARY KEY,
  description  TEXT
);
CREATE TABLE gtfs_transfer_types (
  transfer_type SMALLINT PRIMARY KEY,
  description   TEXT
);
CREATE TABLE gtfs_payment_methods (
  payment_method SMALLINT PRIMARY KEY,
  description    TEXT
);

CREATE TABLE gtfs_schedule_meta (
  etag          TEXT,
  last_modified TIMESTAMPTZ,
  fetched_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_gtfs_schedule_meta__etag          ON gtfs_schedule_meta(etag);
CREATE INDEX idx_gtfs_schedule_meta__last_modified ON gtfs_schedule_meta(last_modified);

CREATE TABLE gtfs_agency (
  agency_id       TEXT PRIMARY KEY,
  agency_name     TEXT NOT NULL,
  agency_url      TEXT NOT NULL,
  agency_timezone TEXT NOT NULL,
  agency_lang     TEXT,
  agency_phone    TEXT,
  fare_url        TEXT
);

CREATE TABLE gtfs_stops (
  stop_id            TEXT PRIMARY KEY,
  stop_code          TEXT,
  stop_name          TEXT,
  tts_stop_name      TEXT,
  latitude           DOUBLE PRECISION,
  longitude          DOUBLE PRECISION,
  zone_id            TEXT,
  stop_url           TEXT,
  location_type      SMALLINT,
  parent_station     TEXT,
  stop_timezone      TEXT,
  wheelchair_boarding SMALLINT,
  level_id           TEXT,
  platform_code      TEXT
);

CREATE TABLE gtfs_routes (
  route_id         TEXT PRIMARY KEY NOT NULL,
  agency_id        TEXT,
  route_short_name TEXT DEFAULT '',
  route_long_name  TEXT DEFAULT '',
  route_desc       TEXT,
  route_type       SMALLINT,
  route_url        TEXT,
  route_color      TEXT,
  route_text_color TEXT
);

CREATE TABLE gtfs_calendar (
  service_id TEXT PRIMARY KEY,
  monday     SMALLINT NOT NULL,
  tuesday    SMALLINT NOT NULL,
  wednesday  SMALLINT NOT NULL,
  thursday   SMALLINT NOT NULL,
  friday     SMALLINT NOT NULL,
  saturday   SMALLINT NOT NULL,
  sunday     SMALLINT NOT NULL,
  start_date TEXT NOT NULL,
  end_date   TEXT NOT NULL
);

CREATE TABLE gtfs_calendar_dates (
  service_id    TEXT NOT NULL,
  date          TEXT NOT NULL,
  exception_type SMALLINT NOT NULL
);
CREATE INDEX idx_gtfs_calendar_dates__service_id ON gtfs_calendar_dates(service_id);

CREATE TABLE service_combo_ids (
  combination_id INTEGER PRIMARY KEY
);
CREATE TABLE service_combinations (
  combination_id INTEGER,
  service_id     TEXT
);
CREATE INDEX idx_service_combinations__combination_id ON service_combinations(combination_id);

CREATE TABLE gtfs_fare_attributes (
  fare_id          TEXT PRIMARY KEY,
  price            NUMERIC(19, 4) NOT NULL,
  currency_type    TEXT NOT NULL,
  payment_method   SMALLINT,
  transfers        SMALLINT,
  transfer_duration INTEGER,
  agency_id        TEXT
);

CREATE TABLE gtfs_fare_rules (
  fare_id        TEXT NOT NULL,
  route_id       TEXT,
  origin_id      INTEGER,
  destination_id INTEGER,
  contains_id    INTEGER,
  service_id     TEXT
);
CREATE INDEX idx_gtfs_fare_rules__fare_id ON gtfs_fare_rules(fare_id);

CREATE TABLE gtfs_shapes (
  shape_id           TEXT NOT NULL,
  shape_pt_lat       DOUBLE PRECISION NOT NULL,
  shape_pt_lon       DOUBLE PRECISION NOT NULL,
  shape_pt_sequence  INTEGER NOT NULL,
  shape_dist_traveled DOUBLE PRECISION
);
CREATE INDEX idx_gtfs_shapes__shape_id__shape_pt_sequence
  ON gtfs_shapes(shape_id, shape_pt_sequence);

CREATE TABLE gtfs_trips (
  trip_id            TEXT PRIMARY KEY,
  route_id           TEXT,
  service_id         TEXT,
  trip_headsign      TEXT,
  trip_short_name    TEXT,
  direction_id       SMALLINT,
  block_id           TEXT,
  shape_id           TEXT,
  wheelchair_boarding SMALLINT,
  bikes_allowed      SMALLINT,
  trip_key           TEXT
);
CREATE INDEX idx_gtfs_trips__route_service ON gtfs_trips(route_id, service_id);
CREATE INDEX idx_gtfs_trips__trip_id       ON gtfs_trips(trip_id);
CREATE INDEX idx_gtfs_trips__trip_key      ON gtfs_trips(trip_key);

CREATE TABLE gtfs_frequencies (
  trip_id             TEXT NOT NULL,
  start_time          TEXT NOT NULL,
  end_time            TEXT NOT NULL,
  headway_secs        INTEGER NOT NULL,
  start_time_seconds  INTEGER,
  end_time_seconds    INTEGER
);
CREATE INDEX idx_gtfs_frequencies__trip_id ON gtfs_frequencies(trip_id);

CREATE TABLE gtfs_transfers (
  from_stop_id    TEXT,
  to_stop_id      TEXT,
  transfer_type   SMALLINT,
  min_transfer_time INTEGER,
  from_route_id   TEXT,
  to_route_id     TEXT,
  service_id      TEXT
);

CREATE TABLE gtfs_feed_info (
  feed_publisher_name TEXT,
  feed_publisher_url  TEXT,
  feed_timezone       TEXT,
  feed_lang           TEXT,
  feed_version        TEXT
);

-- gtfs_stop_times: GTFS time-of-day values are TEXT (may exceed 24:00:00);
-- the *_seconds generated columns are seconds-since-midnight, computed via
-- split_part (immutable). PG >= 12 supports STORED generated columns.
CREATE TABLE gtfs_stop_times (
  trip_id              TEXT NOT NULL,
  arrival_time         TEXT CHECK (arrival_time LIKE '__:__:__'),
  departure_time       TEXT CHECK (departure_time LIKE '__:__:__'),
  stop_id              TEXT NOT NULL,
  stop_sequence        INTEGER NOT NULL,
  stop_headsign        TEXT,
  pickup_type          SMALLINT,
  drop_off_type        SMALLINT,
  shape_dist_traveled  DOUBLE PRECISION,
  arrival_time_seconds INTEGER
    GENERATED ALWAYS AS (
      CASE WHEN arrival_time IS NOT NULL THEN
        split_part(arrival_time, ':', 1)::int * 3600
        + split_part(arrival_time, ':', 2)::int * 60
        + split_part(arrival_time, ':', 3)::int
      END
    ) STORED,
  departure_time_seconds INTEGER
    GENERATED ALWAYS AS (
      CASE WHEN departure_time IS NOT NULL THEN
        split_part(departure_time, ':', 1)::int * 3600
        + split_part(departure_time, ':', 2)::int * 60
        + split_part(departure_time, ':', 3)::int
      END
    ) STORED,
  trip_key TEXT
);
CREATE INDEX idx_gtfs_stop_times__trip_id__stop_sequence
  ON gtfs_stop_times(trip_id, stop_sequence);
CREATE INDEX idx_gtfs_stop_times__stop_id__trip_id
  ON gtfs_stop_times(stop_id, trip_id);
CREATE INDEX idx_gtfs_stop_times__trip_id__stop_id
  ON gtfs_stop_times(trip_id, stop_id);
CREATE INDEX idx_gtfs_stop_times__trip_key ON gtfs_stop_times(trip_key);

-- live_* tables: ephemeral, rewritten every realtime cycle
CREATE TABLE live_trips (
  trip_id TEXT NOT NULL,
  trip_key TEXT
);
CREATE INDEX idx_live_trips__trip_id  ON live_trips(trip_id);
CREATE INDEX idx_live_trips__trip_key ON live_trips(trip_key);

CREATE TABLE live_trip_stop_times (
  trip_id        TEXT NOT NULL,
  stop_id        TEXT NOT NULL,
  stop_sequence  INTEGER NOT NULL,
  arrival_time   INTEGER,
  arrival_delay  INTEGER,
  PRIMARY KEY (trip_id, stop_sequence)
);
CREATE INDEX idx_live_trip_stop_times__trip_id__stop_sequence__arrival_delay
  ON live_trip_stop_times(trip_id, stop_sequence, arrival_delay);

CREATE TABLE live_feed_metadata (
  id           SMALLINT PRIMARY KEY,
  base_midnight BIGINT NOT NULL
);
INSERT INTO live_feed_metadata (id, base_midnight) VALUES (0, 0);

CREATE TABLE live_vehicles (
  vehicle_id            TEXT PRIMARY KEY,
  route_id              TEXT NOT NULL,
  trip_id               TEXT NOT NULL,
  latitude              DOUBLE PRECISION NOT NULL,
  longitude             DOUBLE PRECISION NOT NULL,
  prev_latitude         DOUBLE PRECISION,
  prev_longitude        DOUBLE PRECISION,
  next_stop_id          TEXT,
  next_stop_sequence    INTEGER CHECK (next_stop_sequence IS NULL OR next_stop_sequence >= 0),
  next_stop_arrival_delay INTEGER,
  next_stop_arrival_time  INTEGER,
  bearing               DOUBLE PRECISION,
  route_long_name       TEXT,
  trip_headsign         TEXT,
  trip_key              TEXT
);
CREATE INDEX idx_live_vehicles__trip_id  ON live_vehicles(trip_id);
CREATE INDEX idx_live_vehicles__route_id ON live_vehicles(route_id);
CREATE INDEX idx_live_vehicles__trip_key ON live_vehicles(trip_key);

-- Admin / operator config
CREATE TABLE admin_settings (
  name       TEXT PRIMARY KEY,
  value      JSONB NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL
);
CREATE TABLE admin_metadata (
  name       TEXT PRIMARY KEY,
  value      JSONB NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL
);

-- GBFS (nextbike)
CREATE TABLE gbfs_system_information (
  system_id          TEXT PRIMARY KEY,
  name               TEXT,
  operator           TEXT,
  url                TEXT,
  phone_number       TEXT,
  email              TEXT,
  feed_contact_email TEXT,
  timezone           TEXT,
  language           TEXT,
  license_id         TEXT,
  rental_apps        TEXT
);
CREATE TABLE gbfs_vehicle_types (
  vehicle_type_id TEXT PRIMARY KEY,
  name            TEXT,
  form_factor     TEXT,
  propulsion_type TEXT,
  rider_capacity  INTEGER,
  vehicle_image   TEXT,
  description     TEXT
);
CREATE TABLE gbfs_stations (
  station_id         TEXT PRIMARY KEY,
  name               TEXT,
  short_name         TEXT,
  lat                DOUBLE PRECISION NOT NULL,
  lon                DOUBLE PRECISION NOT NULL,
  region_id          TEXT,
  capacity           INTEGER,
  is_virtual_station BOOLEAN,
  rental_uris        TEXT
);
CREATE INDEX idx_gbfs_stations__region_id ON gbfs_stations(region_id);
CREATE TABLE gbfs_station_status (
  station_id              TEXT PRIMARY KEY,
  num_bikes_available     INTEGER,
  num_docks_available     INTEGER,
  is_installed            BOOLEAN,
  is_renting              BOOLEAN,
  is_returning            BOOLEAN,
  last_reported           BIGINT,
  vehicle_types_available TEXT
);
CREATE TABLE gbfs_regions (
  region_id TEXT PRIMARY KEY,
  name      TEXT
);
CREATE TABLE gbfs_pricing_plans (
  plan_id         TEXT PRIMARY KEY,
  name            TEXT,
  currency        TEXT,
  price           NUMERIC(19, 4),
  is_taxable      BOOLEAN,
  description     TEXT,
  per_min_pricing TEXT
);
CREATE TABLE gbfs_rental_hours (
  id          INTEGER PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
  user_types  TEXT,
  days        TEXT,
  start_time  TEXT,
  end_time    TEXT
);

-- Feedback
CREATE TABLE feedback (
  id          BIGINT PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
  category    TEXT NOT NULL,
  message     TEXT NOT NULL,
  name        TEXT,
  contact     TEXT,
  meta_url    TEXT,
  meta_ua     TEXT,
  meta_lang   TEXT,
  meta_build  TEXT,
  ip          TEXT NOT NULL,
  created_at  TIMESTAMPTZ NOT NULL,
  handled     BOOLEAN NOT NULL DEFAULT FALSE,
  user_id     TEXT,
  reply       TEXT,
  replied_at  TIMESTAMPTZ,
  dismissed   BOOLEAN NOT NULL DEFAULT FALSE
);
CREATE INDEX idx_feedback__created_at ON feedback (created_at DESC);
CREATE INDEX idx_feedback__handled    ON feedback (handled);

-- User accounts / auth
CREATE TABLE users (
  id           TEXT PRIMARY KEY,
  display_name TEXT,
  email        TEXT,
  avatar_url   TEXT,
  created_at   TIMESTAMPTZ NOT NULL,
  updated_at   TIMESTAMPTZ NOT NULL
);
CREATE TABLE user_oauth_identities (
  id                    TEXT PRIMARY KEY,
  user_id               TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  provider              TEXT NOT NULL,
  provider_subject      TEXT NOT NULL,
  provider_email        TEXT,
  provider_display_name TEXT,
  provider_avatar_url   TEXT,
  created_at            TIMESTAMPTZ NOT NULL,
  updated_at            TIMESTAMPTZ NOT NULL,
  UNIQUE (provider, provider_subject)
);
CREATE INDEX idx_user_oauth_identities__user_id ON user_oauth_identities (user_id);
CREATE TABLE user_sessions (
  id         TEXT PRIMARY KEY,
  token_hash BYTEA UNIQUE NOT NULL,
  user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  expires_at TIMESTAMPTZ NOT NULL,
  created_at TIMESTAMPTZ NOT NULL,
  ip         TEXT,
  user_agent TEXT
);
CREATE INDEX idx_user_sessions__user_id    ON user_sessions (user_id);
CREATE INDEX idx_user_sessions__expires_at ON user_sessions (expires_at);
CREATE TABLE user_settings (
  user_id    TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
  settings   JSONB NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL
);
CREATE TABLE oauth_states (
  state         TEXT PRIMARY KEY,
  provider      TEXT NOT NULL,
  pkce_verifier TEXT NOT NULL,
  link          BOOLEAN NOT NULL DEFAULT FALSE,
  origin        TEXT,
  user_id       TEXT REFERENCES users(id) ON DELETE CASCADE,
  created_at    TIMESTAMPTZ NOT NULL,
  expires_at    TIMESTAMPTZ NOT NULL
);
CREATE INDEX idx_oauth_states__expires_at ON oauth_states (expires_at);
CREATE TABLE link_tickets (
  token_hash BYTEA PRIMARY KEY,
  user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  expires_at TIMESTAMPTZ NOT NULL,
  created_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX idx_link_tickets__expires_at ON link_tickets (expires_at);
CREATE TABLE auth_providers (
  id            TEXT PRIMARY KEY,
  client_id     TEXT NOT NULL,
  client_secret TEXT NOT NULL,
  enabled       BOOLEAN NOT NULL DEFAULT TRUE,
  created_at    TIMESTAMPTZ NOT NULL,
  updated_at    TIMESTAMPTZ NOT NULL
);
CREATE TABLE pending_transfers (
  token_hash       BYTEA PRIMARY KEY,
  target_user_id   TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  provider         TEXT NOT NULL,
  provider_subject TEXT NOT NULL,
  source_user_id   TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  expires_at       TIMESTAMPTZ NOT NULL,
  created_at       TIMESTAMPTZ NOT NULL
);
CREATE INDEX idx_pending_transfers__expires_at ON pending_transfers (expires_at);
CREATE TABLE user_notices (
  id         TEXT PRIMARY KEY,
  user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  text       TEXT NOT NULL,
  severity   TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX idx_user_notices__user_id ON user_notices (user_id);
CREATE TABLE data_deletion_requests (
  confirmation_code TEXT PRIMARY KEY,
  provider          TEXT NOT NULL,
  provider_subject  TEXT NOT NULL,
  user_id           TEXT,
  status            TEXT NOT NULL,
  created_at        TIMESTAMPTZ NOT NULL
);
CREATE INDEX idx_data_deletion_requests__created_at
  ON data_deletion_requests (created_at DESC);
```

Notes baked into the schema (verify during Step 4 if anything fails):
- `gbfs_stations.is_virtual_station`, `gbfs_station_status.is_installed/is_renting/is_returning`, `gbfs_pricing_plans.is_taxable` were `INTEGER` in SQLite. Confirm the GBFS loader writes `bool` (or 0/1); if it writes `i64`, Task 6 updates the loader binds to `bool`.
- `feedback.handled` / `dismissed`, `oauth_states.link`, `auth_providers.enabled` → `BOOLEAN`.
- `live_feed_metadata.base_midnight` is `BIGINT` (epoch seconds, not a timestamp).
- The `gtfs_stop_times` `arrival_time_seconds` / `departure_time_seconds` generated-column expressions use `split_part` (immutable). If Step 4 rejects them, the fallback is the original `substr(...,1,2)::int` form — both are portable; `split_part` was chosen for readability.

- [ ] **Step 4: Write `init.down.sql`**

Replace `backend/migrations/<ts>_init.down.sql` with:

```sql
DROP TABLE IF EXISTS data_deletion_requests;
DROP TABLE IF EXISTS user_notices;
DROP TABLE IF EXISTS pending_transfers;
DROP TABLE IF EXISTS link_tickets;
DROP TABLE IF EXISTS oauth_states;
DROP TABLE IF EXISTS user_settings;
DROP TABLE IF EXISTS user_sessions;
DROP TABLE IF EXISTS user_oauth_identities;
DROP TABLE IF EXISTS users;
DROP TABLE IF EXISTS feedback;
DROP TABLE IF EXISTS gbfs_rental_hours;
DROP TABLE IF EXISTS gbfs_pricing_plans;
DROP TABLE IF EXISTS gbfs_regions;
DROP TABLE IF EXISTS gbfs_station_status;
DROP TABLE IF EXISTS gbfs_stations;
DROP TABLE IF EXISTS gbfs_vehicle_types;
DROP TABLE IF EXISTS gbfs_system_information;
DROP TABLE IF EXISTS admin_metadata;
DROP TABLE IF EXISTS admin_settings;
DROP TABLE IF EXISTS live_vehicles;
DROP TABLE IF EXISTS live_feed_metadata;
DROP TABLE IF EXISTS live_trip_stop_times;
DROP TABLE IF EXISTS live_trips;
DROP TABLE IF EXISTS gtfs_stop_times;
DROP TABLE IF EXISTS gtfs_feed_info;
DROP TABLE IF EXISTS gtfs_transfers;
DROP TABLE IF EXISTS gtfs_frequencies;
DROP TABLE IF EXISTS gtfs_trips;
DROP TABLE IF EXISTS gtfs_shapes;
DROP TABLE IF EXISTS gtfs_fare_rules;
DROP TABLE IF EXISTS gtfs_fare_attributes;
DROP TABLE IF EXISTS service_combinations;
DROP TABLE IF EXISTS service_combo_ids;
DROP TABLE IF EXISTS gtfs_calendar_dates;
DROP TABLE IF EXISTS gtfs_calendar;
DROP TABLE IF EXISTS gtfs_routes;
DROP TABLE IF EXISTS gtfs_stops;
DROP TABLE IF EXISTS gtfs_agency;
DROP TABLE IF EXISTS gtfs_payment_methods;
DROP TABLE IF EXISTS gtfs_transfer_types;
DROP TABLE IF EXISTS gtfs_pickup_dropoff_types;
DROP TABLE IF EXISTS gtfs_directions;
DROP TABLE IF EXISTS gtfs_route_types;
DROP TABLE IF EXISTS gtfs_location_types;
DROP TABLE IF EXISTS gtfs_schedule_meta;
```

- [ ] **Step 5: Apply the migration to the dev PG and verify it succeeds**

From `backend/`:
```bash
DATABASE_URL=postgres://postgres:mysecretpassword@localhost:5435/postgres \
  sqlx migrate run
```
Expected: no output (success). If it fails:
- "extension postgis does not exist" → you accidentally used a PostGIS type; remove it (we deferred PostGIS).
- "generation expression is not immutable" → switch the offending generated-column expression to `substr(col,1,2)::int * 3600 + substr(col,4,2)::int * 60 + substr(col,7,2)::int` (equivalent, also immutable).
- Anything about `CHECK (x LIKE '__:__:__')` → unlikely; PG LIKE treats `_` as a wildcard just like SQLite.

Verify the schema landed (optional sanity check):
```bash
docker exec "$(docker ps --filter 'ancestor=postgres' --format '{{.ID}}' | head -1)" \
  psql -U postgres -d postgres -c "\dt"
```
Expected: ~40 tables listed.

- [ ] **Step 6: `cargo check` (expected failure — baseline)**

From `backend/`:
```bash
cargo check 2>&1 | tail -5
```
Expected: fails. The old `.sqlx/` cache is SQLite-flavored and the Rust code still references `Sqlite*` types. Note the error count for comparison after later tasks. This is fine — the codebase does not compile again until Task 8.

- [ ] **Step 7: Stage for commit**

```bash
git add backend/migrations/
git status
```
Show the user the staged migration files and pause. (The user commits when ready.)

---

### Task 2: Swap Cargo deps

**Files:**
- Modify: `backend/Cargo.toml`

**Interfaces:**
- Produces: a `Cargo.toml` whose `[dependencies]` enable sqlx's `postgres` backend and pull in `time` + `rust_decimal` as bridge crates. Later Rust tasks assume these are available.

- [ ] **Step 1: Edit the sqlx line**

In `backend/Cargo.toml`, change:
```toml
sqlx = { version = "0.9", features = ["runtime-tokio", "sqlite", "macros"] }
```
to:
```toml
sqlx = { version = "0.9", features = ["runtime-tokio", "postgres", "macros", "time", "json", "rust_decimal"] }
```

- [ ] **Step 2: Add the bridge crates**

In the same `[dependencies]` block (alphabetical placement is fine), add:
```toml
rust_decimal = { version = "1", features = ["serde"] }
time = { version = "0.3", features = ["serde"] }
```

- [ ] **Step 3: Update the lockfile**

From `backend/`:
```bash
cargo update -p sqlx && cargo fetch
```
Expected: `Cargo.lock` updated to pull in `sqlx-postgres`, `time`, `rust_decimal`, etc. If `cargo fetch` complains about `jiff`, ignore — we didn't touch it.

- [ ] **Step 4: `cargo check` (expected failure, but error shape changes)**

```bash
cargo check 2>&1 | tail -20
```
Expected: fails. The error type shifts from "sqlite feature missing" / type errors to "SqlitePool not found" (the `sqlite` feature is gone). This confirms the dep swap landed.

- [ ] **Step 5: Stage for commit**

```bash
git add backend/Cargo.toml backend/Cargo.lock
git status
```

---

### Task 3: Rewrite the database layer (pool, URL parser, int enums, time bridge)

This task does the infrastructure pieces that don't require touching individual query call sites. After it, `cargo check` still fails (stale cache + query call sites untouched), but the *infrastructure* for the later tasks is in place.

**Files:**
- Modify: `backend/src/database/mod.rs`
- Modify: `backend/src/cli/mod.rs`
- Modify: `backend/src/proto/gtfs_schedule/data/trip.rs`
- Modify: `backend/src/proto/gtfs_schedule/data/route.rs`
- Modify: `backend/src/proto/gtfs_schedule/data/stop.rs`
- Create: `backend/src/database/time.rs`
- Delete: `backend/src/database/sqlx_types.rs`

**Interfaces:**
- Produces:
  - `Database::init(url: &DatabaseUrl) -> anyhow::Result<PgPool>` and `Database::pool() -> PgPool`.
  - `struct DatabaseUrl(String)` with `TryFrom<&str>` / value parser accepting only `postgres://` / `postgresql://`, plus `Display` and a password-masking `Debug`.
  - `crate::database::time::{now, from_jiff, to_jiff}` bridge helpers (signatures in Step 5).

- [ ] **Step 1: Rewrite `backend/src/database/mod.rs`**

Replace the file's entire contents with:

```rust
use std::sync::OnceLock;

use sqlx::{
    PgPool,
    postgres::PgPoolOptions,
};
use tracing::{debug, trace, warn};

use std::time::{Duration, Instant};

use crate::cli::DatabaseUrl;

pub mod time;

static DATABASE: OnceLock<PgPool> = OnceLock::new();

const SLOW_THRESHOLD: Duration = Duration::from_millis(30);

pub struct Database;

impl Database {
    pub async fn init(url: &DatabaseUrl) -> anyhow::Result<PgPool> {
        let connection_string = url.as_str();

        debug!(?connection_string, "Initializing database");

        let pool = PgPoolOptions::new()
            .max_connections(20)
            .connect(connection_string)
            .await?;

        sqlx::migrate!("./migrations").run(&pool).await?;

        DATABASE
            .set(pool.clone())
            .map_err(|_| anyhow::anyhow!("Failed to initialize database, pool already set"))?;

        debug!("Database initialized");

        Ok(pool)
    }

    pub fn pool() -> PgPool {
        DATABASE.get().expect("Database not initialized").clone()
    }
}

impl Database {
    pub async fn logged<F, T>(label: &str, fut: F) -> T
    where
        F: std::future::Future<Output = T>,
    {
        let start = Instant::now();
        let result = fut.await;
        let elapsed = start.elapsed();
        trace!(target: "query", query = label, ?elapsed, "query executed");
        if elapsed > SLOW_THRESHOLD {
            warn!(target: "query", query = label, ?elapsed, "slow query");
        }
        result
    }
}
```

Changes vs. the old file: deleted `CONNECTION_PRAGMAS`, `configure_sqlite_connection`, `Database::optimize`, `run_optimize_periodically`, the `after_connect` callback, the `DatabaseUrl::Memory`/`Local` path-mangling branch, the `AssertSqlSafe` import, and the `pub mod sqlx_types;` declaration (the file is deleted in Step 3). `SqlitePool`/`SqliteConnection`/`SqlitePoolOptions` → `PgPool`/`PgPoolOptions`.

- [ ] **Step 2: Rewrite `DatabaseUrl` in `backend/src/cli/mod.rs`**

(a) Replace the struct-field doc + attribute (around line 157–161):

```rust
    /// The `PostgreSQL` database URL to use.
    ///
    /// Must be a libpq-style URL, e.g. `postgres://user:pass@host:5432/dbname`.
    /// Required: the server will not start without it.
    #[clap(long, env = "DATABASE_URL", value_parser = DatabaseUrl::try_from_string)]
    pub database_url: DatabaseUrl,
```
(The `default_value = ":memory:"` is removed — the field is now required.)

(b) Replace the entire `DatabaseUrl` enum + impls (lines 209–238) with:

```rust
#[derive(Debug, Clone)]
pub struct DatabaseUrl(String);

impl DatabaseUrl {
    pub fn try_from_string(s: &str) -> Result<Self, String> {
        let url = url::Url::parse(s)
            .map_err(|e| format!("Invalid database URL: {e}"))?;
        match url.scheme() {
            "postgres" | "postgresql" => Ok(Self(s.to_string())),
            other => Err(format!(
                "Invalid database URL scheme `{other}` (expected `postgres://` or `postgresql://`)"
            )),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for DatabaseUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
```

Note on password masking: the derive-`Debug` above will print the password. If clippy or a reviewer wants masking, implement `Debug` manually to redact the userinfo. Skip unless asked.

Also remove the now-unused `use std::path::PathBuf;` import if it was only used by the old enum (check the top of `cli/mod.rs`).

- [ ] **Step 3: Drop the int-enum macro and switch reprs**

Delete `backend/src/database/sqlx_types.rs`. Remove its declaration from `backend/src/database/mod.rs` (the `pub mod sqlx_types;` line). If any code still references `sqlx_int_enum_decode!`, that's a compile error this task fixes next.

In `backend/src/proto/gtfs_schedule/data/trip.rs`, `route.rs`, `stop.rs`:
- Remove the `use crate::{..., sqlx_int_enum_decode};` import (keep `BulkInsert`).
- Remove the `sqlx_int_enum_decode!(...)` invocation following each enum.
- Change each enum's `#[repr(u8)]` to `#[repr(i32)]`.

Example for `trip.rs` `Direction` (around line 57):
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr, sqlx::Type)]
#[repr(i32)]
pub enum Direction {
    Default = 0,
    Outbound = 1,
    Inbound = 2,
    Clockwise = 3,
    Anticlockwise = 4,
}
// (the `sqlx_int_enum_decode!(Direction, ...)` block that was here is deleted)
```
Repeat for `BikesAllowed` (trip.rs), `RouteType`/`PickupType`/`DropOffType` (route.rs), `LocationType`/`WheelchairBoarding` (stop.rs) — 7 enums total.

sqlx `#[derive(sqlx::Type)]` on a `#[repr(i32)]` enum emits `INT4` type info directly, which matches the `SMALLINT`/`INTEGER` columns. (`SMALLINT` decodes to `i32` in sqlx; no `i16` cast needed because the derive uses the enum's repr.)

- [ ] **Step 4: Create `backend/src/database/time.rs`**

```rust
//! Bridge between `jiff` (application time) and `time` (sqlx Postgres
//! `timestamptz` decode/encode). The DB layer speaks `time::OffsetDateTime`;
//! the rest of the app speaks `jiff`. Convert at the boundary.

use jiff::Timestamp;

pub fn now() -> time::OffsetDateTime {
    time::OffsetDateTime::now_utc()
}

pub fn from_jiff(t: Timestamp) -> time::OffsetDateTime {
    let nanos = t.as_nanosecond();
    time::OffsetDateTime::from_unix_timestamp_nanos(nanos)
        .expect("jiff timestamp fits in time::OffsetDateTime range")
}

pub fn to_jiff(t: time::OffsetDateTime) -> Timestamp {
    let nanos = t.unix_timestamp_nanos();
    Timestamp::from_nanosecond(nanos).unwrap_or_else(|_| {
        // Fallback for out-of-range dates (extremely unlikely in this app).
        Timestamp::default()
    })
}

/// Convenience for API response structs: parse an ISO-8601 string that the
/// frontend round-trips through jiff's serde. Use when a query decodes
/// `OffsetDateTime` but the response type wants `Timestamp` for serde.
pub fn parse_iso(s: &str) -> Option<Timestamp> {
    s.parse::<Timestamp>().ok()
}
```

Note: verify `jiff::Timestamp::as_nanosecond` is the correct method name in jiff 0.2.28 — if it's `as_nanosecond_ranged` or similar, adjust. The intent is: read the timestamp as integer nanoseconds since epoch. `time::OffsetDateTime::from_unix_timestamp_nanos` returns `Result`; the `.expect` is acceptable because every jiff `Timestamp` is in-range for `time`.

- [ ] **Step 5: `cargo check` (expected failure — but new shape)**

```bash
cargo check 2>&1 | tail -20
```
Expected: fails. Infrastructure compiles, but the query macros and call sites (booleans, jsonb, timestamps, `GROUP_CONCAT`, dynamic `IN`) still reference the old types/SQL. The error count should be lower than Task 2's baseline. Note any error originating in `database/mod.rs`, `cli/mod.rs`, or the enum files — those must be zero before proceeding (fix forward).

- [ ] **Step 6: Stage for commit**

```bash
git add backend/src/database/ backend/src/cli/mod.rs backend/src/proto/gtfs_schedule/data/
git status
```

---

### Task 4: Convert dynamic `IN (...)` builders to `= ANY($1)` macros (template task)

This is the most novel task — do it first among the call-site rewrites so it serves as the template (spec §14 risk #5). 7 sites total.

**Files:**
- Modify: `backend/src/server/routes/v1/schedule/mod.rs` (sites ~253-262, 686-688, 752-754, 813-816)
- Modify: `backend/src/server/routes/v1/mod.rs` (sites ~690, 756, 819)

**Interfaces:**
- Consumes: `Database::pool()`, the schema from Task 1.
- Produces: 7 query sites converted from `format!("... IN ({})", placeholders)` + `AssertSqlSafe` + `.bind()` loop → a `query_as!` / `query!` macro with `col = ANY($1)` and a single `&[T]` bind.

**Conversion pattern (apply to each site):**

Old shape:
```rust
let sql = format!(
    "SELECT ... WHERE col IN ({}) ...",
    items.iter().map(|_| "?").collect::<Vec<_>>().join(", "),
);
let mut q = sqlx::query_as::<_, Row>(AssertSqlSafe(sql));
for item in &items {
    q = q.bind(item.clone());
}
let rows = q.fetch_all(&Database::pool()).await?;
```

New shape:
```rust
let rows = sqlx::query_as!(
    Row,
    "SELECT ... WHERE col = ANY($1) ...",
    &items,
)
.fetch_all(&Database::pool())
.await?;
```

Key rules:
- `ANY($1)` consumes exactly one macro-bind slot. If the query has other params (e.g. `LIMIT $2`, or another `= ANY($2)`), bind them in order after `&items`.
- The bind type is `&[String]` / `&Vec<String>` for text columns; sqlx encodes it as a PG `text[]`. For integer columns, use `&[i64]` etc.
- `AssertSqlSafe(...)` wrapper is removed — `query_as!` takes a string literal directly.
- If the struct was defined inline (`#[derive(FromRow)] struct Row {...}`), it stays inline; `query_as!` generates its own row decode via the macro (no `FromRow` derive needed, but keeping it is harmless).
- **Empty `items` case:** `= ANY($1)` with an empty array matches nothing (correct — same as `IN ()` which is a SQL syntax error in many engines; the SQLite version would have produced `IN ()` and also matched nothing or errored). Verify behavior: if the code path can't tolerate zero rows, the caller already handles `rows.is_empty()`. Don't add new guards.

- [ ] **Step 1: Convert the `get_stop_trips` site in `schedule/mod.rs` (~line 240-290)**

Read the function around lines 240-290. The current code builds `IN ({})` from `query.stop` (a `Vec<String>` of stop IDs). Convert:

```rust
// BEFORE (lines ~248-283):
let sql = format!(
    "
    SELECT ...
    WHERE gst.stop_id IN ({})
    ORDER BY gst.stop_sequence
    ",
    query.stop.iter().map(|_| "?").collect::<Vec<_>>().join(", "),
);
let mut q = {
    #[derive(Debug, FromRow)]
    struct StopTripRow { ... }
    sqlx::query_as::<_, StopTripRow>(AssertSqlSafe(sql))
};
for stop in &query.stop {
    q = q.bind(stop.clone());
}
let rows = match Database::logged("get_stop_trips", q.fetch_all(&Database::pool())).await { ... };
```

```rust
// AFTER:
let rows = {
    #[derive(Debug)]
    struct StopTripRow {
        vehicle_id: String,
        trip_id: String,
        route_id: String,
        stop_id: String,
        stop_sequence: i32,
        next_stop_sequence: Option<i32>,
        live_arrival_time: Option<i64>,
        live_arrival_delay: Option<i64>,
        arrival_time_seconds: Option<i64>,
        effective_delay: Option<i64>,
    }
    sqlx::query_as!(
        StopTripRow,
        "
        SELECT lv.vehicle_id,
               lv.trip_id,
               lv.route_id,
               gst.stop_id,
               gst.stop_sequence,
               lv.next_stop_sequence,
               lst.arrival_time   AS live_arrival_time,
               lst.arrival_delay  AS live_arrival_delay,
               gst.arrival_time_seconds,
               COALESCE(lst.arrival_delay, 0)
                   + COALESCE(gst.arrival_time_seconds, 0) - COALESCE(gst.arrival_time_seconds, 0) AS effective_delay
        FROM gtfs_stop_times gst
        JOIN live_vehicles lv ON lv.trip_id = gst.trip_id
        LEFT JOIN live_trip_stop_times lst
            ON  lst.trip_id = lv.trip_id
            AND lst.stop_sequence = gst.stop_sequence
        WHERE gst.stop_id = ANY($1)
        ORDER BY gst.stop_sequence
        ",
        &query.stop,
    )
};
let rows = match Database::logged("get_stop_trips", rows.fetch_all(&Database::pool())).await { ... };
```

Notes:
- `u32` fields in the struct become `i32` (PG `INTEGER` decodes to `i32`; sqlx macro will flag `u32` as a type mismatch). Apply this `u32`→`i32` change to every converted struct.
- The exact `SELECT` column list must match what's there now (the `effective_delay` COALESCE expression above is illustrative — copy the *actual* current expression from the file). Do not invent SQL; preserve semantics.
- The `Database::logged` wrapper still works — it takes the future, not the query, so wrap `rows.fetch_all(...)` as shown.
- If the macro complains `expected `&[String]`, found `Vec<String>`", bind `&query.stop[..]` or `&query.stop as &[String]`.

- [ ] **Step 2: Convert the other 3 sites in `schedule/mod.rs`**

Apply the same pattern to the three remaining `format!("... IN ({})", ...)` sites (around lines 686-688, 752-754, 813-816). For each:
- Read the current SQL string and column list.
- Rewrite as a `query_as!`/`query!` macro with `= ANY($1)` and `&items` bind.
- Drop `AssertSqlSafe` and the `.bind()` loop.
- Flip `u32` → `i32` in any inline row struct.

- [ ] **Step 3: Convert the 3 sites in `v1/mod.rs`**

Sites are around lines 690, 756, 819 (per the spec inventory). Same conversion pattern. These are in the GTFS-RT realtime writer path (`live_*` table bulk inserts use `IN` for filtering trip IDs / route IDs).

- [ ] **Step 4: `cargo check` (expected failure — cache stale)**

```bash
cargo check 2>&1 | tail -20
```
Expected: still fails. The macros now reference `$1` placeholders and PG column types, but the `.sqlx/` cache is still the SQLite one. Errors should shift to "query XYZ does not match any cached statement" or type mismatches against the (stale) cache. That's correct — Task 8 regenerates the cache and these resolve.

What MUST be true before proceeding: zero errors of the form "method not found", "expected macro `query_as!`", or Rust-level syntax/type errors in these 7 sites. Only sqlx-cache-related errors are acceptable here.

- [ ] **Step 5: Stage for commit**

```bash
git add backend/src/server/routes/v1/schedule/mod.rs backend/src/server/routes/v1/mod.rs
git status
```

---

### Task 5: SQL dialect rewrites (`GROUP_CONCAT`, `VACUUM`, `AssertSqlSafe`)

**Files:**
- Modify: `backend/src/auth/accounts.rs` (~lines 507, 544)
- Modify: `backend/src/proto/gtfs_schedule/data/mod.rs` (~line 303, ~line 42)
- Modify: any remaining `AssertSqlSafe` site found by grep

- [ ] **Step 1: `GROUP_CONCAT` → `string_agg`**

In `backend/src/auth/accounts.rs`, both `list_users` (~line 507) and `user_summary_by_id` (~line 544) contain:
```sql
(SELECT GROUP_CONCAT(provider, ',' ORDER BY created_at)
 FROM user_oauth_identities WHERE user_id = u.id)
```
Change to:
```sql
(SELECT string_agg(provider, ',' ORDER BY created_at)
 FROM user_oauth_identities WHERE user_id = u.id)
```
sqlx's `string_agg(..., ',')` returns `text`, which decodes as `String` — the `AS "providers!: String"` annotation still works. (PG's `string_agg` arg order is `(value, delimiter)`, same as SQLite's `GROUP_CONCAT`.)

- [ ] **Step 2: Remove `VACUUM`**

In `backend/src/proto/gtfs_schedule/data/mod.rs` around line 303:
```rust
sqlx::query!("VACUUM").execute(&Database::pool()).await?;
```
Delete the line entirely. PG `VACUUM` cannot run via a prepared statement (which is what `sqlx::query!` produces) nor inside a transaction; PG autovacuum handles the post-bulk-load case. If you want a re-stat hint for the planner, replace with:
```rust
sqlx::query!("ANALYZE").execute(&Database::pool()).await?;
```
`ANALYZE` is safe in a transactionless context and refreshes planner stats after the bulk GTFS load. Prefer the `ANALYZE` replacement unless the user says otherwise.

- [ ] **Step 3: Remove remaining `AssertSqlSafe` usages**

```bash
rg -n 'AssertSqlSafe' backend/src/
```
Expected hits (per spec inventory): `database/mod.rs` (already removed in Task 3), `gtfs_schedule/data/mod.rs:42`, `schedule/mod.rs:279` (already removed in Task 4), `v1/mod.rs:690,756,819` (already removed in Task 4).

The remaining one is `gtfs_schedule/data/mod.rs:42` — the bulk-load `DELETE FROM {table}` site:
```rust
sqlx::query(AssertSqlSafe(&format!("DELETE FROM {table}"))).execute(&Database::pool()).await?;
```
This is a runtime-built query (table name interpolation, not a value bind). `AssertSqlSafe` is the SQLite-only wrapper; for PG use plain `sqlx::query`:
```rust
sqlx::query(&format!("DELETE FROM {table}")).execute(&Database::pool()).await?;
```
Safety note: `{table}` is interpolated, not bound (you can't bind a table name as a param in any DB). The current code already trusts `table` to be a safe identifier (it comes from a compile-time table list in the GTFS loader). Preserve that invariant — do not introduce user input here.

If the `AssertSqlSafe` import line (`use sqlx::AssertSqlSafe;` or `use sqlx::{AssertSqlSafe, ...}`) becomes unused, remove it from the imports.

- [ ] **Step 4: `cargo check` (expected failure — cache stale)**

```bash
cargo check 2>&1 | tail -10
```
Expected: still fails on cache, but no new Rust-level errors from this task's edits.

- [ ] **Step 5: Stage for commit**

```bash
git add backend/src/auth/accounts.rs backend/src/proto/gtfs_schedule/data/mod.rs
git status
```

---

### Task 6: Type-mapping call sites (booleans, `jsonb`, `rust_decimal`)

**Files:**
- Modify: `backend/src/admin/router.rs` (~lines 329-330, 399-400)
- Modify: `backend/src/admin/feedback.rs` (~lines 100, 133)
- Modify: `backend/src/admin/metadata.rs` (~lines 75-135)
- Modify: `backend/src/admin/settings.rs`
- Modify: `backend/src/server/routes/v1/settings.rs`
- Modify: `backend/src/proto/gbfs/data/system_pricing_plans.rs` (line 25)
- Modify: any GBFS loader writing the boolean columns (`is_installed`, `is_renting`, `is_returning`, `is_virtual_station`, `is_taxable`) — enumerate via grep in Step 4.

- [ ] **Step 1: Boolean rewrites in `admin/router.rs`**

Around line 329-330 (the auth-provider upsert):
```rust
// BEFORE:
let now = jiff::Timestamp::now().to_string();
let enabled_i = i64::from(body.enabled);
// ... later in the query bind: enabled_i
```
```rust
// AFTER:
let now = crate::database::time::now();
// ... in the query bind: body.enabled   (bool, not i64)
```
Drop the `enabled_i` binding. (The `now` change is the timestamp bridge — Task 7 formalizes this; doing it here is fine since you're already in the file.)

Around line 399-400 (the auth-provider update):
```rust
// BEFORE:
let enabled_i = body.enabled.map(i64::from);
```
```rust
// AFTER:
let enabled = body.enabled;   // Option<bool>
```
And change the bind site from `enabled_i` to `enabled`.

In the SQL of both queries, if there's a `WHERE enabled = ?` or `SET enabled = ?` style clause, no SQL change is needed — the bind type changes from `i64` to `bool`, which sqlx maps to PG `boolean` automatically.

- [ ] **Step 2: Boolean rewrites in `admin/feedback.rs`**

Around line 100 (unhandled feedback list):
```sql
-- BEFORE:
WHERE f.handled = 0 AND f.dismissed = 0 AND f.reply IS NULL
-- AFTER:
WHERE NOT f.handled AND NOT f.dismissed AND f.reply IS NULL
```
Around line 133 (handled/dismissed/replied list):
```sql
-- BEFORE:
WHERE f.handled = 1 OR f.dismissed = 1 OR f.reply IS NOT NULL
-- AFTER:
WHERE f.handled OR f.dismissed OR f.reply IS NOT NULL
```
Also check for any bind of `handled`/`dismissed` as `i64` (e.g. `SET handled = ?` updates) — change the Rust bind from `i64`/`0`/`1` to `bool`/`false`/`true`.

- [ ] **Step 3: `jsonb` rewrites in `admin/metadata.rs` and `admin/settings.rs`**

In `admin/metadata.rs` around lines 75-135:
- `MetadataEntry.last_sync_at: jiff::Timestamp` (line 13) — this is a struct field. The DB write currently does `serde_json::to_string(entry)` (line 80) bound to the `value` BLOB column, and the read does `serde_json::from_slice(x.value.as_slice())` (line 132).
- The `value` column is now `jsonb`. Switch the bind from `serde_json::to_string(entry)?` to passing `serde_json::to_value(entry)?` (or `serde_json::Value::Object(...)`) directly — sqlx encodes `serde_json::Value` to `jsonb`.
- Switch the read from `serde_json::from_slice(x.value.as_slice())` to `x.value` directly (the macro decodes `jsonb` to `serde_json::Value` when the column is annotated, or via `as "value: serde_json::Value"`).

Concretely, if the query is:
```rust
sqlx::query!("UPDATE admin_metadata SET value = ?, updated_at = ? WHERE name = ?", bytes, now, name)
```
change to:
```rust
sqlx::query!(
    "UPDATE admin_metadata SET value = $1, updated_at = $2 WHERE name = $3",
    serde_json::to_value(&entry)?,          // serde_json::Value, binds to jsonb
    crate::database::time::now(),
    name,
)
```
and the read:
```rust
// BEFORE:
let x = sqlx::query!("SELECT value FROM admin_metadata WHERE name = ?", name).fetch_one(...).await?;
let entry: MetadataEntry = serde_json::from_slice(x.value.as_slice())?;
// AFTER:
let x = sqlx::query!(
    "SELECT value AS \"value!: serde_json::Value\" FROM admin_metadata WHERE name = $1",
    name,
).fetch_one(...).await?;
let entry: MetadataEntry = serde_json::from_value(x.value)?;
```

Apply the same pattern to `admin_settings.value` in `admin/settings.rs` and to `user_settings.settings` in `server/routes/v1/settings.rs`.

- [ ] **Step 4: GBFS boolean + decimal loader sites**

Find every site writing to the now-boolean GBFS columns:
```bash
rg -n 'is_installed|is_renting|is_returning|is_virtual_station|is_taxable' backend/src/proto/gbfs/
```
For each INSERT bind, the Rust side likely produces `i64`/`u8`/`usize` from the GBFS JSON. Change to `bool`. Example in the GBFS loader: `is_installed: status.is_installed.unwrap_or(1)` → `is_installed: status.is_installed.map(|v| v != 0).unwrap_or(false)`. (Adjust to the actual current code shape.)

For `system_pricing_plans.rs` line 25:
```rust
// BEFORE:
pub price: Option<f64>,
// AFTER:
pub price: Option<rust_decimal::Decimal>,
```
If the struct is populated from GBFS JSON (not a sqlx row decode), add a conversion at the populate site: `rust_decimal::Decimal::try_from(json_price).ok()` or parse from string. Check the file's INSERT site (~line 68-79) — `plan.price` is now `Option<Decimal>`, which sqlx binds to `numeric(19,4)` natively.

If the struct is ALSO used to serialize to the frontend API, `rust_decimal` with its `serde` feature serializes as a JSON number by default (or string, depending on cfg) — verify the frontend accepts the format. If the frontend expected a float, you may need `#[serde(with = "rust_decimal::serde::float")]` on the field.

- [ ] **Step 5: `cargo check` (expected failure — cache stale)**

```bash
cargo check 2>&1 | tail -10
```
Expected: still cache-stale fails; no new Rust-level errors from this task.

- [ ] **Step 6: Stage for commit**

```bash
git add backend/src/admin/ backend/src/server/routes/v1/settings.rs backend/src/proto/gbfs/
git status
```

---

### Task 7: Timestamp migration (apply the bridge at every call site)

The largest mechanical task. The bridge (`database::time`) was created in Task 3; now every call site switches from `jiff::Timestamp::now().to_string()` (String bind to TEXT) to `database::time::now()` (`OffsetDateTime` bind to `timestamptz`), and every `*_at` column read switches from `String` decode to `OffsetDateTime` decode.

**Files:**
- Modify: every file under `backend/src/` containing `jiff::Timestamp::now().to_string()` or reading an `*_at` column. Per spec §9: `admin/{feedback,user_notices,metadata,router,mod}.rs`, `auth/{oauth,accounts,session,config}.rs`, `server/routes/v1/{feedback/mod,settings,auth/mod}.rs`, `proto/gtfs_schedule/{fetcher,data/mod}.rs`, plus `server/routes/v1/schedule/mod.rs` (the `now.as_second()` sites are NOT changed — they read wall-clock seconds for delay math, not DB columns).

**Interfaces:**
- Produces: every timestamp DB bind is `time::OffsetDateTime`; every timestamp DB read decodes as `time::OffsetDateTime`. API-response structs that previously held `String` ISO timestamps hold `jiff::Timestamp` instead (converted via `database::time::to_jiff` at the populate site) so the frontend wire format is unchanged.

**Transformation rules:**

**Rule A — "now" binds (write side):**
```rust
// BEFORE (anywhere):
let now = jiff::Timestamp::now().to_string();
sqlx::query!("... SET created_at = ?", now)
// AFTER:
let now = crate::database::time::now();   // time::OffsetDateTime
sqlx::query!("... SET created_at = $1", now)
```
(The `?` → `$1` swap happens automatically when the macro is regenerated; the source change is the bind value type.)

**Rule B — `now_iso()` / `expires_iso()` helpers (`auth/session.rs` lines 32-44):**
```rust
// BEFORE:
fn now_iso() -> String { jiff::Timestamp::now().to_string() }
fn expires_iso(max_age: Duration) -> String {
    let now = jiff::Timestamp::now();
    let secs = now.as_second().saturating_add(...);
    jiff::Timestamp::from_second(secs).unwrap_or(now).to_string()
}
// AFTER:
fn now() -> time::OffsetDateTime { crate::database::time::now() }
fn expires(max_age: Duration) -> time::OffsetDateTime {
    time::OffsetDateTime::now_utc().saturating_add(time::Duration::try_from(max_age).expect("max_age fits"))
}
```
Update every call site of `now_iso()` / `expires_iso()` in `session.rs` to use the new names + bind the `OffsetDateTime` directly (no `.to_string()`).

**Rule C — `*_at` column reads (decode side):**
If a query currently has `AS "created_at!: String"`, change to decode as `time::OffsetDateTime`:
```rust
// BEFORE:
SELECT u.created_at AS "created_at!: String" ...
// AFTER:
SELECT u.created_at AS "created_at!: time::OffsetDateTime" ...
```
Then convert to jiff at the struct-population site:
```rust
// BEFORE:
.map(|u| UserSummary { created_at: u.created_at, ... })
// AFTER:
.map(|u| UserSummary {
    created_at: crate::database::time::to_jiff(u.created_at),
    ...
})
```
And change the `UserSummary.created_at` field type from `String` to `jiff::Timestamp`. This preserves the API JSON shape (jiff's serde format is what the frontend already consumes).

If a struct field is ONLY used internally (not serialized to the API), you can leave it as `time::OffsetDateTime` and skip the `to_jiff` call. Use judgment per site.

**Rule D — `gtfs_schedule_meta` epoch-float sites (`gtfs_schedule/fetcher.rs` ~lines 160-165, `gtfs_schedule/data/mod.rs` write site):**
```rust
// BEFORE (fetcher.rs): reads last_modified as f64 epoch, compares to jiff::Timestamp
let lm: Option<f64> = sqlx::query_scalar!("SELECT last_modified FROM gtfs_schedule_meta ...");
// AFTER:
let lm: Option<time::OffsetDateTime> = sqlx::query_scalar!(
    "SELECT last_modified FROM gtfs_schedule_meta ...",
);
// then convert the HTTP Last-Modified header (parsed via jiff::fmt::rfc2822) to OffsetDateTime for comparison:
let http_lm = database::time::from_jiff(parsed_rfc2822_timestamp);
if let Some(db_lm) = lm { if db_lm == http_lm { /* skip fetch */ } }
```
And the write side: the column now accepts `OffsetDateTime` directly (or omit `fetched_at` to use `DEFAULT now()`).

- [ ] **Step 1: Convert `auth/session.rs` helpers + call sites**

Apply Rule B (rewrite `now_iso`/`expires_iso`) and update every caller in the file. Then apply Rule C to any `*_at` reads in the file (session `expires_at`, `created_at`).

- [ ] **Step 2: Convert the simple "now" write sites (Rule A)**

Apply Rule A to each of these files (each has a `jiff::Timestamp::now().to_string()` bind):
- `src/admin/feedback.rs` (~line 274)
- `src/admin/user_notices.rs` (~line 25)
- `src/admin/metadata.rs` (~line 79, already touched in Task 6 — reconcile)
- `src/admin/router.rs` (~lines 329, 399, already touched in Task 6 — reconcile)
- `src/admin/mod.rs` (~line 71 — note this one uses `jiff::Zoned::now().to_string()`; `database::time::now()` returns the UTC `OffsetDateTime` equivalent, which is correct for a `timestamptz` column)
- `src/auth/oauth.rs` (~line 109)
- `src/auth/accounts.rs` (~lines 59, 577 — the latter uses `jiff::Zoned::now()`)
- `src/server/routes/v1/feedback/mod.rs` (~line 132)
- `src/server/routes/v1/settings.rs` (~line 50)
- `src/server/routes/v1/auth/mod.rs` (~line 541)

For each: drop the `.to_string()`, bind the `OffsetDateTime` directly.

- [ ] **Step 3: Convert `*_at` read sites (Rule C)**

For each file in Step 2 that also READS an `*_at` column and exposes it (especially `accounts.rs` `UserSummary.created_at`, `user_notices.rs` `created_at`, `feedback.rs` `created_at`/`replied_at`):
- Change the `AS "...!: String"` annotation to `AS "...!: time::OffsetDateTime"` (or remove the `: String` and let the macro infer).
- Change the consuming struct field to `jiff::Timestamp` (if it's serialized) and convert via `database::time::to_jiff`.

Use `rg -n 'AS ".*!: String"' backend/src/` to find all such annotations and convert the timestamp ones (leave non-timestamp String columns alone).

- [ ] **Step 4: Convert the `gtfs_schedule` fetcher (Rule D)**

Edit `src/proto/gtfs_schedule/fetcher.rs` around lines 160-165: the `last_modified` read changes from `f64` epoch to `time::OffsetDateTime`, and the comparison logic switches to compare two `OffsetDateTime` values. Edit `src/proto/gtfs_schedule/data/mod.rs`'s `gtfs_schedule_meta` write site: bind `OffsetDateTime` (or omit `fetched_at`).

- [ ] **Step 5: `cargo check` (expected failure — cache stale)**

```bash
cargo check 2>&1 | tail -10
```
Expected: still fails on cache, but all Rust-level type errors should now be resolved. If there are remaining `jiff::Timestamp::now().to_string()` sites, `rg -n 'Timestamp::now\(\)\.to_string' backend/src/` and convert them.

- [ ] **Step 6: Stage for commit**

```bash
git add backend/src/
git status
```

---

### Task 8: Regenerate the `.sqlx` cache (the compile gate)

This is the gate. If Tasks 1–7 are correct, `cargo sqlx prepare` succeeds and the project compiles offline.

**Files:**
- Delete: all files under `backend/.sqlx/`
- Create: new `.sqlx/*.json` files generated by `cargo sqlx prepare`

- [ ] **Step 1: Wipe the stale cache**

```bash
rm -rf backend/.sqlx && mkdir -p backend/.sqlx
```

- [ ] **Step 2: Ensure the dev PG schema is current**

From `backend/` (PG is ephemeral; re-apply migrations to be safe):
```bash
DATABASE_URL=postgres://postgres:mysecretpassword@localhost:5435/postgres \
  sqlx database drop -y
DATABASE_URL=postgres://postgres:mysecretpassword@localhost:5435/postgres \
  sqlx database create
DATABASE_URL=postgres://postgres:mysecretpassword@localhost:5435/postgres \
  sqlx migrate run
```
Expected: clean recreation. If `database drop` fails because PG refuses (connections open), restart the docker container and retry.

- [ ] **Step 3: Regenerate the cache**

From `backend/`:
```bash
SQLX_OFFLINE= \
DATABASE_URL=postgres://postgres:mysecretpassword@localhost:5435/postgres \
  cargo sqlx prepare --workspace
```
Expected: `sqlx::query!` macros are validated against the live PG, and `.sqlx/query-*.json` files are written. Final line: `cargo sqlx prepare` succeeds.

**If it fails on a specific query**, read the error — common cases:
- "column X decoded as type Y, expected Z" → a Rust bind/decode type mismatch. Fix the source (the file/line is in the error), then re-run this step.
- "unsupported type" → likely a missing sqlx feature (e.g. `Decimal` without `rust_decimal` feature — verify Cargo.toml from Task 2).
- "syntax error at or near" → SQL dialect issue in a query (e.g. leftover `?` placeholder in a runtime-built string, or `GROUP_CONCAT`). Find via the error's query text, fix, re-run.
- A `query!` macro whose SQL you didn't touch still fails → its `?` placeholders need to become `$1..$N` in the cache; this is automatic, but if the *source* still uses `?` in a runtime-built `sqlx::query(...)` string (not a macro), that's a bug — convert to a macro or manually number the placeholders.

This step is iterative. Fix forward until green.

- [ ] **Step 4: `cargo check` offline (the real gate)**

```bash
SQLX_OFFLINE=true cargo check 2>&1 | tail -20
```
Expected: clean compile. This is the first task where `cargo check` passes.

- [ ] **Step 5: `just fmt-dev`**

From `backend/`:
```bash
just fmt-dev
```
Expected: clippy clean (or only pre-existing warnings unrelated to this migration), nightly rustfmt applied. If clippy flags new lints introduced by this migration (e.g. an unused import left after removing `AssertSqlSafe`), fix them. Do NOT add `#[allow]` without user permission.

- [ ] **Step 6: Stage for commit**

```bash
git add backend/.sqlx/ backend/src/
git status
```
This is the big "it compiles against PG" checkpoint. Pause here for the user to commit before continuing to docs/tooling.

---

### Task 9: Update `backend/justfile`

**Files:**
- Modify: `backend/justfile`

- [ ] **Step 1: Rewrite `sqlx-regenerate`**

Replace the `sqlx-regenerate` recipe (lines ~90-101) with:
```makefile
sqlx-regenerate:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -z "$DATABASE_URL" ]; then
      echo "DATABASE_URL must be set to a postgres:// URL"; exit 1;
    fi
    export SQLX_OFFLINE=""
    sqlx database drop -y
    sqlx database create
    sqlx migrate run
    cargo sqlx prepare --workspace
```

- [ ] **Step 2: Simplify `migrations-run` / drop `_migrations-ensure-db`**

Replace the `migrations-run` recipe and delete `_migrations-ensure-db` and `migrations-list`'s dependency on it (lines ~103-128):
```makefile
migrations-run:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -z "$DATABASE_URL" ]; then
      echo "DATABASE_URL is not set"; exit 1;
    fi
    sqlx migrate run

migrations-list:
    sqlx migrate info
```
(Remove the entire `_migrations-ensure-db` recipe — its sqlite-file-creation logic is obsolete.)

- [ ] **Step 3: Smoke-test the recipes**

From `backend/`:
```bash
DATABASE_URL=postgres://postgres:mysecretpassword@localhost:5435/postgres just migrations-list
```
Expected: lists the single `<ts>_init` migration as applied.

```bash
DATABASE_URL=postgres://postgres:mysecretpassword@localhost:5435/postgres just sqlx-regenerate
```
Expected: drops, recreates, migrates, and regenerates the cache cleanly (re-runs Task 8's logic via the recipe).

- [ ] **Step 4: Stage for commit**

```bash
git add backend/justfile
git status
```

---

### Task 10: Update `AGENTS.md` (backend + root)

**Files:**
- Modify: `backend/AGENTS.md`
- Modify: `AGENTS.md` (root)

- [ ] **Step 1: Backend `AGENTS.md`**

Update these sections:

(a) **Migrations section** — the line `just dev-run runs sqlx migrate run first and **requires DATABASE_URL to be set**` stays valid; no change needed there. Add a note that `DATABASE_URL` must be a `postgres://` URL and a running PG is required (no `:memory:`).

(b) **Environment (`backend/.env`)** — update the `DATABASE_URL` bullet:
```markdown
- `DATABASE_URL` — **required**, must be a `postgres://...` URL pointing at a
  running PostgreSQL instance. (Previously defaulted to `:memory:` with a
  SQLite dev file at `sqlite:./dev/db/db.sqlite` — both removed.)
```

(c) **Architecture → Database** — replace:
```markdown
- **Database**: libsql/SQLite via **sqlx**. Connection pool of 20 with WAL and a
  tuned PRAGMA block in `src/database/mod.rs`; `PRAGMA optimize` runs hourly.
```
with:
```markdown
- **Database**: PostgreSQL via **sqlx** (the `postgres` feature). Connection
  pool of 20 in `src/database/mod.rs`. Migrations are applied at startup by
  `sqlx::migrate!("./migrations")`. No PRAGMA/optimize step (PG autovacuum
  handles maintenance). Timestamps are `timestamptz`, decoded via the `time`
  crate and bridged to `jiff` via `src/database/time.rs`.
```

(d) **sqlx offline cache section** — append a note:
```markdown
The `.sqlx/` cache is Postgres-flavored. Regeneration requires a running PG
instance (`DATABASE_URL=postgres://... just sqlx-regenerate`) — there is no
in-process fallback.
```

- [ ] **Step 2: Root `AGENTS.md`**

In the "Docker / deploy" section, append:
```markdown
- **Database dependency:** the runtime image no longer has an in-process DB.
  The container requires `DATABASE_URL=postgres://...` at runtime (pointing at
  an external PostgreSQL instance). The `scratch` runner ships no DB. Cutover
  from the previous SQLite-on-a-volume setup requires a one-shot data
  migration per `docs/superpowers/specs/2026-08-03-move-to-postgres-design.md` §15.
```

- [ ] **Step 3: Stage for commit**

```bash
git add backend/AGENTS.md AGENTS.md
git status
```

---

### Task 11: Final verification (build + boot smoke test)

**Files:** none (verification only).

- [ ] **Step 1: Full backend build**

From `backend/`:
```bash
just build
```
Expected: clean release build (target `x86_64-unknown-linux-musl`, `SQLX_OFFLINE=true`). This is the strongest single signal that the migration is complete.

- [ ] **Step 2: Boot the server against the dev PG**

In one terminal, from `backend/`:
```bash
DATABASE_URL=postgres://postgres:mysecretpassword@localhost:5435/postgres \
ADMIN_KEY=test ADMIN_BIND_TO=127.0.0.1:9012 \
BIND_TO=127.0.0.1:9011 \
  cargo run --target x86_64-unknown-linux-musl -- server
```
Expected: server starts, three background fetchers spawn (GTFS-RT, GTFS schedule, GBFS), migrations apply, the server binds `127.0.0.1:9011` after the initial fetch. No panics.

Common boot failures and fixes:
- "error: relation `xyz` does not exist" → migration didn't apply fully; re-run `sqlx migrate run`.
- "unsupported type `numeric` for column ..." → `rust_decimal` feature missing or a bind type mismatch.
- "invalid input syntax for type timestamp" → a bind produced a string where `timestamptz` expected `OffsetDateTime` (leftover `.to_string()` somewhere).

- [ ] **Step 3: Smoke-test the live endpoints**

In another terminal:
```bash
# Frontend served:
curl -sI http://127.0.0.1:9011/ | head -1
# REST API:
curl -s http://127.0.0.1:9011/api/v1/routes | head -c 200
# WebSocket (use websocat if available, else skip):
echo | timeout 2 websocat ws://127.0.0.1:9011/api/v1/v1/ws 2>&1 | head -c 100 || true
```
Then open `http://127.0.0.1:9011/` in a browser (or `curl` the HTML) and confirm:
- The map loads.
- Vehicle positions stream (GTFS-RT path works → `live_vehicles` writes/reads OK).
- A stop click shows arrivals (`gtfs_stop_times` generated-column + `= ANY($1)` query work).
- Submit feedback (POST `/api/v1/feedback`) → confirms `feedback` INSERT with `boolean`/`timestamptz` binds works.
- (Optional, if OAuth configured) login flow — confirms `user_sessions`/`users` bytea + timestamptz round-trip.

If any endpoint errors, check the server logs — the failing query is the one to fix (re-open the relevant task).

- [ ] **Step 4: Final `just fmt-dev` pass**

```bash
just fmt-dev
```
Expected: clean. If any files were reformatted by nightly rustfmt during the smoke-test fixes, re-stage them.

- [ ] **Step 5: Hand off to the user**

```bash
git status
```
Summarize for the user: build is green, smoke tests pass, all tasks complete. List any follow-ups discovered during smoke testing. The user commits the work as they see fit (the plan did not auto-commit).

---

## Self-Review (run after writing, before handoff)

**Spec coverage:** every spec section §1–§14 maps to a task:
- §1 (Goal) — the plan's goal.
- §2 (Approach) — reflected in Global Constraints + Architecture.
- §3 (Type mapping) — Task 1's migration SQL.
- §4.1 (Cargo) — Task 2.
- §4.2 (database/mod.rs) — Task 3.
- §4.3 (DatabaseUrl) — Task 3.
- §4.4 (sqlx_types macro) — Task 3.
- §5 (Migrations) — Task 1.
- §6 (SQL rewrites: GROUP_CONCAT, VACUUM, ANY($1), AssertSqlSafe) — Tasks 4 & 5.
- §7 (jsonb) — Task 6.
- §8 (booleans) — Task 6.
- §9 (timestamps) — Task 7.
- §10 (gtfs_schedule fetcher) — Task 7 Step 4.
- §11 (.sqlx cache) — Task 8.
- §12 (justfile) — Task 9.
- §13 (AGENTS.md) — Task 10.
- §14 (Risks) — Task 4 is the `= ANY($1)` template (risk #5); Task 8 validates int-enum Type derive (risk #2), jsonb inference (risk #3), generated-column (risk #1 in Task 1 Step 5); Task 7 Step 1 validates jiff↔time (risk #1).
- §15–§16 (data migration / out of scope) — explicitly out of scope; referenced in Task 10's root AGENTS.md note.

No spec section is unmapped.

**Placeholder scan:** no "TBD"/"TODO"/"implement later". Illustrative SQL blocks are marked as such (e.g. Task 4 Step 1 says "copy the actual current expression from the file"). Repetitive transformations (Task 7's 10 sites) are given as an explicit rule + enumeration, not "similar to above".

**Type consistency:** `DatabaseUrl::as_str()` (Task 3) is used by `Database::init` (Task 3 Step 1). `database::time::{now, from_jiff, to_jiff}` (Task 3 Step 4) are used in Tasks 6, 7. The `u32 → i32` rule in Task 4 is consistent with `SMALLINT`/`INTEGER` decoding to `i32`. `rust_decimal::Decimal` (Task 2 dep + Task 6 field) is consistent.

**One known ambiguity:** Task 6 Step 4 notes that `rust_decimal`'s serde output format may need `#[serde(with = "...float")]` depending on frontend expectations — this is flagged for the implementer to verify during smoke testing, not left as a placeholder.
