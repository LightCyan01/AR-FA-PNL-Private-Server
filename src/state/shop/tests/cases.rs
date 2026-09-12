use super::*;
use prost::Message;
use std::collections::BTreeSet;
use std::path::Path;

#[test]
fn item_challenge_parameters_and_reward_claims() {
    let rules = load_rules().unwrap();
    let spec = rules
        .item_challenges
        .iter()
        .find(|r| number(r, "id") == 1)
        .unwrap();
    let param = challenge_row(&rules, "item_challenge_parameter", 2).unwrap();
    let tool = challenge_row(&rules, "equipment_tool", 1).unwrap();
    assert_eq!(
        challenge_tool_points(&rules, spec, param, 6, tool, &[]).unwrap(),
        120
    );
    assert_eq!(
        challenge_tool_points(&rules, spec, param, 6, tool, &[(1, 5), (2, 5)]).unwrap(),
        1200
    );
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let home_rules = home::load_rules().unwrap();
    let mut resources = starter_resources(&proto, &load_fresh_rules().unwrap()).unwrap();
    let mut changed = empty_message(&proto, "blend.model.Resources").unwrap();
    let trait_ids = [1, 2].map(|category| {
        number(
            challenge_rows(&rules, "equipment_tool_trait")
                .iter()
                .find(|r| number(r, "category_id") == category)
                .unwrap(),
            "id",
        )
    });
    let mut inputs = Vec::new();
    for entity in 1..=3 {
        let mut tool = empty_message(&proto, "blend.model.EquipmentTool").unwrap();
        tool.set_field_by_name("entity_id", Value::I32(entity));
        tool.set_field_by_name("tool_id", Value::I32(588));
        tool.set_field_by_name(
            "traits",
            Value::List(
                trait_ids
                    .iter()
                    .map(|id| {
                        Value::Message(
                            trait_params_message(&proto, &TutorialTraitParam { id: *id, rank: 5 })
                                .unwrap(),
                        )
                    })
                    .collect(),
            ),
        );
        home::put(&mut resources, "equipment_tools", "entity_id", tool);
        let mut input = empty_message(&proto, "blend.model.ToolEntity").unwrap();
        input.set_field_by_name("type", Value::I32(6));
        input.set_field_by_name("entity_id", Value::I32(entity));
        inputs.push(Value::Message(input));
    }
    let mut request = empty_message(&proto, "blend.api.ItemChallengeExecuteRequest").unwrap();
    request.set_field_by_name("item_challenge_id", Value::I32(1));
    request.set_field_by_name("challenge_tools", Value::List(inputs));
    let mut response = empty_message(&proto, "blend.api.ItemChallengeExecuteResponse").unwrap();
    let now = spec["start_at"].as_i64().unwrap() + 1;
    item_challenge(
        &proto,
        &rules,
        &home_rules,
        &mut resources,
        &mut changed,
        &mut response,
        "/item_challenge/execute",
        &request,
        now,
    )
    .unwrap();
    let score = i32_field(&response, "score").unwrap();
    assert!(score >= 15_000, "score {score}");
    assert!(i32_list(&response, "failed_parameter_set_ids").is_empty());
    assert_eq!(message_list(&resources, "equipment_tools").len(), 3);
    request = empty_message(&proto, "blend.api.ItemChallengeRewardReceiveRequest").unwrap();
    request.set_field_by_name("item_challenge_id", Value::I32(1));
    response = empty_message(&proto, "blend.api.ItemChallengeRewardReceiveResponse").unwrap();
    item_challenge(
        &proto,
        &rules,
        &home_rules,
        &mut resources,
        &mut changed,
        &mut response,
        "/item_challenge/reward_receive",
        &request,
        now,
    )
    .unwrap();
    assert!(!message_list(&response, "rewards").is_empty());
    let before = resources.encode_to_vec();
    assert!(matches!(
        item_challenge(
            &proto,
            &rules,
            &home_rules,
            &mut resources,
            &mut changed,
            &mut response,
            "/item_challenge/reward_receive",
            &request,
            now
        ),
        Err(StateError::InvalidRequest)
    ));
    assert_eq!(resources.encode_to_vec(), before);
    assert!(matches!(
        item_challenge(
            &proto,
            &rules,
            &home_rules,
            &mut resources,
            &mut changed,
            &mut response,
            "/item_challenge/reward_receive",
            &request,
            spec["end_at"].as_i64().unwrap()
        ),
        Err(StateError::OutOfSchedule)
    ));
}

