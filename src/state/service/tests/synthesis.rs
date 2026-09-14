#[test]
fn tutorial_path_reaches_home_and_persists() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let fresh_rules = load_fresh_rules().unwrap();
    let tutorial_rules = load_tutorial_rules().unwrap();
    let synthesis_rules = load_synthesis_rules().unwrap();
    let opening = reduce_talk_event(
        &proto,
        &fresh_rules,
        &tutorial_rules,
        starter_resources(&proto, &fresh_rules).unwrap(),
        101001001,
        1,
    )
    .unwrap();
    let mut resources = play_tutorial_battle(&proto, &tutorial_rules, opening.resources, 101001002);
    resources = finish_tutorial_talks(
        &proto,
        &fresh_rules,
        &tutorial_rules,
        resources,
        [101001003],
    );
    assert!(matches!(
        reduce_talk_event(
            &proto,
            &fresh_rules,
            &tutorial_rules,
            resources.clone(),
            101001004,
            1,
        ),
        Err(StateError::InvalidRequest)
    ));
    resources =
        synthesize_tutorial_tool(&proto, &synthesis_rules, resources, 6, &[32201, 43101], 67);
    assert_eq!(tutorial_step(&resources), TUTORIAL_STEP_FIRST_SYNTHESIS);
    let first_traits = message_list(
        &message_list(&resources, "battle_tools")
            .into_iter()
            .filter(|tool| i32_field(tool, "tool_id") == Some(3))
            .max_by_key(|tool| i32_field(tool, "entity_id"))
            .unwrap(),
        "traits",
    );
    assert_eq!(first_traits.len(), 3);
    assert!(first_traits.iter().all(|trait_param| {
        [5, 6, 10, 17, 25, 45].contains(&i32_field(trait_param, "id").unwrap_or_default())
            && (1..=5).contains(&i32_field(trait_param, "rank").unwrap_or_default())
    }));
    let first_tool_entity_id = message_list(&resources, "battle_tools")
        .into_iter()
        .filter(|tool| i32_field(tool, "tool_id") == Some(3))
        .max_by_key(|tool| i32_field(tool, "entity_id"))
        .and_then(|tool| i32_field(&tool, "entity_id"))
        .unwrap();
    assert_eq!(item_quantity(&resources, 67), Some(1));
    resources = finish_tutorial_talks(
        &proto,
        &fresh_rules,
        &tutorial_rules,
        resources,
        101001004..=101001007,
    );

    resources = play_tutorial_battle(&proto, &tutorial_rules, resources, 101001008);
    resources = finish_tutorial_talks(
        &proto,
        &fresh_rules,
        &tutorial_rules,
        resources,
        [101001009, 101001010],
    );
    assert!(matches!(
        reduce_talk_event(
            &proto,
            &fresh_rules,
            &tutorial_rules,
            resources.clone(),
            101001011,
            1,
        ),
        Err(StateError::InvalidRequest)
    ));
    resources =
        synthesize_tutorial_tool(&proto, &synthesis_rules, resources, 2, &[32201, 43101], 67);
    assert_eq!(tutorial_step(&resources), TUTORIAL_STEP_SECOND_SYNTHESIS);
    assert_eq!(
        message_list(
            &message_list(&resources, "battle_tools")
                .into_iter()
                .find(|tool| i32_field(tool, "tool_id") == Some(1))
                .unwrap(),
            "traits",
        )
        .len(),
        3
    );
    let second_tool_entity_id = message_list(&resources, "battle_tools")
        .into_iter()
        .find(|tool| i32_field(tool, "tool_id") == Some(1))
        .and_then(|tool| i32_field(&tool, "entity_id"))
        .unwrap();
    assert_eq!(item_quantity(&resources, 68), Some(11));
    assert_eq!(item_quantity(&resources, 67), Some(0));
    resources = finish_tutorial_talks(
        &proto,
        &fresh_rules,
        &tutorial_rules,
        resources,
        [101001011, 101001012],
    );

    let mut party_request = empty_message(&proto, "blend.api.PartyBulkUpdateRequest").unwrap();
    party_request.set_field_by_name("party_type", Value::I32(1));
    party_request.set_field_by_name("number", Value::I32(1));
    party_request.set_field_by_name("leader_position", Value::I32(1));
    let mut first = empty_message(&proto, "blend.model.PartyMemberWithEquipment").unwrap();
    first.set_field_by_name(
        "character_id",
        Value::Message(int32_value(&proto, 43101).unwrap()),
    );
    let mut second = empty_message(&proto, "blend.model.PartyMemberWithEquipment").unwrap();
    second.set_field_by_name(
        "character_id",
        Value::Message(int32_value(&proto, 32201).unwrap()),
    );
    party_request.set_field_by_name(
        "members",
        Value::List(vec![
            Value::Message(first.clone()),
            Value::Message(second.clone()),
        ]),
    );
    party_request.set_field_by_name(
        "battle_tool_entity_ids",
        Value::List(vec![
            Value::I32(first_tool_entity_id),
            Value::I32(second_tool_entity_id),
        ]),
    );
    resources = reduce_party(&proto, &fresh_rules, resources, &party_request, false)
        .unwrap()
        .resources;

    resources = play_tutorial_battle(&proto, &tutorial_rules, resources, 101001013);
    resources = finish_tutorial_talks(
        &proto,
        &fresh_rules,
        &tutorial_rules,
        resources,
        [101001014],
    );
    assert!(matches!(
        reduce_battle_start(&proto, &tutorial_rules, resources.clone(), 101001015, 1,),
        Err(StateError::InvalidRequest)
    ));
    let memoria_entity_id = message_list(&resources, "memorias")
        .into_iter()
        .find(|memoria| i32_field(memoria, "memoria_id") == Some(10001))
        .and_then(|memoria| i32_field(&memoria, "entity_id"))
        .unwrap();
    let mut equipped_first = first.clone();
    equipped_first.set_field_by_name(
        "memoria_entity_id",
        Value::Message(int32_value(&proto, memoria_entity_id).unwrap()),
    );
    party_request.set_field_by_name(
        "members",
        Value::List(vec![Value::Message(equipped_first), Value::Message(second)]),
    );
    resources = reduce_party(&proto, &fresh_rules, resources, &party_request, false)
        .unwrap()
        .resources;
    assert_eq!(tutorial_step(&resources), TUTORIAL_STEP_MEMORIA_EQUIPPED);
    resources = play_tutorial_battle(&proto, &tutorial_rules, resources, 101001015);
    resources = finish_tutorial_talks(
        &proto,
        &fresh_rules,
        &tutorial_rules,
        resources,
        [101001016, 101001017],
    );
    assert!((101001001..=101001017).all(|quest_id| quest_clear_count(&resources, quest_id) == 1));

    let now = 1_788_307_200;
    let list = reduce_gacha_list(&proto, &tutorial_rules, &resources, 1, &[], &[], now).unwrap();
    assert_eq!(
        message_list(&list, "gachas")
            .iter()
            .filter_map(|gacha| i32_field(gacha, "gacha_id"))
            .collect::<Vec<_>>(),
        vec![3080, 3075, 5001, 5000, 2215, 2100, 2099, 2098, 2097, 2096, 2095, 2094, 2093, 2]
    );
    assert!(message_list(&list, "wish_list_states").is_empty());
    let special = reduce_gacha_list(&proto, &tutorial_rules, &resources, 2, &[], &[], now).unwrap();
    assert_eq!(
        message_list(&special, "gachas")
            .iter()
            .filter_map(|gacha| i32_field(gacha, "gacha_id"))
            .collect::<Vec<_>>(),
        Vec::<i32>::new()
    );
    let fes = gacha_rate_set_message(
        &proto,
        &tutorial_rules,
        gacha_rule(&tutorial_rules, 5001).unwrap(),
    )
    .unwrap();
    assert_eq!(
        string_field(&message_list(&fes, "rows")[0], "percent_rate_per_card"),
        Some("0.072".into())
    );
    let mixed = mixed_gacha_rate_set_message(
        &proto,
        &tutorial_rules,
        gacha_rule(&tutorial_rules, 2215).unwrap(),
    )
    .unwrap();
    assert_eq!(
        message_list(&message_list(&mixed, "dynamic_rows")[0], "rates")
            .iter()
            .filter_map(|rate| string_field(rate, "percent_rate_per_card"))
            .collect::<Vec<_>>(),
        vec!["0.037", "0.029"]
    );
    let mut gacha_request = empty_message(&proto, "blend.api.GachaExecuteRequest").unwrap();
    gacha_request.set_field_by_name("gacha_id", Value::I32(2));
    gacha_request.set_field_by_name("gacha_button_id", Value::I32(14));
    let gacha = reduce_gacha_execute(
        &proto,
        &tutorial_rules,
        resources.clone(),
        &gacha_request,
        0,
        None,
        now,
    )
    .unwrap();
    let character_id = i32_field(
        message_list(&gacha.response, "drawn_rewards")
            .first()
            .unwrap(),
        "id",
    )
    .unwrap();

    let mut paid_resources = gacha.resources.clone();
    let mut wallet = paid_resources
        .get_field_by_name("wallet")
        .and_then(|value| value.as_message().cloned())
        .unwrap();
    wallet.set_field_by_name("free", Value::I32(3_000));
    paid_resources.set_field_by_name("wallet", Value::Message(wallet));
    let mut paid_request = empty_message(&proto, "blend.api.GachaExecuteRequest").unwrap();
    paid_request.set_field_by_name("gacha_id", Value::I32(3075));
    paid_request.set_field_by_name("gacha_button_id", Value::I32(10002));
    let paid = reduce_gacha_execute(
        &proto,
        &tutorial_rules,
        paid_resources,
        &paid_request,
        0,
        None,
        now,
    )
    .unwrap();
    assert_eq!(message_list(&paid.response, "drawn_rewards").len(), 10);
    assert_eq!(
        message_list(&paid.response, "medal_rewards")
            .iter()
            .filter_map(|reward| i32_field(reward, "quantity"))
            .collect::<Vec<_>>(),
        vec![10, 1]
    );
    let wallet = paid
        .resources
        .get_field_by_name("wallet")
        .and_then(|value| value.as_message().cloned())
        .unwrap();
    assert_eq!(i32_field(&wallet, "free"), Some(0));
    assert_eq!(item_quantity(&paid.resources, 131), Some(11));
    assert_eq!(total_task_count(&paid.resources, 137), 11);

    let mixed_rule = gacha_rule(&tutorial_rules, 2215).unwrap();
    let wish_list = GachaWishList {
        gacha_id: 2215,
        character_ids: vec![mixed_rule.mixed_pickup_character_ids[0]],
        memoria_ids: Vec::new(),
        character_skin_ids: Vec::new(),
    };
    validate_wish_list(mixed_rule, &wish_list).unwrap();
    assert_eq!(
        gacha_duplicate_counts(mixed_rule, wish_list.character_ids[0], 3).unwrap(),
        (100, 50)
    );
    assert_eq!(
        gacha_duplicate_counts(mixed_rule, 43102, 3).unwrap(),
        (50, 50)
    );
    let weighted = weighted_gacha_cards(&tutorial_rules, mixed_rule, Some(&wish_list)).unwrap();
    assert_eq!(
        weighted.iter().map(|(_, weight)| weight).sum::<u64>(),
        100_000
    );
    assert_eq!(
        weighted
            .iter()
            .find(|(card, _)| card.id == wish_list.character_ids[0])
            .map(|(_, weight)| *weight),
        Some(1_000)
    );

    let path = env::temp_dir().join(format!("atelier-home-{}.sqlite3", Uuid::new_v4()));
    let store = Store::open(&path).unwrap();
    store
        .create_account("tutorial_home", "$argon2id$v=19$test")
        .unwrap();
    store
        .ensure_player_state(1, &resources.encode_to_vec(), "jp")
        .unwrap();
    store.set_gacha_wish_list(1, &wish_list, now).unwrap();
    assert_eq!(store.gacha_wish_lists(1).unwrap(), vec![wish_list.clone()]);
    let response = gacha.response.encode_to_vec();
    let commit = GachaCommit {
        gameplay: GameplayCommit {
            resources_blob: gacha.resources.encode_to_vec(),
            response_plaintext: response.clone(),
            character_index: gacha.character_indexes,
        },
    };
    assert_eq!(
        store
            .apply_gacha(
                1,
                "gacha-1",
                "/gacha/execute",
                &[9; 32],
                2,
                14,
                now,
                |_, count, _| {
                    assert_eq!(count, 0);
                    Ok(commit)
                },
            )
            .unwrap(),
        GameplayMutationResult::Applied
    );
    assert_eq!(
        store
            .apply_gacha(
                1,
                "gacha-1",
                "/gacha/execute",
                &[9; 32],
                2,
                14,
                now + 1,
                |_, _, _| panic!("an idempotent retry must not redraw"),
            )
            .unwrap(),
        GameplayMutationResult::Replay(response)
    );
    assert!(matches!(
        store.apply_gacha(
            1,
            "gacha-2",
            "/gacha/execute",
            &[10; 32],
            2,
            14,
            now + 2,
            |_, count, _| {
                assert_eq!(count, 1);
                Err(StorageError::GachaLimit)
            },
        ),
        Err(StorageError::GachaLimit)
    ));
    drop(store);

    let reopened = Store::open(&path).unwrap();
    assert_eq!(reopened.gacha_wish_lists(1).unwrap(), vec![wish_list]);
    let persisted = proto
        .decode(
            "blend.model.Resources",
            &reopened.player_resources(1).unwrap(),
        )
        .unwrap();
    assert!(character_present(&persisted, character_id));
    assert!((101001001..=101001017).all(|quest_id| quest_clear_count(&persisted, quest_id) == 1));
    let status = status_message(&persisted).unwrap();
    assert_eq!(
        i32_field(&status, "last_main_story_quest_id"),
        Some(101001017)
    );
    assert_eq!(i32_field(&status, "tutorial_step"), Some(500));
    assert_eq!(total_task_count(&persisted, 47), 5);
    assert_eq!(total_task_count(&persisted, 85), 3);
    assert_eq!(total_task_count(&persisted, 137), 1);
    assert_eq!(
        total_task_count(
            &persisted,
            match character_id {
                43102 => 1519,
                39901 => 1520,
                10101 => 1521,
                _ => panic!("unexpected tutorial character"),
            },
        ),
        1
    );
    assert_eq!(i32_field(&status, "cole"), Some(1321));
    assert_eq!(item_quantity(&persisted, 67), Some(0));
    assert_eq!(item_quantity(&persisted, 68), Some(11));
    assert_eq!(item_quantity(&persisted, 83), Some(5));
    assert_eq!(item_quantity(&persisted, 56), Some(5));
    assert!((3..=7).contains(&message_list(&persisted, "battle_tools").len()));
    let memoria = message_list(&persisted, "memorias")
        .into_iter()
        .find(|memoria| i32_field(memoria, "memoria_id") == Some(10001))
        .unwrap();
    let memoria_entity_id = i32_field(&memoria, "entity_id").unwrap();
    assert!(!message_list(&persisted, "battle_tools")
        .iter()
        .any(|tool| i32_field(tool, "entity_id") == Some(memoria_entity_id)));
    assert_eq!(
        i32_list(
            &find_party(&persisted, 1, 1).unwrap(),
            "battle_tool_entity_ids"
        ),
        vec![first_tool_entity_id, second_tool_entity_id]
    );
    assert!(reopened.active_battle(1).unwrap().is_none());
    let states = reopened.gacha_button_states(1).unwrap();
    let draw = states
        .iter()
        .find(|state| state.gacha_id == 2 && state.button_id == 14)
        .unwrap();
    assert_eq!(draw.execution_count, 1);
    let used_list = reduce_gacha_list(
        &proto,
        &tutorial_rules,
        &persisted,
        1,
        &states,
        &[],
        now + 3,
    )
    .unwrap();
    let listed_gacha = message_list(&used_list, "gachas")
        .into_iter()
        .find(|gacha| i32_field(gacha, "gacha_id") == Some(2))
        .unwrap();
    assert_eq!(
        i32_field(
            &message_list(&listed_gacha, "gacha_button_states")
                .pop()
                .unwrap(),
            "execution_count"
        ),
        Some(1)
    );
    drop(reopened);
    let _ = fs::remove_file(path);
}

