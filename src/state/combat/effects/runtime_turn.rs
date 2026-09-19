use super::registry::Expiry;
use super::runtime::{Instance, Runtime};
use super::runtime_lamp::{heal_all, heal_self};
use super::runtime_match::{condition, context_matches, selected_for_source_character};
use super::runtime_panel::scale_panel_value;
use super::runtime_resources::{
    add_party_gauge, apply_burst_gauge_passive, apply_party_gauge_passive,
};
use super::runtime_results::{effect_result, turn_state_change_result};
use crate::state::combat::prelude::*;
use std::collections::BTreeSet;

impl Runtime {
    pub(crate) fn blind_rate(&self, actor: i32) -> i32 {
        (self.pending_actor == actor)
            .then_some(self.pending_blind_rate)
            .unwrap_or_default()
    }

    pub(crate) fn evasion_rate(&self, target: i32) -> i32 {
        self.instances
            .iter()
            .filter(|instance| instance.target == target && instance.rule.operation == "evasion")
            .fold(0i32, |total, instance| total.saturating_add(instance.value))
            .clamp(0, 10_000)
    }

    pub(crate) fn provocation_target(&self, actor: i32) -> Option<i32> {
        (self.pending_actor == actor)
            .then_some(self.pending_provocation_target)
            .flatten()
    }

    pub(crate) fn prepare_turn(
        &mut self,
        proto: &ProtoRegistry,
        state: &mut DynamicMessage,
        actor: i32,
        secret: &[u8],
        start_txid: &str,
        action_number: i32,
    ) -> Result<(Vec<DynamicMessage>, bool, bool), StateError> {
        if self.pending_actor == actor {
            return Ok((Vec::new(), false, false));
        }
        let mut members = message_list(state, "members");
        let actor_index = members
            .iter()
            .position(|member| i32_field(member, "member_id") == Some(actor))
            .ok_or(StateError::InvalidRequest)?;
        let maximum_hp = i64::from(
            i32_field(&members[actor_index], "max_hp")
                .ok_or(StateError::InvalidRequest)?
                .max(1),
        );
        let snapshot = members.clone();
        let mut hp = i32_field(&members[actor_index], "hp")
            .ok_or(StateError::InvalidRequest)?
            .max(0);
        let mut blind_rate = 0;
        let mut provocation_target = None;
        let mut disabled = false;
        let mut results = Vec::new();
        let mut retained = Vec::new();
        for (index, mut change) in message_list(&members[actor_index], "state_changes")
            .into_iter()
            .enumerate()
        {
            let state_id = i32_field(&change, "state_change_id").unwrap_or_default();
            let value = i32_field(&change, "value").unwrap_or_default();
            let remaining = i32_field(&change, "rest_count").unwrap_or_default();
            let handled = matches!(
                state_id,
                910037 | 910059 | 940001 | 940002 | 940005 | 940006 | 940007
            );
            if state_id == 910037 {
                let heal = if value > 0 {
                    (maximum_hp.saturating_mul(i64::from(value)) / 10_000).max(1)
                } else {
                    0
                };
                let heal = i32::try_from(heal).unwrap_or(i32::MAX);
                hp = hp.saturating_add(heal).min(maximum_hp as i32);
                results.push(turn_state_change_result(
                    proto, actor, state_id, 0, heal, false,
                )?);
            } else if matches!(state_id, 940006 | 940007) {
                let damage = if value > 0 {
                    (maximum_hp.saturating_mul(i64::from(value)) / 10_000).max(1)
                } else {
                    0
                };
                hp = hp
                    .saturating_sub(i32::try_from(damage).unwrap_or(i32::MAX))
                    .max(0);
                results.push(turn_state_change_result(
                    proto, actor, state_id, damage, 0, false,
                )?);
            } else if state_id == 910059 {
                disabled = true;
                results.push(turn_state_change_result(
                    proto, actor, state_id, 0, 0, true,
                )?);
            } else if state_id == 940005 {
                if deterministic_roll(
                    secret,
                    start_txid,
                    action_number,
                    b"paralysis",
                    actor,
                    u32::try_from(index).unwrap_or(u32::MAX),
                ) % 10_000
                    < value.clamp(0, 10_000) as u32
                {
                    disabled = true;
                    results.push(turn_state_change_result(
                        proto, actor, state_id, 0, 0, true,
                    )?);
                }
            } else if state_id == 940002 {
                blind_rate = blind_rate.max(value);
            } else if state_id == 940001 {
                provocation_target = optional_i32_field(&change, "target_id").filter(|target| {
                    snapshot.iter().any(|member| {
                        i32_field(member, "member_id") == Some(*target)
                            && bool_field(member, "is_alive")
                    })
                });
            }
            if handled && remaining > 0 {
                change.set_field_by_name("rest_count", Value::I32(remaining - 1));
            }
            if !handled || remaining < 0 || remaining > 1 {
                retained.push(change);
            }
        }
        let killed = hp == 0;
        members[actor_index].set_field_by_name("hp", Value::I32(hp));
        members[actor_index].set_field_by_name("is_alive", Value::Bool(!killed));
        members[actor_index].set_field_by_name(
            "is_stun",
            Value::Bool(
                retained
                    .iter()
                    .any(|change| i32_field(change, "state_change_id") == Some(910059)),
            ),
        );
        members[actor_index].set_field_by_name(
            "state_changes",
            Value::List(retained.into_iter().map(Value::Message).collect()),
        );
        state.set_field_by_name(
            "members",
            Value::List(members.into_iter().map(Value::Message).collect()),
        );
        let mut applied = BTreeSet::new();
        for passive in self.passives.iter().filter(|passive| {
            passive.source == actor
                && passive.rule.operation == "party_gauge"
                && passive.rule.trigger.as_deref() == Some("turn_start")
        }) {
            if applied.insert((passive.rule.owner_id, passive.rule.id)) {
                results.push(apply_party_gauge_passive(proto, state, passive)?);
            }
        }
        self.pending_actor = actor;
        self.pending_blind_rate = blind_rate.clamp(0, 10_000);
        self.pending_provocation_target = provocation_target;
        Ok((results, disabled, killed))
    }

