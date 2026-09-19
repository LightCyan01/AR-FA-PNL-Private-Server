use super::super::*;
use crate::state::combat::prelude::*;
use std::path::Path;

#[test]
fn enemy_counter_probabilities_keep_their_owner_rate_and_action() {
    let expected = [
        (780056016, 280056012, 80056012, 20001931, 5_000),
        (880014004, 780014002, 80014002, 20001765, 1_000),
        (880014005, 280014002, 80014002, 20001765, 3_000),
        (880014007, 280014003, 80014003, 20001765, 6_000),
        (880017001, 280017002, 80017002, 20000977, 5_000),
        (880052004, 280052009, 80052009, 20007525, 1_000),
        (880056021, 280056010, 80056010, 20001931, 3_000),
        (880056028, 280056011, 80056011, 20001931, 6_000),
    ];
    let catalog = registry().unwrap();

    for (effect_id, ability_id, enemy_id, skill_id, rate) in expected {
        let rule = catalog
            .nested_actions
            .iter()
            .find(|rule| rule.owner_id == ability_id && rule.effect_id == effect_id)
            .unwrap();
        assert_eq!(rule.kind, NestedActionKind::Counter);
        assert_eq!(rule.source_enemy_ids, [enemy_id]);
        assert_eq!(rule.skill_id, skill_id);
        assert_eq!(rule.trigger_rate, rate);
        assert_eq!(rule.duration, -1);
        assert_eq!(rule.recipient, "self");
        assert_eq!(rule.target, "attacker");
    }

    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_gameplay_rules().unwrap();
    let mut status = empty_message(&proto, "blend.model.BattleCharacterStatus").unwrap();
    for field in ["attack", "defense", "hp", "magic", "mental", "speed"] {
        status.set_field_by_name(field, Value::I32(100));
    }
    let mut resistance = empty_message(&proto, "blend.model.BattleResistance").unwrap();
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
    let mut enemy = empty_message(&proto, "blend.model.BattleEnemy").unwrap();
    enemy.set_field_by_name("enemy_id", Value::I32(80056012));
    let mut member = empty_message(&proto, "blend.model.BattleMember").unwrap();
    member.set_field_by_name("member_id", Value::I32(11));
    member.set_field_by_name("type", Value::EnumNumber(1));
    member.set_field_by_name("is_alive", Value::Bool(true));
    member.set_field_by_name("hp", Value::I32(100));
    member.set_field_by_name("max_hp", Value::I32(100));
    member.set_field_by_name("enemy", Value::Message(enemy));
    member.set_field_by_name("initial_status", Value::Message(status.clone()));
    member.set_field_by_name("current_status", Value::Message(status));
    member.set_field_by_name("resistance", Value::Message(resistance));
    member.set_field_by_name("state_changes", Value::List(Vec::new()));
    member.set_field_by_name("state_change_summaries", Value::List(Vec::new()));
    let mut state = empty_message(&proto, "blend.model.BattleState").unwrap();
    state.set_field_by_name("members", Value::List(vec![Value::Message(member)]));

    let runtime =
        Runtime::initialize(&proto, &rules, &mut state, "counter-owner", &[], &[]).unwrap();
    assert!(runtime.nested_actions.iter().any(|instance| {
        instance.source == 11
            && instance.rule.owner_id == 280056012
            && instance.rule.skill_id == 20001931
    }));
}
