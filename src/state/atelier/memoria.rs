use super::prelude::*;

pub(crate) fn memoria_rule<'a>(
    rules: &'a AtelierRules,
    memoria: &DynamicMessage,
) -> Result<(&'a MemoriaRule, &'a MemoriaRarityRule), StateError> {
    let row = rules
        .memorias
        .iter()
        .find(|row| row.id == i32_field(memoria, "memoria_id").unwrap_or_default())
        .ok_or(StateError::InvalidRequest)?;
    let rarity = rules
        .memoria_rarities
        .iter()
        .find(|rarity| rarity.id == row.rarity)
        .ok_or(StateError::InvalidRequest)?;
    Ok((row, rarity))
}

pub(crate) fn consume_items(
    proto: &ProtoRegistry,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    request: &DynamicMessage,
    rules: &[ValueRule],
) -> Result<i32, StateError> {
    let mut quantities = BTreeMap::<i32, i32>::new();
    for input in message_list(request, "consumed_items") {
        let id = i32_field(&input, "item_id").ok_or(StateError::InvalidRequest)?;
        let quantity = i32_field(&input, "quantity")
            .filter(|value| *value > 0)
            .ok_or(StateError::InvalidRequest)?;
        let value = quantities.entry(id).or_default();
        *value = value
            .checked_add(quantity)
            .ok_or(StateError::InvalidRequest)?;
    }
    let mut total = 0i32;
    for (id, quantity) in quantities {
        let row = rules
            .iter()
            .find(|row| row.id == id)
            .ok_or(StateError::InvalidRequest)?;
        total = total
            .checked_add(
                row.value
                    .checked_mul(quantity)
                    .ok_or(StateError::InvalidRequest)?,
            )
            .ok_or(StateError::InvalidRequest)?;
        let item = change_item(proto, resources, id, -quantity)?;
        home::put(changed, "items", "item_id", item);
    }
    Ok(total)
}

pub(crate) fn memoria_enhance(
    proto: &ProtoRegistry,
    rules: &AtelierRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    response: &mut DynamicMessage,
    request: &DynamicMessage,
) -> Result<(), StateError> {
    let target_id = i32_field(request, "memoria_entity_id").ok_or(StateError::InvalidRequest)?;
    let mut target = message_list(resources, "memorias")
        .into_iter()
        .find(|row| i32_field(row, "entity_id") == Some(target_id))
        .ok_or(StateError::InvalidRequest)?;
    let (_, target_rarity) = memoria_rule(rules, &target)?;
    let consumed = i32_list(request, "consumed_memoria_entity_ids");
    if !unique(&consumed)
        || consumed.contains(&target_id)
        || consumed.iter().any(|id| assigned_memoria(resources, *id))
    {
        return Err(StateError::InvalidRequest);
    }
    let mut gained = consume_items(proto, resources, changed, request, &rules.memoria_exp_items)?;
    for entity_id in &consumed {
        let memoria = message_list(resources, "memorias")
            .into_iter()
            .find(|row| i32_field(row, "entity_id") == Some(*entity_id))
            .filter(|row| !bool_field(row, "is_locked"))
            .ok_or(StateError::InvalidRequest)?;
        let (_, rarity) = memoria_rule(rules, &memoria)?;
        let returned = i32_field(&memoria, "exp")
            .unwrap_or(0)
            .checked_mul(rules.constants.memoria_exp_return_rate)
            .ok_or(StateError::InvalidRequest)?
            / 100;
        gained = gained
            .checked_add(rarity.exp)
            .and_then(|value| value.checked_add(returned))
            .ok_or(StateError::InvalidRequest)?;
    }
    if gained <= 0 {
        return Err(StateError::InvalidRequest);
    }
    let max_exp = rules
        .memoria_levels
        .iter()
        .filter(|row| row.level <= target_rarity.level_limit)
        .map(|row| row.exp)
        .max()
        .ok_or(StateError::InvalidRequest)?;
    let old_exp = i32_field(&target, "exp").unwrap_or(0);
    if old_exp >= max_exp {
        return Err(StateError::InvalidRequest);
    }
    let cost = gained
        .checked_mul(rules.constants.memoria_enhancement_cole_per_exp_rate)
        .ok_or(StateError::InvalidRequest)?
        / 100;
    character::spend(
        proto,
        resources,
        changed,
        &[RuleCost {
            id: 1,
            quantity: cost,
            resource_type: 3,
        }],
    )?;
    target.set_field_by_name(
        "exp",
        Value::I32(old_exp.saturating_add(gained).min(max_exp)),
    );
    home::put(resources, "memorias", "entity_id", target.clone());
    home::put(changed, "memorias", "entity_id", target);
    for entity_id in &consumed {
        remove_entity(resources, "memorias", *entity_id)?;
    }
    response.set_field_by_name(
        "deleted_resources",
        Value::Message(deleted_resources(proto, &[], &[], &consumed)?),
    );
    Ok(())
}

