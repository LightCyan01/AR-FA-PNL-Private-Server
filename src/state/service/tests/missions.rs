use std::{env, path::Path};

use super::prelude::*;

fn test_state() -> (State, Session) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut config: crate::config::Config =
        toml::from_str(include_str!("../../../../config.example.toml")).unwrap();
    config.paths.protoset = root.join("../schemas/atelier-resleriana-2.16.0.protoset");
    config.paths.master_data = root.join("../data/master_data.pb");
    config.paths.master_data_decoded = Some(root.join("../data/masterdata.jp.decoded"));
    config.storage.path =
        env::temp_dir().join(format!("atelier-mission-{}.sqlite3", Uuid::new_v4()));
    let loaded = LoadedConfig {
        api_addr: "127.0.0.1:0".parse().unwrap(),
        asset_addr: "127.0.0.1:0".parse().unwrap(),
        config,
        server_secret: b"mission-service-test".to_vec(),
        config_path: root.join("config.example.toml"),
    };
    let proto = ProtoRegistry::from_file(&loaded.config.paths.protoset).unwrap();
    let state = State::new(
        Store::open(&loaded.config.storage.path).unwrap(),
        proto,
        loaded,
    )
    .unwrap();
    state
        .create_account("mission_service_test", "test-password")
        .unwrap();
    let grant = state
        .login_grant("mission_service_test", "test-password")
        .unwrap();
    let mut request = empty_message(&state.proto, "blend.api.AuthSignInRequest").unwrap();
    for (field, value) in [
        ("device_secret", grant),
        ("device_unique_id", "mission-service-device".into()),
        ("device_model", "mission-service".into()),
    ] {
        request.set_field_by_name(field, Value::String(value));
    }
    let signed = state.sign_in(&request).unwrap();
    let session = Session {
        account_id: signed.user_id,
        user_id: signed.user_id,
    };
    (state, session)
}

fn seed_quest(state: &State, session: &Session, quest_id: i32, stale_tasks: &[(i32, i32)]) {
    let route = "/test/mission-seed";
    let request_id = format!("seed-{quest_id}-{}", Uuid::new_v4());
    let fingerprint = request_fingerprint(route, request_id.as_bytes());
    let proto = state.proto.clone();
    let stale_tasks = stale_tasks.to_vec();
    state
        .store
        .apply_gameplay_reducer(
            session.account_id,
            &request_id,
            route,
            &fingerprint,
            unix_now(),
            move |stored| {
                let mut resources = proto
                    .decode("blend.model.Resources", stored)
                    .map_err(|error| StorageError::Battle(error.to_string()))?;
                let mut quest = empty_message(&proto, "blend.model.QuestState")
                    .map_err(|error| StorageError::Battle(error.to_string()))?;
                quest.set_field_by_name("quest_id", Value::I32(quest_id));
                quest.set_field_by_name("clear_count", Value::I32(1));
                upsert_quest_state(&mut resources, quest);
                for (condition, count) in stale_tasks {
                    set_total_task_count(&mut resources, condition, count)
                        .map_err(|error| StorageError::Battle(error.to_string()))?;
                }
                Ok(GameplayCommit {
                    resources_blob: resources.encode_to_vec(),
                    response_plaintext: Vec::new(),
                    character_index: Vec::new(),
                })
            },
        )
        .unwrap();
}

fn finish_won_battle(state: &State, session: &Session, quest_id: i32) -> DynamicMessage {
    let mut start = empty_message(&state.proto, "blend.api.QuestBattleStartRequest").unwrap();
    start.set_field_by_name("quest_id", Value::I32(quest_id));
    start.set_field_by_name("party_number", Value::I32(1));
    state
        .quest_battle_start(
            session,
            &format!("start-{quest_id}"),
            &start.encode_to_vec(),
            &start,
        )
        .unwrap();

    let route = "/battle/attack";
    let request_id = format!("force-win-{quest_id}");
    let fingerprint = request_fingerprint(route, request_id.as_bytes());
    let proto = state.proto.clone();
    state
        .store
        .apply_battle_attack(
            session.account_id,
            &request_id,
            route,
            &fingerprint,
            unix_now(),
            move |active, _resources, home| {
                let mut battle = proto
                    .decode("blend.model.BattleState", &active.state_blob)
                    .map_err(|error| StorageError::Battle(error.to_string()))?;
                let wave_count = i32_list(&battle, "wave_ids").len() as i32;
                battle.set_field_by_name("wave", Value::I32(wave_count));
                let mut members = message_list(&battle, "members");
                for member in &mut members {
                    if member_type(member).ok() == Some(1) {
                        member.set_field_by_name("hp", Value::I32(0));
                        member.set_field_by_name("is_alive", Value::Bool(false));
                    } else {
                        member.set_field_by_name("is_alive", Value::Bool(true));
                    }
                }
                battle.set_field_by_name(
                    "members",
                    Value::List(members.into_iter().map(Value::Message).collect()),
                );
                let response = empty_message(&proto, "blend.api.BattleAttackResponse")
                    .map_err(|error| StorageError::Battle(error.to_string()))?;
                Ok((
                    battle.encode_to_vec(),
                    response.encode_to_vec(),
                    home.to_vec(),
                ))
            },
        )
        .unwrap();

    state
        .battle_finish(session, &format!("finish-{quest_id}"), &[])
        .unwrap()
        .0
}

