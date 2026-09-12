PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS memoria_deltas (
    delta_id               INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id             INTEGER NOT NULL REFERENCES accounts(account_id) ON DELETE CASCADE,
    request_id             TEXT NOT NULL CHECK (length(request_id) BETWEEN 1 AND 128),
    route                  TEXT NOT NULL CHECK (route = '/character/memoria_set'),
    character_id           INTEGER NOT NULL CHECK (character_id > 0),
    request_fingerprint    BLOB NOT NULL CHECK (length(request_fingerprint) = 32),
    old_memoria_entity_id  INTEGER,
    new_memoria_entity_id  INTEGER,
    response_plaintext     BLOB NOT NULL,
    applied_at             INTEGER NOT NULL DEFAULT (unixepoch()),
    UNIQUE (account_id, request_id),
    FOREIGN KEY (account_id, character_id)
        REFERENCES characters(account_id, character_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS memoria_deltas_account_idx
    ON memoria_deltas(account_id, applied_at);

INSERT OR IGNORE INTO schema_migrations(version) VALUES (2);
