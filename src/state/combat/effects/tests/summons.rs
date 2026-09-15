use super::super::*;
use crate::state::combat::prelude::*;
use std::path::Path;

#[test]
fn summon_skill_adds_master_enemies_and_respects_side_count() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_gameplay_rules().unwrap();
    let (battle, wave) = rules
        .battles
        .iter()
        .find_map(|battle| {
            let wave = rule_wave(&rules, *battle.wave_ids.first()?).ok()?;
            (wave.enemies.len() == 1).then_some((battle, wave))
        })
        .unwrap();
    let source_enemy = &wave.enemies[0];
    let source = build_enemy_member(&proto, &rules, source_enemy, wave.id, 11, 1).unwrap();
    let source_base_id = rule_enemy(&rules, source_enemy.id).unwrap().base_enemy_id;
    let mut state = empty_message(&proto, "blend.model.BattleState").unwrap();
    state.set_field_by_name("battle_id", Value::I32(battle.id));
    state.set_field_by_name("wave", Value::I32(1));
    state.set_field_by_name("members", Value::List(vec![Value::Message(source)]));
    state.set_field_by_name(
        "timeline_units",
        Value::List(
            [1, 2]
                .into_iter()
                .map(|number| {
                    Value::Message(build_timeline_unit(&proto, 11, number, number * 100).unwrap())
                })
                .collect(),
        ),
    );
    set_base_enemy_numbers(&proto, &mut state, &[(source_base_id, 1)]).unwrap();
    let skill = rule_skill(&rules, 32004197).unwrap();
    let mut runtime = Runtime::default();
    runtime.prepare(&state, "summons").unwrap();

    let results = runtime
        .apply_for_action_with_rules(
            &proto,
            &rules,
            &mut state,
            11,
            skill.id,
            &skill.effects,
            &[11],
            true,
            "after",
            None,
            skill.state_change_application_rate,
            b"summons",
            "summons",
            1,
        )
        .unwrap();

    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|result| {
        member_status(result, "summons")
            .ok()
            .is_some_and(|summons| bool_field(&summons, "is_success"))
    }));
    let living_enemies = message_list(&state, "members")
        .into_iter()
        .filter(|member| member_type(member).ok() == Some(1) && bool_field(member, "is_alive"))
        .collect::<Vec<_>>();
    assert_eq!(living_enemies.len(), 3);
    assert!(living_enemies
        .iter()
        .all(|member| (11..=13).contains(&member_id(member).unwrap())));
    assert!(message_list(&state, "timeline_units")
        .iter()
        .any(|unit| i32_field(unit, "member_id") == Some(13)));

    let rejected = runtime
        .apply_for_action_with_rules(
            &proto,
            &rules,
            &mut state,
            11,
            skill.id,
            &skill.effects,
            &[11],
            true,
            "after",
            None,
            skill.state_change_application_rate,
            b"summons",
            "summons",
            2,
        )
        .unwrap();
    assert_eq!(message_list(&state, "members").len(), 3);
    assert!(rejected.iter().all(|result| {
        member_status(result, "summons")
            .ok()
            .is_some_and(|summons| !bool_field(&summons, "is_success"))
    }));
}
