use super::super::*;
use crate::state::combat::prelude::*;
use std::path::Path;

#[test]
fn equipment_healing_uses_the_outgoing_healing_state() {
    let rule = rule_for(3_000_031, "passive", "", 0).unwrap().unwrap();
    assert_eq!(
        (rule.operation.as_str(), rule.state_id),
        ("healing", 50_013)
    );
}

#[test]
fn pre_attack_skill_damage_stacks_to_its_master_cap() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_gameplay_rules().unwrap();
    let fresh = load_fresh_rules().unwrap();
    let resources = reduce_talk_event(
        &proto,
        &fresh,
        &rules,
        starter_resources(&proto, &fresh).unwrap(),
        101001001,
        1,
    )
    .unwrap()
    .resources;
    let start = reduce_battle_start(&proto, &rules, resources, 101001002, 1).unwrap();
    let mut state = start.state;
    let actor_id = message_list(&state, "members")
        .iter()
        .find(|member| member_type(member).ok() == Some(0))
        .and_then(|member| i32_field(member, "member_id"))
        .unwrap();
    let mut runtime = start.effects;
    runtime.passives.clear();
    runtime.instances.clear();
    runtime.managed.clear();
    runtime.refresh(&proto, &mut state).unwrap();
    let baseline = message_list(&state, "members")
        .into_iter()
        .find(|member| i32_field(member, "member_id") == Some(actor_id))
        .map(|member| state_change_summary_value(&member, 1))
        .unwrap();
    for _ in 0..11 {
        runtime
            .apply(
                &proto,
                &mut state,
                actor_id,
                &[TutorialSkillEffect {
                    id: 91001000,
                    value: 500,
                }],
                &[actor_id],
                true,
                "before",
                None,
            )
            .unwrap();
    }
    assert_eq!(runtime.instances.len(), 1);
    assert_eq!(runtime.instances[0].value, 5_000);
    let actor = message_list(&state, "members")
        .into_iter()
        .find(|member| i32_field(member, "member_id") == Some(actor_id))
        .unwrap();
    assert_eq!(state_change_summary_value(&actor, 1), baseline + 5_000);
    assert!(message_list(&actor, "state_changes").iter().any(|change| {
        i32_field(change, "state_change_id") == Some(610007)
            && i32_field(change, "value") == Some(5_000)
    }));
}

#[test]
fn outgoing_healing_stacks_to_its_master_cap() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_gameplay_rules().unwrap();
    let fresh = load_fresh_rules().unwrap();
    let resources = reduce_talk_event(
        &proto,
        &fresh,
        &rules,
        starter_resources(&proto, &fresh).unwrap(),
        101001001,
        1,
    )
    .unwrap()
    .resources;
    let start = reduce_battle_start(&proto, &rules, resources, 101001002, 1).unwrap();
    let mut state = start.state;
    let actor_id = message_list(&state, "members")
        .iter()
        .find(|member| member_type(member).ok() == Some(0))
        .and_then(|member| i32_field(member, "member_id"))
        .unwrap();
    let mut runtime = start.effects;
    runtime.passives.clear();
    runtime.instances.clear();
    runtime.managed.clear();
    runtime.refresh(&proto, &mut state).unwrap();
    for _ in 0..11 {
        runtime
            .apply(
                &proto,
                &mut state,
                actor_id,
                &[TutorialSkillEffect {
                    id: 91001230,
                    value: 1_000,
                }],
                &[actor_id],
                true,
                "after",
                None,
            )
            .unwrap();
    }
    let actor = message_list(&state, "members")
        .into_iter()
        .find(|member| i32_field(member, "member_id") == Some(actor_id))
        .unwrap();
    assert_eq!(healing_amount(100, &actor, &actor).unwrap(), 200);
    assert!(message_list(&actor, "state_changes").iter().any(|change| {
        i32_field(change, "state_change_id") == Some(610091)
            && i32_field(change, "value") == Some(10_000)
    }));
}

