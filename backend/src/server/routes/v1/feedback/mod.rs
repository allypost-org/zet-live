use std::{
    collections::HashMap,
    net::IpAddr,
    sync::LazyLock,
    time::{Duration, Instant},
};

use axum::{Json, http::HeaderMap, response::IntoResponse};
use axum_client_ip::ClientIp;
use reqwest::StatusCode;
use serde::Deserialize;
use tokio::sync::Mutex;
use tracing::{debug, error, warn};

use crate::{
    auth::resolve_current_user, config::project::ProjectConfig, database::Database,
    server::error::ApiError,
};

const MAX_MESSAGE_LEN: usize = 5_000;
const MAX_NAME_LEN: usize = 200;
const MAX_CONTACT_LEN: usize = 200;
const MAX_META_FIELD_LEN: usize = 512;
const MAX_HONEYPOT_LEN: usize = 200;

const RATE_LIMIT_WINDOW: Duration = Duration::from_hours(1);
const RATE_LIMIT_MAX: usize = 10;

static RATE_LIMITER: LazyLock<Mutex<HashMap<IpAddr, Vec<Instant>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackPayload {
    pub category: FeedbackCategory,
    pub message: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub contact: Option<String>,
    #[serde(default)]
    pub meta: Option<FeedbackMeta>,
    #[serde(default)]
    pub website: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FeedbackCategory {
    Bug,
    Feature,
    Other,
}

impl FeedbackCategory {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Bug => "bug",
            Self::Feature => "feature",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(Default)]
pub struct FeedbackMeta {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub ua: Option<String>,
    #[serde(default)]
    pub lang: Option<String>,
    #[serde(default)]
    pub build: Option<String>,
}

pub async fn submit(
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Json(payload): Json<FeedbackPayload>,
) -> impl IntoResponse {
    if let Some(honeypot) = payload.website.as_deref() {
        let honeypot = honeypot.trim();
        if !honeypot.is_empty() && honeypot.chars().count() <= MAX_HONEYPOT_LEN {
            warn!(%ip, honeypot, "Feedback honeypot triggered, dropping submission");
            return Json(serde_json::json!({ "ok": true })).into_response();
        }
    }

    if let Err(api_error) = check_rate_limit(ip).await {
        return api_error.into_response();
    }

    let message = payload.message.trim().to_string();
    if message.is_empty() || message.chars().count() > MAX_MESSAGE_LEN {
        return ApiError::with_status(StatusCode::BAD_REQUEST, "Message must be 1-5000 characters")
            .into_response();
    }

    let name = payload
        .name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if name
        .as_ref()
        .is_some_and(|s| s.chars().count() > MAX_NAME_LEN)
    {
        return ApiError::with_status(StatusCode::BAD_REQUEST, "Name too long").into_response();
    }

    let contact = payload
        .contact
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if contact
        .as_ref()
        .is_some_and(|s| s.chars().count() > MAX_CONTACT_LEN)
    {
        return ApiError::with_status(StatusCode::BAD_REQUEST, "Contact too long").into_response();
    }

    let meta = payload.meta.unwrap_or_default();
    let meta_url = trim_optional(meta.url, MAX_META_FIELD_LEN);
    let meta_ua = trim_optional(meta.ua, MAX_META_FIELD_LEN);
    let meta_lang = trim_optional(meta.lang, MAX_META_FIELD_LEN);
    let meta_build = trim_optional(meta.build, MAX_META_FIELD_LEN);

    let category = payload.category.as_str();
    let ip_str = ip.to_string();
    let now = jiff::Timestamp::now().to_string();
    let build_default = ProjectConfig::app_and_build_date();
    let meta_build = meta_build.unwrap_or_else(|| build_default.to_string());

    // Optional: attribute the feedback to the authenticated user. Resolved
    // server-side from the session (never trusted from the client body).
    let user_id = resolve_current_user(&headers).await.map(|r| r.user.id);
    if let Some(ref uid) = user_id {
        debug!(%ip, %uid, category, "Authenticated feedback");
    }

    let result = sqlx::query!(
        "
        INSERT INTO feedback
            ( category
            , message
            , name
            , contact
            , meta_url
            , meta_ua
            , meta_lang
            , meta_build
            , ip
            , created_at
            , user_id
            )
        VALUES
            ( ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ? )
        ",
        category,
        message,
        name,
        contact,
        meta_url,
        meta_ua,
        meta_lang,
        meta_build,
        ip_str,
        now,
        user_id,
    )
    .execute(&Database::pool())
    .await;

    match result {
        Ok(_) => {
            debug!(%ip, category, "Feedback stored");
            (StatusCode::CREATED, Json(serde_json::json!({ "ok": true }))).into_response()
        }
        Err(e) => {
            error!(%e, %ip, "Failed to store feedback");
            ApiError::internal("Failed to store feedback").into_response()
        }
    }
}

/// `GET /feedback/mine` -> the authenticated user's submitted feedback, with
/// status and any admin reply (read-only).
pub async fn mine(crate::auth::CurrentUser(user): crate::auth::CurrentUser) -> impl IntoResponse {
    let rows = match sqlx::query!(
        "
        SELECT id            AS \"id!: i64\",
               category      AS \"category!: String\",
               message       AS \"message!: String\",
               created_at    AS \"created_at!: String\",
               handled       AS \"handled!: i64\",
               dismissed     AS \"dismissed!: i64\",
               reply
        FROM feedback
        WHERE user_id = ?
        ORDER BY created_at DESC
        ",
        user.id,
    )
    .fetch_all(&crate::database::Database::pool())
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            error!(error = %e, "Failed to fetch user feedback");
            return ApiError::internal("Failed to fetch feedback").into_response();
        }
    };

    let out: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            let status = if r.reply.is_some() {
                "replied"
            } else if r.dismissed != 0 {
                "dismissed"
            } else if r.handled != 0 {
                "acknowledged"
            } else {
                "open"
            };
            serde_json::json!({
                "id": r.id,
                "category": r.category,
                "message": r.message,
                "createdAt": r.created_at,
                "status": status,
                "reply": r.reply,
            })
        })
        .collect();

    Json(serde_json::json!({ "items": out })).into_response()
}

async fn check_rate_limit(ip: IpAddr) -> Result<(), ApiError> {
    let now = Instant::now();
    let allowed = try_acquire_rate_slot(ip, now).await;

    if allowed {
        Ok(())
    } else {
        Err(ApiError::with_status(
            StatusCode::TOO_MANY_REQUESTS,
            "Rate limit exceeded, try again later",
        ))
    }
}

#[allow(clippy::significant_drop_tightening)]
async fn try_acquire_rate_slot(ip: IpAddr, now: Instant) -> bool {
    let mut map = RATE_LIMITER.lock().await;
    let bucket = map.entry(ip).or_default();
    bucket.retain(|t| now.duration_since(*t) < RATE_LIMIT_WINDOW);

    if bucket.len() >= RATE_LIMIT_MAX {
        false
    } else {
        bucket.push(now);
        true
    }
}

fn trim_optional(value: Option<String>, max_len: usize) -> Option<String> {
    value
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(|s| {
            if s.chars().count() <= max_len {
                s
            } else {
                let truncated: String = s.chars().take(max_len).collect();
                truncated
            }
        })
}
