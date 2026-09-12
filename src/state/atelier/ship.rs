use super::prelude::*;

pub(crate) fn ship_level(
    rules: &AtelierRules,
    exp: i32,
    rank: i32,
) -> Result<&ShipLevelRule, StateError> {
    rules
        .ship_levels
        .iter()
        .filter(|row| row.rank <= rank && row.exp <= exp)
        .max_by_key(|row| row.exp)
        .ok_or(StateError::InvalidRequest)
}

pub(crate) fn ship_create(
    proto: &ProtoRegistry,
    rules: &AtelierRules,
    synthesis: &SynthesisRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    request: &DynamicMessage,
    now: i64,
) -> Result<(), StateError> {
    let part_id = i32_field(request, "ship_part_id").ok_or(StateError::InvalidRequest)?;
    let count = i32_field(request, "count")
        .filter(|value| *value > 0)
        .ok_or(StateError::InvalidRequest)?;
    let part = rules
        .ship_parts
        .iter()
        .find(|row| row.id == part_id && in_period(row.start_at, None, now))
        .ok_or(StateError::OutOfSchedule)?;
    let costs = part
        .costs
        .iter()
        .map(|cost| {
            Ok(RuleCost {
                id: cost.id,
                resource_type: cost.resource_type,
                quantity: cost
                    .quantity
                    .checked_mul(count)
                    .ok_or(StateError::InvalidRequest)?,
            })
        })
        .collect::<Result<Vec<_>, StateError>>()?;
    if part.enhance_target == 1 {
        if !rules.ships.contains(&part.target_id) {
            return Err(StateError::InvalidRequest);
        }
        let mut ship = message_list(resources, "ships")
            .into_iter()
            .find(|row| i32_field(row, "ship_id") == Some(part.target_id))
            .unwrap_or(empty_message(proto, "blend.model.Ship")?);
        ship.set_field_by_name("ship_id", Value::I32(part.target_id));
        let exp = i32_field(&ship, "exp").unwrap_or(0);
        let rank = i32_field(&ship, "rank")
            .filter(|value| *value > 0)
            .unwrap_or(1);
        ship.set_field_by_name("rank", Value::I32(rank));
        let max_rank = rules
            .ship_levels
            .iter()
            .map(|row| row.rank)
            .max()
            .ok_or(StateError::InvalidRequest)?;
        let cap = rules
            .ship_levels
            .iter()
            .filter(|row| row.rank <= rank)
            .map(|row| row.exp)
            .max()
            .ok_or(StateError::InvalidRequest)?;
        if part.enhance_type == 1 {
            let next = exp
                .checked_add(
                    part.value
                        .checked_mul(count)
                        .ok_or(StateError::InvalidRequest)?,
                )
                .ok_or(StateError::InvalidRequest)?
                .min(cap);
            if next == exp {
                return Err(StateError::InvalidRequest);
            }
            ship.set_field_by_name("exp", Value::I32(next));
        } else {
            let next = rank
                .checked_add(
                    part.value
                        .checked_mul(count)
                        .ok_or(StateError::InvalidRequest)?,
                )
                .ok_or(StateError::InvalidRequest)?;
            if exp < cap || next > max_rank {
                return Err(StateError::InvalidRequest);
            }
            ship.set_field_by_name("rank", Value::I32(next));
        }
        if i32_field(&ship, "party_number").unwrap_or(0) == 0 {
            ship.set_field_by_name("party_number", Value::I32(1));
        }
        spend_ship_costs(proto, synthesis, resources, changed, &costs, now)?;
        home::put(resources, "ships", "ship_id", ship.clone());
        home::put(changed, "ships", "ship_id", ship);
    } else if part.enhance_target == 2 {
        if !rules.ship_tools.contains(&part.target_id) {
            return Err(StateError::InvalidRequest);
        }
        let mut tools = message_list(resources, "ship_tools");
        let matches = tools
            .iter()
            .enumerate()
            .filter(|(_, row)| i32_field(row, "tool_id") == Some(part.target_id))
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err(StateError::InvalidRequest);
        }
        let mut tool = tools[matches[0]].clone();
        let exp = i32_field(&tool, "exp").unwrap_or(0);
        let rank = i32_field(&tool, "rank")
            .filter(|value| *value > 0)
            .unwrap_or(1);
        let max_rank = rules
            .ship_tool_levels
            .iter()
            .map(|row| row.rank)
            .max()
            .ok_or(StateError::InvalidRequest)?;
        let cap = rules
            .ship_tool_levels
            .iter()
            .filter(|row| row.rank <= rank)
            .map(|row| row.exp)
            .max()
            .ok_or(StateError::InvalidRequest)?;
        if part.enhance_type == 1 {
            let next = exp
                .checked_add(
                    part.value
                        .checked_mul(count)
                        .ok_or(StateError::InvalidRequest)?,
                )
                .ok_or(StateError::InvalidRequest)?
                .min(cap);
            if next == exp {
                return Err(StateError::InvalidRequest);
            }
            tool.set_field_by_name("exp", Value::I32(next));
        } else {
            let next = rank
                .checked_add(
                    part.value
                        .checked_mul(count)
                        .ok_or(StateError::InvalidRequest)?,
                )
                .ok_or(StateError::InvalidRequest)?;
            if exp < cap || next > max_rank {
                return Err(StateError::InvalidRequest);
            }
            tool.set_field_by_name("rank", Value::I32(next));
        }
        spend_ship_costs(proto, synthesis, resources, changed, &costs, now)?;
        tools[matches[0]] = tool.clone();
        set_messages(resources, "ship_tools", tools);
        home::put(changed, "ship_tools", "entity_id", tool);
    } else {
        return Err(StateError::InvalidRequest);
    }
    Ok(())
}

