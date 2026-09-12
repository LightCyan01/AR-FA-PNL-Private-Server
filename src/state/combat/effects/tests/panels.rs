use super::super::*;
use crate::state::combat::prelude::*;
use std::path::Path;

#[test]
fn panel_disable_suppresses_non_burst_panel_for_one_target_turn() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_gameplay_rules().unwrap();
    let fresh = load_fresh_rules().unwrap();
    let resources = reduce_talk_event(
        &proto,
        &fresh,
        &rules,
        starter_resources(&proto, &fresh).unwrap(),
        101001001,
        1,
    )
    .unwrap()
    .resources;
    let start = reduce_battle_start(&proto, &rules, resources, 101001002, 1).unwrap();
    let mut state = start.state;
    let source_id = message_list(&state, "members")
        .iter()
        .find(|member| member_type(member).ok() == Some(0))
        .and_then(|member| i32_field(member, "member_id"))
        .unwrap();
    let target_id = message_list(&state, "members")
        .iter()
        .find(|member| member_type(member).ok() == Some(1))
        .and_then(|member| i32_field(member, "member_id"))
        .unwrap();
    let units = message_list(&state, "timeline_units")
        .into_iter()
        .map(|mut unit| {
            unit.set_field_by_name(
                "wait",
                Value::I32(if i32_field(&unit, "member_id") == Some(target_id) {
                    0
                } else {
                    1_000
                }),
            );
            Value::Message(unit)
        })
        .collect();
    state.set_field_by_name("timeline_units", Value::List(units));
    set_timeline_panels(&proto, &mut state, &[12], 1, 1).unwrap();
    assert_eq!(battle_panel_multiplier(&state), (140, 100));

    let mut runtime = start.effects;
    runtime.passives.clear();
    runtime.instances.clear();
    runtime.managed.clear();
    runtime.refresh(&proto, &mut state).unwrap();
    let results = runtime
        .apply(
            &proto,
            &mut state,
            source_id,
            &[TutorialSkillEffect {
                id: 91001256,
                value: 0,
            }],
            &[target_id],
            true,
            "after",
            None,
        )
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(battle_panel_multiplier(&state), (100, 100));

    runtime.expire(target_id, &[], true, false);
    runtime.refresh(&proto, &mut state).unwrap();
    assert_eq!(battle_panel_multiplier(&state), (140, 100));
}

#[test]
fn offensive_panel_policy_matches_master_data() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_gameplay_rules().unwrap();
    let fresh = load_fresh_rules().unwrap();
    let resources = reduce_talk_event(
        &proto,
        &fresh,
        &rules,
        starter_resources(&proto, &fresh).unwrap(),
        101001001,
        1,
    )
    .unwrap()
    .resources;
    let mut state = reduce_battle_start(&proto, &rules, resources, 101001002, 1)
        .unwrap()
        .state;
    for (panel_id, damage, break_damage, critical) in [
        (11, (100, 100), 100, false),
        (12, (140, 100), 100, false),
        (13, (60, 100), 100, false),
        (22, (140, 100), 140, false),
        (24, (200, 100), 100, false),
        (26, (40, 100), 100, false),
        (30, (100, 100), 100, true),
        (39, (100, 100), 140, false),
    ] {
        set_timeline_panels(&proto, &mut state, &[panel_id], 1, 1).unwrap();
        assert_eq!(battle_panel_multiplier(&state), damage, "panel {panel_id}");
        assert_eq!(
            battle_panel_break_multiplier(&state),
            break_damage,
            "panel {panel_id}"
        );
        assert_eq!(
            battle_panel_guarantees_critical(&state),
            critical,
            "panel {panel_id}"
        );
    }
}

