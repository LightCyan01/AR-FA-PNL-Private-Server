use super::prelude::*;

pub(crate) fn grant_mission_battle_rewards(
    proto: &ProtoRegistry,
    rules: &HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    quest_id: i32,
    now: i64,
) -> Result<Vec<Value>, StateError> {
    let Some(rule) = rules
        .mission_battle_rewards
        .iter()
        .find(|rule| rule.quest_id == quest_id)
    else {
        return Ok(Vec::new());
    };
    let completed = completed_mission_count(rules, resources, &rule.mission_ids);
    let existing = message_list(resources, "mission_battle_reward_states")
        .into_iter()
        .find(|state| i32_field(state, "mission_battle_quest_id") == Some(quest_id));
    let mut state = existing.clone().unwrap_or(empty_message(
        proto,
        "blend.model.MissionBattleRewardState",
    )?);
    let previous = i32_field(&state, "received_step_count").unwrap_or(0).max(0) as usize;
    let mut index = previous;
    let mut result = Vec::new();
    while let Some(step) = rule
        .steps
        .get(index)
        .filter(|step| step.count as usize <= completed)
    {
        result.extend(grant(
            proto,
            rules,
            resources,
            changed,
            rewards(rules, step.reward_set_id)?,
            now,
        )?);
        index += 1;
    }
    if index != previous {
        state.set_field_by_name("mission_battle_quest_id", Value::I32(quest_id));
        state.set_field_by_name("received_step_count", Value::I32(index as i32));
        put(
            resources,
            "mission_battle_reward_states",
            "mission_battle_quest_id",
            state.clone(),
        );
        put(
            changed,
            "mission_battle_reward_states",
            "mission_battle_quest_id",
            state,
        );
    }
    Ok(result)
}