pub(crate) fn memoria_limit_break(
    proto: &ProtoRegistry,
    rules: &AtelierRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    response: &mut DynamicMessage,
    request: &DynamicMessage,
) -> Result<(), StateError> {
    let target_id = i32_field(request, "memoria_entity_id").ok_or(StateError::InvalidRequest)?;
    let mut target = message_list(resources, "memorias")
        .into_iter()
        .find(|row| i32_field(row, "entity_id") == Some(target_id))
        .ok_or(StateError::InvalidRequest)?;
    let (target_rule, rarity) = memoria_rule(rules, &target)?;
    let consumed = i32_list(request, "consumed_memoria_entity_ids");
    if !unique(&consumed)
        || consumed.contains(&target_id)
        || consumed.iter().any(|id| assigned_memoria(resources, *id))
    {
        return Err(StateError::InvalidRequest);
    }
    for entity_id in &consumed {
        let memoria = message_list(resources, "memorias")
            .into_iter()
            .find(|row| i32_field(row, "entity_id") == Some(*entity_id))
            .filter(|row| !bool_field(row, "is_locked"))
            .ok_or(StateError::InvalidRequest)?;
        if i32_field(&memoria, "memoria_id") != Some(target_rule.id) {
            return Err(StateError::InvalidRequest);
        }
    }
    let mut item_count = 0i32;
    let mut quantities = BTreeMap::<i32, i32>::new();
    for input in message_list(request, "consumed_items") {
        let id = i32_field(&input, "item_id").ok_or(StateError::InvalidRequest)?;
        let quantity = i32_field(&input, "quantity")
            .filter(|value| *value > 0)
            .ok_or(StateError::InvalidRequest)?;
        let row = rules
            .memoria_limit_break_items
            .iter()
            .find(|row| row.id == id && row.rarity == target_rule.rarity)
            .filter(|_| target_rule.item_limit_break_enabled)
            .ok_or(StateError::InvalidRequest)?;
        let total = quantities.entry(id).or_default();
        *total = total
            .checked_add(quantity)
            .ok_or(StateError::InvalidRequest)?;
        item_count = item_count
            .checked_add(
                row.value
                    .checked_mul(quantity)
                    .ok_or(StateError::InvalidRequest)?,
            )
            .ok_or(StateError::InvalidRequest)?;
    }
    let gained = i32::try_from(consumed.len())
        .map_err(|_| StateError::InvalidRequest)?
        .checked_add(item_count)
        .ok_or(StateError::InvalidRequest)?;
    let old = i32_field(&target, "limit_break").unwrap_or(0);
    if gained <= 0
        || old
            .checked_add(gained)
            .is_none_or(|value| value > rarity.max_limit_break)
    {
        return Err(StateError::InvalidRequest);
    }
    for (id, quantity) in quantities {
        let item = change_item(proto, resources, id, -quantity)?;
        home::put(changed, "items", "item_id", item);
    }
    for entity_id in &consumed {
        remove_entity(resources, "memorias", *entity_id)?;
    }
    target.set_field_by_name("limit_break", Value::I32(old + gained));
    home::put(resources, "memorias", "entity_id", target.clone());
    home::put(changed, "memorias", "entity_id", target);
    response.set_field_by_name(
        "deleted_resources",
        Value::Message(deleted_resources(proto, &[], &[], &consumed)?),
    );
    Ok(())
}

pub(crate) fn memoria_lock(
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    request: &DynamicMessage,
) -> Result<(), StateError> {
    let entity_id = i32_field(request, "memoria_entity_id").ok_or(StateError::InvalidRequest)?;
    let mut memoria = message_list(resources, "memorias")
        .into_iter()
        .find(|row| i32_field(row, "entity_id") == Some(entity_id))
        .ok_or(StateError::InvalidRequest)?;
    memoria.set_field_by_name("is_locked", Value::Bool(bool_field(request, "is_locked")));
    home::put(resources, "memorias", "entity_id", memoria.clone());
    home::put(changed, "memorias", "entity_id", memoria);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn memoria_sell(
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
    let ids = i32_list(request, "entity_ids");
    if ids.is_empty() || !unique(&ids) || ids.iter().any(|id| assigned_memoria(resources, *id)) {
        return Err(StateError::InvalidRequest);
    }
    let mut cole = 0i32;
    for entity_id in &ids {
        let memoria = message_list(resources, "memorias")
            .into_iter()
            .find(|row| i32_field(row, "entity_id") == Some(*entity_id))
            .filter(|row| !bool_field(row, "is_locked"))
            .ok_or(StateError::InvalidRequest)?;
        home.memoria_history
            .insert(i32_field(&memoria, "memoria_id").ok_or(StateError::InvalidRequest)?);
        cole = cole
            .checked_add(memoria_rule(rules, &memoria)?.1.sold_cole)
            .ok_or(StateError::InvalidRequest)?;
    }
    for entity_id in &ids {
        remove_entity(resources, "memorias", *entity_id)?;
    }
    let rewards = home::grant(
        proto,
        home_rules,
        resources,
        changed,
        &[TutorialReward {
            resource_type: 3,
            id: 1,
            quantity: cole,
            resource_params: None,
        }],
        now,
    )?;
    response.set_field_by_name(
        "deleted_resources",
        Value::Message(deleted_resources(proto, &[], &[], &ids)?),
    );
    response.set_field_by_name("rewards", Value::List(rewards));
    Ok(())
}
