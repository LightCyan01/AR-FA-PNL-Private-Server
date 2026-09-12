use super::prelude::*;
use home::put;

pub(crate) fn event_open(
    rules: &ActivityRules,
    resources: &DynamicMessage,
    event_id: i32,
    now: i64,
    rewards: bool,
) -> Result<(), StateError> {
    let event = row(rules, "event", event_id)?;
    if message_list(resources, "revived_events")
        .iter()
        .any(|r| i32_field(r, "event_id") == Some(event_id))
    {
        return Ok(());
    }
    if !home::in_period(
        event["start_at"].as_i64(),
        event[if rewards { "reward_end_at" } else { "end_at" }].as_i64(),
        now,
    ) {
        return Err(StateError::OutOfSchedule);
    }
    Ok(())
}

pub(crate) fn house_level<'a>(
    rules: &'a ActivityRules,
    state: &DynamicMessage,
) -> Result<&'a Json, StateError> {
    let id = i32_field(state, "house_building_id").ok_or(StateError::InvalidRequest)?;
    let level = i32_field(state, "level").unwrap_or(0);
    let laps = optional_i32_field(state, "laps").unwrap_or(1);
    rows(rules, "house_building_level")
        .iter()
        .find(|r| {
            number(r, "house_building_id") == id
                && number(r, "level") == level
                && r["laps"].as_i64().is_none_or(|n| n == i64::from(laps))
        })
        .ok_or(StateError::InvalidRequest)
}

