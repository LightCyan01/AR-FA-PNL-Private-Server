use super::super::*;
use crate::state::combat::prelude::*;
use std::path::Path;

#[test]
fn persisted_skill_ranks_evolution_and_locking_select_the_battle_skills() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_gameplay_rules().unwrap();
    let spec = rules
        .battle_characters
        .iter()
        .find(|c| {
            c.skills
                .iter()
                .any(|s| s.skill_type == 1 && !s.evolved_ids.is_empty())
        })
        .unwrap();
    let mut owned = empty_message(&proto, "blend.model.Character").unwrap();
    owned.set_field_by_name("normal1_skill_rank", Value::I32(1));
    owned.set_field_by_name("normal2_skill_rank", Value::I32(1));
    let base = selected_character_skills(&rules, spec.id, Some(&owned), 3).unwrap();
    owned.set_field_by_name("is_normal1_skill_evolved", Value::Bool(true));
    let evolved = selected_character_skills(&rules, spec.id, Some(&owned), 3).unwrap();
    let definition = spec.skills.iter().find(|s| s.skill_type == 1).unwrap();
    assert_eq!(
        evolved.iter().find(|s| s.skill_type == 1).unwrap().id,
        definition.evolved_ids[0]
    );
    assert_ne!(
        base.iter().find(|s| s.skill_type == 1).unwrap().id,
        evolved.iter().find(|s| s.skill_type == 1).unwrap().id
    );
    owned.set_field_by_name("is_normal1_skill_locked", Value::Bool(true));
    assert!(!selected_character_skills(&rules, spec.id, Some(&owned), 3)
        .unwrap()
        .iter()
        .any(|s| s.skill_type == 1));
    owned.set_field_by_name("is_normal2_skill_locked", Value::Bool(true));
    owned.set_field_by_name("is_burst_skill_locked", Value::Bool(true));
    assert!(selected_character_skills(&rules, spec.id, Some(&owned), 3).is_err());
}

#[test]
fn observed_master_damage_passives_apply_once() {
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
    let (mut party, _) = resolve_account_party(
        &rules,
        &atelier::load_rules().unwrap(),
        &load_character_rules().unwrap(),
        &resources,
        1,
    )
    .unwrap();
    apply_leader_passives(&rules, &mut party).unwrap();
    assert!(party.iter().all(|member| member
        .leader_passives
        .iter()
        .any(|effect| effect.id == 72500125 && effect.value == 5000)));

    let member = |character_id, is_leader| BattlePartyMember {
        character_id,
        level: 1,
        rarity: 3,
        memoria_id: None,
        position: 1,
        is_leader,
        integrated_stats: None,
        damage_bonus: 0,
        skills: Vec::new(),
        passives: Vec::new(),
        leader_passives: Vec::new(),
    };
    let mut scaled = vec![member(50101, true), member(50201, false)];
    apply_leader_passives(&rules, &mut scaled).unwrap();
    assert!(scaled
        .iter()
        .all(|member| [72500160, 72500161].into_iter().all(|id| member
            .leader_passives
            .iter()
            .any(|effect| effect.id == id && effect.value == 2000))));

    let start = reduce_battle_start(&proto, &rules, resources, 101001002, 1).unwrap();
    assert!(message_list(&start.state, "members")
        .iter()
        .filter(|member| member_type(member).ok() == Some(0))
        .all(|member| state_change_summary_value(member, 27) == 5000));
    let rule = registry()
        .unwrap()
        .rules
        .iter()
        .find(|rule| rule.id == 72500125)
        .unwrap();
    assert_eq!(
        (rule.mode.as_str(), rule.target.as_str(), rule.summary),
        ("passive", "self", 27)
    );
    for (id, target, operation, summary) in [
        (72000901, "self", "summary", 1),
        (72000907, "allies", "attack", 0),
        (72000908, "allies", "magic", 0),
        (72000909, "allies", "defense", 0),
        (72000946, "allies", "summary", 7),
    ] {
        let rule = registry()
            .unwrap()
            .rules
            .iter()
            .find(|rule| rule.id == id)
            .unwrap();
        assert_eq!(
            (
                rule.mode.as_str(),
                rule.target.as_str(),
                rule.operation.as_str(),
                rule.summary
            ),
            ("passive", target, operation, summary)
        );
    }
    let fire_aura = registry()
        .unwrap()
        .rules
        .iter()
        .find(|rule| rule.id == 72000926)
        .unwrap();
    let fire_member = message_list(&start.state, "members")
        .into_iter()
        .find(|member| message_i32_field(member, "ally", "character_id") == Some(43101))
        .unwrap();
    let mut slash_member = fire_member.clone();
    slash_member.set_field_by_name("member_id", Value::I32(2));
    let mut slash_ally = member_status(&slash_member, "ally").unwrap();
    slash_ally.set_field_by_name("character_id", Value::I32(43701));
    slash_member.set_field_by_name("ally", Value::Message(slash_ally));
    assert!(selected(fire_aura, &fire_member, &fire_member, &[]));
    assert!(!selected(fire_aura, &fire_member, &slash_member, &[]));

    let mut fixed_resources = starter_resources(&proto, &fresh).unwrap();
    let mut cleared = empty_message(&proto, "blend.model.QuestState").unwrap();
    cleared.set_field_by_name("quest_id", Value::I32(101014004));
    cleared.set_field_by_name("clear_count", Value::I32(1));
    upsert_quest_state(&mut fixed_resources, cleared);
    let fixed =
        reduce_battle_start(&proto, &rules, fixed_resources, 101014005, 1_789_000_000).unwrap();
    let leader = message_list(&fixed.state, "members")
        .into_iter()
        .find(|member| message_i32_field(member, "ally", "character_id") == Some(43701))
        .unwrap();
    assert!(message_list(&leader, "state_changes").iter().any(|change| {
        i32_field(change, "state_change_id") == Some(560074)
            && i32_field(change, "value") == Some(3000)
            && i32_field(change, "rest_count") == Some(2)
    }));
    assert_eq!(incoming_multiplier_with_runtime(&leader, 1, None), 7000);
}

