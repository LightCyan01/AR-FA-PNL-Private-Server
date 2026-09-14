use super::*;
use std::path::Path;

#[test]
fn recipe_unlocks_reconcile_from_persisted_progress() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_rules().unwrap();
    let mut resources = starter_resources(&proto, &load_fresh_rules().unwrap()).unwrap();
    let mut quest = empty_message(&proto, "blend.model.QuestState").unwrap();
    quest.set_field_by_name("quest_id", Value::I32(101002006));
    quest.set_field_by_name("clear_count", Value::I32(1));
    upsert_quest_state(&mut resources, quest);
    let mut home = HomeState::default();
    let request = empty_message(&proto, "google.protobuf.Empty").unwrap();
    let result = reduce_home(
        &proto,
        &rules,
        load_character_rules().unwrap(),
        resources,
        &mut home,
        "/recipe/learn",
        &request,
        "blend.api.RecipeLearnResponse",
        unix_now(),
    )
    .unwrap();
    let updated = proto
        .decode("blend.model.Resources", &result.resources_blob)
        .unwrap();
    assert!(recipe_present(&updated, 4));
    let response = proto
        .decode("blend.api.RecipeLearnResponse", &result.response_plaintext)
        .unwrap();
    assert!(i32_list(&response, "learned_recipe_ids").contains(&4));
    println!("RECIPE_UNLOCK_RECONCILIATION_OK recipe=4 quest=101002006");
}

#[test]
fn mission_claim_rejects_expired_unclaimed_progress() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_rules().unwrap();
    let now = unix_now();
    let mission = rules
        .missions
        .iter()
        .find(|row| {
            row.reset_cycle == Some(1)
                && row.prev_mission_id.is_none()
                && row.event_tab_id.is_none()
                && row.total_task_condition_id.is_none()
                && mission_active(&rules, row, now)
                && row
                    .steps
                    .first()
                    .is_some_and(|step| in_period(step.start_at, None, now))
                && !row
                    .objective
                    .counter
                    .iter()
                    .chain(&row.objective.counters)
                    .any(|event| event == "memoria_collection")
                && row.objective.state.is_none()
                && row.objective.synthesis.is_none()
                && row.objective.battle.is_none()
        })
        .unwrap();
    let mut resources = starter_resources(&proto, &load_fresh_rules().unwrap()).unwrap();
    let mut stale = empty_message(&proto, "blend.model.Mission").unwrap();
    stale.set_field_by_name("mission_id", Value::I32(mission.id));
    stale.set_field_by_name("count", Value::I32(mission.steps[0].count));
    stale.set_field_by_name(
        "reset_at",
        Value::Message(timestamp(&proto, reset_at(Some(1), now).unwrap() - 86_400).unwrap()),
    );
    put(&mut resources, "missions", "mission_id", stale);
    let mut request = empty_message(&proto, "blend.api.MissionReceiveRequest").unwrap();
    request.set_field_by_name("mission_ids", Value::List(vec![Value::I32(mission.id)]));
    assert!(reduce_home(
        &proto,
        &rules,
        load_character_rules().unwrap(),
        resources,
        &mut HomeState::default(),
        "/mission/receive",
        &request,
        "blend.api.MissionReceiveResponse",
        now,
    )
    .is_err());
}

#[test]
fn indexed_mission_events_complete_within_budget() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_rules().unwrap();
    let mut resources = starter_resources(&proto, &load_fresh_rules().unwrap()).unwrap();
    let mut changed = empty_message(&proto, "blend.model.Resources").unwrap();
    let now = unix_now();
    advance_missions(&proto, &rules, &mut resources, &mut changed, now, None).unwrap();

    let started = std::time::Instant::now();
    for _ in 0..8 {
        advance_missions(
            &proto,
            &rules,
            &mut resources,
            &mut changed,
            now,
            Some(("synthesis", 1)),
        )
        .unwrap();
    }
    let elapsed = started.elapsed();
    assert!(
        elapsed < std::time::Duration::from_secs(4),
        "indexed mission events took {elapsed:?}"
    );
}

#[test]
fn story_mission_uses_day_and_scene_progress() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_rules().unwrap();
    let mission = rules.missions.iter().find(|row| row.id == 51002).unwrap();
    assert_eq!(
        mission.objective.counter.as_deref(),
        Some("quest_clear:101002007")
    );
    let mut resources = starter_resources(&proto, &load_fresh_rules().unwrap()).unwrap();
    let mut changed = empty_message(&proto, "blend.model.Resources").unwrap();
    let now = unix_now();

    advance_missions(
        &proto,
        &rules,
        &mut resources,
        &mut changed,
        now,
        Some(("quest_clear:101001007", 1)),
    )
    .unwrap();
    assert_eq!(mission_count(&resources, mission), 0);

    advance_missions(
        &proto,
        &rules,
        &mut resources,
        &mut changed,
        now,
        Some(("quest_clear:101002007", 1)),
    )
    .unwrap();
    assert_eq!(mission_count(&resources, mission), 1);
}

