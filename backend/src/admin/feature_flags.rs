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
