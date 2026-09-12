use super::prelude::*;

use serde::Serialize;
use serde_json::Value as Json;

#[derive(Clone, Deserialize)]
pub(crate) struct ShopRules {
    pub(crate) historical_shop_ids: Vec<i32>,
    pub(crate) unavailable_shop_icon_hashes: Vec<i64>,
    #[serde(rename = "format")]
    pub(crate) format_name: String,
    pub source_sha256: String,
    pub(crate) shops: Vec<ShopRule>,
    pub(crate) reward_sets: Vec<TutorialRewardSet>,
    pub(crate) daily_passes: Vec<DailyPassRule>,
    pub(crate) growth_packs: Vec<GrowthPackRule>,
    pub(crate) growth_pack_steps: Vec<GrowthPackStepRule>,
    pub(crate) special_offers: Vec<SpecialOfferRule>,
    pub(crate) bundle_item_sets: Vec<BundleItemSetRule>,
    pub(crate) box_gachas: Vec<Json>,
    pub(crate) box_gacha_boxes: Vec<Json>,
    pub(crate) box_gacha_cards: Vec<Json>,
    pub(crate) shop_wheels: Vec<Json>,
    pub(crate) item_challenges: Vec<Json>,
    pub(crate) item_challenge_rewards: Vec<Json>,
    pub(crate) item_challenge_rules: BTreeMap<String, Vec<Json>>,
    pub(crate) piece_exchange_quantity_table: Vec<PieceExchangeTier>,
    pub(crate) piece_exchange_unavailable_character_ids: Vec<i32>,
    pub(crate) piece_exchange_item_id: i32,
    pub(crate) shop_first_purchase_bonus_memoria_id: i32,
    pub(crate) mission_pass_point_purchase_min_lot: i32,
    pub(crate) mission_pass_point_purchase_one_lot_cost: i32,
}