#[test]
fn local_shop_catalog_is_client_consumable() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let master = proto
        .decode(
            "blend.model.MasterData",
            &std::fs::read(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../data/master_data.pb"
            ))
            .unwrap(),
        )
        .unwrap();
    let rules = load_rules().unwrap();
    let mut local = master.clone();
    local_master(&mut local, &rules);
    let mut resources = starter_resources(&proto, &load_fresh_rules().unwrap()).unwrap();
    let mut changed = empty_message(&proto, "blend.model.Resources").unwrap();
    let list = gem_list(
        &proto,
        &local,
        &mut resources,
        &mut changed,
        2_000_000_000,
        &rules,
        &ShopState::default(),
    )
    .unwrap();
    let products = message_list(&list, "shop_products");
    let stores = message_list(&list, "store_products");
    assert_eq!(products.len(), 255);
    assert!(products
        .iter()
        .all(|row| local_catalog_product(row, &rules)));
    assert!(!rules.unavailable_shop_icon_hashes.is_empty());
    for product in &products {
        assert!(timestamp_seconds(product, "start_at").is_none());
        assert!(timestamp_seconds(product, "end_at").is_none());
    }
    assert!(message_list(&resources, "shop_product_states")
        .iter()
        .all(|state| timestamp_seconds(state, "end_at").is_none()));

    let advertised: BTreeMap<_, _> = products
        .iter()
        .flat_map(purchase_steps)
        .filter_map(|step| {
            optional_i32_field(&step, "store_product_id").map(|id| (id, gem_amount(&step)))
        })
        .collect();
    for store in &stores {
        let id = i32_field(store, "id").unwrap();
        let product_id = wrapped_string(store, "product_id").unwrap();
        let expected_product_id = LOCAL_STORE_PRODUCTS
            .iter()
            .find(|row| row.0 == id)
            .map(|row| row.1)
            .unwrap_or_else(|| panic!("unexpected local store product {id}"));
        assert_eq!(product_id, expected_product_id);
        assert_eq!(wrapped_string(store, "currency").as_deref(), Some("JPY"));
        let money = i32_field(store, "money_amount").unwrap();
        let free = i32_field(store, "free_amount").unwrap();
        let price = store
            .get_field_by_name("price")
            .and_then(|value| value.as_message().cloned())
            .and_then(|wrapper| {
                wrapper
                    .get_field_by_name("value")
                    .and_then(|value| value.as_f64())
            })
            .unwrap();
        assert_eq!(
            price,
            f64::from(if (10..=16).contains(&id) {
                money
            } else {
                money * 2
            })
        );
        if advertised[&id] > 0 {
            assert_eq!(money + free, advertised[&id]);
        }
    }
    println!(
        "LOCAL_SHOP_CATALOG_OK products={} store_products={}",
        products.len(),
        stores.len()
    );
    // Native purchase-result lookup dereferences every saved shop state,
    // including old offers now hidden from the sale catalog.
    let master_ids: BTreeSet<_> = message_list(&master, "shop_products")
        .iter()
        .filter_map(|row| i32_field(row, "id"))
        .collect();
    let local_ids: BTreeSet<_> = message_list(&local, "shop_products")
        .iter()
        .filter_map(|row| i32_field(row, "id"))
        .collect();
    assert!(
        local_ids == master_ids,
        "saved shop states must retain their master definitions"
    );
    let medal_product = product(&local, 301).unwrap();
    assert_eq!(i32_field(&medal_product, "shop_id"), Some(5));
    let home_rules = home::load_rules().unwrap();
    assert!(purchase_product(
        &proto,
        &rules,
        &home_rules,
        &mut resources.clone(),
        &mut changed.clone(),
        &medal_product,
        1,
        2_000_000_000
    )
    .is_err());
    change_item(&proto, &mut resources, 131, 150).unwrap();
    let rewards = purchase_product(
        &proto,
        &rules,
        &home_rules,
        &mut resources,
        &mut changed,
        &medal_product,
        1,
        2_000_000_000,
    )
    .unwrap();
    assert!(!rewards.is_empty());
    assert_eq!(item_quantity(&resources, 131), Some(0));
    assert_eq!(purchased_count(&resources, 301), 1);
}

