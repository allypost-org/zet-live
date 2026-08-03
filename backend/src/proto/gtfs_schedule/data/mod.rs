use std::time::Instant;

use serde::de::DeserializeOwned;
use sqlx::AssertSqlSafe;
use tracing::{Instrument, debug, trace, warn};

use crate::database::Database;

pub mod route;
pub mod shape;
pub mod stop;
pub mod stop_time;
pub mod trip;

pub use route::*;
pub use shape::*;
pub use stop::*;
pub use stop_time::*;
pub use trip::*;

#[derive(Debug)]
pub struct GtfsSchedule;
impl GtfsSchedule {
    #[allow(clippy::too_many_lines)]
    pub async fn read_from_zip_bytes(zip_bytes: prost::bytes::Bytes) -> Result<(), FileDataError> {
        debug!("Reading GTFS schedule from zip bytes");

        let (query_tx, mut query_rx) = tokio::sync::mpsc::unbounded_channel::<BulkInsert>();

        let queries_fut = tokio::task::spawn(
            async move {
                let mut tx = Database::pool().begin().await?;
                let start = Instant::now();
                debug!("Starting query execution");
                let mut i = 0;
                let mut last_checkpoint = Instant::now();
                while let Some(bulk_insert) = query_rx.recv().await {
                    const LOG_EVERY_I: usize = 10_000;

                    let res = match bulk_insert {
                        BulkInsert::DeleteAll(table) => {
                            sqlx::query(AssertSqlSafe(format!("DELETE FROM {table}")))
                                .execute(&mut *tx)
                                .await
                                .map_err(|e| {
                                    anyhow::anyhow!(e)
                                        .context(format!("Failed to delete from {}", table))
                                })
                        }
                        BulkInsert::Route(r) => {
                            let route_type = r.route_type.map(|t| t as i32);
                            let url = r.url.map(|u| u.to_string());
                            sqlx::query!(
                                "
                                INSERT INTO
                                gtfs_routes
                                    ( route_id
                                    , agency_id
                                    , route_short_name
                                    , route_long_name
                                    , route_url
                                    , route_desc
                                    , route_type
                                    , route_color
                                    , route_text_color
                                    )
                                VALUES
                                    ( ?
                                    , ?
                                    , ?
                                    , ?
                                    , ?
                                    , ?
                                    , ?
                                    , ?
                                    , ?
                                    )
                                ",
                                r.id,
                                r.agency_id,
                                r.short_name,
                                r.long_name,
                                url,
                                r.desc,
                                route_type,
                                r.color,
                                r.text_color,
                            )
                            .execute(&mut *tx)
                            .await
                            .map_err(|e| {
                                anyhow::anyhow!(e).context("Failed to insert into gtfs_routes")
                            })
                        }
                        BulkInsert::Shape(s) => sqlx::query!(
                            "
                            INSERT INTO
                            gtfs_shapes
                                ( shape_id
                                , shape_pt_lat
                                , shape_pt_lon
                                , shape_pt_sequence
                                , shape_dist_traveled
                                ) VALUES
                                ( ?
                                , ?
                                , ?
                                , ?
                                , ?
                                )
                            ",
                            s.id,
                            s.latitude,
                            s.longitude,
                            s.sequence,
                            s.distance,
                        )
                        .execute(&mut *tx)
                        .await
                        .map_err(|e| {
                            anyhow::anyhow!(e).context("Failed to insert into gtfs_shapes")
                        }),
                        BulkInsert::Stop(s) => {
                            let location_type = s.location_type.map(|l| l as i32);
                            let wheelchair_boarding = s.wheelchair_boarding as i32;
                            let url = s.url.map(|u| u.to_string());
                            sqlx::query!(
                                "
                                INSERT INTO
                                gtfs_stops
                                    ( stop_id
                                    , stop_code
                                    , stop_name
                                    , tts_stop_name
                                    , latitude
                                    , longitude
                                    , zone_id
                                    , stop_url
                                    , location_type
                                    , parent_station
                                    , stop_timezone
                                    , wheelchair_boarding
                                    , level_id
                                    , platform_code
                                    )
                                VALUES
                                    ( ?
                                    , ?
                                    , ?
                                    , ?
                                    , ?
                                    , ?
                                    , ?
                                    , ?
                                    , ?
                                    , ?
                                    , ?
                                    , ?
                                    , ?
                                    , ?
                                    )
                                ",
                                s.id,
                                s.code,
                                s.name,
                                s.tts_name,
                                s.latitude,
                                s.longitude,
                                s.zone_id,
                                url,
                                location_type,
                                s.parent_station,
                                s.timezone,
                                wheelchair_boarding,
                                s.level_id,
                                s.platform_code,
                            )
                            .execute(&mut *tx)
                            .await
                            .map_err(|e| {
                                anyhow::anyhow!(e).context("Failed to insert into gtfs_stops")
                            })
                        }
                        BulkInsert::Trip(t) => {
                            let direction_id = t.direction_id.map(|d| d as i32);
                            let wheelchair_boarding = t.wheelchair_boarding as i32;
                            let bikes_allowed = t.bikes_allowed as i32;
                            let key = trip_key(&t.id);
                            sqlx::query!(
                                "
                                INSERT INTO
                                gtfs_trips
                                    ( trip_id
                                    , route_id
                                    , service_id
                                    , trip_headsign
                                    , trip_short_name
                                    , direction_id
                                    , block_id
                                    , shape_id
                                    , wheelchair_boarding
                                    , bikes_allowed
                                    , trip_key
                                    )
                                VALUES
                                    ( ?
                                    , ?
                                    , ?
                                    , ?
                                    , ?
                                    , ?
                                    , ?
                                    , ?
                                    , ?
                                    , ?
                                    , ?
                                    )
                                ",
                                t.id,
                                t.route_id,
                                t.service_id,
                                t.headsign,
                                t.short_name,
                                direction_id,
                                t.block_id,
                                t.shape_id,
                                wheelchair_boarding,
                                bikes_allowed,
                                key,
                            )
                            .execute(&mut *tx)
                            .await
                            .map_err(|e| {
                                anyhow::anyhow!(e).context("Failed to insert into gtfs_trips")
                            })
                        }
                        BulkInsert::StopTime(st) => {
                            let key = trip_key(&st.trip_id);
                            sqlx::query!(
                                "
                            INSERT INTO
                            gtfs_stop_times
                                ( trip_id
                                , stop_id
                                , stop_sequence
                                , arrival_time
                                , departure_time
                                , trip_key
                                )
                            VALUES
                                ( ?
                                , ?
                                , ?
                                , ?
                                , ?
                                , ?
                                )
                            ",
                                st.trip_id,
                                st.stop_id,
                                st.stop_sequence,
                                st.arrival_time,
                                st.departure_time,
                                key,
                            )
                            .execute(&mut *tx)
                            .await
                            .map_err(|e| {
                                anyhow::anyhow!(e).context("Failed to insert into gtfs_stop_times")
                            })
                        }
                    };
                    if let Err(e) = res {
                        warn!(error = ?e, "Failed to execute query");
                        panic!("{}", e);
                    }
                    if i % LOG_EVERY_I == 0 {
                        let took = last_checkpoint.elapsed();
                        #[allow(clippy::cast_precision_loss)]
                        let per_sec = LOG_EVERY_I as f64 / took.as_secs_f64();
                        trace!(?i, ?took, ?per_sec, "processed part of the feed");
                        last_checkpoint = Instant::now();
                    }
                    i += 1;
                }
                {
                    let took = start.elapsed();
                    #[allow(clippy::cast_precision_loss)]
                    let per_sec = i as f64 / start.elapsed().as_secs_f64();
                    debug!(
                        count = i,
                        ?took,
                        ?per_sec,
                        "Sent all queries, committing transaction"
                    );
                }
                let start = Instant::now();
                tx.commit().await?;
                debug!(took = ?start.elapsed(), "Transaction committed");

                debug!("Vacuuming database");
                let start = Instant::now();
                if let Err(e) = sqlx::query!("VACUUM").execute(&Database::pool()).await {
                    warn!(?e, "Failed to vacuum database");
                } else {
                    debug!(took = ?start.elapsed(), "Database vacuumed");
                }
                Ok::<_, FileDataError>(())
            }
            .instrument(tracing::Span::current()),
        );

        let res = tokio::task::spawn_blocking(move || {
            debug!("Starting csv decoding");

            let start_task = Instant::now();
            let mut zip = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes))
                .map_err(FileDataError::Zip)?;
            trace!(took = ?start_task.elapsed(), "Zip created");

            {
                let start = Instant::now();
                Route::read_from_zip_notif(&mut zip, &query_tx)?;
                trace!(took = ?start.elapsed(), "Routes updated");
            }

            {
                let start = Instant::now();
                Shape::read_from_zip_notif(&mut zip, &query_tx)?;
                trace!(took = ?start.elapsed(), "Shapes updated");
            }

            {
                let start = Instant::now();
                Stop::read_from_zip_notif(&mut zip, &query_tx)?;
                trace!(took = ?start.elapsed(), "Stops updated");
            }

            {
                let start = Instant::now();
                Trip::read_from_zip_notif(&mut zip, &query_tx)?;
                trace!(took = ?start.elapsed(), "Trips updated");
            }

            {
                let start = Instant::now();
                StopTime::read_from_zip_notif(&mut zip, &query_tx)?;
                trace!(took = ?start.elapsed(), "Stop times updated");
            }

            drop(query_tx);

            debug!(took = ?start_task.elapsed(), "CSV data read");

            Ok::<_, FileDataError>(())
        });

        let (parsers, queries) = tokio::join!(res, queries_fut);

        parsers??;
        queries??;

        debug!("Database update complete");

        Ok(())
    }
}