#[test]
fn conditional_memoria_and_equipment_follow_attack_context() {
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
    let mut clean_state = start.state.clone();
    let clean_members = message_list(&clean_state, "members")
        .into_iter()
        .map(|mut member| {
            member.set_field_by_name("state_changes", Value::List(Vec::new()));
            member.set_field_by_name("state_change_summaries", Value::List(Vec::new()));
            Value::Message(member)
        })
        .collect();
    clean_state.set_field_by_name("members", Value::List(clean_members));
    let actor_id = message_list(&clean_state, "members")
        .into_iter()
        .find(|member| member_type(member).ok() == Some(0))
        .and_then(|member| i32_field(&member, "member_id"))
        .unwrap();
    let enemy_id = message_list(&clean_state, "members")
        .into_iter()
        .find(|member| member_type(member).ok() == Some(1))
        .and_then(|member| i32_field(&member, "member_id"))
        .unwrap();
    let actor = || {
        message_list(&clean_state, "members")
            .into_iter()
            .find(|member| i32_field(member, "member_id") == Some(actor_id))
            .unwrap()
    };
    let physical = rules
        .skills
        .iter()
        .find(|skill| {
            skill.skill_effect_type == 1
                && skill
                    .attack_attributes
                    .iter()
                    .any(|attribute| (1..=3).contains(attribute))
        })
        .unwrap()
        .clone();
    let magic = rules
        .skills
        .iter()
        .find(|skill| {
            skill.skill_effect_type == 1
                && skill
                    .attack_attributes
                    .iter()
                    .any(|attribute| (5..=8).contains(attribute))
        })
        .unwrap()
        .clone();
    let burst = rules
        .skills
        .iter()
        .find(|skill| skill.skill_effect_type == 1 && skill.skill_type == 3)
        .unwrap()
        .clone();
    let wind = rules
        .skills
        .iter()
        .find(|skill| skill.skill_effect_type == 1 && skill.attack_attributes.contains(&8))
        .unwrap()
        .clone();
    let base_runtime = || {
        let mut runtime = start.effects.clone();
        runtime.passives.clear();
        runtime.instances.clear();
        runtime.managed.clear();
        runtime.unsupported.clear();
        runtime
    };
    let add_passive = |runtime: &mut Runtime, id: i32, value: i32, source_character_id: i32| {
        runtime.passives.push(Passive {
            source: actor_id,
            value,
            rule: registry()
                .unwrap()
                .rules
                .iter()
                .find(|rule| rule.id == id)
                .unwrap()
                .clone(),
            source_character_id,
            source_type: 0,
        });
    };

    let mut contextual = base_runtime();
    add_passive(&mut contextual, 3000009, 250, 0);
    add_passive(&mut contextual, 95000104, 350, 33402);
    add_passive(&mut contextual, 95000133, 450, 22001);
    assert_eq!(
        contextual.contextual_summary(&actor(), &physical, false, 1),
        0
    );
    assert_eq!(
        contextual.contextual_summary(&actor(), &magic, false, 1),
        250
    );
    assert_eq!(
        contextual.contextual_summary(&actor(), &burst, false, 1),
        600
    );
    assert_eq!(
        contextual.contextual_summary(&actor(), &burst, true, 7),
        450
    );
    assert_eq!(contextual.contextual_summary(&actor(), &burst, false, 7), 0);

    let mut static_state = clean_state.clone();
    let mut static_runtime = base_runtime();
    let actor_character_id = message_i32_field(&actor(), "ally", "character_id").unwrap_or(0);
    add_passive(&mut static_runtime, 3000027, -1_000, actor_character_id);
    static_runtime.refresh(&proto, &mut static_state).unwrap();
    let static_actor = message_list(&static_state, "members")
        .into_iter()
        .find(|member| i32_field(member, "member_id") == Some(actor_id))
        .unwrap();
    assert_eq!(
        incoming_multiplier_with_runtime(&static_actor, 1, None),
        10_000
    );
    assert_eq!(
        incoming_multiplier_with_runtime(&static_actor, 5, None),
        9_000
    );

    let mut trigger_state = clean_state.clone();
    let mut trigger_runtime = base_runtime();
    add_passive(&mut trigger_runtime, 95000145, 500, actor_character_id);
    trigger_runtime.refresh(&proto, &mut trigger_state).unwrap();
    let trigger_enemy = message_list(&trigger_state, "members")
        .into_iter()
        .find(|member| i32_field(member, "member_id") == Some(enemy_id))
        .unwrap();
    assert!(!message_list(&trigger_enemy, "state_changes")
        .iter()
        .any(|change| i32_field(change, "state_change_id") == Some(910091)));
    let hit = build_skill_result(
        &proto, enemy_id, 1, 0, 0, 0, true, false, false, false, false, false, false,
    )
    .unwrap();
    assert_eq!(
        trigger_runtime
            .trigger_attack_after(
                &proto,
                &mut trigger_state,

                actor_id,
                &physical,
                &[hit.clone()],
            )
            .unwrap()
            .len(),
        1
    );
    let triggered_enemy = message_list(&trigger_state, "members")
        .into_iter()
        .find(|member| i32_field(member, "member_id") == Some(enemy_id))
        .unwrap();
    let physical_down = message_list(&triggered_enemy, "state_changes")
        .into_iter()
        .find(|change| i32_field(change, "state_change_id") == Some(910091))
        .unwrap();
    assert_eq!(i32_field(&physical_down, "rest_count"), Some(2));
    trigger_runtime.expire(actor_id, &[hit.clone()], true, true);
    trigger_runtime.refresh(&proto, &mut trigger_state).unwrap();
    let once_enemy = message_list(&trigger_state, "members")
        .into_iter()
        .find(|member| i32_field(member, "member_id") == Some(enemy_id))
        .unwrap();
    assert_eq!(
        message_list(&once_enemy, "state_changes")
            .into_iter()
            .find(|change| i32_field(change, "state_change_id") == Some(910091))
            .and_then(|change| i32_field(&change, "rest_count")),
        Some(1)
    );
    trigger_runtime.expire(actor_id, &[hit], true, true);
    trigger_runtime.refresh(&proto, &mut trigger_state).unwrap();
    assert!(!message_list(
        &message_list(&trigger_state, "members")
            .into_iter()
            .find(|member| i32_field(member, "member_id") == Some(enemy_id))
            .unwrap(),
        "state_changes"
    )
    .iter()
    .any(|change| i32_field(change, "state_change_id") == Some(910091)));

    let wind_source = registry()
        .unwrap()
        .rules
        .iter()
        .find(|rule| rule.id == 95000156)
        .unwrap()
        .source_character_ids[0];
    let mut wind_state = clean_state.clone();
    let wind_members = message_list(&wind_state, "members")
        .into_iter()
        .map(|mut member| {
            if i32_field(&member, "member_id") == Some(actor_id) {
                let mut ally = member_status(&member, "ally").unwrap();
                ally.set_field_by_name("character_id", Value::I32(wind_source));
                member.set_field_by_name("ally", Value::Message(ally));
            }
            Value::Message(member)
        })
        .collect();
    wind_state.set_field_by_name("members", Value::List(wind_members));
    let mut wind_runtime = base_runtime();
    add_passive(&mut wind_runtime, 95000156, 1_000, wind_source);
    wind_runtime.refresh(&proto, &mut wind_state).unwrap();
    let wind_actor = message_list(&wind_state, "members")
        .into_iter()
        .find(|member| i32_field(member, "member_id") == Some(actor_id))
        .unwrap();
    let wind_enemy = message_list(&wind_state, "members")
        .into_iter()
        .find(|member| i32_field(member, "member_id") == Some(enemy_id))
        .unwrap();
    let before_critical = secondary_damage(
        10_000,
        &wind_actor,
        &wind_enemy,
        &wind,
        Some(&wind_runtime),
        true,
    )
    .unwrap();
    let wind_hit = build_skill_result(
        &proto, enemy_id, 1, 0, 0, 0, true, false, false, false, false, false, false,
    )
    .unwrap();
    wind_runtime
        .trigger_attack_after(&proto, &mut wind_state, actor_id, &wind, &[wind_hit])
        .unwrap();
    let wind_enemy = message_list(&wind_state, "members")
        .into_iter()
        .find(|member| i32_field(member, "member_id") == Some(enemy_id))
        .unwrap();
    assert_eq!(state_change_summary_value(&wind_enemy, 17), 1_000);
    assert!(
        secondary_damage(
            10_000,
            &wind_actor,
            &wind_enemy,
            &wind,
            Some(&wind_runtime),
            true,
        )
        .unwrap()
            > before_critical
    );
}
