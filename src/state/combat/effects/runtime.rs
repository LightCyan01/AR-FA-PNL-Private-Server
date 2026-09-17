use super::registry::{NestedActionKind, NestedActionRule, Rule};
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

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct NestedActionInstance {
    pub(crate) source: i32,
    pub(crate) target: i32,
    pub(crate) remaining: i32,
    pub(crate) rule: NestedActionRule,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PendingAction {
    pub(crate) actor_id: i32,
    pub(crate) skill_id: i32,
    pub(crate) target_id: i32,
    pub(crate) kind: NestedActionKind,
    pub(crate) burst_gauge_cost: i32,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct Runtime {
    pub(crate) transaction: String,
    pub(crate) next_action_number: i32,
    pub(crate) wave: i32,
    pub(crate) bases: BTreeMap<i32, Baseline>,
    #[serde(skip)]
    pub(crate) stat_rates: BTreeMap<i32, BTreeMap<String, i64>>,
    pub(crate) instances: Vec<Instance>,
    pub(crate) passives: Vec<Passive>,
    pub(crate) lamp_abilities: BTreeMap<i32, BTreeSet<i32>>,
    pub(crate) nested_actions: Vec<NestedActionInstance>,
    pub(crate) managed: BTreeMap<i32, BTreeSet<i32>>,
    pub(crate) unsupported: BTreeSet<i32>,
    pub(crate) panel_damage_taken: BTreeMap<i32, i32>,
    pub(crate) acquired_panel_wave: i32,
    pub(crate) acquired_panel_turn: i32,
    pub(crate) pending_actor: i32,
    pub(crate) pending_blind_rate: i32,
    pub(crate) pending_provocation_target: Option<i32>,
}

impl Runtime {
    pub(crate) fn initiative_members(&self) -> BTreeSet<i32> {
        self.passives
            .iter()
            .filter(|passive| passive.rule.operation == "initiative")
            .map(|passive| passive.source)
            .collect()
    }

    pub(crate) fn damage_immunity(&self, member_id: i32, attribute: i32) -> bool {
        self.instances.iter().any(|instance| {
            instance.target == member_id
                && instance.rule.operation == "damage_immunity"
                && (instance.rule.attack_attributes.is_empty()
                    || instance.rule.attack_attributes.contains(&attribute))
        })
    }

    pub(crate) fn stat_rate(&self, member_id: i32, name: &str) -> i64 {
        self.stat_rates
            .get(&member_id)
            .and_then(|rates| rates.get(name))
            .copied()
            .unwrap_or_default()
    }
}
