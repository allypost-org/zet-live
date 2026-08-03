-- Index tuning based on EXPLAIN ANALYZE against a loaded DB (~1.7M gtfs_stop_times).
-- Plain (non-CONCURRENT) CREATE INDEX to match the init migration's convention and
-- sqlx's default transactional migration runner. Brief lock is acceptable here:
-- migration runs once before the app serves traffic.

-- gtfs_stop_times: replace the bare (trip_key) index with a covering one so the
-- active-stop / live-vehicle queries hit Index Only Scans instead of seq scans.
-- Order matters: create the new index before dropping the old one so the planner
-- always has a usable (trip_key) prefix.
CREATE INDEX idx_gtfs_stop_times__trip_key_inc
    ON gtfs_stop_times (trip_key) INCLUDE (stop_id, stop_sequence, arrival_time_seconds);
DROP INDEX idx_gtfs_stop_times__trip_key;

-- gtfs_trips: idx_gtfs_trips__trip_id duplicates the primary key (gtfs_trips_pkey
-- is already a btree on trip_id). Pure write overhead; drop it.
DROP INDEX idx_gtfs_trips__trip_id;

-- feedback: queries filter by user_id and ORDER BY created_at DESC, but the
-- ON DELETE SET NULL FK does not auto-create an index. Cover the common path.
CREATE INDEX idx_feedback__user_id_created_at
    ON feedback (user_id, created_at DESC);
