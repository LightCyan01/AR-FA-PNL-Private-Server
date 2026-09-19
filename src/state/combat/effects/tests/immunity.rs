use super::super::*;
use crate::state::combat::prelude::*;
use std::path::Path;

#[test]
fn role_gated_healing_immunity_selects_only_matching_targets() {
    let supporter = rule_for(780045009, "active", "skill", 20007181)
        .unwrap()
        .unwrap();
    let defender = rule_for(780045010, "active", "skill", 20007181)
        .unwrap()
        .unwrap();
    assert_eq!(
        (
            supporter.operation.as_str(),
            supporter.target.as_str(),
            supporter.fixed,
            supporter.state_id,
            &supporter.expiry,
            supporter.duration,
        ),
        ("healing_received", "targets", Some(10_000), 920001, &Expiry::Turn, 1)
    );

    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let member = |member_id, character_id| {
        let mut ally = empty_message(&proto, "blend.model.BattleAlly").unwrap();
        ally.set_field_by_name("character_id", Value::I32(character_id));
        let mut member = empty_message(&proto, "blend.model.BattleMember").unwrap();
        member.set_field_by_name("member_id", Value::I32(member_id));
        member.set_field_by_name("type", Value::EnumNumber(0));
        member.set_field_by_name("ally", Value::Message(ally));
        member
    };
    let source = member(1, 0);
    let matching = member(2, supporter.target_character_ids[0]);
    let other_role = member(3, defender.target_character_ids[0]);
    assert!(selected(supporter, &source, &matching, &[2]));
    assert!(!selected(supporter, &source, &other_role, &[3]));
}