    fn finish_turn(&mut self, actor: i32) {
        if self.pending_actor == actor {
            self.pending_actor = 0;
            self.pending_blind_rate = 0;
            self.pending_provocation_target = None;
        }
    }

    pub(crate) fn expire(
        &mut self,
        actor: i32,
        results: &[DynamicMessage],
        consumes_turn: bool,
        attack: bool,
    ) {
        self.expire_with_attributes(actor, results, consumes_turn, attack, &[]);
    }

    pub(crate) fn expire_with_attributes(
        &mut self,
        actor: i32,
        results: &[DynamicMessage],
        consumes_turn: bool,
        attack: bool,
        attack_attributes: &[i32],
    ) {
        let hits: BTreeSet<_> = results
            .iter()
            .filter(|r| attack && !bool_field(r, "is_miss") && !bool_field(r, "is_invalid"))
            .filter_map(|r| i32_field(r, "target_id"))
            .collect();
        let attacked: BTreeSet<_> = results
            .iter()
            .filter(|result| attack && !bool_field(result, "is_invalid"))
            .filter_map(|result| i32_field(result, "target_id"))
            .collect();
        for instance in &mut self.instances {
            let matching_attribute = instance.rule.attack_attributes.is_empty()
                || attack_attributes.is_empty()
                || instance
                    .rule
                    .attack_attributes
                    .iter()
                    .any(|attribute| attack_attributes.contains(attribute));
            let tick = match instance.rule.expiry {
                Expiry::Turn => consumes_turn && instance.target == actor,
                Expiry::Attack => attack && consumes_turn && instance.target == actor,
                Expiry::Hit => matching_attribute && hits.contains(&instance.target),
                Expiry::Attacked => matching_attribute && attacked.contains(&instance.target),
                Expiry::Negative => false,
                Expiry::Permanent => false,
            };
            if tick && instance.remaining > 0 {
                instance.remaining -= 1;
            }
        }
        self.instances.retain(|i| i.remaining != 0);
        for target in hits {
            self.panel_damage_taken.remove(&target);
        }
        if consumes_turn {
            self.finish_turn(actor);
        }
    }

