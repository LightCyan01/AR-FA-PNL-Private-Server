use super::registry::{registry, LampAbilityRule, Rule};
use super::runtime::Runtime;
use crate::state::combat::prelude::*;

fn current_lamp(member: &DynamicMessage, skill_id: i32) -> Result<i32, StateError> {
    message_list(&member_status(member, "ally")?, "skills")
        .into_iter()
        .find(|skill| i32_field(skill, "skill_id") == Some(skill_id))
        .and_then(|skill| i32_field(&skill, "lamp"))
        .ok_or(StateError::InvalidRequest)
}

fn next_lamp(current: i32, maximum: i32, increment: i32, clear: bool) -> i32 {
    if current >= maximum {
        if clear {
            0
        } else {
            maximum
        }
    } else {
        current.saturating_add(increment).min(maximum)
    }
}

pub(crate) fn predicted_skill_lamp(
    member: &DynamicMessage,
    skill_id: i32,
) -> Result<Option<i32>, StateError> {
    let Some(rule) = registry()?.lamp_skills.get(&skill_id) else {
        return Ok(None);
    };
    Ok(Some(next_lamp(
        current_lamp(member, skill_id)?,
        rule.maximum,
        rule.increment,
        rule.clear_when_full,
    )))
}

pub(super) fn is_lamp_mechanic(skill_id: i32, effect_id: i32) -> Result<bool, StateError> {
    Ok(registry()?
        .lamp_skills
        .get(&skill_id)
        .is_some_and(|rule| {
            rule.mechanic_effect_ids.contains(&effect_id)
                || rule.consumed_effect_ids.contains(&effect_id)
        }))
}

pub(super) fn lamp_condition_matches(
    member: &DynamicMessage,
    skill_id: i32,
    rule: &Rule,
) -> Result<bool, StateError> {
    let Some(required) = rule.condition.get("skill_lamp_full") else {
        return Ok(true);
    };
    let Some(lamp_rule) = registry()?.lamp_skills.get(&skill_id) else {
        return Ok(false);
    };
    let full = current_lamp(member, skill_id)? >= lamp_rule.maximum;
    Ok((*required != 0) == full)
}

pub(crate) fn skill_transformation(
    member: &DynamicMessage,
    skill_id: i32,
) -> Result<Option<(i32, i32)>, StateError> {
    let Some(lamp) = registry()?.lamp_skills.get(&skill_id) else {
        return Ok(None);
    };
    if current_lamp(member, skill_id)? < lamp.maximum {
        return Ok(None);
    }
    for effect_id in &lamp.full_effect_ids {
        if let Some(rule) = super::registry::rule_for(*effect_id, "active", "skill", skill_id)? {
            if rule.operation == "skill_form" {
                return Ok(rule.fixed.map(|destination| (*effect_id, destination)));
            }
        }
    }
    Ok(None)
}

pub(super) fn is_lamp_ability_effect(ability_id: i32, effect_id: i32) -> Result<bool, StateError> {
    Ok(registry()?
        .lamp_abilities
        .get(&ability_id)
        .is_some_and(|rule| rule.mechanic_effect_ids.contains(&effect_id)))
}

