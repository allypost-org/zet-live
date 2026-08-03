use serde::{Deserialize, Serialize};

use super::GbfsFeed;
use crate::database::Database;

/// `station_status.json` — `data.stations`. Realtime per-station availability.
#[derive(Debug, Deserialize, Serialize)]
pub struct VehicleTypeCount {
    pub vehicle_type_id: String,
    #[serde(default)]
    pub count: i64,
}

#[derive(Debug, Deserialize)]
pub struct StationStatus {
    pub station_id: String,
    #[serde(default)]
    pub num_bikes_available: Option<i64>,
    #[serde(default)]
    pub num_docks_available: Option<i64>,
    #[serde(default)]
    pub vehicle_types_available: Option<Vec<VehicleTypeCount>>,
    #[serde(default)]
    pub is_installed: bool,
    #[serde(default)]
    pub is_renting: bool,
    #[serde(default)]
    pub is_returning: bool,
    #[serde(default)]
    pub last_reported: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct StationStatusData {
    #[serde(default)]
    pub stations: Vec<StationStatus>,
}

pub struct Feed;

#[async_trait::async_trait]
impl GbfsFeed for Feed {
    const FEED_NAME: &str = "station_status";
    const METADATA_NAME: &str = "gbfs_station_status_fetch";
    type Data = StationStatusData;

    async fn write(data: Self::Data) -> anyhow::Result<usize> {
        let mut tx = Database::pool().begin().await?;

        sqlx::query!("DELETE FROM gbfs_station_status")
            .execute(&mut *tx)
            .await?;

        for station in &data.stations {
            let vehicle_types_available = match &station.vehicle_types_available {
                Some(value) => Some(serde_json::to_string(value)?),
                None => None,
            };
            #[allow(clippy::cast_possible_truncation)]
            let num_bikes_available = station.num_bikes_available.map(|v| v as i32);
            #[allow(clippy::cast_possible_truncation)]
            let num_docks_available = station.num_docks_available.map(|v| v as i32);

            sqlx::query!(
                "
                        INSERT INTO
                        gbfs_station_status
                            ( station_id
                            , num_bikes_available
                            , num_docks_available
                            , is_installed
                            , is_renting
                            , is_returning
                            , last_reported
                            , vehicle_types_available
                            )
                        VALUES
                            ( $1, $2, $3, $4, $5, $6, $7, $8 )
                        ",
                station.station_id,
                num_bikes_available,
                num_docks_available,
                station.is_installed,
                station.is_renting,
                station.is_returning,
                station.last_reported,
                vehicle_types_available,
            )
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(data.stations.len())
    }
}
