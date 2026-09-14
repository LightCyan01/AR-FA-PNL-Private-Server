use crate::state::service::prelude::*;

#[allow(clippy::too_many_arguments)]
pub(crate) fn reduce_synthesis(
    proto: &ProtoRegistry,
    rules: &SynthesisRules,
    home_rules: &home::HomeRules,
    activity_rules: &activities::ActivityRules,
    mut resources: DynamicMessage,
    request: &DynamicMessage,
    mode: SynthesisMode,
    now: i64,
) -> Result<ResourceMutation, StateError> {
    let before = resources.clone();
    let recipe_id = i32_field(request, "recipe_id").ok_or(StateError::InvalidRequest)?;
    let recipe = rules
        .recipes
        .iter()
        .find(|recipe| recipe.id == recipe_id)
        .ok_or(StateError::InvalidRequest)?;
    if !recipe_present(&resources, recipe_id) {
        return Err(StateError::InvalidRequest);
    }
    let count = if mode == SynthesisMode::Execute {
        1
    } else {
        i32_field(request, "count").ok_or(StateError::InvalidRequest)?
    };
    let limit = if mode == SynthesisMode::Easy {
        rules.constants.easy_max_count
    } else {
        rules.constants.bulk_max_count
    };
    if count <= 0 || count > limit {
        return Err(StateError::InvalidRequest);
    }
    let rental_rank = if mode == SynthesisMode::Rental {
        let ranking_type = i32_field(request, "ranking_type").ok_or(StateError::InvalidRequest)?;
        let rank_number = i32_field(request, "rank_number").ok_or(StateError::InvalidRequest)?;
        if !(1..=3).contains(&ranking_type)
            || !(1..=rules.constants.max_rental_rank).contains(&rank_number)
        {
            return Err(StateError::InvalidRequest);
        }
        let mut status = status_message(&resources)?;
        let updated_at = message_i64_field(&status, "synthesis_rental_count_updated_at", "seconds")
            .unwrap_or_default();
        let old = if home::day(updated_at) == home::day(now) {
            i32_field(&status, "synthesis_rental_count").unwrap_or_default()
        } else {
            0
        };
        let next = old.checked_add(count).ok_or(StateError::InvalidRequest)?;
        if next > rules.constants.rental_daily_limit {
            return Err(StateError::InvalidRequest);
        }
        status.set_field_by_name("synthesis_rental_count", Value::I32(next));
        status.set_field_by_name(
            "synthesis_rental_count_updated_at",
            Value::Message(timestamp(proto, now)?),
        );
        resources.set_field_by_name("status", Value::Message(status));
        Some((ranking_type, rank_number))
    } else {
        None
    };

    let mut character_ids = i32_list(request, "character_ids");
    let mut ingredient_id = optional_i32_field(request, "ingredient_id");
    if let Some((ranking_type, rank_number)) = rental_rank {
        let combination =
            ranked_synthesis_combination(rules, recipe, &resources, ranking_type, rank_number)?;
        character_ids = vec![combination.character1_id, combination.character2_id];
        ingredient_id = Some(combination.ingredient_id);
    } else if mode == SynthesisMode::Easy {
        character_ids = choose_synthesis_characters(rules, recipe, &resources, mode)?;
        ingredient_id = Some(choose_synthesis_ingredient(
            rules,
            recipe,
            Some(&resources),
            count,
        )?);
    }
    if character_ids.len() != 2
        || character_ids.iter().enumerate().any(|(index, id)| {
            *id <= 0
                || (mode != SynthesisMode::Rental && !character_present(&resources, *id))
                || rules.characters.iter().all(|row| row.id != *id)
                || character_ids[..index].contains(id)
        })
    {
        return Err(StateError::InvalidRequest);
    }
    let ingredient_id = ingredient_id
        .filter(|id| {
            rules
                .ingredients
                .iter()
                .any(|row| row.id == *id && row.item_type == 1)
        })
        .ok_or(StateError::InvalidRequest)?;
    let trait_stimulator_ids = i32_list(request, "trait_stimulator_ids");
    if trait_stimulator_ids.len()
        > usize::try_from(synthesis_trait_count(rules, recipe)?)
            .map_err(|_| StateError::InvalidRequest)?
        || trait_stimulator_ids
            .iter()
            .enumerate()
            .any(|(index, id)| *id <= 0 || trait_stimulator_ids[..index].contains(id))
    {
        return Err(StateError::InvalidRequest);
    }

    let mut required: BTreeMap<(i32, i32), i32> = BTreeMap::new();
    for cost in &recipe.costs {
        let quantity = cost
            .quantity
            .checked_mul(count)
            .ok_or(StateError::InvalidRequest)?;
        let total = required.entry((cost.resource_type, cost.id)).or_default();
        *total = total
            .checked_add(quantity)
            .ok_or(StateError::InvalidRequest)?;
    }
    let ingredient_total = required.entry((5, ingredient_id)).or_default();
    *ingredient_total = ingredient_total
        .checked_add(count)
        .ok_or(StateError::InvalidRequest)?;
    for ((resource_type, id), quantity) in &required {
        let owned = match resource_type {
            5 => item_quantity(&resources, *id).unwrap_or_default(),
            8 => character_piece_quantity(&resources, *id),
            _ => return Err(StateError::InvalidRequest),
        };
        if owned < *quantity {
            return Err(StateError::InvalidRequest);
        }
    }
    let mana_cost = recipe
        .mana_cost
        .checked_mul(count)
        .ok_or(StateError::InvalidRequest)?;
    validate_stimulators(rules, recipe, &resources, &trait_stimulator_ids, count)?;

    let mut changed = empty_message(proto, "blend.model.Resources")?;
    let mut changed_items = BTreeMap::new();
    let mut changed_pieces = BTreeMap::new();
    for ((resource_type, id), quantity) in required {
        match resource_type {
            5 => {
                changed_items.insert(id, change_item(proto, &mut resources, id, -quantity)?);
            }
            8 => {
                changed_pieces.insert(
                    id,
                    change_character_piece(proto, &mut resources, id, -quantity)?,
                );
            }
            _ => unreachable!(),
        }
    }
    let mut status = change_mana(proto, rules, &mut resources, -mana_cost, now)?;
    let next_tutorial_step = match recipe_id {
        6 if quest_clear_count(&resources, 101001003) == 1
            && quest_clear_count(&resources, 101001004) == 0
            && tutorial_step(&resources) == 0 =>
        {
            Some(TUTORIAL_STEP_FIRST_SYNTHESIS)
        }
        2 if quest_clear_count(&resources, 101001010) == 1
            && quest_clear_count(&resources, 101001011) == 0
            && tutorial_step(&resources) == TUTORIAL_STEP_FIRST_SYNTHESIS =>
        {
            Some(TUTORIAL_STEP_SECOND_SYNTHESIS)
        }
        _ => None,
    };
    if let Some(step) = next_tutorial_step {
        status.set_field_by_name("tutorial_step", Value::I32(step));
        resources.set_field_by_name("status", Value::Message(status.clone()));
    }
    changed.set_field_by_name("status", Value::Message(status));

    let recipe_patch = updated_recipe_message(
        proto,
        &resources,
        recipe_id,
        &character_ids,
        Some(ingredient_id),
        &trait_stimulator_ids,
    )?;
    upsert_recipe(&mut resources, recipe_patch.clone());
    changed.set_field_by_name("recipes", Value::List(vec![Value::Message(recipe_patch)]));

    let source_traits = synthesis_trait_candidates(
        rules,
        recipe,
        &resources,
        &character_ids,
        ingredient_id,
        false,
    )?;
    let mut best_result: Option<((i32, i32), i32)> = None;
    let mut rewards = Vec::new();
    let mut bonus_rolls = Vec::new();
    for _ in 0..count {
        for slot in 0..rules.local_policy.result_slot_count {
            let target = slot == 0
                || recipe.bonus_rewards.is_empty()
                || random_below(100)? < rules.local_policy.extra_target_rate;
            let result = if target {
                &recipe.target
            } else {
                let index = usize::try_from(random_below(
                    u32::try_from(recipe.bonus_rewards.len())
                        .map_err(|_| StateError::InvalidRequest)?,
                )?)
                .map_err(|_| StateError::InvalidRequest)?;
                &recipe.bonus_rewards[index]
            };
            if matches!(result.resource_type, 6 | 14 | 25) {
                let traits = select_full_synthesis_traits(
                    rules,
                    recipe,
                    &resources,
                    &character_ids,
                    ingredient_id,
                    &trait_stimulator_ids,
                    mode == SynthesisMode::Easy,
                )?;
                let is_new = !synthesis_output_present(&resources, result);
                let output = add_synthesis_output(proto, &mut resources, result, &traits, now)?;
                let entity_id =
                    i32_field(&output, "entity_id").ok_or(StateError::InvalidRequest)?;
                let rank_total = traits.iter().try_fold(0i32, |total, value| {
                    total
                        .checked_add(value.rank)
                        .ok_or(StateError::InvalidRequest)
                })?;
                let quality = if result.resource_type == 25 {
                    0
                } else {
                    activities::prelude::gift_quality(
                        activity_rules,
                        &output,
                        result.resource_type,
                    )?
                };
                let result_index =
                    i32::try_from(rewards.len()).map_err(|_| StateError::InvalidRequest)?;
                if best_result.is_none_or(|(score, _)| (rank_total, quality) > score) {
                    best_result = Some(((rank_total, quality), result_index));
                }
                append_changed_message(
                    &mut changed,
                    synthesis_output_field(result.resource_type)?,
                    output,
                );
                let mut reward = reward_message(proto, result, is_new)?;
                reward.set_field_by_name("entity_id", Value::I32(entity_id));
                reward.set_field_by_name(
                    "resource_params",
                    Value::Message(synthesis_resource_params(proto, &traits)?),
                );
                rewards.push(Value::Message(reward));
            } else {
                bonus_rolls.push(result);
            }
        }
    }

    let mut item_bonuses = BTreeMap::new();
    for bonus in bonus_rolls {
        let entry = item_bonuses.entry(bonus.id).or_insert(0i32);
        *entry = entry
            .checked_add(bonus.quantity)
            .ok_or(StateError::InvalidRequest)?;
    }
    for (id, quantity) in item_bonuses {
        apply_synthesis_item_bonus(
            proto,
            rules,
            &mut resources,
            &mut changed,
            &mut changed_items,
            &TutorialReward {
                resource_type: 5,
                id,
                quantity,
                resource_params: None,
            },
            &mut rewards,
        )?;
    }
    if !changed_items.is_empty() {
        changed.set_field_by_name(
            "items",
            Value::List(changed_items.into_values().map(Value::Message).collect()),
        );
    }
    if !changed_pieces.is_empty() {
        changed.set_field_by_name(
            "character_pieces",
            Value::List(changed_pieces.into_values().map(Value::Message).collect()),
        );
    }
    let mut awarded_stimulators = BTreeMap::new();
    for _ in 0..count {
        for candidate in &source_traits {
            if random_below(100)? < rules.local_policy.stimulator_drop_rate {
                *awarded_stimulators.entry(candidate.id).or_insert(0i32) += 1;
            }
        }
    }
    consume_and_award_stimulators(
        proto,
        rules,
        recipe,
        &mut resources,
        &mut changed,
        &trait_stimulator_ids,
        &awarded_stimulators,
        count,
        &mut rewards,
    )?;
    home::synthesis_progress(
        proto,
        home_rules,
        &mut resources,
        &mut changed,
        recipe_id,
        count,
        now,
    )?;
    home::advance_multi_mission_synthesis(
        proto,
        home_rules,
        &mut resources,
        &mut changed,
        count,
        now,
    )?;
    home::advance_missions(
        proto,
        home_rules,
        &mut resources,
        &mut changed,
        now,
        Some((&format!("synthesis_ingredient:{ingredient_id}"), count)),
    )?;
    for id in &character_ids {
        home::advance_missions(
            proto,
            home_rules,
            &mut resources,
            &mut changed,
            now,
            Some((&format!("synthesis_character:{id}"), count)),
        )?;
    }
    home::resource_progress(
        proto,
        home_rules,
        &before,
        &mut resources,
        &mut changed,
        now,
    )?;

    let response_name = if mode == SynthesisMode::Easy {
        "blend.api.SynthesisExecuteEasyResponse"
    } else {
        "blend.api.SynthesisExecuteResponse"
    };
    let mut response = empty_message(proto, response_name)?;
    response.set_field_by_name("changed_resources", Value::Message(changed));
    response.set_field_by_name("rewards", Value::List(rewards));
    if mode != SynthesisMode::Easy {
        let (grade, start_grade_up, final_grade_up) = roll_synthesis_grade(
            rules,
            source_traits.iter().all(|candidate| candidate.active),
        )?;
        response.set_field_by_name("grade", Value::I32(grade));
        response.set_field_by_name("start_grade_up", Value::I32(start_grade_up));
        response.set_field_by_name("final_grade_up", Value::I32(final_grade_up));
        if let Some((_, index)) = best_result {
            response.set_field_by_name("drama_reward_index", Value::I32(index));
        }
    }
    Ok(ResourceMutation {
        resources,
        response,
    })
}

