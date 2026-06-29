use axum::{Json, response::IntoResponse};
use serde_json::json;

use crate::auth::config;

pub async fn get_capabilities() -> impl IntoResponse {
    let providers = config::get();
    Json(json!({
        "appUrl": providers.app_url,
        "auth": {
            "providers": providers.public_list(),
        }
    }))
}
