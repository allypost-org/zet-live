-- Fix the `trip_key` generated columns to match the canonical Rust
-- `trip_key()` (`backend/src/proto/gtfs_schedule/data/mod.rs`).
--
-- The init migration's guard regex `^[^_]+_[^_]_` only strips the service-id
-- segment when it is exactly ONE character long, while the Rust function
-- strips it regardless of length (and the `regexp_replace` pattern right next
-- to the guard — `^[^_]+_[^_]+_` — already assumed any length).
--
-- With ZET's current service ids (`0_40`..`0_50`, all two chars) NO static row
-- was ever normalized: `gtfs_trips.trip_key = trip_id` for all rows, while the
-- realtime side stored Rust-normalized keys. Every live↔static `trip_key` join
-- missed, so e.g. `/api/v1/schedule/trip-info/*` always fell back to the
-- live-only "partial" response.
--
-- Verified against live data: with the fixed expression 801/801 live trips
-- match a static trip (0/801 before).
--
-- Generated-column expressions cannot be altered in place on all supported PG
-- versions, so drop + re-add (one table rewrite) and recreate the dependent
-- indexes (they are dropped with the column).

ALTER TABLE gtfs_trips DROP COLUMN trip_key;
ALTER TABLE gtfs_trips ADD COLUMN trip_key TEXT GENERATED ALWAYS AS (
    CASE
      WHEN trip_id ~ '^[^_]+_[^_]+_'
      THEN split_part(trip_id, '_', 1) || '_' || regexp_replace(trip_id, '^[^_]+_[^_]+_', '')
      ELSE trip_id
    END
  ) STORED;
CREATE INDEX idx_gtfs_trips__trip_key ON gtfs_trips(trip_key);

ALTER TABLE gtfs_stop_times DROP COLUMN trip_key;
ALTER TABLE gtfs_stop_times ADD COLUMN trip_key TEXT GENERATED ALWAYS AS (
    CASE
      WHEN trip_id ~ '^[^_]+_[^_]+_'
      THEN split_part(trip_id, '_', 1) || '_' || regexp_replace(trip_id, '^[^_]+_[^_]+_', '')
      ELSE trip_id
    END
  ) STORED;
CREATE INDEX idx_gtfs_stop_times__trip_key_inc
  ON gtfs_stop_times(trip_key) INCLUDE (stop_id, stop_sequence, arrival_time_seconds);