fn roll_synthesis_grade(
    rules: &SynthesisRules,
    fully_linked: bool,
) -> Result<(i32, i32, i32), StateError> {
    let rate = rules.local_policy.great_success_rate.min(100);
    let grade = if random_below(100)? < rate {
        if fully_linked && random_below(100)? < rate {
            4
        } else {
            3
        }
    } else if fully_linked {
        2
    } else {
        0
    };
    // The client treats grade as final and subtracts both upgrade fields.
    let final_grade_up = i32::from(grade == 4 || (grade == 3 && random_below(100)? < rate));
    Ok((grade, 0, final_grade_up))
}

pub(crate) fn synthesis_trait_count(
    rules: &SynthesisRules,
    recipe: &SynthesisRecipe,
) -> Result<i32, StateError> {
    match recipe.target.resource_type {
        6 => Ok(rules.constants.equipment_trait_count),
        14 => Ok(rules.constants.battle_trait_count),
        25 => Ok(0),
        _ => Err(StateError::InvalidRequest),
    }
}

pub(crate) fn synthesis_tool<'a>(
    rules: &'a SynthesisRules,
    reward: &TutorialReward,
) -> Result<Option<&'a SynthesisTool>, StateError> {
    match reward.resource_type {
        6 => rules
            .equipment_tools
            .iter()
            .find(|tool| tool.id == reward.id)
            .map(Some),
        14 => rules
            .battle_tools
            .iter()
            .find(|tool| tool.id == reward.id)
            .map(Some),
        25 => return Ok(None),
        _ => return Err(StateError::InvalidRequest),
    }
    .ok_or(StateError::InvalidRequest)
}

