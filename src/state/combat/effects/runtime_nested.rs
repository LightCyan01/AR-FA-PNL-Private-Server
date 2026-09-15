use super::registry::{registry, NestedActionKind, NestedActionRule};
use super::runtime::{NestedActionInstance, PendingAction, Runtime};
use super::runtime_results::nested_effect_result;
use crate::state::combat::prelude::*;
use std::collections::BTreeSet;

impl Runtime {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_nested_grants(
        &mut self,
        proto: &ProtoRegistry,
        members: &[DynamicMessage],
        source_id: i32,
        skill_id: i32,
        effect: &TutorialSkillEffect,
        targets: &[i32],
        is_skill: bool,
        secret: &[u8],
        transaction: &str,
        action_number: i32,
        effect_index: usize,
    ) -> Result<Vec<DynamicMessage>, StateError> {
        let source_enemy_id = members
            .iter()
            .find(|member| i32_field(member, "member_id") == Some(source_id))
            .and_then(|member| enemy_member_status_enemy_id(member).ok());
        let rules = registry()?
            .nested_actions
            .iter()
            .filter(|rule| {
                rule.owner_type == "skill"
                    && rule.owner_id == skill_id
                    && rule.effect_id == effect.id
                    && (rule.source_enemy_ids.is_empty()
                        || source_enemy_id.is_some_and(|id| rule.source_enemy_ids.contains(&id)))
            })
            .cloned()
            .collect::<Vec<_>>();
        let mut results = Vec::new();
        for rule in rules {
            let recipients = match rule.recipient.as_str() {
                "self" => vec![source_id],
                "targets" => targets.to_vec(),
                _ => return Err(StateError::InvalidRequest),
            };
            for target_id in recipients.into_iter().collect::<BTreeSet<_>>() {
                if !members.iter().any(|member| {
                    i32_field(member, "member_id") == Some(target_id)
                        && bool_field(member, "is_alive")
                }) {
                    continue;
                }
                let succeeded = rule.grant_rate == 10_000
                    || deterministic_roll(
                        secret,
                        transaction,
                        action_number,
                        b"nested-grant",
                        target_id,
                        u32::try_from(effect_index).unwrap_or(u32::MAX),
                    ) % 10_000
                        < rule.grant_rate as u32;
                if succeeded {
                    self.nested_actions.retain(|instance| {
                        !(instance.source == source_id
                            && instance.target == target_id
                            && instance.rule.owner_type == rule.owner_type
                            && instance.rule.owner_id == rule.owner_id
                            && instance.rule.effect_id == rule.effect_id
                            && instance.rule.kind == rule.kind)
                    });
                    self.nested_actions.push(NestedActionInstance {
                        source: source_id,
                        target: target_id,
                        remaining: rule.duration,
                        rule: rule.clone(),
                    });
                    if rule.state_id > 0 {
                        self.managed
                            .entry(target_id)
                            .or_default()
                            .insert(rule.state_id);
                    }
                }
                results.push(nested_effect_result(
                    proto,
                    effect.id,
                    source_id,
                    target_id,
                    is_skill,
                    &rule,
                    if succeeded { 1 } else { 2 },
                )?);
            }
        }
        Ok(results)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn collect_special_counter(
        &mut self,
        rules: &TutorialRules,
        state: &mut DynamicMessage,
        actor_id: i32,
        skill: &TutorialSkill,
        target_ids: &[i32],
        secret: &[u8],
        transaction: &str,
        action_number: i32,
    ) -> Result<Option<PendingAction>, StateError> {
        let members = message_list(state, "members");
        let actor = members
            .iter()
            .find(|member| i32_field(member, "member_id") == Some(actor_id))
            .ok_or(StateError::InvalidRequest)?;
        let actor_type = member_type(actor)?;
        let mut candidates = self
            .nested_actions
            .iter()
            .enumerate()
            .filter(|(_, instance)| {
                instance.rule.kind == NestedActionKind::SpecialCounter && instance.remaining != 0
            })
            .filter_map(|(index, instance)| {
                let owner = members
                    .iter()
                    .find(|member| i32_field(member, "member_id") == Some(instance.target))?;
                let owner_type = member_type(owner).ok()?;
                (bool_field(owner, "is_alive")
                    && owner_type != actor_type
                    && target_ids.iter().any(|target_id| {
                        members.iter().any(|member| {
                            i32_field(member, "member_id") == Some(*target_id)
                                && member_type(member).ok() == Some(owner_type)
                        })
                    })
                    && skill_conditions(&instance.rule, owner, skill, target_ids.len() == 1))
                .then_some((index, instance.clone(), owner.clone()))
            })
            .collect::<Vec<_>>();
        candidates.sort_by_key(|(_, instance, _)| instance.target);
        for (index, instance, owner) in candidates {
            let rate_hit = instance.rule.trigger_rate == 10_000
                || deterministic_roll(
                    secret,
                    transaction,
                    action_number,
                    b"special-counter",
                    instance.target,
                    u32::try_from(index).unwrap_or(u32::MAX),
                ) % 10_000
                    < instance.rule.trigger_rate as u32;
            if !rate_hit {
                continue;
            }
            let pending = PendingAction {
                actor_id: instance.target,
                skill_id: nested_skill_id(rules, &instance.rule, &owner)?,
                target_id: actor_id,
                kind: NestedActionKind::SpecialCounter,
                burst_gauge_cost: instance.rule.burst_gauge_cost,
            };
            consume_instance(&mut self.nested_actions, index);
            spend_burst_gauges(state, &[pending])?;
            return Ok(Some(pending));
        }
        Ok(None)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn collect_nested_actions(
        &mut self,
        rules: &TutorialRules,
        state: &mut DynamicMessage,
        actor_id: i32,
        skill: &TutorialSkill,
        results: &[DynamicMessage],
        secret: &[u8],
        transaction: &str,
        action_number: i32,
    ) -> Result<Vec<PendingAction>, StateError> {
        let members = message_list(state, "members");
        let actor = members
            .iter()
            .find(|member| i32_field(member, "member_id") == Some(actor_id))
            .ok_or(StateError::InvalidRequest)?;
        let actor_type = member_type(actor)?;
        let hit_results = results
            .iter()
            .filter(|result| !bool_field(result, "is_miss") && !bool_field(result, "is_invalid"))
            .collect::<Vec<_>>();
        let hit_ids = hit_results
            .iter()
            .filter_map(|result| i32_field(result, "target_id"))
            .collect::<BTreeSet<_>>();
        let target_broken = hit_ids.iter().any(|target_id| {
            members.iter().any(|member| {
                i32_field(member, "member_id") == Some(*target_id)
                    && member_status(member, "enemy")
                        .ok()
                        .is_some_and(|enemy| bool_field(&enemy, "is_broken"))
            })
        });
        let broken_enemy = members.iter().any(|member| {
            member_type(member)
                .ok()
                .is_some_and(|kind| kind != actor_type)
                && bool_field(member, "is_alive")
                && member_status(member, "enemy")
                    .ok()
                    .is_some_and(|enemy| bool_field(&enemy, "is_broken"))
        });
        let any_weak = hit_results
            .iter()
            .any(|result| bool_field(result, "is_weak"));
        let single_target = hit_ids.len() == 1;
        let instances = self.nested_actions.clone();
        let mut triggered = Vec::new();
        let mut consumed = BTreeSet::new();
        for kind in [
            NestedActionKind::Counter,
            NestedActionKind::AdditionalAttack,
        ] {
            for (index, instance) in instances.iter().enumerate() {
                if instance.rule.kind != kind || instance.remaining == 0 {
                    continue;
                }
                let Some(owner) = members
                    .iter()
                    .find(|member| i32_field(member, "member_id") == Some(instance.target))
                else {
                    continue;
                };
                if !bool_field(owner, "is_alive") {
                    continue;
                }
                let owner_type = member_type(owner)?;
                let is_counter = kind == NestedActionKind::Counter;
                let actor_matches = if is_counter {
                    skill.skill_effect_type == 1
                        && owner_type != actor_type
                        && hit_ids.contains(&instance.target)
                } else if instance.rule.other_ally_only {
                    owner_type == actor_type && instance.target != actor_id
                } else {
                    instance.target == actor_id
                };
                let counter_result = if is_counter {
                    hit_results
                        .iter()
                        .copied()
                        .find(|result| i32_field(result, "target_id") == Some(instance.target))
                } else {
                    None
                };
                let weak = counter_result.map_or(any_weak, |result| bool_field(result, "is_weak"));
                let critical =
                    counter_result.is_some_and(|result| bool_field(result, "is_critical"));
                if !actor_matches
                    || !skill_conditions(&instance.rule, owner, skill, single_target)
                    || (instance.rule.noncritical_only && critical)
                    || (instance.rule.nonweak_only && weak)
                    || (instance.rule.weak_only && !weak)
                    || (instance.rule.broken_target && !target_broken)
                    || (instance.rule.weak_or_broken && !any_weak && !target_broken)
                    || (instance.rule.broken_enemy_or_target && !broken_enemy && !target_broken)
                {
                    continue;
                }
                let rate_hit = instance.rule.trigger_rate == 10_000
                    || deterministic_roll(
                        secret,
                        transaction,
                        action_number,
                        b"nested-trigger",
                        instance.target,
                        u32::try_from(index).unwrap_or(u32::MAX),
                    ) % 10_000
                        < instance.rule.trigger_rate as u32;
                if !rate_hit {
                    continue;
                }
                let skill_id = nested_skill_id(rules, &instance.rule, owner)?;
                let target_id =
                    nested_target_id(&instance.rule, &members, actor_id, &hit_ids, owner_type)?;
                let pending = PendingAction {
                    actor_id: instance.target,
                    skill_id,
                    target_id,
                    kind,
                    burst_gauge_cost: instance.rule.burst_gauge_cost,
                };
                if !triggered.contains(&pending) {
                    triggered.push(pending);
                }
                consumed.insert(index);
            }
        }
        for index in consumed.into_iter().rev() {
            consume_instance(&mut self.nested_actions, index);
        }
        spend_burst_gauges(state, &triggered)?;
        Ok(triggered)
    }
}

fn hp_matches(member: &DynamicMessage, maximum_percent: i32) -> bool {
    if maximum_percent <= 0 {
        return true;
    }
    let hp = i64::from(i32_field(member, "hp").unwrap_or_default().max(0));
    let maximum = i64::from(i32_field(member, "max_hp").unwrap_or(1).max(1));
    hp.saturating_mul(100) <= maximum.saturating_mul(i64::from(maximum_percent))
}

fn gauge_matches(member: &DynamicMessage, rule: &NestedActionRule) -> bool {
    rule.required_burst_gauge <= 0
        || member_status(member, "burst_gauge")
            .ok()
            .and_then(|gauge| i32_field(&gauge, "current_gauge"))
            .unwrap_or_default()
            >= rule.required_burst_gauge
}

fn skill_conditions(
    rule: &NestedActionRule,
    owner: &DynamicMessage,
    skill: &TutorialSkill,
    single_target: bool,
) -> bool {
    let physical = skill
        .attack_attributes
        .iter()
        .any(|attribute| (1..=3).contains(attribute));
    let magic = skill
        .attack_attributes
        .iter()
        .any(|attribute| (5..=8).contains(attribute));
    (!rule.burst_only || skill.skill_type == 3)
        && (!rule.magic_only || magic)
        && (!rule.physical_only || physical)
        && (!rule.single_target_only || single_target)
        && hp_matches(owner, rule.hp_max)
        && gauge_matches(owner, rule)
        && state_matches(owner, rule)
}

fn state_matches(member: &DynamicMessage, rule: &NestedActionRule) -> bool {
    let states = message_list(member, "state_changes");
    (rule.required_state_id == 0
        || states
            .iter()
            .any(|state| i32_field(state, "state_change_id") == Some(rule.required_state_id)))
        && (rule.forbidden_state_id == 0
            || states
                .iter()
                .all(|state| i32_field(state, "state_change_id") != Some(rule.forbidden_state_id)))
}

fn consume_instance(instances: &mut Vec<NestedActionInstance>, index: usize) {
    if let Some(instance) = instances.get_mut(index) {
        if instance.remaining > 0 {
            instance.remaining -= 1;
        }
    }
    instances.retain(|instance| instance.remaining != 0);
}

fn nested_skill_id(
    rules: &TutorialRules,
    rule: &NestedActionRule,
    owner: &DynamicMessage,
) -> Result<i32, StateError> {
    if rule.skill_id > 0 {
        return Ok(rule.skill_id);
    }
    let ids = if let Some(character_id) = message_i32_field(owner, "ally", "character_id") {
        rule_character(rules, character_id)?.extra_skill_ids.clone()
    } else {
        rule_enemy(rules, enemy_member_status_enemy_id(owner)?)?
            .extra_skill_ids
            .clone()
    };
    ids.get(rule.skill_index)
        .copied()
        .filter(|id| *id > 0)
        .ok_or(StateError::InvalidRequest)
}

fn nested_target_id(
    rule: &NestedActionRule,
    members: &[DynamicMessage],
    attacker_id: i32,
    hit_ids: &BTreeSet<i32>,
    owner_type: i32,
) -> Result<i32, StateError> {
    let enemies = || {
        members.iter().filter(|member| {
            member_type(member)
                .ok()
                .is_some_and(|kind| kind != owner_type)
                && bool_field(member, "is_alive")
        })
    };
    match rule.target.as_str() {
        "attacker" => Ok(attacker_id),
        "skill_target" => hit_ids
            .iter()
            .copied()
            .next()
            .ok_or(StateError::InvalidRequest),
        "lowest_hp_enemy" => enemies()
            .min_by_key(|member| {
                (
                    i32_field(member, "hp").unwrap_or(i32::MAX),
                    i32_field(member, "member_id").unwrap_or(i32::MAX),
                )
            })
            .and_then(|member| i32_field(member, "member_id"))
            .ok_or(StateError::InvalidRequest),
        "highest_hp_enemy" => enemies()
            .max_by_key(|member| {
                (
                    i32_field(member, "hp").unwrap_or_default(),
                    i32_field(member, "member_id")
                        .unwrap_or_default()
                        .saturating_neg(),
                )
            })
            .and_then(|member| i32_field(member, "member_id"))
            .ok_or(StateError::InvalidRequest),
        "highest_break_enemy" => enemies()
            .max_by_key(|member| {
                (
                    member_status(member, "enemy")
                        .ok()
                        .and_then(|enemy| i32_field(&enemy, "break_gauge"))
                        .unwrap_or_default(),
                    i32_field(member, "member_id")
                        .unwrap_or_default()
                        .saturating_neg(),
                )
            })
            .and_then(|member| i32_field(member, "member_id"))
            .ok_or(StateError::InvalidRequest),
        _ => Err(StateError::InvalidRequest),
    }
}

fn spend_burst_gauges(
    state: &mut DynamicMessage,
    actions: &[PendingAction],
) -> Result<(), StateError> {
    let mut costs = std::collections::BTreeMap::<i32, i32>::new();
    for action in actions {
        let total = costs.entry(action.actor_id).or_default();
        *total = total.saturating_add(action.burst_gauge_cost);
    }
    if costs.is_empty() {
        return Ok(());
    }
    let mut members = message_list(state, "members");
    for member in &mut members {
        let Some(cost) = i32_field(member, "member_id").and_then(|id| costs.get(&id).copied())
        else {
            continue;
        };
        let mut gauge = member_status(member, "burst_gauge")?;
        let current = i32_field(&gauge, "current_gauge").unwrap_or_default();
        gauge.set_field_by_name("current_gauge", Value::I32(current.saturating_sub(cost)));
        member.set_field_by_name("burst_gauge", Value::Message(gauge));
    }
    state.set_field_by_name(
        "members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );
    Ok(())
}
