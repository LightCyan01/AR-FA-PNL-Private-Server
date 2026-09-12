use super::prelude::*;

pub(crate) struct TalkMutation {
    pub(crate) resources: DynamicMessage,
    pub(crate) response: DynamicMessage,
    #[allow(dead_code)]
    pub(crate) granted_character_id: Option<i32>,
}

pub(crate) fn reduce_talk_event_with_rules(
    proto: &ProtoRegistry,
    fresh_rules: &FreshStateRules,
    tutorial_rules: &TutorialRules,
    home_rules: &home::HomeRules,
    mut resources: DynamicMessage,
    quest_id: i32,
    now: i64,
) -> Result<TalkMutation, StateError> {
    if !(101001001..=101001017).contains(&quest_id) || quest_clear_count(&resources, quest_id) > 0 {
        return quest::finish_talk(proto, tutorial_rules, home_rules, resources, quest_id, now);
    }
    let quest = tutorial_rules
        .quests
        .iter()
        .find(|quest| quest.id == quest_id)
        .ok_or(StateError::InvalidRequest)?;
    if quest.quest_type != 2 || quest.talk_event_id.is_none() {
        return Err(StateError::InvalidRequest);
    }
    if quest.start_at.is_some_and(|start| now < start) || quest.end_at.is_some_and(|end| now >= end)
    {
        return Err(StateError::OutOfSchedule);
    }
    if quest_clear_count(&resources, quest_id) != 0 {
        return Err(StateError::InvalidRequest);
    }
    if quest
        .predecessor_id
        .is_some_and(|predecessor| quest_clear_count(&resources, predecessor) == 0)
    {
        return Err(StateError::InvalidRequest);
    }
    let required_tutorial_step = match quest_id {
        101001004..=101001010 => TUTORIAL_STEP_FIRST_SYNTHESIS,
        101001011..=101001014 => TUTORIAL_STEP_SECOND_SYNTHESIS,
        101001016..=101001017 => TUTORIAL_STEP_MEMORIA_EQUIPPED,
        _ => 0,
    };
    if tutorial_step(&resources) < required_tutorial_step {
        return Err(StateError::InvalidRequest);
    }

    let mut quest_state = empty_message(proto, "blend.model.QuestState")?;
    quest_state.set_field_by_name("quest_id", Value::I32(quest_id));
    quest_state.set_field_by_name("clear_count", Value::I32(1));
    upsert_quest_state(&mut resources, quest_state.clone());

    let mut status = resources
        .get_field_by_name("status")
        .and_then(|value| value.as_message().cloned())
        .ok_or(StateError::InvalidRequest)?;
    status.set_field_by_name("last_main_story_quest_id", Value::I32(quest_id));
    resources.set_field_by_name("status", Value::Message(status.clone()));

    let mut changed = empty_message(proto, "blend.model.Resources")?;
    changed.set_field_by_name("status", Value::Message(status));

    let mut first_clear_rewards = Vec::new();
    upsert_quest_state(&mut changed, quest_state);
    let mut granted_character_id = None;
    if let Some(reward_set_id) = quest.first_clear_reward_set_id {
        let reward_set = tutorial_rules
            .reward_sets
            .iter()
            .find(|reward_set| reward_set.id == reward_set_id)
            .ok_or_else(|| {
                StateError::TutorialRules(format!("missing reward set {reward_set_id}"))
            })?;
        for reward in &reward_set.rewards {
            if reward.resource_type != 4 || reward.quantity != 1 {
                return Err(StateError::TutorialRules(format!(
                    "unsupported talk reward type {} quantity {}",
                    reward.resource_type, reward.quantity
                )));
            }
            if character_present(&resources, reward.id) || granted_character_id.is_some() {
                return Err(StateError::InvalidRequest);
            }
            let params = reward.resource_params.as_ref().ok_or_else(|| {
                StateError::TutorialRules("character reward has no resource params".into())
            })?;
            let level = params
                .level
                .ok_or_else(|| StateError::TutorialRules("character reward has no level".into()))?;
            if params.rank.is_some() || !params.traits.is_empty() {
                return Err(StateError::TutorialRules(
                    "unsupported character reward params".into(),
                ));
            }
            let character_rule = tutorial_rules
                .reward_characters
                .iter()
                .find(|character| character.id == reward.id)
                .ok_or_else(|| {
                    StateError::TutorialRules(format!("missing character {}", reward.id))
                })?;
            let exp = tutorial_rules
                .character_levels
                .iter()
                .find(|row| row.level == level)
                .map(|row| row.exp)
                .ok_or_else(|| {
                    StateError::TutorialRules(format!("missing character level {level}"))
                })?;
            let mut character = character_message(
                proto,
                i64::from(reward.id),
                None,
                Some(now),
                character_rule.initial_rarity,
                fresh_rules.constants.initial_character_level_limit,
            )?;
            character.set_field_by_name("exp", Value::I32(exp));
            if let Some(skin) = params.skin {
                character.set_field_by_name("skin_id", Value::Message(int32_value(proto, skin)?));
            }
            upsert_character(&mut resources, character.clone(), false);
            upsert_character(&mut changed, character, false);
            let character_count = i32::try_from(message_list(&resources, "characters").len())
                .map_err(|_| StateError::InvalidRequest)?;
            let task_level = set_total_task_count(&mut resources, 47, level)?;
            let task_count = set_total_task_count(&mut resources, 85, character_count)?;
            set_changed_task_counts(&mut changed, vec![task_level, task_count]);
            let mut status = status_message(&resources)?;
            status.set_field_by_name(
                "growboard_max_page",
                Value::I32(character_rule.growboard_max_page),
            );
            resources.set_field_by_name("status", Value::Message(status.clone()));
            changed.set_field_by_name("status", Value::Message(status));
            granted_character_id = Some(reward.id);
            first_clear_rewards.push(Value::Message(reward_message(proto, reward, true)?));
        }
    }

    let mut response = empty_message(proto, "blend.api.QuestTalkEventFinishResponse")?;
    response.set_field_by_name("changed_resources", Value::Message(changed));
    response.set_field_by_name("first_clear_rewards", Value::List(first_clear_rewards));
    Ok(TalkMutation {
        resources,
        response,
        granted_character_id,
    })
}

