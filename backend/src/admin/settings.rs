use serde::{Deserialize, Serialize};

use crate::database::Database;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminSettings {
    pub realtime_url: Option<url::Url>,
    pub static_url: Option<url::Url>,
    pub gbfs_url: Option<url::Url>,
    pub realtime_paused: Option<bool>,
    pub static_paused: Option<bool>,
    pub gbfs_paused: Option<bool>,
    #[serde(default)]
    pub global_notices: Vec<GlobalNotice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalNotice {
    pub id: String,
    pub text: String,
    pub severity: NoticeSeverity,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NoticeSeverity {
    #[default]
    Info,
    Warning,
    Error,
}

pub async fn load_from_db() -> AdminSettings {
    let rows = sqlx::query!(
        "
        SELECT
              name as \"name!\"
            , value AS \"value: serde_json::Value\"
        FROM admin_settings
        "
    )
    .fetch_all(&Database::pool())
    .await
    .unwrap_or_default();

    let map = rows.into_iter().fold(serde_json::Map::new(), |mut acc, x| {
        acc.insert(x.name, x.value);

        acc
    });
    let value = serde_json::Value::Object(map);

    serde_json::from_value(value).unwrap_or_default()
}
