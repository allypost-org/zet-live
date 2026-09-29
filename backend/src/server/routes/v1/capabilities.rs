use axum::{Json, http::HeaderMap, response::IntoResponse};
use serde_json::json;

use crate::{
    auth::{self, config},
    feature_flags,
};

pub async fn get_capabilities(headers: HeaderMap) -> impl IntoResponse {
    let providers = config::get();
    let user = auth::resolve_current_user(&headers).await;
    let flag_user = user.as_ref().map(|r| r.user.id.as_str());

    Json(json!({
        "appUrl": providers.app_url,
        "auth": {
            "providers": providers.public_list(),
        },
        "featureFlags": feature_flags::enabled_map(flag_user),
    }))
}
