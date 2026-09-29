# Feature flags

**Date:** 2026-09-29
**Status:** Approved (design) — pending implementation plan
**Scope:** all three components: `backend/` (migration, domain module, admin API, capabilities + WS), `frontend/` (flag store + hook + WS handler), `frontend-admin/` (flags page + drawer). No new env vars, no deploy changes.

---

## 1. Goal

Code-first feature flags with four states, managed at runtime through the admin UI, with a per-request evaluation cost low enough to call 10+ times per request, and live propagation of changes to open frontend sessions.

Secondary goals: admins can manage scoped users comfortably (including bulk add/remove in one request); flag names a user shouldn't see are not exposed to them.

## 2. Flag semantics

| State (DB value) | Anonymous user | Logged-in user |
|---|---|---|
| `disabled` | off | off |
| `enabled` | on | on |
| `logged_in` | off | on |
| `scoped` | off | on iff user is in the flag's scoped-user set |

- Keys are arbitrary non-empty strings. In practice they'll likely be camelCase, but the code makes **zero assumptions about format** beyond validity as a string — no casing rules, no parsing, no normalization.
- **Fail closed everywhere**: unknown key → off (backend evaluation, capabilities payload, and frontend hook alike). A typo cannot enable anything, and absent-from-payload is indistinguishable from off.
- JSON uses camelCase for state names (`loggedIn`); the DB stores snake_case.

## 3. Approach (decisions)

| Decision | Chosen option | Rejected alternatives |
|---|---|---|
| Flag definitions | **Code-first: `FeatureFlag` newtype + macro-generated registry**; DB stores per-flag mutable state | Full DB ownership (flags created via admin UI); code+DB hybrid descriptions; bare `&[&str]` registry (stringly-typed call sites) |
| Description | **DB-only column, admin-editable**; code documents via comments next to the registry entry | In-code description field (duplication, near-zero gain); seeded-from-code-then-editable |
| Primary key | `BIGINT GENERATED ALWAYS AS IDENTITY` (house style, SQL standard) | `SERIAL` (legacy macro: permits manual id inserts, looser sequence ownership) |
| Read path | **In-memory `ArcSwap` snapshot**, reloaded after every admin mutation (mirrors `auth_providers` → `PROVIDERS`) | Per-check DB query; Postgres `LISTEN/NOTIFY` (unnecessary single-process) |
| Seeding | **`INSERT … ON CONFLICT (key) DO NOTHING` at startup, keys only** | Insert-and-swallow-errors; upsert (would clobber admin-edited state/description) |
| Flag removed from code | **Row survives, shown as orphaned, deletable** | Hard-sync delete (a revert would destroy scoped-user lists) |
| Admin mutations | **Single full-state `PATCH`** (state + description + complete `userIds` list); backend reconciles the user set as a diff | Per-field / per-user endpoints (20 users ⇒ 20 requests) |
| Frontend visibility | **Only flags enabled for the caller are exposed** (capabilities + WS frames) | Full map with true/false (leaks flag names) |
| Live updates | **WS broadcast variant, evaluated per connection** | Polling; unauthenticated per-user push |

## 4. Data model

New reversible migration (created via `just migrations-add feature_flags`):

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

- Both FKs cascade: deleting a user (existing admin flow) or a flag cleans up memberships automatically.
- `updated_at` bumps only on admin `PATCH`; `added_at` is preserved for users retained across a `PATCH` (set-diff reconcile, §7).
- Down migration: drop both tables.

## 5. Code registry & seeding

New module `backend/src/feature_flags.rs`. Flags are defined through a newtype so every consumer signals "this is a feature flag", not a random string:

```rust
/// A feature flag key. Constructed only by the `feature_flags!` registry
/// (and internally when loading DB rows) — the inner string is private.
pub struct FeatureFlag(&'static str);
```

Because flags are **referenced far more often than they are defined**, the registry is a declarative macro that generates ergonomic associated constructors *and* the registry array from one list — the two can never drift, and call sites get autocomplete + compile-time checking:

```rust
macro_rules! feature_flags {
    ($($name:ident => $key:literal),* $(,)?) => {
        impl FeatureFlag {
            $(
                #[doc = concat!("The `", stringify!($name), "` feature flag.")]
                pub const fn $name() -> Self { Self($key) }
            )*
        }
        /// Every flag the binary knows about.
        pub const REGISTRY: &[&str] = &[$($key),*];
    };
}

feature_flags! {
    // Document the gated behavior in a comment next to the entry.
    // stop_departures => "stop_departures",
}
```

- Ships empty; entries are added when a feature needs gating. Call sites read `FeatureFlag::stop_departures()` — a typo is a compile error.
- The inner `&'static str` is private; ad-hoc `FeatureFlag("…")` construction is impossible outside the module (internal DB-row loading constructs instances via a private path).
- The duplicate-key unit test (§12) stays: the macro's identifiers make most duplicates a compile error, but two different identifiers can still map to the same string literal.

