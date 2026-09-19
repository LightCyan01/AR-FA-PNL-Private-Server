use crate::state::combat::prelude::*;
use std::path::Path;

#[test]
fn guard_effects_redirect_supported_scopes_and_apply_damage_down() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_tutorial_rules().unwrap();
    let gameplay = load_gameplay_rules().unwrap();
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
    let mut members = message_list(&state, "members");
    let protector = members
        .iter()
        .find(|member| member_type(member).ok() == Some(0))
        .and_then(|member| member_id(member).ok())
        .unwrap();
    let protected = protector + 100;
    let mut protected_member = members
        .iter()
        .find(|member| member_id(member).ok() == Some(protector))
        .unwrap()
        .clone();
    protected_member.set_field_by_name("member_id", Value::I32(protected));
    members.push(protected_member);
    runtime
        .bases
        .insert(protected, runtime.bases[&protector].clone());
    state.set_field_by_name(
        "members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );
    let allies = message_list(&state, "members")
        .iter()
        .filter(|member| member_type(member).ok() == Some(0))
        .filter_map(|member| member_id(member).ok())
        .collect::<Vec<_>>();
    let single = gameplay
        .skills
        .iter()
        .find(|skill| skill.skill_effect_type == 1 && skill.skill_target_type == Some(3))
        .unwrap();
    runtime
        .apply_for_action(
            &proto,
            &mut state,
            protector,
            20002039,
            &[TutorialSkillEffect {
                id: 780021002,
                value: 0,
            }],
            &[protector],
            true,
            "after",
            None,
            10_000,
            b"cover-test",
            "cover-test",
            0,
        )
        .unwrap();

    let (targets, protection) = runtime
        .redirect_targets(&state, 1, single, &[protected])
        .unwrap();
    assert_eq!(targets, vec![protector]);
    let protection = protection.unwrap();
    assert_eq!(
        (protection.protector_id, protection.state_change_id),
        (protector, 910038)
    );

    let mut hit = empty_message(&proto, "blend.model.BattleSkillResult").unwrap();
    hit.set_field_by_name("target_id", Value::I32(protector));
    runtime.expire(99, &[hit], false, true);
    assert_eq!(
        runtime
            .instances
            .iter()
            .find(|instance| instance.rule.id == 780021002)
            .unwrap()
            .remaining,
        2
    );

    for (skill_id, effect_id, duration) in [
        (22001178, 91001398, 2),
        (32000669, 780008001, 3),
        (20007567, 780053002, 3),
        (20002721, 780054002, 3),
    ] {
        runtime
            .apply_for_action(
                &proto,
                &mut state,
                protector,
                skill_id,
                &[TutorialSkillEffect {
                    id: effect_id,
                    value: 0,
                }],
                &[protector],
                true,
                "after",
                None,
                10_000,
                b"cover-test",
                "cover-test",
                1,
            )
            .unwrap();
        assert_eq!(
            runtime
                .instances
                .iter()
                .find(|instance| instance.rule.id == effect_id)
                .unwrap()
                .remaining,
            duration
        );
    }

    let all_cover = gameplay
        .skills
        .iter()
        .find(|skill| skill.id == 20002297)
        .unwrap();
    runtime
        .apply_for_action(
            &proto,
            &mut state,
            protector,
            all_cover.id,
            &all_cover.effects,
            &[protector],
            true,
            "after",
            None,
            10_000,
            b"cover-test",
            "cover-test",
            1,
        )
        .unwrap();
    let all_attack = gameplay
        .skills
        .iter()
        .find(|skill| skill.skill_effect_type == 1 && skill.skill_target_type == Some(5))
        .unwrap();
    let (targets, protection) = runtime
        .redirect_targets(&state, 1, all_attack, &allies)
        .unwrap();
    assert_eq!(targets, vec![protector; allies.len()]);
    assert_eq!(protection.unwrap().protector_id, protector);

    runtime
        .apply_for_action(
            &proto,
            &mut state,
            protector,
            20007548,
            &[TutorialSkillEffect {
                id: 780021001,
                value: 1_500,
            }],
            &[protected],
            true,
            "after",
            None,
            10_000,
            b"cover-test",
            "cover-test",
            2,
        )
        .unwrap();
    let protected = message_list(&state, "members")
        .into_iter()
        .find(|member| member_id(member).ok() == Some(protected))
        .unwrap();
    assert!(message_list(&protected, "state_changes")
        .iter()
        .any(|change| {
            i32_field(change, "state_change_id") == Some(560074)
                && i32_field(change, "value") == Some(1_500)
                && i32_field(change, "rest_count") == Some(3)
        }));
}
