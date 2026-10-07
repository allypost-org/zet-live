use serde::{Deserialize, Serialize};
use sqlx::{Postgres, QueryBuilder};

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

const BATCH_SIZE: usize = 2_048;

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

        for batch in data.stations.chunks(BATCH_SIZE) {
            let rows = batch
                .iter()
                .map(|station| {
                    let vehicle_types_available = station
                        .vehicle_types_available
                        .as_ref()
                        .map(serde_json::to_string)
                        .transpose()?;
                    #[allow(clippy::cast_possible_truncation)]
                    let num_bikes_available = station.num_bikes_available.map(|v| v as i32);
                    #[allow(clippy::cast_possible_truncation)]
                    let num_docks_available = station.num_docks_available.map(|v| v as i32);
                    Ok::<_, serde_json::Error>((
                        station,
                        num_bikes_available,
                        num_docks_available,
                        vehicle_types_available,
                    ))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO gbfs_station_status
                 (station_id, num_bikes_available, num_docks_available, is_installed,
                  is_renting, is_returning, last_reported, vehicle_types_available) ",
            );
            query.push_values(
                &rows,
                |mut row, (station, bikes, docks, vehicle_types_available)| {
                    row.push_bind(&station.station_id)
                        .push_bind(bikes)
                        .push_bind(docks)
                        .push_bind(station.is_installed)
                        .push_bind(station.is_renting)
                        .push_bind(station.is_returning)
                        .push_bind(station.last_reported)
                        .push_bind(vehicle_types_available);
                },
            );
            query.build().persistent(false).execute(&mut *tx).await?;
        }

        tx.commit().await?;
        Ok(data.stations.len())
    }
}
