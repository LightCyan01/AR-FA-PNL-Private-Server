PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS schema_migrations (
    version     INTEGER PRIMARY KEY,
    applied_at  INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE TABLE IF NOT EXISTS accounts (
    account_id   INTEGER PRIMARY KEY AUTOINCREMENT,
    username     TEXT NOT NULL COLLATE NOCASE UNIQUE
                 CHECK (length(username) BETWEEN 3 AND 32),
    password_phc TEXT NOT NULL CHECK (length(password_phc) > 0),
    created_at   INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at   INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE TABLE IF NOT EXISTS account_devices (
    account_id          INTEGER NOT NULL REFERENCES accounts(account_id) ON DELETE CASCADE,
    device_binding_hash BLOB NOT NULL CHECK (length(device_binding_hash) = 32),
    first_seen_at       INTEGER NOT NULL DEFAULT (unixepoch()),
    last_seen_at        INTEGER NOT NULL DEFAULT (unixepoch()),
    PRIMARY KEY (account_id, device_binding_hash)
);

CREATE INDEX IF NOT EXISTS account_devices_binding_idx
    ON account_devices(device_binding_hash);

CREATE TABLE IF NOT EXISTS auth_grants (
    grant_id   INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id INTEGER NOT NULL REFERENCES accounts(account_id) ON DELETE CASCADE,
    token_hash BLOB NOT NULL UNIQUE CHECK (length(token_hash) = 32),
    issued_at  INTEGER NOT NULL DEFAULT (unixepoch()),
    expires_at INTEGER NOT NULL,
    revoked_at INTEGER,
    CHECK (expires_at > issued_at)
);

CREATE INDEX IF NOT EXISTS auth_grants_account_idx
    ON auth_grants(account_id);

CREATE TABLE IF NOT EXISTS sessions (
    session_id   INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id   INTEGER NOT NULL REFERENCES accounts(account_id) ON DELETE CASCADE,
    token_hash   BLOB NOT NULL UNIQUE CHECK (length(token_hash) = 32),
    issued_at    INTEGER NOT NULL DEFAULT (unixepoch()),
    expires_at   INTEGER NOT NULL,
    revoked_at   INTEGER,
    last_seen_at INTEGER NOT NULL DEFAULT (unixepoch()),
    CHECK (expires_at > issued_at)
);

CREATE INDEX IF NOT EXISTS sessions_account_idx ON sessions(account_id);
CREATE INDEX IF NOT EXISTS sessions_expiry_idx ON sessions(expires_at);

CREATE TABLE IF NOT EXISTS player_state (
    account_id          INTEGER PRIMARY KEY REFERENCES accounts(account_id) ON DELETE CASCADE,
    resources_blob      BLOB NOT NULL,
    master_data_version TEXT NOT NULL,
    created_at          INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at          INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE TABLE IF NOT EXISTS characters (
    account_id        INTEGER NOT NULL REFERENCES accounts(account_id) ON DELETE CASCADE,
    character_id      INTEGER NOT NULL CHECK (character_id > 0),
    memoria_entity_id INTEGER,
    created_at        INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at        INTEGER NOT NULL DEFAULT (unixepoch()),
    PRIMARY KEY (account_id, character_id)
);

INSERT OR IGNORE INTO schema_migrations(version) VALUES (1);
