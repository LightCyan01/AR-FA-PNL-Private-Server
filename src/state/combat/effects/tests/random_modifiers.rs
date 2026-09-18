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
