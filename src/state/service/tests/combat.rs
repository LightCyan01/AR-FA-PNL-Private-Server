#[test]
fn invalid_skill_target_is_rejected() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let fresh_rules = load_fresh_rules().unwrap();
    let tutorial_rules = load_tutorial_rules().unwrap();
    let opening = reduce_talk_event(
        &proto,
        &fresh_rules,
        &tutorial_rules,
        starter_resources(&proto, &fresh_rules).unwrap(),
        101001001,
        1,
    )
    .unwrap();
    let start =
        reduce_battle_start(&proto, &tutorial_rules, opening.resources, 101001002, 1).unwrap();
    let mut request = empty_message(&proto, "blend.api.BattleAttackRequest").unwrap();
    request.set_field_by_name("mode", Value::EnumNumber(0));
    let mut command = empty_message(&proto, "blend.model.BattleSkillCommand").unwrap();
    command.set_field_by_name("skill_type", Value::I32(1));
    command.set_field_by_name("main_target_id", Value::I32(999));
    request.set_field_by_name("skill_command", Value::Message(command));

    assert!(matches!(
        reduce_battle_attack_with_effects(
            &proto,
            &tutorial_rules,
            start.state,
            &request,
            b"test-secret",
            "start-txid",
            None,
            &mut effects::Runtime::default(),
        ),
        Err(StateError::InvalidRequest)
    ));
}

#[test]
fn killed_timeline_units_use_sequential_indexes() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let mut units = [
        (1, 1, 0),
        (11, 1, 10),
        (12, 1, 20),
        (11, 2, 30),
        (12, 2, 40),
        (1, 2, 50),
        (11, 3, 60),
        (12, 3, 70),
    ]
    .into_iter()
    .map(|(member_id, number, wait)| build_timeline_unit(&proto, member_id, number, wait).unwrap())
    .collect();

    let moves = remove_timeline_members(&proto, &mut units, &[], &[11]).unwrap();

    assert_eq!(
        moves
            .iter()
            .map(|movement| optional_i32_field(movement, "from_index").unwrap())
            .collect::<Vec<_>>(),
        vec![1, 2, 4]
    );
    assert!(units
        .iter()
        .all(|unit| i32_field(unit, "member_id") != Some(11)));
}

#[test]
fn generic_battle_timeline_uses_master_data_slot_counts() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let fresh_rules = load_fresh_rules().unwrap();
    let tutorial_rules = load_tutorial_rules().unwrap();
    let opening = reduce_talk_event(
        &proto,
        &fresh_rules,
        &tutorial_rules,
        starter_resources(&proto, &fresh_rules).unwrap(),
        101001001,
        1,
    )
    .unwrap();
    let start =
        reduce_battle_start(&proto, &tutorial_rules, opening.resources, 101001002, 1).unwrap();
    let members = message_list(&start.state, "members");
    let units = quest::initial_timeline(&proto, &tutorial_rules, &members).unwrap();

    for member in members
        .iter()
        .filter(|member| bool_field(member, "is_alive"))
    {
        let id = member_id(member).unwrap();
        let expected = if member_type(member).unwrap() == 0 { 2 } else { 3 };
        assert_eq!(
            units
                .iter()
                .filter(|unit| i32_field(unit, "member_id") == Some(id))
                .count(),
            expected
        );
    }
}

fn setup_damage(setup: &DynamicMessage, skill_type: i32, target_id: i32) -> i64 {
    let selection = message_list(setup, "skill_selections")
        .into_iter()
        .find(|selection| i32_field(selection, "skill_type") == Some(skill_type))
        .unwrap();
    let target = message_list(&selection, "targets")
        .into_iter()
        .find(|target| i32_field(target, "target_id") == Some(target_id))
        .unwrap();
    target
        .get_field_by_name("hp_damage")
        .and_then(|value| value.as_message().cloned())
        .and_then(|value| value.get_field_by_name("value")?.as_i64())
        .unwrap()
}

