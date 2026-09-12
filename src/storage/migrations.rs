use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::{StorageError, EXPECTED_SCHEMA_VERSION};

pub(super) fn configure(connection: &Connection) -> Result<(), StorageError> {
    connection.execute_batch("PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;")?;
    Ok(())
}

pub(super) fn encode_home_state(state: &[u8]) -> Result<Vec<u8>, StorageError> {
    let state: Value = serde_json::from_slice(state)
        .map_err(|error| StorageError::Battle(format!("HomeState serialization: {error}")))?;
    serde_json::to_vec(&json!({ "version": 1, "state": state }))
        .map_err(|error| StorageError::Battle(format!("HomeState serialization: {error}")))
}

pub(super) fn decode_home_state(stored: Option<Vec<u8>>) -> Result<Vec<u8>, StorageError> {
    let Some(stored) = stored else {
        return Ok(b"{}".to_vec());
    };
    let value: Value = serde_json::from_slice(&stored)
        .map_err(|error| StorageError::Battle(format!("HomeState serialization: {error}")))?;
    let Some(object) = value.as_object() else {
        return Err(StorageError::HomeStateVersion { version: -1 });
    };
    let Some(version) = object.get("version") else {
        // Legacy unversioned state is accepted once and wrapped on its next write.
        return Ok(stored);
    };
    let version = version
        .as_i64()
        .ok_or(StorageError::HomeStateVersion { version: -1 })?;
    if version != 1 {
        return Err(StorageError::HomeStateVersion { version });
    }
    let state = object
        .get("state")
        .ok_or(StorageError::HomeStateVersion { version })?;
    serde_json::to_vec(state)
        .map_err(|error| StorageError::Battle(format!("HomeState serialization: {error}")))
}

pub(super) fn migrations() -> [(i64, &'static str); 8] {
    [
        (1, include_str!("../../migrations/001_starter_state.sql")),
        (2, include_str!("../../migrations/002_memoria_delta.sql")),
        (
            3,
            include_str!("../../migrations/003_gameplay_mutations.sql"),
        ),
        (4, include_str!("../../migrations/004_active_battles.sql")),
        (5, include_str!("../../migrations/005_tutorial_gacha.sql")),
        (6, include_str!("../../migrations/006_gacha_state.sql")),
        (7, include_str!("../../migrations/007_home_state.sql")),
        (
            8,
            include_str!("../../migrations/008_migration_checksums.sql"),
        ),
    ]
}

pub(super) fn migration_checksum(sql: &str) -> [u8; 32] {
    Sha256::digest(sql.as_bytes()).into()
}

pub(super) fn migrate(connection: &Connection) -> Result<(), StorageError> {
    let has_history: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='schema_migrations')",
        [],
        |row| row.get(0),
    )?;
    let has_metadata: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='schema_migration_checksums')",
        [],
        |row| row.get(0),
    )?;
    if has_history && !has_metadata {
        let versions: Vec<i64> = {
            let mut statement =
                connection.prepare("SELECT version FROM schema_migrations ORDER BY version")?;
            let rows = statement.query_map([], |row| row.get(0))?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let legacy_versions: Vec<i64> = (1..EXPECTED_SCHEMA_VERSION).collect();
        if versions != legacy_versions {
            return Err(StorageError::MigrationMetadataMissing);
        }

        // Legacy databases were created before checksum metadata existed. The
        // complete, contiguous 1..7 history is the only shape we can safely
        // baseline; later opens validate every checksum normally.
        let transaction = connection.unchecked_transaction()?;
        transaction.execute_batch(
            "CREATE TABLE schema_migration_checksums (
                 version INTEGER PRIMARY KEY REFERENCES schema_migrations(version) ON DELETE CASCADE,
                 checksum BLOB NOT NULL CHECK (length(checksum) = 32)
             );",
        )?;
        for (version, sql) in migrations().into_iter().take(legacy_versions.len()) {
            let checksum = migration_checksum(sql);
            transaction.execute(
                "INSERT INTO schema_migration_checksums(version, checksum) VALUES (?1, ?2)",
                params![version, checksum.as_slice()],
            )?;
        }
        transaction.commit()?;
    }
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migration_checksums (
             version INTEGER PRIMARY KEY REFERENCES schema_migrations(version) ON DELETE CASCADE,
             checksum BLOB NOT NULL CHECK (length(checksum) = 32)
         );",
    )?;
    for (version, sql) in migrations() {
        let applied: Option<Vec<u8>> = connection
            .query_row(
                "SELECT checksum FROM schema_migration_checksums WHERE version=?1",
                [version],
                |row| row.get(0),
            )
            .optional()?;
        let checksum = migration_checksum(sql);
        if let Some(applied) = applied {
            if applied != checksum {
                return Err(StorageError::MigrationDrift { version });
            }
            continue;
        }
        if has_history
            && connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version=?1)",
                [version],
                |row| row.get::<_, bool>(0),
            )?
        {
            return Err(StorageError::MigrationMetadataMissing);
        }
        let transaction = connection.unchecked_transaction()?;
        transaction
            .execute_batch(sql)
            .map_err(|error| StorageError::Migration {
                version,
                message: error.to_string(),
            })?;
        transaction.execute(
            "INSERT INTO schema_migration_checksums(version, checksum) VALUES (?1, ?2)",
            params![version, checksum.as_slice()],
        )?;
        transaction.commit()?;
    }
    let versions: Vec<i64> = {
        let mut statement =
            connection.prepare("SELECT version FROM schema_migrations ORDER BY version")?;
        let rows = statement.query_map([], |row| row.get(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    let expected: Vec<i64> = (1..=EXPECTED_SCHEMA_VERSION).collect();
    if versions != expected {
        let actual = versions.last().copied().unwrap_or(0);
        return Err(StorageError::SchemaVersion {
            actual,
            expected: EXPECTED_SCHEMA_VERSION,
        });
    }
    Ok(())
}
