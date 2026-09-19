use super::super::*;
use crate::state::combat::prelude::*;
use std::collections::BTreeSet;
use std::path::Path;

fn member(
    proto: &ProtoRegistry,
    member_id: i32,
    character_id: i32,
    attack: i32,
    magic: i32,
) -> DynamicMessage {
    let mut status = empty_message(proto, "blend.model.BattleCharacterStatus").unwrap();
    for (field, value) in [
        ("attack", attack),
        ("defense", 100),
        ("hp", 100),
        ("magic", magic),
        ("mental", 100),
        ("speed", 100),
    ] {
        status.set_field_by_name(field, Value::I32(value));
    }
    let mut resistance = empty_message(proto, "blend.model.BattleResistance").unwrap();
    for field in [
        "slashing",
        "impact",
        "piercing",
        "fire",
        "ice",
        "lightning",
        "wind",
    ] {
        resistance.set_field_by_name(field, Value::I32(0));
    }
    let mut ally = empty_message(proto, "blend.model.BattleAlly").unwrap();
    ally.set_field_by_name("character_id", Value::I32(character_id));
    let mut member = empty_message(proto, "blend.model.BattleMember").unwrap();
    member.set_field_by_name("member_id", Value::I32(member_id));
    member.set_field_by_name("type", Value::EnumNumber(0));
    member.set_field_by_name("is_alive", Value::Bool(true));
    member.set_field_by_name("hp", Value::I32(100));
    member.set_field_by_name("max_hp", Value::I32(100));
    member.set_field_by_name("ally", Value::Message(ally));
    member.set_field_by_name("current_status", Value::Message(status.clone()));
    member.set_field_by_name("initial_status", Value::Message(status));
    member.set_field_by_name("resistance", Value::Message(resistance));
    member.set_field_by_name("state_changes", Value::List(Vec::new()));
    member.set_field_by_name("state_change_summaries", Value::List(Vec::new()));
    member
}

#[test]
fn post_skill_support_buffs_select_allies_and_highest_stats_once() {
    let rule = |effect_id, ability_id| {
        rule_for_occurrence(effect_id, "passive", "ability", ability_id, Some(0))
            .unwrap()
            .unwrap()
            .clone()
    };
    let all = rule(6000333, 300342);
    let physical = rule(6000911, 301014);
    let weak = rule(6000944, 301016);
    let burst = rule(6000942, 300998);
    for current in [&all, &physical, &weak, &burst] {
        assert_eq!(
            (
                current.operation.as_str(),
                current.summary,
                current.state_id,
                current.trigger.as_deref(),
                &current.expiry,
                current.duration,
            ),
            ("summary", 1, 50001, Some("skill_after"), &Expiry::Turn, 1)
        );
        assert!(!current.source_character_ids.is_empty());
    }
    assert_eq!(all.target, "allies");
    assert_eq!(physical.attack_attributes, [1, 2, 3]);
    assert_eq!(
        (weak.target.as_str(), weak.weak_only, weak.stack_limit),
        ("highest_magic_ally", true, 1)
    );
    assert_eq!(
        (
            burst.target.as_str(),
            burst.skill_types.as_slice(),
            burst.stack_limit
        ),
        ("highest_attack_ally", &[3][..], 1)
    );

    let family_ids = [
        6000333, 6000350, 6000351, 6000911, 6000922, 6000932, 6000942, 6000943, 6000944, 6000945,
        6001039, 6001040, 6001060, 6001061, 6001158, 6001162,
    ];
    assert_eq!(
        registry()
            .unwrap()
            .rules
            .iter()
            .filter(|candidate| {
                candidate.mode == "passive"
                    && candidate.owner_type == "ability"
                    && family_ids.contains(&candidate.id)
            })
            .count(),
        20
    );

    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_gameplay_rules().unwrap();
    let skill = TutorialSkill {
        id: 1,
        skill_type: 1,
        skill_effect_type: 1,
        skill_power_type: 1,
        wait: 100,
        power: 100,
        break_power: 0,
        break_power_type: 1,
        attack_attributes: vec![5],
        skill_target_type: Some(3),
        effects: Vec::new(),
        limit_count: None,
        max_lamp: 0,
        require_command_value: false,
        skill_destination: None,
        state_change_application_rate: 10_000,
        hp_damage_bonus: None,
    };
    let hit = build_skill_result(
        &proto, 11, 1, 0, 0, 0, true, false, false, false, false, false, false,
    )
    .unwrap();
    for (current, value, expected_targets) in [
        (&all, 200, BTreeSet::from([1, 2, 3])),
        (&physical, 400, BTreeSet::from([1, 2, 3])),
        (&weak, 1_800, BTreeSet::from([2])),
        (&burst, 1_800, BTreeSet::from([3])),
    ] {
        let source_character_id = current.source_character_ids[0];
        let mut state = empty_message(&proto, "blend.model.BattleState").unwrap();
        state.set_field_by_name(
            "members",
            Value::List(vec![
                Value::Message(member(&proto, 1, source_character_id, 100, 100)),
                Value::Message(member(&proto, 2, 2, 100, 300)),
                Value::Message(member(&proto, 3, 3, 300, 100)),
            ]),
        );
        let mut runtime = Runtime::default();
        runtime.prepare(&state, "post-skill-support").unwrap();
        runtime.passives.push(Passive {
            source: 1,
            value,
            rule: current.clone(),
            source_character_id,
            source_type: 0,
        });
        for iteration in 1..=2 {
            let results = runtime
                .trigger_attack_after(
                    &proto,
                    &rules,
                    &mut state,
                    1,
                    &skill,
                    std::slice::from_ref(&hit),
                )
                .unwrap();
            assert_eq!(
                results.len(),
                expected_targets.len(),
                "application {iteration}"
            );
        }
        assert_eq!(
            runtime
                .instances
                .iter()
                .map(|instance| instance.target)
                .collect::<BTreeSet<_>>(),
            expected_targets
        );

        let mut support_skill = skill.clone();
        support_skill.skill_effect_type = 2;
        assert_eq!(
            runtime
                .trigger_attack_after(&proto, &rules, &mut state, 1, &support_skill, &[])
                .unwrap()
                .len(),
            expected_targets.len()
        );
    }
}
