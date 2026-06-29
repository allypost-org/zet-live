use std::collections::HashMap;

use axum::{
    Router,
    extract::Path,
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{delete, get, post, put},
};
use axum_extra::extract::Query;
use serde::Deserialize;
use tracing::{debug, warn};

use crate::{
    admin::{self, feedback::FeedbackFilter},
    server::routes::v1::{
        admin_notifications::{ToastPayload, send_notification},
        ws::WS_CONNECTIONS,
    },
};

#[derive(Clone)]
pub struct AdminState {
    pub admin_key: String,
}

pub fn create_admin_router(state: AdminState) -> Router {
    let api = Router::new()
        .route("/connections", get(get_connections))
        .route("/settings", get(get_settings))
        .route("/settings/{name}", get(get_setting).put(put_setting))
        .route("/sync/realtime", post(force_sync_realtime))
        .route("/sync/static", post(force_sync_static))
        .route("/sync/gbfs", post(force_sync_gbfs))
        .route("/metadata", get(get_metadata))
        .route("/notify", post(send_notify))
        .route("/feedback", get(list_feedback).delete(delete_all_feedback))
        .route("/feedback/{id}", delete(delete_feedback))
        .route("/feedback/{id}/handled", put(mark_feedback_handled))
        .route("/feedback/{id}/reply", post(reply_feedback))
        .route("/feedback/{id}/dismiss", post(dismiss_feedback))
        .route(
            "/auth-providers",
            get(get_auth_providers).post(create_auth_provider),
        )
        .route(
            "/auth-providers/{id}",
            put(update_auth_provider).delete(delete_auth_provider),
        )
        .route("/users", get(list_users))
        .route("/users/{id}", delete(delete_user_account))
        .route("/users/{id}/revoke-sessions", post(revoke_user_sessions))
        .route("/sessions", get(list_sessions))
        .route("/sessions/{id}", delete(delete_session))
        .route(
            "/user-notices",
            get(list_user_notices).post(create_user_notice),
        )
        .route("/user-notices/{id}", delete(delete_user_notice))
        .layer(axum::middleware::from_fn_with_state(
            state.admin_key.clone(),
            auth_middleware,
        ))
        .with_state(state);

    Router::new()
        .nest("/api", api)
        .route("/", get(admin_html))
        .route("/index.html", get(admin_html))
}

async fn admin_html() -> impl IntoResponse {
    Html(include_str!("html/index.html"))
}

async fn auth_middleware(
    axum::extract::State(admin_key): axum::extract::State<String>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let auth = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok());

    match auth {
        Some(header) if header == format!("Bearer {admin_key}") => next.run(request).await,
        _ => {
            warn!("Unauthorized admin API request");
            StatusCode::UNAUTHORIZED.into_response()
        }
    }
}

async fn get_connections(headers: HeaderMap) -> impl IntoResponse {
    let connections = WS_CONNECTIONS.read().await.clone();
    crate::server::request::JsonOrAccept(connections, headers).into_response()
}

async fn get_settings() -> impl IntoResponse {
    let settings = admin::ADMIN_SETTINGS.read().await.clone();
    axum::Json(settings).into_response()
}

async fn get_setting(Path(name): Path<String>) -> impl IntoResponse {
    let value = serde_json::to_value(admin::ADMIN_SETTINGS.read().await.clone())
        .expect("Failed to serialize admin settings")
        .as_object()
        .expect("Admin settings must be an object")
        .get(name.as_str())
        .cloned();

    value.map_or_else(
        || StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        |v| axum::Json(serde_json::json!({ "name": name, "value": v })).into_response(),
    )
}

#[derive(Deserialize)]
struct UpdateSettingRequest {
    value: serde_json::Value,
}

async fn put_setting(
    Path(name): Path<String>,
    axum::Json(body): axum::Json<UpdateSettingRequest>,
) -> impl IntoResponse {
    match admin::update_setting(&name, body.value).await {
        Ok(settings) => axum::Json(settings).into_response(),
        Err(e) => {
            warn!(error = %e, "Failed to update setting");
            StatusCode::BAD_REQUEST.into_response()
        }
    }
}

