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
        (91001615, 12002546),
        (91002296, 12003833),
        (91002296, 14003838),
        (91002166, 11003617),
        (91001470, 12002064),
        (91001470, 14002069),
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
        scaled_skill_damage(skill(11000301), &source, 1).unwrap(),
        1_000
    );
    assert_eq!(
        scaled_skill_damage(skill(11000301), &source, 2).unwrap(),
        0
    );
    assert_eq!(
        scaled_skill_damage(skill(14000315), &source, 1).unwrap(),
        4_000
    );
    assert_eq!(
        scaled_skill_damage(skill(12003210), &source, 1).unwrap(),
        0
    );
    assert_eq!(
        scaled_skill_damage(skill(14003215), &source, 1).unwrap(),
        0
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
        scaled_critical_damage(skill(12002546), &source, 1).unwrap(),
        18_000
    );
    assert_eq!(
        scaled_critical_damage(skill(12002546), &source, 4).unwrap(),
        6_000
    );
    assert_eq!(
        scaled_critical_damage(skill(12003833), &source, 1).unwrap(),
        10_000
    );
    assert_eq!(
        scaled_critical_damage(skill(14003838), &source, 1).unwrap(),
        10_000
    );
    let mut critical_skill = skill(12002546).clone();
    critical_skill.effects.retain(|effect| effect.id == 91001616);
    assert_eq!(
        secondary_damage(
            15_000,
            &source,
            &source,
            &critical_skill,
            None,
            1,
            true,
        )
        .unwrap(),
        33_000
    );
    assert_eq!(
        secondary_damage(
            15_000,
            &source,
            &source,
            &critical_skill,
            None,
            4,
            true,
        )
        .unwrap(),
        21_000
    );
    assert_eq!(
        scaled_skill_damage(skill(14002426), &source, 1).unwrap(),
        5_000
    );
    assert_eq!(
        scaled_skill_damage(skill(12002064), &source, 1).unwrap(),
        600
    );
    assert_eq!(
        scaled_skill_damage(skill(14002069), &source, 1).unwrap(),
        1_200
    );
    assert_eq!(
        instant_summary(skill(14002551), &source, false, 1).unwrap(),
        10_000
    );
    assert_eq!(
        instant_summary(skill(12000916), &source, false, 1).unwrap(),
        5_000
    );
    let mut threshold = source.clone();
    threshold.set_field_by_name("hp", Value::I32(50));
    assert_eq!(
        instant_summary(skill(12000916), &threshold, false, 1).unwrap(),
        0
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
        scaled_skill_damage(skill(12003210), &source, 1).unwrap(),
        12_000
    );
    assert_eq!(
        scaled_skill_damage(skill(14003215), &source, 1).unwrap(),
        15_000
    );
    assert_eq!(
        scaled_critical_damage(skill(12003833), &source, 1).unwrap(),
        15_000
    );
    assert_eq!(
        scaled_critical_damage(skill(14003838), &source, 1).unwrap(),
        15_000
    );
    assert_eq!(
        scaled_break_damage(skill(11003617), &source).unwrap(),
        4_000
    );
    assert_eq!(
        scaled_skill_damage(skill(12002064), &source, 1).unwrap(),
        6_000
    );
    assert_eq!(
        scaled_skill_damage(skill(14002069), &source, 1).unwrap(),
        12_000
    );
}

#[test]
fn granted_buffs_scale_with_live_opponent_count() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let source = empty_message(&proto, "blend.model.BattleMember").unwrap();
    for (effect_id, skill_id, summary, minimum, maximum) in [
        (91001023, 11000541, 3, 200, 1_000),
        (91001023, 11003136, 3, 1_000, 4_000),
        (91001856, 11003136, 1, 1_000, 4_000),
    ] {
        let rule = rule_for(effect_id, "active", "skill", skill_id)
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                rule.target.as_str(),
                rule.summary,
                &rule.expiry,
                rule.duration,
                rule.scale_by.as_str(),
            ),
            ("allies", summary, &Expiry::Turn, 1, "opponent_count")
        );
        assert_eq!(
            super::super::runtime_scaling::scaled_effect_value(
                rule, &source, 1, minimum,
            )
            .unwrap(),
            minimum
        );
        assert_eq!(
            super::super::runtime_scaling::scaled_effect_value(
                rule, &source, 4, minimum,
            )
            .unwrap(),
            maximum
        );
    }
}

#[test]
fn received_damage_buffs_scale_with_tagged_party_members() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_gameplay_rules().unwrap();
    let skill_ids = (12002406..=12002410).chain(12002571..=12002575);
    for (effect_id, operation, summary, attribute, state_id) in [
        (91001687, "attribute_taken", 0, Some(5), 50006),
        (91001688, "attribute_taken", 0, Some(2), 50011),
        (91001689, "summary", 13, None, 510306),
    ] {
        for skill_id in skill_ids.clone() {
            let rule = rule_for(effect_id, "active", "skill", skill_id)
                .unwrap()
                .unwrap();
            assert_eq!(
                (
                    rule.operation.as_str(),
                    rule.summary,
                    rule.target.as_str(),
                    rule.state_id,
                    &rule.expiry,
                    rule.duration,
                    rule.fixed,
                    rule.scale_by.as_str(),
                    rule.scale_input_min,
                    rule.scale_input_max,
                    rule.scale_output_max,
                    rule.positive,
                ),
                (
                    operation,
                    summary,
                    "targets",
                    state_id,
                    &Expiry::Attacked,
                    2,
                    Some(500),
                    "party_tag_count",
                    1,
                    5,
                    2_500,
                    false,
                )
            );
            assert_eq!(rule.attack_attributes.first().copied(), attribute);
        }
    }

    let rule = rule_for(91001687, "active", "skill", 12002406)
        .unwrap()
        .unwrap();
    let tag_id = rule.condition["party_tag_id"];
    let tagged = rules
        .battle_characters
        .iter()
        .filter(|character| character.tag_ids.contains(&tag_id))
        .map(|character| character.id)
        .collect::<Vec<_>>();
    let untagged = rules
        .battle_characters
        .iter()
        .find(|character| !character.tag_ids.contains(&tag_id))
        .unwrap()
        .id;
    assert!(tagged.len() >= 6);
    let member = |member_id, character_id, member_type| {
        let mut ally = empty_message(&proto, "blend.model.BattleAlly").unwrap();
        ally.set_field_by_name("character_id", Value::I32(character_id));
        let mut member = empty_message(&proto, "blend.model.BattleMember").unwrap();
        member.set_field_by_name("member_id", Value::I32(member_id));
        member.set_field_by_name("type", Value::EnumNumber(member_type));
        member.set_field_by_name("ally", Value::Message(ally));
        member
    };
    for (count, expected) in [(1, 500), (5, 2_500)] {
        let mut members = tagged
            .iter()
            .take(count)
            .enumerate()
            .map(|(index, character_id)| member(index as i32 + 1, *character_id, 0))
            .collect::<Vec<_>>();
        members.push(member(20, untagged, 0));
        members.push(member(21, tagged[5], 1));
        let actual = super::super::runtime_scaling::party_tag_count(
            &rules,
            &members,
            &members[0],
            tag_id,
        )
        .unwrap();
        assert_eq!(actual, count as i32);
        assert_eq!(
            super::super::runtime_scaling::scaled_effect_value(
                rule,
                &members[0],
                actual,
                rule.fixed.unwrap(),
            )
            .unwrap(),
            expected
        );
    }
}