pub(crate) fn grant(
    proto: &ProtoRegistry,
    rules: &HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    inputs: &[TutorialReward],
    now: i64,
) -> Result<Vec<Value>, StateError> {
    let mut result = Vec::new();
    for reward in inputs {
        if reward.quantity <= 0 {
            return Err(StateError::InvalidRequest);
        }
        let mut output = reward_message(proto, reward, false)?;
        match reward.resource_type {
            1 | 3 | 5 => {
                let mut delta = empty_message(proto, "blend.model.Resources")?;
                result.extend(apply_quest_rewards(
                    proto,
                    resources,
                    &mut delta,
                    std::slice::from_ref(reward),
                )?);
                for (field, value) in delta.fields() {
                    if field.name() == "items" {
                        for item in message_list(&delta, "items") {
                            put(changed, "items", "item_id", item);
                        }
                    } else {
                        changed.set_field(&field, value.clone());
                    }
                }
                // The enclosing transaction's resource_progress owns Cole receipt
                // counters. Updating 109 here would count the same grant twice.
                continue;
            }
            2 => {
                let mut wallet = resources
                    .get_field_by_name("wallet")
                    .and_then(|value| value.as_message().cloned())
                    .ok_or(StateError::InvalidRequest)?;
                let paid = i32_field(&wallet, "paid")
                    .unwrap_or(0)
                    .checked_add(reward.quantity)
                    .ok_or(StateError::InvalidRequest)?;
                wallet.set_field_by_name("paid", Value::I32(paid));
                resources.set_field_by_name("wallet", Value::Message(wallet.clone()));
                changed.set_field_by_name("wallet", Value::Message(wallet));
            }
            7 => {
                let mut status = status_message(resources)?;
                let exp = i32_field(&status, "exp")
                    .unwrap_or(0)
                    .checked_add(reward.quantity)
                    .ok_or(StateError::InvalidRequest)?;
                let old_rank = i32_field(&status, "rank").unwrap_or(1);
                let rank = rules
                    .user_ranks
                    .iter()
                    .filter(|r| r.exp <= exp)
                    .max_by_key(|r| r.id)
                    .ok_or(StateError::InvalidRequest)?;
                let stamina_bonus: i32 = rules
                    .user_ranks
                    .iter()
                    .filter(|r| old_rank < r.id && r.id <= rank.id)
                    .map(|r| r.stamina)
                    .sum();
                status.set_field_by_name("exp", Value::I32(exp));
                status.set_field_by_name("rank", Value::I32(rank.id));
                if stamina_bonus > 0 {
                    let stamina = i32_field(&status, "stamina_when_updated")
                        .unwrap_or(0)
                        .checked_add(stamina_bonus)
                        .ok_or(StateError::InvalidRequest)?;
                    status.set_field_by_name("stamina_when_updated", Value::I32(stamina));
                    status.set_field_by_name(
                        "stamina_updated_at",
                        Value::Message(timestamp(proto, now)?),
                    );
                }
                resources.set_field_by_name("status", Value::Message(status.clone()));
                changed.set_field_by_name("status", Value::Message(status));
                update_task(resources, changed, 1, rank.id)?;
            }
            8 => {
                let piece = change_character_piece(proto, resources, reward.id, reward.quantity)?;
                put(changed, "character_pieces", "character_id", piece);
            }
            9 | 10 => {
                let mut status = status_message(resources)?;
                let field = if reward.resource_type == 9 {
                    "mana_when_updated"
                } else {
                    "stamina_when_updated"
                };
                let old = i32_field(&status, field).unwrap_or(0);
                let count = old
                    .checked_add(reward.quantity)
                    .filter(|count| {
                        *count
                            <= if reward.resource_type == 9 {
                                rules.energy.max_mana
                            } else {
                                rules.energy.max_stamina
                            }
                    })
                    .ok_or(StateError::InvalidRequest)?;
                status.set_field_by_name(field, Value::I32(count));
                status.set_field_by_name(
                    if reward.resource_type == 9 {
                        "mana_updated_at"
                    } else {
                        "stamina_updated_at"
                    },
                    Value::Message(timestamp(proto, now)?),
                );
                resources.set_field_by_name("status", Value::Message(status.clone()));
                changed.set_field_by_name("status", Value::Message(status));
                output.set_field_by_name("old_value", Value::I32(old));
            }
            4 => {
                let character = rules
                    .characters
                    .iter()
                    .find(|r| r.id == reward.id)
                    .ok_or(StateError::InvalidRequest)?;
                for _ in 0..reward.quantity {
                    let mut one = output.clone();
                    one.set_field_by_name("quantity", Value::I32(1));
                    let mut others = Vec::new();
                    if character_present(resources, reward.id) {
                        let piece = change_character_piece(
                            proto,
                            resources,
                            reward.id,
                            character.duplicated_piece_count,
                        )?;
                        put(changed, "character_pieces", "character_id", piece);
                        let stone = change_item(
                            proto,
                            resources,
                            126,
                            character.duplicated_generic_piece_count,
                        )?;
                        put(changed, "items", "item_id", stone);
                        others.push(Value::Message(resource_message(
                            proto,
                            8,
                            reward.id,
                            character.duplicated_piece_count,
                        )?));
                        others.push(Value::Message(resource_message(
                            proto,
                            5,
                            126,
                            character.duplicated_generic_piece_count,
                        )?));
                    } else {
                        let value = character_message(
                            proto,
                            i64::from(reward.id),
                            None,
                            Some(now),
                            character.rarity,
                            rules.initial_character_level_limit,
                        )?;
                        upsert_character(resources, value.clone(), false);
                        put(changed, "characters", "character_id", value);
                        one.set_field_by_name("is_new", Value::Bool(true));
                    }
                    if let Some(id) = reward
                        .resource_params
                        .as_ref()
                        .and_then(|p| p.skin)
                        .filter(|id| *id > 0)
                    {
                        if !message_list(resources, "character_skins")
                            .iter()
                            .any(|s| i32_field(s, "character_skin_id") == Some(id))
                        {
                            let mut skin = empty_message(proto, "blend.model.CharacterSkin")?;
                            skin.set_field_by_name("character_skin_id", Value::I32(id));
                            skin.set_field_by_name(
                                "received_at",
                                Value::Message(timestamp(proto, now)?),
                            );
                            put(
                                resources,
                                "character_skins",
                                "character_skin_id",
                                skin.clone(),
                            );
                            put(changed, "character_skins", "character_skin_id", skin);
                            others.push(Value::Message(resource_message(proto, 22, id, 1)?));
                        }
                    }
                    one.set_field_by_name("other_rewards", Value::List(others));
                    result.push(Value::Message(one));
                }
                update_task(
                    resources,
                    changed,
                    85,
                    message_list(resources, "characters").len() as i32,
                )?;
                continue;
            }
            17 => {
                if !rules.memoria_ids.contains(&reward.id) {
                    return Err(StateError::InvalidRequest);
                }
                for _ in 0..reward.quantity {
                    let is_new = !message_list(resources, "memorias")
                        .iter()
                        .any(|value| i32_field(value, "memoria_id") == Some(reward.id));
                    let id = next_entity_id(resources)?;
                    let mut value = empty_message(proto, "blend.model.Memoria")?;
                    value.set_field_by_name("entity_id", Value::I32(id));
                    value.set_field_by_name("memoria_id", Value::I32(reward.id));
                    value.set_field_by_name("limit_break", Value::I32(0));
                    value.set_field_by_name("exp", Value::I32(0));
                    value.set_field_by_name("is_locked", Value::Bool(false));
                    value.set_field_by_name("received_at", Value::Message(timestamp(proto, now)?));
                    put(resources, "memorias", "entity_id", value.clone());
                    put(changed, "memorias", "entity_id", value);
                    let mut one = output.clone();
                    one.set_field_by_name("quantity", Value::I32(1));
                    one.set_field_by_name("entity_id", Value::I32(id));
                    one.set_field_by_name("is_new", Value::Bool(is_new));
                    result.push(Value::Message(one));
                }
                update_task(
                    resources,
                    changed,
                    139,
                    total_task_count(resources, 139).max(1),
                )?;
                continue;
            }
            6 | 14 | 25 => {
                for _ in 0..reward.quantity {
                    let traits = reward
                        .resource_params
                        .as_ref()
                        .map(|p| p.traits.as_slice())
                        .unwrap_or(&[]);
                    let tool = add_synthesis_output(proto, resources, reward, traits, now)?;
                    let id = i32_field(&tool, "entity_id").ok_or(StateError::InvalidRequest)?;
                    put(
                        changed,
                        synthesis_output_field(reward.resource_type)?,
                        "entity_id",
                        tool,
                    );
                    let mut one = output.clone();
                    one.set_field_by_name("quantity", Value::I32(1));
                    one.set_field_by_name("entity_id", Value::I32(id));
                    result.push(Value::Message(one));
                }
                continue;
            }
            19 | 20 => {
                let kind = if reward.resource_type == 19 { 14 } else { 6 };
                let value = change_stimulator(proto, resources, kind, reward.id, reward.quantity)?;
                let (field, _, key, _) = stimulator_fields(kind)?;
                put(changed, field, key, value);
            }
            21 | 23 | 24 => {
                let (field, name, key) = match reward.resource_type {
                    21 => ("homes", "blend.model.Home", "home_id"),
                    23 => (
                        "chara_home_backgrounds",
                        "blend.model.CharaHomeBackground",
                        "background_id",
                    ),
                    _ => (
                        "chara_home_motions",
                        "blend.model.CharaHomeMotion",
                        "motion_id",
                    ),
                };
                let mut value = empty_message(proto, name)?;
                value.set_field_by_name(key, Value::I32(reward.id));
                value.set_field_by_name("received_at", Value::Message(timestamp(proto, now)?));
                put(resources, field, key, value.clone());
                put(changed, field, key, value);
            }
            other => {
                return Err(StateError::RewardRules(format!(
                    "unsupported home reward type {other}"
                )))
            }
        }
        result.push(Value::Message(output));
    }
    Ok(result)
}

