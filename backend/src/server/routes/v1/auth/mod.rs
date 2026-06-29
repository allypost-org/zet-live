use axum::{
    Json,
    extract::{Form, Path},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
};
use axum_client_ip::ClientIp;
use axum_extra::extract::Query;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, KeyInit, Mac};
use serde::Deserialize;
use serde_json::json;
use sha2::Sha256;
use tracing::{debug, error, warn};
use url::Url;

use crate::{
    auth::{
        CurrentUser, accounts, config,
        oauth::{self, OAuthState},
        resolve_current_user, session,
    },
    database::Database,
    server::error::ApiError,
};

#[derive(Debug, Deserialize)]
pub struct StartQuery {
    /// `?link=1` (or `true`) -> link this provider to the current user instead
    /// of logging in. Requires `?ticket=…` (from `POST /auth/link-ticket`).
    #[serde(default)]
    pub link: Option<String>,

    /// `?origin=…` -> the opener page's origin, used as the `postMessage`
    /// targetOrigin when delivering the session token back. Lets the callback
    /// (backend origin) reach a cross-origin frontend (e.g. Vite on :5173).
    #[serde(default)]
    pub origin: Option<String>,

    /// `?ticket=…` -> a link ticket (link flows only), identifying the user to
    /// attach the provider to.
    #[serde(default)]
    pub ticket: Option<String>,
}

impl StartQuery {
    fn wants_link(&self) -> bool {
        matches!(self.link.as_deref(), Some("1" | "true"))
    }
}

#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
}

/// `GET /auth/{provider}/start` -> set state cookie, redirect to the provider.
pub async fn start(Path(provider_id): Path<String>, Query(q): Query<StartQuery>) -> Response {
    let providers = config::get();
    let Some((provider, redirect_uri)) = resolve_provider(&providers, &provider_id) else {
        return ApiError::not_found(format!("Unknown provider: {provider_id}")).into_response();
    };

    let link = q.wants_link();
    // The opener's origin becomes the `postMessage` targetOrigin (which carries
    // the session token), so it must be allowlisted server-side — never accept
    // an arbitrary http(s) URL. Absent -> same-origin frontend (use app_url).
    let origin = match q.origin.as_deref() {
        None => None,
        Some(o) => {
            if providers.is_allowed_origin(o) {
                Some(o.to_string())
            } else {
                warn!(provider = %provider_id, origin = o, "Disallowed OAuth origin");
                return ApiError::with_status(StatusCode::BAD_REQUEST, "Disallowed origin")
                    .into_response();
            }
        }
    };
    let target_origin = origin
        .clone()
        .or_else(|| {
            providers
                .app_url
                .as_ref()
                .map(|u| u.origin().ascii_serialization())
        })
        .unwrap_or_default();
    debug!(provider = %provider_id, link, origin = ?origin, "Starting OAuth flow");

    // For link flows, resolve the target user from a link ticket (the popup
    // navigation can't carry the bearer token, so the client fetches a ticket
    // first). Reject before redirecting if the ticket is missing/invalid.
    let link_user_id = if link {
        match q.ticket.as_deref() {
            Some(ticket) => match session::consume_link_ticket(ticket).await {
                Ok(Some(user_id)) => Some(user_id),
                Ok(None) => {
                    warn!(provider = %provider_id, "Invalid or expired link ticket");
                    return render_result(
                        &provider_id,
                        false,
                        "Invalid or expired link ticket",
                        None,
                        &target_origin,
                        &[],
                    );
                }
                Err(e) => {
                    error!(error = %e, "Failed to consume link ticket");
                    return ApiError::internal("Failed to start OAuth flow").into_response();
                }
            },
            None => {
                return render_result(
                    &provider_id,
                    false,
                    "Missing link ticket",
                    None,
                    &target_origin,
                    &[],
                );
            }
        }
    } else {
        None
    };

    let (auth_url, state, pkce_verifier) = oauth::build_auth_url(&provider, &redirect_uri, link);

    if let Err(e) = oauth::create_state(
        &state,
        &OAuthState {
            provider: provider_id.clone(),
            pkce_verifier,
            link,
            origin,
            user_id: link_user_id,
        },
    )
    .await
    {
        error!(error = %e, "Failed to persist OAuth state");
        return ApiError::internal("Failed to start OAuth flow").into_response();
    }

    let secure = providers
        .app_url
        .as_ref()
        .is_some_and(|u| u.scheme() == "https");
    let cookie =
        HeaderValue::from_str(&session::state_cookie_header(&state, secure)).expect("valid cookie");

    (
        [(header::SET_COOKIE, cookie)],
        Redirect::temporary(auth_url.as_str()),
    )
        .into_response()
}

