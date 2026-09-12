use super::prelude::*;

#[allow(clippy::too_many_arguments)]
pub(crate) fn quest_skip(
    proto: &ProtoRegistry,
    rules: &HomeRules,
    reward_rules: &RewardRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    request: &DynamicMessage,
    response: &mut DynamicMessage,
    now: i64,
) -> Result<(), StateError> {
    let quest_id = i32_field(request, "quest_id").ok_or(StateError::InvalidRequest)?;
    let party_number = i32_field(request, "party_number").ok_or(StateError::InvalidRequest)?;
    let skip_count = i32_field(request, "skip_count")
        .filter(|count| *count > 0)
        .ok_or(StateError::InvalidRequest)?;
    let quest = reward_rules
        .quests
        .iter()
        .find(|quest| quest.id == quest_id && quest.battle_id.is_some() && quest.quest_type == 1)
        .filter(|quest| matches!(quest.skippable_type, 2 | 3))
        .ok_or(StateError::InvalidRequest)?;
    let mut quest_state = message_list(resources, "quest_states")
        .into_iter()
        .find(|state| i32_field(state, "quest_id") == Some(quest_id))
        .filter(|state| i32_field(state, "clear_count").unwrap_or(0) > 0)
        .ok_or(StateError::InvalidRequest)?;
    if quest.skippable_type == 3 {
        let cleared = i32_list(&quest_state, "cleared_battle_mission_ids");
        if !quest
            .battle_mission_ids
            .iter()
            .all(|id| cleared.contains(id))
        {
            return Err(StateError::InvalidRequest);
        }
    }
    let old_clear_count = i32_field(&quest_state, "clear_count").unwrap_or(0);
    let score_rank = if quest.score_ranks.is_empty() {
        None
    } else {
        Some(
            i32_field(&quest_state, "score_rank")
                .filter(|rank| quest.score_ranks.iter().any(|entry| entry.rank == *rank))
                .ok_or(StateError::InvalidRequest)?,
        )
    };
    let new_clear_count = old_clear_count
        .checked_add(skip_count)
        .filter(|count| quest.max_clear_count == 0 || *count <= quest.max_clear_count)
        .ok_or(StateError::InvalidRequest)?;

    let party_exists = message_list(resources, "parties").iter().any(|party| {
        i32_field(party, "party_type") == Some(1)
            && i32_field(party, "number") == Some(party_number)
    });
    if !party_exists {
        return Err(StateError::InvalidRequest);
    }

    if let Some(episode) = reward_rules
        .episodes
        .iter()
        .find(|episode| episode.id == quest.episode_id && episode.max_daily_clear > 0)
    {
        let mut state = message_list(resources, "episode_states")
            .into_iter()
            .find(|state| i32_field(state, "episode_id") == Some(episode.id))
            .unwrap_or(empty_message(proto, "blend.model.EpisodeState")?);
        state.set_field_by_name("episode_id", Value::I32(episode.id));
        if message_i64_field(&state, "daily_updated_at", "seconds")
            .is_none_or(|time| day(time) != day(now))
        {
            state.set_field_by_name("daily_clear_count", Value::I32(0));
            state.set_field_by_name("daily_clear_addition_count", Value::I32(0));
        }
        let limit = episode
            .max_daily_clear
            .saturating_add(i32_field(&state, "daily_clear_addition_count").unwrap_or(0));
        let count = i32_field(&state, "daily_clear_count")
            .unwrap_or(0)
            .checked_add(skip_count)
            .filter(|count| *count <= limit)
            .ok_or(StateError::InvalidRequest)?;
        state.set_field_by_name("daily_clear_count", Value::I32(count));
        state.set_field_by_name("daily_updated_at", Value::Message(timestamp(proto, now)?));
        put(resources, "episode_states", "episode_id", state.clone());
        put(changed, "episode_states", "episode_id", state);
    }

    let stamina_cost = quest
        .stamina
        .checked_mul(skip_count)
        .ok_or(StateError::InvalidRequest)?;
    if stamina_cost > 0 {
        let mut status = status_message(resources)?;
        let stamina = i32_field(&status, "stamina_when_updated")
            .unwrap_or(0)
            .checked_sub(stamina_cost)
            .filter(|value| *value >= 0)
            .ok_or(StateError::InvalidRequest)?;
        status.set_field_by_name("stamina_when_updated", Value::I32(stamina));
        status.set_field_by_name("stamina_updated_at", Value::Message(timestamp(proto, now)?));
        resources.set_field_by_name("status", Value::Message(status.clone()));
        changed.set_field_by_name("status", Value::Message(status));
    }

    quest_attempt_progress(
        proto,
        rules,
        resources,
        changed,
        (quest.id, quest.stamina),
        skip_count,
        now,
    )?;
    let mut rewards_per_clear = Vec::new();
    let mut all_rewards = Vec::new();
    for _ in 0..skip_count {
        let rolled = roll_ranked_quest_rewards(reward_rules, quest_id, score_rank)?;
        rewards_per_clear.push(Value::Message(reward_list(proto, &rolled)?));
        all_rewards.extend(rolled);
    }
    let mut totals = BTreeMap::new();
    for reward in all_rewards {
        let quantity = totals
            .entry((reward.resource_type, reward.id))
            .or_insert(0i32);
        *quantity = quantity
            .checked_add(reward.quantity)
            .ok_or(StateError::InvalidRequest)?;
    }
    let all_rewards = totals
        .into_iter()
        .map(|((resource_type, id), quantity)| TutorialReward {
            resource_type,
            id,
            quantity,
            resource_params: None,
        })
        .collect::<Vec<_>>();
    let awarded = grant(proto, rules, resources, changed, &all_rewards, now)?;

    if quest.character_exp > 0 {
        let character_ids: BTreeSet<_> = message_list(resources, "party_members")
            .iter()
            .filter(|member| {
                i32_field(member, "party_type") == Some(1)
                    && i32_field(member, "number") == Some(party_number)
            })
            .filter_map(|member| optional_i32_field(member, "character_id"))
            .collect();
        let exp = quest
            .character_exp
            .checked_mul(skip_count)
            .ok_or(StateError::InvalidRequest)?;
        for id in character_ids {
            let mut character = message_list(resources, "characters")
                .into_iter()
                .find(|character| i32_field(character, "character_id") == Some(id))
                .ok_or(StateError::InvalidRequest)?;
            let next = i32_field(&character, "exp")
                .unwrap_or(0)
                .checked_add(exp)
                .ok_or(StateError::InvalidRequest)?;
            character.set_field_by_name("exp", Value::I32(next));
            put(resources, "characters", "character_id", character.clone());
            put(changed, "characters", "character_id", character);
        }
    }

    quest_state.set_field_by_name("clear_count", Value::I32(new_clear_count));
    put(resources, "quest_states", "quest_id", quest_state.clone());
    put(changed, "quest_states", "quest_id", quest_state);
    let mut result = empty_message(proto, "blend.model.QuestSkipResult")?;
    result.set_field_by_name("rewards_per_clear", Value::List(rewards_per_clear));
    result.set_field_by_name("rewards", Value::List(awarded));
    response.set_field_by_name("quest_result", Value::Message(result));
    Ok(())
}

