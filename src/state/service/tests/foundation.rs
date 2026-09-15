use std::{env, fs, path::Path};

use super::*;

#[test]
fn historical_step_up_keeps_payment_order_and_retry_state() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_gameplay_rules().unwrap();
    assert_eq!(
        rules
            .gachas
            .iter()
            .filter(|g| !g.step_up_button_ids.is_empty())
            .count(),
        239
    );
    let mut resources = starter_resources(&proto, &load_fresh_rules().unwrap()).unwrap();
    let mut status = status_message(&resources).unwrap();
    status.set_field_by_name("tutorial_step", Value::I32(TUTORIAL_STEP_GACHA_COMPLETE));
    resources.set_field_by_name("status", Value::Message(status));
    let mut quest = empty_message(&proto, "blend.model.QuestState").unwrap();
    quest.set_field_by_name("quest_id", Value::I32(101001017));
    quest.set_field_by_name("clear_count", Value::I32(1));
    upsert_quest_state(&mut resources, quest);
    let mut wallet = empty_message(&proto, "blend.model.Wallet").unwrap();
    wallet.set_field_by_name("paid", Value::I32(3000));
    resources.set_field_by_name("wallet", Value::Message(wallet));
    let path = env::temp_dir().join(format!("atelier-step-up-{}.sqlite3", Uuid::new_v4()));
    let store = Store::open(&path).unwrap();
    store
        .create_account("step-up", "$argon2id$v=19$test")
        .unwrap();
    store
        .ensure_player_state(1, &resources.encode_to_vec(), "jp")
        .unwrap();
    let mut request = empty_message(&proto, "blend.api.GachaStepUpExecuteRequest").unwrap();
    request.set_field_by_name("gacha_id", Value::I32(2002));
    request.set_field_by_name("gacha_step_up_button_id", Value::I32(2));
    assert!(reduce_gacha_execute(
        &proto,
        &rules,
        resources.clone(),
        &request,
        0,
        None,
        unix_now()
    )
    .is_err());
    for step in [1, 2] {
        request.set_field_by_name("gacha_step_up_button_id", Value::I32(step));
        let bytes = request.encode_to_vec();
        let txid = format!("step-{step}");
        let route = "/gacha/step_up_execute";
        let fingerprint = request_fingerprint(route, &bytes);
        store
            .apply_gacha(
                1,
                &txid,
                route,
                &fingerprint,
                2002,
                -1,
                unix_now(),
                |stored, count, wish| {
                    let mutation = reduce_gacha_execute(
                        &proto,
                        &rules,
                        proto.decode("blend.model.Resources", stored).unwrap(),
                        &request,
                        count,
                        wish.as_ref(),
                        unix_now(),
                    )
                    .map_err(gameplay_storage_error)?;
                    assert_eq!(
                        message_list(&mutation.response, "drawn_rewards").len(),
                        if step == 1 { 10 } else { 1 }
                    );
                    assert_eq!(
                        message_i32_field(&mutation.response, "gacha", "step_up_execution_count"),
                        Some(step)
                    );
                    assert_eq!(
                        message_i32_field(&mutation.resources, "wallet", "paid"),
                        Some(0)
                    );
                    assert_eq!(item_quantity(&mutation.resources, 131), Some(11));
                    if step == 2 {
                        assert_eq!(
                            i32_field(
                                &message_list(&mutation.response, "drawn_rewards")[0],
                                "type"
                            ),
                            Some(4)
                        );
                    }
                    Ok(GachaCommit {
                        gameplay: GameplayCommit {
                            resources_blob: mutation.resources.encode_to_vec(),
                            response_plaintext: mutation.response.encode_to_vec(),
                            character_index: mutation.character_indexes,
                        },
                    })
                },
            )
            .unwrap();
        assert!(matches!(
            store
                .apply_gacha(
                    1,
                    &txid,
                    route,
                    &fingerprint,
                    2002,
                    -1,
                    unix_now(),
                    |_, _, _| panic!("retry drew again")
                )
                .unwrap(),
            GameplayMutationResult::Replay(_)
        ));
    }
    let saved = proto
        .decode("blend.model.Resources", &store.player_resources(1).unwrap())
        .unwrap();
    assert!(reduce_gacha_execute(&proto, &rules, saved, &request, 2, None, unix_now()).is_err());
    assert_eq!(store.gacha_button_states(1).unwrap()[0].execution_count, 2);
    drop(store);
    fs::remove_file(path).unwrap();
}