    pub(crate) fn acquire_current_panel(
        &mut self,
        proto: &ProtoRegistry,
        rules: &TutorialRules,
        state: &mut DynamicMessage,
    ) -> Result<(), StateError> {
        if current_battle_status(state)? != BATTLE_STATUS_IN_BATTLE {
            return Ok(());
        }
        let wave = i32_field(state, "wave").unwrap_or(1);
        let turn = message_list(state, "timeline_panels")
            .first()
            .and_then(|panel| i32_field(panel, "turn"))
            .ok_or(StateError::InvalidRequest)?;
        if self.acquired_panel_wave == wave && self.acquired_panel_turn == turn {
            return Ok(());
        }
        self.acquired_panel_wave = wave;
        self.acquired_panel_turn = turn;
        let actor_id = member_id(&current_actor(state)?)?;
        let panel_rate = self.panel_effect_rate(state)?;
        self.trigger_panel_lamps(state, actor_id)?;
        let mut applied = BTreeSet::new();
        for passive in self.passives.iter().filter(|passive| {
            passive.source == actor_id
                && passive.rule.trigger.as_deref() == Some("panel_acquired")
        }) {
            if !applied.insert((passive.source, passive.rule.owner_id, passive.rule.id)) {
                continue;
            }
            match passive.rule.operation.as_str() {
                "party_gauge" => {
                    add_party_gauge(state, passive.value)?;
                }
                "burst_gauge" => {
                    apply_burst_gauge_passive(proto, rules, state, passive)?;
                }
                _ => return Err(StateError::InvalidRequest),
            }
        }
        let actor_type = member_type(&current_actor(state)?)?;
        let panel_id = self.effective_panel_id(state)?;
        let mut members = message_list(state, "members");
        for member in members.iter_mut().filter(|member| {
            member_type(member).ok() == Some(actor_type) && bool_field(member, "is_alive")
        }) {
            let id = member_id(member)?;
            if matches!(panel_id, 33 | 36) {
                let delta = if panel_id == 33 {
                    -scale_panel_value(4_000, panel_rate)
                } else {
                    4_000
                };
                self.panel_damage_taken
                    .entry(id)
                    .and_modify(|value| *value = value.saturating_add(delta))
                    .or_insert(delta);
            } else if panel_id == 42 {
                let hp = i32_field(member, "hp").unwrap_or(0).max(0);
                let max_hp = i32_field(member, "max_hp").unwrap_or(0).max(0);
                let rate = i64::from(scale_panel_value(2_500, panel_rate));
                let heal = i64::from(max_hp).saturating_mul(rate) / 10_000;
                member.set_field_by_name(
                    "hp",
                    Value::I32(
                        hp.saturating_add(i32::try_from(heal).unwrap_or(i32::MAX))
                            .min(max_hp),
                    ),
                );
            }
        }
        state.set_field_by_name(
            "members",
            Value::List(members.into_iter().map(Value::Message).collect()),
        );
        Ok(())
    }

