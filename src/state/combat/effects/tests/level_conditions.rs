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
fn named_level_thresholds_drive_damage_healing_and_cleanup() {
    for skill_id in (12002592..=12002596).chain(12002617..=12002621) {
        for (index, effect_id, level) in [
            (1, 91001636, 1),
            (2, 91001637, 3),
            (4, 91001638, 5),
            (6, 91001639, 7),
            (8, 91001640, 10),
        ] {
            let rule = rule_for_occurrence(effect_id, "instant", "skill", skill_id, Some(index))
                .unwrap()
                .unwrap();
            assert_eq!(
                (
                    rule.operation.as_str(),
                    rule.summary,
                    rule.source_state_level_min
                ),
                ("summary", 1, level)
            );
            assert_eq!(rule.source_state_ids.as_slice(), [610229].as_slice());
        }
        for (index, effect_id, level) in [(3, 91001641, 3), (5, 91001642, 5), (7, 91001643, 7)] {
            let rule = rule_for_occurrence(effect_id, "active", "skill", skill_id, Some(index))
                .unwrap()
                .unwrap();
            assert_eq!(
                (rule.operation.as_str(), rule.source_state_level_min),
                ("heal", level)
            );
        }
        let removal = rule_for_occurrence(91001644, "active", "skill", skill_id, Some(10))
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                removal.operation.as_str(),
                removal.state_id,
                removal.fixed,
                removal.source_state_ids.as_slice(),
            ),
            ("remove_stack", 610229, None, [610229].as_slice())
        );
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
    let grant = rules
        .skills
        .iter()
        .find(|skill| skill.id == 12002421)
        .unwrap();
    for action_number in 1..=5 {
        runtime
            .apply_for_action_with_rules(
                &proto,
                &rules,
                &mut state,
                actor_id,
                grant.id,
                &grant.effects,
                &[target_id],
                true,
                "after",
                None,
                grant.state_change_application_rate,
                b"level-condition-test",
                &transaction,
                action_number,
            )
            .unwrap();
    }

    let skill = rules
        .skills
        .iter()
        .find(|skill| skill.id == 12002592)
        .unwrap();
    assert_eq!(
        instant_summary_for_source(
            Some(&battle_member(&state, actor_id)),
            skill,
            &battle_member(&state, target_id),
            false,
            1,
        )
        .unwrap(),
        20_000
    );
    let mut members = message_list(&state, "members");
    let actor = members
        .iter_mut()
        .find(|member| i32_field(member, "member_id") == Some(actor_id))
        .unwrap();
    let wounded_hp = i32_field(actor, "max_hp").unwrap() / 10;
    actor.set_field_by_name("hp", Value::I32(wounded_hp));
    state.set_field_by_name(
        "members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );
    let results = runtime
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
            b"level-condition-test",
            &transaction,
            6,
        )
        .unwrap();
    for effect_id in [91001641, 91001642, 91001643, 91001644] {
        assert!(results
            .iter()
            .any(|result| i32_field(result, "effect_id") == Some(effect_id)));
    }
    assert!(i32_field(&battle_member(&state, actor_id), "hp").unwrap() > wounded_hp);
    let removed = results
        .iter()
        .flat_map(|result| message_list(result, "removed_state_changes"))
        .find(|change| i32_field(change, "state_change_id") == Some(610229))
        .unwrap();
    assert_eq!(message_i32_field(&removed, "level", "value"), Some(10));
    assert!(!runtime
        .instances
        .iter()
        .any(|instance| instance.rule.state_id == 610229));
    assert_eq!(
        instant_summary_for_source(
            Some(&battle_member(&state, actor_id)),
            skill,
            &battle_member(&state, target_id),
            false,
            1,
        )
        .unwrap(),
        0
    );
}

#[test]
fn elegant_fragrance_scopes_and_gates_slashing_attack_buffs() {
    let base = rule_for(91002356, "active", "skill", 12003960)
        .unwrap()
        .unwrap();
    let gated = rule_for(91002357, "active", "skill", 12003960)
        .unwrap()
        .unwrap();
    assert!(base.target_character_ids.contains(&10701));
    assert!(!base.target_character_ids.contains(&10101));
    assert_eq!(gated.target_character_ids, base.target_character_ids);
    assert_eq!(gated.source_state_ids.as_slice(), [610545].as_slice());
    assert_eq!(gated.source_state_level_min, 30);

    let grant = rule_for_occurrence(91002358, "active", "skill", 14003965, Some(2))
        .unwrap()
        .unwrap();
    assert_eq!(
        (
            grant.target.as_str(),
            grant.operation.as_str(),
            grant.fixed,
            grant.state_id,
            grant.stack_cap,
        ),
        ("allies", "level_state", Some(3), 610545, 50)
    );

    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let mut source = empty_message(&proto, "blend.model.BattleMember").unwrap();
    source.set_field_by_name(
        "state_changes",
        Value::List(vec![Value::Message(
            super::super::runtime_results::level_display(&proto, 610545, 29, -1).unwrap(),
        )]),
    );
    assert!(!super::super::runtime_match::condition(gated, &source));
    source.set_field_by_name(
        "state_changes",
        Value::List(vec![Value::Message(
            super::super::runtime_results::level_display(&proto, 610545, 30, -1).unwrap(),
        )]),
    );
    assert!(super::super::runtime_match::condition(gated, &source));
}