pub(crate) fn synthesis_trait_valid(
    rules: &SynthesisRules,
    recipe: &SynthesisRecipe,
    trait_id: i32,
) -> bool {
    let traits = if recipe.target.resource_type == 14 {
        &rules.battle_traits
    } else {
        &rules.equipment_traits
    };
    let Some(trait_rule) = traits.iter().find(|row| row.id == trait_id) else {
        return false;
    };
    if recipe.target.resource_type != 14 {
        return true;
    }
    let Ok(Some(tool)) = synthesis_tool(rules, &recipe.target) else {
        return false;
    };
    trait_rule
        .filter_ids
        .iter()
        .any(|id| tool.trait_filter_ids.contains(id))
}

pub(crate) fn synthesis_source_traits<'a>(
    recipe: &SynthesisRecipe,
    battle: &'a [i32],
    equipment: &'a [i32],
) -> &'a [i32] {
    match recipe.target.resource_type {
        14 => battle,
        6 => equipment,
        _ => &[],
    }
}

pub(crate) fn choose_synthesis_characters(
    rules: &SynthesisRules,
    recipe: &SynthesisRecipe,
    resources: &DynamicMessage,
    mode: SynthesisMode,
) -> Result<Vec<i32>, StateError> {
    let needs_traits = synthesis_trait_count(rules, recipe)? > 0;
    let owned: Vec<i32> = message_list(resources, "characters")
        .iter()
        .filter_map(|character| i32_field(character, "character_id"))
        .collect();
    let mut selected = Vec::new();
    for id in recipe
        .support_character_ids
        .iter()
        .chain(rules.characters.iter().map(|row| &row.id))
    {
        let Some(character) = rules.characters.iter().find(|row| row.id == *id) else {
            continue;
        };
        let traits = synthesis_source_traits(
            recipe,
            &character.battle_trait_ids,
            &character.equipment_trait_ids,
        );
        if (needs_traits
            && !traits
                .iter()
                .any(|trait_id| synthesis_trait_valid(rules, recipe, *trait_id)))
            || (mode != SynthesisMode::Rental && !owned.contains(id))
            || selected.contains(id)
        {
            continue;
        }
        selected.push(*id);
        if selected.len() == 2 {
            return Ok(selected);
        }
    }
    Err(StateError::InvalidRequest)
}