#[test]
fn incoming_damage_effects_follow_master_lifetimes_and_break_policy() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let member = |id, kind| {
        let mut status = empty_message(&proto, "blend.model.BattleCharacterStatus").unwrap();
        for field in ["attack", "defense", "hp", "magic", "mental", "speed"] {
            status.set_field_by_name(field, Value::I32(1_000));
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
        let mut member = empty_message(&proto, "blend.model.BattleMember").unwrap();
        member.set_field_by_name("member_id", Value::I32(id));
        member.set_field_by_name("type", Value::EnumNumber(kind));
        member.set_field_by_name("is_alive", Value::Bool(true));
        member.set_field_by_name("hp", Value::I32(1_000));
        member.set_field_by_name("max_hp", Value::I32(1_000));
        member.set_field_by_name("current_status", Value::Message(status.clone()));
        member.set_field_by_name("initial_status", Value::Message(status));
        member.set_field_by_name("resistance", Value::Message(resistance));
        member.set_field_by_name("state_changes", Value::List(Vec::new()));
        member.set_field_by_name("state_change_summaries", Value::List(Vec::new()));
        member
    };
    let mut state = empty_message(&proto, "blend.model.BattleState").unwrap();
    state.set_field_by_name(
        "members",
        Value::List(vec![
            Value::Message(member(1, 0)),
            Value::Message(member(2, 1)),
        ]),
    );
    let mut runtime = Runtime::default();
    runtime.prepare(&state, "incoming-effects").unwrap();
    let rules = registry().unwrap();
    let generic = rules.rules.iter().find(|rule| rule.id == 91000966).unwrap();
    let magic = rules.rules.iter().find(|rule| rule.id == 91001263).unwrap();
    let break_taken = rules.rules.iter().find(|rule| rule.id == 91000973).unwrap();
    let regeneration = rules
        .rules
        .iter()
        .find(|rule| rule.id == 780017004)
        .unwrap();
    assert_eq!(
        (
            regeneration.operation.as_str(),
            regeneration.target.as_str(),
            regeneration.state_id,
            regeneration.duration,
        ),
        ("regeneration", "targets", 910037, 3)
    );
    let regeneration = rules.rules.iter().find(|rule| rule.id == 91001093).unwrap();
    assert_eq!((regeneration.state_id, regeneration.duration), (910037, 2));
    let regeneration = rule_for(71183001, "active", "skill", 22000836)
        .unwrap()
        .unwrap();
    assert_eq!(
        (
            regeneration.operation.as_str(),
            regeneration.target.as_str(),
            regeneration.state_id,
            regeneration.duration,
        ),
        ("regeneration", "self", 910037, 3)
    );
    for (effect_id, owner_id, target, duration) in [
        (91001035, 14000581, "allies", 5),
        (91001123, 11000836, "self", 3),
        (91001124, 14000846, "self", 5),
        (91001357, 14003845, "self", 2),
        (91001493, 12002204, "targets", 1),
        (91001535, 12003419, "allies", 2),
        (91001535, 14002194, "allies", 2),
        (91001535, 32003043, "allies", 2),
        (780042025, 22001900, "self", 2),
        (780052003, 32002117, "targets", 3),
    ] {
        let regeneration = rule_for(effect_id, "active", "skill", owner_id)
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                regeneration.operation.as_str(),
                regeneration.target.as_str(),
                regeneration.state_id,
                regeneration.duration,
            ),
            ("regeneration", target, 910037, duration)
        );
    }

    runtime
        .apply_for_action(
            &proto,
            &mut state,
            1,
            0,
            &[TutorialSkillEffect {
                id: generic.id,
                value: 1_000,
            }],
            &[2],
            true,
            "after",
            None,
            10_000,
            b"test",
            "incoming-effects",
            1,
        )
        .unwrap();
    let target = message_list(&state, "members")
        .into_iter()
        .find(|member| i32_field(member, "member_id") == Some(2))
        .unwrap();
    assert_eq!(
        incoming_multiplier_with_runtime(&target, 1, Some(&runtime)),
        11_000
    );
    assert_eq!(
        incoming_multiplier_with_runtime(&target, 5, Some(&runtime)),
        11_000
    );
    assert!(message_list(&target, "state_changes").iter().any(|change| {
        i32_field(change, "state_change_id") == Some(generic.state_id)
            && i32_field(change, "rest_count") == Some(generic.duration)
    }));

    runtime
        .apply_for_action(
            &proto,
            &mut state,
            1,
            0,
            &[TutorialSkillEffect {
                id: magic.id,
                value: 2_000,
            }],
            &[2],
            true,
            "after",
            None,
            10_000,
            b"test",
            "incoming-effects",
            2,
        )
        .unwrap();
    let target = message_list(&state, "members")
        .into_iter()
        .find(|member| i32_field(member, "member_id") == Some(2))
        .unwrap();
    assert_eq!(
        incoming_multiplier_with_runtime(&target, 1, Some(&runtime)),
        11_000
    );
    assert_eq!(
        incoming_multiplier_with_runtime(&target, 5, Some(&runtime)),
        13_000
    );

    let skill = TutorialSkill {
        id: 0,
        skill_type: 1,
        skill_effect_type: 1,
        skill_power_type: 2,
        wait: 0,
        power: 1_000,
        break_power: 1_000,
        break_power_type: 1,
        attack_attributes: vec![1],
        skill_target_type: Some(3),
        effects: Vec::new(),
        limit_count: None,
        max_lamp: 0,
        require_command_value: false,
        skill_destination: None,
        state_change_application_rate: 10_000,
        hp_damage_bonus: None,
    };
    let source = message_list(&state, "members")
        .into_iter()
        .find(|member| i32_field(member, "member_id") == Some(1))
        .unwrap();
    let before = policy_break_damage(
        &source,
        &target,
        &skill,
        Some(&runtime),
        None,
        100,
        false,
        10_000,
    )
    .unwrap();
    let critical_skill = TutorialSkill {
        id: 11003048,
        effects: vec![TutorialSkillEffect {
            id: 91001310,
            value: 5_000,
        }],
        ..skill.clone()
    };
    assert!(
        policy_break_damage(
            &source,
            &target,
            &critical_skill,
            Some(&runtime),
            None,
            100,
            true,
            10_000,
        )
        .unwrap()
            > policy_break_damage(
                &source,
                &target,
                &critical_skill,
                Some(&runtime),
                None,
                100,
                false,
                10_000,
            )
            .unwrap()
    );
    runtime
        .apply_for_action(
            &proto,
            &mut state,
            1,
            0,
            &[TutorialSkillEffect {
                id: break_taken.id,
                value: 3_000,
            }],
            &[2],
            true,
            "after",
            None,
            10_000,
            b"test",
            "incoming-effects",
            3,
        )
        .unwrap();
    let members = message_list(&state, "members");
    let source = members
        .iter()
        .find(|member| i32_field(member, "member_id") == Some(1))
        .unwrap();
    let target = members
        .iter()
        .find(|member| i32_field(member, "member_id") == Some(2))
        .unwrap();
    assert_eq!(state_change_summary_value(target, 13), 3_000);
    assert!(
        policy_break_damage(
            source,
            target,
            &skill,
            Some(&runtime),
            None,
            100,
            false,
            10_000,
        )
        .unwrap()
            > before
    );
    runtime
        .apply_for_action(
            &proto,
            &mut state,
            1,
            0,
            &[TutorialSkillEffect {
                id: 76201002,
                value: 3_000,
            }],
            &[1],
            true,
            "after",
            None,
            10_000,
            b"test",
            "incoming-effects",
            4,
        )
        .unwrap();
    let source = message_list(&state, "members")
        .into_iter()
        .find(|member| i32_field(member, "member_id") == Some(1))
        .unwrap();
    assert_eq!(state_change_summary_value(&source, 13), -3_000);
    runtime
        .apply_for_action(
            &proto,
            &mut state,
            1,
            0,
            &[TutorialSkillEffect {
                id: 91000914,
                value: 1_500,
            }],
            &[2],
            true,
            "after",
            None,
            10_000,
            b"test",
            "incoming-effects",
            5,
        )
        .unwrap();
    let target = message_list(&state, "members")
        .into_iter()
        .find(|member| i32_field(member, "member_id") == Some(2))
        .unwrap();
    assert_eq!(state_change_summary_value(&target, 2), 1_500);

    let (skill_id, variants) = rules
        .rule_variants_by_skill
        .iter()
        .find_map(|(skill_id, effects)| {
            effects
                .get(&generic.id)
                .filter(|variants| {
                    variants.iter().any(|variant| variant.expiry == Expiry::Hit)
                        && variants
                            .iter()
                            .any(|variant| variant.expiry == Expiry::Turn)
                })
                .map(|variants| (*skill_id, variants.clone()))
        })
        .unwrap();
    runtime
        .apply_for_action(
            &proto,
            &mut state,
            1,
            skill_id,
            &[TutorialSkillEffect {
                id: generic.id,
                value: 400,
            }],
            &[2],
            true,
            "after",
            None,
            10_000,
            b"test",
            "incoming-effects",
            4,
        )
        .unwrap();
    let matching = || {
        runtime
            .instances
            .iter()
            .filter(|instance| instance.rule.id == generic.id && instance.target == 2)
            .collect::<Vec<_>>()
    };
    assert_eq!(matching().len(), variants.len());
    let mut hit = empty_message(&proto, "blend.model.BattleSkillResult").unwrap();
    hit.set_field_by_name("target_id", Value::I32(2));
    let hit_duration = variants
        .iter()
        .find(|variant| variant.expiry == Expiry::Hit)
        .unwrap()
        .duration;
    for _ in 0..hit_duration {
        runtime.expire(1, std::slice::from_ref(&hit), true, true);
    }
    runtime.refresh(&proto, &mut state).unwrap();
    let target = message_list(&state, "members")
        .into_iter()
        .find(|member| i32_field(member, "member_id") == Some(2))
        .unwrap();
    assert_eq!(state_change_summary_value(&target, 11), 400);
    let turn_duration = variants
        .iter()
        .find(|variant| variant.expiry == Expiry::Turn)
        .unwrap()
        .duration;
    for _ in 0..turn_duration {
        runtime.expire(2, &[], true, false);
    }
    runtime.refresh(&proto, &mut state).unwrap();
    let target = message_list(&state, "members")
        .into_iter()
        .find(|member| i32_field(member, "member_id") == Some(2))
        .unwrap();
    assert_eq!(state_change_summary_value(&target, 11), 0);
}
