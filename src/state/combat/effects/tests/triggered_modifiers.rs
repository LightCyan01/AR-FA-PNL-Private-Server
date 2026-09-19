use super::super::*;
use crate::state::combat::prelude::*;
use std::path::Path;

#[test]
fn breaker_attack_before_applies_once_for_matching_support_source() {
    let rule = rule_for_occurrence(500029, "passive", "ability", 500029, Some(0))
        .unwrap()
        .unwrap()
        .clone();
    assert_eq!(
        (
            rule.operation.as_str(),
            rule.target.as_str(),
            rule.trigger.as_deref(),
            &rule.expiry,
            rule.duration,
            rule.trigger_limit,
            rule.attack_attributes.as_slice(),
        ),
        (
            "attribute_taken",
            "targets",
            Some("attack_before"),
            &Expiry::Attacked,
            1,
            1,
            [5].as_slice(),
        )
    );

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
        .find(|member| member_type(member).ok() == Some(1) && bool_field(member, "is_alive"))
        .and_then(|member| member_id(member).ok())
        .unwrap();
    let skill = rules
        .skills
        .iter()
        .find(|skill| skill.skill_effect_type == 1)
        .unwrap();
    let passive = |source_character_id| Passive {
        source: actor_id,
        value: 1_725,
        rule: rule.clone(),
        source_character_id,
        source_type: 0,
    };

    let mut rejected = Runtime::default();
    rejected.prepare(&state, "breaker-before-rejected").unwrap();
    rejected.passives = vec![passive(0)];
    let mut rejected_state = state.clone();
    assert!(rejected
        .trigger_attack_before(&proto, &mut rejected_state, actor_id, skill, &[target_id])
        .unwrap()
        .is_empty());

    let mut runtime = Runtime::default();
    runtime.prepare(&state, "breaker-before").unwrap();
    runtime.passives = vec![passive(rule.source_character_ids[0])];
    let first = runtime
        .trigger_attack_before(&proto, &mut state, actor_id, skill, &[target_id])
        .unwrap();
    let target = message_list(&state, "members")
        .into_iter()
        .find(|member| member_id(member).ok() == Some(target_id))
        .unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(
        incoming_multiplier_with_runtime(&target, 5, Some(&runtime)),
        11_725
    );
    assert_eq!(runtime.limited_effect_uses[&actor_id][&500029], 1);
    assert!(runtime
        .trigger_attack_before(&proto, &mut state, actor_id, skill, &[target_id])
        .unwrap()
        .is_empty());
}
