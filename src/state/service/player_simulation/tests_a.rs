#[test]
fn language_preference_survives_encrypted_relogin() {
    let mut player = Player::new();
    assert_eq!(i32_field(&player.authenticate(), "language"), Some(1));
    player.home("/user/update_language", &[("language", Value::I32(2))]);
    player.reopen();
    assert_eq!(i32_field(&player.authenticate(), "language"), Some(2));
    let invalid = player.message(
        "blend.api.UserUpdateLanguageRequest",
        &[("language", Value::I32(5))],
    );
    assert!(player
        .dispatch(
            "/user/update_language",
            &Uuid::new_v4().to_string(),
            &invalid
        )
        .is_err());
    assert_eq!(i32_field(&player.authenticate(), "language"), Some(2));
}

#[test]
fn story_skip_clear_rewards_replay_relog_and_battle_guard() {
    let mut player = Player::new();
    let empty = player.message("google.protobuf.Empty", &[]);
    for route in [
        "/quest/talk_event/main_story_episode_skip",
        "/quest/talk_event/main_story_season_skip",
    ] {
        let before = player
            .state
            .store
            .player_resources(player.session.account_id)
            .unwrap();
        assert!(player
            .dispatch(route, &Uuid::new_v4().to_string(), &empty)
            .is_err());
        assert_eq!(
            before,
            player
                .state
                .store
                .player_resources(player.session.account_id)
                .unwrap()
        );
    }
    player.home("/quest/talk_event/prologue_skip", &[]);
    let resources = player.resources();
    assert_eq!(message_list(&resources, "quest_states").len(), 17);
    assert!((101001001..=101001017).all(|id| quest_clear_count(&resources, id) == 1));
    assert_eq!(tutorial_step(&resources), TUTORIAL_STEP_MEMORIA_EQUIPPED);
    assert_eq!(
        i32_field(&resource_character(&resources, 43101).unwrap(), "exp"),
        Some(41)
    );
    assert_eq!(message_list(&resources, "memorias").len(), 1);
    let before = player
        .state
        .store
        .player_resources(player.session.account_id)
        .unwrap();
    assert!(player
        .dispatch(
            "/quest/talk_event/prologue_skip",
            &Uuid::new_v4().to_string(),
            &empty
        )
        .is_err());
    assert_eq!(
        before,
        player
            .state
            .store
            .player_resources(player.session.account_id)
            .unwrap()
    );
    player.reopen();
    let list = player
        .dispatch(
            "/gacha/list",
            &Uuid::new_v4().to_string(),
            &player.message(
                "blend.api.GachaListRequest",
                &[("gacha_category", Value::I32(1))],
            ),
        )
        .unwrap();
    assert!(message_list(&list, "gachas")
        .iter()
        .any(|g| i32_field(g, "gacha_id") == Some(2)));
    player.call(
        "/gacha/execute",
        player.message(
            "blend.api.GachaExecuteRequest",
            &[
                ("gacha_id", Value::I32(2)),
                ("gacha_button_id", Value::I32(14)),
            ],
        ),
    );
    player.home("/login_bonus/receive", &[]);
    player.talk(101002001);
    // A later synthesis prerequisite cannot be waived by batching earlier clears.
    let before = player
        .state
        .store
        .player_resources(player.session.account_id)
        .unwrap();
    for route in [
        "/quest/talk_event/main_story_episode_skip",
        "/quest/talk_event/main_story_season_skip",
    ] {
        assert!(player
            .dispatch(route, &Uuid::new_v4().to_string(), &empty)
            .is_err());
        assert_eq!(
            before,
            player
                .state
                .store
                .player_resources(player.session.account_id)
                .unwrap()
        );
    }
    let mut active = Player::new();
    active.talk(101001001);
    active.call(
        "/quest/battle/start",
        active.message(
            "blend.api.QuestBattleStartRequest",
            &[
                ("quest_id", Value::I32(101001002)),
                ("party_number", Value::I32(1)),
            ],
        ),
    );
    let before = active
        .state
        .store
        .active_battle(active.session.account_id)
        .unwrap()
        .unwrap()
        .state_blob;
    for route in [
        "/quest/talk_event/prologue_skip",
        "/quest/talk_event/main_story_episode_skip",
        "/quest/talk_event/main_story_season_skip",
    ] {
        assert!(active
            .dispatch(route, &Uuid::new_v4().to_string(), &empty)
            .is_err());
        assert_eq!(
            before,
            active
                .state
                .store
                .active_battle(active.session.account_id)
                .unwrap()
                .unwrap()
                .state_blob
        );
    }
    println!("STORY_SKIP_PROLOGUE_OK clear=17 replay=relog=atomic_failure=battle_guard=pass");
}

