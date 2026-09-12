use super::*;
use std::path::Path;

#[test]
fn score_rank_objective_counts_only_qualifying_quests() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_rules().unwrap();
    let mut resources = empty_message(&proto, "blend.model.Resources").unwrap();
    for (quest_id, rank) in [(11, 5), (12, 4), (13, 5)] {
        let mut state = empty_message(&proto, "blend.model.QuestState").unwrap();
        state.set_field_by_name("quest_id", Value::I32(quest_id));
        state.set_field_by_name("score_rank", Value::I32(rank));
        upsert_quest_state(&mut resources, state);
    }
    assert_eq!(
        objective_count(
            &rules,
            &resources,
            &ResourceObjective::ScoreRank {
                quest_ids: vec![11, 12],
                minimum_rank: 5,
            },
        ),
        1
    );
    assert_eq!(
        objective_count(
            &rules,
            &resources,
            &ResourceObjective::ScoreRank {
                quest_ids: vec![],
                minimum_rank: 5,
            },
        ),
        2
    );
}

#[test]
fn mission_objectives_count_battle_tool_uses() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_rules().unwrap();
    let fresh = load_fresh_rules().unwrap();
    let mut resources = starter_resources(&proto, &fresh).unwrap();
    let mut changed = empty_message(&proto, "blend.model.Resources").unwrap();
    let mut saved = HomeState::default();
    saved.battle_progress.tool_uses.insert(19, 3);
    finish_battle_progress(
        &proto,
        &rules,
        &mut saved,
        &mut resources,
        &mut changed,
        Some(101001001),
        unix_now(),
    )
    .unwrap();
    assert_eq!(total_task_count(&resources, 245), 3);
    assert_eq!(total_task_count(&resources, 271), 3);
}

#[test]
fn multi_mission_progress_uses_catalog_scope_and_level_thresholds() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_rules().unwrap();
    let mut resources = starter_resources(&proto, &load_fresh_rules().unwrap()).unwrap();
    let mut changed = empty_message(&proto, "blend.model.Resources").unwrap();

    // This quest contains the catalogued クラストビートル target for mission 1.
    advance_multi_mission_battle(
        &proto,
        &rules,
        &mut resources,
        &mut changed,
        603001,
        123,
        1_701_298_801,
    )
    .unwrap();
    assert_eq!(multi_mission_count(&resources, 1), 123);
    assert_eq!(multi_mission_count(&resources, 3), 0);

    let objective = rules
        .total_tasks
        .iter()
        .find(|task| task.condition_id == 472)
        .and_then(|task| task.objective.state.as_ref())
        .unwrap();
    let mut state = empty_message(&proto, "blend.model.MultiMission").unwrap();
    state.set_field_by_name("multi_mission_id", Value::I32(1));
    state.set_field_by_name("count", Value::I32(10_000_000));
    put(&mut resources, "multi_missions", "multi_mission_id", state);
    assert_eq!(objective_count(&rules, &resources, objective), 3);
}

#[test]
fn multi_mission_status_keeps_all_event_tabs_visible() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_rules().unwrap();
    let resources = starter_resources(&proto, &load_fresh_rules().unwrap()).unwrap();
    let mut response = empty_message(&proto, "blend.api.MultiMissionStatusResponse").unwrap();

    // Mission 2 is active; the client still builds tabs for missions 1 and 3.
    multi_mission_status(&proto, &rules, &resources, &mut response, 7, 1_702_300_000).unwrap();
    let ids = message_list(&response, "multi_mission_counts")
        .into_iter()
        .filter_map(|row| i32_field(&row, "multi_mission_id"))
        .collect::<Vec<_>>();
    assert_eq!(ids, vec![1, 2, 3]);
}

