CREATE TABLE IF NOT EXISTS home_state (
    account_id INTEGER PRIMARY KEY REFERENCES accounts(account_id) ON DELETE CASCADE,
    state_json BLOB NOT NULL
);
INSERT OR IGNORE INTO schema_migrations(version) VALUES(7);
