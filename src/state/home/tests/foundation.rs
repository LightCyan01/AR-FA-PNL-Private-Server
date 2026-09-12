use super::*;
use prost::Message;
use std::path::Path;

#[test]
fn atelier_index_covers_loaded_home_rule_events() {
    let rules = load_rules().unwrap();
    let (mission, event) = rules
        .missions
        .iter()
        .find_map(|mission| {
            mission
                .objective
                .counter
                .as_ref()
                .map(|event| (mission, event))
                .or_else(|| {
                    mission
                        .objective
                        .counters
                        .first()
                        .map(|event| (mission, event))
                })
        })
        .expect("home rules contain indexed mission events");
    assert!(rules
        .mission_index
        .missions_for_event(event)
        .contains(&mission.id));
    assert!(rules.mission_index.indexed_condition_count() > 0);
}

#[test]
fn cole_reward_ticks_shared_mission_counter_once() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_rules().unwrap();
    let before = starter_resources(&proto, &load_fresh_rules().unwrap()).unwrap();
    let mut resources = before.clone();
    let mut changed = empty_message(&proto, "blend.model.Resources").unwrap();
    grant(
        &proto,
        &rules,
        &mut resources,
        &mut changed,
        &[TutorialReward {
            resource_type: 3,
            id: 1,
            quantity: 50,
            resource_params: None,
        }],
        unix_now(),
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
    assert_eq!(
        total_task_count(&resources, 109) - total_task_count(&before, 109),
        50
    );
    assert_eq!(
        message_i32_field(&resources, "status", "cole").unwrap()
            - message_i32_field(&before, "status", "cole").unwrap(),
        50
    );
}

