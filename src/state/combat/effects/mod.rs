//! Combat effect rules, per-battle lifetime bookkeeping, and shared policies.
//!
//! The facade intentionally exposes the small API consumed by the combat
//! state machine while keeping rule metadata, runtime mutation, and formulas
//! in separate modules.

mod policy;
mod registry;
mod runtime;
mod runtime_apply;
mod runtime_cover;
mod runtime_form;
mod runtime_lamp;
mod runtime_lifecycle;
mod runtime_match;
mod runtime_nested;
mod runtime_potency;
mod runtime_results;
mod runtime_scaling;
mod runtime_summary;
mod runtime_targeting;
mod runtime_turn;
#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use policy::incoming_multiplier_with_runtime;
pub(crate) use policy::{
    healing_amount, incoming_multiplier_for_skill, penetration_factor, secondary_damage,
};
pub(crate) use registry::{registry, rule_for, validate, Expiry, NestedActionKind};
pub(crate) use runtime::{Passive, PendingAction, Runtime};
pub(crate) use runtime_cover::Protection;
pub(crate) use runtime_lamp::{advance_skill_lamp, predicted_skill_lamp, skill_transformation};
pub(crate) use runtime_match::selected;
pub(crate) use runtime_results::display;
pub(crate) use runtime_scaling::scaled_skill_damage;
pub(crate) use runtime_summary::{instant_summary, instant_summary_for_source, timeline_slots};
