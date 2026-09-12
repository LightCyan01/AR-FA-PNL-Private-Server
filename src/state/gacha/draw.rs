use crate::state::service::prelude::*;

pub(crate) fn gacha_rule(
    rules: &TutorialRules,
    gacha_id: i32,
) -> Result<&TutorialGacha, StateError> {
    rules
        .gachas
        .iter()
        .find(|gacha| gacha.id == gacha_id)
        .ok_or(StateError::InvalidRequest)
}

pub(crate) fn gacha_button_rule(
    rules: &TutorialRules,
    button_id: i32,
) -> Result<&TutorialGachaButton, StateError> {
    rules
        .gacha_buttons
        .iter()
        .find(|button| button.id == button_id)
        .ok_or(StateError::InvalidRequest)
}

pub(crate) fn selected_gacha_button(
    rules: &TutorialRules,
    id: i32,
    step_up: bool,
) -> Result<&TutorialGachaButton, StateError> {
    if step_up {
        rules
            .gacha_step_up_buttons
            .iter()
            .find(|b| b.id == id)
            .ok_or(StateError::InvalidRequest)
    } else {
        gacha_button_rule(rules, id)
    }
}

pub(crate) fn gacha_message(
    proto: &ProtoRegistry,
    gacha_rule: &TutorialGacha,
    button_states: &[GachaButtonState],
) -> Result<DynamicMessage, StateError> {
    let mut gacha = empty_message(proto, "blend.model.Gacha")?;
    gacha.set_field_by_name("gacha_id", Value::I32(gacha_rule.id));
    let mut states = Vec::new();
    for button_id in &gacha_rule.button_ids {
        let stored = button_states.iter().find(|state| {
            state.gacha_id == i64::from(gacha_rule.id) && state.button_id == i64::from(*button_id)
        });
        let mut state = empty_message(proto, "blend.model.GachaButtonState")?;
        state.set_field_by_name("gacha_id", Value::I32(gacha_rule.id));
        state.set_field_by_name("gacha_button_id", Value::I32(*button_id));
        state.set_field_by_name(
            "execution_count",
            Value::I32(
                stored
                    .map(|value| value.execution_count as i32)
                    .unwrap_or(0),
            ),
        );
        if let Some(stored) = stored {
            state.set_field_by_name(
                "last_executed_at",
                Value::Message(timestamp(proto, stored.last_executed_at)?),
            );
        }
        states.push(Value::Message(state));
    }
    gacha.set_field_by_name("gacha_button_states", Value::List(states));
    let steps = button_states
        .iter()
        .find(|s| s.gacha_id == i64::from(gacha_rule.id) && s.button_id == -1)
        .map(|s| s.execution_count)
        .unwrap_or(0);
    gacha.set_field_by_name(
        "step_up_execution_count",
        Value::I32(i32::try_from(steps).map_err(|_| StateError::InvalidRequest)?),
    );
    Ok(gacha)
}

pub(crate) fn gacha_card_message(
    proto: &ProtoRegistry,
    card: &TutorialGachaCard,
) -> Result<DynamicMessage, StateError> {
    let mut message = empty_message(proto, "blend.model.GachaCard")?;
    message.set_field_by_name("card_type", Value::I32(card.resource_type));
    message.set_field_by_name("card_id", Value::I32(card.id));
    message.set_field_by_name("card_quantity", Value::I32(1));
    if card.resource_type == 4 {
        message.set_field_by_name(
            "character_acquisition_rarity_id",
            Value::Message(int32_value(proto, card.rarity)?),
        );
    }
    Ok(message)
}

pub(crate) fn gacha_rate_message(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    rate: &TutorialGachaRate,
) -> Result<DynamicMessage, StateError> {
    let mut row = empty_message(proto, "blend.model.GachaRate")?;
    row.set_field_by_name("gacha_rate_id", Value::I32(rate.id));
    row.set_field_by_name(
        "percent_rate_per_card",
        Value::String(format_rate(
            u64::try_from(rate.total_basis_points).map_err(|_| StateError::InvalidRequest)? * 10,
            rate.cards(rules).len(),
        )?),
    );
    row.set_field_by_name(
        "cards",
        Value::List(
            rate.cards(rules)
                .iter()
                .map(|card| Ok(Value::Message(gacha_card_message(proto, card)?)))
                .collect::<Result<Vec<_>, StateError>>()?,
        ),
    );
    Ok(row)
}

