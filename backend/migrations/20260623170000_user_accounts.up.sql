CREATE TABLE users (
    id           TEXT PRIMARY KEY,
    display_name TEXT,
    email        TEXT,
    avatar_url   TEXT,
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL
) STRICT;

CREATE TABLE user_oauth_identities (
    id                    TEXT PRIMARY KEY,
    user_id               TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider              TEXT NOT NULL,
    provider_subject      TEXT NOT NULL,
    provider_email        TEXT,
    provider_display_name TEXT,
    provider_avatar_url   TEXT,
    created_at            TEXT NOT NULL,
    updated_at            TEXT NOT NULL,
    UNIQUE (provider, provider_subject)
) STRICT;

CREATE INDEX idx_user_oauth_identities__user_id ON user_oauth_identities (user_id);

CREATE TABLE user_sessions (
    id         TEXT PRIMARY KEY,
    token_hash BLOB UNIQUE NOT NULL,
    user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    ip         TEXT,
    user_agent TEXT
) STRICT;

CREATE INDEX idx_user_sessions__user_id ON user_sessions (user_id);
CREATE INDEX idx_user_sessions__expires_at ON user_sessions (expires_at);

CREATE TABLE user_settings (
    user_id    TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    settings   BLOB NOT NULL,
    updated_at TEXT NOT NULL
) STRICT;

CREATE TABLE oauth_states (
    state         TEXT PRIMARY KEY,
    provider      TEXT NOT NULL,
    pkce_verifier TEXT NOT NULL,
    link          INTEGER NOT NULL DEFAULT 0,
    origin        TEXT,
    user_id       TEXT REFERENCES users(id) ON DELETE CASCADE,
    created_at    TEXT NOT NULL,
    expires_at    TEXT NOT NULL
) STRICT;

CREATE INDEX idx_oauth_states__expires_at ON oauth_states (expires_at);

CREATE TABLE link_tickets (
    token_hash BLOB PRIMARY KEY,
    user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL
) STRICT;
CREATE INDEX idx_link_tickets__expires_at ON link_tickets (expires_at);

CREATE TABLE auth_providers (
    id            TEXT PRIMARY KEY,
    client_id     TEXT NOT NULL,
    client_secret TEXT NOT NULL,
    enabled       INTEGER NOT NULL DEFAULT 1,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
) STRICT;

CREATE TABLE pending_transfers (
    token_hash       BLOB PRIMARY KEY,
    target_user_id   TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider         TEXT NOT NULL,
    provider_subject TEXT NOT NULL,
    source_user_id   TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at       TEXT NOT NULL,
    created_at       TEXT NOT NULL
) STRICT;
CREATE INDEX idx_pending_transfers__expires_at ON pending_transfers (expires_at);

CREATE TABLE user_notices (
    id         TEXT PRIMARY KEY,
    user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    text       TEXT NOT NULL,
    severity   TEXT NOT NULL,
    created_at TEXT NOT NULL
) STRICT;

CREATE INDEX idx_user_notices__user_id ON user_notices (user_id);

CREATE TABLE data_deletion_requests (
    confirmation_code TEXT PRIMARY KEY,
    provider          TEXT NOT NULL,
    provider_subject  TEXT NOT NULL,
    user_id           TEXT,
    status            TEXT NOT NULL,
    created_at        TEXT NOT NULL
) STRICT;

CREATE INDEX idx_data_deletion_requests__created_at
    ON data_deletion_requests (created_at DESC);


ALTER TABLE feedback ADD COLUMN user_id TEXT REFERENCES users(id) ON DELETE SET NULL;
ALTER TABLE feedback ADD COLUMN reply TEXT;
ALTER TABLE feedback ADD COLUMN replied_at TEXT;
ALTER TABLE feedback ADD COLUMN dismissed INTEGER NOT NULL DEFAULT 0;
