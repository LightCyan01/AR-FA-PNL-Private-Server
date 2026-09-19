use super::registry::{registry, rule_for_occurrence, Expiry, Rule};
use super::runtime::{Baseline, Instance, NestedActionInstance, Passive, Runtime};
use super::runtime_lamp::is_lamp_ability_effect;
use super::runtime_match::{amount, condition, contextual_rule, selected_for_source_character};
use super::runtime_resources::{add_burst_gauge, add_party_gauge};
use super::runtime_results::{display, level_display};
use crate::state::combat::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

impl Runtime {
    fn register_nested_ability(&mut self, source: i32, ability_id: i32) -> Result<(), StateError> {
        for rule in registry()?
            .nested_actions
            .iter()
            .filter(|rule| rule.owner_type == "ability" && rule.owner_id == ability_id)
        {
            if rule.state_id > 0 {
                self.managed
                    .entry(source)
                    .or_default()
                    .insert(rule.state_id);
            }
            self.nested_actions.push(NestedActionInstance {
                source,
                target: source,
                remaining: rule.duration,
                rule: rule.clone(),
            });
        }
        Ok(())
    }

    fn register_passive(
        &mut self,
        source: i32,
        source_character_id: i32,
        source_type: i32,
        ability_id: i32,
        effect_index: Option<usize>,
        effect: &TutorialSkillEffect,
        leader: bool,
    ) -> Result<(), StateError> {
        if is_lamp_ability_effect(ability_id, effect.id)?
            || rule_for_occurrence(
                effect.id,
                "catalog",
                "ability",
                ability_id,
                effect_index,
            )?
            .is_some()
        {
            return Ok(());
        }
        let Some(rule) = rule_for_occurrence(
            effect.id,
            "passive",
            "ability",
            ability_id,
            effect_index,
        )?
        else {
            self.unsupported.insert(effect.id);
            return Ok(());
        };
        let mut rule = rule.clone();
        if leader {
            rule.condition.remove("party_tag_id");
        }
        let value = amount(&rule, effect.value)?;
        if rule.expiry == Expiry::Permanent || rule.trigger.is_some() {
            self.passives.push(Passive {
                source,
                value,
                rule,
                source_character_id,
                source_type,
            });
        } else {
            self.managed
                .entry(source)
                .or_default()
                .insert(rule.state_id);
            self.instances.push(Instance {
                source,
                source_character_id,
                target: source,
                value,
                remaining: rule.duration,
                rule,
            });
        }
        Ok(())
    }

