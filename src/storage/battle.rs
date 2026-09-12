use rusqlite::{params, OptionalExtension, TransactionBehavior};

use super::migrations::{decode_home_state, encode_home_state};
use super::{
    ActiveBattle, BattleFinishCommit, BattleStartCommit, GameplayMutationResult, StorageError,
    Store,
};

impl Store {
    pub fn active_battle(&self, account_id: i64) -> Result<Option<ActiveBattle>, StorageError> {
        let connection = self.lock_connection();
        connection
            .query_row(
                "SELECT account_id, quest_id, battle_id, start_txid, state_version, state_blob, status, updated_at FROM active_battles WHERE account_id=?1",
                params![account_id],
                |row| {
                    Ok(ActiveBattle {
                        account_id: row.get(0)?,
                        quest_id: row.get(1)?,
                        battle_id: row.get(2)?,
                        start_txid: row.get(3)?,
                        state_version: row.get(4)?,
                        state_blob: row.get(5)?,
                        status: row.get(6)?,
                        updated_at: row.get(7)?,
                    })
                },
            )
            .optional()
            .map_err(StorageError::from)
    }

    pub fn apply_battle_start(
        &self,
        account_id: i64,
        request_id: &str,
        route: &str,
        fingerprint: &[u8],
        quest_id: i64,
        battle_id: i64,
        start_txid: &str,
        state_blob: &[u8],
        response: &[u8],
        now: i64,
    ) -> Result<GameplayMutationResult, StorageError> {
        self.apply_battle_start_reducer(
            account_id,
            request_id,
            route,
            fingerprint,
            now,
            |resources, _home| {
                Ok(BattleStartCommit {
                    resources_blob: resources.to_vec(),
                    quest_id,
                    battle_id,
                    start_txid: start_txid.to_owned(),
                    state_blob: state_blob.to_vec(),
                    response_plaintext: response.to_vec(),
                    home_state: None,
                })
            },
        )
    }