fn add_lamp(
    state: &mut DynamicMessage,
    source_id: i32,
    rule: &LampAbilityRule,
    amount: i32,
) -> Result<(), StateError> {
    if amount <= 0 {
        return Ok(());
    }
    let mut members = message_list(state, "members");
    let member = members
        .iter_mut()
        .find(|member| member_id(member).ok() == Some(source_id))
        .ok_or(StateError::InvalidRequest)?;
    let mut ally = member_status(member, "ally")?;
    let mut skills = message_list(&ally, "skills");
    let Some(skill) = skills.iter_mut().find(|skill| {
        rule.target_skill_ids
            .contains(&i32_field(skill, "skill_id").unwrap_or(0))
    }) else {
        return Ok(());
    };
    let current = i32_field(skill, "lamp").unwrap_or_default().max(0);
    skill.set_field_by_name(
        "lamp",
        Value::I32(current.saturating_add(amount).min(rule.maximum)),
    );
    ally.set_field_by_name(
        "skills",
        Value::List(skills.into_iter().map(Value::Message).collect()),
    );
    member.set_field_by_name("ally", Value::Message(ally));
    state.set_field_by_name(
        "members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );
    Ok(())
}

fn heal_targets(
    proto: &ProtoRegistry,
    state: &mut DynamicMessage,
    source_id: i32,
    effect_id: i32,
    value: i32,
    target_id: Option<i32>,
) -> Result<Vec<DynamicMessage>, StateError> {
    let mut members = message_list(state, "members");
    let source = members
        .iter()
        .find(|member| member_id(member).ok() == Some(source_id))
        .cloned()
        .ok_or(StateError::InvalidRequest)?;
    let source_type = member_type(&source)?;
    let mut results = Vec::new();
    for target in members.iter_mut().filter(|member| {
        bool_field(member, "is_alive")
            && member_type(member).ok() == Some(source_type)
            && target_id.is_none_or(|id| member_id(member).ok() == Some(id))
    }) {
        let target_id = member_id(target)?;
        let maximum = i32_field(target, "max_hp")
            .ok_or(StateError::InvalidRequest)?
            .max(0);
        let current = i32_field(target, "hp")
            .ok_or(StateError::InvalidRequest)?
            .max(0);
        let base = i64::from(maximum).saturating_mul(i64::from(value)) / 10_000;
        let heal = super::policy::healing_amount(
            i32::try_from(base).map_err(|_| StateError::InvalidRequest)?,
            &source,
            target,
        )?
        .min(maximum.saturating_sub(current));
        target.set_field_by_name("hp", Value::I32(current.saturating_add(heal)));
        let mut result = empty_message(proto, "blend.model.BattleEffectResult")?;
        result.set_field_by_name("effect_id", Value::I32(effect_id));
        result.set_field_by_name("effector_id", Value::I32(source_id));
        result.set_field_by_name(
            "effect_target_id",
            Value::Message(wrapper_i32(proto, target_id)?),
        );
        result.set_field_by_name("is_skill", Value::Bool(false));
        if heal > 0 {
            result.set_field_by_name("hp_heal", Value::Message(wrapper_i32(proto, heal)?));
        }
        results.push(result);
    }
    state.set_field_by_name(
        "members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );
    Ok(results)
}

pub(super) fn heal_all(
    proto: &ProtoRegistry,
    state: &mut DynamicMessage,
    source_id: i32,
    effect_id: i32,
    value: i32,
) -> Result<Vec<DynamicMessage>, StateError> {
    heal_targets(proto, state, source_id, effect_id, value, None)
}

pub(super) fn heal_self(
    proto: &ProtoRegistry,
    state: &mut DynamicMessage,
    source_id: i32,
    effect_id: i32,
    value: i32,
) -> Result<Vec<DynamicMessage>, StateError> {
    heal_targets(proto, state, source_id, effect_id, value, Some(source_id))
}

impl Runtime {
    pub(super) fn register_lamp_ability(
        &mut self,
        source_id: i32,
        ability_id: i32,
    ) -> Result<(), StateError> {
        if registry()?.lamp_abilities.contains_key(&ability_id) {
            self.lamp_abilities
                .entry(source_id)
                .or_default()
                .insert(ability_id);
        }
        Ok(())
    }

    pub(super) fn initialize_skill_lamps(
        &self,
        proto: &ProtoRegistry,
        state: &mut DynamicMessage,
    ) -> Result<(), StateError> {
        for (source_id, ability_ids) in &self.lamp_abilities {
            for ability_id in ability_ids {
                let rule = registry()?
                    .lamp_abilities
                    .get(ability_id)
                    .ok_or(StateError::InvalidRequest)?;
                add_lamp(state, *source_id, rule, rule.start)?;
                if rule.start_heal > 0 {
                    heal_all(proto, state, *source_id, rule.heal_effect_id, rule.start_heal)?;
                }
            }
        }
        Ok(())
    }

    fn trigger_lamps(
        &self,
        state: &mut DynamicMessage,
        trigger: &str,
        source_filter: Option<i32>,
    ) -> Result<Vec<(i32, i32)>, StateError> {
        let mut triggered = Vec::new();
        for (source_id, ability_ids) in &self.lamp_abilities {
            if source_filter.is_some_and(|id| id != *source_id) {
                continue;
            }
            for ability_id in ability_ids {
                let rule = registry()?
                    .lamp_abilities
                    .get(ability_id)
                    .ok_or(StateError::InvalidRequest)?;
                if rule.triggers.iter().any(|value| value == trigger) {
                    add_lamp(state, *source_id, rule, rule.increment)?;
                    triggered.push((*source_id, *ability_id));
                }
            }
        }
        Ok(triggered)
    }

    pub(crate) fn trigger_party_tool_effects(
        &self,
        proto: &ProtoRegistry,
        state: &mut DynamicMessage,
    ) -> Result<Vec<DynamicMessage>, StateError> {
        let triggered = self.trigger_lamps(state, "party_tool_after", None)?;
        let mut results = Vec::new();
        for (source_id, ability_id) in triggered {
            let rule = registry()?
                .lamp_abilities
                .get(&ability_id)
                .ok_or(StateError::InvalidRequest)?;
            if rule
                .heal_triggers
                .iter()
                .any(|trigger| trigger == "party_tool_after")
            {
                results.extend(heal_all(
                    proto,
                    state,
                    source_id,
                    rule.heal_effect_id,
                    rule.heal_value,
                )?);
            }
        }
        for passive in self.passives.iter().filter(|passive| {
            passive.rule.trigger.as_deref() == Some("party_tool_after")
        }) {
            if passive.rule.operation != "heal" {
                return Err(StateError::InvalidRequest);
            }
            results.extend(heal_all(
                proto,
                state,
                passive.source,
                passive.rule.id,
                passive.value,
            )?);
        }
        Ok(results)
    }

    pub(super) fn trigger_panel_lamps(
        &self,
        state: &mut DynamicMessage,
        actor_id: i32,
    ) -> Result<(), StateError> {
        self.trigger_lamps(state, "panel_acquired", Some(actor_id))?;
        Ok(())
    }

    pub(crate) fn trigger_lamps_after_action(
        &self,
        proto: &ProtoRegistry,
        state: &mut DynamicMessage,
        before: &DynamicMessage,
        actor_id: i32,
        skill: &TutorialSkill,
        results: &[DynamicMessage],
        is_skill: bool,
    ) -> Result<Vec<DynamicMessage>, StateError> {
        let valid_results = results
            .iter()
            .filter(|result| !bool_field(result, "is_miss") && !bool_field(result, "is_invalid"))
            .collect::<Vec<_>>();
        let mut heals = Vec::new();
        for (source_id, ability_ids) in &self.lamp_abilities {
            for ability_id in ability_ids {
                let rule = registry()?
                    .lamp_abilities
                    .get(ability_id)
                    .ok_or(StateError::InvalidRequest)?;
                let trigger = if is_skill
                    && *source_id == actor_id
                    && rule
                        .triggers
                        .iter()
                        .any(|trigger| trigger == "action_after")
                {
                    Some("action_after")
                } else if rule
                        .triggers
                        .iter()
                        .any(|trigger| trigger == "heal_received")
                        && valid_results.iter().any(|result| {
                            i32_field(result, "target_id") == Some(*source_id)
                                && message_i32_field(result, "hp_heal", "value").unwrap_or(0) > 0
                        })
                {
                    Some("heal_received")
                } else if skill.skill_effect_type == 1
                        && rule.triggers.iter().any(|trigger| trigger == "attacked")
                        && valid_results
                            .iter()
                            .any(|result| i32_field(result, "target_id") == Some(*source_id))
                {
                    Some("attacked")
                } else if is_skill
                        && *source_id == actor_id
                        && rule
                            .triggers
                            .iter()
                            .any(|trigger| trigger == "weak_attack_after")
                        && rule.trigger_skill_ids.contains(&skill.id)
                        && valid_results
                            .iter()
                            .any(|result| bool_field(result, "is_weak"))
                {
                    Some("weak_attack_after")
                } else if is_skill
                        && *source_id == actor_id
                        && rule
                            .triggers
                            .iter()
                            .any(|trigger| trigger == "abnormal_attack_after")
                        && valid_results.iter().any(|result| {
                            let target_id = i32_field(result, "target_id");
                            message_list(before, "members")
                                .iter()
                                .find(|member| i32_field(member, "member_id") == target_id)
                                .is_some_and(|target| {
                                    message_list(target, "state_changes").iter().any(|change| {
                                        registry().ok().is_some_and(|data| {
                                            data.abnormal_state_ids.contains(
                                                &i32_field(change, "state_change_id")
                                                    .unwrap_or_default(),
                                            )
                                        })
                                    })
                                })
                        })
                {
                    Some("abnormal_attack_after")
                } else {
                    None
                };
                if let Some(trigger) = trigger {
                    add_lamp(state, *source_id, rule, rule.increment)?;
                    if rule.heal_triggers.iter().any(|value| value == trigger) {
                        heals.push((*source_id, rule.heal_effect_id, rule.heal_value));
                    }
                }
            }
        }
        let mut results = Vec::new();
        for (source_id, effect_id, value) in heals {
            results.extend(heal_all(proto, state, source_id, effect_id, value)?);
        }
        Ok(results)
    }

    pub(super) fn lamp_skill_damage(&self, member: &DynamicMessage) -> i64 {
        let source_id = member_id(member).unwrap_or_default();
        self.lamp_abilities
            .get(&source_id)
            .into_iter()
            .flatten()
            .filter_map(|ability_id| registry().ok()?.lamp_abilities.get(ability_id))
            .filter_map(|rule| {
                let lamp = rule
                    .target_skill_ids
                    .iter()
                    .find_map(|skill_id| current_lamp(member, *skill_id).ok())?;
                usize::try_from(lamp.checked_sub(1)?)
                    .ok()
                    .and_then(|index| rule.damage_by_lamp.get(index))
                    .copied()
            })
            .map(i64::from)
            .sum()
    }
}

pub(crate) fn advance_skill_lamp(
    state: &mut DynamicMessage,
    source_id: i32,
    skill_id: i32,
) -> Result<(), StateError> {
    let Some(rule) = registry()?.lamp_skills.get(&skill_id) else {
        return Ok(());
    };
    let mut members = message_list(state, "members");
    let member = members
        .iter_mut()
        .find(|member| member_id(member).ok() == Some(source_id))
        .ok_or(StateError::InvalidRequest)?;
    let mut ally = member_status(member, "ally")?;
    let mut skills = message_list(&ally, "skills");
    let skill = skills
        .iter_mut()
        .find(|skill| i32_field(skill, "skill_id") == Some(skill_id))
        .ok_or(StateError::InvalidRequest)?;
    let current = i32_field(skill, "lamp").unwrap_or_default().max(0);
    skill.set_field_by_name(
        "lamp",
        Value::I32(next_lamp(
            current,
            rule.maximum,
            rule.increment,
            rule.clear_when_full,
        )),
    );
    ally.set_field_by_name(
        "skills",
        Value::List(skills.into_iter().map(Value::Message).collect()),
    );
    member.set_field_by_name("ally", Value::Message(ally));
    state.set_field_by_name(
        "members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn lamp_advances_and_clears_at_full() {
        assert_eq!(next_lamp(0, 3, 1, true), 1);
        assert_eq!(next_lamp(2, 3, 1, true), 3);
        assert_eq!(next_lamp(3, 3, 1, true), 0);
        assert_eq!(next_lamp(2, 2, 1, false), 2);
    }

    #[test]
    fn lamp_state_uses_start_skill_and_tool_transitions() {
        let proto = ProtoRegistry::from_file(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../schemas/atelier-resleriana-2.16.0.protoset"
        )))
        .unwrap();
        let mut skill = empty_message(&proto, "blend.model.BattleSkill").unwrap();
        skill.set_field_by_name("skill_id", Value::I32(12001539));
        skill.set_field_by_name("skill_type", Value::I32(2));
        skill.set_field_by_name("lamp", Value::I32(0));
        let mut ally = empty_message(&proto, "blend.model.BattleAlly").unwrap();
        ally.set_field_by_name("skills", Value::List(vec![Value::Message(skill)]));
        let mut member = empty_message(&proto, "blend.model.BattleMember").unwrap();
        member.set_field_by_name("member_id", Value::I32(1));
        member.set_field_by_name("ally", Value::Message(ally));
        let mut state = empty_message(&proto, "blend.model.BattleState").unwrap();
        state.set_field_by_name("members", Value::List(vec![Value::Message(member)]));
        let mut runtime = Runtime::default();
        runtime.lamp_abilities.entry(1).or_default().insert(1990220);

        runtime.initialize_skill_lamps(&proto, &mut state).unwrap();
        let members = message_list(&state, "members");
        let member = &members[0];
        assert_eq!(current_lamp(member, 12001539).unwrap(), 2);
        assert_eq!(predicted_skill_lamp(member, 12001539).unwrap(), Some(0));

        advance_skill_lamp(&mut state, 1, 12001539).unwrap();
        runtime.trigger_party_tool_effects(&proto, &mut state).unwrap();
        let members = message_list(&state, "members");
        let member = &members[0];
        assert_eq!(current_lamp(member, 12001539).unwrap(), 1);
        assert_eq!(predicted_skill_lamp(member, 12001539).unwrap(), Some(2));
    }

    #[test]
    fn full_lamp_uses_generated_transformation_and_lamp_damage() {
        let proto = ProtoRegistry::from_file(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../schemas/atelier-resleriana-2.16.0.protoset"
        )))
        .unwrap();
        let mut skill = empty_message(&proto, "blend.model.BattleSkill").unwrap();
        skill.set_field_by_name("skill_id", Value::I32(12001714));
        skill.set_field_by_name("lamp", Value::I32(3));
        let mut ally = empty_message(&proto, "blend.model.BattleAlly").unwrap();
        ally.set_field_by_name("skills", Value::List(vec![Value::Message(skill)]));
        let mut member = empty_message(&proto, "blend.model.BattleMember").unwrap();
        member.set_field_by_name("member_id", Value::I32(1));
        member.set_field_by_name("ally", Value::Message(ally));
        assert_eq!(skill_transformation(&member, 12001714).unwrap(), Some((91001369, 12001699)));

        let mut damage_skill = empty_message(&proto, "blend.model.BattleSkill").unwrap();
        damage_skill.set_field_by_name("skill_id", Value::I32(12002331));
        damage_skill.set_field_by_name("lamp", Value::I32(2));
        let mut ally = empty_message(&proto, "blend.model.BattleAlly").unwrap();
        ally.set_field_by_name("skills", Value::List(vec![Value::Message(damage_skill)]));
        member.set_field_by_name("ally", Value::Message(ally));
        let mut runtime = Runtime::default();
        runtime.lamp_abilities.entry(1).or_default().insert(1990330);
        assert_eq!(runtime.lamp_skill_damage(&member), 7_000);
    }
}