pub enum BulkInsert {
    DeleteAll(&'static str),
    Route(Route),
    Shape(Shape),
    Stop(Stop),
    Trip(Trip),
    StopTime(StopTime),
}

pub trait FileData: Sized + DeserializeOwned {
    fn file_name() -> &'static str;

    fn table_name() -> &'static str;

    fn into_bulk_insert(self) -> BulkInsert;

    fn read_from_zip_notif(
        zip: &mut zip::ZipArchive<std::io::Cursor<prost::bytes::Bytes>>,
        tx: &tokio::sync::mpsc::UnboundedSender<BulkInsert>,
    ) -> Result<(), FileDataError> {
        let zip_file = zip.by_name(Self::file_name()).map_err(FileDataError::Zip)?;

        trace!(file = ?zip_file.name(), "Reading file");

        let its = csv::ReaderBuilder::new()
            .from_reader(zip_file)
            .into_deserialize::<Self>()
            .filter_map(std::result::Result::ok);

        let mut i = 0;
        let _ = tx.send(BulkInsert::DeleteAll(Self::table_name()));
        for it in its {
            let _ = tx.send(it.into_bulk_insert());
            i += 1;
        }
        debug!(count = i, table = ?Self::table_name(), "Parsed rows and sent queries");

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FileDataError {
    #[error("Failed to read from zip: {0:?}")]
    Zip(#[from] zip::result::ZipError),
    #[error("Failed to parse: {0:?}")]
    Parse(#[from] csv::Error),
    #[error("Failed to join blocking task: {0:?}")]
    JoinBlocking(#[from] tokio::task::JoinError),
    #[error("Failed to execute query: {0:?}")]
    DatabaseInsert(#[from] sqlx::Error),
}

/// Normalize a GTFS `trip_id` into a stable key by dropping its service-id
/// segment (the 2nd `_`-separated component).
///
/// ZET's GTFS-RT feed publishes every trip under the synthetic service id
/// `0_40`, which does not exist in the static schedule (whose service ids are
/// `0_4`..`0_14`). Exact `trip_id` equality joins between realtime and static
/// data therefore never match. The real trip identity is carried by the other
/// segments, so this strips the service-id segment and yields a key that is
/// identical on both sides:
///
/// | `trip_id`               | `trip_key`          |
/// |-------------------------|---------------------|
/// | `0_40_20601_206_10157`  | `0_20601_206_10157` |
/// | `0_4_26820_268_10003`   | `0_26820_268_10003` |
///
/// Inputs with fewer than two `_`-separated segments (no service-id component
/// to strip) are returned unchanged.
///
/// The SQL counterpart used to backfill existing rows lives in migration
/// `20260803110000_add_trip_key.up.sql` and must stay in sync with this.
pub fn trip_key(trip_id: &str) -> String {
    let Some(first_us) = trip_id.find('_') else {
        return trip_id.to_string();
    };
    let rest = &trip_id[first_us + 1..];
    let Some(second_us_rel) = rest.find('_') else {
        return trip_id.to_string();
    };
    let prefix = &trip_id[..first_us];
    let suffix = &rest[second_us_rel + 1..];
    let mut out = String::with_capacity(prefix.len() + 1 + suffix.len());
    out.push_str(prefix);
    out.push('_');
    out.push_str(suffix);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_service_segment() {
        assert_eq!(trip_key("0_40_20601_206_10157"), "0_20601_206_10157");
        assert_eq!(trip_key("0_4_26820_268_10003"), "0_26820_268_10003");
        assert_eq!(trip_key("0_10_10101_101_20001"), "0_10101_101_20001");
    }

    #[test]
    fn realtime_and_static_collide() {
        assert_eq!(
            trip_key("0_40_20601_206_10157"),
            trip_key("0_4_20601_206_10157"),
        );
        assert_eq!(
            trip_key("0_40_20601_206_10157"),
            trip_key("0_13_20601_206_10157"),
        );
    }

    #[test]
    fn fewer_segments_unchanged() {
        assert_eq!(trip_key("weird"), "weird");
        assert_eq!(trip_key("a_b"), "a_b");
        assert_eq!(trip_key(""), "");
    }
}