#[test]
fn acquisition_panels_apply_once_and_expire_on_hit() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_gameplay_rules().unwrap();
    let fresh = load_fresh_rules().unwrap();
    let resources = reduce_talk_event(
        &proto,
        &fresh,
        &rules,
        starter_resources(&proto, &fresh).unwrap(),
        101001001,
        1,
    )
    .unwrap()
    .resources;
    let start = reduce_battle_start(&proto, &rules, resources, 101001002, 1).unwrap();
    let mut state = start.state;
    let actor_id = member_id(&current_actor(&state).unwrap()).unwrap();
    let mut runtime = start.effects;

    set_timeline_panels(&proto, &mut state, &[33], 1, 101).unwrap();
    runtime.acquire_current_panel(&mut state).unwrap();
    assert_eq!(runtime.panel_damage_taken.get(&actor_id), Some(&-4_000));
    runtime.acquire_current_panel(&mut state).unwrap();
    assert_eq!(runtime.panel_damage_taken.get(&actor_id), Some(&-4_000));
    let actor = message_list(&state, "members")
        .into_iter()
        .find(|member| member_id(member).ok() == Some(actor_id))
        .unwrap();
    assert_eq!(
        incoming_multiplier_with_runtime(&actor, 1, Some(&runtime)),
        (incoming_multiplier_with_runtime(&actor, 1, None) - 4_000).max(0)
    );
    let mut hit = empty_message(&proto, "blend.model.BattleSkillResult").unwrap();
    hit.set_field_by_name("target_id", Value::I32(actor_id));
    runtime.expire(0, &[hit], true, true);
    assert!(!runtime.panel_damage_taken.contains_key(&actor_id));

    set_timeline_panels(&proto, &mut state, &[36], 1, 102).unwrap();
    runtime.acquire_current_panel(&mut state).unwrap();
    assert_eq!(runtime.panel_damage_taken.get(&actor_id), Some(&4_000));

    let mut members = message_list(&state, "members");
    let actor = members
        .iter_mut()
        .find(|member| member_id(member).ok() == Some(actor_id))
        .unwrap();
    let max_hp = i32_field(actor, "max_hp").unwrap();
    actor.set_field_by_name("hp", Value::I32(1));
    state.set_field_by_name(
        "members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );
    set_timeline_panels(&proto, &mut state, &[42], 1, 103).unwrap();
    runtime.acquire_current_panel(&mut state).unwrap();
    let actor = message_list(&state, "members")
        .into_iter()
        .find(|member| member_id(member).ok() == Some(actor_id))
        .unwrap();
    assert_eq!(i32_field(&actor, "hp"), Some((1 + max_hp / 4).min(max_hp)));
}

#[test]
fn ally_panel_conversion_changes_only_future_eligible_ally_panels() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_gameplay_rules().unwrap();
    let fresh = load_fresh_rules().unwrap();
    let resources = reduce_talk_event(
        &proto,
        &fresh,
        &rules,
        starter_resources(&proto, &fresh).unwrap(),
        101001001,
        1,
    )
    .unwrap()
    .resources;
    let start = reduce_battle_start(&proto, &rules, resources, 101001002, 1).unwrap();
    let mut state = start.state;
    let source_id = message_list(&state, "members")
        .iter()
        .find(|member| member_type(member).ok() == Some(0))
        .and_then(|member| i32_field(member, "member_id"))
        .unwrap();
    let enemy_id = message_list(&state, "members")
        .iter()
        .find(|member| member_type(member).ok() == Some(1))
        .and_then(|member| i32_field(member, "member_id"))
        .unwrap();
    let units = [(source_id, 1), (enemy_id, 1), (source_id, 2), (enemy_id, 2)]
        .into_iter()
        .enumerate()
        .map(|(wait, (member_id, number))| {
            Value::Message(build_timeline_unit(&proto, member_id, number, wait as i32).unwrap())
        })
        .collect();
    state.set_field_by_name("timeline_units", Value::List(units));
    set_timeline_panels(&proto, &mut state, &[13, 11, 11, 14], 4, 1).unwrap();
    let panel_context = state.clone();
    let mut consumed_units = message_list(&state, "timeline_units");
    consumed_units.remove(0);
    state.set_field_by_name(
        "timeline_units",
        Value::List(consumed_units.into_iter().map(Value::Message).collect()),
    );

    let mut runtime = start.effects;
    runtime.passives.clear();
    runtime.instances.clear();
    runtime.managed.clear();
    runtime.refresh(&proto, &mut state).unwrap();
    let results = runtime
        .apply(
            &proto,
            &mut state,
            source_id,
            &[TutorialSkillEffect {
                id: 91001215,
                value: 1_000,
            }],
            &[source_id],
            true,
            "after",
            Some(&panel_context),
        )
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(
        message_list(&results[0], "overwritten_timeline_panels")
            .iter()
            .map(|panel| (
                i32_field(panel, "turn").unwrap(),
                optional_i32_field(panel, "panel_id").unwrap_or(11),
            ))
            .collect::<Vec<_>>(),
        vec![(3, 12)]
    );
    assert_eq!(
        message_list(&state, "timeline_panels")
            .iter()
            .map(|panel| optional_i32_field(panel, "panel_id").unwrap_or(11))
            .collect::<Vec<_>>(),
        vec![13, 11, 12, 14]
    );
}