#[test]
fn quest_and_rental_utilities_follow_static_rules() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_rules().unwrap();
    let fresh = load_fresh_rules().unwrap();
    let reward_rules = load_reward_rules().unwrap();
    let characters = load_character_rules().unwrap();
    let mut resources = starter_resources(&proto, &fresh).unwrap();
    let mut wallet = empty_message(&proto, "blend.model.Wallet").unwrap();
    wallet.set_field_by_name("free", Value::I32(100));
    resources.set_field_by_name("wallet", Value::Message(wallet));
    let quest_id = 204053001;
    let mut quest_state = empty_message(&proto, "blend.model.QuestState").unwrap();
    quest_state.set_field_by_name("quest_id", Value::I32(quest_id));
    quest_state.set_field_by_name("clear_count", Value::I32(1));
    quest_state.set_field_by_name(
        "cleared_battle_mission_ids",
        Value::List(vec![Value::I32(1), Value::I32(2), Value::I32(6)]),
    );
    put(&mut resources, "quest_states", "quest_id", quest_state);
    let before_exp = message_list(&resources, "characters")
        .iter()
        .map(|character| i32_field(character, "exp").unwrap_or(0))
        .sum::<i32>();
    let party_character_count = message_list(&resources, "party_members")
        .iter()
        .filter(|member| {
            i32_field(member, "party_type") == Some(1)
                && i32_field(member, "number") == Some(1)
                && optional_i32_field(member, "character_id").is_some()
        })
        .count() as i32;
    let mut home = HomeState::default();

    let mut add = empty_message(&proto, "blend.api.QuestDailyClearAddRequest").unwrap();
    add.set_field_by_name("episode_id", Value::I32(204053));
    add.set_field_by_name("count", Value::I32(2));
    resources = proto
        .decode(
            "blend.model.Resources",
            &reduce_home(
                &proto,
                &rules,
                characters,
                resources,
                &mut home,
                "/quest/daily_clear_add",
                &add,
                "blend.api.ChangedResourcesResponse",
                1_800_000_000,
            )
            .unwrap()
            .resources_blob,
        )
        .unwrap();
    assert_eq!(
        i32_field(&member_status(&resources, "wallet").unwrap(), "free"),
        Some(50)
    );

    let mut skip = empty_message(&proto, "blend.api.QuestBattleSkipRequest").unwrap();
    skip.set_field_by_name("quest_id", Value::I32(quest_id));
    skip.set_field_by_name("party_number", Value::I32(1));
    skip.set_field_by_name("skip_count", Value::I32(2));
    let mut ranked_rules = reward_rules.clone();
    let drops = ranked_rules
        .quests
        .iter()
        .find(|q| q.id == 204052001)
        .unwrap()
        .drop_reward_set_ids
        .clone();
    let ranked_quest = ranked_rules
        .quests
        .iter_mut()
        .find(|q| q.id == quest_id)
        .unwrap();
    ranked_quest.drop_reward_set_ids.clear();
    ranked_quest.score_ranks = vec![ScoreRankDrops {
        rank: 5,
        reward_set_ids: Vec::new(),
        drop_reward_set_ids: drops,
    }];
    let mut ranked = resources.clone();
    let mut ranked_state = message_list(&ranked, "quest_states")
        .into_iter()
        .find(|q| i32_field(q, "quest_id") == Some(quest_id))
        .unwrap();
    ranked_state.set_field_by_name("score_rank", Value::I32(5));
    put(&mut ranked, "quest_states", "quest_id", ranked_state);
    let mut delta = empty_message(&proto, "blend.model.Resources").unwrap();
    let mut response = empty_message(&proto, "blend.api.QuestBattleSkipResponse").unwrap();
    quest_skip(
        &proto,
        &rules,
        &ranked_rules,
        &mut ranked,
        &mut delta,
        &skip,
        &mut response,
        1_800_000_001,
    )
    .unwrap();
    assert_eq!(
        message_list(
            &member_status(&response, "quest_result").unwrap(),
            "rewards"
        )
        .len(),
        2
    );
    assert!(message_list(&ranked, "quest_states")
        .iter()
        .any(|q| i32_field(q, "quest_id") == Some(quest_id)
            && i32_field(q, "score_rank") == Some(5)
            && i32_field(q, "clear_count") == Some(3)));
    let mut rejected = resources.clone();
    let delta_before = delta.clone();
    let response_before = response.clone();
    assert!(matches!(
        quest_skip(
            &proto,
            &rules,
            &ranked_rules,
            &mut rejected,
            &mut delta,
            &skip,
            &mut response,
            1_800_000_001
        ),
        Err(StateError::InvalidRequest)
    ));
    assert_eq!(rejected, resources);
    assert_eq!(delta, delta_before);
    assert_eq!(response, response_before);
    resources = proto
        .decode(
            "blend.model.Resources",
            &reduce_home(
                &proto,
                &rules,
                characters,
                resources,
                &mut home,
                "/quest/battle/skip",
                &skip,
                "blend.api.QuestBattleSkipResponse",
                1_800_000_001,
            )
            .unwrap()
            .resources_blob,
        )
        .unwrap();
    assert!(message_list(&resources, "quest_states")
        .iter()
        .any(|state| {
            i32_field(state, "quest_id") == Some(quest_id)
                && i32_field(state, "clear_count") == Some(3)
        }));
    assert_eq!(
        message_list(&resources, "characters")
            .iter()
            .map(|character| i32_field(character, "exp").unwrap_or(0))
            .sum::<i32>(),
        before_exp + 100 * party_character_count
    );

    let fixed = &reward_rules.fixed_parties[0];
    let mut rental = empty_message(&proto, "blend.api.RentalPartyBulkUpdateRequest").unwrap();
    rental.set_field_by_name("fixed_party_id", Value::I32(fixed.id));
    rental.set_field_by_name(
        "members",
        Value::List(
            fixed
                .character_ids
                .iter()
                .map(|id| {
                    let mut member =
                        empty_message(&proto, "blend.model.PartyMemberWithEquipment").unwrap();
                    member.set_field_by_name(
                        "character_id",
                        Value::Message(int32_value(&proto, *id).unwrap()),
                    );
                    Value::Message(member)
                })
                .collect(),
        ),
    );
    resources = proto
        .decode(
            "blend.model.Resources",
            &reduce_home(
                &proto,
                &rules,
                characters,
                resources,
                &mut home,
                "/rental_party/bulk_update",
                &rental,
                "blend.api.ChangedResourcesResponse",
                1_800_000_002,
            )
            .unwrap()
            .resources_blob,
        )
        .unwrap();
    assert!(message_list(&resources, "rental_parties")
        .iter()
        .any(|party| i32_field(party, "fixed_party_id") == Some(fixed.id)));
    assert_eq!(
        message_list(&resources, "rental_party_members")
            .iter()
            .filter(|member| i32_field(member, "fixed_party_id") == Some(fixed.id))
            .count(),
        fixed.character_ids.len()
    );

    let mut list = empty_message(&proto, "blend.api.QuestClearedPartyListRequest").unwrap();
    list.set_field_by_name("quest_id", Value::I32(quest_id));
    let response = reduce_home(
        &proto,
        &rules,
        characters,
        resources,
        &mut home,
        "/quest/cleared_party_list",
        &list,
        "blend.api.QuestClearedPartyListResponse",
        1_800_000_003,
    )
    .unwrap();
    let response = proto
        .decode(
            "blend.api.QuestClearedPartyListResponse",
            &response.response_plaintext,
        )
        .unwrap();
    assert_eq!(message_list(&response, "cleared_parties").len(), 1);
}

