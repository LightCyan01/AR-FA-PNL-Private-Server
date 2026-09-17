use super::super::*;
use crate::state::combat::prelude::*;
use std::path::Path;

#[test]
fn kachikochi_uses_master_rate_lifetime_and_critical_policy() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let gameplay = load_gameplay_rules().unwrap();
    for (skill_id, rate) in [
        (22000643, 7_000),
        (22001082, 5_000),
        (22000470, 5_000),
        (28000059, 5_000),
        (20008965, 10_000),
    ] {
        assert_eq!(
            gameplay
                .skills
                .iter()
                .find(|skill| skill.id == skill_id)
                .unwrap()
                .state_change_application_rate,
            rate
        );
    }
    for (effect_id, skill_id, target, expiry, duration) in [
        (71180011, 22000643, "targets", Expiry::Permanent, -1),
        (71193005, 22001082, "enemies", Expiry::Attacked, 2),
        (780120001, 20008965, "targets", Expiry::Attacked, 2),
        (780120005, 22000470, "targets", Expiry::Attacked, 2),
    ] {
        let rule = rule_for(effect_id, "active", "skill", skill_id)
            .unwrap()
            .unwrap();
        assert_eq!(
            (
                rule.operation.as_str(),
                rule.target.as_str(),
                rule.state_id,
                &rule.expiry,
                rule.duration,
            ),
            ("status", target, 610052, &expiry, duration)
        );
    }

    let fresh = load_fresh_rules().unwrap();
    let resources = reduce_talk_event(
        &proto,
        &fresh,
        &gameplay,
        starter_resources(&proto, &fresh).unwrap(),
        101001001,
        1,
    )
    .unwrap()
    .resources;
    let start = reduce_battle_start(&proto, &gameplay, resources, 101001002, 1).unwrap();
    let mut state = start.state;
    let source = message_list(&state, "members")
        .iter()
        .find(|member| member_type(member).ok() == Some(0))
        .and_then(|member| i32_field(member, "member_id"))
        .unwrap();
    let target = message_list(&state, "members")
        .iter()
        .find(|member| member_type(member).ok() == Some(1))
        .and_then(|member| i32_field(member, "member_id"))
        .unwrap();
    let skill = gameplay
        .skills
        .iter()
        .find(|skill| skill.id == 22000470)
        .unwrap();
    let action = (1..1_000)
        .find(|number| {
            deterministic_roll(
                b"kachikochi",
                &start.start_txid,
                *number,
                b"state-change",
                target,
                0,
            ) % 10_000
                < skill.state_change_application_rate as u32
        })
        .unwrap();
    let mut runtime = start.effects;
    runtime
        .apply_for_action(
            &proto,
            &mut state,
            source,
            skill.id,
            &skill.effects,
            &[target],
            true,
            "after",
            None,
            skill.state_change_application_rate,
            b"kachikochi",
            &start.start_txid,
            action,
        )
        .unwrap();
    let mut hit = empty_message(&proto, "blend.model.BattleSkillResult").unwrap();
    hit.set_field_by_name("target_id", Value::I32(target));
    let target_member = |state: &DynamicMessage| {
        message_list(state, "members")
            .into_iter()
            .find(|member| i32_field(member, "member_id") == Some(target))
            .unwrap()
    };
    assert!(receives_guaranteed_critical(&target_member(&state)));
    runtime.expire(target, std::slice::from_ref(&hit), false, true);
    runtime.refresh(&proto, &mut state).unwrap();
    assert!(receives_guaranteed_critical(&target_member(&state)));
    runtime.expire(target, &[hit], false, true);
    runtime.refresh(&proto, &mut state).unwrap();
    assert!(!receives_guaranteed_critical(&target_member(&state)));
}