fn resources(state: &State, session: &Session) -> DynamicMessage {
    state
        .proto
        .decode(
            "blend.model.Resources",
            &state.store.player_resources(session.account_id).unwrap(),
        )
        .unwrap()
}

fn changed_task_count(resources: &DynamicMessage, condition_id: i32) -> Option<i32> {
    message_list(resources, "total_task_counts")
        .into_iter()
        .find(|row| i32_field(row, "condition_id") == Some(condition_id))
        .and_then(|row| i32_field(&row, "count"))
}

fn claim_mission(
    state: &State,
    session: &Session,
    mission_id: i32,
    request_id: &str,
) -> (DynamicMessage, GameplayMutationResult) {
    let mut request = empty_message(&state.proto, "blend.api.MissionReceiveRequest").unwrap();
    request.set_field_by_name("mission_ids", Value::List(vec![Value::I32(mission_id)]));
    request.set_field_by_name("bulk_receive", Value::Bool(true));
    let bytes = request.encode_to_vec();
    state
        .home_request(
            session,
            request_id,
            &bytes,
            "/mission/receive",
            &request,
            "blend.api.MissionReceiveResponse",
        )
        .unwrap()
}

#[test]
fn chapter_mission_mapping_survives_service_battle_finish() {
    let (state, session) = test_state();

    // Model the stale values observed on an existing account.
    seed_quest(
        &state,
        &session,
        101002006,
        &[(6, 1), (7, 1), (8, 2), (1061, 1), (186, 24)],
    );
    let first = finish_won_battle(&state, &session, 101002007);
    let first_changed = member_status(&first, "changed_resources").unwrap();
    let first_resources = resources(&state, &session);
    let item_total = message_list(&first_resources, "items")
        .into_iter()
        .find(|row| i32_field(row, "item_id") == Some(68))
        .and_then(|row| i32_field(&row, "total_quantity"))
        .unwrap();
    assert_eq!(changed_task_count(&first_changed, 5), Some(1));
    assert_eq!(changed_task_count(&first_changed, 6), Some(0));
    assert_eq!(changed_task_count(&first_changed, 7), Some(0));
    assert_eq!(changed_task_count(&first_changed, 8), Some(0));
    assert_eq!(changed_task_count(&first_changed, 1061), Some(0));
    assert_eq!(changed_task_count(&first_changed, 186), Some(item_total));
    assert_eq!(total_task_count(&first_resources, 5), 1);
    assert_eq!(total_task_count(&first_resources, 186), item_total);
    assert_eq!(total_task_count(&first_resources, 7), 0);
    assert_eq!(
        home::prelude::mission_count(
            &first_resources,
            state
                .home_rules
                .missions
                .iter()
                .find(|row| row.id == 51002)
                .unwrap(),
        ),
        1
    );
    assert_eq!(
        home::prelude::mission_count(
            &first_resources,
            state
                .home_rules
                .missions
                .iter()
                .find(|row| row.id == 51007)
                .unwrap(),
        ),
        0
    );

    let (claimed_502, applied_502) = claim_mission(&state, &session, 51002, "claim-51002");
    assert!(matches!(applied_502, GameplayMutationResult::Applied));
    assert!(!message_list(&claimed_502, "rewards").is_empty());
    assert_eq!(
        home::prelude::received(&resources(&state, &session), 51002),
        1
    );
    let (replayed_502, replay_status) = claim_mission(&state, &session, 51002, "claim-51002");
    assert!(matches!(replay_status, GameplayMutationResult::Replay(_)));
    assert_eq!(replayed_502, claimed_502);

    seed_quest(&state, &session, 101002019, &[(51, 5)]);
    let second = finish_won_battle(&state, &session, 101002020);
    let second_changed = member_status(&second, "changed_resources").unwrap();
    assert_eq!(changed_task_count(&second_changed, 7), Some(1));

    let second_resources = resources(&state, &session);
    assert_eq!(total_task_count(&second_resources, 7), 1);
    assert_eq!(
        home::prelude::mission_count(
            &second_resources,
            state
                .home_rules
                .missions
                .iter()
                .find(|row| row.id == 51007)
                .unwrap(),
        ),
        1
    );
    let (claimed_507, applied_507) = claim_mission(&state, &session, 51007, "claim-51007");
    assert!(matches!(applied_507, GameplayMutationResult::Applied));
    assert!(!message_list(&claimed_507, "rewards").is_empty());
    assert_eq!(
        home::prelude::received(&resources(&state, &session), 51007),
        1
    );
    let (replayed_507, replay_status) = claim_mission(&state, &session, 51007, "claim-51007");
    assert!(matches!(replay_status, GameplayMutationResult::Replay(_)));
    assert_eq!(replayed_507, claimed_507);
}