#[test]
fn multi_mission_receive_is_sequential_and_catalog_rewarded() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_rules().unwrap();
    let mut resources = starter_resources(&proto, &load_fresh_rules().unwrap()).unwrap();
    let mut state = empty_message(&proto, "blend.model.MultiMission").unwrap();
    state.set_field_by_name("multi_mission_id", Value::I32(1));
    state.set_field_by_name("count", Value::I32(2_500_000));
    put(&mut resources, "multi_missions", "multi_mission_id", state);
    let mut changed = empty_message(&proto, "blend.model.Resources").unwrap();
    let mut response = empty_message(&proto, "blend.api.MultiMissionReceiveResponse").unwrap();
    receive_multi_mission(
        &proto,
        &rules,
        &mut resources,
        &mut changed,
        &mut response,
        101,
        1_701_298_801,
    )
    .unwrap();
    let state = message_list(&resources, "multi_missions")
        .into_iter()
        .find(|row| i32_field(row, "multi_mission_id") == Some(1))
        .unwrap();
    assert_eq!(i32_field(&state, "received_step"), Some(1));
    assert_eq!(message_list(&response, "rewards").len(), 3);
    assert!(receive_multi_mission(
        &proto,
        &rules,
        &mut resources,
        &mut changed,
        &mut response,
        101,
        1_701_298_801,
    )
    .is_err());
}

#[test]
fn mission_objectives_count_only_qualified_synthesis_outputs() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_rules().unwrap();
    let fresh = load_fresh_rules().unwrap();
    let synthesis = load_synthesis_rules().unwrap();
    let mut resources = starter_resources(&proto, &fresh).unwrap();
    let now = unix_now();
    let trait_recipe = synthesis.recipes.iter().find(|row| row.id == 69).unwrap();
    for (rank, expected) in [(3, 0), (4, 1)] {
        let output = add_synthesis_output(
            &proto,
            &mut resources,
            &trait_recipe.target,
            &[TutorialTraitParam { id: 1, rank }],
            now,
        )
        .unwrap();
        let mut changed = empty_message(&proto, "blend.model.Resources").unwrap();
        changed.set_field_by_name("battle_tools", Value::List(vec![Value::Message(output)]));
        synthesis_progress(
            &proto,
            &rules,
            &mut resources,
            &mut changed,
            trait_recipe.id,
            1,
            now,
        )
        .unwrap();
        assert_eq!(total_task_count(&resources, 246), expected);
    }

    let (equipment_recipe, sr_reward) = synthesis
        .recipes
        .iter()
        .find_map(|recipe| {
            let target = rules
                .equipment_tools
                .iter()
                .find(|tool| tool.id == recipe.target.id)?;
            let alternate = recipe.bonus_rewards.iter().find(|reward| {
                reward.resource_type == 6
                    && rules.equipment_tools.iter().any(|tool| {
                        tool.id == reward.id
                            && target.slot_type == Some(1)
                            && tool.slot_type == Some(1)
                            && target.rarity == 3
                            && tool.rarity < 3
                    })
            })?;
            Some((recipe, alternate))
        })
        .unwrap();
    let traits = [
        TutorialTraitParam { id: 1, rank: 4 },
        TutorialTraitParam { id: 2, rank: 5 },
    ];
    for (reward, expected) in [(sr_reward, 0), (&equipment_recipe.target, 1)] {
        let output = add_synthesis_output(&proto, &mut resources, reward, &traits, now).unwrap();
        let mut changed = empty_message(&proto, "blend.model.Resources").unwrap();
        changed.set_field_by_name("equipment_tools", Value::List(vec![Value::Message(output)]));
        synthesis_progress(
            &proto,
            &rules,
            &mut resources,
            &mut changed,
            equipment_recipe.id,
            1,
            now,
        )
        .unwrap();
        assert_eq!(total_task_count(&resources, 828), expected);
    }
}

