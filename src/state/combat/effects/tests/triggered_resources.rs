use super::super::*;
use crate::state::combat::prelude::*;
use std::collections::BTreeMap;
use std::path::Path;

#[test]
fn triggered_item_gauge_rules_follow_their_catalog_events() {
    let catalog = registry().unwrap();
    let mut trigger_counts = BTreeMap::new();
    for rule in catalog.rules.iter().filter(|rule| {
        rule.id == 72001085 && rule.mode == "passive" && rule.owner_type == "ability"
    }) {
        *trigger_counts
            .entry(rule.trigger.as_deref().unwrap())
            .or_insert(0) += 1;
    }
    assert_eq!(
        trigger_counts,
        BTreeMap::from([
            ("action_after", 9),
            ("attacked", 2),
            ("heal_received", 1),
            ("panel_acquired", 3),
        ])
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
    let skill = rules
        .skills
        .iter()
        .find(|skill| skill.skill_effect_type == 1)
        .unwrap();
    let passive = |ability_id, owner_index, value| Passive {
        source: actor_id,
        value,
        rule: rule_for_occurrence(
            72001085,
            "passive",
            "ability",
            ability_id,
            owner_index,
        )
        .unwrap()
        .unwrap()
        .clone(),
        source_character_id: 0,
        source_type: 0,
    };
    let expected = |value| catalog.max_party_gauge * value / 10_000;
    let mut runtime = Runtime::default();
    runtime.prepare(&state, "triggered-item-gauge").unwrap();

    runtime.passives = vec![passive(1990267, Some(0), 1_000)];
    state.set_field_by_name("party_gauge", Value::I32(0));
    let results = runtime
        .trigger_attack_after(&proto, &rules, &mut state, actor_id, skill, &[])
        .unwrap();
    assert_eq!(i32_field(&state, "party_gauge"), Some(expected(1_000)));
    assert_eq!(
        message_i32_field(&results[0], "party_gauge_heal", "value"),
        Some(expected(1_000))
    );

    let attacked = build_skill_result(
        &proto, actor_id, 1, 0, 0, 0, true, false, false, false, false, false, false,
    )
    .unwrap();
    runtime.passives = vec![passive(1990208, Some(0), 500)];
    state.set_field_by_name("party_gauge", Value::I32(0));
    let before = state.clone();
    runtime
        .trigger_lamps_after_action(
            &proto,
            &mut state,
            &before,
            actor_id,
            skill,
            &[attacked],
            true,
        )
        .unwrap();
    assert_eq!(i32_field(&state, "party_gauge"), Some(expected(500)));

    let healed = build_skill_result(
        &proto, actor_id, 0, 0, 1, 0, false, false, false, false, false, false, false,
    )
    .unwrap();
    runtime.passives = vec![passive(1990368, Some(6), 200)];
    state.set_field_by_name("party_gauge", Value::I32(0));
    let before = state.clone();
    runtime
        .trigger_lamps_after_action(
            &proto,
            &mut state,
            &before,
            actor_id,
            skill,
            &[healed],
            true,
        )
        .unwrap();
    assert_eq!(i32_field(&state, "party_gauge"), Some(expected(200)));

    runtime.passives = vec![passive(1990543, Some(2), 3_000)];
    state.set_field_by_name("party_gauge", Value::I32(0));
    runtime.acquired_panel_turn = 0;
    runtime.acquire_current_panel(&mut state).unwrap();
    assert_eq!(i32_field(&state, "party_gauge"), Some(expected(3_000)));
}