pub(crate) fn gacha_summaries(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    gacha: &TutorialGacha,
) -> Result<Vec<Value>, StateError> {
    let mut rates = rules
        .gacha_rates
        .iter()
        .filter(|rate| rate.rate_set_id == gacha.rate_set_id)
        .collect::<Vec<_>>();
    rates.sort_by_key(|rate| {
        rules
            .gacha_decks
            .iter()
            .find(|deck| deck.id == rate.deck_id)
            .map(|deck| (deck.card_type, deck.rarity))
            .unwrap_or((i32::MAX, i32::MAX))
    });
    rates
        .into_iter()
        .map(|rate| {
            let deck = rules
                .gacha_decks
                .iter()
                .find(|deck| deck.id == rate.deck_id)
                .ok_or(StateError::InvalidRequest)?;
            let mut summary = empty_message(proto, "blend.model.GachaRateSummary")?;
            summary.set_field_by_name("card_type", Value::I32(deck.card_type));
            summary.set_field_by_name("rarity", Value::I32(deck.rarity));
            summary.set_field_by_name(
                "percent_rate",
                Value::String(format!("{:.3}", f64::from(rate.total_basis_points) / 100.0)),
            );
            Ok(Value::Message(summary))
        })
        .collect()
}

pub(crate) fn gacha_rate_set_message(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    gacha: &TutorialGacha,
) -> Result<DynamicMessage, StateError> {
    let rows = rules
        .gacha_rates
        .iter()
        .filter(|rate| rate.rate_set_id == gacha.rate_set_id)
        .map(|rate| Ok(Value::Message(gacha_rate_message(proto, rules, rate)?)))
        .collect::<Result<Vec<_>, StateError>>()?;
    let mut set = empty_message(proto, "blend.model.GachaRateSet")?;
    set.set_field_by_name("gacha_rate_set_id", Value::I32(gacha.rate_set_id));
    set.set_field_by_name("rows", Value::List(rows));
    set.set_field_by_name(
        "summaries",
        Value::List(gacha_summaries(proto, rules, gacha)?),
    );
    for (field, count) in [
        "wish_list_character_count",
        "wish_list_memoria_count",
        "wish_list_character_skin_count",
    ]
    .into_iter()
    .zip(gacha.wish_list_counts)
    {
        set.set_field_by_name(field, Value::I32(count as i32));
    }
    Ok(set)
}

pub(crate) fn validate_wish_list(
    gacha: &TutorialGacha,
    wish_list: &GachaWishList,
) -> Result<(), StateError> {
    let selected = wish_list.character_ids.len()
        + wish_list.memoria_ids.len()
        + wish_list.character_skin_ids.len();
    if wish_list.gacha_id != i64::from(gacha.id)
        || if gacha.step_up_button_ids.is_empty() {
            selected != usize::try_from(gacha.mixed_select_count).unwrap_or(usize::MAX)
        } else {
            [
                wish_list.character_ids.len(),
                wish_list.memoria_ids.len(),
                wish_list.character_skin_ids.len(),
            ] != gacha.wish_list_counts
        }
        || wish_list
            .character_ids
            .iter()
            .any(|id| !gacha.mixed_pickup_character_ids.contains(id))
        || wish_list
            .memoria_ids
            .iter()
            .any(|id| !gacha.mixed_pickup_memoria_ids.contains(id))
        || !wish_list.character_skin_ids.is_empty()
    {
        return Err(StateError::InvalidRequest);
    }
    let mut ids = wish_list.character_ids.clone();
    ids.sort_unstable();
    if ids.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(StateError::InvalidRequest);
    }
    let mut ids = wish_list.memoria_ids.clone();
    ids.sort_unstable();
    if ids.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(StateError::InvalidRequest);
    }
    Ok(())
}

pub(crate) fn gacha_wish_list_message(
    proto: &ProtoRegistry,
    wish_list: &GachaWishList,
) -> Result<DynamicMessage, StateError> {
    let mut state = empty_message(proto, "blend.model.GachaWishListState")?;
    state.set_field_by_name("gacha_id", Value::I32(wish_list.gacha_id as i32));
    state.set_field_by_name(
        "character_ids",
        Value::List(
            wish_list
                .character_ids
                .iter()
                .copied()
                .map(Value::I32)
                .collect(),
        ),
    );
    state.set_field_by_name(
        "memoria_ids",
        Value::List(
            wish_list
                .memoria_ids
                .iter()
                .copied()
                .map(Value::I32)
                .collect(),
        ),
    );
    state.set_field_by_name(
        "character_skin_ids",
        Value::List(
            wish_list
                .character_skin_ids
                .iter()
                .copied()
                .map(Value::I32)
                .collect(),
        ),
    );
    Ok(state)
}