pub(crate) fn recipe_count_state_index(state: &DynamicMessage) -> i32 {
    i32_field(state, "index").unwrap_or(0)
}

pub(crate) fn put_recipe_count_state(
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    plan_id: i32,
    index: i32,
    value: DynamicMessage,
) {
    let field = "recipe_count_reward_states";
    let mut rows = message_list(resources, field);
    if let Some(slot) = rows.iter_mut().find(|row| {
        i32_field(row, "recipe_plan_id") == Some(plan_id) && recipe_count_state_index(row) == index
    }) {
        *slot = value.clone();
    } else {
        rows.push(value.clone());
    }
    resources.set_field_by_name(
        field,
        Value::List(rows.into_iter().map(Value::Message).collect()),
    );
    let mut rows = message_list(changed, "recipe_count_reward_states");
    if let Some(slot) = rows.iter_mut().find(|row| {
        i32_field(row, "recipe_plan_id") == Some(plan_id) && recipe_count_state_index(row) == index
    }) {
        *slot = value;
    } else {
        rows.push(value);
    }
    changed.set_field_by_name(
        "recipe_count_reward_states",
        Value::List(rows.into_iter().map(Value::Message).collect()),
    );
}

pub(crate) fn claim_recipe_counts(
    proto: &ProtoRegistry,
    rules: &HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    request: &DynamicMessage,
    now: i64,
) -> Result<Vec<Value>, StateError> {
    let plans = message_list(request, "receive_plans");
    if plans.is_empty() {
        return Err(StateError::InvalidRequest);
    }
    let mut rewards_out = Vec::new();
    for plan_request in plans {
        let plan_id =
            i32_field(&plan_request, "recipe_plan_id").ok_or(StateError::InvalidRequest)?;
        let plan = rules
            .recipe_plans
            .iter()
            .find(|row| row.id == plan_id)
            .ok_or(StateError::InvalidRequest)?;
        if !in_period(plan.start_at, plan.end_at, now) {
            return Err(StateError::OutOfSchedule);
        }
        let indices = i32_list(&plan_request, "indices");
        if indices.is_empty()
            || indices.iter().any(|index| *index < 0)
            || indices.iter().copied().collect::<BTreeSet<_>>().len() != indices.len()
        {
            return Err(StateError::InvalidRequest);
        }
        let learned = rules
            .recipes
            .iter()
            .filter(|recipe| {
                recipe.recipe_plan_id == plan_id && recipe_present(resources, recipe.id)
            })
            .count();
        for index in indices {
            let step = plan
                .recipe_count_rewards
                .get(index as usize)
                .ok_or(StateError::InvalidRequest)?;
            if learned < usize::try_from(step.count).map_err(|_| StateError::InvalidRequest)? {
                return Err(StateError::InvalidRequest);
            }
            let claimed = message_list(resources, "recipe_count_reward_states")
                .iter()
                .any(|state| {
                    i32_field(state, "recipe_plan_id") == Some(plan_id)
                        && recipe_count_state_index(state) == index
                });
            if claimed {
                continue;
            }
            rewards_out.extend(grant(
                proto,
                rules,
                resources,
                changed,
                rewards(rules, step.reward_set_id)?,
                now,
            )?);
            let mut state = empty_message(proto, "blend.model.RecipeCountRewardState")?;
            state.set_field_by_name("recipe_plan_id", Value::I32(plan_id));
            if index > 0 {
                state.set_field_by_name("index", Value::I32(index));
            }
            put_recipe_count_state(resources, changed, plan_id, index, state);
        }
    }
    Ok(rewards_out)
}

