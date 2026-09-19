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
    for (skill_id, base_effect_id, base_index, increment) in
        (12003833..=12003837)
            .map(|id| (id, 91002293, 2, 1))
            .chain((14003838..=14003842).map(|id| (id, 91002294, 2, 2)))
            .chain(std::iter::once((14003845, 91002295, 3, 5)))
    {
        let base = rule_for_occurrence(
            base_effect_id,
            "active",
            "skill",
            skill_id,
            Some(base_index),
        )
        .unwrap()
        .unwrap();
        let bonus_index = if skill_id == 14003845 { 4 } else { 5 };
        let bonus = rule_for_occurrence(6001303, "active", "skill", skill_id, Some(bonus_index))
            .unwrap()
            .unwrap();
        assert_eq!(
            (base.state_id, base.fixed, base.stack_cap),
            (610517, Some(increment), 10)
        );
        assert_eq!(
            (bonus.state_id, bonus.fixed, bonus.required_ability_id),
            (610517, Some(1), 301354)
        );
    }
    let marker = rule_for_occurrence(6001298, "passive", "ability", 301354, Some(2))
        .unwrap()
        .unwrap();
    assert_eq!(marker.operation, "marker");
    assert_eq!(marker.source_character_ids.len(), 1);
    for index in [4, 5] {
        let crystal = rule_for_occurrence(91001939, "active", "skill", 14003260, Some(index))
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                crystal.operation.as_str(),
                crystal.state_id,
                crystal.fixed,
                crystal.stack_cap,
            ),
            ("level_state", 510246, Some(1), 5)
        );
        assert_eq!(
            crystal
                .level_modifiers
                .iter()
                .map(|modifier| (modifier.summary, modifier.value))
                .collect::<Vec<_>>(),
            vec![(4, 3_000), (6, 3_000)]
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

    let mut members = message_list(&state, "members");
    let actor = members
        .iter_mut()
        .find(|member| member_id(member).ok() == Some(actor_id))
        .unwrap();
    let mut ally = member_status(actor, "ally").unwrap();
    ally.set_field_by_name(
        "character_id",
        Value::I32(marker.source_character_ids[0]),
    );
    actor.set_field_by_name("ally", Value::Message(ally));
    state.set_field_by_name(
        "members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );
    let lichtlumen = rules.skills.iter().find(|skill| skill.id == 12003833).unwrap();
    runtime
        .apply_for_action_with_rules(
            &proto,
            &rules,
            &mut state,
            actor_id,
            lichtlumen.id,
            &lichtlumen.effects,
            &[target_id],
            true,
            "after",
            None,
            lichtlumen.state_change_application_rate,
            b"level-test",
            &transaction,
            13,
        )
        .unwrap();
    assert_eq!(level(&state, actor_id, 610517), Some(1));
    runtime.passives.push(Passive {
        source: actor_id,
        value: 0,
        rule: marker.clone(),
        source_character_id: marker.source_character_ids[0],
        source_type: member_type(&member(&state, actor_id)).unwrap(),
    });
    runtime
        .apply_for_action_with_rules(
            &proto,
            &rules,
            &mut state,
            actor_id,
            lichtlumen.id,
            &lichtlumen.effects,
            &[target_id],
            true,
            "after",
            None,
            lichtlumen.state_change_application_rate,
            b"level-test",
            &transaction,
            14,
        )
        .unwrap();
    assert_eq!(level(&state, actor_id, 610517), Some(3));

    let yellow_crystal = rules.skills.iter().find(|skill| skill.id == 14003260).unwrap();
    let before_power = state_change_summary_value(&member(&state, actor_id), 4);
    let before_critical = state_change_summary_value(&member(&state, actor_id), 6);
    for action_number in 15..=17 {
        runtime
            .apply_for_action_with_rules(
                &proto,
                &rules,
                &mut state,
                actor_id,
                yellow_crystal.id,
                &yellow_crystal.effects,
                &[target_id],
                true,
                "after",
                None,
                yellow_crystal.state_change_application_rate,
                b"level-test",
                &transaction,
                action_number,
            )
            .unwrap();
    }
    assert_eq!(level(&state, actor_id, 510246), Some(5));
    assert_eq!(
        state_change_summary_value(&member(&state, actor_id), 4),
        before_power + 15_000
    );
    assert_eq!(
        state_change_summary_value(&member(&state, actor_id), 6),
        before_critical + 15_000
    );
}
