use std::{
    str::FromStr,
    sync::OnceLock,
    time::{Duration, Instant},
};

use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use tracing::{debug, trace, warn};

use crate::cli::DatabaseUrl;

pub mod schedule_metadata;
pub mod schedule_offsets;
pub mod service_days;
pub mod time;

static DATABASE: OnceLock<PgPool> = OnceLock::new();

const SLOW_THRESHOLD: Duration = Duration::from_millis(30);

pub struct Database;

impl Database {
    pub async fn init(url: &DatabaseUrl) -> anyhow::Result<PgPool> {
        let connection_string = url.as_str();

        debug!(url = ?url, "Initializing database");

        let connect_options =
            PgConnectOptions::from_str(connection_string)?.options([("random_page_cost", "1.1")]);

        let pool = PgPoolOptions::new()
            .max_connections(20)
            .connect_with(connect_options)
            .await?;

        sqlx::migrate!("./migrations").run(&pool).await?;

        DATABASE
            .set(pool.clone())
            .map_err(|_| anyhow::anyhow!("Failed to initialize database, pool already set"))?;

        schedule_offsets::init().await?;
        schedule_metadata::init().await?;
        service_days::init().await?;

        debug!("Database initialized");

        Ok(pool)
    }

    pub fn pool() -> PgPool {
        DATABASE.get().expect("Database not initialized").clone()
    }
}

impl Database {
    pub async fn logged<F, T>(label: &str, fut: F) -> T
    where
        F: std::future::Future<Output = T>,
    {
        let start = Instant::now();
        let result = fut.await;
        let elapsed = start.elapsed();
        trace!(target: "query", query = label, ?elapsed, "query executed");
        if elapsed > SLOW_THRESHOLD {
            warn!(target: "query", query = label, ?elapsed, "slow query");
        }
        result
    }
}