pub(crate) fn present_wrapper(request: &DynamicMessage, name: &str) -> Option<Option<i32>> {
    if !request.has_field_by_name(name) {
        return None;
    }
    let value = request.get_field_by_name(name)?;
    let message = value.as_message()?;
    Some(
        message
            .get_field_by_name("value")
            .and_then(|value| value.as_i32()),
    )
}

pub(crate) fn apply_profile_name(
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    request: &DynamicMessage,
) -> Result<(), StateError> {
    let name = request
        .get_field_by_name("name")
        .and_then(|value| value.as_str().map(str::to_owned))
        .ok_or(StateError::InvalidRequest)?;
    if name.is_empty() || name.len() > 64 {
        return Err(StateError::InvalidRequest);
    }
    let mut profile = resources
        .get_field_by_name("profile")
        .and_then(|value| value.as_message().cloned())
        .ok_or(StateError::InvalidRequest)?;
    profile.set_field_by_name("name", Value::String(name));
    resources.set_field_by_name("profile", Value::Message(profile.clone()));
    changed.set_field_by_name("profile", Value::Message(profile));
    update_task(resources, changed, 529, 1)?;
    Ok(())
}

pub(crate) fn apply_selected_home(
    proto: &ProtoRegistry,
    rules: &HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    request: &DynamicMessage,
) -> Result<(), StateError> {
    let mut profile = resources
        .get_field_by_name("profile")
        .and_then(|value| value.as_message().cloned())
        .ok_or(StateError::InvalidRequest)?;
    if let Some(value) = present_wrapper(request, "selected_home_id") {
        let value = value.ok_or(StateError::InvalidRequest)?;
        if value < 0
            || (value > 0
                && (!rules.home_ids.contains(&value)
                    || !message_list(resources, "homes")
                        .iter()
                        .any(|home| i32_field(home, "home_id") == Some(value))))
        {
            return Err(StateError::InvalidRequest);
        }
        profile.set_field_by_name(
            "selected_home_id",
            Value::Message(int32_value(proto, value)?),
        );
    }
    if let Some(value) = present_wrapper(request, "selected_chara_home_slot_id") {
        let value = value.ok_or(StateError::InvalidRequest)?;
        if value < 0
            || (value > 0
                && !message_list(resources, "chara_homes")
                    .iter()
                    .any(|home| i32_field(home, "slot_id") == Some(value)))
        {
            return Err(StateError::InvalidRequest);
        }
        profile.set_field_by_name(
            "selected_chara_home_slot_id",
            Value::Message(int32_value(proto, value)?),
        );
    }
    resources.set_field_by_name("profile", Value::Message(profile.clone()));
    changed.set_field_by_name("profile", Value::Message(profile));
    Ok(())
}

