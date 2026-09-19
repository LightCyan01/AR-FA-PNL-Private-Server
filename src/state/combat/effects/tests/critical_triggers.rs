use super::super::*;
use crate::state::combat::prelude::*;
use std::path::Path;

fn summary(state: &DynamicMessage, member_id: i32, summary: i32) -> i32 {
    message_list(state, "members")
        .into_iter()
        .find(|member| i32_field(member, "member_id") == Some(member_id))
        .map_or(0, |member| state_change_summary_value(&member, summary))
}

#[test]
fn critical_skill_modifiers_apply_only_after_critical_hits() {
    for (effect_id, skill_id, operation, summary, expiry, duration, cap) in [
        (
            91001334,
            12001624,
            "summary",
            1,
            Expiry::Permanent,
            -1,
            10_000,
        ),
        (91001480, 12002124, "magic", 0, Expiry::Turn, 2, 0),
        (91001565, 12002234, "attack", 0, Expiry::Turn, 1, 0),
    ] {
        let rule = rule_for(effect_id, "active", "skill", skill_id)
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                rule.operation.as_str(),
                rule.summary,
                rule.target.as_str(),
                &rule.expiry,
                rule.duration,
                rule.stack_cap,
                rule.critical_only,
            ),
            (operation, summary, "self", &expiry, duration, cap, true)
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
    let mut state = start.state;
    let actor_id = member_id(&current_actor(&state).unwrap()).unwrap();
    let target_id = message_list(&state, "members")
        .iter()
        .find(|member| member_type(member).ok() == Some(1))
        .and_then(|member| i32_field(member, "member_id"))
        .unwrap();
    let skill = rules
        .skills
        .iter()
        .find(|skill| skill.id == 12001624)
        .unwrap();
    let mut runtime = Runtime::default();
    runtime.prepare(&state, "critical-trigger").unwrap();
    let initial = summary(&state, actor_id, 1);

    runtime
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
            b"critical-trigger",
            "critical-trigger",
            1,
        )
        .unwrap();
    let normal = build_skill_result(
        &proto, target_id, 1, 0, 0, 0, true, false, false, false, false, false, false,
    )
    .unwrap();
    runtime
        .trigger_attack_after(&proto, &rules, &mut state, actor_id, skill, &[normal])
        .unwrap();
    assert_eq!(summary(&state, actor_id, 1), initial);

    let critical = build_skill_result(
        &proto, target_id, 1, 1, 0, 0, true, false, false, true, false, false, false,
    )
    .unwrap();
    runtime
        .trigger_attack_after(
            &proto,
            &rules,
            &mut state,
            actor_id,
            skill,
            std::slice::from_ref(&critical),
        )
        .unwrap();
    assert_eq!(summary(&state, actor_id, 1), initial + 5_000);
    runtime
        .trigger_attack_after(&proto, &rules, &mut state, actor_id, skill, &[critical])
        .unwrap();
    assert_eq!(summary(&state, actor_id, 1), initial + 10_000);

    let stacked_rule = rule_for_occurrence(6000374, "passive", "ability", 300396, Some(0))
        .unwrap()
        .unwrap()
        .clone();
    assert_eq!(
        (
            stacked_rule.target.as_str(),
            stacked_rule.trigger.as_deref(),
            stacked_rule.skill_target_types.as_slice(),
            &stacked_rule.expiry,
            stacked_rule.duration,
        ),
        ("self", Some("attack_after"), &[3][..], &Expiry::Turn, 1)
    );
    for ability_id in [300396, 300397] {
        for index in 0..4 {
            assert!(
                rule_for_occurrence(6000374, "passive", "ability", ability_id, Some(index),)
                    .unwrap()
                    .is_some()
            );
        }
    }

    let mut stacked = Runtime::default();
    stacked.prepare(&state, "stacked-attack-after").unwrap();
    for _ in 0..4 {
        stacked.passives.push(Passive {
            source: actor_id,
            value: 500,
            rule: stacked_rule.clone(),
            source_character_id: stacked_rule.source_character_ids[0],
            source_type: 0,
        });
    }
    let before = summary(&state, actor_id, 1);
    let hit = build_skill_result(
        &proto, target_id, 1, 0, 0, 0, true, false, false, false, false, false, false,
    )
    .unwrap();
    assert_eq!(
        stacked
            .trigger_attack_after(
                &proto,
                &rules,
                &mut state,
                actor_id,
                skill,
                std::slice::from_ref(&hit),
            )
            .unwrap()
            .len(),
        4
    );
    assert_eq!(summary(&state, actor_id, 1), before + 2_000);
    assert_eq!(
        stacked
            .instances
            .iter()
            .filter(|instance| instance.rule.id == 6000374)
            .count(),
        4
    );
    stacked
        .trigger_attack_after(&proto, &rules, &mut state, actor_id, skill, &[hit])
        .unwrap();
    assert_eq!(summary(&state, actor_id, 1), before + 2_000);
}
