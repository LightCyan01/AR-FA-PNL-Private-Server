use super::super::*;
use crate::state::combat::prelude::*;
use std::path::Path;

#[test]
fn initiative_delays_every_non_initiative_opening_turn() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_tutorial_rules().unwrap();
    let rule = rule_for(72500099, "passive", "ability", 1980252)
        .unwrap()
        .unwrap()
        .clone();
    assert_eq!(
        (rule.operation.as_str(), rule.target.as_str()),
        ("initiative", "self")
    );

    let mut runtime = Runtime::default();
    runtime.passives.push(Passive {
        source: 1,
        value: 0,
        rule: rule.clone(),
        source_character_id: 0,
        source_type: 0,
    });
    runtime.passives.push(Passive {
        source: 2,
        value: 0,
        rule,
        source_character_id: 0,
        source_type: 0,
    });
    let initiative = runtime.initiative_members();

    let member = |id, speed| {
        let mut status = empty_message(&proto, "blend.model.BattleCharacterStatus").unwrap();
        status.set_field_by_name("speed", Value::I32(speed));
        let mut member = empty_message(&proto, "blend.model.BattleMember").unwrap();
        member.set_field_by_name("member_id", Value::I32(id));
        member.set_field_by_name("type", Value::EnumNumber(0));
        member.set_field_by_name("is_alive", Value::Bool(true));
        member.set_field_by_name("current_status", Value::Message(status));
        member
    };
    let members = [member(1, 300), member(2, 100), member(3, 200)];
    let units = tutorial_timeline_units(&proto, &rules, 0, 1, &members, &initiative).unwrap();
    let opening = units
        .iter()
        .take(4)
        .map(|unit| (member_id(unit).unwrap(), i32_field(unit, "wait").unwrap()))
        .collect::<Vec<_>>();
    assert_eq!(opening, [(1, 0), (1, 192), (2, 384), (3, 385)]);
}
