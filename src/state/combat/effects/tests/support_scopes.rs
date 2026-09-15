use super::super::*;
use crate::state::combat::prelude::*;
use std::path::Path;

#[test]
fn common_support_effects_keep_their_scope_and_conditions() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let member = |member_id, member_type| {
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
        let mut member = empty_message(&proto, "blend.model.BattleMember").unwrap();
        member.set_field_by_name("member_id", Value::I32(member_id));
        member.set_field_by_name("type", Value::EnumNumber(member_type));
        member.set_field_by_name("is_alive", Value::Bool(true));
        member.set_field_by_name("hp", Value::I32(50));
        member.set_field_by_name("max_hp", Value::I32(100));
        member.set_field_by_name("current_status", Value::Message(status.clone()));
        member.set_field_by_name("initial_status", Value::Message(status));
        member.set_field_by_name("resistance", Value::Message(resistance));
        member.set_field_by_name("state_changes", Value::List(Vec::new()));
        member.set_field_by_name("state_change_summaries", Value::List(Vec::new()));
        member
    };
    let mut high_magic = member(2, 0);
    for field in ["current_status", "initial_status"] {
        let mut status = member_status(&high_magic, field).unwrap();
        status.set_field_by_name("magic", Value::I32(200));
        high_magic.set_field_by_name(field, Value::Message(status));
    }
    let mut state = empty_message(&proto, "blend.model.BattleState").unwrap();
    state.set_field_by_name(
        "members",
        Value::List(vec![
            Value::Message(member(1, 0)),
            Value::Message(high_magic),
            Value::Message(member(3, 1)),
        ]),
    );
    let mut runtime = Runtime::default();
    runtime.prepare(&state, "support-effects").unwrap();
    runtime
        .apply(
            &proto,
            &mut state,
            1,
            &[
                TutorialSkillEffect {
                    id: 780139022,
                    value: 1_000,
                },
                TutorialSkillEffect {
                    id: 780110001,
                    value: 2_000,
                },
                TutorialSkillEffect {
                    id: 91001017,
                    value: 500,
                },
                TutorialSkillEffect {
                    id: 91001631,
                    value: 2_000,
                },
            ],
            &[2, 3],
            true,
            "after",
            None,
        )
        .unwrap();
    runtime
        .apply(
            &proto,
            &mut state,
            1,
            &[
                TutorialSkillEffect {
                    id: 71156001,
                    value: 1_000,
                },
                TutorialSkillEffect {
                    id: 780004005,
                    value: 3_000,
                },
            ],
            &[2],
            true,
            "after",
            None,
        )
        .unwrap();
    runtime
        .apply(
            &proto,
            &mut state,
            1,
            &[TutorialSkillEffect {
                id: 91000916,
                value: 1_000,
            }],
            &[3],
            true,
            "after",
            None,
        )
        .unwrap();
    runtime
        .apply_inner(
            &proto,
            &mut state,
            1,
            12000576,
            &[TutorialSkillEffect {
                id: 91001069,
                value: 2_000,
            }],
            &[2],
            true,
            "after",
            None,
            10_000,
            None,
        )
        .unwrap();
    runtime
        .apply_inner(
            &proto,
            &mut state,
            1,
            12000871,
            &[TutorialSkillEffect {
                id: 91001069,
                value: 5_000,
            }],
            &[2],
            true,
            "after",
            None,
            10_000,
            None,
        )
        .unwrap();
    let broken_magic = [TutorialSkillEffect {
        id: 91001264,
        value: 5_000,
    }];
    runtime
        .apply_inner(
            &proto,
            &mut state,
            1,
            14002801,
            &broken_magic,
            &[3],
            true,
            "after",
            None,
            10_000,
            None,
        )
        .unwrap();
    assert_eq!(
        message_list(&state, "members")
            .into_iter()
            .find(|member| i32_field(member, "member_id") == Some(2))
            .and_then(|member| message_i32_field(&member, "current_status", "magic")),
        Some(200)
    );
    let mut members = message_list(&state, "members");
    let mut enemy_status = empty_message(&proto, "blend.model.BattleEnemy").unwrap();
    enemy_status.set_field_by_name("is_broken", Value::Bool(true));
    members
        .iter_mut()
        .find(|member| i32_field(member, "member_id") == Some(3))
        .unwrap()
        .set_field_by_name("enemy", Value::Message(enemy_status));
    state.set_field_by_name(
        "members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );
    runtime
        .apply_inner(
            &proto,
            &mut state,
            1,
            14002801,
            &broken_magic,
            &[3],
            true,
            "after",
            None,
            10_000,
            None,
        )
        .unwrap();

    let members = message_list(&state, "members");
    let source = members
        .iter()
        .find(|member| i32_field(member, "member_id") == Some(1))
        .unwrap();
    let ally = members
        .iter()
        .find(|member| i32_field(member, "member_id") == Some(2))
        .unwrap();
    let enemy = members
        .iter()
        .find(|member| i32_field(member, "member_id") == Some(3))
        .unwrap();
    assert_eq!(i32_field(source, "hp"), Some(60));
    assert_eq!(i32_field(ally, "hp"), Some(60));
    assert_eq!(i32_field(enemy, "hp"), Some(50));
    assert_eq!(state_change_summary_value(source, 1), 2_000);
    assert_eq!(state_change_summary_value(ally, 1), 3_000);
    assert_eq!(state_change_summary_value(enemy, 2), 1_000);
    assert_eq!(
        message_i32_field(source, "current_status", "speed"),
        Some(105)
    );
    assert_eq!(
        message_i32_field(source, "current_status", "attack"),
        Some(120)
    );
    assert_eq!(message_i32_field(ally, "current_status", "magic"), Some(300));
    assert_eq!(incoming_multiplier_with_runtime(source, 1, None), 8_000);
    assert_eq!(incoming_multiplier_with_runtime(ally, 1, None), 2_000);

    let skill = TutorialSkill {
        id: 12000051,
        skill_type: 1,
        skill_effect_type: 1,
        skill_power_type: 2,
        wait: 0,
        power: 100,
        break_power: 0,
        break_power_type: 1,
        attack_attributes: vec![5],
        skill_target_type: Some(3),
        effects: vec![TutorialSkillEffect {
            id: 91000906,
            value: 4_000,
        }],
        limit_count: None,
        max_lamp: 0,
        require_command_value: false,
        skill_destination: None,
        state_change_application_rate: 10_000,
        hp_damage_bonus: None,
    };
    let mut unbroken = enemy.clone();
    let mut enemy_status = member_status(&unbroken, "enemy").unwrap();
    enemy_status.set_field_by_name("is_broken", Value::Bool(false));
    unbroken.set_field_by_name("enemy", Value::Message(enemy_status));
    assert_eq!(instant_summary(&skill, &unbroken, false, 1).unwrap(), 0);
    let mut broken = unbroken;
    let mut enemy_status = empty_message(&proto, "blend.model.BattleEnemy").unwrap();
    enemy_status.set_field_by_name("is_broken", Value::Bool(true));
    broken.set_field_by_name("enemy", Value::Message(enemy_status));
    assert_eq!(instant_summary(&skill, &broken, false, 1).unwrap(), 4_000);
}

