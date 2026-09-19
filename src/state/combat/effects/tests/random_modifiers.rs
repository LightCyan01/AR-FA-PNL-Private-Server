use super::super::*;
use crate::state::combat::prelude::*;
use std::path::Path;

#[test]
fn ghost_party_draws_four_deterministic_modifiers_for_each_ally() {
    let ghost_rules = registry()
        .unwrap()
        .rules
        .iter()
        .filter(|rule| rule.id == 91001052 && rule.operation == "random_modifier")
        .collect::<Vec<_>>();
    assert_eq!(ghost_rules.len(), 11);
    for rule in ghost_rules {
        assert_eq!(rule.random_draws, 4);
        assert_eq!(rule.random_choices.len(), 3);
        assert!(rule.random_choices.iter().all(|choice| choice.positive));
        assert_eq!(
            rule.random_choices
                .iter()
                .map(|choice| (
                    choice.operation.as_str(),
                    choice.summary,
                    &choice.expiry,
                    choice.duration,
                ))
                .collect::<Vec<_>>(),
            vec![
                ("summary", 1, &Expiry::Turn, 1),
                ("summary", 3, &Expiry::Turn, 1),
                ("taken_down", 0, &Expiry::Hit, 1),
            ]
        );
    }

    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_gameplay_rules().unwrap();
    let fresh = load_fresh_rules().unwrap();
    let opened = reduce_talk_event(
        &proto,
        &fresh,
        &rules,
        starter_resources(&proto, &fresh).unwrap(),
        101001001,
        1,
    )
    .unwrap();
    let start = reduce_battle_start(&proto, &rules, opened.resources, 101001002, 1).unwrap();
    let actor_id = member_id(&current_actor(&start.state).unwrap()).unwrap();
    let actor_type = member_type(&current_actor(&start.state).unwrap()).unwrap();
    let target_id = message_list(&start.state, "members")
        .iter()
        .find(|member| member_type(member).ok() != Some(actor_type))
        .and_then(|member| i32_field(member, "member_id"))
        .unwrap();
    let ally_ids = message_list(&start.state, "members")
        .iter()
        .filter(|member| {
            bool_field(member, "is_alive") && member_type(member).ok() == Some(actor_type)
        })
        .map(member_id)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let skill = rules
        .skills
        .iter()
        .find(|skill| skill.id == 12000621)
        .unwrap();
    let mut first_state = start.state.clone();
    let mut first_runtime = start.effects.clone();
    let mut second_state = start.state;
    let mut second_runtime = start.effects;
    let first_results = first_runtime
        .apply_for_action_with_rules(
            &proto,
            &rules,
            &mut first_state,
            actor_id,
            skill.id,
            &skill.effects,
            &[target_id],
            true,
            "after",
            None,
            skill.state_change_application_rate,
            b"ghost-party-test",
            &start.start_txid,
            1,
        )
        .unwrap();
    let second_results = second_runtime
        .apply_for_action_with_rules(
            &proto,
            &rules,
            &mut second_state,
            actor_id,
            skill.id,
            &skill.effects,
            &[target_id],
            true,
            "after",
            None,
            skill.state_change_application_rate,
            b"ghost-party-test",
            &start.start_txid,
            1,
        )
        .unwrap();

    let instances = |runtime: &Runtime| {
        let mut rows = runtime
            .instances
            .iter()
            .filter(|instance| instance.rule.id == 91001052)
            .map(|instance| {
                (
                    instance.target,
                    instance.rule.state_id,
                    instance.value,
                    instance.remaining,
                )
            })
            .collect::<Vec<_>>();
        rows.sort_unstable();
        rows
    };
    assert_eq!(first_results.len(), ally_ids.len() * 4);
    assert_eq!(first_results, second_results);
    assert_eq!(instances(&first_runtime), instances(&second_runtime));
    for ally_id in ally_ids {
        assert_eq!(
            first_runtime
                .instances
                .iter()
                .filter(|instance| instance.rule.id == 91001052 && instance.target == ally_id)
                .map(|instance| instance.value)
                .sum::<i32>(),
            skill.effects[0].value * 4
        );
    }
}

#[test]
fn infamous_absword_applies_one_deterministic_target_debuff() {
    let rules = registry()
        .unwrap()
        .rules
        .iter()
        .filter(|rule| rule.id == 71149005)
        .collect::<Vec<_>>();
    assert_eq!(rules.len(), 10);
    for rule in &rules {
        assert_eq!((rule.target.as_str(), rule.random_draws), ("targets", 1));
        assert_eq!(
            rule.random_choices
                .iter()
                .map(|choice| (
                    choice.operation.as_str(),
                    choice.summary,
                    choice.sign,
                    choice.state_id,
                    &choice.expiry,
                    choice.duration,
                ))
                .collect::<Vec<_>>(),
            vec![
                ("summary", 2, 1, 920015, &Expiry::Turn, 3),
                ("summary", 11, 1, 920008, &Expiry::Hit, 3),
                ("healing_received", 0, -1, 920005, &Expiry::Turn, 3),
            ]
        );
    }
    let protection = registry()
        .unwrap()
        .rules
        .iter()
        .find(|rule| rule.id == 71149003 && rule.owner_id == 22000294)
        .unwrap();
    assert_eq!(
        (
            protection.operation.as_str(),
            protection.target.as_str(),
            protection.state_id,
            &protection.expiry,
            protection.duration,
            protection.positive,
        ),
        ("taken_down", "allies", 560074, &Expiry::Attacked, 3, true)
    );

    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let mut ally = empty_message(&proto, "blend.model.BattleAlly").unwrap();
    ally.set_field_by_name("character_id", Value::I32(1));
    let mut source = empty_message(&proto, "blend.model.BattleMember").unwrap();
    source.set_field_by_name("member_id", Value::I32(1));
    source.set_field_by_name("type", Value::EnumNumber(0));
    source.set_field_by_name("is_alive", Value::Bool(true));
    source.set_field_by_name("ally", Value::Message(ally));
    let enemy = empty_message(&proto, "blend.model.BattleEnemy").unwrap();
    let mut target = empty_message(&proto, "blend.model.BattleMember").unwrap();
    target.set_field_by_name("member_id", Value::I32(2));
    target.set_field_by_name("type", Value::EnumNumber(1));
    target.set_field_by_name("is_alive", Value::Bool(true));
    target.set_field_by_name("enemy", Value::Message(enemy));
    let members = vec![source.clone(), target];
    let rule = rules[0];
    let effect = TutorialSkillEffect {
        id: 71149005,
        value: 2_000,
    };
    let apply = |action_number| {
        let mut runtime = Runtime::default();
        let results = runtime
            .apply_random_modifier(
                &proto,
                &members,
                &source,
                1,
                &effect,
                &[2],
                true,
                rule,
                effect.value,
                b"infamous-absword-test",
                "infamous-absword-test",
                action_number,
            )
            .unwrap();
        let instances = runtime
            .instances
            .iter()
            .map(|instance| {
                (
                    instance.target,
                    instance.rule.state_id,
                    instance.rule.sign,
                    instance.value,
                    instance.remaining,
                )
            })
            .collect::<Vec<_>>();
        (results, instances)
    };
    assert_eq!(apply(1), apply(1));
    let healing_down = (1..=32)
        .map(apply)
        .find_map(|(_, instances)| {
            instances
                .into_iter()
                .find(|(_, state_id, _, _, _)| *state_id == 920005)
        })
        .unwrap();
    assert_eq!(healing_down, (2, 920005, -1, -2_000, 3));
}