#[test]
fn next_enemy_panel_conversion_uses_pre_action_timeline() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_gameplay_rules().unwrap();
    let fresh = load_fresh_rules().unwrap();
    let resources = reduce_talk_event(
        &proto,
        &fresh,
        &rules,
        starter_resources(&proto, &fresh).unwrap(),
        101001001,
        1,
    )
    .unwrap()
    .resources;
    let start = reduce_battle_start(&proto, &rules, resources, 101001002, 1).unwrap();
    let mut state = start.state;
    let source_id = message_list(&state, "members")
        .iter()
        .find(|member| member_type(member).ok() == Some(0))
        .and_then(|member| i32_field(member, "member_id"))
        .unwrap();
    let enemy = message_list(&state, "members")
        .into_iter()
        .find(|member| member_type(member).ok() == Some(1))
        .unwrap();
    let enemy_id = i32_field(&enemy, "member_id").unwrap();
    let next_enemy_id = enemy_id + 1_000;
    let mut next_enemy = enemy.clone();
    next_enemy.set_field_by_name("member_id", Value::I32(next_enemy_id));
    let mut members = message_list(&state, "members");
    members.push(next_enemy);
    state.set_field_by_name(
        "members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );
    let units = [
        (source_id, 1),
        (enemy_id, 1),
        (enemy_id, 2),
        (next_enemy_id, 1),
    ]
    .into_iter()
    .enumerate()
    .map(|(wait, (member_id, number))| {
        Value::Message(build_timeline_unit(&proto, member_id, number, wait as i32).unwrap())
    })
    .collect();
    state.set_field_by_name("timeline_units", Value::List(units));
    set_timeline_panels(&proto, &mut state, &[11, 14, 12, 11], 4, 1).unwrap();
    let panel_context = state.clone();
    let mut consumed_units = message_list(&state, "timeline_units");
    consumed_units.remove(0);
    state.set_field_by_name(
        "timeline_units",
        Value::List(consumed_units.into_iter().map(Value::Message).collect()),
    );

    let mut runtime = start.effects;
    runtime.passives.clear();
    runtime.instances.clear();
    runtime.managed.clear();
    runtime.refresh(&proto, &mut state).unwrap();
    let results = runtime
        .apply(
            &proto,
            &mut state,
            source_id,
            &[TutorialSkillEffect {
                id: 91001249,
                value: 5_000,
            }],
            &[enemy_id, next_enemy_id],
            true,
            "after",
            Some(&panel_context),
        )
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(
        message_list(&results[0], "overwritten_timeline_panels")
            .iter()
            .map(|panel| (
                i32_field(panel, "turn").unwrap(),
                optional_i32_field(panel, "panel_id").unwrap_or(11),
            ))
            .collect::<Vec<_>>(),
        vec![(2, 36), (3, 36)]
    );
    assert_eq!(
        message_list(&state, "timeline_panels")
            .iter()
            .map(|panel| optional_i32_field(panel, "panel_id").unwrap_or(11))
            .collect::<Vec<_>>(),
        vec![11, 36, 36, 11]
    );
}
