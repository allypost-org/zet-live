DROP TABLE IF EXISTS oauth_states;
DROP TABLE IF EXISTS user_settings;
DROP TABLE IF EXISTS user_sessions;
DROP TABLE IF EXISTS user_oauth_identities;
DROP TABLE IF EXISTS users;
DROP TABLE IF EXISTS link_tickets;
DROP TABLE IF EXISTS auth_providers;
DROP TABLE IF EXISTS pending_transfers;
DROP TABLE IF EXISTS user_notices;
DROP TABLE IF EXISTS data_deletion_requests;

ALTER TABLE feedback DROP COLUMN user_id;
ALTER TABLE feedback DROP COLUMN reply;
ALTER TABLE feedback DROP COLUMN replied_at;
ALTER TABLE feedback DROP COLUMN dismissed;
