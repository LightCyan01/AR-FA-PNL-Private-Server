use super::registry::{rule_for, Rule};
use crate::state::combat::prelude::*;

pub(crate) fn scaled_skill_damage(
    skill: &TutorialSkill,
    source: &DynamicMessage,
    opponent_count: i32,
) -> Result<i64, StateError> {
    scaled_skill_modifier(skill, source, opponent_count, 1, None)
}

pub(crate) fn scaled_break_damage(
    skill: &TutorialSkill,
    source: &DynamicMessage,
    party_context: Option<(&TutorialRules, &[DynamicMessage])>,
) -> Result<i64, StateError> {
    scaled_skill_modifier(skill, source, 0, 3, party_context)
}

pub(crate) fn scaled_critical_damage(
    skill: &TutorialSkill,
    source: &DynamicMessage,
    opponent_count: i32,
) -> Result<i64, StateError> {
    scaled_skill_modifier(skill, source, opponent_count, 7, None)
}

pub(super) fn party_tag_count(
    rules: &TutorialRules,
    members: &[DynamicMessage],
    source: &DynamicMessage,
    tag_id: i32,
) -> Result<i32, StateError> {
    let source_type = member_type(source)?;
    i32::try_from(
        members
            .iter()
            .filter(|member| {
                member_type(member).ok() == Some(source_type)
                    && message_i32_field(member, "ally", "character_id")
                        .and_then(|id| rule_character(rules, id).ok())
                        .is_some_and(|character| character.tag_ids.contains(&tag_id))
            })
            .count(),
    )
    .map_err(|_| StateError::InvalidRequest)
}

pub(super) fn scaled_effect_value(
    rule: &Rule,
    source: &DynamicMessage,
    scale_count: i32,
    minimum_output: i32,
) -> Result<i32, StateError> {
    let (input, denominator) = match rule.scale_by.as_str() {
        "opponent_count" | "party_tag_count" => (i64::from(scale_count), 1),
        "source_hp" => (
            i64::from(i32_field(source, "hp").unwrap_or_default().max(0)) * 100,
            i64::from(i32_field(source, "max_hp").unwrap_or(1).max(1)),
        ),
        _ => return Err(StateError::InvalidRequest),
    };
    let minimum = i64::from(rule.scale_input_min) * denominator;
    let span = i64::from(rule.scale_input_max - rule.scale_input_min) * denominator;
    let mut progress = input.clamp(minimum, minimum + span) - minimum;
    if rule.scale_descending {
        progress = span - progress;
    }
    i32::try_from(
        i64::from(minimum_output)
            + i64::from(rule.scale_output_max - minimum_output) * progress / span,
    )
    .map_err(|_| StateError::InvalidRequest)
}

fn scaled_skill_modifier(
    skill: &TutorialSkill,
    source: &DynamicMessage,
    opponent_count: i32,
    summary: i32,
    party_context: Option<(&TutorialRules, &[DynamicMessage])>,
) -> Result<i64, StateError> {
    skill.effects.iter().try_fold(0i64, |total, effect| {
        let Some(rule) = rule_for(effect.id, "instant", "skill", skill.id)? else {
            return Ok(total);
        };
        if rule.operation != "skill_damage_scale" || rule.summary != summary {
            return Ok(total);
        }
        if rule.scale_by == "opponent_count"
            && rule.scale_input_min == rule.scale_input_max
        {
            return Ok(if opponent_count == rule.scale_input_min {
                total.saturating_add(i64::from(effect.value))
            } else {
                total
            });
        }
        let (input, denominator) = match rule.scale_by.as_str() {
            "opponent_count" => (i64::from(opponent_count), 1),
            "party_tag_count" => {
                let (rules, members) = party_context.ok_or(StateError::InvalidRequest)?;
                let tag_id = *rule
                    .condition
                    .get("party_tag_id")
                    .ok_or(StateError::InvalidRequest)?;
                (i64::from(party_tag_count(rules, members, source, tag_id)?), 1)
            }
            "source_hp" => (
                i64::from(i32_field(source, "hp").unwrap_or_default().max(0)) * 100,
                i64::from(i32_field(source, "max_hp").unwrap_or(1).max(1)),
            ),
            _ => return Err(StateError::InvalidRequest),
        };
        let minimum = i64::from(rule.scale_input_min) * denominator;
        let span = i64::from(rule.scale_input_max - rule.scale_input_min) * denominator;
        let mut progress = input.clamp(minimum, minimum + span) - minimum;
        if rule.scale_descending {
            progress = span - progress;
        }
        let minimum_output = i64::from(rule.fixed.unwrap_or(effect.value));
        let value =
            minimum_output + (i64::from(rule.scale_output_max) - minimum_output) * progress / span;
        Ok(total.saturating_add(value))
    })
}
