#[test]
fn master_drop_rewards_roll_and_apply() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let fresh = load_fresh_rules().unwrap();
    let rules = load_reward_rules().unwrap();
    let mut resources = starter_resources(&proto, &fresh).unwrap();
    let old_cole = i32_field(&status_message(&resources).unwrap(), "cole").unwrap();
    let rolled = roll_quest_rewards(&rules, 204052001).unwrap();
    let mut changed = empty_message(&proto, "blend.model.Resources").unwrap();
    let rewards = apply_quest_rewards(&proto, &mut resources, &mut changed, &rolled).unwrap();

    assert_eq!(
        i32_field(&status_message(&resources).unwrap(), "cole"),
        Some(old_cole + 100)
    );
    assert!(matches!(item_quantity(&resources, 98), Some(3 | 5)));
    assert_eq!(rewards.len(), 2);
    assert_eq!(message_list(&changed, "items").len(), 1);
    let mut ranked_rules = rules.clone();
    let quest = ranked_rules
        .quests
        .iter_mut()
        .find(|quest| quest.id == 204052001)
        .unwrap();
    quest.score_ranks = vec![
        ScoreRankDrops {
            rank: 1,
            reward_set_ids: Vec::new(),
            drop_reward_set_ids: std::mem::take(&mut quest.drop_reward_set_ids),
        },
        ScoreRankDrops {
            rank: 2,
            reward_set_ids: Vec::new(),
            drop_reward_set_ids: Vec::new(),
        },
    ];
    assert_eq!(
        roll_ranked_quest_rewards(&ranked_rules, 204052001, Some(1))
            .unwrap()
            .len(),
        2
    );
    assert!(roll_ranked_quest_rewards(&ranked_rules, 204052001, Some(2))
        .unwrap()
        .is_empty());
    assert!(matches!(
        roll_ranked_quest_rewards(&ranked_rules, 204052001, Some(99)),
        Err(StateError::InvalidRequest)
    ));
}

#[test]
fn storage_reopens_synthesis_state() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let fresh = load_fresh_rules().unwrap();
    let tutorial = load_tutorial_rules().unwrap();
    let rules = load_synthesis_rules().unwrap();
    let resources = synthesis_test_resources(&proto, &fresh, &tutorial);
    let request = synthesis_request(&proto, SynthesisMode::Bulk);
    let request_bytes = request.encode_to_vec();
    let fingerprint = request_fingerprint("/synthesis/bulk_execute", &request_bytes);
    let path = env::temp_dir().join(format!(
        "atelier-synthesis-{}.sqlite3",
        uuid::Uuid::new_v4()
    ));
    let store = Store::open(&path).unwrap();
    store
        .create_account("synthesis_reopen", "$argon2id$v=19$test")
        .unwrap();
    store
        .ensure_player_state(1, &resources.encode_to_vec(), "jp")
        .unwrap();
    let apply = |stored: &[u8]| {
        let resources = proto.decode("blend.model.Resources", stored).unwrap();
        let mutation = reduce_synthesis(
            &proto,
            &rules,
            &home::load_rules().unwrap(),
            &activities::load_rules().unwrap(),
            resources,
            &request,
            SynthesisMode::Bulk,
            1,
        )
        .unwrap();
        Ok(GameplayCommit {
            resources_blob: mutation.resources.encode_to_vec(),
            response_plaintext: mutation.response.encode_to_vec(),
            character_index: Vec::new(),
        })
    };
    assert_eq!(
        store
            .apply_gameplay_reducer(
                1,
                "synthesis-1",
                "/synthesis/bulk_execute",
                &fingerprint,
                1,
                apply,
            )
            .unwrap(),
        GameplayMutationResult::Applied
    );
    assert!(matches!(
        store
            .apply_gameplay_reducer(
                1,
                "synthesis-1",
                "/synthesis/bulk_execute",
                &fingerprint,
                2,
                |_| panic!("idempotent replay must not rerun synthesis"),
            )
            .unwrap(),
        GameplayMutationResult::Replay(_)
    ));
    drop(store);
    let reopened = Store::open(&path).unwrap();
    let persisted = proto
        .decode(
            "blend.model.Resources",
            &reopened.player_resources(1).unwrap(),
        )
        .unwrap();
    assert!((2..=4).contains(&message_list(&persisted, "battle_tools").len()));
    assert_eq!(item_quantity(&persisted, 67), Some(1));
    assert!(reopened
        .gameplay_replay(1, "synthesis-1", "/synthesis/bulk_execute", &fingerprint,)
        .unwrap()
        .is_some());
    let persisted_bytes = reopened.player_resources(1).unwrap();
    assert!(matches!(
        reopened.apply_gameplay_reducer(
            1,
            "synthesis-1",
            "/synthesis/bulk_execute",
            &[0; 32],
            3,
            |_| panic!("a conflicting request must not rerun synthesis"),
        ),
        Err(StorageError::RequestConflict)
    ));
    assert_eq!(reopened.player_resources(1).unwrap(), persisted_bytes);
    drop(reopened);
    let _ = fs::remove_file(path);
}

#[test]
fn timeline_continues_until_terminal() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let fresh = load_fresh_rules().unwrap();
    let rules = load_tutorial_rules().unwrap();
    let opening = reduce_talk_event(
        &proto,
        &fresh,
        &rules,
        starter_resources(&proto, &fresh).unwrap(),
        101001001,
        1,
    )
    .unwrap();
    let start = reduce_battle_start(&proto, &rules, opening.resources, 101001002, 1).unwrap();
    let mut state = start.state;
    let mut members = message_list(&state, "members");
    for member in &mut members {
        member.set_field_by_name("hp", Value::I32(1_000_000));
    }
    state.set_field_by_name(
        "members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );
    for _ in 0..20 {
        let target = earliest_living_member(&state, Some(1)).unwrap();
        let attack = tutorial_skill_attack(
            &proto,
            &rules,
            state,
            &start.start_txid,
            1,
            member_id(&target).unwrap(),
        );
        state = attack.state;
        if current_battle_status(&state).unwrap() != BATTLE_STATUS_IN_BATTLE {
            break;
        }
        assert!(!message_list(&state, "timeline_units").is_empty());
        let history = member_status(&attack.response, "history").unwrap();
        assert!(message_list(&history, "action_setups")
            .iter()
            .any(|setup| i32_or_enum_field(setup, "actor_type") == Some(0)));
    }
    assert_eq!(
        current_battle_status(&state).unwrap(),
        BATTLE_STATUS_IN_BATTLE
    );
}
use super::*;