#[test]
fn gacha_list_survives_replaying_the_last_prologue_talk() {
    let mut player = Player::new();
    player.home("/quest/talk_event/prologue_skip", &[]);
    player.talk(101001017);
    assert_eq!(quest_clear_count(&player.resources(), 101001017), 2);
    let request = player.message(
        "blend.api.GachaListRequest",
        &[("gacha_category", Value::I32(1))],
    );
    let list = player
        .dispatch("/gacha/list", &Uuid::new_v4().to_string(), &request)
        .unwrap();
    assert!(message_list(&list, "gachas")
        .iter()
        .any(|g| i32_field(g, "gacha_id") == Some(2)));
    player.reopen();
    assert!(player
        .dispatch("/gacha/list", &Uuid::new_v4().to_string(), &request)
        .is_ok());
}

#[test]
fn fresh_login_does_not_invent_equipped_memoria() {
    let mut player = Player::new();
    for _ in 0..2 {
        assert!(message_list(&player.resources(), "characters")
            .iter()
            .all(|c| !c.has_field_by_name("memoria_entity_id")));
        assert!(player
            .state
            .store
            .characters(player.session.account_id)
            .unwrap()
            .iter()
            .all(|(_, memoria)| memoria.is_none()));
        player.reopen();
    }
}

#[test]
fn tutorial_clear_response_contains_the_committed_quest() {
    let mut player = Player::new();
    let response = player.call(
        "/quest/talk_event/finish",
        player.message(
            "blend.api.QuestTalkEventFinishRequest",
            &[("quest_id", Value::I32(101001001))],
        ),
    );
    let changed = member_status(&response, "changed_resources").unwrap();
    assert_eq!(
        quest_clear_count(&changed, 101001001),
        1,
        "a client cannot merge a quest clear omitted from changed_resources"
    );
    player.reopen();
    assert_eq!(quest_clear_count(&player.resources(), 101001001), 1);
}

#[test]
fn resumed_battle_uses_persisted_action_cursor() {
    let mut player = Player::new();
    let start = player.call(
        "/gacha/battle_start",
        player.message(
            "blend.api.GachaBattleStartRequest",
            &[
                ("gacha_id", Value::I32(2045)),
                ("gacha_battle_id", Value::I32(22)),
            ],
        ),
    );
    let gauge = player.call(
        "/battle/attack",
        player.message(
            "blend.api.BattleAttackRequest",
            &[("mode", Value::EnumNumber(6))],
        ),
    );
    let gauge_setup = message_list(&member_status(&gauge, "history").unwrap(), "action_setups")
        .pop()
        .unwrap();
    let tool_numbers = message_list(&gauge_setup, "battle_tool_selections")
        .into_iter()
        .take(2)
        .map(|tool| Value::I32(i32_field(&tool, "number").unwrap()))
        .collect();
    let command = player.message(
        "blend.model.BattleBattleToolCommand",
        &[("battle_tool_numbers", Value::List(tool_numbers))],
    );
    let attack = player.call(
        "/battle/attack",
        player.message(
            "blend.api.BattleAttackRequest",
            &[
                ("mode", Value::EnumNumber(1)),
                ("battle_tool_command", Value::Message(command)),
            ],
        ),
    );
    let attack_history = member_status(&attack, "history").unwrap();
    let next_setup = message_list(&attack_history, "action_setups")
        .pop()
        .expect("tool turn must advertise the next player decision");
    let expected_number = i32_field(&next_setup, "number").unwrap();
    let expected_turn = i32_field(&member_status(&next_setup, "state").unwrap(), "total_turn")
        .unwrap();
    assert!(expected_number > expected_turn);

    player.reopen();
    let resume = player.read_api("/battle/resume", "blend.api.BattleResumeResponse");
    assert_eq!(
        member_status(&start, "context").unwrap(),
        member_status(&resume, "context").unwrap()
    );
    let history = member_status(&resume, "history").unwrap();
    let setup = message_list(&history, "action_setups")
        .pop()
        .expect("resume must advertise the persisted player decision");
    assert_eq!(i32_field(&setup, "number"), Some(expected_number));
    assert_eq!(
        i32_field(&member_status(&history, "previous_state").unwrap(), "total_turn"),
        Some(expected_turn)
    );
    assert!(!message_list(&setup, "skill_selections").is_empty());
    assert!(message_list(&history, "wave_starts")
        .iter()
        .any(|wave| i32_field(wave, "action_number") == i32_field(&setup, "number")));
}

