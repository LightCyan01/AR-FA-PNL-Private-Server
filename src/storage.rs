use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex, MutexGuard,
    },
};

use rusqlite::Connection;
use sha2::{Digest, Sha256};
use thiserror::Error;

const EXPECTED_SCHEMA_VERSION: i64 = 8;
/// Completed retries are kept for one day and, independently, the newest 256 per inactive account.
const JOURNAL_RETENTION_SECONDS: i64 = 24 * 60 * 60;
const JOURNAL_RETENTION_COUNT: i64 = 256;
const MAINTENANCE_EVERY_REQUESTS: usize = 32;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("storage I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("sqlite failed: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("migration {version} failed: {message}")]
    Migration { version: i64, message: String },
    #[error("storage schema version {actual} does not match expected {expected}")]
    SchemaVersion { actual: i64, expected: i64 },
    #[error("migration checksum metadata is missing for an existing database")]
    MigrationMetadataMissing,
    #[error("migration {version} checksum differs from the applied script")]
    MigrationDrift { version: i64 },
    #[error("unsupported HomeState version {version}")]
    HomeStateVersion { version: i64 },
    #[error("request ID was reused with different request data")]
    RequestConflict,
    #[error("account or character does not exist")]
    NotFound,
    #[error("account already has an active battle")]
    ActiveBattleExists,
    #[error("battle is not active")]
    BattleInactive,
    #[error("battle command was rejected")]
    BattleRejected,
    #[error("battle transition rejected: {0}")]
    Battle(String),
    #[error("tutorial gacha was already executed")]
    GachaLimit,
}

#[derive(Clone)]
pub struct Store {
    path: PathBuf,
    connection: Arc<Mutex<Connection>>,
    maintenance_tick: Arc<AtomicUsize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub account_id: i64,
    pub user_id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MutationResult {
    Applied {
        old_memoria_entity_id: Option<i64>,
        new_memoria_entity_id: Option<i64>,
    },
    Replay(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameplayMutationResult {
    Applied,
    Replay(Vec<u8>),
}

#[derive(Debug, Clone)]
pub struct BattleFinishCommit {
    pub resources_blob: Vec<u8>,
    pub response_plaintext: Vec<u8>,
    pub character_index: Vec<(i64, Option<i64>)>,
    pub home_state: Option<Vec<u8>>,
}

pub struct BattleStartCommit {
    pub resources_blob: Vec<u8>,
    pub quest_id: i64,
    pub battle_id: i64,
    pub start_txid: String,
    pub state_blob: Vec<u8>,
    pub response_plaintext: Vec<u8>,
    pub home_state: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct GameplayCommit {
    pub resources_blob: Vec<u8>,
    pub response_plaintext: Vec<u8>,
    pub character_index: Vec<(i64, Option<i64>)>,
}

#[derive(Debug, Clone)]
pub struct GachaCommit {
    pub gameplay: GameplayCommit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GachaButtonState {
    pub gacha_id: i64,
    pub button_id: i64,
    pub execution_count: i64,
    pub last_executed_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GachaWishList {
    pub gacha_id: i64,
    pub character_ids: Vec<i32>,
    pub memoria_ids: Vec<i32>,
    pub character_skin_ids: Vec<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveBattle {
    pub account_id: i64,
    pub quest_id: i64,
    pub battle_id: i64,
    pub start_txid: String,
    pub state_version: i64,
    pub state_blob: Vec<u8>,
    pub status: String,
    pub updated_at: i64,
}

#[allow(clippy::too_many_arguments)]
mod accounts;
#[allow(clippy::too_many_arguments)]
mod battle;
#[allow(clippy::too_many_arguments)]
mod gacha;
mod journal;
mod migrations;
#[allow(clippy::too_many_arguments)]
mod mutations;
#[cfg(test)]
mod tests;

pub fn hash_token(token: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(token);
    hasher.finalize().into()
}

pub fn device_binding_hash(secret: &[u8], device_unique_id: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(secret);
    hasher.update([0]);
    hasher.update(device_unique_id.as_bytes());
    hasher.finalize().into()
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let path = path.as_ref().to_path_buf();
        #[cfg(test)]
        assert_ne!(
            path.file_name().and_then(|name| name.to_str()),
            Some("atelier.sqlite3"),
            "tests must never open the live account database"
        );
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(&path)?;
        migrations::configure(&connection)?;
        migrations::migrate(&connection)?;
        Ok(Self {
            path,
            connection: Arc::new(Mutex::new(connection)),
            maintenance_tick: Arc::new(AtomicUsize::new(0)),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn maintenance_due(&self) -> bool {
        self.maintenance_tick
            .fetch_add(1, Ordering::Relaxed)
            .is_multiple_of(MAINTENANCE_EVERY_REQUESTS)
    }

    pub(super) fn lock_connection(&self) -> MutexGuard<'_, Connection> {
        let started = std::time::Instant::now();
        let connection = self.connection.lock().expect("storage mutex poisoned");
        tracing::debug!(
            event = "storage_mutex",
            wait_us = started.elapsed().as_micros() as u64,
            redacted = true
        );
        connection
    }
}
