use super::super::*;
use crate::state::combat::prelude::*;
use std::path::Path;

#[test]
fn healing_immunity_cleanse_preserves_other_negative_effects() {
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
}
