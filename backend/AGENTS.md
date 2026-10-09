# backend/AGENTS.md

Apply the [root instructions](../AGENTS.md) with these backend rules.
This is one Rust edition 2024 crate, `zet-live`, using Axum and PostgreSQL through sqlx.
Run recipes from `backend/`, or use `just backend <args>` from the root.

## Commands and checks

| Command                      | Purpose                                                        |
| ---------------------------- | -------------------------------------------------------------- |
| `just fmt-dev`               | Clippy auto-fix and nightly rustfmt; required after Rust edits |
| `just check`                 | Read-only formatting and locked Clippy; warnings fail          |
| `just test [args]`           | Offline Rust tests, all workspace features, musl target        |
| `just dev-run server`        | Migrate this worktree's database, then run the dev server      |
| `just dev-watch server`      | Restart the dev server with watchexec                          |
| `just build`                 | Offline release build for musl                                 |
| `just run server`            | Offline release run for musl                                   |
| `just dev-db`                | List database-management commands                              |
| `just dev-env`               | Link `backend/.env` to the shared secrets file                 |
| `just sqlx-regenerate`       | Regenerate committed query metadata against a scratch database |
| `just migrations-add <name>` | Create paired, reversible migration files                      |
| `just migrations-list`       | Show migration status                                          |
| `just migrations-run`        | Apply migrations to this worktree's database                   |

- Use `just fmt-dev` after Rust edits, then `just check` for verification. Do not run `just fmt`, `just lint`, or `just lint-fix` separately.
- Review remaining Clippy warnings and the auto-fix diff. A successful exit does not mean that all warnings were fixed.
- Do not remove Clippy rules or add `#[allow(...)]` or `#[expect(...)]` without explicit user permission.
- Clippy settings are in `Cargo.toml`; formatting settings are in `rustfmt.toml`. Use nightly rustfmt for these settings.
- Build, run, and test recipes target `x86_64-unknown-linux-musl` with `-C target-feature=+crt-static`. Install that rustup target and `musl-tools`.
- Both frontend `dist/` directories must exist before compilation. Build them with the root frontend recipes.

## Rust interfaces

- Make a function a method when its main subject is a type defined in this crate. Use a free function for subjects from different types.
- Group related free functions in an existing module. Use a unit struct namespace only for a useful grouping beyond one or two functions. Add fields only for real state.
- Prefer type annotations on producing expressions: `collect::<Vec<_>>()`, `parse::<T>()`, and serde helpers. Use binding annotations when the expression cannot express the type.
- Put unit tests in an inline `#[cfg(test)] mod tests` beside the code they cover. Follow the root test policy.

## Worktree database and environment

- Each worktree uses its own database in the shared `zet-live-pg` container. `scripts/dev-db.sh` manages it.
- For a new worktree, run `just dev-db create`, then `just dev-env`.
- The justfile exports a worktree-derived `DATABASE_URL`, which overrides `.env`. Do not set it in `backend/.env`; use `just dev-db url` for manual runs.
- `just dev-env` links to `~/.config/zet-live/dev.env`. Keep shared secrets there; do not copy them into worktrees.
- Use `.env.example` and `src/cli/` for configuration keys and defaults. Both the binary and the component justfile load `.env`.
- Database commands require Docker and the sqlx CLI. They use `docker exec ... psql`; a local PostgreSQL client is not required.
- `create` forks the source database (`main`, unless `DEV_PG_SOURCE` selects another). Forking disconnects source sessions.
- If the source has migrations absent from this branch, select a compatible source with `DEV_PG_SOURCE`.
- `reset` drops, forks, and migrates the worktree database. `remove` refuses to delete the source or an active database without `--force`.
- Database overrides use `DEV_PG_*` variables in `scripts/dev-db.sh`. `DEV_PG_PASSWORD` must be URL-safe because URLs include it without escaping.

## Queries and migrations

- Build, release-run, and test recipes use `SQLX_OFFLINE=true` and committed `.sqlx/` metadata.
- After changing a sqlx query, run `just sqlx-regenerate`. Commit the regenerated `.sqlx/` files with the query change.
- Regeneration uses a scratch database and does not alter worktree data. It requires Docker and the sqlx CLI.
- Migrations live in `migrations/`, as `<timestamp>_<name>.up.sql` and `.down.sql` pairs. Use `just migrations-add`.
- Startup applies migrations through `sqlx::migrate!`. `just dev-run server` also migrates this worktree's database before running.

## Component boundaries

- Public REST routes live in `src/server/routes/v1/`. `/api/v1/ws` sends CBOR binary frames through `minicbor-serde`.
- The admin API and SPA use a separate listener in `src/admin/`. Both `ADMIN_KEY` and `ADMIN_BIND_TO` are required.
- OAuth provider configuration lives in the `auth_providers` table and is managed through the admin API. Enabling auth requires `APP_URL`.
- Keep admin response shapes consistent with `frontend-admin/src/entity/schemas.ts`.
- Database timestamps use `timestamptz`. `src/database/time.rs` bridges sqlx's `time` values to `jiff`.
- GTFS-RT protobuf definitions live in `protobuf/`. `build.rs` generates Rust code in `OUT_DIR`; edit the definitions, not generated output.
- Feature-flag definitions live in `src/feature_flags.rs`. Public capabilities and WebSocket updates expose enabled flags to users.
