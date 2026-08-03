DROP INDEX idx_live_vehicles__trip_key;
DROP INDEX idx_live_trips__trip_key;
DROP INDEX idx_gtfs_stop_times__trip_key;
DROP INDEX idx_gtfs_trips__trip_key;

ALTER TABLE live_vehicles   DROP COLUMN trip_key;
ALTER TABLE live_trips      DROP COLUMN trip_key;
ALTER TABLE gtfs_stop_times DROP COLUMN trip_key;
ALTER TABLE gtfs_trips      DROP COLUMN trip_key;
