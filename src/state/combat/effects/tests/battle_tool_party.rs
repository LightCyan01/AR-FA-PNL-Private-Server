use super::super::*;
use crate::state::combat::prelude::*;
use std::path::Path;

fn battle_member(
    proto: &ProtoRegistry,
    member_id: i32,
    character_id: Option<i32>,
    member_type: i32,
) -> DynamicMessage {
    let mut member = empty_message(proto, "blend.model.BattleMember").unwrap();
    member.set_field_by_name("member_id", Value::I32(member_id));
    member.set_field_by_name("type", Value::EnumNumber(member_type));
    member.set_field_by_name("is_alive", Value::Bool(true));
    member.set_field_by_name("hp", Value::I32(10_000));
    member.set_field_by_name("max_hp", Value::I32(10_000));
    member.set_field_by_name(
        "resistance",
        Value::Message(empty_message(proto, "blend.model.BattleResistance").unwrap()),
    );
    if let Some(character_id) = character_id {
        let mut ally = empty_message(proto, "blend.model.BattleAlly").unwrap();
        ally.set_field_by_name("character_id", Value::I32(character_id));
        member.set_field_by_name("ally", Value::Message(ally));
    }
    member
}

fn family_passive(kind: &str, source: i32, character_id: i32) -> Passive {
    let catalog = registry().unwrap();
    let family = catalog
        .battle_tool_party_rules
        .iter()
        .find(|family| family.kind == kind)
        .unwrap();
    let rule = catalog
        .rules
        .iter()
        .find(|rule| {
            rule.id == family.anchor_effect_id
                && rule.mode == "passive"
                && rule.owner_type == "ability"
                && rule.owner_id == family.owner_id
        })
        .unwrap()
        .clone();
    Passive {
        source,
        value: family.anchor_value,
        rule,
        source_character_id: character_id,
        source_type: 0,
    }
}