#[test]
fn character_progression_and_saved_party_persist_atomically() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_rules().unwrap();
    let characters = load_character_rules().unwrap();
    let fresh = load_fresh_rules().unwrap();
    let mut resources = starter_resources(&proto, &fresh).unwrap();
    upsert_character(
        &mut resources,
        character_message(&proto, 25501, None, Some(1), 3, 10).unwrap(),
        false,
    );
    let mut status = status_message(&resources).unwrap();
    status.set_field_by_name("cole", Value::I32(100_000));
    status.set_field_by_name("growboard_max_page", Value::I32(16));
    resources.set_field_by_name("status", Value::Message(status));
    for id in [103, 104, 108, 111, 114, 117, 120] {
        change_item(&proto, &mut resources, id, 100).unwrap();
    }
    change_character_piece(&proto, &mut resources, 25501, 100).unwrap();
    let path = std::env::temp_dir().join(format!("atelier-character-{}.sqlite3", Uuid::new_v4()));
    let store = Store::open(&path).unwrap();
    let account = store.create_account("character-test", "test-only").unwrap();
    store
        .ensure_player_state(account, &resources.encode_to_vec(), "test")
        .unwrap();
    let run = |store: &Store, id: &str, route: &str, request: &DynamicMessage| {
        let fp = request_fingerprint(route, &request.encode_to_vec());
        store.apply_home_reducer(account, id, route, &fp, 100, |saved, home| {
            let mut home = serde_json::from_slice(home).unwrap();
            let commit = reduce_home(
                &proto,
                &rules,
                characters,
                proto.decode("blend.model.Resources", saved).unwrap(),
                &mut home,
                route,
                request,
                if route == "/character/enhancement_reset" {
                    "blend.api.CharacterEnhancementResetResponse"
                } else {
                    "blend.api.ChangedResourcesResponse"
                },
                100,
            )
            .map_err(gameplay_storage_error)?;
            Ok((commit, serde_json::to_vec(&home).unwrap()))
        })
    };
    let mut board =
        empty_message(&proto, "blend.api.CharacterGrowboardBulkReleaseRequest").unwrap();
    board.set_field_by_name("character_id", Value::I32(25501));
    board.set_field_by_name("target_page", Value::I32(1));
    board.set_field_by_name("panel_bits", Value::I32(1));
    run(
        &store,
        "board1",
        "/character/growboard_bulk_release",
        &board,
    )
    .unwrap();
    board.set_field_by_name("target_page", Value::I32(2));
    board.set_field_by_name("panel_bits", Value::I32(95));
    run(
        &store,
        "board2",
        "/character/growboard_bulk_release",
        &board,
    )
    .unwrap();
    let saved = proto
        .decode(
            "blend.model.Resources",
            &store.player_resources(account).unwrap(),
        )
        .unwrap();
    let character = resource_character(&saved, 25501).unwrap();
    assert_eq!(i32_field(&character, "growboard_hp"), Some(22));
    assert_eq!(i32_field(&character, "growboard_magic"), Some(11));
    assert_eq!(i32_field(&character, "growboard_level_limit"), Some(20));
    assert_eq!(
        i32_field(&status_message(&saved).unwrap(), "cole"),
        Some(94_200)
    );
    let mut enhance = empty_message(&proto, "blend.api.CharacterEnhanceRequest").unwrap();
    enhance.set_field_by_name("character_id", Value::I32(25501));
    let mut consumed = empty_message(&proto, "blend.model.ConsumedItem").unwrap();
    consumed.set_field_by_name("item_id", Value::I32(104));
    consumed.set_field_by_name("quantity", Value::I32(1));
    enhance.set_field_by_name(
        "consumed_items",
        Value::List(vec![Value::Message(consumed.clone())]),
    );
    run(&store, "enhance", "/character/enhance", &enhance).unwrap();
    let mut rarity = empty_message(&proto, "blend.api.CharacterRarityEnhanceRequest").unwrap();
    rarity.set_field_by_name("character_id", Value::I32(25501));
    rarity.set_field_by_name("rarity_count", Value::I32(1));
    run(&store, "rarity", "/character/rarity_enhance", &rarity).unwrap();
    let before_failure = store.player_resources(account).unwrap();
    rarity.set_field_by_name("rarity_count", Value::I32(2));
    assert!(run(&store, "insufficient", "/character/rarity_enhance", &rarity).is_err());
    assert_eq!(store.player_resources(account).unwrap(), before_failure);
    let mut party = empty_message(&proto, "blend.api.PartyBulkUpdateRequest").unwrap();
    party.set_field_by_name("party_type", Value::I32(1));
    party.set_field_by_name("number", Value::I32(2));
    party.set_field_by_name("leader_position", Value::I32(1));
    let mut member = empty_message(&proto, "blend.model.PartyMemberWithEquipment").unwrap();
    member.set_field_by_name(
        "character_id",
        Value::Message(int32_value(&proto, 25501).unwrap()),
    );
    party.set_field_by_name("members", Value::List(vec![Value::Message(member)]));
    let result = reduce_party(
        &proto,
        &fresh,
        proto
            .decode("blend.model.Resources", &before_failure)
            .unwrap(),
        &party,
        false,
    )
    .unwrap();
    let fp = request_fingerprint("/party/bulk_update", &party.encode_to_vec());
    store
        .apply_gameplay_reducer(account, "party", "/party/bulk_update", &fp, 100, |_| {
            Ok(GameplayCommit {
                resources_blob: result.resources.encode_to_vec(),
                response_plaintext: result.response.encode_to_vec(),
                character_index: Vec::new(),
            })
        })
        .unwrap();
    drop(store);
    let store = Store::open(&path).unwrap();
    let before_retry = store.player_resources(account).unwrap();
    assert!(matches!(
        run(&store, "enhance", "/character/enhance", &enhance).unwrap(),
        GameplayMutationResult::Replay(_)
    ));
    assert_eq!(store.player_resources(account).unwrap(), before_retry);
    let saved = proto
        .decode("blend.model.Resources", &before_retry)
        .unwrap();
    assert!(find_party(&saved, 1, 2).is_some());
    assert!(message_list(&saved, "party_members")
        .iter()
        .any(|row| i32_field(row, "number") == Some(2)
            && optional_i32_field(row, "character_id") == Some(25501)));
    assert_eq!(character_piece_quantity(&saved, 25501), 20);
    assert_eq!(item_quantity(&saved, 104), Some(99));
    let mut reset = empty_message(&proto, "blend.api.CharacterEnhancementResetRequest").unwrap();
    reset.set_field_by_name("character_id", Value::I32(25501));
    run(&store, "reset", "/character/enhancement_reset", &reset).unwrap();
    let saved = proto
        .decode(
            "blend.model.Resources",
            &store.player_resources(account).unwrap(),
        )
        .unwrap();
    let character = resource_character(&saved, 25501).unwrap();
    assert_eq!(i32_field(&character, "exp"), Some(0));
    assert_eq!(i32_field(&character, "growboard_hp"), Some(0));
    assert_eq!(i32_field(&character, "growboard_level_limit"), Some(10));
    assert_eq!(
        i32_field(&status_message(&saved).unwrap(), "cole"),
        Some(100_000)
    );
    assert_eq!(item_quantity(&saved, 104), Some(100));
    assert_eq!(character_piece_quantity(&saved, 25501), 20);
}

