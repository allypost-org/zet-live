-- Add a normalized `trip_key` column to trip-bearing tables.
--
-- ZET's GTFS-RT feed publishes every trip under a synthetic service id `0_40`
-- that does not exist in the static schedule (whose service ids are
-- `0_4`..`0_14`). Exact `trip_id` equality joins between realtime and static
-- data therefore never match, leaving `SimpleStops` (and stop-arrival features)
-- permanently empty.
--
-- `trip_key` drops the service-id segment (the 2nd `_`-separated component),
-- yielding a stable join key that is identical on both sides, e.g. both
-- `0_40_20601_206_10157` and `0_4_20601_206_10157` collapse to
-- `0_20601_206_10157`.
--
-- The canonical implementation lives in `backend/src/gtfs.rs::trip_key`; the
-- backfill UPDATE below mirrors it in SQL so existing rows are populated
-- without requiring a static re-sync.

ALTER TABLE gtfs_trips      ADD COLUMN trip_key TEXT;
ALTER TABLE gtfs_stop_times ADD COLUMN trip_key TEXT;
ALTER TABLE live_trips      ADD COLUMN trip_key TEXT;
ALTER TABLE live_vehicles   ADD COLUMN trip_key TEXT;

UPDATE gtfs_trips
SET trip_key = substr(trip_id, 1, instr(trip_id, '_'))
            || substr(trip_id, instr(trip_id, '_') + instr(substr(trip_id, instr(trip_id, '_') + 1), '_') + 1)
WHERE trip_key IS NULL;
UPDATE gtfs_stop_times
SET trip_key = substr(trip_id, 1, instr(trip_id, '_'))
            || substr(trip_id, instr(trip_id, '_') + instr(substr(trip_id, instr(trip_id, '_') + 1), '_') + 1)
WHERE trip_key IS NULL;
-- live_trips / live_vehicles are ephemeral (rewritten every realtime cycle),
-- so they are populated by the server on the next feed update, not here.

CREATE INDEX idx_gtfs_trips__trip_key      ON gtfs_trips(trip_key);
CREATE INDEX idx_gtfs_stop_times__trip_key ON gtfs_stop_times(trip_key);
CREATE INDEX idx_live_trips__trip_key      ON live_trips(trip_key);
CREATE INDEX idx_live_vehicles__trip_key   ON live_vehicles(trip_key);