#[test]
fn specialized_battle_starts_cover_current_catalog() {
    let mut player = Player::new();
    let catalog = load_gameplay_rules().unwrap();
    let rules = &catalog;
    let rental = rules
        .quests
        .iter()
        .filter(|q| q.rental_fixed_party_id.is_some())
        .collect::<Vec<_>>();
    let solo = rules
        .quests
        .iter()
        .filter(|q| q.solo_raid_id.is_some())
        .collect::<Vec<_>>();
    let total = rules
        .quests
        .iter()
        .filter(|q| q.episode_type == 6)
        .collect::<Vec<_>>();
    assert_eq!((rental.len(), solo.len(), total.len()), (35, 11, 108));
    for quest in &rental {
        rule_battle(rules, quest.battle_id.unwrap()).unwrap();
        resolve_fixed_party(
            &player.state.proto,
            rules,
            quest.rental_fixed_party_id.unwrap(),
        )
        .unwrap();
    }
    for quest in &solo {
        assert!(!quest.battle_ids.is_empty());
        for id in &quest.battle_ids {
            rule_battle(rules, *id).unwrap();
        }
    }
    for quest in &total {
        rule_battle(rules, quest.battle_id.unwrap()).unwrap();
        assert!(quest.total_battle_panel_id.is_some() && quest.total_battle_id.is_some());
    }

    let mut member = player.message("blend.model.PartyMemberWithEquipment", &[]);
    member.set_field_by_name(
        "character_id",
        Value::Message(int32_value(&player.state.proto, 43101).unwrap()),
    );
    for (route, name, quest_id) in [
        (
            "/quest/battle/rental_party_start",
            "blend.api.QuestBattleRentalPartyStartRequest",
            1010306,
        ),
        (
            "/quest/battle/solo_raid_battle_start",
            "blend.api.QuestBattleSoloRaidBattleStartRequest",
            13404001,
        ),
        (
            "/quest/battle/total_battle_start",
            "blend.api.QuestBattleTotalBattleStartRequest",
            101000101,
        ),
    ] {
        let fields = if route.ends_with("rental_party_start") {
            vec![("quest_id", Value::I32(quest_id))]
        } else {
            vec![
                ("quest_id", Value::I32(quest_id)),
                ("members", Value::List(vec![Value::Message(member.clone())])),
                ("leader_position", Value::I32(1)),
            ]
        };
        let response = player.call(route, player.message(name, &fields));
        assert_eq!(
            i32_field(&member_status(&response, "context").unwrap(), "quest_id"),
            Some(quest_id)
        );
        player.call(
            "/battle/retire",
            player.message("google.protobuf.Empty", &[]),
        );
    }
    let resources = player.resources();
    assert!(message_list(&resources, "solo_raid_states")
        .iter()
        .any(|s| i32_field(s, "quest_id") == Some(13404001)));
    assert!(message_list(&resources, "total_battle_panel_states")
        .iter()
        .any(|s| i32_field(s, "total_battle_panel_id") == Some(14)));
    println!("SPECIALIZED_BATTLE_STARTS_OK rental=35 solo_raid=11 total=108");
}