#[test]
fn changed_quest_state_advances_story_mission_once() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_rules().unwrap();
    let before = starter_resources(&proto, &load_fresh_rules().unwrap()).unwrap();
    let mut resources = before.clone();
    let mut quest = empty_message(&proto, "blend.model.QuestState").unwrap();
    quest.set_field_by_name("quest_id", Value::I32(101002007));
    quest.set_field_by_name("clear_count", Value::I32(1));
    upsert_quest_state(&mut resources, quest);
    let mut changed = empty_message(&proto, "blend.model.Resources").unwrap();

    resource_progress(
        &proto,
        &rules,
        &before,
        &mut resources,
        &mut changed,
        unix_now(),
    )
    .unwrap();

    let mission = rules.missions.iter().find(|row| row.id == 51002).unwrap();
    assert_eq!(total_task_count(&resources, 5), 1);
    assert_eq!(mission_count(&resources, mission), 1);
    assert_eq!(
        mission_count(
            &resources,
            &rules.missions.iter().find(|row| row.id == 51007).unwrap()
        ),
        0
    );
}

#[test]
fn pre_reconciled_quest_state_is_not_counted_again() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_rules().unwrap();
    let before = starter_resources(&proto, &load_fresh_rules().unwrap()).unwrap();
    let mut resources = before.clone();
    let mut quest = empty_message(&proto, "blend.model.QuestState").unwrap();
    quest.set_field_by_name("quest_id", Value::I32(101002007));
    quest.set_field_by_name("clear_count", Value::I32(1));
    upsert_quest_state(&mut resources, quest);
    let mut changed = empty_message(&proto, "blend.model.Resources").unwrap();

    advance_missions(
        &proto,
        &rules,
        &mut resources,
        &mut changed,
        unix_now(),
        None,
    )
    .unwrap();
    resource_progress(
        &proto,
        &rules,
        &before,
        &mut resources,
        &mut changed,
        unix_now(),
    )
    .unwrap();

    let mission = rules.missions.iter().find(|row| row.id == 51002).unwrap();
    assert_eq!(total_task_count(&resources, 5), 1);
    assert_eq!(mission_count(&resources, mission), 1);
}

#[test]
fn present_producer_rejects_duplicate_tools_and_advances_real_missions() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_rules().unwrap();
    let characters = load_character_rules().unwrap();
    let activities = activities::load_rules().unwrap();
    let now = activities::row(&activities, "present", 55).unwrap()["start_at"]
        .as_i64()
        .unwrap()
        + 1;
    let mut resources = starter_resources(&proto, &load_fresh_rules().unwrap()).unwrap();
    let mut gifts = Vec::new();
    for id in 900_000..900_050 {
        let mut tool = empty_message(&proto, "blend.model.EquipmentTool").unwrap();
        tool.set_field_by_name("entity_id", Value::I32(id));
        tool.set_field_by_name("tool_id", Value::I32(1253));
        put(&mut resources, "equipment_tools", "entity_id", tool);
        let mut gift = empty_message(&proto, "blend.model.ToolEntity").unwrap();
        gift.set_field_by_name("type", Value::I32(6));
        gift.set_field_by_name("entity_id", Value::I32(id));
        gifts.push(Value::Message(gift));
    }
    let before = resources.clone();
    let mut request = empty_message(&proto, "blend.api.PresentExecuteRequest").unwrap();
    request.set_field_by_name("present_id", Value::I32(55));
    request.set_field_by_name(
        "gifted_tools",
        Value::List(vec![gifts[0].clone(), gifts[0].clone()]),
    );
    let mut home = HomeState::default();
    assert!(reduce_home(
        &proto,
        &rules,
        characters,
        resources.clone(),
        &mut home,
        "/present/execute",
        &request,
        "blend.api.PresentExecuteResponse",
        now
    )
    .is_err());
    assert_eq!(resources, before);
    request.set_field_by_name("gifted_tools", Value::List(gifts));
    let result = reduce_home(
        &proto,
        &rules,
        characters,
        resources,
        &mut home,
        "/present/execute",
        &request,
        "blend.api.PresentExecuteResponse",
        now,
    )
    .unwrap();
    let mut resources = proto
        .decode("blend.model.Resources", &result.resources_blob)
        .unwrap();
    let present = message_list(&resources, "present_states")
        .into_iter()
        .find(|p| i32_field(p, "present_id") == Some(55))
        .unwrap();
    assert_eq!(i32_field(&present, "total_friendship_point"), Some(50));
    assert_eq!(i32_field(&present, "closeness"), Some(51));
    assert_eq!(i32_field(&present, "friendship_point"), Some(0));
    assert!(!message_list(&resources, "equipment_tools")
        .iter()
        .any(|t| i32_field(t, "entity_id").is_some_and(|id| (900_000..900_050).contains(&id))));
    let response = proto
        .decode(
            "blend.api.PresentExecuteResponse",
            &result.response_plaintext,
        )
        .unwrap();
    assert_eq!(
        i32_list(
            &member_status(&response, "deleted_resources").unwrap(),
            "equipment_tool_entity_ids"
        )
        .len(),
        50
    );
    let mut claim = empty_message(&proto, "blend.api.MissionReceiveRequest").unwrap();
    claim.set_field_by_name("mission_ids", Value::List(vec![Value::I32(150087)]));
    claim.set_field_by_name("bulk_receive", Value::Bool(true));
    let mut changed = empty_message(&proto, "blend.model.Resources").unwrap();
    claim_missions(
        &proto,
        &rules,
        &mut resources,
        &mut changed,
        &mut home,
        &claim,
        now,
    )
    .unwrap();
    assert_eq!(received(&resources, 150087), 1);
    let persisted = resources.clone();
    assert!(reduce_home(
        &proto,
        &rules,
        characters,
        resources.clone(),
        &mut home,
        "/present/execute",
        &request,
        "blend.api.PresentExecuteResponse",
        now
    )
    .is_err());
    assert_eq!(resources, persisted);
    // Awakening objectives count upgrades above initial rarity, not raw enums.
    let mut character = empty_message(&proto, "blend.model.Character").unwrap();
    character.set_field_by_name("character_id", Value::I32(43111));
    character.set_field_by_name("rarity", Value::I32(7));
    put(&mut resources, "characters", "character_id", character);
    let objective = ResourceObjective::CharacterRarity {
        character_ids: vec![43111],
        initial_rarity: 1,
    };
    assert_eq!(objective_count(&rules, &resources, &objective), 6);
}

