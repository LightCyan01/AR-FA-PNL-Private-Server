use super::super::*;
use crate::state::combat::prelude::*;
use std::path::Path;

#[test]
fn weak_skill_modifier_applies_only_to_lowest_resistance() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let target = |fire, ice| {
        let mut resistance = empty_message(&proto, "blend.model.BattleResistance").unwrap();
        for (field, value) in [
            ("slashing", 0),
            ("impact", 0),
            ("piercing", 0),
            ("fire", fire),
            ("ice", ice),
            ("lightning", 0),
            ("wind", 0),
        ] {
            resistance.set_field_by_name(field, Value::I32(value));
        }
        let mut member = empty_message(&proto, "blend.model.BattleMember").unwrap();
        member.set_field_by_name("resistance", Value::Message(resistance));
        member
    };
    let skill = TutorialSkill {
        id: 11000106,
        skill_type: 1,
        skill_effect_type: 1,
        skill_power_type: 1,
        wait: 100,
        power: 100,
        break_power: 100,
        break_power_type: 1,
        attack_attributes: vec![5, 6],
        skill_target_type: Some(3),
        effects: vec![
            TutorialSkillEffect {
                id: 91000903,
                value: 1_500,
            },
            TutorialSkillEffect {
                id: 91000907,
                value: 2_500,
            },
            TutorialSkillEffect {
                id: 780100001,
                value: 5_000,
            },
        ],
        limit_count: None,
        max_lamp: 0,
        require_command_value: false,
        skill_destination: None,
        state_change_application_rate: 10_000,
        hp_damage_bonus: None,
    };

    assert_eq!(
        instant_summary(&skill, &target(25, -50), false, 1).unwrap(),
        1_500
    );
    assert_eq!(
        instant_summary(&skill, &target(25, 50), false, 1).unwrap(),
        0
    );
    assert_eq!(
        instant_summary(&skill, &target(25, -50), false, 3).unwrap(),
        2_500
    );
    assert_eq!(
        instant_summary(&skill, &target(25, 50), false, 3).unwrap(),
        0
    );

    let mut negative_target = target(25, 50);
    negative_target.set_field_by_name(
        "state_changes",
        Value::List(vec![Value::Message(
            display(&proto, 920008, 1_000, 1).unwrap(),
        )]),
    );
    assert_eq!(
        instant_summary(&skill, &negative_target, false, 1).unwrap(),
        5_000
    );

    let mut abnormal_target = target(25, 50);
    abnormal_target.set_field_by_name(
        "state_changes",
        Value::List(vec![Value::Message(
            display(&proto, 940007, 1_000, 1).unwrap(),
        )]),
    );
    assert_eq!(
        instant_summary(&skill, &abnormal_target, false, 1).unwrap(),
        0
    );
    let abnormal_skill = TutorialSkill {
        id: 20002360,
        effects: vec![TutorialSkillEffect {
            id: 780014001,
            value: 1_500,
        }],
        ..skill
    };
    assert_eq!(
        instant_summary(&abnormal_skill, &abnormal_target, false, 1).unwrap(),
        1_500
    );
    assert_eq!(
        instant_summary(&abnormal_skill, &negative_target, false, 1).unwrap(),
        0
    );
}