#[test]
fn starter_state_matches_verified_fresh_shape() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_fresh_rules().unwrap();
    let resources = starter_resources(&proto, &rules).unwrap();

    for field in ["wallet", "notifications", "status", "profile"] {
        assert!(resources.has_field_by_name(field), "missing {field}");
    }
    for (field, expected) in [
        ("characters", 1),
        ("items", 16),
        ("parties", 1),
        ("battle_tools", 1),
        ("recipes", 2),
        ("party_members", 5),
        ("total_task_counts", 3008),
    ] {
        assert_eq!(
            resources
                .get_field_by_name(field)
                .and_then(|value| value.as_list().map(|list| list.len())),
            Some(expected),
            "unexpected {field} count"
        );
    }

    let task_count = |condition_id: i32| {
        resources
            .get_field_by_name("total_task_counts")
            .and_then(|value| value.as_list().map(|tasks| tasks.to_vec()))
            .and_then(|tasks| {
                tasks.iter().find_map(|task| {
                    let task = task.as_message()?;
                    if task.get_field_by_name("condition_id")?.as_i32()? == condition_id {
                        task.get_field_by_name("count")?.as_i32()
                    } else {
                        None
                    }
                })
            })
    };
    for (condition_id, expected) in [
        (1, 1),
        (47, 1),
        (55, 5),
        (56, 5),
        (85, 1),
        (109, 1000),
        (134, 2),
        (1434, 1),
        (186, 13),
        (187, 7),
        (189, 5),
        (195, 5),
        (208, 5),
        (290, 65),
        (293, 25),
    ] {
        assert_eq!(
            task_count(condition_id),
            Some(expected),
            "condition {condition_id}"
        );
    }
    for condition_id in [1464, 1477, 1914, 1929, 2703, 2851] {
        assert_eq!(
            task_count(condition_id),
            Some(0),
            "unresolved {condition_id}"
        );
    }
}

#[test]
fn opening_talk_reduces_from_rules_without_replay_data() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let fresh_rules = load_fresh_rules().unwrap();
    let tutorial_rules = load_tutorial_rules().unwrap();
    let resources = starter_resources(&proto, &fresh_rules).unwrap();

    assert!(matches!(
        reduce_talk_event(
            &proto,
            &fresh_rules,
            &tutorial_rules,
            resources.clone(),
            101001003,
            1,
        ),
        Err(StateError::InvalidRequest)
    ));

    let mutation = reduce_talk_event(
        &proto,
        &fresh_rules,
        &tutorial_rules,
        resources,
        101001001,
        1,
    )
    .unwrap();
    assert_eq!(quest_clear_count(&mutation.resources, 101001001), 1);
    assert!(character_present(&mutation.resources, 32201));
    assert_eq!(
        message_list(&mutation.resources, "party_members")
            .iter()
            .find(|member| i32_field(member, "position") == Some(2))
            .and_then(|member| optional_i32_field(member, "character_id")),
        None
    );

    let status = mutation
        .resources
        .get_field_by_name("status")
        .and_then(|value| value.as_message().cloned())
        .unwrap();
    assert_eq!(
        status
            .get_field_by_name("last_main_story_quest_id")
            .and_then(|value| value.as_i32()),
        Some(101001001)
    );
    let character = mutation
        .resources
        .get_field_by_name("characters")
        .and_then(|value| value.as_list().map(|characters| characters.to_vec()))
        .and_then(|characters| {
            characters.into_iter().find_map(|character| {
                let character = character.as_message()?.clone();
                (character.get_field_by_name("character_id")?.as_i32()? == 32201)
                    .then_some(character)
            })
        })
        .unwrap();
    assert_eq!(
        character
            .get_field_by_name("rarity")
            .and_then(|value| value.as_i32()),
        Some(2)
    );
    assert_eq!(
        character
            .get_field_by_name("exp")
            .and_then(|value| value.as_i32()),
        Some(5)
    );

    let changed = mutation
        .response
        .get_field_by_name("changed_resources")
        .and_then(|value| value.as_message().cloned())
        .unwrap();
    assert_eq!(quest_clear_count(&changed, 101001001), 1);
    assert_eq!(total_task_count(&changed, 47), 2);
    assert_eq!(total_task_count(&changed, 85), 2);
    assert_eq!(
        member_status(&changed, "status")
            .ok()
            .and_then(|status| i32_field(&status, "growboard_max_page")),
        Some(16)
    );
    assert!(message_list(&changed, "party_members").is_empty());
    assert_eq!(
        mutation
            .response
            .get_field_by_name("first_clear_rewards")
            .and_then(|value| value.as_list().map(|rewards| rewards.len())),
        Some(1)
    );

    let path = env::temp_dir().join(format!(
        "atelier-talk-reducer-{}.sqlite3",
        uuid::Uuid::new_v4()
    ));
    let store = Store::open(&path).unwrap();
    store
        .create_account("talk_reducer", "$argon2id$v=19$test")
        .unwrap();
    store.ensure_player_state(1, b"old", "jp").unwrap();
    assert_eq!(
        store
            .apply_gameplay(
                1,
                "talk-1",
                "/quest/talk_event/finish",
                &[1; 32],
                &mutation.resources.encode_to_vec(),
                mutation.granted_character_id.map(i64::from),
                &mutation.response.encode_to_vec(),
                1,
            )
            .unwrap(),
        GameplayMutationResult::Applied
    );
    drop(store);

    let reopened = Store::open(&path).unwrap();
    let persisted = proto
        .decode(
            "blend.model.Resources",
            &reopened.player_resources(1).unwrap(),
        )
        .unwrap();
    assert_eq!(quest_clear_count(&persisted, 101001001), 1);
    assert!(character_present(&persisted, 32201));
    drop(reopened);
    let _ = fs::remove_file(path);
}