    pub(crate) fn trigger_attack_after(
        &mut self,
        proto: &ProtoRegistry,
        rules: &TutorialRules,
        state: &mut DynamicMessage,
        source_id: i32,
        skill: &TutorialSkill,
        results: &[DynamicMessage],
    ) -> Result<Vec<DynamicMessage>, StateError> {
        let members = message_list(state, "members");
        let source = members
            .iter()
            .find(|member| i32_field(member, "member_id") == Some(source_id))
            .ok_or(StateError::InvalidRequest)?;
        let source_type = member_type(source)?;
        let mut triggered = Vec::new();
        let mut applied = BTreeSet::new();
        for passive in self.passives.clone().into_iter().filter(|passive| {
            match passive.rule.trigger.as_deref() {
                Some("action_after") => match passive.rule.target.as_str() {
                    "self" => passive.source == source_id,
                    "allies" => passive.source_type == source_type,
                    _ => false,
                },
                Some("party_action_after") => passive.source_type == source_type,
                _ => false,
            }
        }) {
            if matches!(passive.rule.operation.as_str(), "party_gauge" | "burst_gauge")
                && !applied.insert((passive.source, passive.rule.owner_id, passive.rule.id))
            {
                continue;
            }
            match passive.rule.operation.as_str() {
                "bomb_gauge" => {
                    let maximum = rules.constants.max_bomb_gauge;
                    let current = i32_field(state, "bomb_gauge").unwrap_or_default().max(0);
                    let delta =
                        i64::from(maximum).saturating_mul(i64::from(passive.value)) / 10_000;
                    let next = i64::from(current)
                        .saturating_add(delta)
                        .clamp(0, i64::from(maximum)) as i32;
                    state.set_field_by_name("bomb_gauge", Value::I32(next));
                    let mut result = effect_result(
                        proto,
                        passive.rule.id,
                        passive.source,
                        source_id,
                        false,
                        &passive.rule,
                        passive.value,
                    )?;
                    result.set_field_by_name(
                        "add_bomb_gauge",
                        Value::Message(wrapper_i32(proto, next - current)?),
                    );
                    triggered.push(result);
                }
                "heal" => triggered.extend(heal_all(
                    proto,
                    state,
                    passive.source,
                    passive.rule.id,
                    passive.value,
                )?),
                "party_gauge" => {
                    triggered.push(apply_party_gauge_passive(proto, state, &passive)?)
                }
                "burst_gauge" => triggered.push(apply_burst_gauge_passive(
                    proto, rules, state, &passive,
                )?),
                _ => return Err(StateError::InvalidRequest),
            }
        }
        if skill.skill_effect_type != 1 {
            return Ok(triggered);
        }
        let hit_results = results
            .iter()
            .filter(|result| !bool_field(result, "is_miss") && !bool_field(result, "is_invalid"));
        for passive in self.passives.clone().into_iter().filter(|passive| {
            passive.source == source_id && passive.rule.trigger.as_deref() == Some("attack_after")
        }) {
            if passive.rule.operation == "heal" {
                if bool_field(source, "is_alive")
                    && context_matches(
                        &passive.rule,
                        passive.source_character_id,
                        skill,
                        false,
                    )
                    && condition(&passive.rule, source)
                {
                    triggered.extend(heal_self(
                        proto,
                        state,
                        source_id,
                        passive.rule.id,
                        passive.value,
                    )?);
                }
                continue;
            }
            for result in hit_results.clone() {
                let Some(target_id) = i32_field(result, "target_id") else {
                    continue;
                };
                let Some(target) = members
                    .iter()
                    .find(|member| i32_field(member, "member_id") == Some(target_id))
                else {
                    continue;
                };
                if !bool_field(target, "is_alive")
                    || !context_matches(
                        &passive.rule,
                        passive.source_character_id,
                        skill,
                        bool_field(result, "is_critical"),
                    )
                    || !selected_for_source_character(
                        &passive.rule,
                        source,
                        passive.source_character_id,
                        target,
                        &[target_id],
                    )
                    || !condition(&passive.rule, source)
                {
                    continue;
                }
                self.instances.retain(|instance| {
                    !(instance.source == source_id
                        && instance.target == target_id
                        && instance.rule.id == passive.rule.id)
                });
                self.instances.push(Instance {
                    source: source_id,
                    source_character_id: passive.source_character_id,
                    target: target_id,
                    value: passive.value,
                    remaining: passive.rule.duration,
                    rule: passive.rule.clone(),
                });
                self.managed
                    .entry(target_id)
                    .or_default()
                    .insert(passive.rule.state_id);
                triggered.push(effect_result(
                    proto,
                    passive.rule.id,
                    source_id,
                    target_id,
                    true,
                    &passive.rule,
                    passive.value,
                )?);
            }
        }
        triggered.extend(self.apply_critical_skill_effects(
            proto, rules, state, source_id, skill, results,
        )?);
        self.refresh(proto, state)?;
        Ok(triggered)
    }
}
