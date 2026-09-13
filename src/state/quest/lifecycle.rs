use crate::state::service::prelude::*;

pub(crate) fn validate_quest(
    quest: &TutorialQuest,
    resources: &DynamicMessage,
    now: i64,
) -> Result<(), StateError> {
    if quest.start_at.is_some_and(|t| now < t) || quest.end_at.is_some_and(|t| now >= t) {
        return Err(StateError::OutOfSchedule);
    }
    if quest
        .predecessor_id
        .is_some_and(|id| quest_clear_count(resources, id) == 0)
        || quest
            .key_tasks
            .iter()
            .any(|task| total_task_count(resources, task.condition_id) < task.count)
        || quest.key_story_id.is_some_and(|id| {
            !message_list(resources, "character_story_states")
                .iter()
                .any(|s| {
                    i32_field(s, "character_story_id") == Some(id)
                        && i32_field(s, "clear_count").unwrap_or(0) > 0
                })
        })
        || (quest.max_clear_count > 0
            && quest_clear_count(resources, quest.id) >= quest.max_clear_count)
    {
        return Err(StateError::InvalidRequest);
    }
    Ok(())
}

pub(crate) fn charge_quest(
    proto: &ProtoRegistry,
    home_rules: &home::HomeRules,
    quest: &TutorialQuest,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    now: i64,
) -> Result<(), StateError> {
    validate_quest(quest, resources, now)?;
    if quest.stamina > 0 {
        let mut status = status_message(resources)?;
        let remaining = i32_field(&status, "stamina_when_updated")
            .unwrap_or(0)
            .checked_sub(quest.stamina)
            .filter(|v| *v >= 0)
            .ok_or(StateError::InvalidRequest)?;
        status.set_field_by_name("stamina_when_updated", Value::I32(remaining));
        status.set_field_by_name("stamina_updated_at", Value::Message(timestamp(proto, now)?));
        resources.set_field_by_name("status", Value::Message(status));
    }
    pay_resource_cost(proto, resources, quest.item_cost.as_ref())?;
    home::quest_attempt_progress(
        proto,
        home_rules,
        resources,
        changed,
        (quest.id, quest.stamina),
        1,
        now,
    )
}

pub(crate) fn clear(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    home_rules: &home::HomeRules,
    resources: &mut DynamicMessage,
    quest: &TutorialQuest,
    now: i64,
) -> Result<(DynamicMessage, Vec<Value>), StateError> {
    let first = quest_clear_count(resources, quest.id) == 0;
    let mut state = message_list(resources, "quest_states")
        .into_iter()
        .find(|q| i32_field(q, "quest_id") == Some(quest.id))
        .unwrap_or(empty_message(proto, "blend.model.QuestState")?);
    state.set_field_by_name("quest_id", Value::I32(quest.id));
    state.set_field_by_name(
        "clear_count",
        Value::I32(
            quest_clear_count(resources, quest.id)
                .checked_add(1)
                .ok_or(StateError::InvalidRequest)?,
        ),
    );
    upsert_quest_state(resources, state.clone());
    let mut changed = empty_message(proto, "blend.model.Resources")?;
    changed.set_field_by_name("quest_states", Value::List(vec![Value::Message(state)]));
    if quest.episode_type == 1 {
        let mut status = status_message(resources)?;
        let last = i32_field(&status, "last_main_story_quest_id")
            .unwrap_or(0)
            .max(quest.id);
        status.set_field_by_name("last_main_story_quest_id", Value::I32(last));
        resources.set_field_by_name("status", Value::Message(status.clone()));
        changed.set_field_by_name("status", Value::Message(status));
    }
    let rewards = if first {
        quest
            .first_clear_reward_set_id
            .map(|id| {
                rules
                    .reward_sets
                    .iter()
                    .find(|r| r.id == id)
                    .ok_or(StateError::InvalidRequest)
            })
            .transpose()?
            .map(|r| r.rewards.as_slice())
            .unwrap_or(&[])
    } else {
        &[]
    };
    let granted = if rewards.is_empty() {
        Vec::new()
    } else {
        home::grant(proto, home_rules, resources, &mut changed, rewards, now)?
    };
    Ok((changed, granted))
}

