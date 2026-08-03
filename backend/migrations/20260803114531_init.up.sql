-- ZET Live schema for PostgreSQL.
-- Mirrors the SQLite schema's final form with idiomatic PG types; see
-- docs/superpowers/specs/2026-08-03-move-to-postgres-design.md §3.

-- Reference data enums (values inserted by the GTFS loader, not auto-generated)
CREATE TABLE gtfs_location_types (
  location_type SMALLINT PRIMARY KEY,
  description   TEXT
);
CREATE TABLE gtfs_route_types (
  route_type   SMALLINT PRIMARY KEY,
  description  TEXT
);
CREATE TABLE gtfs_directions (
  direction_id SMALLINT PRIMARY KEY,
  description  TEXT
);
CREATE TABLE gtfs_pickup_dropoff_types (
  type_id      SMALLINT PRIMARY KEY,
  description  TEXT
);
CREATE TABLE gtfs_transfer_types (
  transfer_type SMALLINT PRIMARY KEY,
  description   TEXT
);
CREATE TABLE gtfs_payment_methods (
  payment_method SMALLINT PRIMARY KEY,
  description    TEXT
);

CREATE TABLE gtfs_schedule_meta (
  etag          TEXT,
  last_modified TIMESTAMPTZ,
  fetched_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_gtfs_schedule_meta__etag          ON gtfs_schedule_meta(etag);
CREATE INDEX idx_gtfs_schedule_meta__last_modified ON gtfs_schedule_meta(last_modified);

CREATE TABLE gtfs_agency (
  agency_id       TEXT PRIMARY KEY,
  agency_name     TEXT NOT NULL,
  agency_url      TEXT NOT NULL,
  agency_timezone TEXT NOT NULL,
  agency_lang     TEXT,
  agency_phone    TEXT,
  fare_url        TEXT
);

CREATE TABLE gtfs_stops (
  stop_id            TEXT PRIMARY KEY,
  stop_code          TEXT,
  stop_name          TEXT,
  tts_stop_name      TEXT,
  latitude           DOUBLE PRECISION,
  longitude          DOUBLE PRECISION,
  zone_id            TEXT,
  stop_url           TEXT,
  location_type      SMALLINT,
  parent_station     TEXT,
  stop_timezone      TEXT,
  wheelchair_boarding SMALLINT,
  level_id           TEXT,
  platform_code      TEXT
);

CREATE TABLE gtfs_routes (
  route_id         TEXT PRIMARY KEY NOT NULL,
  agency_id        TEXT,
  route_short_name TEXT DEFAULT '',
  route_long_name  TEXT DEFAULT '',
  route_desc       TEXT,
  route_type       SMALLINT,
  route_url        TEXT,
  route_color      TEXT,
  route_text_color TEXT
);

CREATE TABLE gtfs_calendar (
  service_id TEXT PRIMARY KEY,
  monday     SMALLINT NOT NULL,
  tuesday    SMALLINT NOT NULL,
  wednesday  SMALLINT NOT NULL,
  thursday   SMALLINT NOT NULL,
  friday     SMALLINT NOT NULL,
  saturday   SMALLINT NOT NULL,
  sunday     SMALLINT NOT NULL,
  start_date TEXT NOT NULL,
  end_date   TEXT NOT NULL
);

CREATE TABLE gtfs_calendar_dates (
  service_id    TEXT NOT NULL,
  date          TEXT NOT NULL,
  exception_type SMALLINT NOT NULL
);
CREATE INDEX idx_gtfs_calendar_dates__service_id ON gtfs_calendar_dates(service_id);

CREATE TABLE service_combo_ids (
  combination_id INTEGER PRIMARY KEY
);
CREATE TABLE service_combinations (
  combination_id INTEGER,
  service_id     TEXT
);
CREATE INDEX idx_service_combinations__combination_id ON service_combinations(combination_id);

CREATE TABLE gtfs_fare_attributes (
  fare_id          TEXT PRIMARY KEY,
  price            NUMERIC(19, 4) NOT NULL,
  currency_type    TEXT NOT NULL,
  payment_method   SMALLINT,
  transfers        SMALLINT,
  transfer_duration INTEGER,
  agency_id        TEXT
);

CREATE TABLE gtfs_fare_rules (
  fare_id        TEXT NOT NULL,
  route_id       TEXT,
  origin_id      INTEGER,
  destination_id INTEGER,
  contains_id    INTEGER,
  service_id     TEXT
);
CREATE INDEX idx_gtfs_fare_rules__fare_id ON gtfs_fare_rules(fare_id);

CREATE TABLE gtfs_shapes (
  shape_id           TEXT NOT NULL,
  shape_pt_lat       DOUBLE PRECISION NOT NULL,
  shape_pt_lon       DOUBLE PRECISION NOT NULL,
  shape_pt_sequence  INTEGER NOT NULL,
  shape_dist_traveled DOUBLE PRECISION
);
CREATE INDEX idx_gtfs_shapes__shape_id__shape_pt_sequence
  ON gtfs_shapes(shape_id, shape_pt_sequence);

CREATE TABLE gtfs_trips (
  trip_id            TEXT PRIMARY KEY,
  route_id           TEXT,
  service_id         TEXT,
  trip_headsign      TEXT,
  trip_short_name    TEXT,
  direction_id       SMALLINT,
  block_id           TEXT,
  shape_id           TEXT,
  wheelchair_boarding SMALLINT,
  bikes_allowed      SMALLINT,
  trip_key           TEXT
);
CREATE INDEX idx_gtfs_trips__route_service ON gtfs_trips(route_id, service_id);
CREATE INDEX idx_gtfs_trips__trip_id       ON gtfs_trips(trip_id);
CREATE INDEX idx_gtfs_trips__trip_key      ON gtfs_trips(trip_key);

CREATE TABLE gtfs_frequencies (
  trip_id             TEXT NOT NULL,
  start_time          TEXT NOT NULL,
  end_time            TEXT NOT NULL,
  headway_secs        INTEGER NOT NULL,
  start_time_seconds  INTEGER,
  end_time_seconds    INTEGER
);
CREATE INDEX idx_gtfs_frequencies__trip_id ON gtfs_frequencies(trip_id);

CREATE TABLE gtfs_transfers (
  from_stop_id    TEXT,
  to_stop_id      TEXT,
  transfer_type   SMALLINT,
  min_transfer_time INTEGER,
  from_route_id   TEXT,
  to_route_id     TEXT,
  service_id      TEXT
);

CREATE TABLE gtfs_feed_info (
  feed_publisher_name TEXT,
  feed_publisher_url  TEXT,
  feed_timezone       TEXT,
  feed_lang           TEXT,
  feed_version        TEXT
);

-- gtfs_stop_times: GTFS time-of-day values are TEXT (may exceed 24:00:00);
-- the *_seconds generated columns are seconds-since-midnight, computed via
-- split_part (immutable). PG >= 12 supports STORED generated columns.
CREATE TABLE gtfs_stop_times (
  trip_id              TEXT NOT NULL,
  arrival_time         TEXT CHECK (arrival_time LIKE '__:__:__'),
  departure_time       TEXT CHECK (departure_time LIKE '__:__:__'),
  stop_id              TEXT NOT NULL,
  stop_sequence        INTEGER NOT NULL,
  stop_headsign        TEXT,
  pickup_type          SMALLINT,
  drop_off_type        SMALLINT,
  shape_dist_traveled  DOUBLE PRECISION,
  arrival_time_seconds INTEGER
    GENERATED ALWAYS AS (
      CASE WHEN arrival_time IS NOT NULL THEN
        split_part(arrival_time, ':', 1)::int * 3600
        + split_part(arrival_time, ':', 2)::int * 60
        + split_part(arrival_time, ':', 3)::int
      END
    ) STORED,
  departure_time_seconds INTEGER
    GENERATED ALWAYS AS (
      CASE WHEN departure_time IS NOT NULL THEN
        split_part(departure_time, ':', 1)::int * 3600
        + split_part(departure_time, ':', 2)::int * 60
        + split_part(departure_time, ':', 3)::int
      END
    ) STORED,
  trip_key TEXT
);
CREATE INDEX idx_gtfs_stop_times__trip_id__stop_sequence
  ON gtfs_stop_times(trip_id, stop_sequence);
CREATE INDEX idx_gtfs_stop_times__stop_id__trip_id
  ON gtfs_stop_times(stop_id, trip_id);
CREATE INDEX idx_gtfs_stop_times__trip_id__stop_id
  ON gtfs_stop_times(trip_id, stop_id);
CREATE INDEX idx_gtfs_stop_times__trip_key ON gtfs_stop_times(trip_key);

-- live_* tables: ephemeral, rewritten every realtime cycle
CREATE TABLE live_trips (
  trip_id TEXT NOT NULL,
  trip_key TEXT
);
CREATE INDEX idx_live_trips__trip_id  ON live_trips(trip_id);
CREATE INDEX idx_live_trips__trip_key ON live_trips(trip_key);

CREATE TABLE live_trip_stop_times (
  trip_id        TEXT NOT NULL,
  stop_id        TEXT NOT NULL,
  stop_sequence  INTEGER NOT NULL,
  arrival_time   INTEGER,
  arrival_delay  INTEGER,
  PRIMARY KEY (trip_id, stop_sequence)
);
CREATE INDEX idx_live_trip_stop_times__trip_id__stop_sequence__arrival_delay
  ON live_trip_stop_times(trip_id, stop_sequence, arrival_delay);

CREATE TABLE live_feed_metadata (
  id           SMALLINT PRIMARY KEY,
  base_midnight BIGINT NOT NULL
);
INSERT INTO live_feed_metadata (id, base_midnight) VALUES (0, 0);

CREATE TABLE live_vehicles (
  vehicle_id            TEXT PRIMARY KEY,
  route_id              TEXT NOT NULL,
  trip_id               TEXT NOT NULL,
  latitude              DOUBLE PRECISION NOT NULL,
  longitude             DOUBLE PRECISION NOT NULL,
  prev_latitude         DOUBLE PRECISION,
  prev_longitude        DOUBLE PRECISION,
  next_stop_id          TEXT,
  next_stop_sequence    INTEGER CHECK (next_stop_sequence IS NULL OR next_stop_sequence >= 0),
  next_stop_arrival_delay INTEGER,
  next_stop_arrival_time  INTEGER,
  bearing               DOUBLE PRECISION,
  route_long_name       TEXT,
  trip_headsign         TEXT,
  trip_key              TEXT
);
CREATE INDEX idx_live_vehicles__trip_id  ON live_vehicles(trip_id);
CREATE INDEX idx_live_vehicles__route_id ON live_vehicles(route_id);
CREATE INDEX idx_live_vehicles__trip_key ON live_vehicles(trip_key);

-- Admin / operator config
CREATE TABLE admin_settings (
  name       TEXT PRIMARY KEY,
  value      JSONB NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL
);
CREATE TABLE admin_metadata (
  name       TEXT PRIMARY KEY,
  value      JSONB NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL
);

-- GBFS (nextbike)
CREATE TABLE gbfs_system_information (
  system_id          TEXT PRIMARY KEY,
  name               TEXT,
  operator           TEXT,
  url                TEXT,
  phone_number       TEXT,
  email              TEXT,
  feed_contact_email TEXT,
  timezone           TEXT,
  language           TEXT,
  license_id         TEXT,
  rental_apps        TEXT
);
CREATE TABLE gbfs_vehicle_types (
  vehicle_type_id TEXT PRIMARY KEY,
  name            TEXT,
  form_factor     TEXT,
  propulsion_type TEXT,
  rider_capacity  INTEGER,
  vehicle_image   TEXT,
  description     TEXT
);
CREATE TABLE gbfs_stations (
  station_id         TEXT PRIMARY KEY,
  name               TEXT,
  short_name         TEXT,
  lat                DOUBLE PRECISION NOT NULL,
  lon                DOUBLE PRECISION NOT NULL,
  region_id          TEXT,
  capacity           INTEGER,
  is_virtual_station BOOLEAN,
  rental_uris        TEXT
);
CREATE INDEX idx_gbfs_stations__region_id ON gbfs_stations(region_id);
CREATE TABLE gbfs_station_status (
  station_id              TEXT PRIMARY KEY,
  num_bikes_available     INTEGER,
  num_docks_available     INTEGER,
  is_installed            BOOLEAN,
  is_renting              BOOLEAN,
  is_returning            BOOLEAN,
  last_reported           BIGINT,
  vehicle_types_available TEXT
);
CREATE TABLE gbfs_regions (
  region_id TEXT PRIMARY KEY,
  name      TEXT
);
CREATE TABLE gbfs_pricing_plans (
  plan_id         TEXT PRIMARY KEY,
  name            TEXT,
  currency        TEXT,
  price           NUMERIC(19, 4),
  is_taxable      BOOLEAN,
  description     TEXT,
  per_min_pricing TEXT
);
CREATE TABLE gbfs_rental_hours (
  id          INTEGER PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
  user_types  TEXT,
  days        TEXT,
  start_time  TEXT,
  end_time    TEXT
);

-- Feedback
CREATE TABLE feedback (
  id          BIGINT PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
  category    TEXT NOT NULL,
  message     TEXT NOT NULL,
  name        TEXT,
  contact     TEXT,
  meta_url    TEXT,
  meta_ua     TEXT,
  meta_lang   TEXT,
  meta_build  TEXT,
  ip          TEXT NOT NULL,
  created_at  TIMESTAMPTZ NOT NULL,
  handled     BOOLEAN NOT NULL DEFAULT FALSE,
  user_id     TEXT,
  reply       TEXT,
  replied_at  TIMESTAMPTZ,
  dismissed   BOOLEAN NOT NULL DEFAULT FALSE
);
CREATE INDEX idx_feedback__created_at ON feedback (created_at DESC);
CREATE INDEX idx_feedback__handled    ON feedback (handled);

-- User accounts / auth
CREATE TABLE users (
  id           TEXT PRIMARY KEY,
  display_name TEXT,
  email        TEXT,
  avatar_url   TEXT,
  created_at   TIMESTAMPTZ NOT NULL,
  updated_at   TIMESTAMPTZ NOT NULL
);
CREATE TABLE user_oauth_identities (
  id                    TEXT PRIMARY KEY,
  user_id               TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  provider              TEXT NOT NULL,
  provider_subject      TEXT NOT NULL,
  provider_email        TEXT,
  provider_display_name TEXT,
  provider_avatar_url   TEXT,
  created_at            TIMESTAMPTZ NOT NULL,
  updated_at            TIMESTAMPTZ NOT NULL,
  UNIQUE (provider, provider_subject)
);
CREATE INDEX idx_user_oauth_identities__user_id ON user_oauth_identities (user_id);
CREATE TABLE user_sessions (
  id         TEXT PRIMARY KEY,
  token_hash BYTEA UNIQUE NOT NULL,
  user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  expires_at TIMESTAMPTZ NOT NULL,
  created_at TIMESTAMPTZ NOT NULL,
  ip         TEXT,
  user_agent TEXT
);
CREATE INDEX idx_user_sessions__user_id    ON user_sessions (user_id);
CREATE INDEX idx_user_sessions__expires_at ON user_sessions (expires_at);
CREATE TABLE user_settings (
  user_id    TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
  settings   JSONB NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL
);
CREATE TABLE oauth_states (
  state         TEXT PRIMARY KEY,
  provider      TEXT NOT NULL,
  pkce_verifier TEXT NOT NULL,
  link          BOOLEAN NOT NULL DEFAULT FALSE,
  origin        TEXT,
  user_id       TEXT REFERENCES users(id) ON DELETE CASCADE,
  created_at    TIMESTAMPTZ NOT NULL,
  expires_at    TIMESTAMPTZ NOT NULL
);
CREATE INDEX idx_oauth_states__expires_at ON oauth_states (expires_at);
CREATE TABLE link_tickets (
  token_hash BYTEA PRIMARY KEY,
  user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  expires_at TIMESTAMPTZ NOT NULL,
  created_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX idx_link_tickets__expires_at ON link_tickets (expires_at);
CREATE TABLE auth_providers (
  id            TEXT PRIMARY KEY,
  client_id     TEXT NOT NULL,
  client_secret TEXT NOT NULL,
  enabled       BOOLEAN NOT NULL DEFAULT TRUE,
  created_at    TIMESTAMPTZ NOT NULL,
  updated_at    TIMESTAMPTZ NOT NULL
);
CREATE TABLE pending_transfers (
  token_hash       BYTEA PRIMARY KEY,
  target_user_id   TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  provider         TEXT NOT NULL,
  provider_subject TEXT NOT NULL,
  source_user_id   TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  expires_at       TIMESTAMPTZ NOT NULL,
  created_at       TIMESTAMPTZ NOT NULL
);
CREATE INDEX idx_pending_transfers__expires_at ON pending_transfers (expires_at);
CREATE TABLE user_notices (
  id         TEXT PRIMARY KEY,
  user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  text       TEXT NOT NULL,
  severity   TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX idx_user_notices__user_id ON user_notices (user_id);
CREATE TABLE data_deletion_requests (
  confirmation_code TEXT PRIMARY KEY,
  provider          TEXT NOT NULL,
  provider_subject  TEXT NOT NULL,
  user_id           TEXT,
  status            TEXT NOT NULL,
  created_at        TIMESTAMPTZ NOT NULL
);
CREATE INDEX idx_data_deletion_requests__created_at
  ON data_deletion_requests (created_at DESC);

-- Restore the SET-NULL FK on feedback.user_id (the only non-CASCADE user FK).
-- Defined via ALTER because `feedback` is created before `users` in this file.
ALTER TABLE feedback ADD CONSTRAINT feedback_user_id_fkey
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE SET NULL;