pub async fn callback(
    Path(provider_id): Path<String>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Query(q): Query<CallbackQuery>,
) -> Response {
    let providers = config::get();
    let app_url = providers.app_url.clone();
    let fallback_origin = app_url
        .as_ref()
        .map_or_else(|| String::from("*"), |u| u.origin().ascii_serialization());
    let Some((provider, redirect_uri)) = resolve_provider(&providers, &provider_id) else {
        return render_result(
            &provider_id,
            false,
            "Unknown provider",
            None,
            &fallback_origin,
            &[],
        );
    };

    let clear_state = cookie_header(&session::clear_state_cookie_header());

    if let Some(err) = q.error.as_deref() {
        debug!(provider = %provider_id, err, "OAuth provider returned error");
        return callback_error(
            &provider_id,
            &format!("Provider error: {err}"),
            &fallback_origin,
            &clear_state,
        );
    }

    let (Some(code), Some(state_param)) = (q.code.as_deref(), q.state.as_deref()) else {
        return callback_error(
            &provider_id,
            "Missing authorization code or state",
            &fallback_origin,
            &clear_state,
        );
    };

    if session::cookie_value(&headers, session::STATE_COOKIE) != Some(state_param) {
        warn!(provider = %provider_id, "OAuth state cookie mismatch");
        return callback_error(
            &provider_id,
            "Invalid OAuth state",
            &fallback_origin,
            &clear_state,
        );
    }

    let flow = match consume_and_validate_state(state_param, &provider_id).await {
        Ok(flow) => flow,
        Err(message) => {
            return callback_error(&provider_id, &message, &fallback_origin, &clear_state);
        }
    };

    let target_origin = flow
        .origin
        .clone()
        .or_else(|| app_url.as_ref().map(|u| u.origin().ascii_serialization()))
        // `resolve_provider` above guarantees `app_url` is `Some` here.
        .unwrap_or_default();

    let info = match exchange_userinfo(&provider, code, &redirect_uri, &flow.pkce_verifier).await {
        Ok(info) => info,
        Err(message) => {
            return callback_error(&provider_id, &message, &target_origin, &clear_state);
        }
    };

    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    let ip_str = ip.to_string();
    if flow.link {
        link_flow(
            &provider_id,
            &info,
            flow.user_id.as_deref(),
            &target_origin,
            &clear_state,
        )
        .await
    } else {
        login_flow(
            &provider_id,
            &info,
            &providers,
            &target_origin,
            &clear_state,
            Some(ip_str.as_str()),
            user_agent.as_deref(),
        )
        .await
    }
}

pub async fn logout(headers: HeaderMap) -> Response {
    if let Some(token) = session::bearer_token(&headers)
        && let Err(e) = session::delete_session(token).await
    {
        error!(error = %e, "Failed to delete session on logout");
    }
    (StatusCode::OK, Json(json!({ "ok": true }))).into_response()
}

pub async fn link_ticket(CurrentUser(user): CurrentUser) -> Response {
    match session::create_link_ticket(&user.id).await {
        Ok(created) => (StatusCode::OK, Json(json!({ "ticket": created.ticket }))).into_response(),
        Err(e) => {
            error!(error = %e, "Failed to create link ticket");
            ApiError::internal("Failed to create link ticket").into_response()
        }
    }
}

