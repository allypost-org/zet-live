# backend/AGENTS.md

Rust (edition 2024) Axum server, single crate `zet-live`. Serves the REST + WebSocket API
and the baked-in frontend. Run commands from `backend/` (or via `just backend <args>`).

## Developer commands

```
just dev-watch          # hot-reload dev server (watchexec, NOT cargo watch)
just dev-run            # cargo run (target musl); depends on migrations-run
just build              # SQLX_OFFLINE=true cargo build --release --target musl
just run                # SQLX_OFFLINE=true cargo run --release --target musl
just test               # SQLX_OFFLINE=true cargo test --workspace --all-features (no tests yet)
just test-watch         # watchexec + test
just fmt-dev            # clippy --fix + nightly cargo fmt  (REQUIRED — see below)
just sqlx-regenerate    # regenerate the .sqlx offline cache (see below)
just migrations-add     # sqlx migrate add -r <name>  (creates reversible .up/.down.sql)
just migrations-list    # sqlx migrate info
just migrations-run     # sqlx migrate run
```

**Always use `just fmt-dev`** for formatting/lint/compile-checking. It runs
`clippy --fix --allow-dirty --allow-staged` then **nightly** `cargo fmt`. Do NOT
run `just fmt`, `just lint`, or `just lint-fix` on their own.

All commands set `RUSTFLAGS='-C target-feature=+crt-static'` and target
**`x86_64-unknown-linux-musl`** — the `musl` rustup target and `musl-tools` must
be installed. (The Dockerfile instead builds for `x86_64-unknown-linux-gnu`.)

## sqlx offline cache (important)

The backend builds/tests/runs with `SQLX_OFFLINE=true` (set in the justfile).
Prepared-query metadata is read from the **committed `backend/.sqlx/` directory**,
not a live database.

**If you add or change any sqlx query (`sqlx::query*`, `query_as!`, etc.), you
MUST regenerate the cache or the offline build will fail:**

```
just sqlx-regenerate    # temp DB → run migrations → cargo sqlx prepare --workspace
```

Commit the regenerated `.sqlx/` files alongside your query change.

## Migrations

- Location: **`backend/migrations/`**.
- Reversible sqlx migrations: paired `<timestamp>_<name>.up.sql` / `.down.sql`,
  created via `just migrations-add`.
- Applied at startup by `sqlx::migrate!("./migrations")` (`src/database/mod.rs`).
- `just dev-run` runs `sqlx migrate run` first and **requires `DATABASE_URL`
  to be set** (the recipe errors out otherwise).

## Environment (`backend/.env`)

Loaded by `dotenvy::dotenv()` (in `main.rs`) and by the backend justfile
(`dotenv-load`). Key vars:

- `DATABASE_URL` — defaults to `:memory:`. Dev uses `sqlite:./dev/db/db.sqlite`
  (gitignored; `backend/dev/`).
- `LOG_LEVEL` — comma-separated, e.g. `zet_live=trace,query=trace,warn`.
- `ZI_DATA_FETCH_ENDPOINT`, `ZI_DATA_FETCH_INTERVAL` — GTFS-RT vehicle positions.
- `ZI_SCHEDULE_FETCH_ENDPOINT`, `ZI_SCHEDULE_FETCH_INTERVAL` — GTFS schedule zip.
- `GBFS_FETCH_ENDPOINT`, `GBFS_LANGUAGE`, `GBFS_MIN_FETCH_INTERVAL` — bike-share.
- `ADMIN_KEY` + `ADMIN_BIND_TO` — **both** required to start the admin API.
- `BIND_TO` (default `0.0.0.0:9011`), `IP_SOURCE`.
- `APP_URL` — **required iff any auth provider is enabled** (used to build
  OAuth redirect URIs). If unset, user accounts are disabled entirely and the
  binary still boots. The admin API refuses to enable a provider while it's
  unset (returns 400).
- `ALLOWED_FRONTEND_ORIGINS` — comma-separated extra origins allowed as the
  OAuth popup `postMessage` target (for split-origin dev). `APP_URL`'s origin
  is always allowed automatically.
- `SESSION_MAX_AGE_SECS` — session token lifetime (default 2_592_000 = 30d).

## Architecture

- **Entry**: `src/main.rs` → clap CLI. Only subcommand: `server`. A global
  `--dump-completions <shell>` flag prints shell completions and exits.
  `main.rs` installs a Unix signal handler that cancels a
  `CancellationToken` (8s graceful-shutdown window) before force-exiting.
- **Three background fetchers** spawned in `src/server/mod.rs` at startup:
  GTFS-RT realtime, GTFS schedule, and GBFS. The server waits for an initial
  realtime + schedule update before serving.
- **API routes**: `/api/v1/*` in `src/server/routes/v1/`. The frontend is served
  as the router fallback (`routes/frontend/`).
- **WebSocket**: real-time vehicle / active-stop / GBFS-station updates via
  `/api/v1/ws`. Binary frames are CBOR-encoded (`minicbor-serde`).
- **Admin API**: a separate listener (`src/admin/`) gated by `ADMIN_KEY` +
  `ADMIN_BIND_TO`. Not started unless both are set.
- **Database**: libsql/SQLite via **sqlx**. Connection pool of 20 with WAL and a
  tuned PRAGMA block in `src/database/mod.rs`; `PRAGMA optimize` runs hourly.
- **Protobuf**: GTFS-RT proto in `backend/protobuf/`. `build.rs` compiles it via
  `prost-build` → generated `_gtfs_realtime.rs` in `OUT_DIR` (adds serde derives).
- **Build info**: `build-info` / `build-info-build` inject version, build date,
  rustc version at compile time.
- **listenfd**: socket-activated dev supported (`listenfd` crate).

## Lint / formatting conventions

- Clippy: **nursery + pedantic** enabled (warn). `unwrap_used` is warn. Several
  common lints allowed (see `Cargo.toml` `[lints.clippy]`).
- **Never remove clippy lints from `Cargo.toml` or add `#[allow(...)]` /
  `#[expect(...)]` to suppress warnings without explicit user permission.**
- rustfmt: grouped imports (`StdExternalCrate`), vertical layout, crate-level
  granularity, `format_macro_matchers`, `format_strings` (see `rustfmt.toml`).
  `fmt-dev` runs **nightly** rustfmt — use it, not stable `cargo fmt`.
