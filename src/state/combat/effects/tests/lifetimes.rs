use super::super::*;
use crate::state::combat::prelude::*;
use prost::Message;
use std::path::Path;

#[test]
fn compound_evasion_skills_bind_only_the_evasion_effect() {
    for (effect_id, skill_id, target) in [
        (91001209, 12002705, "self"),
        (91001674, 14002500, "allies"),
    ] {
        let rule = rule_for(effect_id, "active", "skill", skill_id)
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                rule.operation.as_str(),
                rule.target.as_str(),
                &rule.expiry,
                rule.duration,
            ),
            ("evasion", target, &Expiry::Attacked, 1)
        );
    }
    for (effect_id, skill_id) in [
        (91001671, 12002705),
        (91001694, 12002705),
        (91001699, 12002705),
        (91001700, 12002705),
        (91001701, 14002500),
    ] {
        if let Some(rule) = rule_for(effect_id, "active", "skill", skill_id).unwrap() {
            assert_ne!(rule.operation, "evasion");
        }
    }
}

#[test]
fn combat_effects_persist_expire_and_preserve_conditional_passives() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_tutorial_rules().unwrap();
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
    let mut state = start.state;
    let mut runtime = start.effects;
    runtime.passives.clear();
    runtime.refresh(&proto, &mut state).unwrap();
    let actor = member_id(&current_actor(&state).unwrap()).unwrap();
    let summary = |state: &DynamicMessage| {
        state_change_summary_value(
            &message_list(state, "members")
                .into_iter()
                .find(|m| i32_field(m, "member_id") == Some(actor))
                .unwrap(),
            1,
        )
    };
    let baseline = summary(&state);
    let base_incoming = {
        let member = message_list(&state, "members")
            .into_iter()
            .find(|member| i32_field(member, "member_id") == Some(actor))
            .unwrap();
        incoming_multiplier_with_runtime(&member, 1, None)
    };
    runtime
        .apply(
            &proto,
            &mut state,
            actor,
            &[TutorialSkillEffect {
                id: 780109003,
                value: 2_000,
            }],
            &[actor],
            true,
            "after",
            None,
        )
        .unwrap();
    let reduced = message_list(&state, "members")
        .into_iter()
        .find(|member| i32_field(member, "member_id") == Some(actor))
        .unwrap();
    assert_eq!(
        incoming_multiplier_with_runtime(&reduced, 1, None),
        base_incoming - 2_000
    );
    let mut hit = empty_message(&proto, "blend.model.BattleSkillResult").unwrap();
    hit.set_field_by_name("target_id", Value::I32(actor));
    runtime.expire(0, std::slice::from_ref(&hit), true, true);
    assert_eq!(runtime.instances[0].remaining, 1);
    runtime.expire(0, &[hit], true, true);
    runtime.refresh(&proto, &mut state).unwrap();
    let restored = message_list(&state, "members")
        .into_iter()
        .find(|member| i32_field(member, "member_id") == Some(actor))
        .unwrap();
    assert_eq!(
        incoming_multiplier_with_runtime(&restored, 1, None),
        base_incoming
    );
    let mut passive = registry()
        .unwrap()
        .rules
        .iter()
        .find(|r| r.id == 3000011)
        .unwrap()
        .clone();
    passive.condition.insert("hp_min".into(), 50);
    runtime.passives.push(Passive {
        source: actor,
        value: 500,
        rule: passive,
        source_character_id: message_i32_field(
            &message_list(&state, "members")
                .into_iter()
                .find(|member| i32_field(member, "member_id") == Some(actor))
                .unwrap(),
            "ally",
            "character_id",
        )
        .unwrap_or_default(),
        source_type: 0,
    });
    runtime.refresh(&proto, &mut state).unwrap();
    assert_eq!(summary(&state), baseline + 500);
    let effect = TutorialSkillEffect {
        id: 91001006,
        value: 700,
    };
    runtime
        .apply(
            &proto,
            &mut state,
            actor,
            std::slice::from_ref(&effect),
            &[11],
            true,
            "after",
            None,
        )
        .unwrap();
    runtime
        .apply(
            &proto,
            &mut state,
            actor,
            &[effect],
            &[11],
            true,
            "after",
            None,
        )
        .unwrap();
    assert_eq!(summary(&state), baseline + 1200); // refresh same source, not double-stack
    runtime.expire(actor, &[], false, false); // a tool action is not an owner turn
    assert_eq!(runtime.instances[0].remaining, 2);
    runtime.expire(actor, &[], true, false);
    let saved = serde_json::to_vec(&runtime).unwrap();
    let saved_state = state.encode_to_vec();
    let mut runtime: Runtime = serde_json::from_slice(&saved).unwrap();
    let mut state = proto
        .decode("blend.model.BattleState", &saved_state)
        .unwrap();
    runtime.prepare(&state, &start.start_txid).unwrap();
    runtime.expire(actor, &[], true, false);
    runtime.refresh(&proto, &mut state).unwrap();
    assert_eq!(summary(&state), baseline + 500); // expiry must not erase equipment/passive buckets
    let mut members = message_list(&state, "members");
    members
        .iter_mut()
        .find(|m| i32_field(m, "member_id") == Some(actor))
        .unwrap()
        .set_field_by_name("hp", Value::I32(1));
    state.set_field_by_name(
        "members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );
    runtime.refresh(&proto, &mut state).unwrap();
    assert_eq!(summary(&state), baseline); // owner-HP condition reevaluates after resume
    let before = state.clone();
    let result = runtime
        .apply(
            &proto,
            &mut state,
            actor,
            &[TutorialSkillEffect {
                id: 999_999_999,
                value: 9999,
            }],
            &[11],
            true,
            "after",
            None,
        )
        .unwrap();
    assert!(result.is_empty() && runtime.unsupported.contains(&999_999_999));
    assert_eq!(state, before); // unknown effects must not be approximated as damage buffs
    runtime
        .apply(
            &proto,
            &mut state,
            actor,
            &[TutorialSkillEffect {
                id: 3000049,
                value: 700,
            }],
            &[11],
            false,
            "after",
            None,
        )
        .unwrap();
    let mut members = message_list(&state, "members");
    let enemy = members
        .iter_mut()
        .find(|m| i32_field(m, "member_id") == Some(11))
        .unwrap();
    let mut stats = member_status(enemy, "current_status").unwrap();
    stats.set_field_by_name("magic", Value::I32(777));
    enemy.set_field_by_name("current_status", Value::Message(stats));
    enemy.set_field_by_name("state_changes", Value::List(Vec::new()));
    state.set_field_by_name(
        "members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );
    state.set_field_by_name("wave", Value::I32(2));
    runtime.refresh(&proto, &mut state).unwrap();
    let enemy = message_list(&state, "members")
        .into_iter()
        .find(|m| i32_field(m, "member_id") == Some(11))
        .unwrap();
    assert_eq!(
        message_i32_field(&enemy, "current_status", "magic"),
        Some(777)
    );
    assert!(message_list(&enemy, "state_changes").is_empty());
}
#[test]
fn reused_effect_ids_keep_their_skill_specific_recipients() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_tutorial_rules().unwrap();
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
    let source = message_list(&start.state, "members")
        .into_iter()
        .find(|member| member_type(member).ok() == Some(0))
        .and_then(|member| member_id(&member).ok())
        .unwrap();
    let enemy = message_list(&start.state, "members")
        .into_iter()
        .find(|member| member_type(member).ok() == Some(1))
        .and_then(|member| member_id(&member).ok())
        .unwrap();

    let mut enemy_state = start.state.clone();
    let mut enemy_runtime = start.effects.clone();
    enemy_runtime.passives.clear();
    enemy_runtime
        .apply_inner(
            &proto,
            &mut enemy_state,
            source,
            32002362,
            &[TutorialSkillEffect {
                id: 780109002,
                value: 3_000,
            }],
            &[enemy],
            true,
            "after",
            None,
            10_000,
            None,
        )
        .unwrap();
    let enemy_recipients = enemy_runtime
        .instances
        .iter()
        .filter(|instance| instance.rule.id == 780109002)
        .map(|instance| instance.target)
        .collect::<Vec<_>>();
    assert!(enemy_recipients.contains(&enemy));
    assert!(!enemy_recipients.contains(&source));

    let mut target_state = start.state;
    let mut target_runtime = start.effects;
    target_runtime.passives.clear();
    target_runtime
        .apply_inner(
            &proto,
            &mut target_state,
            source,
            32000573,
            &[TutorialSkillEffect {
                id: 91000997,
                value: 3_500,
            }],
            &[enemy],
            true,
            "after",
            None,
            10_000,
            None,
        )
        .unwrap();
    assert!(target_runtime
        .instances
        .iter()
        .any(|instance| { instance.rule.id == 91000997 && instance.target == enemy }));
    assert!(!target_runtime
        .instances
        .iter()
        .any(|instance| { instance.rule.id == 91000997 && instance.target == source }));
}
