use super::prelude::*;

pub(crate) fn challenge_rows<'a>(rules: &'a ShopRules, table: &str) -> &'a [Json] {
    rules
        .item_challenge_rules
        .get(table)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

pub(crate) fn challenge_row<'a>(
    rules: &'a ShopRules,
    table: &str,
    id: i32,
) -> Result<&'a Json, StateError> {
    challenge_rows(rules, table)
        .iter()
        .find(|r| number(r, "id") == id)
        .ok_or(StateError::InvalidRequest)
}

pub(crate) fn challenge_tool_allowed(rules: &ShopRules, spec: &Json, kind: i32, id: i32) -> bool {
    let restrictions = values(spec, "item_challenge_restriction_ids");
    restrictions.is_empty()
        || restrictions
            .iter()
            .filter_map(Json::as_i64)
            .any(|restriction| {
                challenge_row(rules, "item_challenge_restriction", restriction as i32).is_ok_and(
                    |r| {
                        values(
                            r,
                            if kind == 6 {
                                "equipment_tool_ids"
                            } else {
                                "battle_tool_ids"
                            },
                        )
                        .contains(&Json::from(id))
                    },
                )
            })
}

pub(crate) fn challenge_parameters<'a>(
    rules: &'a ShopRules,
    spec: &Json,
) -> Result<Vec<(i32, &'a Json)>, StateError> {
    values(spec, "item_challenge_parameter_set_ids")
        .iter()
        .map(|id| {
            let id = id
                .as_i64()
                .and_then(|n| i32::try_from(n).ok())
                .ok_or(StateError::InvalidRequest)?;
            let set = challenge_row(rules, "item_challenge_parameter_set", id)?;
            Ok((
                id,
                challenge_row(
                    rules,
                    "item_challenge_parameter",
                    number(set, "item_challenge_parameter_id"),
                )?,
            ))
        })
        .collect()
}

// Client VA 0x181E80710: (slot/filter + trait-category points) * rarity * total-rank * event bonus / 100^3.
pub(crate) fn challenge_tool_points(
    rules: &ShopRules,
    spec: &Json,
    param: &Json,
    kind: i32,
    tool: &Json,
    traits: &[(i32, i32)],
) -> Result<i64, StateError> {
    let mut base = if kind == 6 {
        number(
            &param["equipment_tool"],
            &format!("slot{}", number(tool, "slot_type")),
        )
    } else {
        values(tool, "trait_filter_ids")
            .iter()
            .map(|filter| {
                values(param, "battle_tools")
                    .iter()
                    .find(|p| p["trait_filter_id"] == *filter)
                    .map(|p| number(p, "value"))
                    .unwrap_or(0)
            })
            .sum()
    };
    for (category, _) in traits {
        base += values(param, "traits")
            .iter()
            .find(|p| number(p, "trait_category_id") == *category)
            .map(|p| number(p, "value"))
            .unwrap_or(0);
    }
    let rarity = values(param, "rarities")
        .iter()
        .find(|r| number(r, "tool_rarity_id") == number(tool, "rarity"))
        .map(|r| number(r, "coefficient"))
        .unwrap_or(100);
    let rank = traits.iter().map(|(_, rank)| rank).sum::<i32>();
    let rank = challenge_row(rules, "trait_rank_total", rank + 1)
        .map(|r| {
            number(
                r,
                if kind == 6 {
                    "item_challenge_equipment_tool_coefficient"
                } else {
                    "item_challenge_battle_tool_coefficient"
                },
            )
        })
        .unwrap_or(100);
    let bonus = values(spec, "item_challenge_special_bonus_ids")
        .iter()
        .filter_map(Json::as_i64)
        .filter_map(|id| challenge_row(rules, "item_challenge_special_bonus", id as i32).ok())
        .find(|r| {
            values(
                r,
                if kind == 6 {
                    "equipment_tool_ids"
                } else {
                    "battle_tool_ids"
                },
            )
            .contains(&tool["id"])
        })
        .map(|r| number(r, "coefficient"))
        .unwrap_or(100);
    Ok(i64::from(base) * i64::from(rarity) * i64::from(rank) * i64::from(bonus) / 1_000_000)
}

pub(crate) fn challenge_reference_points(
    rules: &ShopRules,
    spec: &Json,
    params: &[(i32, &Json)],
) -> Result<i64, StateError> {
    let mut best = 0;
    for kind in [6, 14] {
        let table = if kind == 6 {
            "equipment_tool"
        } else {
            "battle_tool"
        };
        for tool in challenge_rows(rules, table)
            .iter()
            .filter(|tool| challenge_tool_allowed(rules, spec, kind, number(tool, "id")))
        {
            for category in 1..=4 {
                let traits = vec![(category, 5); if kind == 6 { 2 } else { 3 }];
                let total = params
                    .iter()
                    .map(|(_, param)| {
                        challenge_tool_points(rules, spec, param, kind, tool, &traits)
                    })
                    .collect::<Result<Vec<_>, _>>()?
                    .iter()
                    .sum::<i64>();
                best = best.max(total);
            }
        }
    }
    best.checked_mul(i64::from(number(spec, "max_tool_count")))
        .filter(|v| *v > 0)
        .ok_or(StateError::InvalidRequest)
}

