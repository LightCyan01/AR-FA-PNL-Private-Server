//! Combat effect rules, per-battle lifetime bookkeeping, and shared policies.
//!
//! The facade intentionally exposes the small API consumed by the combat
//! state machine while keeping rule metadata, runtime mutation, and formulas
//! in separate modules.

mod policy;
mod registry;
mod runtime;
mod runtime_apply;
mod runtime_lifecycle;
mod runtime_match;
mod runtime_results;
mod runtime_summary;
mod runtime_turn;
#[cfg(test)]
mod tests;

pub(crate) use policy::{
    healing_amount, incoming_multiplier_with_runtime, penetration_factor, secondary_damage,
};
pub(crate) use registry::{registry, validate, Expiry};
pub(crate) use runtime::{Passive, Runtime};
pub(crate) use runtime_match::selected;
pub(crate) use runtime_results::display;
pub(crate) use runtime_summary::{instant_summary, timeline_slots};