pub(super) fn synthesis_test_resources(
    proto: &ProtoRegistry,
    fresh: &FreshStateRules,
    tutorial: &TutorialRules,
) -> DynamicMessage {
    reduce_talk_event(
        proto,
        fresh,
        tutorial,
        starter_resources(proto, fresh).unwrap(),
        101001001,
        1,
    )
    .unwrap()
    .resources
}

pub(super) fn synthesis_request(proto: &ProtoRegistry, mode: SynthesisMode) -> DynamicMessage {
    let message = match mode {
        SynthesisMode::Execute => "blend.api.SynthesisExecuteRequest",
        SynthesisMode::Bulk => "blend.api.SynthesisBulkExecuteRequest",
        SynthesisMode::Easy => "blend.api.SynthesisExecuteEasyRequest",
        SynthesisMode::Rental => "blend.api.SynthesisExecuteRentalRequest",
    };
    let mut request = empty_message(proto, message).unwrap();
    request.set_field_by_name("recipe_id", Value::I32(6));
    if mode != SynthesisMode::Execute {
        request.set_field_by_name("count", Value::I32(1));
    }
    if matches!(mode, SynthesisMode::Execute | SynthesisMode::Bulk) {
        request.set_field_by_name(
            "character_ids",
            Value::List(vec![Value::I32(32201), Value::I32(43101)]),
        );
        request.set_field_by_name(
            "ingredient_id",
            Value::Message(int32_value(proto, 67).unwrap()),
        );
    }
    if mode == SynthesisMode::Rental {
        request.set_field_by_name("ranking_type", Value::I32(1));
        request.set_field_by_name("rank_number", Value::I32(1));
    }
    request
}

