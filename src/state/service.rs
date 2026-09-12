//! Stateful application service and route-facing operations.

mod auth_and_login;
mod foundation;
mod helpers;
mod quest_and_battle;
mod synthesis_party_gacha;
mod talk_and_resources;

#[cfg(test)]
mod player_simulation;
#[cfg(test)]
mod rng_simulation;
#[cfg(test)]
mod tests;

pub use crate::state::{account::SignInResult, error::StateError};
pub use foundation::State;
pub(crate) use helpers::unix_now;
#[cfg(test)]
pub(crate) use helpers::{set_simulation_now, simulation_now};

pub(crate) mod prelude {
    pub(crate) use std::{
        collections::{BTreeMap, BTreeSet},
        time::{SystemTime, UNIX_EPOCH},
    };

    pub(crate) use argon2::{
        password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
        Argon2,
    };
    pub(crate) use prost::Message;
    pub(crate) use prost_reflect::{DynamicMessage, ReflectMessage, Value};
    pub(crate) use rand_core::{OsRng, RngCore};
    pub(crate) use serde::Deserialize;
    pub(crate) use serde_json::Value as Json;
    pub(crate) use sha2::{Digest, Sha256};
    pub(crate) use uuid::Uuid;

    pub(crate) use super::{foundation::*, helpers::*, talk_and_resources::*};
    pub(crate) use crate::state::progression::context::StateContext;
    pub(crate) use crate::state::{combat::*, gacha::*, party::*, synthesis::*};
    pub(crate) use crate::{
        config::LoadedConfig,
        state::{
            account::*, activities, atelier, catalog::*, character, energy, error::*, home, modes,
            progression, quest, resources::*, shop,
        },
        storage::{
            device_binding_hash, ActiveBattle, BattleFinishCommit, BattleStartCommit,
            GachaButtonState, GachaCommit, GachaWishList, GameplayCommit, GameplayMutationResult,
            Session, StorageError, Store,
        },
        transport::{request_fingerprint, ProtoRegistry},
    };
}
