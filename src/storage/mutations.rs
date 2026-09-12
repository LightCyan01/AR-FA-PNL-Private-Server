use rusqlite::{params, OptionalExtension, TransactionBehavior};

use super::migrations::{decode_home_state, encode_home_state};
use super::{GameplayCommit, GameplayMutationResult, MutationResult, StorageError, Store};

impl Store {
    pub fn apply_memoria(
        &self,
        account_id: i64,
        request_id: &str,
        fingerprint: &[u8],
        character_id: i64,
        new_memoria: Option<i64>,
        response: &[u8],
        now: i64,
    ) -> Result<MutationResult, StorageError> {
        let mut connection = self.lock_connection();
        let transaction_started = std::time::Instant::now();
        let transaction = connection.transaction()?;
        if let Some((existing_fingerprint, existing_response)) = transaction.query_row("SELECT request_fingerprint, response_plaintext FROM memoria_deltas WHERE account_id=?1 AND request_id=?2", params![account_id, request_id], |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?))).optional()? {
            if existing_fingerprint == fingerprint { return Ok(MutationResult::Replay(existing_response)); }
            return Err(StorageError::RequestConflict);
        }
        let old: Option<Option<i64>> = transaction
            .query_row(
                "SELECT memoria_entity_id FROM characters WHERE account_id=?1 AND character_id=?2",
                params![account_id, character_id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(old) = old else {
            return Err(StorageError::NotFound);
        };
        transaction.execute("UPDATE characters SET memoria_entity_id=?1, updated_at=?2 WHERE account_id=?3 AND character_id=?4", params![new_memoria, now, account_id, character_id])?;
        transaction.execute("INSERT INTO memoria_deltas(account_id, request_id, route, character_id, request_fingerprint, old_memoria_entity_id, new_memoria_entity_id, response_plaintext, applied_at) VALUES (?1, ?2, '/character/memoria_set', ?3, ?4, ?5, ?6, ?7, ?8)", params![account_id, request_id, character_id, fingerprint, old, new_memoria, response, now])?;
        transaction.commit()?;
        tracing::debug!(
            event = "storage_transaction",
            elapsed_us = transaction_started.elapsed().as_micros() as u64,
            redacted = true
        );
        Ok(MutationResult::Applied {
            old_memoria_entity_id: old,
            new_memoria_entity_id: new_memoria,
        })
    }

    pub fn gameplay_replay(
        &self,
        account_id: i64,
        request_id: &str,
        route: &str,
        fingerprint: &[u8],
    ) -> Result<Option<Vec<u8>>, StorageError> {
        let connection = self.lock_connection();
        let existing = connection
            .query_row(
                "SELECT route, request_fingerprint, response_plaintext FROM gameplay_mutations WHERE account_id=?1 AND request_id=?2",
                params![account_id, request_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                    ))
                },
            )
            .optional()?;
        match existing {
            Some((existing_route, existing_fingerprint, response))
                if existing_route == route && existing_fingerprint == fingerprint =>
            {
                Ok(Some(response))
            }
            Some(_) => Err(StorageError::RequestConflict),
            None => Ok(None),
        }
    }

    pub fn home_state(&self, account_id: i64) -> Result<Vec<u8>, StorageError> {
        let connection = self.lock_connection();
        decode_home_state(
            connection
                .query_row(
                    "SELECT state_json FROM home_state WHERE account_id=?1",
                    params![account_id],
                    |row| row.get(0),
                )
                .optional()?,
        )
    }

    pub fn apply_gameplay_reducer<F>(
        &self,
        account_id: i64,
        request_id: &str,
        route: &str,
        fingerprint: &[u8],
        now: i64,
        reduce: F,
    ) -> Result<GameplayMutationResult, StorageError>
    where
        F: FnOnce(&[u8]) -> Result<GameplayCommit, StorageError>,
    {
        let mut connection = self.lock_connection();
        let transaction_started = std::time::Instant::now();
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some((existing_route, existing_fingerprint, response)) = transaction
            .query_row(
                "SELECT route, request_fingerprint, response_plaintext FROM gameplay_mutations WHERE account_id=?1 AND request_id=?2",
                params![account_id, request_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?, row.get::<_, Vec<u8>>(2)?)),
            )
            .optional()?
        {
            if existing_route == route && existing_fingerprint == fingerprint {
                return Ok(GameplayMutationResult::Replay(response));
            }
            return Err(StorageError::RequestConflict);
        }
        let resources: Vec<u8> = transaction
            .query_row(
                "SELECT resources_blob FROM player_state WHERE account_id=?1",
                params![account_id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(StorageError::NotFound)?;
        let reducer_started = std::time::Instant::now();
        let commit = reduce(&resources)?;
        tracing::debug!(
            event = "storage_reducer",
            elapsed_us = reducer_started.elapsed().as_micros() as u64,
            redacted = true
        );
        transaction.execute(
            "UPDATE player_state SET resources_blob=?1, updated_at=?2 WHERE account_id=?3",
            params![commit.resources_blob, now, account_id],
        )?;
        for (character_id, memoria_entity_id) in commit.character_index {
            transaction.execute(
                "INSERT INTO characters(account_id, character_id, memoria_entity_id, updated_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(account_id, character_id) DO UPDATE SET memoria_entity_id=excluded.memoria_entity_id, updated_at=excluded.updated_at",
                params![account_id, character_id, memoria_entity_id, now],
            )?;
        }
        transaction.execute(
            "INSERT INTO gameplay_mutations(account_id, request_id, route, request_fingerprint, response_plaintext, applied_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![account_id, request_id, route, fingerprint, commit.response_plaintext, now],
        )?;
        transaction.commit()?;
        tracing::debug!(
            event = "storage_transaction",
            elapsed_us = transaction_started.elapsed().as_micros() as u64,
            redacted = true
        );
        Ok(GameplayMutationResult::Applied)
    }

    pub fn apply_home_reducer<F>(
        &self,
        account_id: i64,
        request_id: &str,
        route: &str,
        fingerprint: &[u8],
        now: i64,
        reduce: F,
    ) -> Result<GameplayMutationResult, StorageError>
    where
        F: FnOnce(&[u8], &[u8]) -> Result<(GameplayCommit, Vec<u8>), StorageError>,
    {
        let mut connection = self.lock_connection();
        let transaction_started = std::time::Instant::now();
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some((old_route, old_fingerprint, response)) = transaction.query_row(
            "SELECT route, request_fingerprint, response_plaintext FROM gameplay_mutations WHERE account_id=?1 AND request_id=?2",
            params![account_id, request_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?, row.get::<_, Vec<u8>>(2)?)),
        ).optional()? {
            return if old_route == route && old_fingerprint == fingerprint {
                Ok(GameplayMutationResult::Replay(response))
            } else { Err(StorageError::RequestConflict) };
        }
        if matches!(
            route,
            "/solo_raid/reset"
                | "/total_battle/reset_panel"
                | "/quest/talk_event/main_story_episode_skip"
                | "/quest/talk_event/main_story_season_skip"
                | "/quest/talk_event/prologue_skip"
                | "/exploration/skip"
        ) && transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM active_battles WHERE account_id=?1)",
            [account_id],
            |row| row.get::<_, bool>(0),
        )? {
            return Err(StorageError::ActiveBattleExists);
        }
        let resources: Vec<u8> = transaction.query_row(
            "SELECT resources_blob FROM player_state WHERE account_id=?1",
            [account_id],
            |row| row.get(0),
        )?;
        let home = decode_home_state(
            transaction
                .query_row(
                    "SELECT state_json FROM home_state WHERE account_id=?1",
                    [account_id],
                    |row| row.get(0),
                )
                .optional()?,
        )?;
        let reducer_started = std::time::Instant::now();
        let (commit, home) = reduce(&resources, &home)?;
        tracing::debug!(
            event = "storage_reducer",
            elapsed_us = reducer_started.elapsed().as_micros() as u64,
            redacted = true
        );
        transaction.execute(
            "UPDATE player_state SET resources_blob=?1, updated_at=?2 WHERE account_id=?3",
            params![commit.resources_blob, now, account_id],
        )?;
        transaction.execute("INSERT INTO home_state(account_id,state_json) VALUES(?1,?2) ON CONFLICT(account_id) DO UPDATE SET state_json=excluded.state_json",
            params![account_id, encode_home_state(&home)?])?;
        for (id, memoria) in commit.character_index {
            transaction.execute("INSERT INTO characters(account_id,character_id,memoria_entity_id,updated_at) VALUES(?1,?2,?3,?4) ON CONFLICT(account_id,character_id) DO UPDATE SET memoria_entity_id=excluded.memoria_entity_id,updated_at=excluded.updated_at",
                params![account_id, id, memoria, now])?;
        }
        transaction.execute("INSERT INTO gameplay_mutations(account_id,request_id,route,request_fingerprint,response_plaintext,applied_at) VALUES(?1,?2,?3,?4,?5,?6)",
            params![account_id, request_id, route, fingerprint, commit.response_plaintext, now])?;
        transaction.commit()?;
        tracing::debug!(
            event = "storage_transaction",
            elapsed_us = transaction_started.elapsed().as_micros() as u64,
            redacted = true
        );
        Ok(GameplayMutationResult::Applied)
    }

    pub fn apply_home_refresh_reducer<F>(
        &self,
        account_id: i64,
        now: i64,
        reduce: F,
    ) -> Result<(), StorageError>
    where
        F: FnOnce(&[u8], &[u8]) -> Result<(GameplayCommit, Vec<u8>), StorageError>,
    {
        let mut connection = self.lock_connection();
        let transaction_started = std::time::Instant::now();
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let resources: Vec<u8> = transaction.query_row(
            "SELECT resources_blob FROM player_state WHERE account_id=?1",
            [account_id],
            |row| row.get(0),
        )?;
        let home = decode_home_state(
            transaction
                .query_row(
                    "SELECT state_json FROM home_state WHERE account_id=?1",
                    [account_id],
                    |row| row.get(0),
                )
                .optional()?,
        )?;
        let (commit, home) = reduce(&resources, &home)?;
        transaction.execute(
            "UPDATE player_state SET resources_blob=?1, updated_at=?2 WHERE account_id=?3",
            params![commit.resources_blob, now, account_id],
        )?;
        transaction.execute(
            "INSERT INTO home_state(account_id,state_json) VALUES(?1,?2) ON CONFLICT(account_id) DO UPDATE SET state_json=excluded.state_json",
            params![account_id, encode_home_state(&home)?],
        )?;
        for (id, memoria) in commit.character_index {
            transaction.execute(
                "INSERT INTO characters(account_id,character_id,memoria_entity_id,updated_at) VALUES(?1,?2,?3,?4) ON CONFLICT(account_id,character_id) DO UPDATE SET memoria_entity_id=excluded.memoria_entity_id,updated_at=excluded.updated_at",
                params![account_id, id, memoria, now],
            )?;
        }
        transaction.commit()?;
        tracing::debug!(
            event = "storage_transaction",
            elapsed_us = transaction_started.elapsed().as_micros() as u64,
            redacted = true
        );
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn apply_gameplay(
        &self,
        account_id: i64,
        request_id: &str,
        route: &str,
        fingerprint: &[u8],
        resources_blob: &[u8],
        granted_character_id: Option<i64>,
        response: &[u8],
        now: i64,
    ) -> Result<GameplayMutationResult, StorageError> {
        let mut connection = self.lock_connection();
        let transaction_started = std::time::Instant::now();
        let transaction = connection.transaction()?;
        if let Some((existing_route, existing_fingerprint, existing_response)) = transaction.query_row(
            "SELECT route, request_fingerprint, response_plaintext FROM gameplay_mutations WHERE account_id=?1 AND request_id=?2",
            params![account_id, request_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?, row.get::<_, Vec<u8>>(2)?)),
        ).optional()? {
            return if existing_route == route && existing_fingerprint == fingerprint {
                Ok(GameplayMutationResult::Replay(existing_response))
            } else { Err(StorageError::RequestConflict) };
        }
        if transaction.execute(
            "UPDATE player_state SET resources_blob=?1, updated_at=?2 WHERE account_id=?3",
            params![resources_blob, now, account_id],
        )? != 1
        {
            return Err(StorageError::NotFound);
        }
        if let Some(character_id) = granted_character_id {
            transaction.execute(
                "INSERT OR IGNORE INTO characters(account_id, character_id) VALUES (?1, ?2)",
                params![account_id, character_id],
            )?;
        }
        transaction.execute("INSERT INTO gameplay_mutations(account_id, request_id, route, request_fingerprint, response_plaintext, applied_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params![account_id, request_id, route, fingerprint, response, now])?;
        transaction.commit()?;
        tracing::debug!(
            event = "storage_transaction",
            elapsed_us = transaction_started.elapsed().as_micros() as u64,
            redacted = true
        );
        Ok(GameplayMutationResult::Applied)
    }
}