pub(crate) fn daily_clear_add(
    proto: &ProtoRegistry,
    reward_rules: &RewardRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    request: &DynamicMessage,
    now: i64,
) -> Result<(), StateError> {
    let episode_id = i32_field(request, "episode_id").ok_or(StateError::InvalidRequest)?;
    let count = i32_field(request, "count")
        .filter(|count| *count > 0)
        .ok_or(StateError::InvalidRequest)?;
    let episode = reward_rules
        .episodes
        .iter()
        .find(|episode| episode.id == episode_id && episode.max_daily_clear > 0)
        .ok_or(StateError::InvalidRequest)?;
    let mut state = message_list(resources, "episode_states")
        .into_iter()
        .find(|state| i32_field(state, "episode_id") == Some(episode_id))
        .unwrap_or(empty_message(proto, "blend.model.EpisodeState")?);
    state.set_field_by_name("episode_id", Value::I32(episode_id));
    if message_i64_field(&state, "daily_updated_at", "seconds")
        .is_none_or(|time| day(time) != day(now))
    {
        state.set_field_by_name("daily_clear_count", Value::I32(0));
        state.set_field_by_name("daily_clear_addition_count", Value::I32(0));
    }
    let start = i32_field(&state, "daily_clear_addition_count")
        .unwrap_or(0)
        .max(0) as usize;
    let end = start
        .checked_add(count as usize)
        .filter(|end| *end <= episode.max_daily_clear_addition_gem_costs.len())
        .ok_or(StateError::InvalidRequest)?;
    let price = episode.max_daily_clear_addition_gem_costs[start..end]
        .iter()
        .try_fold(0i32, |sum, value| sum.checked_add(*value))
        .ok_or(StateError::InvalidRequest)?;
    if let Some((field, value)) = pay_resource_cost(
        proto,
        resources,
        Some(&TutorialRecipeCost {
            resource_type: 1,
            id: 0,
            quantity: price,
        }),
    )? {
        changed.set_field_by_name(field, Value::Message(value));
    }
    state.set_field_by_name("daily_clear_addition_count", Value::I32(end as i32));
    state.set_field_by_name("daily_updated_at", Value::Message(timestamp(proto, now)?));
    put(resources, "episode_states", "episode_id", state.clone());
    put(changed, "episode_states", "episode_id", state);
    Ok(())
}

