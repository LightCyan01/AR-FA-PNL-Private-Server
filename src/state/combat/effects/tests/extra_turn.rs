use super::super::*;
use crate::state::combat::prelude::*;
use std::path::Path;

fn proto() -> ProtoRegistry {
    ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap()
}

#[test]
fn generated_extra_turn_survives_resume_and_replaces_only_itself() {
    let proto = proto();
    let rules = load_gameplay_rules().unwrap();
    let skill = rule_skill(&rules, 14000998).unwrap();
    let linked = rule_skill(&rules, 14001047).unwrap();
    assert_eq!(linked.skill_type, 0);

    let ally = build_ally_member(
        &proto,
        &rules,
        &BattlePartyMember {
            character_id: 43203,
            level: 1,
            rarity: 3,
            memoria_id: None,
            position: 1,
            is_leader: true,
            integrated_stats: None,
            damage_bonus: 0,
            skills: vec![TutorialCharacterSkill {
                id: skill.id,
                skill_type: 3,
                rank_ids: Vec::new(),
                evolved_ids: Vec::new(),
            }],
            ability_ids: Vec::new(),
            passives: Vec::new(),
            leader_passives: Vec::new(),
        },
        1,
        0,
        0,
    )
    .unwrap();
    let wave = rules
        .waves
        .iter()
        .find(|wave| !wave.enemies.is_empty())
        .unwrap();
    let enemy = build_enemy_member(&proto, &rules, &wave.enemies[0], wave.id, 11, 1).unwrap();
    let mut state = empty_message(&proto, "blend.model.BattleState").unwrap();
    state.set_field_by_name(
        "members",
        Value::List(vec![Value::Message(ally), Value::Message(enemy)]),
    );
    let members = message_list(&state, "members");
    let mut units = [(1, 1, 0), (11, 1, 50), (1, 2, 100), (11, 2, 150)]
        .into_iter()
        .map(|(id, number, wait)| build_timeline_unit(&proto, id, number, wait).unwrap())
        .collect::<Vec<_>>();
    consume_timeline_turn(&proto, &mut units, &members, 1, 200, 200, false).unwrap();
    state.set_field_by_name(
        "timeline_units",
        Value::List(units.into_iter().map(Value::Message).collect()),
    );

    let mut runtime = Runtime::default();
    let results = runtime
        .apply_for_action_with_rules(
            &proto,
            &rules,
            &mut state,
            1,
            skill.id,
            &skill.effects,
            &[11],
            true,
            "after",
            None,
            10_000,
            b"extra-turn",
            "extra-turn",
            1,
        )
        .unwrap();
    assert!(results
        .iter()
        .any(|result| i32_field(result, "effect_id") == Some(91001175)));
    let moves = runtime
        .apply_pending_extra_turns(&proto, &mut state)
        .unwrap();
    assert_eq!(moves.len(), 1);
    assert_eq!(i32_or_enum_field(&moves[0], "reason"), Some(6));
    assert_eq!(optional_i32_field(&moves[0], "to_index"), Some(2));

    let mut runtime: Runtime =
        serde_json::from_str(&serde_json::to_string(&runtime).unwrap()).unwrap();
    let mut units = message_list(&state, "timeline_units");
    for unit in &mut units {
        let wait = if i32_or_enum_field(unit, "type") == Some(1) {
            0
        } else {
            100
        };
        unit.set_field_by_name("wait", Value::I32(wait));
    }
    state.set_field_by_name(
        "timeline_units",
        Value::List(units.into_iter().map(Value::Message).collect()),
    );
    let selections =
        build_skill_selections(&proto, &rules, &state, 1, None, Some(&runtime)).unwrap();
    assert_eq!(selections.len(), 1);
    assert_eq!(i32_field(&selections[0], "skill_type"), Some(0));
    assert_eq!(i32_field(&selections[0], "skill_id"), Some(14001047));
    assert_eq!(
        runtime.take_current_extra_skill(&state).unwrap(),
        Some(14001047)
    );

    let members = message_list(&state, "members");
    let mut units = message_list(&state, "timeline_units");
    let movement = consume_timeline_turn(&proto, &mut units, &members, 1, 200, 200, false).unwrap();
    assert_eq!(i32_or_enum_field(&movement, "reason"), Some(1));
    assert_eq!(
        units
            .iter()
            .filter(|unit| member_id(unit).ok() == Some(1))
            .count(),
        2
    );
    assert!(units
        .iter()
        .all(|unit| i32_or_enum_field(unit, "type") == Some(0)));
}

#[test]
fn extra_turn_rules_preserve_conditions_and_activation_limits() {
    let gameplay = load_gameplay_rules().unwrap();
    let rules = registry()
        .unwrap()
        .rules
        .iter()
        .filter(|rule| rule.operation == "extra_turn")
        .collect::<Vec<_>>();
    assert_eq!(rules.len(), 32);
    assert_eq!(
        rules
            .iter()
            .filter(|rule| rule.critical_only && rule.trigger_limit == 5)
            .count(),
        5
    );
    assert_eq!(
        rules
            .iter()
            .filter(|rule| rule.source_state_ids.as_slice() == [510319]
                && rule.source_state_level_min == 10)
            .count(),
        5
    );
    assert!(rules.iter().all(|rule| {
        rule.fixed
            .and_then(|skill_id| rule_skill(&gameplay, skill_id).ok())
            .is_some_and(|skill| skill.skill_type == 0)
    }));

    let critical = rules.iter().find(|rule| rule.trigger_limit == 5).unwrap();
    let mut runtime = Runtime::default();
    for _ in 0..5 {
        assert!(runtime
            .queue_extra_turn(1, critical.owner_id, critical)
            .unwrap());
    }
    assert!(!runtime
        .queue_extra_turn(1, critical.owner_id, critical)
        .unwrap());

    let conditional = rules
        .iter()
        .find(|rule| rule.source_state_level_min == 10)
        .unwrap();
    let proto = proto();
    let mut source = empty_message(&proto, "blend.model.BattleMember").unwrap();
    source.set_field_by_name("hp", Value::I32(1));
    source.set_field_by_name("max_hp", Value::I32(1));
    source.set_field_by_name(
        "state_changes",
        Value::List(vec![Value::Message(
            super::super::runtime_results::level_display(&proto, 510319, 9, -1).unwrap(),
        )]),
    );
    assert!(!super::super::runtime_match::condition(conditional, &source));
    source.set_field_by_name(
        "state_changes",
        Value::List(vec![Value::Message(
            super::super::runtime_results::level_display(&proto, 510319, 10, -1).unwrap(),
        )]),
    );
    assert!(super::super::runtime_match::condition(conditional, &source));
}