#[test]
fn synthesis_bonus_tools_keep_traits_and_slot_order() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let fresh = load_fresh_rules().unwrap();
    let tutorial = load_tutorial_rules().unwrap();
    let mut rules = load_synthesis_rules().unwrap();
    rules.local_policy.extra_target_rate = 0;
    for rank in &mut rules.trait_ranks {
        rank.weight = u32::from(rank.id == 1);
    }
    for character in rules
        .characters
        .iter_mut()
        .filter(|character| [32201, 43101].contains(&character.id))
    {
        character.battle_trait_ids = vec![45];
    }
    rules
        .ingredients
        .iter_mut()
        .find(|ingredient| ingredient.id == 67)
        .unwrap()
        .battle_trait_ids = vec![45];
    rules
        .recipes
        .iter_mut()
        .find(|recipe| recipe.id == 6)
        .unwrap()
        .bonus_rewards = vec![TutorialReward {
        resource_type: 14,
        id: 1,
        quantity: 1,
        resource_params: None,
    }];

    let mutation = reduce_synthesis(
        &proto,
        &rules,
        &home::load_rules().unwrap(),
        &activities::load_rules().unwrap(),
        synthesis_test_resources(&proto, &fresh, &tutorial),
        &synthesis_request(&proto, SynthesisMode::Bulk),
        SynthesisMode::Bulk,
        1,
    )
    .unwrap();
    let rewards = message_list(&mutation.response, "rewards")
        .into_iter()
        .filter(|reward| i32_field(reward, "type") == Some(14))
        .collect::<Vec<_>>();
    assert_eq!(
        rewards
            .iter()
            .filter_map(|reward| i32_field(reward, "id"))
            .collect::<Vec<_>>(),
        vec![3, 1, 1]
    );
    assert!(rewards.iter().all(|reward| {
        member_status(reward, "resource_params")
            .is_ok_and(|params| message_list(&params, "traits").len() == 3)
    }));
    let changed = member_status(&mutation.response, "changed_resources").unwrap();
    let changed_tools = message_list(&changed, "battle_tools");
    assert_eq!(
        changed_tools
            .iter()
            .filter_map(|tool| i32_field(tool, "tool_id"))
            .collect::<Vec<_>>(),
        vec![3, 1, 1]
    );
    assert!(changed_tools
        .iter()
        .all(|tool| message_list(tool, "traits").len() == 3));
    assert_eq!(i32_field(&mutation.response, "drama_reward_index"), Some(1));
}

