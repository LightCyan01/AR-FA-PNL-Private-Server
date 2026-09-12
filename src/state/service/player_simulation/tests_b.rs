#[test]
fn natural_account_progression() {
    let mut player = Player::new();
    player.tutorial();
    let tutorial_characters = message_list(&player.resources(), "characters").len();
    player.talk(101001001);
    assert_eq!(quest_clear_count(&player.resources(), 101001001), 2);
    assert_eq!(
        message_list(&player.resources(), "characters").len(),
        tutorial_characters
    );
    let birthdate = player.home(
        "/user/update_birthdate",
        &[("year", Value::I32(1990)), ("month", Value::I32(1))],
    );
    let changed_status = member_status(
        &member_status(&birthdate, "changed_resources").unwrap(),
        "status",
    )
    .unwrap();
    assert_eq!(
        message_i32_field(&changed_status, "birth_year", "value"),
        Some(1990)
    );
    assert_eq!(
        message_i32_field(&changed_status, "birth_month", "value"),
        Some(1)
    );
    let second_birthdate = player.message(
        "blend.api.UserUpdateBirthdateRequest",
        &[("year", Value::I32(1991)), ("month", Value::I32(2))],
    );
    let before_second_birthdate = player.resources().encode_to_vec();
    assert!(player
        .dispatch(
            "/user/update_birthdate",
            &Uuid::new_v4().to_string(),
            &second_birthdate
        )
        .is_err());
    assert_eq!(player.resources().encode_to_vec(), before_second_birthdate);
    player.prepare();
    let catalog = load_gameplay_rules().unwrap();
    for quest in 101002001..=101002022 {
        if quest == 101002010 {
            player.battle(204051001);
        }
        if quest == 101002013 {
            player.craft(5, 1);
        }
        if quest == 101002014 {
            player.battle(204055001);
        }
        if quest == 101002020 {
            player.craft(5, 2);
        }
        let rule = catalog.quests.iter().find(|q| q.id == quest).unwrap();
        if rule.quest_type == 1 {
            player.battle(quest);
        } else {
            player.talk(quest);
        }
    }
    player.talk(101002001);
    assert_eq!(quest_clear_count(&player.resources(), 101002001), 2);
    player.prepare();
    for _ in 0..10 {
        player.home("/shop/wheel", &[("shop_wheel_id", Value::I32(4))]);
    }
    player.home("/shop/wheel", &[("shop_wheel_id", Value::I32(5))]);
    player.call(
        "/gacha/execute",
        player.message(
            "blend.api.GachaExecuteRequest",
            &[
                ("gacha_id", Value::I32(3080)),
                ("gacha_button_id", Value::I32(10002)),
            ],
        ),
    );
    for quest in [204055006, 204055011, 204055016, 204055021] {
        player.battle(quest);
    }
    player.prepare();
    player.explore(101002023);
    player.wait_hours(24);
    let stamina = i32_field(
        &status_message(&player.resources()).unwrap(),
        "stamina_when_updated",
    )
    .unwrap_or(0);
    player.home("/dish/order", &[("dish_id", Value::I32(1))]);
    assert_eq!(
        i32_field(
            &status_message(&player.resources()).unwrap(),
            "stamina_when_updated"
        ),
        Some(stamina + 200)
    );
    let expedition = player.message(
        "blend.model.NewExpedition",
        &[
            ("number", Value::I32(1)),
            ("expedition_id", Value::I32(50)),
            ("character_ids", Value::List(vec![Value::I32(43101)])),
        ],
    );
    player.home(
        "/expedition/start",
        &[(
            "new_expeditions",
            Value::List(vec![Value::Message(expedition)]),
        )],
    );
    player.reopen();
    player.wait_hours(6);
    let cole = i32_field(&status_message(&player.resources()).unwrap(), "cole").unwrap();
    let expedition_reward = player.home("/expedition/reward_receive", &[]);
    assert!(!message_list(&expedition_reward, "rewards").is_empty());
    assert!(i32_field(&status_message(&player.resources()).unwrap(), "cole").unwrap() > cole);
    player.home(
        "/atelier/research",
        &[("group_id", Value::I32(1)), ("count", Value::I32(2))],
    );
    assert_eq!(
        i32_field(
            &message_list(&player.resources(), "research_groups")[0],
            "level"
        ),
        Some(2)
    );
    player.battle(204052001);
    let memoria = i32_field(
        &message_list(&player.resources(), "memorias")[0],
        "entity_id",
    )
    .unwrap();
    let material = player.message(
        "blend.model.ConsumedItem",
        &[("item_id", Value::I32(98)), ("quantity", Value::I32(1))],
    );
    player.home(
        "/memoria/enhance",
        &[
            ("memoria_entity_id", Value::I32(memoria)),
            (
                "consumed_items",
                Value::List(vec![Value::Message(material)]),
            ),
        ],
    );
    player.home(
        "/communication/story_clear",
        &[
            ("character_id", Value::I32(43101)),
            ("story_number", Value::I32(1)),
        ],
    );
    player.home(
        "/communication/story_release",
        &[
            ("character_id", Value::I32(43101)),
            ("story_number", Value::I32(2)),
        ],
    );
    player.home(
        "/communication/story_clear",
        &[
            ("character_id", Value::I32(43101)),
            ("story_number", Value::I32(2)),
        ],
    );
    player.battle(204055001);
    player.home(
        "/character_story/clear",
        &[("character_story_id", Value::I32(6801001))],
    );
    player.home(
        "/character_story/clear",
        &[
            ("character_story_id", Value::I32(6801002)),
            (
                "condition",
                Value::Message(int32_value(&player.state.proto, 3).unwrap()),
            ),
        ],
    );
    player.home(
        "/character_story/clear",
        &[("character_story_id", Value::I32(6801003))],
    );
    let wallet = member_status(&player.resources(), "wallet")
        .unwrap()
        .encode_to_vec();
    player.home(
        "/character_story/clear",
        &[("character_story_id", Value::I32(6801001))],
    );
    assert_eq!(
        wallet,
        member_status(&player.resources(), "wallet")
            .unwrap()
            .encode_to_vec()
    );
    player.home("/shop/wheel", &[("shop_wheel_id", Value::I32(4))]);
    // The master first-clear reward at 101002030 supplies recipe 9's item 1.
    // Reach it normally instead of granting missing ingredients or counters.
    for quest in 101002024..=101002030 {
        let rule = catalog.quests.iter().find(|q| q.id == quest).unwrap();
        if rule.quest_type == 1 {
            player.battle(quest);
        } else {
            player.talk(quest);
        }
    }
    player.craft(9, 1);
    let prior = message_list(&player.resources(), "quest_states");
    player.home("/quest/talk_event/main_story_episode_skip", &[]);
    assert_eq!(
        message_i32_field(&player.resources(), "status", "last_main_story_quest_id"),
        Some(101002059)
    );
    for q in prior {
        let id = i32_field(&q, "quest_id").unwrap();
        assert_eq!(
            quest_clear_count(&player.resources(), id),
            i32_field(&q, "clear_count").unwrap()
        );
    }
    let before_duplicate = player
        .state
        .store
        .player_resources(player.session.account_id)
        .unwrap();
    assert!(player
        .dispatch(
            "/quest/talk_event/main_story_episode_skip",
            &Uuid::new_v4().to_string(),
            &player.message("google.protobuf.Empty", &[])
        )
        .is_err());
    assert_eq!(
        before_duplicate,
        player
            .state
            .store
            .player_resources(player.session.account_id)
            .unwrap()
    );
    player.reopen();
    std::fs::write(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../work/natural-player-actions.txt"),
        player.actions.join("\n"),
    )
    .unwrap();
    println!(
        "NATURAL_TUTORIAL_OK actions={} owned_characters={} recipes={}",
        player.actions.len(),
        message_list(&player.resources(), "characters").len(),
        message_list(&player.resources(), "recipes").len()
    );
    let path = player.path.clone();
    drop(player);
    std::fs::remove_file(path).unwrap();
    set_simulation_now(None);
}

