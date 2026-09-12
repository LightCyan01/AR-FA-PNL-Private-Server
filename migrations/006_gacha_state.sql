CREATE TABLE IF NOT EXISTS gacha_button_states (
    account_id INTEGER NOT NULL REFERENCES accounts(account_id) ON DELETE CASCADE,
    gacha_id INTEGER NOT NULL,
    button_id INTEGER NOT NULL,
    execution_count INTEGER NOT NULL DEFAULT 0 CHECK (execution_count >= 0),
    last_executed_at INTEGER NOT NULL,
    PRIMARY KEY(account_id, gacha_id, button_id)
);

INSERT OR IGNORE INTO gacha_button_states(account_id, gacha_id, button_id, execution_count, last_executed_at)
SELECT account_id, gacha_id, button_id, execution_count, created_at FROM tutorial_gacha;

CREATE TABLE IF NOT EXISTS gacha_wish_lists (
    account_id INTEGER NOT NULL REFERENCES accounts(account_id) ON DELETE CASCADE,
    gacha_id INTEGER NOT NULL,
    character_ids TEXT NOT NULL DEFAULT '',
    memoria_ids TEXT NOT NULL DEFAULT '',
    character_skin_ids TEXT NOT NULL DEFAULT '',
    updated_at INTEGER NOT NULL,
    PRIMARY KEY(account_id, gacha_id)
);

INSERT OR IGNORE INTO schema_migrations(version) VALUES (6);