#[test]
fn academy_field_bonuses_scale_tool_damage_critical_and_gauge() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_gameplay_rules().unwrap();
    let catalog = registry().unwrap();
    let expected = [
        ("gauge", 26241001, 126241053, 500, 100, 5, 1_000),
        ("damage", 26241002, 126241051, 15_000, 3_000, 5, 30_000),
        ("critical", 26241003, 126241052, 5_000, 1_000, 5, 10_000),
    ];
    assert_eq!(catalog.battle_tool_party_rules.len(), expected.len());
    let tag_id = catalog.battle_tool_party_rules[0].tag_id;
    for (kind, owner, anchor, anchor_value, per_member, count_cap, maximum) in expected {
        let family = catalog
            .battle_tool_party_rules
            .iter()
            .find(|family| family.kind == kind)
            .unwrap();
        assert_eq!(
            (
                family.owner_id,
                family.anchor_effect_id,
                family.anchor_value,
                family.tag_id,
                family.per_member,
                family.count_cap,
                family.maximum,
            ),
            (
                owner,
                anchor,
                anchor_value,
                tag_id,
                per_member,
                count_cap,
                maximum
            )
        );
        assert_eq!(
            catalog
                .rules
                .iter()
                .filter(|rule| rule.owner_id == owner && rule.mode == "passive")
                .count(),
            1
        );
        assert_eq!(
            catalog
                .rules
                .iter()
                .filter(|rule| rule.owner_id == owner && rule.mode == "catalog")
                .count(),
            11
        );
    }

    let tagged = rules
        .battle_characters
        .iter()
        .filter(|character| character.tag_ids.contains(&tag_id))
        .map(|character| character.id)
        .take(5)
        .collect::<Vec<_>>();
    assert_eq!(tagged.len(), 5);
    let mut members = tagged
        .iter()
        .enumerate()
        .map(|(index, character_id)| {
            battle_member(&proto, index as i32 + 1, Some(*character_id), 0)
        })
        .collect::<Vec<_>>();
    members.push(battle_member(&proto, 11, None, 1));

    let mut runtime = Runtime::default();
    runtime.passives = ["gauge", "damage", "critical"]
        .map(|kind| family_passive(kind, 1, tagged[0]))
        .to_vec();
    assert_eq!(
        runtime
            .battle_tool_party_bonus(&rules, &members, "gauge")
            .unwrap(),
        1_000
    );
    assert_eq!(
        runtime
            .battle_tool_party_bonus(&rules, &members, "damage")
            .unwrap(),
        30_000
    );
    assert_eq!(
        runtime
            .battle_tool_party_bonus(&rules, &members, "critical")
            .unwrap(),
        10_000
    );
    assert_eq!(
        Runtime::default()
            .battle_tool_party_bonus(&rules, &members, "damage")
            .unwrap(),
        0
    );

    let mut dead_source = members.clone();
    dead_source[0].set_field_by_name("is_alive", Value::Bool(false));
    assert_eq!(
        runtime
            .battle_tool_party_bonus(&rules, &dead_source, "damage")
            .unwrap(),
        30_000
    );

    let mut gauge_state = empty_message(&proto, "blend.model.BattleState").unwrap();
    gauge_state.set_field_by_name("party_gauge", Value::I32(0));
    assert_eq!(add_party_gauge(&mut gauge_state, 1_000).unwrap(), 100);
    assert_eq!(i32_field(&gauge_state, "party_gauge"), Some(100));

    let tool_rule = rules
        .battle_tools
        .iter()
        .find(|tool| {
            rules.skills.iter().any(|skill| {
                skill.id == tool.skill_id
                    && skill.skill_effect_type == 1
                    && !skill.attack_attributes.is_empty()
            })
        })
        .unwrap();
    let mut skill = rule_skill(&rules, tool_rule.skill_id).unwrap().clone();
    skill.power = 1_000;
    skill.attack_attributes = vec![1];
    skill.effects.clear();
    let tools = [BattlePartyTool {
        tool_id: tool_rule.id,
        usage_count: 1,
        traits: Vec::new(),
    }];
    let target = members.last().unwrap();
    assert_eq!(
        policy_tool_damage(&rules, &tools, &members, target, &skill, None, false, false, 10_000)
            .unwrap(),
        100
    );
    assert_eq!(
        policy_tool_damage(
            &rules,
            &tools,
            &members,
            target,
            &skill,
            Some(&runtime),
            false,
            false,
            10_000,
        )
        .unwrap(),
        400
    );

    let mut state = empty_message(&proto, "blend.model.BattleState").unwrap();
    state.set_field_by_name(
        "members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );
    state.set_field_by_name(
        "timeline_units",
        Value::List(vec![Value::Message(
            build_timeline_unit(&proto, 1, 1, 0).unwrap(),
        )]),
    );
    let mut without_owner = state.clone();
    let no_owner = Runtime::default();
    let (plain, _, _) = apply_attack_results(
        &proto,
        &rules,
        &mut without_owner,
        1,
        0,
        &skill,
        &[],
        None,
        &no_owner,
        &[11],
        (100, 100),
        Some(&tools),
        false,
        false,
        b"academy-family",
        "plain",
        1,
    )
    .unwrap();
    assert!(!bool_field(&plain[0], "is_critical"));
    assert_eq!(
        message_i64_field(&plain[0], "hp_damage", "value"),
        Some(100)
    );

    let (boosted, _, _) = apply_attack_results(
        &proto,
        &rules,
        &mut state,
        1,
        0,
        &skill,
        &[],
        None,
        &runtime,
        &[11],
        (100, 100),
        Some(&tools),
        false,
        false,
        b"academy-family",
        "boosted",
        1,
    )
    .unwrap();
    assert!(bool_field(&boosted[0], "is_critical"));
    let variance =
        10_000 + deterministic_roll(b"academy-family", "boosted", 1, b"variance", 11, 0) % 101;
    let expected_damage = policy_tool_damage(
        &rules,
        &tools,
        &message_list(&state, "members"),
        message_list(&state, "members").last().unwrap(),
        &skill,
        Some(&runtime),
        false,
        true,
        variance,
    )
    .unwrap();
    assert_eq!(
        message_i64_field(&boosted[0], "hp_damage", "value"),
        Some(expected_damage)
    );
    assert_eq!(
        message_i64_field(&boosted[0], "hp_damage_for_critical", "value"),
        Some(expected_damage)
    );
}