    pub(crate) fn initialize(
        proto: &ProtoRegistry,
        rules: &TutorialRules,
        state: &mut DynamicMessage,
        transaction: &str,
        party: &[BattlePartyMember],
        external_passives: &[BattleExternalPassive],
    ) -> Result<Self, StateError> {
        let mut runtime = Self::default();
        runtime.prepare(state, transaction)?;
        for member in message_list(state, "members") {
            let id = member_id(&member)?;
            let Some(character) = message_i32_field(&member, "ally", "character_id") else {
                continue;
            };
            let source_type = member_type(&member)?;
            let Some(owner) = party.iter().find(|p| p.character_id == character) else {
                continue;
            };
            for passive in &owner.passives {
                runtime.register_passive(
                    id,
                    character,
                    source_type,
                    passive.ability_id,
                    passive.effect_index,
                    &passive.effect,
                    false,
                )?;
            }
            for passive in &owner.leader_passives {
                runtime.register_passive(
                    id,
                    character,
                    source_type,
                    passive.ability_id,
                    passive.effect_index,
                    &passive.effect,
                    true,
                )?;
            }
            for ability_id in &owner.ability_ids {
                runtime.register_nested_ability(id, *ability_id)?;
                runtime.register_lamp_ability(id, *ability_id)?;
            }
        }
        if !external_passives.is_empty() {
            let leader = party
                .iter()
                .find(|member| member.is_leader)
                .ok_or(StateError::InvalidRequest)?;
            let source = message_list(state, "members")
                .into_iter()
                .find(|member| {
                    message_i32_field(member, "ally", "character_id") == Some(leader.character_id)
                })
                .ok_or(StateError::InvalidRequest)?;
            let source_id = member_id(&source)?;
            for (source_character_id, ability_id, effect_index, effect) in external_passives {
                runtime.register_passive(
                    source_id,
                    source_character_id.unwrap_or(leader.character_id),
                    0,
                    *ability_id,
                    *effect_index,
                    effect,
                    false,
                )?;
            }
        }
        runtime.initialize_skill_lamps(proto, state)?;
        let mut start_gain = 0i64;
        let mut start_burst = BTreeMap::<i32, i32>::new();
        for passive in runtime
            .passives
            .iter()
            .filter(|passive| passive.rule.trigger.as_deref() == Some("battle_start"))
        {
            match passive.rule.operation.as_str() {
                "bomb_gauge" => {
                    start_gain = start_gain
                        .checked_add(i64::from(passive.value))
                        .ok_or(StateError::InvalidRequest)?;
                }
                "burst_gauge" => {
                    let value = start_burst.entry(passive.source).or_default();
                    *value = value
                        .checked_add(passive.value)
                        .ok_or(StateError::InvalidRequest)?;
                }
                "party_gauge" => {
                    add_party_gauge(state, passive.value)?;
                }
                "heal" => {
                    super::runtime_lamp::heal_all(
                        proto,
                        state,
                        passive.source,
                        passive.rule.id,
                        passive.value,
                    )?;
                }
                _ => return Err(StateError::InvalidRequest),
            }
        }
        if start_gain != 0 {
            let maximum = rules.constants.max_bomb_gauge;
            let delta = i64::from(maximum).saturating_mul(start_gain) / 10_000;
            let current = i32_field(state, "bomb_gauge").unwrap_or_default().max(0);
            state.set_field_by_name(
                "bomb_gauge",
                Value::I32(
                    i64::from(current)
                        .saturating_add(delta)
                        .clamp(0, i64::from(maximum)) as i32,
                ),
            );
        }
        if !start_burst.is_empty() {
            let mut members = message_list(state, "members");
            for (source, value) in start_burst {
                let member = members
                    .iter_mut()
                    .find(|member| member_id(member).ok() == Some(source))
                    .ok_or(StateError::InvalidRequest)?;
                add_burst_gauge(
                    member,
                    value,
                    rules.constants.burst_gauge_required_for_one_burst_skill,
                )?;
            }
            state.set_field_by_name(
                "members",
                Value::List(members.into_iter().map(Value::Message).collect()),
            );
        }
        runtime.refresh(proto, state)?;
        Ok(runtime)
    }

    pub(crate) fn prepare(
        &mut self,
        state: &DynamicMessage,
        transaction: &str,
    ) -> Result<(), StateError> {
        if self.transaction != transaction {
            *self = Self {
                transaction: transaction.to_owned(),
                ..Self::default()
            };
            self.capture(state)?;
            // Older persisted battles carried these three effects directly. Import
            // them once, subtracting their contribution from the immutable baseline.
            for member in message_list(state, "members") {
                let target = member_id(&member)?;
                for change in message_list(&member, "state_changes") {
                    let state_id = i32_field(&change, "state_change_id").unwrap_or(0);
                    let effect = match state_id {
                        910039 => 91001006,
                        920004 => 3000047,
                        920003 => 3000049,
                        _ => continue,
                    };
                    let rule = registry()?
                        .rules
                        .iter()
                        .find(|rule| rule.id == effect && rule.mode == "active")
                        .ok_or(StateError::InvalidRequest)?
                        .clone();
                    let value = i32_field(&change, "value").unwrap_or(0);
                    let remaining = i32_field(&change, "rest_count").unwrap_or(0);
                    if remaining <= 0 {
                        continue;
                    }
                    if rule.summary != 0 {
                        let base = self
                            .bases
                            .get_mut(&target)
                            .ok_or(StateError::InvalidRequest)?;
                        *base.summaries.entry(rule.summary).or_default() -= value;
                    }
                    self.managed.entry(target).or_default().insert(state_id);
                    self.instances.push(Instance {
                        source: target,
                        source_character_id: message_i32_field(&member, "ally", "character_id")
                            .unwrap_or_default(),
                        target,
                        value,
                        remaining,
                        rule,
                    });
                }
            }
        }
        if self.next_action_number <= 0 {
            self.next_action_number = i32_field(state, "total_turn").unwrap_or(1).max(1);
        }
        self.capture(state)
    }

