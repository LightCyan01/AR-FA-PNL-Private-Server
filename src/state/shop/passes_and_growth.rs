use super::prelude::*;

#[allow(clippy::too_many_arguments)]
pub(crate) fn daily_pass(
    proto: &ProtoRegistry,
    rules: &ShopRules,
    home_rules: &home::HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    response: &mut DynamicMessage,
    request: &DynamicMessage,
    bulk: bool,
    now: i64,
) -> Result<(), StateError> {
    let id = i32_field(request, "daily_pass_id").ok_or(StateError::InvalidRequest)?;
    let rule = rules
        .daily_passes
        .iter()
        .find(|row| row.id == id)
        .ok_or(StateError::InvalidRequest)?;
    let mut state = message_list(resources, "daily_pass_states")
        .into_iter()
        .find(|row| i32_field(row, "daily_pass_id") == Some(id))
        .ok_or(StateError::InvalidRequest)?;
    let started = timestamp_seconds(&state, "started_at").ok_or(StateError::InvalidRequest)?;
    let expires = timestamp_seconds(&state, "expires_at").ok_or(StateError::InvalidRequest)?;
    if now >= expires {
        return Err(StateError::OutOfSchedule);
    }
    let due = ((home::day(now) - home::day(started) + 1) as i32).clamp(0, rule.days);
    let mut received = i32_list(&state, "received_days");
    let targets = if bulk {
        (1..=due)
            .filter(|day| !received.contains(day))
            .collect::<Vec<_>>()
    } else {
        vec![i32_field(request, "target_day")
            .filter(|day| *day > 0 && *day <= due && !received.contains(day))
            .ok_or(StateError::InvalidRequest)?]
    };
    let mut awarded = Vec::new();
    for target in targets {
        received.push(target);
        awarded.extend(home::grant(
            proto,
            home_rules,
            resources,
            changed,
            std::slice::from_ref(&rule.daily_pass_reward),
            now,
        )?);
    }
    received.sort_unstable();
    state.set_field_by_name(
        "login_days",
        Value::List((1..=due).map(Value::I32).collect()),
    );
    state.set_field_by_name(
        "received_days",
        Value::List(received.into_iter().map(Value::I32).collect()),
    );
    home::put(
        resources,
        "daily_pass_states",
        "daily_pass_id",
        state.clone(),
    );
    home::put(changed, "daily_pass_states", "daily_pass_id", state);
    response.set_field_by_name("rewards", Value::List(awarded));
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn growth_receive(
    proto: &ProtoRegistry,
    rules: &ShopRules,
    home_rules: &home::HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    response: &mut DynamicMessage,
    request: &DynamicMessage,
    bulk: bool,
    now: i64,
) -> Result<(), StateError> {
    let pack_id = if bulk {
        i32_field(request, "growth_pack_id").ok_or(StateError::InvalidRequest)?
    } else {
        let step_id =
            i32_field(request, "growth_pack_step_id").ok_or(StateError::InvalidRequest)?;
        rules
            .growth_pack_steps
            .iter()
            .find(|row| row.id == step_id)
            .map(|row| row.growth_pack_id)
            .ok_or(StateError::InvalidRequest)?
    };
    let pack = rules
        .growth_packs
        .iter()
        .find(|row| row.id == pack_id)
        .ok_or(StateError::InvalidRequest)?;
    if !home::in_period(pack.start_at, pack.end_at, now) {
        return Err(StateError::OutOfSchedule);
    }
    let premium_owned = message_list(resources, "growth_pack_states")
        .iter()
        .any(|row| i32_field(row, "growth_pack_id") == Some(pack_id));
    let progress = total_task_count(resources, pack.total_task_condition_id);
    let requested_premium = request
        .get_field_by_name("is_premium")
        .and_then(|v| v.as_bool());
    let mut awarded = Vec::new();
    for step in rules
        .growth_pack_steps
        .iter()
        .filter(|row| row.growth_pack_id == pack_id && row.count <= progress)
    {
        if !bulk && i32_field(request, "growth_pack_step_id") != Some(step.id) {
            continue;
        }
        let mut state = message_list(resources, "growth_pack_step_states")
            .into_iter()
            .find(|row| i32_field(row, "growth_pack_step_id") == Some(step.id))
            .unwrap_or(empty_message(proto, "blend.model.GrowthPackStepState")?);
        state.set_field_by_name("growth_pack_step_id", Value::I32(step.id));
        let want_free = bulk || requested_premium == Some(false);
        let want_premium = (bulk || requested_premium == Some(true)) && premium_owned;
        if want_free
            && !state
                .get_field_by_name("is_received")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        {
            awarded.extend(home::grant(
                proto,
                home_rules,
                resources,
                changed,
                reward_set(rules, step.reward_set_id)?,
                now,
            )?);
            state.set_field_by_name("is_received", Value::Bool(true));
        }
        if want_premium
            && !state
                .get_field_by_name("is_premium_received")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        {
            awarded.extend(home::grant(
                proto,
                home_rules,
                resources,
                changed,
                reward_set(rules, step.premium_reward_set_id)?,
                now,
            )?);
            state.set_field_by_name("is_premium_received", Value::Bool(true));
        }
        home::put(
            resources,
            "growth_pack_step_states",
            "growth_pack_step_id",
            state.clone(),
        );
        home::put(
            changed,
            "growth_pack_step_states",
            "growth_pack_step_id",
            state,
        );
    }
    if awarded.is_empty() {
        return Err(StateError::InvalidRequest);
    }
    response.set_field_by_name("rewards", Value::List(awarded));
    Ok(())
}
