use serde::{Deserialize, Serialize};

use crate::database::Database;

/// The end-user-facing status, derived from `reply`/`dismissed`/`handled`.
const fn status_of(reply: Option<&str>, dismissed: bool, handled: bool) -> &'static str {
    if reply.is_some() {
        "replied"
    } else if dismissed {
        "dismissed"
    } else if handled {
        "acknowledged"
    } else {
        "open"
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackRow {
    pub id: i64,
    pub category: String,
    pub message: String,
    pub name: Option<String>,
    pub contact: Option<String>,
    pub meta_url: Option<String>,
    pub meta_ua: Option<String>,
    pub meta_lang: Option<String>,
    pub meta_build: Option<String>,
    pub ip: String,
    pub created_at: String,
    pub handled: bool,
    pub dismissed: bool,
    pub status: String,
    pub reply: Option<String>,
    pub replied_at: Option<String>,
    pub user_id: Option<String>,
    pub user_email: Option<String>,
    pub user_display_name: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackFilter {
    /// `all` (default), `new`, `archived`
    pub handled: Option<String>,
}

macro_rules! map_row {
    ($r:expr) => {
        FeedbackRow {
            id: $r.id,
            category: $r.category,
            message: $r.message,
            name: $r.name,
            contact: $r.contact,
            meta_url: $r.meta_url,
            meta_ua: $r.meta_ua,
            meta_lang: $r.meta_lang,
            meta_build: $r.meta_build,
            ip: $r.ip,
            created_at: $r.created_at,
            handled: $r.handled != 0,
            dismissed: $r.dismissed != 0,
            status: status_of($r.reply.as_deref(), $r.dismissed != 0, $r.handled != 0).to_string(),
            reply: $r.reply,
            replied_at: $r.replied_at,
            user_id: $r.user_id,
            user_email: $r.user_email,
            user_display_name: $r.user_display_name,
        }
    };
}

pub async fn list(filter: &FeedbackFilter) -> Result<Vec<FeedbackRow>, sqlx::Error> {
    let rows: Vec<FeedbackRow> = match filter.handled.as_deref() {
        // "new" = open (not acknowledged, not dismissed, no reply)
        Some("new") => sqlx::query!(
            "
            SELECT
                  f.id            AS \"id!\"
                , f.category      AS \"category!\"
                , f.message       AS \"message!\"
                , f.name
                , f.contact
                , f.meta_url
                , f.meta_ua
                , f.meta_lang
                , f.meta_build
                , f.ip            AS \"ip!\"
                , f.created_at    AS \"created_at!\"
                , f.handled       AS \"handled!\"
                , f.dismissed     AS \"dismissed!\"
                , f.reply
                , f.replied_at
                , f.user_id
                , u.email         AS \"user_email\"
                , u.display_name  AS \"user_display_name\"
            FROM feedback f
            LEFT JOIN users u ON u.id = f.user_id
            WHERE f.handled = 0 AND f.dismissed = 0 AND f.reply IS NULL
            ORDER BY f.created_at DESC
            "
        )
        .fetch_all(&Database::pool())
        .await?
        .into_iter()
        .map(|r| map_row!(r))
        .collect(),
        // "archived" = closed (acknowledged OR dismissed OR replied)
        Some("archived") => sqlx::query!(
            "
            SELECT
                  f.id            AS \"id!\"
                , f.category      AS \"category!\"
                , f.message       AS \"message!\"
                , f.name
                , f.contact
                , f.meta_url
                , f.meta_ua
                , f.meta_lang
                , f.meta_build
                , f.ip            AS \"ip!\"
                , f.created_at    AS \"created_at!\"
                , f.handled       AS \"handled!\"
                , f.dismissed     AS \"dismissed!\"
                , f.reply
                , f.replied_at
                , f.user_id
                , u.email         AS \"user_email\"
                , u.display_name  AS \"user_display_name\"
            FROM feedback f
            LEFT JOIN users u ON u.id = f.user_id
            WHERE f.handled = 1 OR f.dismissed = 1 OR f.reply IS NOT NULL
            ORDER BY f.created_at DESC
            "
        )
        .fetch_all(&Database::pool())
        .await?
        .into_iter()
        .map(|r| map_row!(r))
        .collect(),
        _ => sqlx::query!(
            "
            SELECT
                  f.id            AS \"id!\"
                , f.category      AS \"category!\"
                , f.message       AS \"message!\"
                , f.name
                , f.contact
                , f.meta_url
                , f.meta_ua
                , f.meta_lang
                , f.meta_build
                , f.ip            AS \"ip!\"
                , f.created_at    AS \"created_at!\"
                , f.handled       AS \"handled!\"
                , f.dismissed     AS \"dismissed!\"
                , f.reply
                , f.replied_at
                , f.user_id
                , u.email         AS \"user_email\"
                , u.display_name  AS \"user_display_name\"
            FROM feedback f
            LEFT JOIN users u ON u.id = f.user_id
            ORDER BY f.created_at DESC
            "
        )
        .fetch_all(&Database::pool())
        .await?
        .into_iter()
        .map(|r| map_row!(r))
        .collect(),
    };

    Ok(rows)
}

pub async fn delete(id: i64) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!("DELETE FROM feedback WHERE id = ?", id)
        .execute(&Database::pool())
        .await?;
    Ok(result.rows_affected() > 0)
}

async fn fetch_one(id: i64) -> Result<Option<FeedbackRow>, sqlx::Error> {
    let row = sqlx::query!(
        "
        SELECT
              f.id            AS \"id!\"
            , f.category      AS \"category!\"
            , f.message       AS \"message!\"
            , f.name
            , f.contact
            , f.meta_url
            , f.meta_ua
            , f.meta_lang
            , f.meta_build
            , f.ip            AS \"ip!\"
            , f.created_at    AS \"created_at!\"
            , f.handled       AS \"handled!\"
            , f.dismissed     AS \"dismissed!\"
            , f.reply
            , f.replied_at
            , f.user_id
            , u.email         AS \"user_email\"
            , u.display_name  AS \"user_display_name\"
        FROM feedback f
        LEFT JOIN users u ON u.id = f.user_id
        WHERE f.id = ?
        ",
        id
    )
    .fetch_optional(&Database::pool())
    .await?;
    Ok(row.map(|r| map_row!(r)))
}

pub async fn set_handled(id: i64, handled: bool) -> Result<Option<FeedbackRow>, sqlx::Error> {
    let handled_int: i64 = handled.into();
    let result = sqlx::query!(
        "UPDATE feedback SET handled = ?, dismissed = 0 WHERE id = ?",
        handled_int,
        id
    )
    .execute(&Database::pool())
    .await?;

    if result.rows_affected() == 0 {
        return Ok(None);
    }
    fetch_one(id).await
}

/// Admin reply: sets `reply`/`replied_at` and marks acknowledged.
pub async fn reply(id: i64, reply: &str) -> Result<Option<FeedbackRow>, sqlx::Error> {
    let now = jiff::Timestamp::now().to_string();
    let result = sqlx::query!(
        "UPDATE feedback SET reply = ?, replied_at = ?, handled = 1, dismissed = 0 WHERE id = ?",
        reply,
        now,
        id
    )
    .execute(&Database::pool())
    .await?;

    if result.rows_affected() == 0 {
        return Ok(None);
    }
    fetch_one(id).await
}

/// Admin dismiss: closes the feedback without a reply.
pub async fn dismiss(id: i64) -> Result<Option<FeedbackRow>, sqlx::Error> {
    let result = sqlx::query!(
        "UPDATE feedback SET dismissed = 1, handled = 0 WHERE id = ?",
        id
    )
    .execute(&Database::pool())
    .await?;

    if result.rows_affected() == 0 {
        return Ok(None);
    }
    fetch_one(id).await
}
