pub mod accounts;
pub mod config;
pub mod oauth;
pub mod session;

use axum::{
    extract::FromRequestParts,
    http::{HeaderMap, StatusCode, request::Parts},
};

use crate::server::error::ApiError;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
}

pub struct ResolvedUser {
    pub user: User,
    pub session_id: String,
}

pub struct CurrentUser(pub User);

impl<S> FromRequestParts<S> for CurrentUser
where
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let resolved = resolve_current_user(&parts.headers)
            .await
            .ok_or_else(|| ApiError::with_status(StatusCode::UNAUTHORIZED, "Not authenticated"))?;
        Ok(Self(resolved.user))
    }
}

pub async fn resolve_current_user(headers: &HeaderMap) -> Option<ResolvedUser> {
    let token = session::bearer_token(headers)?;
    let session_row = session::lookup_session(token).await.ok()??;
    let user = accounts::user_by_id(&session_row.user_id).await?;
    Some(ResolvedUser {
        user,
        session_id: session_row.id,
    })
}
