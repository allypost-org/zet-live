-- Switch trip_key from a Rust-computed column to a GENERATED column so the
-- schedule loader can use COPY FROM (raw CSV bytes) instead of per-row INSERTs.
-- Also adds gtfs_stops.stop_desc so stops.txt can be COPY'd without a column
-- mismatch (the CSV has stop_desc; the table didn't).

-- The expression mirrors Rust's `trip_key()` in proto/gtfs_schedule/data/mod.rs:
-- drop the 2nd `_`-separated segment (the ZET service id), keep the rest.
-- CASE handles trip_ids with fewer than 2 `_` separators (returned unchanged).
-- Uses split_part + regexp_replace (instr() is not available on all PG builds).

-- gtfs_stop_times
DROP INDEX IF EXISTS idx_gtfs_stop_times__trip_key_inc;
ALTER TABLE gtfs_stop_times DROP COLUMN IF EXISTS trip_key;
ALTER TABLE gtfs_stop_times ADD COLUMN trip_key TEXT GENERATED ALWAYS AS (
    CASE
        WHEN trip_id ~ '^[^_]+_[^_]_'
        THEN split_part(trip_id, '_', 1) || '_' || regexp_replace(trip_id, '^[^_]+_[^_]+_', '')
        ELSE trip_id
    END
) STORED;
CREATE INDEX idx_gtfs_stop_times__trip_key_inc
    ON gtfs_stop_times (trip_key) INCLUDE (stop_id, stop_sequence, arrival_time_seconds);

-- gtfs_trips
DROP INDEX IF EXISTS idx_gtfs_trips__trip_key;
ALTER TABLE gtfs_trips DROP COLUMN IF EXISTS trip_key;
ALTER TABLE gtfs_trips ADD COLUMN trip_key TEXT GENERATED ALWAYS AS (
    CASE
        WHEN trip_id ~ '^[^_]+_[^_]_'
        THEN split_part(trip_id, '_', 1) || '_' || regexp_replace(trip_id, '^[^_]+_[^_]+_', '')
        ELSE trip_id
    END
) STORED;
CREATE INDEX idx_gtfs_trips__trip_key ON gtfs_trips (trip_key);

-- gtfs_stops: add stop_desc so the CSV column maps 1:1 during COPY
ALTER TABLE gtfs_stops ADD COLUMN IF NOT EXISTS stop_desc TEXT;
