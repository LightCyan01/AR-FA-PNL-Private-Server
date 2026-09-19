use super::super::*;
use crate::state::combat::prelude::*;
use std::path::Path;

#[test]
fn repeated_poison_uses_pre_action_abnormal_state() {
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
    let transaction = start.start_txid;
    let mut state = start.state;
    let mut runtime = start.effects;
    let source = message_list(&state, "members")
        .iter()
        .find(|member| member_type(member).ok() == Some(0))
        .and_then(|member| member_id(member).ok())
        .unwrap();
    let target = message_list(&state, "members")
        .iter()
        .find(|member| member_type(member).ok() == Some(1))
        .and_then(|member| member_id(member).ok())
        .unwrap();
    let skill = gameplay
        .skills
        .iter()
        .find(|skill| skill.id == 22001879)
        .unwrap();

    for (skill_id, first_duration, extra_duration) in [(22001879, 3, 2), (32004186, 3, 3)] {
        let first = rule_for(780056012, "active", "skill", skill_id)
            .unwrap()
            .unwrap();
        let extra = rule_for(71261002, "active", "skill", skill_id)
            .unwrap()
            .unwrap();
        assert_eq!((first.state_id, first.duration), (940007, first_duration));
        assert_eq!((extra.state_id, extra.duration), (940007, extra_duration));
        assert_eq!(extra.condition.get("target_abnormal"), Some(&1));
    }

    let clean_snapshot = state.clone();
    let applied = runtime
        .apply_for_action(
            &proto,
            &mut state,
            source,
            skill.id,
            &skill.effects,
            &[target],
            true,
            "after",
            Some(&clean_snapshot),
            10_000,
            b"poison-stack-test",
            &transaction,
            1,
        )
        .unwrap();
    assert_eq!(applied.len(), 3);

    let mut members = message_list(&state, "members");
    members
        .iter_mut()
        .find(|member| member_id(member).ok() == Some(target))
        .unwrap()
        .set_field_by_name(
            "state_changes",
            Value::List(vec![Value::Message(display(&proto, 940005, 0, 1).unwrap())]),
        );
    state.set_field_by_name(
        "members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );
    let abnormal_snapshot = state.clone();
    let applied = runtime
        .apply_for_action(
            &proto,
            &mut state,
            source,
            skill.id,
            &skill.effects,
            &[target],
            true,
            "after",
            Some(&abnormal_snapshot),
            10_000,
            b"poison-stack-test",
            &transaction,
            2,
        )
        .unwrap();
    assert_eq!(applied.len(), 6);
    assert_eq!(
        message_list(
            message_list(&state, "members")
                .iter()
                .find(|member| member_id(member).ok() == Some(target))
                .unwrap(),
            "state_changes",
        )
        .iter()
        .filter(|change| i32_field(change, "state_change_id") == Some(940007))
        .count(),
        6
    );
}

#[test]
fn battle_start_poison_registers_every_stack() {
    for index in 0..10 {
        let rule = rule_for_occurrence(120010135, "passive", "ability", 600010070, Some(index))
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
            ("status", "self", 940007, &Expiry::Turn, 5)
        );
    }
}
