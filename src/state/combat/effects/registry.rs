use crate::state::combat::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::OnceLock,
};

// Rule metadata and validation are isolated from runtime bookkeeping.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Expiry {
    Permanent,
    Turn,
    Hit,
    Attack,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct RuleVariant {
    pub(crate) expiry: Expiry,
    pub(crate) duration: i32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Rule {
    pub(crate) id: i32,
    pub(crate) mode: String,
    pub(crate) target: String,
    pub(crate) phase: String,
    pub(crate) operation: String,
    pub(crate) summary: i32,
    pub(crate) sign: i32,
    pub(crate) fixed: Option<i32>,
    pub(crate) state_id: i32,
    pub(crate) expiry: Expiry,
    pub(crate) duration: i32,
    pub(crate) condition: BTreeMap<String, i32>,
    #[serde(default)]
    pub(crate) target_character_ids: Vec<i32>,
    #[serde(default)]
    pub(crate) source_character_ids: Vec<i32>,
    #[serde(default)]
    pub(crate) skill_types: Vec<i32>,
    #[serde(default)]
    pub(crate) attack_attributes: Vec<i32>,
    #[serde(default)]
    pub(crate) critical_only: bool,
    #[serde(default)]
    pub(crate) weak_only: bool,
    #[serde(default)]
    pub(crate) target_broken: bool,
    #[serde(default)]
    pub(crate) trigger: Option<String>,
    #[serde(default)]
    pub(crate) panel_from_ids: Vec<i32>,
    #[serde(default)]
    pub(crate) panel_to_id: i32,
    #[serde(default)]
    pub(crate) stack_cap: i32,
    pub(crate) positive: bool,
}

#[derive(Deserialize)]
pub(crate) struct Registry {
    pub(crate) source_sha256: String,
    pub(crate) max_party_gauge: i32,
    pub(crate) removable_negative_state_ids: Vec<i32>,
    pub(crate) removable_abnormal_state_ids: Vec<i32>,
    pub(crate) negative_state_ids: Vec<i32>,
    pub(crate) abnormal_state_ids: Vec<i32>,
    pub(crate) negative_immunity_state_ids: Vec<i32>,
    pub(crate) abnormal_immunity_state_ids: Vec<i32>,
    pub(crate) status_durations_by_skill: BTreeMap<i32, BTreeMap<i32, i32>>,
    pub(crate) targets_by_skill: BTreeMap<i32, BTreeMap<i32, String>>,
    pub(crate) phases_by_skill: BTreeMap<i32, BTreeMap<i32, String>>,
    pub(crate) rule_variants_by_skill: BTreeMap<i32, BTreeMap<i32, Vec<RuleVariant>>>,
    pub(crate) rules: Vec<Rule>,
}

pub(crate) fn registry() -> Result<&'static Registry, StateError> {
    static RULES: OnceLock<Result<Registry, String>> = OnceLock::new();
    RULES
        .get_or_init(|| {
            serde_json::from_str(include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../data/combat_effect_rules.json"
            )))
            .map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(|e| StateError::MasterData(e.clone()))
}

