//! Per-account (targeted) notices — persistent notice bars shown only to a
//! specific account. Managed via the admin page; delivered over the
//! user-authenticated WebSocket (initial state on connect + push on change).

use serde::Serialize;

use crate::{
    admin::settings::{GlobalNotice, NoticeSeverity},
    database::Database,
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserNoticeRow {
    pub id: String,
    pub user_id: String,
    pub user_email: Option<String>,
    pub user_display_name: Option<String>,
    pub text: String,
    pub severity: NoticeSeverity,
    pub created_at: jiff::Timestamp,
}

fn now_db() -> time::OffsetDateTime {
    crate::database::time::now()
}

fn new_id() -> String {
    ulid::Ulid::generate().to_string()
}

const fn severity_str(s: NoticeSeverity) -> &'static str {
    match s {
        NoticeSeverity::Info => "info",
        NoticeSeverity::Warning => "warning",
        NoticeSeverity::Error => "error",
    }
}

fn parse_severity(s: &str) -> NoticeSeverity {
    match s {
        "warning" => NoticeSeverity::Warning,
        "error" => NoticeSeverity::Error,
        _ => NoticeSeverity::Info,
    }
}

/// All notices targeting `user_id` (for WS delivery).
pub async fn for_user(user_id: &str) -> Vec<GlobalNotice> {
    let rows = sqlx::query!(
        "
        SELECT id            AS \"id!: String\",
               text          AS \"text!: String\",
               severity      AS \"severity!: String\"
        FROM user_notices
        WHERE user_id = $1
        ORDER BY created_at
        ",
        user_id,
    )
    .fetch_all(&Database::pool())
    .await
    .unwrap_or_default();

    rows.into_iter()
        .map(|r| GlobalNotice {
            id: r.id,
            text: r.text,
            severity: parse_severity(&r.severity),
        })
        .collect()
}

/// All per-account notices with their target account (for the admin UI).
pub async fn list_all(
    page: &crate::admin::pagination::PageRequest,
) -> Result<crate::admin::pagination::Page<UserNoticeRow>, sqlx::Error> {
    // CASE ordering needs a custom plan to use the selected ordering/filter index.
    let mut tx = Database::pool().begin().await?;
    sqlx::query("SET LOCAL plan_cache_mode = 'force_custom_plan'")
        .execute(&mut *tx)
        .await?;
    let rows = sqlx::query!(
        "
        SELECT n.id              AS \"id!: String\",
               n.user_id         AS \"user_id!: String\",
               u.email,
               u.display_name,
               n.text            AS \"text!: String\",
               n.severity        AS \"severity!: String\",
               n.created_at      AS \"created_at!: time::OffsetDateTime\"
        FROM user_notices n
        LEFT JOIN users u ON u.id = n.user_id
        WHERE ($3 = '%%' OR concat_ws(' ', u.display_name, u.email, n.user_id, n.text, n.severity) \
         ILIKE $3)
        ORDER BY CASE WHEN $1 = 'createdAt' AND NOT $2 THEN n.created_at END ASC,
                     CASE WHEN $1 = 'createdAt' AND $2 THEN n.created_at END DESC,
                     CASE WHEN $1 = 'user' AND NOT $2 THEN COALESCE(u.display_name, u.email, u.id) \
         END ASC NULLS LAST,
                     CASE WHEN $1 = 'user' AND $2 THEN COALESCE(u.display_name, u.email, u.id) END \
         DESC NULLS LAST,
                     CASE WHEN $1 = 'text' AND NOT $2 THEN n.text END ASC NULLS LAST,
                     CASE WHEN $1 = 'text' AND $2 THEN n.text END DESC NULLS LAST,
                     CASE WHEN $1 = 'severity' AND NOT $2 THEN n.severity END ASC NULLS LAST,
                     CASE WHEN $1 = 'severity' AND $2 THEN n.severity END DESC NULLS LAST, n.id \
         DESC
        LIMIT $4 OFFSET $5
        ",
        page.sort.as_str(),
        page.descending,
        page.search,
        page.fetch_limit(),
        page.offset,
    )
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;

    let items = rows
        .into_iter()
        .map(|r| UserNoticeRow {
            id: r.id,
            user_id: r.user_id,
            user_email: r.email,
            user_display_name: r.display_name,
            text: r.text,
            severity: parse_severity(&r.severity),
            created_at: crate::database::time::to_jiff(r.created_at),
        })
        .collect();
    Ok(crate::admin::pagination::Page::new(items, page))
}

/// Create a per-account notice. Returns the created notice.
pub async fn create(
    user_id: &str,
    text: &str,
    severity: NoticeSeverity,
) -> Result<GlobalNotice, sqlx::Error> {
    let id = new_id();
    let now = now_db();
    let sev = severity_str(severity);
    sqlx::query!(
        "
        INSERT INTO user_notices
            ( id, user_id, text, severity, created_at )
        VALUES
            ( $1, $2, $3, $4, $5 )
        ",
        id,
        user_id,
        text,
        sev,
        now,
    )
    .execute(&Database::pool())
    .await?;

    Ok(GlobalNotice {
        id,
        text: text.to_string(),
        severity,
    })
}

/// Delete a per-account notice. Returns the `user_id` it belonged to (so the
/// caller can push the updated set to that account), if a row was removed.
pub async fn delete(id: &str) -> Result<Option<String>, sqlx::Error> {
    let row = sqlx::query!(
        "DELETE FROM user_notices WHERE id = $1 RETURNING user_id AS \"user_id!: String\"",
        id,
    )
    .fetch_optional(&Database::pool())
    .await?;
    Ok(row.map(|r| r.user_id))
}
