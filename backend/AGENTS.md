# backend/AGENTS.md

Rust (edition 2024) Axum server, single crate `zet-live`. Serves the REST + WebSocket API
and the baked-in frontend. Run commands from `backend/` (or via `just backend <args>`).

## Developer commands

```
just dev-watch          # hot-reload dev server (watchexec, NOT cargo watch)
just dev-run server     # cargo run (target musl); depends on migrations-run
just build              # SQLX_OFFLINE=true cargo build --release --target musl
just run                # SQLX_OFFLINE=true cargo run --release --target musl
just test               # SQLX_OFFLINE=true cargo test --workspace --all-features (no tests yet)
just test-watch         # watchexec + test
just fmt-dev            # clippy --fix + nightly cargo fmt  (REQUIRED — see below)
just dev-db             # per-worktree dev database (subcommands listed below)
just dev-env            # link backend/.env to a file shared by all worktrees
just sqlx-regenerate    # regenerate the .sqlx offline cache (see below)
just migrations-add     # sqlx migrate add -r <name>  (creates reversible .up/.down.sql)
just migrations-list    # sqlx migrate info
just migrations-run     # sqlx migrate run
```

`just dev-run` takes the `server` subcommand; a bare `just dev-run` exits with a
usage error.

**Always use `just fmt-dev`** for formatting/lint/compile-checking. It runs
`clippy --fix --allow-dirty --allow-staged` then **nightly** `cargo fmt`. Do NOT
run `just fmt`, `just lint`, or `just lint-fix` on their own.

All commands set `RUSTFLAGS='-C target-feature=+crt-static'` and target
**`x86_64-unknown-linux-musl`** — the `musl` rustup target and `musl-tools` must
be installed. (The Dockerfile instead builds for `x86_64-unknown-linux-gnu`.)

## Dev database (one per worktree)

Every worktree gets its own database inside **one** shared `zet-live-pg`
container, so worktrees never collide on ports, container names, or data.
`just` derives `DATABASE_URL` from the worktree's directory — see `scripts/dev-db.sh`.
`just dev-db` with no arguments lists the subcommands:

```
just dev-db up                  # start the shared container
just dev-db down                # stop it
just dev-db create              # create this worktree's database, forked from the source
just dev-db reset               # drop, re-fork, and migrate this worktree's database
just dev-db remove [branch|dir] # drop a worktree's database (--force kicks live sessions)
just dev-db list                # list dev databases and the worktrees that own them
just dev-db migrate [branch|dir] # run migrations against a worktree's database
just dev-db name                # print this worktree's database name
just dev-db url [branch|dir]    # print a worktree's DATABASE_URL
```

- **Forking.** A new database is a copy of the *source* database — the worktree
  for branch `main`, unless `DEV_PG_SOURCE` names another. Forking kicks any
  session connected to the source, since `CREATE DATABASE ... TEMPLATE` refuses
  to run otherwise, then copies it and migrates the copy. Migrations fail on a
  branch that lacks migrations the source recorded, in which case the new
  database is dropped again and you must point `DEV_PG_SOURCE` at a compatible
  database.
- **`remove`** takes a branch name or worktree directory, refuses to drop the
  source database, and refuses to drop a database with live connections unless
  given `--force`.
- **`sqlx-regenerate` is non-destructive**: it prepares against a
  `zet_live_scratch-<hash>` database, so it never touches worktree data.
- Overridable via env: `DEV_PG_CONTAINER`, `DEV_PG_IMAGE`, `DEV_PG_HOST`,
  `DEV_PG_PORT`, `DEV_PG_USER`, `DEV_PG_PASSWORD`, `DEV_PG_DB_PREFIX`,
  `DEV_PG_SOURCE`. The password is interpolated into the URL raw, so it must be
  URL-safe.
- The scripts talk to the server through `docker exec ... psql`, so no local
  PostgreSQL client is required. `docker` and the `sqlx` CLI are.

**New worktree checklist:** `just dev-db create`, then `just dev-env`.

## sqlx offline cache (important)

The backend builds/tests/runs with `SQLX_OFFLINE=true` (set in the justfile).
Prepared-query metadata is read from the **committed `backend/.sqlx/` directory**,
not a live database.

