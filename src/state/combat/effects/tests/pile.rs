use super::super::*;
use crate::state::combat::prelude::*;
use std::path::Path;

fn battle_member(state: &DynamicMessage, member_id: i32) -> DynamicMessage {
    message_list(state, "members")
        .into_iter()
        .find(|member| i32_field(member, "member_id") == Some(member_id))
        .unwrap()
}

#[test]
fn pile_grants_scales_and_is_consumed_by_fortress() {
    for skill_id in (12002406..=12002410).chain(12002571..=12002575) {
        let grant = rule_for(91001623, "active", "skill", skill_id)
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                grant.operation.as_str(),
                grant.target.as_str(),
                grant.state_id,
                &grant.expiry,
            ),
            ("attack", "self", 610228, &Expiry::Permanent)
        );
    }
    for skill_id in (12002561..=12002565).chain(12002581..=12002585) {
        let conditional = rule_for(91001624, "instant", "skill", skill_id)
            .unwrap()
            .unwrap();
        assert_eq!(conditional.source_state_ids.as_slice(), [610228].as_slice());
        let removal = rule_for(91001626, "active", "skill", skill_id)
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                removal.operation.as_str(),
                removal.state_id,
                removal.fixed,
                removal.source_state_ids.as_slice(),
            ),
            ("remove_stack", 610228, Some(5_000), [610228].as_slice())
        );
        for (effect_id, operation, summary, fixed, maximum) in [
            (91001690, "attribute_taken", 0, 300, 1_500),
            (91001691, "attribute_taken", 0, 300, 1_500),
            (91001692, "summary", 3, 400, 2_000),
        ] {
            let scaled = rule_for(effect_id, "active", "skill", skill_id)
                .unwrap()
                .unwrap();
            assert_eq!(
                (
                    scaled.operation.as_str(),
                    scaled.summary,
                    scaled.fixed,
                    scaled.scale_by.as_str(),
                    scaled.scale_input_min,
                    scaled.scale_input_max,
                    scaled.scale_output_max,
                ),
                (
                    operation,
                    summary,
                    Some(fixed),
                    "party_tag_count",
                    1,
                    5,
                    maximum,
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
    let crusade = rules.skills.iter().find(|skill| skill.id == 12002406).unwrap();
    let fortress = rules.skills.iter().find(|skill| skill.id == 12002561).unwrap();
    let initial_attack_rate = runtime.stat_rate(actor_id, "attack");
    assert_eq!(
        instant_summary_for_source(
            Some(&battle_member(&state, actor_id)),
            fortress,
            &battle_member(&state, target_id),
            false,
            1,
        )
        .unwrap(),
        0
    );

    runtime
        .apply_for_action_with_rules(
            &proto,
            &rules,
            &mut state,
            actor_id,
            crusade.id,
            &crusade.effects,
            &[target_id],
            true,
            "after",
            None,
            crusade.state_change_application_rate,
            b"pile-test",
            &transaction,
            1,
        )
        .unwrap();
    assert_eq!(runtime.stat_rate(actor_id, "attack"), initial_attack_rate + 5_000);
    assert!(message_list(&battle_member(&state, actor_id), "state_changes")
        .iter()
        .any(|change| i32_field(change, "state_change_id") == Some(610228)));
    assert_eq!(
        instant_summary_for_source(
            Some(&battle_member(&state, actor_id)),
            fortress,
            &battle_member(&state, target_id),
            false,
            1,
        )
        .unwrap(),
        20_000
    );

    for (phase, action_number) in [("before", 2), ("after", 3)] {
        runtime
            .apply_for_action_with_rules(
                &proto,
                &rules,
                &mut state,
                actor_id,
                fortress.id,
                &fortress.effects,
                &[target_id],
                true,
                phase,
                None,
                fortress.state_change_application_rate,
                b"pile-test",
                &transaction,
                action_number,
            )
            .unwrap();
    }
    for (effect_id, value) in [(91001690, 300), (91001691, 300), (91001692, 400)] {
        assert!(runtime
            .instances
            .iter()
            .any(|instance| instance.rule.id == effect_id && instance.value == value));
    }
    assert_eq!(runtime.stat_rate(actor_id, "attack"), initial_attack_rate);
    assert!(!runtime.instances.iter().any(|instance| instance.rule.state_id == 610228));
    assert!(!message_list(&battle_member(&state, actor_id), "state_changes")
        .iter()
        .any(|change| i32_field(change, "state_change_id") == Some(610228)));
    assert_eq!(
        instant_summary_for_source(
            Some(&battle_member(&state, actor_id)),
            fortress,
            &battle_member(&state, target_id),
            false,
            1,
        )
        .unwrap(),
        0
    );
}