pub(crate) fn spend_ship_costs(
    proto: &ProtoRegistry,
    synthesis: &SynthesisRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    costs: &[RuleCost],
    now: i64,
) -> Result<(), StateError> {
    let mana = costs
        .iter()
        .filter(|cost| cost.resource_type == 9)
        .try_fold(0i32, |total, cost| {
            total
                .checked_add(cost.quantity)
                .ok_or(StateError::InvalidRequest)
        })?;
    character::spend(
        proto,
        resources,
        changed,
        &costs
            .iter()
            .filter(|cost| cost.resource_type != 9)
            .cloned()
            .collect::<Vec<_>>(),
    )?;
    if mana > 0 {
        let status = change_mana(proto, synthesis, resources, -mana, now)?;
        changed.set_field_by_name("status", Value::Message(status));
    }
    Ok(())
}

pub(crate) fn ship_party(
    proto: &ProtoRegistry,
    rules: &AtelierRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    route: &str,
    request: &DynamicMessage,
) -> Result<(), StateError> {
    let ship_id = i32_field(request, "ship_id").ok_or(StateError::InvalidRequest)?;
    let mut ship = message_list(resources, "ships")
        .into_iter()
        .find(|row| i32_field(row, "ship_id") == Some(ship_id))
        .ok_or(StateError::InvalidRequest)?;
    let rank = i32_field(&ship, "rank")
        .filter(|value| *value > 0)
        .unwrap_or(1);
    let level = ship_level(rules, i32_field(&ship, "exp").unwrap_or(0), rank)?;
    let number = if route == "/ship/bulk_update" {
        i32_field(request, "number")
            .filter(|value| (1..=rules.constants.party_count_per_content).contains(value))
            .ok_or(StateError::InvalidRequest)?
    } else {
        i32_field(&ship, "party_number")
            .filter(|value| *value > 0)
            .ok_or(StateError::InvalidRequest)?
    };
    let mut party = message_list(resources, "ship_parties")
        .into_iter()
        .find(|row| i32_field(row, "number") == Some(number))
        .unwrap_or(empty_message(proto, "blend.model.ShipParty")?);
    party.set_field_by_name("number", Value::I32(number));
    if route == "/ship/bulk_update" {
        let characters = i32_list(request, "character_ids");
        let main = optional_i32_field(request, "main_memoria_entity_id");
        let sub = i32_list(request, "sub_memoria_entity_ids");
        let mut memoria = sub.clone();
        if let Some(value) = main {
            memoria.push(value);
        }
        if characters.len() > usize::try_from(level.max_character_count).unwrap_or(0)
            || sub.len() > usize::try_from(level.max_sub_memoria_count).unwrap_or(0)
            || !unique(&characters)
            || !unique(&memoria)
            || characters
                .iter()
                .any(|id| resource_character(resources, *id).is_err())
            || memoria.iter().any(|id| !owned(resources, "memorias", *id))
        {
            return Err(StateError::InvalidRequest);
        }
        party.set_field_by_name(
            "character_ids",
            Value::List(characters.into_iter().map(Value::I32).collect()),
        );
        if let Some(value) = main {
            party.set_field_by_name(
                "main_memoria_entity_id",
                Value::Message(int32_value(proto, value)?),
            );
        } else {
            party.clear_field_by_name("main_memoria_entity_id");
        }
        party.set_field_by_name(
            "sub_memoria_entity_ids",
            Value::List(sub.into_iter().map(Value::I32).collect()),
        );
        ship.set_field_by_name("party_number", Value::I32(number));
        home::put(resources, "ships", "ship_id", ship.clone());
        home::put(changed, "ships", "ship_id", ship);
    } else {
        let tools = i32_list(request, "ship_tool_entity_ids");
        if tools.len() > usize::try_from(level.max_tool_count).unwrap_or(0)
            || !unique(&tools)
            || tools.iter().any(|id| !owned(resources, "ship_tools", *id))
        {
            return Err(StateError::InvalidRequest);
        }
        party.set_field_by_name(
            "ship_tool_entity_ids",
            Value::List(tools.into_iter().map(Value::I32).collect()),
        );
    }
    home::put(resources, "ship_parties", "number", party.clone());
    home::put(changed, "ship_parties", "number", party);
    Ok(())
}