/// Compatibility wrapper for isolated fixtures that do not own `State`.
#[cfg(test)]
pub(crate) fn reduce_talk_event(
    proto: &ProtoRegistry,
    fresh_rules: &FreshStateRules,
    tutorial_rules: &TutorialRules,
    resources: DynamicMessage,
    quest_id: i32,
    now: i64,
) -> Result<TalkMutation, StateError> {
    reduce_talk_event_with_rules(
        proto,
        fresh_rules,
        tutorial_rules,
        &home::load_rules()?,
        resources,
        quest_id,
        now,
    )
}

pub(crate) fn message_list(message: &DynamicMessage, field: &str) -> Vec<DynamicMessage> {
    message
        .get_field_by_name(field)
        .and_then(|value| value.as_list().map(|values| values.to_vec()))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|value| value.as_message().cloned())
        .collect()
}

pub(crate) fn i32_list(message: &DynamicMessage, field: &str) -> Vec<i32> {
    message
        .get_field_by_name(field)
        .and_then(|value| value.as_list().map(|values| values.to_vec()))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|value| value.as_i32())
        .collect()
}

pub(crate) fn total_task_count(resources: &DynamicMessage, condition_id: i32) -> i32 {
    message_list(resources, "total_task_counts")
        .iter()
        .find(|task| i32_field(task, "condition_id") == Some(condition_id))
        .and_then(|task| i32_field(task, "count"))
        .unwrap_or(0)
}

pub(crate) fn set_total_task_count(
    resources: &mut DynamicMessage,
    condition_id: i32,
    count: i32,
) -> Result<DynamicMessage, StateError> {
    let mut tasks = message_list(resources, "total_task_counts");
    let task = tasks
        .iter_mut()
        .find(|task| i32_field(task, "condition_id") == Some(condition_id))
        .ok_or(StateError::InvalidRequest)?;
    task.set_field_by_name("count", Value::I32(count));
    let changed = task.clone();
    resources.set_field_by_name(
        "total_task_counts",
        Value::List(tasks.into_iter().map(Value::Message).collect()),
    );
    Ok(changed)
}