pub(crate) fn choose_synthesis_ingredient(
    rules: &SynthesisRules,
    recipe: &SynthesisRecipe,
    resources: Option<&DynamicMessage>,
    count: i32,
) -> Result<i32, StateError> {
    let needs_traits = synthesis_trait_count(rules, recipe)? > 0;
    rules
        .ingredients
        .iter()
        .filter(|ingredient| {
            if ingredient.item_type != 1 {
                return false;
            }
            let traits = synthesis_source_traits(
                recipe,
                &ingredient.battle_trait_ids,
                &ingredient.equipment_trait_ids,
            );
            (!needs_traits
                || traits
                    .iter()
                    .any(|trait_id| synthesis_trait_valid(rules, recipe, *trait_id)))
                && (recipe.support_color_ids.is_empty()
                    || recipe
                        .support_color_ids
                        .contains(&ingredient.trait_color_id))
                && resources.is_none_or(|resources| {
                    let recipe_cost = recipe
                        .costs
                        .iter()
                        .filter(|cost| cost.resource_type == 5 && cost.id == ingredient.id)
                        .map(|cost| cost.quantity)
                        .sum::<i32>();
                    item_quantity(resources, ingredient.id).unwrap_or_default()
                        >= (recipe_cost + 1).saturating_mul(count)
                })
        })
        .max_by_key(|ingredient| {
            resources
                .and_then(|resources| item_quantity(resources, ingredient.id))
                .unwrap_or_default()
        })
        .map(|ingredient| ingredient.id)
        .ok_or(StateError::InvalidRequest)
}