async fn force_sync_realtime() -> impl IntoResponse {
    debug!("Force realtime sync triggered via admin API");
    crate::proto::gtfs_realtime::fetcher::force_sync();
    StatusCode::ACCEPTED.into_response()
}

async fn force_sync_static() -> impl IntoResponse {
    debug!("Force static sync triggered via admin API");
    crate::proto::gtfs_schedule::fetcher::force_sync();
    StatusCode::ACCEPTED.into_response()
}

async fn force_sync_gbfs() -> impl IntoResponse {
    debug!("Force GBFS sync triggered via admin API");
    crate::proto::gbfs::fetcher::force_sync();
    StatusCode::ACCEPTED.into_response()
}

async fn get_metadata() -> impl IntoResponse {
    let entries = admin::metadata::read_all_metadata().await;
    let map = entries.into_iter().collect::<HashMap<_, _>>();
    axum::Json(map).into_response()
}

async fn send_notify(axum::Json(payload): axum::Json<ToastPayload>) -> impl IntoResponse {
    debug!(?payload, "Sending admin notification");
    send_notification(payload).await;
    StatusCode::ACCEPTED.into_response()
}

async fn list_feedback(Query(filter): Query<FeedbackFilter>) -> impl IntoResponse {
    match admin::feedback::list(&filter).await {
        Ok(rows) => axum::Json(rows).into_response(),
        Err(e) => {
            warn!(error = %e, "Failed to list feedback");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn delete_feedback(Path(id): Path<i64>) -> impl IntoResponse {
    match admin::feedback::delete(id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            warn!(error = %e, id, "Failed to delete feedback");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn delete_all_feedback() -> impl IntoResponse {
    match sqlx::query!("DELETE FROM feedback")
        .execute(&crate::database::Database::pool())
        .await
    {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            warn!(error = %e, "Failed to delete all feedback");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Deserialize)]
struct MarkHandledRequest {
    handled: bool,
}

async fn mark_feedback_handled(
    Path(id): Path<i64>,
    axum::Json(body): axum::Json<MarkHandledRequest>,
) -> impl IntoResponse {
    match admin::feedback::set_handled(id, body.handled).await {
        Ok(Some(row)) => (StatusCode::OK, axum::Json(row)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            warn!(error = %e, id, "Failed to update feedback handled flag");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Deserialize)]
struct ReplyRequest {
    reply: String,
}

/// `POST /api/feedback/{id}/reply` -> admin replies (marks acknowledged).
async fn reply_feedback(
    Path(id): Path<i64>,
    axum::Json(body): axum::Json<ReplyRequest>,
) -> impl IntoResponse {
    let reply = body.reply.trim();
    if reply.is_empty() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    match admin::feedback::reply(id, reply).await {
        Ok(Some(row)) => (StatusCode::OK, axum::Json(row)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            warn!(error = %e, id, "Failed to reply to feedback");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// `POST /api/feedback/{id}/dismiss` -> admin dismisses (closes without reply).
async fn dismiss_feedback(Path(id): Path<i64>) -> impl IntoResponse {
    match admin::feedback::dismiss(id).await {
        Ok(Some(row)) => (StatusCode::OK, axum::Json(row)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            warn!(error = %e, id, "Failed to dismiss feedback");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

// --- Auth providers ---

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthProviderAdmin {
    id: String,
    name: String,
    client_id: String,
    enabled: bool,
}

/// `GET /api/auth-providers` -> configured providers (secret masked to a flag)
/// and all known presets (for the "add" dropdown).
async fn get_auth_providers() -> impl IntoResponse {
    let presets = crate::auth::config::preset_list();

    let configured: Vec<AuthProviderAdmin> = match sqlx::query!(
        "
        SELECT id            AS \"id!: String\",
               client_id     AS \"client_id!: String\",
               enabled       AS \"enabled!: i64\"
        FROM auth_providers
        ORDER BY id
        "
    )
    .fetch_all(&crate::database::Database::pool())
    .await
    {
        Ok(rows) => rows
            .into_iter()
            .map(|r| AuthProviderAdmin {
                name: presets
                    .iter()
                    .find(|p| p.id == r.id)
                    .map_or_else(|| r.id.clone(), |p| p.name.clone()),
                id: r.id,
                client_id: r.client_id,
                enabled: r.enabled != 0,
            })
            .collect(),
        Err(e) => {
            warn!(error = %e, "Failed to list auth providers");
            vec![]
        }
    };

    axum::Json(serde_json::json!({ "providers": configured, "presets": presets })).into_response()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateAuthProvider {
    id: String,
    client_id: String,
    client_secret: String,
    #[serde(default = "default_true")]
    enabled: bool,
}

const fn default_true() -> bool {
    true
}

/// `POST /api/auth-providers` -> add (or replace) a provider's credentials.
async fn create_auth_provider(axum::Json(body): axum::Json<CreateAuthProvider>) -> Response {
    if !crate::auth::config::preset_exists(&body.id) {
        return (StatusCode::BAD_REQUEST, "Unknown provider preset").into_response();
    }
    if body.enabled && !crate::auth::config::get().has_app_url() {
        return (
            StatusCode::BAD_REQUEST,
            "APP_URL is not configured — set it before enabling an auth provider",
        )
            .into_response();
    }
    let now = jiff::Timestamp::now().to_string();
    let enabled_i = i64::from(body.enabled);
    match sqlx::query!(
        "
        INSERT INTO auth_providers
            ( id
            , client_id
            , client_secret
            , enabled
            , created_at
            , updated_at
            )
        VALUES
            ( ?
            , ?
            , ?
            , ?
            , ?
            , ?
            )
        ON CONFLICT(id) DO UPDATE SET
              client_id     = excluded.client_id
            , client_secret = excluded.client_secret
            , enabled       = excluded.enabled
            , updated_at    = excluded.updated_at
        ",
        body.id,
        body.client_id,
        body.client_secret,
        enabled_i,
        now,
        now,
    )
    .execute(&crate::database::Database::pool())
    .await
    {
        Ok(_) => {
            crate::auth::config::reload().await;
            StatusCode::CREATED.into_response()
        }
        Err(e) => {
            warn!(error = %e, "Failed to create auth provider");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateAuthProvider {
    #[serde(default)]
    client_id: Option<String>,
    #[serde(default)]
    client_secret: Option<String>,
    #[serde(default)]
    enabled: Option<bool>,
}

/// `PUT /api/auth-providers/{id}` -> patch credentials / enabled flag.
async fn update_auth_provider(
    Path(id): Path<String>,
    axum::Json(body): axum::Json<UpdateAuthProvider>,
) -> Response {
    if body.enabled == Some(true) && !crate::auth::config::get().has_app_url() {
        return (
            StatusCode::BAD_REQUEST,
            "APP_URL is not configured — set it before enabling an auth provider",
        )
            .into_response();
    }
    let now = jiff::Timestamp::now().to_string();
    let enabled_i = body.enabled.map(i64::from);
    match sqlx::query!(
        "
        UPDATE auth_providers
        SET client_id     = COALESCE(?, client_id),
            client_secret = COALESCE(?, client_secret),
            enabled       = COALESCE(?, enabled),
            updated_at    = ?
        WHERE id = ?
        ",
        body.client_id,
        body.client_secret,
        enabled_i,
        now,
        id,
    )
    .execute(&crate::database::Database::pool())
    .await
    {
        Ok(res) if res.rows_affected() > 0 => {
            crate::auth::config::reload().await;
            StatusCode::OK.into_response()
        }
        Ok(_) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            warn!(error = %e, "Failed to update auth provider");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// `DELETE /api/auth-providers/{id}` -> remove a provider's config.
async fn delete_auth_provider(Path(id): Path<String>) -> Response {
    match sqlx::query!("DELETE FROM auth_providers WHERE id = ?", id)
        .execute(&crate::database::Database::pool())
        .await
    {
        Ok(res) if res.rows_affected() > 0 => {
            crate::auth::config::reload().await;
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(_) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            warn!(error = %e, "Failed to delete auth provider");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

// --- Users + per-account notices ---

/// `GET /api/users` -> all accounts (id, name, email, linked providers).
async fn list_users() -> impl IntoResponse {
    match crate::auth::accounts::list_users().await {
        Ok(users) => axum::Json(users).into_response(),
        Err(e) => {
            warn!(error = %e, "Failed to list users");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// `DELETE /api/users/{id}` -> permanently remove an account.
async fn delete_user_account(Path(id): Path<String>) -> Response {
    match crate::auth::accounts::delete_user(&id).await {
        Ok(result) if result.deleted => {
            for session_id in &result.session_ids {
                crate::server::routes::v1::notify_session_revoked(&id, session_id);
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(_) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            warn!(error = %e, "Failed to delete user account");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// `POST /api/users/{id}/revoke-sessions` -> revoke all of a user's sessions
/// without deleting the account. Each revoked session's WS connection is
/// notified so the client signs out immediately.
async fn revoke_user_sessions(Path(id): Path<String>) -> Response {
    match crate::auth::session::delete_other_sessions_for_user(&id, "").await {
        Ok(ids) => {
            for session_id in &ids {
                crate::server::routes::v1::notify_session_revoked(&id, session_id);
            }
            axum::Json(serde_json::json!({ "revoked": ids.len() })).into_response()
        }
        Err(e) => {
            warn!(error = %e, "Failed to revoke user sessions");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// `GET /api/sessions` -> all sessions across all users.
async fn list_sessions() -> impl IntoResponse {
    match crate::auth::session::list_all_sessions().await {
        Ok(sessions) => axum::Json(sessions).into_response(),
        Err(e) => {
            warn!(error = %e, "Failed to list sessions");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// `DELETE /api/sessions/{id}` -> force-expire a session and notify the affected
/// WS connection.
async fn delete_session(Path(id): Path<String>) -> Response {
    match crate::auth::session::delete_session_by_id(&id).await {
        Ok(Some(user_id)) => {
            crate::server::routes::v1::notify_session_revoked(&user_id, &id);
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            warn!(error = %e, "Failed to delete session");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// `GET /api/user-notices` -> all per-account notices (with target account).
async fn list_user_notices() -> impl IntoResponse {
    axum::Json(crate::admin::user_notices::list_all().await).into_response()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateUserNoticeRequest {
    user_id: String,
    text: String,
    severity: crate::admin::settings::NoticeSeverity,
}

/// `POST /api/user-notices` -> create a per-account notice and push it.
async fn create_user_notice(axum::Json(body): axum::Json<CreateUserNoticeRequest>) -> Response {
    let text = body.text.trim();
    if text.is_empty() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    match crate::admin::user_notices::create(&body.user_id, text, body.severity).await {
        Ok(_) => {
            // Push the updated notice set to that account's connections.
            let notices = crate::admin::user_notices::for_user(&body.user_id).await;
            crate::server::routes::v1::send_user_notice(&body.user_id, &notices);
            StatusCode::CREATED.into_response()
        }
        Err(e) => {
            warn!(error = %e, "Failed to create user notice");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// `DELETE /api/user-notices/{id}` -> delete a per-account notice and push update.
async fn delete_user_notice(Path(id): Path<String>) -> Response {
    match crate::admin::user_notices::delete(&id).await {
        Ok(Some(user_id)) => {
            let notices = crate::admin::user_notices::for_user(&user_id).await;
            crate::server::routes::v1::send_user_notice(&user_id, &notices);
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            warn!(error = %e, "Failed to delete user notice");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