#[test]
fn specialized_battle_lifecycle_is_atomic() {
    let mut player = Player::new();
    let quest_id = 1010306;
    let request = player.message(
        "blend.api.QuestBattleRentalPartyStartRequest",
        &[("quest_id", Value::I32(quest_id))],
    );
    let start = player.call("/quest/battle/rental_party_start", request);
    player.reopen();
    let login = player.read_api("/user/log_in", "blend.api.UserLogInResponse");
    assert_eq!(i32_or_enum_field(&login, "battle_resume_status"), Some(1));
    let resume = player.read_api("/battle/resume", "blend.api.BattleResumeResponse");
    assert_eq!(
        member_status(&resume, "context").unwrap(),
        member_status(&start, "context").unwrap()
    );
    assert!(player
        .state
        .store
        .active_battle(player.session.account_id)
        .unwrap()
        .is_some());
    player.fight(quest_id);
    let records = message_list(&player.resources(), "character_skill_records");
    assert!(
        !records.is_empty(),
        "completed battle must remember viewed skills"
    );
    assert!(records.iter().all(|r| !i32_list(r, "skill_ids").is_empty()));
    player.reopen();
    let relog = player.read_api("/user/log_in", "blend.api.UserLogInResponse");
    assert_eq!(
        message_list(
            &member_status(&relog, "resources").unwrap(),
            "character_skill_records"
        ),
        records,
        "client must receive viewed skills again on relog"
    );
    let login = player.read_api("/user/log_in", "blend.api.UserLogInResponse");
    assert_eq!(i32_or_enum_field(&login, "battle_resume_status"), Some(0));
    assert_eq!(quest_clear_count(&player.resources(), quest_id), 1);
    let before = player.resources().encode_to_vec();
    let invalid = player.message(
        "blend.api.QuestBattleRentalPartyStartRequest",
        &[("quest_id", Value::I32(101000101))],
    );
    assert!(player
        .dispatch(
            "/quest/battle/rental_party_start",
            &Uuid::new_v4().to_string(),
            &invalid
        )
        .is_err());
    assert_eq!(player.resources().encode_to_vec(), before);
    println!("SPECIALIZED_BATTLE_LIFECYCLE_OK quest={quest_id}");
}

#[test]
fn gacha_trial_fixed_party_rewards_and_relog() {
    let mut player = Player::new();
    player.tutorial();
    let list_request = player.message(
        "blend.api.GachaListRequest",
        &[("gacha_category", Value::I32(1))],
    );
    let list = player
        .dispatch("/gacha/list", &Uuid::new_v4().to_string(), &list_request)
        .unwrap();
    player.merge_response(&list);
    let visible = message_list(&list, "gachas")
        .iter()
        .filter_map(|g| i32_field(g, "gacha_id"))
        .collect::<Vec<_>>();
    let catalog = load_gameplay_rules().unwrap();
    let offered_trial = catalog
        .gachas
        .iter()
        .find(|g| visible.contains(&g.id) && !g.gacha_battle_ids.is_empty());
    // Finish safe endpoint checks even when the operator-selected catalog hides
    // every trial. This fallback is static master selection, NOT UI acceptance.
    let gacha = offered_trial
        .or_else(|| {
            catalog.gachas.iter().find(|g| {
                !g.gacha_battle_ids.is_empty()
                    && g.start_at.is_none_or(|t| t <= unix_now())
                    && g.end_at.is_none_or(|t| unix_now() < t)
            })
        })
        .expect("master trial candidate");
    let id = gacha.gacha_battle_ids[0];
    let request = player.message(
        "blend.api.GachaBattleStartRequest",
        &[
            ("gacha_id", Value::I32(gacha.id)),
            ("gacha_battle_id", Value::I32(id)),
        ],
    );
    let quests = message_list(&player.resources(), "quest_states");
    let mut first_items = Vec::new();
    for run in 0..2 {
        let start = player.call("/gacha/battle_start", request.clone());
        player.reopen();
        assert_eq!(
            member_status(&start, "context").unwrap(),
            member_status(
                &player.read_api("/battle/resume", "blend.api.BattleResumeResponse"),
                "context"
            )
            .unwrap()
        );
        player.fight(id);
        let current = player.resources();
        assert!(message_list(&current, "gacha_battle_states")
            .iter()
            .any(|s| i32_field(s, "gacha_battle_id") == Some(id)));
        assert_eq!(message_list(&current, "quest_states"), quests);
        if run == 0 {
            first_items = message_list(&current, "items");
        } else {
            assert_eq!(message_list(&current, "items"), first_items);
        }
        player.reopen();
    }
    println!("GACHA_TRIAL_LIFECYCLE_OK");
    std::fs::write(Path::new(env!("CARGO_MANIFEST_DIR")).join("../work/progression-acceptance-20260908/gacha-client-gate.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "visible_banners": visible, "visible_trial": offered_trial.map(|g| g.id),
            "master_selected_trial": gacha.id, "trial_id": id,
            "endpoint_lifecycle": "SIMULATED", "client_discovery": if offered_trial.is_some() { "SIMULATED" } else { "BLOCKED" }
        })).unwrap()).unwrap();
    assert!(offered_trial.is_some(), "client-visible catalog must offer an executable trial; endpoint lifecycle alone is not client acceptance");
}