pub(crate) fn set_changed_task_counts(changed: &mut DynamicMessage, tasks: Vec<DynamicMessage>) {
    if !tasks.is_empty() {
        let mut current = message_list(changed, "total_task_counts");
        for task in tasks {
            let condition_id = i32_field(&task, "condition_id");
            if let Some(existing) = current
                .iter_mut()
                .find(|existing| i32_field(existing, "condition_id") == condition_id)
            {
                *existing = task;
            } else {
                current.push(task);
            }
        }
        changed.set_field_by_name(
            "total_task_counts",
            Value::List(current.into_iter().map(Value::Message).collect()),
        );
    }
}

pub(crate) fn optional_i32_field(message: &DynamicMessage, field: &str) -> Option<i32> {
    if !message.has_field_by_name(field) {
        return None;
    }
    message
        .get_field_by_name(field)
        .and_then(|value| value.as_message().cloned())
        .and_then(|value| i32_field(&value, "value"))
}

pub(crate) fn rule_battle(rules: &TutorialRules, id: i32) -> Result<&TutorialBattle, StateError> {
    rules
        .battles
        .iter()
        .find(|row| row.id == id)
        .ok_or_else(|| StateError::TutorialRules(format!("missing battle {id}")))
}

pub(crate) fn rule_wave(rules: &TutorialRules, id: i32) -> Result<&TutorialWave, StateError> {
    rules
        .waves
        .iter()
        .find(|row| row.id == id)
        .ok_or_else(|| StateError::TutorialRules(format!("missing wave {id}")))
}

pub(crate) fn rule_enemy(rules: &TutorialRules, id: i32) -> Result<&TutorialEnemy, StateError> {
    rules
        .enemies
        .iter()
        .find(|row| row.id == id)
        .ok_or_else(|| StateError::TutorialRules(format!("missing enemy {id}")))
}

pub(crate) fn rule_character(
    rules: &TutorialRules,
    id: i32,
) -> Result<&TutorialBattleCharacter, StateError> {
    rules
        .battle_characters
        .iter()
        .find(|row| row.id == id)
        .ok_or_else(|| StateError::TutorialRules(format!("missing battle character {id}")))
}

pub(crate) fn rule_growth(
    rules: &TutorialRules,
    id: i32,
) -> Result<&TutorialCharacterGrowth, StateError> {
    rules
        .character_growths
        .iter()
        .find(|row| row.id == id)
        .ok_or_else(|| StateError::TutorialRules(format!("missing character growth {id}")))
}

pub(crate) fn rule_rarity(
    rules: &TutorialRules,
    id: i32,
) -> Result<&TutorialCharacterRarity, StateError> {
    rules
        .character_rarities
        .iter()
        .find(|row| row.id == id)
        .ok_or_else(|| StateError::TutorialRules(format!("missing character rarity {id}")))
}

pub(crate) fn rule_tool(rules: &TutorialRules, id: i32) -> Result<&TutorialBattleTool, StateError> {
    rules
        .battle_tools
        .iter()
        .find(|row| row.id == id)
        .ok_or_else(|| StateError::TutorialRules(format!("missing battle tool {id}")))
}

pub(crate) fn rule_battle_tool_trait(
    rules: &TutorialRules,
    id: i32,
) -> Result<&TutorialBattleToolTrait, StateError> {
    rules
        .battle_tool_traits
        .iter()
        .find(|row| row.id == id)
        .ok_or_else(|| StateError::TutorialRules(format!("missing battle tool trait {id}")))
}

pub(crate) fn map_value(values: &BTreeMap<String, i32>, name: &str) -> Result<i32, StateError> {
    values
        .get(name)
        .copied()
        .ok_or_else(|| StateError::TutorialRules(format!("missing stat {name}")))
}

pub(crate) fn checked_i32(value: i64) -> Result<i32, StateError> {
    i32::try_from(value).map_err(|_| StateError::TutorialRules("stat overflows int32".into()))
}

pub(crate) fn quest_clear_count(resources: &DynamicMessage, quest_id: i32) -> i32 {
    resources
        .get_field_by_name("quest_states")
        .and_then(|value| value.as_list().map(|quests| quests.to_vec()))
        .and_then(|quests| {
            quests.iter().find_map(|quest| {
                let quest = quest.as_message()?;
                (quest.get_field_by_name("quest_id")?.as_i32()? == quest_id).then(|| {
                    quest
                        .get_field_by_name("clear_count")
                        .and_then(|value| value.as_i32())
                        .unwrap_or_default()
                })
            })
        })
        .unwrap_or_default()
}