pub(crate) fn mixed_gacha_rate_set_message(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    gacha: &TutorialGacha,
) -> Result<DynamicMessage, StateError> {
    let mut dynamic_rows = Vec::new();
    let mut rows = Vec::new();
    for rate in rules
        .gacha_rates
        .iter()
        .filter(|rate| rate.rate_set_id == gacha.rate_set_id)
    {
        let deck = rules
            .gacha_decks
            .iter()
            .find(|deck| deck.id == rate.deck_id)
            .ok_or(StateError::InvalidRequest)?;
        if deck.rarity != 3 {
            rows.push(Value::Message(gacha_rate_message(proto, rules, rate)?));
            continue;
        }
        let mut row = empty_message(proto, "blend.model.GachaMixedWishListRate")?;
        row.set_field_by_name("gacha_rate_id", Value::I32(rate.id));
        row.set_field_by_name(
            "cards",
            Value::List(
                rate.cards(rules)
                    .iter()
                    .map(|card| Ok(Value::Message(gacha_card_message(proto, card)?)))
                    .collect::<Result<Vec<_>, StateError>>()?,
            ),
        );
        let mut selection_rates = Vec::new();
        for selected_type in [17, 4] {
            let selectable = if selected_type == 4 {
                &gacha.mixed_pickup_character_ids
            } else {
                &gacha.mixed_pickup_memoria_ids
            };
            if selectable.is_empty() {
                continue;
            }
            let pickups = if deck.card_type == 4 {
                &gacha.mixed_pickup_character_ids
            } else {
                &gacha.mixed_pickup_memoria_ids
            };
            let selected_pickups: i32 = if selected_type == deck.card_type {
                1
            } else {
                0
            };
            let fixed_units =
                selected_pickups * 1000 + (pickups.len() as i32 - selected_pickups) * 250;
            if rate.cards(rules).is_empty() {
                continue;
            }
            let mut by_selection =
                empty_message(proto, "blend.model.GachaMixedWishListRateBySelection")?;
            if selected_type == 4 {
                by_selection.set_field_by_name("character_count", Value::I32(1));
            } else {
                by_selection.set_field_by_name("memoria_count", Value::I32(1));
            }
            by_selection.set_field_by_name(
                "percent_rate_per_card",
                Value::String(format_rate(
                    u64::try_from(rate.total_basis_points * 10 - fixed_units)
                        .map_err(|_| StateError::InvalidRequest)?,
                    rate.cards(rules).len(),
                )?),
            );
            selection_rates.push(Value::Message(by_selection));
        }
        row.set_field_by_name("rates", Value::List(selection_rates));
        dynamic_rows.push(Value::Message(row));
    }
    let mut set = empty_message(proto, "blend.model.GachaMixedWishListRateSet")?;
    set.set_field_by_name("gacha_id", Value::I32(gacha.id));
    set.set_field_by_name("selected_card_rate", Value::String("1.000".into()));
    set.set_field_by_name("non_selected_card_rate", Value::String("0.250".into()));
    set.set_field_by_name("dynamic_rows", Value::List(dynamic_rows));
    set.set_field_by_name("rows", Value::List(rows));
    set.set_field_by_name(
        "summaries",
        Value::List(gacha_summaries(proto, rules, gacha)?),
    );
    set.set_field_by_name(
        "pickup_total_character_ids",
        Value::List(
            gacha
                .mixed_pickup_character_ids
                .iter()
                .copied()
                .map(Value::I32)
                .collect(),
        ),
    );
    Ok(set)
}

pub(crate) fn reduce_gacha_list(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    resources: &DynamicMessage,
    category: i32,
    button_states: &[GachaButtonState],
    stored_wish_lists: &[GachaWishList],
    now: i64,
) -> Result<DynamicMessage, StateError> {
    if quest_clear_count(resources, 101001017) == 0
        || tutorial_step(resources) < TUTORIAL_STEP_MEMORIA_EQUIPPED
    {
        return Err(StateError::InvalidRequest);
    }
    let gachas = rules
        .gachas
        .iter()
        .filter(|gacha| {
            gacha.category == category
                && gacha.start_at.is_none_or(|start| start <= now)
                && gacha.end_at.is_none_or(|end| now < end)
                && matches!(
                    gacha.id,
                    2 | 2093
                        ..=2100
                            | 2017
                            | 2020
                            | 2022
                            | 2024
                            | 2026
                            | 2030
                            | 2032
                            | 2034
                            | 2040
                            | 2045
                            | 2047
                            | 2049
                            | 2051
                            | 2054
                            | 2056
                            | 2058
                            | 2060
                            | 2062
                            | 2064
                            | 2066
                            | 2068
                            | 2070
                            | 2072
                            | 2075
                            | 2078
                            | 2083
                            | 2086
                            | 2089
                            | 2092
                            | 2107
                            | 2118
                            | 2120
                            | 2126
                            | 2128
                            | 2132
                            | 2134
                            | 2137
                            | 2139
                            | 2142
                            | 2145
                            | 2215
                            | 3075
                            | 3080
                            | 5000
                            | 5001
                )
        })
        .collect::<Vec<_>>();
    let messages = gachas
        .iter()
        .map(|gacha| Ok(Value::Message(gacha_message(proto, gacha, button_states)?)))
        .collect::<Result<Vec<_>, StateError>>()?;
    let mut rate_sets = Vec::new();
    let mut mixed_rate_sets = Vec::new();
    let mut wish_lists = Vec::new();
    for gacha in &gachas {
        if !gacha.step_up_button_ids.is_empty() {
            for id in &gacha.step_up_button_ids {
                let button = selected_gacha_button(rules, *id, true)?;
                if rate_sets
                    .iter()
                    .filter_map(|v: &Value| v.as_message())
                    .any(|m| i32_field(m, "gacha_rate_set_id") == Some(button.rate_set_id))
                {
                    continue;
                }
                let mut step = (*gacha).clone();
                step.rate_set_id = button.rate_set_id;
                rate_sets.push(Value::Message(gacha_rate_set_message(proto, rules, &step)?));
            }
            if let Some(wish_list) = stored_wish_lists
                .iter()
                .find(|s| s.gacha_id == i64::from(gacha.id))
            {
                validate_wish_list(gacha, wish_list)?;
                wish_lists.push(Value::Message(gacha_wish_list_message(proto, wish_list)?));
            }
            continue;
        }
        if gacha.mixed_select_count == 0 {
            rate_sets.push(Value::Message(gacha_rate_set_message(proto, rules, gacha)?));
            continue;
        }
        mixed_rate_sets.push(Value::Message(mixed_gacha_rate_set_message(
            proto, rules, gacha,
        )?));
        if let Some(wish_list) = stored_wish_lists
            .iter()
            .find(|state| state.gacha_id == i64::from(gacha.id))
        {
            validate_wish_list(gacha, wish_list)?;
            wish_lists.push(Value::Message(gacha_wish_list_message(proto, wish_list)?));
        }
    }
    let mut response = empty_message(proto, "blend.api.GachaListResponse")?;
    response.set_field_by_name("gachas", Value::List(messages));
    response.set_field_by_name("gacha_rate_sets", Value::List(rate_sets));
    response.set_field_by_name(
        "gacha_mixed_wish_list_rate_sets",
        Value::List(mixed_rate_sets),
    );
    response.set_field_by_name("wish_list_states", Value::List(wish_lists));
    response.set_field_by_name(
        "changed_resources",
        Value::Message(empty_message(proto, "blend.model.Resources")?),
    );
    Ok(response)
}

