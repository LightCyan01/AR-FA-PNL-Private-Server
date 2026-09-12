use rusqlite::{params, OptionalExtension, TransactionBehavior};

use super::{
    GachaButtonState, GachaCommit, GachaWishList, GameplayMutationResult, StorageError, Store,
};

impl Store {
    pub fn gacha_button_states(
        &self,
        account_id: i64,
    ) -> Result<Vec<GachaButtonState>, StorageError> {
        let connection = self.lock_connection();
        let mut statement = connection.prepare(
            "SELECT gacha_id, button_id, execution_count, last_executed_at FROM gacha_button_states WHERE account_id=?1",
        )?;
        let rows = statement.query_map(params![account_id], |row| {
            Ok(GachaButtonState {
                gacha_id: row.get(0)?,
                button_id: row.get(1)?,
                execution_count: row.get(2)?,
                last_executed_at: row.get(3)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)
    }

    pub fn gacha_wish_lists(&self, account_id: i64) -> Result<Vec<GachaWishList>, StorageError> {
        let connection = self.lock_connection();
        let mut statement = connection.prepare(
            "SELECT gacha_id, character_ids, memoria_ids, character_skin_ids FROM gacha_wish_lists WHERE account_id=?1",
        )?;
        let rows = statement.query_map(params![account_id], |row| {
            Ok(GachaWishList {
                gacha_id: row.get(0)?,
                character_ids: decode_ids(&row.get::<_, String>(1)?),
                memoria_ids: decode_ids(&row.get::<_, String>(2)?),
                character_skin_ids: decode_ids(&row.get::<_, String>(3)?),
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)
    }

    pub fn set_gacha_wish_list(
        &self,
        account_id: i64,
        wish_list: &GachaWishList,
        now: i64,
    ) -> Result<(), StorageError> {
        let connection = self.lock_connection();
        if connection.execute(
            "INSERT INTO gacha_wish_lists(account_id, gacha_id, character_ids, memoria_ids, character_skin_ids, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT(account_id, gacha_id) DO UPDATE SET character_ids=excluded.character_ids, memoria_ids=excluded.memoria_ids, character_skin_ids=excluded.character_skin_ids, updated_at=excluded.updated_at",
            params![
                account_id,
                wish_list.gacha_id,
                encode_ids(&wish_list.character_ids),
                encode_ids(&wish_list.memoria_ids),
                encode_ids(&wish_list.character_skin_ids),
                now,
            ],
        )? != 1
        {
            return Err(StorageError::NotFound);
        }
        Ok(())
    }

    pub fn apply_gacha<F>(
        &self,
        account_id: i64,
        request_id: &str,
        route: &str,
        fingerprint: &[u8],
        gacha_id: i64,
        button_id: i64,
        now: i64,
        reduce: F,
    ) -> Result<GameplayMutationResult, StorageError>
    where
        F: FnOnce(&[u8], i64, Option<GachaWishList>) -> Result<GachaCommit, StorageError>,
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
        let execution_count = transaction
            .query_row(
                "SELECT execution_count FROM gacha_button_states WHERE account_id=?1 AND gacha_id=?2 AND button_id=?3",
                params![account_id, gacha_id, button_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .unwrap_or(0);
        let wish_list = transaction
            .query_row(
                "SELECT character_ids, memoria_ids, character_skin_ids FROM gacha_wish_lists WHERE account_id=?1 AND gacha_id=?2",
                params![account_id, gacha_id],
                |row| {
                    Ok(GachaWishList {
                        gacha_id,
                        character_ids: decode_ids(&row.get::<_, String>(0)?),
                        memoria_ids: decode_ids(&row.get::<_, String>(1)?),
                        character_skin_ids: decode_ids(&row.get::<_, String>(2)?),
                    })
                },
            )
            .optional()?;
        let resources: Vec<u8> = transaction
            .query_row(
                "SELECT resources_blob FROM player_state WHERE account_id=?1",
                params![account_id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(StorageError::NotFound)?;
        let reducer_started = std::time::Instant::now();
        let commit = reduce(&resources, execution_count, wish_list)?;
        tracing::debug!(
            event = "storage_reducer",
            elapsed_us = reducer_started.elapsed().as_micros() as u64,
            redacted = true
        );
        transaction.execute(
            "UPDATE player_state SET resources_blob=?1, updated_at=?2 WHERE account_id=?3",
            params![commit.gameplay.resources_blob, now, account_id],
        )?;
        for (character_id, memoria_entity_id) in commit.gameplay.character_index {
            transaction.execute(
                "INSERT INTO characters(account_id, character_id, memoria_entity_id, updated_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(account_id, character_id) DO UPDATE SET memoria_entity_id=excluded.memoria_entity_id, updated_at=excluded.updated_at",
                params![account_id, character_id, memoria_entity_id, now],
            )?;
        }
        transaction.execute(
            "INSERT INTO gacha_button_states(account_id, gacha_id, button_id, execution_count, last_executed_at) VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT(account_id, gacha_id, button_id) DO UPDATE SET execution_count=excluded.execution_count, last_executed_at=excluded.last_executed_at",
            params![account_id, gacha_id, button_id, execution_count + 1, now],
        )?;
        transaction.execute(
            "INSERT INTO gameplay_mutations(account_id, request_id, route, request_fingerprint, response_plaintext, applied_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![account_id, request_id, route, fingerprint, commit.gameplay.response_plaintext, now],
        )?;
        transaction.commit()?;
        tracing::debug!(
            event = "storage_transaction",
            elapsed_us = transaction_started.elapsed().as_micros() as u64,
            redacted = true
        );
        Ok(GameplayMutationResult::Applied)
    }
}

fn encode_ids(values: &[i32]) -> String {
    values
        .iter()
        .map(i32::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn decode_ids(value: &str) -> Vec<i32> {
    value
        .split(',')
        .filter_map(|part| part.parse().ok())
        .collect()
}