pub(crate) fn rental_member_put(resources: &mut DynamicMessage, value: DynamicMessage) {
    let fixed_party_id = i32_field(&value, "fixed_party_id");
    let character_id = i32_field(&value, "character_id");
    let mut members = message_list(resources, "rental_party_members");
    if let Some(old) = members.iter_mut().find(|member| {
        i32_field(member, "fixed_party_id") == fixed_party_id
            && i32_field(member, "character_id") == character_id
    }) {
        *old = value;
    } else {
        members.push(value);
    }
    resources.set_field_by_name(
        "rental_party_members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );
}

pub(crate) fn rental_fixed_party(
    rules: &RewardRules,
    fixed_party_id: i32,
) -> Result<&RewardFixedParty, StateError> {
    rules
        .fixed_parties
        .iter()
        .find(|party| party.id == fixed_party_id)
        .ok_or(StateError::InvalidRequest)
}

pub(crate) fn validate_rental_equipment(
    characters: &CharacterRules,
    resources: &DynamicMessage,
    value: &DynamicMessage,
) -> Result<(), StateError> {
    for (field, slot) in [
        ("slot1_equipment_tool_entity_id", 1),
        ("slot2_equipment_tool_entity_id", 2),
        ("slot3_equipment_tool_entity_id", 3),
    ] {
        if optional_i32_field(value, field)
            .is_some_and(|entity| !character::valid_equipment(characters, resources, entity, slot))
        {
            return Err(StateError::InvalidRequest);
        }
    }
    Ok(())
}

pub(crate) fn rental_member(
    proto: &ProtoRegistry,
    fixed_party_id: i32,
    character_id: i32,
    input: &DynamicMessage,
) -> Result<DynamicMessage, StateError> {
    let mut member = empty_message(proto, "blend.model.RentalPartyMember")?;
    member.set_field_by_name("party_type", Value::I32(1));
    member.set_field_by_name("fixed_party_id", Value::I32(fixed_party_id));
    member.set_field_by_name("character_id", Value::I32(character_id));
    for field in [
        "slot1_equipment_tool_entity_id",
        "slot2_equipment_tool_entity_id",
        "slot3_equipment_tool_entity_id",
    ] {
        if input.has_field_by_name(field) {
            member.set_field_by_name(field, input.get_field_by_name(field).unwrap().into_owned());
        }
    }
    Ok(member)
}

pub(crate) fn rental_tools(
    proto: &ProtoRegistry,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    fixed_party_id: i32,
    entity_ids: Vec<i32>,
) -> Result<(), StateError> {
    if entity_ids.iter().copied().collect::<BTreeSet<_>>().len() != entity_ids.len()
        || entity_ids.len()
            > i32_field(&status_message(resources)?, "party_max_battle_tool_count")
                .unwrap_or(0)
                .max(0) as usize
        || entity_ids.iter().any(|id| {
            !message_list(resources, "battle_tools")
                .iter()
                .any(|tool| i32_field(tool, "entity_id") == Some(*id))
        })
    {
        return Err(StateError::InvalidRequest);
    }
    let mut party = empty_message(proto, "blend.model.RentalParty")?;
    party.set_field_by_name("party_type", Value::I32(1));
    party.set_field_by_name("fixed_party_id", Value::I32(fixed_party_id));
    party.set_field_by_name(
        "battle_tool_entity_ids",
        Value::List(entity_ids.into_iter().map(Value::I32).collect()),
    );
    put(resources, "rental_parties", "fixed_party_id", party.clone());
    put(changed, "rental_parties", "fixed_party_id", party);
    Ok(())
}

pub(crate) fn rental_party_update(
    proto: &ProtoRegistry,
    reward_rules: &RewardRules,
    characters: &CharacterRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    route: &str,
    request: &DynamicMessage,
) -> Result<(), StateError> {
    let fixed_party_id = i32_field(request, "fixed_party_id").ok_or(StateError::InvalidRequest)?;
    let fixed = rental_fixed_party(reward_rules, fixed_party_id)?;
    if route == "/rental_party/battle_tools_set" || route == "/rental_party/bulk_update" {
        rental_tools(
            proto,
            resources,
            changed,
            fixed_party_id,
            i32_list(request, "battle_tool_entity_ids"),
        )?;
    }
    let inputs = if route == "/rental_party/bulk_update" {
        message_list(request, "members")
    } else if route == "/rental_party/character_equip"
        || route == "/rental_party/character_bulk_equip"
    {
        vec![request.clone()]
    } else {
        return Ok(());
    };
    let mut seen = BTreeSet::new();
    let mut outputs = Vec::new();
    for input in inputs {
        let character_id = optional_i32_field(&input, "character_id")
            .or_else(|| i32_field(&input, "character_id"))
            .filter(|id| fixed.character_ids.contains(id) && seen.insert(*id))
            .ok_or(StateError::InvalidRequest)?;
        validate_rental_equipment(characters, resources, &input)?;
        let mut member = message_list(resources, "rental_party_members")
            .into_iter()
            .find(|member| {
                i32_field(member, "fixed_party_id") == Some(fixed_party_id)
                    && i32_field(member, "character_id") == Some(character_id)
            })
            .unwrap_or(rental_member(proto, fixed_party_id, character_id, &input)?);
        if route == "/rental_party/character_equip" {
            let slot = i32_field(request, "slot_type")
                .filter(|slot| (1..=3).contains(slot))
                .ok_or(StateError::InvalidRequest)?;
            let field = format!("slot{slot}_equipment_tool_entity_id");
            if request.has_field_by_name("equipment_tool_entity_id") {
                member.set_field_by_name(
                    &field,
                    request
                        .get_field_by_name("equipment_tool_entity_id")
                        .unwrap()
                        .into_owned(),
                );
            } else {
                member.clear_field_by_name(&field);
            }
        } else {
            member = rental_member(proto, fixed_party_id, character_id, &input)?;
        }
        validate_rental_equipment(characters, resources, &member)?;
        outputs.push(member);
    }
    if route == "/rental_party/bulk_update" && seen.len() != fixed.character_ids.len() {
        return Err(StateError::InvalidRequest);
    }
    if route == "/rental_party/bulk_update" {
        let mut members = message_list(resources, "rental_party_members");
        members.retain(|member| i32_field(member, "fixed_party_id") != Some(fixed_party_id));
        resources.set_field_by_name(
            "rental_party_members",
            Value::List(members.into_iter().map(Value::Message).collect()),
        );
    }
    for member in outputs {
        rental_member_put(resources, member.clone());
        rental_member_put(changed, member);
    }
    Ok(())
}

pub(crate) fn copy_traits(target: &mut DynamicMessage, source: &DynamicMessage) {
    target.set_field_by_name(
        "traits",
        source
            .get_field_by_name("traits")
            .map(|value| value.into_owned())
            .unwrap_or(Value::List(Vec::new())),
    );
}

pub(crate) fn cleared_party_list(
    proto: &ProtoRegistry,
    reward_rules: &RewardRules,
    resources: &DynamicMessage,
    request: &DynamicMessage,
    response: &mut DynamicMessage,
    now: i64,
) -> Result<(), StateError> {
    let quest_id = i32_field(request, "quest_id").ok_or(StateError::InvalidRequest)?;
    let quest = reward_rules
        .quests
        .iter()
        .find(|quest| quest.id == quest_id)
        .ok_or(StateError::InvalidRequest)?;
    if !quest.send_cleared_party
        || !message_list(resources, "quest_states").iter().any(|state| {
            i32_field(state, "quest_id") == Some(quest_id)
                && i32_field(state, "clear_count").unwrap_or(0) > 0
        })
    {
        response.set_field_by_name("cleared_parties", Value::List(Vec::new()));
        return Ok(());
    }
    let party = message_list(resources, "parties")
        .into_iter()
        .find(|party| {
            i32_field(party, "party_type") == Some(1) && i32_field(party, "number") == Some(1)
        })
        .ok_or(StateError::InvalidRequest)?;
    let mut members = Vec::new();
    for party_member in message_list(resources, "party_members")
        .into_iter()
        .filter(|member| {
            i32_field(member, "party_type") == Some(1) && i32_field(member, "number") == Some(1)
        })
    {
        let Some(character_id) = optional_i32_field(&party_member, "character_id") else {
            continue;
        };
        let character = message_list(resources, "characters")
            .into_iter()
            .find(|character| i32_field(character, "character_id") == Some(character_id))
            .ok_or(StateError::InvalidRequest)?;
        let mut summary = empty_message(proto, "blend.model.QuestClearedPartyMember")?;
        let mut character_summary = empty_message(proto, "blend.model.QuestClearedPartyCharacter")?;
        for field in [
            "rarity",
            "exp",
            "growboard_current_page",
            "growboard_ex_current_page",
            "growboard_neo_current_page",
        ] {
            character_summary
                .set_field_by_name(field, Value::I32(i32_field(&character, field).unwrap_or(0)));
        }
        character_summary.set_field_by_name("id", Value::I32(character_id));
        summary.set_field_by_name("character", Value::Message(character_summary));
        let mut equipment = Vec::new();
        for field in [
            "slot1_equipment_tool_entity_id",
            "slot2_equipment_tool_entity_id",
            "slot3_equipment_tool_entity_id",
        ] {
            let entity = optional_i32_field(&party_member, field)
                .or_else(|| optional_i32_field(&character, field));
            if let Some(tool) = entity.and_then(|id| {
                message_list(resources, "equipment_tools")
                    .into_iter()
                    .find(|tool| i32_field(tool, "entity_id") == Some(id))
            }) {
                let mut value = empty_message(proto, "blend.model.QuestClearedPartyEquipmentTool")?;
                value.set_field_by_name(
                    "id",
                    Value::I32(i32_field(&tool, "tool_id").ok_or(StateError::InvalidRequest)?),
                );
                copy_traits(&mut value, &tool);
                equipment.push(Value::Message(value));
            }
        }
        summary.set_field_by_name("equipment_tools", Value::List(equipment));
        let memoria_entity = optional_i32_field(&party_member, "memoria_entity_id")
            .or_else(|| optional_i32_field(&character, "memoria_entity_id"));
        if let Some(memoria) = memoria_entity.and_then(|id| {
            message_list(resources, "memorias")
                .into_iter()
                .find(|memoria| i32_field(memoria, "entity_id") == Some(id))
        }) {
            let mut value = empty_message(proto, "blend.model.QuestClearedPartyMemoria")?;
            value.set_field_by_name(
                "id",
                Value::I32(i32_field(&memoria, "memoria_id").ok_or(StateError::InvalidRequest)?),
            );
            value.set_field_by_name(
                "limit_break",
                Value::I32(i32_field(&memoria, "limit_break").unwrap_or(0)),
            );
            value.set_field_by_name("exp", Value::I32(i32_field(&memoria, "exp").unwrap_or(0)));
            summary.set_field_by_name("memoria", Value::Message(value));
        }
        members.push(Value::Message(summary));
    }
    let mut tools = Vec::new();
    for entity in i32_list(&party, "battle_tool_entity_ids") {
        let tool = message_list(resources, "battle_tools")
            .into_iter()
            .find(|tool| i32_field(tool, "entity_id") == Some(entity))
            .ok_or(StateError::InvalidRequest)?;
        let mut value = empty_message(proto, "blend.model.QuestClearedPartyBattleTool")?;
        value.set_field_by_name(
            "id",
            Value::I32(i32_field(&tool, "tool_id").ok_or(StateError::InvalidRequest)?),
        );
        copy_traits(&mut value, &tool);
        tools.push(Value::Message(value));
    }
    let mut result = empty_message(proto, "blend.model.QuestClearedParty")?;
    result.set_field_by_name("quest_id", Value::I32(quest_id));
    if let Some(rank) = message_list(resources, "quest_states")
        .iter()
        .find(|state| i32_field(state, "quest_id") == Some(quest_id))
        .and_then(|state| i32_field(state, "score_rank"))
        .filter(|rank| *rank > 0)
    {
        result.set_field_by_name("score_rank", Value::Message(int32_value(proto, rank)?));
    }
    result.set_field_by_name("party_members", Value::List(members));
    result.set_field_by_name("battle_tools", Value::List(tools));
    result.set_field_by_name("calculated_at", Value::Message(timestamp(proto, now)?));
    result.set_field_by_name(
        "leader_position",
        Value::I32(i32_field(&party, "leader_position").unwrap_or(1)),
    );
    response.set_field_by_name("cleared_parties", Value::List(vec![Value::Message(result)]));
    Ok(())
}
