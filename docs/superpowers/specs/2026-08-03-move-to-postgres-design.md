# Move backend from SQLite to PostgreSQL

**Date:** 2026-08-03
**Status:** Approved (design) — pending implementation plan
**Scope:** `backend/` only (Rust crate `zet-live`, migrations, build config, developer docs). Frontend (`frontend/`, `frontend-admin/`) untouched. Production deployment (Dockerfile, CI, Watchtower) is explicitly out of scope beyond a documentation note.

---

## 1. Goal

Replace the SQLite persistence layer (via sqlx's `sqlite` feature — not libsql) with PostgreSQL. End state is a Postgres-only backend with an idiomatic PG schema, no dual-backend support, and no in-process DB fallback.

## 2. Approach (decisions)

| Decision | Chosen option | Rejected alternatives |
|---|---|---|
| Backend support | **Postgres-only (clean cut)** | Dual-backend via feature flags; embedded PG dev shim |
| Existing migrations | **Collapse all 6 pairs into one new `init`** | Rewrite each 1:1 in PG dialect (no data to preserve; pure busywork) |
| Type fidelity | **Maximally idiomatic PG** | Minimal-change (would leave schema feeling like ported SQLite) |
| Timestamps | **`timestamptz` columns + `time` crate as jiff bridge** | Keep `TEXT` (jiff has no sqlx feature; chose the lighter bridge over `chrono`) |
| Provisioning | **`DATABASE_URL` required, no compose** | Add `docker-compose.yml`; auto-provision embedded PG |
| Deploy scope | **Code + dev workflow only; document Dockerfile implication** | Also wire production PG sidecar/managed DB |
| Currency | **`numeric(19,4)`** | PG native `money` (locale-dependent footgun, explicitly discouraged by PG docs); keep `REAL` |
| Spatial (coords) | **Defer PostGIS; keep lat/lon as `double precision`** | Adopt PostGIS `geography(Point,4326)` now (needs extension not present on the dev image; runtime dep + scope creep) |

**Rationale for clean cut / collapse:** The schema can be rebuilt fresh in PG (the init migration creates it in final form) and data transfer is handled by a separate out-of-band script (see §15) that reads the old SQLite prod DB and writes the new PG schema directly — it does not rely on replaying migrations. Maintaining dual-backend would force every query to satisfy both engines forever. The 6 existing migrations contain SQLite-only constructs (`instr()`, `AUTOINCREMENT`, `unixepoch`, `STRICT`, `GENERATED ... substr`) whose 1:1 translation yields zero functional benefit.

**Production data note:** Prod does NOT run `:memory:` — that's only the Dockerfile default. Real prod mounts a volume and persists data. The `:memory:` default goes away in this migration (replaced by a required `DATABASE_URL`); cutover requires the operator to run the data-migration script (§15) against the existing volume.

## 3. Type mapping (SQLite → Postgres)

| SQLite | Postgres | Notes |
|---|---|---|
| `INTEGER PRIMARY KEY AUTOINCREMENT` (`feedback.id`) | `BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY` | |
| Reference-data `INTEGER PRIMARY KEY` (gtfs enum tables, `live_feed_metadata.id`, `gbfs_rental_hours.id`, `service_combo_ids.combination_id`) | `INTEGER PRIMARY KEY` / `SMALLINT` | values inserted explicitly; not auto-generated |
| `BLOB` hashes (`user_sessions.token_hash`, `link_tickets.token_hash`, `pending_transfers.token_hash`) | `BYTEA` | `Vec<u8>` binds unchanged |
| `BLOB` JSON (`admin_settings.value`, `admin_metadata.value`, `user_settings.settings`) | `JSONB NOT NULL` | rewrite Rust binds/decodes (see §7) |
| `REAL` (coordinates: lat/lon across `gtfs_stops`, `gtfs_shapes`, `live_vehicles`, `gbfs_stations`, and similar) | `DOUBLE PRECISION` | PostGIS deferred (see decision §2); two-column lat/lon retained |
| `REAL` (currency: `gtfs_fare_attributes.price`, `gbfs_pricing_plans.price`) | `NUMERIC(19, 4)` | idiomatic currency; Rust decodes via `rust_decimal::Decimal` (see §4.1) |
| `REAL` (other numeric: `gtfs_shapes.shape_dist_traveled`, `gtfs_fare_attributes.transfer_duration`, etc.) | `DOUBLE PRECISION` | measurements, not currency or coordinates |
| Boolean-as-integer (`feedback.handled`, `feedback.dismissed`, `oauth_states.link`, `auth_providers.enabled`) | `BOOLEAN NOT NULL DEFAULT FALSE` (`auth_providers.enabled` → `DEFAULT TRUE`) | rewrite Rust bool sites (see §8) |
| Timestamps (`*_at`: `created_at`, `updated_at`, `expires_at`, `replied_at`, etc.) | `TIMESTAMPTZ NOT NULL` | jiff↔time bridge (see §9) |
| `gtfs_schedule_meta.fetched_at` (epoch REAL, `unixepoch('subsec')` default) | `TIMESTAMPTZ DEFAULT now()` | fetcher rewritten (see §10) |
| `gtfs_schedule_meta.last_modified` (epoch REAL) | `TIMESTAMPTZ` | compared in fetcher (see §10) |
| `arrival_time` / `departure_time` (GTFS time-of-day) | `TEXT` | stays TEXT — GTFS values may exceed 24:00:00 |
| `arrival_time_seconds` / `departure_time_seconds` (`GENERATED ALWAYS AS (...) STORED`) | same (`GENERATED ALWAYS AS (...) STORED`), rewritten with `split_part(arrival_time,':',1)::int * 3600 + split_part(arrival_time,':',2)::int * 60 + split_part(arrival_time,':',3)::int` | PG ≥12; `split_part` is immutable |
| `live_feed_metadata.base_midnight` (epoch seconds) | `BIGINT` | semantics preserved (integer epoch) |
| `CHECK (x LIKE '__:__:__')` on `arrival_time`/`departure_time` | kept verbatim | PG `LIKE` has same single-char wildcard semantics |
| `STRICT` table mode | removed | PG has strict typing natively |

## 4. Component changes

### 4.1 `Cargo.toml`

- `sqlx`: swap `"sqlite"` feature → `"postgres"`; add `"time"`, `"json"`, and `"rust_decimal"` features.
  - **Result:** `sqlx = { version = "0.9", features = ["runtime-tokio", "postgres", "macros", "time", "json", "rust_decimal"] }`
- Add `time = { version = "0.3", features = ["serde"] }` (jiff↔PG bridge).
- Add `rust_decimal = { version = "1", features = ["serde"] }` (decode `numeric(19,4)` price columns). The `Option<f64>` field at `src/proto/gbfs/data/system_pricing_plans.rs:25` becomes `Option<rust_decimal::Decimal>`; `serde` keeps JSON output numeric.
- `jiff` stays unchanged (application logic: spans, fetcher TTLs, RFC2822 parsing).

### 4.2 `src/database/mod.rs`

- Replace `SqlitePool` / `SqliteConnection` / `SqlitePoolOptions` → `PgPool` / `PgConnection` / `PgPoolOptions`.
- **Delete:** the `CONNECTION_PRAGMAS` array (13 entries), `configure_sqlite_connection`, `Database::optimize`, `run_optimize_periodically`. All SQLite-specific.
- `Database::init(url)` validates the PG URL, builds `PgPoolOptions::new().max_connections(20).connect(...)`, runs `sqlx::migrate!("./migrations")`. No `after_connect` callback (no PRAGMAs to set).
- Keep unchanged: `DATABASE: OnceLock<PgPool>`, `pool()`, `logged()` (backend-agnostic query-timing wrapper).

### 4.3 `src/cli/mod.rs` — `DatabaseUrl`

- Replace the `enum DatabaseUrl { Memory, Local(PathBuf) }` with a validating newtype: `pub struct DatabaseUrl(String)`.
- Accepts only `postgres://` / `postgresql://` schemes; rejects all others with a clear error message.
- CLI field `--database-url` keeps `env = "DATABASE_URL"` but **loses its default** — it becomes required.
- `Display` prints the inner string. `Debug` masks the password component to avoid leaking it in logs.

### 4.4 `src/database/sqlx_types.rs` — `sqlx_int_enum_decode!` macro

- **Delete the file** (and its `pub mod sqlx_types;` declaration).
- The 7 integer-backed enums (`Direction`, `RouteType`, `PickupType`, `DropOffType`, `LocationType`, `WheelchairBoarding`, `BikesAllowed`) move to `#[repr(i32)]` + bare `#[derive(sqlx::Type)]`, which decodes `INT4` directly on PG with no custom `TryFrom<i64>` impl.

## 5. Migrations

- **Delete** all 6 existing migration pairs (12 files) under `backend/migrations/`.
- **Create** one new reversible migration `<new-timestamp>_init.up.sql` / `.down.sql` via `just migrations-add init`.
- `up.sql` creates the full PG schema in its final form, applying the §3 type mapping. Indexes use `CREATE INDEX IF NOT EXISTS` and are otherwise unchanged in column coverage.
- `down.sql` drops all tables in reverse dependency order.

## 6. SQL rewrites in `.rs` files

- **`src/auth/accounts.rs:507,544`** — `GROUP_CONCAT(x, ',' ORDER BY y)` → `string_agg(x, ',' ORDER BY y)`.
- **`src/proto/gtfs_schedule/data/mod.rs:303`** — `sqlx::query!("VACUUM")` → **removed**. PG `VACUUM` cannot run inside the `query!` macro or a transaction; PG autovacuum handles bloat. Optionally emit `ANALYZE` (safe in a tx) after the bulk schedule load if a re-stat is wanted.
- **Dynamic `IN (...)` builders** — `schedule/mod.rs:253-262,686-688,752-754,813-816`; `v1/mod.rs:690,756,819`. Rewrite each as `col = ANY($1)` with a `&[T]` / `Vec<T>` bind, and **convert each from raw `sqlx::query(...)` + `AssertSqlSafe` to a `query_as!` / `query!` macro** so the SQL is a static literal, statically type-checked by sqlx. No placeholder-number generation, no runtime SQL string building. sqlx encodes `&[String]` / `Vec<String>` as a PG text array; `ANY($1)` works against `text[]`.
- **`AssertSqlSafe`** (SQLite-only sqlx wrapper) — remove at every site (`database/mod.rs:37`, `schedule/mod.rs:279`, `gtfs_schedule/data/mod.rs:42`, `v1/mod.rs:690,756,819`); unnecessary on PG.

## 7. `jsonb` Rust code (3 columns)

- Bind `serde_json::Value` (not `String`) for writes; decode `serde_json::Value` (not `from_slice`) for reads.
- **`src/admin/metadata.rs:80`** — `serde_json::to_string(entry)` → `entry.clone()` (produce `Value`); **`:132`** — `serde_json::from_slice(x.value.as_slice())` → `row.get::<serde_json::Value, _>`.
- **`src/server/routes/v1/settings.rs`** — `user_settings.settings` write/read switches to `Value`.
- **`src/admin/settings.rs`** — same for `admin_settings.value`.

## 8. Boolean Rust code

- **`src/admin/router.rs:330`** — drop `let enabled_i = i64::from(body.enabled)`; bind `body.enabled: bool` directly.
- **`src/admin/router.rs:400`** — `let enabled_i = body.enabled.map(i64::from)` → keep as `Option<bool>`.
- **`src/admin/feedback.rs:100`** — `WHERE f.handled = 0 AND f.dismissed = 0 AND f.reply IS NULL` → `WHERE NOT f.handled AND NOT f.dismissed AND f.reply IS NULL`.
- **`src/admin/feedback.rs:133`** — `WHERE f.handled = 1 OR f.dismissed = 1 OR f.reply IS NOT NULL` → `WHERE f.handled OR f.dismissed OR f.reply IS NOT NULL`.

## 9. Timestamp Rust code (jiff↔time bridge)

Add `src/database/time.rs` with three helpers:

```rust
pub fn now() -> time::OffsetDateTime;                          // OffsetDateTime::now_utc()
pub fn from_jiff(j: jiff::Timestamp) -> time::OffsetDateTime;  // via unix_timestamp_nanos
pub fn to_jiff(t: time::OffsetDateTime) -> jiff::Timestamp;
```

- Every helper that produces a timestamp **for binding** switches its return type from `String` to `time::OffsetDateTime`. This covers both `now_iso()`-style helpers (e.g. `src/auth/session.rs:32` `now_iso()`, and the ~20 inline `jiff::Timestamp::now().to_string()` call sites across `admin/*`, `auth/*`, `feedback`, `settings`, `user_notices`) and arithmetic helpers like `expires_iso(max_age)` (`src/auth/session.rs:36`), which becomes `time::OffsetDateTime::now_utc() + max_age`. Each returns an `OffsetDateTime` that sqlx binds to `timestamptz` directly — no `.to_string()`.
- Every **read** of an `*_at` column → decode as `time::OffsetDateTime`, convert to `jiff::Timestamp` via `to_jiff` at the use site (the codebase currently passes these through as `String`, so the implementation plan will enumerate the actual decode sites).
- Sites that compute time from a non-timestamp column — e.g. `live_feed_metadata.base_midnight` (stays `BIGINT`, epoch seconds) — are **not** in scope for the bridge; they continue to use `jiff::Timestamp::from_second(...)`.

## 10. `gtfs_schedule` fetcher epoch-float sites

- **`src/proto/gtfs_schedule/fetcher.rs:163-164`** — `last_modified` read from DB as `f64` epoch and compared against `jiff::Timestamp` from the HTTP `Last-Modified` header. Rewrite: read `last_modified` as `time::OffsetDateTime`, compare to the parsed RFC2822 timestamp (converted via `from_jiff`).
- **`src/proto/gtfs_schedule/data/mod.rs`** — the `gtfs_schedule_meta` write currently binds an `f64` epoch; rewrite to bind `OffsetDateTime` / use `DEFAULT now()` for `fetched_at`.

## 11. `.sqlx/` offline cache

- **Delete** all 112 existing SQLite `.json` files under `backend/.sqlx/`.
- Regenerate against the running PG dev instance (see §12) via the updated `just sqlx-regenerate`.
- Commit the regenerated cache alongside the query changes.

## 12. `backend/justfile`

- **`sqlx-regenerate`** — drop the temp-SQLite-file logic. Read `DATABASE_URL` from env (must be a `postgres://`/`postgresql://` URL); run `cargo sqlx database create && cargo sqlx migrate run && cargo sqlx prepare --workspace`.
- **`migrations-run` / `_migrations-ensure-db`** — delete the `sqlite:` URL detection and the `touch`-file-create branch. Simplify to plain `sqlx migrate run` (errors clearly if `DATABASE_URL` is unset). PG's `database create` handles provisioning; no file-path shim needed.
- **`dev-run`** keeps its `migrations-run` dependency.

## 13. Documentation

- **`backend/AGENTS.md`**:
  - "Architecture → Database": libsql/SQLite → Postgres via sqlx; remove the PRAGMA block / hourly optimize mention.
  - "sqlx offline cache": note the cache is now PG-flavored and regeneration requires a running PG instance.
  - "Environment (`backend/.env`)": `DATABASE_URL` is **required** and must be `postgres://...`; remove the `:memory:` default and `sqlite:./dev/db/db.sqlite` references.
  - Developer commands: note that `DATABASE_URL` is required to start the server.
- **Root `AGENTS.md`**: add a one-line note under "Docker / deploy" that the runtime container now needs `DATABASE_URL` (pointing at an external PG); the `scratch` image still ships no DB.

## 14. Risks (de-risk early in the implementation plan)

1. **jiff↔time conversion correctness** — timezone handling at the boundary; verify a round-trip (`now()` → write → read → `to_jiff`) equals the original within the DB's microsecond precision.
2. **sqlx `Type` derive for int enums on PG** — confirm `#[repr(i32)] #[derive(sqlx::Type)]` decodes `INT4` without the deleted macro.
3. **`jsonb` macro inference** — confirm binding `serde_json::Value` type-checks inside `query!`.
4. **Generated-column `split_part` immutability** — confirm PG accepts the expression as `STORED` generated.
5. **`= ANY($1)` + `query_as!` template** — land one of the six dynamic-`IN` conversions first as the template before doing the remaining five.

## 15. Data migration (out-of-band script)

Production persists data on a mounted volume (the `:memory:` Dockerfile default is overridden at deploy time). A one-shot data-migration script, run by the operator as part of cutover, transfers non-fetchable rows from the existing SQLite DB into the new PG schema. **The script itself is out of scope for this PR** — it's a separate operational artifact — but the mapping is documented here so it can be implemented confidently.

**Re-fetchable / ephemeral — do NOT migrate:**
- All `gtfs_*` tables — rebuilt from the GTFS zip on schedule-fetcher startup.
- All `gbfs_*` tables — rebuilt from the GBFS feed.
- All `live_*` tables (`live_trips`, `live_trip_stop_times`, `live_vehicles`, `live_feed_metadata`) — rewritten every realtime cycle.
- `gtfs_schedule_meta` — conditional-fetch cache (etag / `last_modified`); losing it triggers one unconditional refetch, harmless.
- `oauth_states`, `link_tickets`, `pending_transfers` — short-lived flow state gated by `expires_at`; letting them expire forces users to re-authenticate, which is acceptable during a scheduled cutover.

**Must migrate (persistent, non-fetchable):**
- `users`, `user_oauth_identities`, `user_sessions`, `user_settings`
- `auth_providers` (operator-configured OAuth clients)
- `user_notices`
- `feedback` (including `reply`, `replied_at`, `handled`, `dismissed`, `user_id`)
- `admin_settings`, `admin_metadata`
- `data_deletion_requests` (compliance records)

**Type conversions the script must apply (mirror §3):**
- `*_at` columns currently TEXT (ISO-8601) → `timestamptz` (`CAST(col AS timestamptz)` or load as string and let PG coerce).
- `feedback.handled` / `feedback.dismissed` / `oauth_states.link` / `auth_providers.enabled`: integer 0/1 → `boolean` (`(col <> 0)`).
- `admin_settings.value` / `admin_metadata.value` / `user_settings.settings`: BLOB containing JSON bytes → `jsonb` (decode the blob and `::jsonb`).
- `user_sessions.token_hash` / `link_tickets.token_hash` / `pending_transfers.token_hash`: BLOB → `bytea` (binary pass-through).
- `gtfs_fare_attributes.price` / `gbfs_pricing_plans.price`: REAL → `numeric(19,4)` (`CAST(col AS numeric(19,4))`).

A reasonable implementation strategy: dump each must-migrate SQLite table to `COPY ... TO STDOUT WITH CSV HEADER`, then load into a staging table in PG and run `INSERT INTO <real> SELECT <casts> FROM staging`. Or write a small Rust/Python helper that opens both DBs and copies row-by-row with explicit casts.

## 16. Out of scope

- `frontend/`, `frontend-admin/`.
- Production PG provisioning (Dockerfile, CI, Watchtower) — documentation note only.
- The data-migration script itself (mapping documented in §15; operator runs it at cutover).
- Adding tests (the project has no tests today; adding them is a separate effort).