pub(crate) fn distribute_cards(
    output: &mut Vec<(TutorialGachaCard, u64)>,
    cards: &[TutorialGachaCard],
    total_units: u64,
) -> Result<(), StateError> {
    if cards.is_empty() || total_units < cards.len() as u64 {
        return Err(StateError::InvalidRequest);
    }
    let base = total_units / cards.len() as u64;
    let remainder = total_units % cards.len() as u64;
    output.extend(
        cards
            .iter()
            .enumerate()
            .map(|(index, card)| (card.clone(), base + u64::from((index as u64) < remainder))),
    );
    Ok(())
}

pub(crate) fn format_rate(total_units: u64, card_count: usize) -> Result<String, StateError> {
    let per_card = total_units
        .checked_div(u64::try_from(card_count).map_err(|_| StateError::InvalidRequest)?)
        .ok_or(StateError::InvalidRequest)?;
    Ok(format!("{}.{:03}", per_card / 1000, per_card % 1000))
}

pub(crate) fn weighted_gacha_cards(
    rules: &TutorialRules,
    gacha: &TutorialGacha,
    wish_list: Option<&GachaWishList>,
) -> Result<Vec<(TutorialGachaCard, u64)>, StateError> {
    let mut output = Vec::new();
    for rate in rules
        .gacha_rates
        .iter()
        .filter(|rate| rate.rate_set_id == gacha.rate_set_id)
    {
        let total_units =
            u64::try_from(rate.total_basis_points).map_err(|_| StateError::InvalidRequest)? * 10;
        let cards = rate.cards(rules);
        let first = cards.first().ok_or(StateError::InvalidRequest)?;
        if gacha.mixed_select_count == 0 || first.rarity != 3 {
            distribute_cards(&mut output, cards, total_units)?;
            continue;
        }
        let wish_list = wish_list.ok_or(StateError::InvalidRequest)?;
        validate_wish_list(gacha, wish_list)?;
        let pickups = if first.resource_type == 4 {
            &gacha.mixed_pickup_character_ids
        } else {
            &gacha.mixed_pickup_memoria_ids
        };
        let selected = if first.resource_type == 4 {
            &wish_list.character_ids
        } else {
            &wish_list.memoria_ids
        };
        let mut fixed_units = 0u64;
        for pickup_id in pickups {
            let card = rules
                .gacha_rates
                .iter()
                .flat_map(|row| row.cards(rules))
                .find(|card| card.resource_type == first.resource_type && card.id == *pickup_id)
                .ok_or(StateError::InvalidRequest)?;
            let weight = if selected.contains(&card.id) {
                1000
            } else {
                250
            };
            fixed_units += weight;
            output.push((card.clone(), weight));
        }
        let normal = cards
            .iter()
            .filter(|card| !pickups.contains(&card.id))
            .cloned()
            .collect::<Vec<_>>();
        distribute_cards(
            &mut output,
            &normal,
            total_units
                .checked_sub(fixed_units)
                .ok_or(StateError::InvalidRequest)?,
        )?;
    }
    if output.iter().map(|(_, weight)| weight).sum::<u64>() != 100_000 {
        return Err(StateError::InvalidRequest);
    }
    Ok(output)
}

