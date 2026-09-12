CREATE TABLE IF NOT EXISTS tutorial_gacha (
    account_id INTEGER PRIMARY KEY REFERENCES accounts(account_id) ON DELETE CASCADE,
    gacha_id INTEGER NOT NULL,
    button_id INTEGER NOT NULL,
    execution_count INTEGER NOT NULL CHECK (execution_count = 1),
    resource_type INTEGER NOT NULL,
    resource_id INTEGER NOT NULL,
    entity_id INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE(account_id, gacha_id, button_id)
);

INSERT OR IGNORE INTO schema_migrations(version) VALUES (5);
