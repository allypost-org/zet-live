use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::Rng;
use sha2::{Digest, Sha256};
use url::Url;

use crate::{
    auth::config::{Provider, UserinfoMapping},
    database::Database,
    http_client::OAUTH_HTTP_CLIENT,
};

#[derive(Debug, Clone)]
pub struct ProviderUserInfo {
    pub subject: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub picture: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OAuthState {
    pub provider: String,
    pub pkce_verifier: String,
    pub link: bool,
    pub origin: Option<String>,
    pub user_id: Option<String>,
}

pub fn build_auth_url(
    provider: &Provider,
    redirect_uri: &Url,
    link: bool,
) -> (Url, String, String) {
    let mut bytes = [0u8; 64];

    rand::rng().fill_bytes(&mut bytes);
    let state = URL_SAFE_NO_PAD.encode(bytes);

    rand::rng().fill_bytes(&mut bytes);
    let pkce_verifier = URL_SAFE_NO_PAD.encode(bytes);
    let pkce_challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(pkce_verifier.as_bytes()));

    let mut url = provider.auth_url.clone();
    {
        let mut query = url.query_pairs_mut();
        query
            .append_pair("response_type", "code")
            .append_pair("client_id", &provider.client_id)
            .append_pair("redirect_uri", redirect_uri.as_str())
            .append_pair("state", &state)
            .append_pair("code_challenge", &pkce_challenge)
            .append_pair("code_challenge_method", "S256");
        query.append_pair("scope", &provider.scopes.join(" "));
        if link {
            query.append_pair("prompt", "login");
        }
    }

    (url, state, pkce_verifier)
}

pub async fn create_state(state: &str, flow: &OAuthState) -> Result<(), sqlx::Error> {
    let now = jiff::Timestamp::now();
    let expires = now
        + jiff::Span::new()
            .try_seconds(600)
            .expect("600s is a valid span");
    let link_i = i64::from(flow.link);

    sqlx::query!(
        "
        INSERT INTO oauth_states
            ( state
            , provider
            , pkce_verifier
            , link
            , origin
            , user_id
            , created_at
            , expires_at
            )
        VALUES
            ( ?
            , ?
            , ?
            , ?
            , ?
            , ?
            , ?
            , ?
            )
        ",
        state,
        flow.provider,
        flow.pkce_verifier,
        link_i,
        flow.origin,
        flow.user_id,
        now.to_string(),
        expires.to_string(),
    )
    .execute(&Database::pool())
    .await?;

    Ok(())
}

pub async fn consume_state(state: &str) -> Result<Option<OAuthState>, sqlx::Error> {
    let now = jiff::Timestamp::now().to_string();

    let row = sqlx::query!(
        "
        DELETE FROM oauth_states
        WHERE state = ? AND expires_at > ?
        RETURNING provider, pkce_verifier, link, origin, user_id
        ",
        state,
        now,
    )
    .fetch_optional(&Database::pool())
    .await?;

    Ok(row.map(|r| OAuthState {
        provider: r.provider,
        pkce_verifier: r.pkce_verifier,
        link: r.link != 0,
        origin: r.origin,
        user_id: r.user_id,
    }))
}

pub async fn exchange_code(
    provider: &Provider,
    code: &str,
    redirect_uri: &Url,
    code_verifier: &str,
) -> Result<String, OAuthError> {
    let body = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("grant_type", "authorization_code")
        .append_pair("code", code)
        .append_pair("redirect_uri", redirect_uri.as_str())
        .append_pair("client_id", provider.client_id.as_str())
        .append_pair("client_secret", provider.client_secret.as_str())
        .append_pair("code_verifier", code_verifier)
        .finish();

    let res = OAUTH_HTTP_CLIENT
        .post(provider.token_url.clone())
        .header(
            axum::http::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(body)
        .send()
        .await
        .map_err(|e| OAuthError::Provider(format!("token request failed: {e}")))?;

    let status = res.status();
    let value: serde_json::Value = res
        .json()
        .await
        .map_err(|e| OAuthError::Provider(format!("token decode failed: {e}")))?;

    if let Some(token) = value
        .get("access_token")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        return Ok(token.to_string());
    }

    let detail = token_error_message(&value).unwrap_or_else(|| format!("HTTP {status}"));
    Err(OAuthError::Provider(format!("token endpoint: {detail}")))
}

fn token_error_message(value: &serde_json::Value) -> Option<String> {
    fn as_str(v: Option<&serde_json::Value>) -> Option<String> {
        v.and_then(|v| v.as_str())
            .map(str::to_string)
            .filter(|s| !s.is_empty())
    }
    as_str(value.get("error_description"))
        .or_else(|| as_str(value.get("error"))) // Google/Microsoft: flat error string
        .or_else(|| as_str(value.get("error").and_then(|e| e.get("message")))) // Facebook: nested
        .or_else(|| as_str(value.get("error_user_msg"))) // Facebook
        .or_else(|| as_str(value.get("message")))
}

pub async fn fetch_userinfo(
    provider: &Provider,
    access_token: &str,
) -> Result<ProviderUserInfo, OAuthError> {
    let res = OAUTH_HTTP_CLIENT
        .get(provider.userinfo_url.clone())
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| OAuthError::Provider(format!("userinfo request failed: {e}")))?;

    if !res.status().is_success() {
        return Err(OAuthError::Provider(format!(
            "userinfo HTTP {}",
            res.status()
        )));
    }

    let value: serde_json::Value = res
        .json()
        .await
        .map_err(|e| OAuthError::Provider(format!("userinfo decode failed: {e}")))?;

    extract_userinfo(provider.mapping, &value)
}

fn extract_userinfo(
    mapping: UserinfoMapping,
    value: &serde_json::Value,
) -> Result<ProviderUserInfo, OAuthError> {
    let subject = str_at(value, mapping.subject)
        .ok_or_else(|| OAuthError::Provider("userinfo missing subject".to_string()))?;

    Ok(ProviderUserInfo {
        subject: subject.to_string(),
        email: str_at(value, mapping.email).map(str::to_string),
        name: str_at(value, mapping.name).map(str::to_string),
        picture: resolve_picture(value, mapping.picture),
    })
}

fn str_at<'a>(value: &'a serde_json::Value, path: &str) -> Option<&'a str> {
    let mut current = value;
    for seg in path.split('.') {
        if seg.is_empty() {
            return None;
        }
        current = current.get(seg)?;
    }
    let current = current.as_str()?;

    if current.is_empty() {
        None
    } else {
        Some(current)
    }
}

/// Resolve the avatar `picture` format string. `{path}` placeholders are
/// replaced by the (dotted-path) string value. If `format` is empty, or any
/// placeholder can't be resolved, returns `None` (clean brand-icon fallback).
fn resolve_picture(value: &serde_json::Value, format: &str) -> Option<String> {
    if format.is_empty() {
        return None;
    }
    let mut out = String::new();
    let mut rest = format;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let close = after.find('}')?;
        let path = &after[..close];
        let resolved = str_at(value, path)?;
        out.push_str(resolved);
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    Some(out)
}

#[derive(Debug, thiserror::Error)]
pub enum OAuthError {
    #[error("provider error: {0}")]
    Provider(String),
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}