    pub fn apply_battle_start_reducer<F>(
        &self,
        account_id: i64,
        request_id: &str,
        route: &str,
        fingerprint: &[u8],
        now: i64,
        reduce: F,
    ) -> Result<GameplayMutationResult, StorageError>
    where
        F: FnOnce(&[u8], &[u8]) -> Result<BattleStartCommit, StorageError>,
    {
        let mut connection = self.lock_connection();
        let transaction_started = std::time::Instant::now();
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing = transaction
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
        if let Some((existing_route, existing_fingerprint, existing_response)) = existing {
            if existing_route == route && existing_fingerprint == fingerprint {
                return Ok(GameplayMutationResult::Replay(existing_response));
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
        let active: Option<i64> = transaction
            .query_row(
                "SELECT account_id FROM active_battles WHERE account_id=?1",
                params![account_id],
                |row| row.get(0),
            )
            .optional()?;
        if active.is_some() {
            return Err(StorageError::ActiveBattleExists);
        }
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
        let commit = reduce(&resources, &home)?;
        tracing::debug!(
            event = "storage_reducer",
            elapsed_us = reducer_started.elapsed().as_micros() as u64,
            redacted = true
        );
        if let Some(home) = commit.home_state {
            transaction.execute("INSERT INTO home_state(account_id,state_json) VALUES(?1,?2) ON CONFLICT(account_id) DO UPDATE SET state_json=excluded.state_json",params![account_id, encode_home_state(&home)?])?;
        }
        transaction.execute(
            "UPDATE player_state SET resources_blob=?1, updated_at=?2 WHERE account_id=?3",
            params![commit.resources_blob, now, account_id],
        )?;
        transaction.execute(
            "INSERT INTO active_battles(account_id, quest_id, battle_id, start_txid, state_version, state_blob, status, updated_at) VALUES (?1, ?2, ?3, ?4, 1, ?5, 'active', ?6)",
            params![account_id, commit.quest_id, commit.battle_id, commit.start_txid, commit.state_blob, now],
        )?;
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

    pub fn apply_battle_attack<F>(
        &self,
        account_id: i64,
        request_id: &str,
        route: &str,
        fingerprint: &[u8],
        now: i64,
        reduce: F,
    ) -> Result<GameplayMutationResult, StorageError>
    where
        F: FnOnce(&ActiveBattle, &[u8], &[u8]) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>), StorageError>,
    {
        let mut connection = self.lock_connection();
        let transaction_started = std::time::Instant::now();
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some((existing_route, existing_fingerprint, existing_response)) = transaction
            .query_row(
                "SELECT route, request_fingerprint, response_plaintext FROM gameplay_mutations WHERE account_id=?1 AND request_id=?2",
                params![account_id, request_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?, row.get::<_, Vec<u8>>(2)?)),
            )
            .optional()?
        {
            if existing_route == route && existing_fingerprint == fingerprint {
                return Ok(GameplayMutationResult::Replay(existing_response));
            }
            return Err(StorageError::RequestConflict);
        }
        let active = transaction
            .query_row(
                "SELECT account_id, quest_id, battle_id, start_txid, state_version, state_blob, status, updated_at FROM active_battles WHERE account_id=?1",
                params![account_id],
                |row| Ok(ActiveBattle {
                    account_id: row.get(0)?, quest_id: row.get(1)?, battle_id: row.get(2)?,
                    start_txid: row.get(3)?, state_version: row.get(4)?, state_blob: row.get(5)?,
                    status: row.get(6)?, updated_at: row.get(7)?,
                }),
            )
            .optional()?
            .ok_or(StorageError::BattleInactive)?;
        if active.state_version != 1 || active.status != "active" {
            return Err(StorageError::BattleInactive);
        }
        let resources: Vec<u8> = transaction.query_row(
            "SELECT resources_blob FROM player_state WHERE account_id=?1",
            params![account_id],
            |row| row.get(0),
        )?;
        let home = decode_home_state(
            transaction
                .query_row(
                    "SELECT state_json FROM home_state WHERE account_id=?1",
                    params![account_id],
                    |row| row.get(0),
                )
                .optional()?,
        )?;
        let reducer_started = std::time::Instant::now();
        let (state_blob, response, home) = reduce(&active, &resources, &home)?;
        tracing::debug!(
            event = "storage_reducer",
            elapsed_us = reducer_started.elapsed().as_micros() as u64,
            redacted = true
        );
        transaction.execute("INSERT INTO home_state(account_id,state_json) VALUES(?1,?2) ON CONFLICT(account_id) DO UPDATE SET state_json=excluded.state_json", params![account_id, encode_home_state(&home)?])?;
        if transaction.execute(
            "UPDATE active_battles SET state_blob=?1, updated_at=?2 WHERE account_id=?3 AND state_version=1 AND status='active'",
            params![state_blob, now, account_id],
        )? != 1
        {
            return Err(StorageError::BattleInactive);
        }
        transaction.execute(
            "INSERT INTO gameplay_mutations(account_id, request_id, route, request_fingerprint, response_plaintext, applied_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![account_id, request_id, route, fingerprint, response, now],
        )?;
        transaction.commit()?;
        tracing::debug!(
            event = "storage_transaction",
            elapsed_us = transaction_started.elapsed().as_micros() as u64,
            redacted = true
        );
        Ok(GameplayMutationResult::Applied)
    }

    pub fn apply_battle_finish<F>(
        &self,
        account_id: i64,
        request_id: &str,
        route: &str,
        fingerprint: &[u8],
        now: i64,
        finish: F,
    ) -> Result<GameplayMutationResult, StorageError>
    where
        F: FnOnce(&ActiveBattle, &[u8], &[u8]) -> Result<BattleFinishCommit, StorageError>,
    {
        let mut connection = self.lock_connection();
        let transaction_started = std::time::Instant::now();
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some((existing_route, existing_fingerprint, existing_response)) = transaction
            .query_row(
                "SELECT route, request_fingerprint, response_plaintext FROM gameplay_mutations WHERE account_id=?1 AND request_id=?2",
                params![account_id, request_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?, row.get::<_, Vec<u8>>(2)?)),
            )
            .optional()?
        {
            if existing_route == route && existing_fingerprint == fingerprint {
                return Ok(GameplayMutationResult::Replay(existing_response));
            }
            return Err(StorageError::RequestConflict);
        }
        let active = transaction
            .query_row(
                "SELECT account_id, quest_id, battle_id, start_txid, state_version, state_blob, status, updated_at FROM active_battles WHERE account_id=?1",
                params![account_id],
                |row| Ok(ActiveBattle {
                    account_id: row.get(0)?, quest_id: row.get(1)?, battle_id: row.get(2)?,
                    start_txid: row.get(3)?, state_version: row.get(4)?, state_blob: row.get(5)?,
                    status: row.get(6)?, updated_at: row.get(7)?,
                }),
            )
            .optional()?
            .ok_or(StorageError::BattleInactive)?;
        if active.state_version != 1 || active.status != "active" {
            return Err(StorageError::BattleInactive);
        }
        let resources: Vec<u8> = transaction.query_row(
            "SELECT resources_blob FROM player_state WHERE account_id=?1",
            params![account_id],
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
        let commit = finish(&active, &resources, &home)?;
        tracing::debug!(
            event = "storage_reducer",
            elapsed_us = reducer_started.elapsed().as_micros() as u64,
            redacted = true
        );
        if let Some(home) = commit.home_state {
            transaction.execute("INSERT INTO home_state(account_id,state_json) VALUES(?1,?2) ON CONFLICT(account_id) DO UPDATE SET state_json=excluded.state_json",params![account_id, encode_home_state(&home)?])?;
        }
        if transaction.execute(
            "UPDATE player_state SET resources_blob=?1, updated_at=?2 WHERE account_id=?3",
            params![commit.resources_blob, now, account_id],
        )? != 1
        {
            return Err(StorageError::NotFound);
        }
        for (character_id, memoria_entity_id) in commit.character_index {
            transaction.execute(
                "INSERT INTO characters(account_id, character_id, memoria_entity_id, updated_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(account_id, character_id) DO UPDATE SET memoria_entity_id=excluded.memoria_entity_id, updated_at=excluded.updated_at",
                params![account_id, character_id, memoria_entity_id, now],
            )?;
        }
        if transaction.execute(
            "DELETE FROM active_battles WHERE account_id=?1",
            params![account_id],
        )? != 1
        {
            return Err(StorageError::BattleInactive);
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

    pub fn latest_battle_start_response(
        &self,
        account_id: i64,
    ) -> Result<Option<Vec<u8>>, StorageError> {
        let connection = self.lock_connection();
        Ok(connection.query_row("SELECT response_plaintext FROM gameplay_mutations WHERE account_id=?1 AND route IN ('/quest/battle/start','/quest/battle/rental_party_start','/exploration/battle_start','/quest/battle/solo_raid_battle_start','/quest/battle/total_battle_start','/gacha/battle_start') ORDER BY mutation_id DESC LIMIT 1",[account_id],|row|row.get(0)).optional()?)
    }

    pub fn active_battle_count(&self, account_id: i64) -> Result<i64, StorageError> {
        let connection = self.lock_connection();
        Ok(connection.query_row(
            "SELECT COUNT(*) FROM active_battles WHERE account_id=?1",
            params![account_id],
            |row| row.get(0),
        )?)
    }

    pub fn gameplay_mutation_count(&self, account_id: i64) -> Result<i64, StorageError> {
        let connection = self.lock_connection();
        Ok(connection.query_row(
            "SELECT COUNT(*) FROM gameplay_mutations WHERE account_id=?1",
            params![account_id],
            |row| row.get(0),
        )?)
    }
}
