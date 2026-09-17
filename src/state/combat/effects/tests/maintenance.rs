use super::super::*;
use crate::state::combat::prelude::*;
use std::path::Path;

#[test]
fn effect_healing_uses_max_hp_and_protocol_target_scope() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_tutorial_rules().unwrap();
    let fresh = load_fresh_rules().unwrap();
    let opened = reduce_talk_event(
        &proto,
        &fresh,
        &rules,
        starter_resources(&proto, &fresh).unwrap(),
        101001001,
        1,
    )
    .unwrap();
    let start = reduce_battle_start(&proto, &rules, opened.resources, 101001002, 1).unwrap();
    let mut state = start.state;
    let mut runtime = start.effects;
    let original = message_list(&state, "members");
    let source = original
        .iter()
        .find(|member| member_type(member).ok() == Some(0))
        .and_then(|member| member_id(member).ok())
        .unwrap();
    let enemy = original
        .iter()
        .find(|member| member_type(member).ok() == Some(1))
        .and_then(|member| member_id(member).ok())
        .unwrap();
    let ally_count = original
        .iter()
        .filter(|member| member_type(member).ok() == Some(0))
        .count();
    let mut members = original;
    for member in members
        .iter_mut()
        .filter(|member| member_type(member).ok() == Some(0))
    {
        let maximum = i32_field(member, "max_hp").unwrap();
        member.set_field_by_name("hp", Value::I32(maximum / 2));
    }
    state.set_field_by_name(
        "members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );

    let results = runtime
        .apply(
            &proto,
            &mut state,
            source,
            &[TutorialSkillEffect {
                id: 91000925,
                value: 1_000,
            }],
            &[enemy],
            true,
            "after",
            None,
        )
        .unwrap();
    assert_eq!(results.len(), ally_count);
    let healed_members = message_list(&state, "members");
    for result in &results {
        let target = optional_i32_field(result, "effect_target_id").unwrap();
        let member = healed_members
            .iter()
            .find(|member| member_id(member).ok() == Some(target))
            .unwrap();
        let maximum = i32_field(member, "max_hp").unwrap();
        let expected = maximum / 10;
        assert_eq!(
            message_i32_field(result, "hp_heal", "value"),
            Some(expected)
        );
        assert_eq!(i32_field(member, "hp"), Some(maximum / 2 + expected));
    }

    let target = source;
    let mut members = healed_members;
    members
        .iter_mut()
        .find(|member| member_id(member).ok() == Some(target))
        .unwrap()
        .set_field_by_name("hp", Value::I32(0));
    state.set_field_by_name(
        "members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );
    let result = runtime
        .apply(
            &proto,
            &mut state,
            source,
            &[TutorialSkillEffect {
                id: 91001163,
                value: 2_000,
            }],
            &[target],
            true,
            "after",
            None,
        )
        .unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(
        optional_i32_field(&result[0], "effect_target_id"),
        Some(target)
    );
}

#[test]
fn triggered_ability_resources_bind_by_catalog_slot() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_tutorial_rules().unwrap();
    let fresh = load_fresh_rules().unwrap();
    let opened = reduce_talk_event(
        &proto,
        &fresh,
        &rules,
        starter_resources(&proto, &fresh).unwrap(),
        101001001,
        1,
    )
    .unwrap();
    let start = reduce_battle_start(&proto, &rules, opened.resources, 101001002, 1).unwrap();
    let mut state = start.state;
    let source_member = message_list(&state, "members")
        .into_iter()
        .find(|member| member_type(member).ok() == Some(0))
        .unwrap();
    let source = member_id(&source_member).unwrap();
    let character_id = message_i32_field(&source_member, "ally", "character_id").unwrap();
    let burst_before = i32_field(
        &member_status(&source_member, "burst_gauge").unwrap(),
        "current_gauge",
    )
    .unwrap();
    let party = [BattlePartyMember {
        character_id,
        level: 1,
        rarity: 3,
        memoria_id: None,
        position: 1,
        is_leader: true,
        integrated_stats: None,
        damage_bonus: 0,
        skills: Vec::new(),
        ability_ids: Vec::new(),
        passives: Vec::new(),
        leader_passives: Vec::new(),
    }];
    let attacker_character_id = registry()
        .unwrap()
        .rules
        .iter()
        .find(|rule| rule.id == 95000088 && rule.owner_id == 4990388)
        .unwrap()
        .source_character_ids[0];
    let external = [
        (
            None,
            1990531,
            Some(1),
            TutorialSkillEffect {
                id: 72001131,
                value: 1_000,
            },
        ),
        (
            None,
            1990531,
            Some(4),
            TutorialSkillEffect {
                id: 72001131,
                value: 10_000,
            },
        ),
        (
            None,
            1990322,
            Some(1),
            TutorialSkillEffect {
                id: 72001131,
                value: 500,
            },
        ),
        (
            Some(attacker_character_id),
            4990388,
            Some(0),
            TutorialSkillEffect {
                id: 95000088,
                value: 100,
            },
        ),
        (
            Some(attacker_character_id),
            4990388,
            Some(1),
            TutorialSkillEffect {
                id: 95000088,
                value: 100,
            },
        ),
        (
            None,
            4991248,
            Some(2),
            TutorialSkillEffect {
                id: 95000351,
                value: 1_000,
            },
        ),
    ];
    let lower_hp = |state: &mut DynamicMessage| {
        let mut members = message_list(state, "members");
        for member in members
            .iter_mut()
            .filter(|member| member_type(member).ok() == Some(0))
        {
            member.set_field_by_name("hp", Value::I32(i32_field(member, "max_hp").unwrap() / 2));
        }
        state.set_field_by_name(
            "members",
            Value::List(members.into_iter().map(Value::Message).collect()),
        );
    };

    lower_hp(&mut state);
    let mut runtime = Runtime::initialize(
        &proto,
        &rules,
        &mut state,
        "triggered-heal",
        &party,
        &external,
    )
    .unwrap();
    assert!(message_list(&state, "members")
        .iter()
        .filter(|member| member_type(member).ok() == Some(0))
        .all(|member| i32_field(member, "hp") == i32_field(member, "max_hp")));
    let source_member = message_list(&state, "members")
        .into_iter()
        .find(|member| member_id(member).ok() == Some(source))
        .unwrap();
    assert_eq!(
        i32_field(
            &member_status(&source_member, "burst_gauge").unwrap(),
            "current_gauge",
        ),
        Some(burst_before + 10)
    );

    lower_hp(&mut state);
    let skill = rules
        .skills
        .iter()
        .find(|skill| skill.skill_effect_type == 1)
        .unwrap();
    let results = runtime
        .trigger_attack_after(&proto, &rules, &mut state, source, skill, &[])
        .unwrap();
    let allies = message_list(&state, "members")
        .into_iter()
        .filter(|member| member_type(member).ok() == Some(0))
        .collect::<Vec<_>>();
    assert_eq!(results.len(), allies.len() + 2);
    assert!(allies.iter().all(|member| {
        let maximum = i32_field(member, "max_hp").unwrap();
        let attack_heal = if member_id(member).unwrap() == source {
            (maximum / 100) * 2
        } else {
            0
        };
        i32_field(member, "hp") == Some(maximum / 2 + maximum / 10 + attack_heal)
    }));

    lower_hp(&mut state);
    let results = runtime
        .trigger_party_tool_effects(&proto, &mut state)
        .unwrap();
    let allies = message_list(&state, "members")
        .into_iter()
        .filter(|member| member_type(member).ok() == Some(0))
        .collect::<Vec<_>>();
    assert_eq!(results.len(), allies.len());
    assert!(allies.iter().all(|member| {
        let maximum = i32_field(member, "max_hp").unwrap();
        i32_field(member, "hp") == Some(maximum / 2 + maximum / 20)
    }));
}

