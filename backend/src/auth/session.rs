use std::time::Duration;

use axum::http::HeaderMap;
use axum_extra::extract::cookie::{Cookie, SameSite};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::Rng;
use sha2::{Digest, Sha256};
use tracing::warn;

use crate::database::Database;

pub const STATE_COOKIE: &str = "zet_oauth_state";

const STATE_MAX_AGE: Duration = Duration::from_mins(10);

const TOKEN_LEN: usize = 64;

fn new_id() -> String {
    ulid::Ulid::new().to_string()
}

fn random_bytes(len: usize) -> Vec<u8> {
    let mut buf = vec![0u8; len];
    rand::rng().fill_bytes(&mut buf);
    buf
}

fn sha256(bytes: &[u8]) -> Vec<u8> {
    Sha256::digest(bytes).to_vec()
}

fn now_iso() -> String {
    jiff::Timestamp::now().to_string()
}

fn expires_iso(max_age: Duration) -> String {
    let now = jiff::Timestamp::now();
    let secs = now
        .as_second()
        .saturating_add(i64::try_from(max_age.as_secs()).unwrap_or(0));
    jiff::Timestamp::from_second(secs)
        .unwrap_or(now)
        .to_string()
}

fn decode_token(token: &str) -> Option<Vec<u8>> {
    let raw = URL_SAFE_NO_PAD.decode(token).ok()?;
    if raw.len() == TOKEN_LEN {
        Some(raw)
    } else {
        None
    }
}

pub struct CreatedSession {
    pub id: String,
    pub token: String,
}

#[derive(Debug)]
pub struct SessionRow {
    pub id: String,
    pub user_id: String,
}

pub async fn create_session(
    user_id: &str,
    max_age: Duration,
    ip: Option<&str>,
    user_agent: Option<&str>,
) -> Result<CreatedSession, sqlx::Error> {
    let id = new_id();
    let raw = random_bytes(TOKEN_LEN);
    let hash = sha256(&raw);
    let token = URL_SAFE_NO_PAD.encode(&raw);
    let created_at = now_iso();
    let expires_at = expires_iso(max_age);

    sqlx::query!(
        "
        INSERT INTO user_sessions
            ( id
            , token_hash
            , user_id
            , expires_at
            , created_at
            , ip
            , user_agent
            )
        VALUES
            ( ?
            , ?
            , ?
            , ?
            , ?
            , ?
            , ?
            )
        ",
        id,
        hash,
        user_id,
        expires_at,
        created_at,
        ip,
        user_agent,
    )
    .execute(&Database::pool())
    .await?;

    Ok(CreatedSession { id, token })
}

pub async fn lookup_session(token: &str) -> Result<Option<SessionRow>, sqlx::Error> {
    let Some(raw) = decode_token(token) else {
        return Ok(None);
    };
    let hash = sha256(&raw);
    let now = now_iso();

    let row = sqlx::query!(
        "
        SELECT id, user_id
        FROM user_sessions
        WHERE token_hash = ? AND expires_at > ?
        ",
        hash,
        now,
    )
    .fetch_optional(&Database::pool())
    .await?;

    Ok(row.map(|r| SessionRow {
        id: r.id,
        user_id: r.user_id,
    }))
}

pub async fn delete_session(token: &str) -> Result<bool, sqlx::Error> {
    let Some(raw) = decode_token(token) else {
        return Ok(false);
    };
    let hash = sha256(&raw);

    let res = sqlx::query!("DELETE FROM user_sessions WHERE token_hash = ?", hash)
        .execute(&Database::pool())
        .await?;

    Ok(res.rows_affected() > 0)
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: String,
    pub user_id: String,
    pub created_at: String,
    pub expires_at: String,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
}