pub(crate) fn synthesis_trait_rule<'a>(
    rules: &'a SynthesisRules,
    recipe: &SynthesisRecipe,
    trait_id: i32,
) -> Option<&'a SynthesisTrait> {
    let traits = if recipe.target.resource_type == 14 {
        &rules.battle_traits
    } else {
        &rules.equipment_traits
    };
    traits.iter().find(|row| row.id == trait_id)
}

pub(crate) fn synthesis_user_rank_bonus(rules: &SynthesisRules, resources: &DynamicMessage) -> u32 {
    let rank = status_message(resources)
        .ok()
        .and_then(|status| i32_field(&status, "rank"))
        .unwrap_or(1);
    rules
        .user_rank_bonuses
        .iter()
        .find(|row| row.id == rank)
        .map(|row| row.weight_bonus)
        .unwrap_or_default()
}

pub(crate) fn synthesis_character_rank_bonus(
    rules: &SynthesisRules,
    resources: &DynamicMessage,
    character_id: i32,
) -> Result<u32, StateError> {
    let Ok(character) = resource_character(resources, character_id) else {
        return Ok(0);
    };
    let rarity = i32_field(&character, "rarity").ok_or(StateError::InvalidRequest)?;
    let rarity_bonus = rules
        .character_rarity_bonuses
        .iter()
        .find(|row| row.id == rarity)
        .map(|row| row.weight_bonus)
        .unwrap_or_default();
    let growboard_bonus = i32_field(&character, "growboard_trait_rank_weight_bonus")
        .unwrap_or_default()
        .max(0) as u32;
    rarity_bonus
        .checked_add(growboard_bonus)
        .ok_or(StateError::InvalidRequest)
}

pub(crate) fn synthesis_trait_candidates(
    rules: &SynthesisRules,
    recipe: &SynthesisRecipe,
    resources: &DynamicMessage,
    character_ids: &[i32],
    ingredient_id: i32,
    easy: bool,
) -> Result<Vec<SynthesisTraitCandidate>, StateError> {
    let user_bonus = synthesis_user_rank_bonus(rules, resources);
    let mut pool = Vec::new();
    if easy {
        let tool = synthesis_tool(rules, &recipe.target)?.ok_or(StateError::InvalidRequest)?;
        let lotteries = if recipe.target.resource_type == 14 {
            &rules.battle_lotteries
        } else {
            &rules.equipment_lotteries
        };
        pool.extend(
            lotteries
                .iter()
                .find(|row| row.id == tool.easy_lottery_id)
                .ok_or(StateError::InvalidRequest)?
                .trait_ids
                .iter()
                .copied()
                .map(|id| SynthesisTraitCandidate {
                    id,
                    rank_weight_bonus: user_bonus,
                    active: true,
                }),
        );
    } else {
        let first = rules
            .characters
            .iter()
            .find(|row| Some(row.id) == character_ids.first().copied())
            .ok_or(StateError::InvalidRequest)?;
        let second = rules
            .characters
            .iter()
            .find(|row| Some(row.id) == character_ids.get(1).copied())
            .ok_or(StateError::InvalidRequest)?;
        let ingredient = rules
            .ingredients
            .iter()
            .find(|row| row.id == ingredient_id)
            .ok_or(StateError::InvalidRequest)?;
        let active = [
            recipe.support_color_ids.is_empty()
                || recipe.support_color_ids.contains(&first.trait_color_id),
            first.support_color_id == second.trait_color_id,
            second.support_color_id == ingredient.trait_color_id,
        ];
        for (index, row) in [first, second].into_iter().enumerate() {
            let rank_weight_bonus = user_bonus
                .checked_add(synthesis_character_rank_bonus(rules, resources, row.id)?)
                .ok_or(StateError::InvalidRequest)?;
            pool.extend(
                synthesis_source_traits(recipe, &row.battle_trait_ids, &row.equipment_trait_ids)
                    .iter()
                    .copied()
                    .map(|id| SynthesisTraitCandidate {
                        id,
                        rank_weight_bonus,
                        active: active[index],
                    }),
            );
        }
        pool.extend(
            synthesis_source_traits(
                recipe,
                &ingredient.battle_trait_ids,
                &ingredient.equipment_trait_ids,
            )
            .iter()
            .copied()
            .map(|id| SynthesisTraitCandidate {
                id,
                rank_weight_bonus: user_bonus,
                active: active[2],
            }),
        );
    }
    Ok(pool)
}

