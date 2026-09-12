use rusqlite::{params, OptionalExtension, TransactionBehavior};

use super::{hash_token, Session, StorageError, Store};

impl Store {
    pub fn create_account(&self, username: &str, password_phc: &str) -> Result<i64, StorageError> {
        let connection = self.lock_connection();
        connection.execute(
            "INSERT INTO accounts(username, password_phc) VALUES (?1, ?2)",
            params![username, password_phc],
        )?;
        Ok(connection.last_insert_rowid())
    }

    pub fn account_by_username(
        &self,
        username: &str,
    ) -> Result<Option<(i64, String)>, StorageError> {
        let connection = self.lock_connection();
        Ok(connection
            .query_row(
                "SELECT account_id, password_phc FROM accounts WHERE username=?1",
                params![username],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?)
    }

    pub fn delete_account(&self, account_id: i64) -> Result<(), StorageError> {
        let connection = self.lock_connection();
        // All account-owned tables use ON DELETE CASCADE; one statement is atomic.
        if connection.execute("DELETE FROM accounts WHERE account_id=?1", [account_id])? != 1 {
            return Err(StorageError::NotFound);
        }
        Ok(())
    }

    pub fn issue_grant(
        &self,
        account_id: i64,
        token: &[u8],
        now: i64,
        expires_at: i64,
    ) -> Result<(), StorageError> {
        let hash = hash_token(token);
        let connection = self.lock_connection();
        connection.execute("INSERT INTO auth_grants(account_id, token_hash, issued_at, expires_at) VALUES (?1, ?2, ?3, ?4)", params![account_id, hash.as_slice(), now, expires_at])?;
        Ok(())
    }

    pub fn resolve_grant(&self, token: &[u8], now: i64) -> Result<Option<i64>, StorageError> {
        let hash = hash_token(token);
        let connection = self.lock_connection();
        Ok(connection.query_row("SELECT account_id FROM auth_grants WHERE token_hash=?1 AND revoked_at IS NULL AND expires_at>?2", params![hash.as_slice(), now], |row| row.get(0)).optional()?)
    }

    pub fn consume_grant_and_initialize(
        &self,
        grant: &[u8],
        device_binding: &[u8],
        resources_blob: &[u8],
        master_data_version: &str,
        initial_character_id: i64,
        session_token: &[u8],
        now: i64,
        session_expires_at: i64,
    ) -> Result<Option<Session>, StorageError> {
        let grant_hash = hash_token(grant);
        let session_hash = hash_token(session_token);
        let mut connection = self.lock_connection();
        let transaction_started = std::time::Instant::now();
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let consumed = transaction.execute(
            "UPDATE auth_grants SET revoked_at=?1 WHERE token_hash=?2 AND revoked_at IS NULL AND expires_at>?1",
            params![now, grant_hash.as_slice()],
        )?;
        if consumed != 1 {
            return Ok(None);
        }
        let account_id: i64 = transaction.query_row(
            "SELECT account_id FROM auth_grants WHERE token_hash=?1",
            params![grant_hash.as_slice()],
            |row| row.get(0),
        )?;
        transaction.execute(
            "INSERT INTO account_devices(account_id, device_binding_hash, first_seen_at, last_seen_at) VALUES (?1, ?2, ?3, ?3) ON CONFLICT(account_id, device_binding_hash) DO UPDATE SET last_seen_at=excluded.last_seen_at",
            params![account_id, device_binding, now],
        )?;
        transaction.execute(
            "INSERT OR IGNORE INTO player_state(account_id, resources_blob, master_data_version) VALUES (?1, ?2, ?3)",
            params![account_id, resources_blob, master_data_version],
        )?;
        transaction.execute(
            "INSERT OR IGNORE INTO characters(account_id, character_id) VALUES (?1, ?2)",
            params![account_id, initial_character_id],
        )?;
        transaction.execute(
            "INSERT INTO sessions(account_id, token_hash, issued_at, expires_at, last_seen_at) VALUES (?1, ?2, ?3, ?4, ?3)",
            params![account_id, session_hash.as_slice(), now, session_expires_at],
        )?;
        transaction.commit()?;
        tracing::debug!(
            event = "storage_transaction",
            elapsed_us = transaction_started.elapsed().as_micros() as u64,
            redacted = true
        );
        Ok(Some(Session {
            account_id,
            user_id: account_id,
        }))
    }

    pub fn issue_session(
        &self,
        account_id: i64,
        token: &[u8],
        now: i64,
        expires_at: i64,
    ) -> Result<(), StorageError> {
        let hash = hash_token(token);
        let connection = self.lock_connection();
        connection.execute("INSERT INTO sessions(account_id, token_hash, issued_at, expires_at, last_seen_at) VALUES (?1, ?2, ?3, ?4, ?3)", params![account_id, hash.as_slice(), now, expires_at])?;
        Ok(())
    }

    pub fn resolve_session(&self, token: &[u8], now: i64) -> Result<Option<Session>, StorageError> {
        let hash = hash_token(token);
        let connection = self.lock_connection();
        let session = connection.query_row("SELECT s.account_id, a.account_id FROM sessions s JOIN accounts a ON a.account_id=s.account_id WHERE s.token_hash=?1 AND s.revoked_at IS NULL AND s.expires_at>?2", params![hash.as_slice(), now], |row| Ok(Session { account_id: row.get(0)?, user_id: row.get(1)? })).optional()?;
        if session.is_some() {
            connection.execute(
                "UPDATE sessions SET last_seen_at=?1 WHERE token_hash=?2",
                params![now, hash.as_slice()],
            )?;
        }
        Ok(session)
    }

    pub fn upsert_device(
        &self,
        account_id: i64,
        binding: &[u8],
        now: i64,
    ) -> Result<(), StorageError> {
        let connection = self.lock_connection();
        connection.execute("INSERT INTO account_devices(account_id, device_binding_hash, first_seen_at, last_seen_at) VALUES (?1, ?2, ?3, ?3) ON CONFLICT(account_id, device_binding_hash) DO UPDATE SET last_seen_at=excluded.last_seen_at", params![account_id, binding, now])?;
        Ok(())
    }

    pub fn ensure_player_state(
        &self,
        account_id: i64,
        resources_blob: &[u8],
        master_data_version: &str,
    ) -> Result<(), StorageError> {
        let connection = self.lock_connection();
        connection.execute("INSERT OR IGNORE INTO player_state(account_id, resources_blob, master_data_version) VALUES (?1, ?2, ?3)", params![account_id, resources_blob, master_data_version])?;
        Ok(())
    }

    pub fn player_resources(&self, account_id: i64) -> Result<Vec<u8>, StorageError> {
        let connection = self.lock_connection();
        connection
            .query_row(
                "SELECT resources_blob FROM player_state WHERE account_id=?1",
                params![account_id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(StorageError::NotFound)
    }

    pub fn ensure_character(&self, account_id: i64, character_id: i64) -> Result<(), StorageError> {
        let connection = self.lock_connection();
        connection.execute(
            "INSERT OR IGNORE INTO characters(account_id, character_id) VALUES (?1, ?2)",
            params![account_id, character_id],
        )?;
        Ok(())
    }

    pub fn characters(&self, account_id: i64) -> Result<Vec<(i64, Option<i64>)>, StorageError> {
        let connection = self.lock_connection();
        let mut statement = connection.prepare("SELECT character_id, memoria_entity_id FROM characters WHERE account_id=?1 ORDER BY character_id")?;
        let rows =
            statement.query_map(params![account_id], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)
    }
}