pub async fn me(headers: HeaderMap) -> Response {
    match resolve_current_user(&headers).await {
        Some(resolved) => match accounts::identities_for_user(&resolved.user.id).await {
            Ok(identities) => (
                StatusCode::OK,
                Json(json!({
                    "user": resolved.user,
                    "identities": identities,
                    "sessionId": resolved.session_id,
                })),
            )
                .into_response(),
            Err(e) => {
                error!(error = %e, "Failed to fetch identities");
                ApiError::internal("Failed to fetch identities").into_response()
            }
        },
        None => {
            ApiError::with_status(StatusCode::UNAUTHORIZED, "Not authenticated").into_response()
        }
    }
}

pub async fn list_sessions(CurrentUser(user): CurrentUser) -> Response {
    match session::list_sessions_for_user(&user.id).await {
        Ok(sessions) => (StatusCode::OK, Json(json!({ "sessions": sessions }))).into_response(),
        Err(e) => {
            error!(error = %e, "Failed to list sessions");
            ApiError::internal("Failed to list sessions").into_response()
        }
    }
}

pub async fn delete_session(
    CurrentUser(user): CurrentUser,
    Path(session_id): Path<String>,
) -> Response {
    match session::delete_session_by_id_for_user(&session_id, &user.id).await {
        Ok(true) => {
            crate::server::routes::v1::notify_session_revoked(&user.id, &session_id);
            (StatusCode::OK, Json(json!({ "ok": true }))).into_response()
        }
        Ok(false) => ApiError::not_found("Session not found").into_response(),
        Err(e) => {
            error!(error = %e, "Failed to delete session");
            ApiError::internal("Failed to delete session").into_response()
        }
    }
}

/// `POST /auth/sessions/revoke-all` -> revoke all of the caller's sessions
/// except the current one ("sign out everywhere else"). The current session is
/// kept so the caller stays logged in on this device.
pub async fn revoke_all_sessions(headers: HeaderMap) -> Response {
    let Some(resolved) = resolve_current_user(&headers).await else {
        return ApiError::with_status(StatusCode::UNAUTHORIZED, "Not authenticated")
            .into_response();
    };
    match session::delete_other_sessions_for_user(&resolved.user.id, &resolved.session_id).await {
        Ok(ids) => {
            for id in &ids {
                crate::server::routes::v1::notify_session_revoked(&resolved.user.id, id);
            }
            (StatusCode::OK, Json(json!({ "revoked": ids.len() }))).into_response()
        }
        Err(e) => {
            error!(error = %e, "Failed to revoke sessions");
            ApiError::internal("Failed to revoke sessions").into_response()
        }
    }
}

pub async fn delete_account(CurrentUser(user): CurrentUser) -> Response {
    match accounts::delete_user(&user.id).await {
        Ok(result) if result.deleted => {
            for session_id in &result.session_ids {
                crate::server::routes::v1::notify_session_revoked(&user.id, session_id);
            }
            (StatusCode::OK, Json(json!({ "ok": true }))).into_response()
        }
        Ok(_) => ApiError::not_found("Account not found").into_response(),
        Err(e) => {
            error!(error = %e, "Failed to delete account");
            ApiError::internal("Failed to delete account").into_response()
        }
    }
}

pub async fn unlink(CurrentUser(user): CurrentUser, Path(provider_id): Path<String>) -> Response {
    match accounts::unlink(&user.id, &provider_id).await {
        Ok(accounts::UnlinkResult::Unlinked) => {
            (StatusCode::OK, Json(json!({ "ok": true }))).into_response()
        }
        Ok(accounts::UnlinkResult::NotFound) => {
            ApiError::not_found(format!("No linked provider: {provider_id}")).into_response()
        }
        Ok(accounts::UnlinkResult::LastIdentity) => ApiError::with_status(
            StatusCode::BAD_REQUEST,
            "Cannot unlink the last remaining provider",
        )
        .into_response(),
        Err(e) => {
            error!(error = %e, "Failed to unlink provider");
            ApiError::internal("Failed to unlink provider").into_response()
        }
    }
}

