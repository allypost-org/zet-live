CREATE TABLE feature_flags (
  id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  key         TEXT NOT NULL UNIQUE,
  description TEXT NOT NULL DEFAULT '',
  state       TEXT NOT NULL DEFAULT 'disabled'
              CHECK (state IN ('disabled', 'enabled', 'logged_in', 'scoped')),
  updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE feature_flag_scoped_users (
  flag_id  BIGINT NOT NULL REFERENCES feature_flags(id) ON DELETE CASCADE,
  user_id  TEXT   NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  added_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (flag_id, user_id)
);