fn assert_tutorial_start_matches_oracle(start: &BattleStartMutation, quest_id: i32) {
    let allies: Vec<_> = message_list(&start.state, "members")
        .into_iter()
        .filter(|member| member_type(member).ok() == Some(0))
        .map(|member| {
            let ally = member_status(&member, "ally").unwrap();
            (
                i32_field(&ally, "character_id").unwrap(),
                i32_field(&member, "max_hp").unwrap(),
                i32_field(&member_status(&member, "initial_status").unwrap(), "mental").unwrap(),
                i32_field(&member_status(&member, "current_status").unwrap(), "mental").unwrap(),
            )
        })
        .collect();
    let enemies: Vec<_> = message_list(&start.state, "members")
        .into_iter()
        .filter(|member| member_type(member).ok() == Some(1))
        .map(|member| {
            let enemy = member_status(&member, "enemy").unwrap();
            (
                i32_field(&enemy, "enemy_id").unwrap(),
                i32_field(&member, "max_hp").unwrap(),
                i32_field(&enemy, "break_gauge").unwrap(),
            )
        })
        .collect();
    let panels: Vec<_> = message_list(&start.state, "timeline_panels")
        .iter()
        .map(|panel| optional_i32_field(panel, "panel_id"))
        .collect();
    let (expected_allies, expected_enemies, expected_panels) = match quest_id {
        101001002 => (
            vec![(43101, 142, 43, 45)],
            vec![(80001050, 68, 15_468), (80001050, 68, 15_468)],
            vec![None; 10],
        ),
        101001008 => (
            vec![(39901, 483, 162, 186)],
            vec![(80009036, 600, 9_770); 3],
            vec![
                None,
                Some(14),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            ],
        ),
        101001013 => (
            vec![(43101, 156, 47, 51), (32201, 185, 50, 55)],
            vec![
                (80001008, 100, 8),
                (80001009, 100, 8),
                (80003006, 500, 7),
                (80001010, 100, 8),
                (80001011, 100, 8),
            ],
            vec![
                None,
                None,
                Some(14),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            ],
        ),
        101001015 => (
            vec![(43101, 178, 53, 58), (32201, 202, 54, 59)],
            vec![
                (80004004, 1000, 30),
                (80003007, 2160, 46),
                (80004005, 1000, 30),
            ],
            vec![
                None,
                Some(12),
                None,
                Some(14),
                Some(14),
                Some(13),
                Some(12),
                None,
                Some(14),
                None,
            ],
        ),
        _ => panic!("unexpected tutorial quest {quest_id}"),
    };
    assert_eq!(allies, expected_allies);
    assert_eq!(enemies, expected_enemies);
    assert_eq!(panels, expected_panels);
    assert!(!start.state.has_field_by_name("enemy_hp_gauge"));
    let history = member_status(&start.response, "history").unwrap();
    let setup = message_list(&history, "action_setups").pop().unwrap();
    assert_eq!(message_list(&setup, "skill_selections").len(), 2);
    let expected_damage = match quest_id {
        101001002 => vec![(11, 80, 321)],
        101001008 => vec![(11, 326, 1_609)],
        101001013 => vec![(11, 125, 499), (13, 127, 510)],
        101001015 => vec![(11, 250, 998), (12, 166, 665)],
        _ => unreachable!(),
    };
    for (target_id, skill_1, skill_2) in expected_damage {
        for (skill_type, expected) in [(1, skill_1), (2, skill_2)] {
            assert_eq!(setup_damage(&setup, skill_type, target_id), expected);
        }
    }
}

pub(super) fn tutorial_skill_attack(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    state: DynamicMessage,
    start_txid: &str,
    skill_type: i32,
    target_id: i32,
) -> BattleAttackMutation {
    let mut request = empty_message(proto, "blend.api.BattleAttackRequest").unwrap();
    request.set_field_by_name("mode", Value::EnumNumber(0));
    let mut command = empty_message(proto, "blend.model.BattleSkillCommand").unwrap();
    command.set_field_by_name("skill_type", Value::I32(skill_type));
    command.set_field_by_name("main_target_id", Value::I32(target_id));
    request.set_field_by_name("skill_command", Value::Message(command));
    reduce_battle_attack_with_effects(
        proto,
        rules,
        state,
        &request,
        b"test-secret",
        start_txid,
        None,
        &mut effects::Runtime::default(),
    )
    .unwrap()
}