#[test]
fn every_talk_story_can_be_replayed_without_rewards() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let fresh_rules = load_fresh_rules().unwrap();
    let tutorial_rules = load_gameplay_rules().unwrap();
    let mut base = starter_resources(&proto, &fresh_rules).unwrap();
    let tasks = message_list(&base, "total_task_counts")
        .into_iter()
        .map(|mut task| {
            task.set_field_by_name("count", Value::I32(i32::MAX));
            Value::Message(task)
        })
        .collect();
    base.set_field_by_name("total_task_counts", Value::List(tasks));

    let stories: Vec<_> = tutorial_rules
        .quests
        .iter()
        .filter(|quest| quest.quest_type == 2 && quest.talk_event_id.is_some())
        .collect();
    assert_eq!(stories.len(), 1_888);
    for quest in stories {
        let mut resources = base.clone();
        let mut states = Vec::new();
        for id in [Some(quest.id), quest.predecessor_id].into_iter().flatten() {
            let mut state = empty_message(&proto, "blend.model.QuestState").unwrap();
            state.set_field_by_name("quest_id", Value::I32(id));
            state.set_field_by_name("clear_count", Value::I32(1));
            states.push(Value::Message(state));
        }
        resources.set_field_by_name("quest_states", Value::List(states));
        let now = quest
            .start_at
            .unwrap_or_else(|| quest.end_at.map(|end| end - 1).unwrap_or(1));
        let mutation = reduce_talk_event(
            &proto,
            &fresh_rules,
            &tutorial_rules,
            resources,
            quest.id,
            now,
        )
        .unwrap_or_else(|error| panic!("story {} failed: {error:?}", quest.id));
        assert_eq!(quest_clear_count(&mutation.resources, quest.id), 2);
        assert_eq!(
            mutation
                .response
                .get_field_by_name("first_clear_rewards")
                .and_then(|value| value.as_list().map(|rewards| rewards.len())),
            Some(0),
            "story {} repeated first-clear rewards",
            quest.id
        );
    }
    println!("ALL_TALK_STORIES_REPLAY_OK stories=1888");
}

