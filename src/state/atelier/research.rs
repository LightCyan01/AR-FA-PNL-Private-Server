use super::prelude::*;

pub(crate) fn research(
    proto: &ProtoRegistry,
    rules: &AtelierRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    request: &DynamicMessage,
    now: i64,
) -> Result<(), StateError> {
    let group_id = i32_field(request, "group_id").ok_or(StateError::InvalidRequest)?;
    let count = i32_field(request, "count").ok_or(StateError::InvalidRequest)?;
    if count <= 0 {
        return Err(StateError::InvalidRequest);
    }
    let current = message_list(resources, "research_groups")
        .iter()
        .find(|row| i32_field(row, "group_id") == Some(group_id))
        .and_then(|row| i32_field(row, "level"))
        .unwrap_or(0);
    let start = current.checked_add(1).ok_or(StateError::InvalidRequest)?;
    let end = current
        .checked_add(count)
        .ok_or(StateError::InvalidRequest)?;
    let rows = rules
        .research
        .iter()
        .filter(|row| row.group_id == group_id && (start..=end).contains(&row.level))
        .collect::<Vec<_>>();
    if rows.len() != usize::try_from(count).map_err(|_| StateError::InvalidRequest)? {
        return Err(StateError::InvalidRequest);
    }
    let minimum_level = status_message(resources)
        .ok()
        .and_then(|status| i32_field(&status, "minimum_character_level"))
        .unwrap_or(1);
    if rows.iter().any(|row| {
        !in_period(row.start_at, None, now)
            || row
                .minimum_character_level
                .is_some_and(|level| minimum_level < level)
            || row
                .requirements
                .iter()
                .any(|task| total_task_count(resources, task.condition_id) < task.count)
    }) {
        return Err(StateError::InvalidRequest);
    }
    let costs = rows
        .iter()
        .try_fold(BTreeMap::<(i32, i32), i32>::new(), |mut all, row| {
            let value = all
                .entry((row.cost.resource_type, row.cost.id))
                .or_default();
            *value = value
                .checked_add(row.cost.quantity)
                .ok_or(StateError::InvalidRequest)?;
            Ok::<_, StateError>(all)
        })?;
    character::spend(
        proto,
        resources,
        changed,
        &costs
            .into_iter()
            .map(|((resource_type, id), quantity)| RuleCost {
                id,
                quantity,
                resource_type,
            })
            .collect::<Vec<_>>(),
    )?;
    let mut group = empty_message(proto, "blend.model.ResearchGroup")?;
    group.set_field_by_name("group_id", Value::I32(group_id));
    group.set_field_by_name("level", Value::I32(end));
    home::put(resources, "research_groups", "group_id", group.clone());
    home::put(changed, "research_groups", "group_id", group);
    let task_id = rules
        .constants
        .research_task_condition_ids
        .get(&group_id)
        .copied()
        .ok_or(StateError::InvalidRequest)?;
    let task = set_total_task_count(resources, task_id, end)?;
    home::put(changed, "total_task_counts", "condition_id", task);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn communication(
    proto: &ProtoRegistry,
    rules: &AtelierRules,
    home_rules: &home::HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    response: &mut DynamicMessage,
    route: &str,
    request: &DynamicMessage,
    now: i64,
) -> Result<(), StateError> {
    let character_id = i32_field(request, "character_id").ok_or(StateError::InvalidRequest)?;
    let story_number = i32_field(request, "story_number").ok_or(StateError::InvalidRequest)?;
    if !(1..=rules.constants.max_communication_story_number).contains(&story_number) {
        return Err(StateError::InvalidRequest);
    }
    let row = rules
        .communications
        .iter()
        .find(|row| row.character_id == character_id && row.story_number == story_number)
        .filter(|row| in_period(row.start_at, row.end_at, now))
        .ok_or(StateError::OutOfSchedule)?;
    resource_character(resources, character_id)?;
    let mut state = message_list(resources, "communication_states")
        .into_iter()
        .find(|state| i32_field(state, "character_id") == Some(character_id))
        .unwrap_or(empty_message(proto, "blend.model.CommunicationState")?);
    state.set_field_by_name("character_id", Value::I32(character_id));
    let mut released = i32_list(&state, "released_story_numbers");
    let mut cleared = i32_list(&state, "cleared_story_numbers");
    if route == "/communication/story_release" {
        if released.contains(&story_number)
            || cleared.contains(&story_number)
            || (story_number > 1 && !cleared.contains(&(story_number - 1)))
            || (story_number > rules.constants.max_free_communication_story_number
                && row
                    .key_tasks
                    .iter()
                    .filter(|task| total_task_count(resources, task.condition_id) >= task.count)
                    .count()
                    < usize::try_from(
                        rules
                            .constants
                            .number_of_keys_required_communication_release,
                    )
                    .map_err(|_| StateError::InvalidRequest)?)
        {
            return Err(StateError::InvalidRequest);
        }
        released.push(story_number);
    } else {
        if cleared.contains(&story_number)
            || (story_number > rules.constants.max_free_communication_story_number
                && !released.contains(&story_number))
        {
            return Err(StateError::InvalidRequest);
        }
        cleared.push(story_number);
        let rewards = home::grant(proto, home_rules, resources, changed, &row.rewards, now)?;
        response.set_field_by_name("rewards", Value::List(rewards));
        if let Some(scene) = row.reward_scene_id {
            state.set_field_by_name(
                "reward_scene_id",
                Value::Message(int32_value(proto, scene)?),
            );
        }
    }
    released.sort_unstable();
    cleared.sort_unstable();
    state.set_field_by_name(
        "released_story_numbers",
        Value::List(released.into_iter().map(Value::I32).collect()),
    );
    state.set_field_by_name(
        "cleared_story_numbers",
        Value::List(cleared.into_iter().map(Value::I32).collect()),
    );
    home::put(
        resources,
        "communication_states",
        "character_id",
        state.clone(),
    );
    home::put(changed, "communication_states", "character_id", state);
    Ok(())
}
