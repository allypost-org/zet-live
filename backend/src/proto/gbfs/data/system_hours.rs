use serde::Deserialize;

use super::GbfsFeed;
use crate::database::Database;

/// `system_hours.json` — `data.rental_hours`.
#[derive(Debug, Deserialize)]
pub struct RentalHour {
    #[serde(default)]
    pub user_types: Option<Vec<String>>,
    #[serde(default)]
    pub days: Option<Vec<String>>,
    #[serde(default)]
    pub start_time: Option<String>,
    #[serde(default)]
    pub end_time: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SystemHoursData {
    #[serde(default)]
    pub rental_hours: Vec<RentalHour>,
}

pub struct Feed;

#[async_trait::async_trait]
impl GbfsFeed for Feed {
    const FEED_NAME: &str = "system_hours";
    const METADATA_NAME: &str = "gbfs_system_hours_fetch";
    type Data = SystemHoursData;

    async fn write(data: Self::Data) -> anyhow::Result<usize> {
        let mut tx = Database::pool().begin().await?;

        sqlx::query!("DELETE FROM gbfs_rental_hours")
            .execute(&mut *tx)
            .await?;

        for hour in &data.rental_hours {
            let user_types = match &hour.user_types {
                Some(value) => Some(serde_json::to_string(value)?),
                None => None,
            };
            let days = match &hour.days {
                Some(value) => Some(serde_json::to_string(value)?),
                None => None,
            };

            sqlx::query!(
                "
                INSERT INTO
                gbfs_rental_hours
                    ( user_types
                    , days
                    , start_time
                    , end_time
                    )
                VALUES
                    ( $1
                    , $2
                    , $3
                    , $4
                    )
                ",
                user_types,
                days,
                hour.start_time,
                hour.end_time,
            )
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(data.rental_hours.len())
    }
}
