-- Reverses 20260803182207: trip_key back to a plain column, drop stop_desc.

DROP INDEX IF EXISTS idx_gtfs_trips__trip_key;
ALTER TABLE gtfs_trips DROP COLUMN IF EXISTS trip_key;
ALTER TABLE gtfs_trips ADD COLUMN trip_key TEXT;
CREATE INDEX idx_gtfs_trips__trip_key ON gtfs_trips (trip_key);

DROP INDEX IF EXISTS idx_gtfs_stop_times__trip_key_inc;
ALTER TABLE gtfs_stop_times DROP COLUMN IF EXISTS trip_key;
ALTER TABLE gtfs_stop_times ADD COLUMN trip_key TEXT;
CREATE INDEX idx_gtfs_stop_times__trip_key_inc
    ON gtfs_stop_times (trip_key) INCLUDE (stop_id, stop_sequence, arrival_time_seconds);

ALTER TABLE gtfs_stops DROP COLUMN IF EXISTS stop_desc;