#[test]
fn terminal_battle_advances_action_cursor() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let fresh_rules = load_fresh_rules().unwrap();
    let rules = load_tutorial_rules().unwrap();
    let opening = reduce_talk_event(
        &proto,
        &fresh_rules,
        &rules,
        starter_resources(&proto, &fresh_rules).unwrap(),
        101001001,
        1,
    )
    .unwrap();
    let start = reduce_battle_start(&proto, &rules, opening.resources, 101001002, 1).unwrap();
    let txid = start.start_txid;
    let mut state = start.state;
    let target_id = message_list(&state, "members")
        .into_iter()
        .find(|member| member_type(member).ok() == Some(1))
        .and_then(|member| i32_field(&member, "member_id"))
        .unwrap();
    let mut members = message_list(&state, "members");
    for member in members.iter_mut() {
        if member_type(member).ok() == Some(1) {
            if i32_field(member, "member_id") == Some(target_id) {
                member.set_field_by_name("hp", Value::I32(1));
            } else {
                member.set_field_by_name("hp", Value::I32(0));
                member.set_field_by_name("is_alive", Value::Bool(false));
            }
        }
    }
    state.set_field_by_name(
        "members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );
    let alive_ids = message_list(&state, "members")
        .into_iter()
        .filter(|member| bool_field(member, "is_alive"))
        .filter_map(|member| i32_field(&member, "member_id"))
        .collect::<Vec<_>>();
    let mut units = message_list(&state, "timeline_units");
    units.retain(|unit| i32_field(unit, "member_id").is_some_and(|id| alive_ids.contains(&id)));
    state.set_field_by_name(
        "timeline_units",
        Value::List(units.into_iter().map(Value::Message).collect()),
    );
    let mut wave_ids = i32_list(&state, "wave_ids");
    wave_ids.truncate(1);
    state.set_field_by_name(
        "wave_ids",
        Value::List(wave_ids.into_iter().map(Value::I32).collect()),
    );

    let mut request = empty_message(&proto, "blend.api.BattleAttackRequest").unwrap();
    request.set_field_by_name("mode", Value::EnumNumber(0));
    let mut command = empty_message(&proto, "blend.model.BattleSkillCommand").unwrap();
    command.set_field_by_name("skill_type", Value::I32(1));
    command.set_field_by_name("main_target_id", Value::I32(target_id));
    request.set_field_by_name("skill_command", Value::Message(command));
    let mut runtime = start.effects;
    let attack = reduce_battle_attack_with_effects(
        &proto,
        &rules,
        state,
        &request,
        b"test-secret",
        &txid,
        None,
        &mut runtime,
    )
    .unwrap();
    let history = member_status(&attack.response, "history").unwrap();
    let action = message_list(&history, "actions")
        .into_iter()
        .find(|action| i32_or_enum_field(action, "actor_type") == Some(0))
        .unwrap();
    let action_number = i32_field(&action, "number").unwrap();
    assert_eq!(action_number, 1);
    assert_eq!(runtime.next_action_number, action_number + 1);
    assert_eq!(i32_field(&attack.state, "total_turn"), Some(action_number + 1));
    assert_eq!(current_battle_status(&attack.state).unwrap(), BATTLE_STATUS_WON);
}

#[test]
fn battle_tools_preserve_action_and_turn_cursors() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let fresh_rules = load_fresh_rules().unwrap();
    let rules = load_tutorial_rules().unwrap();
    let opening = reduce_talk_event(
        &proto,
        &fresh_rules,
        &rules,
        starter_resources(&proto, &fresh_rules).unwrap(),
        101001001,
        1,
    )
    .unwrap();
    let start = reduce_battle_start(&proto, &rules, opening.resources, 101001002, 1).unwrap();
    let txid = start.start_txid;
    let mut state = start.state;
    let mut runtime = start.effects;
    let mut tools = message_list(&state, "battle_tools");
    let mut second_tool = tools[0].clone();
    second_tool.set_field_by_name("number", Value::I32(2));
    second_tool.set_field_by_name("tool_id", Value::I32(1));
    tools.push(second_tool);
    state.set_field_by_name(
        "battle_tools",
        Value::List(tools.into_iter().map(Value::Message).collect()),
    );

    let mut gauge_request = empty_message(&proto, "blend.api.BattleAttackRequest").unwrap();
    gauge_request.set_field_by_name("mode", Value::EnumNumber(6));
    state = reduce_battle_attack_with_effects(
        &proto,
        &rules,
        state,
        &gauge_request,
        b"test-secret",
        &txid,
        None,
        &mut runtime,
    )
    .unwrap()
    .state;
    let turn_number = i32_field(&state, "total_turn").unwrap();

    let mut tool_request = empty_message(&proto, "blend.api.BattleAttackRequest").unwrap();
    tool_request.set_field_by_name("mode", Value::EnumNumber(1));
    let mut tool_command = empty_message(&proto, "blend.model.BattleBattleToolCommand").unwrap();
    tool_command.set_field_by_name(
        "battle_tool_numbers",
        Value::List(vec![Value::I32(1), Value::I32(2)]),
    );
    tool_request.set_field_by_name("battle_tool_command", Value::Message(tool_command));
    let tool_attack = reduce_battle_attack_with_effects(
        &proto,
        &rules,
        state,
        &tool_request,
        b"test-secret",
        &txid,
        None,
        &mut runtime,
    )
    .unwrap();
    let history = member_status(&tool_attack.response, "history").unwrap();
    let tool_actions = message_list(&history, "actions");
    assert_eq!(
        tool_actions
            .iter()
            .map(|action| i32_field(action, "number").unwrap())
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert!(tool_actions.iter().all(|action| i32_field(
        &member_status(action, "state").unwrap(),
        "total_turn"
    ) == Some(turn_number)));
    let expected_action_number = message_list(&history, "action_setups")
        .into_iter()
        .last()
        .and_then(|setup| i32_field(&setup, "number"))
        .unwrap();

    let next_turn = i32_field(&tool_attack.state, "total_turn").unwrap();
    let target_id =
        member_id(&earliest_living_member(&tool_attack.state, Some(1)).unwrap()).unwrap();
    let mut skill_request = empty_message(&proto, "blend.api.BattleAttackRequest").unwrap();
    skill_request.set_field_by_name("mode", Value::EnumNumber(0));
    let mut skill_command = empty_message(&proto, "blend.model.BattleSkillCommand").unwrap();
    skill_command.set_field_by_name("skill_type", Value::I32(1));
    skill_command.set_field_by_name("main_target_id", Value::I32(target_id));
    skill_request.set_field_by_name("skill_command", Value::Message(skill_command));
    let next = reduce_battle_attack_with_effects(
        &proto,
        &rules,
        tool_attack.state,
        &skill_request,
        b"test-secret",
        &txid,
        None,
        &mut runtime,
    )
    .unwrap();
    let actions = message_list(
        &member_status(&next.response, "history").unwrap(),
        "actions",
    );
    let action = actions
        .iter()
        .find(|action| i32_or_enum_field(action, "actor_type") == Some(0))
        .unwrap();
    assert_eq!(i32_field(action, "number"), Some(expected_action_number));
    assert_eq!(
        i32_field(&member_status(action, "state").unwrap(), "total_turn"),
        Some(next_turn)
    );
    let enemy_action = actions
        .iter()
        .find(|action| i32_or_enum_field(action, "actor_type") == Some(1))
        .unwrap();
    assert_eq!(
        i32_field(&member_status(enemy_action, "state").unwrap(), "total_turn"),
        Some(next_turn + 1)
    );
}

