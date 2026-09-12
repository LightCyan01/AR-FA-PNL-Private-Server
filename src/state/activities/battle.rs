use super::prelude::*;

#[allow(clippy::too_many_arguments)]
pub(crate) fn battle_start(
    proto: &ProtoRegistry,
    rules: &ActivityRules,
    tutorial_rules: &TutorialRules,
    atelier: &atelier::AtelierRules,
    characters: &CharacterRules,
    resources: &mut DynamicMessage,
    saved: &mut ActivityState,
    request: &DynamicMessage,
    now: i64,
) -> Result<BattleStartMutation, StateError> {
    let area_id = i32_field(request, "area_id").ok_or(StateError::InvalidRequest)?;
    let quest_id = number(row(rules, "exploration_area", area_id)?, "quest_id");
    let run = saved
        .explorations
        .get_mut(&quest_id)
        .ok_or(StateError::InvalidRequest)?;
    skip_talks(run);
    let step = run.steps.get(run.next).ok_or(StateError::InvalidRequest)?;
    if run.pending
        || step.area_id != area_id
        || step.kind != 1
        || i32_field(request, "enemy_weak_level").unwrap_or(0) != 0
    {
        return Err(StateError::InvalidRequest);
    }
    let mut battle_rules = tutorial_rules.clone();
    let quest = battle_rules
        .quests
        .iter_mut()
        .find(|q| q.id == quest_id)
        .ok_or(StateError::InvalidRequest)?;
    quest.quest_type = 1;
    quest.battle_id = step.battle_id;
    let mut status = empty_message(proto, "blend.model.ExplorationPartyStatus")?;
    status.set_field_by_name(
        "hps",
        Value::List(run.hps.iter().copied().map(Value::I32).collect()),
    );
    status.set_field_by_name(
        "battle_tool_usage_counts",
        Value::List(run.tool_counts.iter().copied().map(Value::I32).collect()),
    );
    status.set_field_by_name("party_gauge", Value::I32(run.party_gauge));
    let mut mutation = reduce_battle_start_with_progression(
        proto,
        &battle_rules,
        atelier,
        characters,
        resources.clone(),
        quest_id,
        run.party_number,
        Some(&status),
        BattleStartMode::Standard,
        now,
    )?;
    let mut context = member_status(&mutation.response, "context")?;
    context.set_field_by_name("exploration_area_id", Value::I32(area_id));
    mutation
        .response
        .set_field_by_name("context", Value::Message(context));
    run.pending = true;
    let mut changed = empty_message(proto, "blend.model.Resources")?;
    progress(proto, &battle_rules, resources, &mut changed, run)?;
    mutation
        .response
        .set_field_by_name("changed_resources", Value::Message(changed));
    Ok(mutation)
}

pub(crate) fn battle_finish(
    proto: &ProtoRegistry,
    quest: &TutorialRules,
    state: &DynamicMessage,
    mut resources: DynamicMessage,
    saved: &mut ActivityState,
    quest_id: i32,
) -> Result<BattleFinishMutation, StateError> {
    let run = saved
        .explorations
        .get_mut(&quest_id)
        .ok_or(StateError::InvalidRequest)?;
    let step = run.steps.get(run.next).ok_or(StateError::InvalidRequest)?;
    if !run.pending
        || step.battle_id != i32_field(state, "battle_id")
        || current_battle_status(state)? != BATTLE_STATUS_WON
        || i32_field(state, "wave") != Some(i32_list(state, "wave_ids").len() as i32)
    {
        return Err(StateError::InvalidRequest);
    }
    run.hps = message_list(state, "members")
        .iter()
        .filter(|m| member_type(m).ok() == Some(0))
        .map(|m| i32_field(m, "hp").unwrap_or(0))
        .collect();
    run.tool_counts = message_list(state, "battle_tools")
        .iter()
        .map(|t| i32_field(t, "usage_count").unwrap_or(0))
        .collect();
    run.party_gauge = i32_field(state, "party_gauge").unwrap_or(0);
    run.character_exp = run
        .character_exp
        .checked_add(step.exp)
        .ok_or(StateError::InvalidRequest)?;
    for id in &run.character_ids {
        let exp = run.earned_exp.entry(*id).or_default();
        *exp = exp
            .checked_add(step.exp)
            .ok_or(StateError::InvalidRequest)?;
    }
    if step.cole > 0 {
        run.rewards.push((3, 1, step.cole));
    }
    run.total_turn = run
        .total_turn
        .checked_add(i32_field(state, "total_turn").unwrap_or(0))
        .ok_or(StateError::InvalidRequest)?;
    run.pending = false;
    run.next += 1;
    skip_talks(run);
    let mut changed = empty_message(proto, "blend.model.Resources")?;
    progress(proto, quest, &mut resources, &mut changed, run)?;
    let mut response = empty_message(proto, "blend.api.BattleFinishResponse")?;
    response.set_field_by_name("changed_resources", Value::Message(changed));
    response.set_field_by_name(
        "exploration_result",
        Value::Message(empty_message(proto, "blend.model.ExplorationResult")?),
    );
    Ok(BattleFinishMutation {
        resources,
        response,
        character_indexes: Vec::new(),
    })
}

pub(crate) fn retire_battle(saved: &mut ActivityState, quest_id: i32) {
    if let Some(run) = saved.explorations.get_mut(&quest_id) {
        run.pending = false;
    }
}

pub(crate) fn finish_gacha_battle(
    proto: &ProtoRegistry,
    rules: &ActivityRules,
    home_rules: &home::HomeRules,
    state: &DynamicMessage,
    mut resources: DynamicMessage,
    id: i32,
    now: i64,
) -> Result<BattleFinishMutation, StateError> {
    let spec = row(rules, "gacha_battle", id)?;
    if current_battle_status(state)? != BATTLE_STATUS_WON
        || i32_field(state, "wave") != Some(i32_list(state, "wave_ids").len() as i32)
        || i32_field(state, "battle_id") != Some(number(spec, "battle_id"))
    {
        return Err(StateError::InvalidRequest);
    }
    let mut changed = empty_message(proto, "blend.model.Resources")?;
    let mut result = empty_message(proto, "blend.model.GachaBattleResult")?;
    if !message_list(&resources, "gacha_battle_states")
        .iter()
        .any(|s| i32_field(s, "gacha_battle_id") == Some(id))
    {
        let awarded = home::grant(
            proto,
            home_rules,
            &mut resources,
            &mut changed,
            &rewards(spec, "rewards")?,
            now,
        )?;
        result.set_field_by_name("first_clear_rewards", Value::List(awarded));
        let mut cleared = empty_message(proto, "blend.model.GachaBattleState")?;
        cleared.set_field_by_name("gacha_battle_id", Value::I32(id));
        save(
            &mut resources,
            &mut changed,
            "gacha_battle_states",
            "gacha_battle_id",
            cleared,
        );
    }
    let mut response = empty_message(proto, "blend.api.BattleFinishResponse")?;
    response.set_field_by_name("gacha_battle_result", Value::Message(result));
    response.set_field_by_name("changed_resources", Value::Message(changed));
    Ok(BattleFinishMutation {
        resources,
        response,
        character_indexes: Vec::new(),
    })
}