pub(crate) fn apply_character_bulk_set(
    rules: &HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    request: &DynamicMessage,
) -> Result<(), StateError> {
    let character_id = i32_field(request, "character_id").ok_or(StateError::InvalidRequest)?;
    let mut character = message_list(resources, "characters")
        .into_iter()
        .find(|row| i32_field(row, "character_id") == Some(character_id))
        .ok_or(StateError::InvalidRequest)?;
    for (request_field, character_field, slot) in [
        (
            "slot1_equipment_tool_entity_id",
            "slot1_equipment_tool_entity_id",
            1,
        ),
        (
            "slot2_equipment_tool_entity_id",
            "slot2_equipment_tool_entity_id",
            2,
        ),
        (
            "slot3_equipment_tool_entity_id",
            "slot3_equipment_tool_entity_id",
            3,
        ),
        ("memoria_entity_id", "memoria_entity_id", 0),
    ] {
        let Some(value) = present_wrapper(request, request_field) else {
            character.clear_field_by_name(character_field);
            continue;
        };
        let value = value.ok_or(StateError::InvalidRequest)?;
        if value <= 0
            || (slot != 0
                && !message_list(resources, "equipment_tools")
                    .iter()
                    .any(|tool| {
                        i32_field(tool, "entity_id") == Some(value)
                            && rules.equipment_tools.iter().any(|rule| {
                                Some(rule.id) == i32_field(tool, "tool_id")
                                    && rule.slot_type == Some(slot)
                            })
                    }))
            || (slot == 0 && !owned_memoria(resources, value))
        {
            return Err(StateError::InvalidRequest);
        }
        character.set_field_by_name(
            character_field,
            request
                .get_field_by_name(request_field)
                .unwrap()
                .into_owned(),
        );
    }
    upsert_character(resources, character.clone(), false);
    put(changed, "characters", "character_id", character);
    Ok(())
}

pub(crate) fn apply_chara_home_register(
    proto: &ProtoRegistry,
    rules: &HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    request: &DynamicMessage,
    now: i64,
) -> Result<(), StateError> {
    let slot_id = i32_field(request, "slot_id").ok_or(StateError::InvalidRequest)?;
    let character_id = i32_field(request, "character_id").ok_or(StateError::InvalidRequest)?;
    let motion_id = i32_field(request, "motion_id").ok_or(StateError::InvalidRequest)?;
    let camera_id = i32_field(request, "camera_id").ok_or(StateError::InvalidRequest)?;
    let bgm_id = i32_field(request, "bgm_id").ok_or(StateError::InvalidRequest)?;
    let background_id = i32_field(request, "background_id").ok_or(StateError::InvalidRequest)?;
    if slot_id <= 0
        || !character_present(resources, character_id)
        || !rules.chara_home_motion_ids.contains(&motion_id)
        || !rules.chara_home_camera_ids.contains(&camera_id)
        || !rules.chara_home_bgm_ids.contains(&bgm_id)
        || background_id <= 0
    {
        return Err(StateError::InvalidRequest);
    }
    let mut value = empty_message(proto, "blend.model.CharaHome")?;
    for (field, number) in [
        ("slot_id", slot_id),
        ("character_id", character_id),
        ("motion_id", motion_id),
        ("camera_id", camera_id),
        ("bgm_id", bgm_id),
        ("background_id", background_id),
    ] {
        value.set_field_by_name(field, Value::I32(number));
    }
    value.set_field_by_name("created_at", Value::Message(timestamp(proto, now)?));
    put(resources, "chara_homes", "slot_id", value.clone());
    put(changed, "chara_homes", "slot_id", value);
    Ok(())
}

pub(crate) fn notifications(
    proto: &ProtoRegistry,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    home: &HomeState,
    now: i64,
) -> Result<(), StateError> {
    let mut value = resources
        .get_field_by_name("notifications")
        .and_then(|v| v.as_message().cloned())
        .unwrap_or(empty_message(proto, "blend.model.Notifications")?);
    let mut mail = empty_message(proto, "google.protobuf.BoolValue")?;
    let unread = home.mails.iter().try_fold(false, |found, bytes| {
        let message = proto
            .decode("blend.model.Mail", bytes)
            .map_err(|e| StateError::Descriptor(e.to_string()))?;
        Ok::<_, StateError>(
            found
                || (!message.has_field_by_name("opened_at")
                    && message_i64_field(&message, "end_at", "seconds")
                        .is_none_or(|end| now < end)),
        )
    })?;
    mail.set_field_by_name("value", Value::Bool(unread));
    value.set_field_by_name("mail", Value::Message(mail));
    value.set_field_by_name(
        "news",
        Value::Message(empty_message(proto, "blend.model.NewsNotification")?),
    );
    resources.set_field_by_name("notifications", Value::Message(value.clone()));
    changed.set_field_by_name("notifications", Value::Message(value));
    Ok(())
}

