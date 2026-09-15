use super::registry::registry;
use super::runtime::Runtime;
use super::runtime_summary::instant_summary_for_source;
use crate::state::combat::prelude::*;

// Shared damage/healing policies consume the runtime's derived summaries.
fn healing_bonus(member: &DynamicMessage, operation: &str) -> Result<i64, StateError> {
    let rules = &registry()?.rules;
    message_list(member, "state_changes")
        .iter()
        .filter(|change| {
            let state_id = i32_field(change, "state_change_id").unwrap_or_default();
            rules
                .iter()
                .any(|rule| rule.state_id == state_id && rule.operation == operation)
        })
        .try_fold(0i64, |total, change| {
            total
                .checked_add(i64::from(i32_field(change, "value").unwrap_or_default()))
                .ok_or(StateError::InvalidRequest)
        })
}

pub(crate) fn healing_amount(
    base: i32,
    source: &DynamicMessage,
    target: &DynamicMessage,
) -> Result<i32, StateError> {
    let bonus = healing_bonus(source, "healing")?
        .checked_add(healing_bonus(target, "healing_received")?)
        .ok_or(StateError::InvalidRequest)?;
    let value = i64::from(base.max(0)) * (10_000 + bonus).clamp(0, 1_000_000) / 10_000;
    i32::try_from(value).map_err(|_| StateError::InvalidRequest)
}

pub(crate) fn incoming_multiplier_with_runtime(
    target: &DynamicMessage,
    attribute: i32,
    runtime: Option<&Runtime>,
) -> i64 {
    let target_id = i32_field(target, "member_id").unwrap_or_default();
    if runtime.is_some_and(|runtime| runtime.damage_immunity(target_id, attribute)) {
        return 0;
    }
    let physical = (1..=3).contains(&attribute);
    let state_rate: i64 = message_list(target, "state_changes")
        .iter()
        .map(|change| {
            let state_id = i32_field(change, "state_change_id").unwrap_or(0);
            let value = i64::from(i32_field(change, "value").unwrap_or(0));
            match registry()
                .ok()
                .and_then(|registry| {
                    registry.rules.iter().find(|rule| {
                        rule.state_id == state_id
                            && (rule.attack_attributes.is_empty()
                                || rule.attack_attributes.contains(&attribute))
                            && matches!(
                                rule.operation.as_str(),
                                "physical_taken" | "magic_taken" | "taken_down" | "attribute_taken"
                            )
                    })
                })
                .map(|rule| rule.operation.as_str())
            {
                Some("physical_taken") if physical => value,
                Some("magic_taken") if !physical => value,
                Some("taken_down") => -value,
                Some("attribute_taken") => value,
                _ => 0,
            }
        })
        .sum();
    (10_000
        + state_rate
        + runtime
            .and_then(|runtime| runtime.panel_damage_taken.get(&target_id))
            .copied()
            .map(i64::from)
            .unwrap_or(0)
        + i64::from(state_change_summary_value(target, 11))
        - i64::from(state_change_summary_value(target, 12)))
    .clamp(0, 1_000_000)
}

pub(crate) fn incoming_multiplier_for_skill(
    target: &DynamicMessage,
    skill: &TutorialSkill,
    runtime: Option<&Runtime>,
    critical: bool,
) -> Result<i64, StateError> {
    Ok((incoming_multiplier_with_runtime(
        target,
        preferred_attack_attribute(target, skill)?,
        runtime,
    ) + runtime.map_or(0, |runtime| {
        runtime.contextual_summary(target, skill, critical, 11)
            - runtime.contextual_summary(target, skill, critical, 12)
    }))
    .clamp(0, 1_000_000))
}

/// Convert the protocol's hundredths-of-a-percent penetration value into the
/// client damage factor: (10x + 3000) / (3x + 3000), where x is percent.
pub(crate) fn penetration_factor(raw: i64) -> (i128, i128) {
    let penetration = i128::from(raw.max(0));
    (10 * penetration + 300_000, 3 * penetration + 300_000)
}

/// Exact integer composition of the explicitly supported power/critical/taken
/// buckets. The base skill-damage bucket is already included by the caller.
pub(crate) fn secondary_damage(
    base: i64,
    source: &DynamicMessage,
    target: &DynamicMessage,
    skill: &TutorialSkill,
    runtime: Option<&Runtime>,
    critical: bool,
) -> Result<i64, StateError> {
    let contextual = |summary| {
        runtime.map_or(0, |runtime| {
            runtime.contextual_summary_against(source, Some(target), skill, critical, summary)
        })
    };
    let power = (10_000i64
        + i64::from(state_change_summary_value(source, 4))
        + contextual(4)
        + instant_summary_for_source(Some(source), skill, target, critical, 4)?
        - i64::from(state_change_summary_value(source, 5))
        - contextual(5))
    .clamp(0, 1_000_000);
    let crit = if critical {
        (15_000i64
            + i64::from(state_change_summary_value(source, 7))
            + contextual(7)
            + instant_summary_for_source(Some(source), skill, target, critical, 7)?
            + i64::from(state_change_summary_value(target, 17)))
        .clamp(0, 1_000_000)
    } else {
        15_000
    };
    let incoming = incoming_multiplier_for_skill(target, skill, runtime, critical)?;
    let penetration = (i64::from(state_change_summary_value(source, 10))
        + contextual(10)
        + instant_summary_for_source(Some(source), skill, target, critical, 10)?
        + i64::from(state_change_summary_value(target, 19))
        + runtime.map_or(0, |runtime| {
            runtime.contextual_summary(target, skill, critical, 19)
        }))
    .max(0);
    let (penetration_numerator, penetration_denominator) = penetration_factor(penetration);
    Ok((i128::from(base)
        * i128::from(power)
        * i128::from(incoming)
        * i128::from(crit)
        * penetration_numerator
        / (10_000i128 * 10_000 * 15_000 * penetration_denominator))
        .clamp(0, 9_999_999_999) as i64)
}
