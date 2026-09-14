use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use crate::state::service::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SynthesisCombination {
    pub(crate) character1_id: i32,
    pub(crate) character2_id: i32,
    pub(crate) ingredient_id: i32,
}

fn scope_tiebreak(recipe_id: i32, scope: i32, first: i32, second: i32, ingredient: i32) -> u64 {
    let mut hasher = DefaultHasher::new();
    (recipe_id, scope, first, second, ingredient).hash(&mut hasher);
    hasher.finish()
}

fn stored_combination(resources: &DynamicMessage, recipe_id: i32) -> Option<SynthesisCombination> {
    let recipe = message_list(resources, "recipes")
        .into_iter()
        .find(|value| i32_field(value, "recipe_id") == Some(recipe_id))?;
    let characters = i32_list(&recipe, "last_character_ids");
    Some(SynthesisCombination {
        character1_id: *characters.first()?,
        character2_id: *characters.get(1)?,
        ingredient_id: optional_i32_field(&recipe, "last_ingredient_id")?,
    })
}

fn valid_trait_count(
    rules: &SynthesisRules,
    recipe: &SynthesisRecipe,
    battle_traits: &[i32],
    equipment_traits: &[i32],
) -> usize {
    synthesis_source_traits(recipe, battle_traits, equipment_traits)
        .iter()
        .filter(|id| synthesis_trait_valid(rules, recipe, **id))
        .count()
}

fn ranked_combinations(
    rules: &SynthesisRules,
    recipe: &SynthesisRecipe,
    resources: &DynamicMessage,
    scope: i32,
) -> Result<Vec<SynthesisCombination>, StateError> {
    if !(1..=3).contains(&scope) {
        return Err(StateError::InvalidRequest);
    }
    let row_count =
        usize::try_from(rules.constants.max_rental_rank).map_err(|_| StateError::InvalidRequest)?;
    if row_count == 0 {
        return Err(StateError::InvalidRequest);
    }
    let needs_traits = synthesis_trait_count(rules, recipe)? > 0;
    let mut characters = Vec::new();
    for character in &rules.characters {
        let traits = valid_trait_count(
            rules,
            recipe,
            &character.battle_trait_ids,
            &character.equipment_trait_ids,
        );
        if needs_traits && traits == 0 {
            continue;
        }
        characters.push((
            character,
            traits,
            character_present(resources, character.id),
            synthesis_character_rank_bonus(rules, resources, character.id)?,
        ));
    }
    characters.sort_unstable_by_key(|(character, traits, owned, bonus)| {
        (
            recipe.support_character_ids.contains(&character.id),
            *owned,
            *bonus,
            *traits,
            scope_tiebreak(recipe.id, scope, character.id, 0, 0),
        )
    });
    characters.reverse();

    let mut ingredients = Vec::new();
    for ingredient in rules
        .ingredients
        .iter()
        .filter(|ingredient| ingredient.item_type == 1)
    {
        let traits = valid_trait_count(
            rules,
            recipe,
            &ingredient.battle_trait_ids,
            &ingredient.equipment_trait_ids,
        );
        if needs_traits && traits == 0 {
            continue;
        }
        let recipe_cost = recipe
            .costs
            .iter()
            .filter(|cost| cost.resource_type == 5 && cost.id == ingredient.id)
            .try_fold(0i32, |total, cost| total.checked_add(cost.quantity))
            .ok_or(StateError::InvalidRequest)?;
        let quantity = item_quantity(resources, ingredient.id).unwrap_or_default();
        let needed = recipe_cost
            .checked_add(1)
            .ok_or(StateError::InvalidRequest)?;
        ingredients.push((ingredient, traits, quantity, quantity >= needed));
    }
    ingredients.sort_unstable_by_key(|(ingredient, traits, quantity, available)| {
        (
            *available,
            *quantity,
            *traits,
            scope_tiebreak(recipe.id, scope, 0, 0, ingredient.id),
        )
    });
    ingredients.reverse();

    let mut result = Vec::with_capacity(row_count);
    if let Some(value) = stored_combination(resources, recipe.id) {
        let first = characters
            .iter()
            .find(|(character, ..)| character.id == value.character1_id);
        let second = characters
            .iter()
            .find(|(character, ..)| character.id == value.character2_id);
        let ingredient = ingredients
            .iter()
            .find(|(ingredient, ..)| ingredient.id == value.ingredient_id);
        if let (Some((first, ..)), Some((second, ..)), Some((ingredient, _, _, available))) =
            (first, second, ingredient)
        {
            if *available
                && first.id != second.id
                && (recipe.support_color_ids.is_empty()
                    || recipe.support_color_ids.contains(&first.trait_color_id))
                && first.support_color_id == second.trait_color_id
                && second.support_color_id == ingredient.trait_color_id
            {
                result.push(value);
            }
        }
    }
    for (first, ..) in &characters {
        if !recipe.support_color_ids.is_empty()
            && !recipe.support_color_ids.contains(&first.trait_color_id)
        {
            continue;
        }
        for (second, ..) in &characters {
            if first.id == second.id || first.support_color_id != second.trait_color_id {
                continue;
            }
            for (ingredient, ..) in &ingredients {
                if second.support_color_id != ingredient.trait_color_id {
                    continue;
                }
                let combination = SynthesisCombination {
                    character1_id: first.id,
                    character2_id: second.id,
                    ingredient_id: ingredient.id,
                };
                if !result.contains(&combination) {
                    result.push(combination);
                    if result.len() == row_count {
                        return Ok(result);
                    }
                }
            }
        }
    }
    if result.is_empty() {
        Err(StateError::InvalidRequest)
    } else {
        Ok(result)
    }
}