#[test]
fn maintenance_effects_cleanse_regenerate_and_restore_gauge() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_tutorial_rules().unwrap();
    let fresh = load_fresh_rules().unwrap();
    let opened = reduce_talk_event(
        &proto,
        &fresh,
        &rules,
        starter_resources(&proto, &fresh).unwrap(),
        101001001,
        1,
    )
    .unwrap();
    let start = reduce_battle_start(&proto, &rules, opened.resources, 101001002, 1).unwrap();
    let transaction = start.start_txid.clone();
    let mut state = start.state;
    let mut runtime = start.effects;
    let source = message_list(&state, "members")
        .into_iter()
        .find(|member| member_type(member).ok() == Some(0))
        .and_then(|member| member_id(&member).ok())
        .unwrap();
    let enemy = message_list(&state, "members")
        .into_iter()
        .find(|member| member_type(member).ok() == Some(1))
        .and_then(|member| member_id(&member).ok())
        .unwrap();
    let mut members = message_list(&state, "members");
    let source_member = members
        .iter_mut()
        .find(|member| member_id(member).ok() == Some(source))
        .unwrap();
    source_member.set_field_by_name(
        "state_changes",
        Value::List(vec![
            Value::Message(display(&proto, 920015, 3_000, 1).unwrap()),
            Value::Message(display(&proto, 910039, 1_000, 1).unwrap()),
            Value::Message(display(&proto, 940007, 200, 5).unwrap()),
        ]),
    );
    state.set_field_by_name(
        "members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );

    let cleanse = runtime
        .apply(
            &proto,
            &mut state,
            source,
            &[TutorialSkillEffect {
                id: 91001007,
                value: 10_000,
            }],
            &[enemy],
            true,
            "before",
            None,
        )
        .unwrap();
    assert_eq!(cleanse.len(), 1);
    assert_eq!(
        message_list(&cleanse[0], "removed_state_changes")
            .into_iter()
            .map(|change| i32_field(&change, "state_change_id").unwrap())
            .collect::<Vec<_>>(),
        [920015]
    );
    let source_member = message_list(&state, "members")
        .into_iter()
        .find(|member| member_id(member).ok() == Some(source))
        .unwrap();
    assert!(message_list(&source_member, "state_changes")
        .iter()
        .all(|change| i32_field(change, "state_change_id") != Some(920015)));
    assert!(message_list(&source_member, "state_changes")
        .iter()
        .any(|change| i32_field(change, "state_change_id") == Some(910039)));
    assert!(message_list(&source_member, "state_changes")
        .iter()
        .any(|change| i32_field(change, "state_change_id") == Some(940007)));

    let cleanse_abnormal = runtime
        .apply(
            &proto,
            &mut state,
            source,
            &[TutorialSkillEffect {
                id: 91001107,
                value: 10_000,
            }],
            &[enemy],
            true,
            "before",
            None,
        )
        .unwrap();
    assert_eq!(cleanse_abnormal.len(), 1);
    assert_eq!(
        message_list(&cleanse_abnormal[0], "removed_state_changes")
            .into_iter()
            .map(|change| i32_field(&change, "state_change_id").unwrap())
            .collect::<Vec<_>>(),
        [940007]
    );

    state.set_field_by_name("party_gauge", Value::I32(0));
    let applied = runtime
        .apply(
            &proto,
            &mut state,
            source,
            &[
                TutorialSkillEffect {
                    id: 91002004,
                    value: 1_000,
                },
                TutorialSkillEffect {
                    id: 91001534,
                    value: 500,
                },
            ],
            &[enemy],
            true,
            "after",
            None,
        )
        .unwrap();
    assert_eq!(i32_field(&state, "party_gauge"), Some(100));
    assert!(applied.iter().any(|result| {
        i32_field(result, "effect_id") == Some(91002004)
            && message_i32_field(result, "party_gauge_heal", "value") == Some(100)
    }));
    let mut members = message_list(&state, "members");

    let source_member = members
        .iter_mut()
        .find(|member| member_id(member).ok() == Some(source))
        .unwrap();
    let maximum = i32_field(source_member, "max_hp").unwrap();
    source_member.set_field_by_name("hp", Value::I32(maximum / 2));
    state.set_field_by_name(
        "members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );
    let (turn_results, _, _) = runtime
        .prepare_turn(
            &proto,
            &mut state,
            source,
            b"maintenance-test",
            &transaction,
            1,
        )
        .unwrap();
    assert!(turn_results.iter().any(|result| {
        i32_field(result, "state_change_id") == Some(910037)
            && message_i32_field(result, "hp_heal", "value") == Some(maximum * 500 / 10_000)
    }));
    let source_member = message_list(&state, "members")
        .into_iter()
        .find(|member| member_id(member).ok() == Some(source))
        .unwrap();
    assert!(message_list(&source_member, "state_changes")
        .iter()
        .all(|change| i32_field(change, "state_change_id") != Some(910037)));
}

#[test]
fn direct_gauge_effects_update_state_and_protocol_results() {
    let inferred_break_heal = rule_for(780022018, "active", "skill", 20007317)
        .unwrap()
        .unwrap();
    assert_eq!(
        (
            inferred_break_heal.operation.as_str(),
            inferred_break_heal.target.as_str(),
            inferred_break_heal.sign,
        ),
        ("break_gauge", "self", 1)
    );
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_tutorial_rules().unwrap();
    let fresh = load_fresh_rules().unwrap();
    let opened = reduce_talk_event(
        &proto,
        &fresh,
        &rules,
        starter_resources(&proto, &fresh).unwrap(),
        101001001,
        1,
    )
    .unwrap();
    let start = reduce_battle_start(&proto, &rules, opened.resources, 101001002, 1).unwrap();
    let transaction = start.start_txid;
    let mut state = start.state;
    let mut runtime = start.effects;
    let mut members = message_list(&state, "members");
    let source = members
        .iter()
        .find(|member| member_type(member).ok() == Some(0))
        .and_then(|member| member_id(member).ok())
        .unwrap();
    let enemy = members
        .iter()
        .find(|member| member_type(member).ok() == Some(1))
        .and_then(|member| member_id(member).ok())
        .unwrap();
    let source_member = members
        .iter_mut()
        .find(|member| member_id(member).ok() == Some(source))
        .unwrap();
    let mut burst = member_status(source_member, "burst_gauge").unwrap();
    burst.set_field_by_name("current_gauge", Value::I32(100));
    burst.set_field_by_name("is_enable", Value::Bool(true));
    source_member.set_field_by_name("burst_gauge", Value::Message(burst));
    let enemy_member = members
        .iter_mut()
        .find(|member| member_id(member).ok() == Some(enemy))
        .unwrap();
    let mut enemy_status = member_status(enemy_member, "enemy").unwrap();
    let max_break = i32_field(&enemy_status, "max_break_gauge").unwrap();
    enemy_status.set_field_by_name("break_gauge", Value::I32(max_break / 2));
    enemy_member.set_field_by_name("enemy", Value::Message(enemy_status));
    state.set_field_by_name(
        "members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );

    let burst_result = runtime
        .apply_for_action_with_rules(
            &proto,
            &rules,
            &mut state,
            enemy,
            32005428,
            &[TutorialSkillEffect {
                id: 76278002,
                value: 2_000,
            }],
            &[source],
            true,
            "after",
            None,
            10_000,
            b"gauge-test",
            &transaction,
            1,
        )
        .unwrap();
    let source_member = message_list(&state, "members")
        .into_iter()
        .find(|member| member_id(member).ok() == Some(source))
        .unwrap();
    let burst = member_status(&source_member, "burst_gauge").unwrap();
    assert_eq!(i32_field(&burst, "current_gauge"), Some(80));
    assert!(!bool_field(&burst, "is_enable"));
    assert_eq!(
        message_i32_field(&burst_result[0], "add_burst_gauge", "value"),
        Some(-20)
    );

    let break_result = runtime
        .apply_for_action_with_rules(
            &proto,
            &rules,
            &mut state,
            enemy,
            22000641,
            &[TutorialSkillEffect {
                id: 71179009,
                value: 5_000,
            }],
            &[enemy],
            true,
            "after",
            None,
            10_000,
            b"gauge-test",
            &transaction,
            2,
        )
        .unwrap();
    let enemy_member = message_list(&state, "members")
        .into_iter()
        .find(|member| member_id(member).ok() == Some(enemy))
        .unwrap();
    let enemy_status = member_status(&enemy_member, "enemy").unwrap();
    assert_eq!(i32_field(&enemy_status, "break_gauge"), Some(max_break));
    assert_eq!(
        message_i32_field(&break_result[0], "break_gauge_heal", "value"),
        Some(max_break - max_break / 2)
    );

    let source_member = message_list(&state, "members")
        .into_iter()
        .find(|member| member_id(member).ok() == Some(source))
        .unwrap();
    let character_id = message_i32_field(&source_member, "ally", "character_id").unwrap();
    let party = [BattlePartyMember {
        character_id,
        level: 1,
        rarity: 3,
        memoria_id: None,
        position: 1,
        is_leader: true,
        integrated_stats: None,
        damage_bonus: 0,
        skills: Vec::new(),
        ability_ids: Vec::new(),
        passives: Vec::new(),
        leader_passives: Vec::new(),
    }];
    let external = [
        (
            None,
            26267010,
            None,
            TutorialSkillEffect {
                id: 2000392,
                value: 10_000,
            },
        ),
        (
            None,
            26267011,
            None,
            TutorialSkillEffect {
                id: 2000392,
                value: 4_000,
            },
        ),
    ];
    let mut trigger_runtime = Runtime::initialize(
        &proto,
        &rules,
        &mut state,
        "gauge-trigger",
        &party,
        &external,
    )
    .unwrap();
    assert_eq!(
        i32_field(&state, "bomb_gauge"),
        Some(rules.constants.max_bomb_gauge)
    );
    state.set_field_by_name("bomb_gauge", Value::I32(0));
    let skill = rules
        .skills
        .iter()
        .find(|skill| skill.skill_effect_type == 1)
        .unwrap();
    let triggered = trigger_runtime
        .trigger_attack_after(&proto, &rules, &mut state, source, skill, &[])
        .unwrap();
    let expected = rules.constants.max_bomb_gauge * 4 / 10;
    assert_eq!(i32_field(&state, "bomb_gauge"), Some(expected));
    assert!(triggered.iter().any(|result| {
        i32_field(result, "effect_id") == Some(2000392)
            && message_i32_field(result, "add_bomb_gauge", "value") == Some(expected)
    }));
}

#[test]
fn skill_form_effect_replaces_the_selected_skill() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_gameplay_rules().unwrap();
    let fresh = load_fresh_rules().unwrap();
    let opened = reduce_talk_event(
        &proto,
        &fresh,
        &rules,
        starter_resources(&proto, &fresh).unwrap(),
        101001001,
        1,
    )
    .unwrap();
    let start = reduce_battle_start(&proto, &rules, opened.resources, 101001002, 1).unwrap();
    let transaction = start.start_txid;
    let mut state = start.state;
    let mut runtime = start.effects;
    let mut members = message_list(&state, "members");
    let source = members
        .iter()
        .find(|member| member_type(member).ok() == Some(0))
        .and_then(|member| member_id(member).ok())
        .unwrap();
    let member = members
        .iter_mut()
        .find(|member| member_id(member).ok() == Some(source))
        .unwrap();
    let mut ally = member_status(member, "ally").unwrap();
    let mut skills = message_list(&ally, "skills");
    skills[0].set_field_by_name("skill_id", Value::I32(11002386));
    ally.set_field_by_name(
        "skills",
        Value::List(skills.into_iter().map(Value::Message).collect()),
    );
    member.set_field_by_name("ally", Value::Message(ally));
    state.set_field_by_name(
        "members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );

    let results = runtime
        .apply_for_action_with_rules(
            &proto,
            &rules,
            &mut state,
            source,
            11002386,
            &[TutorialSkillEffect {
                id: 91001607,
                value: 0,
            }],
            &[],
            true,
            "after",
            None,
            10_000,
            b"skill-form-test",
            &transaction,
            1,
        )
        .unwrap();
    assert_eq!(i32_field(&results[0], "effect_id"), Some(91001607));
    let member = message_list(&state, "members")
        .into_iter()
        .find(|member| member_id(member).ok() == Some(source))
        .unwrap();
    assert!(
        message_list(&member_status(&member, "ally").unwrap(), "skills")
            .iter()
            .any(|skill| i32_field(skill, "skill_id") == Some(11002541))
    );
}

#[test]
fn immunity_states_block_only_their_effect_category() {
    for (effect_id, target, duration) in [
        (91001022, "self", 1),
        (91001392, "allies", 1),
        (91001518, "targets", 2),
    ] {
        let rule = rule_for(effect_id, "active", "skill", 0)
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                rule.operation.as_str(),
                rule.target.as_str(),
                rule.state_id,
                &rule.expiry,
                rule.duration,
            ),
            (
                "negative_immunity",
                target,
                910002,
                &Expiry::Turn,
                duration,
            )
        );
    }
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_tutorial_rules().unwrap();
    let fresh = load_fresh_rules().unwrap();
    let opened = reduce_talk_event(
        &proto,
        &fresh,
        &rules,
        starter_resources(&proto, &fresh).unwrap(),
        101001001,
        1,
    )
    .unwrap();
    let start = reduce_battle_start(&proto, &rules, opened.resources, 101001002, 1).unwrap();
    let mut state = start.state;
    let mut runtime = start.effects;
    let source = message_list(&state, "members")
        .into_iter()
        .find(|member| member_type(member).ok() == Some(0))
        .and_then(|member| member_id(&member).ok())
        .unwrap();
    let enemy = message_list(&state, "members")
        .into_iter()
        .find(|member| member_type(member).ok() == Some(1))
        .and_then(|member| member_id(&member).ok())
        .unwrap();

    runtime
        .apply(
            &proto,
            &mut state,
            source,
            &[TutorialSkillEffect {
                id: 780139027,
                value: 10_000,
            }],
            &[enemy],
            true,
            "after",
            None,
        )
        .unwrap();
    let blocked = runtime
        .apply(
            &proto,
            &mut state,
            source,
            &[TutorialSkillEffect {
                id: 3000047,
                value: 3_000,
            }],
            &[enemy],
            true,
            "after",
            None,
        )
        .unwrap();
    assert_eq!(blocked.len(), 1);
    assert_eq!(
        i32_or_enum_field(&blocked[0], "deal_state_change_result"),
        Some(4)
    );
    let poison = runtime
        .apply(
            &proto,
            &mut state,
            source,
            &[TutorialSkillEffect {
                id: 780010025,
                value: 200,
            }],
            &[enemy],
            true,
            "after",
            None,
        )
        .unwrap();
    assert_eq!(
        i32_or_enum_field(&poison[0], "deal_state_change_result"),
        Some(1)
    );

    runtime
        .apply(
            &proto,
            &mut state,
            source,
            &[TutorialSkillEffect {
                id: 71140006,
                value: 10_000,
            }],
            &[enemy],
            true,
            "after",
            None,
        )
        .unwrap();
    let blocked = runtime
        .apply(
            &proto,
            &mut state,
            enemy,
            &[TutorialSkillEffect {
                id: 780010025,
                value: 200,
            }],
            &[source],
            true,
            "after",
            None,
        )
        .unwrap();
    assert_eq!(blocked.len(), 1);
    assert_eq!(
        i32_or_enum_field(&blocked[0], "deal_state_change_result"),
        Some(4)
    );
    assert!(registry()
        .unwrap()
        .rules
        .iter()
        .any(|rule| rule.id == 72000987 && rule.mode == "passive"));
}