pub(crate) fn validate(source_hash: &str) -> Result<(), StateError> {
    let data = registry()?;
    let mut ids = BTreeSet::new();
    if data.source_sha256 != source_hash
        || data.rules.iter().any(|r| {
            !ids.insert(r.id)
                || !matches!(r.mode.as_str(), "active" | "passive" | "instant")
                || !matches!(
                    r.target.as_str(),
                    "self" | "targets" | "allies" | "enemies" | "next_enemy"
                )
                || !matches!(r.phase.as_str(), "before" | "after")
                || !matches!(
                    r.operation.as_str(),
                    "summary"
                        | "attack"
                        | "magic"
                        | "defense"
                        | "mental"
                        | "speed"
                        | "physical_taken"
                        | "magic_taken"
                        | "taken_down"
                        | "attribute_taken"
                        | "panel_disable"
                        | "panel_convert"
                        | "timeline_shift"
                        | "healing"
                        | "healing_received"
                        | "heal"
                        | "party_gauge"
                        | "cleanse"
                        | "cleanse_abnormal"
                        | "regeneration"
                        | "negative_immunity"
                        | "abnormal_immunity"
                        | "status"
                )
                || (r.operation == "summary" && !(1..=36).contains(&r.summary))
                || (r.mode == "active"
                    && r.state_id <= 0
                    && !matches!(
                        r.operation.as_str(),
                        "panel_convert"
                            | "timeline_shift"
                            | "heal"
                            | "party_gauge"
                            | "cleanse"
                            | "cleanse_abnormal"
                    ))
                || (r.operation == "panel_convert"
                    && (r.panel_to_id <= 0 || r.panel_from_ids.is_empty()))
                || r.stack_cap < 0
                || (r.expiry != Expiry::Permanent && (r.duration <= 0 || r.state_id <= 0))
                || r.target_character_ids.iter().any(|id| *id <= 0)
                || r.source_character_ids.iter().any(|id| *id <= 0)
                || r.skill_types.iter().any(|id| !matches!(id, 1..=3))
                || r.attack_attributes
                    .iter()
                    .any(|id| !matches!(id, 1..=3 | 5..=8))
                || r.trigger
                    .as_deref()
                    .is_some_and(|trigger| trigger != "attack_after")
        })
        || data.max_party_gauge <= 0
        || data.removable_negative_state_ids.is_empty()
        || data.removable_negative_state_ids.iter().any(|id| *id <= 0)
        || data.removable_abnormal_state_ids.is_empty()
        || data.removable_abnormal_state_ids.iter().any(|id| *id <= 0)
        || data.negative_state_ids.is_empty()
        || data.abnormal_state_ids.is_empty()
        || data.negative_immunity_state_ids.is_empty()
        || data.abnormal_immunity_state_ids.is_empty()
        || data.negative_state_ids.iter().any(|id| *id <= 0)
        || data.abnormal_state_ids.iter().any(|id| *id <= 0)
        || data.negative_immunity_state_ids.iter().any(|id| *id <= 0)
        || data.abnormal_immunity_state_ids.iter().any(|id| *id <= 0)
        || data.status_durations_by_skill.values().any(|effects| {
            effects.iter().any(|(effect_id, duration)| {
                *duration <= 0
                    || !data
                        .rules
                        .iter()
                        .any(|rule| rule.id == *effect_id && rule.operation == "status")
            })
        })
        || data.targets_by_skill.iter().any(|(skill_id, effects)| {
            *skill_id <= 0
                || effects.iter().any(|(effect_id, target)| {
                    !matches!(target.as_str(), "self" | "targets" | "allies" | "enemies")
                        || !data.rules.iter().any(|rule| rule.id == *effect_id)
                })
        })
        || data.phases_by_skill.iter().any(|(skill_id, effects)| {
            *skill_id <= 0
                || effects.iter().any(|(effect_id, phase)| {
                    !matches!(phase.as_str(), "before" | "after")
                        || !data.rules.iter().any(|rule| rule.id == *effect_id)
                })
        })
        || data
            .rule_variants_by_skill
            .iter()
            .any(|(skill_id, effects)| {
                *skill_id <= 0
                    || effects.iter().any(|(effect_id, variants)| {
                        variants.is_empty()
                            || !data.rules.iter().any(|rule| {
                                rule.id == *effect_id && rule.mode == "active" && rule.state_id > 0
                            })
                            || variants.iter().enumerate().any(|(index, variant)| {
                                variant.duration <= 0
                                    || variant.expiry == Expiry::Permanent
                                    || variants[..index]
                                        .iter()
                                        .any(|earlier| earlier.expiry == variant.expiry)
                            })
                    })
            })
    {
        return Err(StateError::MasterData(
            "invalid combat effect rules or source hash".into(),
        ));
    }
    Ok(())
}
