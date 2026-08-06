use serde::Deserialize;

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

        for station in &data.stations {
            let rental_uris = match &station.rental_uris {
                Some(value) => Some(serde_json::to_string(value)?),
                None => None,
            };
            #[allow(clippy::cast_possible_truncation)]
            let capacity = station.capacity.map(|c| c as i32);

            sqlx::query!(
                "
                        INSERT INTO
                        gbfs_stations
                            ( station_id
                            , name
                            , short_name
                            , lat
                            , lon
                            , region_id
                            , capacity
                            , is_virtual_station
                            , rental_uris
                            )
                        VALUES
                            ( $1, $2, $3, $4, $5, $6, $7, $8, $9 )
                        ",
                station.station_id,
                station.name,
                station.short_name,
                station.lat,
                station.lon,
                station.region_id,
                capacity,
                station.is_virtual_station,
                rental_uris,
            )
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(data.stations.len())
    }
}