#[test]
fn local_purchase_verification_grants_paid_gems_once() {
    let proto = ProtoRegistry::from_file(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let master = proto
        .decode(
            "blend.model.MasterData",
            &std::fs::read(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../data/master_data.pb"
            ))
            .unwrap(),
        )
        .unwrap();
    let rules = load_rules().unwrap();
    let home_rules = home::load_rules().unwrap();
    let mut resources = starter_resources(&proto, &load_fresh_rules().unwrap()).unwrap();
    let mut changed = empty_message(&proto, "blend.model.Resources").unwrap();
    let mut state = ShopState::default();
    let product_id = message_list(&master, "shop_products")
        .into_iter()
        .find(|row| {
            purchase_steps(row)
                .iter()
                .any(|step| optional_i32_field(step, "store_product_id") == Some(4))
        })
        .and_then(|row| i32_field(&row, "id"))
        .unwrap();
    let paid_before = message_i32_field(&resources, "wallet", "paid").unwrap_or(0);
    let free_before = message_i32_field(&resources, "wallet", "free").unwrap_or(0);
    let mut start = empty_message(&proto, "blend.api.PurchaseSessionStartRequest").unwrap();
    start.set_field_by_name("shop_product_id", Value::I32(product_id));
    let mut start_response =
        empty_message(&proto, "blend.api.PurchaseSessionStartResponse").unwrap();
    apply(
        &proto,
        &rules,
        &home_rules,
        &master,
        &mut resources,
        &mut changed,
        &mut start_response,
        &mut state,
        "/purchase/session_start",
        &start,
        "",
        100,
    )
    .unwrap();
    let session_id = start_response
        .get_field_by_name("purchase_session_id")
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap();
    let mut verify = empty_message(&proto, "blend.api.PurchaseVerifyRequest").unwrap();
    verify.set_field_by_name("purchase_session_id", Value::String(session_id));
    let transaction_id = wrapped_string(&start_response, "transaction_id").unwrap();
    assert!(transaction_id.starts_with("local-"));
    verify.set_field_by_name("transaction_id", Value::String(transaction_id));
    let mut response = empty_message(&proto, "blend.api.ChangedResourcesResponse").unwrap();
    apply(
        &proto,
        &rules,
        &home_rules,
        &master,
        &mut resources,
        &mut changed,
        &mut response,
        &mut state,
        "/purchase/verify",
        &verify,
        "",
        100,
    )
    .unwrap();
    assert_eq!(
        message_i32_field(&resources, "wallet", "paid"),
        Some(paid_before + 240)
    );
    assert_eq!(
        message_i32_field(&resources, "wallet", "free"),
        Some(free_before + 10)
    );
    assert!(apply(
        &proto,
        &rules,
        &home_rules,
        &master,
        &mut resources,
        &mut changed,
        &mut response,
        &mut state,
        "/purchase/verify",
        &verify,
        "",
        100,
    )
    .is_err());

    assert_eq!(
        message_i32_field(&resources, "wallet", "paid"),
        Some(paid_before + 240)
    );
    assert_eq!(
        message_i32_field(&resources, "wallet", "free"),
        Some(free_before + 10)
    );

    start.set_field_by_name("shop_product_id", Value::I32(10000));
    start_response = empty_message(&proto, "blend.api.PurchaseSessionStartResponse").unwrap();
    apply(
        &proto,
        &rules,
        &home_rules,
        &master,
        &mut resources,
        &mut changed,
        &mut start_response,
        &mut state,
        "/purchase/session_start",
        &start,
        "",
        100,
    )
    .unwrap();
    verify.set_field_by_name(
        "purchase_session_id",
        start_response
            .get_field_by_name("purchase_session_id")
            .unwrap()
            .into_owned(),
    );
    verify.set_field_by_name("transaction_id", Value::String("local-pass".into()));
    apply(
        &proto,
        &rules,
        &home_rules,
        &master,
        &mut resources,
        &mut changed,
        &mut response,
        &mut state,
        "/purchase/verify",
        &verify,
        "",
        100,
    )
    .unwrap();
    assert!(message_list(&resources, "daily_pass_states")
        .iter()
        .any(|row| i32_field(row, "daily_pass_id") == Some(1)));
    assert_eq!(
        message_i32_field(&resources, "wallet", "paid"),
        Some(paid_before + 640)
    );
    let mut receive = empty_message(&proto, "blend.api.DailyPassReceiveRequest").unwrap();
    receive.set_field_by_name("daily_pass_id", Value::I32(1));
    receive.set_field_by_name("target_day", Value::I32(1));
    let mut receive_response = empty_message(&proto, "blend.api.DailyPassReceiveResponse").unwrap();
    apply(
        &proto,
        &rules,
        &home_rules,
        &master,
        &mut resources,
        &mut changed,
        &mut receive_response,
        &mut state,
        "/daily_pass/receive",
        &receive,
        "",
        100,
    )
    .unwrap();

    let mut wallet = member_status(&resources, "wallet").unwrap();
    wallet.set_field_by_name("paid", Value::I32(5_000));
    resources.set_field_by_name("wallet", Value::Message(wallet));
    let mut pack = empty_message(&proto, "blend.api.GrowthPackPurchaseRequest").unwrap();
    pack.set_field_by_name("growth_pack_id", Value::I32(1));
    apply(
        &proto,
        &rules,
        &home_rules,
        &master,
        &mut resources,
        &mut changed,
        &mut response,
        &mut state,
        "/growth_pack/purchase",
        &pack,
        "",
        2_000_000_000,
    )
    .unwrap();
    set_total_task_count(&mut resources, 1, 3).unwrap();
    let mut pack_receive = empty_message(&proto, "blend.api.GrowthPackReceiveRequest").unwrap();
    pack_receive.set_field_by_name("growth_pack_step_id", Value::I32(1));
    pack_receive.set_field_by_name("is_premium", Value::Bool(true));
    let mut pack_response = empty_message(&proto, "blend.api.GrowthPackReceiveResponse").unwrap();
    apply(
        &proto,
        &rules,
        &home_rules,
        &master,
        &mut resources,
        &mut changed,
        &mut pack_response,
        &mut state,
        "/growth_pack/receive",
        &pack_receive,
        "",
        2_000_000_000,
    )
    .unwrap();
    assert!(message_list(&resources, "growth_pack_step_states")
        .iter()
        .any(|row| i32_field(row, "growth_pack_step_id") == Some(1)));

    change_item(&proto, &mut resources, 1290, 1).unwrap();
    let mut bundle = empty_message(&proto, "blend.api.ItemBundleOpenRequest").unwrap();
    bundle.set_field_by_name("item_id", Value::I32(1290));
    let mut bundle_response = empty_message(&proto, "blend.api.ItemBundleOpenResponse").unwrap();
    apply(
        &proto,
        &rules,
        &home_rules,
        &master,
        &mut resources,
        &mut changed,
        &mut bundle_response,
        &mut state,
        "/item/bundle_open",
        &bundle,
        "",
        100,
    )
    .unwrap();
    assert_eq!(item_quantity(&resources, 1290), Some(0));
    assert_eq!(item_quantity(&resources, 382), Some(100));
}