pub async fn list_sessions_for_user(user_id: &str) -> Result<Vec<SessionInfo>, sqlx::Error> {
    let rows = sqlx::query!(
        "
        SELECT
              id
            , user_id
            , created_at
            , expires_at
            , ip
            , user_agent
        FROM user_sessions
        WHERE user_id = ?
        ORDER BY created_at DESC
        ",
        user_id,
    )
    .fetch_all(&Database::pool())
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| SessionInfo {
            id: r.id,
            user_id: r.user_id,
            created_at: r.created_at,
            expires_at: r.expires_at,
            ip: r.ip,
            user_agent: r.user_agent,
        })
        .collect())
}

pub async fn list_all_sessions() -> Result<Vec<SessionInfo>, sqlx::Error> {
    let rows = sqlx::query!(
        "
        SELECT
              id
            , user_id
            , created_at
            , expires_at
            , ip
            , user_agent
        FROM user_sessions
        ORDER BY created_at DESC
        "
    )
    .fetch_all(&Database::pool())
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| SessionInfo {
            id: r.id,
            user_id: r.user_id,
            created_at: r.created_at,
            expires_at: r.expires_at,
            ip: r.ip,
            user_agent: r.user_agent,
        })
        .collect())
}

pub async fn delete_session_by_id(id: &str) -> Result<Option<String>, sqlx::Error> {
    let row = sqlx::query!(
        "DELETE FROM user_sessions WHERE id = ? RETURNING user_id AS \"user_id!: String\"",
        id,
    )
    .fetch_optional(&Database::pool())
    .await?;

    Ok(row.map(|r| r.user_id))
}

pub async fn delete_session_by_id_for_user(id: &str, user_id: &str) -> Result<bool, sqlx::Error> {
    let res = sqlx::query!(
        "DELETE FROM user_sessions WHERE id = ? AND user_id = ?",
        id,
        user_id,
    )
    .execute(&Database::pool())
    .await?;

    Ok(res.rows_affected() > 0)
}

pub async fn delete_other_sessions_for_user(
    user_id: &str,
    keep_session_id: &str,
) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query!(
        r#"DELETE FROM user_sessions WHERE user_id = ? AND id != ? RETURNING id"#,
        user_id,
        keep_session_id,
    )
    .fetch_all(&Database::pool())
    .await?;

    Ok(rows.into_iter().map(|r| r.id).collect())
}

const LINK_TICKET_MAX_AGE: Duration = Duration::from_mins(5);

pub struct CreatedLinkTicket {
    pub ticket: String,
}

pub async fn create_link_ticket(user_id: &str) -> Result<CreatedLinkTicket, sqlx::Error> {
    let raw = random_bytes(TOKEN_LEN);
    let hash = sha256(&raw);
    let ticket = URL_SAFE_NO_PAD.encode(&raw);
    let now = now_iso();
    let expires_at = expires_iso(LINK_TICKET_MAX_AGE);

    sqlx::query!(
        "
        INSERT INTO link_tickets
            ( token_hash
            , user_id
            , expires_at
            , created_at
            )
        VALUES
            ( ?
            , ?
            , ?
            , ?
            )
        ",
        hash,
        user_id,
        expires_at,
        now,
    )
    .execute(&Database::pool())
    .await?;

    Ok(CreatedLinkTicket { ticket })
}

pub async fn consume_link_ticket(ticket: &str) -> Result<Option<String>, sqlx::Error> {
    let Some(raw) = decode_token(ticket) else {
        return Ok(None);
    };
    let hash = sha256(&raw);
    let now = now_iso();

    let row = sqlx::query!(
        "
        DELETE FROM link_tickets
        WHERE token_hash = ? AND expires_at > ?
        RETURNING user_id
        ",
        hash,
        now,
    )
    .fetch_optional(&Database::pool())
    .await?;

    Ok(row.map(|r| r.user_id))
}

#[derive(Debug)]
pub struct PendingTransfer {
    pub target_user_id: String,
    pub provider: String,
    pub provider_subject: String,
    pub source_user_id: String,
}

const TRANSFER_MAX_AGE: Duration = Duration::from_mins(5);

