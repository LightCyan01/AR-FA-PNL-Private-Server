use super::registry::Rule;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

// Per-battle effect instances, lifetime handling, and protocol state refresh.
#[derive(Clone, Default, Deserialize, Serialize)]
pub(crate) struct Baseline {
    pub(crate) stats: BTreeMap<String, i32>,
    pub(crate) summaries: BTreeMap<i32, i32>,
}

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct Instance {
    pub(crate) source: i32,
    #[serde(default)]
    pub(crate) source_character_id: i32,
    pub(crate) target: i32,
    pub(crate) value: i32,
    pub(crate) remaining: i32,
    pub(crate) rule: Rule,
}

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct Passive {
    pub(crate) source: i32,
    pub(crate) value: i32,
    pub(crate) rule: Rule,
    #[serde(default)]
    pub(crate) source_character_id: i32,
    #[serde(default)]
    pub(crate) source_type: i32,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct Runtime {
    pub(crate) transaction: String,
    pub(crate) wave: i32,
    pub(crate) bases: BTreeMap<i32, Baseline>,
    pub(crate) instances: Vec<Instance>,
    pub(crate) passives: Vec<Passive>,
    pub(crate) managed: BTreeMap<i32, BTreeSet<i32>>,
    pub(crate) unsupported: BTreeSet<i32>,
    pub(crate) panel_damage_taken: BTreeMap<i32, i32>,
    pub(crate) acquired_panel_wave: i32,
    pub(crate) acquired_panel_turn: i32,
    pub(crate) pending_actor: i32,
    pub(crate) pending_blind_rate: i32,
    pub(crate) pending_provocation_target: Option<i32>,
}
