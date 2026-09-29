# Feature Flags Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Code-first feature flags (disabled / enabled / logged-in / scoped) with an in-memory hot path, admin management via a URL-deep-linkable drawer, and per-user live exposure over `/api/v1/capabilities` + WebSocket.

**Architecture:** A `FeatureFlag` newtype + macro-generated registry (`backend/src/feature_flags.rs`) seeds a `feature_flags` table at startup and feeds an `ArcSwap` in-memory snapshot (mirrors the existing `auth_providers` → `PROVIDERS` pattern). Admin mutations go through one full-state `PATCH` on the admin listener, then reload the snapshot and push a `FeatureFlagsChanged` transmission that every WS connection evaluates **for its own user**. The public frontend keeps a Zustand `Record<string, boolean>` hydrated from capabilities and replaced wholesale on WS frames; the admin SPA gets a table + slide-over drawer driven by a `?flag=<key>` search param.

**Tech Stack:** Rust 2024 / Axum / sqlx (Postgres, offline cache) / arc-swap / minicbor-serde; React 19 + Vite + Zustand + zod + cbor2 (`frontend/`); React 19 + TanStack Router/Query/Table + zod + sonner (`frontend-admin/`).

**Spec:** `docs/superpowers/specs/2026-09-29-feature-flags-design.md` (read it first).

## Global Constraints

- Read `AGENTS.md` at repo root, plus the per-component `AGENTS.md` (`backend/`, `frontend/`, `frontend-admin/`) **before editing that component**.
- All work happens on branch `feat/feature-flags` (exists; specs committed there). If executing in a worktree, create it with the user's script: `/home/allypost/.scripts/git-worktree-create` (see superpowers:using-git-worktrees).
- Indentation: **Rust 4 spaces; everything else 2 spaces.** LF endings, no trailing whitespace.
- Rust formatting/lint gate is **`just fmt-dev`** (run from `backend/` or `just backend fmt-dev`) — never bare `cargo fmt`/`clippy`. Clippy pedantic+nursery is warn; **never add `#[allow]`/`#[expect]` or remove lints without explicit user permission** — fix the code instead.
- sqlx offline cache: any new/changed `sqlx::query!` requires `just backend sqlx-regenerate` (needs `DATABASE_URL` in `backend/.env` pointing at a live PostgreSQL) **and the regenerated `backend/.sqlx/` files must be committed together with the query change**.
- Frontend gates: `just frontend check` and `just frontend-admin check` (type-check + lint-check + fmt-check). Prettier: double quotes, 100-char width, trailing commas. Never `import React`.
- **Fail closed everywhere**: unknown flag key ⇒ off — backend evaluation, capabilities payload, frontend hook.
- Flag keys are **arbitrary strings** — no casing/format assumptions anywhere.
- `FeatureFlag` construction is module-private (registry macro + tests only); `is_enabled` takes `FeatureFlag`, never `&str`.
- Admin API JSON is camelCase (`serde(rename_all = "camelCase")`; DB stores snake_case state values).
- Commit after every task (short imperative messages, e.g. `feat: admin API for feature flags`).

---

### Task 1: Backend domain module — `FeatureFlag`, registry macro, `FlagState`, evaluation (TDD)

**Files:**
- Create: `backend/src/feature_flags.rs`
- Modify: `backend/src/main.rs:9-18` (add module declaration)

**Interfaces:**
- Consumes: nothing (pure module; no DB in this task).
- Produces (used by Tasks 2, 3, 4, 5):
  - `crate::feature_flags::FeatureFlag` — pub struct, `pub const fn key(&self) -> &'static str`
  - `crate::feature_flags::REGISTRY: &[&str]`
  - `crate::feature_flags::FlagState` — `{Disabled, Enabled, LoggedIn, Scoped}`, `pub const fn as_str(self) -> &'static str`, `pub const fn parse_db(s: &str) -> Option<Self>`, serde camelCase (`"loggedIn"`)
  - `crate::feature_flags::FlagEntry { pub state: FlagState, scoped_users: Arc<HashSet<String>> }` with `pub fn evaluate(&self, user: Option<&str>) -> bool`
  - `crate::feature_flags::is_enabled(flag: FeatureFlag, user: Option<&str>) -> bool`
  - `crate::feature_flags::enabled_map(user: Option<&str>) -> HashMap<String, bool>` (only-true entries)

- [ ] **Step 1: Declare the module**

In `backend/src/main.rs`, the module list (lines 9-18) is alphabetical — add `feature_flags` between `entity` and `http_client`:

```rust
mod admin;
mod auth;
mod cli;
mod config;
mod database;
mod entity;
mod feature_flags;
mod http_client;
mod logger;
mod proto;
mod server;
```

- [ ] **Step 2: Create `backend/src/feature_flags.rs` with failing tests and stubs**

Write the file with tests plus **stubbed** `evaluate` (always `false`) so tests compile but fail:

