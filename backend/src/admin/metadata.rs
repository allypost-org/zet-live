use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_with::{DurationMilliSeconds, serde_as};

use crate::database::Database;

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataEntry {
    pub status: MetadataStatus,
    pub last_sync_at: jiff::Timestamp,
    pub error_message: Option<String>,
    pub records_processed: Option<u64>,
    #[serde_as(as = "Option<DurationMilliSeconds>")]
    pub duration_ms: Option<Duration>,
}
impl MetadataEntry {
    pub fn in_progress() -> Self {
        Self::new(MetadataStatus::InProgress)
    }

    pub fn success() -> Self {
        Self::new(MetadataStatus::Success)
    }

    pub fn error() -> Self {
        Self::new(MetadataStatus::Error)
    }

    pub fn skipped() -> Self {
        Self::new(MetadataStatus::Skipped)
    }

    pub fn paused() -> Self {
        Self::new(MetadataStatus::Paused)
    }
}

impl MetadataEntry {
    pub fn new(status: MetadataStatus) -> Self {
        Self {
            status,
            last_sync_at: jiff::Timestamp::now(),
            error_message: None,
            records_processed: None,
            duration_ms: None,
        }
    }

    pub fn with_error_message(mut self, error_message: String) -> Self {
        self.error_message = Some(error_message);
        self
    }

    pub const fn with_records_processed(mut self, records_processed: u64) -> Self {
        self.records_processed = Some(records_processed);
        self
    }

    pub const fn with_duration(mut self, duration: Duration) -> Self {
        self.duration_ms = Some(duration);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MetadataStatus {
    InProgress,
    Success,
    Error,
    Skipped,
    Paused,
}

pub async fn write_metadata(name: &str, entry: &MetadataEntry) {
    let now = crate::database::time::now();
    let value = match serde_json::to_value(entry) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(name, error = %e, "Failed to serialize metadata entry");
            return;
        }
    };

    let res = sqlx::query!(
        "
        INSERT INTO admin_metadata
            ( name
            , value
            , updated_at
            )
        VALUES
            ( $1
            , $2
            , $3
            )
        ON CONFLICT(name)
        DO UPDATE SET
              value = excluded.value
            , updated_at = excluded.updated_at
        ",
        name,
        value,
        now,
    )
    .execute(&Database::pool())
    .await;

    if let Err(e) = res {
        tracing::warn!(name, error = %e, "Failed to write metadata to database");
    }
}

pub async fn read_all_metadata() -> Vec<(String, MetadataEntry)> {
    let rows = sqlx::query!(
        "
        SELECT
              name as \"name!\"
            , value AS \"value: serde_json::Value\"
        FROM admin_metadata
        "
    )
    .fetch_all(&Database::pool())
    .await
    .unwrap_or_default();

    rows.into_iter()
        .filter_map(|x| {
            let entry: MetadataEntry = serde_json::from_value(x.value).ok()?;
            Some((x.name, entry))
        })
        .collect()
}
