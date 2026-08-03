use serde::Deserialize;

use super::GbfsFeed;
use crate::database::Database;

/// `system_regions.json` — `data.regions`.
#[derive(Debug, Deserialize)]
pub struct Region {
    pub region_id: String,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SystemRegionsData {
    #[serde(default)]
    pub regions: Vec<Region>,
}

pub struct Feed;

#[async_trait::async_trait]
impl GbfsFeed for Feed {
    const FEED_NAME: &str = "system_regions";
    const METADATA_NAME: &str = "gbfs_system_regions_fetch";
    type Data = SystemRegionsData;

    async fn write(data: Self::Data) -> anyhow::Result<usize> {
        let mut tx = Database::pool().begin().await?;

        sqlx::query!("DELETE FROM gbfs_regions")
            .execute(&mut *tx)
            .await?;

        for region in &data.regions {
            sqlx::query!(
                "
                INSERT INTO
                gbfs_regions
                    ( region_id, name )
                VALUES
                    ( $1, $2 )
                ",
                region.region_id,
                region.name,
            )
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(data.regions.len())
    }
}
