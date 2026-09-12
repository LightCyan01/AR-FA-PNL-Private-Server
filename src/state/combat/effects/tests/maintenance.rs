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
fn immunity_states_block_only_their_effect_category() {
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
