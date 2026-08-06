use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::Value;
use tracing::{debug, error};

use crate::{auth::CurrentUser, database::Database, server::error::ApiError};

/// `GET /settings` -> the user's settings JSON (`404` if none saved yet).
pub async fn get_settings(CurrentUser(user): CurrentUser) -> Response {
    match sqlx::query_scalar!(
        "SELECT settings AS \"settings: serde_json::Value\" FROM user_settings WHERE user_id = $1",
        user.id,
    )
    .fetch_optional(&Database::pool())
    .await
    {
        Ok(Some(value)) => (StatusCode::OK, Json(value)).into_response(),
        Ok(None) => ApiError::not_found("No settings saved").into_response(),
        Err(e) => {
            error!(error = %e, "Failed to fetch settings");
            ApiError::internal("Failed to fetch settings").into_response()
        }
    }
}

/// `PUT /settings` -> upsert the settings JSON blob.
pub async fn put_settings(CurrentUser(user): CurrentUser, Json(value): Json<Value>) -> Response {
    // Reject non-objects to keep the blob a settings map.
    if !value.is_object() {
        return ApiError::with_status(StatusCode::BAD_REQUEST, "Settings must be a JSON object")
            .into_response();
    }

    let now = crate::database::time::now();

    let res = sqlx::query!(
        "
        INSERT INTO user_settings ( user_id, settings, updated_at )
        VALUES ( $1, $2, $3 )
        ON CONFLICT(user_id) DO UPDATE
            SET settings   = excluded.settings,
                updated_at = excluded.updated_at
        ",
        user.id,
        value,
        now,
    )
    .execute(&Database::pool())
    .await;

    match res {
        Ok(_) => {
            debug!(user_id = %user.id, "Settings saved");
            (StatusCode::OK, Json(value)).into_response()
        }
        Err(e) => {
            error!(error = %e, "Failed to save settings");
            ApiError::internal("Failed to save settings").into_response()
        }
    }
}
