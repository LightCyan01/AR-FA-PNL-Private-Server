use super::super::*;
use crate::state::combat::prelude::*;
use std::path::Path;

#[test]
fn direct_statuses_stack_tick_and_gate_the_turn() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_tutorial_rules().unwrap();
    let gameplay = load_gameplay_rules().unwrap();
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
    let target = message_list(&state, "members")
        .into_iter()
        .find(|member| member_type(member).ok() == Some(1))
        .and_then(|member| member_id(&member).ok())
        .unwrap();
    let explicit_poison_skill = gameplay
        .skills
        .iter()
        .find(|skill| skill.id == 22000471)
        .unwrap();
    assert_eq!(explicit_poison_skill.state_change_application_rate, 8_000);
    assert_eq!(
        gameplay
            .skills
            .iter()
            .find(|skill| skill.id == 22000933)
            .unwrap()
            .state_change_application_rate,
        6_000
    );
    assert_eq!(
        registry()
            .unwrap()
            .rules
            .iter()
            .find(|rule| rule.id == 780051011)
            .unwrap()
            .duration,
        5
    );
    for (effect_id, duration) in [(91000951, 2), (71203002, 3)] {
        let rule = registry()
            .unwrap()
            .rules
            .iter()
            .find(|rule| rule.id == effect_id)
            .unwrap();
        assert_eq!((rule.state_id, rule.duration), (940007, duration));
    }
    let shared_poison = rule_for(1039, "active", "skill", 20000627)
        .unwrap()
        .unwrap();
    assert_eq!((shared_poison.state_id, shared_poison.duration), (940007, 1));
    let poison_override = rule_for(1039, "active", "skill", 20008490)
        .unwrap()
        .unwrap();
    assert_eq!((poison_override.state_id, poison_override.duration), (940007, 3));
    let miss_action = (1..1_000)
        .find(|number| {
            deterministic_roll(
                b"status-test",
                &transaction,
                *number,
                b"state-change",
                target,
                0,
            ) % 10_000
                >= 8_000
        })
        .unwrap();
    let missed = runtime
        .apply_for_action(
            &proto,
            &mut state,
            source,
            explicit_poison_skill.id,
            &explicit_poison_skill.effects[..1],
            &[target],
            true,
            "after",
            None,
            explicit_poison_skill.state_change_application_rate,
            b"status-test",
            &transaction,
            miss_action,
        )
        .unwrap();
    assert_eq!(
        i32_or_enum_field(&missed[0], "deal_state_change_result"),
        Some(2)
    );
    assert!(message_list(
        &message_list(&state, "members")
            .into_iter()
            .find(|member| i32_field(member, "member_id") == Some(target))
            .unwrap(),
        "state_changes",
    )
    .into_iter()
    .all(|change| i32_field(&change, "state_change_id") != Some(940007)));
    let poison = TutorialSkillEffect {
        id: 780080004,
        value: 200,
    };
    let applied = runtime
        .apply_for_action(
            &proto,
            &mut state,
            source,
            0,
            &[poison.clone(), poison.clone(), poison],
            &[target],
            true,
            "after",
            None,
            10_000,
            b"status-test",
            &transaction,
            1,
        )
        .unwrap();
    assert_eq!(applied.len(), 3);
    let target_before = message_list(&state, "members")
        .into_iter()
        .find(|member| i32_field(member, "member_id") == Some(target))
        .unwrap();
    let maximum_hp = i32_field(&target_before, "max_hp").unwrap();
    let hp_before = i32_field(&target_before, "hp").unwrap();
    assert_eq!(
        message_list(&target_before, "state_changes")
            .into_iter()
            .filter(|change| i32_field(change, "state_change_id") == Some(940007))
            .count(),
        3
    );
    runtime
        .apply_for_action(
            &proto,
            &mut state,
            source,
            0,
            &[TutorialSkillEffect {
                id: 780107001,
                value: 0,
            }],
            &[target],
            true,
            "after",
            None,
            10_000,
            b"status-test",
            &transaction,
            2,
        )
        .unwrap();
    let blocked = runtime
        .apply(
            &proto,
            &mut state,
            target,
            &[TutorialSkillEffect {
                id: 91001017,
                value: 1_000,
            }],
            &[target],
            true,
            "after",
            None,
        )
        .unwrap();
    assert_eq!(
        i32_or_enum_field(&blocked[0], "deal_state_change_result"),
        Some(4)
    );
    assert!(message_list(
        &message_list(&state, "members")
            .into_iter()
            .find(|member| i32_field(member, "member_id") == Some(target))
            .unwrap(),
        "state_changes",
    )
    .into_iter()
    .all(|change| i32_field(&change, "state_change_id") != Some(910050)));
    let (ticks, disabled, killed) = runtime
        .prepare_turn(&proto, &mut state, target, b"status-test", &transaction, 2)
        .unwrap();
    let damage = (maximum_hp.saturating_mul(200) / 10_000).max(1);
    assert_eq!(ticks.len(), 3);
    assert!(!disabled && !killed);
    let target_after = message_list(&state, "members")
        .into_iter()
        .find(|member| i32_field(member, "member_id") == Some(target))
        .unwrap();
    assert_eq!(i32_field(&target_after, "hp"), Some(hp_before - damage * 3));
    assert!(message_list(&target_after, "state_changes")
        .into_iter()
        .filter(|change| i32_field(change, "state_change_id") == Some(940007))
        .all(|change| i32_field(&change, "rest_count") == Some(4)));

    let conditional_poison = TutorialSkillEffect {
        id: 780089007,
        value: 200,
    };
    assert!(runtime
        .apply_for_action(
            &proto,
            &mut state,
            source,
            20002581,
            std::slice::from_ref(&conditional_poison),
            &[target],
            true,
            "after",
            None,
            10_000,
            b"status-test",
            &transaction,
            3,
        )
        .unwrap()
        .is_empty());
    let mut members = message_list(&state, "members");
    members
        .iter_mut()
        .find(|member| i32_field(member, "member_id") == Some(target))
        .unwrap()
        .set_field_by_name("hp", Value::I32(maximum_hp * 70 / 100));
    state.set_field_by_name(
        "members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );
    let mut members = message_list(&state, "members");
    members
        .iter_mut()
        .find(|member| i32_field(member, "member_id") == Some(target))
        .unwrap()
        .set_field_by_name("state_changes", Value::List(Vec::new()));
    state.set_field_by_name(
        "members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );
    let three_turn_poison = TutorialSkillEffect {
        id: 780007013,
        value: 200,
    };
    runtime
        .apply_for_action(
            &proto,
            &mut state,
            source,
            26002086,
            &[
                three_turn_poison.clone(),
                three_turn_poison.clone(),
                three_turn_poison,
            ],
            &[target],
            true,
            "after",
            None,
            10_000,
            b"status-test",
            &transaction,
            6,
        )
        .unwrap();
    let poison_states = message_list(
        &message_list(&state, "members")
            .into_iter()
            .find(|member| i32_field(member, "member_id") == Some(target))
            .unwrap(),
        "state_changes",
    )
    .into_iter()
    .filter(|change| i32_field(change, "state_change_id") == Some(940007))
    .collect::<Vec<_>>();
    assert_eq!(poison_states.len(), 3);
    assert!(poison_states
        .iter()
        .all(|change| i32_field(change, "rest_count") == Some(3)));
    let mut members = message_list(&state, "members");
    members
        .iter_mut()
        .find(|member| i32_field(member, "member_id") == Some(target))
        .unwrap()
        .set_field_by_name("state_changes", Value::List(Vec::new()));
    state.set_field_by_name(
        "members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );
    runtime
        .apply_for_action(
            &proto,
            &mut state,
            source,
            20002581,
            std::slice::from_ref(&conditional_poison),
            &[target],
            true,
            "after",
            None,
            10_000,
            b"status-test",
            &transaction,
            4,
        )
        .unwrap();
    runtime
        .apply_for_action(
            &proto,
            &mut state,
            source,
            32000409,
            &[TutorialSkillEffect {
                id: 780014003,
                value: 200,
            }],
            &[target],
            true,
            "after",
            None,
            10_000,
            b"status-test",
            &transaction,
            5,
        )
        .unwrap();
    let poison_rests = message_list(
        &message_list(&state, "members")
            .into_iter()
            .find(|member| i32_field(member, "member_id") == Some(target))
            .unwrap(),
        "state_changes",
    )
    .into_iter()
    .filter(|change| i32_field(change, "state_change_id") == Some(940007))
    .filter_map(|change| i32_field(&change, "rest_count"))
    .collect::<Vec<_>>();
    assert!(poison_rests.contains(&3));
    assert!(poison_rests.contains(&5));

    runtime.expire(target, &[], true, false);
    let mut members = message_list(&state, "members");
    let target_member = members
        .iter_mut()
        .find(|member| i32_field(member, "member_id") == Some(target))
        .unwrap();
    target_member.set_field_by_name("state_changes", Value::List(Vec::new()));
    state.set_field_by_name(
        "members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );
    runtime
        .apply_for_action(
            &proto,
            &mut state,
            source,
            0,
            &[
                TutorialSkillEffect {
                    id: 780023003,
                    value: 2_500,
                },
                TutorialSkillEffect {
                    id: 780039006,
                    value: 0,
                },
                TutorialSkillEffect {
                    id: 780015042,
                    value: 2_000,
                },
            ],
            &[target],
            true,
            "after",
            None,
            10_000,
            b"status-test",
            &transaction,
            3,
        )
        .unwrap();
    let paralysis_index = message_list(
        &message_list(&state, "members")
            .into_iter()
            .find(|member| i32_field(member, "member_id") == Some(target))
            .unwrap(),
        "state_changes",
    )
    .iter()
    .position(|change| i32_field(change, "state_change_id") == Some(940005))
    .unwrap();
    let action_number = (4..1_000)
        .find(|number| {
            deterministic_roll(
                b"status-test",
                &transaction,
                *number,
                b"paralysis",
                target,
                paralysis_index as u32,
            ) % 10_000
                < 2_000
        })
        .unwrap();
    let (turn_results, disabled, killed) = runtime
        .prepare_turn(
            &proto,
            &mut state,
            target,
            b"status-test",
            &transaction,
            action_number,
        )
        .unwrap();
    assert!(disabled && !killed);
    assert!(turn_results
        .iter()
        .any(|result| bool_field(result, "disabled_action")));
    assert_eq!(runtime.blind_rate(target), 2_500);
    assert_eq!(runtime.provocation_target(target), Some(source));

    runtime.expire(target, &[], true, false);
    let mut members = message_list(&state, "members");
    let target_member = members
        .iter_mut()
        .find(|member| i32_field(member, "member_id") == Some(target))
        .unwrap();
    target_member.set_field_by_name("hp", Value::I32(1));
    target_member.set_field_by_name("is_alive", Value::Bool(true));
    target_member.set_field_by_name("state_changes", Value::List(Vec::new()));
    state.set_field_by_name(
        "members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );
    let burn = TutorialSkillEffect {
        id: 780026001,
        value: 100,
    };
    runtime
        .apply_for_action(
            &proto,
            &mut state,
            source,
            0,
            &[burn.clone(), burn],
            &[target],
            true,
            "after",
            None,
            10_000,
            b"status-test",
            &transaction,
            action_number + 1,
        )
        .unwrap();
    let (burn_ticks, _, killed) = runtime
        .prepare_turn(
            &proto,
            &mut state,
            target,
            b"status-test",
            &transaction,
            action_number + 2,
        )
        .unwrap();
    let target_after_burn = message_list(&state, "members")
        .into_iter()
        .find(|member| i32_field(member, "member_id") == Some(target))
        .unwrap();
    assert_eq!(burn_ticks.len(), 2);
    assert!(killed);
    assert_eq!(i32_field(&target_after_burn, "hp"), Some(0));
    assert_eq!(
        message_list(&target_after_burn, "state_changes")
            .into_iter()
            .filter(|change| i32_field(change, "state_change_id") == Some(940006))
            .count(),
        2
    );

    runtime.expire(target, &[], true, false);
    let mut members = message_list(&state, "members");
    let target_member = members
        .iter_mut()
        .find(|member| i32_field(member, "member_id") == Some(target))
        .unwrap();
    target_member.set_field_by_name("hp", Value::I32(maximum_hp));
    target_member.set_field_by_name("is_alive", Value::Bool(true));
    target_member.set_field_by_name("state_changes", Value::List(Vec::new()));
    state.set_field_by_name(
        "members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );
    runtime
        .apply_for_action(
            &proto,
            &mut state,
            source,
            14000711,
            &[TutorialSkillEffect {
                id: 91001064,
                value: 0,
            }],
            &[target],
            true,
            "after",
            None,
            10_000,
            b"status-test",
            &transaction,
            action_number + 3,
        )
        .unwrap();
    assert!(message_list(&state, "members")
        .iter()
        .find(|member| i32_field(member, "member_id") == Some(target))
        .is_some_and(|member| bool_field(member, "is_stun")));
    let (stun_results, disabled, killed) = runtime
        .prepare_turn(
            &proto,
            &mut state,
            target,
            b"status-test",
            &transaction,
            action_number + 4,
        )
        .unwrap();
    assert!(disabled && !killed);
    assert!(stun_results
        .iter()
        .any(|result| bool_field(result, "disabled_action")));
    assert!(message_list(&state, "members")
        .iter()
        .find(|member| i32_field(member, "member_id") == Some(target))
        .is_some_and(|member| !bool_field(member, "is_stun")));
}