#[test]
fn combat_vector_first_tutorial_battle_preserves_item_and_burst_boundary() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let fresh_rules = load_fresh_rules().unwrap();
    let rules = load_gameplay_rules().unwrap();
    let opening = reduce_talk_event(
        &proto,
        &fresh_rules,
        &rules,
        starter_resources(&proto, &fresh_rules).unwrap(),
        101001001,
        1,
    )
    .unwrap();
    let start = reduce_battle_start(&proto, &rules, opening.resources, 101001002, 1).unwrap();
    let txid = start.start_txid;
    let mut state = start.state;
    let mut effect_state = state.clone();
    let tool_effects = battle_tool_effects(
        &rules,
        &BattlePartyTool {
            tool_id: 3,
            usage_count: 2,
            traits: vec![TutorialTraitParam { id: 17, rank: 3 }],
        },
    )
    .unwrap();
    let mut runtime = start.effects;
    let effects = runtime
        .apply(
            &proto,
            &mut effect_state,
            1,
            &tool_effects,
            &[11],
            false,
            "after",
            None,
        )
        .unwrap();
    assert_eq!(i32_field(&effects[0], "effect_id"), Some(3_000_049));
    assert_eq!(
        i32_field(
            &message_list(
                &message_list(&effect_state, "members")
                    .into_iter()
                    .find(|member| i32_field(member, "member_id") == Some(11))
                    .unwrap(),
                "state_changes",
            )[0],
            "state_change_id",
        ),
        Some(920_003)
    );
    for (index, (skill_type, target_id)) in [(1, 11), (2, 12), (2, 12), (1, 11), (2, 11)]
        .into_iter()
        .enumerate()
    {
        let attack = tutorial_skill_attack(&proto, &rules, state, &txid, skill_type, target_id);
        if index <= 2 {
            let history = member_status(&attack.response, "history").unwrap();
            let setup = message_list(&history, "action_setups")
                .into_iter()
                .rev()
                .find(|setup| i32_or_enum_field(setup, "actor_type").unwrap_or(0) == 0)
                .unwrap();
            let expected = [(88, 353), (120, 481), (80, 321)][index];
            let target_id = if index == 0 { 12 } else { 11 };
            assert_eq!(setup_damage(&setup, 1, target_id), expected.0);
            assert_eq!(setup_damage(&setup, 2, target_id), expected.1);
            let actor = message_list(&attack.state, "members")
                .into_iter()
                .find(|member| i32_field(member, "member_id") == Some(1))
                .unwrap();
            assert_eq!(
                state_change_summary_value(&actor, 1),
                if index < 2 { 1_000 } else { 0 }
            );
        }
        if index == 4 {
            let history = member_status(&attack.response, "history").unwrap();
            assert_eq!(
                message_list(&history, "actions")
                    .iter()
                    .map(|action| (
                        i32_or_enum_field(action, "actor_type").unwrap_or(0),
                        i32_field(action, "actor_id").unwrap()
                    ))
                    .collect::<Vec<_>>(),
                vec![(0, 1), (1, 12), (1, 13)]
            );
        }
        state = attack.state;
    }
    assert_eq!(i32_field(&state, "wave"), Some(3));
    assert_eq!(
        message_list(&state, "members")
            .iter()
            .filter(|member| member_type(member).ok() == Some(1) && bool_field(member, "is_alive"))
            .count(),
        2
    );

    let mut gauge_request = empty_message(&proto, "blend.api.BattleAttackRequest").unwrap();
    gauge_request.set_field_by_name("mode", Value::EnumNumber(6));
    let gauge_attack = reduce_battle_attack_with_effects(
        &proto,
        &rules,
        state,
        &gauge_request,
        b"test-secret",
        &txid,
        None,
        &mut effects::Runtime::default(),
    )
    .unwrap();
    let gauge_history = member_status(&gauge_attack.response, "history").unwrap();
    let gauge_setup = message_list(&gauge_history, "action_setups")
        .into_iter()
        .next()
        .unwrap();
    let tool_selection = message_list(&gauge_setup, "battle_tool_selections")
        .into_iter()
        .find(|selection| i32_field(selection, "number") == Some(1))
        .unwrap();
    let tool_target = message_list(&tool_selection, "targets")
        .into_iter()
        .find(|target| i32_field(target, "target_id") == Some(12))
        .unwrap();
    assert!(message_i64_field(&tool_target, "hp_damage", "value").unwrap_or_default() > 0);
    assert!(message_list(&gauge_attack.state, "members")
        .iter()
        .filter(|member| member_type(member).ok() == Some(0))
        .all(|member| !member.has_field_by_name("enemy")));
    state = gauge_attack.state;

    let panel_before_tools = current_panel_id(&state);
    let panel_turn_before_tools = message_list(&state, "timeline_panels")
        .first()
        .and_then(|panel| i32_field(panel, "turn"))
        .unwrap();
    let turn_before_tools = i32_field(&state, "total_turn").unwrap();
    let actor_before_tools = member_id(&current_actor(&state).unwrap()).unwrap();
    let mut tools = message_list(&state, "battle_tools");
    let starting_usage = i32_field(&tools[0], "usage_count").unwrap();
    let mut healing_tool = tools[0].clone();
    healing_tool.set_field_by_name("number", Value::I32(2));
    healing_tool.set_field_by_name("tool_id", Value::I32(1));
    tools.push(healing_tool);
    state.set_field_by_name(
        "battle_tools",
        Value::List(tools.into_iter().map(Value::Message).collect()),
    );

    let mut tool_request = empty_message(&proto, "blend.api.BattleAttackRequest").unwrap();
    tool_request.set_field_by_name("mode", Value::EnumNumber(1));
    let mut tool_command = empty_message(&proto, "blend.model.BattleBattleToolCommand").unwrap();
    tool_command.set_field_by_name(
        "battle_tool_numbers",
        Value::List(vec![Value::I32(1), Value::I32(2)]),
    );
    tool_request.set_field_by_name("battle_tool_command", Value::Message(tool_command));
    let tool_attack = reduce_battle_attack_with_effects(
        &proto,
        &rules,
        state,
        &tool_request,
        b"test-secret",
        &txid,
        None,
        &mut effects::Runtime::default(),
    )
    .unwrap();
    let tool_history = member_status(&tool_attack.response, "history").unwrap();
    let tool_actions = message_list(&tool_history, "actions");
    assert_eq!(
        tool_actions
            .iter()
            .map(|action| (
                i32_field(action, "number").unwrap(),
                i32_or_enum_field(action, "action_type").unwrap(),
                message_i32_field(action, "battle_tool_number", "value").unwrap(),
                i32_field(action, "actor_id").unwrap(),
            ))
            .collect::<Vec<_>>(),
        vec![
            (turn_before_tools, 1, 1, actor_before_tools),
            (turn_before_tools + 1, 1, 2, actor_before_tools),
        ]
    );
    let next_setup = message_list(&tool_history, "action_setups")
        .into_iter()
        .last()
        .unwrap();
    assert_eq!(
        i32_field(&next_setup, "number"),
        Some(turn_before_tools + 2)
    );
    assert_eq!(i32_field(&next_setup, "actor_id"), Some(actor_before_tools));
    state = tool_attack.state;
    assert_eq!(i32_field(&state, "total_turn"), Some(turn_before_tools + 1));
    assert_eq!(panel_before_tools, 11);
    assert_eq!(current_panel_id(&state), 14);
    assert_eq!(
        message_list(&state, "timeline_panels")
            .first()
            .and_then(|panel| i32_field(panel, "turn")),
        Some(panel_turn_before_tools + 1)
    );
    assert_eq!(
        member_id(&current_actor(&state).unwrap()).unwrap(),
        actor_before_tools
    );
    assert!(message_list(&state, "battle_tools").iter().all(|tool| {
        matches!(i32_field(tool, "number"), Some(1 | 2))
            && i32_field(tool, "usage_count") == Some(starting_usage - 1)
    }));
    let enemies: Vec<_> = message_list(&state, "members")
        .into_iter()
        .filter(|member| member_type(member).ok() == Some(1) && bool_field(member, "is_alive"))
        .collect();
    assert_eq!(enemies.len(), 2);
    assert!(enemies
        .iter()
        .all(|enemy| i32_field(enemy, "hp") == Some(23)));
    assert_eq!(current_panel_id(&state), 14);
    let actor = current_actor(&state).unwrap();
    assert_eq!(
        member_status(&actor, "burst_gauge")
            .ok()
            .and_then(|gauge| i32_field(&gauge, "current_gauge")),
        Some(rules.constants.burst_gauge_required_for_one_burst_skill)
    );
    assert!(bool_field(
        &member_status(&actor, "burst_gauge").unwrap(),
        "is_enable"
    ));
}