#[test]
fn battle_tool_role_buffs_select_only_attackers() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let member = |member_id, character_id| {
        let mut ally = empty_message(&proto, "blend.model.BattleAlly").unwrap();
        ally.set_field_by_name("character_id", Value::I32(character_id));
        let mut member = empty_message(&proto, "blend.model.BattleMember").unwrap();
        member.set_field_by_name("member_id", Value::I32(member_id));
        member.set_field_by_name("type", Value::EnumNumber(0));
        member.set_field_by_name("ally", Value::Message(ally));
        member
    };
    let damage_rule = registry()
        .unwrap()
        .rules
        .iter()
        .find(|rule| rule.id == 6001256)
        .unwrap();
    let attacker_id = damage_rule.target_character_ids[0];
    let source = member(1, attacker_id);
    let attacker = member(2, attacker_id);
    let non_attacker = member(3, i32::MAX);
    assert!(selected(damage_rule, &source, &attacker, &[]));
    assert!(!selected(damage_rule, &source, &non_attacker, &[]));

    for (effect_id, summary, state_id) in [
        (6001256, 1, 50001),
        (6001279, 6, 910007),
        (6001280, 7, 50002),
    ] {
        let rule = registry()
            .unwrap()
            .rules
            .iter()
            .find(|rule| rule.id == effect_id)
            .unwrap();
        assert_eq!(
            (
                rule.target.as_str(),
                rule.summary,
                rule.state_id,
                &rule.expiry,
                rule.duration,
                &rule.target_character_ids,
            ),
            (
                "allies",
                summary,
                state_id,
                &Expiry::Turn,
                2,
                &damage_rule.target_character_ids,
            )
        );
    }
}

#[test]
fn skill_two_vulnerability_affects_only_skill_two_damage() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_gameplay_rules().unwrap();
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
    let mut state = start.state;
    let mut runtime = start.effects;
    runtime.passives.clear();
    runtime.instances.clear();
    runtime.managed.clear();
    runtime.refresh(&proto, &mut state).unwrap();
    let source_id = message_list(&state, "members")
        .into_iter()
        .find(|member| member_type(member).ok() == Some(0))
        .and_then(|member| member_id(&member).ok())
        .unwrap();
    let target_id = message_list(&state, "members")
        .into_iter()
        .find(|member| member_type(member).ok() == Some(1))
        .and_then(|member| member_id(&member).ok())
        .unwrap();
    runtime
        .apply(
            &proto,
            &mut state,
            source_id,
            &[TutorialSkillEffect {
                id: 6001288,
                value: 2_000,
            }],
            &[target_id],
            false,
            "after",
            None,
        )
        .unwrap();
    let source = message_list(&state, "members")
        .into_iter()
        .find(|member| i32_field(member, "member_id") == Some(source_id))
        .unwrap();
    let target = message_list(&state, "members")
        .into_iter()
        .find(|member| i32_field(member, "member_id") == Some(target_id))
        .unwrap();
    let mut skill_one = rules
        .skills
        .iter()
        .find(|skill| skill.skill_type == 1 && skill.skill_effect_type == 1)
        .unwrap()
        .clone();
    skill_one.effects.clear();
    let mut skill_two = rules
        .skills
        .iter()
        .find(|skill| skill.skill_type == 2 && skill.skill_effect_type == 1)
        .unwrap()
        .clone();
    skill_two.effects.clear();
    let one_without =
        secondary_damage(1_000_000, &source, &target, &skill_one, None, false).unwrap();
    let one_with = secondary_damage(
        1_000_000,
        &source,
        &target,
        &skill_one,
        Some(&runtime),
        false,
    )
    .unwrap();
    let two_without =
        secondary_damage(1_000_000, &source, &target, &skill_two, None, false).unwrap();
    let two_with = secondary_damage(
        1_000_000,
        &source,
        &target,
        &skill_two,
        Some(&runtime),
        false,
    )
    .unwrap();
    assert_eq!(one_with, one_without);
    assert_eq!(two_with, two_without * 12 / 10);
}