**If you add or change any sqlx query (`sqlx::query*`, `query_as!`, etc.), you
MUST regenerate the cache or the offline build will fail:**

```
just sqlx-regenerate    # scratch DB → run migrations → cargo sqlx prepare --workspace
```

Commit the regenerated `.sqlx/` files alongside your query change.

The `.sqlx/` cache is Postgres-flavored. Regeneration needs `docker` (for the
scratch database) and the `sqlx` CLI — there is no in-process fallback. It runs
against a throwaway database, so worktree data is unaffected.

## Migrations

- Location: **`backend/migrations/`**.
- Reversible sqlx migrations: paired `<timestamp>_<name>.up.sql` / `.down.sql`,
  created via `just migrations-add`.
- Applied at startup by `sqlx::migrate!("./migrations")` (`src/database/mod.rs`).
- `just dev-run` runs `sqlx migrate run` first, against **this worktree's**
  database (see *Dev database* above). Run `just dev-db create` in a new
  worktree before `just dev-run`, or the database will not exist. There is no
  SQLite or `:memory:` fallback.

## Environment (`backend/.env`)

Loaded by `dotenvy::dotenv()` (in `main.rs`) and by the backend justfile
(`dotenv-load`). `backend/.env.example` documents the keys. `just dev-env`
symlinks `backend/.env` to `~/.config/zet-live/dev.env` (mode 600), so every
worktree shares one set of secrets instead of copying them per branch.

- `DATABASE_URL` — **required by the binary**, but it is **not** read from
  `.env`: the justfile exports one derived per worktree (see *Dev database*
  above), and that export wins over `.env`. Run `just dev-db url` to get it for
  a manual `cargo run`.
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
- `SESSION_MAX_AGE` — session token lifetime (default 30 days).

OAuth provider config lives in the `auth_providers` table and is managed through
the admin API; there are no `AUTH_*` environment variables.

## Architecture

- **Entry**: `src/main.rs` → clap CLI. Only subcommand: `server`. A global
  `--dump-completions <shell>` flag prints shell completions and exits.
  `main.rs` installs a Unix signal handler that cancels a
  `CancellationToken` (8s graceful-shutdown window) before force-exiting.
- **Three background fetchers** spawned in `src/server/mod.rs` at startup:
  GTFS-RT realtime, GTFS schedule, and GBFS. The server waits for an initial
  realtime + schedule update before serving.
- **API routes**: `/api/v1/*` in `src/server/routes/v1/`. The frontend is served
  as the router fallback (`routes/frontend/`, embedding `../frontend/dist`).
- **WebSocket**: real-time vehicle / active-stop / GBFS-station updates via
  `/api/v1/ws`. Binary frames are CBOR-encoded (`minicbor-serde`).
- **Admin API + UI**: a separate listener (`src/admin/`) gated by `ADMIN_KEY` +
  `ADMIN_BIND_TO`. Not started unless both are set. `/api/*` is the token-authed
  admin API; everything else is the admin SPA (`src/admin/static_assets.rs`,
  embedding `../frontend-admin/dist`). Both `frontend/dist` and
  `frontend-admin/dist` must exist at compile time — run `just build` (or at
  least `just frontend build && just frontend-admin build`) before building the
  backend.
- **Feature flags**: code-first registry in `src/feature_flags.rs` (keys seeded
  into the `feature_flags` table at startup, `ON CONFLICT DO NOTHING`), an
  `ArcSwap` in-memory snapshot for hot-path checks (`is_enabled`, fail closed),
  admin CRUD at `/api/feature-flags` on the admin listener, and per-user
  exposure via `/api/v1/capabilities` (`featureFlags`, only-enabled entries)
  plus the WS `FeatureFlagsChanged` per-connection push.
- **Database**: PostgreSQL via **sqlx** (the `postgres` feature). Connection
  pool of 20 in `src/database/mod.rs`. Migrations are applied at startup by
  `sqlx::migrate!("./migrations")`. No PRAGMA/optimize step (PG autovacuum
  handles maintenance). Timestamps are `timestamptz`, decoded via the `time`
  crate and bridged to `jiff` via `src/database/time.rs`.
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
