use super::prelude::*;

#[allow(clippy::too_many_arguments)]
pub(crate) fn collection(
    proto: &ProtoRegistry,
    rules: &AtelierRules,
    home_rules: &home::HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    response: &mut DynamicMessage,
    home: &mut home::HomeState,
    request: &DynamicMessage,
    now: i64,
) -> Result<(), StateError> {
    let id = i32_field(request, "illustrated_book_reward_id").ok_or(StateError::InvalidRequest)?;
    rules
        .illustrated_books
        .iter()
        .find(|row| row.id == id && in_period(row.start_at, row.end_at, now))
        .ok_or(StateError::OutOfSchedule)?;
    for memoria in message_list(resources, "memorias") {
        if let Some(value) = i32_field(&memoria, "memoria_id") {
            home.memoria_history.insert(value);
        }
    }
    let valuable = message_list(resources, "items")
        .into_iter()
        .filter(|row| {
            i32_field(row, "total_quantity").unwrap_or(0) > 0
                && i32_field(row, "item_id")
                    .is_some_and(|value| rules.valuable_item_ids.contains(&value))
        })
        .filter_map(|row| i32_field(&row, "item_id"))
        .collect::<Vec<_>>();
    let mut awarded = Vec::new();
    for (reward_type, actual, milestones) in [
        (
            1,
            home.memoria_history.len() as i32,
            &rules.collection_memoria,
        ),
        (2, valuable.len() as i32, &rules.collection_valuable_items),
    ] {
        let previous = message_list(resources, "illustrated_book_reward_states")
            .iter()
            .find(|row| {
                i32_field(row, "illustrated_book_reward_id") == Some(id)
                    && i32_field(row, "reward_type") == Some(reward_type)
            })
            .and_then(|row| i32_field(row, "count"))
            .unwrap_or(0);
        let rewards = milestones
            .iter()
            .filter(|row| {
                row.illustrated_book_reward_id == id
                    && row.reward_type == reward_type
                    && previous < row.count
                    && row.count <= actual
            })
            .flat_map(|row| row.reward_set_ids.iter())
            .map(|reward_id| {
                rules
                    .collection_reward_sets
                    .get(reward_id)
                    .ok_or(StateError::InvalidRequest)
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flat_map(|rewards| rewards.iter().cloned())
            .collect::<Vec<_>>();
        awarded.extend(home::grant(
            proto, home_rules, resources, changed, &rewards, now,
        )?);
        let mut state = empty_message(proto, "blend.model.IllustratedBookRewardState")?;
        state.set_field_by_name("illustrated_book_reward_id", Value::I32(id));
        state.set_field_by_name("reward_type", Value::I32(reward_type));
        state.set_field_by_name("count", Value::I32(actual));
        state.set_field_by_name("is_new", Value::Bool(actual > previous));
        for target in [&mut *resources, &mut *changed] {
            let mut states = message_list(target, "illustrated_book_reward_states");
            if let Some(old) = states.iter_mut().find(|old| {
                i32_field(old, "illustrated_book_reward_id") == Some(id)
                    && i32_field(old, "reward_type") == Some(reward_type)
            }) {
                *old = state.clone();
            } else {
                states.push(state.clone());
            }
            set_messages(target, "illustrated_book_reward_states", states);
        }
    }
    response.set_field_by_name("rewards", Value::List(awarded));
    response.set_field_by_name(
        "memoria_ids",
        Value::List(
            home.memoria_history
                .iter()
                .copied()
                .map(Value::I32)
                .collect(),
        ),
    );
    response.set_field_by_name(
        "valuable_item_ids",
        Value::List(valuable.into_iter().map(Value::I32).collect()),
    );
    Ok(())
}

pub(crate) fn tool_rule(
    rules: &AtelierRules,
    resource_type: i32,
    tool_id: i32,
) -> Result<&ToolRule, StateError> {
    match resource_type {
        6 => &rules.equipment_tools,
        14 => &rules.battle_tools,
        _ => return Err(StateError::InvalidRequest),
    }
    .iter()
    .find(|row| row.id == tool_id)
    .ok_or(StateError::InvalidRequest)
}

pub(crate) fn tool_rarity(
    rules: &AtelierRules,
    resource_type: i32,
    tool_id: i32,
) -> Result<i32, StateError> {
    Ok(tool_rule(rules, resource_type, tool_id)?.rarity)
}

pub(crate) fn tool_field(resource_type: i32) -> Result<&'static str, StateError> {
    match resource_type {
        6 => Ok("equipment_tools"),
        14 => Ok("battle_tools"),
        _ => Err(StateError::InvalidRequest),
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn tool_convert(
    proto: &ProtoRegistry,
    rules: &AtelierRules,
    home_rules: &home::HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    response: &mut DynamicMessage,
    request: &DynamicMessage,
    now: i64,
) -> Result<(), StateError> {
    let tools = message_list(request, "consumed_tools");
    if tools.is_empty()
        || tools.len() > usize::try_from(rules.constants.tool_conversion_limit_count).unwrap_or(0)
    {
        return Err(StateError::InvalidRequest);
    }
    let mut keys = Vec::new();
    let mut rewards = Vec::new();
    for input in tools {
        let resource_type = i32_field(&input, "type").ok_or(StateError::InvalidRequest)?;
        let entity_id = i32_field(&input, "entity_id").ok_or(StateError::InvalidRequest)?;
        if entity_id <= 0
            || keys.contains(&(resource_type, entity_id))
            || assigned_tool(resources, resource_type, entity_id)
        {
            return Err(StateError::InvalidRequest);
        }
        keys.push((resource_type, entity_id));
        let tool = message_list(resources, tool_field(resource_type)?)
            .into_iter()
            .find(|row| i32_field(row, "entity_id") == Some(entity_id))
            .filter(|row| !bool_field(row, "is_locked"))
            .ok_or(StateError::InvalidRequest)?;
        let rule = tool_rule(
            rules,
            resource_type,
            i32_field(&tool, "tool_id").ok_or(StateError::InvalidRequest)?,
        )?;
        let rarity = rules
            .tool_rarities
            .iter()
            .find(|row| row.id == rule.rarity)
            .filter(|row| !row.disable_conversion)
            .ok_or(StateError::InvalidRequest)?;
        rewards.extend(if resource_type == 6 {
            rarity.equipment_tool_conversion_rewards.clone()
        } else {
            rarity.battle_tool_conversion_rewards.clone()
        });
        let total = message_list(&tool, "traits")
            .iter()
            .try_fold(0i32, |sum, row| {
                sum.checked_add(i32_field(row, "rank").ok_or(StateError::InvalidRequest)?)
                    .ok_or(StateError::InvalidRequest)
            })?;
        // Master row 1 represents a total rank of zero, as in the client's tool calculations.
        if let Some(row) = rules
            .trait_rank_totals
            .iter()
            .find(|row| row.id == total + 1)
        {
            rewards.extend(if resource_type == 6 {
                row.equipment_tool_conversion_rewards.clone()
            } else {
                row.battle_tool_conversion_rewards.clone()
            });
        }
    }
    let rewards = aggregate_rewards(rewards)?;
    let reward_messages = home::grant(proto, home_rules, resources, changed, &rewards, now)?;
    let mut equipment = Vec::new();
    let mut battle = Vec::new();
    for (resource_type, entity_id) in keys {
        remove_entity(resources, tool_field(resource_type)?, entity_id)?;
        if resource_type == 6 {
            equipment.push(entity_id);
        } else {
            battle.push(entity_id);
        }
    }
    response.set_field_by_name(
        "deleted_resources",
        Value::Message(deleted_resources(proto, &equipment, &battle, &[])?),
    );
    response.set_field_by_name("rewards", Value::List(reward_messages));
    response.set_field_by_name(
        "tool_conversion_limit_count",
        Value::I32(rules.constants.tool_conversion_limit_count),
    );
    Ok(())
}

pub(crate) fn tool_update(
    proto: &ProtoRegistry,
    rules: &AtelierRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    route: &str,
    request: &DynamicMessage,
) -> Result<(), StateError> {
    let input = request_message(request, "tool")?;
    let resource_type = i32_field(&input, "type").ok_or(StateError::InvalidRequest)?;
    let entity_id = i32_field(&input, "entity_id").ok_or(StateError::InvalidRequest)?;
    let field = tool_field(resource_type)?;
    let mut tool = message_list(resources, field)
        .into_iter()
        .find(|row| i32_field(row, "entity_id") == Some(entity_id))
        .ok_or(StateError::InvalidRequest)?;
    if route == "/tool/lock" {
        tool.set_field_by_name("is_locked", Value::Bool(bool_field(request, "is_locked")));
    } else {
        let index =
            usize::try_from(i32_field(request, "trait_index").ok_or(StateError::InvalidRequest)?)
                .map_err(|_| StateError::InvalidRequest)?;
        let mut traits = message_list(&tool, "traits");
        let trait_value = traits.get_mut(index).ok_or(StateError::InvalidRequest)?;
        let rank = i32_field(trait_value, "rank").ok_or(StateError::InvalidRequest)?;
        let current = rules
            .trait_ranks
            .iter()
            .find(|row| row.id == rank)
            .ok_or(StateError::InvalidRequest)?;
        let next = rules
            .trait_ranks
            .iter()
            .find(|row| row.id == rank + 1)
            .ok_or(StateError::InvalidRequest)?;
        character::spend(
            proto,
            resources,
            changed,
            std::slice::from_ref(&current.rank_up_cost),
        )?;
        trait_value.set_field_by_name("rank", Value::I32(next.id));
        set_messages(&mut tool, "traits", traits);
    }
    home::put(resources, field, "entity_id", tool.clone());
    home::put(changed, field, "entity_id", tool);
    Ok(())
}