pub(crate) fn ship_synthesize(
    proto: &ProtoRegistry,
    synthesis: &SynthesisRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    response: &mut DynamicMessage,
    request: &DynamicMessage,
    now: i64,
) -> Result<(), StateError> {
    let recipe_id = i32_field(request, "recipe_id").ok_or(StateError::InvalidRequest)?;
    let recipe = synthesis
        .recipes
        .iter()
        .find(|row| row.id == recipe_id && row.target.resource_type == 25)
        .ok_or(StateError::InvalidRequest)?;
    if !recipe_present(resources, recipe_id)
        || recipe.target.quantity <= 0
        || message_list(resources, "ship_tools")
            .iter()
            .any(|tool| i32_field(tool, "tool_id") == Some(recipe.target.id))
    {
        return Err(StateError::InvalidRequest);
    }
    let costs = recipe
        .costs
        .iter()
        .map(|cost| RuleCost {
            id: cost.id,
            quantity: cost.quantity,
            resource_type: cost.resource_type,
        })
        .collect::<Vec<_>>();
    character::spend(proto, resources, changed, &costs)?;
    let status = change_mana(proto, synthesis, resources, -recipe.mana_cost, now)?;
    changed.set_field_by_name("status", Value::Message(status));
    let mut rewards = Vec::new();
    for _ in 0..recipe.target.quantity {
        let output = add_synthesis_output(proto, resources, &recipe.target, &[], now)?;
        home::put(changed, "ship_tools", "entity_id", output.clone());
        let mut reward = reward_message(proto, &recipe.target, false)?;
        reward.set_field_by_name("quantity", Value::I32(1));
        reward.set_field_by_name(
            "entity_id",
            Value::I32(i32_field(&output, "entity_id").ok_or(StateError::InvalidRequest)?),
        );
        rewards.push(Value::Message(reward));
    }
    response.set_field_by_name("rewards", Value::List(rewards));
    Ok(())
}