#[test]
fn tutorial_battle_start_is_rule_derived_and_persistent() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let fresh_rules = load_fresh_rules().unwrap();
    let tutorial_rules = load_tutorial_rules().unwrap();
    let resources = starter_resources(&proto, &fresh_rules).unwrap();
    let opening = reduce_talk_event(
        &proto,
        &fresh_rules,
        &tutorial_rules,
        resources,
        101001001,
        1,
    )
    .unwrap();
    let mutation = reduce_battle_start(
        &proto,
        &tutorial_rules,
        opening.resources.clone(),
        101001002,
        1,
    )
    .unwrap();
    assert_eq!(mutation.battle_id, 10000279);
    assert!(!mutation.start_txid.is_empty());
    let history = mutation
        .response
        .get_field_by_name("history")
        .and_then(|value| value.as_message().cloned())
        .unwrap();
    let start = history
        .get_field_by_name("start")
        .and_then(|value| value.as_message().cloned())
        .unwrap();
    let start_state = start
        .get_field_by_name("state")
        .and_then(|value| value.as_message().cloned())
        .unwrap();
    assert_eq!(i32_field(&start_state, "battle_id"), Some(10000279));
    assert_eq!(i32_field(&start_state, "total_turn"), Some(1));
    assert_eq!(message_list(&start_state, "members").len(), 1);
    assert!(message_list(&start_state, "timeline_units").is_empty());
    let wave_starts = message_list(&history, "wave_starts");
    let state = wave_starts[0]
        .get_field_by_name("state")
        .and_then(|value| value.as_message().cloned())
        .unwrap();
    assert_eq!(i32_field(&state, "battle_id"), Some(10000279));
    assert_eq!(
        i32_list(&state, "wave_ids"),
        vec![100002791, 100002792, 100002793]
    );
    assert_eq!(message_list(&state, "members").len(), 3);
    assert!(!state.has_field_by_name("enemy_hp_gauge"));
    assert!(message_list(&state, "timeline_panels")
        .iter()
        .all(|panel| !panel.has_field_by_name("panel_id")));
    let members = message_list(&state, "members");
    let ally = members
        .iter()
        .find(|member| member_type(member).ok() == Some(0))
        .unwrap();
    assert_eq!(i32_field(ally, "max_hp"), Some(142));
    assert!(members
        .iter()
        .filter(|member| member_type(member).ok() == Some(1))
        .all(|member| i32_field(member, "max_hp") == Some(68)));
    let units = message_list(&state, "timeline_units");
    assert_eq!(units.len(), 8);
    for member_id in [1, 11, 12] {
        let expected = if member_id >= 11 { 3 } else { 2 };
        let member_units: Vec<_> = units
            .iter()
            .filter(|unit| i32_field(unit, "member_id") == Some(member_id))
            .collect();
        assert_eq!(member_units.len(), expected);
        assert_eq!(
            member_units
                .iter()
                .filter_map(|unit| i32_field(unit, "number"))
                .collect::<Vec<_>>(),
            (1..=expected as i32).collect::<Vec<_>>()
        );
    }
    assert_eq!(member_type(&current_actor(&state).unwrap()).unwrap(), 0);
    assert_eq!(i32_field(&state, "party_gauge"), Some(500));
    assert_eq!(wave_starts.len(), 1);
    assert_eq!(i32_field(&wave_starts[0], "action_number"), Some(1));
    assert_eq!(
        wave_starts[0]
            .get_field_by_name("state")
            .and_then(|value| value.as_message().cloned())
            .map(|value| i32_field(&value, "battle_id")),
        Some(Some(10000279))
    );
    let action_setups = message_list(&history, "action_setups");
    assert_eq!(action_setups.len(), 1);
    assert_eq!(i32_field(&action_setups[0], "number"), Some(1));
    assert_eq!(i32_field(&action_setups[0], "actor_id"), Some(1));
    assert_eq!(i32_or_enum_field(&action_setups[0], "actor_type"), Some(0));
    assert_eq!(message_list(&action_setups[0], "skill_selections").len(), 2);
    assert!(action_setups[0]
        .get_field_by_name("state")
        .and_then(|value| value.as_message().cloned())
        .is_some());

    let path = env::temp_dir().join(format!("atelier-battle-start-{}.sqlite3", Uuid::new_v4()));
    let store = Store::open(&path).unwrap();
    store
        .create_account("battle_start_test", "$argon2id$v=19$test")
        .unwrap();
    store
        .ensure_player_state(1, &opening.resources.encode_to_vec(), "jp")
        .unwrap();
    let fingerprint = [3u8; 32];
    let response_bytes = mutation.response.encode_to_vec();
    assert_eq!(
        store
            .apply_battle_start(
                1,
                "battle-1",
                "/quest/battle/start",
                &fingerprint,
                101001002,
                10000279,
                &mutation.start_txid,
                &mutation.state.encode_to_vec(),
                &response_bytes,
                1,
            )
            .unwrap(),
        GameplayMutationResult::Applied
    );
    assert!(matches!(
        store.apply_battle_start(
            1,
            "battle-2",
            "/quest/battle/start",
            &[4u8; 32],
            101001002,
            10000279,
            "another-txid",
            &mutation.state.encode_to_vec(),
            &response_bytes,
            1,
        ),
        Err(StorageError::ActiveBattleExists)
    ));
    drop(store);
    let reopened = Store::open(&path).unwrap();
    let active = reopened.active_battle(1).unwrap().unwrap();
    assert_eq!(active.quest_id, 101001002);
    assert_eq!(active.battle_id, 10000279);
    assert_eq!(active.state_blob, mutation.state.encode_to_vec());
    let resume = battle_resume_response(&proto, &active).unwrap();
    assert_eq!(
        message_i32_field(&resume, "context", "quest_id"),
        Some(101001002)
    );
    assert_eq!(
        resume
            .get_field_by_name("history")
            .and_then(|value| value.as_message().cloned())
            .and_then(|history| {
                history
                    .get_field_by_name("previous_state")
                    .map(|value| value.into_owned())
            })
            .and_then(|value| value.as_message().cloned())
            .and_then(|state| i32_field(&state, "battle_id")),
        Some(10000279)
    );
    assert_eq!(
        reopened
            .gameplay_replay(1, "battle-1", "/quest/battle/start", &fingerprint)
            .unwrap(),
        Some(response_bytes)
    );
    assert_eq!(
        reopened
            .apply_battle_finish(
                1,
                "retire-1",
                "/battle/retire",
                &[5u8; 32],
                2,
                |_active, resources, _home| Ok(BattleFinishCommit {
                    resources_blob: resources.to_vec(),
                    response_plaintext: b"retired".to_vec(),
                    character_index: Vec::new(),
                    home_state: None,
                }),
            )
            .unwrap(),
        GameplayMutationResult::Applied
    );
    assert!(reopened.active_battle(1).unwrap().is_none());
    assert_eq!(
        reopened
            .apply_battle_finish(
                1,
                "retire-1",
                "/battle/retire",
                &[5u8; 32],
                3,
                |_, _, _| panic!("an idempotent retire must not run twice"),
            )
            .unwrap(),
        GameplayMutationResult::Replay(b"retired".to_vec())
    );
    assert!(matches!(
        reduce_battle_start(&proto, &tutorial_rules, opening.resources, 101001008, 1),
        Err(StateError::InvalidRequest)
    ));
    drop(reopened);
    let _ = fs::remove_file(path);
}

