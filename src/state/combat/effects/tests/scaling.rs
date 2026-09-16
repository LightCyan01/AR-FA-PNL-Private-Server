use super::super::*;
use crate::state::combat::prelude::*;
use std::path::Path;

#[test]
fn skill_damage_curves_follow_live_count_hp_and_direction() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_gameplay_rules().unwrap();
    let skill = |id| rules.skills.iter().find(|skill| skill.id == id).unwrap();
    let mut source = empty_message(&proto, "blend.model.BattleMember").unwrap();
    source.set_field_by_name("hp", Value::I32(20));
    source.set_field_by_name("max_hp", Value::I32(100));
    source.set_field_by_name(
        "resistance",
        Value::Message(empty_message(&proto, "blend.model.BattleResistance").unwrap()),
    );
    assert_eq!(
        rule_for(91000949, "catalog", "skill", 11000286)
            .unwrap()
            .unwrap()
            .operation,
        "scaling_metadata"
    );
    assert_eq!(
        rule_for(91000943, "catalog", "skill", 12000111)
            .unwrap()
            .unwrap()
            .operation,
        "scaling_metadata"
    );

    assert_eq!(
        scaled_skill_damage(skill(11000286), &source, 1).unwrap(),
        1_000
    );
    assert_eq!(
        scaled_skill_damage(skill(11000286), &source, 4).unwrap(),
        2_000
    );
    assert_eq!(
        scaled_skill_damage(skill(11000511), &source, 1).unwrap(),
        500
    );
    assert_eq!(
        scaled_skill_damage(skill(12000111), &source, 1).unwrap(),
        2_500
    );
    assert_eq!(
        scaled_skill_damage(skill(12002546), &source, 1).unwrap(),
        3_000
    );
    assert_eq!(
        scaled_skill_damage(skill(12002546), &source, 4).unwrap(),
        12_000
    );
    assert_eq!(
        scaled_skill_damage(skill(14002426), &source, 1).unwrap(),
        5_000
    );
    assert_eq!(
        instant_summary(skill(14002551), &source, false, 1).unwrap(),
        10_000
    );

    source.set_field_by_name("hp", Value::I32(100));
    assert_eq!(
        scaled_skill_damage(skill(11000511), &source, 1).unwrap(),
        2_500
    );
    assert_eq!(
        scaled_skill_damage(skill(12000111), &source, 1).unwrap(),
        500
    );
    assert_eq!(
        scaled_skill_damage(skill(14002426), &source, 1).unwrap(),
        15_000
    );
}