#[test]
fn home_mission_claims_persist_without_duplicate_rewards() {
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
    let mut wallet = empty_message(&proto, "blend.model.Wallet").unwrap();
    wallet.set_field_by_name("free", Value::I32(100));
    resources.set_field_by_name("wallet", Value::Message(wallet));
    let now = unix_now();
    let path = std::env::temp_dir().join(format!("atelier-home-{}.sqlite3", Uuid::new_v4()));
    let store = Store::open(&path).unwrap();
    let account = store.create_account("home-test", "test-only").unwrap();
    store
        .ensure_player_state(account, &resources.encode_to_vec(), "test")
        .unwrap();
    let run =
        |store: &Store, id: &str, route: &str, request: &DynamicMessage, output: &str, now: i64| {
            let fingerprint = request_fingerprint(route, &request.encode_to_vec());
            let result = store
                .apply_home_reducer(account, id, route, &fingerprint, now, |stored, saved| {
                    let resources = proto.decode("blend.model.Resources", stored).unwrap();
                    let mut home: HomeState = serde_json::from_slice(saved).unwrap();
                    let commit = reduce_home(
                        &proto,
                        &rules,
                        load_character_rules().unwrap(),
                        resources,
                        &mut home,
                        route,
                        request,
                        output,
                        now,
                    )
                    .map_err(gameplay_storage_error)?;
                    Ok((commit, serde_json::to_vec(&home).unwrap()))
                })
                .unwrap();
            gameplay_response(
                store,
                &proto,
                account,
                id,
                route,
                &fingerprint,
                output,
                result,
            )
            .unwrap()
            .0
        };
    let empty = empty_message(&proto, "google.protobuf.Empty").unwrap();
    let first = run(
        &store,
        "login",
        "/login_bonus/receive",
        &empty,
        "blend.api.LoginBonusReceiveResponse",
        now,
    );
    assert!(!message_list(&first, "login_bonuses").is_empty());
    assert_eq!(
        tutorial_step(&member_status(&first, "changed_resources").unwrap()),
        TUTORIAL_STEP_HOME_READY
    );
    assert!(message_list(
        &member_status(&first, "changed_resources").unwrap(),
        "missions"
    )
    .iter()
    .any(|row| i32_field(row, "mission_id") == Some(10011)));
    assert_eq!(
        first,
        run(
            &store,
            "login",
            "/login_bonus/receive",
            &empty,
            "blend.api.LoginBonusReceiveResponse",
            now
        )
    );
    assert!(message_list(
        &run(
            &store,
            "login-again",
            "/login_bonus/receive",
            &empty,
            "blend.api.LoginBonusReceiveResponse",
            now
        ),
        "login_bonuses"
    )
    .is_empty());
    let listed = run(
        &store,
        "list",
        "/mail/list",
        &empty,
        "blend.api.MailListResponse",
        now,
    );
    let ids: Vec<_> = message_list(&member_status(&listed, "list").unwrap(), "unopened")
        .iter()
        .map(|m| Value::I32(i32_field(m, "entity_id").unwrap()))
        .collect();
    assert!(!ids.is_empty());
    let mut open = empty_message(&proto, "blend.api.MailOpenRequest").unwrap();
    open.set_field_by_name("entity_ids", Value::List(ids));
    let opened = run(
        &store,
        "open",
        "/mail/open",
        &open,
        "blend.api.MailOpenResponse",
        now,
    );
    assert!(!message_list(&opened, "rewards").is_empty());
    assert!(message_list(
        &run(
            &store,
            "open-again",
            "/mail/open",
            &open,
            "blend.api.MailOpenResponse",
            now
        ),
        "rewards"
    )
    .is_empty());
    let after_open = store.player_resources(account).unwrap();
    let mut purchase = empty_message(&proto, "blend.api.ManaPurchaseRequest").unwrap();
    purchase.set_field_by_name("count", Value::I32(1));
    let purchased = run(
        &store,
        "mana-purchase",
        "/mana/purchase",
        &purchase,
        "blend.api.ChangedResourcesResponse",
        now,
    );
    assert_eq!(
        purchased,
        run(
            &store,
            "mana-purchase",
            "/mana/purchase",
            &purchase,
            "blend.api.ChangedResourcesResponse",
            now
        )
    );
    let after_purchase = store.player_resources(account).unwrap();
    let previous = proto.decode("blend.model.Resources", &after_open).unwrap();
    let current = proto
        .decode("blend.model.Resources", &after_purchase)
        .unwrap();
    assert_eq!(
        message_i32_field(&current, "wallet", "free"),
        message_i32_field(&previous, "wallet", "free")
            .map(|free| free - rules.energy.mana_purchase_cost)
    );
    let mut invalid = open.clone();
    let mut invalid_ids = i32_list(&open, "entity_ids");
    invalid_ids.push(i32::MAX);
    invalid.set_field_by_name(
        "entity_ids",
        Value::List(invalid_ids.into_iter().map(Value::I32).collect()),
    );
    let invalid_bytes = invalid.encode_to_vec();
    assert!(store
        .apply_home_reducer(
            account,
            "invalid-mail",
            "/mail/open",
            &request_fingerprint("/mail/open", &invalid_bytes),
            now,
            |stored, saved| {
                let mut home = serde_json::from_slice(saved).unwrap();
                let commit = reduce_home(
                    &proto,
                    &rules,
                    load_character_rules().unwrap(),
                    proto.decode("blend.model.Resources", stored).unwrap(),
                    &mut home,
                    "/mail/open",
                    &invalid,
                    "blend.api.MailOpenResponse",
                    now,
                )
                .map_err(gameplay_storage_error)?;
                Ok((commit, serde_json::to_vec(&home).unwrap()))
            }
        )
        .is_err());
    assert_eq!(store.player_resources(account).unwrap(), after_purchase);
    let mut request = empty_message(&proto, "blend.api.MissionReceiveRequest").unwrap();
    request.set_field_by_name("mission_ids", Value::List(vec![Value::I32(10001)]));
    let claim = run(
        &store,
        "claim",
        "/mission/receive",
        &request,
        "blend.api.MissionReceiveResponse",
        now,
    );
    assert!(!message_list(&claim, "rewards").is_empty());
    assert!(message_list(
        &run(
            &store,
            "claim-again",
            "/mission/receive",
            &request,
            "blend.api.MissionReceiveResponse",
            now
        ),
        "rewards"
    )
    .is_empty());
    drop(store);
    let store = Store::open(&path).unwrap();
    assert_eq!(
        purchased,
        run(
            &store,
            "mana-purchase",
            "/mana/purchase",
            &purchase,
            "blend.api.ChangedResourcesResponse",
            now
        )
    );
    assert_eq!(
        claim,
        run(
            &store,
            "claim",
            "/mission/receive",
            &request,
            "blend.api.MissionReceiveResponse",
            now
        )
    );
    let next = reset_at(Some(1), now).unwrap();
    assert!(!message_list(
        &run(
            &store,
            "next-day",
            "/login_bonus/receive",
            &empty,
            "blend.api.LoginBonusReceiveResponse",
            next
        ),
        "login_bonuses"
    )
    .is_empty());
    assert!(!message_list(
        &run(
            &store,
            "next-claim",
            "/mission/receive",
            &request,
            "blend.api.MissionReceiveResponse",
            next
        ),
        "rewards"
    )
    .is_empty());
    let mut current = proto
        .decode(
            "blend.model.Resources",
            &store.player_resources(account).unwrap(),
        )
        .unwrap();
    let mut changed = empty_message(&proto, "blend.model.Resources").unwrap();
    advance_missions(
        &proto,
        &rules,
        &mut current,
        &mut changed,
        next,
        Some(("synthesis", 3)),
    )
    .unwrap();
    assert_eq!(
        mission_count(
            &current,
            rules.missions.iter().find(|r| r.id == 10011).unwrap()
        ),
        3
    );
    advance_missions(
        &proto,
        &rules,
        &mut current,
        &mut changed,
        next + 86400,
        None,
    )
    .unwrap();
    assert_eq!(
        mission_count(
            &current,
            rules.missions.iter().find(|r| r.id == 10011).unwrap()
        ),
        0
    );
    drop(store);
    std::fs::remove_file(path).unwrap();
}
