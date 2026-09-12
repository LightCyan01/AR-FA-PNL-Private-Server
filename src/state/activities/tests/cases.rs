use super::*;
use prost::Message;

#[test]
fn episode_and_season_skip_boundaries_use_master_order() {
    // Pure component fixtures, not seeded acceptance accounts. The natural
    // encrypted journey separately proves the reachable episode path.
    let proto = ProtoRegistry::from_file(std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let rules = load_rules().unwrap();
    let quest = load_gameplay_rules().unwrap();
    let home_rules = home::load_rules().unwrap();
    let atelier = atelier::load_rules().unwrap();
    let characters = load_character_rules().unwrap();
    let reward_rules = load_reward_rules().unwrap();
    let now = 1_789_000_000;
    for (route, target) in [
        ("/quest/talk_event/main_story_episode_skip", 101002059),
        ("/quest/talk_event/main_story_season_skip", 101025033),
    ] {
        let target = quest.quests.iter().find(|q| q.id == target).unwrap();
        let mut resources = starter_resources(&proto, &load_fresh_rules().unwrap()).unwrap();
        let mut status = status_message(&resources).unwrap();
        status.set_field_by_name("tutorial_step", Value::I32(home::TUTORIAL_STEP_HOME_READY));
        status.set_field_by_name(
            "last_main_story_quest_id",
            Value::I32(target.predecessor_id.unwrap()),
        );
        resources.set_field_by_name("status", Value::Message(status));
        for q in quest
            .quests
            .iter()
            .filter(|q| q.episode_type == 1 && q.id < target.id)
        {
            let mut state = empty_message(&proto, "blend.model.QuestState").unwrap();
            state.set_field_by_name("quest_id", Value::I32(q.id));
            state.set_field_by_name("clear_count", Value::I32(1));
            upsert_quest_state(&mut resources, state);
        }
        let mut changed = empty_message(&proto, "blend.model.Resources").unwrap();
        story_skip(
            &proto,
            &rules,
            &quest,
            &atelier,
            characters,
            &reward_rules,
            &home_rules,
            &mut resources,
            &mut changed,
            &ActivityState::default(),
            route,
            now,
        )
        .unwrap();
        assert_eq!(quest_clear_count(&resources, target.id), 1);
        assert_eq!(quest_clear_count(&changed, target.id), 1);
        let encoded = resources.encode_to_vec();
        resources = proto.decode("blend.model.Resources", &encoded).unwrap();
        assert!(story_skip(
            &proto,
            &rules,
            &quest,
            &atelier,
            characters,
            &reward_rules,
            &home_rules,
            &mut resources,
            &mut changed,
            &ActivityState::default(),
            route,
            now
        )
        .is_err());
        assert_eq!(encoded, resources.encode_to_vec());
    }
}

#[test]
fn batch_delta_merge_keeps_distinct_party_positions() {
    let proto = ProtoRegistry::from_file(std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let fresh = starter_resources(&proto, &load_fresh_rules().unwrap()).unwrap();
    let mut changed = empty_message(&proto, "blend.model.Resources").unwrap();
    merge(&mut changed, fresh.clone());
    merge(&mut changed, fresh.clone());
    assert_eq!(
        message_list(&changed, "party_members"),
        message_list(&fresh, "party_members")
    );
}
