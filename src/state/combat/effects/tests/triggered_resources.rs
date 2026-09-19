use super::super::*;
use crate::state::combat::prelude::*;
use std::collections::BTreeMap;
use std::path::Path;

fn set_burst_gauge(state: &mut DynamicMessage, member: i32, value: i32) {
    let mut members = message_list(state, "members");
    let member = members
        .iter_mut()
        .find(|candidate| member_id(candidate).ok() == Some(member))
        .unwrap();
    let mut gauge = member_status(member, "burst_gauge").unwrap();
    gauge.set_field_by_name("current_gauge", Value::I32(value));
    gauge.set_field_by_name("is_enable", Value::Bool(false));
    member.set_field_by_name("burst_gauge", Value::Message(gauge));
    state.set_field_by_name(
        "members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );
}

fn burst_gauge(state: &DynamicMessage, member: i32) -> i32 {
    message_list(state, "members")
        .into_iter()
        .find(|candidate| member_id(candidate).ok() == Some(member))
        .and_then(|member| member_status(&member, "burst_gauge").ok())
        .and_then(|gauge| i32_field(&gauge, "current_gauge"))
        .unwrap()
}

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
    assert_eq!(
        catalog
            .rules
            .iter()
            .filter(|rule| {
                rule.id == 120000292
                    && rule.mode == "passive"
                    && rule.owner_type == "ability"
                    && rule.trigger.as_deref() == Some("turn_start")
            })
            .count(),
        10
    );
    assert!(catalog.rules.iter().any(|rule| {
        rule.id == 120000243
            && rule.owner_id == 600000282
            && rule.trigger.as_deref() == Some("action_after")
    }));
    assert!(catalog.rules.iter().any(|rule| {
        rule.id == 120000232
            && rule.owner_id == 600000563
            && rule.trigger.as_deref() == Some("battle_start")
    }));

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
    let passive = |effect_id, ability_id, owner_index, value| Passive {
        source: actor_id,
        value,
        rule: rule_for_occurrence(
            effect_id,
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

    runtime.passives = vec![passive(72001085, 1990267, Some(0), 1_000)];
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
    runtime.passives = vec![passive(72001085, 1990208, Some(0), 500)];
    state.set_field_by_name("party_gauge", Value::I32(0));
    let before = state.clone();
    runtime
        .trigger_lamps_after_action(
            &proto,
            &rules,
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
    runtime.passives = vec![passive(72001085, 1990368, Some(6), 200)];
    state.set_field_by_name("party_gauge", Value::I32(0));
    let before = state.clone();
    runtime
        .trigger_lamps_after_action(
            &proto,
            &rules,
            &mut state,
            &before,
            actor_id,
            skill,
            &[healed],
            true,
        )
        .unwrap();
    assert_eq!(i32_field(&state, "party_gauge"), Some(expected(200)));

    runtime.passives = vec![passive(72001085, 1990543, Some(2), 3_000)];
    state.set_field_by_name("party_gauge", Value::I32(0));
    runtime.acquired_panel_turn = 0;
    runtime
        .acquire_current_panel(&proto, &rules, &mut state)
        .unwrap();
    assert_eq!(i32_field(&state, "party_gauge"), Some(expected(3_000)));

    runtime.passives = vec![passive(120000292, 600000335, Some(0), 1_000)];
    state.set_field_by_name("party_gauge", Value::I32(0));
    let results = runtime
        .prepare_turn(&proto, &mut state, actor_id, b"turn-start", "tx", 1)
        .unwrap()
        .0;
    assert_eq!(i32_field(&state, "party_gauge"), Some(expected(1_000)));
    assert_eq!(
        message_i32_field(&results[0], "party_gauge_heal", "value"),
        Some(expected(1_000))
    );
    runtime
        .prepare_turn(&proto, &mut state, actor_id, b"turn-start", "tx", 1)
        .unwrap();
    assert_eq!(i32_field(&state, "party_gauge"), Some(expected(1_000)));
}

#[test]
fn shared_burst_gauge_rules_follow_events_without_duplicate_gains() {
    let catalog = registry().unwrap();
    let mut trigger_counts = BTreeMap::new();
    for rule in catalog.rules.iter().filter(|rule| {
        matches!(rule.id, 72001454 | 72001459 | 72001474)
            && rule.mode == "passive"
            && rule.owner_type == "ability"
    }) {
        *trigger_counts
            .entry(rule.trigger.as_deref().unwrap())
            .or_insert(0) += 1;
    }
    assert_eq!(
        trigger_counts,
        BTreeMap::from([
            ("action_after", 3),
            ("attacked", 3),
            ("heal_received", 1),
            ("no_damage_received", 6),
            ("panel_acquired", 1),
            ("party_action_after", 6),
            ("party_tool_after", 2),
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
    let actor = current_actor(&state).unwrap();
    let actor_id = member_id(&actor).unwrap();
    let actor_type = member_type(&actor).unwrap();
    let skill = rules
        .skills
        .iter()
        .find(|skill| skill.skill_effect_type == 1)
        .unwrap();
    let passive = |effect_id, ability_id, owner_index, value| Passive {
        source: actor_id,
        value,
        rule: rule_for_occurrence(
            effect_id,
            "passive",
            "ability",
            ability_id,
            owner_index,
        )
        .unwrap()
        .unwrap()
        .clone(),
        source_character_id: 0,
        source_type: actor_type,
    };
    let mut runtime = Runtime::default();
    runtime.prepare(&state, "triggered-burst-gauge").unwrap();

    runtime.passives = vec![passive(72001474, 1990474, Some(1), 1_500)];
    set_burst_gauge(&mut state, actor_id, 0);
    runtime
        .trigger_attack_after(&proto, &rules, &mut state, actor_id, skill, &[])
        .unwrap();
    assert_eq!(burst_gauge(&state, actor_id), 15);

    runtime.passives = vec![
        passive(72001474, 1990565, Some(0), 1_000),
        passive(72001474, 1990565, Some(1), 1_000),
    ];
    set_burst_gauge(&mut state, actor_id, 0);
    runtime
        .trigger_attack_after(&proto, &rules, &mut state, actor_id, skill, &[])
        .unwrap();
    assert_eq!(burst_gauge(&state, actor_id), 10);

    let hit = build_skill_result(
        &proto, actor_id, 1, 0, 0, 0, true, false, false, false, false, false, false,
    )
    .unwrap();
    runtime.passives = vec![passive(72001474, 1990504, Some(0), 1_000)];
    set_burst_gauge(&mut state, actor_id, 0);
    let before = state.clone();
    runtime
        .trigger_lamps_after_action(
            &proto,
            &rules,
            &mut state,
            &before,
            actor_id,
            skill,
            &[hit],
            true,
        )
        .unwrap();
    assert_eq!(burst_gauge(&state, actor_id), 10);

    let healed = build_skill_result(
        &proto, actor_id, 0, 0, 1, 0, false, false, false, false, false, false, false,
    )
    .unwrap();
    runtime.passives = vec![passive(72001474, 1990535, Some(0), 1_000)];
    set_burst_gauge(&mut state, actor_id, 0);
    let before = state.clone();
    runtime
        .trigger_lamps_after_action(
            &proto,
            &rules,
            &mut state,
            &before,
            actor_id,
            skill,
            &[healed],
            true,
        )
        .unwrap();
    assert_eq!(burst_gauge(&state, actor_id), 10);

    let no_damage = build_skill_result(
        &proto, actor_id, 0, 0, 0, 0, true, false, false, false, false, false, false,
    )
    .unwrap();
    runtime.passives = vec![passive(72001474, 1990471, Some(1), 3_000)];
    set_burst_gauge(&mut state, actor_id, 0);
    let before = state.clone();
    runtime
        .trigger_lamps_after_action(
            &proto,
            &rules,
            &mut state,
            &before,
            actor_id,
            skill,
            &[no_damage],
            true,
        )
        .unwrap();
    assert_eq!(burst_gauge(&state, actor_id), 30);

    runtime.passives = vec![passive(72001474, 1990543, Some(0), 5_000)];
    set_burst_gauge(&mut state, actor_id, 0);
    runtime
        .trigger_party_tool_effects(&proto, &rules, &mut state)
        .unwrap();
    assert_eq!(burst_gauge(&state, actor_id), 50);

    runtime.passives = vec![passive(72001474, 1990559, Some(0), 2_500)];
    set_burst_gauge(&mut state, actor_id, 0);
    runtime.acquired_panel_wave = 0;
    runtime.acquired_panel_turn = 0;
    runtime
        .acquire_current_panel(&proto, &rules, &mut state)
        .unwrap();
    assert_eq!(burst_gauge(&state, actor_id), 25);
}

#[test]
fn party_tool_modifier_requires_its_named_character_and_expires_with_the_rule() {
    let rule = rule_for_occurrence(6001208, "passive", "ability", 301303, Some(0))
        .unwrap()
        .unwrap()
        .clone();
    assert_eq!(
        (
            rule.operation.as_str(),
            rule.summary,
            rule.trigger.as_deref(),
            &rule.expiry,
            rule.duration,
            rule.source_character_ids.as_slice(),
        ),
        ("summary", 4, Some("party_tool_after"), &Expiry::Turn, 1, [60101].as_slice())
    );
    assert_eq!(
        rule_for(6001207, "catalog", "skill", 3020)
            .unwrap()
            .unwrap()
            .operation,
        "marker"
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
    let mut runtime = Runtime::default();
    runtime.prepare(&state, "party-tool-modifier").unwrap();
    runtime.passives.push(Passive {
        source: actor_id,
        value: 4_000,
        rule,
        source_character_id: 1,
        source_type: 0,
    });

    assert!(runtime
        .trigger_party_tool_effects(&proto, &rules, &mut state)
        .unwrap()
        .is_empty());
    runtime.passives[0].source_character_id = 60101;
    let results = runtime
        .trigger_party_tool_effects(&proto, &rules, &mut state)
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(runtime.instances.len(), 1);
    assert_eq!(runtime.instances[0].remaining, 1);
    let actor = message_list(&state, "members")
        .into_iter()
        .find(|member| member_id(member).ok() == Some(actor_id))
        .unwrap();
    assert_eq!(
        message_list(&actor, "state_change_summaries")
            .into_iter()
            .find(|summary| i32_field(summary, "id") == Some(4))
            .and_then(|summary| i32_field(&summary, "value")),
        Some(4_000)
    );
}