pub(crate) fn house_investigations(rules: &ActivityRules, state: &mut DynamicMessage) {
    let id = i32_field(state, "house_building_id").unwrap_or(0);
    let presets = message_list(state, "preset_states");
    let mut records = i32_list(state, "investigation_record_ids");
    for _ in 0..rows(rules, "house_building_investigation").len() {
        let before = records.len();
        for spec in rows(rules, "house_building_investigation")
            .iter()
            .filter(|r| number(r, "house_building_id") == id)
        {
            if !records.contains(&number(spec, "id"))
                && spec["key_product"].as_i64().is_none_or(|p| {
                    presets
                        .iter()
                        .any(|s| i32_field(s, "product") == Some(p as i32))
                })
                && values(spec, "detail_missions").iter().all(|m| {
                    presets
                        .iter()
                        .filter(|s| optional_i32_field(s, "type") == Some(number(m, "type")))
                        .count()
                        >= number(m, "count") as usize
                })
                && values(spec, "special_conditions")
                    .iter()
                    .all(|r| r.as_i64().is_some_and(|n| records.contains(&(n as i32))))
            {
                records.push(number(spec, "id"));
            }
        }
        if before == records.len() {
            break;
        }
    }
    state.set_field_by_name(
        "investigation_record_ids",
        Value::List(records.into_iter().map(Value::I32).collect()),
    );
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn house_rewards(
    proto: &ProtoRegistry,
    rules: &ActivityRules,
    home_rules: &home::HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    spec: &Json,
    state: &mut DynamicMessage,
    now: i64,
) -> Result<Vec<Value>, StateError> {
    let interval = i64::from(number(spec, "reward_interval_seconds"));
    if interval <= 0 {
        return Ok(Vec::new());
    }
    let since = message_i64_field(state, "reward_received_at", "seconds").unwrap_or(now);
    let elapsed = (now - since).max(0);
    // Local production policy: collect at most 24 hours, one uniform table reward per tick.
    let ticks = elapsed.min(86_400) / interval;
    let level = house_level(rules, state)?;
    let choices = values(level, "reward_lotteries");
    let mut inputs = Vec::new();
    if !choices.is_empty() {
        for _ in 0..ticks {
            inputs.push(
                serde_json::from_value::<TutorialReward>(
                    choices[random_below(choices.len() as u32)? as usize]["reward"].clone(),
                )
                .map_err(|e| StateError::MasterData(e.to_string()))?,
            );
        }
    }
    if ticks > 0 {
        state.set_field_by_name(
            "reward_received_at",
            Value::Message(timestamp(
                proto,
                if elapsed > 86_400 {
                    now
                } else {
                    since + ticks * interval
                },
            )?),
        );
    }
    home::grant(
        proto,
        home_rules,
        resources,
        changed,
        &atelier::aggregate_rewards(inputs)?,
        now,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn housing(
    proto: &ProtoRegistry,
    rules: &ActivityRules,
    atelier: &atelier::AtelierRules,
    home_rules: &home::HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    response: &mut DynamicMessage,
    saved: &mut ActivityState,
    route: &str,
    request: &DynamicMessage,
    now: i64,
) -> Result<(), StateError> {
    let id = i32_field(request, "house_building_id").ok_or(StateError::InvalidRequest)?;
    let spec = row(rules, "house_building", id)?;
    event_open(
        rules,
        resources,
        number(spec, "event_id"),
        now,
        route.ends_with("reward_receive"),
    )?;
    if spec["house_building_key_quest_id"]
        .as_i64()
        .is_some_and(|q| quest_clear_count(resources, q as i32) == 0)
    {
        return Err(StateError::InvalidRequest);
    }
    let old = message_list(resources, "house_building_states")
        .into_iter()
        .find(|s| i32_field(s, "house_building_id") == Some(id));
    if route == "/house_building/start" && old.is_none() {
        let first = rows(rules, "house_building_level")
            .iter()
            .filter(|r| number(r, "house_building_id") == id)
            .min_by_key(|r| (number(r, "laps"), number(r, "level")))
            .ok_or(StateError::InvalidRequest)?;
        eligible(first, resources, now)?;
        let mut state = empty_message(proto, "blend.model.HouseBuildingState")?;
        state.set_field_by_name("house_building_id", Value::I32(id));
        state.set_field_by_name("level", Value::I32(number(first, "level")));
        if let Some(laps) = first["laps"].as_i64() {
            state.set_field_by_name("laps", Value::Message(int32_value(proto, laps as i32)?));
        }
        state.set_field_by_name("reward_received_at", Value::Message(timestamp(proto, now)?));
        save(
            resources,
            changed,
            "house_building_states",
            "house_building_id",
            state,
        );
        return Ok(());
    }
    let mut state = old.ok_or(StateError::InvalidRequest)?;
    let mut awarded = Vec::new();
    match route {
        "/house_building/start" => {}
        "/house_building/reward_receive" => {
            awarded = house_rewards(
                proto, rules, home_rules, resources, changed, spec, &mut state, now,
            )?
        }
        "/house_building/enhance" => {
            let count = optional_i32_field(request, "count").unwrap_or(1);
            if !(1..=100).contains(&count) {
                return Err(StateError::InvalidRequest);
            }
            awarded.extend(house_rewards(
                proto, rules, home_rules, resources, changed, spec, &mut state, now,
            )?);
            for _ in 0..count {
                state.set_field_by_name(
                    "level",
                    Value::I32(
                        i32_field(&state, "level")
                            .unwrap_or(0)
                            .checked_add(1)
                            .ok_or(StateError::InvalidRequest)?,
                    ),
                );
                let level = house_level(rules, &state)?;
                eligible(level, resources, now)?;
                for cost in values(level, "item_costs") {
                    let (_, item) = pay_resource_cost(
                        proto,
                        resources,
                        Some(&TutorialRecipeCost {
                            resource_type: 5,
                            id: number(cost, "id"),
                            quantity: number(cost, "quantity"),
                        }),
                    )?
                    .ok_or(StateError::InvalidRequest)?;
                    put(changed, "items", "item_id", item);
                }
                let options = values(level, "preset_products");
                if !options.is_empty() {
                    let product = optional_i32_field(request, "house_building_product_id");
                    let kind = optional_i32_field(request, "house_building_product_type_id");
                    let selected = options
                        .iter()
                        .find(|p| {
                            product.map_or(options.len() == 1, |n| number(p, "product") == n)
                                && kind.is_none_or(|n| p["type"].as_i64() == Some(i64::from(n)))
                        })
                        .ok_or(StateError::InvalidRequest)?;
                    let mut preset = empty_message(proto, "blend.model.HouseBuildingPresetState")?;
                    preset.set_field_by_name("product", Value::I32(number(selected, "product")));
                    if let Some(kind) = selected["type"].as_i64() {
                        preset.set_field_by_name(
                            "type",
                            Value::Message(int32_value(proto, kind as i32)?),
                        );
                    }
                    let mut presets = message_list(&state, "preset_states");
                    presets.push(preset.clone());
                    state.set_field_by_name(
                        "preset_states",
                        Value::List(presets.into_iter().map(Value::Message).collect()),
                    );
                    let mut history =
                        empty_message(proto, "blend.model.HouseBuildingPresetHistoryState")?;
                    for (field, value) in preset.fields() {
                        history.set_field_by_name(field.name(), value.clone());
                    }
                    history.set_field_by_name("level", Value::I32(number(level, "level")));
                    let mut histories = message_list(&state, "preset_history_states");
                    histories.push(history);
                    state.set_field_by_name(
                        "preset_history_states",
                        Value::List(histories.into_iter().map(Value::Message).collect()),
                    );
                }
                awarded.extend(home::grant(
                    proto,
                    home_rules,
                    resources,
                    changed,
                    &rewards(level, "rewards")?,
                    now,
                )?);
                let total = saved.housing_counts.entry(id).or_default();
                *total = total.checked_add(1).ok_or(StateError::InvalidRequest)?;
            }
            house_investigations(rules, &mut state);
        }
        "/house_building/reset" => {
            let current = i32_field(&state, "level").unwrap_or(0);
            let laps = optional_i32_field(&state, "laps").ok_or(StateError::InvalidRequest)?;
            if rows(rules, "house_building_level").iter().any(|r| {
                number(r, "house_building_id") == id
                    && number(r, "laps") == laps
                    && number(r, "level") > current
            }) {
                return Err(StateError::InvalidRequest);
            }
            state.set_field_by_name(
                "laps",
                Value::Message(int32_value(
                    proto,
                    laps.checked_add(1).ok_or(StateError::InvalidRequest)?,
                )?),
            );
            state.set_field_by_name("level", Value::I32(1));
            house_level(rules, &state)?;
            state.set_field_by_name("preset_states", Value::List(Vec::new()));
            state.set_field_by_name("reset_at", Value::Message(timestamp(proto, now)?));
        }
        "/house_building/count_reward_receive" => {
            let received = message_list(resources, "house_building_count_reward_states")
                .iter()
                .find(|r| i32_field(r, "house_building_id") == Some(id))
                .and_then(|r| i32_field(r, "count"))
                .unwrap_or(0);
            let total = *saved.housing_counts.get(&id).unwrap_or(&0);
            let mut maximum = received;
            for reward in rows(rules, "house_building_count_reward")
                .iter()
                .filter(|r| {
                    number(r, "house_building_id") == id
                        && number(r, "count") > received
                        && number(r, "count") <= total
                })
            {
                awarded.extend(home::grant(
                    proto,
                    home_rules,
                    resources,
                    changed,
                    &rewards(reward, "rewards")?,
                    now,
                )?);
                maximum = maximum.max(number(reward, "count"));
            }
            if maximum == received {
                return Err(StateError::InvalidRequest);
            }
            let mut received = empty_message(proto, "blend.model.HouseBuildingCountRewardState")?;
            received.set_field_by_name("house_building_id", Value::I32(id));
            received.set_field_by_name("count", Value::I32(maximum));
            save(
                resources,
                changed,
                "house_building_count_reward_states",
                "house_building_id",
                received,
            );
        }
        "/house_building/rent" => {
            let shop = &spec["shop"];
            if shop.is_null() {
                return Err(StateError::InvalidRequest);
            }
            eligible(shop, resources, now)?;
            let tools = message_list(request, "rental_tools");
            let capacity = number(house_level(rules, &state)?, "rental_level");
            if tools.is_empty()
                || tools.len() > capacity as usize
                || !message_list(&state, "rental_tools").is_empty()
            {
                return Err(StateError::InvalidRequest);
            }
            let mut keys = BTreeSet::new();
            let mut loans = Vec::new();
            for tool in tools {
                let kind = i32_field(&tool, "type").ok_or(StateError::InvalidRequest)?;
                let entity = i32_field(&tool, "entity_id").ok_or(StateError::InvalidRequest)?;
                if !keys.insert((kind, entity)) {
                    return Err(StateError::InvalidRequest);
                }
                let field = match kind {
                    6 => "equipment_tools",
                    14 => "battle_tools",
                    _ => return Err(StateError::InvalidRequest),
                };
                let owned = message_list(resources, field)
                    .into_iter()
                    .find(|t| i32_field(t, "entity_id") == Some(entity))
                    .ok_or(StateError::InvalidRequest)?;
                let tool_id = i32_field(&owned, "tool_id").ok_or(StateError::InvalidRequest)?;
                let rarity = atelier::tool_rarity(atelier, kind, tool_id)?;
                let mut params = empty_message(proto, "blend.model.ResourceParams")?;
                params.set_field_by_name("rank", Value::Message(int32_value(proto, rarity)?));
                params.set_field_by_name(
                    "traits",
                    Value::List(
                        message_list(&owned, "traits")
                            .into_iter()
                            .map(Value::Message)
                            .collect(),
                    ),
                );
                loans.push(Value::Message(resource_message(proto, kind, tool_id, 1)?));
                if let Some(Value::Message(loan)) = loans.last_mut() {
                    loan.set_field_by_name("resource_params", Value::Message(params));
                }
            }
            // Local rental policy: one-hour loans snapshot the owned tools; ownership is retained.
            state.set_field_by_name("rental_tools", Value::List(loans));
            state.set_field_by_name(
                "rental_end_at",
                Value::Message(timestamp(proto, now + 3600)?),
            );
        }
        "/house_building/rental_reward_receive" => {
            let shop = &spec["shop"];
            let loans = message_list(&state, "rental_tools");
            if shop.is_null()
                || loans.is_empty()
                || message_i64_field(&state, "rental_end_at", "seconds").is_none_or(|end| now < end)
            {
                return Err(StateError::InvalidRequest);
            }
            let mut amount = 0_i64;
            for loan in loans {
                let params = member_status(&loan, "resource_params")?;
                let rarity =
                    optional_i32_field(&params, "rank").ok_or(StateError::InvalidRequest)?;
                let value = rows(rules, "house_building_shop_tool_rarity")
                    .iter()
                    .find(|r| {
                        number(r, "set_id") == number(shop, "tool_rarity_set_id")
                            && number(r, "tool_rarity_id") == rarity
                    })
                    .ok_or(StateError::InvalidRequest)?;
                let rank = 1 + message_list(&params, "traits")
                    .iter()
                    .map(|t| i32_field(t, "rank").unwrap_or(0))
                    .sum::<i32>();
                let bonus = rows(rules, "house_building_shop_trait_rank_total")
                    .iter()
                    .find(|r| {
                        number(r, "set_id") == number(shop, "trait_rank_total_set_id")
                            && number(r, "trait_rank_total_id") == rank
                    })
                    .map(|r| {
                        number(
                            r,
                            if i32_field(&loan, "type") == Some(6) {
                                "equipment_tool_value"
                            } else {
                                "battle_tool_value"
                            },
                        )
                    })
                    .unwrap_or(0);
                amount = amount
                    .checked_add(i64::from(number(value, "value")) + i64::from(bonus))
                    .ok_or(StateError::InvalidRequest)?;
            }
            amount = amount
                .checked_mul(10_000 + i64::from(number(house_level(rules, &state)?, "shop_bonus")))
                .ok_or(StateError::InvalidRequest)?
                / 10_000;
            awarded = home::grant(
                proto,
                home_rules,
                resources,
                changed,
                &[TutorialReward {
                    resource_type: 5,
                    id: number(shop, "reward_item_id"),
                    quantity: checked_i32(amount)?,
                    resource_params: None,
                }],
                now,
            )?;
            state.set_field_by_name("rental_tools", Value::List(Vec::new()));
            state.clear_field_by_name("rental_end_at");
        }
        _ => return Err(StateError::InvalidRequest),
    }
    save(
        resources,
        changed,
        "house_building_states",
        "house_building_id",
        state,
    );
    if response.get_field_by_name("rewards").is_some() {
        response.set_field_by_name("rewards", Value::List(awarded));
    }
    Ok(())
}