    fn capture(&mut self, state: &DynamicMessage) -> Result<(), StateError> {
        let wave = i32_field(state, "wave").unwrap_or(1);
        if self.wave != 0 && self.wave != wave {
            // Enemy IDs 11+ are reused for each replacement wave. Neither old
            // debuffs nor an earlier enemy's base stats may follow that ID.
            let allies: BTreeSet<_> = message_list(state, "members")
                .iter()
                .filter(|m| member_type(m).ok() == Some(0))
                .filter_map(|m| i32_field(m, "member_id"))
                .collect();
            self.bases.retain(|id, _| allies.contains(id));
            self.managed.retain(|id, _| allies.contains(id));
            self.instances.retain(|i| allies.contains(&i.target));
            self.passives.retain(|p| allies.contains(&p.source));
            self.nested_actions
                .retain(|action| allies.contains(&action.target));
            self.extra_skills.retain(|id, _| allies.contains(id));
            self.extra_turn_uses.retain(|id, _| allies.contains(id));
            self.pending_extra_turns.clear();
            self.panel_damage_taken.retain(|id, _| allies.contains(id));
            self.pending_actor = 0;
            self.pending_blind_rate = 0;
            self.pending_provocation_target = None;
        }
        self.wave = wave;
        for member in message_list(state, "members") {
            let id = member_id(&member)?;
            if self.bases.contains_key(&id) {
                continue;
            }
            let status = member_status(&member, "current_status")?;
            let stats = status
                .fields()
                .filter_map(|(field, value)| value.as_i32().map(|v| (field.name().to_string(), v)))
                .collect();
            let summaries = message_list(&member, "state_change_summaries")
                .iter()
                .filter_map(|r| Some((i32_field(r, "id")?, i32_field(r, "value").unwrap_or(0))))
                .collect();
            self.bases.insert(id, Baseline { stats, summaries });
        }
        Ok(())
    }