pub(crate) fn challenge_score_variation() -> Result<i64, StateError> {
    Ok(90 + i64::from(random_below(21)?))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn item_challenge(
    proto: &ProtoRegistry,
    rules: &ShopRules,
    home_rules: &home::HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    response: &mut DynamicMessage,
    route: &str,
    request: &DynamicMessage,
    now: i64,
) -> Result<(), StateError> {
    let id = i32_field(request, "item_challenge_id").ok_or(StateError::InvalidRequest)?;
    let spec = rules
        .item_challenges
        .iter()
        .find(|r| number(r, "id") == id)
        .ok_or(StateError::InvalidRequest)?;
    if !home::in_period(spec["start_at"].as_i64(), spec["end_at"].as_i64(), now) {
        return Err(StateError::OutOfSchedule);
    }
    let old = message_list(resources, "item_challenges")
        .into_iter()
        .find(|r| i32_field(r, "item_challenge_id") == Some(id));
    let mut state = old.unwrap_or(empty_message(proto, "blend.model.ItemChallenge")?);
    state.set_field_by_name("item_challenge_id", Value::I32(id));
    let reward_rows = rules
        .item_challenge_rewards
        .iter()
        .filter(|r| number(r, "item_challenge_id") == id)
        .collect::<Vec<_>>();
    if route.ends_with("/execute") {
        let inputs = message_list(request, "challenge_tools");
        if inputs.is_empty() || inputs.len() > number(spec, "max_tool_count") as usize {
            return Err(StateError::InvalidRequest);
        }
        let params = challenge_parameters(rules, spec)?;
        let mut totals = vec![0_i64; params.len()];
        let mut seen = std::collections::BTreeSet::new();
        for input in inputs {
            let kind = i32_field(&input, "type").ok_or(StateError::InvalidRequest)?;
            let entity = i32_field(&input, "entity_id").ok_or(StateError::InvalidRequest)?;
            if !seen.insert((kind, entity)) {
                return Err(StateError::InvalidRequest);
            }
            let table = match kind {
                6 => "equipment_tool",
                14 => "battle_tool",
                _ => return Err(StateError::InvalidRequest),
            };
            let tool = message_list(resources, &format!("{table}s"))
                .into_iter()
                .find(|r| i32_field(r, "entity_id") == Some(entity))
                .ok_or(StateError::InvalidRequest)?;
            let tool_id = i32_field(&tool, "tool_id").ok_or(StateError::InvalidRequest)?;
            if !challenge_tool_allowed(rules, spec, kind, tool_id) {
                return Err(StateError::InvalidRequest);
            }
            let traits = message_list(&tool, "traits")
                .iter()
                .map(|t| {
                    let rank = i32_field(t, "rank")
                        .filter(|r| (1..=5).contains(r))
                        .ok_or(StateError::InvalidRequest)?;
                    let trait_spec = challenge_row(
                        rules,
                        &format!("{table}_trait"),
                        i32_field(t, "id").ok_or(StateError::InvalidRequest)?,
                    )?;
                    Ok((number(trait_spec, "category_id"), rank))
                })
                .collect::<Result<Vec<_>, StateError>>()?;
            let tool_spec = challenge_row(rules, table, tool_id)?;
            for (i, (_, param)) in params.iter().enumerate() {
                totals[i] += challenge_tool_points(rules, spec, param, kind, tool_spec, &traits)?;
            }
        }
        let failed = params
            .iter()
            .zip(&totals)
            .filter(|(_, total)| **total == 0)
            .map(|((id, _), _)| Value::I32(*id))
            .collect::<Vec<_>>();
        // Owner-approved local policy: summed parameters, uniform integer +/-10%, zero-parameter failure.
        // Normalize to the event reward range; raw parameter units cannot reach even the first reward in several events.
        let ceiling = reward_rows
            .iter()
            .map(|r| number(r, "score"))
            .max()
            .ok_or(StateError::InvalidRequest)?;
        let score = if failed.is_empty() {
            checked_i32(
                totals.iter().sum::<i64>() * i64::from(ceiling) * challenge_score_variation()?
                    / challenge_reference_points(rules, spec, &params)?
                    / 100,
            )?
        } else {
            0
        };
        state.set_field_by_name(
            "high_score",
            Value::I32(score.max(i32_field(&state, "high_score").unwrap_or(0))),
        );
        response.set_field_by_name("score", Value::I32(score));
        response.set_field_by_name("failed_parameter_set_ids", Value::List(failed));
    } else {
        let high = i32_field(&state, "high_score").unwrap_or(0);
        let received = i32_field(&state, "received_score").unwrap_or(0);
        let mut maximum = received;
        let mut rewards = Vec::new();
        for row in reward_rows
            .iter()
            .filter(|r| number(r, "score") > received && number(r, "score") <= high)
        {
            maximum = maximum.max(number(row, "score"));
            rewards.extend(
                serde_json::from_value::<Vec<TutorialReward>>(row["rewards"].clone())
                    .map_err(|e| StateError::MasterData(e.to_string()))?,
            );
        }
        if rewards.is_empty() {
            return Err(StateError::InvalidRequest);
        }
        response.set_field_by_name(
            "rewards",
            Value::List(home::grant(
                proto,
                home_rules,
                resources,
                changed,
                &atelier::aggregate_rewards(rewards)?,
                now,
            )?),
        );
        state.set_field_by_name("received_score", Value::I32(maximum));
    }
    home::put(
        resources,
        "item_challenges",
        "item_challenge_id",
        state.clone(),
    );
    home::put(changed, "item_challenges", "item_challenge_id", state);
    Ok(())
}
