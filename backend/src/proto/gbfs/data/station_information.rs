use serde::Deserialize;
use sqlx::{Postgres, QueryBuilder};

use super::GbfsFeed;
use crate::database::Database;

/// `station_information.json` — `data.stations`. Static-ish station locations.
#[derive(Debug, Deserialize)]
pub struct Station {
    pub station_id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub short_name: Option<String>,
    pub lat: f64,
    pub lon: f64,
    #[serde(default)]
    pub region_id: Option<String>,
    #[serde(default)]
    pub capacity: Option<i64>,
    #[serde(default)]
    pub is_virtual_station: bool,
    #[serde(default)]
    pub rental_uris: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct StationInformationData {
    #[serde(default)]
    pub stations: Vec<Station>,
}

const BATCH_SIZE: usize = 2_048;

pub struct Feed;

#[async_trait::async_trait]
impl GbfsFeed for Feed {
    const FEED_NAME: &str = "station_information";
    const METADATA_NAME: &str = "gbfs_station_information_fetch";
    type Data = StationInformationData;

    async fn write(data: Self::Data) -> anyhow::Result<usize> {
        let mut tx = Database::pool().begin().await?;

        sqlx::query!("DELETE FROM gbfs_stations")
            .execute(&mut *tx)
            .await?;

        for batch in data.stations.chunks(BATCH_SIZE) {
            let rows = batch
                .iter()
                .map(|station| {
                    let rental_uris = station
                        .rental_uris
                        .as_ref()
                        .map(serde_json::to_string)
                        .transpose()?;
                    #[allow(clippy::cast_possible_truncation)]
                    let capacity = station.capacity.map(|c| c as i32);
                    Ok::<_, serde_json::Error>((station, capacity, rental_uris))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO gbfs_stations
                 (station_id, name, short_name, lat, lon, region_id, capacity,
                  is_virtual_station, rental_uris) ",
            );
            query.push_values(&rows, |mut row, (station, capacity, rental_uris)| {
                row.push_bind(&station.station_id)
                    .push_bind(&station.name)
                    .push_bind(&station.short_name)
                    .push_bind(station.lat)
                    .push_bind(station.lon)
                    .push_bind(&station.region_id)
                    .push_bind(capacity)
                    .push_bind(station.is_virtual_station)
                    .push_bind(rental_uris);
            });
            query.build().persistent(false).execute(&mut *tx).await?;
        }

        tx.commit().await?;
        Ok(data.stations.len())
    }
}
