use super::*;
use crate::storage::migrations::{decode_home_state, migration_checksum, migrations};
use rusqlite::{params, Connection};

#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use super::*;
    use std::{fs, sync::Arc, thread};

    fn fresh_store() -> (Store, std::path::PathBuf) {
        let path =
            std::env::temp_dir().join(format!("atelier-handoff-{}.sqlite3", uuid::Uuid::new_v4()));
        (Store::open(&path).unwrap(), path)
    }

    #[test]
    fn handoff_grant_is_single_use() {
        let (store, path) = fresh_store();
        let account_id = store.create_account("handoff_once", "test").unwrap();
        store.issue_grant(account_id, b"grant", 100, 200).unwrap();
        let first = store
            .consume_grant_and_initialize(
                b"grant",
                &[1; 32],
                b"resources",
                "master",
                43101,
                b"session-1",
                101,
                201,
            )
            .unwrap();
        assert_eq!(first.unwrap().account_id, account_id);
        assert!(store
            .consume_grant_and_initialize(
                b"grant",
                &[1; 32],
                b"resources",
                "master",
                43101,
                b"session-2",
                102,
                202,
            )
            .unwrap()
            .is_none());
        assert_eq!(store.player_resources(account_id).unwrap(), b"resources");
        assert_eq!(store.characters(account_id).unwrap(), vec![(43101, None)]);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn handoff_grant_concurrent_use_has_one_winner() {
        let (store, path) = fresh_store();
        let account_id = store.create_account("handoff_race", "test").unwrap();
        store.issue_grant(account_id, b"grant", 100, 200).unwrap();
        let store = Arc::new(store);
        thread::scope(|scope| {
            let first = Arc::clone(&store);
            let second = Arc::clone(&store);
            let call = |store: Arc<Store>, token: &'static [u8]| {
                store.consume_grant_and_initialize(
                    b"grant",
                    &[2; 32],
                    b"resources",
                    "master",
                    43101,
                    token,
                    101,
                    201,
                )
            };
            let left = scope.spawn(move || call(first, b"session-left"));
            let right = scope.spawn(move || call(second, b"session-right"));
            let results = [left.join().unwrap(), right.join().unwrap()];
            assert_eq!(
                results
                    .iter()
                    .filter(|result| result.as_ref().unwrap().is_some())
                    .count(),
                1
            );
        });
        assert_eq!(store.characters(account_id).unwrap(), vec![(43101, None)]);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn handoff_grant_failure_rolls_back_initialization() {
        let (store, path) = fresh_store();
        let account_id = store.create_account("handoff_rollback", "test").unwrap();
        store.issue_grant(account_id, b"grant", 100, 200).unwrap();
        store
            .issue_session(account_id, b"existing", 100, 200)
            .unwrap();
        let result = store.consume_grant_and_initialize(
            b"grant",
            &[3; 32],
            b"resources",
            "master",
            43101,
            b"existing",
            101,
            201,
        );
        assert!(matches!(
            result,
            Err(StorageError::Sql(rusqlite::Error::SqliteFailure(_, _)))
        ));
        assert_eq!(
            store.resolve_grant(b"grant", 101).unwrap(),
            Some(account_id)
        );
        assert!(matches!(
            store.player_resources(account_id),
            Err(StorageError::NotFound)
        ));
        assert!(store.characters(account_id).unwrap().is_empty());
        assert!(store.resolve_session(b"existing", 101).unwrap().is_some());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn storage_fixture_preserves_atomic_account_battle_and_replay_contracts() {
        let (store, path) = fresh_store();
        let account_id = store.create_account("fixture_user", "test").unwrap();
        store.issue_grant(account_id, b"grant", 1, 10).unwrap();
        assert_eq!(store.resolve_grant(b"grant", 2).unwrap(), Some(account_id));
        store.issue_session(account_id, b"session", 1, 10).unwrap();
        assert_eq!(
            store
                .resolve_session(b"session", 2)
                .unwrap()
                .unwrap()
                .account_id,
            account_id
        );
        store
            .ensure_player_state(account_id, b"before", "test")
            .unwrap();
        store.ensure_character(account_id, 43101).unwrap();

        let fingerprint = [1; 32];
        assert!(matches!(
            store.apply_gameplay_reducer(
                account_id,
                "rollback",
                "/fixture",
                &fingerprint,
                3,
                |_| Err(StorageError::Battle("injected".into()))
            ),
            Err(StorageError::Battle(_))
        ));
        assert_eq!(store.player_resources(account_id).unwrap(), b"before");

        assert_eq!(
            store
                .apply_battle_start(
                    account_id,
                    "start",
                    "/quest/battle/start",
                    &fingerprint,
                    1,
                    2,
                    "txid",
                    b"state",
                    b"start-response",
                    4
                )
                .unwrap(),
            GameplayMutationResult::Applied
        );
        assert!(store.active_battle(account_id).unwrap().is_some());
        assert_eq!(
            store.latest_battle_start_response(account_id).unwrap(),
            Some(b"start-response".to_vec())
        );
        assert_eq!(
            store
                .apply_battle_start_reducer(
                    account_id,
                    "start",
                    "/quest/battle/start",
                    &fingerprint,
                    4,
                    |_resources, _home| panic!("replay must not reduce"),
                )
                .unwrap(),
            GameplayMutationResult::Replay(b"start-response".to_vec())
        );
        assert_eq!(
            store
                .apply_battle_attack(
                    account_id,
                    "attack",
                    "/battle/attack",
                    &fingerprint,
                    5,
                    |_battle, _resources, _home| Ok((
                        b"next".to_vec(),
                        b"attack-response".to_vec(),
                        b"{}".to_vec()
                    ))
                )
                .unwrap(),
            GameplayMutationResult::Applied
        );
        assert_eq!(
            store.active_battle(account_id).unwrap().unwrap().state_blob,
            b"next".to_vec()
        );
        assert_eq!(
            store
                .apply_battle_attack(
                    account_id,
                    "attack",
                    "/battle/attack",
                    &fingerprint,
                    5,
                    |_battle, _resources, _home| panic!("replay must not reduce"),
                )
                .unwrap(),
            GameplayMutationResult::Replay(b"attack-response".to_vec())
        );
        assert_eq!(
            store
                .apply_battle_finish(
                    account_id,
                    "finish",
                    "/battle/finish",
                    &fingerprint,
                    6,
                    |_battle, _resources, _home| Ok(BattleFinishCommit {
                        resources_blob: b"after".to_vec(),
                        response_plaintext: b"finish-response".to_vec(),
                        character_index: Vec::new(),
                        home_state: Some(b"{}".to_vec())
                    })
                )
                .unwrap(),
            GameplayMutationResult::Applied
        );
        assert!(store.active_battle(account_id).unwrap().is_none());
        assert_eq!(
            store
                .apply_battle_finish(
                    account_id,
                    "finish",
                    "/battle/finish",
                    &fingerprint,
                    6,
                    |_battle, _resources, _home| panic!("replay must not reduce"),
                )
                .unwrap(),
            GameplayMutationResult::Replay(b"finish-response".to_vec())
        );
        assert_eq!(store.player_resources(account_id).unwrap(), b"after");
        assert_eq!(
            store
                .gameplay_replay(account_id, "finish", "/battle/finish", &fingerprint)
                .unwrap(),
            Some(b"finish-response".to_vec())
        );
        assert!(matches!(
            store.gameplay_replay(account_id, "finish", "/battle/finish", &[2; 32]),
            Err(StorageError::RequestConflict)
        ));
        drop(store);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn journal_retention_bounds_completed_rows_and_keeps_active_battle_recovery() {
        let (store, path) = fresh_store();
        let account_id = store.create_account("retention_user", "test").unwrap();
        store
            .ensure_player_state(account_id, b"before", "test")
            .unwrap();
        store.ensure_character(account_id, 43101).unwrap();
        store
            .issue_grant(account_id, b"expired-grant", 1, 2)
            .unwrap();
        store
            .issue_session(account_id, b"expired-session", 1, 2)
            .unwrap();
        for index in 0..(JOURNAL_RETENTION_COUNT + 4) {
            store
                .apply_gameplay(
                    account_id,
                    &format!("request-{index}"),
                    "/fixture",
                    &[3; 32],
                    b"before",
                    None,
                    b"response",
                    index,
                )
                .unwrap();
        }
        for index in 0..(JOURNAL_RETENTION_COUNT + 4) {
            store
                .apply_memoria(
                    account_id,
                    &format!("memoria-{index}"),
                    &[7; 32],
                    43101,
                    Some(index),
                    b"memoria-response",
                    index,
                )
                .unwrap();
        }
        assert_eq!(
            store
                .apply_gameplay(
                    account_id,
                    "same-request",
                    "/fixture",
                    &[5; 32],
                    b"before",
                    None,
                    b"exact-response",
                    300,
                )
                .unwrap(),
            GameplayMutationResult::Applied
        );
        assert_eq!(
            store
                .apply_gameplay(
                    account_id,
                    "same-request",
                    "/fixture",
                    &[5; 32],
                    b"different",
                    None,
                    b"different-response",
                    301,
                )
                .unwrap(),
            GameplayMutationResult::Replay(b"exact-response".to_vec())
        );
        assert!(matches!(
            store.apply_gameplay(
                account_id,
                "same-request",
                "/fixture",
                &[6; 32],
                b"different",
                None,
                b"different-response",
                301,
            ),
            Err(StorageError::RequestConflict)
        ));
        for _ in 0..4 {
            store.prune_expired(10_000).unwrap();
        }
        assert!(store.gameplay_mutation_count(account_id).unwrap() <= JOURNAL_RETENTION_COUNT);
        assert!(
            store
                .connection
                .lock()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM memoria_deltas WHERE account_id=?1",
                    [account_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap()
                <= JOURNAL_RETENTION_COUNT
        );
        assert_eq!(store.resolve_grant(b"expired-grant", 10_000).unwrap(), None);
        assert_eq!(
            store.resolve_session(b"expired-session", 10_000).unwrap(),
            None
        );

        store
            .apply_battle_start(
                account_id,
                "active-start",
                "/quest/battle/start",
                &[4; 32],
                1,
                2,
                "retention-txid",
                b"state",
                b"start",
                10_001,
            )
            .unwrap();
        store
            .apply_gameplay(
                account_id,
                "discard-after-active",
                "/fixture",
                &[8; 32],
                b"before",
                None,
                b"discard",
                0,
            )
            .unwrap();
        for _ in 0..4 {
            store.prune_expired(100_000).unwrap();
        }
        assert_eq!(
            store.latest_battle_start_response(account_id).unwrap(),
            Some(b"start".to_vec())
        );
        assert_eq!(
            store
                .gameplay_replay(account_id, "discard-after-active", "/fixture", &[8; 32])
                .unwrap(),
            None
        );
        let before_refresh = store.gameplay_mutation_count(account_id).unwrap();
        store
            .apply_home_refresh_reducer(account_id, 20_001, |resources, home| {
                Ok((
                    GameplayCommit {
                        resources_blob: resources.to_vec(),
                        response_plaintext: Vec::new(),
                        character_index: Vec::new(),
                    },
                    home.to_vec(),
                ))
            })
            .unwrap();
        assert_eq!(
            store.gameplay_mutation_count(account_id).unwrap(),
            before_refresh
        );
        drop(store);
        let reopened = Store::open(&path).unwrap();
        assert_eq!(
            reopened.latest_battle_start_response(account_id).unwrap(),
            Some(b"start".to_vec())
        );
        drop(reopened);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn migration_drift_checks_incremental_history_and_home_state_versions() {
        let rollback = std::env::temp_dir().join(format!(
            "atelier-migration-rollback-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let connection = Connection::open(&rollback).unwrap();
        let transaction = connection.unchecked_transaction().unwrap();
        assert!(transaction
            .execute_batch(
                "CREATE TABLE rollback_probe(value INTEGER); INSERT INTO missing_probe VALUES (1);"
            )
            .is_err());
        drop(transaction);
        let exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='rollback_probe')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!exists);
        drop(connection);
        let _ = fs::remove_file(rollback);

        let fresh =
            std::env::temp_dir().join(format!("atelier-fresh-{}.sqlite3", uuid::Uuid::new_v4()));
        let fresh_store = Store::open(&fresh).unwrap();
        drop(fresh_store);
        assert!(Store::open(&fresh).is_ok());
        let _ = fs::remove_file(fresh);

        let incomplete =
            std::env::temp_dir().join(format!("atelier-metadata-{}.sqlite3", uuid::Uuid::new_v4()));
        let connection = Connection::open(&incomplete).unwrap();
        connection
            .execute_batch(include_str!("../../migrations/001_starter_state.sql"))
            .unwrap();
        connection.execute_batch("CREATE TABLE schema_migration_checksums (version INTEGER PRIMARY KEY REFERENCES schema_migrations(version) ON DELETE CASCADE, checksum BLOB NOT NULL CHECK (length(checksum) = 32));").unwrap();
        drop(connection);
        assert!(matches!(
            Store::open(&incomplete),
            Err(StorageError::MigrationMetadataMissing)
        ));
        let _ = fs::remove_file(incomplete);

        for checkpoint in 1..=7 {
            let checkpoint_path = std::env::temp_dir().join(format!(
                "atelier-checkpoint-{checkpoint}-{}.sqlite3",
                uuid::Uuid::new_v4()
            ));
            let connection = Connection::open(&checkpoint_path).unwrap();
            for (version, sql) in migrations().into_iter().take(checkpoint) {
                connection.execute_batch(sql).unwrap();
                if version == 1 {
                    connection.execute_batch("CREATE TABLE schema_migration_checksums (version INTEGER PRIMARY KEY REFERENCES schema_migrations(version) ON DELETE CASCADE, checksum BLOB NOT NULL CHECK (length(checksum) = 32));").unwrap();
                }
                let checksum = migration_checksum(sql);
                connection
                    .execute(
                        "INSERT INTO schema_migration_checksums(version, checksum) VALUES (?1, ?2)",
                        params![version, checksum.as_slice()],
                    )
                    .unwrap();
            }
            drop(connection);
            let upgraded = Store::open(&checkpoint_path).unwrap();
            assert_eq!(
                upgraded
                    .connection
                    .lock()
                    .unwrap()
                    .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| row
                        .get::<_, i64>(
                        0
                    ))
                    .unwrap(),
                EXPECTED_SCHEMA_VERSION
            );
            drop(upgraded);
            assert!(Store::open(&checkpoint_path).is_ok());
            let _ = fs::remove_file(checkpoint_path);
        }

        let path = std::env::temp_dir().join(format!(
            "atelier-migration-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(include_str!("../../migrations/001_starter_state.sql"))
            .unwrap();
        connection.execute_batch("CREATE TABLE schema_migration_checksums (version INTEGER PRIMARY KEY REFERENCES schema_migrations(version) ON DELETE CASCADE, checksum BLOB NOT NULL CHECK (length(checksum) = 32));").unwrap();
        let checksum = migration_checksum(include_str!("../../migrations/001_starter_state.sql"));
        connection
            .execute(
                "INSERT INTO schema_migration_checksums(version, checksum) VALUES (?1, ?2)",
                params![1, checksum.as_slice()],
            )
            .unwrap();
        drop(connection);
        let store = Store::open(&path).unwrap();
        assert_eq!(
            store
                .connection
                .lock()
                .unwrap()
                .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| row
                    .get::<_, i64>(
                    0
                ))
                .unwrap(),
            EXPECTED_SCHEMA_VERSION
        );
        assert_eq!(
            decode_home_state(Some(br#"{"language":1}"#.to_vec())).unwrap(),
            br#"{"language":1}"#.to_vec()
        );
        assert_eq!(
            decode_home_state(Some(br#"{"version":1,"state":{"language":1}}"#.to_vec())).unwrap(),
            br#"{"language":1}"#.to_vec()
        );
        assert!(matches!(
            decode_home_state(Some(br#"{"version":2,"state":{}}"#.to_vec())),
            Err(StorageError::HomeStateVersion { version: 2 })
        ));
        let account_id = store.create_account("saved_home", "test").unwrap();
        store
            .ensure_player_state(account_id, b"resources", "test")
            .unwrap();
        store
            .connection
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO home_state(account_id, state_json) VALUES (?1, ?2)",
                params![account_id, br#"{"language":1}"#],
            )
            .unwrap();
        store
            .apply_home_refresh_reducer(account_id, 1, |resources, home| {
                Ok((
                    GameplayCommit {
                        resources_blob: resources.to_vec(),
                        response_plaintext: Vec::new(),
                        character_index: Vec::new(),
                    },
                    home.to_vec(),
                ))
            })
            .unwrap();
        let version: i64 = store
            .connection
            .lock()
            .unwrap()
            .query_row(
                "SELECT json_extract(state_json, '$.version') FROM home_state WHERE account_id=?1",
                [account_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, 1);
        drop(store);
        let reopened = Store::open(&path).unwrap();
        assert_eq!(
            reopened
                .connection
                .lock()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM schema_migration_checksums",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            EXPECTED_SCHEMA_VERSION
        );
        drop(reopened);
        let connection = Connection::open(&path).unwrap();
        connection
            .execute(
                "UPDATE schema_migration_checksums SET checksum=?1 WHERE version=1",
                [vec![0; 32]],
            )
            .unwrap();
        drop(connection);
        assert!(matches!(
            Store::open(&path),
            Err(StorageError::MigrationDrift { version: 1 })
        ));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn legacy_complete_history_is_baselined_with_checksums() {
        let path = std::env::temp_dir().join(format!(
            "atelier-legacy-metadata-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let connection = Connection::open(&path).unwrap();
        for (_, sql) in migrations().into_iter().take(7) {
            connection.execute_batch(sql).unwrap();
        }
        drop(connection);

        let store = Store::open(&path).unwrap();
        let connection = store.connection.lock().unwrap();
        assert_eq!(
            connection
                .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| row
                    .get::<_, i64>(
                    0
                ))
                .unwrap(),
            EXPECTED_SCHEMA_VERSION
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM schema_migration_checksums",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            EXPECTED_SCHEMA_VERSION
        );
        drop(connection);
        drop(store);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn async_storage_temp_db_measurement_reports_p50_p95() {
        let (store, path) = fresh_store();
        let account_id = store.create_account("timing_user", "test").unwrap();
        store
            .ensure_player_state(account_id, b"resources", "test")
            .unwrap();
        let mut samples = Vec::new();
        for _ in 0..16 {
            let started = std::time::Instant::now();
            assert_eq!(store.player_resources(account_id).unwrap(), b"resources");
            samples.push(started.elapsed().as_micros());
        }
        samples.sort_unstable();
        let p50 = samples[samples.len() / 2];
        let p95 = samples[samples.len() * 95 / 100];
        println!("async_storage temporary-db operation_us p50={p50} p95={p95}");
        drop(store);
        let _ = fs::remove_file(path);
    }
}