pub(crate) fn ranked_synthesis_combination(
    rules: &SynthesisRules,
    recipe: &SynthesisRecipe,
    resources: &DynamicMessage,
    scope: i32,
    rank_number: i32,
) -> Result<SynthesisCombination, StateError> {
    let index = usize::try_from(
        rank_number
            .checked_sub(1)
            .ok_or(StateError::InvalidRequest)?,
    )
    .map_err(|_| StateError::InvalidRequest)?;
    ranked_combinations(rules, recipe, resources, scope)?
        .get(index)
        .copied()
        .ok_or(StateError::InvalidRequest)
}

pub(crate) fn reduce_synthesis_ranking(
    proto: &ProtoRegistry,
    rules: &SynthesisRules,
    resources: &DynamicMessage,
    request: &DynamicMessage,
    now: i64,
) -> Result<DynamicMessage, StateError> {
    let recipe_id = i32_field(request, "recipe_id").ok_or(StateError::InvalidRequest)?;
    let recipe = rules
        .recipes
        .iter()
        .find(|recipe| recipe.id == recipe_id)
        .ok_or(StateError::InvalidRequest)?;
    if !recipe_present(resources, recipe_id) {
        return Err(StateError::InvalidRequest);
    }
    let mut response = empty_message(proto, "blend.api.SynthesisCombinationRankingResponse")?;
    for (scope, field) in [(1, "total"), (2, "weekly"), (3, "monthly")] {
        let mut ranking = empty_message(proto, "blend.model.SynthesisCombinationRanking")?;
        ranking.set_field_by_name("calculated_at", Value::Message(timestamp(proto, now)?));
        ranking.set_field_by_name(
            "ranks",
            Value::List(
                ranked_combinations(rules, recipe, resources, scope)?
                    .into_iter()
                    .map(|value| {
                        let mut rank =
                            empty_message(proto, "blend.model.SynthesisCombinationRank")?;
                        rank.set_field_by_name("character1_id", Value::I32(value.character1_id));
                        rank.set_field_by_name("character2_id", Value::I32(value.character2_id));
                        rank.set_field_by_name(
                            "ingredient_id",
                            Value::Message(int32_value(proto, value.ingredient_id)?),
                        );
                        Ok(Value::Message(rank))
                    })
                    .collect::<Result<Vec<_>, StateError>>()?,
            ),
        );
        response.set_field_by_name(field, Value::Message(ranking));
    }
    Ok(response)
}
