use super::prelude::*;

pub(crate) fn gift_quality(
    rules: &ActivityRules,
    tool: &DynamicMessage,
    resource_type: i32,
) -> Result<i32, StateError> {
    // Native KHKKADNDINF::CNKJFAMLOEI / NLEOFPOKOPP::CNKJFAMLOEI
    // sum the quality table entries selected by tool group and each trait rank.
    let data = &rules.present_quality[resource_type.to_string()];
    let id = i32_field(tool, "tool_id").ok_or(StateError::InvalidRequest)?;
    let group = data["tools"][id.to_string()]
        .as_i64()
        .ok_or(StateError::InvalidRequest)?;
    let mut quality = 0i64;
    for trait_ in message_list(tool, "traits") {
        let id = i32_field(&trait_, "id").ok_or(StateError::InvalidRequest)?;
        let rank = i32_field(&trait_, "rank")
            .filter(|r| *r > 0)
            .ok_or(StateError::InvalidRequest)?;
        let trait_group = data["traits"][id.to_string()][(rank - 1) as usize]
            .as_i64()
            .ok_or(StateError::InvalidRequest)?;
        let value = data["values"][format!("{group}:{trait_group}")]
            .as_i64()
            .ok_or(StateError::InvalidRequest)?;
        quality = quality
            .checked_add(value)
            .ok_or(StateError::InvalidRequest)?;
    }
    checked_i32(quality)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn present_execute(
    proto: &ProtoRegistry,
    rules: &ActivityRules,
    home_rules: &home::HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    response: &mut DynamicMessage,
    request: &DynamicMessage,
    now: i64,
) -> Result<(), StateError> {
    let id = i32_field(request, "present_id").ok_or(StateError::InvalidRequest)?;
    let spec = row(rules, "present", id)?;
    eligible(spec, resources, now)?;
    // Branch-selection presents need a recovered selection contract. Do not assign
    // an arbitrary character or consume gifts for a branch we cannot resolve.
    let character = spec["character_id"]
        .as_i64()
        .filter(|id| *id > 0)
        .ok_or(StateError::InvalidRequest)? as i32;
    let gifts = message_list(request, "gifted_tools");
    if gifts.is_empty() || gifts.len() > 1000 {
        return Err(StateError::InvalidRequest);
    }
    let mut points = rows(rules, "present_friendship_point")
        .iter()
        .filter(|r| number(r, "present_id") == id)
        .collect::<Vec<_>>();
    points.sort_by_key(|r| number(r, "max_quality"));
    let mut seen = BTreeSet::new();
    let mut consumed = Vec::new();
    let mut gained = 0i32;
    for gift in gifts {
        let kind = i32_or_enum_field(&gift, "type").ok_or(StateError::InvalidRequest)?;
        let entity = i32_field(&gift, "entity_id").ok_or(StateError::InvalidRequest)?;
        if !matches!(kind, 6 | 14)
            || !seen.insert((kind, entity))
            || atelier::assigned_tool(resources, kind, entity)
        {
            return Err(StateError::InvalidRequest);
        }
        let field = atelier::tool_field(kind)?;
        let tool = message_list(resources, field)
            .into_iter()
            .find(|t| i32_field(t, "entity_id") == Some(entity))
            .ok_or(StateError::InvalidRequest)?;
        if bool_field(&tool, "is_locked") {
            return Err(StateError::InvalidRequest);
        }
        let quality = gift_quality(rules, &tool, kind)?;
        let tier = points
            .iter()
            .find(|p| quality <= number(p, "max_quality"))
            .ok_or(StateError::InvalidRequest)?;
        let field_name = if kind == 6 {
            "equipment_tools"
        } else {
            "battle_tools"
        };
        let value = values(tier, field_name)
            .iter()
            .find(|p| i32_field(&tool, "tool_id") == Some(number(p, "id")))
            .map(|p| number(p, "point"))
            .filter(|p| *p > 0)
            .ok_or(StateError::InvalidRequest)?;
        gained = gained
            .checked_add(value)
            .ok_or(StateError::InvalidRequest)?;
        consumed.push((kind, entity));
    }
    let mut state = message_list(resources, "present_states")
        .into_iter()
        .find(|s| i32_field(s, "present_id") == Some(id))
        .unwrap_or(empty_message(proto, "blend.model.PresentState")?);
    let total = i32_field(&state, "total_friendship_point")
        .unwrap_or(0)
        .checked_add(gained)
        .ok_or(StateError::InvalidRequest)?;
    let mut levels = rows(rules, "present_closeness")
        .iter()
        .filter(|r| number(r, "present_id") == id)
        .collect::<Vec<_>>();
    levels.sort_by_key(|r| number(r, "closeness"));
    let last = levels.last().ok_or(StateError::InvalidRequest)?;
    let last_cost = number(last, "friendship_point");
    if last_cost <= 0 {
        return Err(StateError::InvalidRequest);
    }
    // GetClosenessProgress(0x184A4EEB0) consumes successive thresholds from
    // cumulative total points; after the last row it repeats that threshold.
    let mut rest = total;
    let mut closeness = 1;
    for level in &levels {
        let cost = number(level, "friendship_point");
        if cost < 0 {
            return Err(StateError::InvalidRequest);
        }
        if rest < cost {
            break;
        }
        rest -= cost;
        closeness = number(level, "closeness");
    }
    if closeness == number(last, "closeness") {
        closeness = closeness
            .checked_add(rest / last_cost)
            .ok_or(StateError::InvalidRequest)?;
        rest %= last_cost;
    }
    let received = i32_field(&state, "received_closeness_rewards").unwrap_or(0);
    let mut rewards = Vec::new();
    for level in &levels {
        let level_number = number(level, "closeness");
        if received < level_number && level_number <= closeness {
            for reward in values(level, "rewards") {
                rewards.push(
                    serde_json::from_value::<TutorialReward>(reward.clone())
                        .map_err(|e| StateError::MasterData(e.to_string()))?,
                );
            }
        }
    }
    // Only explicit reward rows are awarded. Unbounded extra rewards past the
    // last row are not inferred from the UI's unbounded level calculation.
    state.set_field_by_name("present_id", Value::I32(id));
    state.set_field_by_name("character_id", Value::I32(character));
    state.set_field_by_name("total_friendship_point", Value::I32(total));
    state.set_field_by_name("friendship_point", Value::I32(rest));
    state.set_field_by_name("closeness", Value::I32(closeness));
    state.set_field_by_name(
        "received_closeness_rewards",
        Value::I32(closeness.max(received)),
    );
    let mut equipment = Vec::new();
    let mut battle = Vec::new();
    for (kind, id) in consumed {
        atelier::remove_entity(resources, atelier::tool_field(kind)?, id)?;
        if kind == 6 {
            equipment.push(id);
        } else {
            battle.push(id);
        }
    }
    save(resources, changed, "present_states", "present_id", state);
    let awarded = home::grant(proto, home_rules, resources, changed, &rewards, now)?;
    response.set_field_by_name("rewards", Value::List(awarded));
    response.set_field_by_name(
        "deleted_resources",
        Value::Message(atelier::deleted_resources(proto, &equipment, &battle, &[])?),
    );
    Ok(())
}