#[test]
fn battle_tool_attribute_traits_follow_catalog_attributes() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let fresh = load_fresh_rules().unwrap();
    let rules = load_gameplay_rules().unwrap();
    let resources = reduce_talk_event(
        &proto,
        &fresh,
        &rules,
        starter_resources(&proto, &fresh).unwrap(),
        101001001,
        1,
    )
    .unwrap()
    .resources;
    let start = reduce_battle_start(&proto, &rules, resources, 101001002, 1).unwrap();
    let members = message_list(&start.state, "members");
    let target = members
        .iter()
        .find(|member| member_type(member).ok() == Some(1))
        .unwrap();
    let skill = rule_skill(&rules, 31_550).unwrap();
    let plain = BattlePartyTool {
        tool_id: 2,
        usage_count: 2,
        traits: Vec::new(),
    };
    let fire_blessing = BattlePartyTool {
        traits: vec![TutorialTraitParam { id: 6, rank: 5 }],
        ..plain.clone()
    };
    let plain_damage =
        policy_tool_damage(&rules, &plain, &members, target, skill, None, false, 10_000).unwrap();
    let blessed_damage = policy_tool_damage(
        &rules,
        &fire_blessing,
        &members,
        target,
        skill,
        None,
        false,
        10_000,
    )
    .unwrap();
    assert!(
        blessed_damage > plain_damage,
        "fire blessing must affect the fire battle-tool skill"
    );
}

