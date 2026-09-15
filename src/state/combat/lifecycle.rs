use super::prelude::*;

pub(crate) fn enemy_member_status_enemy_id(member: &DynamicMessage) -> Result<i32, StateError> {
    member
        .get_field_by_name("enemy")
        .and_then(|value| value.as_message().cloned())
        .and_then(|enemy| i32_field(&enemy, "enemy_id"))
        .ok_or(StateError::InvalidRequest)
}

pub(crate) fn roll_quest_rewards(
    rules: &RewardRules,
    quest_id: i32,
) -> Result<Vec<TutorialReward>, StateError> {
    roll_ranked_quest_rewards(rules, quest_id, None)
}

pub(crate) fn roll_ranked_quest_rewards(
    rules: &RewardRules,
    quest_id: i32,
    score_rank: Option<i32>,
) -> Result<Vec<TutorialReward>, StateError> {
    let Some(quest) = rules.quests.iter().find(|quest| quest.id == quest_id) else {
        return Ok(Vec::new());
    };
    let mut quantities = BTreeMap::new();
    let rank_drops = score_rank
        .map(|rank| {
            quest
                .score_ranks
                .iter()
                .find(|entry| entry.rank == rank)
                .ok_or(StateError::InvalidRequest)
        })
        .transpose()?;
    for set_id in rank_drops.iter().flat_map(|rank| &rank.reward_set_ids) {
        let set = rules
            .reward_sets
            .iter()
            .find(|set| set.id == *set_id)
            .ok_or_else(|| StateError::RewardRules(format!("missing reward set {set_id}")))?;
        for reward in &set.rewards {
            let quantity = quantities
                .entry((reward.resource_type, reward.id))
                .or_insert(0i32);
            *quantity = quantity
                .checked_add(reward.quantity)
                .ok_or(StateError::InvalidRequest)?;
        }
    }
    for set_id in quest
        .drop_reward_set_ids
        .iter()
        .chain(rank_drops.iter().flat_map(|rank| &rank.drop_reward_set_ids))
    {
        let set = rules
            .drop_reward_sets
            .iter()
            .find(|set| set.id == *set_id)
            .ok_or_else(|| StateError::RewardRules(format!("missing drop set {set_id}")))?;
        for roll in &set.rolls {
            if roll.rate < 100 && random_below(100)? >= roll.rate {
                continue;
            }
            for reward in &roll.rewards {
                let width = reward
                    .max_quantity
                    .checked_sub(reward.min_quantity)
                    .and_then(|value| value.checked_add(1))
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or(StateError::InvalidRequest)?;
                let quantity = reward
                    .min_quantity
                    .checked_add(
                        i32::try_from(random_below(width)?)
                            .map_err(|_| StateError::InvalidRequest)?,
                    )
                    .ok_or(StateError::InvalidRequest)?;
                let entry = quantities
                    .entry((reward.resource_type, reward.id))
                    .or_insert(0i32);
                *entry = entry
                    .checked_add(quantity)
                    .ok_or(StateError::InvalidRequest)?;
            }
        }
    }
    Ok(quantities
        .into_iter()
        .map(|((resource_type, id), quantity)| TutorialReward {
            resource_type,
            id,
            quantity,
            resource_params: None,
        })
        .collect())
}