pub(crate) fn finish_talk(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    home_rules: &home::HomeRules,
    mut resources: DynamicMessage,
    quest_id: i32,
    now: i64,
) -> Result<TalkMutation, StateError> {
    let quest = rules
        .quests
        .iter()
        .find(|q| q.id == quest_id && q.quest_type == 2 && q.talk_event_id.is_some())
        .ok_or(StateError::InvalidRequest)?;
    validate_quest(quest, &resources, now)?;
    let (changed, rewards) = clear(proto, rules, home_rules, &mut resources, quest, now)?;
    let mut response = empty_message(proto, "blend.api.QuestTalkEventFinishResponse")?;
    response.set_field_by_name("changed_resources", Value::Message(changed));
    response.set_field_by_name("first_clear_rewards", Value::List(rewards));
    Ok(TalkMutation {
        resources,
        response,
        granted_character_id: None,
    })
}

fn apply_battle_missions(
    proto: &ProtoRegistry,
    rules: &RewardRules,
    quest_id: i32,
    dead_allies: usize,
    total_turn: i32,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    result: &mut DynamicMessage,
) -> Result<(), StateError> {
    let Some(quest) = rules.quests.iter().find(|quest| quest.id == quest_id) else {
        return Ok(());
    };
    if quest.battle_mission_ids.is_empty() {
        return Ok(());
    }
    let mut cleared = Vec::new();
    for id in &quest.battle_mission_ids {
        let mission = rules
            .battle_missions
            .iter()
            .find(|mission| mission.id == *id)
            .ok_or_else(|| StateError::RewardRules(format!("missing battle mission {id}")))?;
        let achieved = match mission.kind {
            BattleMissionKind::Clear => true,
            BattleMissionKind::NoIncapacitated => dead_allies == 0,
            BattleMissionKind::TurnLimit => total_turn <= mission.turn_limit.unwrap_or(0),
        };
        if achieved {
            cleared.push(*id);
        }
    }
    let mut quest_state = message_list(resources, "quest_states")
        .into_iter()
        .find(|state| i32_field(state, "quest_id") == Some(quest_id))
        .ok_or(StateError::InvalidRequest)?;
    let old: BTreeSet<_> = i32_list(&quest_state, "cleared_battle_mission_ids")
        .into_iter()
        .collect();
    let all: BTreeSet<_> = old.iter().copied().chain(cleared.iter().copied()).collect();
    quest_state.set_field_by_name(
        "cleared_battle_mission_ids",
        Value::List(all.into_iter().map(Value::I32).collect()),
    );
    upsert_quest_state(resources, quest_state.clone());
    upsert_quest_state(changed, quest_state);

    let mut mission_result = empty_message(proto, "blend.model.BattleMissionResult")?;
    mission_result.set_field_by_name(
        "old_cleared_ids",
        Value::List(old.into_iter().map(Value::I32).collect()),
    );
    mission_result.set_field_by_name(
        "cleared_ids",
        Value::List(cleared.into_iter().map(Value::I32).collect()),
    );
    result.set_field_by_name("mission_result", Value::Message(mission_result));
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn finish_battle(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    reward_rules: &RewardRules,
    home_rules: &home::HomeRules,
    character_rules: &CharacterRules,
    battle_progress: &home::BattleProgress,
    state: DynamicMessage,
    mut resources: DynamicMessage,
    quest_id: i32,
) -> Result<BattleFinishMutation, StateError> {
    if current_battle_status(&state)? != BATTLE_STATUS_WON
        || i32_field(&state, "wave") != Some(i32_list(&state, "wave_ids").len() as i32)
    {
        return Err(StateError::InvalidRequest);
    }
    let battle_id = i32_field(&state, "battle_id");
    let quest = rules
        .quests
        .iter()
        .find(|q| {
            q.id == quest_id
                && q.quest_type == 1
                && (q.battle_id == battle_id
                    || q.battle_ids.contains(&battle_id.unwrap_or_default()))
        })
        .ok_or(StateError::InvalidRequest)?;
    let (mut changed, first) = clear(proto, rules, home_rules, &mut resources, quest, unix_now())?;
    let score = super::score::apply_battle_score(
        proto,
        rules,
        quest,
        &state,
        battle_progress,
        &mut resources,
        &mut changed,
    )?;
    let damage_contest_result = super::score::apply_damage_contest_score(
        proto,
        quest,
        battle_progress,
        &mut resources,
        &mut changed,
    )?;
    let rewards = roll_ranked_quest_rewards(
        reward_rules,
        quest_id,
        score.as_ref().map(|outcome| outcome.rank),
    )?;
    let rewards = home::grant(
        proto,
        home_rules,
        &mut resources,
        &mut changed,
        &rewards,
        unix_now(),
    )?;
    if quest.character_exp > 0
        && quest.fixed_party_id.is_none()
        && quest.rental_fixed_party_id.is_none()
    {
        for member in message_list(&state, "members")
            .iter()
            .filter(|m| member_type(m).ok() == Some(0))
        {
            let id = i32_field(&member_status(member, "ally")?, "character_id")
                .ok_or(StateError::InvalidRequest)?;
            character_exp(
                proto,
                character_rules,
                &mut resources,
                &mut changed,
                id,
                quest.character_exp,
            )?;
        }
    }
    let mut result = empty_message(proto, "blend.model.QuestResult")?;
    result.set_field_by_name("rewards", Value::List(rewards));
    result.set_field_by_name("first_clear_rewards", Value::List(first));
    if let Some(score) = score {
        result.set_field_by_name("score_detail", Value::Message(score.detail));
        result.set_field_by_name("score_result", Value::Message(score.result));
    }
    if let Some(damage_contest_result) = damage_contest_result {
        result.set_field_by_name(
            "damage_contest_result",
            Value::Message(damage_contest_result),
        );
    }
    let dead_allies = message_list(&state, "members")
        .iter()
        .filter(|member| member_type(member).ok() == Some(0) && !bool_field(member, "is_alive"))
        .count();
    apply_battle_missions(
        proto,
        reward_rules,
        quest_id,
        dead_allies,
        i32_field(&state, "total_turn").unwrap_or(1).max(1),
        &mut resources,
        &mut changed,
        &mut result,
    )?;
    let mut response = empty_message(proto, "blend.api.BattleFinishResponse")?;
    response.set_field_by_name("quest_result", Value::Message(result));
    response.set_field_by_name("changed_resources", Value::Message(changed));
    let character_indexes = message_list(&resources, "characters")
        .iter()
        .filter_map(|c| {
            Some((
                i64::from(i32_field(c, "character_id")?),
                optional_i32_field(c, "memoria_entity_id").map(i64::from),
            ))
        })
        .collect();
    Ok(BattleFinishMutation {
        resources,
        response,
        character_indexes,
    })
}

pub(crate) fn character_exp(
    _proto: &ProtoRegistry,
    rules: &CharacterRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    id: i32,
    amount: i32,
) -> Result<(), StateError> {
    if amount < 0 {
        return Err(StateError::InvalidRequest);
    }
    let mut character = resource_character(resources, id)?;
    let limit = i32_field(&character, "growboard_level_limit")
        .unwrap_or(0)
        .checked_add(i32_field(&character, "level_limit_increase_value").unwrap_or(0))
        .ok_or(StateError::InvalidRequest)?
        .max(rules.constants.initial_character_level_limit);
    let cap = rules
        .levels
        .iter()
        .find(|l| l.level == limit + 1)
        .map(|l| l.exp - 1)
        .unwrap_or(i32::MAX);
    let exp = i32_field(&character, "exp")
        .unwrap_or(0)
        .checked_add(amount)
        .ok_or(StateError::InvalidRequest)?
        .min(cap);
    character.set_field_by_name("exp", Value::I32(exp));
    upsert_character(resources, character.clone(), false);
    upsert_character(changed, character, false);
    Ok(())
}

pub(crate) fn initial_timeline(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    members: &[DynamicMessage],
) -> Result<Vec<DynamicMessage>, StateError> {
    // Speed orders opening turns, with an ally acting first.  The client keeps
    // two ally placements; scaled/boss enemy rows expose a third placement.
    let first = members
        .iter()
        .filter(|m| member_type(m).ok() == Some(0) && bool_field(m, "is_alive"))
        .map(|m| {
            member_status(m, "current_status")
                .map(|s| base_wait(i32_field(&s, "speed").unwrap_or(1)))
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .min()
        .ok_or(StateError::InvalidRequest)?;
    let mut units = Vec::new();
    for member in members.iter().filter(|m| bool_field(m, "is_alive")) {
        let id = member_id(member)?;
        let wait =
            base_wait(i32_field(&member_status(member, "current_status")?, "speed").unwrap_or(1))
                .max(1);
        let initial = (wait - first).max(if member_type(member)? == 1 { 1 } else { 0 });
        let slots = if member_type(member)? == 0 {
            2
        } else {
            let enemy = rule_enemy(rules, enemy_member_status_enemy_id(member)?)?;
            if enemy.is_boss || enemy.status_growth.values().any(|value| *value != 0) {
                3
            } else {
                2
            }
        };
        for number in 1..=slots {
            units.push(build_timeline_unit(
                proto,
                id,
                number,
                initial + wait * (number - 1),
            )?);
        }
    }
    sort_timeline_units(&mut units, members);
    Ok(units)
}

pub(crate) fn heal_amount(
    actor: &DynamicMessage,
    target: &DynamicMessage,
    skill: &TutorialSkill,
) -> Result<i32, StateError> {
    // Power enum is OACLBCMILLH in the current client; percentage powers use 100.
    let value = match skill.skill_power_type {
        5 => {
            i64::from(i32_field(&member_status(actor, "current_status")?, "magic").unwrap_or(0))
                * i64::from(skill.power)
                / 100
        }
        6 => i64::from(skill.power),
        7 => i64::from(i32_field(target, "max_hp").unwrap_or(0)) * i64::from(skill.power) / 100,
        _ => return Err(StateError::InvalidRequest),
    };
    checked_i32(value.max(0))
}

pub(crate) fn enemy_damage(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    actor: &DynamicMessage,
    target: &DynamicMessage,
    skill: &TutorialSkill,
    variance: u32,
) -> Result<i64, StateError> {
    let (attack, defense, attribute) =
        member_offense_and_defense(proto, rules, actor, target, None, skill)?;
    let base = match skill.skill_power_type {
        2 => {
            i128::from(attack) * i128::from(skill.power.max(0)) * 100
                / (100 + i128::from(defense))
                / 100
        }
        3 => i128::from(skill.power.max(0)),
        4 => {
            i128::from(i32_field(target, "hp").unwrap_or(0)) * i128::from(skill.power.max(0)) / 100
        }
        _ => return Err(StateError::InvalidRequest),
    };
    // Local enemy balance: 20% of percentage attack with defense mitigation. Tutorial
    // calibration remains scoped to its four scripted encounters.
    Ok((base
        * i128::from((100 - target_resistance(target, attribute)?).max(0))
        * i128::from(variance)
        / 500
        / 10_000)
        .clamp(0, 9_999_999_999) as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn battle_mission_result_uses_catalog_and_preserves_previous_stars() {
        let proto = ProtoRegistry::from_file(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../schemas/atelier-resleriana-2.16.0.protoset"
        )))
        .unwrap();
        let rules = load_reward_rules().unwrap();
        let quest_id = 204051001;
        let mut resources = empty_message(&proto, "blend.model.Resources").unwrap();
        let mut state = empty_message(&proto, "blend.model.QuestState").unwrap();
        state.set_field_by_name("quest_id", Value::I32(quest_id));
        state.set_field_by_name("clear_count", Value::I32(1));
        upsert_quest_state(&mut resources, state);

        let mut changed = empty_message(&proto, "blend.model.Resources").unwrap();
        let mut result = empty_message(&proto, "blend.model.QuestResult").unwrap();
        apply_battle_missions(
            &proto,
            &rules,
            quest_id,
            0,
            20,
            &mut resources,
            &mut changed,
            &mut result,
        )
        .unwrap();
        let persisted = message_list(&resources, "quest_states").pop().unwrap();
        assert_eq!(
            i32_list(&persisted, "cleared_battle_mission_ids"),
            [1, 2, 4]
        );
        let mission_result = member_status(&result, "mission_result").unwrap();
        assert!(i32_list(&mission_result, "old_cleared_ids").is_empty());
        assert_eq!(i32_list(&mission_result, "cleared_ids"), [1, 2, 4]);

        let mut changed = empty_message(&proto, "blend.model.Resources").unwrap();
        let mut result = empty_message(&proto, "blend.model.QuestResult").unwrap();
        apply_battle_missions(
            &proto,
            &rules,
            quest_id,
            1,
            21,
            &mut resources,
            &mut changed,
            &mut result,
        )
        .unwrap();
        let persisted = message_list(&resources, "quest_states").pop().unwrap();
        assert_eq!(
            i32_list(&persisted, "cleared_battle_mission_ids"),
            [1, 2, 4]
        );
        let mission_result = member_status(&result, "mission_result").unwrap();
        assert_eq!(i32_list(&mission_result, "old_cleared_ids"), [1, 2, 4]);
        assert_eq!(i32_list(&mission_result, "cleared_ids"), [1]);
    }
}