pub async fn transfer(
    CurrentUser(user): CurrentUser,
    Json(body): Json<TransferRequest>,
) -> Response {
    let pending = match session::consume_link_transfer(&body.token, &user.id).await {
        Ok(Some(p)) => p,
        Ok(None) => {
            return ApiError::with_status(
                StatusCode::BAD_REQUEST,
                "Invalid or expired transfer token",
            )
            .into_response();
        }
        Err(e) => {
            error!(error = %e, "Failed to consume transfer token");
            return ApiError::internal("Failed to confirm transfer").into_response();
        }
    };

    match accounts::transfer(
        &user.id,
        &pending.provider,
        &pending.provider_subject,
        &pending.source_user_id,
    )
    .await
    {
        Ok(accounts::TransferResult::Transferred) => {
            debug!(
                user_id = %user.id,
                provider = %pending.provider,
                source = %pending.source_user_id,
                "Provider identity transferred"
            );
            (StatusCode::OK, Json(json!({ "ok": true }))).into_response()
        }
        Ok(accounts::TransferResult::NotApplicable) => {
            // Already on the target account — treat as success.
            (StatusCode::OK, Json(json!({ "ok": true }))).into_response()
        }
        Ok(accounts::TransferResult::NotFound) => ApiError::with_status(
            StatusCode::CONFLICT,
            "Transfer state changed; please retry linking",
        )
        .into_response(),
        Err(e) => {
            error!(error = %e, "Transfer failed");
            ApiError::internal("Failed to transfer provider").into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct TransferRequest {
    pub token: String,
}

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Deserialize)]
struct SignedRequestPayload {
    algorithm: String,
    user_id: String,
}

#[derive(Debug, Deserialize)]
pub struct FacebookDeletionForm {
    pub signed_request: String,
}

/// `POST /auth/facebook/deletion` — Facebook Data Deletion Request callback.
/// Verifies the HMAC-SHA256 signed request, deletes the matching user (if any),
/// records the request, and returns the JSON shape Facebook requires.
pub async fn facebook_deletion(Form(form): Form<FacebookDeletionForm>) -> Response {
    let providers = config::get();
    let Some(provider) = providers.get("facebook") else {
        return ApiError::not_found("Facebook provider not configured").into_response();
    };

    let Some(payload) = parse_signed_request(&form.signed_request, &provider.client_secret) else {
        warn!("Facebook data deletion callback: invalid signed_request");
        return ApiError::with_status(StatusCode::BAD_REQUEST, "Invalid signed request")
            .into_response();
    };

    let found_user_id = match sqlx::query_scalar!(
        "SELECT user_id FROM user_oauth_identities WHERE provider = ? AND provider_subject = ?",
        "facebook",
        payload.user_id,
    )
    .fetch_optional(&Database::pool())
    .await
    {
        Ok(uid) => uid,
        Err(e) => {
            error!(error = %e, "Failed to look up Facebook identity for deletion");
            return ApiError::internal("Failed to process deletion request").into_response();
        }
    };

    let status = if let Some(uid) = &found_user_id {
        match accounts::delete_user(uid).await {
            Ok(result) if result.deleted => {
                debug!(user_id = %uid, "Deleted user via Facebook data deletion callback");
                for session_id in &result.session_ids {
                    crate::server::routes::v1::notify_session_revoked(uid, session_id);
                }
                "completed"
            }
            Ok(_) => "unknown_user",
            Err(e) => {
                error!(error = %e, "Failed to delete user from Facebook callback");
                return ApiError::internal("Failed to process deletion request").into_response();
            }
        }
    } else {
        debug!(
            provider_subject = payload.user_id,
            "Facebook deletion request for unknown user — recording and acknowledging"
        );
        "unknown_user"
    };

    let code = ulid::Ulid::new().to_string();

    if let Err(e) = sqlx::query!(
        "
        INSERT INTO data_deletion_requests
            ( confirmation_code
            , provider
            , provider_subject
            , user_id
            , status
            , created_at
            )
        VALUES
            ( ?
            , ?
            , ?
            , ?
            , ?
            , ?
            )
        ",
        code,
        "facebook",
        payload.user_id,
        found_user_id,
        status,
        jiff::Timestamp::now().to_string(),
    )
    .execute(&Database::pool())
    .await
    {
        error!(error = %e, "Failed to record data deletion request");
        return ApiError::internal("Failed to process deletion request").into_response();
    }

    let status_url = {
        let Some(app_url) = providers.app_url.as_ref() else {
            return ApiError::internal("Auth not configured").into_response();
        };

        let mut url = app_url.clone();
        url.path_segments_mut()
            .expect("Failed to get path segments")
            .extend(["api", "v1", "auth", "facebook", "deletion", "status", &code]);
        url.set_query(None);
        url.set_fragment(None);
        url
    };

    (
        StatusCode::OK,
        Json(json!({
            "url": status_url.as_str(),
            "confirmation_code": code,
        })),
    )
        .into_response()
}

/// `GET /auth/facebook/deletion/status/{code}` — human-readable status page
/// for a previously-submitted data deletion request (linked from Facebook's
/// confirmation flow via the `url` returned by [`facebook_deletion`]).
pub async fn facebook_deletion_status(Path(code): Path<String>) -> Response {
    let row = match sqlx::query!(
        "SELECT status, created_at FROM data_deletion_requests WHERE confirmation_code = ?",
        code,
    )
    .fetch_optional(&Database::pool())
    .await
    {
        Ok(row) => row,
        Err(e) => {
            error!(error = %e, "Failed to fetch data deletion status");
            return ApiError::internal("Failed to fetch status").into_response();
        }
    };

    let Some(row) = row else {
        return ApiError::not_found("Invalid confirmation code").into_response();
    };

    let message = match row.status.as_str() {
        "completed" => format!(
            "Your data deletion request (confirmation code {code}) was received on {received} and \
             has been completed.",
            code = code,
            received = row.created_at,
        ),
        "unknown_user" => format!(
            "Your data deletion request (confirmation code {code}) was received on {received}. No \
             account was associated with this Facebook identity, so no further action was \
             required.",
            code = code,
            received = row.created_at,
        ),
        other => format!(
            "Your data deletion request (confirmation code {code}) was received on {received}. \
             Current status: {status}.",
            code = code,
            received = row.created_at,
            status = other,
        ),
    };

    Html(deletion_status_html(&message)).into_response()
}

fn resolve_provider(
    providers: &config::Providers,
    provider_id: &str,
) -> Option<(config::Provider, Url)> {
    let provider = providers.get(provider_id).cloned()?;
    let redirect_uri = providers.redirect_uri(provider_id)?;
    Some((provider, redirect_uri))
}

fn cookie_header(value: &str) -> HeaderValue {
    HeaderValue::from_str(value).expect("cookie header value is always valid")
}

async fn consume_and_validate_state(state: &str, provider_id: &str) -> Result<OAuthState, String> {
    match oauth::consume_state(state).await {
        Ok(Some(flow)) if flow.provider == provider_id => Ok(flow),
        Ok(Some(_)) => Err("State/provider mismatch".to_string()),
        Ok(None) => Err("Invalid or expired OAuth state".to_string()),
        Err(e) => {
            error!(error = %e, "Failed to consume OAuth state");
            Err("Internal error".to_string())
        }
    }
}

async fn exchange_userinfo(
    provider: &config::Provider,
    code: &str,
    redirect_uri: &Url,
    pkce_verifier: &str,
) -> Result<oauth::ProviderUserInfo, String> {
    let access_token = oauth::exchange_code(provider, code, redirect_uri, pkce_verifier)
        .await
        .map_err(|e| {
            warn!(error = %e, "OAuth token exchange failed");
            e.to_string()
        })?;
    oauth::fetch_userinfo(provider, &access_token)
        .await
        .map_err(|e| {
            warn!(error = %e, "OAuth userinfo fetch failed");
            e.to_string()
        })
}

fn callback_error(
    provider_id: &str,
    message: &str,
    target_origin: &str,
    clear_state: &HeaderValue,
) -> Response {
    render_result(
        provider_id,
        false,
        message,
        None,
        target_origin,
        &[clear_state],
    )
}

async fn link_flow(
    provider_id: &str,
    info: &oauth::ProviderUserInfo,
    user_id: Option<&str>,
    target_origin: &str,
    clear_state: &HeaderValue,
) -> Response {
    match user_id {
        Some(user_id) => match accounts::link(provider_id, info, user_id).await {
            Ok(_) => {
                debug!(user_id = %user_id, provider = provider_id, "Provider linked");
                render_result(provider_id, true, "", None, target_origin, &[clear_state])
            }
            Err(accounts::LinkError::AlreadyLinkedToAnother { source_user_id, .. }) => {
                warn!(
                    provider = provider_id,
                    target_user_id = %user_id,
                    source_user_id = %source_user_id,
                    "Link collision: provider on another account; offering transfer"
                );
                match session::create_link_transfer(&session::PendingTransfer {
                    target_user_id: user_id.to_string(),
                    provider: provider_id.to_string(),
                    provider_subject: info.subject.clone(),
                    source_user_id: source_user_id.clone(),
                })
                .await
                {
                    Ok(token) => {
                        render_conflict(provider_id, &token, target_origin, &[clear_state])
                    }
                    Err(e) => {
                        error!(error = %e, "Failed to create transfer token");
                        render_result(
                            provider_id,
                            false,
                            "Internal error",
                            None,
                            target_origin,
                            &[clear_state],
                        )
                    }
                }
            }
            Err(e) => {
                warn!(error = %e, provider = provider_id, "Link failed");
                render_result(
                    provider_id,
                    false,
                    &e.to_string(),
                    None,
                    target_origin,
                    &[clear_state],
                )
            }
        },
        None => render_result(
            provider_id,
            false,
            "Not authenticated; cannot link",
            None,
            target_origin,
            &[clear_state],
        ),
    }
}

async fn login_flow(
    provider_id: &str,
    info: &oauth::ProviderUserInfo,
    providers: &config::Providers,
    target_origin: &str,
    clear_state: &HeaderValue,
    ip: Option<&str>,
    user_agent: Option<&str>,
) -> Response {
    let outcome = match accounts::login(provider_id, info).await {
        Ok(outcome) => outcome,
        Err(e) => {
            error!(error = %e, "Login upsert failed");
            return render_result(
                provider_id,
                false,
                "Internal error",
                None,
                target_origin,
                &[clear_state],
            );
        }
    };
    debug!(user_id = %outcome.user_id, new = outcome.is_new_user, "User logged in");

    match session::create_session(&outcome.user_id, providers.session_max_age, ip, user_agent).await
    {
        Ok(created) => {
            debug!(session_id = %created.id, "Session created");
            render_result(
                provider_id,
                true,
                "",
                Some(&created.token),
                target_origin,
                &[clear_state],
            )
        }
        Err(e) => {
            error!(error = %e, "Failed to create session");
            render_result(
                provider_id,
                false,
                "Internal error",
                None,
                target_origin,
                &[clear_state],
            )
        }
    }
}

fn render_result(
    provider_id: &str,
    ok: bool,
    message: &str,
    token: Option<&str>,
    target_origin: &str,
    cookies: &[&HeaderValue],
) -> Response {
    let payload = json!({
        "type": "zet-auth-callback",
        "ok": ok,
        "provider": provider_id,
        "error": if ok { None } else { Some(message) },
        "token": token,
    });
    let html = callback_html(&payload, target_origin);
    let mut resp = Html(html).into_response();
    for c in cookies {
        resp.headers_mut().append(header::SET_COOKIE, (*c).clone());
    }
    resp
}

fn render_conflict(
    provider_id: &str,
    transfer_token: &str,
    target_origin: &str,
    cookies: &[&HeaderValue],
) -> Response {
    let payload = json!({
        "type": "zet-auth-callback",
        "ok": false,
        "conflict": true,
        "provider": provider_id,
        "transferToken": transfer_token,
    });
    let html = callback_html(&payload, target_origin);
    let mut resp = Html(html).into_response();
    for c in cookies {
        resp.headers_mut().append(header::SET_COOKIE, (*c).clone());
    }
    resp
}

fn callback_html(payload: &serde_json::Value, target_origin: &str) -> String {
    let payload_json = serde_json::to_string(payload)
        .unwrap_or_else(|_| r#"{"type":"zet-auth-callback","ok":false}"#.to_string())
        .replace('&', "\\u0026")
        .replace('\'', "\\u0027")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e");
    let target_origin_repr =
        serde_json::to_string(target_origin).unwrap_or_else(|_| "\"null\"".to_string());
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Signing in…</title>
<style>
  body {{ font-family: system-ui, sans-serif; margin: 0; display: grid; place-items: center; min-height: 100dvh; color: #333; background: #fff; }}
  p {{ opacity: .8; }}
</style>
</head>
<body>
<p>Signing you in… you can close this window.</p>
<script>
(function () {{
  var opener = window.opener;
  var payload = {payload_json};
  var targetOrigin = {target_origin_repr};
  if (opener) {{ try {{ opener.postMessage(payload, targetOrigin); }} catch (e) {{}} }}
  setTimeout(function () {{ window.close(); }}, 50);
}})();
</script>
</body>
</html>"#,
        payload_json = payload_json,
        target_origin_repr = target_origin_repr,
    )
}

/// Verify a Facebook `signed_request`: split on `.`, HMAC-SHA256 the payload
/// with the app secret, constant-time compare to the signature, then decode
/// the payload JSON. Returns the parsed payload only if the signature and
/// algorithm are valid.
fn parse_signed_request(signed_request: &str, app_secret: &str) -> Option<SignedRequestPayload> {
    let (encoded_sig, payload) = signed_request.split_once('.')?;
    let sig = URL_SAFE_NO_PAD
        .decode(encoded_sig.trim_end_matches('='))
        .ok()?;

    let mut mac = HmacSha256::new_from_slice(app_secret.as_bytes()).ok()?;
    mac.update(payload.as_bytes());
    mac.verify_slice(&sig).ok()?;

    let json_bytes = URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?;
    let parsed: SignedRequestPayload = serde_json::from_slice(&json_bytes).ok()?;
    if parsed.algorithm != "HMAC-SHA256" {
        return None;
    }
    Some(parsed)
}

fn deletion_status_html(message: &str) -> String {
    let safe_message = message
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;");
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Data deletion request — ZET Live</title>
<style>
  body {{ font-family: system-ui, sans-serif; margin: 0; display: grid; place-items: center; min-height: 100dvh; color: #333; background: #fff; padding: 1rem; box-sizing: border-box; }}
  main {{ max-width: 36rem; }}
  h1 {{ font-size: 1.25rem; margin: 0 0 1rem; }}
  p {{ line-height: 1.5; opacity: .85; }}
</style>
</head>
<body>
<main>
<h1>Data deletion request</h1>
<p>{safe_message}</p>
</main>
</body>
</html>"#,
        safe_message = safe_message,
    )
}
