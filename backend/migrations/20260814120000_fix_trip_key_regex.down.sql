-- Restore the original (broken) generated-column expressions from the init
-- migration `20260803114531_init.up.sql`. Reverting reintroduces the
-- live↔static join miss for multi-character service-id segments; this exists
-- only to satisfy reversibility.

ALTER TABLE gtfs_trips DROP COLUMN trip_key;
ALTER TABLE gtfs_trips ADD COLUMN trip_key TEXT GENERATED ALWAYS AS (
    CASE
      WHEN trip_id ~ '^[^_]+_[^_]_'
      THEN split_part(trip_id, '_', 1) || '_' || regexp_replace(trip_id, '^[^_]+_[^_]+_', '')
      ELSE trip_id
    END
  ) STORED;
CREATE INDEX idx_gtfs_trips__trip_key ON gtfs_trips(trip_key);

ALTER TABLE gtfs_stop_times DROP COLUMN trip_key;
ALTER TABLE gtfs_stop_times ADD COLUMN trip_key TEXT GENERATED ALWAYS AS (
    CASE
      WHEN trip_id ~ '^[^_]+_[^_]_'
      THEN split_part(trip_id, '_', 1) || '_' || regexp_replace(trip_id, '^[^_]+_[^_]+_', '')
      ELSE trip_id
    END
  ) STORED;
CREATE INDEX idx_gtfs_stop_times__trip_key_inc
  ON gtfs_stop_times(trip_key) INCLUDE (stop_id, stop_sequence, arrival_time_seconds);
