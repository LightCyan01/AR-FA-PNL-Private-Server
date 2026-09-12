use super::prelude::*;

fn rule<'a>(rules: &'a HomeRules, id: i32, now: i64) -> Option<&'a MultiMissionRule> {
    rules
        .multi_missions
        .iter()
        .find(|row| row.id == id && in_period(row.start_at, row.end_at, now))
}

pub(crate) fn multi_mission_count(resources: &DynamicMessage, id: i32) -> i64 {
    // ponytail: cooperative totals are account-local until this private server needs multi-account play.
    message_list(resources, "multi_missions")
        .iter()
        .find(|row| i32_field(row, "multi_mission_id") == Some(id))
        .and_then(|row| i32_field(row, "count"))
        .unwrap_or(0)
        .max(0) as i64
}

fn advance_one(
    proto: &ProtoRegistry,
    rules: &HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    id: i32,
    delta: i64,
    now: i64,
) -> Result<bool, StateError> {
    if delta <= 0 || rule(rules, id, now).is_none() {
        return Ok(false);
    }
    let current = multi_mission_count(resources, id);
    let next = current
        .checked_add(delta)
        .and_then(|value| i32::try_from(value).ok())
        .ok_or(StateError::InvalidRequest)?;
    let mut state = message_list(resources, "multi_missions")
        .into_iter()
        .find(|row| i32_field(row, "multi_mission_id") == Some(id))
        .unwrap_or(empty_message(proto, "blend.model.MultiMission")?);
    state.set_field_by_name("multi_mission_id", Value::I32(id));
    state.set_field_by_name("count", Value::I32(next));
    put(
        resources,
        "multi_missions",
        "multi_mission_id",
        state.clone(),
    );
    put(changed, "multi_missions", "multi_mission_id", state);
    Ok(true)
}

fn advance_many(
    proto: &ProtoRegistry,
    rules: &HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    ids: impl IntoIterator<Item = i32>,
    delta: i64,
    now: i64,
) -> Result<(), StateError> {
    let mut changed_any = false;
    for id in ids {
        changed_any |= advance_one(proto, rules, resources, changed, id, delta, now)?;
    }
    if changed_any {
        advance_missions(proto, rules, resources, changed, now, None)?;
    }
    Ok(())
}

pub(crate) fn advance_multi_mission_synthesis(
    proto: &ProtoRegistry,
    rules: &HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    count: i32,
    now: i64,
) -> Result<(), StateError> {
    advance_many(
        proto,
        rules,
        resources,
        changed,
        rules
            .multi_missions
            .iter()
            .filter(|row| {
                row.progress_kind == "synthesis" && in_period(row.start_at, row.end_at, now)
            })
            .map(|row| row.id),
        i64::from(count.max(0)),
        now,
    )
}

pub(crate) fn advance_multi_mission_item(
    proto: &ProtoRegistry,
    rules: &HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    item_id: i32,
    count: i32,
    now: i64,
) -> Result<(), StateError> {
    advance_many(
        proto,
        rules,
        resources,
        changed,
        rules
            .multi_missions
            .iter()
            .filter(|row| {
                row.progress_kind == "item"
                    && row.item_id == Some(item_id)
                    && in_period(row.start_at, row.end_at, now)
            })
            .map(|row| row.id),
        i64::from(count.max(0)),
        now,
    )
}

pub(crate) fn advance_multi_mission_battle(
    proto: &ProtoRegistry,
    rules: &HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    quest_id: i32,
    damage: i64,
    now: i64,
) -> Result<(), StateError> {
    advance_many(
        proto,
        rules,
        resources,
        changed,
        rules
            .multi_missions
            .iter()
            .filter(|row| {
                row.progress_kind == "battle_damage"
                    && row.quest_ids.contains(&quest_id)
                    && in_period(row.start_at, row.end_at, now)
            })
            .map(|row| row.id),
        damage.max(0),
        now,
    )
}

pub(crate) fn multi_mission_status(
    proto: &ProtoRegistry,
    rules: &HomeRules,
    resources: &DynamicMessage,
    response: &mut DynamicMessage,
    event_id: i32,
    now: i64,
) -> Result<(), StateError> {
    let rows: Vec<_> = rules
        .multi_missions
        .iter()
        .filter(|row| row.event_id == event_id)
        .collect();
    if !rows
        .iter()
        .any(|row| in_period(row.start_at, row.end_at, now))
    {
        return Err(StateError::OutOfSchedule);
    }
    let counts = rows
        .into_iter()
        .map(|row| {
            let mut value = empty_message(proto, "blend.model.MultiMissionCount")?;
            value.set_field_by_name("multi_mission_id", Value::I32(row.id));
            value.set_field_by_name("count", Value::I64(multi_mission_count(resources, row.id)));
            Ok::<_, StateError>(Value::Message(value))
        })
        .collect::<Result<Vec<_>, _>>()?;
    response.set_field_by_name("multi_mission_counts", Value::List(counts));
    Ok(())
}

pub(crate) fn receive_multi_mission(
    proto: &ProtoRegistry,
    rules: &HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    response: &mut DynamicMessage,
    step_id: i32,
    now: i64,
) -> Result<(), StateError> {
    let step = rules
        .multi_mission_steps
        .iter()
        .find(|step| step.id == step_id)
        .ok_or(StateError::InvalidRequest)?;
    let mission = rule(rules, step.multi_mission_id, now).ok_or(StateError::OutOfSchedule)?;
    if !in_period(step.start_at, None, now) {
        return Err(StateError::OutOfSchedule);
    }
    let state = message_list(resources, "multi_missions")
        .into_iter()
        .find(|row| i32_field(row, "multi_mission_id") == Some(mission.id));
    let received = state
        .as_ref()
        .and_then(|row| i32_field(row, "received_step"))
        .unwrap_or(0);
    if step.step != received.saturating_add(1)
        || multi_mission_count(resources, mission.id) < step.count
    {
        return Err(StateError::InvalidRequest);
    }
    if step
        .learnable_recipe_ids
        .iter()
        .any(|id| !rules.recipes.iter().any(|recipe| recipe.id == *id))
    {
        return Err(StateError::MasterData(
            "multi-mission recipe is missing".into(),
        ));
    }
    let rewards = grant(proto, rules, resources, changed, &step.rewards, now)?;
    let mut state = state.unwrap_or(empty_message(proto, "blend.model.MultiMission")?);
    state.set_field_by_name("multi_mission_id", Value::I32(mission.id));
    state.set_field_by_name("received_step", Value::I32(step.step));
    put(
        resources,
        "multi_missions",
        "multi_mission_id",
        state.clone(),
    );
    put(changed, "multi_missions", "multi_mission_id", state);
    response.set_field_by_name("rewards", Value::List(rewards));
    let mut count = empty_message(proto, "blend.model.MultiMissionCount")?;
    count.set_field_by_name("multi_mission_id", Value::I32(mission.id));
    count.set_field_by_name(
        "count",
        Value::I64(multi_mission_count(resources, mission.id)),
    );
    response.set_field_by_name(
        "multi_mission_counts",
        Value::List(vec![Value::Message(count)]),
    );
    Ok(())
}
