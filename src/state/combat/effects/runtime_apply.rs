use super::policy::healing_amount;
use super::registry::registry;
use super::runtime::{Instance, Runtime};
use super::runtime_match::{amount, condition, selected, state_application_blocked};
use super::runtime_results::{effect_result, status_display, status_effect_result};
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
        let mut members = message_list(state, "members");
        let source = members
            .iter()
            .find(|m| i32_field(m, "member_id") == Some(source_id))
            .ok_or(StateError::InvalidRequest)?
            .clone();
        let mut results = Vec::new();
        for (effect_index, effect) in effects.iter().enumerate() {
            let Some(base_rule) = registry()?.rules.iter().find(|r| r.id == effect.id) else {
                self.unsupported.insert(effect.id);
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
            if rule.mode != "active" {
                continue;
            }
            if rule.phase != phase || !condition(rule, &source) {
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
            let value = amount(rule, effect.value)?;
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
                let maximum = registry()?.max_party_gauge;
                let heal = i64::from(maximum).saturating_mul(i64::from(value.max(0))) / 10_000;
                let heal = i32::try_from(heal).map_err(|_| StateError::InvalidRequest)?;
                let current = i32_field(state, "party_gauge").unwrap_or_default().max(0);
                state.set_field_by_name(
                    "party_gauge",
                    Value::I32(current.saturating_add(heal).min(maximum)),
                );
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
            if matches!(rule.operation.as_str(), "cleanse" | "cleanse_abnormal") {
                for target_index in 0..members.len() {
                    if !bool_field(&members[target_index], "is_alive")
                        || !selected(rule, &source, &members[target_index], targets)
                    {
                        continue;
                    }
                    let target_id = member_id(&members[target_index])?;
                    let removable = if rule.operation == "cleanse" {
                        &registry()?.removable_negative_state_ids
                    } else {
                        &registry()?.removable_abnormal_state_ids
                    };
                    let mut removed = Vec::new();
                    let mut changes = message_list(&members[target_index], "state_changes");
                    changes.retain(|change| {
                        if removable
                            .contains(&i32_field(change, "state_change_id").unwrap_or_default())
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
                    self.instances.retain(|instance| {
                        instance.target != target_id || !removable.contains(&instance.rule.state_id)
                    });
                    if let Some(managed) = self.managed.get_mut(&target_id) {
                        managed.retain(|state_id| !removable.contains(state_id));
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
                    if !bool_field(&members[target_index], "is_alive")
                        || !selected(rule, &source, &members[target_index], targets)
                    {
                        continue;
                    }
                    let target_id = member_id(&members[target_index])?;
                    let resistance =
                        message_list(&members[target_index], "state_change_resistances")
                            .into_iter()
                            .find(|row| i32_field(row, "state_change_id") == Some(rule.state_id));
                    let invalid = state_application_blocked(&members[target_index], rule.state_id)?
                        || resistance
                            .as_ref()
                            .is_some_and(|row| bool_field(row, "is_invalid"));
                    let resistance_rate = resistance
                        .as_ref()
                        .and_then(|row| i32_field(row, "value"))
                        .unwrap_or_default()
                        .saturating_mul(100);
                    let chance = application_rate
                        .saturating_sub(resistance_rate)
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
                        || member_status(m, "enemy")
                            .ok()
                            .is_some_and(|enemy| bool_field(&enemy, "is_broken")))
            }) {
                let target_id = member_id(target)?;
                if state_application_blocked(target, rule.state_id)? {
                    results.push(status_effect_result(
                        proto, effect.id, source_id, target_id, is_skill, rule, value, 4,
                    )?);
                    continue;
                }
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
                self.instances.retain(|i| {
                    !(i.source == source_id && i.target == target_id && i.rule.id == rule.id)
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