pub async fn create_link_transfer(transfer: &PendingTransfer) -> Result<String, sqlx::Error> {
    let raw = random_bytes(TOKEN_LEN);
    let hash = sha256(&raw);
    let token = URL_SAFE_NO_PAD.encode(&raw);
    let now = now_iso();
    let expires_at = expires_iso(TRANSFER_MAX_AGE);

    sqlx::query!(
        "
        INSERT INTO pending_transfers
            ( token_hash
            , target_user_id
            , provider
            , provider_subject
            , source_user_id
            , expires_at
            , created_at
            )
        VALUES
            ( ?
            , ?
            , ?
            , ?
            , ?
            , ?
            , ?
            )
        ",
        hash,
        transfer.target_user_id,
        transfer.provider,
        transfer.provider_subject,
        transfer.source_user_id,
        expires_at,
        now,
    )
    .execute(&Database::pool())
    .await?;

    Ok(token)
}

/// Consume a pending-transfer token (single-use), returning its details if
/// valid, not expired, and bound to `target_user_id`.
pub async fn consume_link_transfer(
    token: &str,
    target_user_id: &str,
) -> Result<Option<PendingTransfer>, sqlx::Error> {
    let Some(raw) = decode_token(token) else {
        return Ok(None);
    };
    let hash = sha256(&raw);
    let now = now_iso();

    let row = sqlx::query!(
        "
        DELETE FROM pending_transfers
        WHERE token_hash = ? AND expires_at > ? AND target_user_id = ?
        RETURNING provider, provider_subject, source_user_id
        ",
        hash,
        now,
        target_user_id,
    )
    .fetch_optional(&Database::pool())
    .await?;

    Ok(row.map(|r| PendingTransfer {
        target_user_id: target_user_id.to_string(),
        provider: r.provider,
        provider_subject: r.provider_subject,
        source_user_id: r.source_user_id,
    }))
}

pub fn spawn_expiry_reaper() {
    tokio::task::spawn(async {
        loop {
            if let Err(e) = reap_expired().await {
                warn!(error = %e, "Failed to reap expired auth rows");
            }
            tokio::time::sleep(Duration::from_hours(1)).await;
        }
    });
}

async fn reap_expired() -> Result<(), sqlx::Error> {
    let now = now_iso();
    sqlx::query!(
        "DELETE FROM user_sessions WHERE expires_at < ?",
        now.as_str()
    )
    .execute(&Database::pool())
    .await?;
    sqlx::query!(
        "DELETE FROM oauth_states WHERE expires_at < ?",
        now.as_str()
    )
    .execute(&Database::pool())
    .await?;
    sqlx::query!(
        "DELETE FROM link_tickets WHERE expires_at < ?",
        now.as_str()
    )
    .execute(&Database::pool())
    .await?;
    sqlx::query!(
        "DELETE FROM pending_transfers WHERE expires_at < ?",
        now.as_str()
    )
    .execute(&Database::pool())
    .await?;
    Ok(())
}

pub fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?;
    let (scheme, rest) = value.split_once(' ')?;
    if scheme.eq_ignore_ascii_case("bearer") {
        Some(rest.trim())
    } else {
        None
    }
}

pub fn cookie_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let header = headers.get(axum::http::header::COOKIE)?.to_str().ok()?;
    let prefix = format!("{name}=");
    for part in header.split(';') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix(prefix.as_str()) {
            return Some(rest);
        }
    }
    None
}

pub fn state_cookie_header(state: &str, secure: bool) -> String {
    let mut cookie = Cookie::build((STATE_COOKIE.to_string(), state.to_string()))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(cookie::time::Duration::seconds(
            i64::try_from(STATE_MAX_AGE.as_secs()).unwrap_or(0),
        ));

    if secure {
        cookie = cookie.secure(true);
    }

    cookie.build().to_string()
}

pub fn clear_state_cookie_header() -> String {
    Cookie::build((STATE_COOKIE.to_string(), String::new()))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(cookie::time::Duration::ZERO)
        .build()
        .to_string()
}
