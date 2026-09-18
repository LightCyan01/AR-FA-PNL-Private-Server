use super::policy::healing_amount;
use super::registry::{registry, rule_for_occurrence, Expiry, Rule};
use super::runtime::{Instance, Runtime};
use super::runtime_form::apply_skill_form;
use super::runtime_lamp::{is_lamp_mechanic, lamp_condition_matches};
use super::runtime_levels::apply_level_state;
use super::runtime_match::{
    amount, condition, contextual_recipient, selected, selected_for_source_character,
    selected_with_condition_target, state_application_blocked,
};
use super::runtime_results::{
    display, effect_result, level_display, status_display, status_effect_result,
};
use super::runtime_resources::{add_burst_gauge, add_party_gauge};
use super::runtime_scaling::{party_tag_count, scaled_effect_value};
use super::runtime_targeting::resolved_targets;
use crate::state::combat::prelude::*;
use std::collections::BTreeSet;

impl Runtime {
    #[allow(clippy::too_many_arguments)]
    #[cfg(test)]
    pub(crate) fn apply(
        &mut self,
        proto: &ProtoRegistry,
        state: &mut DynamicMessage,
        source_id: i32,
        effects: &[TutorialSkillEffect],
        targets: &[i32],
        is_skill: bool,
        phase: &str,
        panel_context: Option<&DynamicMessage>,
    ) -> Result<Vec<DynamicMessage>, StateError> {
        self.apply_inner(
            proto,
            state,
            source_id,
            0,
            effects,
            targets,
            is_skill,
            phase,
            panel_context,
            10_000,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_for_action(
        &mut self,
        proto: &ProtoRegistry,
        state: &mut DynamicMessage,
        source_id: i32,
        skill_id: i32,
        effects: &[TutorialSkillEffect],
        targets: &[i32],
        is_skill: bool,
        phase: &str,
        panel_context: Option<&DynamicMessage>,
        application_rate: i32,
        secret: &[u8],
        start_txid: &str,
        action_number: i32,
    ) -> Result<Vec<DynamicMessage>, StateError> {
        self.apply_inner(
            proto,
            state,
            source_id,
            skill_id,
            effects,
            targets,
            is_skill,
            phase,
            panel_context,
            application_rate,
            Some((secret, start_txid, action_number)),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_for_action_with_rules(
        &mut self,
        proto: &ProtoRegistry,
        rules: &TutorialRules,
        state: &mut DynamicMessage,
        source_id: i32,
        skill_id: i32,
        effects: &[TutorialSkillEffect],
        targets: &[i32],
        is_skill: bool,
        phase: &str,
        panel_context: Option<&DynamicMessage>,
        application_rate: i32,
        secret: &[u8],
        start_txid: &str,
        action_number: i32,
    ) -> Result<Vec<DynamicMessage>, StateError> {
        self.apply_inner_with_rules(
            proto,
            Some(rules),
            state,
            source_id,
            skill_id,
            effects,
            targets,
            is_skill,
            phase,
            panel_context,
            application_rate,
            Some((secret, start_txid, action_number)),
            false,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_inner(
        &mut self,
        proto: &ProtoRegistry,
        state: &mut DynamicMessage,
        source_id: i32,
        skill_id: i32,
        effects: &[TutorialSkillEffect],
        targets: &[i32],
        is_skill: bool,
        phase: &str,
        panel_context: Option<&DynamicMessage>,
        application_rate: i32,
        rng: Option<(&[u8], &str, i32)>,
    ) -> Result<Vec<DynamicMessage>, StateError> {
        self.apply_inner_with_rules(
            proto,
            None,
            state,
            source_id,
            skill_id,
            effects,
            targets,
            is_skill,
            phase,
            panel_context,
            application_rate,
            rng,
            false,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_inner_with_rules(
        &mut self,
        proto: &ProtoRegistry,
        rules: Option<&TutorialRules>,
        state: &mut DynamicMessage,
        source_id: i32,
        skill_id: i32,
        effects: &[TutorialSkillEffect],
        targets: &[i32],
        is_skill: bool,
        phase: &str,
        panel_context: Option<&DynamicMessage>,
        application_rate: i32,
        rng: Option<(&[u8], &str, i32)>,
        critical_trigger: bool,
    ) -> Result<Vec<DynamicMessage>, StateError> {
        let mut members = message_list(state, "members");
        let condition_members = panel_context.map(|context| message_list(context, "members"));
        let source = members
            .iter()
            .find(|m| i32_field(m, "member_id") == Some(source_id))
            .ok_or(StateError::InvalidRequest)?
            .clone();
        let mut results = Vec::new();
        for (effect_index, effect) in effects.iter().enumerate() {
            if rule_for_occurrence(
                effect.id,
                "catalog",
                "skill",
                skill_id,
                Some(effect_index),
            )?
            .is_some()
            {
                continue;
            }
            if is_lamp_mechanic(skill_id, effect.id)? {
                continue;
            }
            let has_nested_rule = registry()?.nested_actions.iter().any(|rule| {
                rule.owner_type == "skill"
                    && rule.owner_id == skill_id
                    && rule.effect_id == effect.id
            });
            if phase == "after" && has_nested_rule && !critical_trigger {
                let (secret, transaction, action_number) = rng.ok_or(StateError::InvalidRequest)?;
                results.extend(self.apply_nested_grants(
                    proto,
                    &members,
                    source_id,
                    skill_id,
                    effect,
                    targets,
                    is_skill,
                    secret,
                    transaction,
                    action_number,
                    effect_index,
                )?);
            }
            let Some(base_rule) = rule_for_occurrence(
                effect.id,
                "active",
                "skill",
                skill_id,
                Some(effect_index),
            )?
            else {
                if !has_nested_rule
                    && rule_for_occurrence(
                        effect.id,
                        "instant",
                        "skill",
                        skill_id,
                        Some(effect_index),
                    )?
                    .is_none()
                {
                    self.unsupported.insert(effect.id);
                }
                continue;
            };
            let mut effective_rule = base_rule.clone();
            if let Some(duration) = registry()?
                .status_durations_by_skill
                .get(&skill_id)
                .and_then(|durations| durations.get(&effect.id))
            {
                effective_rule.duration = *duration;
            }
            if let Some(target) = registry()?
                .targets_by_skill
                .get(&skill_id)
                .and_then(|targets| targets.get(&effect.id))
            {
                effective_rule.target.clone_from(target);
            }
            if let Some(phase) = registry()?
                .phases_by_skill
                .get(&skill_id)
                .and_then(|phases| phases.get(&effect.id))
            {
                effective_rule.phase.clone_from(phase);
            }
            if critical_trigger {
                effective_rule.critical_only = false;
            }
            let mut extra_rules = Vec::new();
            if let Some(variants) = registry()?
                .rule_variants_by_skill
                .get(&skill_id)
                .and_then(|effects| effects.get(&effect.id))
            {
                for (index, variant) in variants.iter().enumerate() {
                    if index == 0 {
                        effective_rule.expiry = variant.expiry.clone();
                        effective_rule.duration = variant.duration;
                    } else {
                        let mut extra = effective_rule.clone();
                        extra.expiry = variant.expiry.clone();
                        extra.duration = variant.duration;
                        extra_rules.push(extra);
                    }
                }
            }
            let rule = &effective_rule;
            let required_ability = rule.required_ability_id == 0
                || self.passives.iter().any(|passive| {
                    passive.source == source_id
                        && passive.rule.owner_type == "ability"
                        && passive.rule.owner_id == rule.required_ability_id
                        && condition(&passive.rule, &source)
                        && selected_for_source_character(
                            &passive.rule,
                            &source,
                            passive.source_character_id,
                            &source,
                            &[],
                        )
                });
            if rule.phase != phase
                || (rule.critical_only && !critical_trigger)
                || !required_ability
                || !condition(rule, &source)
                || !lamp_condition_matches(&source, skill_id, rule)?
            {
                continue;
            }
            if rule.operation == "action_reroll" {
                // No queued choice exists; enemies sample skills at execution.
                continue;
            }
            let requested_targets = targets;
            let requested_target_broken = members.iter().any(|member| {
                requested_targets.contains(&member_id(member).unwrap_or_default())
                    && member_status(member, "enemy")
                        .ok()
                        .is_some_and(|enemy| bool_field(&enemy, "is_broken"))
            });
            let resolved_targets = resolved_targets(rule, &source, &members, panel_context, targets)?;
            let targets = resolved_targets.as_slice();
            if rule.operation == "skill_form" {
                results.push(apply_skill_form(
                    proto,
                    rules.ok_or(StateError::InvalidRequest)?,
                    &mut members,
                    source_id,
                    skill_id,
                    effect,
                    is_skill,
                    rule,
                )?);
                continue;
            }
            if rule.operation == "field_effect" {
                let field_effect_id = rule.fixed.ok_or(StateError::InvalidRequest)?;
                set_battle_field_effect(proto, state, Some(field_effect_id))?;
                let mut result = effect_result(
                    proto,
                    effect.id,
                    source_id,
                    source_id,
                    is_skill,
                    rule,
                    effect.value,
                )?;
                result.set_field_by_name(
                    "field_effect_id",
                    Value::Message(wrapper_i32(proto, field_effect_id)?),
                );
                results.push(result);
                continue;
            }
            if rule.operation == "summons" {
                results.push(apply_summons(
                    proto,
                    rules.ok_or(StateError::InvalidRequest)?,
                    state,
                    &mut members,
                    &source,
                    source_id,
                    effect,
                    is_skill,
                    rule,
                )?);
                continue;
            }
            if rule.operation == "panel_convert" {
                let context = panel_context.unwrap_or(state);
                let context_members = message_list(context, "members");
                let units = message_list(context, "timeline_units");
                let context_panels = message_list(context, "timeline_panels");
                let target_ids = if rule.target == "next_enemy" {
                    units
                        .iter()
                        .skip(usize::from(phase == "after"))
                        .filter_map(|unit| member_id(unit).ok())
                        .find(|id| {
                            context_members.iter().any(|member| {
                                member_id(member).ok() == Some(*id)
                                    && member_type(member).ok() == Some(1)
                                    && bool_field(member, "is_alive")
                            })
                        })
                        .into_iter()
                        .collect()
                } else {
                    members
                        .iter()
                        .filter(|member| {
                            bool_field(member, "is_alive")
                                && selected(rule, &source, member, targets)
                        })
                        .map(member_id)
                        .collect::<Result<Vec<_>, _>>()?
                };
                let mut panels = message_list(state, "timeline_panels");
                for target_id in target_ids {
                    let turns = context_panels
                        .iter()
                        .zip(&units)
                        .skip(usize::from(phase == "after"))
                        .filter_map(|(panel, unit)| {
                            (member_id(unit).ok() == Some(target_id)
                                && rule
                                    .panel_from_ids
                                    .contains(&optional_i32_field(panel, "panel_id").unwrap_or(11)))
                            .then(|| i32_field(panel, "turn"))?
                        })
                        .take(if rule.panel_limit == 0 {
                            usize::MAX
                        } else {
                            rule.panel_limit
                        })
                        .collect::<BTreeSet<_>>();
                    let mut overwritten = Vec::new();
                    for panel in &mut panels {
                        if !i32_field(panel, "turn").is_some_and(|turn| turns.contains(&turn)) {
                            continue;
                        }
                        let turn = i32_field(panel, "turn").ok_or(StateError::InvalidRequest)?;
                        *panel = build_timeline_panel(proto, rule.panel_to_id, turn)?;
                        overwritten.push(Value::Message(panel.clone()));
                    }
                    if !overwritten.is_empty() {
                        let mut result = effect_result(
                            proto,
                            effect.id,
                            source_id,
                            target_id,
                            is_skill,
                            rule,
                            effect.value,
                        )?;
                        result.set_field_by_name(
                            "overwritten_timeline_panels",
                            Value::List(overwritten),
                        );
                        results.push(result);
                    }
                }
                state.set_field_by_name(
                    "timeline_panels",
                    Value::List(panels.into_iter().map(Value::Message).collect()),
                );
                continue;
            }
            if rule.operation == "extra_turn" {
                if self.queue_extra_turn(source_id, skill_id, rule)? {
                    results.push(effect_result(
                        proto,
                        effect.id,
                        source_id,
                        source_id,
                        is_skill,
                        rule,
                        effect.value,
                    )?);
                }
                continue;
            }
            let mut value = amount(rule, effect.value)?;
            if rule.operation == "random_modifier" {
                let (secret, transaction, action_number) = rng.ok_or(StateError::InvalidRequest)?;
                results.extend(self.apply_random_modifier(
                    proto,
                    &members,
                    &source,
                    source_id,
                    effect,
                    targets,
                    is_skill,
                    rule,
                    value,
                    secret,
                    transaction,
                    action_number,
                )?);
                continue;
            }
            if !rule.scale_by.is_empty() {
                let scale_count = match rule.scale_by.as_str() {
                    "opponent_count" => {
                        let source_type = member_type(&source)?;
                        i32::try_from(
                            members
                                .iter()
                                .filter(|member| {
                                    bool_field(member, "is_alive")
                                        && member_type(member).ok() != Some(source_type)
                                })
                                .count(),
                        )
                        .map_err(|_| StateError::InvalidRequest)?
                    }
                    "party_tag_count" => party_tag_count(
                        rules.ok_or(StateError::InvalidRequest)?,
                        &members,
                        &source,
                        *rule
                            .condition
                            .get("party_tag_id")
                            .ok_or(StateError::InvalidRequest)?,
                    )?,
                    _ => 0,
                };
                value = scaled_effect_value(rule, &source, scale_count, value)?;
            }
            if rule.operation == "timeline_shift" {
                for target in members.iter().filter(|member| {
                    bool_field(member, "is_alive") && selected(rule, &source, member, targets)
                }) {
                    results.push(effect_result(
                        proto,
                        effect.id,
                        source_id,
                        member_id(target)?,
                        is_skill,
                        rule,
                        value,
                    )?);
                }
                continue;
            }
            if rule.operation == "party_gauge" {
                let heal = add_party_gauge(state, value)?;
                let mut result = effect_result(
                    proto, effect.id, source_id, source_id, is_skill, rule, value,
                )?;
                if heal > 0 {
                    result.set_field_by_name(
                        "party_gauge_heal",
                        Value::Message(wrapper_i32(proto, heal)?),
                    );
                }
                results.push(result);
                continue;
            }
            if rule.operation == "bomb_gauge" {
                let maximum = rules
                    .ok_or(StateError::InvalidRequest)?
                    .constants
                    .max_bomb_gauge;
                let delta = i64::from(maximum).saturating_mul(i64::from(value)) / 10_000;
                let delta = i32::try_from(delta).map_err(|_| StateError::InvalidRequest)?;
                let current = i32_field(state, "bomb_gauge").unwrap_or_default().max(0);
                let next = current.saturating_add(delta).clamp(0, maximum);
                state.set_field_by_name("bomb_gauge", Value::I32(next));
                let mut result = effect_result(
                    proto, effect.id, source_id, source_id, is_skill, rule, value,
                )?;
                result.set_field_by_name(
                    "add_bomb_gauge",
                    Value::Message(wrapper_i32(proto, next - current)?),
                );
                results.push(result);
                continue;
            }
            if rule.operation == "burst_gauge" {
                for target_index in 0..members.len() {
                    if !bool_field(&members[target_index], "is_alive")
                        || !selected(rule, &source, &members[target_index], targets)
                    {
                        continue;
                    }
                    let target_id = member_id(&members[target_index])?;
                    let required = rules
                        .ok_or(StateError::InvalidRequest)?
                        .constants
                        .burst_gauge_required_for_one_burst_skill;
                    let delta = add_burst_gauge(&mut members[target_index], value, required)?;
                    let mut result = effect_result(
                        proto, effect.id, source_id, target_id, is_skill, rule, value,
                    )?;
                    result.set_field_by_name(
                        "add_burst_gauge",
                        Value::Message(wrapper_i32(proto, delta)?),
                    );
                    results.push(result);
                }
                continue;
            }
            if rule.operation == "break_gauge" {
                for target_index in 0..members.len() {
                    if !bool_field(&members[target_index], "is_alive")
                        || !selected(rule, &source, &members[target_index], targets)
                    {
                        continue;
                    }
                    let target_id = member_id(&members[target_index])?;
                    let Ok(mut enemy) = member_status(&members[target_index], "enemy") else {
                        continue;
                    };
                    let maximum = i32_field(&enemy, "max_break_gauge")
                        .ok_or(StateError::InvalidRequest)?
                        .max(0);
                    let current = i32_field(&enemy, "break_gauge").unwrap_or_default().max(0);
                    let delta = i64::from(maximum).saturating_mul(i64::from(value)) / 10_000;
                    let delta = i32::try_from(delta).map_err(|_| StateError::InvalidRequest)?;
                    let next = current.saturating_add(delta).clamp(0, maximum);
                    enemy.set_field_by_name("break_gauge", Value::I32(next));
                    members[target_index].set_field_by_name("enemy", Value::Message(enemy));
                    let mut result = effect_result(
                        proto, effect.id, source_id, target_id, is_skill, rule, value,
                    )?;
                    result.set_field_by_name(
                        "break_gauge_heal",
                        Value::Message(wrapper_i32(proto, next - current)?),
                    );
                    results.push(result);
                }
                continue;
            }
            if rule.operation == "level_state" {
                results.extend(apply_level_state(
                    self, proto, &source, &members, source_id, effect, targets, is_skill, rule,
                    value,
                )?);
                continue;
            }
            if rule.operation == "remove_stack" {
                for target_index in 0..members.len() {
                    if !bool_field(&members[target_index], "is_alive")
                        || !selected(rule, &source, &members[target_index], targets)
                    {
                        continue;
                    }
                    let target_id = member_id(&members[target_index])?;
                    let index = self.instances.iter().position(|instance| {
                        instance.target == target_id && instance.rule.state_id == rule.state_id
                    });
                    let mut result_rule = rule.clone();
                    result_rule.state_id = 0;
                    let mut result = effect_result(
                        proto,
                        effect.id,
                        source_id,
                        target_id,
                        is_skill,
                        &result_rule,
                        effect.value,
                    )?;
                    if let Some(index) = index {
                        let previous = self.instances[index].clone();
                        let count = effect.value.saturating_div(100).max(1);
                        let removed = rule
                            .fixed
                            .map_or(previous.value, |value| value.saturating_mul(count));
                        if previous.value <= removed {
                            self.instances.remove(index);
                            let removed_state = if previous.rule.operation == "level_state" {
                                level_display(
                                    proto,
                                    rule.state_id,
                                    previous.value,
                                    previous.remaining,
                                )?
                            } else {
                                display(
                                    proto,
                                    rule.state_id,
                                    previous.value,
                                    previous.remaining,
                                )?
                            };
                            result.set_field_by_name(
                                "removed_state_changes",
                                Value::List(vec![Value::Message(removed_state)]),
                            );
                            if !self.instances.iter().any(|instance| {
                                instance.target == target_id
                                    && instance.rule.state_id == rule.state_id
                            }) {
                                let mut changes =
                                    message_list(&members[target_index], "state_changes");
                                changes.retain(|change| {
                                    i32_field(change, "state_change_id") != Some(rule.state_id)
                                });
                                members[target_index].set_field_by_name(
                                    "state_changes",
                                    Value::List(
                                        changes.into_iter().map(Value::Message).collect(),
                                    ),
                                );
                                self.managed
                                    .entry(target_id)
                                    .or_default()
                                    .remove(&rule.state_id);
                            }
                        } else {
                            self.instances[index].value = previous.value - removed;
                        }
                    }
                    results.push(result);
                }
                continue;
            }
            if matches!(
                rule.operation.as_str(),
                "cleanse" | "cleanse_abnormal" | "cleanse_positive"
            ) {
                let limit = if (100..10_000).contains(&value) {
                    usize::try_from(value / 100).map_err(|_| StateError::InvalidRequest)?
                } else {
                    usize::MAX
                };
                for target_index in 0..members.len() {
                    if !bool_field(&members[target_index], "is_alive")
                        || !selected(rule, &source, &members[target_index], targets)
                    {
                        continue;
                    }
                    let target_id = member_id(&members[target_index])?;
                    let removable = match rule.operation.as_str() {
                        "cleanse" => &registry()?.removable_negative_state_ids,
                        "cleanse_abnormal" => &registry()?.removable_abnormal_state_ids,
                        "cleanse_positive" => &registry()?.removable_positive_state_ids,
                        _ => unreachable!(),
                    };
                    let can_remove = |state_id| {
                        removable.contains(&state_id)
                            && (rule.affected_state_ids.is_empty()
                                || rule.affected_state_ids.contains(&state_id))
                    };
                    let mut removed = Vec::new();
                    let mut changes = message_list(&members[target_index], "state_changes");
                    changes.retain(|change| {
                        if removed.len() < limit
                            && can_remove(
                                i32_field(change, "state_change_id").unwrap_or_default(),
                            )
                        {
                            removed.push(Value::Message(change.clone()));
                            false
                        } else {
                            true
                        }
                    });
                    members[target_index].set_field_by_name(
                        "state_changes",
                        Value::List(changes.into_iter().map(Value::Message).collect()),
                    );
                    if removed.iter().any(|change| {
                        change
                            .as_message()
                            .and_then(|change| i32_field(change, "state_change_id"))
                            == Some(910059)
                    }) {
                        members[target_index].set_field_by_name("is_stun", Value::Bool(false));
                    }
                    self.instances.retain(|instance| {
                        instance.target != target_id || !can_remove(instance.rule.state_id)
                    });
                    if let Some(managed) = self.managed.get_mut(&target_id) {
                        managed.retain(|state_id| !can_remove(*state_id));
                    }
                    let mut result = effect_result(
                        proto, effect.id, source_id, target_id, is_skill, rule, value,
                    )?;
                    result.set_field_by_name("removed_state_changes", Value::List(removed));
                    results.push(result);
                }
                continue;
            }
            if rule.operation == "heal" {
                for target_index in 0..members.len() {
                    if !bool_field(&members[target_index], "is_alive")
                        || !selected(rule, &source, &members[target_index], targets)
                    {
                        continue;
                    }
                    let target_id = member_id(&members[target_index])?;
                    let maximum_hp = i64::from(
                        i32_field(&members[target_index], "max_hp")
                            .ok_or(StateError::InvalidRequest)?
                            .max(0),
                    );
                    let base = maximum_hp.saturating_mul(i64::from(value.max(0))) / 10_000;
                    let heal = healing_amount(
                        i32::try_from(base).map_err(|_| StateError::InvalidRequest)?,
                        &source,
                        &members[target_index],
                    )?;
                    let hp = i32_field(&members[target_index], "hp")
                        .ok_or(StateError::InvalidRequest)?
                        .max(0);
                    members[target_index].set_field_by_name(
                        "hp",
                        Value::I32(hp.saturating_add(heal).min(maximum_hp as i32)),
                    );
                    let mut result = effect_result(
                        proto, effect.id, source_id, target_id, is_skill, rule, value,
                    )?;
                    if heal > 0 {
                        result.set_field_by_name(
                            "hp_heal",
                            Value::Message(wrapper_i32(proto, heal)?),
                        );
                    }
                    results.push(result);
                }
                continue;
            }
            if rule.operation == "status" {
                for target_index in 0..members.len() {
                    if !bool_field(&members[target_index], "is_alive") {
                        continue;
                    }
                    let target_id = member_id(&members[target_index])?;
                    let condition_target = condition_members
                        .as_ref()
                        .and_then(|context| {
                            context.iter().find(|member| member_id(member).ok() == Some(target_id))
                        })
                        .unwrap_or(&members[target_index]);
                    if !selected_with_condition_target(
                        rule,
                        &source,
                        &members[target_index],
                        condition_target,
                        targets,
                    ) {
                        continue;
                    }
                    let resistance =
                        message_list(&members[target_index], "state_change_resistances")
                            .into_iter()
                            .find(|row| i32_field(row, "state_change_id") == Some(rule.state_id));
                    let invalid = state_application_blocked(&members[target_index], rule)?
                        || resistance
                            .as_ref()
                            .is_some_and(|row| bool_field(row, "is_invalid"));
                    let resistance_rate = resistance
                        .as_ref()
                        .and_then(|row| i32_field(row, "value"))
                        .unwrap_or_default()
                        .saturating_mul(100);
                    let runtime_resistance = if registry()?.abnormal_state_ids.contains(&rule.state_id)
                    {
                        let applies = |candidate: &Rule| {
                            candidate.affected_state_ids.is_empty()
                                || candidate.affected_state_ids.contains(&rule.state_id)
                        };
                        let active = self
                            .instances
                            .iter()
                            .filter(|instance| {
                                instance.target == target_id
                                    && instance.rule.operation == "abnormal_resistance"
                                    && applies(&instance.rule)
                            })
                            .fold(0i32, |total, instance| total.saturating_add(instance.value));
                        self.passives
                            .iter()
                            .filter(|passive| {
                                passive.rule.operation == "abnormal_resistance"
                                    && applies(&passive.rule)
                                    && contextual_recipient(
                                        passive,
                                        &members[target_index],
                                    )
                                    && members.iter().any(|source| {
                                        i32_field(source, "member_id") == Some(passive.source)
                                            && bool_field(source, "is_alive")
                                            && condition(&passive.rule, source)
                                    })
                            })
                            .fold(active, |total, passive| {
                                total.saturating_add(passive.value)
                            })
                    } else {
                        0
                    };
                    let chance = application_rate
                        .saturating_sub(resistance_rate)
                        .saturating_sub(runtime_resistance)
                        .clamp(0, 10_000);
                    let succeeded = !invalid
                        && (chance == 10_000
                            || rng.is_none()
                            || rng.is_some_and(|(secret, transaction, action_number)| {
                                deterministic_roll(
                                    secret,
                                    transaction,
                                    action_number,
                                    b"state-change",
                                    target_id,
                                    u32::try_from(effect_index).unwrap_or(u32::MAX),
                                ) % 10_000
                                    < chance as u32
                            }));
                    let outcome = if invalid {
                        4
                    } else if succeeded {
                        1
                    } else {
                        2
                    };
                    if succeeded {
                        let mut changes = message_list(&members[target_index], "state_changes");
                        // Damage-over-time rows are deliberately repeated in master data;
                        // the other direct conditions refresh their one state row.
                        if !matches!(rule.state_id, 940006 | 940007) {
                            changes.retain(|change| {
                                i32_field(change, "state_change_id") != Some(rule.state_id)
                            });
                        }
                        changes.push(status_display(
                            proto,
                            rule.state_id,
                            value,
                            rule.duration,
                            source_id,
                        )?);
                        members[target_index].set_field_by_name(
                            "state_changes",
                            Value::List(changes.into_iter().map(Value::Message).collect()),
                        );
                        if !matches!(rule.state_id, 940006 | 940007) {
                            self.instances.retain(|instance| {
                                instance.target != target_id
                                    || instance.rule.operation != "status"
                                    || instance.rule.state_id != rule.state_id
                            });
                            if rule.expiry == Expiry::Attacked {
                                self.instances.push(Instance {
                                    source: source_id,
                                    source_character_id: message_i32_field(
                                        &source,
                                        "ally",
                                        "character_id",
                                    )
                                    .unwrap_or_default(),
                                    target: target_id,
                                    value,
                                    remaining: rule.duration,
                                    rule: rule.clone(),
                                });
                                self.managed
                                    .entry(target_id)
                                    .or_default()
                                    .insert(rule.state_id);
                            } else if let Some(managed) = self.managed.get_mut(&target_id) {
                                managed.remove(&rule.state_id);
                            }
                        }
                        if rule.state_id == 910059 {
                            members[target_index].set_field_by_name("is_stun", Value::Bool(true));
                        }
                    }
                    results.push(status_effect_result(
                        proto, effect.id, source_id, target_id, is_skill, rule, value, outcome,
                    )?);
                }
                continue;
            }
            for target in members.iter().filter(|m| {
                bool_field(m, "is_alive")
                    && selected(rule, &source, m, targets)
                    && (!rule.target_broken
                        || if requested_targets.contains(&member_id(m).unwrap_or_default()) {
                            member_status(m, "enemy")
                                .ok()
                                .is_some_and(|enemy| bool_field(&enemy, "is_broken"))
                        } else {
                            requested_target_broken
                        })
            }) {
                let target_id = member_id(target)?;
                if state_application_blocked(target, rule)? {
                    results.push(status_effect_result(
                        proto, effect.id, source_id, target_id, is_skill, rule, value, 4,
                    )?);
                    continue;
                }
                let value = self.apply_potency(&members, &source, target_id, rule, value)?;
                let value = if rule.stack_cap > 0 {
                    self.instances
                        .iter()
                        .find(|instance| {
                            instance.source == source_id
                                && instance.target == target_id
                                && instance.rule.id == rule.id
                        })
                        .map_or(value, |instance| instance.value.saturating_add(value))
                        .min(rule.stack_cap)
                } else {
                    value
                };
                self.instances.retain(|instance| {
                    !(instance.target == target_id && if rule.operation == "panel_potency" {
                        instance.rule.operation == "panel_potency"
                    } else {
                        instance.source == source_id && instance.rule.id == rule.id
                    })
                });
                let source_character_id =
                    message_i32_field(&source, "ally", "character_id").unwrap_or_default();
                for instance_rule in std::iter::once(rule).chain(extra_rules.iter()) {
                    self.instances.push(Instance {
                        source: source_id,
                        source_character_id,
                        target: target_id,
                        value,
                        remaining: instance_rule.duration,
                        rule: instance_rule.clone(),
                    });
                }
                self.managed
                    .entry(target_id)
                    .or_default()
                    .insert(rule.state_id);
                results.push(effect_result(
                    proto, effect.id, source_id, target_id, is_skill, rule, value,
                )?);
            }
        }
        state.set_field_by_name(
            "members",
            Value::List(members.into_iter().map(Value::Message).collect()),
        );
        self.refresh(proto, state)?;
        Ok(results)
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_summons(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    state: &mut DynamicMessage,
    members: &mut Vec<DynamicMessage>,
    source: &DynamicMessage,
    source_id: i32,
    effect: &TutorialSkillEffect,
    is_skill: bool,
    rule: &Rule,
) -> Result<DynamicMessage, StateError> {
    let source_type = member_type(source)?;
    let source_side_count = i32::try_from(
        members
            .iter()
            .filter(|member| {
                bool_field(member, "is_alive") && member_type(member).ok() == Some(source_type)
            })
            .count(),
    )
    .map_err(|_| StateError::InvalidRequest)?;
    let allowed = source_side_count >= rule.source_side_count_min
        && (rule.source_side_count_max == 0 || source_side_count <= rule.source_side_count_max);
    let summons_effect_id = rule.fixed.ok_or(StateError::InvalidRequest)?;
    let summons_rule = rules
        .summons_effects
        .iter()
        .find(|summons| summons.id == summons_effect_id)
        .ok_or_else(|| {
            StateError::TutorialRules(format!("missing summons effect {summons_effect_id}"))
        })?;
    let free_member_ids = (11..=10 + rules.constants.max_enemy_member_count)
        .filter(|member_id| {
            !members.iter().any(|member| {
                i32_field(member, "member_id") == Some(*member_id) && bool_field(member, "is_alive")
            })
        })
        .take(summons_rule.enemies.len())
        .collect::<Vec<_>>();
    let succeeded = allowed && free_member_ids.len() == summons_rule.enemies.len();
    let mut summons = empty_message(proto, "blend.model.BattleEffectSummons")?;
    summons.set_field_by_name("is_success", Value::Bool(succeeded));
    if succeeded {
        let battle_id = i32_field(state, "battle_id").ok_or(StateError::InvalidRequest)?;
        let wave_number = i32_field(state, "wave").unwrap_or(1).max(1);
        let wave_id = *rule_battle(rules, battle_id)?
            .wave_ids
            .get(usize::try_from(wave_number - 1).map_err(|_| StateError::InvalidRequest)?)
            .ok_or(StateError::InvalidRequest)?;
        let mut base_numbers = message_list(state, "base_enemy_numbers")
            .into_iter()
            .map(|row| {
                Ok((
                    i32_field(&row, "base_enemy_id").ok_or(StateError::InvalidRequest)?,
                    message_i32_field(&row, "current_number", "value")
                        .ok_or(StateError::InvalidRequest)?,
                ))
            })
            .collect::<Result<Vec<_>, StateError>>()?;
        let mut units = message_list(state, "timeline_units");
        let mut result_members = Vec::new();
        for (index, (wave_enemy, member_id)) in
            summons_rule.enemies.iter().zip(free_member_ids).enumerate()
        {
            members.retain(|member| i32_field(member, "member_id") != Some(member_id));
            units.retain(|unit| i32_field(unit, "member_id") != Some(member_id));
            let enemy = rule_enemy(rules, wave_enemy.id)?;
            let number = if let Some((_, count)) = base_numbers
                .iter_mut()
                .find(|(base_id, _)| *base_id == enemy.base_enemy_id)
            {
                *count = count.saturating_add(1);
                *count
            } else {
                base_numbers.push((enemy.base_enemy_id, 1));
                1
            };
            let member = build_enemy_member(proto, rules, wave_enemy, wave_id, member_id, number)?;
            let wait = base_wait(
                i32_field(&member_status(&member, "current_status")?, "speed").unwrap_or(1),
            )
            .max(1);
            let slots = if enemy.is_boss || enemy.status_growth.values().any(|value| *value != 0) {
                3
            } else {
                2
            };
            for timeline_number in 1..=slots {
                units.push(build_timeline_unit(
                    proto,
                    member_id,
                    timeline_number,
                    wait.saturating_mul(timeline_number),
                )?);
            }
            members.push(member);
            let mut result_member = empty_message(proto, "blend.model.BattleEffectSummonsMember")?;
            result_member.set_field_by_name("member_id", Value::I32(member_id));
            result_member.set_field_by_name(
                "enemies_index",
                Value::I32(i32::try_from(index).map_err(|_| StateError::InvalidRequest)?),
            );
            result_members.push(Value::Message(result_member));
        }
        sort_timeline_units(&mut units, members);
        state.set_field_by_name(
            "timeline_units",
            Value::List(units.into_iter().map(Value::Message).collect()),
        );
        set_base_enemy_numbers(proto, state, &base_numbers)?;
        summons.set_field_by_name("members", Value::List(result_members));
    }
    let mut result = effect_result(
        proto,
        effect.id,
        source_id,
        source_id,
        is_skill,
        rule,
        effect.value,
    )?;
    result.set_field_by_name("summons", Value::Message(summons));
    Ok(result)
}