#[test]
fn street_step_objective_tracks_saved_phase() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_rules().unwrap();
    let mut resources = starter_resources(&proto, &load_fresh_rules().unwrap()).unwrap();
    let mut street = empty_message(&proto, "blend.model.StreetState").unwrap();
    street.set_field_by_name("quest_id", Value::I32(10_710_001));
    street.set_field_by_name("phase", Value::I32(9));
    put(&mut resources, "street_states", "quest_id", street);
    assert_eq!(
        objective_count(
            &rules,
            &resources,
            &ResourceObjective::StreetPhase {
                quest_id: 10_710_001
            }
        ),
        9
    );
}

#[test]
fn battle_composition_missions_require_the_matching_quest_and_saved_party() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_rules().unwrap();
    let quest = load_gameplay_rules().unwrap();
    let task = rules
        .total_tasks
        .iter()
        .find(|t| t.objective.battle.is_some())
        .unwrap();
    let objective = task.objective.battle.as_ref().unwrap();
    let mut state = empty_message(&proto, "blend.model.BattleState").unwrap();
    let mut members = Vec::new();
    for id in objective.character_ids.iter().take(objective.minimum) {
        let mut ally = empty_message(&proto, "blend.model.BattleAlly").unwrap();
        ally.set_field_by_name("character_id", Value::I32(*id));
        let mut member = empty_message(&proto, "blend.model.BattleMember").unwrap();
        member.set_field_by_name("ally", Value::Message(ally));
        members.push(Value::Message(member));
    }
    state.set_field_by_name("members", Value::List(members));
    let mut resources = starter_resources(&proto, &load_fresh_rules().unwrap()).unwrap();
    let mut changed = empty_message(&proto, "blend.model.Resources").unwrap();
    // A matching party in an unrelated battle cannot satisfy this objective.
    battle_clear_progress(
        &proto,
        &rules,
        &quest,
        &state,
        &mut resources,
        &mut changed,
        -1,
        true,
        unix_now(),
    )
    .unwrap();
    assert_eq!(total_task_count(&resources, task.condition_id), 0);
    battle_clear_progress(
        &proto,
        &rules,
        &quest,
        &state,
        &mut resources,
        &mut changed,
        objective.quest_ids[0],
        false,
        unix_now(),
    )
    .unwrap();
    assert_eq!(total_task_count(&resources, task.condition_id), 0);
    battle_clear_progress(
        &proto,
        &rules,
        &quest,
        &state,
        &mut resources,
        &mut changed,
        objective.quest_ids[0],
        true,
        unix_now(),
    )
    .unwrap();
    assert_eq!(total_task_count(&resources, task.condition_id), 1);
}
