use crate::state::combat::prelude::*;
use std::path::Path;

#[test]
fn weak_break_zero_uses_the_normal_break_path() {
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
    let skill = TutorialSkill {
        id: 14001764,
        skill_type: 3,
        skill_effect_type: 1,
        skill_power_type: 2,
        wait: 0,
        power: 360,
        break_power: 300,
        break_power_type: 2,
        attack_attributes: vec![5],
        skill_target_type: Some(3),
        effects: vec![TutorialSkillEffect {
            id: 91001187,
            value: 0,
        }],
        limit_count: None,
        max_lamp: 0,
        require_command_value: false,
        skill_destination: None,
        state_change_application_rate: 10_000,
        hp_damage_bonus: None,
    };
    let actor = message_list(&start.state, "members")
        .iter()
        .find(|member| member_type(member).ok() == Some(0))
        .and_then(|member| member_id(member).ok())
        .unwrap();
    let enemy = message_list(&start.state, "members")
        .iter()
        .find(|member| member_type(member).ok() == Some(1))
        .and_then(|member| member_id(member).ok())
        .unwrap();

    let run = |weak: bool| {
        let mut state = start.state.clone();
        let mut members = message_list(&state, "members");
        let target = members
            .iter_mut()
            .find(|member| member_id(member).ok() == Some(enemy))
            .unwrap();
        let mut resistance = member_status(target, "resistance").unwrap();
        let attribute = skill.attack_attributes[0];
        resistance.set_field_by_name(
            resistance_name(attribute).unwrap(),
            Value::I32(if weak { -1 } else { 0 }),
        );
        target.set_field_by_name("resistance", Value::Message(resistance));
        let mut enemy_status = member_status(target, "enemy").unwrap();
        enemy_status.set_field_by_name("max_break_gauge", Value::I32(10_000_000));
        enemy_status.set_field_by_name("break_gauge", Value::I32(10_000_000));
        target.set_field_by_name("enemy", Value::Message(enemy_status));
        state.set_field_by_name(
            "members",
            Value::List(members.into_iter().map(Value::Message).collect()),
        );
        let mut runtime = start.effects.clone();
        let (results, movements, _) = apply_attack_results(
            &proto,
            &rules,
            &mut state,
            actor,
            0,
            &skill,
            &skill.effects,
            None,
            &runtime,
            &[enemy],
            (1, 1),
            None,
            false,
            false,
            b"weak-break-zero",
            &start.start_txid,
            1,
        )
        .unwrap();
        runtime
            .apply_for_action_with_rules(
                &proto,
                &rules,
                &mut state,
                actor,
                skill.id,
                &skill.effects,
                &[enemy],
                true,
                "after",
                None,
                skill.state_change_application_rate,
                b"weak-break-zero",
                &start.start_txid,
                1,
            )
            .unwrap();
        let target = message_list(&state, "members")
            .into_iter()
            .find(|member| member_id(member).ok() == Some(enemy))
            .unwrap();
        let enemy_status = member_status(&target, "enemy").unwrap();
        (
            i32_field(&enemy_status, "break_gauge").unwrap(),
            bool_field(&enemy_status, "is_broken"),
            results,
            movements,
            runtime,
        )
    };

    let (gauge, broken, results, movements, runtime) = run(true);
    assert_eq!(gauge, 0);
    assert!(broken);
    assert_eq!(i32_or_enum_field(&results[0], "break_type"), Some(2));
    assert!(!movements.is_empty());
    assert!(!runtime.unsupported.contains(&91001187));

    let (gauge, broken, _, _, _) = run(false);
    assert!(gauge > 0);
    assert!(!broken);
}