pub(crate) fn draw_trait_rank(
    rules: &SynthesisRules,
    rank_weight_bonus: u32,
) -> Result<i32, StateError> {
    let weights = rules
        .trait_ranks
        .iter()
        .map(|rank| {
            if rank.id >= 4 {
                let denominator = rules.local_policy.high_rank_bonus_denominator;
                u64::from(rank.weight) * u64::from(denominator + rank_weight_bonus)
                    / u64::from(denominator)
            } else {
                u64::from(rank.weight)
            }
        })
        .collect::<Vec<_>>();
    let total = weights.iter().sum::<u64>();
    let mut draw = u64::from(random_below(
        u32::try_from(total).map_err(|_| StateError::InvalidRequest)?,
    )?);
    for (rank, weight) in rules.trait_ranks.iter().zip(weights) {
        if draw < weight {
            return Ok(rank.id);
        }
        draw -= weight;
    }
    Err(StateError::InvalidRequest)
}

pub(crate) fn select_full_synthesis_traits(
    rules: &SynthesisRules,
    recipe: &SynthesisRecipe,
    resources: &DynamicMessage,
    character_ids: &[i32],
    ingredient_id: i32,
    stimulators: &[i32],
    easy: bool,
) -> Result<Vec<TutorialTraitParam>, StateError> {
    let count = usize::try_from(synthesis_trait_count(rules, recipe)?)
        .map_err(|_| StateError::InvalidRequest)?;
    if count == 0 {
        return Ok(Vec::new());
    }
    let mut pool =
        synthesis_trait_candidates(rules, recipe, resources, character_ids, ingredient_id, easy)?;
    if stimulators
        .iter()
        .any(|id| pool.iter().all(|candidate| candidate.id != *id))
    {
        return Err(StateError::InvalidRequest);
    }
    let mut selected = Vec::with_capacity(count);
    for _ in 0..count {
        if pool.is_empty() {
            break;
        }
        let total_weight = pool.iter().try_fold(0u32, |total, candidate| {
            total
                .checked_add(if stimulators.contains(&candidate.id) {
                    rules.local_policy.stimulator_weight
                } else {
                    1
                })
                .ok_or(StateError::InvalidRequest)
        })?;
        let mut draw = random_below(total_weight)?;
        let index = pool
            .iter()
            .position(|candidate| {
                let weight = if stimulators.contains(&candidate.id) {
                    rules.local_policy.stimulator_weight
                } else {
                    1
                };
                if draw < weight {
                    true
                } else {
                    draw -= weight;
                    false
                }
            })
            .ok_or(StateError::InvalidRequest)?;
        let candidate = if easy {
            pool[index].clone()
        } else {
            pool.swap_remove(index)
        };
        if synthesis_trait_rule(rules, recipe, candidate.id)
            .is_some_and(|row| row.disable_duplicate_assign)
        {
            pool.retain(|row| row.id != candidate.id);
        }
        if candidate.active {
            selected.push(TutorialTraitParam {
                id: candidate.id,
                rank: draw_trait_rank(rules, candidate.rank_weight_bonus)?,
            });
        }
    }
    Ok(selected)
}

pub(crate) fn synthesis_output_field(resource_type: i32) -> Result<&'static str, StateError> {
    match resource_type {
        6 => Ok("equipment_tools"),
        14 => Ok("battle_tools"),
        25 => Ok("ship_tools"),
        _ => Err(StateError::InvalidRequest),
    }
}

pub(crate) fn synthesis_output_present(
    resources: &DynamicMessage,
    reward: &TutorialReward,
) -> bool {
    synthesis_output_field(reward.resource_type)
        .ok()
        .is_some_and(|field| {
            message_list(resources, field)
                .iter()
                .any(|tool| i32_field(tool, "tool_id") == Some(reward.id))
        })
}

pub(crate) fn synthesis_resource_params(
    proto: &ProtoRegistry,
    traits: &[TutorialTraitParam],
) -> Result<DynamicMessage, StateError> {
    let mut params = empty_message(proto, "blend.model.ResourceParams")?;
    params.set_field_by_name(
        "traits",
        Value::List(
            traits
                .iter()
                .map(|value| trait_params_message(proto, value).map(Value::Message))
                .collect::<Result<Vec<_>, _>>()?,
        ),
    );
    Ok(params)
}