pub(crate) fn apply_quest_rewards(
    proto: &ProtoRegistry,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    rewards: &[TutorialReward],
) -> Result<Vec<Value>, StateError> {
    let mut messages = Vec::new();
    let mut changed_items = BTreeMap::new();
    for reward in rewards {
        let is_new = reward.resource_type == 5
            && item_quantity(resources, reward.id).unwrap_or_default() == 0;
        match reward.resource_type {
            1 => {
                let mut wallet = resources
                    .get_field_by_name("wallet")
                    .and_then(|value| value.as_message().cloned())
                    .ok_or(StateError::InvalidRequest)?;
                let next = i32_field(&wallet, "free")
                    .unwrap_or_default()
                    .checked_add(reward.quantity)
                    .ok_or(StateError::InvalidRequest)?;
                wallet.set_field_by_name("free", Value::I32(next));
                resources.set_field_by_name("wallet", Value::Message(wallet.clone()));
                changed.set_field_by_name("wallet", Value::Message(wallet));
            }
            3 => {
                let mut status = status_message(resources)?;
                let next = i32_field(&status, "cole")
                    .unwrap_or_default()
                    .checked_add(reward.quantity)
                    .ok_or(StateError::InvalidRequest)?;
                status.set_field_by_name("cole", Value::I32(next));
                resources.set_field_by_name("status", Value::Message(status.clone()));
                changed.set_field_by_name("status", Value::Message(status));
            }
            5 => {
                changed_items.insert(
                    reward.id,
                    change_item(proto, resources, reward.id, reward.quantity)?,
                );
            }
            other => {
                return Err(StateError::RewardRules(format!(
                    "unsupported drop reward type {other}"
                )))
            }
        }
        messages.push(Value::Message(reward_message(proto, reward, is_new)?));
    }
    if !changed_items.is_empty() {
        changed.set_field_by_name(
            "items",
            Value::List(changed_items.into_values().map(Value::Message).collect()),
        );
    }
    Ok(messages)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn reduce_battle_finish_with_rules(
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
    if !(101001001..=101001017).contains(&quest_id) {
        return quest::finish_battle(
            proto,
            rules,
            reward_rules,
            home_rules,
            character_rules,
            battle_progress,
            state,
            resources,
            quest_id,
        );
    }
    if current_battle_status(&state)? != BATTLE_STATUS_WON {
        return Err(StateError::InvalidRequest);
    }
    let quest = rules
        .quests
        .iter()
        .find(|quest| quest.id == quest_id && quest.quest_type == 1)
        .ok_or(StateError::InvalidRequest)?;
    let wave_ids = i32_list(&state, "wave_ids");
    if quest.battle_id != i32_field(&state, "battle_id")
        || usize::try_from(i32_field(&state, "wave").unwrap_or(0)).ok() != Some(wave_ids.len())
    {
        return Err(StateError::InvalidRequest);
    }
    if quest_clear_count(&resources, quest_id) != 0 {
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
    upsert_quest_state(&mut changed, quest_state);
    let rolled_rewards = roll_quest_rewards(reward_rules, quest_id)?;
    let rewards = apply_quest_rewards(proto, &mut resources, &mut changed, &rolled_rewards)?;
    let mut first_clear_rewards = Vec::new();
    let mut indexes = Vec::new();
    let reward_set = quest
        .first_clear_reward_set_id
        .map(|id| {
            rules
                .reward_sets
                .iter()
                .find(|set| set.id == id)
                .ok_or_else(|| StateError::TutorialRules(format!("missing reward set {id}")))
        })
        .transpose()?;
    let mut cole_delta = 0i32;
    for reward in reward_set.into_iter().flat_map(|set| set.rewards.iter()) {
        match reward.resource_type {
            3 => {
                cole_delta = cole_delta
                    .checked_add(reward.quantity)
                    .ok_or(StateError::InvalidRequest)?;
                let message = reward_message(proto, reward, false)?;
                first_clear_rewards.push(Value::Message(message));
            }
            17 => {
                let memoria_rule = rules
                    .reward_memorias
                    .iter()
                    .find(|memoria| memoria.id == reward.id)
                    .ok_or_else(|| {
                        StateError::TutorialRules(format!("missing memoria {}", reward.id))
                    })?;
                let entity_id = next_entity_id(&resources)?;
                let mut memoria = empty_message(proto, "blend.model.Memoria")?;
                memoria.set_field_by_name("entity_id", Value::I32(entity_id));
                memoria.set_field_by_name("memoria_id", Value::I32(memoria_rule.id));
                memoria.set_field_by_name("limit_break", Value::I32(0));
                memoria.set_field_by_name("exp", Value::I32(0));
                memoria.set_field_by_name("is_locked", Value::Bool(false));
                memoria.set_field_by_name(
                    "received_at",
                    Value::Message(timestamp(proto, unix_now())?),
                );
                let mut all_memorias = message_list(&resources, "memorias");
                all_memorias.push(memoria.clone());
                resources.set_field_by_name(
                    "memorias",
                    Value::List(all_memorias.into_iter().map(Value::Message).collect()),
                );
                changed.set_field_by_name("memorias", Value::List(vec![Value::Message(memoria)]));
                let count = total_task_count(&resources, 139).max(1);
                let task = set_total_task_count(&mut resources, 139, count)?;
                set_changed_task_counts(&mut changed, vec![task]);
                let mut message = reward_message(proto, reward, true)?;
                message.set_field_by_name("entity_id", Value::I32(entity_id));
                first_clear_rewards.push(Value::Message(message));
            }
            other => {
                return Err(StateError::TutorialRules(format!(
                    "unsupported finish reward type {other}"
                )))
            }
        }
    }
    if cole_delta != 0 {
        let mut updated_status = resources
            .get_field_by_name("status")
            .and_then(|value| value.as_message().cloned())
            .ok_or(StateError::InvalidRequest)?;
        let old = i32_field(&updated_status, "cole").unwrap_or(0);
        updated_status.set_field_by_name(
            "cole",
            Value::I32(
                old.checked_add(cole_delta)
                    .ok_or(StateError::InvalidRequest)?,
            ),
        );
        resources.set_field_by_name("status", Value::Message(updated_status.clone()));
        changed.set_field_by_name("status", Value::Message(updated_status));
    }

    if quest.character_exp > 0 && quest.fixed_party_id.is_none() {
        let character_ids: Vec<_> = message_list(&resources, "party_members")
            .into_iter()
            .filter(|row| {
                i32_field(row, "party_type") == Some(1) && i32_field(row, "number") == Some(1)
            })
            .filter_map(|row| optional_i32_field(&row, "character_id"))
            .collect();
        if character_ids.is_empty() {
            return Err(StateError::InvalidRequest);
        }
        for character_id in character_ids {
            let mut character = resource_character(&resources, character_id)?;
            let exp = i32_field(&character, "exp")
                .unwrap_or(0)
                .checked_add(quest.character_exp)
                .ok_or(StateError::InvalidRequest)?;
            character.set_field_by_name("exp", Value::I32(exp));
            upsert_character(&mut resources, character.clone(), false);
            let mut changed_characters = message_list(&changed, "characters");
            changed_characters.push(character.clone());
            changed.set_field_by_name(
                "characters",
                Value::List(changed_characters.into_iter().map(Value::Message).collect()),
            );
            indexes.push((
                i64::from(character_id),
                optional_i32_field(&character, "memoria_entity_id").map(i64::from),
            ));
        }
        let max_level = message_list(&resources, "characters")
            .iter()
            .map(|character| level_for_exp(rules, i32_field(character, "exp").unwrap_or(0)))
            .collect::<Result<Vec<_>, StateError>>()?
            .into_iter()
            .max()
            .ok_or(StateError::InvalidRequest)?;
        let task = set_total_task_count(&mut resources, 47, max_level)?;
        set_changed_task_counts(&mut changed, vec![task]);
    }
    for character in message_list(&resources, "characters") {
        let id = i32_field(&character, "character_id").ok_or(StateError::InvalidRequest)?;
        if !indexes
            .iter()
            .any(|(existing, _)| *existing == i64::from(id))
        {
            indexes.push((
                i64::from(id),
                optional_i32_field(&character, "memoria_entity_id").map(i64::from),
            ));
        }
    }
    let mut quest_result = empty_message(proto, "blend.model.QuestResult")?;
    quest_result.set_field_by_name("rewards", Value::List(rewards));
    quest_result.set_field_by_name("first_clear_rewards", Value::List(first_clear_rewards));
    let mut response = empty_message(proto, "blend.api.BattleFinishResponse")?;
    response.set_field_by_name("quest_result", Value::Message(quest_result));
    response.set_field_by_name("changed_resources", Value::Message(changed));
    Ok(BattleFinishMutation {
        resources,
        response,
        character_indexes: indexes,
    })
}

/// Compatibility wrapper retained for isolated fixture tests.
#[cfg(test)]
pub(crate) fn reduce_battle_finish(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    reward_rules: &RewardRules,
    state: DynamicMessage,
    resources: DynamicMessage,
    quest_id: i32,
) -> Result<BattleFinishMutation, StateError> {
    let home_rules = home::load_rules()?;
    let character_rules = load_character_rules()?;
    let battle_progress = home::BattleProgress::default();
    reduce_battle_finish_with_rules(
        proto,
        rules,
        reward_rules,
        &home_rules,
        character_rules,
        &battle_progress,
        state,
        resources,
        quest_id,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn reduce_battle_start_with_progression(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    atelier_rules: &atelier::AtelierRules,
    character_rules: &CharacterRules,
    resources: DynamicMessage,
    quest_id: i32,
    party_number: i32,
    ship_id: Option<i32>,
    party_status: Option<&DynamicMessage>,
    mode: BattleStartMode,
    now: i64,
) -> Result<BattleStartMutation, StateError> {
    let quest = rules
        .quests
        .iter()
        .find(|quest| quest.id == quest_id)
        .ok_or(StateError::InvalidRequest)?;
    build_battle_start(
        proto,
        rules,
        atelier_rules,
        character_rules,
        resources,
        quest,
        party_number,
        ship_id,
        party_status,
        mode,
        now,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_battle_start(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    atelier_rules: &atelier::AtelierRules,
    character_rules: &CharacterRules,
    resources: DynamicMessage,
    quest: &TutorialQuest,
    party_number: i32,
    ship_id: Option<i32>,
    party_status: Option<&DynamicMessage>,
    mode: BattleStartMode,
    now: i64,
) -> Result<BattleStartMutation, StateError> {
    let quest_id = quest.id;
    if quest.quest_type != 1 {
        return Err(StateError::InvalidRequest);
    }
    if quest.start_at.is_some_and(|start| now < start) || quest.end_at.is_some_and(|end| now >= end)
    {
        return Err(StateError::OutOfSchedule);
    }
    quest::validate_quest(quest, &resources, now)?;
    if (101001001..=101001017).contains(&quest_id) && quest_clear_count(&resources, quest_id) != 0 {
        return Err(StateError::InvalidRequest);
    }
    let required_tutorial_step = match quest_id {
        101001008 => TUTORIAL_STEP_FIRST_SYNTHESIS,
        101001013 => TUTORIAL_STEP_SECOND_SYNTHESIS,
        101001015 => TUTORIAL_STEP_MEMORIA_EQUIPPED,
        _ => 0,
    };
    if tutorial_step(&resources) < required_tutorial_step {
        return Err(StateError::InvalidRequest);
    }
    let battle_id = match mode {
        BattleStartMode::SoloRaid => *quest.battle_ids.first().ok_or(StateError::InvalidRequest)?,
        _ => quest.battle_id.ok_or(StateError::InvalidRequest)?,
    };
    let battle = rule_battle(rules, battle_id)?;
    let fixed_party_id = match mode {
        BattleStartMode::Rental => quest.rental_fixed_party_id,
        _ => quest.fixed_party_id,
    };
    let (mut party_members, mut party_tools) = match fixed_party_id {
        Some(fixed_party_id) => resolve_fixed_party_with_rules(
            proto,
            rules,
            atelier_rules,
            character_rules,
            fixed_party_id,
        )?,
        None => resolve_account_party(
            rules,
            atelier_rules,
            character_rules,
            &resources,
            party_number,
        )?,
    };
    let (battle_ship, ship_tools, mut external_passives) =
        resolve_battle_ship(proto, rules, atelier_rules, &resources, ship_id)?;
    external_passives.extend(scaled_ability_effects(
        rules,
        &quest.field_ability_ids,
        10_000,
        None,
    )?);
    apply_leader_passives(rules, &mut party_members)?;
    if battle.wave_ids.is_empty() || party_members.is_empty() {
        return Err(StateError::InvalidRequest);
    }
    if let Some(status) = party_status {
        let counts = i32_list(status, "battle_tool_usage_counts");
        if counts.len() != party_tools.len() {
            return Err(StateError::InvalidRequest);
        }
        for (tool, count) in party_tools.iter_mut().zip(counts) {
            if !(0..=tool.usage_count).contains(&count) {
                return Err(StateError::InvalidRequest);
            }
            tool.usage_count = count;
        }
    }
    let first_wave = rule_wave(rules, battle.wave_ids[0])?;
    if first_wave.enemies.is_empty() {
        return Err(StateError::TutorialRules(
            "first battle wave has no enemies".into(),
        ));
    }

    let mut member_values = Vec::new();
    if party_tools.len() > usize::try_from(rules.constants.turn_max_battle_tool_count).unwrap_or(0)
    {
        return Err(StateError::InvalidRequest);
    }
    let mental_buff = 0; // Party abilities are evaluated by the shared effect runtime.
    for (index, member) in party_members.iter().enumerate() {
        let member_id = i32::try_from(index + 1).map_err(|_| StateError::InvalidRequest)?;
        let mut ally = build_ally_member(proto, rules, member, member_id, battle_id, mental_buff)?;
        if let Some(status) = party_status {
            let hp = *i32_list(status, "hps")
                .get(index)
                .ok_or(StateError::InvalidRequest)?;
            if hp < 0 {
                return Err(StateError::InvalidRequest);
            }
            ally.set_field_by_name(
                "hp",
                Value::I32(hp.min(i32_field(&ally, "max_hp").unwrap_or(0))),
            );
            ally.set_field_by_name("is_alive", Value::Bool(hp > 0));
        }
        member_values.push(Value::Message(ally));
    }
    let mut base_numbers: Vec<(i32, i32)> = Vec::new();
    for (index, wave_enemy) in first_wave.enemies.iter().enumerate() {
        // Match the client battle-member namespace: enemies start at 11.
        let member_id = 11 + i32::try_from(index).map_err(|_| StateError::InvalidRequest)?;
        let enemy = rule_enemy(rules, wave_enemy.id)?;
        let number = if let Some((_, count)) = base_numbers
            .iter_mut()
            .find(|(base_id, _)| *base_id == enemy.base_enemy_id)
        {
            *count += 1;
            *count
        } else {
            base_numbers.push((enemy.base_enemy_id, 1));
            1
        };
        let enemy_member =
            build_enemy_member(proto, rules, wave_enemy, first_wave.id, member_id, number)?;
        member_values.push(Value::Message(enemy_member));
    }

    let mut state = empty_message(proto, "blend.model.BattleState")?;
    state.set_field_by_name("battle_id", Value::I32(battle_id));
    state.set_field_by_name(
        "wave_ids",
        Value::List(battle.wave_ids.iter().copied().map(Value::I32).collect()),
    );
    state.set_field_by_name("wave", Value::I32(1));
    state.set_field_by_name("total_turn", Value::I32(1));
    state.set_field_by_name("members", Value::List(member_values));
    set_battle_field_effect(proto, &mut state, first_wave.field_effect_id)?;
    state.set_field_by_name("ship", Value::Message(battle_ship));
    state.set_field_by_name(
        "ship_tools",
        Value::List(ship_tools.into_iter().map(Value::Message).collect()),
    );
    state.set_field_by_name("bomb_gauge", Value::I32(rules.constants.initial_bomb_gauge));
    let start_txid = Uuid::new_v4().to_string();
    let mut effects = effects::Runtime::initialize(
        proto,
        rules,
        &mut state,
        &start_txid,
        &party_members,
        &external_passives,
    )?;
    let member_messages = message_list(&state, "members");
    let timeline_values = tutorial_timeline_units(proto, rules, battle_id, 1, &member_messages)?;
    state.set_field_by_name(
        "timeline_units",
        Value::List(timeline_values.into_iter().map(Value::Message).collect()),
    );
    state.set_field_by_name(
        "battle_tools",
        Value::List(
            party_tools
                .iter()
                .enumerate()
                .map(|(index, tool)| {
                    build_battle_tool(proto, tool, i32::try_from(index + 1).unwrap_or(1))
                        .map(Value::Message)
                })
                .collect::<Result<Vec<_>, _>>()?,
        ),
    );
    state.set_field_by_name(
        "party_gauge",
        Value::I32(
            party_status
                .and_then(|s| i32_field(s, "party_gauge"))
                .unwrap_or(500),
        ),
    );
    set_base_enemy_numbers(proto, &mut state, &base_numbers)?;
    set_timeline_panels(
        proto,
        &mut state,
        &tutorial_timeline_panels(rules, battle_id, 1)?,
        rules.constants.timeline_panel_count,
        1,
    )?;
    effects.acquire_current_panel(&mut state)?;
    refresh_burst_enable(rules, &mut state)?;

    let mut start_state = state.clone();
    start_state.set_field_by_name(
        "members",
        Value::List(
            member_messages
                .iter()
                .filter(|member| member_type(member).ok() == Some(0))
                .cloned()
                .map(Value::Message)
                .collect(),
        ),
    );
    for field in [
        "timeline_units",
        "timeline_panels",
        "base_enemy_numbers",
        "enemy_hp_gauge",
        "bomb_gauge",
        "field_effect",
        "ship_tools",
    ] {
        start_state.clear_field_by_name(field);
    }
    let mut start = empty_message(proto, "blend.model.BattleStart")?;
    start.set_field_by_name("state", Value::Message(start_state));
    let mut history = empty_message(proto, "blend.model.BattleHistory")?;
    history.set_field_by_name("status", Value::EnumNumber(0));
    history.set_field_by_name("start", Value::Message(start));
    // BattleServerControl.SetInfoOnBattleStart indexes the first converted
    // action setup and resolves a wave start by its action number.  These are
    // server-generated initial records, not a recorded response.
    let mut wave_start = empty_message(proto, "blend.model.BattleWaveStart")?;
    wave_start.set_field_by_name("action_number", Value::I32(1));
    wave_start.set_field_by_name("state", Value::Message(state.clone()));
    history.set_field_by_name("wave_starts", Value::List(vec![Value::Message(wave_start)]));
    let initial_actor = current_actor(&state)?;
    let action_setup = build_action_setup(
        proto,
        rules,
        1,
        state.clone(),
        member_type(&initial_actor)?,
        member_id(&initial_actor)?,
        Some(&resources),
        Some(&effects),
    )?;
    history.set_field_by_name(
        "action_setups",
        Value::List(vec![Value::Message(action_setup)]),
    );
    history.set_field_by_name("auto_type", Value::EnumNumber(0));

    let mut context = empty_message(proto, "blend.model.BattleContext")?;
    context.set_field_by_name("quest_id", Value::I32(quest_id));
    context.set_field_by_name("start_txid", Value::String(start_txid.clone()));
    context.set_field_by_name(
        "party_number",
        Value::Message(int32_value(proto, party_number)?),
    );
    if mode == BattleStartMode::Rental {
        context.set_field_by_name(
            "rental_fixed_party_id",
            Value::Message(int32_value(
                proto,
                quest
                    .rental_fixed_party_id
                    .ok_or(StateError::InvalidRequest)?,
            )?),
        );
    }
    let mut response = empty_message(proto, "blend.api.BattleStartResponse")?;
    response.set_field_by_name("history", Value::Message(history.clone()));
    response.set_field_by_name("context", Value::Message(context));
    response.set_field_by_name(
        "changed_resources",
        Value::Message(empty_message(proto, "blend.model.Resources")?),
    );
    Ok(BattleStartMutation {
        state,
        response,
        start_txid,
        quest_id,
        battle_id,
        effects,
    })
}

pub(crate) fn register_special_battle_state(
    proto: &ProtoRegistry,
    resources: &mut DynamicMessage,
    quest: &TutorialQuest,
    mode: BattleStartMode,
    now: i64,
) -> Result<(), StateError> {
    let ids = message_list(resources, "party_members")
        .into_iter()
        .filter(|member| {
            i32_field(member, "party_type") == Some(1) && i32_field(member, "number") == Some(1)
        })
        .filter_map(|member| optional_i32_field(&member, "character_id"))
        .collect::<Vec<_>>();
    match mode {
        BattleStartMode::SoloRaid => {
            let mut state = message_list(resources, "solo_raid_states")
                .into_iter()
                .find(|s| i32_field(s, "quest_id") == Some(quest.id))
                .unwrap_or(empty_message(proto, "blend.model.SoloRaidState")?);
            let mut used = i32_list(&state, "used_character_ids");
            if ids.iter().any(|id| used.contains(id)) {
                return Err(StateError::InvalidRequest);
            }
            used.extend(ids);
            state.set_field_by_name("quest_id", Value::I32(quest.id));
            if !state.has_field_by_name("entered_at") {
                state.set_field_by_name("entered_at", Value::Message(timestamp(proto, now)?));
            }
            state.set_field_by_name(
                "used_character_ids",
                Value::List(used.into_iter().map(Value::I32).collect()),
            );
            home::put(resources, "solo_raid_states", "quest_id", state);
        }
        BattleStartMode::Total => {
            let panel = quest
                .total_battle_panel_id
                .ok_or(StateError::InvalidRequest)?;
            let total = quest.total_battle_id.ok_or(StateError::InvalidRequest)?;
            if message_list(resources, "total_battle_panel_states")
                .iter()
                .any(|p| {
                    i32_field(p, "total_battle_id") == Some(total)
                        && !i32_list(p, "character_ids").is_empty()
                        && (i32_field(p, "total_battle_panel_id") == Some(panel)
                            || i32_list(p, "character_ids")
                                .iter()
                                .any(|id| ids.contains(id)))
                })
            {
                return Err(StateError::InvalidRequest);
            }
            let mut panel_state = empty_message(proto, "blend.model.TotalBattlePanelState")?;
            panel_state.set_field_by_name("total_battle_panel_id", Value::I32(panel));
            panel_state.set_field_by_name("total_battle_id", Value::I32(total));
            // Character IDs signify a cleared panel; reserve them only on victory.
            home::put(
                resources,
                "total_battle_panel_states",
                "total_battle_panel_id",
                panel_state,
            );
            let mut total_state = message_list(resources, "total_battle_states")
                .into_iter()
                .find(|s| i32_field(s, "total_battle_id") == Some(total))
                .unwrap_or(empty_message(proto, "blend.model.TotalBattleState")?);
            total_state.set_field_by_name("total_battle_id", Value::I32(total));
            home::put(
                resources,
                "total_battle_states",
                "total_battle_id",
                total_state,
            );
        }
        BattleStartMode::Rental | BattleStartMode::Standard | BattleStartMode::Gacha => {}
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn reduce_battle_start(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    resources: DynamicMessage,
    quest_id: i32,
    now: i64,
) -> Result<BattleStartMutation, StateError> {
    reduce_battle_start_with_progression(
        proto,
        rules,
        &atelier::load_rules()?,
        load_character_rules()?,
        resources,
        quest_id,
        1,
        None,
        None,
        BattleStartMode::Standard,
        now,
    )
}
