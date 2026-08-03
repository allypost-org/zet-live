use serde::{Deserialize, Serialize};

use super::GbfsFeed;
use crate::database::Database;

/// `system_pricing_plans.json` — `data.plans`.
#[derive(Debug, Deserialize, Serialize)]
pub struct PerMinutePricing {
    #[serde(default)]
    pub start: Option<i64>,
    #[serde(default)]
    pub interval: Option<i64>,
    #[serde(default)]
    pub rate: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct PricingPlan {
    pub plan_id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub currency: Option<String>,
    #[serde(default)]
    pub price: Option<f64>,
    #[serde(default)]
    pub is_taxable: bool,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub per_min_pricing: Option<Vec<PerMinutePricing>>,
}

#[derive(Debug, Deserialize)]
pub struct SystemPricingPlansData {
    #[serde(default)]
    pub plans: Vec<PricingPlan>,
}

pub struct Feed;

#[async_trait::async_trait]
impl GbfsFeed for Feed {
    const FEED_NAME: &str = "system_pricing_plans";
    const METADATA_NAME: &str = "gbfs_system_pricing_plans_fetch";
    type Data = SystemPricingPlansData;

    async fn write(data: Self::Data) -> anyhow::Result<usize> {
        let mut tx = Database::pool().begin().await?;

        sqlx::query!("DELETE FROM gbfs_pricing_plans")
            .execute(&mut *tx)
            .await?;

        for plan in &data.plans {
            let per_min_pricing = match &plan.per_min_pricing {
                Some(value) => Some(serde_json::to_string(value)?),
                None => None,
            };
            let price = plan
                .price
                .and_then(|f| rust_decimal::Decimal::try_from(f).ok());

            sqlx::query!(
                "
                INSERT INTO
                gbfs_pricing_plans
                    ( plan_id
                    , name
                    , currency
                    , price
                    , is_taxable
                    , description
                    , per_min_pricing
                    )
                VALUES
                    ( $1, $2, $3, $4, $5, $6, $7 )
                ",
                plan.plan_id,
                plan.name,
                plan.currency,
                price,
                plan.is_taxable,
                plan.description,
                per_min_pricing,
            )
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(data.plans.len())
    }
}
