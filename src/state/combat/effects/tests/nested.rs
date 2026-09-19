use super::super::runtime::NestedActionInstance;
use super::super::*;
use crate::state::combat::prelude::*;
use std::path::Path;

fn proto() -> ProtoRegistry {
    ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap()
}

fn enemy_member(proto: &ProtoRegistry, rules: &TutorialRules, member_id: i32) -> DynamicMessage {
    let wave = rules
        .waves
        .iter()
        .find(|wave| !wave.enemies.is_empty())
        .unwrap();
    build_enemy_member(proto, rules, &wave.enemies[0], wave.id, member_id, 1).unwrap()
}

fn ally_member(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    character_id: i32,
    ability_ids: Vec<i32>,
) -> DynamicMessage {
    build_ally_member(
        proto,
        rules,
        &BattlePartyMember {
            character_id,
            level: 1,
            rarity: 3,
            memoria_id: None,
            position: 1,
            is_leader: true,
            integrated_stats: None,
            damage_bonus: 0,
            skills: Vec::new(),
            ability_ids,
            passives: Vec::new(),
            leader_passives: Vec::new(),
        },
        1,
        0,
        0,
    )
    .unwrap()
}

#[test]
fn counter_rule_queues_the_owners_ranked_extra_skill() {
    let proto = proto();
    let rules = load_gameplay_rules().unwrap();
    let ally = ally_member(&proto, &rules, 41501, Vec::new());
    let enemy = enemy_member(&proto, &rules, 11);
    let mut state = empty_message(&proto, "blend.model.BattleState").unwrap();
    state.set_field_by_name(
        "members",
        Value::List(vec![Value::Message(ally), Value::Message(enemy)]),
    );
    let rule = registry()
        .unwrap()
        .nested_actions
        .iter()
        .find(|rule| rule.owner_id == 12000786)
        .unwrap()
        .clone();
    let mut runtime = Runtime::default();
    runtime.nested_actions.push(NestedActionInstance {
        source: 1,
        target: 1,
        remaining: rule.duration,
        rule,
    });
    let enemy_skill = rules
        .skills
        .iter()
        .find(|skill| skill.skill_effect_type == 1)
        .unwrap();
    let result = build_skill_result(
        &proto, 1, 1, 0, 0, 0, true, false, false, false, false, false, false,
    )
    .unwrap();

    let pending = runtime
        .collect_nested_actions(
            &rules,
            &mut state,
            11,
            enemy_skill,
            &[result],
            b"counter",
            "counter",
            1,
        )
        .unwrap();

    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].actor_id, 1);
    assert_eq!(pending[0].target_id, 11);
    assert_eq!(pending[0].skill_id, 14000816);
    assert_eq!(pending[0].kind, NestedActionKind::Counter);
    assert_eq!(runtime.nested_actions[0].remaining, 1);
}

#[test]
fn stored_burst_gauge_pays_for_the_conditional_additional_attack() {
    let proto = proto();
    let rules = load_gameplay_rules().unwrap();
    let mut ally = ally_member(&proto, &rules, 45602, vec![1990535, 1990536]);
    let mut gauge = member_status(&ally, "burst_gauge").unwrap();
    assert_eq!(i32_field(&gauge, "max_gauge"), Some(300));
    let capacity = rules
        .abilities
        .iter()
        .find(|ability| ability.id == 1990535)
        .unwrap()
        .burst_gauge_max
        .unwrap();
    assert!(!character_passives(&rules, 45602, None, 3)
        .unwrap()
        .iter()
        .any(|passive| passive.effect.value == capacity * 100));
    gauge.set_field_by_name("current_gauge", Value::I32(200));
    ally.set_field_by_name("burst_gauge", Value::Message(gauge));
    let enemy = enemy_member(&proto, &rules, 11);
    let mut state = empty_message(&proto, "blend.model.BattleState").unwrap();
    state.set_field_by_name(
        "members",
        Value::List(vec![Value::Message(ally), Value::Message(enemy)]),
    );
    let catalog = registry().unwrap();
    for (effect_id, index) in [(72001683, 1), (72001684, 2)] {
        assert!(
            rule_for_occurrence(effect_id, "catalog", "ability", 1990536, Some(index))
                .unwrap()
                .is_some()
        );
    }
    let rule = catalog
        .nested_actions
        .iter()
        .find(|rule| rule.owner_id == 1990536)
        .unwrap()
        .clone();
    let mut runtime = Runtime::default();
    runtime.nested_actions.push(NestedActionInstance {
        source: 1,
        target: 1,
        remaining: rule.duration,
        rule,
    });
    let burst = rules
        .skills
        .iter()
        .find(|skill| skill.skill_type == 3 && skill.skill_effect_type == 1)
        .unwrap();
    let result = build_skill_result(
        &proto, 11, 1, 0, 0, 0, true, false, false, false, false, false, false,
    )
    .unwrap();

    let pending = runtime
        .collect_nested_actions(
            &rules,
            &mut state,
            1,
            burst,
            &[result],
            b"additional",
            "additional",
            1,
        )
        .unwrap();

    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].skill_id, 14003726);
    assert_eq!(pending[0].kind, NestedActionKind::AdditionalAttack);
    let actor = message_list(&state, "members")
        .into_iter()
        .find(|member| i32_field(member, "member_id") == Some(1))
        .unwrap();
    assert_eq!(
        member_status(&actor, "burst_gauge")
            .ok()
            .and_then(|gauge| i32_field(&gauge, "current_gauge")),
        Some(100)
    );
}

#[test]
fn mode_gravis_stacks_and_is_removed_one_at_a_time() {
    let proto = proto();
    let rules = load_gameplay_rules().unwrap();
    let ally = ally_member(&proto, &rules, 45602, Vec::new());
    let mut state = empty_message(&proto, "blend.model.BattleState").unwrap();
    state.set_field_by_name("members", Value::List(vec![Value::Message(ally)]));
    let mut runtime = Runtime::default();
    runtime.prepare(&state, "gravis").unwrap();
    let stack = TutorialSkillEffect {
        id: 91002342,
        value: 1_500,
    };
    let stacks = vec![stack; 5];
    runtime
        .apply_inner(
            &proto,
            &mut state,
            1,
            14003917,
            &stacks,
            &[1],
            true,
            "after",
            None,
            10_000,
            None,
        )
        .unwrap();
    assert!(runtime
        .instances
        .iter()
        .any(|instance| { instance.rule.state_id == 610538 && instance.value == 7_500 }));

    let remove = TutorialSkillEffect {
        id: 91002343,
        value: 100,
    };
    runtime
        .apply_inner(
            &proto,
            &mut state,
            1,
            14003923,
            &[remove.clone()],
            &[1],
            true,
            "after",
            None,
            10_000,
            None,
        )
        .unwrap();
    assert!(runtime
        .instances
        .iter()
        .any(|instance| { instance.rule.state_id == 610538 && instance.value == 6_000 }));

    for _ in 0..4 {
        runtime
            .apply_inner(
                &proto,
                &mut state,
                1,
                14003923,
                &[remove.clone()],
                &[1],
                true,
                "after",
                None,
                10_000,
                None,
            )
            .unwrap();
    }
    assert!(!runtime
        .instances
        .iter()
        .any(|instance| instance.rule.state_id == 610538));
    assert!(!runtime
        .managed
        .get(&1)
        .is_some_and(|states| states.contains(&610538)));
}
