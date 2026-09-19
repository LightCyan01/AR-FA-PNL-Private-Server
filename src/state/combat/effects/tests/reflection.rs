use crate::state::combat::prelude::*;
use std::path::Path;

#[test]
fn magic_reflection_consumes_only_qualifying_hits() {
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
    let members = message_list(&state, "members");
    let reflector = members
        .iter()
        .find(|member| member_type(member).ok() == Some(0))
        .and_then(|member| member_id(member).ok())
        .unwrap();
    let attacker = members
        .iter()
        .find(|member| member_type(member).ok() == Some(1))
        .and_then(|member| member_id(member).ok())
        .unwrap();
    runtime
        .apply(
            &proto,
            &mut state,
            reflector,
            &[TutorialSkillEffect {
                id: 780025015,
                value: 2_000,
            }],
            &[reflector],
            true,
            "after",
            None,
        )
        .unwrap();
    let mut magic = gameplay
        .skills
        .iter()
        .find(|skill| skill.skill_effect_type == 1)
        .unwrap()
        .clone();
    magic.attack_attributes = vec![5];
    let hit = build_skill_result(
        &proto, reflector, 100, 0, 0, 0, true, false, false, false, false, false, false,
    )
    .unwrap();
    let hp = message_list(&state, "members")
        .into_iter()
        .find(|member| member_id(member).ok() == Some(attacker))
        .and_then(|member| i32_field(&member, "hp"))
        .unwrap();

    let (reflected, _) = runtime
        .reflect_magic_damage(
            &proto,
            &mut state,
            attacker,
            &magic,
            std::slice::from_ref(&hit),
        )
        .unwrap();
    runtime.expire_with_attributes(
        attacker,
        std::slice::from_ref(&hit),
        true,
        true,
        &magic.attack_attributes,
    );
    assert_eq!(
        reflected
            .first()
            .and_then(|result| message_i64_field(result, "hp_damage", "value")),
        Some(20)
    );
    assert_eq!(
        runtime
            .instances
            .iter()
            .find(|instance| instance.rule.operation == "reflection")
            .unwrap()
            .remaining,
        1
    );

    let mut physical = magic.clone();
    physical.attack_attributes = vec![1];
    let (physical_reflection, _) = runtime
        .reflect_magic_damage(
            &proto,
            &mut state,
            attacker,
            &physical,
            std::slice::from_ref(&hit),
        )
        .unwrap();
    runtime.expire_with_attributes(
        attacker,
        std::slice::from_ref(&hit),
        true,
        true,
        &physical.attack_attributes,
    );
    assert!(physical_reflection.is_empty());
    assert_eq!(
        message_list(&state, "members")
            .into_iter()
            .find(|member| member_id(member).ok() == Some(attacker))
            .and_then(|member| i32_field(&member, "hp")),
        Some(hp - 20)
    );
    assert!(runtime
        .reflect_magic_damage(&proto, &mut state, attacker, &magic, &reflected)
        .unwrap()
        .0
        .is_empty());
    let (final_reflection, _) = runtime
        .reflect_magic_damage(
            &proto,
            &mut state,
            attacker,
            &magic,
            std::slice::from_ref(&hit),
        )
        .unwrap();
    runtime.expire_with_attributes(
        attacker,
        std::slice::from_ref(&hit),
        true,
        true,
        &magic.attack_attributes,
    );
    assert_eq!(final_reflection.len(), 1);
    assert!(!runtime
        .instances
        .iter()
        .any(|instance| instance.rule.operation == "reflection"));
}
