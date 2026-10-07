CREATE INDEX idx_gtfs_stop_times__stop_id__departure_time_seconds
  ON gtfs_stop_times(stop_id, departure_time_seconds) INCLUDE (trip_id, stop_headsign);

DROP INDEX idx_live_trips__trip_id;
DROP INDEX idx_live_vehicles__route_id;

CREATE INDEX idx_oauth_states__user_id ON oauth_states(user_id);
CREATE INDEX idx_link_tickets__user_id ON link_tickets(user_id);
CREATE INDEX idx_pending_transfers__source_user_id ON pending_transfers(source_user_id);
CREATE INDEX idx_pending_transfers__target_user_id ON pending_transfers(target_user_id);
CREATE INDEX idx_feature_flag_scoped_users__user_id ON feature_flag_scoped_users(user_id);

CREATE INDEX idx_users__created_at__id ON users(created_at DESC, id DESC);
CREATE INDEX idx_user_sessions__created_at__id ON user_sessions(created_at DESC, id DESC);
CREATE INDEX idx_user_notices__created_at__id ON user_notices(created_at DESC, id DESC);
CREATE INDEX idx_feedback__created_at__id ON feedback(created_at DESC, id DESC);
CREATE INDEX idx_feedback__pending__created_at__id
  ON feedback(created_at DESC, id DESC)
  WHERE NOT handled AND NOT dismissed AND reply IS NULL;
