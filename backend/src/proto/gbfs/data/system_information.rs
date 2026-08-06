use serde::Deserialize;

use super::GbfsFeed;
use crate::database::Database;

/// `system_information.json` — `data` is a single object describing the system.
#[derive(Debug, Deserialize)]
pub struct SystemInformation {
    pub system_id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub operator: Option<String>,
    #[serde(default)]
    pub url: Option<url::Url>,
    #[serde(default)]
    pub phone_number: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub feed_contact_email: Option<String>,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub license_id: Option<String>,
    #[serde(default)]
    pub rental_apps: Option<serde_json::Value>,
}

pub struct Feed;

#[async_trait::async_trait]
impl GbfsFeed for Feed {
    const FEED_NAME: &str = "system_information";
    const METADATA_NAME: &str = "gbfs_system_information_fetch";
    type Data = SystemInformation;

    async fn write(data: Self::Data) -> anyhow::Result<usize> {
        let rental_apps = match &data.rental_apps {
            Some(value) => Some(serde_json::to_string(value)?),
            None => None,
        };

        Database::logged(
            "upsert_gbfs_system_information",
            sqlx::query!(
                "
                INSERT INTO
                gbfs_system_information
                    ( system_id
                    , name
                    , operator
                    , url
                    , phone_number
                    , email
                    , feed_contact_email
                    , timezone
                    , language
                    , license_id
                    , rental_apps
                    )
                VALUES
                    ( $1
                    , $2
                    , $3
                    , $4
                    , $5
                    , $6
                    , $7
                    , $8
                    , $9
                    , $10
                    , $11
                    )
                    ON CONFLICT(system_id)
                    DO UPDATE SET
                      name               = excluded.name
                    , operator           = excluded.operator
                    , url                = excluded.url
                    , phone_number       = excluded.phone_number
                    , email              = excluded.email
                    , feed_contact_email = excluded.feed_contact_email
                    , timezone           = excluded.timezone
                    , language           = excluded.language
                    , license_id         = excluded.license_id
                    , rental_apps        = excluded.rental_apps
                ",
                data.system_id,
                data.name,
                data.operator,
                data.url.map(|url| url.to_string()),
                data.phone_number,
                data.email,
                data.feed_contact_email,
                data.timezone,
                data.language,
                data.license_id,
                rental_apps,
            )
            .execute(&Database::pool()),
        )
        .await?;

        Ok(1)
    }
}
