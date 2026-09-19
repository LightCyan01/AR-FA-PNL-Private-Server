use super::super::*;
use crate::state::combat::prelude::*;
use std::path::Path;

#[test]
fn strengthen_removal_covers_all_owners() {
    for (skill_id, target) in [
        (20007183, "enemies"),
        (20009810, "targets"),
        (20009811, "targets"),
        (22000175, "targets"),
        (22000489, "targets"),
        (22000566, "enemies"),
        (22000787, "enemies"),
        (32000918, "targets"),
        (32000959, "targets"),
    ] {
        let rule = rule_for(780045012, "active", "skill", skill_id)
            .unwrap()
            .unwrap();
        assert_eq!(
            (rule.target.as_str(), rule.phase.as_str(), rule.operation.as_str()),
            (target, "after", "cleanse_positive")
        );
        assert!(rule.affected_state_ids.is_empty());
        assert!(!rule.positive);
    }
}

#[test]
fn bs_attack_damage_up_cleanse_covers_all_owners() {
    for skill_id in [
        22001839, 32002691, 32002692, 32003287, 32004253, 32004272, 32004725, 32005081,
    ] {
        let rule = rule_for(76246012, "active", "skill", skill_id)
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                rule.target.as_str(),
                rule.phase.as_str(),
                rule.operation.as_str()
            ),
            ("self", "after", "cleanse_positive")
        );
        assert_eq!(rule.affected_state_ids, [50001]);
        assert!(!rule.positive);
    }
}

#[test]
fn specific_cleanses_preserve_unrelated_states() {
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
    let source = message_list(&start.state, "members")
        .into_iter()
        .find(|member| member_type(member).ok() == Some(0))
        .and_then(|member| member_id(&member).ok())
        .unwrap();
    let mut state = start.state;
    let mut members = message_list(&state, "members");
    members
        .iter_mut()
        .find(|member| member_id(member).ok() == Some(source))
        .unwrap()
        .set_field_by_name(
            "state_changes",
            Value::List(vec![
                Value::Message(display(&proto, 920001, 10_000, 1).unwrap()),
                Value::Message(display(&proto, 920015, 2_000, 1).unwrap()),
            ]),
        );
    state.set_field_by_name(
        "members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );
    let mut runtime = start.effects;
    let results = runtime
        .apply_for_action(
            &proto,
            &mut state,
            source,
            12000901,
            &[TutorialSkillEffect {
                id: 91001137,
                value: 10_000,
            }],
            &[source],
            true,
            "after",
            None,
            10_000,
            b"specific-cleanse",
            &start.start_txid,
            1,
        )
        .unwrap();
    assert_eq!(
        message_list(&results[0], "removed_state_changes")
            .into_iter()
            .map(|change| i32_field(&change, "state_change_id").unwrap())
            .collect::<Vec<_>>(),
        [920001]
    );
    let remaining = message_list(
        &message_list(&state, "members")
            .into_iter()
            .find(|member| member_id(member).ok() == Some(source))
            .unwrap(),
        "state_changes",
    )
    .into_iter()
    .map(|change| i32_field(&change, "state_change_id").unwrap())
    .collect::<Vec<_>>();
    assert!(!remaining.contains(&920001));
    assert!(remaining.contains(&920015));

    let before_heal = rule_for(91001137, "active", "skill", 14000906)
        .unwrap()
        .unwrap();
    assert_eq!(before_heal.phase, "before");
    assert_eq!(before_heal.affected_state_ids, [920001]);

    let enemy = message_list(&state, "members")
        .into_iter()
        .find(|member| member_type(member).ok() == Some(1))
        .and_then(|member| member_id(&member).ok())
        .unwrap();
    let mut members = message_list(&state, "members");
    let target = members
        .iter_mut()
        .find(|member| member_id(member).ok() == Some(enemy))
        .unwrap();
    target.set_field_by_name(
        "state_changes",
        Value::List(vec![
            Value::Message(display(&proto, 910045, 3_000, 1).unwrap()),
            Value::Message(display(&proto, 50001, 2_000, 1).unwrap()),
            Value::Message(display(&proto, 920015, 1_000, 1).unwrap()),
        ]),
    );
    let mut enemy_status = member_status(target, "enemy").unwrap();
    enemy_status.set_field_by_name("is_broken", Value::Bool(false));
    target.set_field_by_name("enemy", Value::Message(enemy_status));
    state.set_field_by_name(
        "members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );
    let effect = [TutorialSkillEffect {
        id: 91001590,
        value: 10_000,
    }];
    assert!(runtime
        .apply_for_action(
            &proto,
            &mut state,
            source,
            12002867,
            &effect,
            &[enemy],
            true,
            "after",
            None,
            10_000,
            b"damage-reduction-cleanse",
            &start.start_txid,
            2,
        )
        .unwrap()
        .is_empty());

    let mut members = message_list(&state, "members");
    let target = members
        .iter_mut()
        .find(|member| member_id(member).ok() == Some(enemy))
        .unwrap();
    let mut enemy_status = member_status(target, "enemy").unwrap();
    enemy_status.set_field_by_name("is_broken", Value::Bool(true));
    target.set_field_by_name("enemy", Value::Message(enemy_status));
    state.set_field_by_name(
        "members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );
    let results = runtime
        .apply_for_action(
            &proto,
            &mut state,
            source,
            12002867,
            &effect,
            &[enemy],
            true,
            "after",
            None,
            10_000,
            b"damage-reduction-cleanse",
            &start.start_txid,
            3,
        )
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(
        message_list(&results[0], "removed_state_changes")
            .into_iter()
            .map(|change| i32_field(&change, "state_change_id").unwrap())
            .collect::<Vec<_>>(),
        [910045]
    );
    let remaining = message_list(
        &message_list(&state, "members")
            .into_iter()
            .find(|member| member_id(member).ok() == Some(enemy))
            .unwrap(),
        "state_changes",
    )
    .into_iter()
    .map(|change| i32_field(&change, "state_change_id").unwrap())
    .collect::<Vec<_>>();
    assert!(!remaining.contains(&910045));
    assert!(remaining.contains(&50001));
    assert!(remaining.contains(&920015));

    let broken_cleanup = rule_for(91001590, "active", "skill", 12002298)
        .unwrap()
        .unwrap();
    assert!(broken_cleanup.target_broken);
    assert!(broken_cleanup.affected_state_ids.contains(&910045));
    assert!(!broken_cleanup.affected_state_ids.contains(&50001));
    assert!(!broken_cleanup.affected_state_ids.contains(&810189));
    let before_cleanup = rule_for(91001142, "active", "skill", 14000925)
        .unwrap()
        .unwrap();
    assert_eq!(before_cleanup.phase, "before");
    assert!(!before_cleanup.target_broken);
    assert_eq!(
        before_cleanup.affected_state_ids,
        broken_cleanup.affected_state_ids
    );
}