pub(crate) fn add_synthesis_output(
    proto: &ProtoRegistry,
    resources: &mut DynamicMessage,
    reward: &TutorialReward,
    traits: &[TutorialTraitParam],
    now: i64,
) -> Result<DynamicMessage, StateError> {
    let (message_name, field) = match reward.resource_type {
        6 => ("blend.model.EquipmentTool", "equipment_tools"),
        14 => ("blend.model.BattleTool", "battle_tools"),
        25 => ("blend.model.ShipTool", "ship_tools"),
        _ => return Err(StateError::InvalidRequest),
    };
    let entity_id = next_entity_id(resources)?;
    let mut tool = empty_message(proto, message_name)?;
    tool.set_field_by_name("entity_id", Value::I32(entity_id));
    tool.set_field_by_name("tool_id", Value::I32(reward.id));
    if reward.resource_type != 25 {
        tool.set_field_by_name(
            "traits",
            Value::List(
                traits
                    .iter()
                    .map(|value| trait_params_message(proto, value).map(Value::Message))
                    .collect::<Result<Vec<_>, _>>()?,
            ),
        );
        tool.set_field_by_name("is_locked", Value::Bool(false));
    } else {
        tool.set_field_by_name("rank", Value::I32(1));
    }
    tool.set_field_by_name("received_at", Value::Message(timestamp(proto, now)?));
    let mut tools = message_list(resources, field);
    tools.push(tool.clone());
    resources.set_field_by_name(
        field,
        Value::List(tools.into_iter().map(Value::Message).collect()),
    );
    Ok(tool)
}

