use super::registry::rule_for;
use crate::state::combat::prelude::*;

pub(crate) fn scaled_skill_damage(
    skill: &TutorialSkill,
    source: &DynamicMessage,
    opponent_count: i32,
) -> Result<i64, StateError> {
    scaled_skill_modifier(skill, source, opponent_count, 1)
}

pub(crate) fn scaled_break_damage(
    skill: &TutorialSkill,
    source: &DynamicMessage,
) -> Result<i64, StateError> {
    scaled_skill_modifier(skill, source, 0, 3)
}

fn scaled_skill_modifier(
    skill: &TutorialSkill,
    source: &DynamicMessage,
    opponent_count: i32,
    summary: i32,
) -> Result<i64, StateError> {
    skill.effects.iter().try_fold(0i64, |total, effect| {
        let Some(rule) = rule_for(effect.id, "instant", "skill", skill.id)? else {
            return Ok(total);
        };
        if rule.operation != "skill_damage_scale" || rule.summary != summary {
            return Ok(total);
        }
        let (input, denominator) = match rule.scale_by.as_str() {
            "opponent_count" => (i64::from(opponent_count), 1),
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
        let minimum_output = i64::from(effect.value);
        let value =
            minimum_output + (i64::from(rule.scale_output_max) - minimum_output) * progress / span;
        Ok(total.saturating_add(value))
    })
}
