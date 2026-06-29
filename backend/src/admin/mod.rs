use std::sync::LazyLock;

use tokio::{net::TcpListener, sync::RwLock};
use tracing::{debug, error, info, trace};

use crate::{cli::ServerConfig, database::Database};

pub mod feedback;
pub mod metadata;
pub mod router;
pub mod settings;
pub mod user_notices;

pub static ADMIN_SETTINGS: LazyLock<RwLock<settings::AdminSettings>> =
    LazyLock::new(|| RwLock::new(settings::AdminSettings::default()));

pub async fn init() {
    debug!("Loading admin settings from database");
    let loaded = settings::load_from_db().await;
    *ADMIN_SETTINGS.write().await = loaded.clone();
    debug!(?loaded, "Admin settings loaded");

    crate::server::routes::v1::broadcast_notices(&loaded.global_notices).await;
}

pub async fn run(config: &ServerConfig) -> anyhow::Result<()> {
    let Some(admin_key) = config.admin_key.as_ref() else {
        info!("Admin key not set, not starting admin server");
        return Ok(());
    };

    let Some(addr) = config.admin_bind_to else {
        info!("Admin bind not set, not starting admin server");
        return Ok(());
    };

    let state = crate::admin::router::AdminState {
        admin_key: admin_key.clone(),
    };

    let app = crate::admin::router::create_admin_router(state);

    info!("Starting admin server on http://{addr}");

    let listener = match TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            error!(?e, "Failed to bind admin server");
            return Err(
                anyhow::anyhow!(e).context(format!("Failed to bind admin server {:?}", addr))
            );
        }
    };

    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            error!(?e, "Admin server error");
        }
    });

    Ok(())
}

pub async fn update_setting(
    name: &str,
    value: serde_json::Value,
) -> Result<settings::AdminSettings, UpdateSettingError> {
    trace!(name, ?value, "Updating admin setting");

    let now = jiff::Zoned::now().to_string();
    let value_str = serde_json::to_string(&value)
        .map_err(|e| UpdateSettingError::Serialization(e.to_string()))?;

    let probe = {
        let mut map = serde_json::Map::new();
        map.insert(name.to_string(), value);
        serde_json::Value::Object(map)
    };
    serde_json::from_value::<settings::AdminSettings>(probe)
        .map_err(|e| UpdateSettingError::InvalidSetting(name.to_string(), e.to_string()))?;

    sqlx::query!(
        "
        INSERT INTO admin_settings
            ( name
            , value
            , updated_at
            )
        VALUES
            ( ?
            , ?
            , ?
            )
         ON CONFLICT(name)
         DO UPDATE SET
              value = excluded.value
            , updated_at = excluded.updated_at
        ",
        name,
        value_str,
        now,
    )
    .execute(&Database::pool())
    .await
    .map_err(|e| UpdateSettingError::Database(e.to_string()))?;

    let loaded = settings::load_from_db().await;
    *ADMIN_SETTINGS.write().await = loaded.clone();

    if name == "globalNotices" {
        crate::server::routes::v1::broadcast_notices(&loaded.global_notices).await;
    }

    if name == "gbfsUrl" {
        crate::proto::gbfs::discovery::invalidate().await;
        crate::proto::gbfs::fetcher::force_sync();
    }

    debug!(name, "Admin setting updated and reloaded");
    Ok(loaded)
}

#[derive(Debug, thiserror::Error)]
pub enum UpdateSettingError {
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("Database error: {0}")]
    Database(String),
    #[error("Invalid setting {0:?}: {1}")]
    InvalidSetting(String, String),
}
