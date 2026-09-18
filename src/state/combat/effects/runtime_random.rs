use super::registry::{RandomModifierChoice, Rule};
use super::runtime::{Instance, Runtime};
use super::runtime_match::{selected, state_application_blocked};
use super::runtime_results::{effect_result, status_effect_result};
use crate::state::combat::prelude::*;
use std::collections::BTreeMap;

impl Runtime {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_random_modifier(
        &mut self,
        proto: &ProtoRegistry,
        members: &[DynamicMessage],
        source: &DynamicMessage,
        source_id: i32,
        effect: &TutorialSkillEffect,
        targets: &[i32],
        is_skill: bool,
        rule: &Rule,
        value: i32,
        secret: &[u8],
        transaction: &str,
        action_number: i32,
    ) -> Result<Vec<DynamicMessage>, StateError> {
        let choice_count =
            u32::try_from(rule.random_choices.len()).map_err(|_| StateError::InvalidRequest)?;
        let mut roll_kind = b"random-modifier".to_vec();
        roll_kind.extend_from_slice(&effect.id.to_le_bytes());
        let source_character_id =
            message_i32_field(source, "ally", "character_id").unwrap_or_default();
        let mut results = Vec::new();

        for target in members.iter().filter(|target| {
            bool_field(target, "is_alive") && selected(rule, source, target, targets)
        }) {
            let target_id = member_id(target)?;
            let mut totals = BTreeMap::<usize, i32>::new();
            for draw in 0..rule.random_draws {
                let draw = u32::try_from(draw).map_err(|_| StateError::InvalidRequest)?;
                let choice_index = usize::try_from(
                    deterministic_roll(
                        secret,
                        transaction,
                        action_number,
                        &roll_kind,
                        target_id,
                        draw,
                    ) % choice_count,
                )
                .map_err(|_| StateError::InvalidRequest)?;
                let choice_rule = choice_rule(rule, &rule.random_choices[choice_index]);
                if state_application_blocked(target, &choice_rule)? {
                    results.push(status_effect_result(
                        proto,
                        effect.id,
                        source_id,
                        target_id,
                        is_skill,
                        &choice_rule,
                        value,
                        4,
                    )?);
                    continue;
                }
                let value = self.apply_potency(members, source, target_id, &choice_rule, value)?;
                let total = totals.entry(choice_index).or_default();
                *total = total.checked_add(value).ok_or(StateError::InvalidRequest)?;
                results.push(effect_result(
                    proto,
                    effect.id,
                    source_id,
                    target_id,
                    is_skill,
                    &choice_rule,
                    value,
                )?);
            }

            for (choice_index, value) in totals {
                let choice_rule = choice_rule(rule, &rule.random_choices[choice_index]);
                self.instances.retain(|instance| {
                    instance.source != source_id
                        || instance.target != target_id
                        || instance.rule.id != rule.id
                        || instance.rule.state_id != choice_rule.state_id
                });
                self.instances.push(Instance {
                    source: source_id,
                    source_character_id,
                    target: target_id,
                    value,
                    remaining: choice_rule.duration,
                    rule: choice_rule.clone(),
                });
                self.managed
                    .entry(target_id)
                    .or_default()
                    .insert(choice_rule.state_id);
            }
        }
        Ok(results)
    }
}

fn choice_rule(base: &Rule, choice: &RandomModifierChoice) -> Rule {
    let mut rule = base.clone();
    rule.operation.clone_from(&choice.operation);
    rule.summary = choice.summary;
    rule.state_id = choice.state_id;
    rule.expiry = choice.expiry.clone();
    rule.duration = choice.duration;
    rule.positive = choice.positive;
    rule.random_draws = 0;
    rule.random_choices.clear();
    rule
}
