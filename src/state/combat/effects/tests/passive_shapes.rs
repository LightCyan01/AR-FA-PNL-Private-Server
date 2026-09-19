use super::super::*;
use crate::state::combat::prelude::*;
use std::path::Path;

#[test]
fn passive_attack_shape_and_role_filters_follow_master_context() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let member = |character_id| {
        let mut ally = empty_message(&proto, "blend.model.BattleAlly").unwrap();
        ally.set_field_by_name("character_id", Value::I32(character_id));
        let mut member = empty_message(&proto, "blend.model.BattleMember").unwrap();
        member.set_field_by_name("member_id", Value::I32(1));
        member.set_field_by_name("type", Value::EnumNumber(0));
        member.set_field_by_name("ally", Value::Message(ally));
        member
    };

    let all_target = rule_for(95000020, "passive", "ability", 4990102)
        .unwrap()
        .unwrap()
        .clone();
    assert_eq!(
        (
            all_target.operation.as_str(),
            all_target.summary,
            all_target.skill_target_types.as_slice(),
        ),
        ("summary", 3, &[5][..])
    );
    let actor = member(0);
    let runtime = Runtime {
        passives: vec![Passive {
            source: 1,
            value: 700,
            rule: all_target,
            source_character_id: 0,
            source_type: 0,
        }],
        ..Runtime::default()
    };
    let skill = TutorialSkill {
        id: 1,
        skill_type: 1,
        skill_effect_type: 1,
        skill_power_type: 1,
        wait: 100,
        power: 100,
        break_power: 100,
        break_power_type: 1,
        attack_attributes: vec![1],
        skill_target_type: Some(5),
        effects: Vec::new(),
        limit_count: None,
        max_lamp: 0,
        require_command_value: false,
        skill_destination: None,
        state_change_application_rate: 10_000,
        hp_damage_bonus: None,
    };
    assert_eq!(runtime.contextual_summary(&actor, &skill, false, 3), 700);
    assert_eq!(
        runtime.contextual_summary(
            &actor,
            &TutorialSkill {
                skill_target_type: Some(3),
                ..skill
            },
            false,
            3,
        ),
        0
    );

    let attacker = rule_for(95000350, "passive", "ability", 4991248)
        .unwrap()
        .unwrap();
    let attacker_id = attacker.source_character_ids[0];
    let attacker_member = member(attacker_id);
    let other_member = member(i32::MAX);
    assert!(selected(attacker, &attacker_member, &attacker_member, &[]));
    assert!(!selected(attacker, &other_member, &other_member, &[]));
}

#[test]
fn broken_target_passives_keep_source_filters_and_runtime_gate() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rule = rule_for(72500122, "passive", "ability", 1980168)
        .unwrap()
        .unwrap()
        .clone();
    assert!(rule.target_broken);
    assert!(rule.source_character_ids.contains(&60201));
    let mut ally = empty_message(&proto, "blend.model.BattleAlly").unwrap();
    ally.set_field_by_name("character_id", Value::I32(60201));
    let mut actor = empty_message(&proto, "blend.model.BattleMember").unwrap();
    actor.set_field_by_name("member_id", Value::I32(1));
    actor.set_field_by_name("type", Value::EnumNumber(0));
    actor.set_field_by_name("ally", Value::Message(ally));
    let mut enemy = empty_message(&proto, "blend.model.BattleMember").unwrap();
    let mut enemy_status = empty_message(&proto, "blend.model.BattleEnemy").unwrap();
    enemy_status.set_field_by_name("is_broken", Value::Bool(false));
    enemy.set_field_by_name("enemy", Value::Message(enemy_status));
    let skill = TutorialSkill {
        id: 1,
        skill_type: 1,
        skill_effect_type: 1,
        skill_power_type: 1,
        wait: 100,
        power: 100,
        break_power: 100,
        break_power_type: 1,
        attack_attributes: vec![7],
        skill_target_type: Some(3),
        effects: Vec::new(),
        limit_count: None,
        max_lamp: 0,
        require_command_value: false,
        skill_destination: None,
        state_change_application_rate: 10_000,
        hp_damage_bonus: None,
    };
    let runtime = Runtime {
        passives: vec![Passive {
            source: 1,
            value: 10_000,
            rule,
            source_character_id: 60201,
            source_type: 0,
        }],
        ..Runtime::default()
    };
    assert_eq!(
        runtime.contextual_summary_against(&actor, Some(&enemy), &skill, false, 1),
        0
    );
    let mut enemy_status = member_status(&enemy, "enemy").unwrap();
    enemy_status.set_field_by_name("is_broken", Value::Bool(true));
    enemy.set_field_by_name("enemy", Value::Message(enemy_status));
    assert_eq!(
        runtime.contextual_summary_against(&actor, Some(&enemy), &skill, false, 1),
        10_000
    );
}
