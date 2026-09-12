use super::registry::{registry, Rule};
use super::runtime::Passive;
use crate::state::combat::prelude::*;

pub(super) fn condition(rule: &Rule, source: &DynamicMessage) -> bool {
    let hp = i64::from(i32_field(source, "hp").unwrap_or(0));
    let maximum = i64::from(i32_field(source, "max_hp").unwrap_or(1).max(1));
    rule.condition.keys().all(|key| {
        matches!(
            key.as_str(),
            "hp_min" | "hp_max" | "target_hp_min" | "target_hp_max" | "target_negative"
        )
    }) && rule
        .condition
        .get("hp_min")
        .is_none_or(|n| hp * 100 >= maximum * i64::from(*n))
        && rule
            .condition
            .get("hp_max")
            .is_none_or(|n| hp * 100 <= maximum * i64::from(*n))
}

pub(super) fn target_condition(rule: &Rule, target: &DynamicMessage) -> bool {
    let hp = i64::from(i32_field(target, "hp").unwrap_or(0));
    let maximum = i64::from(i32_field(target, "max_hp").unwrap_or(1).max(1));
    rule.condition
        .get("target_hp_min")
        .is_none_or(|n| hp * 100 >= maximum * i64::from(*n))
        && rule
            .condition
            .get("target_hp_max")
            .is_none_or(|n| hp * 100 <= maximum * i64::from(*n))
        && rule
            .condition
            .get("target_negative")
            .is_none_or(|required| {
                *required == 0
                    || registry().ok().is_some_and(|rules| {
                        message_list(target, "state_changes").iter().any(|change| {
                            rules
                                .negative_state_ids
                                .contains(&i32_field(change, "state_change_id").unwrap_or_default())
                        })
                    })
            })
}

pub(crate) fn selected(
    rule: &Rule,
    source: &DynamicMessage,
    target: &DynamicMessage,
    targets: &[i32],
) -> bool {
    let id = i32_field(target, "member_id").unwrap_or(0);
    let recipient_matches = match rule.target.as_str() {
        "self" => Some(id) == i32_field(source, "member_id"),
        "allies" => member_type(source).ok() == member_type(target).ok(),
        "enemies" => member_type(source).ok() != member_type(target).ok(),
        "targets" => targets.contains(&id),
        _ => false,
    };
    recipient_matches
        && target_condition(rule, target)
        && (rule.source_character_ids.is_empty()
            || message_i32_field(source, "ally", "character_id")
                .is_some_and(|id| rule.source_character_ids.contains(&id)))
        && (rule.target_character_ids.is_empty()
            || message_i32_field(target, "ally", "character_id")
                .is_some_and(|id| rule.target_character_ids.contains(&id)))
}

pub(super) fn state_application_blocked(
    target: &DynamicMessage,
    state_id: i32,
) -> Result<bool, StateError> {
    let rules = registry()?;
    let immunities = if rules.negative_state_ids.contains(&state_id) {
        &rules.negative_immunity_state_ids
    } else if rules.abnormal_state_ids.contains(&state_id) {
        &rules.abnormal_immunity_state_ids
    } else {
        return Ok(false);
    };
    Ok(message_list(target, "state_changes").iter().any(|change| {
        immunities.contains(&i32_field(change, "state_change_id").unwrap_or_default())
    }))
}

pub(super) fn contextual_rule(rule: &Rule) -> bool {
    rule.trigger.is_some()
        || rule.critical_only
        || !rule.skill_types.is_empty()
        || !rule.attack_attributes.is_empty()
}

pub(super) fn context_matches(
    rule: &Rule,
    source_character_id: i32,
    skill: &TutorialSkill,
    critical: bool,
) -> bool {
    (rule.source_character_ids.is_empty()
        || rule.source_character_ids.contains(&source_character_id))
        && (rule.skill_types.is_empty() || rule.skill_types.contains(&skill.skill_type))
        && (rule.attack_attributes.is_empty()
            || skill
                .attack_attributes
                .iter()
                .any(|attribute| rule.attack_attributes.contains(attribute)))
        && (!rule.critical_only || critical)
}

pub(super) fn amount(rule: &Rule, raw: i32) -> Result<i32, StateError> {
    rule.fixed
        .unwrap_or(raw)
        .checked_mul(rule.sign)
        .ok_or(StateError::InvalidRequest)
}

pub(super) fn contextual_recipient(passive: &Passive, target: &DynamicMessage) -> bool {
    let target_id = i32_field(target, "member_id").unwrap_or(0);
    let target_type = member_type(target).ok();
    let recipient_matches = match passive.rule.target.as_str() {
        "self" => passive.source == target_id,
        "allies" => target_type == Some(passive.source_type),
        "enemies" => target_type.is_some_and(|kind| kind != passive.source_type),
        _ => false,
    };
    recipient_matches
        && (passive.rule.target_character_ids.is_empty()
            || message_i32_field(target, "ally", "character_id")
                .is_some_and(|id| passive.rule.target_character_ids.contains(&id)))
        // Contextual owner-H.P. conditions need the owner's message.  Current
        // compiled contextual rules are unconditional; keep unknown forms off
        // rather than evaluating them against the wrong party member.
        && (passive.rule.condition.is_empty()
            || (passive.source == target_id && condition(&passive.rule, target)))
}
