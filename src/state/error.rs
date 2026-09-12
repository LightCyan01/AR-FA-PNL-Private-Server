// Errors returned by state-owned operations.

use thiserror::Error;

use crate::storage::StorageError;

#[derive(Debug, Error)]
pub enum StateError {
    #[error("storage: {0}")]
    Storage(#[from] StorageError),
    #[error("invalid account credentials")]
    Credentials,
    #[error("invalid request")]
    InvalidRequest,
    #[error("grant is invalid or expired")]
    InvalidGrant,
    #[error("session is invalid or expired")]
    InvalidSession,
    #[error("descriptor: {0}")]
    Descriptor(String),
    #[error("static master data is invalid: {0}")]
    MasterData(String),
    #[error("fresh-state rules are invalid: {0}")]
    FreshRules(String),
    #[error("tutorial rules are invalid: {0}")]
    TutorialRules(String),
    #[error("character rules are invalid: {0}")]
    CharacterRules(String),
    #[error("synthesis rules are invalid: {0}")]
    SynthesisRules(String),
    #[error("reward rules are invalid: {0}")]
    RewardRules(String),
    #[error("quest is out of schedule")]
    OutOfSchedule,
}