#[derive(Clone, Deserialize)]
pub(crate) struct ShopRule {
    pub(crate) id: i32,
    pub(crate) shop_type: i32,
    pub(crate) lineup_refresh_interval_minutes: i32,
    pub(crate) lineup_refresh_costs: Vec<RefreshCost>,
    pub(crate) start_at: Option<i64>,
    pub(crate) end_at: Option<i64>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct RefreshCost {
    pub(crate) count: i32,
    pub(crate) gem_cost: i32,
}

#[derive(Clone, Deserialize)]
pub(crate) struct DailyPassRule {
    pub(crate) id: i32,
    pub(crate) days: i32,
    pub(crate) daily_pass_reward: TutorialReward,
}

#[derive(Clone, Deserialize)]
pub(crate) struct GrowthPackRule {
    pub(crate) id: i32,
    #[serde(rename = "type")]
    pub(crate) pack_type: i32,
    pub(crate) premium_cost: TutorialReward,
    pub(crate) total_task_condition_id: i32,
    pub(crate) start_at: Option<i64>,
    pub(crate) end_at: Option<i64>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct GrowthPackStepRule {
    pub(crate) id: i32,
    pub(crate) growth_pack_id: i32,
    pub(crate) count: i32,
    pub(crate) reward_set_id: i32,
    pub(crate) premium_reward_set_id: i32,
}

#[derive(Clone, Deserialize)]
pub(crate) struct SpecialOfferRule {
    pub(crate) id: i32,
    pub(crate) days: i32,
    pub(crate) cost: TutorialReward,
    pub(crate) start_at: Option<i64>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct BundleItemSetRule {
    pub(crate) item_id: i32,
    pub(crate) is_direct: bool,
    pub(crate) is_random: bool,
    pub(crate) items: Vec<ItemAmount>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct ItemAmount {
    pub(crate) id: i32,
    pub(crate) quantity: i32,
}

#[derive(Clone, Deserialize)]
pub(crate) struct PieceExchangeTier {
    pub(crate) purchase_count: Option<i32>,
    pub(crate) quantity: i32,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct ShopState {
    pub(crate) sessions: BTreeMap<String, PurchaseSession>,
    pub(crate) random_shops: BTreeMap<i32, RandomShop>,
    pub(crate) wheels: BTreeMap<i32, (i64, i32)>,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct PurchaseSession {
    pub(crate) shop_product_id: i32,
    pub(crate) transaction_id: Option<String>,
    pub(crate) verified: bool,
}

#[derive(Default, Deserialize, Serialize)]
pub(crate) struct RandomShop {
    pub(crate) refresh_count: i32,
    pub(crate) refresh_at: i64,
    pub(crate) product_ids: Vec<i32>,
    pub(crate) purchased: BTreeMap<i32, i32>,
}

pub(crate) fn load_rules() -> Result<ShopRules, StateError> {
    let rules: ShopRules = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../data/shop_rules.json"
    )))
    .map_err(|error| StateError::MasterData(error.to_string()))?;
    if rules.format_name != "atelier-shop-rules-v1"
        || rules.shops.is_empty()
        || rules.reward_sets.is_empty()
        || rules.daily_passes.is_empty()
    {
        return Err(StateError::MasterData("shop rules are incomplete".into()));
    }
    Ok(rules)
}

pub(crate) fn is_route(route: &str) -> bool {
    route.starts_with("/shop/")
        || route.starts_with("/purchase/")
        || route.starts_with("/daily_pass/")
        || route.starts_with("/growth_pack/")
        || route == "/special_offer/purchase"
        || route.starts_with("/item/")
        || route.starts_with("/item_challenge/")
}

pub(crate) fn reward_set(rules: &ShopRules, id: i32) -> Result<&[TutorialReward], StateError> {
    rules
        .reward_sets
        .iter()
        .find(|row| row.id == id)
        .map(|row| row.rewards.as_slice())
        .ok_or_else(|| StateError::RewardRules(format!("missing shop reward set {id}")))
}

pub(crate) fn local_master(master: &mut DynamicMessage, rules: &ShopRules) {
    // Keep definitions for persisted ShopProductState lookups. Only gem_list
    // filters unavailable sale offers; deleting definitions crashes purchase UI.
    let rows = products(master)
        .into_iter()
        .map(|mut row| {
            if i32_field(&row, "shop_id").is_some_and(|id| rules.historical_shop_ids.contains(&id))
            {
                row.clear_field_by_name("start_at");
                row.clear_field_by_name("end_at");
            }
            Value::Message(row)
        })
        .collect();
    master.set_field_by_name("shop_products", Value::List(rows));
}

pub(crate) fn products(master: &DynamicMessage) -> Vec<DynamicMessage> {
    message_list(master, "shop_products")
        .into_iter()
        .map(|mut row| {
            // The offline gem catalog is permanent. The native window independently
            // filters these dates, even when gem_list includes the product.
            if i32_field(&row, "shop_id") == Some(1) {
                row.clear_field_by_name("start_at");
                row.clear_field_by_name("end_at");
            }
            row
        })
        .collect()
}

pub(crate) fn product(master: &DynamicMessage, id: i32) -> Result<DynamicMessage, StateError> {
    products(master)
        .into_iter()
        .find(|row| i32_field(row, "id") == Some(id))
        .ok_or(StateError::InvalidRequest)
}

pub(crate) fn wrapped_string(message: &DynamicMessage, field: &str) -> Option<String> {
    message
        .get_field_by_name(field)
        .and_then(|value| value.as_message().cloned())
        .and_then(|value| {
            value
                .get_field_by_name("value")
                .and_then(|v| v.as_str().map(str::to_owned))
        })
}

pub(crate) fn string_wrapper(
    proto: &ProtoRegistry,
    value: impl Into<String>,
) -> Result<DynamicMessage, StateError> {
    let mut wrapper = empty_message(proto, "google.protobuf.StringValue")?;
    wrapper.set_field_by_name("value", Value::String(value.into()));
    Ok(wrapper)
}

pub(crate) fn double_wrapper(
    proto: &ProtoRegistry,
    value: f64,
) -> Result<DynamicMessage, StateError> {
    let mut wrapper = empty_message(proto, "google.protobuf.DoubleValue")?;
    wrapper.set_field_by_name("value", Value::F64(value));
    Ok(wrapper)
}

pub(crate) fn timestamp_seconds(message: &DynamicMessage, field: &str) -> Option<i64> {
    if !message.has_field_by_name(field) {
        return None;
    }
    message
        .get_field_by_name(field)
        .and_then(|value| value.as_message().cloned())
        .and_then(|value| value.get_field_by_name("seconds").and_then(|v| v.as_i64()))
}

pub(crate) fn local_catalog_product(row: &DynamicMessage, rules: &ShopRules) -> bool {
    i32_field(row, "shop_id") == Some(1)
        && row
            .get_field_by_name("still_path_hash")
            .and_then(|value| value.as_i64())
            .is_some_and(|hash| hash != 0 && !rules.unavailable_shop_icon_hashes.contains(&hash))
}

pub(crate) fn purchase_steps(product: &DynamicMessage) -> Vec<DynamicMessage> {
    message_list(product, "purchase_steps")
}

pub(crate) fn purchased_count(resources: &DynamicMessage, product_id: i32) -> i32 {
    message_list(resources, "shop_product_states")
        .iter()
        .find(|row| i32_field(row, "shop_product_id") == Some(product_id))
        .and_then(|row| i32_field(row, "purchased_count"))
        .unwrap_or(0)
}

pub(crate) fn set_product_count(
    proto: &ProtoRegistry,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    product: &DynamicMessage,
    count: i32,
    now: i64,
) -> Result<(), StateError> {
    let id = i32_field(product, "id").ok_or(StateError::InvalidRequest)?;
    let limit = i32_field(product, "limit_count").unwrap_or(0);
    if count < 0 || (limit > 0 && count > limit) {
        return Err(StateError::InvalidRequest);
    }
    let mut value = empty_message(proto, "blend.model.ShopProductState")?;
    value.set_field_by_name("shop_product_id", Value::I32(id));
    value.set_field_by_name("purchased_count", Value::I32(count));
    if let Some(end) = timestamp_seconds(product, "end_at") {
        value.set_field_by_name("end_at", Value::Message(timestamp(proto, end)?));
    }
    if let Some(reset) = optional_i32_field(product, "reset_cycle") {
        value.set_field_by_name(
            "next_reset_at",
            Value::Message(timestamp(proto, next_reset(reset, now)?)?),
        );
    }
    home::put(
        resources,
        "shop_product_states",
        "shop_product_id",
        value.clone(),
    );
    home::put(changed, "shop_product_states", "shop_product_id", value);
    Ok(())
}

pub(crate) fn next_reset(cycle: i32, now: i64) -> Result<i64, StateError> {
    let day = home::day(now);
    Ok(match cycle {
        1 => (day + 1) * 86400 + 3 * 3600,
        2 => (day + 7 - (day + 3).rem_euclid(7)) * 86400 + 3 * 3600,
        3 => {
            let (year, month, _) = civil_from_days(day);
            let (year, month) = if month == 12 {
                (year + 1, 1)
            } else {
                (year, month + 1)
            };
            days_from_civil(year, month, 1) * 86400 + 3 * 3600
        }
        _ => return Err(StateError::InvalidRequest),
    })
}

pub(crate) fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month, day)
}

pub(crate) fn days_from_civil(mut year: i64, month: i64, day: i64) -> i64 {
    year -= i64::from(month <= 2);
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let mp = month + if month > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468
}

pub(crate) fn pay(
    proto: &ProtoRegistry,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    resource_type: i32,
    id: i32,
    quantity: i32,
) -> Result<(), StateError> {
    let cost = TutorialRecipeCost {
        resource_type,
        id,
        quantity,
    };
    if let Some((field, value)) = pay_resource_cost(proto, resources, Some(&cost))? {
        if field == "items" || field == "character_pieces" {
            home::put(
                changed,
                field,
                if field == "items" {
                    "item_id"
                } else {
                    "character_id"
                },
                value,
            );
        } else {
            changed.set_field_by_name(field, Value::Message(value));
        }
    }
    Ok(())
}

pub(crate) fn pay_step(
    proto: &ProtoRegistry,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    step: &DynamicMessage,
    quantity: i32,
) -> Result<(), StateError> {
    for cost in message_list(step, "costs") {
        let amount = i32_field(&cost, "quantity")
            .and_then(|value| value.checked_mul(quantity))
            .ok_or(StateError::InvalidRequest)?;
        pay(
            proto,
            resources,
            changed,
            i32_field(&cost, "type").ok_or(StateError::InvalidRequest)?,
            i32_field(&cost, "id").unwrap_or_default(),
            amount,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn grant_step(
    proto: &ProtoRegistry,
    rules: &ShopRules,
    home_rules: &home::HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    step: &DynamicMessage,
    quantity: i32,
    now: i64,
) -> Result<Vec<Value>, StateError> {
    let Some(id) = optional_i32_field(step, "reward_set_id") else {
        return Ok(Vec::new());
    };
    let mut result = Vec::new();
    for _ in 0..quantity {
        result.extend(home::grant(
            proto,
            home_rules,
            resources,
            changed,
            reward_set(rules, id)?,
            now,
        )?);
    }
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn purchase_product(
    proto: &ProtoRegistry,
    rules: &ShopRules,
    home_rules: &home::HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    product: &DynamicMessage,
    quantity: i32,
    now: i64,
) -> Result<Vec<Value>, StateError> {
    if quantity <= 0 {
        return Err(StateError::InvalidRequest);
    }
    let steps = purchase_steps(product);
    if steps.is_empty()
        || steps
            .iter()
            .any(|step| optional_i32_field(step, "store_product_id").is_some())
    {
        return Err(StateError::InvalidRequest);
    }
    let old = purchased_count(resources, i32_field(product, "id").unwrap_or_default());
    let limit = i32_field(product, "limit_count").unwrap_or(0);
    let next = old
        .checked_add(quantity)
        .ok_or(StateError::InvalidRequest)?;
    if limit > 0 && next > limit {
        return Err(StateError::InvalidRequest);
    }
    let mut rewards = Vec::new();
    for index in old..next {
        let step = &steps[(index as usize).min(steps.len() - 1)];
        pay_step(proto, resources, changed, step, 1)?;
        rewards.extend(grant_step(
            proto, rules, home_rules, resources, changed, step, 1, now,
        )?);
    }
    set_product_count(proto, resources, changed, product, next, now)?;
    Ok(rewards)
}

pub(crate) fn gem_amount(step: &DynamicMessage) -> i32 {
    wrapped_string(step, "name")
        .filter(|name| name.contains('×'))
        .and_then(|name| name.rsplit('×').next().map(str::to_owned))
        .map(|value| {
            value
                .chars()
                .filter(|c| c.is_ascii_digit())
                .collect::<String>()
        })
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

pub(crate) const LOCAL_STORE_PRODUCTS: &[(i32, &str, i32, i32, i32)] = &[
    (3, "1", 80, 0, 160),
    (4, "2", 240, 10, 480),
    (5, "3", 500, 50, 1_000),
    (6, "4", 750, 100, 1_500),
    (7, "5", 1_500, 250, 3_000),
    (8, "6", 3_000, 550, 6_000),
    (9, "7", 5_900, 1_200, 11_800),
    (10, "10001", 160, 0, 160),
    (11, "10002", 480, 0, 480),
    (12, "10003", 1_000, 0, 1_000),
    (13, "10004", 1_500, 0, 1_500),
    (14, "10005", 3_000, 0, 3_000),
    (15, "10006", 6_000, 0, 6_000),
    (16, "10007", 11_800, 0, 11_800),
    (17, "8", 2_200, 4_400, 4_400),
    (18, "9", 2_200, 4_400, 4_400),
    (19, "10", 3_000, 6_000, 6_000),
    (20, "11", 5_900, 11_800, 11_800),
    (21, "12", 3_000, 6_000, 6_000),
    (22, "13", 5_900, 11_800, 11_800),
    (23, "14", 2_200, 4_400, 4_400),
    (24, "15", 3_000, 6_000, 6_000),
    (25, "16", 5_900, 11_800, 11_800),
    (26, "100", 240, 0, 480),
    (27, "17", 2_200, 4_400, 4_400),
    (28, "18", 3_000, 6_000, 6_000),
    (29, "19", 5_900, 11_800, 11_800),
    (30, "20", 2_200, 4_400, 4_400),
    (31, "21", 3_000, 6_000, 6_000),
    (32, "22", 5_900, 11_800, 11_800),
    (33, "23", 2_200, 4_400, 4_400),
    (34, "24", 3_000, 6_000, 6_000),
    (35, "25", 5_900, 11_800, 11_800),
    (36, "101", 1_500, 0, 3_000),
    (37, "102", 1_500, 0, 3_000),
    (38, "26", 2_200, 4_400, 4_400),
    (39, "27", 3_000, 6_000, 6_000),
    (40, "28", 5_900, 11_800, 11_800),
    (41, "103", 4_500, 0, 9_000),
    (42, "104", 3_000, 0, 6_000),
    (43, "105", 3_000, 0, 6_000),
    (44, "106", 4_500, 0, 9_000),
    (45, "107", 4_500, 0, 9_000),
    (46, "29", 2_200, 4_400, 4_400),
    (47, "30", 3_000, 6_000, 6_000),
    (48, "31", 5_900, 11_800, 11_800),
    (49, "108", 1_500, 0, 3_000),
    (50, "109", 80, 160, 160),
    (51, "110", 6_000, 12_000, 12_000),
    (52, "32", 3_000, 3_000, 6_000),
    (53, "33", 3_000, 3_000, 6_000),
    (54, "34", 3_000, 3_000, 6_000),
    (55, "35", 3_000, 3_000, 6_000),
    (56, "36", 3_000, 3_000, 6_000),
    (57, "37", 3_000, 3_000, 6_000),
    (58, "111", 500, 0, 1_000),
    (59, "112", 4_500, 1_500, 9_000),
    (74, "38", 3_000, 3_000, 6_000),
    (100, "10000", 400, 0, 800),
];

pub(crate) fn store_product(
    proto: &ProtoRegistry,
    id: i32,
    amount: i32,
) -> Result<DynamicMessage, StateError> {
    let (product_id, money_amount, free_amount, price) = LOCAL_STORE_PRODUCTS
        .iter()
        .find(|row| row.0 == id)
        .map(|(_, product_id, money, free, price)| (*product_id, *money, *free, *price))
        .ok_or(StateError::InvalidRequest)?;
    if amount > 0 && amount != money_amount + free_amount {
        return Err(StateError::MasterData(format!(
            "store product {id} amount changed"
        )));
    }
    let mut value = empty_message(proto, "blend.model.StoreProduct")?;
    value.set_field_by_name("id", Value::I32(id));
    value.set_field_by_name(
        "product_id",
        Value::Message(string_wrapper(proto, product_id)?),
    );
    value.set_field_by_name("money_amount", Value::I32(money_amount));
    value.set_field_by_name("free_amount", Value::I32(free_amount));
    value.set_field_by_name(
        "price",
        Value::Message(double_wrapper(proto, f64::from(price))?),
    );
    value.set_field_by_name("currency", Value::Message(string_wrapper(proto, "JPY")?));
    Ok(value)
}

pub(crate) fn gem_list(
    proto: &ProtoRegistry,
    master: &DynamicMessage,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    now: i64,
    rules: &ShopRules,
    state: &ShopState,
) -> Result<DynamicMessage, StateError> {
    let catalog: Vec<_> = products(master)
        .into_iter()
        .filter(|row| local_catalog_product(row, rules))
        .collect();
    let mut stores = BTreeMap::new();
    for row in &catalog {
        set_product_count(
            proto,
            resources,
            changed,
            row,
            purchased_count(resources, i32_field(row, "id").unwrap_or_default()),
            now,
        )?;
        for step in purchase_steps(row) {
            if let Some(id) = optional_i32_field(&step, "store_product_id") {
                stores.entry(id).or_insert(gem_amount(&step));
            }
        }
    }
    let mut list = empty_message(proto, "blend.model.ShopGemList")?;
    list.set_field_by_name(
        "shop_products",
        Value::List(catalog.into_iter().map(Value::Message).collect()),
    );
    list.set_field_by_name(
        "store_products",
        Value::List(
            stores
                .into_iter()
                .map(|(id, amount)| store_product(proto, id, amount).map(Value::Message))
                .collect::<Result<Vec<_>, _>>()?,
        ),
    );
    let mut wheels = Vec::new();
    for spec in rules
        .shop_wheels
        .iter()
        .filter(|r| home::in_period(r["start_at"].as_i64(), r["end_at"].as_i64(), now))
    {
        let id = number(spec, "id");
        let mut product = empty_message(proto, "blend.model.ShopWheelProduct")?;
        product.set_field_by_name("shop_wheel_id", Value::I32(id));
        product.set_field_by_name(
            "count",
            Value::I32(
                state
                    .wheels
                    .get(&id)
                    .filter(|(day, _)| *day == home::day(now))
                    .map(|(_, count)| *count)
                    .unwrap_or(0),
            ),
        );
        let (kind, item, _, max) = wheel_reward(id)?;
        product.set_field_by_name(
            "rewards",
            Value::List(vec![Value::Message(resource_message(
                proto, kind, item, max,
            )?)]),
        );
        wheels.push(Value::Message(product));
    }
    list.set_field_by_name("shop_wheel_products", Value::List(wheels));
    Ok(list)
}