pub(crate) fn character_present(resources: &DynamicMessage, character_id: i32) -> bool {
    resources
        .get_field_by_name("characters")
        .and_then(|value| value.as_list().map(|characters| characters.to_vec()))
        .is_some_and(|characters| {
            characters.iter().any(|character| {
                character
                    .as_message()
                    .and_then(|character| character.get_field_by_name("character_id"))
                    .and_then(|value| value.as_i32())
                    == Some(character_id)
            })
        })
}

pub(crate) fn upsert_quest_state(resources: &mut DynamicMessage, patch: DynamicMessage) {
    let mut values = resources
        .get_field_by_name("quest_states")
        .and_then(|value| value.as_list().map(|quests| quests.to_vec()))
        .unwrap_or_default();
    let quest_id = patch
        .get_field_by_name("quest_id")
        .and_then(|value| value.as_i32())
        .unwrap_or_default();
    if let Some(slot) = values.iter_mut().find(|value| {
        value
            .as_message()
            .and_then(|quest| quest.get_field_by_name("quest_id"))
            .and_then(|value| value.as_i32())
            == Some(quest_id)
    }) {
        *slot = Value::Message(patch);
    } else {
        values.push(Value::Message(patch));
    }
    resources.set_field_by_name("quest_states", Value::List(values));
}