`feature_flags::init()` runs in `server::run` immediately after `Database::init` (i.e. after migrations, before routes/admin listener): one multi-row `INSERT INTO feature_flags (key) VALUES … ON CONFLICT (key) DO NOTHING`, then the initial snapshot load. Existing rows are never touched, so admin state and edited descriptions survive restarts and redeploys; new flags start `disabled` with an empty description.

## 6. Runtime snapshot & evaluation

Same shape as `PROVIDERS` (`src/auth/config.rs`):

```rust
pub enum FlagState { Disabled, Enabled, LoggedIn, Scoped }

struct FlagEntry {
    state: FlagState,
    scoped_users: Arc<HashSet<String>>, // user ids
}

static FLAGS: LazyLock<ArcSwap<HashMap<String, FlagEntry>>>;
```

- `reload()` — two queries (all flags; all scoped-user rows), rebuild the map, `ArcSwap::store`. Called by `init()` and after every successful admin mutation. Single-process deployment ⇒ the write and the reload happen in the same process; no cross-instance notification is needed.
- `pub fn is_enabled(flag: FeatureFlag, user: Option<&str>) -> bool` — the hot path: `ArcSwap::load` + hash lookup + enum match, a few nanoseconds; state per §2, unknown key → `false`. Taking `FeatureFlag` (not `&str`) is the point of the newtype — call sites can't pass an arbitrary string.
- `pub fn enabled_map(user: Option<&str>) -> serde_json::Map<String, Value>` — full evaluated view for a caller, **containing only the enabled (`true`) flags**. Used by capabilities and the WS push. May be empty.

Handlers obtain the caller the usual way (`CurrentUser` extractor, or `resolve_current_user(&headers)` when auth is optional) and pass `user.map(|u| u.user.id.as_str())`.

## 7. Admin API

On the admin listener under `/api` (existing `ADMIN_KEY` auth middleware). Domain logic and SQL in a new `backend/src/admin/feature_flags.rs`; routes registered in `src/admin/router.rs`; camelCase JSON throughout.

### `GET /api/feature-flags`

All flags ordered by key:

```json
[{
  "id": 3,
  "key": "trip_playback",
  "description": "Animated trip history replay",
  "state": "scoped",
  "updatedAt": "2026-09-29T12:00:00Z",
  "orphaned": false,
  "scopedUsers": [
    { "id": "01J8…", "displayName": "Ivana Horvat", "email": "…", "avatarUrl": "…" }
  ]
}]
```

`orphaned: true` when the key is absent from `REGISTRY` (survives as deletable metadata; its stored description keeps it identifiable).

### `PATCH /api/feature-flags/{id}`

Full-state update — the client always sends the complete desired state, all fields required:

```json
{ "state": "scoped", "description": "Animated trip history replay", "userIds": ["01J8…", "01J9…"] }
```

One transaction:

1. `UPDATE feature_flags SET state, description, updated_at = now() WHERE id = $1` — 404 if no row.
2. Reconcile scoped users as a set difference: bulk-insert missing ids (`ON CONFLICT DO NOTHING`; FK violation ⇒ 400 naming the unknown user), delete removed ids. Retained users keep `added_at`.
3. Respond with the updated flag representation (same shape as the GET entry).

Then (outside the transaction): `feature_flags::reload()` and the WS push (§8). If the reload fails after a successful write, log `error!` and keep serving the previous snapshot — the next successful mutation or restart converges.

Validation failures ⇒ 400 with a message; DB errors ⇒ 500 + `warn!` per existing handler conventions.

### `DELETE /api/feature-flags/{id}`

Deletes the row (cascades memberships). 404 if missing. Followed by reload + WS push. The admin UI only offers this for orphaned flags — a live flag would be re-seeded on the next restart anyway.

## 8. Public exposure

### `GET /api/v1/capabilities`

Gains a `featureFlags` object containing **only the flags enabled for the caller** (absence = off). The endpoint becomes auth-aware via `resolve_current_user(&headers)` — optional, no 401; anonymous callers see only `enabled` flags. This keeps sensitive/unshipped flag names invisible.

### WebSocket live push

- New `Transmission::FeatureFlagsChanged` variant on the existing watch channel (`src/server/routes/v1/mod.rs`), plus a small push helper called by the admin handlers after reload.
- New `Broadcast::FeatureFlags` CBOR frame whose payload is the **full replacement map** (only-true entries, possibly empty). The `select!` loop in `ws/mod.rs` handles the variant by evaluating `feature_flags::enabled_map(<that connection's authenticated user id>)` per connection — each client receives only its own truth, so scoped memberships cannot leak across users. Anonymous connections receive the enabled-only map.
- Whole-map replacement (not a delta) is what makes a flag turning *off* propagate correctly.
- Initial state still comes from capabilities at page load; the WS frame covers subsequent changes.

## 9. Public frontend (`frontend/`)