pub(crate) fn select_weighted_card(
    cards: &[(TutorialGachaCard, u64)],
) -> Result<TutorialGachaCard, StateError> {
    let total = cards.iter().map(|(_, weight)| weight).sum::<u64>();
    let mut draw = u64::from(random_below(
        u32::try_from(total).map_err(|_| StateError::InvalidRequest)?,
    )?);
    for (card, weight) in cards {
        if draw < *weight {
            return Ok(card.clone());
        }
        draw -= *weight;
    }
    Err(StateError::InvalidRequest)
}

pub(crate) fn gacha_duplicate_counts(
    gacha: &TutorialGacha,
    character_id: i32,
    rarity: i32,
) -> Result<(i32, i32), StateError> {
    if let Some(pieces) = gacha
        .duplicate_pieces
        .iter()
        .find(|pieces| pieces.character_ids.contains(&character_id))
    {
        return Ok((pieces.piece_count, pieces.generic_piece_count));
    }
    let quantity = match rarity {
        1 => 1,
        2 => 10,
        3..=8 => 50,
        _ => return Err(StateError::InvalidRequest),
    };
    Ok((quantity, quantity))
}

pub(crate) fn change_character_piece(
    proto: &ProtoRegistry,
    resources: &mut DynamicMessage,
    character_id: i32,
    delta: i32,
) -> Result<DynamicMessage, StateError> {
    let mut pieces = message_list(resources, "character_pieces");
    let mut changed = None;
    for piece in &mut pieces {
        if i32_field(piece, "character_id") == Some(character_id) {
            let quantity = i32_field(piece, "quantity")
                .unwrap_or(0)
                .checked_add(delta)
                .filter(|value| *value >= 0)
                .ok_or(StateError::InvalidRequest)?;
            piece.set_field_by_name("quantity", Value::I32(quantity));
            changed = Some(piece.clone());
            break;
        }
    }
    if changed.is_none() {
        let mut piece = empty_message(proto, "blend.model.CharacterPiece")?;
        if delta < 0 {
            return Err(StateError::InvalidRequest);
        }
        piece.set_field_by_name("character_id", Value::I32(character_id));
        piece.set_field_by_name("quantity", Value::I32(delta));
        pieces.push(piece.clone());
        changed = Some(piece);
    }
    resources.set_field_by_name(
        "character_pieces",
        Value::List(pieces.into_iter().map(Value::Message).collect()),
    );
    changed.ok_or(StateError::InvalidRequest)
}

pub(crate) fn resource_message(
    proto: &ProtoRegistry,
    resource_type: i32,
    id: i32,
    quantity: i32,
) -> Result<DynamicMessage, StateError> {
    let mut resource = empty_message(proto, "blend.model.Resource")?;
    resource.set_field_by_name("type", Value::I32(resource_type));
    resource.set_field_by_name("id", Value::I32(id));
    resource.set_field_by_name("quantity", Value::I32(quantity));
    Ok(resource)
}

pub(crate) fn pay_resource_cost(
    proto: &ProtoRegistry,
    resources: &mut DynamicMessage,
    cost: Option<&TutorialRecipeCost>,
) -> Result<Option<(&'static str, DynamicMessage)>, StateError> {
    let Some(cost) = cost else {
        return Ok(None);
    };
    if cost.quantity < 0 {
        return Err(StateError::InvalidRequest);
    }
    if cost.quantity == 0 {
        return Ok(None);
    }
    match cost.resource_type {
        3 => {
            let mut status = status_message(resources)?;
            let cole = i32_field(&status, "cole")
                .unwrap_or(0)
                .checked_sub(cost.quantity)
                .filter(|value| *value >= 0)
                .ok_or(StateError::InvalidRequest)?;
            status.set_field_by_name("cole", Value::I32(cole));
            resources.set_field_by_name("status", Value::Message(status.clone()));
            Ok(Some(("status", status)))
        }
        8 => Ok(Some((
            "character_pieces",
            change_character_piece(proto, resources, cost.id, -cost.quantity)?,
        ))),
        1 => {
            let mut wallet = resources
                .get_field_by_name("wallet")
                .and_then(|value| value.as_message().cloned())
                .ok_or(StateError::InvalidRequest)?;
            let free = i32_field(&wallet, "free").unwrap_or(0);
            let paid = i32_field(&wallet, "paid").unwrap_or(0);
            if free
                .checked_add(paid)
                .is_none_or(|total| total < cost.quantity)
            {
                return Err(StateError::InvalidRequest);
            }
            let from_free = free.min(cost.quantity);
            wallet.set_field_by_name("free", Value::I32(free - from_free));
            wallet.set_field_by_name("paid", Value::I32(paid - (cost.quantity - from_free)));
            resources.set_field_by_name("wallet", Value::Message(wallet.clone()));
            Ok(Some(("wallet", wallet)))
        }
        2 => {
            let mut wallet = resources
                .get_field_by_name("wallet")
                .and_then(|value| value.as_message().cloned())
                .ok_or(StateError::InvalidRequest)?;
            let paid = i32_field(&wallet, "paid")
                .unwrap_or(0)
                .checked_sub(cost.quantity)
                .filter(|value| *value >= 0)
                .ok_or(StateError::InvalidRequest)?;
            wallet.set_field_by_name("paid", Value::I32(paid));
            resources.set_field_by_name("wallet", Value::Message(wallet.clone()));
            Ok(Some(("wallet", wallet)))
        }
        5 => Ok(Some((
            "items",
            change_item(proto, resources, cost.id, -cost.quantity)?,
        ))),
        _ => Err(StateError::InvalidRequest),
    }
}