#[test]
fn attribute_resistance_down_affects_only_matching_damage() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let member = |member_id, member_type| {
        let mut status = empty_message(&proto, "blend.model.BattleCharacterStatus").unwrap();
        for field in ["attack", "defense", "hp", "magic", "mental", "speed"] {
            status.set_field_by_name(field, Value::I32(100));
        }
        let mut member = empty_message(&proto, "blend.model.BattleMember").unwrap();
        member.set_field_by_name("member_id", Value::I32(member_id));
        member.set_field_by_name("type", Value::EnumNumber(member_type));
        member.set_field_by_name("is_alive", Value::Bool(true));
        member.set_field_by_name("current_status", Value::Message(status.clone()));
        member.set_field_by_name("initial_status", Value::Message(status));
        member.set_field_by_name("state_changes", Value::List(Vec::new()));
        member.set_field_by_name("state_change_summaries", Value::List(Vec::new()));
        member
    };
    let mut state = empty_message(&proto, "blend.model.BattleState").unwrap();
    state.set_field_by_name(
        "members",
        Value::List(vec![
            Value::Message(member(1, 0)),
            Value::Message(member(2, 1)),
        ]),
    );
    let mut runtime = Runtime::default();
    runtime.prepare(&state, "attribute-effects").unwrap();
    for (skill_id, effect_id, value, phase) in [
        (11000661, 91001050, 2_000, "after"),
        (11002817, 91000935, 1_500, "before"),
        (11002940, 91001028, 1_500, "before"),
        (12000456, 91000938, 2_000, "after"),
        (12000171, 91001014, 3_000, "after"),
        (11002791, 91001261, 1_500, "before"),
        (11002832, 91001430, 2_500, "after"),
        (11003601, 91000934, 1_500, "before"),
    ] {
        runtime
            .apply_inner(
                &proto,
                &mut state,
                1,
                skill_id,
                &[TutorialSkillEffect {
                    id: effect_id,
                    value,
                }],
                &[2],
                true,
                phase,
                None,
                10_000,
                None,
            )
            .unwrap();
    }
    let target = message_list(&state, "members")
        .into_iter()
        .find(|member| i32_field(member, "member_id") == Some(2))
        .unwrap();
    for (attribute, expected) in [
        (1, 13_000),
        (2, 15_000),
        (3, 17_000),
        (5, 15_000),
        (6, 16_000),
        (7, 13_000),
        (8, 14_500),
    ] {
        assert_eq!(
            incoming_multiplier_with_runtime(&target, attribute, None),
            expected
        );
    }
}

#[test]
fn common_active_modifiers_change_only_their_declared_buckets() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let member = |member_id, member_type| {
        let mut status = empty_message(&proto, "blend.model.BattleCharacterStatus").unwrap();
        for field in ["attack", "defense", "hp", "magic", "mental", "speed"] {
            status.set_field_by_name(field, Value::I32(100));
        }
        let mut member = empty_message(&proto, "blend.model.BattleMember").unwrap();
        member.set_field_by_name("member_id", Value::I32(member_id));
        member.set_field_by_name("type", Value::EnumNumber(member_type));
        member.set_field_by_name("is_alive", Value::Bool(true));
        member.set_field_by_name("hp", Value::I32(100));
        member.set_field_by_name("max_hp", Value::I32(100));
        member.set_field_by_name("current_status", Value::Message(status.clone()));
        member.set_field_by_name("initial_status", Value::Message(status));
        member.set_field_by_name("state_changes", Value::List(Vec::new()));
        member.set_field_by_name("state_change_summaries", Value::List(Vec::new()));
        member
    };
    let mut state = empty_message(&proto, "blend.model.BattleState").unwrap();
    state.set_field_by_name(
        "members",
        Value::List(vec![
            Value::Message(member(1, 0)),
            Value::Message(member(2, 1)),
        ]),
    );
    let mut runtime = Runtime::default();
    runtime.prepare(&state, "ordinary-effects").unwrap();
    runtime
        .apply(
            &proto,
            &mut state,
            1,
            &[
                TutorialSkillEffect {
                    id: 780109007,
                    value: 2_000,
                },
                TutorialSkillEffect {
                    id: 91001018,
                    value: 1_000,
                },
                TutorialSkillEffect {
                    id: 91001041,
                    value: 1_500,
                },
                TutorialSkillEffect {
                    id: 780107003,
                    value: 1_200,
                },
                TutorialSkillEffect {
                    id: 780123003,
                    value: 3_000,
                },
                TutorialSkillEffect {
                    id: 91001311,
                    value: 2_500,
                },
            ],
            &[2],
            true,
            "after",
            None,
        )
        .unwrap();

    let source = message_list(&state, "members")
        .into_iter()
        .find(|member| i32_field(member, "member_id") == Some(1))
        .unwrap();
    let target = message_list(&state, "members")
        .into_iter()
        .find(|member| i32_field(member, "member_id") == Some(2))
        .unwrap();
    assert_eq!(state_change_summary_value(&source, 1), 3_500);
    assert_eq!(
        message_i32_field(&source, "current_status", "attack"),
        Some(110)
    );
    assert_eq!(state_change_summary_value(&target, 2), 1_200);
    assert_eq!(state_change_summary_value(&target, 17), 2_500);
    assert_eq!(healing_amount(100, &target, &target).unwrap(), 70);
    assert_eq!(
        runtime
            .instances
            .iter()
            .find(|instance| instance.rule.id == 91001311)
            .map(|instance| instance.remaining),
        Some(2)
    );
}