#[test]
fn client_api_remembers_skill_animations_across_battles_and_relog() {
    let player = Player::new();
    player.runtime.block_on(async {
        use crate::{
            api::ApiService,
            assets::AssetService,
            transport::{decrypt_frame, encrypt_frame, full},
        };
        use http_body_util::BodyExt;
        use hyper::Request;
        use std::{collections::BTreeSet, sync::Arc};

        async fn post(
            api: &ApiService,
            proto: &ProtoRegistry,
            config: &LoadedConfig,
            token: &(String, i64),
            route: &str,
            request: Option<DynamicMessage>,
            output: &str,
        ) -> DynamicMessage {
            let id = Uuid::new_v4().to_string();
            let body = request
                .map(|m| encrypt_frame(0x91, &m.encode_to_vec()).unwrap())
                .unwrap_or_default();
            let mut responses = Vec::new();
            for _ in 0..2 {
                let request = Request::post(route)
                    .header("x-session-token", &token.0)
                    .header("x-user-id", token.1)
                    .header("x-request-id", &id)
                    .header("x-asset-version", &config.config.versions.asset)
                    .header("x-master-data-version", &config.config.versions.master_data)
                    .body(full(body.clone()))
                    .unwrap();
                let response = api.handle(request).await;
                let status = response.status();
                let bytes = response.into_body().collect().await.unwrap().to_bytes();
                assert_eq!(status, 200, "{route}: {}", String::from_utf8_lossy(&bytes));
                responses.push(decrypt_frame(&bytes).unwrap().1);
            }
            // Login has time-dependent fields; mutations must be byte-identical on retry.
            if route != "/user/log_in" {
                assert_eq!(responses[0], responses[1], "{route} retry");
            }
            proto.decode(output, &responses[0]).unwrap()
        }

        let config = player.state.config.clone();
        let proto = player.state.proto.clone();
        let assets = AssetService::load(Arc::new(config.clone())).await.unwrap();
        let mut known = BTreeMap::<i32, BTreeSet<i32>>::new();
        for battle_number in 0..2 {
            // A new server state and session each time, retaining only the disposable DB.
            let state = State::new(
                Store::open(&player.path).unwrap(),
                proto.clone(),
                config.clone(),
            )
            .unwrap();
            let grant = state
                .login_grant("natural_player", "test-password")
                .unwrap();
            let mut auth = empty_message(&proto, "blend.api.AuthSignInRequest").unwrap();
            auth.set_field_by_name("device_secret", Value::String(grant));
            auth.set_field_by_name(
                "device_unique_id",
                Value::String("simulation-device".into()),
            );
            auth.set_field_by_name("device_model", Value::String("simulation".into()));
            let signed_in = state.sign_in(&auth).unwrap();
            let token = (signed_in.session_token, signed_in.user_id);
            state.session(&token.0, Some(token.1)).unwrap();
            let api = ApiService::new(Arc::new(state), assets.clone());
            let login = post(
                &api,
                &proto,
                &config,
                &token,
                "/user/log_in",
                None,
                "blend.api.UserLogInResponse",
            )
            .await;
            let resources = member_status(&login, "resources").unwrap();
            let received: BTreeMap<_, BTreeSet<_>> =
                message_list(&resources, "character_skill_records")
                    .iter()
                    .map(|r| {
                        (
                            i32_field(r, "character_id").unwrap(),
                            i32_list(r, "skill_ids").into_iter().collect(),
                        )
                    })
                    .collect();
            assert_eq!(received, known, "relog loses client skill records");
            let start = player.message(
                "blend.api.QuestBattleRentalPartyStartRequest",
                &[("quest_id", Value::I32(1010306))],
            );
            let response = post(
                &api,
                &proto,
                &config,
                &token,
                "/quest/battle/rental_party_start",
                Some(start),
                "blend.api.BattleStartResponse",
            )
            .await;
            let mut history = member_status(&response, "history").unwrap();
            let mut used = BTreeMap::<i32, BTreeSet<i32>>::new();
            for turn in 0..500 {
                if i32_or_enum_field(&history, "status") == Some(BATTLE_STATUS_WON) {
                    break;
                }
                assert!(turn < 499, "client simulation did not finish");
                // Choose only from the server's advertised UI selections, never internal battle state.
                let setup = message_list(&history, "action_setups")
                    .pop()
                    .expect("client action setup");
                let selection = message_list(&setup, "skill_selections")
                    .into_iter()
                    .find(|s| i32_field(s, "skill_type") == Some(2))
                    .unwrap();
                let target = message_list(&selection, "targets")
                    .into_iter()
                    .max_by_key(|t| message_i64_field(t, "hp_damage", "value").unwrap_or(0))
                    .unwrap();
                let command = player.message(
                    "blend.model.BattleSkillCommand",
                    &[
                        ("skill_type", Value::I32(2)),
                        (
                            "main_target_id",
                            Value::I32(i32_field(&target, "target_id").unwrap()),
                        ),
                    ],
                );
                let request = player.message(
                    "blend.api.BattleAttackRequest",
                    &[("skill_command", Value::Message(command))],
                );
                let response = post(
                    &api,
                    &proto,
                    &config,
                    &token,
                    "/battle/attack",
                    Some(request),
                    "blend.api.BattleAttackResponse",
                )
                .await;
                history = member_status(&response, "history").unwrap();
                let mut before = member_status(&history, "previous_state").unwrap();
                for action in message_list(&history, "actions") {
                    if optional_i32_field(&action, "skill_type").is_some()
                        && !bool_field(&action, "is_skipped")
                    {
                        if let Some(character) = message_list(&before, "members")
                            .iter()
                            .find(|m| i32_field(m, "member_id") == i32_field(&action, "actor_id"))
                            .and_then(|m| message_i32_field(m, "ally", "character_id"))
                            .filter(|id| *id > 0)
                        {
                            used.entry(character)
                                .or_default()
                                .insert(i32_field(&action, "skill_id").unwrap());
                        }
                    }
                    before = member_status(&action, "state").unwrap();
                }
            }
            let finish = post(
                &api,
                &proto,
                &config,
                &token,
                "/battle/finish",
                None,
                "blend.api.BattleFinishResponse",
            )
            .await;
            for record in message_list(
                &member_status(&finish, "changed_resources").unwrap(),
                "character_skill_records",
            ) {
                // Native client replaces the per-character row, so each delta must contain the full known list.
                known.insert(
                    i32_field(&record, "character_id").unwrap(),
                    i32_list(&record, "skill_ids").into_iter().collect(),
                );
            }
            assert!(!used.is_empty());
            for (character, skills) in used {
                assert!(skills.is_subset(known.get(&character).expect("viewed character")));
            }
            if battle_number == 0 {
                assert!(!known.is_empty(), "first finish never updated client");
            }
        }
        println!("CLIENT_API_SKILL_ANIMATION_OK");
    });
}
use super::*;