#[test]
fn mission_battle_counts_catalog_missions_and_grants_each_step_once() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_rules().unwrap();
    let mut resources = starter_resources(&proto, &load_fresh_rules().unwrap()).unwrap();
    let mut changed = empty_message(&proto, "blend.model.Resources").unwrap();
    let group = rules
        .mission_battle_rewards
        .iter()
        .find(|rule| rule.quest_id == 1381101)
        .unwrap();
    for id in group.mission_ids.iter().take(5) {
        let mission = rules
            .missions
            .iter()
            .find(|mission| mission.id == *id)
            .unwrap();
        update_task(
            &mut resources,
            &mut changed,
            mission.total_task_condition_id.unwrap(),
            mission.steps.last().unwrap().count,
        )
        .unwrap();
    }
    let objective = rules
        .total_tasks
        .iter()
        .find(|task| task.condition_id == 2757)
        .and_then(|task| task.objective.state.as_ref())
        .unwrap();
    assert_eq!(objective_count(&rules, &resources, objective), 5);

    let before = message_i32_field(&resources, "wallet", "free").unwrap_or(0);
    let rewards = grant_mission_battle_rewards(
        &proto,
        &rules,
        &mut resources,
        &mut changed,
        1381101,
        1_800_000_000,
    )
    .unwrap();
    assert_eq!(rewards.len(), 1);
    assert_eq!(
        message_i32_field(&resources, "wallet", "free"),
        Some(before + 100)
    );
    let state = message_list(&resources, "mission_battle_reward_states")
        .into_iter()
        .find(|state| i32_field(state, "mission_battle_quest_id") == Some(1381101))
        .unwrap();
    assert_eq!(i32_field(&state, "received_step_count"), Some(1));
    assert!(grant_mission_battle_rewards(
        &proto,
        &rules,
        &mut resources,
        &mut changed,
        1381101,
        1_800_000_000,
    )
    .unwrap()
    .is_empty());
}

#[test]
fn mission_objectives_unlock_camera_from_owned_motion() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_rules().unwrap();
    let objective = rules
        .total_tasks
        .iter()
        .find(|task| task.condition_id == 2030)
        .and_then(|task| task.objective.state.as_ref())
        .unwrap();
    let ResourceObjective::OwnedHomeMotion { motion_ids } = objective else {
        panic!("condition 2030 must use its owned Home motion")
    };
    let mut resources = empty_message(&proto, "blend.model.Resources").unwrap();
    assert_eq!(objective_count(&rules, &resources, objective), 0);
    let mut motion = empty_message(&proto, "blend.model.CharaHomeMotion").unwrap();
    motion.set_field_by_name("motion_id", Value::I32(motion_ids[0]));
    put(&mut resources, "chara_home_motions", "motion_id", motion);
    assert_eq!(objective_count(&rules, &resources, objective), 1);
}

