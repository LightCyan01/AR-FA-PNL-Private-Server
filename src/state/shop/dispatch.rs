use super::prelude::*;

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply(
    proto: &ProtoRegistry,
    rules: &ShopRules,
    home_rules: &home::HomeRules,
    master: &DynamicMessage,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    response: &mut DynamicMessage,
    state: &mut ShopState,
    route: &str,
    request: &DynamicMessage,
    payment_provider_url: &str,
    now: i64,
) -> Result<(), StateError> {
    match route {
        "/shop/gem_list" => response.set_field_by_name(
            "gem_list",
            Value::Message(gem_list(
                proto, master, resources, changed, now, rules, state,
            )?),
        ),
        "/shop/purchase" => {
            if request.has_field_by_name("shop_product_limited_discount_id") {
                return Err(StateError::InvalidRequest);
            }
            let product = product(
                master,
                i32_field(request, "shop_product_id").ok_or(StateError::InvalidRequest)?,
            )?;
            response.set_field_by_name(
                "rewards",
                Value::List(purchase_product(
                    proto,
                    rules,
                    home_rules,
                    resources,
                    changed,
                    &product,
                    i32_field(request, "quantity").unwrap_or(0),
                    now,
                )?),
            );
        }
        "/shop/random_shop/list" | "/shop/random_shop/refresh" | "/shop/random_shop/purchase" => {
            let shop_id = if route.ends_with("purchase") {
                i32_field(
                    &product(
                        master,
                        i32_field(request, "shop_product_id").ok_or(StateError::InvalidRequest)?,
                    )?,
                    "shop_id",
                )
                .ok_or(StateError::InvalidRequest)?
            } else {
                i32_field(request, "shop_id").ok_or(StateError::InvalidRequest)?
            };
            let rule = rules
                .shops
                .iter()
                .find(|row| {
                    row.id == shop_id
                        && row.shop_type == 5
                        && home::in_period(row.start_at, row.end_at, now)
                })
                .ok_or(StateError::OutOfSchedule)?;
            let random = state.random_shops.entry(shop_id).or_default();
            if random.product_ids.is_empty() || now >= random.refresh_at {
                refresh_random_shop(master, rule, random, now)?;
            }
            if route.ends_with("refresh") {
                if random.refresh_count
                    >= rule.lineup_refresh_costs.iter().map(|row| row.count).sum()
                {
                    return Err(StateError::InvalidRequest);
                }
                let cost = rule
                    .lineup_refresh_costs
                    .iter()
                    .find(|row| random.refresh_count < row.count)
                    .or_else(|| rule.lineup_refresh_costs.last())
                    .map(|row| row.gem_cost)
                    .unwrap_or(0);
                pay(proto, resources, changed, 1, 1, cost)?;
                random.refresh_count += 1;
                refresh_random_shop(master, rule, random, now)?;
            } else if route.ends_with("purchase") {
                let id = i32_field(request, "shop_product_id").ok_or(StateError::InvalidRequest)?;
                if !random.product_ids.contains(&id) {
                    return Err(StateError::InvalidRequest);
                }
                let row = product(master, id)?;
                let quantity = i32_field(request, "quantity").unwrap_or(0);
                purchase_product(
                    proto, rules, home_rules, resources, changed, &row, quantity, now,
                )?;
                *random.purchased.entry(id).or_default() += quantity;
            }
            response.set_field_by_name(
                "random_shop_state",
                Value::Message(random_shop_message(proto, random, shop_id)?),
            );
        }
        "/shop/piece_exchange" => {
            let character_id =
                i32_field(request, "character_id").ok_or(StateError::InvalidRequest)?;
            let quantity = i32_field(request, "quantity")
                .filter(|v| *v > 0)
                .ok_or(StateError::InvalidRequest)?;
            if rules
                .piece_exchange_unavailable_character_ids
                .contains(&character_id)
                || !message_list(resources, "characters")
                    .iter()
                    .any(|row| i32_field(row, "character_id") == Some(character_id))
            {
                return Err(StateError::InvalidRequest);
            }
            let old = message_list(resources, "shop_piece_exchange_states")
                .into_iter()
                .find(|row| i32_field(row, "character_id") == Some(character_id))
                .and_then(|row| i32_field(&row, "purchased_count"))
                .unwrap_or(0);
            let cost = (old..old + quantity).try_fold(0i32, |sum, count| {
                let unit = rules
                    .piece_exchange_quantity_table
                    .iter()
                    .find(|row| row.purchase_count.is_none_or(|end| count < end))
                    .map(|row| row.quantity)
                    .ok_or(StateError::InvalidRequest)?;
                sum.checked_add(unit).ok_or(StateError::InvalidRequest)
            })?;
            pay(
                proto,
                resources,
                changed,
                5,
                rules.piece_exchange_item_id,
                cost,
            )?;
            let piece = change_character_piece(proto, resources, character_id, quantity)?;
            home::put(changed, "character_pieces", "character_id", piece);
            let mut value = empty_message(proto, "blend.model.ShopPieceExchangeState")?;
            value.set_field_by_name("character_id", Value::I32(character_id));
            value.set_field_by_name("purchased_count", Value::I32(old + quantity));
            home::put(
                resources,
                "shop_piece_exchange_states",
                "character_id",
                value.clone(),
            );
            home::put(changed, "shop_piece_exchange_states", "character_id", value);
        }
        "/purchase/session_start" => {
            let id = i32_field(request, "shop_product_id").ok_or(StateError::InvalidRequest)?;
            let row = product(master, id)?;
            let old = purchased_count(resources, id);
            if !local_catalog_product(&row, rules)
                || i32_field(&row, "limit_count").is_some_and(|limit| limit > 0 && old >= limit)
                || !purchase_steps(&row)
                    .iter()
                    .any(|step| optional_i32_field(step, "store_product_id").is_some())
            {
                return Err(StateError::InvalidRequest);
            }
            let session_id = Uuid::new_v4().to_string();
            let transaction_id = format!("local-{session_id}");
            state.sessions.insert(
                session_id.clone(),
                PurchaseSession {
                    shop_product_id: id,
                    transaction_id: Some(transaction_id.clone()),
                    verified: false,
                },
            );
            response.set_field_by_name("purchase_session_id", Value::String(session_id));
            response.set_field_by_name(
                "transaction_id",
                Value::Message(string_wrapper(proto, transaction_id)?),
            );
        }
        "/purchase/session_publish" => {
            let id = request
                .get_field_by_name("purchase_session_id")
                .and_then(|v| v.as_str().map(str::to_owned))
                .ok_or(StateError::InvalidRequest)?;
            let session = state
                .sessions
                .get_mut(&id)
                .ok_or(StateError::InvalidRequest)?;
            session.transaction_id = wrapped_string(request, "transaction_id");
        }
        "/purchase/verify" => {
            if !payment_provider_url.is_empty() {
                return Err(StateError::InvalidRequest);
            }
            let id = request
                .get_field_by_name("purchase_session_id")
                .and_then(|v| v.as_str().map(str::to_owned))
                .ok_or(StateError::InvalidRequest)?;
            let transaction = request
                .get_field_by_name("transaction_id")
                .and_then(|v| v.as_str().map(str::to_owned))
                .filter(|v| !v.is_empty())
                .ok_or(StateError::InvalidRequest)?;
            if state.sessions.values().any(|existing| {
                existing.verified
                    && existing.transaction_id.as_deref() == Some(transaction.as_str())
            }) {
                return Err(StateError::InvalidRequest);
            }
            let session = state
                .sessions
                .get_mut(&id)
                .filter(|session| !session.verified)
                .ok_or(StateError::InvalidRequest)?;
            session.transaction_id = Some(transaction);
            let row = product(master, session.shop_product_id)?;
            let steps = purchase_steps(&row);
            let old_count = purchased_count(resources, session.shop_product_id);
            let step = steps
                .get((old_count as usize).min(steps.len().saturating_sub(1)))
                .cloned()
                .ok_or(StateError::InvalidRequest)?;
            let store_id =
                optional_i32_field(&step, "store_product_id").ok_or(StateError::InvalidRequest)?;
            let store = store_product(proto, store_id, gem_amount(&step))?;
            for (resource_type, field) in [(2, "money_amount"), (1, "free_amount")] {
                let amount = i32_field(&store, field).unwrap_or_default();
                if amount == 0 {
                    continue;
                }
                home::grant(
                    proto,
                    home_rules,
                    resources,
                    changed,
                    &[TutorialReward {
                        resource_type,
                        id: 1,
                        quantity: amount,
                        resource_params: None,
                    }],
                    now,
                )?;
            }
            if let Some(reward_set_id) = optional_i32_field(&step, "reward_set_id") {
                home::grant(
                    proto,
                    home_rules,
                    resources,
                    changed,
                    reward_set(rules, reward_set_id)?,
                    now,
                )?;
            }
            if let Some(pass_id) = optional_i32_field(&step, "daily_pass_id") {
                let pass = rules
                    .daily_passes
                    .iter()
                    .find(|row| row.id == pass_id)
                    .ok_or(StateError::InvalidRequest)?;
                let mut value = message_list(resources, "daily_pass_states")
                    .into_iter()
                    .find(|row| i32_field(row, "daily_pass_id") == Some(pass_id))
                    .unwrap_or(empty_message(proto, "blend.model.DailyPassState")?);
                let started = timestamp_seconds(&value, "started_at").unwrap_or(now);
                let expires = timestamp_seconds(&value, "expires_at")
                    .unwrap_or(now)
                    .max(now);
                value.set_field_by_name("daily_pass_id", Value::I32(pass_id));
                value.set_field_by_name("started_at", Value::Message(timestamp(proto, started)?));
                value.set_field_by_name(
                    "expires_at",
                    Value::Message(timestamp(proto, expires + i64::from(pass.days) * 86400)?),
                );
                home::put(
                    resources,
                    "daily_pass_states",
                    "daily_pass_id",
                    value.clone(),
                );
                home::put(changed, "daily_pass_states", "daily_pass_id", value);
            }
            set_product_count(proto, resources, changed, &row, old_count + 1, now)?;
            session.verified = true;
        }
        "/shop/receive_first_purchase_bonus" => {
            if !state.sessions.values().any(|session| session.verified) {
                return Err(StateError::InvalidRequest);
            }
            let old = resources
                .get_field_by_name("shop_first_purchase_bonus_state")
                .and_then(|v| v.as_message().cloned())
                .and_then(|v| v.get_field_by_name("is_received").and_then(|x| x.as_bool()))
                .unwrap_or(false);
            if old {
                return Err(StateError::InvalidRequest);
            }
            response.set_field_by_name(
                "rewards",
                Value::List(home::grant(
                    proto,
                    home_rules,
                    resources,
                    changed,
                    &[TutorialReward {
                        resource_type: 17,
                        id: rules.shop_first_purchase_bonus_memoria_id,
                        quantity: 1,
                        resource_params: None,
                    }],
                    now,
                )?),
            );
            let mut value = empty_message(proto, "blend.model.ShopFirstPurchaseBonusState")?;
            value.set_field_by_name("is_received", Value::Bool(true));
            resources.set_field_by_name(
                "shop_first_purchase_bonus_state",
                Value::Message(value.clone()),
            );
            changed.set_field_by_name("shop_first_purchase_bonus_state", Value::Message(value));
        }
        "/daily_pass/receive" => daily_pass(
            proto, rules, home_rules, resources, changed, response, request, false, now,
        )?,
        "/daily_pass/bulk_receive" => daily_pass(
            proto, rules, home_rules, resources, changed, response, request, true, now,
        )?,
        "/growth_pack/purchase" => {
            let id = i32_field(request, "growth_pack_id").ok_or(StateError::InvalidRequest)?;
            let pack = rules
                .growth_packs
                .iter()
                .find(|row| row.id == id && home::in_period(row.start_at, row.end_at, now))
                .ok_or(StateError::OutOfSchedule)?;
            if message_list(resources, "growth_pack_states")
                .iter()
                .any(|row| i32_field(row, "growth_pack_id") == Some(id))
            {
                return Err(StateError::InvalidRequest);
            }
            pay(
                proto,
                resources,
                changed,
                pack.premium_cost.resource_type,
                pack.premium_cost.id,
                pack.premium_cost.quantity,
            )?;
            let mut value = empty_message(proto, "blend.model.GrowthPackState")?;
            value.set_field_by_name("growth_pack_id", Value::I32(id));
            home::put(
                resources,
                "growth_pack_states",
                "growth_pack_id",
                value.clone(),
            );
            home::put(changed, "growth_pack_states", "growth_pack_id", value);
        }
        "/growth_pack/point_purchase" => {
            let id = i32_field(request, "growth_pack_id").ok_or(StateError::InvalidRequest)?;
            let point = i32_field(request, "point")
                .filter(|value| {
                    *value >= rules.mission_pass_point_purchase_min_lot
                        && *value % rules.mission_pass_point_purchase_min_lot == 0
                })
                .ok_or(StateError::InvalidRequest)?;
            let pack = rules
                .growth_packs
                .iter()
                .find(|row| row.id == id && row.pack_type == 3)
                .ok_or(StateError::InvalidRequest)?;
            let lots = point / rules.mission_pass_point_purchase_min_lot;
            pay(
                proto,
                resources,
                changed,
                1,
                1,
                lots.checked_mul(rules.mission_pass_point_purchase_one_lot_cost)
                    .ok_or(StateError::InvalidRequest)?,
            )?;
            home::update_task(
                resources,
                changed,
                pack.total_task_condition_id,
                total_task_count(resources, pack.total_task_condition_id)
                    .checked_add(point)
                    .ok_or(StateError::InvalidRequest)?,
            )?;
        }
        "/growth_pack/receive" => growth_receive(
            proto, rules, home_rules, resources, changed, response, request, false, now,
        )?,
        "/growth_pack/bulk_receive" => growth_receive(
            proto, rules, home_rules, resources, changed, response, request, true, now,
        )?,
        "/special_offer/purchase" => {
            let id = i32_field(request, "special_offer_id").ok_or(StateError::InvalidRequest)?;
            let offer = rules
                .special_offers
                .iter()
                .find(|row| row.id == id && row.start_at.is_none_or(|start| start <= now))
                .ok_or(StateError::OutOfSchedule)?;
            if message_list(resources, "special_offer_states")
                .iter()
                .any(|row| {
                    i32_field(row, "special_offer_id") == Some(id)
                        && timestamp_seconds(row, "expires_at").is_some_and(|end| now < end)
                })
            {
                return Err(StateError::InvalidRequest);
            }
            pay(
                proto,
                resources,
                changed,
                offer.cost.resource_type,
                offer.cost.id,
                offer.cost.quantity,
            )?;
            let mut value = empty_message(proto, "blend.model.SpecialOfferState")?;
            value.set_field_by_name("special_offer_id", Value::I32(id));
            value.set_field_by_name(
                "expires_at",
                Value::Message(timestamp(proto, now + i64::from(offer.days) * 86400)?),
            );
            home::put(
                resources,
                "special_offer_states",
                "special_offer_id",
                value.clone(),
            );
            home::put(changed, "special_offer_states", "special_offer_id", value);
        }
        "/item/bundle_open" => {
            let id = i32_field(request, "item_id").ok_or(StateError::InvalidRequest)?;
            let bundle = rules
                .bundle_item_sets
                .iter()
                .find(|row| row.item_id == id && row.is_direct)
                .ok_or(StateError::InvalidRequest)?;
            pay(proto, resources, changed, 5, id, 1)?;
            let selected = if bundle.is_random {
                std::slice::from_ref(
                    bundle
                        .items
                        .get(random_below(bundle.items.len() as u32)? as usize)
                        .ok_or(StateError::InvalidRequest)?,
                )
            } else {
                bundle.items.as_slice()
            };
            let rewards: Vec<_> = selected
                .iter()
                .map(|item| TutorialReward {
                    resource_type: 5,
                    id: item.id,
                    quantity: item.quantity,
                    resource_params: None,
                })
                .collect();
            response.set_field_by_name(
                "rewards",
                Value::List(home::grant(
                    proto, home_rules, resources, changed, &rewards, now,
                )?),
            );
        }
        "/shop/box_gacha/execute" => box_gacha(
            proto, rules, home_rules, resources, changed, response, request, now,
        )?,
        "/shop/wheel" => {
            let id = i32_field(request, "shop_wheel_id").ok_or(StateError::InvalidRequest)?;
            let spec = rules
                .shop_wheels
                .iter()
                .find(|r| number(r, "id") == id)
                .ok_or(StateError::InvalidRequest)?;
            if !home::in_period(spec["start_at"].as_i64(), spec["end_at"].as_i64(), now) {
                return Err(StateError::OutOfSchedule);
            }
            let counter = state.wheels.entry(id).or_insert((home::day(now), 0));
            if counter.0 != home::day(now) {
                *counter = (home::day(now), 0);
            }
            if counter.1 >= number(spec, "daily_count_limit") {
                return Err(StateError::InvalidRequest);
            }
            let (kind, item, min, max) = wheel_reward(id)?;
            let amount = min + random_below((max - min + 1) as u32)? as i32;
            counter.1 += 1;
            let awarded = home::grant(
                proto,
                home_rules,
                resources,
                changed,
                &[TutorialReward {
                    resource_type: kind,
                    id: item,
                    quantity: amount,
                    resource_params: None,
                }],
                now,
            )?;
            response.set_field_by_name("rewards", Value::List(awarded));
            response.set_field_by_name("rarity_id", Value::I32(if amount == max { 3 } else { 1 }));
            response.set_field_by_name("drama_id", Value::I32(1));
            response.set_field_by_name(
                "gem_list",
                Value::Message(gem_list(
                    proto, master, resources, changed, now, rules, state,
                )?),
            );
        }
        "/item_challenge/execute" | "/item_challenge/reward_receive" => item_challenge(
            proto, rules, home_rules, resources, changed, response, route, request, now,
        )?,
        _ => return Err(StateError::InvalidRequest),
    }
    Ok(())
}
