use super::super::*;
use crate::state::combat::prelude::*;
use std::path::Path;

#[test]
fn incoming_reduction_uses_the_attackers_hp_threshold() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let member = |id, kind, hp| {
        let mut resistance = empty_message(&proto, "blend.model.BattleResistance").unwrap();
        resistance.set_field_by_name("slashing", Value::I32(0));
        let mut member = empty_message(&proto, "blend.model.BattleMember").unwrap();
        member.set_field_by_name("member_id", Value::I32(id));
        member.set_field_by_name("type", Value::EnumNumber(kind));
        member.set_field_by_name("hp", Value::I32(hp));
        member.set_field_by_name("max_hp", Value::I32(100));
        member.set_field_by_name("resistance", Value::Message(resistance));
        member.set_field_by_name("state_changes", Value::List(Vec::new()));
        member.set_field_by_name("state_change_summaries", Value::List(Vec::new()));
        member
    };
    let target = member(2, 1, 100);
    let mut attacker = member(1, 0, 50);
    let runtime = Runtime {
        passives: vec![Passive {
            source: 2,
            value: 3_000,
            rule: rule_for(880_047_008, "passive", "", 0)
                .unwrap()
                .unwrap()
                .clone(),
            source_character_id: 0,
            source_type: 1,
        }],
        ..Runtime::default()
    };
    let skill = TutorialSkill {
        id: 1,
        skill_type: 1,
        skill_effect_type: 1,
        skill_power_type: 1,
        wait: 0,
        power: 100,
        break_power: 0,
        break_power_type: 0,
        attack_attributes: vec![1],
        skill_target_type: Some(3),
        effects: Vec::new(),
        limit_count: None,
        max_lamp: 0,
        require_command_value: false,
        skill_destination: None,
        state_change_application_rate: 10_000,
        hp_damage_bonus: None,
    };
    assert_eq!(
        incoming_multiplier_for_skill(&target, &skill, Some(&runtime), Some(&attacker), false)
            .unwrap(),
        7_000
    );
    attacker.set_field_by_name("hp", Value::I32(51));
    assert_eq!(
        incoming_multiplier_for_skill(&target, &skill, Some(&runtime), Some(&attacker), false)
            .unwrap(),
        10_000
    );
}
