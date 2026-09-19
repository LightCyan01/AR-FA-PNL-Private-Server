use super::super::*;
use crate::state::combat::prelude::*;
use std::path::Path;

#[test]
fn sacrifice_limits_panels_and_preserves_individual_buff_lifetimes() {
    let reduction = rule_for(91001695, "active", "skill", 12002495)
        .unwrap()
        .unwrap();
    let tag = rule_for(91001698, "active", "skill", 12002495)
        .unwrap()
        .unwrap();
    assert_eq!(
        (reduction.operation.as_str(), reduction.stack_limit),
        ("taken_down", 2)
    );
    assert!(!tag.target_character_ids.is_empty());
    assert!(reduction
        .target_character_ids
        .iter()
        .all(|id| !tag.target_character_ids.contains(id)));
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
    let mut runtime = start.effects;
    runtime.passives.clear();
    runtime.instances.clear();
    runtime.unsupported.clear();
    let mut members = message_list(&state, "members");
    let target = member_id(
        members
            .iter()
            .find(|m| member_type(m).ok() == Some(1))
            .unwrap(),
    )
    .unwrap();
    let source = members
        .iter_mut()
        .find(|m| member_type(m).ok() == Some(0))
        .unwrap();
    let source_id = member_id(source).unwrap();
    let mut ally = member_status(source, "ally").unwrap();
    ally.set_field_by_name(
        "character_id",
        Value::I32(reduction.target_character_ids[0]),
    );
    let mut skills = message_list(&ally, "skills");
    skills[0].set_field_by_name("skill_id", Value::I32(12002495));
    ally.set_field_by_name(
        "skills",
        Value::List(skills.into_iter().map(Value::Message).collect()),
    );
    source.set_field_by_name("ally", Value::Message(ally));
    state.set_field_by_name(
        "members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );
    let units = [(source_id, 1), (target, 1), (source_id, 2), (target, 2)]
        .into_iter()
        .enumerate()
        .map(|(wait, (id, number))| {
            Value::Message(build_timeline_unit(&proto, id, number, wait as i32).unwrap())
        })
        .collect();
    state.set_field_by_name("timeline_units", Value::List(units));
    let effects = &rules
        .skills
        .iter()
        .find(|skill| skill.id == 12002495)
        .unwrap()
        .effects;
    let mut hit = empty_message(&proto, "blend.model.BattleSkillResult").unwrap();
    hit.set_field_by_name("target_id", Value::I32(source_id));
    runtime.refresh(&proto, &mut state).unwrap();
    let source = message_list(&state, "members")
        .into_iter()
        .find(|m| member_id(m).ok() == Some(source_id))
        .unwrap();
    let base_incoming = incoming_multiplier_with_runtime(&source, 2, Some(&runtime));
    for action in 1..=4 {
        set_timeline_panels(&proto, &mut state, &[11, 12, 14, 11], 4, 1).unwrap();
        runtime
            .apply_for_action_with_rules(
                &proto,
                &rules,
                &mut state,
                source_id,
                12002495,
                effects,
                &[target],
                true,
                "after",
                None,
                10_000,
                b"sacrifice-test",
                &start.start_txid,
                action,
            )
            .unwrap();
        let panels: Vec<_> = message_list(&state, "timeline_panels")
            .iter()
            .map(|p| optional_i32_field(p, "panel_id").unwrap_or(11))
            .collect();
        assert_eq!(
            panels,
            if action <= 3 {
                vec![11, 36, 14, 36]
            } else {
                vec![11, 12, 14, 11]
            }
        );
        let stacks: Vec<_> = runtime
            .instances
            .iter()
            .filter(|instance| instance.rule.id == 91001695)
            .map(|i| i.remaining)
            .collect();
        assert_eq!(stacks, if action == 1 { vec![2] } else { vec![1, 2] });
        let member = message_list(&state, "members")
            .into_iter()
            .find(|m| member_id(m).ok() == Some(source_id))
            .unwrap();
        assert_eq!(
            incoming_multiplier_with_runtime(&member, 2, Some(&runtime)),
            (base_incoming - 2_000 * stacks.len() as i64).max(0),
        );
        if action == 1 || action == 3 {
            runtime.expire(target, &[hit.clone()], true, true);
        }
        if action == 2 {
            runtime = serde_json::from_slice(&serde_json::to_vec(&runtime).unwrap()).unwrap();
        }
        if action == 3 {
            state.set_field_by_name("wave", Value::I32(2));
            runtime.refresh(&proto, &mut state).unwrap();
        }
    }
    assert_eq!(runtime.limited_effect_uses[&source_id][&91001672], 3);
    assert!(runtime.unsupported.is_empty());
    let source = message_list(&state, "members")
        .into_iter()
        .find(|m| member_id(m).ok() == Some(source_id))
        .unwrap();
    assert!(
        message_list(&member_status(&source, "ally").unwrap(), "skills")
            .iter()
            .any(|skill| i32_field(skill, "skill_id") == Some(12002705))
    );
}