pub(crate) fn append_changed_message(
    changed: &mut DynamicMessage,
    field: &str,
    value: DynamicMessage,
) {
    let mut values = message_list(changed, field);
    values.push(value);
    changed.set_field_by_name(
        field,
        Value::List(values.into_iter().map(Value::Message).collect()),
    );
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_synthesis_item_bonus(
    proto: &ProtoRegistry,
    rules: &SynthesisRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    changed_items: &mut BTreeMap<i32, DynamicMessage>,
    bonus: &TutorialReward,
    rewards: &mut Vec<Value>,
) -> Result<(), StateError> {
    if bonus.resource_type != 5 {
        return Err(StateError::InvalidRequest);
    }
    let is_new = item_quantity(resources, bonus.id).unwrap_or_default() == 0;
    let reward = reward_message(proto, bonus, is_new)?;
    changed_items.insert(
        bonus.id,
        change_item(proto, resources, bonus.id, bonus.quantity)?,
    );
    if rules
        .ingredients
        .iter()
        .any(|item| item.id == bonus.id && item.item_type == 4)
    {
        let task = increment_task_count(resources, 293, bonus.quantity)?;
        let mut tasks = message_list(changed, "total_task_counts");
        tasks.push(task);
        set_changed_task_counts(changed, tasks);
    }
    rewards.push(Value::Message(reward));
    Ok(())
}

pub(crate) fn character_piece_quantity(resources: &DynamicMessage, character_id: i32) -> i32 {
    message_list(resources, "character_pieces")
        .iter()
        .find(|piece| i32_field(piece, "character_id") == Some(character_id))
        .and_then(|piece| i32_field(piece, "quantity"))
        .unwrap_or_default()
}

pub(crate) fn stimulator_fields(
    resource_type: i32,
) -> Result<(&'static str, &'static str, &'static str, i32), StateError> {
    match resource_type {
        14 => Ok((
            "battle_tool_trait_stimulators",
            "blend.model.BattleToolTraitStimulator",
            "battle_tool_trait_id",
            19,
        )),
        6 => Ok((
            "equipment_tool_trait_stimulators",
            "blend.model.EquipmentToolTraitStimulator",
            "equipment_tool_trait_id",
            20,
        )),
        _ => Err(StateError::InvalidRequest),
    }
}

pub(crate) fn stimulator_quantity(
    resources: &DynamicMessage,
    resource_type: i32,
    trait_id: i32,
) -> i32 {
    let Ok((field, _, id_field, _)) = stimulator_fields(resource_type) else {
        return 0;
    };
    message_list(resources, field)
        .iter()
        .find(|value| i32_field(value, id_field) == Some(trait_id))
        .and_then(|value| i32_field(value, "quantity"))
        .unwrap_or_default()
}

pub(crate) fn change_stimulator(
    proto: &ProtoRegistry,
    resources: &mut DynamicMessage,
    resource_type: i32,
    trait_id: i32,
    delta: i32,
) -> Result<DynamicMessage, StateError> {
    let (field, message_name, id_field, _) = stimulator_fields(resource_type)?;
    let mut values = message_list(resources, field);
    let mut changed = None;
    if let Some(value) = values
        .iter_mut()
        .find(|value| i32_field(value, id_field) == Some(trait_id))
    {
        let next = i32_field(value, "quantity")
            .unwrap_or_default()
            .checked_add(delta)
            .filter(|value| *value >= 0)
            .ok_or(StateError::InvalidRequest)?;
        value.set_field_by_name("quantity", Value::I32(next));
        changed = Some(value.clone());
    } else if delta >= 0 {
        let mut value = empty_message(proto, message_name)?;
        value.set_field_by_name(id_field, Value::I32(trait_id));
        value.set_field_by_name("quantity", Value::I32(delta));
        values.push(value.clone());
        changed = Some(value);
    }
    resources.set_field_by_name(
        field,
        Value::List(values.into_iter().map(Value::Message).collect()),
    );
    changed.ok_or(StateError::InvalidRequest)
}

pub(crate) fn validate_stimulators(
    rules: &SynthesisRules,
    recipe: &SynthesisRecipe,
    resources: &DynamicMessage,
    stimulators: &[i32],
    count: i32,
) -> Result<(), StateError> {
    if recipe.target.resource_type == 25 && !stimulators.is_empty() {
        return Err(StateError::InvalidRequest);
    }
    if stimulators.iter().any(|id| {
        !synthesis_trait_valid(rules, recipe, *id)
            || stimulator_quantity(resources, recipe.target.resource_type, *id) < count
    }) {
        return Err(StateError::InvalidRequest);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn consume_and_award_stimulators(
    proto: &ProtoRegistry,
    _rules: &SynthesisRules,
    recipe: &SynthesisRecipe,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    consumed: &[i32],
    awarded: &BTreeMap<i32, i32>,
    count: i32,
    rewards: &mut Vec<Value>,
) -> Result<(), StateError> {
    if recipe.target.resource_type == 25 {
        return Ok(());
    }
    let (field, _, _, reward_type) = stimulator_fields(recipe.target.resource_type)?;
    let mut deltas = BTreeMap::<i32, i32>::new();
    for id in consumed {
        let value = deltas.entry(*id).or_default();
        *value = value.checked_sub(count).ok_or(StateError::InvalidRequest)?;
    }
    for (id, quantity) in awarded {
        let value = deltas.entry(*id).or_default();
        *value = value
            .checked_add(*quantity)
            .ok_or(StateError::InvalidRequest)?;
    }
    let mut changes = Vec::new();
    for (id, delta) in deltas {
        let was_new = stimulator_quantity(resources, recipe.target.resource_type, id) == 0;
        let change = change_stimulator(proto, resources, recipe.target.resource_type, id, delta)?;
        changes.push(Value::Message(change));
        if delta > 0 {
            let mut reward = empty_message(proto, "blend.model.Reward")?;
            reward.set_field_by_name("type", Value::I32(reward_type));
            reward.set_field_by_name("id", Value::I32(id));
            reward.set_field_by_name("quantity", Value::I32(delta));
            reward.set_field_by_name("is_new", Value::Bool(was_new));
            rewards.push(Value::Message(reward));
        }
    }
    changed.set_field_by_name(field, Value::List(changes));
    Ok(())
}

pub(crate) fn increment_task_count(
    resources: &mut DynamicMessage,
    condition_id: i32,
    delta: i32,
) -> Result<DynamicMessage, StateError> {
    let next = total_task_count(resources, condition_id)
        .checked_add(delta)
        .ok_or(StateError::InvalidRequest)?;
    set_total_task_count(resources, condition_id, next)
}

pub(crate) fn updated_recipe_message(
    proto: &ProtoRegistry,
    resources: &DynamicMessage,
    recipe_id: i32,
    character_ids: &[i32],
    ingredient_id: Option<i32>,
    trait_stimulator_ids: &[i32],
) -> Result<DynamicMessage, StateError> {
    let mut recipe = message_list(resources, "recipes")
        .into_iter()
        .find(|recipe| i32_field(recipe, "recipe_id") == Some(recipe_id))
        .unwrap_or(empty_message(proto, "blend.model.Recipe")?);
    recipe.set_field_by_name("recipe_id", Value::I32(recipe_id));
    recipe.set_field_by_name(
        "last_character_ids",
        Value::List(character_ids.iter().copied().map(Value::I32).collect()),
    );
    if let Some(ingredient_id) = ingredient_id {
        recipe.set_field_by_name(
            "last_ingredient_id",
            Value::Message(int32_value(proto, ingredient_id)?),
        );
    }
    recipe.set_field_by_name(
        "last_trait_stimulator_ids",
        Value::List(
            trait_stimulator_ids
                .iter()
                .copied()
                .map(Value::I32)
                .collect(),
        ),
    );
    Ok(recipe)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthesis_grade_contract_covers_all_values() {
        let mut rules = load_synthesis_rules().unwrap();
        rules.local_policy.great_success_rate = 0;
        assert_eq!(roll_synthesis_grade(&rules, false).unwrap(), (0, 0, 0));
        assert_eq!(roll_synthesis_grade(&rules, true).unwrap(), (2, 0, 0));

        rules.local_policy.great_success_rate = 100;
        assert_eq!(roll_synthesis_grade(&rules, false).unwrap(), (3, 0, 1));
        assert_eq!(roll_synthesis_grade(&rules, true).unwrap(), (4, 0, 1));
    }
}
