use super::super::*;
use crate::state::combat::prelude::*;
use std::path::Path;

fn member(state: &DynamicMessage, member_id: i32) -> DynamicMessage {
    message_list(state, "members")
        .into_iter()
        .find(|member| i32_field(member, "member_id") == Some(member_id))
        .unwrap()
}

fn level(state: &DynamicMessage, member_id: i32, state_id: i32) -> Option<i32> {
    message_list(&member(state, member_id), "state_changes")
        .into_iter()
        .find(|change| i32_field(change, "state_change_id") == Some(state_id))
        .and_then(|change| message_i32_field(&change, "level", "value"))
}

#[test]
fn named_levels_increment_to_their_cap_and_use_the_protocol_level_field() {
    for (effect_id, owner_index, skill_ids, state_id, increment) in [
        (
            91001172,
            0,
            (12000992..=12000996).collect::<Vec<_>>(),
            610079,
            1,
        ),
        (
            91001632,
            2,
            (12002421..=12002425)
                .chain(12002602..=12002606)
                .collect(),
            610229,
            2,
        ),
    ] {
        for skill_id in skill_ids {
            let rule = rule_for_occurrence(
                effect_id,
                "active",
                "skill",
                skill_id,
                Some(owner_index),
            )
            .unwrap()
            .unwrap();
            assert_eq!(
                (
                    rule.operation.as_str(),
                    rule.target.as_str(),
                    rule.state_id,
                    rule.fixed,
                    rule.stack_cap,
                    &rule.expiry,
                ),
                (
                    "level_state",
                    "self",
                    state_id,
                    Some(increment),
                    10,
                    &Expiry::Permanent,
                )
            );
        }
    }

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
    let transaction = start.start_txid.clone();
    let mut state = start.state;
    let mut runtime = start.effects;
    let actor_id = member_id(&current_actor(&state).unwrap()).unwrap();
    let target_id = message_list(&state, "members")
        .iter()
        .find(|member| member_type(member).ok() == Some(1))
        .and_then(|member| i32_field(member, "member_id"))
        .unwrap();
    let skill = rules.skills.iter().find(|skill| skill.id == 12000992).unwrap();
    let mut last_results = Vec::new();
    for action_number in 1..=12 {
        last_results = runtime
            .apply_for_action_with_rules(
                &proto,
                &rules,
                &mut state,
                actor_id,
                skill.id,
                &skill.effects,
                &[target_id],
                true,
                "after",
                None,
                skill.state_change_application_rate,
                b"level-test",
                &transaction,
                action_number,
            )
            .unwrap();
    }

    assert_eq!(level(&state, actor_id, 610079), Some(10));
    let visible = message_list(&member(&state, actor_id), "state_changes")
        .into_iter()
        .find(|change| i32_field(change, "state_change_id") == Some(610079))
        .unwrap();
    assert_eq!(i32_field(&visible, "value"), Some(0));
    assert_eq!(i32_field(&visible, "rest_count"), Some(-1));
    let dealt_value = last_results[0]
        .get_field_by_name("dealt_state_change")
        .unwrap();
    let dealt = dealt_value.as_message().unwrap();
    assert_eq!(message_i32_field(dealt, "level", "value"), Some(10));
    assert_eq!(
        runtime
            .instances
            .iter()
            .find(|instance| instance.rule.state_id == 610079)
            .map(|instance| instance.value),
        Some(10)
    );
}
