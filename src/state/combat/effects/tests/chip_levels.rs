use super::super::*;
use crate::state::combat::prelude::*;
use std::path::Path;

fn ally(proto: &ProtoRegistry, member_id: i32) -> DynamicMessage {
    let mut ally = empty_message(proto, "blend.model.BattleAlly").unwrap();
    ally.set_field_by_name("character_id", Value::I32(1));
    let mut member = empty_message(proto, "blend.model.BattleMember").unwrap();
    member.set_field_by_name("member_id", Value::I32(member_id));
    member.set_field_by_name("type", Value::EnumNumber(0));
    member.set_field_by_name("is_alive", Value::Bool(true));
    member.set_field_by_name("ally", Value::Message(ally));
    member
}

#[test]
fn casino_chip_draws_are_deterministic_and_add_to_one_level_state() {
    let fixed = registry()
        .unwrap()
        .rules
        .iter()
        .filter(|rule| {
            rule.owner_type.is_empty() && rule.operation == "level_state" && rule.state_id == 850013
        })
        .collect::<Vec<_>>();
    assert_eq!(fixed.len(), 15);
    assert_eq!(
        fixed
            .iter()
            .filter_map(|rule| rule.fixed)
            .collect::<std::collections::BTreeSet<_>>(),
        (1..=11).collect()
    );

    let rule = rule_for_occurrence(76266022, "active", "skill", 32003620, Some(1))
        .unwrap()
        .unwrap();
    assert_eq!(
        (
            rule.operation.as_str(),
            rule.target.as_str(),
            rule.state_id,
            rule.scale_input_min,
            rule.scale_input_max,
        ),
        ("random_level_state", "self", 850013, 1, 10)
    );

    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let source = ally(&proto, 1);
    let members = vec![source.clone()];
    let effect = TutorialSkillEffect {
        id: 76266022,
        value: 0,
    };
    let apply = |runtime: &mut Runtime| {
        runtime
            .apply_random_level_state(
                &proto,
                &members,
                &source,
                1,
                &effect,
                &[],
                true,
                rule,
                b"chip-level-test",
                "chip-level-test",
                1,
            )
            .unwrap()
    };
    let mut first = Runtime::default();
    let first_result = apply(&mut first);
    let increment = first.instances[0].value;
    assert!((1..=10).contains(&increment));
    assert_eq!(
        message_i32_field(
            first_result[0]
                .get_field_by_name("dealt_state_change")
                .unwrap()
                .as_message()
                .unwrap(),
            "level",
            "value",
        ),
        Some(increment)
    );

    let mut second = Runtime::default();
    assert_eq!(first_result, apply(&mut second));
    apply(&mut first);
    assert_eq!(first.instances[0].value, increment * 2);
}
