PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS gameplay_mutations (
    mutation_id          INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id           INTEGER NOT NULL REFERENCES accounts(account_id) ON DELETE CASCADE,
    request_id           TEXT NOT NULL CHECK (length(request_id) BETWEEN 1 AND 128),
    route                TEXT NOT NULL CHECK (length(route) > 0),
    request_fingerprint  BLOB NOT NULL CHECK (length(request_fingerprint) = 32),
    response_plaintext   BLOB NOT NULL,
    applied_at           INTEGER NOT NULL DEFAULT (unixepoch()),
    UNIQUE (account_id, request_id)
);

CREATE INDEX IF NOT EXISTS gameplay_mutations_account_idx
    ON gameplay_mutations(account_id, applied_at);

INSERT OR IGNORE INTO schema_migrations(version) VALUES (3);