```rust
//! Feature flags: code-first definitions (newtype + macro registry), pure
//! evaluation, and — added later — DB persistence + an in-memory snapshot for
//! hot-path checks. Spec: `docs/superpowers/specs/2026-09-29-feature-flags-design.md`.

use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
};

use arc_swap::ArcSwap;
use serde::{Deserialize, Serialize};

/// A feature-flag key. Constructed only by the [`feature_flags!`] registry
/// below (and by tests inside this module) — the inner string is private, so
/// call sites cannot pass arbitrary strings where a flag is expected.
pub struct FeatureFlag(&'static str);

impl FeatureFlag {
    const fn new(key: &'static str) -> Self {
        Self(key)
    }

    /// The flag's DB/API key.
    #[must_use]
    pub const fn key(&self) -> &'static str {
        self.0
    }
}

/// Declares the binary's feature flags. Each entry generates a
/// `FeatureFlag::<name>()` constructor and feeds [`REGISTRY`], so the two can
/// never drift. Keys are arbitrary strings; document the gated behavior in a
/// comment next to the entry.
macro_rules! feature_flags {
    ($($name:ident => $key:literal),* $(,)?) => {
        impl FeatureFlag {
            $(
                #[doc = concat!("The `", stringify!($name), "` feature flag.")]
                #[must_use]
                pub const fn $name() -> Self {
                    Self::new($key)
                }
            )*
        }

        /// Every flag the binary knows about (mirrors the constructors above).
        pub const REGISTRY: &[&str] = &[$($key),*];
    };
}

feature_flags! {
    // Flags are added here when a feature needs gating, e.g.:
    // stop_departures => "stop_departures",
}

/// A flag's rollout state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FlagState {
    /// Off for everyone.
    Disabled,
    /// On for everyone (including anonymous visitors).
    Enabled,
    /// On for every logged-in user.
    LoggedIn,
    /// On only for the flag's scoped-user set.
    Scoped,
}

impl FlagState {
    /// The DB representation (`feature_flags.state`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Enabled => "enabled",
            Self::LoggedIn => "logged_in",
            Self::Scoped => "scoped",
        }
    }

    /// Parses a DB `state` value. `None` on anything unexpected.
    #[must_use]
    pub const fn parse_db(s: &str) -> Option<Self> {
        match s {
            "disabled" => Some(Self::Disabled),
            "enabled" => Some(Self::Enabled),
            "logged_in" => Some(Self::LoggedIn),
            "scoped" => Some(Self::Scoped),
            _ => None,
        }
    }
}

/// The in-memory view of one flag (part of the `FLAGS` snapshot).
pub struct FlagEntry {
    pub state: FlagState,
    scoped_users: Arc<HashSet<String>>,
}

impl FlagEntry {
    /// Evaluates this flag for `user` (user id; `None` when anonymous).
    #[must_use]
    pub fn evaluate(&self, _user: Option<&str>) -> bool {
        false // stub — implemented in Step 4
    }
}

static FLAGS: LazyLock<ArcSwap<HashMap<String, FlagEntry>>> =
    LazyLock::new(|| ArcSwap::from_pointee(HashMap::new()));

/// Whether `flag` is enabled for `user` (user id; `None` when anonymous).
/// Unknown flags are always `false` (fail closed).
#[must_use]
pub fn is_enabled(flag: FeatureFlag, user: Option<&str>) -> bool {
    FLAGS
        .load()
        .get(flag.key())
        .is_some_and(|e| e.evaluate(user))
}

/// The caller's evaluated view: only flags **enabled** for `user`, mapped to
/// `true`. Omitting disabled flags keeps unshipped flag names private.
#[must_use]
pub fn enabled_map(user: Option<&str>) -> HashMap<String, bool> {
    FLAGS
        .load()
        .iter()
        .filter(|(_, entry)| entry.evaluate(user))
        .map(|(key, _)| (key.clone(), true))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(state: FlagState, users: &[&str]) -> FlagEntry {
        FlagEntry {
            state,
            scoped_users: Arc::new(users.iter().map(|u| (*u).to_string()).collect()),
        }
    }

    #[test]
    fn registry_has_no_duplicate_keys() {
        let mut seen = std::collections::HashSet::new();
        for key in REGISTRY {
            assert!(seen.insert(*key), "duplicate feature-flag key: {key}");
        }
    }

    #[test]
    fn unknown_flag_is_disabled() {
        // FLAGS is the default empty snapshot in tests (no DB involved).
        assert!(!is_enabled(FeatureFlag::new("nonexistent"), None));
        assert!(!is_enabled(FeatureFlag::new("nonexistent"), Some("01J8TEST")));
    }

    #[test]
    fn disabled_is_off_for_everyone() {
        let e = entry(FlagState::Disabled, &["u1"]);
        assert!(!e.evaluate(None));
        assert!(!e.evaluate(Some("u1")));
    }

    #[test]
    fn enabled_is_on_for_everyone() {
        let e = entry(FlagState::Enabled, &[]);
        assert!(e.evaluate(None));
        assert!(e.evaluate(Some("u1")));
    }

    #[test]
    fn logged_in_requires_a_user() {
        let e = entry(FlagState::LoggedIn, &[]);
        assert!(!e.evaluate(None));
        assert!(e.evaluate(Some("anyone")));
    }

    #[test]
    fn scoped_requires_membership() {
        let e = entry(FlagState::Scoped, &["u1"]);
        assert!(!e.evaluate(None));
        assert!(!e.evaluate(Some("u2")));
        assert!(e.evaluate(Some("u1")));
    }

    #[test]
    fn flag_state_db_roundtrip() {
        for state in [
            FlagState::Disabled,
            FlagState::Enabled,
            FlagState::LoggedIn,
            FlagState::Scoped,
        ] {
            assert_eq!(FlagState::parse_db(state.as_str()), Some(state));
        }
        assert_eq!(FlagState::parse_db("bogus"), None);
    }
}
```

Note: `HashSet` is not yet imported at the top (only `HashMap`) — add `HashSet` to the `std::collections` import in this step since `FlagEntry` uses it:

```rust
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, LazyLock},
};
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `just backend test`
Expected: compile succeeds; **3 tests FAIL** (`enabled_is_on_for_everyone`, `logged_in_requires_a_user`, `scoped_requires_membership`) because `evaluate` is stubbed to `false`. The others pass vacuously.

- [ ] **Step 4: Implement `evaluate`**

Replace the stub in `impl FlagEntry`:

```rust
impl FlagEntry {
    /// Evaluates this flag for `user` (user id; `None` when anonymous).
    #[must_use]
    pub fn evaluate(&self, user: Option<&str>) -> bool {
        match self.state {
            FlagState::Disabled => false,
            FlagState::Enabled => true,
            FlagState::LoggedIn => user.is_some(),
            FlagState::Scoped => user.is_some_and(|u| self.scoped_users.contains(u)),
        }
    }
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `just backend test`
Expected: all tests in the `feature_flags::tests` module PASS (7 tests).

- [ ] **Step 6: Format/lint gate**

Run: `just backend fmt-dev`
Expected: exits clean; any clippy findings it can auto-fix are applied. If warnings remain that can't be auto-fixed, fix the code (do NOT add `#[allow]`).

- [ ] **Step 7: Commit**

```bash
git add backend/src/feature_flags.rs backend/src/main.rs
git commit -m "feat: feature flag core (newtype, registry, evaluation)"
```

---

### Task 2: Migration + persistence (seed, snapshot load/reload) + startup wiring

**Files:**
- Create: `backend/migrations/<ts>_feature_flags.up.sql` and `.down.sql` (via `just backend migrations-add feature_flags`)
- Modify: `backend/src/feature_flags.rs` (add `init`/`seed`/`reload`)
- Modify: `backend/src/server/mod.rs:21-26` (call `feature_flags::init()`)
- Regenerate: `backend/.sqlx/` (committed)

**Interfaces:**
- Consumes: `FeatureFlag`, `REGISTRY`, `FlagState::{as_str, parse_db}`, `FLAGS` from Task 1; `crate::database::Database::pool()`.
- Produces (used by Tasks 3, 5):
  - `crate::feature_flags::init()` — `pub async fn`, seeds registry keys then loads the snapshot
  - `crate::feature_flags::reload()` — `pub async fn`, rebuilds the snapshot from the DB (call after admin mutations)

- [ ] **Step 1: Create the migration**

Run from repo root: `just backend migrations-add feature_flags`
Expected: creates `backend/migrations/<timestamp>_feature_flags.up.sql` and `.down.sql` pair.

Put this in the `.up.sql`:

```sql
CREATE TABLE feature_flags (
  id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  key         TEXT NOT NULL UNIQUE,
  description TEXT NOT NULL DEFAULT '',
  state       TEXT NOT NULL DEFAULT 'disabled'
              CHECK (state IN ('disabled', 'enabled', 'logged_in', 'scoped')),
  updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE feature_flag_scoped_users (
  flag_id  BIGINT NOT NULL REFERENCES feature_flags(id) ON DELETE CASCADE,
  user_id  TEXT   NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  added_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (flag_id, user_id)
);
```

And this in the `.down.sql`:

```sql
DROP TABLE feature_flag_scoped_users;
DROP TABLE feature_flags;
```

- [ ] **Step 2: Add `init`, `seed`, `reload` to `backend/src/feature_flags.rs`**

Extend the imports at the top:

```rust
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, LazyLock},
};

use arc_swap::ArcSwap;
use serde::{Deserialize, Serialize};
use tracing::{error, warn};

use crate::database::Database;
```

Add below `enabled_map`:

```rust
/// Seeds registry keys (`ON CONFLICT DO NOTHING` — never clobbers admin state
/// or descriptions), then loads the snapshot. Runs once at startup, after
/// migrations.
pub async fn init() {
    seed().await;
    reload().await;
}

async fn seed() {
    if REGISTRY.is_empty() {
        return;
    }
    let keys: Vec<String> = REGISTRY.iter().map(|k| (*k).to_string()).collect();
    if let Err(e) = sqlx::query!(
        "INSERT INTO feature_flags (key)
         SELECT x FROM unnest($1::text[]) AS x
         ON CONFLICT (key) DO NOTHING",
        &keys,
    )
    .execute(&Database::pool())
    .await
    {
        error!(%e, "Failed to seed feature flags");
    }
}

/// Rebuilds the in-memory snapshot from the DB. Called at startup and after
/// every admin mutation. On failure, logs and keeps serving the previous
/// snapshot (next successful mutation or restart converges).
pub async fn reload() {
    let flags = match sqlx::query!(
        r#"
        SELECT key   AS "key!: String",
               state AS "state!: String"
        FROM feature_flags
        "#
    )
    .fetch_all(&Database::pool())
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            error!(%e, "Failed to load feature flags");
            return;
        }
    };

    let members = match sqlx::query!(
        r#"
        SELECT f.key     AS "key!: String",
               s.user_id AS "user_id!: String"
        FROM feature_flag_scoped_users s
        JOIN feature_flags f ON f.id = s.flag_id
        "#
    )
    .fetch_all(&Database::pool())
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            error!(%e, "Failed to load feature-flag scoped users");
            return;
        }
    };

    let mut map: HashMap<String, FlagEntry> = HashMap::new();
    for row in flags {
        match FlagState::parse_db(&row.state) {
            Some(state) => {
                map.insert(
                    row.key,
                    FlagEntry {
                        state,
                        scoped_users: Arc::new(HashSet::new()),
                    },
                );
            }
            None => warn!(state = %row.state, key = %row.key, "Unknown feature-flag state, skipping"),
        }
    }
    for row in members {
        if let Some(entry) = map.get_mut(&row.key) {
            Arc::get_mut(&mut entry.scoped_users)
                .expect("freshly built entries are uniquely owned")
                .insert(row.user_id);
        }
    }

    FLAGS.store(Arc::new(map));
}
```

- [ ] **Step 3: Wire startup in `backend/src/server/mod.rs`**

Add `feature_flags` to the `use crate::{...}` list (lines 11-16) and the call immediately after the `Database::init` block:

```rust
use crate::{
    admin, auth,
    cli::ServerConfig,
    database::Database,
    feature_flags,
    proto::{gbfs, gtfs_realtime, gtfs_schedule},
};
```

```rust
    if let Err(e) = Database::init(&server_config.database_url).await {
        error!(%e, "Failed to initialize database");
        return Err(anyhow::anyhow!(e).context("Failed to initialize database"));
    }

    feature_flags::init().await;
```

- [ ] **Step 4: Regenerate the sqlx offline cache**

Prerequisite: `backend/.env` has `DATABASE_URL=postgres://...` pointing at a running PostgreSQL.

Run: `just backend sqlx-regenerate`
Expected: succeeds; new files appear under `backend/.sqlx/` for the three new queries.

- [ ] **Step 5: Verify**

Run: `just backend test` → all tests pass.
Run: `just backend fmt-dev` → clean.
Run: `just backend migrations-list` → the `feature_flags` migration shows as applied (regeneration ran it against the temp DB).

- [ ] **Step 6: Commit (including the `.sqlx` cache)**

```bash
git add backend/migrations backend/src/feature_flags.rs backend/src/server/mod.rs backend/.sqlx
git commit -m "feat: feature flags migration, seeding and snapshot reload"
```

---

### Task 3: Expose evaluated flags in `/api/v1/capabilities`

**Files:**
- Modify: `backend/src/server/routes/v1/capabilities.rs` (whole file is 14 lines)

**Interfaces:**
- Consumes: `crate::feature_flags::enabled_map(user: Option<&str>) -> HashMap<String, bool>`; `crate::auth::resolve_current_user(&HeaderMap) -> Option<ResolvedUser>` (`ResolvedUser.user.id: String`).
- Produces: `GET /api/v1/capabilities` response gains `"featureFlags": { "<key>": true, ... }` — only flags enabled for the caller (anonymous callers: only `enabled`-state flags). Empty object when nothing is on.

- [ ] **Step 1: Rewrite `capabilities.rs`**

```rust
use axum::{Json, http::HeaderMap, response::IntoResponse};
use serde_json::json;

use crate::{
    auth::{self, config},
    feature_flags,
};

pub async fn get_capabilities(headers: HeaderMap) -> impl IntoResponse {
    let providers = config::get();
    let user = auth::resolve_current_user(&headers).await;
    let flag_user = user.as_ref().map(|r| r.user.id.as_str());

    Json(json!({
        "appUrl": providers.app_url,
        "auth": {
            "providers": providers.public_list(),
        },
        "featureFlags": feature_flags::enabled_map(flag_user),
    }))
}
```

Note: `resolve_current_user` is optional-lookup (returns `None` for anonymous/invalid — no 401). Keep it that way: capabilities must stay callable logged-out.

- [ ] **Step 2: Verify compile + formatting**

Run: `just backend fmt-dev` → clean.
Run: `just backend test` → pass.

- [ ] **Step 3: Verify manually**

With `backend/.env` set (`DATABASE_URL`, `BIND_TO`), start the dev server in a second terminal: `just backend dev-run`.
Then: `curl -s localhost:9011/api/v1/capabilities`
Expected JSON contains `"featureFlags":{}` (registry is empty; nothing enabled).

- [ ] **Step 4: Commit**

```bash
git add backend/src/server/routes/v1/capabilities.rs
git commit -m "feat: expose evaluated feature flags in capabilities"
```

---

### Task 4: WebSocket push — `FeatureFlagsChanged` transmission

**Files:**
- Modify: `backend/src/server/routes/v1/mod.rs:1049-1081` (`Broadcast` + `Transmission` enums) + new push helper
- Modify: `backend/src/server/routes/v1/ws/mod.rs:224-237` (`handle_transmission` match)

**Interfaces:**
- Consumes: `crate::feature_flags::enabled_map`; `V1_APP_STATE`, `Versioned`, `minicbor_serde`.
- Produces (used by Task 5):
  - `crate::server::routes::v1::broadcast_feature_flags_changed()` — `pub fn`, pokes every WS connection to re-evaluate and send its own flag map
  - New `Broadcast::FeatureFlags(HashMap<String, bool>)` wire variant (CBOR, camelCase-external-tag `"featureFlags"` — only-true entries, possibly empty map)

- [ ] **Step 1: Add the enum variants + push helper in `v1/mod.rs`**

Extend `Broadcast` (after `SimpleStops`):

```rust
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
    SimpleStops(Vec<Vec<MixedValue>>),
    /// The connection's full feature-flag set (only enabled flags, mapped to
    /// `true`). Whole-map replacement.
    FeatureFlags(std::collections::HashMap<String, bool>),
}
```

Extend `Transmission` (after `UserNotice`):

```rust
pub enum Transmission {
    Empty,
    BroadcastToAll(Bytes),
    /// Per-account notice(s) for `user_id` (broadcast to all connection tasks,
    /// each filters by its own user). `bytes` is a serialized `Broadcast::UserNotices`.
    UserNotice {
        user_id: String,
        bytes: Bytes,
    },
    /// Feature flags changed; every connection task evaluates and sends its
    /// own (user-specific) enabled-flag map.
    FeatureFlagsChanged,
}
```

Add the push helper next to `broadcast_notices` (after it, ~line 201):

```rust
/// Tell every WS connection to re-evaluate and push its own feature-flag set.
pub fn broadcast_feature_flags_changed() {
    if let Some(state) = V1_APP_STATE.get() {
        state.send_transmission(Transmission::FeatureFlagsChanged);
    }
}
```

- [ ] **Step 2: Handle the variant in `ws/mod.rs` `handle_transmission`**

In the `match transmission` inside `handle_transmission` (the `Transmission::BroadcastToAll` arm is at ~line 231), add a new arm:

```rust
        Transmission::FeatureFlagsChanged => {
            let flags = crate::feature_flags::enabled_map(user_id);
            let versioned = Versioned::new(1, Broadcast::FeatureFlags(flags));
            let Ok(bytes) = minicbor_serde::to_vec(&versioned) else {
                warn!(?addr, "Failed to serialize feature-flags broadcast");
                return true;
            };
            sender
                .send(Message::Binary(Bytes::from(bytes)))
                .await
                .is_ok()
        }
```

Check the file's existing `use` block first: it already uses `Bytes`, `Message`, `warn`, and `Transmission`. Add whatever is missing among:

```rust
use crate::{
    entity::util::versioned::Versioned,
    server::routes::v1::Broadcast,
};
```

(merge into the existing `use crate::{...}` if one exists; follow what `fmt-dev`'s grouped-import style produces).

- [ ] **Step 3: Verify**

Run: `just backend fmt-dev` → clean.
Run: `just backend test` → pass.
(Run-time WS verification happens in Task 10's manual pass.)

- [ ] **Step 4: Commit**

```bash
git add backend/src/server/routes/v1/mod.rs backend/src/server/routes/v1/ws/mod.rs
git commit -m "feat: push feature-flag updates over WS per connection"
```

---

### Task 5: Admin API — list / full-state PATCH / delete

**Files:**
- Create: `backend/src/admin/feature_flags.rs`
- Modify: `backend/src/admin/mod.rs:8-13` (module declaration)
- Modify: `backend/src/admin/router.rs` (imports, routes at lines 27-72, handlers)
- Regenerate: `backend/.sqlx/` (committed)

**Interfaces:**
- Consumes: `FlagState` (Task 1), `REGISTRY` (Task 1), `crate::feature_flags::reload` (Task 2), `crate::server::routes::v1::broadcast_feature_flags_changed` (Task 4), `crate::database::Database`, `crate::database::time::to_jiff`.
- Produces (used by Tasks 7-9, the admin frontend):
  - `GET /api/feature-flags` → `[{ id: number, key: string, description: string, state: "disabled"|"enabled"|"loggedIn"|"scoped", updatedAt: string, orphaned: boolean, scopedUsers: [{ id: string, displayName: string|null, email: string|null, avatarUrl: string|null }] }]`
  - `PATCH /api/feature-flags/{id}` body `{ state, description, userIds: string[] }` (all required, full state) → 200 with the updated row (same shape as GET entries); 404 unknown id; 400 `{ message }` unknown user ids
  - `DELETE /api/feature-flags/{id}` → 204; 404 unknown id

- [ ] **Step 1: Declare the module**

In `backend/src/admin/mod.rs`, add alphabetically (after `feedback`):

```rust
pub mod feature_flags;
```

- [ ] **Step 2: Create `backend/src/admin/feature_flags.rs`**

```rust
//! Admin-side feature-flag management: listing (with hydrated scoped users),
//! full-state update (state + description + scoped-user set, reconciled as a
//! diff), and deletion. Mutating router handlers must call
//! [`crate::feature_flags::reload`] and
//! [`crate::server::routes::v1::broadcast_feature_flags_changed`] afterward.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::{
    database::Database,
    feature_flags::{FlagState, REGISTRY},
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopedUserRow {
    pub id: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureFlagRow {
    pub id: i64,
    pub key: String,
    pub description: String,
    pub state: FlagState,
    pub updated_at: jiff::Timestamp,
    /// Key no longer exists in the code registry (dead metadata, deletable).
    pub orphaned: bool,
    pub scoped_users: Vec<ScopedUserRow>,
}

/// Errors from [`update`].
pub enum UpdateFlagError {
    /// No flag row with that id.
    NotFound,
    /// Scoped-user ids that don't match any account.
    UnknownUsers(Vec<String>),
    Db(sqlx::Error),
}

impl From<sqlx::Error> for UpdateFlagError {
    fn from(e: sqlx::Error) -> Self {
        Self::Db(e)
    }
}

struct FlagRecord {
    id: i64,
    key: String,
    description: String,
    state: String,
    updated_at: time::OffsetDateTime,
}

fn to_row(record: FlagRecord, users: Vec<ScopedUserRow>) -> Option<FeatureFlagRow> {
    Some(FeatureFlagRow {
        id: record.id,
        orphaned: !REGISTRY.contains(&record.key.as_str()),
        key: record.key,
        description: record.description,
        state: FlagState::parse_db(&record.state)?,
        updated_at: crate::database::time::to_jiff(record.updated_at),
        scoped_users: users,
    })
}

/// All flags with hydrated scoped users, ordered by key.
pub async fn list() -> Result<Vec<FeatureFlagRow>, sqlx::Error> {
    let flags = sqlx::query!(
        r#"
        SELECT id          AS "id!: i64",
               key         AS "key!: String",
               description AS "description!: String",
               state       AS "state!: String",
               updated_at  AS "updated_at!: time::OffsetDateTime"
        FROM feature_flags
        ORDER BY key
        "#
    )
    .fetch_all(&Database::pool())
    .await?;

    let users = sqlx::query!(
        r#"
        SELECT s.flag_id     AS "flag_id!: i64",
               u.id          AS "user_id!: String",
               u.display_name,
               u.email,
               u.avatar_url
        FROM feature_flag_scoped_users s
        JOIN users u ON u.id = s.user_id
        ORDER BY u.id
        "#
    )
    .fetch_all(&Database::pool())
    .await?;

    let mut grouped: HashMap<i64, Vec<ScopedUserRow>> = HashMap::new();
    for r in users {
        grouped.entry(r.flag_id).or_default().push(ScopedUserRow {
            id: r.user_id,
            display_name: r.display_name,
            email: r.email,
            avatar_url: r.avatar_url,
        });
    }

    Ok(flags
        .into_iter()
        .filter_map(|f| {
            to_row(
                FlagRecord {
                    id: f.id,
                    key: f.key,
                    description: f.description,
                    state: f.state,
                    updated_at: f.updated_at,
                },
                grouped.remove(&f.id).unwrap_or_default(),
            )
        })
        .collect())
}

async fn get(id: i64) -> Result<Option<FeatureFlagRow>, sqlx::Error> {
    let Some(f) = sqlx::query!(
        r#"
        SELECT id          AS "id!: i64",
               key         AS "key!: String",
               description AS "description!: String",
               state       AS "state!: String",
               updated_at  AS "updated_at!: time::OffsetDateTime"
        FROM feature_flags
        WHERE id = $1
        "#,
        id,
    )
    .fetch_optional(&Database::pool())
    .await?
    else {
        return Ok(None);
    };

    let users = sqlx::query!(
        r#"
        SELECT u.id           AS "user_id!: String",
               u.display_name,
               u.email,
               u.avatar_url
        FROM feature_flag_scoped_users s
        JOIN users u ON u.id = s.user_id
        WHERE s.flag_id = $1
        ORDER BY u.id
        "#,
        id,
    )
    .fetch_all(&Database::pool())
    .await?;

    let record = FlagRecord {
        id: f.id,
        key: f.key,
        description: f.description,
        state: f.state,
        updated_at: f.updated_at,
    };
    Ok(to_row(
        record,
        users
            .into_iter()
            .map(|u| ScopedUserRow {
                id: u.user_id,
                display_name: u.display_name,
                email: u.email,
                avatar_url: u.avatar_url,
            })
            .collect(),
    ))
}

/// Full-state update: writes state + description, then reconciles the
/// scoped-user set as a diff (inserts new ids, deletes removed ids; retained
/// ids keep their `added_at`). Returns the updated row.
pub async fn update(
    id: i64,
    state: FlagState,
    description: &str,
    user_ids: &[String],
) -> Result<FeatureFlagRow, UpdateFlagError> {
    let mut tx = Database::pool().begin().await?;

    let wanted: HashSet<&str> = user_ids.iter().map(String::as_str).collect();

    let found = sqlx::query!(
        r#"
        SELECT id AS "id!: String"
        FROM users
        WHERE id = ANY($1)
        "#,
        user_ids,
    )
    .fetch_all(&mut *tx)
    .await?;
    let found_ids: HashSet<&str> = found.iter().map(|r| r.id.as_str()).collect();
    let missing: Vec<String> = wanted
        .difference(&found_ids)
        .map(|s| (*s).to_string())
        .collect();
    if !missing.is_empty() {
        return Err(UpdateFlagError::UnknownUsers(missing));
    }

    let updated = sqlx::query!(
        r#"
        UPDATE feature_flags
        SET state = $2, description = $3, updated_at = now()
        WHERE id = $1
        RETURNING id AS "id!: i64"
        "#,
        id,
        state.as_str(),
        description,
    )
    .fetch_optional(&mut *tx)
    .await?;
    if updated.is_none() {
        return Err(UpdateFlagError::NotFound);
    }

    let wanted_vec: Vec<String> = wanted.into_iter().map(str::to_string).collect();

    sqlx::query!(
        "DELETE FROM feature_flag_scoped_users WHERE flag_id = $1 AND NOT (user_id = ANY($2))",
        id,
        &wanted_vec,
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query!(
        r#"
        INSERT INTO feature_flag_scoped_users (flag_id, user_id)
        SELECT $1, x FROM unnest($2::text[]) AS x
        ON CONFLICT DO NOTHING
        "#,
        id,
        &wanted_vec,
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    get(id).await?.ok_or(UpdateFlagError::NotFound)
}

/// Delete a flag row (cascades scoped users). `true` when a row was removed.
pub async fn delete(id: i64) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!("DELETE FROM feature_flags WHERE id = $1", id)
        .execute(&Database::pool())
        .await?;
    Ok(result.rows_affected() > 0)
}
```

- [ ] **Step 3: Register routes + handlers in `backend/src/admin/router.rs`**

Add `patch` to the routing imports (line 8):

```rust
    routing::{delete, get, patch, post, put},
```

Register routes in `create_admin_router` (after the `/user-notices` routes, before `.layer(...)`):

```rust
        .route("/feature-flags", get(list_feature_flags))
        .route(
            "/feature-flags/{id}",
            patch(update_feature_flag).delete(delete_feature_flag),
        )
```

Add handlers at the end of the file:

```rust
// --- Feature flags ---

async fn list_feature_flags() -> impl IntoResponse {
    match admin::feature_flags::list().await {
        Ok(flags) => axum::Json(flags).into_response(),
        Err(e) => {
            warn!(error = %e, "Failed to list feature flags");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateFeatureFlagRequest {
    state: crate::feature_flags::FlagState,
    description: String,
    user_ids: Vec<String>,
}

async fn update_feature_flag(
    Path(id): Path<i64>,
    axum::Json(body): axum::Json<UpdateFeatureFlagRequest>,
) -> Response {
    let result =
        admin::feature_flags::update(id, body.state, &body.description, &body.user_ids).await;
    match result {
        Ok(flag) => {
            crate::feature_flags::reload().await;
            crate::server::routes::v1::broadcast_feature_flags_changed();
            axum::Json(flag).into_response()
        }
        Err(admin::feature_flags::UpdateFlagError::NotFound) => {
            StatusCode::NOT_FOUND.into_response()
        }
        Err(admin::feature_flags::UpdateFlagError::UnknownUsers(missing)) => (
            StatusCode::BAD_REQUEST,
            format!("Unknown user ids: {}", missing.join(", ")),
        )
            .into_response(),
        Err(admin::feature_flags::UpdateFlagError::Db(e)) => {
            warn!(error = %e, id, "Failed to update feature flag");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn delete_feature_flag(Path(id): Path<i64>) -> Response {
    match admin::feature_flags::delete(id).await {
        Ok(true) => {
            crate::feature_flags::reload().await;
            crate::server::routes::v1::broadcast_feature_flags_changed();
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            warn!(error = %e, id, "Failed to delete feature flag");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
```

- [ ] **Step 4: Regenerate sqlx cache + gates**

Run: `just backend sqlx-regenerate` → succeeds (six new queries).
Run: `just backend fmt-dev` → clean. Run: `just backend test` → pass.

- [ ] **Step 5: Verify manually with curl**

Start the server (`just backend dev-run`, needs `ADMIN_KEY` + `ADMIN_BIND_TO` in `backend/.env`; admin listener default in dev setups is `localhost:9013`). Then, with `$KEY` = the admin key:

```bash
# List (empty — registry is empty, nothing seeded)
curl -s -H "Authorization: Bearer $KEY" localhost:9013/api/feature-flags
# Expected: []

# Insert a scratch flag row directly (simulating an orphaned flag)
psql "$DATABASE_URL" -c "INSERT INTO feature_flags (key, description) VALUES ('scratch_demo', 'temp test flag')"

curl -s -H "Authorization: Bearer $KEY" localhost:9013/api/feature-flags
# Expected: one row, "state":"disabled","orphaned":true,"scopedUsers":[]

# Grab a real user id
curl -s -H "Authorization: Bearer $KEY" localhost:9013/api/users

# PATCH to scoped with that user (id = 1 from the list response below; substitute)
curl -s -X PATCH -H "Authorization: Bearer $KEY" -H "Content-Type: application/json" \
  -d '{"state":"scoped","description":"temp","userIds":["<USER_ID>"]}' \
  localhost:9013/api/feature-flags/1
# Expected: updated row JSON with scopedUsers containing the user

# PATCH with a bogus user -> 400
curl -s -o /dev/null -w "%{http_code}\n" -X PATCH -H "Authorization: Bearer $KEY" \
  -H "Content-Type: application/json" \
  -d '{"state":"disabled","description":"","userIds":["nope"]}' \
  localhost:9013/api/feature-flags/1
# Expected: 400

# DELETE -> 204; second DELETE -> 404
curl -s -o /dev/null -w "%{http_code}\n" -X DELETE -H "Authorization: Bearer $KEY" localhost:9013/api/feature-flags/1
curl -s -o /dev/null -w "%{http_code}\n" -X DELETE -H "Authorization: Bearer $KEY" localhost:9013/api/feature-flags/1
```

- [ ] **Step 6: Commit**

```bash
git add backend/src/admin backend/src/admin/router.rs backend/.sqlx
git commit -m "feat: admin API for feature flags"
```

---

### Task 6: Public frontend — flag store, capabilities hydration, WS frame

**Files:**
- Create: `frontend/src/feature-flags-store.ts`
- Modify: `frontend/src/app/entity/v1/auth.ts:3-16` (capabilities schema)
- Modify: `frontend/src/hooks/use-capabilities.ts:7-35` (hydrate store)
- Modify: `frontend/src/app/entity/v1/message.ts:10-67` (WS schema)
- Modify: `frontend/src/hooks/use-websocket.ts:26-58` (dispatch branch)

**Interfaces:**
- Consumes: capabilities response now has `featureFlags`; WS frames may carry `{ featureFlags: Record<string, boolean> }`.
- Produces (for feature code from now on):
  - `featureFlagsStore` — Zustand store `{ flags: Record<string, boolean> }`
  - `setFeatureFlags(flags: Record<string, boolean>): void` — wholesale replacement
  - `useFeatureFlag(key: string): boolean` — fail-closed (`false` when absent)

- [ ] **Step 1: Create `frontend/src/feature-flags-store.ts`**

```ts
import { create } from "zustand";

type FeatureFlagsState = {
  /** Only flags the backend reports as enabled for the current user. */
  flags: Record<string, boolean>;
};

export const featureFlagsStore = create<FeatureFlagsState>()(() => ({
  flags: {},
}));

export function setFeatureFlags(flags: Record<string, boolean>): void {
  featureFlagsStore.setState({ flags });
}

/** Whether `key` is enabled for the current user. Fail-closed: unknown/absent = false. */
export function useFeatureFlag(key: string): boolean {
  return featureFlagsStore((s) => s.flags[key] ?? false);
}
```

- [ ] **Step 2: Extend the capabilities schema** (`frontend/src/app/entity/v1/auth.ts`)

```ts
export const capabilitiesSchema = z.object({
  appUrl: z.string().nullable(),
  auth: z.object({
    providers: z.array(providerPublicSchema),
  }),
  featureFlags: z.record(z.string(), z.boolean()).catch({}),
});
```

(`.catch({})` keeps an older backend's response parseable; the dist ships in the same binary anyway.)

- [ ] **Step 3: Hydrate the store in `use-capabilities.ts`**

Add the import and one line in `load()` right after `capabilitiesStore.setState({...})`:

```ts
import { setFeatureFlags } from "@/feature-flags-store";
```

```ts
      capabilitiesStore.setState({
        providers: data?.auth.providers ?? [],
        backendOrigin,
        loading: false,
      });
      setFeatureFlags(data?.featureFlags ?? {});
```

- [ ] **Step 4: Add the WS message variant** (`frontend/src/app/entity/v1/message.ts`)

Append one more `.or(...)` to the `v1MessageSchema` union chain (after the `gbfsStations` entry):

```ts
    .or(z.object({ featureFlags: z.record(z.string(), z.boolean()) })),
```

- [ ] **Step 5: Dispatch the frame** (`frontend/src/hooks/use-websocket.ts`)

Add the import, then a branch in the `processed-message` case — after the `"toast" in data` branch, **before** `processMessage(response.data)`:

```ts
import { setFeatureFlags } from "@/feature-flags-store";
```

```ts
      if (typeof data === "object" && "featureFlags" in data) {
        setFeatureFlags(data.featureFlags);
        return;
      }
```

- [ ] **Step 6: Verify + commit**

Run: `just frontend check` → clean (type-check + lint-check + fmt-check; run `just frontend fix` first if prettier reflow is needed, then re-check).

```bash
git add frontend/src
git commit -m "feat: frontend feature-flag store with WS live updates"
```

---

### Task 7: Admin frontend — zod schemas + query hooks

**Files:**
- Modify: `frontend-admin/src/entity/schemas.ts` (append)
- Modify: `frontend-admin/src/lib/queries.ts` (qk + hooks)

**Interfaces:**
- Consumes: the admin API shapes from Task 5; existing `api` wrapper, `parse` helper, `useUsers`/`qk.users`.
- Produces (used by Tasks 8-9):
  - `flagStateSchema`, `flagStates` (`readonly ["disabled","enabled","loggedIn","scoped"]`), `FlagState`
  - `scopedUserSchema`, `ScopedUser`
  - `featureFlagRowSchema`, `FeatureFlagRow` — `{ id: number; key: string; description: string; state: FlagState; updatedAt: string; orphaned: boolean; scopedUsers: ScopedUser[] }`
  - `featureFlagUpdateSchema`, `FeatureFlagUpdate` — `{ state: FlagState; description: string; userIds: string[] }`
  - `qk.featureFlags`, `useFeatureFlags()`, `useUpdateFeatureFlag()`, `useDeleteFeatureFlag()`

- [ ] **Step 1: Append schemas to `frontend-admin/src/entity/schemas.ts`**

```ts
export const flagStateSchema = z.enum(["disabled", "enabled", "loggedIn", "scoped"]);
export const flagStates = flagStateSchema.options;
export type FlagState = z.infer<typeof flagStateSchema>;

export const scopedUserSchema = z.object({
  id: z.string(),
  displayName: z.string().nullable().optional(),
  email: z.string().nullable().optional(),
  avatarUrl: z.string().nullable().optional(),
});
export type ScopedUser = z.infer<typeof scopedUserSchema>;

export const featureFlagRowSchema = z.object({
  id: z.number(),
  key: z.string(),
  description: z.string(),
  state: flagStateSchema,
  updatedAt: z.string(),
  orphaned: z.boolean(),
  scopedUsers: z.array(scopedUserSchema),
});
export type FeatureFlagRow = z.infer<typeof featureFlagRowSchema>;

export const featureFlagUpdateSchema = z.object({
  state: flagStateSchema,
  description: z.string(),
  userIds: z.array(z.string()),
});
export type FeatureFlagUpdate = z.infer<typeof featureFlagUpdateSchema>;
```

- [ ] **Step 2: Add `qk.featureFlags` and hooks to `frontend-admin/src/lib/queries.ts`**

Add to the `qk` map:

```ts
  featureFlags: ["feature-flags"] as const,
```

Add the hooks (next to the user-notice hooks):

```ts
export function useFeatureFlags() {
  return useQuery({
    queryKey: qk.featureFlags,
    queryFn: async ({ signal }) =>
      parse(featureFlagRowSchema.array(), await api.get("/feature-flags", signal)),
  });
}

export function useUpdateFeatureFlag() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ id, body }: { id: number; body: FeatureFlagUpdate }) =>
      parse(featureFlagRowSchema, await api.patch<FeatureFlagRow>(`/feature-flags/${id}`, body)),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: qk.featureFlags });
    },
  });
}

export function useDeleteFeatureFlag() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (id: number) => api.del(`/feature-flags/${id}`),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: qk.featureFlags });
    },
  });
}
```

Add the schema imports to the file's existing `entity/schemas` import block (`featureFlagRowSchema`, `featureFlagUpdateSchema`, and types `FeatureFlagRow`, `FeatureFlagUpdate`).

- [ ] **Step 3: Verify + commit**

Run: `just frontend-admin check` → clean.

```bash
git add frontend-admin/src
git commit -m "feat: admin schemas and queries for feature flags"
```

---

### Task 8: Admin frontend — the flag drawer component

**Files:**
- Create: `frontend-admin/src/components/flag-drawer.tsx`

**Interfaces:**
- Consumes: `FeatureFlagRow`, `FlagState`, `flagStates` (Task 7); `useUpdateFeatureFlag`, `useUsers` (Task 7 + existing); `Button`/`Input`/`Textarea`/`Badge`, `userLabel`.
- Produces: `<FlagDrawer flag={row} onClose={() => void} />` — slide-over editing surface with local state and a single full-state Save (used by Task 9).

- [ ] **Step 1: Create `frontend-admin/src/components/flag-drawer.tsx`**

```tsx
import { useEffect, useMemo, useState } from "react";
import { toast } from "sonner";

import { Badge, Button, Input, Textarea } from "@/components/ui";
import { type FeatureFlagRow, type FlagState, flagStates } from "@/entity/schemas";
import { useUpdateFeatureFlag, useUsers } from "@/lib/queries";
import { userLabel } from "@/lib/utils";

function sameSet(a: string[], b: string[]): boolean {
  if (a.length !== b.length) return false;
  const setB = new Set(b);
  return a.every((x) => setB.has(x));
}

function stateLabel(state: FlagState): string {
  if (state === "loggedIn") return "Logged in";
  return state.charAt(0).toUpperCase() + state.slice(1);
}

export function FlagDrawer({ flag, onClose }: { flag: FeatureFlagRow; onClose: () => void }) {
  const users = useUsers();
  const update = useUpdateFeatureFlag();

  const [state, setState] = useState<FlagState>(flag.state);
  const [description, setDescription] = useState(flag.description);
  const [userIds, setUserIds] = useState<string[]>(flag.scopedUsers.map((u) => u.id));
  const [query, setQuery] = useState("");

  useEffect(() => {
    setState(flag.state);
    setDescription(flag.description);
    setUserIds(flag.scopedUsers.map((u) => u.id));
    setQuery("");
  }, [flag]);

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") onClose();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const originalUserIds = flag.scopedUsers.map((u) => u.id);
  const dirty =
    state !== flag.state ||
    description !== flag.description ||
    !sameSet(userIds, originalUserIds);

  const selectedUsers = useMemo(
    () =>
      userIds.map(
        (id) =>
          users.data?.find((u) => u.id === id) ??
          flag.scopedUsers.find((u) => u.id === id) ?? {
            id,
            displayName: null,
            email: null,
            providers: [],
            createdAt: "",
            noticeCount: 0,
          },
      ),
    [userIds, users.data, flag.scopedUsers],
  );

  const results = useMemo(() => {
    const q = query.trim().toLowerCase();
    return (users.data ?? [])
      .filter((u) => !userIds.includes(u.id))
      .filter(
        (u) =>
          q === "" || `${u.displayName ?? ""} ${u.email ?? ""} ${u.id}`.toLowerCase().includes(q),
      )
      .slice(0, 8);
  }, [users.data, userIds, query]);

  async function save() {
    try {
      await update.mutateAsync({ id: flag.id, body: { state, description, userIds } });
      toast.success("Flag saved");
    } catch (e) {
      toast.error(`Failed: ${e instanceof Error ? e.message : ""}`);
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex justify-end">
      <button
        type="button"
        aria-label="Close"
        className="absolute inset-0 cursor-default bg-black/50"
        onClick={onClose}
      />
      <aside className="bg-bg border-border-soft relative flex h-full w-full max-w-md flex-col gap-4 overflow-y-auto border-l p-6 shadow-2xl">
        <div className="flex items-start justify-between gap-2">
          <div className="flex flex-wrap items-center gap-2">
            <h2 className="font-mono text-lg font-semibold text-[#f8fafc]">{flag.key}</h2>
            {flag.orphaned ? (
              <Badge className="bg-[#7f1d1d] text-[#fca5a5]">orphaned — not in code</Badge>
            ) : null}
          </div>
          <Button variant="secondary" onClick={onClose}>
            ✕
          </Button>
        </div>

        <div>
          <p className="text-text-muted mb-1 text-xs font-semibold tracking-wide uppercase">
            State
          </p>
          <div className="flex flex-wrap gap-1">
            {flagStates.map((s) => (
              <Button
                key={s}
                variant={state === s ? "primary" : "secondary"}
                onClick={() => {
                  setState(s);
                }}
              >
                {stateLabel(s)}
              </Button>
            ))}
          </div>
        </div>

        <div>
          <p className="text-text-muted mb-1 text-xs font-semibold tracking-wide uppercase">
            Description
          </p>
          <Textarea
            value={description}
            onChange={(e) => {
              setDescription(e.target.value);
            }}
            placeholder="What does this flag gate?"
          />
        </div>

        <div>
          <p className="text-text-muted mb-1 text-xs font-semibold tracking-wide uppercase">
            Scoped users ({userIds.length})
          </p>
          <p className="text-text-dim mb-2 text-xs">Applies only while the state is Scoped.</p>
          <Input
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
            }}
            placeholder="Search users by name, email, id…"
          />
          {results.length > 0 ? (
            <div className="border-border-soft bg-surface mt-1 rounded border">
              {results.map((u) => (
                <button
                  key={u.id}
                  type="button"
                  className="hover:bg-border block w-full cursor-pointer px-2 py-1.5 text-left text-xs"
                  onClick={() => {
                    setUserIds((prev) => [...prev, u.id]);
                    setQuery("");
                  }}
                >
                  {userLabel(u)}
                </button>
              ))}
            </div>
          ) : null}
          <div className="mt-2 flex flex-wrap gap-1">
            {selectedUsers.map((u) => (
              <span
                key={u.id}
                className="border-border bg-surface text-text inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-xs"
              >
                {userLabel(u)}
                <button
                  type="button"
                  aria-label={`Remove ${userLabel(u)}`}
                  className="text-text-dim hover:text-[#fca5a5] cursor-pointer"
                  onClick={() => {
                    setUserIds((prev) => prev.filter((id) => id !== u.id));
                  }}
                >
                  ✕
                </button>
              </span>
            ))}
            {selectedUsers.length === 0 ? (
              <span className="text-text-dim text-xs italic">No users selected</span>
            ) : null}
          </div>
        </div>

        <div className="mt-auto flex justify-end gap-2 pt-4">
          <Button variant="secondary" onClick={onClose}>
            Close
          </Button>
          <Button disabled={!dirty || update.isPending} onClick={() => void save()}>
            {update.isPending ? "Saving…" : "Save"}
          </Button>
        </div>
      </aside>
    </div>
  );
}
```

- [ ] **Step 2: Verify + commit**

Run: `just frontend-admin check` → clean (`just frontend-admin fix` first if prettier reflow needed).

```bash
git add frontend-admin/src/components/flag-drawer.tsx
git commit -m "feat: admin feature-flag drawer component"
```

---

### Task 9: Admin frontend — flags page, route with `?flag=` search param, nav

**Files:**
- Create: `frontend-admin/src/routes/feature-flags.tsx`
- Modify: `frontend-admin/src/router.tsx` (import, route, `addChildren`)
- Modify: `frontend-admin/src/components/layout.tsx:12-20` (nav entry)

**Interfaces:**
- Consumes: `useFeatureFlags`, `useDeleteFeatureFlag` (Task 7); `FlagDrawer` (Task 8); `DataTable`, `useUsers` not needed here (drawer handles it).
- Produces: `/feature-flags` page (table + drawer, deep-linkable via `?flag=<key>`).

- [ ] **Step 1: Create `frontend-admin/src/routes/feature-flags.tsx`**

```tsx
import type { ColumnDef } from "@tanstack/react-table";
import { useNavigate, useSearch } from "@tanstack/react-router";
import { toast } from "sonner";

import { DataTable } from "@/components/data-table";
import { FlagDrawer } from "@/components/flag-drawer";
import { Badge, Button, Card, Empty, Spinner } from "@/components/ui";
import { type FeatureFlagRow, type FlagState } from "@/entity/schemas";
import { useDeleteFeatureFlag, useFeatureFlags } from "@/lib/queries";
import { confirmAction } from "@/lib/utils";

const FLAG_STATE_CLASS: Record<FlagState, string> = {
  disabled: "bg-[#374151] text-[#d1d5db]",
  enabled: "bg-[#065f46] text-[#6ee7b7]",
  loggedIn: "bg-[#1e3a5f] text-[#93c5fd]",
  scoped: "bg-[#713f12] text-[#fde68a]",
};

function FlagStateBadge({ state }: { state: FlagState }) {
  return (
    <Badge className={FLAG_STATE_CLASS[state]}>{state === "loggedIn" ? "logged in" : state}</Badge>
  );
}

export function FeatureFlagsRoute() {
  const navigate = useNavigate();
  const search = useSearch({ strict: false });
  const { data, isLoading, isError } = useFeatureFlags();
  const remove = useDeleteFeatureFlag();

  const flags = data ?? [];
  const flagKey = typeof search.flag === "string" ? search.flag : undefined;
  const selected = flags.find((f) => f.key === flagKey);

  async function handleDelete(flag: FeatureFlagRow) {
    if (!confirmAction(`Delete the "${flag.key}" flag? Its scoped users will be removed too.`)) {
      return;
    }
    try {
      await remove.mutateAsync(flag.id);
      void navigate({ to: "/feature-flags", search: {}, replace: true });
      toast.success("Flag deleted");
    } catch (e) {
      toast.error(`Failed: ${e instanceof Error ? e.message : ""}`);
    }
  }

  const columns: ColumnDef<FeatureFlagRow>[] = [
    {
      header: "Key",
      accessorKey: "key",
      cell: ({ row }) => <span className="font-mono text-[#cbd5e1]">{row.original.key}</span>,
    },
    {
      header: "Description",
      accessorKey: "description",
      cell: ({ row }) => (
        <span className="text-text-muted">{row.original.description || "—"}</span>
      ),
    },
    {
      header: "State",
      accessorKey: "state",
      cell: ({ row }) => <FlagStateBadge state={row.original.state} />,
    },
    {
      header: "Users",
      accessorFn: (f) => (f.state === "scoped" ? f.scopedUsers.length : -1),
      cell: ({ row }) =>
        row.original.state === "scoped" ? row.original.scopedUsers.length : "—",
    },
    {
      header: "Updated",
      accessorFn: (f) => f.updatedAt,
      cell: ({ row }) => (
        <span className="text-text-dim">{new Date(row.original.updatedAt).toLocaleString()}</span>
      ),
    },
    {
      id: "actions",
      header: "",
      enableSorting: false,
      enableGlobalFilter: false,
      cell: ({ row }) =>
        row.original.orphaned ? (
          <div className="flex items-center gap-2">
            <Badge className="bg-[#7f1d1d] text-[#fca5a5]">orphaned</Badge>
            <Button
              variant="danger"
              className="px-2 py-1 text-[0.7rem]"
              onClick={(e) => {
                e.stopPropagation();
                void handleDelete(row.original);
              }}
            >
              Delete
            </Button>
          </div>
        ) : null,
    },
  ];

  return (
    <div>
      <h1 className="mb-3 text-xl font-semibold text-[#f8fafc]">Feature Flags</h1>
      <Card>
        {isLoading ? (
          <Spinner />
        ) : isError ? (
          <Empty>Failed to load feature flags.</Empty>
        ) : flags.length === 0 ? (
          <Empty>No feature flags.</Empty>
        ) : (
          <DataTable
            columns={columns}
            data={flags}
            searchAccessor={(f) => `${f.key} ${f.description}`}
            searchPlaceholder="Search flags…"
            onRowClick={(f) => {
              void navigate({ to: "/feature-flags", search: { flag: f.key } });
            }}
            emptyMessage="No feature flags."
          />
        )}
      </Card>
      {selected ? (
        <FlagDrawer
          flag={selected}
          onClose={() => {
            void navigate({ to: "/feature-flags", search: {}, replace: true });
          }}
        />
      ) : null}
    </div>
  );
}
```

- [ ] **Step 2: Register the route in `frontend-admin/src/router.tsx`**

Add the import with the other route imports:

```tsx
import { FeatureFlagsRoute } from "@/routes/feature-flags";
```

Define the route (mirroring `feedbackRoute`'s `validateSearch` pattern):

```tsx
const featureFlagsRoute = createRoute({
  getParentRoute: () => layoutRoute,
  path: "feature-flags",
  component: FeatureFlagsRoute,
  validateSearch: z.object({
    flag: z.string().optional(),
  }),
});
```

Add `featureFlagsRoute` to `layoutRoute.addChildren([...])` (e.g. after `feedbackRoute`).

- [ ] **Step 3: Add the nav entry in `frontend-admin/src/components/layout.tsx`**

```tsx
const NAV: NavItem[] = [
  { to: "/", label: "Dashboard" },
  { to: "/settings", label: "Settings" },
  { to: "/auth", label: "Auth" },
  { to: "/feature-flags", label: "Feature Flags" },
  { to: "/notices", label: "Notices" },
  { to: "/notifications", label: "Send Notification" },
  { to: "/sync", label: "Sync" },
  { to: "/feedback", label: "Feedback" },
];
```

- [ ] **Step 4: Verify + commit**

Run: `just frontend-admin check` → clean.

```bash
git add frontend-admin/src
git commit -m "feat: admin feature flags page with deep-linkable drawer"
```

---

### Task 10: Docs, full build, end-to-end manual pass

**Files:**
- Modify: `backend/AGENTS.md` (Architecture section — one bullet)

**Interfaces:**
- Consumes: everything above.
- Produces: a verified, shippable branch.

- [ ] **Step 1: Document the subsystem in `backend/AGENTS.md`**

In the Architecture bullet list, after the "Admin API + UI" bullet, add:

```markdown
- **Feature flags**: code-first registry in `src/feature_flags.rs` (keys seeded
  into the `feature_flags` table at startup, `ON CONFLICT DO NOTHING`), an
  `ArcSwap` in-memory snapshot for hot-path checks (`is_enabled`, fail closed),
  admin CRUD at `/api/feature-flags` on the admin listener, and per-user
  exposure via `/api/v1/capabilities` (`featureFlags`, only-enabled entries)
  plus the WS `FeatureFlagsChanged` per-connection push.
```

- [ ] **Step 2: Full build (all three components)**

Run from repo root: `just build`
Expected: frontend build, frontend-admin build, and the release backend build (SQLX_OFFLINE) all succeed. This catches any missed `.sqlx` regeneration and verifies the embedded dist requirement.

- [ ] **Step 3: End-to-end manual pass**

Temporarily add a scratch flag to the registry in `backend/src/feature_flags.rs`:

```rust
feature_flags! {
    scratch_demo => "scratch_demo",
}
```

Rebuild + run (`just backend dev-run`), then verify, in order:

1. **Seeding**: `curl -s -H "Authorization: Bearer $KEY" localhost:9013/api/feature-flags` shows `scratch_demo`, state `disabled`, `orphaned: false`. Restart the server — the row is not duplicated and stays `disabled`.
2. **Capabilities anonymous**: `curl -s localhost:9011/api/v1/capabilities` → `"featureFlags":{}`.
3. **Enable**: PATCH the flag to `{"state":"enabled","description":"","userIds":[]}` → capabilities now shows `{"scratch_demo":true}` for the anonymous curl.
4. **LoggedIn**: PATCH to `loggedIn` → anonymous capabilities show `{}` again; with a session token (`Authorization: Bearer <user token>` from the frontend localStorage) capabilities show `{"scratch_demo":true}`.
5. **Scoped**: PATCH to `scoped` with one real `userId` → that user's capabilities show the flag; a different logged-in user's show `{}`.
6. **WS push**: open the public frontend in a browser (its WS connects), PATCH the flag `enabled` → `disabled`; the browser devtools WS tab shows a binary frame arriving (decode optional). With Task 6 code in place, `featureFlagsStore.getState()` in the console reflects the change after the frame.
7. **Drawer deep link**: open the admin UI → Feature Flags → click the row (URL becomes `/feature-flags?flag=scratch_demo`) → reload the page → the drawer re-opens on that flag. Edit description, switch state, add 3 users at once, remove 2 at once, Save → one network request; the table updates.
8. **Orphan lifecycle**: comment the `scratch_demo` registry entry back out, restart → the row shows the `orphaned` badge + Delete; deleting works; a further restart does not re-seed it.
9. **Cleanup**: remove the scratch flag from the registry (registry ships empty), delete its DB row if still present.

- [ ] **Step 4: Final gates**

Run: `just backend fmt-dev && just backend test && just frontend check && just frontend-admin check`
Expected: all clean.

- [ ] **Step 5: Commit**

```bash
git add backend/AGENTS.md
git commit -m "docs: document feature flags in backend AGENTS.md"
```

(Small fixes discovered during the manual pass get their own commits on the branch.)

---

## Self-review notes (already applied)

- Spec coverage: §2 semantics → Task 1; §4 schema → Task 2; §5 registry/seeding → Tasks 1-2; §6 snapshot/eval → Tasks 1-2; §7 admin API → Task 5 (+ reload/push wiring); §8 capabilities + WS → Tasks 3-4; §9 public frontend → Task 6; §10 admin UI → Tasks 7-9; §11 error handling → embedded in Tasks 2/5 (fail-closed, 404/400/500, log-and-keep-snapshot); §12 testing/verification → per-task steps + Task 10; §14 file map matches the tasks above.
- Type consistency: `FlagState` camelCase JSON (`loggedIn`) everywhere; admin row shape identical between Rust `FeatureFlagRow` (serde camelCase) and zod `featureFlagRowSchema`; `useFeatureFlag` fail-closed; `enabled_map` returns only-true entries in both capabilities and the WS frame.
- `just frontend check` in Task 6 may surface prettier reflow — the step says run `just frontend fix` first if needed; same for `frontend-admin`.
