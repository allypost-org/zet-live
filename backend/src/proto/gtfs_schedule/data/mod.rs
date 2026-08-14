use std::{io::Read, time::Instant};

use sqlx::AssertSqlSafe;
use tracing::{debug, trace, warn};

use crate::database::Database;

pub mod route;
pub mod shape;
pub mod stop;
pub mod trip;

pub use route::*;
pub use shape::*;
pub use stop::*;
pub use trip::*;

#[derive(Debug)]
pub struct GtfsSchedule;
impl GtfsSchedule {
    #[allow(clippy::too_many_lines)]
    pub async fn read_from_zip_bytes(zip_bytes: prost::bytes::Bytes) -> Result<(), FileDataError> {
        debug!("Reading GTFS schedule from zip bytes");

        let mut tx = Database::pool().begin().await?;
        let start = Instant::now();

        copy_csv(
            &mut tx,
            &zip_bytes,
            FileSpec {
                file: "routes.txt",
                table: "gtfs_routes",
                rebuild_indexes_around_copy: false,
                columns: &[
                    ("route_id", "route_id"),
                    ("agency_id", "agency_id"),
                    ("route_short_name", "route_short_name"),
                    ("route_long_name", "route_long_name"),
                    ("route_desc", "route_desc"),
                    ("route_type", "route_type"),
                    ("route_url", "route_url"),
                    ("route_color", "route_color"),
                    ("route_text_color", "route_text_color"),
                ],
            },
        )
        .await?;

        copy_csv(
            &mut tx,
            &zip_bytes,
            FileSpec {
                file: "shapes.txt",
                table: "gtfs_shapes",
                rebuild_indexes_around_copy: false,
                columns: &[
                    ("shape_id", "shape_id"),
                    ("shape_pt_lat", "shape_pt_lat"),
                    ("shape_pt_lon", "shape_pt_lon"),
                    ("shape_pt_sequence", "shape_pt_sequence"),
                    ("shape_dist_traveled", "shape_dist_traveled"),
                ],
            },
        )
        .await?;

        copy_csv(
            &mut tx,
            &zip_bytes,
            FileSpec {
                file: "stops.txt",
                table: "gtfs_stops",
                rebuild_indexes_around_copy: false,
                columns: &[
                    ("stop_id", "stop_id"),
                    ("stop_code", "stop_code"),
                    ("stop_name", "stop_name"),
                    ("stop_desc", "stop_desc"),
                    ("stop_lat", "latitude"),
                    ("stop_lon", "longitude"),
                    ("zone_id", "zone_id"),
                    ("stop_url", "stop_url"),
                    ("location_type", "location_type"),
                    ("parent_station", "parent_station"),
                ],
            },
        )
        .await?;

        copy_csv(
            &mut tx,
            &zip_bytes,
            FileSpec {
                file: "trips.txt",
                table: "gtfs_trips",
                rebuild_indexes_around_copy: false,
                columns: &[
                    ("route_id", "route_id"),
                    ("service_id", "service_id"),
                    ("trip_id", "trip_id"),
                    ("trip_headsign", "trip_headsign"),
                    ("trip_short_name", "trip_short_name"),
                    ("direction_id", "direction_id"),
                    ("block_id", "block_id"),
                    ("shape_id", "shape_id"),
                ],
            },
        )
        .await?;

        copy_csv(
            &mut tx,
            &zip_bytes,
            FileSpec {
                file: "stop_times.txt",
                table: "gtfs_stop_times",
                rebuild_indexes_around_copy: true,
                columns: &[
                    ("trip_id", "trip_id"),
                    ("arrival_time", "arrival_time"),
                    ("departure_time", "departure_time"),
                    ("stop_id", "stop_id"),
                    ("stop_sequence", "stop_sequence"),
                    ("stop_headsign", "stop_headsign"),
                    ("pickup_type", "pickup_type"),
                    ("drop_off_type", "drop_off_type"),
                    ("shape_dist_traveled", "shape_dist_traveled"),
                ],
            },
        )
        .await?;

        tx.commit().await?;
        debug!(took = ?start.elapsed(), "Schedule loaded via COPY, analyzing");

        let analyze_start = Instant::now();
        if let Err(e) = sqlx::query!("ANALYZE").execute(&Database::pool()).await {
            warn!(?e, "Failed to analyze database");
        } else {
            debug!(took = ?analyze_start.elapsed(), "Database analyzed");
        }

        debug!("Database update complete");
        Ok(())
    }
}

struct FileSpec {
    file: &'static str,
    table: &'static str,
    /// Drop non-PK/non-unique indexes before COPY and recreate them after.
    /// Bulk `CREATE INDEX` is far cheaper than per-row maintenance during COPY
    /// — worth it on large tables (~1.7M-row `gtfs_stop_times` measured
    /// 15.7 s → 6.2 s), negligible benefit on small ones.
    rebuild_indexes_around_copy: bool,
    /// `(csv_column, table_column)` pairs covering every column the CSV may
    /// contain. Order is irrelevant — the CSV header drives the mapping.
    columns: &'static [(&'static str, &'static str)],
}