#[test]
fn self_heals_follow_the_originating_skill_phase() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let mut status = empty_message(&proto, "blend.model.BattleCharacterStatus").unwrap();
    for field in ["attack", "defense", "hp", "magic", "mental", "speed"] {
        status.set_field_by_name(field, Value::I32(100));
    }
    let mut member = empty_message(&proto, "blend.model.BattleMember").unwrap();
    member.set_field_by_name("member_id", Value::I32(1));
    member.set_field_by_name("type", Value::EnumNumber(0));
    member.set_field_by_name("is_alive", Value::Bool(true));
    member.set_field_by_name("hp", Value::I32(50));
    member.set_field_by_name("max_hp", Value::I32(100));
    member.set_field_by_name("current_status", Value::Message(status.clone()));
    member.set_field_by_name("initial_status", Value::Message(status));
    member.set_field_by_name("state_changes", Value::List(Vec::new()));
    member.set_field_by_name("state_change_summaries", Value::List(Vec::new()));
    let mut state = empty_message(&proto, "blend.model.BattleState").unwrap();
    state.set_field_by_name("members", Value::List(vec![Value::Message(member)]));
    let mut runtime = Runtime::default();
    runtime.prepare(&state, "phase-heals").unwrap();
    let before_heal = [TutorialSkillEffect {
        id: 780104005,
        value: 3_000,
    }];

    assert!(runtime
        .apply_inner(
            &proto,
            &mut state,
            1,
            20000383,
            &before_heal,
            &[1],
            true,
            "after",
            None,
            10_000,
            None,
        )
        .unwrap()
        .is_empty());
    assert_eq!(
        message_list(&state, "members")
            .into_iter()
            .next()
            .and_then(|member| i32_field(&member, "hp")),
        Some(50)
    );
    runtime
        .apply_inner(
            &proto,
            &mut state,
            1,
            20000383,
            &before_heal,
            &[1],
            true,
            "before",
            None,
            10_000,
            None,
        )
        .unwrap();
    runtime
        .apply_inner(
            &proto,
            &mut state,
            1,
            20000390,
            &[TutorialSkillEffect {
                id: 780109004,
                value: 1_500,
            }],
            &[1],
            true,
            "after",
            None,
            10_000,
            None,
        )
        .unwrap();
    assert_eq!(
        message_list(&state, "members")
            .into_iter()
            .next()
            .and_then(|member| i32_field(&member, "hp")),
        Some(95)
    );
}

