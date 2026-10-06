//! Feature flags: code-first definitions (newtype + macro registry), pure
//! evaluation, and — added later — DB persistence + an in-memory snapshot for
//! hot-path checks. Spec: `docs/superpowers/specs/2026-09-29-feature-flags-design.md`.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, LazyLock},
};

use arc_swap::ArcSwap;
use serde::{Deserialize, Serialize};
use tracing::{error, warn};

use crate::database::Database;

/// A feature-flag key. Constructed only by the [`feature_flags!`] registry
/// below (and by tests inside this module) — the inner string is private, so
/// call sites cannot pass arbitrary strings where a flag is expected.
///
/// `Copy` so hot-path checks can pass it by value (`clippy` dislikes
/// non-consumed by-value args).
#[derive(Clone, Copy)]
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
    // Gates the stop departure board and zoom-gated all-stops map in the
    // frontend, plus the user-aware stop-departures endpoint.
    stop_departures => "stop_departures",
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
    ///
    /// Not `const fn`: matching on `&str` isn't allowed in const functions on
    /// stable Rust, and this only runs when reading DB rows.
    #[must_use]
    pub fn parse_db(s: &str) -> Option<Self> {
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
    pub fn evaluate(&self, user: Option<&str>) -> bool {
        match self.state {
            FlagState::Disabled => false,
            FlagState::Enabled => true,
            FlagState::LoggedIn => user.is_some(),
            FlagState::Scoped => user.is_some_and(|u| self.scoped_users.contains(u)),
        }
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
            None => {
                warn!(
                    state = %row.state,
                    key = %row.key,
                    "Unknown feature-flag state, skipping"
                );
            }
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
        assert!(!is_enabled(
            FeatureFlag::new("nonexistent"),
            Some("01J8TEST")
        ));
    }

    #[test]
    fn enabled_map_is_empty_for_default_snapshot() {
        // FLAGS is the default empty snapshot in tests (no DB involved), so
        // nothing is enabled for anyone — no entries in the caller's view.
        assert!(enabled_map(None).is_empty());
        assert!(enabled_map(Some("u1")).is_empty());
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

    #[test]
    fn flag_state_serializes_camel_case() {
        assert_eq!(
            serde_json::to_string(&FlagState::LoggedIn).expect("serialize"),
            "\"loggedIn\""
        );
        assert_eq!(
            serde_json::to_string(&FlagState::Disabled).expect("serialize"),
            "\"disabled\""
        );
    }
}