    pub(crate) fn refresh(
        &mut self,
        proto: &ProtoRegistry,
        state: &mut DynamicMessage,
    ) -> Result<(), StateError> {
        self.capture(state)?;
        let snapshot = message_list(state, "members");
        for passive in &mut self.passives {
            if let Some(source) = snapshot
                .iter()
                .find(|member| i32_field(member, "member_id") == Some(passive.source))
            {
                // Backfill metadata for runtimes persisted before contextual
                // passive ownership was serialized.
                passive.source_character_id =
                    message_i32_field(source, "ally", "character_id").unwrap_or_default();
                passive.source_type = member_type(source)?;
            }
        }
        let mut members = snapshot.clone();
        self.stat_rates.clear();
        for member in &mut members {
            let id = member_id(member)?;
            let base = self.bases.get(&id).ok_or(StateError::InvalidRequest)?;
            let mut summaries = base.summaries.clone();
            let mut level_summaries = BTreeMap::<i32, i32>::new();
            let mut stat_rates = BTreeMap::<String, i64>::new();
            let mut visible = BTreeMap::<i32, (i32, i32)>::new();
            let mut levels = BTreeMap::<i32, (i32, i32)>::new();
            let mut add = |rule: &Rule, value: i32| -> Result<(), StateError> {
                match rule.operation.as_str() {
                    "summary" => {
                        let current = summaries.entry(rule.summary).or_default();
                        *current = current
                            .checked_add(value)
                            .ok_or(StateError::InvalidRequest)?;
                    }
                    "attack" | "magic" | "defense" | "mental" | "speed" => {
                        *stat_rates.entry(rule.operation.clone()).or_default() += i64::from(value);
                    }
                    "physical_taken" | "magic_taken" | "taken_down" | "attribute_taken"
                    | "panel_disable" | "panel_convert" | "panel_potency"
                    | "healing" | "healing_received"
                    | "regeneration" | "negative_immunity" | "abnormal_immunity"
                    | "positive_immunity" | "negative_potency" | "positive_potency"
                    | "given_negative_potency" | "given_positive_potency"
                    | "damage_immunity" | "cover" | "cleanse_positive"
                    | "evasion" | "abnormal_resistance" | "target_rate" | "status"
                    | "initiative" | "marker" => (), // read at use sites
                    _ => return Err(StateError::InvalidRequest),
                }
                Ok(())
            };
            for passive in &self.passives {
                if let Some(source) = snapshot
                    .iter()
                    .find(|m| i32_field(m, "member_id") == Some(passive.source))
                {
                    if bool_field(source, "is_alive")
                        && condition(&passive.rule, source)
                        && selected_for_source_character(
                            &passive.rule,
                            source,
                            passive.source_character_id,
                            member,
                            &[],
                        )
                    {
                        if passive.rule.trigger.is_none()
                            && matches!(
                                passive.rule.operation.as_str(),
                                "physical_taken"
                                    | "magic_taken"
                                    | "taken_down"
                                    | "healing"
                                    | "healing_received"
                                    | "negative_immunity"
                                    | "abnormal_immunity"
                                    | "positive_immunity"
                            )
                        {
                            let entry = visible.entry(passive.rule.state_id).or_default();
                            entry.0 = entry
                                .0
                                .checked_add(passive.value)
                                .ok_or(StateError::InvalidRequest)?;
                            entry.1 = -1;
                            self.managed
                                .entry(id)
                                .or_default()
                                .insert(passive.rule.state_id);
                        } else if !contextual_rule(&passive.rule) {
                            add(&passive.rule, passive.value)?;
                        }
                    }
                }
            }
            for active in self.instances.iter().filter(|i| i.target == id) {
                if active.rule.operation == "level_state" {
                    for modifier in &active.rule.level_modifiers {
                        let value = modifier
                            .value
                            .checked_mul(active.value)
                            .ok_or(StateError::InvalidRequest)?;
                        let current = level_summaries.entry(modifier.summary).or_default();
                        *current = current
                            .checked_add(value)
                            .ok_or(StateError::InvalidRequest)?;
                    }
                    levels.insert(active.rule.state_id, (active.value, active.remaining));
                    continue;
                }
                // Application predicates are already satisfied before the instance exists.
                if !contextual_rule(&active.rule)
                    || active.rule.trigger.is_some()
                    || active.rule.target_broken
                {
                    add(&active.rule, active.value)?;
                }
                let entry = visible.entry(active.rule.state_id).or_default();
                entry.0 = entry
                    .0
                    .checked_add(active.value)
                    .ok_or(StateError::InvalidRequest)?;
                entry.1 = if active.remaining < 0 || entry.1 < 0 {
                    -1
                } else {
                    entry.1.max(active.remaining)
                };
            }
            drop(add);
            for (summary, value) in level_summaries {
                let current = summaries.entry(summary).or_default();
                *current = current
                    .checked_add(value)
                    .ok_or(StateError::InvalidRequest)?;
            }
            for action in self
                .nested_actions
                .iter()
                .filter(|action| action.target == id && action.rule.state_id > 0)
            {
                let entry = visible.entry(action.rule.state_id).or_default();
                entry.1 = if action.remaining < 0 || entry.1 < 0 {
                    -1
                } else {
                    entry.1.max(action.remaining)
                };
                self.managed
                    .entry(id)
                    .or_default()
                    .insert(action.rule.state_id);
            }
            let mut status = member_status(member, "current_status")?;
            for (name, base_value) in &base.stats {
                let rate =
                    (10_000 + stat_rates.get(name).copied().unwrap_or(0)).clamp(0, 1_000_000);
                let value = i64::from(*base_value) * rate / 10_000;
                status.set_field_by_name(
                    name,
                    Value::I32(
                        i32::try_from(value.max(1)).map_err(|_| StateError::InvalidRequest)?,
                    ),
                );
            }
            member.set_field_by_name("current_status", Value::Message(status));
            self.stat_rates.insert(id, stat_rates);
            let summaries = summaries
                .into_iter()
                .filter(|(_, v)| *v != 0)
                .map(|(id, value)| {
                    let mut summary = empty_message(proto, "blend.model.BattleStateChangeSummary")?;
                    summary.set_field_by_name("id", Value::I32(id));
                    summary.set_field_by_name("value", Value::I32(value));
                    Ok(Value::Message(summary))
                })
                .collect::<Result<Vec<_>, StateError>>()?;
            member.set_field_by_name("state_change_summaries", Value::List(summaries));
            let mut changes = message_list(member, "state_changes");
            changes.retain(|r| {
                !self
                    .managed
                    .get(&id)
                    .is_some_and(|ids| ids.contains(&i32_field(r, "state_change_id").unwrap_or(0)))
            });
            for (state_id, (value, remaining)) in visible {
                changes.push(display(proto, state_id, value, remaining)?);
            }
            for (state_id, (level, remaining)) in levels {
                changes.push(level_display(proto, state_id, level, remaining)?);
            }
            member.set_field_by_name(
                "state_changes",
                Value::List(changes.into_iter().map(Value::Message).collect()),
            );
        }
        state.set_field_by_name(
            "members",
            Value::List(members.into_iter().map(Value::Message).collect()),
        );
        Ok(())
    }
}
