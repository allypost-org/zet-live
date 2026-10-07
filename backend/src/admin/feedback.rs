use serde::{Deserialize, Serialize};

use crate::database::Database;

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
    pub created_at: jiff::Timestamp,
    pub handled: bool,
    pub dismissed: bool,
    pub status: String,
    pub reply: Option<String>,
    pub replied_at: Option<jiff::Timestamp>,
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
            created_at: crate::database::time::to_jiff($r.created_at),
            handled: $r.handled,
            dismissed: $r.dismissed,
            status: status_of($r.reply.as_deref(), $r.dismissed, $r.handled).to_string(),
            reply: $r.reply,
            replied_at: $r.replied_at.map(crate::database::time::to_jiff),
            user_id: $r.user_id,
            user_email: $r.user_email,
            user_display_name: $r.user_display_name,
        }
    };
}

pub async fn list(
    filter: &FeedbackFilter,
    page: &crate::admin::pagination::PageRequest,
) -> Result<crate::admin::pagination::Page<FeedbackRow>, sqlx::Error> {
    // CASE ordering needs a custom plan to use the selected ordering/filter index.
    let mut tx = Database::pool().begin().await?;
    sqlx::query("SET LOCAL plan_cache_mode = 'force_custom_plan'")
        .execute(&mut *tx)
        .await?;
    let rows = sqlx::query!(
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
                , f.created_at    AS \"created_at!: time::OffsetDateTime\"
                , f.handled       AS \"handled!\"
                , f.dismissed     AS \"dismissed!\"
                , f.reply
                , f.replied_at   AS \"replied_at: time::OffsetDateTime\"
                , f.user_id
                , u.email         AS \"user_email\"
                , u.display_name  AS \"user_display_name\"
            FROM feedback f
            LEFT JOIN users u ON u.id = f.user_id
            WHERE ($6 = 'all'
                   OR ($6 = 'new' AND NOT f.handled AND NOT f.dismissed AND f.reply IS NULL)
                   OR ($6 = 'archived' AND (f.handled OR f.dismissed OR f.reply IS NOT NULL)))
              AND ($3 = '%%' OR concat_ws(' ', f.id, f.category, f.message, f.name, f.contact, \
         f.ip, f.reply, u.email, u.display_name) ILIKE $3)
            ORDER BY CASE WHEN $1 = 'createdAt' AND NOT $2 THEN f.created_at END ASC,
                     CASE WHEN $1 = 'createdAt' AND $2 THEN f.created_at END DESC, f.id DESC
            LIMIT $4 OFFSET $5
            ",
        page.sort.as_str(),
        page.descending,
        page.search,
        page.fetch_limit(),
        page.offset,
        filter.handled.as_deref().unwrap_or("all"),
    )
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    let items = rows.into_iter().map(|r| map_row!(r)).collect();
    Ok(crate::admin::pagination::Page::new(items, page))
}

pub async fn delete(id: i64) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!("DELETE FROM feedback WHERE id = $1", id)
        .execute(&Database::pool())
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn list_for_user(user_id: &str) -> Result<Vec<FeedbackRow>, sqlx::Error> {
    let rows = sqlx::query!(
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
            , f.created_at    AS \"created_at!: time::OffsetDateTime\"
            , f.handled       AS \"handled!\"
            , f.dismissed     AS \"dismissed!\"
            , f.reply
            , f.replied_at   AS \"replied_at: time::OffsetDateTime\"
            , f.user_id
            , u.email         AS \"user_email\"
            , u.display_name  AS \"user_display_name\"
        FROM feedback f
        LEFT JOIN users u ON u.id = f.user_id
        WHERE f.user_id = $1
        ORDER BY f.created_at DESC
        ",
        user_id,
    )
    .fetch_all(&Database::pool())
    .await?
    .into_iter()
    .map(|r| map_row!(r))
    .collect();

    Ok(rows)
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
            , f.created_at    AS \"created_at!: time::OffsetDateTime\"
            , f.handled       AS \"handled!\"
            , f.dismissed     AS \"dismissed!\"
            , f.reply
            , f.replied_at   AS \"replied_at: time::OffsetDateTime\"
            , f.user_id
            , u.email         AS \"user_email\"
            , u.display_name  AS \"user_display_name\"
        FROM feedback f
        LEFT JOIN users u ON u.id = f.user_id
        WHERE f.id = $1
        ",
        id
    )
    .fetch_optional(&Database::pool())
    .await?;
    Ok(row.map(|r| map_row!(r)))
}

pub async fn set_handled(id: i64, handled: bool) -> Result<Option<FeedbackRow>, sqlx::Error> {
    let result = sqlx::query!(
        "UPDATE feedback SET handled = $1, dismissed = false WHERE id = $2",
        handled,
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
    let now = crate::database::time::now();
    let result = sqlx::query!(
        "UPDATE feedback SET reply = $1, replied_at = $2, handled = true, dismissed = false WHERE \
         id = $3",
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
        "UPDATE feedback SET dismissed = true, handled = false WHERE id = $1",
        id
    )
    .execute(&Database::pool())
    .await?;

    if result.rows_affected() == 0 {
        return Ok(None);
    }
    fetch_one(id).await
}