#[test]
fn combat_level_comes_from_persisted_character_exp() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let fresh_rules = load_fresh_rules().unwrap();
    let tutorial_rules = load_tutorial_rules().unwrap();
    let mut resources = starter_resources(&proto, &fresh_rules).unwrap();
    let character_id = fresh_rules.initial_character.id;
    let mut character = resource_character(&resources, character_id).unwrap();
    character.set_field_by_name(
        "exp",
        Value::I32(
            tutorial_rules
                .character_levels
                .iter()
                .find(|row| row.level == 10)
                .unwrap()
                .exp,
        ),
    );
    let rarity = i32_field(&character, "rarity").unwrap();
    upsert_character(&mut resources, character, false);
    let member = build_ally_member(
        &proto,
        &tutorial_rules,
        &BattlePartyMember {
            character_id,
            level: 10,
            rarity,
            memoria_id: None,
            position: 1,
            is_leader: true,
            integrated_stats: Some(BattleStats {
                hp: 99_999,
                speed: 999,
                attack: 999,
                magic: 999,
                defense: 999,
                mental: 999,
            }),
            damage_bonus: 0,
            skills: selected_character_skills(&tutorial_rules, character_id, None, rarity).unwrap(),
            ability_ids: Vec::new(),
            passives: Vec::new(),
            leader_passives: Vec::new(),
        },
        1,
        10000003,
        0,
    )
    .unwrap();

    assert_eq!(
        member_level(&proto, &tutorial_rules, &member, Some(&resources)).unwrap(),
        10
    );
    assert!(member_level(&proto, &tutorial_rules, &member, None).is_err());
}