pub(crate) fn mail_list(
    proto: &ProtoRegistry,
    home: &HomeState,
    now: i64,
) -> Result<DynamicMessage, StateError> {
    let mut result = empty_message(proto, "blend.model.MailList")?;
    for bytes in &home.mails {
        let value = proto
            .decode("blend.model.Mail", bytes)
            .map_err(|e| StateError::Descriptor(e.to_string()))?;
        if message_i64_field(&value, "end_at", "seconds").is_some_and(|end| now >= end) {
            continue;
        }
        let field = if value.has_field_by_name("opened_at") {
            "opened"
        } else {
            "unopened"
        };
        append_changed_message(&mut result, field, value);
    }
    Ok(result)
}

pub(crate) fn login_bonus(
    proto: &ProtoRegistry,
    rules: &HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    home: &mut HomeState,
    now: i64,
) -> Result<Vec<Value>, StateError> {
    if tutorial_step(resources) < TUTORIAL_STEP_GACHA_COMPLETE {
        return Ok(Vec::new());
    }
    if tutorial_step(resources) == TUTORIAL_STEP_GACHA_COMPLETE {
        let mut status = status_message(resources)?;
        status.set_field_by_name("tutorial_step", Value::I32(TUTORIAL_STEP_HOME_READY));
        resources.set_field_by_name("status", Value::Message(status.clone()));
        changed.set_field_by_name("status", Value::Message(status));
    }
    if home.login_day != Some(day(now)) {
        home.login_day = Some(day(now));
        advance_missions(proto, rules, resources, changed, now, Some(("login", 1)))?;
    }
    let mut results = Vec::new();
    for bonus in &rules.login_bonuses {
        if !in_period(bonus.start_at, bonus.end_at, now)
            || bonus
                .elapsed_days
                .is_some_and(|limit| day(now) - day(home.initialized_at) >= i64::from(limit))
        {
            continue;
        }
        let progress = home.bonuses.entry(bonus.id).or_insert(BonusState {
            day: 0,
            last_day: i64::MIN,
        });
        if progress.last_day == day(now) {
            continue;
        }
        let next = progress.day + 1;
        let mut value = empty_message(proto, "blend.model.LoginBonus")?;
        value.set_field_by_name("login_bonus_id", Value::I32(bonus.id));
        value.set_field_by_name("day", Value::I32(next));
        let reward_set = if bonus.kind == 1 {
            let row = rules
                .login_regular_days
                .iter()
                .filter(|r| r.login_bonus_id == bonus.id && in_period(r.start_at, None, now))
                .max_by_key(|r| r.start_at)
                .ok_or(StateError::InvalidRequest)?;
            let index = (next as usize - 1) % row.reward_set_ids.len();
            value.set_field_by_name(
                "regular_day_id",
                Value::Message(int32_value(proto, row.id)?),
            );
            value.set_field_by_name("regular_step", Value::I32(index as i32));
            row.reward_set_ids[index]
        } else {
            let Some(row) = rules
                .login_days
                .iter()
                .find(|r| r.login_bonus_id == bonus.id && r.day == next)
            else {
                continue;
            };
            row.reward_set_id
        };
        progress.day = next;
        progress.last_day = day(now);
        home.next_mail_id = home
            .next_mail_id
            .checked_add(1)
            .ok_or(StateError::InvalidRequest)?;
        let mut mail = empty_message(proto, "blend.model.Mail")?;
        mail.set_field_by_name("entity_id", Value::I32(home.next_mail_id));
        mail.set_field_by_name("mail_type", Value::I32(1));
        mail.set_field_by_name("created_at", Value::Message(timestamp(proto, now)?));
        mail.set_field_by_name(
            "end_at",
            Value::Message(timestamp(proto, now + 90 * 86400)?),
        );
        let mut params = empty_message(proto, "blend.model.MailParams")?;
        params.set_field_by_name(
            "mail_template_id",
            Value::Message(int32_value(proto, if bonus.kind == 1 { 4 } else { 2 })?),
        );
        params.set_field_by_name(
            "login_bonus_id",
            Value::Message(int32_value(proto, bonus.id)?),
        );
        params.set_field_by_name("value", Value::Message(int32_value(proto, next)?));
        mail.set_field_by_name("mail_params", Value::Message(params));
        let contents = rewards(rules, reward_set)?
            .iter()
            .map(|r| {
                let mut value = resource_message(proto, r.resource_type, r.id, r.quantity)?;
                let source = reward_message(proto, r, false)?;
                if let Some(params) = source.get_field_by_name("resource_params") {
                    value.set_field_by_name("resource_params", params.into_owned());
                }
                Ok(Value::Message(value))
            })
            .collect::<Result<Vec<_>, StateError>>()?;
        mail.set_field_by_name("rewards", Value::List(contents));
        home.mails.push(mail.encode_to_vec());
        results.push(Value::Message(value));
    }
    Ok(results)
}