#[test]
fn enemy_defense_down_reaches_hidden_damage_defense() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_tutorial_rules().unwrap();
    let (wave, wave_enemy) = rules
        .waves
        .iter()
        .find_map(|wave| {
            wave.enemies
                .iter()
                .find(|enemy| !matches!(enemy.id, 80001050 | 80001051))
                .map(|enemy| (wave, enemy))
        })
        .unwrap();
    let status = battle_status_message(
        &proto,
        BattleStats {
            hp: 100,
            speed: 100,
            attack: 100,
            magic: 100,
            defense: 100,
            mental: 100,
        },
    )
    .unwrap();
    let mut attacker = empty_message(&proto, "blend.model.BattleMember").unwrap();
    attacker.set_field_by_name("member_id", Value::I32(1));
    attacker.set_field_by_name("type", Value::EnumNumber(0));
    attacker.set_field_by_name("is_alive", Value::Bool(true));
    attacker.set_field_by_name("current_status", Value::Message(status.clone()));
    attacker.set_field_by_name("initial_status", Value::Message(status));
    let enemy = build_enemy_member(&proto, &rules, wave_enemy, wave.id, 11, 1).unwrap();
    let mut state = empty_message(&proto, "blend.model.BattleState").unwrap();
    state.set_field_by_name("wave", Value::I32(1));
    state.set_field_by_name(
        "members",
        Value::List(vec![
            Value::Message(attacker.clone()),
            Value::Message(enemy.clone()),
        ]),
    );
    let skill = TutorialSkill {
        id: 1,
        skill_type: 1,
        skill_effect_type: 1,
        skill_power_type: 1,
        wait: 100,
        power: 100,
        break_power: 0,
        break_power_type: 1,
        attack_attributes: vec![1],
        skill_target_type: Some(3),
        effects: Vec::new(),
        limit_count: None,
        max_lamp: 0,
        require_command_value: false,
        skill_destination: None,
        state_change_application_rate: 10_000,
        hp_damage_bonus: None,
    };
    let mut runtime = Runtime::default();
    runtime.prepare(&state, "enemy-defense-down").unwrap();
    runtime.refresh(&proto, &mut state).unwrap();
    let (_, base_defense, _) =
        member_offense_and_defense(&proto, &rules, &attacker, &enemy, Some(&runtime), &skill)
            .unwrap();

    runtime
        .apply(
            &proto,
            &mut state,
            1,
            &[TutorialSkillEffect {
                id: 71045007,
                value: 2_000,
            }],
            &[11],
            true,
            "after",
            None,
        )
        .unwrap();
    let enemy = message_list(&state, "members")
        .into_iter()
        .find(|member| member_id(member).ok() == Some(11))
        .unwrap();
    let (_, reduced_defense, _) =
        member_offense_and_defense(&proto, &rules, &attacker, &enemy, Some(&runtime), &skill)
            .unwrap();

    assert_eq!(reduced_defense, (base_defense * 8 / 10).max(1));
}
