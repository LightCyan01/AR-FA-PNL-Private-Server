use super::runtime::{Instance, Passive, Runtime};
use super::runtime_match::{condition, selected_for_source_character, state_application_blocked};
use super::runtime_results::{effect_result, status_effect_result};
use crate::state::combat::prelude::*;
use std::collections::BTreeSet;

impl Runtime {
    pub(super) fn grant_triggered_modifier(
        &mut self,
        proto: &ProtoRegistry,
        passive: &Passive,
        source: &DynamicMessage,
        members: &[DynamicMessage],
        target_ids: &[i32],
    ) -> Result<Vec<DynamicMessage>, StateError> {
        let mut results = Vec::new();
        for target_id in target_ids.iter().copied() {
            let Some(target) = members
                .iter()
                .find(|member| member_id(member).ok() == Some(target_id))
            else {
                continue;
            };
            if !selected_for_source_character(
                &passive.rule,
                source,
                passive.source_character_id,
                target,
                target_ids,
            ) {
                continue;
            }
            if state_application_blocked(target, &passive.rule)? {
                results.push(status_effect_result(
                    proto,
                    passive.rule.id,
                    passive.source,
                    target_id,
                    true,
                    &passive.rule,
                    passive.value,
                    4,
                )?);
                continue;
            }
            let value =
                self.apply_potency(members, source, target_id, &passive.rule, passive.value)?;
            let existing = self
                .instances
                .iter()
                .filter(|instance| {
                    instance.source == passive.source
                        && instance.target == target_id
                        && instance.rule.id == passive.rule.id
                })
                .count();
            if passive.rule.stack_limit > 1 && existing >= passive.rule.stack_limit {
                continue;
            }
            if passive.rule.stack_limit <= 1 {
                self.instances.retain(|instance| {
                    !(instance.source == passive.source
                        && instance.target == target_id
                        && instance.rule.id == passive.rule.id)
                });
            }
            self.instances.push(Instance {
                source: passive.source,
                source_character_id: passive.source_character_id,
                target: target_id,
                value,
                remaining: passive.rule.duration,
                rule: passive.rule.clone(),
            });
            self.managed
                .entry(target_id)
                .or_default()
                .insert(passive.rule.state_id);
            results.push(effect_result(
                proto,
                passive.rule.id,
                passive.source,
                target_id,
                true,
                &passive.rule,
                value,
            )?);
        }
        Ok(results)
    }

    pub(crate) fn trigger_attack_before(
        &mut self,
        proto: &ProtoRegistry,
        state: &mut DynamicMessage,
        source_id: i32,
        skill: &TutorialSkill,
        targets: &[i32],
    ) -> Result<Vec<DynamicMessage>, StateError> {
        if skill.skill_effect_type != 1 {
            return Ok(Vec::new());
        }
        let members = message_list(state, "members");
        let source = members
            .iter()
            .find(|member| member_id(member).ok() == Some(source_id))
            .ok_or(StateError::InvalidRequest)?;
        if !bool_field(source, "is_alive") {
            return Ok(Vec::new());
        }
        let mut results = Vec::new();
        let mut changed = false;
        let mut applied = BTreeSet::new();
        for passive in self.passives.clone().into_iter().filter(|passive| {
            passive.source == source_id && passive.rule.trigger.as_deref() == Some("attack_before")
        }) {
            let usage_key = if passive.rule.owner_id == 0 {
                passive.rule.id
            } else {
                passive.rule.owner_id
            };
            if !applied.insert((passive.source, usage_key, passive.rule.id))
                || !condition(&passive.rule, source)
                || (!passive.rule.source_character_ids.is_empty()
                    && !passive
                        .rule
                        .source_character_ids
                        .contains(&passive.source_character_id))
                || (passive.rule.trigger_limit > 0
                    && self
                        .limited_effect_uses
                        .get(&passive.source)
                        .and_then(|uses| uses.get(&usage_key))
                        .is_some_and(|uses| *uses >= passive.rule.trigger_limit))
            {
                continue;
            }
            let mut triggered = false;
            for target in members.iter().filter(|target| {
                bool_field(target, "is_alive")
                    && selected_for_source_character(
                        &passive.rule,
                        source,
                        passive.source_character_id,
                        target,
                        targets,
                    )
            }) {
                let target_id = member_id(target)?;
                triggered = true;
                if state_application_blocked(target, &passive.rule)? {
                    results.push(status_effect_result(
                        proto,
                        passive.rule.id,
                        source_id,
                        target_id,
                        false,
                        &passive.rule,
                        passive.value,
                        4,
                    )?);
                    continue;
                }
                let value =
                    self.apply_potency(&members, source, target_id, &passive.rule, passive.value)?;
                self.instances.retain(|instance| {
                    !(instance.source == source_id
                        && instance.target == target_id
                        && instance.rule.id == passive.rule.id)
                });
                self.instances.push(Instance {
                    source: source_id,
                    source_character_id: passive.source_character_id,
                    target: target_id,
                    value,
                    remaining: passive.rule.duration,
                    rule: passive.rule.clone(),
                });
                changed = true;
                self.managed
                    .entry(target_id)
                    .or_default()
                    .insert(passive.rule.state_id);
                results.push(effect_result(
                    proto,
                    passive.rule.id,
                    source_id,
                    target_id,
                    false,
                    &passive.rule,
                    value,
                )?);
            }
            if triggered {
                let uses = self
                    .limited_effect_uses
                    .entry(passive.source)
                    .or_default()
                    .entry(usage_key)
                    .or_default();
                *uses = uses.checked_add(1).ok_or(StateError::InvalidRequest)?;
            }
        }
        if changed {
            self.refresh(proto, state)?;
        }
        Ok(results)
    }
}
