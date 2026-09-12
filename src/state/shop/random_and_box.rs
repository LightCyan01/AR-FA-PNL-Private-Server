use super::prelude::*;

pub(crate) fn wheel_reward(id: i32) -> Result<(i32, i32, i32, i32), StateError> {
    // The current wheel text specifies fixed rewards. Earlier wheels disclose only
    // ranges; the local server draws uniformly over each integer in that range.
    Ok(match id {
        1 => (1, 1, 10, 300),
        2 | 3 => (5, 242, 1, 30),
        4 => (1, 1, 300, 300),
        5 => (5, 242, 30, 30),
        _ => return Err(StateError::InvalidRequest),
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn box_gacha(
    proto: &ProtoRegistry,
    rules: &ShopRules,
    home_rules: &home::HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    response: &mut DynamicMessage,
    request: &DynamicMessage,
    now: i64,
) -> Result<(), StateError> {
    let id = i32_field(request, "box_gacha_id").ok_or(StateError::InvalidRequest)?;
    let count = i32_field(request, "count").unwrap_or(0);
    if !(1..=1000).contains(&count) {
        return Err(StateError::InvalidRequest);
    }
    let spec = rules
        .box_gachas
        .iter()
        .find(|r| number(r, "id") == id)
        .ok_or(StateError::InvalidRequest)?;
    if !home::in_period(spec["start_at"].as_i64(), spec["end_at"].as_i64(), now) {
        return Err(StateError::OutOfSchedule);
    }
    let mut state = message_list(resources, "box_gacha_states")
        .into_iter()
        .find(|s| i32_field(s, "box_gacha_id") == Some(id))
        .unwrap_or(empty_message(proto, "blend.model.BoxGachaState")?);
    state.set_field_by_name("box_gacha_id", Value::I32(id));
    let mut box_number = i32_field(&state, "number").unwrap_or(0).max(1);
    let mut drawn = message_list(&state, "card_states")
        .into_iter()
        .filter_map(|s| Some((i32_field(&s, "card_id")?, i32_field(&s, "count")?)))
        .collect::<BTreeMap<_, _>>();
    let mut rewards = Vec::new();
    let mut pickup = Vec::new();
    let mut reset = false;
    pay(
        proto,
        resources,
        changed,
        5,
        number(&spec["item_cost"], "id"),
        number(&spec["item_cost"], "quantity")
            .checked_mul(count)
            .ok_or(StateError::InvalidRequest)?,
    )?;
    for _ in 0..count {
        let box_spec = rules
            .box_gacha_boxes
            .iter()
            .find(|r| {
                number(r, "box_gacha_id") == id
                    && number(r, "min_number") <= box_number
                    && r["max_number"]
                        .as_i64()
                        .is_none_or(|max| i64::from(box_number) <= max)
            })
            .ok_or(StateError::InvalidRequest)?;
        let cards = rules
            .box_gacha_cards
            .iter()
            .filter(|r| {
                values(box_spec, "box_gacha_deck_ids")
                    .iter()
                    .any(|d| d.as_i64() == Some(i64::from(number(r, "box_gacha_deck_id"))))
            })
            .collect::<Vec<_>>();
        let weights = cards
            .iter()
            .map(|c| {
                u32::try_from(
                    number(c, "count") - drawn.get(&number(c, "id")).copied().unwrap_or(0),
                )
                .map_err(|_| StateError::InvalidRequest)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let card = cards[activities::weighted_index(&weights)?];
        let card_id = number(card, "id");
        *drawn.entry(card_id).or_default() += 1;
        let input: TutorialReward = serde_json::from_value(card["reward"].clone())
            .map_err(|e| StateError::MasterData(e.to_string()))?;
        let awarded = home::grant(proto, home_rules, resources, changed, &[input], now)?;
        if card["is_pickup"].as_bool() == Some(true) {
            pickup.extend(awarded.clone());
        }
        rewards.extend(awarded);
        if cards
            .iter()
            .all(|c| drawn.get(&number(c, "id")).copied().unwrap_or(0) == number(c, "count"))
        {
            box_number = box_number
                .checked_add(1)
                .ok_or(StateError::InvalidRequest)?;
            drawn.clear();
            reset = true;
        }
    }
    let mut states = Vec::new();
    for (id, count) in drawn {
        let mut card = empty_message(proto, "blend.model.BoxGachaCardState")?;
        card.set_field_by_name("card_id", Value::I32(id));
        card.set_field_by_name("count", Value::I32(count));
        states.push(Value::Message(card));
    }
    state.set_field_by_name("number", Value::I32(box_number));
    state.set_field_by_name("card_states", Value::List(states));
    home::put(resources, "box_gacha_states", "box_gacha_id", state.clone());
    home::put(changed, "box_gacha_states", "box_gacha_id", state);
    response.set_field_by_name("pickup_rewards", Value::List(pickup));
    response.set_field_by_name("rewards", Value::List(rewards));
    response.set_field_by_name("is_reset", Value::Bool(reset));
    Ok(())
}

pub(crate) fn random_shop_message(
    proto: &ProtoRegistry,
    state: &RandomShop,
    shop_id: i32,
) -> Result<DynamicMessage, StateError> {
    let mut value = empty_message(proto, "blend.model.RandomShopState")?;
    value.set_field_by_name("shop_id", Value::I32(shop_id));
    value.set_field_by_name("refresh_count", Value::I32(state.refresh_count));
    value.set_field_by_name(
        "refresh_at",
        Value::Message(timestamp(proto, state.refresh_at)?),
    );
    value.set_field_by_name(
        "purchase_states",
        Value::List(
            state
                .product_ids
                .iter()
                .map(|id| {
                    let mut row = empty_message(proto, "blend.model.RandomShopPurchaseState")?;
                    row.set_field_by_name("shop_product_id", Value::I32(*id));
                    row.set_field_by_name(
                        "count",
                        Value::I32(*state.purchased.get(id).unwrap_or(&0)),
                    );
                    Ok(Value::Message(row))
                })
                .collect::<Result<Vec<_>, StateError>>()?,
        ),
    );
    Ok(value)
}

pub(crate) fn refresh_random_shop(
    master: &DynamicMessage,
    rule: &ShopRule,
    state: &mut RandomShop,
    now: i64,
) -> Result<(), StateError> {
    let mut ids: Vec<_> = products(master)
        .into_iter()
        .filter(|row| i32_field(row, "shop_id") == Some(rule.id))
        .filter_map(|row| i32_field(&row, "id"))
        .collect();
    ids.sort_unstable();
    if ids.is_empty() {
        return Err(StateError::InvalidRequest);
    }
    let offset = (state.refresh_count.max(0) as usize * 10) % ids.len();
    // ponytail: fixed ten-slot local lineup; replace only if the original server's slot policy is recovered.
    state.product_ids = (0..10)
        .map(|index| ids[(offset + index) % ids.len()])
        .collect();
    state.purchased.clear();
    state.refresh_at = now + i64::from(rule.lineup_refresh_interval_minutes) * 60;
    Ok(())
}