#[test]
fn synthesis_routes_use_complete_master_rules() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let fresh = load_fresh_rules().unwrap();
    let tutorial = load_tutorial_rules().unwrap();
    let mut rules = load_synthesis_rules().unwrap();
    let home_rules = home::load_rules().unwrap();
    let activity_rules = activities::load_rules().unwrap();
    let now = unix_now();
    let mut regenerating = synthesis_test_resources(&proto, &fresh, &tutorial);
    let mut status = status_message(&regenerating).unwrap();
    status.set_field_by_name("mana_when_updated", Value::I32(0));
    status.set_field_by_name(
        "mana_updated_at",
        Value::Message(timestamp(&proto, now - 7201).unwrap()),
    );
    regenerating.set_field_by_name("status", Value::Message(status));
    let status = change_mana(&proto, &rules, &mut regenerating, -1, now).unwrap();
    assert_eq!(i32_field(&status, "mana_when_updated"), Some(1));
    assert_eq!(
        message_i64_field(&status, "mana_updated_at", "seconds"),
        Some(now - 1)
    );
    assert!(change_mana(&proto, &rules, &mut regenerating, -2, now).is_err());
    assert_eq!(
        i32_field(&status_message(&regenerating).unwrap(), "mana_when_updated"),
        Some(1)
    );
    let recipe = rules.recipes.iter().find(|recipe| recipe.id == 6).unwrap();
    assert_eq!(
        synthesis_trait_candidates(
            &rules,
            recipe,
            &synthesis_test_resources(&proto, &fresh, &tutorial),
            &[32201, 43101],
            67,
            false,
        )
        .unwrap()
        .len(),
        6
    );
    for mode in [
        SynthesisMode::Execute,
        SynthesisMode::Bulk,
        SynthesisMode::Easy,
        SynthesisMode::Rental,
    ] {
        let request = synthesis_request(&proto, mode);
        let mutation = reduce_synthesis(
            &proto,
            &rules,
            &home_rules,
            &activity_rules,
            synthesis_test_resources(&proto, &fresh, &tutorial),
            &request,
            mode,
            1,
        )
        .unwrap();
        let target_count = message_list(&mutation.response, "rewards")
            .iter()
            .filter(|reward| i32_field(reward, "type") == Some(14))
            .count();
        assert!((1..=3).contains(&target_count));
        for tool in message_list(&mutation.resources, "battle_tools") {
            for trait_param in message_list(&tool, "traits") {
                assert!(synthesis_trait_rule(
                    &rules,
                    recipe,
                    i32_field(&trait_param, "id").unwrap()
                )
                .is_some());
                assert!((1..=5).contains(&i32_field(&trait_param, "rank").unwrap()));
            }
        }
        assert_eq!(
            message_list(&mutation.resources, "battle_tools").len(),
            1 + target_count
        );
        let saved_recipe = message_list(&mutation.resources, "recipes")
            .into_iter()
            .find(|value| i32_field(value, "recipe_id") == Some(6))
            .unwrap();
        assert_eq!(
            item_quantity(&mutation.resources, 67),
            Some(
                if optional_i32_field(&saved_recipe, "last_ingredient_id") == Some(67) {
                    1
                } else {
                    2
                }
            )
        );
    }
    let resources = synthesis_test_resources(&proto, &fresh, &tutorial);
    let mut ingredient_resources = resources.clone();
    change_item(&proto, &mut ingredient_resources, 67, 1).unwrap();
    let mut ingredient_request = synthesis_request(&proto, SynthesisMode::Bulk);
    ingredient_request.set_field_by_name(
        "ingredient_id",
        Value::Message(int32_value(&proto, 67).unwrap()),
    );
    ingredient_request.set_field_by_name("count", Value::I32(1));
    let ingredient_result = reduce_synthesis(
        &proto,
        &rules,
        &home_rules,
        &activity_rules,
        ingredient_resources,
        &ingredient_request,
        SynthesisMode::Bulk,
        1,
    )
    .unwrap();
    assert_eq!(total_task_count(&ingredient_result.resources, 239), 0);
    assert_eq!(total_task_count(&ingredient_result.resources, 240), 0);
    let mut request =
        empty_message(&proto, "blend.api.SynthesisCombinationRankingRequest").unwrap();
    request.set_field_by_name("recipe_id", Value::I32(6));
    let ranking = reduce_synthesis_ranking(&proto, &rules, &resources, &request, 1).unwrap();
    let combinations = ["total", "weekly", "monthly"].map(|field| {
        message_list(&member_status(&ranking, field).unwrap(), "ranks")
            .into_iter()
            .map(|rank| {
                (
                    i32_field(&rank, "character1_id").unwrap(),
                    i32_field(&rank, "character2_id").unwrap(),
                    optional_i32_field(&rank, "ingredient_id").unwrap(),
                )
            })
            .collect::<Vec<_>>()
    });
    let ranking_count = usize::try_from(rules.constants.max_rental_rank).unwrap();
    assert!(combinations
        .iter()
        .all(|values| values.len() == ranking_count));
    assert!(combinations
        .iter()
        .all(|values| values.iter().collect::<BTreeSet<_>>().len() == ranking_count));
    assert_ne!(combinations[0], combinations[1]);
    assert_ne!(combinations[1], combinations[2]);

    let selected = combinations[1][1];
    let mut rental = synthesis_request(&proto, SynthesisMode::Rental);
    rental.set_field_by_name("ranking_type", Value::I32(2));
    rental.set_field_by_name("rank_number", Value::I32(2));
    let rental_mutation = reduce_synthesis(
        &proto,
        &rules,
        &home_rules,
        &activity_rules,
        resources.clone(),
        &rental,
        SynthesisMode::Rental,
        1,
    )
    .unwrap();
    let saved_recipe = message_list(&rental_mutation.resources, "recipes")
        .into_iter()
        .find(|recipe| i32_field(recipe, "recipe_id") == Some(6))
        .unwrap();
    assert_eq!(
        i32_list(&saved_recipe, "last_character_ids"),
        vec![selected.0, selected.1]
    );
    assert_eq!(
        optional_i32_field(&saved_recipe, "last_ingredient_id"),
        Some(selected.2)
    );

    rules.local_policy.extra_target_rate = 100;
    rules.local_policy.great_success_rate = 100;
    rules.local_policy.stimulator_drop_rate = 0;
    for rank in &mut rules.trait_ranks {
        rank.weight = u32::from(rank.id == 1);
    }
    let mut bulk_resources = synthesis_test_resources(&proto, &fresh, &tutorial);
    change_item(&proto, &mut bulk_resources, 67, 12).unwrap();
    let ingredient_before = item_quantity(&bulk_resources, 67).unwrap();
    let tools_before = message_list(&bulk_resources, "battle_tools").len();
    let mut bulk = synthesis_request(&proto, SynthesisMode::Bulk);
    bulk.set_field_by_name("count", Value::I32(2));
    let bulk_mutation = reduce_synthesis(
        &proto,
        &rules,
        &home_rules,
        &activity_rules,
        bulk_resources,
        &bulk,
        SynthesisMode::Bulk,
        1,
    )
    .unwrap();
    assert_eq!(i32_field(&bulk_mutation.response, "grade"), Some(4));
    assert_eq!(
        i32_field(&bulk_mutation.response, "start_grade_up"),
        Some(0)
    );
    assert_eq!(
        i32_field(&bulk_mutation.response, "final_grade_up"),
        Some(1)
    );
    assert_eq!(
        item_quantity(&bulk_mutation.resources, 67),
        Some(ingredient_before - 12)
    );
    assert_eq!(
        message_list(&bulk_mutation.resources, "battle_tools").len(),
        tools_before + 6
    );
    assert_eq!(
        message_list(&bulk_mutation.response, "rewards")
            .iter()
            .filter(|reward| i32_field(reward, "type") == Some(14))
            .count(),
        6
    );

    let mut invalid = synthesis_request(&proto, SynthesisMode::Execute);
    invalid.set_field_by_name(
        "character_ids",
        Value::List(vec![Value::I32(28402), Value::I32(43101)]),
    );
    assert!(matches!(
        reduce_synthesis(
            &proto,
            &rules,
            &home_rules,
            &activity_rules,
            resources,
            &invalid,
            SynthesisMode::Execute,
            1
        ),
        Err(StateError::InvalidRequest)
    ));

    let mut disconnected = synthesis_request(&proto, SynthesisMode::Bulk);
    disconnected.set_field_by_name("recipe_id", Value::I32(2));
    disconnected.set_field_by_name(
        "character_ids",
        Value::List(vec![Value::I32(43101), Value::I32(32201)]),
    );
    disconnected.set_field_by_name(
        "ingredient_id",
        Value::Message(int32_value(&proto, 56).unwrap()),
    );
    let mutation = reduce_synthesis(
        &proto,
        &rules,
        &home_rules,
        &activity_rules,
        synthesis_test_resources(&proto, &fresh, &tutorial),
        &disconnected,
        SynthesisMode::Bulk,
        1,
    )
    .unwrap();
    for reward in message_list(&mutation.response, "rewards")
        .into_iter()
        .filter(|reward| i32_field(reward, "type") == Some(14))
    {
        let params = member_status(&reward, "resource_params").unwrap();
        let traits = message_list(&params, "traits");
        assert!((1..=3).contains(&traits.len()));
        assert!(traits.iter().all(|value| {
            [5, 13, 44, 45].contains(&i32_field(value, "id").unwrap_or_default())
        }));
    }
}
use super::*;
