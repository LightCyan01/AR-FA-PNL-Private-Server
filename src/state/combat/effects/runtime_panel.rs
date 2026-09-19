use super::registry::{registry, Rule};
use super::runtime::Runtime;
use super::runtime_match::{contextual_recipient, selected};
use super::runtime_results::effect_result;
use crate::state::combat::prelude::*;
use std::collections::BTreeSet;

pub(super) fn scale_panel_value(value: i32, rate: i128) -> i32 {
    i32::try_from(i128::from(value) * rate / 10_000).unwrap_or_else(|_| {
        if value.is_negative() {
            i32::MIN
        } else {
            i32::MAX
        }
    })
}

impl Runtime {
    pub(super) fn effective_panel_id(&self, state: &DynamicMessage) -> Result<i32, StateError> {
        let panel_id = current_panel_id(state);
        let actor_id = member_id(&current_actor(state)?)?;
        if self.instances.iter().any(|instance| {
            instance.target == actor_id
                && instance.rule.operation == "panel_disable"
                && !instance.rule.panel_from_ids.is_empty()
                && instance.rule.panel_from_ids.contains(&panel_id)
        }) {
            return Ok(11);
        }
        Ok(effective_battle_panel_id(state))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn convert_panels(
        &mut self,
        proto: &ProtoRegistry,
        state: &mut DynamicMessage,
        source: &DynamicMessage,
        members: &[DynamicMessage],
        targets: &[i32],
        effect: &TutorialSkillEffect,
        is_skill: bool,
        rule: &Rule,
        panel_context: Option<&DynamicMessage>,
    ) -> Result<Vec<DynamicMessage>, StateError> {
        let source_id = member_id(source)?;
        if rule.trigger_limit > 0
            && self
                .limited_effect_uses
                .get(&source_id)
                .and_then(|uses| uses.get(&rule.id))
                .is_some_and(|uses| *uses >= rule.trigger_limit)
        {
            return Ok(Vec::new());
        }
        let context = panel_context.unwrap_or(state);
        let context_members = message_list(context, "members");
        let units = message_list(context, "timeline_units");
        let context_panels = message_list(context, "timeline_panels");
        let target_ids = if rule.target == "next_enemy" {
            units
                .iter()
                .skip(usize::from(rule.phase == "after"))
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
                    bool_field(member, "is_alive") && selected(rule, source, member, targets)
                })
                .map(member_id)
                .collect::<Result<Vec<_>, _>>()?
        };
        let mut panels = message_list(state, "timeline_panels");
        let mut results = Vec::new();
        for target_id in target_ids {
            let turns = context_panels
                .iter()
                .zip(&units)
                .skip(usize::from(rule.phase == "after"))
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
                result.set_field_by_name("overwritten_timeline_panels", Value::List(overwritten));
                results.push(result);
            }
        }
        state.set_field_by_name(
            "timeline_panels",
            Value::List(panels.into_iter().map(Value::Message).collect()),
        );
        if !results.is_empty() && rule.trigger_limit > 0 {
            *self
                .limited_effect_uses
                .entry(source_id)
                .or_default()
                .entry(rule.id)
                .or_default() += 1;
        }
        Ok(results)
    }

    pub(super) fn panel_effect_rate(&self, state: &DynamicMessage) -> Result<i128, StateError> {
        if !registry()?
            .enhancement_panel_ids
            .contains(&self.effective_panel_id(state)?)
        {
            return Ok(10_000);
        }
        let actor = current_actor(state)?;
        let actor_id = member_id(&actor)?;
        let instance = self
            .instances
            .iter()
            .filter(|instance| {
                instance.target == actor_id && instance.rule.operation == "panel_potency"
            })
            .map(|instance| instance.value)
            .max()
            .unwrap_or_default();
        let passive = self
            .passives
            .iter()
            .filter(|passive| {
                passive.rule.operation == "panel_potency" && contextual_recipient(passive, &actor)
            })
            .map(|passive| passive.value)
            .max()
            .unwrap_or_default();
        Ok(i128::from(
            10_000i32.saturating_add(instance.max(passive)).max(0),
        ))
    }

    pub(crate) fn panel_multiplier(
        &self,
        state: &DynamicMessage,
    ) -> Result<(i128, i128), StateError> {
        if self.effective_panel_id(state)? == 11 {
            return Ok((100, 100));
        }
        let (numerator, denominator) = battle_panel_multiplier(state);
        let bonus = (numerator - denominator) * self.panel_effect_rate(state)? / 10_000;
        Ok((denominator + bonus, denominator))
    }

    pub(crate) fn panel_break_multiplier(
        &self,
        state: &DynamicMessage,
    ) -> Result<i128, StateError> {
        if self.effective_panel_id(state)? == 11 {
            return Ok(100);
        }
        let base = battle_panel_break_multiplier(state);
        Ok(100 + (base - 100) * self.panel_effect_rate(state)? / 10_000)
    }

    pub(crate) fn consume_panel_potency(
        &mut self,
        state: &DynamicMessage,
        actor_id: i32,
    ) -> Result<(), StateError> {
        let panel_id = current_panel_id(state);
        for instance in self.instances.iter_mut().filter(|instance| {
            instance.target == actor_id
                && instance.rule.operation == "panel_disable"
                && !instance.rule.panel_from_ids.is_empty()
                && instance.rule.panel_from_ids.contains(&panel_id)
        }) {
            if instance.remaining > 0 {
                instance.remaining -= 1;
            }
        }
        self.instances.retain(|instance| instance.remaining != 0);
        if !registry()?
            .enhancement_panel_ids
            .contains(&self.effective_panel_id(state)?)
        {
            return Ok(());
        }
        for instance in self.instances.iter_mut().filter(|instance| {
            instance.target == actor_id && instance.rule.operation == "panel_potency"
        }) {
            if instance.remaining > 0 {
                instance.remaining -= 1;
            }
        }
        self.instances.retain(|instance| instance.remaining != 0);
        Ok(())
    }
}
