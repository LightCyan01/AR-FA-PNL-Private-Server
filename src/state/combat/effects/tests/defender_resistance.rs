use super::super::*;
use crate::state::combat::prelude::*;
use std::{collections::BTreeSet, path::Path};

#[test]
fn defender_resistance_covers_both_triggers_stacking_and_expiry() {
    let family = registry()
        .unwrap()
        .rules
        .iter()
        .filter(|rule| (95000305..=95000318).contains(&rule.id))
        .collect::<Vec<_>>();
    assert_eq!(family.len(), 70);
    for trigger in ["attack_after", "attacked"] {
        let rules = family
            .iter()
            .filter(|rule| rule.trigger.as_deref() == Some(trigger))
            .collect::<Vec<_>>();
        assert_eq!(rules.len(), 35);
        assert_eq!(
            rules
                .iter()
                .map(|rule| rule.id)
                .collect::<BTreeSet<_>>()
                .len(),
            7
        );
    }

    let attack = rule_for_occurrence(95000305, "passive", "ability", 4991193, Some(0))
        .unwrap()
        .unwrap()
        .clone();
    let attacked = rule_for_occurrence(95000315, "passive", "ability", 4991193, Some(10))
        .unwrap()
        .unwrap()
        .clone();
    assert_eq!(
        (
            attack.target.as_str(),
            attack.trigger.as_deref(),
            attack.attack_attributes.as_slice(),
            attack.state_id,
            &attack.expiry,
            attack.duration,
            attack.stack_limit,
            attack.positive,
        ),
        (
            "enemies",
            Some("attack_after"),
            &[5][..],
            50006,
            &Expiry::Attacked,
            1,
            4,
            false,
        )
    );
    assert_eq!(
        (
            attacked.target.as_str(),
            attacked.trigger.as_deref(),
            attacked.attack_attributes.as_slice(),
            attacked.state_id,
        ),
        ("enemies", Some("attacked"), &[8][..], 50009)
    );
    assert!(!attack.source_character_ids.is_empty());
    assert!(!attacked.source_character_ids.is_empty());

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
    let base_state = start.state;
    let actor_id = member_id(&current_actor(&base_state).unwrap()).unwrap();
    let enemies = message_list(&base_state, "members")
        .iter()
        .filter(|member| member_type(member).ok() == Some(1) && bool_field(member, "is_alive"))
        .filter_map(|member| member_id(member).ok())
        .collect::<Vec<_>>();
    assert!(enemies.len() > 1);
    let skill = rules
        .skills
        .iter()
        .find(|skill| skill.skill_effect_type == 1)
        .unwrap();
    let state_for = |character_id| {
        let mut state = base_state.clone();
        let members = message_list(&state, "members")
            .into_iter()
            .map(|mut member| {
                if member_id(&member).ok() == Some(actor_id) {
                    let mut ally = member_status(&member, "ally").unwrap();
                    ally.set_field_by_name("character_id", Value::I32(character_id));
                    member.set_field_by_name("ally", Value::Message(ally));
                }
                Value::Message(member)
            })
            .collect();
        state.set_field_by_name("members", Value::List(members));
        state
    };
    let multiplier = |state: &DynamicMessage, target_id, attribute, runtime: &Runtime| {
        let target = message_list(state, "members")
            .into_iter()
            .find(|member| member_id(member).ok() == Some(target_id))
            .unwrap();
        incoming_multiplier_with_runtime(&target, attribute, Some(runtime))
    };

    let mut attack_state = state_for(attack.source_character_ids[0]);
    let mut attack_runtime = Runtime::default();
    attack_runtime
        .prepare(&attack_state, "defender-attack")
        .unwrap();
    attack_runtime.passives.push(Passive {
        source: actor_id,
        value: 300,
        rule: attack,
        source_character_id: message_i32_field(
            &current_actor(&attack_state).unwrap(),
            "ally",
            "character_id",
        )
        .unwrap(),
        source_type: 0,
    });
    let enemy_hit = build_skill_result(
        &proto, enemies[0], 1, 0, 0, 0, true, false, false, false, false, false, false,
    )
    .unwrap();
    for _ in 0..5 {
        attack_runtime
            .trigger_attack_after(
                &proto,
                &rules,
                &mut attack_state,
                actor_id,
                skill,
                std::slice::from_ref(&enemy_hit),
            )
            .unwrap();
    }
    assert_eq!(attack_runtime.instances.len(), enemies.len() * 4);
    assert!(enemies
        .iter()
        .all(|enemy| multiplier(&attack_state, *enemy, 5, &attack_runtime) == 11_200));
    attack_runtime.expire_with_attributes(
        actor_id,
        std::slice::from_ref(&enemy_hit),
        true,
        true,
        &[5],
    );
    attack_runtime.refresh(&proto, &mut attack_state).unwrap();
    assert_eq!(
        multiplier(&attack_state, enemies[0], 5, &attack_runtime),
        10_000
    );
    assert_eq!(
        multiplier(&attack_state, enemies[1], 5, &attack_runtime),
        11_200
    );

    let mut attacked_state = state_for(attacked.source_character_ids[0]);
    let mut attacked_runtime = Runtime::default();
    attacked_runtime
        .prepare(&attacked_state, "defender-attacked")
        .unwrap();
    attacked_runtime.passives.push(Passive {
        source: actor_id,
        value: 300,
        rule: attacked,
        source_character_id: message_i32_field(
            &current_actor(&attacked_state).unwrap(),
            "ally",
            "character_id",
        )
        .unwrap(),
        source_type: 0,
    });
    let ally_hit = build_skill_result(
        &proto, actor_id, 1, 0, 0, 0, true, false, false, false, false, false, false,
    )
    .unwrap();
    assert_eq!(
        attacked_runtime
            .trigger_attack_after(
                &proto,
                &rules,
                &mut attacked_state,
                enemies[0],
                skill,
                &[ally_hit],
            )
            .unwrap()
            .len(),
        enemies.len()
    );
    assert!(enemies
        .iter()
        .all(|enemy| multiplier(&attacked_state, *enemy, 8, &attacked_runtime) == 10_300));
}
