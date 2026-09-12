//! Domain state facade.
//!
//! The service implementation and feature facades live in named modules so
//! callers do not depend on a compatibility module.

mod account;
pub(crate) mod activities;
pub(crate) mod atelier;
mod catalog;
pub(crate) mod character;
pub(crate) mod combat;
pub(crate) mod energy;
mod error;
pub(crate) mod gacha;
pub(crate) mod home;
pub(crate) mod modes;
pub(crate) mod party;
pub(crate) mod progression;
pub(crate) mod quest;
mod resources;
pub(crate) mod saved_state;
pub(crate) mod service;
pub(crate) mod shop;
pub(crate) mod synthesis;
pub(crate) use progression::context;
pub(crate) use service::unix_now;
#[cfg(test)]
pub(crate) use service::{set_simulation_now, simulation_now};

pub(crate) use combat::BattleStartMode;
pub use service::{SignInResult, State, StateError};