pub(crate) fn reduce_gacha_execute(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    mut resources: DynamicMessage,
    request: &DynamicMessage,
    execution_count: i64,
    wish_list: Option<&GachaWishList>,
    now: i64,
) -> Result<GachaMutation, StateError> {
    if quest_clear_count(&resources, 101001017) != 1 {
        return Err(StateError::InvalidRequest);
    }
    let step_up = request.descriptor().full_name() == "blend.api.GachaStepUpExecuteRequest";
    let gacha_id = i32_field(request, "gacha_id").ok_or(StateError::InvalidRequest)?;
    let button_id = i32_field(
        request,
        if step_up {
            "gacha_step_up_button_id"
        } else {
            "gacha_button_id"
        },
    )
    .ok_or(StateError::InvalidRequest)?;
    if (gacha_id == 2 && tutorial_step(&resources) != TUTORIAL_STEP_MEMORIA_EQUIPPED)
        || (gacha_id != 2 && tutorial_step(&resources) < TUTORIAL_STEP_GACHA_COMPLETE)
    {
        return Err(StateError::InvalidRequest);
    }
    let gacha = gacha_rule(rules, gacha_id)?;
    let button = selected_gacha_button(rules, button_id, step_up)?;
    if execution_count < 0
        || if step_up {
            let count = usize::try_from(execution_count).map_err(|_| StateError::InvalidRequest)?;
            let length = gacha.step_up_button_ids.len();
            length == 0
                || gacha.step_up_loop_count <= 0
                || count >= length * gacha.step_up_loop_count as usize
                || gacha.step_up_button_ids[count % length] != button_id
        } else {
            !gacha.button_ids.contains(&button_id)
        }
        || gacha.start_at.is_some_and(|start| now < start)
        || gacha.end_at.is_some_and(|end| now >= end)
    {
        return Err(StateError::InvalidRequest);
    }
    let mut changed = empty_message(proto, "blend.model.Resources")?;
    let mut changed_items = BTreeMap::new();
    if let Some((field, value)) = pay_resource_cost(proto, &mut resources, button.cost.as_ref())? {
        if field == "wallet" {
            changed.set_field_by_name(field, Value::Message(value));
        } else {
            let item_id = i32_field(&value, "item_id").ok_or(StateError::InvalidRequest)?;
            changed_items.insert(item_id, value);
        }
    }
    let weighted_cards = if step_up {
        let mut step = gacha.clone();
        step.rate_set_id = button.rate_set_id;
        if button.is_wish_list {
            let wish = wish_list.ok_or(StateError::InvalidRequest)?;
            validate_wish_list(gacha, wish)?;
            let mut cards = Vec::new();
            for card in rules
                .gacha_rates
                .iter()
                .filter(|r| r.rate_set_id == button.rate_set_id)
                .flat_map(|r| r.cards(rules))
            {
                let chosen = match card.resource_type {
                    4 => &wish.character_ids,
                    17 => &wish.memoria_ids,
                    22 => &wish.character_skin_ids,
                    _ => continue,
                };
                if chosen.contains(&card.id)
                    && !cards.iter().any(|c: &TutorialGachaCard| {
                        c.resource_type == card.resource_type && c.id == card.id
                    })
                {
                    cards.push(card.clone());
                }
            }
            let mut weighted = Vec::new();
            distribute_cards(&mut weighted, &cards, 100_000)?;
            weighted
        } else {
            weighted_gacha_cards(rules, &step, None)?
        }
    } else {
        weighted_gacha_cards(rules, gacha, wish_list)?
    };
    let mut drawn_rewards = Vec::new();
    let mut changed_characters = BTreeMap::new();
    let mut changed_pieces = BTreeMap::new();
    let mut changed_memorias = Vec::new();
    let mut changed_skins = BTreeMap::new();
    let mut character_indexes = Vec::new();
    let mut drew_character = false;
    let mut drew_memoria = false;
    let mut drew_duplicate = false;
    let mut tutorial_character_id = None;
    for _ in 0..button.draw_count {
        let selected = select_weighted_card(&weighted_cards)?;
        let mut reward = empty_message(proto, "blend.model.Reward")?;
        reward.set_field_by_name("type", Value::I32(selected.resource_type));
        reward.set_field_by_name("id", Value::I32(selected.id));
        reward.set_field_by_name("quantity", Value::I32(1));
        match selected.resource_type {
            4 if character_present(&resources, selected.id) => {
                drew_character = true;
                drew_duplicate = true;
                let (piece_count, stone_count) =
                    if let Some(bonus) = button.duplicate_pieces.iter().find(|b| {
                        b.character_ids.is_empty() || b.character_ids.contains(&selected.id)
                    }) {
                        (bonus.piece_count, bonus.generic_piece_count)
                    } else {
                        gacha_duplicate_counts(gacha, selected.id, selected.rarity)?
                    };
                let piece =
                    change_character_piece(proto, &mut resources, selected.id, piece_count)?;
                let stone = change_item(proto, &mut resources, 126, stone_count)?;
                changed_pieces.insert(selected.id, piece);
                changed_items.insert(126, stone);
                reward.set_field_by_name(
                    "other_rewards",
                    Value::List(vec![
                        Value::Message(resource_message(proto, 8, selected.id, piece_count)?),
                        Value::Message(resource_message(proto, 5, 126, stone_count)?),
                    ]),
                );
            }
            4 => {
                drew_character = true;
                tutorial_character_id = (gacha_id == 2).then_some(selected.id);
                let mut character = character_message(
                    proto,
                    i64::from(selected.id),
                    None,
                    Some(now),
                    selected.rarity,
                    10,
                )?;
                character.set_field_by_name("exp", Value::I32(0));
                upsert_character(&mut resources, character.clone(), false);
                changed_characters.insert(selected.id, character);
                character_indexes.push((i64::from(selected.id), None));
                reward.set_field_by_name("is_new", Value::Bool(true));
                if let Some(skin_id) = selected.skin_id.filter(|id| *id > 0) {
                    let mut skin = empty_message(proto, "blend.model.CharacterSkin")?;
                    skin.set_field_by_name("character_skin_id", Value::I32(skin_id));
                    skin.set_field_by_name("received_at", Value::Message(timestamp(proto, now)?));
                    let mut skins = message_list(&resources, "character_skins");
                    if !skins
                        .iter()
                        .any(|skin| i32_field(skin, "character_skin_id") == Some(skin_id))
                    {
                        skins.push(skin.clone());
                        resources.set_field_by_name(
                            "character_skins",
                            Value::List(skins.into_iter().map(Value::Message).collect()),
                        );
                        changed_skins.insert(skin_id, skin);
                        reward.set_field_by_name(
                            "other_rewards",
                            Value::List(vec![Value::Message(resource_message(
                                proto, 22, skin_id, 1,
                            )?)]),
                        );
                    }
                }
            }
            17 => {
                drew_memoria = true;
                let is_new = !message_list(&resources, "memorias")
                    .iter()
                    .any(|memoria| i32_field(memoria, "memoria_id") == Some(selected.id));
                let entity_id = next_entity_id(&resources)?;
                let mut memoria = empty_message(proto, "blend.model.Memoria")?;
                memoria.set_field_by_name("entity_id", Value::I32(entity_id));
                memoria.set_field_by_name("memoria_id", Value::I32(selected.id));
                memoria.set_field_by_name("limit_break", Value::I32(0));
                memoria.set_field_by_name("exp", Value::I32(0));
                memoria.set_field_by_name("is_locked", Value::Bool(false));
                memoria.set_field_by_name("received_at", Value::Message(timestamp(proto, now)?));
                let mut memorias = message_list(&resources, "memorias");
                memorias.push(memoria.clone());
                resources.set_field_by_name(
                    "memorias",
                    Value::List(memorias.into_iter().map(Value::Message).collect()),
                );
                changed_memorias.push(memoria);
                reward.set_field_by_name("entity_id", Value::I32(entity_id));
                reward.set_field_by_name("is_new", Value::Bool(is_new));
            }
            5 => {
                let item = change_item(proto, &mut resources, selected.id, 1)?;
                changed_items.insert(selected.id, item);
            }
            22 => {
                let is_new = !message_list(&resources, "character_skins")
                    .iter()
                    .any(|s| i32_field(s, "character_skin_id") == Some(selected.id));
                if is_new {
                    let mut skin = empty_message(proto, "blend.model.CharacterSkin")?;
                    skin.set_field_by_name("character_skin_id", Value::I32(selected.id));
                    skin.set_field_by_name("received_at", Value::Message(timestamp(proto, now)?));
                    home::put(
                        &mut resources,
                        "character_skins",
                        "character_skin_id",
                        skin.clone(),
                    );
                    changed_skins.insert(selected.id, skin);
                } else {
                    let mut delta = empty_message(proto, "blend.model.Resources")?;
                    let other = apply_quest_rewards(
                        proto,
                        &mut resources,
                        &mut delta,
                        &selected.duplicate_rewards,
                    )?;
                    for item in message_list(&delta, "items") {
                        changed_items.insert(
                            i32_field(&item, "item_id").ok_or(StateError::InvalidRequest)?,
                            item,
                        );
                    }
                    reward.set_field_by_name("other_rewards", Value::List(other));
                }
                reward.set_field_by_name("is_new", Value::Bool(is_new));
            }
            _ => return Err(StateError::InvalidRequest),
        }
        drawn_rewards.push(Value::Message(reward));
    }
    if !changed_characters.is_empty() {
        changed.set_field_by_name(
            "characters",
            Value::List(
                changed_characters
                    .into_values()
                    .map(Value::Message)
                    .collect(),
            ),
        );
    }
    if !changed_pieces.is_empty() {
        changed.set_field_by_name(
            "character_pieces",
            Value::List(changed_pieces.into_values().map(Value::Message).collect()),
        );
    }
    if !changed_items.is_empty() {
        changed.set_field_by_name(
            "items",
            Value::List(changed_items.into_values().map(Value::Message).collect()),
        );
    }
    if !changed_memorias.is_empty() {
        changed.set_field_by_name(
            "memorias",
            Value::List(changed_memorias.into_iter().map(Value::Message).collect()),
        );
    }
    if !changed_skins.is_empty() {
        changed.set_field_by_name(
            "character_skins",
            Value::List(changed_skins.into_values().map(Value::Message).collect()),
        );
    }
    if gacha_id == 2 {
        let mut status = status_message(&resources)?;
        status.set_field_by_name("tutorial_step", Value::I32(TUTORIAL_STEP_GACHA_COMPLETE));
        resources.set_field_by_name("status", Value::Message(status.clone()));
        changed.set_field_by_name("status", Value::Message(status));
    }
    let mut medal_rewards = Vec::new();
    let medal_quantity = button
        .medal_quantity
        .checked_add(button.additional_medal)
        .ok_or(StateError::InvalidRequest)?;
    if let Some(medal_id) = gacha.medal_id.filter(|_| medal_quantity > 0) {
        let item = change_item(proto, &mut resources, medal_id, medal_quantity)?;
        let mut items = message_list(&changed, "items");
        items.retain(|value| i32_field(value, "item_id") != Some(medal_id));
        items.push(item);
        changed.set_field_by_name(
            "items",
            Value::List(items.into_iter().map(Value::Message).collect()),
        );
        for quantity in [button.medal_quantity, button.additional_medal]
            .into_iter()
            .filter(|quantity| *quantity > 0)
        {
            let mut reward = empty_message(proto, "blend.model.Reward")?;
            reward.set_field_by_name("type", Value::I32(5));
            reward.set_field_by_name("id", Value::I32(medal_id));
            reward.set_field_by_name("quantity", Value::I32(quantity));
            medal_rewards.push(Value::Message(reward));
        }
    }
    let mut changed_tasks = Vec::new();
    if drew_character {
        let max_level = total_task_count(&resources, 47);
        changed_tasks.push(set_total_task_count(&mut resources, 47, max_level)?);
        let character_count = i32::try_from(message_list(&resources, "characters").len())
            .map_err(|_| StateError::InvalidRequest)?;
        changed_tasks.push(set_total_task_count(&mut resources, 85, character_count)?);
    }
    if drew_memoria {
        let max_level = total_task_count(&resources, 139).max(1);
        changed_tasks.push(set_total_task_count(&mut resources, 139, max_level)?);
    }
    if drew_duplicate {
        let stone_count = item_quantity(&resources, 126).ok_or(StateError::InvalidRequest)?;
        changed_tasks.push(set_total_task_count(&mut resources, 294, stone_count)?);
    }
    if let Some(condition_id) = tutorial_character_id.and_then(|id| match id {
        43102 => Some(1519),
        39901 => Some(1520),
        10101 => Some(1521),
        _ => None,
    }) {
        changed_tasks.push(set_total_task_count(&mut resources, condition_id, 1)?);
    }
    let draw_count = total_task_count(&resources, 137)
        .checked_add(button.draw_count)
        .ok_or(StateError::InvalidRequest)?;
    changed_tasks.push(set_total_task_count(&mut resources, 137, draw_count)?);
    set_changed_task_counts(&mut changed, changed_tasks);
    let next_state = GachaButtonState {
        gacha_id: i64::from(gacha_id),
        button_id: if step_up { -1 } else { i64::from(button_id) },
        execution_count: execution_count + 1,
        last_executed_at: now,
    };
    let gacha_message = gacha_message(proto, gacha, &[next_state])?;
    let mut response = empty_message(
        proto,
        if step_up {
            "blend.api.GachaStepUpExecuteResponse"
        } else {
            "blend.api.GachaExecuteResponse"
        },
    )?;
    response.set_field_by_name("drawn_rewards", Value::List(drawn_rewards));
    response.set_field_by_name("medal_rewards", Value::List(medal_rewards));
    response.set_field_by_name("changed_resources", Value::Message(changed));
    response.set_field_by_name("gacha", Value::Message(gacha_message));
    if let Some(wish_list) = wish_list {
        response.set_field_by_name(
            "wish_list_state",
            Value::Message(gacha_wish_list_message(proto, wish_list)?),
        );
    }
    Ok(GachaMutation {
        resources,
        response,
        character_indexes,
    })
}