#[test]
fn tutorial_skill_damage_effects_use_their_master_context() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let target = empty_message(&proto, "blend.model.BattleMember").unwrap();
    let critical = TutorialSkill {
        id: 11000496,
        skill_type: 1,
        skill_effect_type: 1,
        skill_power_type: 2,
        wait: 0,
        power: 1,
        break_power: 1,
        break_power_type: 1,
        attack_attributes: vec![3],
        skill_target_type: Some(3),
        effects: vec![TutorialSkillEffect {
            id: 91000904,
            value: 2_000,
        }],
        limit_count: None,
        max_lamp: 0,
        require_command_value: false,
        skill_destination: None,
        state_change_application_rate: 10_000,
        hp_damage_bonus: None,
    };
    assert_eq!(instant_summary(&critical, &target, true, 7).unwrap(), 2_000);
    assert_eq!(instant_summary(&critical, &target, false, 7).unwrap(), 0);
    let current_attack = TutorialSkill {
        effects: vec![TutorialSkillEffect {
            id: 3000044,
            value: 5_000,
        }],
        ..critical.clone()
    };
    assert_eq!(
        instant_summary(&current_attack, &target, false, 6).unwrap(),
        5_000
    );

    let mut state = empty_message(&proto, "blend.model.BattleState").unwrap();
    let mut actor = empty_message(&proto, "blend.model.BattleMember").unwrap();
    actor.set_field_by_name("member_id", Value::I32(1));
    actor.set_field_by_name("type", Value::EnumNumber(0));
    actor.set_field_by_name("is_alive", Value::Bool(true));
    actor.set_field_by_name(
        "ally",
        Value::Message({
            let mut ally = empty_message(&proto, "blend.model.BattleAlly").unwrap();
            ally.set_field_by_name("character_id", Value::I32(39901));
            ally
        }),
    );
    actor.set_field_by_name(
        "current_status",
        Value::Message({
            let mut status = empty_message(&proto, "blend.model.BattleCharacterStatus").unwrap();
            for field in ["attack", "defense", "hp", "magic", "mental", "speed"] {
                status.set_field_by_name(field, Value::I32(100));
            }
            status
        }),
    );
    actor.set_field_by_name(
        "initial_status",
        actor
            .get_field_by_name("current_status")
            .unwrap()
            .clone()
            .into_owned(),
    );
    actor.set_field_by_name("state_changes", Value::List(Vec::new()));
    actor.set_field_by_name("state_change_summaries", Value::List(Vec::new()));
    state.set_field_by_name("members", Value::List(vec![Value::Message(actor)]));
    let mut runtime = Runtime::default();
    runtime.prepare(&state, "effect-test").unwrap();
    runtime
        .apply(
            &proto,
            &mut state,
            1,
            &[TutorialSkillEffect {
                id: 91000958,
                value: 2_000,
            }],
            &[1],
            true,
            "after",
            None,
        )
        .unwrap();
    runtime
        .apply(
            &proto,
            &mut state,
            1,
            &[TutorialSkillEffect {
                id: 91000974,
                value: 2_500,
            }],
            &[1],
            true,
            "after",
            None,
        )
        .unwrap();
    let actor = message_list(&state, "members").into_iter().next().unwrap();
    assert_eq!(state_change_summary_value(&actor, 1), 2_500);
    assert_eq!(runtime.contextual_summary(&actor, &critical, false, 1), 0);
    let burst = TutorialSkill {
        skill_type: 3,
        ..critical.clone()
    };
    assert_eq!(
        i64::from(state_change_summary_value(&actor, 1))
            + runtime.contextual_summary(&actor, &burst, false, 1),
        4_500
    );
}

#[test]
fn target_physical_damage_down_changes_only_physical_multiplier() {
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
    let enemy_id = message_list(&state, "members")
        .iter()
        .find(|member| member_type(member).ok() == Some(1))
        .and_then(|member| i32_field(member, "member_id"))
        .unwrap();
    let mut runtime = start.effects;
    runtime.passives.clear();
    runtime.instances.clear();
    runtime.managed.clear();
    runtime.refresh(&proto, &mut state).unwrap();
    let effect = TutorialSkillEffect {
        id: 91000921,
        value: 5_000,
    };
    runtime
        .apply(
            &proto,
            &mut state,
            actor_id,
            &[effect],
            &[enemy_id],
            true,
            "after",
            None,
        )
        .unwrap();
    let enemy = message_list(&state, "members")
        .into_iter()
        .find(|member| i32_field(member, "member_id") == Some(enemy_id))
        .unwrap();
    assert_eq!(incoming_multiplier_with_runtime(&enemy, 1, None), 5_000);
    assert_eq!(incoming_multiplier_with_runtime(&enemy, 5, None), 10_000);
    assert!(message_list(&enemy, "state_changes").iter().any(|change| {
        i32_field(change, "state_change_id") == Some(920012)
            && i32_field(change, "value") == Some(-5_000)
    }));
}
