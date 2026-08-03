-- Reverses 20260803165840_index_tuning.

DROP INDEX idx_feedback__user_id_created_at;

CREATE INDEX idx_gtfs_trips__trip_id
    ON gtfs_trips (trip_id);

CREATE INDEX idx_gtfs_stop_times__trip_key
    ON gtfs_stop_times (trip_key);
DROP INDEX idx_gtfs_stop_times__trip_key_inc;