#[test]
fn battle_tool_damage_uses_contextual_incoming_modifiers() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let fresh = load_fresh_rules().unwrap();
    let rules = load_gameplay_rules().unwrap();
    let resources = reduce_talk_event(
        &proto,
        &fresh,
        &rules,
        starter_resources(&proto, &fresh).unwrap(),
        101001001,
        1,
    )
    .unwrap()
    .resources;
    let start = reduce_battle_start(&proto, &rules, resources, 101001002, 1).unwrap();
    let mut state = start.state.clone();
    let members = message_list(&state, "members");
    let source_id = members
        .iter()
        .find(|member| member_type(member).ok() == Some(0))
        .and_then(|member| member_id(member).ok())
        .unwrap();
    let target_id = members
        .iter()
        .find(|member| member_type(member).ok() == Some(1))
        .and_then(|member| member_id(member).ok())
        .unwrap();
    let skill = rule_skill(&rules, 31_551).unwrap();
    let tool = BattlePartyTool {
        tool_id: 3,
        usage_count: 2,
        traits: Vec::new(),
    };
    let baseline_members = members.clone();
    let mut runtime = effects::Runtime::default();
    runtime.prepare(&state, "tool-incoming").unwrap();
    runtime
        .apply(
            &proto,
            &mut state,
            source_id,
            &[TutorialSkillEffect {
                id: 71140004,
                value: 2_000,
            }],
            &[target_id],
            false,
            "after",
            None,
        )
        .unwrap();
    let members = message_list(&state, "members");
    let target = members
        .iter()
        .find(|member| member_id(member).ok() == Some(target_id))
        .unwrap();
    let baseline_target = baseline_members
        .iter()
        .find(|member| member_id(member).ok() == Some(target_id))
        .unwrap();
    let plain = policy_tool_damage(
        &rules,
        &tool,
        &baseline_members,
        baseline_target,
        skill,
        None,
        false,
        10_000,
    )
    .unwrap();
    let with_incoming = policy_tool_damage(
        &rules,
        &tool,
        &members,
        target,
        skill,
        Some(&runtime),
        false,
        10_000,
    )
    .unwrap();
    assert_eq!(with_incoming, plain * 12 / 10);
}

