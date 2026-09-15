use super::super::*;
use crate::state::combat::prelude::*;
use std::path::Path;

#[test]
fn legacy_skill_modifiers_are_active_and_apply_to_their_real_recipients() {
    let expected = [
        (
            780110003,
            "summary",
            "targets",
            1,
            1,
            50001,
            Expiry::Turn,
            1,
        ),
        (
            780110007,
            "taken_down",
            "targets",
            0,
            1,
            560074,
            Expiry::Hit,
            1,
        ),
        (
            780107002,
            "summary",
            "targets",
            3,
            -1,
            1220041,
            Expiry::Turn,
            2,
        ),
        (
            780107012,
            "taken_down",
            "self",
            0,
            1,
            560074,
            Expiry::Hit,
            3,
        ),
        (780109008, "summary", "self", 13, -1, 910001, Expiry::Hit, 1),
        (780091007, "heal", "allies", 0, 1, 0, Expiry::Permanent, -1),
        (
            780110004,
            "regeneration",
            "targets",
            0,
            1,
            910037,
            Expiry::Turn,
            2,
        ),
        (
            780014005,
            "summary",
            "targets",
            1,
            1,
            50001,
            Expiry::Turn,
            2,
        ),
        (
            780014007,
            "taken_down",
            "targets",
            0,
            1,
            560074,
            Expiry::Hit,
            2,
        ),
        (
            780035005,
            "speed",
            "targets",
            0,
            -1,
            800199,
            Expiry::Turn,
            2,
        ),
        (
            780107007,
            "magic",
            "targets",
            0,
            -1,
            920033,
            Expiry::Turn,
            2,
        ),
        (
            780107008,
            "attack",
            "targets",
            0,
            -1,
            920034,
            Expiry::Turn,
            2,
        ),
        (
            780107009,
            "speed",
            "targets",
            0,
            -1,
            800199,
            Expiry::Turn,
            2,
        ),
        (
            780100002,
            "regeneration",
            "allies",
            0,
            1,
            910037,
            Expiry::Turn,
            3,
        ),
        (91002036, "magic", "self", 0, 1, 50003, Expiry::Turn, 3),
    ];
    for (id, operation, target, summary, sign, state_id, expiry, duration) in expected {
        let rule = rule_for(id, "active", "", 0).unwrap().unwrap();
        assert_eq!(
            (
                rule.mode.as_str(),
                rule.operation.as_str(),
                rule.target.as_str(),
                rule.summary,
                rule.sign,
                rule.state_id,
                &rule.expiry,
                rule.duration,
            ),
            ("active", operation, target, summary, sign, state_id, &expiry, duration),
            "effect {id}",
        );
    }

    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let gameplay = load_gameplay_rules().unwrap();
    let fresh = load_fresh_rules().unwrap();
    let resources = reduce_talk_event(
        &proto,
        &fresh,
        &gameplay,
        starter_resources(&proto, &fresh).unwrap(),
        101001001,
        1,
    )
    .unwrap()
    .resources;
    let start = reduce_battle_start(&proto, &gameplay, resources, 101001002, 1).unwrap();
    let mut state = start.state;
    let mut runtime = start.effects;
    runtime.passives.clear();
    runtime.instances.clear();
    runtime.managed.clear();
    let mut members = message_list(&state, "members");
    let source_id = members
        .iter()
        .find(|member| member_type(member).ok() == Some(0))
        .and_then(|member| member_id(member).ok())
        .unwrap();
    let enemy_id = members
        .iter()
        .find(|member| member_type(member).ok() == Some(1))
        .and_then(|member| member_id(member).ok())
        .unwrap();
    let source = members
        .iter_mut()
        .find(|member| member_id(member).ok() == Some(source_id))
        .unwrap();
    let maximum_hp = i32_field(source, "max_hp").unwrap();
    let starting_hp = maximum_hp / 2;
    source.set_field_by_name("hp", Value::I32(starting_hp));
    state.set_field_by_name(
        "members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );
    runtime.refresh(&proto, &mut state).unwrap();

    let mut skill = gameplay
        .skills
        .iter()
        .find(|skill| skill.skill_effect_type == 1 && skill.break_power > 0)
        .unwrap()
        .clone();
    skill.effects.clear();
    skill.break_power = 10_000;
    skill.attack_attributes = vec![1];
    let members = message_list(&state, "members");
    let source = members
        .iter()
        .find(|member| member_id(member).ok() == Some(source_id))
        .unwrap();
    let enemy = members
        .iter()
        .find(|member| member_id(member).ok() == Some(enemy_id))
        .unwrap();
    let baseline =
        policy_break_damage(enemy, source, &skill, Some(&runtime), 100, false, 10_000).unwrap();

    runtime
        .apply(
            &proto,
            &mut state,
            source_id,
            &[TutorialSkillEffect {
                id: 780107002,
                value: 2_000,
            }],
            &[enemy_id],
            true,
            "after",
            None,
        )
        .unwrap();
    let members = message_list(&state, "members");
    let source = members
        .iter()
        .find(|member| member_id(member).ok() == Some(source_id))
        .unwrap();
    let enemy = members
        .iter()
        .find(|member| member_id(member).ok() == Some(enemy_id))
        .unwrap();
    assert_eq!(state_change_summary_value(enemy, 3), -2_000);
    assert!(
        policy_break_damage(enemy, source, &skill, Some(&runtime), 100, false, 10_000).unwrap()
            < baseline
    );

    runtime
        .apply(
            &proto,
            &mut state,
            source_id,
            &[TutorialSkillEffect {
                id: 780091007,
                value: 1_500,
            }],
            &[enemy_id],
            true,
            "after",
            None,
        )
        .unwrap();
    let source = message_list(&state, "members")
        .into_iter()
        .find(|member| member_id(member).ok() == Some(source_id))
        .unwrap();
    assert_eq!(
        i32_field(&source, "hp"),
        Some(starting_hp + maximum_hp * 1_500 / 10_000)
    );
}
