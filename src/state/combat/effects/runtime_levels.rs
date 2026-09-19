use super::registry::Rule;
use super::runtime::{Instance, Runtime};
use super::runtime_match::{selected, state_application_blocked};
use super::runtime_results::{effect_result, status_effect_result};
use crate::state::combat::prelude::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_level_state(
    runtime: &mut Runtime,
    proto: &ProtoRegistry,
    source: &DynamicMessage,
    members: &[DynamicMessage],
    source_id: i32,
    effect: &TutorialSkillEffect,
    targets: &[i32],
    is_skill: bool,
    rule: &Rule,
    increment: i32,
) -> Result<Vec<DynamicMessage>, StateError> {
    let mut results = Vec::new();
    for target in members
        .iter()
        .filter(|target| bool_field(target, "is_alive") && selected(rule, source, target, targets))
    {
        let target_id = member_id(target)?;
        if state_application_blocked(target, rule)? {
            results.push(status_effect_result(
                proto, effect.id, source_id, target_id, is_skill, rule, increment, 4,
            )?);
            continue;
        }
        let index = runtime.instances.iter().position(|instance| {
            instance.target == target_id
                && instance.rule.operation == "level_state"
                && instance.rule.state_id == rule.state_id
        });
        let level = index
            .map_or(increment, |index| {
                runtime.instances[index].value.saturating_add(increment)
            })
            .min(rule.stack_cap);
        let instance = Instance {
            source: source_id,
            source_character_id: message_i32_field(source, "ally", "character_id")
                .unwrap_or_default(),
            target: target_id,
            value: level,
            remaining: rule.duration,
            rule: rule.clone(),
        };
        if let Some(index) = index {
            runtime.instances[index] = instance;
        } else {
            runtime.instances.push(instance);
        }
        runtime
            .managed
            .entry(target_id)
            .or_default()
            .insert(rule.state_id);
        results.push(effect_result(
            proto, effect.id, source_id, target_id, is_skill, rule, level,
        )?);
    }
    Ok(results)
}