#[test]
fn home_routes_update_progress_recipe_rewards_and_profile() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_rules().unwrap();
    let fresh = load_fresh_rules().unwrap();
    let mut resources = starter_resources(&proto, &fresh).unwrap();
    let mut status = status_message(&resources).unwrap();
    status.set_field_by_name("tutorial_step", Value::I32(TUTORIAL_STEP_GACHA_COMPLETE));
    resources.set_field_by_name("status", Value::Message(status));
    let mut quest = empty_message(&proto, "blend.model.QuestState").unwrap();
    quest.set_field_by_name("quest_id", Value::I32(101001017));
    quest.set_field_by_name("clear_count", Value::I32(1));
    upsert_quest_state(&mut resources, quest);
    let now = unix_now();
    let mut home = HomeState::default();
    let mut timeline = empty_message(&proto, "blend.api.ModTimelineReleaseRequest").unwrap();
    timeline.set_field_by_name("mod_timeline_id", Value::I32(24));
    let mut wallet = member_status(&resources, "wallet").unwrap();
    wallet.set_field_by_name("free", Value::I32(300));
    resources.set_field_by_name("wallet", Value::Message(wallet));
    let released = reduce_home(
        &proto,
        &rules,
        load_character_rules().unwrap(),
        resources.clone(),
        &mut home,
        "/mod_timeline/release",
        &timeline,
        "blend.api.ChangedResourcesResponse",
        now,
    )
    .unwrap();
    resources = proto
        .decode("blend.model.Resources", &released.resources_blob)
        .unwrap();
    assert_eq!(message_i32_field(&resources, "wallet", "free"), Some(0));
    assert_eq!(
        i32_field(
            &message_list(&resources, "mod_timeline_states")[0],
            "mod_timeline_id"
        ),
        Some(24)
    );
    assert!(reduce_home(
        &proto,
        &rules,
        load_character_rules().unwrap(),
        resources.clone(),
        &mut home,
        "/mod_timeline/release",
        &timeline,
        "blend.api.ChangedResourcesResponse",
        now
    )
    .is_err());
    for (route, input, field, model, key, earned, seen) in [
        (
            "/emblem/acquisition_drama",
            "blend.api.EmblemAcquisitionDramaRequest",
            "emblems",
            "blend.model.Emblem",
            "emblem_id",
            "rarity",
            "drama_rarity",
        ),
        (
            "/total_battle/achieve_line_drama",
            "blend.api.TotalBattleAchieveLineDramaRequest",
            "total_battle_states",
            "blend.model.TotalBattleState",
            "total_battle_id",
            "best_achieved_line_count",
            "drama_achieved_line_count",
        ),
    ] {
        let mut request = empty_message(&proto, input).unwrap();
        if field == "emblems" {
            request.set_field_by_name("emblem_ids", Value::List(vec![Value::I32(1)]));
        } else {
            request.set_field_by_name(key, Value::I32(1));
        }
        let mut state = empty_message(&proto, model).unwrap();
        state.set_field_by_name(key, Value::I32(1));
        state.set_field_by_name(earned, Value::I32(3));
        put(&mut resources, field, key, state);
        let result = reduce_home(
            &proto,
            &rules,
            load_character_rules().unwrap(),
            resources.clone(),
            &mut home,
            route,
            &request,
            "blend.api.ChangedResourcesResponse",
            now,
        )
        .unwrap();
        resources = proto
            .decode("blend.model.Resources", &result.resources_blob)
            .unwrap();
        assert_eq!(
            i32_field(&message_list(&resources, field)[0], seen),
            Some(3)
        );
    }
    let empty = empty_message(&proto, "google.protobuf.Empty").unwrap();
    let progress = reduce_home(
        &proto,
        &rules,
        load_character_rules().unwrap(),
        resources,
        &mut home,
        "/login_bonus/receive",
        &empty,
        "blend.api.LoginBonusReceiveResponse",
        now,
    )
    .unwrap();
    resources = proto
        .decode("blend.model.Resources", &progress.resources_blob)
        .unwrap();
    assert_eq!(tutorial_step(&resources), TUTORIAL_STEP_HOME_READY);

    let event = reduce_home(
        &proto,
        &rules,
        load_character_rules().unwrap(),
        resources.clone(),
        &mut home,
        "/event/top",
        &empty,
        "blend.api.EventTopResponse",
        now,
    )
    .unwrap();
    let event_response = proto
        .decode("blend.api.EventTopResponse", &event.response_plaintext)
        .unwrap();
    assert!(event_response.has_field_by_name("changed_resources"));
    resources = proto
        .decode("blend.model.Resources", &event.resources_blob)
        .unwrap();

    let extra_recipe_id = rules
        .recipes
        .iter()
        .find(|recipe| recipe.recipe_plan_id == 1 && !recipe_present(&resources, recipe.id))
        .unwrap()
        .id;
    let mut recipe = empty_message(&proto, "blend.model.Recipe").unwrap();
    recipe.set_field_by_name("recipe_id", Value::I32(extra_recipe_id));
    recipe.set_field_by_name(
        "received_at",
        Value::Message(timestamp(&proto, now).unwrap()),
    );
    upsert_recipe(&mut resources, recipe);
    let mut count_request =
        empty_message(&proto, "blend.api.RecipeCountRewardReceiveRequest").unwrap();
    let mut plan = empty_message(&proto, "blend.model.RecipeCountRewardReceive").unwrap();
    plan.set_field_by_name("recipe_plan_id", Value::I32(1));
    plan.set_field_by_name("indices", Value::List(vec![Value::I32(0)]));
    count_request.set_field_by_name("receive_plans", Value::List(vec![Value::Message(plan)]));
    let old_cole = i32_field(&status_message(&resources).unwrap(), "cole").unwrap_or(0);
    let count = reduce_home(
        &proto,
        &rules,
        load_character_rules().unwrap(),
        resources,
        &mut home,
        "/recipe/count_reward_receive",
        &count_request,
        "blend.api.RecipeCountRewardReceiveResponse",
        now,
    )
    .unwrap();
    resources = proto
        .decode("blend.model.Resources", &count.resources_blob)
        .unwrap();
    let count_response = proto
        .decode(
            "blend.api.RecipeCountRewardReceiveResponse",
            &count.response_plaintext,
        )
        .unwrap();
    assert!(!message_list(&count_response, "rewards").is_empty());
    assert_eq!(
        i32_field(&status_message(&resources).unwrap(), "cole"),
        Some(old_cole + 30000)
    );
    assert!(message_list(&resources, "recipe_count_reward_states")
        .iter()
        .any(|state| i32_field(state, "recipe_plan_id") == Some(1)));

    let extra_recipe_ids: Vec<_> = rules
        .recipes
        .iter()
        .filter(|recipe| recipe.recipe_plan_id == 1 && !recipe_present(&resources, recipe.id))
        .take(3)
        .map(|recipe| recipe.id)
        .collect();
    for recipe_id in extra_recipe_ids {
        let mut recipe = empty_message(&proto, "blend.model.Recipe").unwrap();
        recipe.set_field_by_name("recipe_id", Value::I32(recipe_id));
        recipe.set_field_by_name(
            "received_at",
            Value::Message(timestamp(&proto, now).unwrap()),
        );
        upsert_recipe(&mut resources, recipe);
    }
    let mut item_plan = empty_message(&proto, "blend.model.RecipeCountRewardReceive").unwrap();
    item_plan.set_field_by_name("recipe_plan_id", Value::I32(1));
    item_plan.set_field_by_name("indices", Value::List(vec![Value::I32(1)]));
    count_request.set_field_by_name(
        "receive_plans",
        Value::List(vec![Value::Message(item_plan)]),
    );
    let item_count = reduce_home(
        &proto,
        &rules,
        load_character_rules().unwrap(),
        resources,
        &mut home,
        "/recipe/count_reward_receive",
        &count_request,
        "blend.api.RecipeCountRewardReceiveResponse",
        now,
    )
    .unwrap();
    resources = proto
        .decode("blend.model.Resources", &item_count.resources_blob)
        .unwrap();
    let item_response = proto
        .decode(
            "blend.api.RecipeCountRewardReceiveResponse",
            &item_count.response_plaintext,
        )
        .unwrap();
    assert!(message_list(&item_response, "rewards")
        .iter()
        .any(|reward| i32_field(reward, "type") == Some(5)));
    assert!(message_list(&resources, "items")
        .iter()
        .any(|item| i32_field(item, "item_id") == Some(104)
            && i32_field(item, "quantity").unwrap_or(0) >= 3));
    let duplicate = reduce_home(
        &proto,
        &rules,
        load_character_rules().unwrap(),
        resources,
        &mut home,
        "/recipe/count_reward_receive",
        &count_request,
        "blend.api.RecipeCountRewardReceiveResponse",
        now,
    )
    .unwrap();
    let duplicate_response = proto
        .decode(
            "blend.api.RecipeCountRewardReceiveResponse",
            &duplicate.response_plaintext,
        )
        .unwrap();
    assert!(message_list(&duplicate_response, "rewards").is_empty());
    resources = proto
        .decode("blend.model.Resources", &duplicate.resources_blob)
        .unwrap();

    let mut profile_request = empty_message(&proto, "blend.api.ProfileUpdateNameRequest").unwrap();
    profile_request.set_field_by_name("name", Value::String("HomeTest".into()));
    let named = reduce_home(
        &proto,
        &rules,
        load_character_rules().unwrap(),
        resources,
        &mut home,
        "/profile/update_name",
        &profile_request,
        "blend.api.ChangedResourcesResponse",
        now,
    )
    .unwrap();
    let named_resources = proto
        .decode("blend.model.Resources", &named.resources_blob)
        .unwrap();
    let name = named_resources
        .get_field_by_name("profile")
        .and_then(|value| value.as_message().cloned())
        .and_then(|profile| {
            profile
                .get_field_by_name("name")
                .and_then(|value| value.as_str().map(str::to_owned))
        });
    assert_eq!(name.as_deref(), Some("HomeTest"));

    resources = named_resources;
    let mut changed = empty_message(&proto, "blend.model.Resources").unwrap();
    let mut character_reward = rules
        .reward_sets
        .iter()
        .flat_map(|set| &set.rewards)
        .find(|r| r.resource_type == 4 && r.resource_params.as_ref().and_then(|p| p.skin).is_some())
        .unwrap()
        .clone();
    character_reward.quantity = 2;
    let character_rule = rules
        .characters
        .iter()
        .find(|r| r.id == character_reward.id)
        .unwrap();
    let character_rewards = grant(
        &proto,
        &rules,
        &mut resources,
        &mut changed,
        &[character_reward.clone()],
        now,
    )
    .unwrap();
    assert_eq!(character_rewards.len(), 2);
    assert_eq!(
        message_list(&resources, "characters")
            .iter()
            .filter(|r| i32_field(r, "character_id") == Some(character_reward.id))
            .count(),
        1
    );
    assert_eq!(
        message_list(&resources, "character_pieces")
            .iter()
            .find(|r| i32_field(r, "character_id") == Some(character_reward.id))
            .and_then(|r| i32_field(r, "quantity")),
        Some(character_rule.duplicated_piece_count)
    );
    assert_eq!(message_list(&changed, "character_skins").len(), 1);
    assert_eq!(
        message_list(character_rewards[0].as_message().unwrap(), "other_rewards").len(),
        1
    );
    assert_eq!(
        message_list(character_rewards[1].as_message().unwrap(), "other_rewards").len(),
        2
    );
    let memoria_id = rules.memoria_ids[0];
    let granted = grant(
        &proto,
        &rules,
        &mut resources,
        &mut changed,
        &[TutorialReward {
            resource_type: 17,
            id: memoria_id,
            quantity: 2,
            resource_params: None,
        }],
        now,
    )
    .unwrap();
    assert_eq!(
        granted[0]
            .as_message()
            .unwrap()
            .get_field_by_name("is_new")
            .unwrap()
            .as_bool(),
        Some(true)
    );
    assert_eq!(
        granted[1]
            .as_message()
            .unwrap()
            .get_field_by_name("is_new")
            .unwrap()
            .as_bool(),
        Some(false)
    );
    let entity_id = i32_field(granted[0].as_message().unwrap(), "entity_id").unwrap();
    let character_id = fresh.initial_character.id;
    let mut equip = empty_message(&proto, "blend.api.CharacterBulkSetRequest").unwrap();
    equip.set_field_by_name("character_id", Value::I32(character_id));
    equip.set_field_by_name(
        "memoria_entity_id",
        Value::Message(int32_value(&proto, entity_id).unwrap()),
    );
    apply_character_bulk_set(&rules, &mut resources, &mut changed, &equip).unwrap();
    let equipped = message_list(&resources, "characters")
        .into_iter()
        .find(|row| i32_field(row, "character_id") == Some(character_id))
        .unwrap();
    assert_eq!(
        message_i32_field(&equipped, "memoria_entity_id", "value"),
        Some(entity_id)
    );
    equip.clear_field_by_name("memoria_entity_id");
    apply_character_bulk_set(&rules, &mut resources, &mut changed, &equip).unwrap();
    let unequipped = message_list(&resources, "characters")
        .into_iter()
        .find(|row| i32_field(row, "character_id") == Some(character_id))
        .unwrap();
    assert!(!unequipped.has_field_by_name("memoria_entity_id"));

    let panel_id = 4;
    let panel: Vec<_> = rules
        .missions
        .iter()
        .filter(|row| row.guide_mission_step_id == Some(panel_id))
        .collect();
    for row in &panel {
        let count = row.steps.last().unwrap().count;
        if let Some(id) = row.total_task_condition_id {
            update_task(&mut resources, &mut changed, id, count).unwrap();
        } else {
            let mut value = empty_message(&proto, "blend.model.Mission").unwrap();
            value.set_field_by_name("mission_id", Value::I32(row.id));
            value.set_field_by_name("count", Value::I32(count));
            put(&mut resources, "missions", "mission_id", value);
        }
    }
    let mut claim = empty_message(&proto, "blend.api.MissionReceiveRequest").unwrap();
    claim.set_field_by_name(
        "mission_ids",
        Value::List(panel.iter().map(|row| Value::I32(row.id)).collect()),
    );
    let (_, panel_rewards) = claim_missions(
        &proto,
        &rules,
        &mut resources,
        &mut changed,
        &mut home,
        &claim,
        now,
    )
    .unwrap();
    assert!(!panel_rewards.is_empty());
    assert!(home.guide_rewards.contains(&panel_id));
    let before_retry = resources.clone();
    let (rewards, panel_rewards) = claim_missions(
        &proto,
        &rules,
        &mut resources,
        &mut changed,
        &mut home,
        &claim,
        now,
    )
    .unwrap();
    assert!(rewards.is_empty() && panel_rewards.is_empty());
    assert_eq!(resources, before_retry);

    let mut resources = starter_resources(&proto, &fresh).unwrap();
    let synthesis = load_synthesis_rules().unwrap();
    let target = &synthesis.recipes.iter().find(|r| r.id == 2).unwrap().target;
    for rank in [1, 3] {
        let traits = [
            TutorialTraitParam { id: 1, rank },
            TutorialTraitParam { id: 2, rank },
        ];
        let output = add_synthesis_output(&proto, &mut resources, target, &traits, now).unwrap();
        let mut changed = empty_message(&proto, "blend.model.Resources").unwrap();
        changed.set_field_by_name("battle_tools", Value::List(vec![Value::Message(output)]));
        synthesis_progress(&proto, &rules, &mut resources, &mut changed, 2, 1, now).unwrap();
        assert_eq!(total_task_count(&resources, 128), i32::from(rank == 3));
    }
    assert_eq!(total_task_count(&resources, 51), 2);
    assert_eq!(total_task_count(&resources, 52), 0);
    assert_eq!(total_task_count(&resources, 53), 2);
    assert_eq!(total_task_count(&resources, 120), 2);
    let recipe = synthesis
        .recipes
        .iter()
        .find(|r| r.target.resource_type == 6)
        .unwrap();
    let output = add_synthesis_output(&proto, &mut resources, &recipe.target, &[], now).unwrap();
    let mut changed = empty_message(&proto, "blend.model.Resources").unwrap();
    changed.set_field_by_name("equipment_tools", Value::List(vec![Value::Message(output)]));
    synthesis_progress(
        &proto,
        &rules,
        &mut resources,
        &mut changed,
        recipe.id,
        1,
        now,
    )
    .unwrap();
    assert_eq!(total_task_count(&resources, 52), 1);
    assert_eq!(total_task_count(&resources, 53), 2);
    // Trait-qualified synthesis counts matching outputs, not every craft.
    for trait_id in [12, 11] {
        let output = add_synthesis_output(
            &proto,
            &mut resources,
            &recipe.target,
            &[TutorialTraitParam {
                id: trait_id,
                rank: 1,
            }],
            now,
        )
        .unwrap();
        let mut delta = empty_message(&proto, "blend.model.Resources").unwrap();
        delta.set_field_by_name("equipment_tools", Value::List(vec![Value::Message(output)]));
        synthesis_progress(
            &proto,
            &rules,
            &mut resources,
            &mut delta,
            recipe.id,
            1,
            now,
        )
        .unwrap();
        assert_eq!(total_task_count(&resources, 241), i32::from(trait_id == 11));
    }
    update_task(&mut resources, &mut changed, 51, 30).unwrap();
    let mut claim = empty_message(&proto, "blend.api.MissionReceiveRequest").unwrap();
    claim.set_field_by_name("mission_ids", Value::List(vec![Value::I32(300060)]));
    claim.set_field_by_name("bulk_receive", Value::Bool(true));
    claim_missions(
        &proto,
        &rules,
        &mut resources,
        &mut changed,
        &mut HomeState::default(),
        &claim,
        now,
    )
    .unwrap();
    assert_eq!(received(&resources, 300060), 2);
}
