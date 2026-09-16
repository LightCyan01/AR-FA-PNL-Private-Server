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
    for (effect_id, skill_id) in [
        (91000949, 11000286),
        (91001076, 11000511),
        (91001871, 11003078),
        (91000943, 12000111),
        (91000950, 12000306),
        (91001066, 14000671),
        (91001211, 14001077),
        (91001633, 14002426),
        (91002166, 11003617),
    ] {
        assert_eq!(
            rule_for(effect_id, "catalog", "skill", skill_id)
                .unwrap()
                .unwrap()
                .operation,
            "scaling_metadata"
        );
    }

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
    assert_eq!(
        scaled_break_damage(skill(11003617), &source).unwrap(),
        1_000
    );

    let negative_ids = registry().unwrap().negative_state_ids.clone();
    let target_with_negatives = |count: usize| {
        let mut target = source.clone();
        let changes = negative_ids
            .iter()
            .take(count)
            .map(|state_id| {
                let mut change =
                    empty_message(&proto, "blend.model.BattleStateChange").unwrap();
                change.set_field_by_name("state_change_id", Value::I32(*state_id));
                Value::Message(change)
            })
            .collect();
        target.set_field_by_name("state_changes", Value::List(changes));
        target
    };
    for (count, expected) in [0, 1_000, 2_000, 3_000, 4_000, 5_000]
        .into_iter()
        .enumerate()
    {
        assert_eq!(
            instant_summary(skill(20000336), &target_with_negatives(count), false, 1).unwrap(),
            expected
        );
    }
    for (count, expected) in [0, 1_000, 3_000, 6_000, 10_000, 15_000]
        .into_iter()
        .enumerate()
    {
        assert_eq!(
            instant_summary(skill(20009932), &target_with_negatives(count), false, 1).unwrap(),
            expected
        );
    }

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
    assert_eq!(
        scaled_break_damage(skill(11003617), &source).unwrap(),
        4_000
    );
}