#[test]
fn historical_account_activities() {
    // The account begins during the actual event window; no progress or inventory is seeded.
    set_simulation_now(Some(1_722_394_801));
    let mut player = Player::new();
    player.tutorial();
    let mut inputs = Vec::new();
    for (kind, field) in [(6, "equipment_tools"), (14, "battle_tools")] {
        for tool in message_list(&player.resources(), field)
            .into_iter()
            .take(3 - inputs.len())
        {
            inputs.push(Value::Message(player.message(
                "blend.model.ToolEntity",
                &[
                    ("type", Value::I32(kind)),
                    (
                        "entity_id",
                        Value::I32(i32_field(&tool, "entity_id").unwrap()),
                    ),
                ],
            )));
        }
    }
    let response = player.home(
        "/item_challenge/execute",
        &[
            ("item_challenge_id", Value::I32(10)),
            ("challenge_tools", Value::List(inputs)),
        ],
    );
    let score = i32_field(&response, "score").unwrap_or(0);
    assert!(score > 0);
    player.reopen();
    assert_eq!(
        i32_field(
            &message_list(&player.resources(), "item_challenges")[0],
            "high_score"
        ),
        Some(score)
    );
    // Advance the same account into the current housing event and walk its gate.
    set_simulation_now(Some(1_743_390_001));
    player.authenticate();
    player.talk(9101001);
    player.talk(9101002);
    player.battle(9101003);
    player.talk(9101004);
    player.talk(9101005);
    player.home(
        "/house_building/start",
        &[("house_building_id", Value::I32(10002))],
    );
    assert_eq!(
        i32_field(
            &message_list(&player.resources(), "house_building_states")[0],
            "house_building_id"
        ),
        Some(10002)
    );
    println!(
        "NATURAL_HISTORICAL_OK actions={} challenge_score={score}",
        player.actions.len()
    );
    let path = player.path.clone();
    drop(player);
    std::fs::remove_file(path).unwrap();
    set_simulation_now(None);
}
use super::*;
