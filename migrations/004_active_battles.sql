PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS active_battles (
    account_id   INTEGER PRIMARY KEY REFERENCES accounts(account_id) ON DELETE CASCADE,
    quest_id     INTEGER NOT NULL CHECK (quest_id > 0),
    battle_id    INTEGER NOT NULL CHECK (battle_id > 0),
    start_txid   TEXT NOT NULL UNIQUE CHECK (length(start_txid) > 0),
    state_version INTEGER NOT NULL CHECK (state_version = 1),
    state_blob   BLOB NOT NULL CHECK (length(state_blob) > 0),
    status       TEXT NOT NULL CHECK (status = 'active'),
    updated_at   INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX IF NOT EXISTS active_battles_status_idx
    ON active_battles(status, updated_at);

INSERT OR IGNORE INTO schema_migrations(version) VALUES (4);
