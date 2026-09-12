use std::path::Path;

use super::*;

fn proto() -> ProtoRegistry {
    ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap()
}

fn add_memoria(
    proto: &ProtoRegistry,
    resources: &mut DynamicMessage,
    entity_id: i32,
    memoria_id: i32,
) {
    let mut memoria = empty_message(proto, "blend.model.Memoria").unwrap();
    memoria.set_field_by_name("entity_id", Value::I32(entity_id));
    memoria.set_field_by_name("memoria_id", Value::I32(memoria_id));
    memoria.set_field_by_name("received_at", Value::Message(timestamp(proto, 1).unwrap()));
    home::put(resources, "memorias", "entity_id", memoria);
}

#[test]
fn complete_atelier_player_simulation() {
    let proto = proto();
    let fresh = load_fresh_rules().unwrap();
    let rules = load_rules().unwrap();
    let synthesis = load_synthesis_rules().unwrap();
    let home_rules = home::load_rules().unwrap();
    let mut resources = starter_resources(&proto, &fresh).unwrap();
    let mut home = home::HomeState::default();
    let now = 2_000_000_000;

    let mut status = status_message(&resources).unwrap();
    status.set_field_by_name("cole", Value::I32(2_000_000));
    status.set_field_by_name("mana_when_updated", Value::I32(1_000));
    status.set_field_by_name("minimum_character_level", Value::I32(100));
    status.set_field_by_name(
        "mana_updated_at",
        Value::Message(timestamp(&proto, now).unwrap()),
    );
    resources.set_field_by_name("status", Value::Message(status));
    for (id, quantity) in [(98, 10), (1268, 10), (1286, 10), (1320, 10)] {
        change_item(&proto, &mut resources, id, quantity).unwrap();
    }

    let character_id = rules.communications[0].character_id;
    upsert_character(
        &mut resources,
        character_message(&proto, i64::from(character_id), None, Some(1), 3, 10).unwrap(),
        false,
    );
    let memoria_ids = rules
        .memorias
        .iter()
        .take(7)
        .map(|row| row.id)
        .collect::<Vec<_>>();
    for (index, memoria_id) in memoria_ids.iter().copied().enumerate() {
        add_memoria(&proto, &mut resources, 200 + index as i32, memoria_id);
    }
    add_memoria(&proto, &mut resources, 210, memoria_ids[2]);

    let run = |resources: &mut DynamicMessage,
               home: &mut home::HomeState,
               route: &str,
               request: &DynamicMessage,
               response_name: &str| {
        let mut changed = empty_message(&proto, "blend.model.Resources").unwrap();
        let mut response = empty_message(&proto, response_name).unwrap();
        apply(
            &proto,
            &rules,
            &synthesis,
            &home_rules,
            resources,
            &mut changed,
            &mut response,
            home,
            route,
            request,
            now,
        )
        .unwrap_or_else(|error| panic!("{route}: {error:?}"));
        (changed, response)
    };

    let mut research_request = empty_message(&proto, "blend.api.AtelierResearchRequest").unwrap();
    research_request.set_field_by_name("group_id", Value::I32(1));
    research_request.set_field_by_name("count", Value::I32(2));
    run(
        &mut resources,
        &mut home,
        "/atelier/research",
        &research_request,
        "blend.api.ChangedResourcesResponse",
    );
    assert_eq!(
        i32_field(&message_list(&resources, "research_groups")[0], "level"),
        Some(2)
    );
    assert_eq!(
        total_task_count(&resources, rules.constants.research_task_condition_ids[&1]),
        2
    );

    let mut battle_tool = empty_message(&proto, "blend.model.BattleTool").unwrap();
    battle_tool.set_field_by_name("entity_id", Value::I32(100));
    battle_tool.set_field_by_name("tool_id", Value::I32(rules.battle_tools[0].id));
    battle_tool.set_field_by_name(
        "traits",
        Value::List(vec![Value::Message(
            trait_params_message(&proto, &TutorialTraitParam { id: 1, rank: 1 }).unwrap(),
        )]),
    );
    home::put(&mut resources, "battle_tools", "entity_id", battle_tool);
    let mut equipment_tool = empty_message(&proto, "blend.model.EquipmentTool").unwrap();
    equipment_tool.set_field_by_name("entity_id", Value::I32(101));
    equipment_tool.set_field_by_name("tool_id", Value::I32(rules.equipment_tools[0].id));
    equipment_tool.set_field_by_name(
        "traits",
        Value::List(vec![Value::Message(
            trait_params_message(&proto, &TutorialTraitParam { id: 1, rank: 3 }).unwrap(),
        )]),
    );
    home::put(
        &mut resources,
        "equipment_tools",
        "entity_id",
        equipment_tool,
    );
    let tool_entity = |resource_type, entity_id| {
        let mut tool = empty_message(&proto, "blend.model.ToolEntity").unwrap();
        tool.set_field_by_name("type", Value::I32(resource_type));
        tool.set_field_by_name("entity_id", Value::I32(entity_id));
        tool
    };
    let mut lock = empty_message(&proto, "blend.api.ToolLockRequest").unwrap();
    lock.set_field_by_name("tool", Value::Message(tool_entity(14, 100)));
    lock.set_field_by_name("is_locked", Value::Bool(true));
    run(
        &mut resources,
        &mut home,
        "/tool/lock",
        &lock,
        "blend.api.ChangedResourcesResponse",
    );
    lock.set_field_by_name("is_locked", Value::Bool(false));
    run(
        &mut resources,
        &mut home,
        "/tool/lock",
        &lock,
        "blend.api.ChangedResourcesResponse",
    );
    let mut rank = empty_message(&proto, "blend.api.ToolTraitRankUpRequest").unwrap();
    rank.set_field_by_name("tool", Value::Message(tool_entity(14, 100)));
    rank.set_field_by_name("trait_index", Value::I32(0));
    run(
        &mut resources,
        &mut home,
        "/tool/trait_rank_up",
        &rank,
        "blend.api.ChangedResourcesResponse",
    );
    let mut convert = empty_message(&proto, "blend.api.ToolConvertRequest").unwrap();
    convert.set_field_by_name(
        "consumed_tools",
        Value::List(vec![
            Value::Message(tool_entity(14, 100)),
            Value::Message(tool_entity(6, 101)),
        ]),
    );
    let coins_before = item_quantity(&resources, 134).unwrap_or(0);
    let (_, converted) = run(
        &mut resources,
        &mut home,
        "/tool/convert",
        &convert,
        "blend.api.ToolConvertResponse",
    );
    assert_eq!(
        i32_list(
            &member_status(&converted, "deleted_resources").unwrap(),
            "battle_tool_entity_ids"
        ),
        vec![100]
    );
    assert!(!owned(&resources, "equipment_tools", 101));
    assert_eq!(
        item_quantity(&resources, 134).unwrap_or(0) - coins_before,
        11
    );

    let story1 = rules
        .communications
        .iter()
        .find(|row| row.character_id == character_id && row.story_number == 1)
        .unwrap();
    let mut story = empty_message(&proto, "blend.api.CommunicationStoryClearRequest").unwrap();
    story.set_field_by_name("character_id", Value::I32(character_id));
    story.set_field_by_name("story_number", Value::I32(1));
    run(
        &mut resources,
        &mut home,
        "/communication/story_clear",
        &story,
        "blend.api.CommunicationStoryClearResponse",
    );
    let story2 = rules
        .communications
        .iter()
        .find(|row| row.character_id == character_id && row.story_number == 2)
        .unwrap();
    let task = story2.key_tasks.first().unwrap();
    set_total_task_count(&mut resources, task.condition_id, task.count).unwrap();
    let mut release = empty_message(&proto, "blend.api.CommunicationStoryReleaseRequest").unwrap();
    release.set_field_by_name("character_id", Value::I32(character_id));
    release.set_field_by_name("story_number", Value::I32(2));
    run(
        &mut resources,
        &mut home,
        "/communication/story_release",
        &release,
        "blend.api.ChangedResourcesResponse",
    );
    story.set_field_by_name("story_number", Value::I32(2));
    run(
        &mut resources,
        &mut home,
        "/communication/story_clear",
        &story,
        "blend.api.CommunicationStoryClearResponse",
    );
    assert_eq!(story1.rewards[0].quantity, 100);

    let mut book = empty_message(&proto, "blend.api.IllustratedBookStartRequest").unwrap();
    book.set_field_by_name(
        "illustrated_book_reward_id",
        Value::I32(rules.illustrated_books[0].id),
    );
    let (_, book_response) = run(
        &mut resources,
        &mut home,
        "/illustrated_book/start",
        &book,
        "blend.api.IllustratedBookStartResponse",
    );
    assert!(i32_list(&book_response, "memoria_ids").len() >= 5);
    assert!(!message_list(&book_response, "rewards").is_empty());

    let mut enhance = empty_message(&proto, "blend.api.MemoriaEnhanceRequest").unwrap();
    enhance.set_field_by_name("memoria_entity_id", Value::I32(200));
    let mut item = empty_message(&proto, "blend.model.ConsumedItem").unwrap();
    item.set_field_by_name("item_id", Value::I32(98));
    item.set_field_by_name("quantity", Value::I32(1));
    enhance.set_field_by_name("consumed_items", Value::List(vec![Value::Message(item)]));
    enhance.set_field_by_name(
        "consumed_memoria_entity_ids",
        Value::List(vec![Value::I32(201)]),
    );
    run(
        &mut resources,
        &mut home,
        "/memoria/enhance",
        &enhance,
        "blend.api.MemoriaEnhanceResponse",
    );
    assert!(!owned(&resources, "memorias", 201));
    let mut limit = empty_message(&proto, "blend.api.MemoriaLimitBreakRequest").unwrap();
    limit.set_field_by_name("memoria_entity_id", Value::I32(202));
    limit.set_field_by_name(
        "consumed_memoria_entity_ids",
        Value::List(vec![Value::I32(210)]),
    );
    run(
        &mut resources,
        &mut home,
        "/memoria/limit_break",
        &limit,
        "blend.api.MemoriaLimitBreakResponse",
    );
    let mut memoria_lock_request = empty_message(&proto, "blend.api.MemoriaLockRequest").unwrap();
    memoria_lock_request.set_field_by_name("memoria_entity_id", Value::I32(203));
    memoria_lock_request.set_field_by_name("is_locked", Value::Bool(true));
    run(
        &mut resources,
        &mut home,
        "/memoria/lock",
        &memoria_lock_request,
        "blend.api.ChangedResourcesResponse",
    );
    memoria_lock_request.set_field_by_name("is_locked", Value::Bool(false));
    run(
        &mut resources,
        &mut home,
        "/memoria/lock",
        &memoria_lock_request,
        "blend.api.ChangedResourcesResponse",
    );
    let mut sell = empty_message(&proto, "blend.api.MemoriaSellRequest").unwrap();
    sell.set_field_by_name("entity_ids", Value::List(vec![Value::I32(203)]));
    run(
        &mut resources,
        &mut home,
        "/memoria/sell",
        &sell,
        "blend.api.MemoriaSellResponse",
    );
    assert!(!owned(&resources, "memorias", 203));

    let recipe = synthesis
        .recipes
        .iter()
        .find(|row| row.target.resource_type == 25)
        .unwrap();
    let mut learned = empty_message(&proto, "blend.model.Recipe").unwrap();
    learned.set_field_by_name("recipe_id", Value::I32(recipe.id));
    home::put(&mut resources, "recipes", "recipe_id", learned);
    for cost in &recipe.costs {
        change_item(&proto, &mut resources, cost.id, cost.quantity).unwrap();
    }
    let mut synthesize = empty_message(&proto, "blend.api.ShipSynthesizeRequest").unwrap();
    synthesize.set_field_by_name("recipe_id", Value::I32(recipe.id));
    run(
        &mut resources,
        &mut home,
        "/ship/synthesize",
        &synthesize,
        "blend.api.ShipSynthesizeResponse",
    );
    let ship_tool_count = message_list(&resources, "ship_tools").len();
    let mut duplicate_changed = empty_message(&proto, "blend.model.Resources").unwrap();
    let mut duplicate_response = empty_message(&proto, "blend.api.ShipSynthesizeResponse").unwrap();
    assert!(matches!(
        apply(
            &proto,
            &rules,
            &synthesis,
            &home_rules,
            &mut resources,
            &mut duplicate_changed,
            &mut duplicate_response,
            &mut home,
            "/ship/synthesize",
            &synthesize,
            now,
        ),
        Err(StateError::InvalidRequest)
    ));
    assert_eq!(
        message_list(&resources, "ship_tools").len(),
        ship_tool_count
    );
    let ship_tool_entity = i32_field(
        message_list(&resources, "ship_tools").last().unwrap(),
        "entity_id",
    )
    .unwrap();
    let part = rules
        .ship_parts
        .iter()
        .find(|row| row.enhance_target == 1 && row.enhance_type == 1)
        .unwrap();
    assert!(rules.ships.contains(&part.target_id));
    assert!(rules.ship_levels.iter().any(|row| row.rank == 1));
    assert!(message_list(&resources, "ships").is_empty());
    for cost in &part.costs {
        if cost.resource_type == 5 {
            change_item(&proto, &mut resources, cost.id, cost.quantity).unwrap();
        }
    }
    let mut create = empty_message(&proto, "blend.api.ShipCreateRequest").unwrap();
    create.set_field_by_name("ship_part_id", Value::I32(part.id));
    create.set_field_by_name("count", Value::I32(1));
    run(
        &mut resources,
        &mut home,
        "/ship/create",
        &create,
        "blend.api.ChangedResourcesResponse",
    );
    let ship_id = part.target_id;
    let mut party = empty_message(&proto, "blend.api.ShipBulkUpdateRequest").unwrap();
    party.set_field_by_name("ship_id", Value::I32(ship_id));
    party.set_field_by_name("number", Value::I32(1));
    party.set_field_by_name("character_ids", Value::List(vec![Value::I32(character_id)]));
    party.set_field_by_name(
        "main_memoria_entity_id",
        Value::Message(int32_value(&proto, 204).unwrap()),
    );
    run(
        &mut resources,
        &mut home,
        "/ship/bulk_update",
        &party,
        "blend.api.ChangedResourcesResponse",
    );
    let mut set_tools = empty_message(&proto, "blend.api.ShipShipToolsSetRequest").unwrap();
    set_tools.set_field_by_name("ship_id", Value::I32(ship_id));
    set_tools.set_field_by_name(
        "ship_tool_entity_ids",
        Value::List(vec![Value::I32(ship_tool_entity)]),
    );
    run(
        &mut resources,
        &mut home,
        "/ship/ship_tools_set",
        &set_tools,
        "blend.api.ChangedResourcesResponse",
    );
    assert_eq!(
        i32_list(
            &message_list(&resources, "ship_parties")[0],
            "ship_tool_entity_ids"
        ),
        vec![ship_tool_entity]
    );
}
