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
    Attacked,
    Attack,
    Negative,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum NestedActionKind {
    Counter,
    AdditionalAttack,
    SpecialCounter,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct NestedActionRule {
    pub(crate) owner_type: String,
    pub(crate) owner_id: i32,
    #[serde(default)]
    pub(crate) effect_id: i32,
    #[serde(default)]
    pub(crate) source_enemy_ids: Vec<i32>,
    pub(crate) kind: NestedActionKind,
    pub(crate) recipient: String,
    pub(crate) target: String,
    #[serde(default)]
    pub(crate) state_id: i32,
    #[serde(default)]
    pub(crate) duration: i32,
    #[serde(default)]
    pub(crate) skill_id: i32,
    #[serde(default)]
    pub(crate) skill_index: usize,
    #[serde(default = "full_rate")]
    pub(crate) grant_rate: i32,
    #[serde(default = "full_rate")]
    pub(crate) trigger_rate: i32,
    #[serde(default)]
    pub(crate) weak_only: bool,
    #[serde(default)]
    pub(crate) broken_target: bool,
    #[serde(default)]
    pub(crate) weak_or_broken: bool,
    #[serde(default)]
    pub(crate) broken_enemy_or_target: bool,
    #[serde(default)]
    pub(crate) burst_only: bool,
    #[serde(default)]
    pub(crate) noncritical_only: bool,
    #[serde(default)]
    pub(crate) nonweak_only: bool,
    #[serde(default)]
    pub(crate) magic_only: bool,
    #[serde(default)]
    pub(crate) physical_only: bool,
    #[serde(default)]
    pub(crate) single_target_only: bool,
    #[serde(default)]
    pub(crate) other_ally_only: bool,
    #[serde(default)]
    pub(crate) hp_max: i32,
    #[serde(default)]
    pub(crate) required_burst_gauge: i32,
    #[serde(default)]
    pub(crate) burst_gauge_cost: i32,
    #[serde(default)]
    pub(crate) required_state_id: i32,
    #[serde(default)]
    pub(crate) forbidden_state_id: i32,
}

const fn full_rate() -> i32 {
    10_000
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct RuleVariant {
    pub(crate) expiry: Expiry,
    pub(crate) duration: i32,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct LampSkillRule {
    pub(crate) maximum: i32,
    pub(crate) increment: i32,
    pub(crate) clear_when_full: bool,
    pub(crate) mechanic_effect_ids: Vec<i32>,
    #[serde(default)]
    pub(crate) consumed_effect_ids: Vec<i32>,
    pub(crate) full_effect_ids: Vec<i32>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct LampAbilityRule {
    pub(crate) target_skill_ids: Vec<i32>,
    pub(crate) maximum: i32,
    pub(crate) start: i32,
    pub(crate) increment: i32,
    pub(crate) triggers: Vec<String>,
    pub(crate) trigger_skill_ids: Vec<i32>,
    pub(crate) mechanic_effect_ids: Vec<i32>,
    #[serde(default)]
    pub(crate) damage_by_lamp: Vec<i32>,
    #[serde(default)]
    pub(crate) heal_effect_id: i32,
    #[serde(default)]
    pub(crate) heal_value: i32,
    #[serde(default)]
    pub(crate) start_heal: i32,
    #[serde(default)]
    pub(crate) heal_triggers: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Rule {
    pub(crate) id: i32,
    #[serde(default)]
    pub(crate) owner_type: String,
    #[serde(default)]
    pub(crate) owner_id: i32,
    #[serde(default)]
    pub(crate) owner_index: Option<usize>,
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
    pub(crate) source_state_ids: Vec<i32>,
    #[serde(default)]
    pub(crate) affected_state_ids: Vec<i32>,
    #[serde(default)]
    pub(crate) skill_types: Vec<i32>,
    #[serde(default)]
    pub(crate) skill_target_types: Vec<i32>,
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
    pub(crate) panel_limit: usize,
    #[serde(default)]
    pub(crate) stack_cap: i32,
    #[serde(default)]
    pub(crate) source_side_count_min: i32,
    #[serde(default)]
    pub(crate) source_side_count_max: i32,
    #[serde(default)]
    pub(crate) scale_by: String,
    #[serde(default)]
    pub(crate) scale_input_min: i32,
    #[serde(default)]
    pub(crate) scale_input_max: i32,
    #[serde(default)]
    pub(crate) scale_output_max: i32,
    #[serde(default)]
    pub(crate) scale_descending: bool,
    #[serde(default)]
    pub(crate) cover_all: bool,
    pub(crate) positive: bool,
}

#[derive(Deserialize)]
pub(crate) struct Registry {
    pub(crate) source_sha256: String,
    pub(crate) max_party_gauge: i32,
    pub(crate) removable_negative_state_ids: Vec<i32>,
    pub(crate) removable_abnormal_state_ids: Vec<i32>,
    pub(crate) removable_positive_state_ids: Vec<i32>,
    pub(crate) negative_state_ids: Vec<i32>,
    pub(crate) positive_state_ids: Vec<i32>,
    pub(crate) abnormal_state_ids: Vec<i32>,
    pub(crate) negative_immunity_state_ids: Vec<i32>,
    pub(crate) abnormal_immunity_state_ids: Vec<i32>,
    pub(crate) positive_immunity_state_ids: Vec<i32>,
    pub(crate) enhancement_panel_ids: Vec<i32>,
    pub(crate) lamp_skills: BTreeMap<i32, LampSkillRule>,
    pub(crate) lamp_abilities: BTreeMap<i32, LampAbilityRule>,
    pub(crate) status_durations_by_skill: BTreeMap<i32, BTreeMap<i32, i32>>,
    pub(crate) targets_by_skill: BTreeMap<i32, BTreeMap<i32, String>>,
    pub(crate) phases_by_skill: BTreeMap<i32, BTreeMap<i32, String>>,
    pub(crate) rule_variants_by_skill: BTreeMap<i32, BTreeMap<i32, Vec<RuleVariant>>>,
    #[serde(default)]
    pub(crate) nested_actions: Vec<NestedActionRule>,
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

pub(crate) fn rule_for(
    effect_id: i32,
    mode: &str,
    owner_type: &str,
    owner_id: i32,
) -> Result<Option<&'static Rule>, StateError> {
    rule_for_occurrence(effect_id, mode, owner_type, owner_id, None)
}

pub(crate) fn rule_for_occurrence(
    effect_id: i32,
    mode: &str,
    owner_type: &str,
    owner_id: i32,
    owner_index: Option<usize>,
) -> Result<Option<&'static Rule>, StateError> {
    let rules = &registry()?.rules;
    static RULES_BY_EFFECT: OnceLock<BTreeMap<i32, Vec<usize>>> = OnceLock::new();
    let rule_indices = RULES_BY_EFFECT.get_or_init(|| {
        let mut index = BTreeMap::<i32, Vec<usize>>::new();
        for (position, rule) in rules.iter().enumerate() {
            index.entry(rule.id).or_default().push(position);
        }
        index
    });
    let Some(indices) = rule_indices.get(&effect_id) else {
        return Ok(None);
    };
    Ok(owner_index
        .and_then(|owner_index| {
            indices
                .iter()
                .map(|position| &rules[*position])
                .find(|rule| {
                    rule.id == effect_id
                        && rule.mode == mode
                        && rule.owner_type == owner_type
                        && rule.owner_id == owner_id
                        && rule.owner_index == Some(owner_index)
                })
        })
        .or_else(|| {
            indices
                .iter()
                .map(|position| &rules[*position])
                .find(|rule| {
                    rule.id == effect_id
                        && rule.mode == mode
                        && rule.owner_type == owner_type
                        && rule.owner_id == owner_id
                        && rule.owner_index.is_none()
                })
        })
        .or_else(|| {
            indices
                .iter()
                .map(|position| &rules[*position])
                .find(|rule| {
                    rule.id == effect_id
                        && rule.mode == mode
                        && rule.owner_type.is_empty()
                        && rule.owner_id == 0
                })
        }))
}

pub(crate) fn validate(source_hash: &str) -> Result<(), StateError> {
    let data = registry()?;
    let mut ids = BTreeSet::new();
    if data.source_sha256 != source_hash
        || data.rules.iter().any(|r| {
            !ids.insert((
                r.id,
                r.mode.as_str(),
                r.owner_type.as_str(),
                r.owner_id,
                r.owner_index,
            )) || (r.owner_type.is_empty() != (r.owner_id == 0))
                || (r.owner_type.is_empty() && r.owner_index.is_some())
                || (!r.owner_type.is_empty()
                    && !matches!(r.owner_type.as_str(), "skill" | "ability"))
                || !matches!(
                    r.mode.as_str(),
                    "active" | "passive" | "instant" | "catalog"
                )
                || !matches!(
                    r.target.as_str(),
                    "self"
                        | "targets"
                        | "allies"
                        | "enemies"
                        | "next_enemy"
                        | "highest_attack_ally"
                        | "highest_magic_ally"
                        | "all"
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
                        | "evasion"
                        | "abnormal_resistance"
                        | "physical_taken"
                        | "magic_taken"
                        | "taken_down"
                        | "attribute_taken"
                        | "panel_disable"
                        | "panel_convert"
                        | "panel_potency"
                        | "field_effect"
                        | "summons"
                        | "skill_form"
                        | "skill_damage_scale"
                        | "scaling_metadata"
                        | "action_reroll"
                        | "target_rate"
                        | "cover"
                        | "initiative"
                        | "timeline_shift"
                        | "healing"
                        | "healing_received"
                        | "heal"
                        | "party_gauge"
                        | "burst_gauge"
                        | "bomb_gauge"
                        | "break_gauge"
                        | "break_gauge_zero"
                        | "remove_stack"
                        | "cleanse"
                        | "cleanse_abnormal"
                        | "cleanse_positive"
                        | "regeneration"
                        | "negative_immunity"
                        | "negative_potency"
                        | "positive_potency"
                        | "given_negative_potency"
                        | "given_positive_potency"
                        | "damage_immunity"
                        | "abnormal_immunity"
                        | "positive_immunity"
                        | "status"
                )
                || (r.operation == "summary" && !(1..=36).contains(&r.summary))
                || (r.mode == "active"
                    && r.state_id <= 0
                    && !matches!(
                        r.operation.as_str(),
                        "panel_convert"
                            | "field_effect"
                            | "summons"
                            | "skill_form"
                            | "action_reroll"
                            | "timeline_shift"
                            | "heal"
                            | "party_gauge"
                            | "burst_gauge"
                            | "bomb_gauge"
                            | "break_gauge"
                            | "cleanse"
                            | "cleanse_abnormal"
                            | "cleanse_positive"
                    ))
                || (r.operation == "panel_convert"
                    && (r.panel_to_id <= 0 || r.panel_from_ids.is_empty()))
                || (r.operation == "field_effect" && r.fixed.is_none_or(|id| id <= 0))
                || (r.operation == "summons" && r.fixed.is_none_or(|id| id <= 0))
                || (r.operation == "skill_form" && r.fixed.is_some_and(|id| id <= 0))
                || (r.operation == "skill_damage_scale"
                    && (r.mode != "instant"
                        || !matches!(r.summary, 1 | 3)
                        || !matches!(r.scale_by.as_str(), "opponent_count" | "source_hp")
                        || (r.summary == 3 && r.scale_by != "source_hp")
                        || r.scale_input_min < 0
                        || r.scale_input_max < r.scale_input_min
                        || (r.scale_input_max == r.scale_input_min
                            && (r.scale_by != "opponent_count" || r.scale_input_min == 0))
                        || (r.scale_input_max > r.scale_input_min
                            && r.scale_output_max <= 0)))
                || (r.operation == "heal"
                    && !r.scale_by.is_empty()
                    && (r.mode != "active"
                        || r.scale_by != "source_hp"
                        || r.scale_input_min < 0
                        || r.scale_input_max <= r.scale_input_min
                        || r.scale_output_max <= r.fixed.unwrap_or_default()))
                || (!r.scale_by.is_empty()
                    && !matches!(r.operation.as_str(), "skill_damage_scale" | "heal"))
                || r.stack_cap < 0
                || r.source_side_count_min < 0
                || r.source_side_count_max < 0
                || (r.source_side_count_max > 0
                    && r.source_side_count_max < r.source_side_count_min)
                || (r.expiry != Expiry::Permanent && (r.duration <= 0 || r.state_id <= 0))
                || r.target_character_ids.iter().any(|id| *id <= 0)
                || r.source_character_ids.iter().any(|id| *id <= 0)
                || r.source_state_ids.iter().any(|id| *id <= 0)
                || r
                    .affected_state_ids
                    .iter()
                    .any(|id| !data.abnormal_state_ids.contains(id))
                || r.skill_types.iter().any(|id| !matches!(id, 1..=3))
                || r.skill_target_types.iter().any(|id| !matches!(id, 1..=6))
                || r.attack_attributes
                    .iter()
                    .any(|id| !matches!(id, 1..=3 | 5..=8))
                || r.trigger.as_deref().is_some_and(|trigger| {
                    !matches!(
                        trigger,
                        "attack_after"
                            | "action_after"
                            | "party_tool_after"
                            | "battle_start"
                            | "attacked"
                            | "heal_received"
                            | "panel_acquired"
                    )
                })
        })
        || data.max_party_gauge <= 0
        || data.removable_negative_state_ids.is_empty()
        || data.removable_negative_state_ids.iter().any(|id| *id <= 0)
        || data.removable_abnormal_state_ids.is_empty()
        || data.removable_abnormal_state_ids.iter().any(|id| *id <= 0)
        || data.removable_positive_state_ids.is_empty()
        || data.removable_positive_state_ids.iter().any(|id| *id <= 0)
        || data.negative_state_ids.is_empty()
        || data.positive_state_ids.is_empty()
        || data.abnormal_state_ids.is_empty()
        || data.negative_immunity_state_ids.is_empty()
        || data.abnormal_immunity_state_ids.is_empty()
        || data.positive_immunity_state_ids.is_empty()
        || data.enhancement_panel_ids.is_empty()
        || data.negative_state_ids.iter().any(|id| *id <= 0)
        || data.positive_state_ids.iter().any(|id| *id <= 0)
        || data.abnormal_state_ids.iter().any(|id| *id <= 0)
        || data.negative_immunity_state_ids.iter().any(|id| *id <= 0)
        || data.abnormal_immunity_state_ids.iter().any(|id| *id <= 0)
        || data.positive_immunity_state_ids.iter().any(|id| *id <= 0)
        || data.enhancement_panel_ids.iter().any(|id| *id <= 0)
        || data.lamp_skills.iter().any(|(skill_id, rule)| {
            *skill_id <= 0
                || rule.maximum <= 0
                || rule.increment <= 0
                || rule.increment > rule.maximum
                || rule.mechanic_effect_ids.is_empty()
                || rule.mechanic_effect_ids.iter().any(|id| *id <= 0)
                || rule.consumed_effect_ids.iter().any(|id| *id <= 0)
                || rule.full_effect_ids.is_empty()
                || rule.full_effect_ids.iter().any(|id| *id <= 0)
        })
        || data.lamp_abilities.iter().any(|(ability_id, rule)| {
            *ability_id <= 0
                || rule.target_skill_ids.is_empty()
                || rule.target_skill_ids.iter().any(|skill_id| {
                    data.lamp_skills
                        .get(skill_id)
                        .is_none_or(|skill| skill.maximum != rule.maximum)
                })
                || rule.maximum <= 0
                || !(0..=rule.maximum).contains(&rule.start)
                || rule.increment < 0
                || (rule.triggers.is_empty() != (rule.increment == 0))
                || rule.triggers.iter().any(|trigger| {
                    !matches!(
                        trigger.as_str(),
                        "party_tool_after"
                            | "heal_received"
                            | "action_after"
                            | "panel_acquired"
                            | "attacked"
                            | "weak_attack_after"
                            | "abnormal_attack_after"
                    )
                })
                || rule.trigger_skill_ids.iter().any(|id| *id <= 0)
                || rule.mechanic_effect_ids.is_empty()
                || rule.mechanic_effect_ids.iter().any(|id| *id <= 0)
                || (!rule.damage_by_lamp.is_empty()
                    && (rule.damage_by_lamp.len() != rule.maximum as usize
                        || rule.damage_by_lamp.iter().any(|value| *value <= 0)))
                || ((rule.heal_effect_id == 0) != (rule.heal_value == 0))
                || rule.heal_value < 0
                || rule.start_heal < 0
                || rule.start_heal > rule.heal_value
                || rule.heal_triggers.iter().any(|trigger| {
                    !matches!(trigger.as_str(), "action_after" | "party_tool_after")
                        || !rule.triggers.contains(trigger)
                })
        })
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
        || data.nested_actions.iter().any(|rule| {
            !matches!(rule.owner_type.as_str(), "skill" | "ability")
                || rule.owner_id <= 0
                || rule.effect_id < 0
                || rule.source_enemy_ids.iter().any(|id| *id <= 0)
                || !matches!(rule.recipient.as_str(), "self" | "targets")
                || !matches!(
                    rule.target.as_str(),
                    "attacker"
                        | "skill_target"
                        | "lowest_hp_enemy"
                        | "highest_hp_enemy"
                        | "highest_break_enemy"
                )
                || rule.state_id < 0
                || rule.duration == 0
                || rule.skill_id < 0
                || !(0..=10_000).contains(&rule.grant_rate)
                || !(0..=10_000).contains(&rule.trigger_rate)
                || rule.hp_max < 0
                || rule.required_burst_gauge < 0
                || rule.burst_gauge_cost < 0
                || rule.required_state_id < 0
                || rule.forbidden_state_id < 0
                || (rule.owner_type == "skill" && rule.effect_id <= 0)
        })
    {
        return Err(StateError::MasterData(
            "invalid combat effect rules or source hash".into(),
        ));
    }
    Ok(())
}