- New Zustand store `src/feature-flags-store.ts`: `Record<string, boolean>`, hydrated from the capabilities response at boot; replaced wholesale on each `FeatureFlags` WS frame.
- `useFeatureFlag(key: string): boolean` — `false` when absent (fail closed).
- Wire the new `Broadcast` variant into the existing WS message handler (same place `Notices`/`GbfsStations` etc. are dispatched).

## 10. Admin UI (`frontend-admin/`)

- New route `src/routes/feature-flags.tsx` (registered in `router.tsx`) + "Feature flags" nav entry (after Auth in `src/components/layout.tsx`).
- **List**: table — key (mono), description (dim), state badge color-coded per state (pattern of `StatusBadge`), scoped-user count, `updatedAt`. Client-side search like the Users tab.
- **Drawer** (`src/components/flag-drawer.tsx`): slide-over from the right, backdrop, ESC/✕ close. Selected flag tracked as a validated TanStack Router search param `?flag=<key>` — deep-linkable and shareable between admins; closing clears the param. Contents:
  - 4-way segmented state control (Disabled / Enabled / Logged in / Scoped),
  - description textarea,
  - scoped users: search box filtering the already-cached `GET /api/users` (excludes already-selected), click to add; selected users rendered as avatar chips with ✕ to remove; visible in every state with a hint that it applies only while Scoped,
  - local edit state + a single **Save** button (disabled until dirty) issuing one full-state `PATCH` (§7). Toasts via sonner; errors roll nothing back (last-save-wins UI is acceptable).
- Orphaned rows: badge in the list + delete via `confirmAction`.
- Zod schemas in `src/entity/schemas.ts` kept in sync with the Rust structs; queries/mutations + invalidation in `src/lib/queries.ts` (`qk.featureFlags…`).

## 11. Error handling summary

- Fail closed on unknown keys everywhere (backend, capabilities, frontend hook).
- `PATCH`/`DELETE` on unknown id ⇒ 404; invalid state value or unknown user id ⇒ 400; DB failures ⇒ 500 + `warn!` (existing conventions).
- Post-write reload failure: logged, previous snapshot keeps serving (§7).
- WS push best-effort: failures logged, never fail the admin mutation.

## 12. Testing & verification

No test infrastructure exists yet in either component; keep this lean:

- **Rust unit tests** (`just test`, DB-free, pure functions): evaluation matrix — each state × {anonymous, logged-in non-member, logged-in member}, unknown key ⇒ false; `REGISTRY` contains no duplicate keys (guards the macro's same-literal-different-identifier case, which identifiers alone don't prevent).
- **Existing gates**: `just fmt-dev` (pedantic/nursery clippy), `just sqlx-regenerate` with a live PG **and commit the regenerated `backend/.sqlx/`** (offline builds fail otherwise), `just frontend-admin check`.
- **Manual pass**: toggle each state in the drawer; verify `capabilities` differs between logged-out and logged-in (and scoped member vs non-member); verify an open tab updates on flag change via WS (including a flag turning off); deep-link `/feature-flags?flag=<key>`; bulk add/remove scoped users in one save.

## 13. Non-goals / future work

- Percentage rollouts, cohorts, kill switches — the four states cover current needs; a new state is a small migration.
- Audit history of flag changes (only `updated_at`).
- Multi-instance cache invalidation (Postgres `LISTEN/NOTIFY` or TTL refresh) — irrelevant while deployment is a single container.
- Admin-notification fan-out on flag changes (single-admin reality).

## 14. Touch points (file map)

| File | Change |
|---|---|
| `backend/migrations/<ts>_feature_flags.up.sql` / `.down.sql` | new — schema in §4 |
| `backend/src/feature_flags.rs` | new — registry, `init`, snapshot, `reload`, `is_enabled`, `enabled_map` |
| `backend/src/server/mod.rs` | call `feature_flags::init()` after `Database::init` |
| `backend/src/admin/feature_flags.rs` | new — list/update/delete SQL + response structs |
| `backend/src/admin/router.rs` | register the three routes |
| `backend/src/server/routes/v1/capabilities.rs` | auth-aware `featureFlags` |
| `backend/src/server/routes/v1/mod.rs` | `Transmission::FeatureFlagsChanged` + push helper |
| `backend/src/server/routes/v1/ws/mod.rs` | per-connection handling of the variant |
| `backend/.sqlx/` | regenerated offline cache (committed) |
| `frontend/src/feature-flags-store.ts` | new — store + `useFeatureFlag` |
| `frontend/src/app/entity/v1/api.ts` (+ WS dispatch site) | hydrate store from capabilities; handle `FeatureFlags` frame |
| `frontend-admin/src/routes/feature-flags.tsx` | new — list page |
| `frontend-admin/src/components/flag-drawer.tsx` | new — drawer |
| `frontend-admin/src/router.tsx`, `src/components/layout.tsx` | route + `?flag` search-param validation, nav entry |
| `frontend-admin/src/entity/schemas.ts`, `src/lib/queries.ts` | zod schemas, query/mutation hooks |