#[test]
fn battle_tool_healing_uses_source_and_target_modifiers() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_gameplay_rules().unwrap();
    let skill = rule_skill(&rules, 31_549).unwrap();
    let tool = BattlePartyTool {
        tool_id: 1,
        usage_count: 2,
        traits: vec![TutorialTraitParam { id: 44, rank: 3 }],
    };
    let fresh = load_fresh_rules().unwrap();
    let resources = reduce_talk_event(
        &proto,
        &fresh,
        &rules,
        starter_resources(&proto, &fresh).unwrap(),
        101001001,
        1,
    )
    .unwrap()
    .resources;
    let start = reduce_battle_start(&proto, &rules, resources, 101001002, 1).unwrap();
    let allies: Vec<_> = message_list(&start.state, "members")
        .into_iter()
        .filter(|member| member_type(member).ok() == Some(0))
        .collect();
    let source = allies[0].clone();
    let target = source.clone();
    let plain = policy_tool_heal(&rules, &tool, skill, &source, &target).unwrap();
    let mut boosted_source = source.clone();
    boosted_source.set_field_by_name(
        "state_changes",
        Value::List(vec![Value::Message(
            effects::display(&proto, 50013, 1_000, -1).unwrap(),
        )]),
    );
    let mut boosted_target = target.clone();
    boosted_target.set_field_by_name(
        "state_changes",
        Value::List(vec![Value::Message(
            effects::display(&proto, 50014, 1_000, -1).unwrap(),
        )]),
    );
    let boosted = policy_tool_heal(&rules, &tool, skill, &boosted_source, &boosted_target).unwrap();
    assert_eq!(boosted, plain * 12 / 10);
}

pub(super) fn play_tutorial_battle(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    resources: DynamicMessage,
    quest_id: i32,
) -> DynamicMessage {
    let start = reduce_battle_start(proto, rules, resources.clone(), quest_id, 1).unwrap();
    assert_tutorial_start_matches_oracle(&start, quest_id);
    let start_txid = start.start_txid;
    let mut state = start.state;
    let mut runtime = start.effects;
    if quest_id == 101001013 {
        let mut members = message_list(&state, "members");
        let ally = members
            .iter_mut()
            .find(|member| member_type(member).ok() == Some(0))
            .unwrap();
        let max_hp = i32_field(ally, "max_hp").unwrap();
        ally.set_field_by_name("hp", Value::I32(max_hp - 50));
        state.set_field_by_name(
            "members",
            Value::List(members.into_iter().map(Value::Message).collect()),
        );
    }
    let mut used_tutorial_heal = false;
    for _ in 0..500 {
        match current_battle_status(&state).unwrap() {
            BATTLE_STATUS_WON => break,
            BATTLE_STATUS_LOST => panic!(
                "tutorial battle {quest_id} lost at turn {} wave {} ally_hp={:?} enemy_hp={:?}",
                i32_field(&state, "total_turn").unwrap_or_default(),
                i32_field(&state, "wave").unwrap_or_default(),
                message_list(&state, "members")
                    .iter()
                    .filter(|member| member_type(member).ok() == Some(0))
                    .map(|member| i32_field(member, "hp").unwrap_or_default())
                    .collect::<Vec<_>>(),
                message_list(&state, "members")
                    .iter()
                    .filter(|member| member_type(member).ok() == Some(1))
                    .map(|member| i32_field(member, "hp").unwrap_or_default())
                    .collect::<Vec<_>>()
            ),
            BATTLE_STATUS_IN_BATTLE => {}
            status => panic!("unexpected battle status {status}"),
        }
        let actor = current_actor(&state).unwrap();
        assert_eq!(member_type(&actor).unwrap(), 0);
        if quest_id == 101001013
            && !used_tutorial_heal
            && message_list(&state, "members").iter().any(|member| {
                member_type(member).ok() == Some(0)
                    && i32_field(member, "hp").unwrap_or_default()
                        < i32_field(member, "max_hp").unwrap_or_default()
            })
        {
            let tool = message_list(&state, "battle_tools")
                .into_iter()
                .find(|tool| {
                    i32_field(tool, "tool_id") == Some(1)
                        && i32_field(tool, "usage_count").unwrap_or_default() > 0
                })
                .unwrap();
            let tool_number = i32_field(&tool, "number").unwrap();
            let hp_before: i32 = message_list(&state, "members")
                .iter()
                .filter(|member| member_type(member).ok() == Some(0))
                .map(|member| i32_field(member, "hp").unwrap_or_default())
                .sum();

            let mut gauge_request = empty_message(proto, "blend.api.BattleAttackRequest").unwrap();
            gauge_request.set_field_by_name("mode", Value::EnumNumber(6));
            state = reduce_battle_attack_with_effects(
                proto,
                rules,
                state,
                &gauge_request,
                b"test-secret",
                &start_txid,
                None,
                &mut runtime,
            )
            .unwrap()
            .state;

            let mut tool_request = empty_message(proto, "blend.api.BattleAttackRequest").unwrap();
            tool_request.set_field_by_name("mode", Value::EnumNumber(1));
            let mut tool_command =
                empty_message(proto, "blend.model.BattleBattleToolCommand").unwrap();
            tool_command.set_field_by_name(
                "battle_tool_numbers",
                Value::List(vec![Value::I32(tool_number)]),
            );
            tool_request.set_field_by_name("battle_tool_command", Value::Message(tool_command));
            state = reduce_battle_attack_with_effects(
                proto,
                rules,
                state,
                &tool_request,
                b"test-secret",
                &start_txid,
                None,
                &mut runtime,
            )
            .unwrap()
            .state;
            let hp_after: i32 = message_list(&state, "members")
                .iter()
                .filter(|member| member_type(member).ok() == Some(0))
                .map(|member| i32_field(member, "hp").unwrap_or_default())
                .sum();
            assert_eq!(hp_after - hp_before, 50);
            assert_eq!(i32_field(&state, "party_gauge"), Some(0));
            used_tutorial_heal = true;
            continue;
        }
        let burst_enabled = member_status(&actor, "burst_gauge")
            .ok()
            .and_then(|gauge| i32_field(&gauge, "current_gauge"))
            .unwrap_or(0)
            >= rules.constants.burst_gauge_required_for_one_burst_skill;
        assert_eq!(
            bool_field(&member_status(&actor, "burst_gauge").unwrap(), "is_enable"),
            burst_enabled,
            "tutorial {quest_id}: client burst availability must match the usable gauge"
        );
        let target_id = member_id(&earliest_living_member(&state, Some(1)).unwrap()).unwrap();
        let mut request = empty_message(proto, "blend.api.BattleAttackRequest").unwrap();
        request.set_field_by_name("mode", Value::EnumNumber(0));
        let mut command = empty_message(proto, "blend.model.BattleSkillCommand").unwrap();
        let skill_type = if burst_enabled {
            3
        } else if i32_field(&state, "total_turn") == Some(1) {
            1
        } else {
            2
        };
        command.set_field_by_name("skill_type", Value::I32(skill_type));
        command.set_field_by_name("main_target_id", Value::I32(target_id));
        request.set_field_by_name("skill_command", Value::Message(command));
        state = reduce_battle_attack_with_effects(
            proto,
            rules,
            state,
            &request,
            b"test-secret",
            &start_txid,
            None,
            &mut runtime,
        )
        .unwrap()
        .state;
    }
    assert_eq!(current_battle_status(&state).unwrap(), BATTLE_STATUS_WON);
    if quest_id == 101001013 {
        assert!(used_tutorial_heal);
    }
    let reward_rules = load_reward_rules().unwrap();
    let finish =
        reduce_battle_finish(proto, rules, &reward_rules, state, resources, quest_id).unwrap();
    assert_eq!(quest_clear_count(&finish.resources, quest_id), 1);
    let quest_result = member_status(&finish.response, "quest_result").unwrap();
    assert!(message_list(&quest_result, "rewards").is_empty());
    let first_clear_rewards: Vec<_> = message_list(&quest_result, "first_clear_rewards")
        .iter()
        .map(|reward| {
            (
                i32_field(reward, "type").unwrap(),
                i32_field(reward, "id").unwrap(),
                i32_field(reward, "quantity").unwrap(),
            )
        })
        .collect();
    assert_eq!(
        first_clear_rewards,
        match quest_id {
            101001002 => vec![(3, 1, 102)],
            101001008 => vec![],
            101001013 => vec![(3, 1, 107), (17, 10001, 1)],
            101001015 => vec![(3, 1, 112)],
            _ => unreachable!(),
        }
    );
    finish.resources
}