async fn copy_csv(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    zip_bytes: &prost::bytes::Bytes,
    spec: FileSpec,
) -> Result<(), FileDataError> {
    let file_start = Instant::now();

    let index_defs = if spec.rebuild_indexes_around_copy {
        Some(capture_and_drop_indexes(tx, spec.table).await?)
    } else {
        None
    };

    let zip_bytes = zip_bytes.clone();
    let file = spec.file.to_string();
    let (csv_header, data) = tokio::task::spawn_blocking(move || -> Result<_, FileDataError> {
        let mut zip =
            zip::ZipArchive::new(std::io::Cursor::new(zip_bytes)).map_err(FileDataError::Zip)?;
        let mut entry = zip.by_name(&file).map_err(FileDataError::Zip)?;
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf)?;
        drop(entry);

        let nl = buf
            .iter()
            .position(|&b| b == b'\n')
            .ok_or_else(|| FileDataError::ColumnMapping("no header line".into()))?;
        let header = std::str::from_utf8(&buf[..nl])
            .map_err(|e| FileDataError::ColumnMapping(format!("non-utf8 header: {e}")))?
            .to_string();
        let data = buf[nl + 1..].to_vec();
        Ok((header, data))
    })
    .await??;

    let cols_csv = csv_header
        .split(',')
        .map(|h| h.trim().trim_matches('\u{feff}'))
        .map(|csv_col| {
            spec.columns
                .iter()
                .find(|(c, _)| *c == csv_col)
                .map(|(_, t)| *t)
                .ok_or_else(|| FileDataError::ColumnMapping(format!("unmapped column: {csv_col}")))
        })
        .try_fold(String::new(), |mut acc, col| {
            match col {
                Ok(col) => {
                    acc.push_str(col);
                    acc.push_str(", ");
                }
                Err(e) => {
                    return Err(e);
                }
            }

            Ok(acc)
        })?;

    let cols_csv = cols_csv.trim_end_matches(", ");

    sqlx::query(AssertSqlSafe(format!("DELETE FROM {}", spec.table)))
        .execute(&mut **tx)
        .await?;

    let copy_sql = format!(
        "COPY {} ({}) FROM STDIN WITH (FORMAT csv)",
        spec.table, cols_csv
    );
    let mut copy = tx.copy_in_raw(&copy_sql).await?;
    copy.send(data).await?;
    let rows = copy.finish().await?;

    if let Some(defs) = &index_defs {
        let rebuild_start = Instant::now();
        for def in defs {
            sqlx::query(AssertSqlSafe(def.clone()))
                .execute(&mut **tx)
                .await?;
        }
        trace!(
            table = spec.table,
            index_count = defs.len(),
            took = ?rebuild_start.elapsed(),
            "indexes rebuilt after COPY"
        );
    }

    trace!(table = spec.table, rows, took = ?file_start.elapsed(), "COPY loaded");
    Ok(())
}

async fn capture_and_drop_indexes(
    tx: &mut sqlx::PgConnection,
    table: &str,
) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query!(
        "SELECT c.relname, i.indexdef
         FROM pg_indexes i
         JOIN pg_class c ON c.relname = i.indexname
         JOIN pg_index x ON x.indexrelid = c.oid
         WHERE i.schemaname = 'public'
           AND i.tablename = $1
           AND NOT x.indisprimary
           AND NOT x.indisunique",
        table,
    )
    .fetch_all(&mut *tx)
    .await?;

    let mut defs = Vec::with_capacity(rows.len());
    for row in rows {
        let name = row.relname;
        let def = row.indexdef.ok_or(sqlx::Error::RowNotFound)?;
        sqlx::query(AssertSqlSafe(format!("DROP INDEX {name}")))
            .execute(&mut *tx)
            .await?;
        defs.push(def);
    }
    Ok(defs)
}

#[derive(Debug, thiserror::Error)]
pub enum FileDataError {
    #[error("Failed to read from zip: {0:?}")]
    Zip(#[from] zip::result::ZipError),
    #[error("Failed to read: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to parse: {0:?}")]
    Parse(#[from] csv::Error),
    #[error("Failed to join blocking task: {0:?}")]
    JoinBlocking(#[from] tokio::task::JoinError),
    #[error("Failed to execute query: {0:?}")]
    DatabaseInsert(#[from] sqlx::Error),
    #[error("Column mapping failed: {0}")]
    ColumnMapping(String),
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
/// The static schedule tables compute this via a GENERATED column (migration
/// `20260814120000_fix_trip_key_regex.up.sql`); this function remains the
/// source of truth for the realtime tables (`live_trips`, `live_vehicles`)
/// which are still populated per-row from the GTFS-RT feed. The SQL and Rust
/// implementations MUST stay in sync — the guard regex strips the service-id
/// segment regardless of its length.
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