pub(crate) fn reward_message(
    proto: &ProtoRegistry,
    reward: &TutorialReward,
    is_new: bool,
) -> Result<DynamicMessage, StateError> {
    let mut message = empty_message(proto, "blend.model.Reward")?;
    message.set_field_by_name("type", Value::I32(reward.resource_type));
    message.set_field_by_name("id", Value::I32(reward.id));
    message.set_field_by_name("quantity", Value::I32(reward.quantity));
    message.set_field_by_name("is_new", Value::Bool(is_new));
    if let Some(params) = &reward.resource_params {
        let mut resource_params = empty_message(proto, "blend.model.ResourceParams")?;
        for (field, value) in [
            ("level", params.level),
            ("rank", params.rank),
            ("skin", params.skin),
        ] {
            if let Some(value) = value {
                resource_params
                    .set_field_by_name(field, Value::Message(int32_value(proto, value)?));
            }
        }
        resource_params.set_field_by_name(
            "traits",
            Value::List(
                params
                    .traits
                    .iter()
                    .map(|trait_param| trait_params_message(proto, trait_param).map(Value::Message))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
        );
        message.set_field_by_name("resource_params", Value::Message(resource_params));
    }
    Ok(message)
}

pub(crate) fn character_message(
    proto: &ProtoRegistry,
    character_id: i64,
    memoria: Option<i64>,
    starter_at: Option<i64>,
    starter_rarity: i32,
    starter_level_limit: i32,
) -> Result<DynamicMessage, StateError> {
    let mut character = empty_message(proto, "blend.model.Character")?;
    character.set_field_by_name("character_id", Value::I32(character_id as i32));
    if let Some(received_at) = starter_at {
        character.set_field_by_name("rarity", Value::I32(starter_rarity));
        character.set_field_by_name("normal1_skill_rank", Value::I32(1));
        character.set_field_by_name("normal2_skill_rank", Value::I32(1));
        character.set_field_by_name(
            "received_at",
            Value::Message(timestamp(proto, received_at)?),
        );
        character.set_field_by_name("growboard_current_page", Value::I32(1));
        character.set_field_by_name("growboard_ex_current_page", Value::I32(1));
        character.set_field_by_name("growboard_level_limit", Value::I32(starter_level_limit));
        character.set_field_by_name("growboard_neo_current_page", Value::I32(1));
        character.set_field_by_name("growboard_neo_max_page", Value::I32(1));
        if let Some(lock) = load_character_rules()?
            .skill_lock_release
            .iter()
            .find(|row| i64::from(row.character_id) == character_id)
        {
            for (field, costs) in [
                (
                    "is_normal1_skill_locked",
                    &lock.normal1_skill_lock_release_costs,
                ),
                (
                    "is_normal2_skill_locked",
                    &lock.normal2_skill_lock_release_costs,
                ),
                (
                    "is_burst_skill_locked",
                    &lock.burst_skill_lock_release_costs,
                ),
            ] {
                character.set_field_by_name(field, Value::Bool(!costs.is_empty()));
            }
        }
    }
    if let Some(memoria) = memoria {
        character.set_field_by_name(
            "memoria_entity_id",
            Value::Message(int32_value(proto, memoria as i32)?),
        );
    }
    Ok(character)
}

pub(crate) fn upsert_character(
    resources: &mut DynamicMessage,
    patch: DynamicMessage,
    patch_memoria: bool,
) {
    let Some(existing) = resources.get_field_by_name("characters") else {
        return;
    };
    let mut values = existing
        .as_list()
        .map(|items| items.to_vec())
        .unwrap_or_default();
    let id = patch
        .get_field_by_name("character_id")
        .and_then(|value| value.as_i32())
        .unwrap_or_default();
    if let Some(slot) = values.iter_mut().find(|value| {
        value
            .as_message()
            .and_then(|message| message.get_field_by_name("character_id"))
            .and_then(|value| value.as_i32())
            == Some(id)
    }) {
        if patch_memoria {
            if let Some(existing_message) = slot.as_message().cloned() {
                let mut merged = existing_message;
                if patch.has_field_by_name("memoria_entity_id") {
                    if let Some(memoria) = patch.get_field_by_name("memoria_entity_id") {
                        merged.set_field_by_name("memoria_entity_id", memoria.into_owned());
                    }
                } else {
                    merged.clear_field_by_name("memoria_entity_id");
                }
                *slot = Value::Message(merged);
            }
        } else {
            *slot = Value::Message(patch);
        }
    } else {
        values.push(Value::Message(patch));
    }
    resources.set_field_by_name("characters", Value::List(values));
}

pub(crate) fn item_quantity(resources: &DynamicMessage, item_id: i32) -> Option<i32> {
    message_list(resources, "items")
        .into_iter()
        .find(|item| i32_field(item, "item_id") == Some(item_id))
        .and_then(|item| i32_field(&item, "quantity"))
}

pub(crate) fn change_item(
    proto: &ProtoRegistry,
    resources: &mut DynamicMessage,
    item_id: i32,
    delta: i32,
) -> Result<DynamicMessage, StateError> {
    if item_id <= 0 {
        return Err(StateError::InvalidRequest);
    }
    let mut items = message_list(resources, "items");
    let mut changed = None;
    for item in &mut items {
        if i32_field(item, "item_id") != Some(item_id) {
            continue;
        }
        let old = i32_field(item, "quantity").unwrap_or(0);
        let next = old
            .checked_add(delta)
            .filter(|value| *value >= 0)
            .ok_or(StateError::InvalidRequest)?;
        item.set_field_by_name("quantity", Value::I32(next));
        let total = i32_field(item, "total_quantity").unwrap_or(old).max(next);
        item.set_field_by_name("total_quantity", Value::I32(total));
        changed = Some(item.clone());
        break;
    }
    if changed.is_none() {
        if delta < 0 {
            return Err(StateError::InvalidRequest);
        }
        let mut item = empty_message(proto, "blend.model.Item")?;
        item.set_field_by_name("item_id", Value::I32(item_id));
        item.set_field_by_name("quantity", Value::I32(delta));
        item.set_field_by_name("total_quantity", Value::I32(delta));
        items.push(item.clone());
        changed = Some(item);
    }
    resources.set_field_by_name(
        "items",
        Value::List(items.into_iter().map(Value::Message).collect()),
    );
    changed.ok_or(StateError::InvalidRequest)
}

pub(crate) fn status_message(resources: &DynamicMessage) -> Result<DynamicMessage, StateError> {
    resources
        .get_field_by_name("status")
        .and_then(|value| value.as_message().cloned())
        .ok_or(StateError::InvalidRequest)
}

pub(crate) fn tutorial_step(resources: &DynamicMessage) -> i32 {
    status_message(resources)
        .ok()
        .and_then(|status| i32_field(&status, "tutorial_step"))
        .unwrap_or_default()
}

pub(crate) fn recover_resource(
    proto: &ProtoRegistry,
    status: &mut DynamicMessage,
    kind: &str,
    limit: i32,
    interval: i64,
    now: i64,
) -> Result<(), StateError> {
    if limit <= 0 || interval <= 0 {
        return Err(StateError::InvalidRequest);
    }
    let field = format!("{kind}_when_updated");
    let time_field = format!("{kind}_updated_at");
    let old = i32_field(status, &field).unwrap_or(0);
    let updated = message_i64_field(status, &time_field, "seconds").unwrap_or(now);
    let elapsed = now.saturating_sub(updated).max(0);
    let count = if old >= limit {
        old
    } else {
        (i64::from(old) + elapsed / interval).min(i64::from(limit)) as i32
    };
    let updated = if count >= limit {
        now
    } else {
        now - elapsed % interval
    };
    status.set_field_by_name(&field, Value::I32(count));
    status.set_field_by_name(&time_field, Value::Message(timestamp(proto, updated)?));
    Ok(())
}

pub(crate) fn change_mana(
    proto: &ProtoRegistry,
    rules: &SynthesisRules,
    resources: &mut DynamicMessage,
    delta: i32,
    now: i64,
) -> Result<DynamicMessage, StateError> {
    let mut status = status_message(resources)?;
    recover_resource(
        proto,
        &mut status,
        "mana",
        rules.constants.mana_recovery_limit,
        rules.constants.mana_recovery_interval_seconds,
        now,
    )?;
    let old = i32_field(&status, "mana_when_updated").unwrap_or(0);
    let next = old
        .checked_add(delta)
        .filter(|value| *value >= 0)
        .ok_or(StateError::InvalidRequest)?;
    status.set_field_by_name("mana_when_updated", Value::I32(next));
    resources.set_field_by_name("status", Value::Message(status.clone()));
    Ok(status)
}

pub(crate) fn next_entity_id(resources: &DynamicMessage) -> Result<i32, StateError> {
    let mut max_id = 0i32;
    for field in ["battle_tools", "equipment_tools", "memorias", "ship_tools"] {
        for item in message_list(resources, field) {
            if let Some(id) = i32_field(&item, "entity_id") {
                max_id = max_id.max(id);
            }
        }
    }
    max_id.checked_add(1).ok_or(StateError::InvalidRequest)
}

pub(crate) fn trait_params_message(
    proto: &ProtoRegistry,
    trait_param: &TutorialTraitParam,
) -> Result<DynamicMessage, StateError> {
    let mut message = empty_message(proto, "blend.model.TraitParams")?;
    message.set_field_by_name("id", Value::I32(trait_param.id));
    message.set_field_by_name("rank", Value::I32(trait_param.rank));
    Ok(message)
}

pub(crate) fn random_below(limit: u32) -> Result<u32, StateError> {
    if limit == 0 {
        return Err(StateError::InvalidRequest);
    }
    let zone = (u64::from(u32::MAX) + 1) / u64::from(limit) * u64::from(limit);
    let mut rng = OsRng;
    loop {
        let value = u64::from(rng.next_u32());
        if value < zone {
            return Ok((value % u64::from(limit)) as u32);
        }
    }
}

pub(crate) fn owned_battle_tool(resources: &DynamicMessage, entity_id: i32) -> bool {
    message_list(resources, "battle_tools")
        .iter()
        .any(|tool| i32_field(tool, "entity_id") == Some(entity_id))
}

pub(crate) fn owned_memoria(resources: &DynamicMessage, entity_id: i32) -> bool {
    message_list(resources, "memorias")
        .iter()
        .any(|memoria| i32_field(memoria, "entity_id") == Some(entity_id))
}

pub(crate) fn recipe_present(resources: &DynamicMessage, recipe_id: i32) -> bool {
    message_list(resources, "recipes")
        .iter()
        .any(|recipe| i32_field(recipe, "recipe_id") == Some(recipe_id))
}

pub(crate) fn upsert_recipe(resources: &mut DynamicMessage, patch: DynamicMessage) {
    let mut recipes = message_list(resources, "recipes");
    let id = i32_field(&patch, "recipe_id").unwrap_or_default();
    if let Some(slot) = recipes
        .iter_mut()
        .find(|recipe| i32_field(recipe, "recipe_id") == Some(id))
    {
        *slot = patch;
    } else {
        recipes.push(patch);
    }
    resources.set_field_by_name(
        "recipes",
        Value::List(recipes.into_iter().map(Value::Message).collect()),
    );
}
