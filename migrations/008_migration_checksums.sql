CREATE TABLE IF NOT EXISTS schema_migration_checksums (
    version  INTEGER PRIMARY KEY REFERENCES schema_migrations(version) ON DELETE CASCADE,
    checksum BLOB NOT NULL CHECK (length(checksum) = 32)
);

INSERT OR IGNORE INTO schema_migrations(version) VALUES (8);