pub(crate) fn claim_counts(
    proto: &ProtoRegistry,
    rules: &HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    category: i32,
    now: i64,
) -> Result<Vec<Value>, StateError> {
    let Some(rule) = rules
        .mission_count_rewards
        .iter()
        .find(|r| r.category == category && in_period(r.start_at, r.end_at, now))
    else {
        return Ok(Vec::new());
    };
    let count: usize = rules
        .missions
        .iter()
        .filter(|r| r.category == category && mission_active(rules, r, now))
        .map(|r| received(resources, r.id))
        .sum();
    let mut state = message_list(resources, "mission_count_reward_states")
        .into_iter()
        .find(|s| i32_field(s, "category") == Some(category))
        .unwrap_or(empty_message(proto, "blend.model.MissionCountRewardState")?);
    state.set_field_by_name("category", Value::I32(category));
    let mut index = i32_field(&state, "received_step_count").unwrap_or(0).max(0) as usize;
    let mut result = Vec::new();
    while let Some(step) = rule
        .steps
        .get(index)
        .filter(|s| s.count as usize <= count && in_period(s.start_at, None, now))
    {
        result.extend(grant(
            proto,
            rules,
            resources,
            changed,
            rewards(rules, step.reward_set_id)?,
            now,
        )?);
        index += 1;
    }
    state.set_field_by_name("received_step_count", Value::I32(index as i32));
    if let Some(reset) = reset_at(rule.reset_cycle, now) {
        state.set_field_by_name("reset_at", Value::Message(timestamp(proto, reset)?));
    }
    put(
        resources,
        "mission_count_reward_states",
        "category",
        state.clone(),
    );
    put(changed, "mission_count_reward_states", "category", state);
    Ok(result)
}

