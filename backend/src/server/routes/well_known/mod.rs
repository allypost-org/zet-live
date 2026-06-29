use axum::{Json, response::IntoResponse};
use reqwest::StatusCode;

use crate::auth;

pub async fn microsoft_identity_association_json() -> impl IntoResponse {
    let providers = auth::config::get();
    let ms = providers.get("microsoft");

    ms.map_or_else(
        || (StatusCode::NOT_FOUND, "No configuration set").into_response(),
        |p| {
            Json(serde_json::json!({
              "associatedApplications": [
                {
                  "applicationId": &p.client_id,
                }
              ]
            }))
            .into_response()
        },
    )
}
