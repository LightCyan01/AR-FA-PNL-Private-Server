// Distribution checks call production paths; fixtures here are not natural-player evidence.
use super::prelude::*;
use crate::state::shop::prelude::challenge_score_variation;
use serde_json::{json, Value as Json};

fn bin(report: &mut Vec<Json>, name: String, samples: u32, observed: u32, expected: f64) {
    // Five-sigma Wilson intervals limit false alarms across the many reported bins.
    let n = f64::from(samples);
    let p = f64::from(observed) / n;
    let center = (p + 25.0 / (2.0 * n)) / (1.0 + 25.0 / n);
    let radius = 5.0 * (p * (1.0 - p) / n + 25.0 / (4.0 * n * n)).sqrt() / (1.0 + 25.0 / n);
    let (low, high) = ((center - radius).max(0.0), (center + radius).min(1.0));
    let passed = expected >= low - 1e-12 && expected <= high + 1e-12;
    report.push(json!({"path":name,"samples":samples,"observed_count":observed,
        "observed_probability":p,"expected_probability":expected,"wilson_low":low,"wilson_high":high,"passed":passed}));
}

#[test]
#[ignore = "extensive Monte Carlo: run --release --lib monte_carlo_production_rng -- --ignored --nocapture"]
fn monte_carlo_production_rng() {
    const N: u32 = 100_000;
    let mut report = Vec::new();
    let tutorial = load_tutorial_rules().unwrap();
    for id in [3075, 3080, 2215] {
        let gacha = gacha_rule(&tutorial, id).unwrap();
        let wish = (gacha.mixed_select_count > 0).then(|| GachaWishList {
            gacha_id: i64::from(id),
            character_ids: vec![gacha.mixed_pickup_character_ids[0]],
            memoria_ids: vec![],
            character_skin_ids: vec![],
        });
        let cards = weighted_gacha_cards(&tutorial, gacha, wish.as_ref()).unwrap();
        let mut counts = BTreeMap::new();
        for _ in 0..N {
            let card = select_weighted_card(&cards).unwrap();
            *counts.entry((card.resource_type, card.id)).or_insert(0) += 1;
        }
        let mut weights = BTreeMap::new();
        for (card, weight) in cards {
            *weights
                .entry((card.resource_type, card.id))
                .or_insert(0_u64) += weight;
        }
        for (key, weight) in weights {
            bin(
                &mut report,
                format!("gacha/{id}/{}/{}", key.0, key.1),
                N,
                *counts.get(&key).unwrap_or(&0),
                weight as f64 / 100_000.0,
            );
        }
    }
    let synthesis = load_synthesis_rules().unwrap();
    for bonus in [0, 5_000, 10_000] {
        let mut counts = BTreeMap::new();
        for _ in 0..N {
            *counts
                .entry(draw_trait_rank(&synthesis, bonus).unwrap())
                .or_insert(0) += 1;
        }
        let weights = synthesis
            .trait_ranks
            .iter()
            .map(|r| {
                (
                    r.id,
                    if r.id >= 4 {
                        u64::from(r.weight)
                            * u64::from(synthesis.local_policy.high_rank_bonus_denominator + bonus)
                            / u64::from(synthesis.local_policy.high_rank_bonus_denominator)
                    } else {
                        u64::from(r.weight)
                    },
                )
            })
            .collect::<Vec<_>>();
        let total = weights.iter().map(|(_, w)| w).sum::<u64>();
        for (rank, weight) in weights {
            bin(
                &mut report,
                format!("synthesis/rank/bonus{bonus}/{rank}"),
                N,
                *counts.get(&rank).unwrap_or(&0),
                weight as f64 / total as f64,
            );
        }
    }
    let reward_rules = load_reward_rules().unwrap();
    let mut extras = 0;
    for _ in 0..N {
        let rolled = roll_quest_rewards(&reward_rules, 204052001).unwrap();
        assert_eq!(
            rolled
                .iter()
                .find(|r| r.resource_type == 3)
                .unwrap()
                .quantity,
            100
        );
        let quantity = rolled
            .iter()
            .find(|r| r.resource_type == 5 && r.id == 98)
            .unwrap()
            .quantity;
        assert!(matches!(quantity, 3 | 5));
        extras += u32::from(quantity == 5);
    }
    bin(
        &mut report,
        "quest/204052001/bonus_item98".into(),
        N,
        extras,
        0.5,
    );
    let mut rates = BTreeMap::new();
    for set in &reward_rules.drop_reward_sets {
        for roll in &set.rolls {
            if roll.rewards.len() == 1 {
                rates.entry(roll.rate).or_insert((set.id, roll.clone()));
            }
        }
    }
    for (rate, (set_id, roll)) in rates {
        // Isolate one unchanged master roll so overlapping reward IDs cannot hide its outcome.
        let mut isolated = reward_rules.clone();
        isolated.quests.truncate(1);
        isolated.quests[0].drop_reward_set_ids = vec![set_id];
        isolated.drop_reward_sets = vec![DropRewardSet {
            id: set_id,
            rolls: vec![roll.clone()],
        }];
        let mut landed = 0;
        for _ in 0..N {
            let rewards = roll_quest_rewards(&isolated, isolated.quests[0].id).unwrap();
            if let Some(reward) = rewards.first() {
                assert!(
                    (roll.rewards[0].min_quantity..=roll.rewards[0].max_quantity)
                        .contains(&reward.quantity)
                );
                landed += 1;
            }
        }
        bin(
            &mut report,
            format!("quest/master_set{set_id}/rate{rate}"),
            N,
            landed,
            f64::from(rate) / 100.0,
        );
    }
    let mut variation = [0; 21];
    let mut critical = 0;
    let mut damage = [0; 101];
    for action in 0..N {
        variation[(challenge_score_variation().unwrap() - 90) as usize] += 1;
        let roll = deterministic_roll(
            b"rng-simulation",
            "battle-1",
            action as i32,
            b"critical",
            101,
            0,
        );
        assert_eq!(
            roll,
            deterministic_roll(
                b"rng-simulation",
                "battle-1",
                action as i32,
                b"critical",
                101,
                0
            )
        );
        critical += u32::from(roll % 10_000 < 1_000);
        damage[(deterministic_roll(
            b"rng-simulation",
            "battle-1",
            action as i32,
            b"variance",
            101,
            0,
        ) % 101) as usize] += 1;
    }
    for (index, count) in variation.into_iter().enumerate() {
        bin(
            &mut report,
            format!("challenge/variation/{}", 90 + index),
            N,
            count,
            1.0 / 21.0,
        );
    }
    bin(&mut report, "combat/critical".into(), N, critical, 0.1);
    for (index, count) in damage.into_iter().enumerate() {
        bin(
            &mut report,
            format!("combat/variance/{index}"),
            N,
            count,
            1.0 / 101.0,
        );
    }

    // Whole synthesis reducer checks the actual branch wiring, including optional rewards.
    let proto = ProtoRegistry::from_file(std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../schemas/atelier-resleriana-2.16.0.protoset"
    )))
    .unwrap();
    let fresh = load_fresh_rules().unwrap();
    let resources = reduce_talk_event(
        &proto,
        &fresh,
        &tutorial,
        starter_resources(&proto, &fresh).unwrap(),
        101001001,
        1,
    )
    .unwrap()
    .resources;
    let home_rules = home::load_rules().unwrap();
    let activity_rules = activities::load_rules().unwrap();
    let shop_data: Json =
        serde_json::from_str(include_str!("../../../../data/shop_rules.json")).unwrap();
    let cards = shop_data["box_gacha_cards"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| matches!(c["box_gacha_deck_id"].as_i64(), Some(3 | 4)))
        .collect::<Vec<_>>();
    let weights = cards
        .iter()
        .map(|c| c["count"].as_u64().unwrap() as u32)
        .collect::<Vec<_>>();
    let total = weights.iter().sum::<u32>();
    let mut counts = vec![0; cards.len()];
    for _ in 0..N {
        counts[activities::weighted_index(&weights).unwrap()] += 1;
    }
    for (index, card) in cards.iter().enumerate() {
        bin(
            &mut report,
            format!("box/2/first_draw/card{}", card["id"]),
            N,
            counts[index],
            f64::from(weights[index]) / f64::from(total),
        );
    }
    let mut box_resources = resources.clone();
    change_item(&proto, &mut box_resources, 391, (total * 10) as i32).unwrap();
    let mut box_request = empty_message(&proto, "blend.api.ShopBoxGachaExecuteRequest").unwrap();
    box_request.set_field_by_name("box_gacha_id", Value::I32(2));
    box_request.set_field_by_name("count", Value::I32(total as i32));
    let mut box_response = empty_message(&proto, "blend.api.ShopBoxGachaExecuteResponse").unwrap();
    shop::apply(
        &proto,
        &shop::load_rules().unwrap(),
        &home_rules,
        &empty_message(&proto, "blend.model.MasterData").unwrap(),
        &mut box_resources,
        &mut empty_message(&proto, "blend.model.Resources").unwrap(),
        &mut box_response,
        &mut shop::ShopState::default(),
        "/shop/box_gacha/execute",
        &box_request,
        "",
        1_707_447_601,
    )
    .unwrap();
    let box_state = message_list(&box_resources, "box_gacha_states").remove(0);
    assert_eq!(i32_field(&box_state, "number"), Some(2));
    assert!(message_list(&box_state, "card_states").is_empty());
    assert_eq!(item_quantity(&box_resources, 391), Some(0));
    let mut expected = BTreeMap::new();
    let mut actual = BTreeMap::new();
    for (card, count) in cards.iter().zip(weights) {
        *expected
            .entry((
                card["reward"]["type"].as_i64().unwrap() as i32,
                card["reward"]["id"].as_i64().unwrap() as i32,
            ))
            .or_insert(0_i64) += i64::from(count) * card["reward"]["quantity"].as_i64().unwrap();
    }
    for reward in message_list(&box_response, "rewards") {
        *actual
            .entry((
                i32_field(&reward, "type").unwrap(),
                i32_field(&reward, "id").unwrap(),
            ))
            .or_insert(0_i64) += i64::from(i32_field(&reward, "quantity").unwrap());
    }
    assert_eq!(
        actual, expected,
        "depleted box must grant every card exactly its finite count"
    );
    let mut request = empty_message(&proto, "blend.api.SynthesisExecuteRequest").unwrap();
    request.set_field_by_name("recipe_id", Value::I32(6));
    request.set_field_by_name(
        "character_ids",
        Value::List(vec![Value::I32(32201), Value::I32(43101)]),
    );
    request.set_field_by_name(
        "ingredient_id",
        Value::Message(int32_value(&proto, 67).unwrap()),
    );
    let recipe = synthesis.recipes.iter().find(|r| r.id == 6).unwrap();
    let candidates =
        synthesis_trait_candidates(&synthesis, recipe, &resources, &[32201, 43101], 67, false)
            .unwrap();
    let stimulator_type = stimulator_fields(recipe.target.resource_type).unwrap().3;
    for (label, easy, boost) in [
        ("normal", false, false),
        ("stimulated", false, true),
        ("easy", true, false),
    ] {
        let pool =
            synthesis_trait_candidates(&synthesis, recipe, &resources, &[32201, 43101], 67, easy)
                .unwrap();
        assert!(
            pool.iter().all(|c| c.active),
            "distribution fixture requires connected synthesis colors"
        );
        let stimulators = if boost { vec![pool[0].id] } else { vec![] };
        let mut weights = BTreeMap::new();
        for candidate in &pool {
            *weights.entry(candidate.id).or_insert(0_u32) += if stimulators.contains(&candidate.id)
            {
                synthesis.local_policy.stimulator_weight
            } else {
                1
            };
        }
        let total = weights.values().sum::<u32>();
        let mut counts = BTreeMap::new();
        const SELECTIONS: u32 = 20_000;
        for _ in 0..SELECTIONS {
            let traits = select_full_synthesis_traits(
                &synthesis,
                recipe,
                &resources,
                &[32201, 43101],
                67,
                &stimulators,
                easy,
            )
            .unwrap();
            *counts.entry(traits[0].id).or_insert(0) += 1;
            assert!(traits.iter().all(|t| weights.contains_key(&t.id)));
        }
        for (id, weight) in weights {
            bin(
                &mut report,
                format!("synthesis/first_trait/{label}/{id}"),
                SELECTIONS,
                *counts.get(&id).unwrap_or(&0),
                f64::from(weight) / f64::from(total),
            );
        }
    }
    let mut great = 0;
    let mut targets = 0;
    let mut stimulators = 0;
    const CRAFTS: u32 = 2_000;
    for sample in 0..CRAFTS {
        let mutation = reduce_synthesis(
            &proto,
            &synthesis,
            &home_rules,
            &activity_rules,
            resources.clone(),
            &request,
            SynthesisMode::Execute,
            1,
        )
        .unwrap();
        great += u32::from(i32_field(&mutation.response, "grade") == Some(3));
        let rewards = message_list(&mutation.response, "rewards");
        targets += rewards
            .iter()
            .filter(|r| {
                i32_field(r, "type") == Some(recipe.target.resource_type)
                    && i32_field(r, "id") == Some(recipe.target.id)
            })
            .count() as u32
            - 1;
        stimulators += rewards
            .iter()
            .filter(|r| i32_field(r, "type") == Some(stimulator_type))
            .map(|r| i32_field(r, "quantity").unwrap() as u32)
            .sum::<u32>();
        if (sample + 1) % 500 == 0 {
            println!("MONTE_CARLO_SYNTHESIS samples={}", sample + 1);
        }
    }
    bin(
        &mut report,
        "synthesis/reducer/great_success".into(),
        CRAFTS,
        great,
        f64::from(synthesis.local_policy.great_success_rate) / 100.0,
    );
    bin(
        &mut report,
        "synthesis/reducer/extra_target".into(),
        CRAFTS * (synthesis.local_policy.result_slot_count as u32 - 1),
        targets,
        f64::from(synthesis.local_policy.extra_target_rate) / 100.0,
    );
    bin(
        &mut report,
        "synthesis/reducer/stimulator_drop".into(),
        CRAFTS * candidates.len() as u32,
        stimulators,
        f64::from(synthesis.local_policy.stimulator_drop_rate) / 100.0,
    );
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../work/monte-carlo.json");
    std::fs::write(&path,serde_json::to_vec_pretty(&json!({"generated_at":unix_now(),"interval":"Wilson score, z=5 (per-bin confidence approximately 99.9999427%)","fixtures":"isolated reducer inputs; not natural account progression","bins":report})).unwrap()).unwrap();
    let failures = report
        .iter()
        .filter(|r| r["passed"] != true)
        .collect::<Vec<_>>();
    assert!(failures.is_empty(), "distribution failures: {failures:?}");
    println!(
        "MONTE_CARLO_OK bins={} report={}",
        report.len(),
        path.display()
    );
}