pub(crate) fn claim_missions(
    proto: &ProtoRegistry,
    rules: &HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    home: &mut HomeState,
    request: &DynamicMessage,
    now: i64,
) -> Result<(Vec<Value>, Vec<Value>), StateError> {
    let mut ids = i32_list(request, "mission_ids");
    if ids.is_empty() || ids.iter().copied().collect::<BTreeSet<_>>().len() != ids.len() {
        return Err(StateError::InvalidRequest);
    }
    let mut result = Vec::new();
    let mut categories = BTreeSet::new();
    ids.sort_by_key(|id| {
        let mut depth = 0;
        let mut current = *id;
        while let Some(previous) = rules
            .missions
            .iter()
            .find(|r| r.id == current)
            .and_then(|r| r.prev_mission_id)
        {
            depth += 1;
            current = previous;
            if depth > rules.missions.len() {
                break;
            }
        }
        depth
    });
    let bulk = request
        .get_field_by_name("bulk_receive")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    for id in ids {
        let row = rules
            .missions
            .iter()
            .find(|r| r.id == id)
            .ok_or(StateError::InvalidRequest)?;
        if !mission_active(rules, row, now) {
            return Err(StateError::OutOfSchedule);
        }
        if row.prev_mission_id.is_some_and(|id| {
            rules
                .missions
                .iter()
                .find(|r| r.id == id)
                .is_none_or(|r| received(resources, id) < r.steps.len())
        }) {
            return Err(StateError::InvalidRequest);
        }
        let mut index = received(resources, id);
        let Some(step) = row.steps.get(index) else {
            continue;
        };
        if mission_count(resources, row) < step.count || !in_period(step.start_at, None, now) {
            return Err(StateError::InvalidRequest);
        }
        loop {
            let step = &row.steps[index];
            result.extend(grant(
                proto,
                rules,
                resources,
                changed,
                rewards(rules, step.reward_set_id)?,
                now,
            )?);
            index += 1;
            if !bulk
                || row.steps.get(index).is_none_or(|s| {
                    mission_count(resources, row) < s.count || !in_period(s.start_at, None, now)
                })
            {
                break;
            }
        }
        let mut value = message_list(resources, "missions")
            .into_iter()
            .find(|r| i32_field(r, "mission_id") == Some(id))
            .ok_or(StateError::InvalidRequest)?;
        value.set_field_by_name("received_step_count", Value::I32(index as i32));
        put(resources, "missions", "mission_id", value.clone());
        put(changed, "missions", "mission_id", value);
        categories.insert(row.category);
    }
    for category in categories {
        result.extend(claim_counts(
            proto, rules, resources, changed, category, now,
        )?);
    }
    let mut step_rewards = Vec::new();
    for step in &rules.guide_steps {
        if home.guide_rewards.contains(&step.id) {
            continue;
        }
        let members: Vec<_> = rules
            .missions
            .iter()
            .filter(|r| r.guide_mission_step_id == Some(step.id) && mission_active(rules, r, now))
            .collect();
        if !members.is_empty()
            && members
                .iter()
                .all(|r| received(resources, r.id) == r.steps.len())
        {
            step_rewards.extend(grant(
                proto,
                rules,
                resources,
                changed,
                rewards(rules, step.reward_set_id)?,
                now,
            )?);
            home.guide_rewards.insert(step.id);
        }
    }
    Ok((result, step_rewards))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn open_mail(
    proto: &ProtoRegistry,
    rules: &HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    home: &mut HomeState,
    request: &DynamicMessage,
    now: i64,
    delete: bool,
) -> Result<Vec<Value>, StateError> {
    let ids = i32_list(request, "entity_ids");
    if ids.is_empty() || ids.iter().copied().collect::<BTreeSet<_>>().len() != ids.len() {
        return Err(StateError::InvalidRequest);
    }
    let mut result = Vec::new();
    for id in ids {
        let index = home
            .mails
            .iter()
            .position(|bytes| {
                proto
                    .decode("blend.model.Mail", bytes)
                    .ok()
                    .and_then(|v| i32_field(&v, "entity_id"))
                    == Some(id)
            })
            .ok_or(StateError::InvalidRequest)?;
        let mut mail = proto
            .decode("blend.model.Mail", &home.mails[index])
            .map_err(|e| StateError::Descriptor(e.to_string()))?;
        if delete {
            if !mail.has_field_by_name("opened_at") {
                return Err(StateError::InvalidRequest);
            }
            home.mails.remove(index);
            continue;
        }
        if mail.has_field_by_name("opened_at") {
            continue;
        }
        if message_i64_field(&mail, "end_at", "seconds").is_some_and(|end| now >= end) {
            return Err(StateError::OutOfSchedule);
        }
        let inputs: Vec<_> = message_list(&mail, "rewards")
            .iter()
            .map(|r| {
                let resource_params = r
                    .get_field_by_name("resource_params")
                    .and_then(|value| value.as_message().cloned())
                    .map(|params| TutorialResourceParams {
                        level: message_i32_field(&params, "level", "value"),
                        rank: message_i32_field(&params, "rank", "value"),
                        skin: message_i32_field(&params, "skin", "value"),
                        traits: message_list(&params, "traits")
                            .into_iter()
                            .filter_map(|trait_param| {
                                Some(TutorialTraitParam {
                                    id: i32_field(&trait_param, "id")?,
                                    rank: i32_field(&trait_param, "rank")?,
                                })
                            })
                            .collect(),
                    });
                Ok(TutorialReward {
                    resource_type: i32_field(r, "type").unwrap_or(0),
                    id: i32_field(r, "id").unwrap_or(0),
                    quantity: i32_field(r, "quantity").unwrap_or(0),
                    resource_params,
                })
            })
            .collect::<Result<Vec<_>, StateError>>()?;
        result.extend(grant(proto, rules, resources, changed, &inputs, now)?);
        mail.set_field_by_name("opened_at", Value::Message(timestamp(proto, now)?));
        home.mails[index] = mail.encode_to_vec();
    }
    Ok(result)
}

pub(crate) fn reward_list(
    proto: &ProtoRegistry,
    rewards: &[TutorialReward],
) -> Result<DynamicMessage, StateError> {
    let mut list = empty_message(proto, "blend.model.RewardList")?;
    list.set_field_by_name(
        "rewards",
        Value::List(
            rewards
                .iter()
                .map(|reward| reward_message(proto, reward, false).map(Value::Message))
                .collect::<Result<Vec<_>, _>>()?,
        ),
    );
    Ok(list)
}
