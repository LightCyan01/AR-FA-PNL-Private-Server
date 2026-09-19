use super::super::*;
use crate::state::combat::prelude::*;
use std::{collections::BTreeSet, path::Path};

const NAMED_POTENCY: [(i32, &str); 18] = [
    (3000028, "given_positive_potency"),
    (3000029, "given_positive_potency"),
    (3000030, "given_negative_potency"),
    (3000211, "given_negative_potency"),
    (3000212, "given_negative_potency"),
    (3000225, "given_negative_potency"),
    (3000232, "given_negative_potency"),
    (3000325, "given_negative_potency"),
    (3000326, "given_negative_potency"),
    (3000327, "given_negative_potency"),
    (3000394, "given_positive_potency"),
    (3000395, "given_positive_potency"),
    (3000396, "given_positive_potency"),
    (3000397, "given_positive_potency"),
    (3000398, "given_positive_potency"),
    (3000399, "given_positive_potency"),
    (3000400, "given_positive_potency"),
    (3000401, "given_positive_potency"),
];

fn member(proto: &ProtoRegistry, member_id: i32, character_id: Option<i32>) -> DynamicMessage {
    let mut member = empty_message(proto, "blend.model.BattleMember").unwrap();
    member.set_field_by_name("member_id", Value::I32(member_id));
    member.set_field_by_name(
        "type",
        Value::EnumNumber(if character_id.is_some() { 0 } else { 1 }),
    );
    member.set_field_by_name("is_alive", Value::Bool(true));
    if let Some(character_id) = character_id {
        let mut ally = empty_message(proto, "blend.model.BattleAlly").unwrap();
        ally.set_field_by_name("character_id", Value::I32(character_id));
        member.set_field_by_name("ally", Value::Message(ally));
    }
    member
}

#[test]
fn named_potency_catalog_and_runtime_filters_are_exact() {
    let rules = &registry().unwrap().rules;
    let ids = NAMED_POTENCY
        .iter()
        .map(|(id, ..)| *id)
        .collect::<BTreeSet<_>>();
    let named = rules
        .iter()
        .filter(|rule| {
            ids.contains(&rule.id) && rule.mode == "passive" && rule.owner_type == "ability"
        })
        .collect::<Vec<_>>();

    assert_eq!(named.len(), 90);
    assert_eq!(
        named
            .iter()
            .map(|rule| rule.owner_id)
            .collect::<BTreeSet<_>>()
            .len(),
        90
    );
    for (id, operation) in NAMED_POTENCY {
        let occurrences = named
            .iter()
            .filter(|rule| rule.id == id)
            .copied()
            .collect::<Vec<_>>();
        assert_eq!(occurrences.len(), 5, "effect {id}");
        assert!(occurrences.iter().all(|rule| {
            rule.operation == operation
                && rule.target == "self"
                && rule.positive
                && !rule.affected_state_ids.is_empty()
                && rule.effect_target_character_ids.is_empty()
                    == !matches!(id, 3000029 | 3000394..=3000401)
        }));
    }

    let rules = &registry().unwrap().rules;
    let named = |id| {
        rules
            .iter()
            .find(|rule| rule.id == id && rule.mode == "passive")
            .unwrap()
            .clone()
    };
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();

    let attacker = named(3000395);
    let matching_character = attacker.effect_target_character_ids[0];
    let other_character = named(3000394)
        .effect_target_character_ids
        .into_iter()
        .find(|id| !attacker.effect_target_character_ids.contains(id))
        .unwrap();
    let members = vec![
        member(&proto, 1, Some(other_character)),
        member(&proto, 2, Some(matching_character)),
        member(&proto, 3, Some(other_character)),
        member(&proto, 4, None),
    ];
    let mut effect = rule_for(91001018, "active", "skill", 0)
        .unwrap()
        .unwrap()
        .clone();
    effect.state_id = attacker.affected_state_ids[0];
    let mut runtime = Runtime::default();
    runtime.passives.push(Passive {
        source: 1,
        value: 1_000,
        rule: attacker.clone(),
        source_character_id: other_character,
        source_type: 0,
    });
    assert_eq!(
        runtime
            .apply_potency(&members, &members[0], 2, &effect, 1_000)
            .unwrap(),
        1_100
    );
    assert_eq!(
        runtime
            .apply_potency(&members, &members[0], 3, &effect, 1_000)
            .unwrap(),
        1_000
    );
    effect.state_id = *registry()
        .unwrap()
        .positive_state_ids
        .iter()
        .find(|id| !attacker.affected_state_ids.contains(id))
        .unwrap();
    assert_eq!(
        runtime
            .apply_potency(&members, &members[0], 2, &effect, 1_000)
            .unwrap(),
        1_000
    );

    let resistance = named(3000211);
    effect.state_id = resistance.affected_state_ids[0];
    runtime.passives[0].rule = resistance;
    assert_eq!(
        runtime
            .apply_potency(&members, &members[0], 4, &effect, 1_000)
            .unwrap(),
        1_100
    );
}
