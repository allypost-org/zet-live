use serde::Deserialize;

use super::GbfsFeed;
use crate::database::Database;

/// `vehicle_types.json` — `data.vehicle_types`.
#[derive(Debug, Deserialize)]
pub struct VehicleType {
    pub vehicle_type_id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub form_factor: Option<String>,
    #[serde(default)]
    pub propulsion_type: Option<String>,
    #[serde(default)]
    pub rider_capacity: Option<i64>,
    #[serde(default)]
    pub vehicle_image: Option<String>,
    #[serde(default, alias = "_description")]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct VehicleTypesData {
    #[serde(default)]
    pub vehicle_types: Vec<VehicleType>,
}

pub struct Feed;

#[async_trait::async_trait]
impl GbfsFeed for Feed {
    const FEED_NAME: &str = "vehicle_types";
    const METADATA_NAME: &str = "gbfs_vehicle_types_fetch";
    type Data = VehicleTypesData;

    async fn write(data: Self::Data) -> anyhow::Result<usize> {
        let mut tx = Database::pool().begin().await?;

        sqlx::query!("DELETE FROM gbfs_vehicle_types")
            .execute(&mut *tx)
            .await?;

        for vt in &data.vehicle_types {
            #[allow(clippy::cast_possible_truncation)]
            let rider_capacity = vt.rider_capacity.map(|v| v as i32);
            sqlx::query!(
                "
                INSERT INTO
                gbfs_vehicle_types
                    ( vehicle_type_id
                    , name
                    , form_factor
                    , propulsion_type
                    , rider_capacity
                    , vehicle_image
                    , description
                    )
                VALUES
                    ( $1
                    , $2
                    , $3
                    , $4
                    , $5
                    , $6
                    , $7
                    )
                ",
                vt.vehicle_type_id,
                vt.name,
                vt.form_factor,
                vt.propulsion_type,
                rider_capacity,
                vt.vehicle_image,
                vt.description,
            )
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(data.vehicle_types.len())
    }
}