pub(super) fn finish_tutorial_talks(
    proto: &ProtoRegistry,
    fresh_rules: &FreshStateRules,
    rules: &TutorialRules,
    mut resources: DynamicMessage,
    quest_ids: impl IntoIterator<Item = i32>,
) -> DynamicMessage {
    for quest_id in quest_ids {
        resources = reduce_talk_event(proto, fresh_rules, rules, resources, quest_id, 1)
            .unwrap()
            .resources;
    }
    resources
}

pub(super) fn synthesize_tutorial_tool(
    proto: &ProtoRegistry,
    rules: &SynthesisRules,
    resources: DynamicMessage,
    recipe_id: i32,
    character_ids: &[i32],
    ingredient_id: i32,
) -> DynamicMessage {
    let mut request = empty_message(proto, "blend.api.SynthesisBulkExecuteRequest").unwrap();
    request.set_field_by_name("recipe_id", Value::I32(recipe_id));
    request.set_field_by_name(
        "character_ids",
        Value::List(character_ids.iter().copied().map(Value::I32).collect()),
    );
    request.set_field_by_name(
        "ingredient_id",
        Value::Message(int32_value(proto, ingredient_id).unwrap()),
    );
    request.set_field_by_name("count", Value::I32(1));
    reduce_synthesis(
        proto,
        rules,
        &home::load_rules().unwrap(),
        &activities::load_rules().unwrap(),
        resources,
        &request,
        SynthesisMode::Bulk,
        1,
    )
    .unwrap()
    .resources
}
use super::*;
