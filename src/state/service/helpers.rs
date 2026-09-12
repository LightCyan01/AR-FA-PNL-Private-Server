use super::prelude::*;

pub(crate) fn number(value: &Json, key: &str) -> i32 {
    value[key]
        .as_i64()
        .and_then(|value| i32::try_from(value).ok())
        .unwrap_or(0)
}

pub(crate) fn values<'a>(value: &'a Json, key: &str) -> &'a [Json] {
    value[key].as_array().map(Vec::as_slice).unwrap_or(&[])
}

pub(crate) fn gameplay_storage_error(error: StateError) -> StorageError {
    match error {
        StateError::InvalidRequest | StateError::OutOfSchedule => StorageError::BattleRejected,
        other => StorageError::Battle(other.to_string()),
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn gameplay_response(
    store: &Store,
    proto: &ProtoRegistry,
    account_id: i64,
    request_id: &str,
    route: &str,
    fingerprint: &[u8],
    message_name: &str,
    result: GameplayMutationResult,
) -> Result<(DynamicMessage, GameplayMutationResult), StateError> {
    let applied = matches!(&result, GameplayMutationResult::Applied);
    let bytes = match result {
        GameplayMutationResult::Applied => store
            .gameplay_replay(account_id, request_id, route, fingerprint)?
            .ok_or_else(|| StateError::Storage(StorageError::NotFound))?,
        GameplayMutationResult::Replay(bytes) => bytes,
    };
    let response = proto
        .decode(message_name, &bytes)
        .map_err(|error| StateError::Descriptor(error.to_string()))?;
    Ok((
        response,
        if applied {
            GameplayMutationResult::Applied
        } else {
            GameplayMutationResult::Replay(bytes)
        },
    ))
}

pub fn validate_username(username: &str) -> Result<(), StateError> {
    if !(3..=32).contains(&username.len())
        || !username.is_ascii()
        || username.chars().any(char::is_whitespace)
    {
        return Err(StateError::Credentials);
    }
    Ok(())
}

pub(crate) fn string_field(message: &DynamicMessage, name: &str) -> Option<String> {
    message
        .get_field_by_name(name)?
        .as_str()
        .map(ToOwned::to_owned)
}

pub(crate) fn i32_field(message: &DynamicMessage, name: &str) -> Option<i32> {
    message.get_field_by_name(name)?.as_i32()
}

pub(crate) fn i32_or_enum_field(message: &DynamicMessage, name: &str) -> Option<i32> {
    let value = message.get_field_by_name(name)?;
    value.as_i32().or_else(|| value.as_enum_number())
}

pub(crate) fn message_i32_field(message: &DynamicMessage, outer: &str, inner: &str) -> Option<i32> {
    message
        .get_field_by_name(outer)?
        .as_message()?
        .get_field_by_name(inner)?
        .as_i32()
}

pub(crate) fn message_i64_field(message: &DynamicMessage, outer: &str, inner: &str) -> Option<i64> {
    message
        .get_field_by_name(outer)?
        .as_message()?
        .get_field_by_name(inner)?
        .as_i64()
}

pub(crate) fn unix_now() -> i64 {
    #[cfg(test)]
    if let Some(now) = SIMULATION_NOW.with(|clock| clock.get()) {
        return now;
    }
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
thread_local! {static SIMULATION_NOW:std::cell::Cell<Option<i64>>=const{std::cell::Cell::new(None)};}

#[cfg(test)]
pub(crate) fn simulation_now() -> Option<i64> {
    SIMULATION_NOW.with(|clock| clock.get())
}

#[cfg(test)]
pub(crate) fn set_simulation_now(now: Option<i64>) {
    SIMULATION_NOW.with(|clock| clock.set(now));
}

pub(crate) fn load_fresh_rules() -> Result<FreshStateRules, StateError> {
    let rules: FreshStateRules = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../data/fresh_state_rules.json"
    )))
    .map_err(|error| StateError::FreshRules(error.to_string()))?;
    if rules.format != "atelier-fresh-state-rules-v1"
        || rules.task_condition_ids.is_empty()
        || rules.initial_rewards.is_empty()
    {
        return Err(StateError::FreshRules("unsupported rules artifact".into()));
    }
    Ok(rules)
}

pub(crate) fn load_tutorial_rules() -> Result<TutorialRules, StateError> {
    let rules: TutorialRules = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../data/tutorial_rules.json"
    )))
    .map_err(|error| StateError::TutorialRules(error.to_string()))?;
    if rules.format != "atelier-tutorial-rules-v2"
        || rules.episode_id != 101001
        || rules.quests.len() != 17
        || rules
            .quests
            .iter()
            .filter(|quest| quest.quest_type == 2)
            .count()
            != 13
        || rules.battles.len() != 4
        || rules.waves.len() != 6
        || rules.stages.is_empty()
        || rules.enemies.is_empty()
        || rules.battle_characters.is_empty()
        || rules.skills.is_empty()
        || rules.gachas.is_empty()
        || rules.gacha_buttons.is_empty()
        || rules.gacha_rates.is_empty()
        || rules.gacha_decks.is_empty()
    {
        return Err(StateError::TutorialRules(
            "unsupported tutorial rules artifact".into(),
        ));
    }
    validate_tutorial_closure(&rules)?;
    Ok(rules)
}

pub(crate) fn load_gameplay_rules() -> Result<TutorialRules, StateError> {
    let tutorial = load_tutorial_rules()?;
    let rules: TutorialRules = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../data/gameplay_rules.json"
    )))
    .map_err(|e| StateError::TutorialRules(e.to_string()))?;
    if rules.source_sha256 != tutorial.source_sha256
        || rules.quests.len() < tutorial.quests.len()
        || [
            rules.constants.total_turn_weight,
            rules.constants.max_dealt_hp_damage_weight,
            rules.constants.received_hp_damage_weight,
            rules.constants.dead_allies_count_weight,
        ]
        .iter()
        .sum::<i32>()
            != 100
    {
        return Err(StateError::TutorialRules("quest source mismatch".into()));
    }
    effects::validate(&rules.source_sha256)?;
    Ok(rules)
}

pub(crate) fn load_synthesis_rules() -> Result<SynthesisRules, StateError> {
    let rules: SynthesisRules = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../data/synthesis_rules.json"
    )))
    .map_err(|error| StateError::SynthesisRules(error.to_string()))?;
    if rules.format != "atelier-synthesis-rules-v1"
        || rules.recipes.len() != 876
        || rules.characters.is_empty()
        || rules.ingredients.is_empty()
        || rules.battle_tools.is_empty()
        || rules.equipment_tools.is_empty()
        || rules.character_rarity_bonuses.is_empty()
        || rules.user_rank_bonuses.is_empty()
        || rules.local_policy.result_slot_count != 3
        || rules.local_policy.extra_target_rate > 100
        || rules.local_policy.great_success_rate > 100
        || rules.local_policy.stimulator_weight == 0
        || rules.local_policy.stimulator_drop_rate > 100
        || rules.local_policy.high_rank_bonus_denominator == 0
        || rules
            .trait_ranks
            .iter()
            .map(|rank| rank.weight)
            .sum::<u32>()
            != 10_000
    {
        return Err(StateError::SynthesisRules(
            "unsupported synthesis rules artifact".into(),
        ));
    }
    Ok(rules)
}

pub(crate) fn load_reward_rules() -> Result<RewardRules, StateError> {
    let rules: RewardRules = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../data/reward_rules.json"
    )))
    .map_err(|error| StateError::RewardRules(error.to_string()))?;
    if rules.format != "atelier-reward-rules-v1"
        || rules.battle_missions.is_empty()
        || rules.quests.is_empty()
        || rules.episodes.is_empty()
        || rules.fixed_parties.is_empty()
        || rules.drop_reward_sets.is_empty()
        || rules.battle_missions.iter().any(|mission| {
            mission.id <= 0
                || match mission.kind {
                    BattleMissionKind::TurnLimit => {
                        mission.turn_limit.is_none_or(|limit| limit <= 0)
                    }
                    _ => mission.turn_limit.is_some(),
                }
        })
        || rules.quests.iter().any(|quest| {
            quest.battle_mission_ids.iter().any(|id| {
                rules
                    .battle_missions
                    .iter()
                    .all(|mission| mission.id != *id)
            }) || quest.score_ranks.iter().any(|rank| {
                rank.reward_set_ids.iter().any(|id| {
                    rules
                        .reward_sets
                        .iter()
                        .all(|reward_set| reward_set.id != *id)
                })
            }) || quest
                .drop_reward_set_ids
                .iter()
                .chain(
                    quest
                        .score_ranks
                        .iter()
                        .flat_map(|rank| &rank.drop_reward_set_ids),
                )
                .any(|id| {
                    rules
                        .drop_reward_sets
                        .iter()
                        .all(|drop_set| drop_set.id != *id)
                })
        })
    {
        return Err(StateError::RewardRules(
            "unsupported reward rules artifact".into(),
        ));
    }
    Ok(rules)
}

pub(crate) fn load_character_rules() -> Result<&'static CharacterRules, StateError> {
    static RULES: std::sync::OnceLock<Result<CharacterRules, String>> = std::sync::OnceLock::new();
    let rules = RULES
        .get_or_init(|| {
            serde_json::from_str::<CharacterRules>(include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../data/character_rules.json"
            )))
            .map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(|error| StateError::CharacterRules(error.clone()))?;
    if rules.format != "atelier-character-rules-v1"
        || rules.characters.is_empty()
        || rules.levels.is_empty()
        || rules.rarities.len() != 8
        || rules.boards.is_empty()
        || rules.pages.is_empty()
        || rules.panels.is_empty()
    {
        return Err(StateError::CharacterRules(
            "unsupported rules artifact".into(),
        ));
    }
    Ok(rules)
}

pub(crate) fn validate_tutorial_closure(rules: &TutorialRules) -> Result<(), StateError> {
    let ids = |values: &[TutorialId], name: &str| {
        if values.iter().any(|value| value.id <= 0) {
            Err(StateError::TutorialRules(format!("invalid {name} closure")))
        } else {
            Ok(())
        }
    };
    ids(&rules.stages, "stage")?;
    ids(&rules.base_enemies, "base enemy")?;
    if rules.timeline_panels.iter().any(|value| value.id <= 0) {
        return Err(StateError::TutorialRules(
            "invalid timeline panel closure".into(),
        ));
    }
    for battle in &rules.battles {
        if battle.id <= 0
            || battle.wave_ids.is_empty()
            || battle.timeline_panel_ids.is_empty()
            || battle
                .wave_ids
                .iter()
                .any(|id| rule_wave(rules, *id).is_err())
            || battle
                .timeline_panel_ids
                .iter()
                .any(|id| !rules.timeline_panels.iter().any(|row| row.id == *id))
        {
            return Err(StateError::TutorialRules(format!(
                "invalid battle {} closure",
                battle.id
            )));
        }
    }
    for wave in &rules.waves {
        if wave.id <= 0
            || wave.stage_id <= 0
            || wave.field_effect_id.is_some_and(|id| id <= 0)
            || !rules.stages.iter().any(|stage| stage.id == wave.stage_id)
            || wave.enemies.is_empty()
            || wave
                .enemies
                .iter()
                .any(|enemy| enemy.level < 1 || rule_enemy(rules, enemy.id).is_err())
        {
            return Err(StateError::TutorialRules(format!(
                "invalid wave {} closure",
                wave.id
            )));
        }
    }
    for enemy in &rules.enemies {
        if enemy.id <= 0
            || enemy.base_enemy_id <= 0
            || !rules
                .base_enemies
                .iter()
                .any(|row| row.id == enemy.base_enemy_id)
            || rule_skill(rules, enemy.burst_skill_id).is_err()
            || enemy
                .extra_skill_ids
                .iter()
                .any(|id| rule_skill(rules, *id).is_err())
        {
            return Err(StateError::TutorialRules(format!(
                "invalid enemy {} closure",
                enemy.id
            )));
        }
    }
    for character in &rules.battle_characters {
        let rarity = rule_rarity(rules, character.initial_rarity)?;
        if character.id <= 0
            || rule_growth(rules, character.growth_id).is_err()
            || character.skills.is_empty()
            || character
                .skills
                .iter()
                .any(|skill| rule_skill(rules, skill.id).is_err())
            || character.ability_ids.len() > usize::try_from(rarity.ability_count).unwrap_or(0)
            || character
                .ability_ids
                .iter()
                .any(|id| !rules.abilities.iter().any(|row| row.id == *id))
            || character.leader_abilities.iter().any(|leader| {
                !rules
                    .abilities
                    .iter()
                    .any(|row| row.id == leader.ability_id)
                    || leader.target_character_ids.is_empty()
                    || leader
                        .target_character_ids
                        .iter()
                        .any(|id| !rules.battle_characters.iter().any(|row| row.id == *id))
            })
            || character.tag_ids.iter().any(|id| *id <= 0)
        {
            return Err(StateError::TutorialRules(format!(
                "invalid character {} closure",
                character.id
            )));
        }
    }
    for skill in &rules.skills {
        if skill.id <= 0
            || !(0..=3).contains(&skill.skill_type)
            || !(1..=4).contains(&skill.skill_effect_type)
            || !(1..=7).contains(&skill.skill_power_type)
            || !(-10_000..=10_000).contains(&skill.wait)
            || skill.power < 0
            || skill.break_power < 0
            || skill.break_power_type < 0
            || skill.attack_attributes.iter().any(|value| *value < 0)
            || skill.skill_target_type.is_some_and(|value| value < 0)
            || skill
                .effects
                .iter()
                .any(|effect| !rules.effects.iter().any(|row| row.id == effect.id))
        {
            return Err(StateError::TutorialRules(format!(
                "invalid skill {} closure",
                skill.id
            )));
        }
    }
    for ability in &rules.abilities {
        if ability.id <= 0
            || ability
                .effects
                .iter()
                .any(|effect| !rules.effects.iter().any(|row| row.id == effect.id))
        {
            return Err(StateError::TutorialRules(format!(
                "invalid ability {} closure",
                ability.id
            )));
        }
    }
    if rules
        .skills
        .iter()
        .flat_map(|skill| skill.effects.iter())
        .chain(
            rules
                .abilities
                .iter()
                .flat_map(|ability| ability.effects.iter()),
        )
        .any(|effect| effect.value == i32::MIN)
    {
        return Err(StateError::TutorialRules("invalid effect value".into()));
    }
    for effect in &rules.effects {
        if effect.id <= 0 || effect.field_effect_id.is_some_and(|id| id <= 0) {
            return Err(StateError::TutorialRules(format!(
                "invalid effect {} closure",
                effect.id
            )));
        }
    }
    for party in &rules.fixed_parties {
        if party.id <= 0
            || party.members.is_empty()
            || !party.members.iter().all(|member| {
                member.level > 0
                    && member.rarity > 0
                    && rule_character(rules, member.character_id).is_ok()
            })
            || !party
                .members
                .iter()
                .enumerate()
                .any(|(index, _)| i32::try_from(index + 1).ok() == Some(party.leader_position))
            || party
                .battle_tools
                .iter()
                .any(|tool| rule_tool(rules, tool.tool_id).is_err())
        {
            return Err(StateError::TutorialRules(format!(
                "invalid fixed party {} closure",
                party.id
            )));
        }
    }
    for tool in &rules.battle_tools {
        if tool.id <= 0
            || tool.skill_id <= 0
            || tool.usage_count < 0
            || tool.trait_filter_ids.is_empty()
            || tool.trait_filter_ids.iter().any(|id| *id <= 0)
        {
            return Err(StateError::TutorialRules(format!(
                "invalid battle tool {} closure",
                tool.id
            )));
        }
        rule_skill(rules, tool.skill_id)?;
    }
    for recipe in &rules.recipes {
        if recipe.id <= 0
            || recipe.mana_cost < 0
            || recipe.costs.is_empty()
            || recipe
                .costs
                .iter()
                .any(|cost| cost.resource_type <= 0 || cost.id <= 0 || cost.quantity <= 0)
            || recipe.target_reward.resource_type != 14
            || recipe.target_reward.id <= 0
            || recipe.target_reward.quantity != 1
            || recipe.support_character_ids.is_empty()
            || recipe.support_character_ids.iter().any(|id| *id <= 0)
            || recipe.trait_count <= 0
            || recipe.tutorial_grade < 0
        {
            return Err(StateError::TutorialRules(format!(
                "invalid recipe {} closure",
                recipe.id
            )));
        }
    }
    if rules.trait_ranks.is_empty()
        || rules
            .trait_ranks
            .iter()
            .any(|row| row.id <= 0 || row.weight == 0)
        || rules.trait_ranks.iter().map(|row| row.weight).sum::<u32>() != 10_000
        || rules.battle_tool_traits.is_empty()
        || rules.synthesis_characters.is_empty()
        || rules.synthesis_ingredients.is_empty()
    {
        return Err(StateError::TutorialRules(
            "invalid synthesis trait closure".into(),
        ));
    }
    for source in rules
        .synthesis_characters
        .iter()
        .chain(&rules.synthesis_ingredients)
    {
        if source.id <= 0
            || source.trait_ids.is_empty()
            || source
                .trait_ids
                .iter()
                .any(|id| rule_battle_tool_trait(rules, *id).is_err())
        {
            return Err(StateError::TutorialRules(format!(
                "invalid synthesis source {}",
                source.id
            )));
        }
    }
    for trait_rule in &rules.battle_tool_traits {
        if trait_rule.id <= 0
            || trait_rule.filter_ids.is_empty()
            || trait_rule.filter_ids.iter().any(|id| *id <= 0)
            || trait_rule
                .effects
                .iter()
                .any(|effect| effect.id <= 0 || effect.values.len() != rules.trait_ranks.len())
        {
            return Err(StateError::TutorialRules(format!(
                "invalid battle tool trait {}",
                trait_rule.id
            )));
        }
    }
    for gacha in &rules.gachas {
        if gacha.id <= 0
            || gacha.category <= 0
            || gacha.gacha_type <= 0
            || gacha.rate_set_id <= 0
            || gacha.priority < 0
            || gacha.start_at.is_some_and(|value| value <= 0)
            || gacha.end_at.is_some_and(|value| value <= 0)
            || gacha.button_ids.is_empty()
            || gacha
                .button_ids
                .iter()
                .any(|id| !rules.gacha_buttons.iter().any(|button| button.id == *id))
        {
            return Err(StateError::TutorialRules(format!(
                "invalid gacha {} closure",
                gacha.id
            )));
        }
    }
    for button in &rules.gacha_buttons {
        if button.id <= 0
            || button.draw_count <= 0
            || button.limit_count.is_some_and(|value| value <= 0)
            || button.medal_quantity < 0
            || button.additional_medal < 0
            || button
                .cost
                .as_ref()
                .is_some_and(|cost| cost.resource_type <= 0 || cost.id <= 0 || cost.quantity <= 0)
        {
            return Err(StateError::TutorialRules(format!(
                "invalid gacha button {} closure",
                button.id
            )));
        }
    }
    for rate in &rules.gacha_rates {
        if rate.id <= 0
            || rate.rate_set_id <= 0
            || rate.deck_id <= 0
            || rate.priority < 0
            || rate.total_basis_points <= 0
            || rate.cards(rules).is_empty()
            || !rules.gacha_decks.iter().any(|deck| deck.id == rate.deck_id)
            || !rules
                .gachas
                .iter()
                .any(|gacha| gacha.rate_set_id == rate.rate_set_id)
        {
            return Err(StateError::TutorialRules(format!(
                "invalid gacha rate {} closure",
                rate.id
            )));
        }
        let deck = rules
            .gacha_decks
            .iter()
            .find(|deck| deck.id == rate.deck_id)
            .ok_or_else(|| StateError::TutorialRules("missing gacha deck".into()))?;
        if rate.cards(rules).iter().any(|card| {
            card.id <= 0 || card.resource_type != deck.card_type || card.rarity != deck.rarity
        }) {
            return Err(StateError::TutorialRules(format!(
                "invalid gacha rate {} cards",
                rate.id
            )));
        }
    }
    for deck in &rules.gacha_decks {
        if deck.id <= 0 || deck.card_type <= 0 || deck.rarity <= 0 {
            return Err(StateError::TutorialRules(format!(
                "invalid gacha deck {} closure",
                deck.id
            )));
        }
    }
    Ok(())
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect()
}

pub(crate) fn empty_message(
    proto: &ProtoRegistry,
    name: &str,
) -> Result<DynamicMessage, StateError> {
    proto
        .empty(name)
        .map_err(|error| StateError::Descriptor(error.to_string()))
}

pub(crate) fn timestamp(proto: &ProtoRegistry, seconds: i64) -> Result<DynamicMessage, StateError> {
    let mut timestamp = empty_message(proto, "google.protobuf.Timestamp")?;
    timestamp.set_field_by_name("seconds", Value::I64(seconds));
    Ok(timestamp)
}

pub(crate) fn int32_value(proto: &ProtoRegistry, value: i32) -> Result<DynamicMessage, StateError> {
    let mut wrapper = empty_message(proto, "google.protobuf.Int32Value")?;
    wrapper.set_field_by_name("value", Value::I32(value));
    Ok(wrapper)
}

pub(crate) fn starter_resources(
    proto: &ProtoRegistry,
    rules: &FreshStateRules,
) -> Result<DynamicMessage, StateError> {
    let now = unix_now();
    let mut resources = empty_message(proto, "blend.model.Resources")?;
    resources.set_field_by_name(
        "wallet",
        Value::Message(empty_message(proto, "blend.model.Wallet")?),
    );
    upsert_character(
        &mut resources,
        character_message(
            proto,
            i64::from(rules.initial_character.id),
            None,
            Some(now),
            rules.initial_character.rarity,
            rules.constants.initial_character_level_limit,
        )?,
        false,
    );

    let item_rewards = rules
        .initial_rewards
        .iter()
        .filter(|reward| reward.resource_type == 5);
    let mut items = Vec::new();
    for reward in item_rewards {
        let mut item = empty_message(proto, "blend.model.Item")?;
        item.set_field_by_name("item_id", Value::I32(reward.id));
        item.set_field_by_name("quantity", Value::I32(reward.quantity));
        item.set_field_by_name("total_quantity", Value::I32(reward.quantity));
        items.push(Value::Message(item));
    }
    resources.set_field_by_name("items", Value::List(items));

    let mut notifications = empty_message(proto, "blend.model.Notifications")?;
    notifications.set_field_by_name(
        "multi_mission",
        Value::Message(empty_message(
            proto,
            "blend.model.MultiMissionNotification",
        )?),
    );
    let mut mail = empty_message(proto, "google.protobuf.BoolValue")?;
    mail.set_field_by_name("value", Value::Bool(true));
    notifications.set_field_by_name("mail", Value::Message(mail));
    let mut news = empty_message(proto, "blend.model.NewsNotification")?;
    news.set_field_by_name("updated_at", Value::Message(timestamp(proto, now)?));
    news.set_field_by_name(
        "important_updated_at",
        Value::Message(timestamp(proto, now)?),
    );
    notifications.set_field_by_name("news", Value::Message(news));
    let mut gacha = empty_message(proto, "blend.model.GachaNotification")?;
    gacha.set_field_by_name(
        "gacha_category",
        Value::I32(rules.protocol_defaults.gacha_category),
    );
    gacha.set_field_by_name("is_daily_or_free_gacha_remaining", Value::Bool(true));
    gacha.set_field_by_name(
        "latest_gacha_start_at",
        Value::Message(timestamp(proto, now)?),
    );
    gacha.set_field_by_name(
        "executable_gacha_ids",
        Value::List(vec![Value::I32(rules.protocol_defaults.gacha_id)]),
    );
    let mut gacha_category_2 = empty_message(proto, "blend.model.GachaNotification")?;
    gacha_category_2.set_field_by_name(
        "gacha_category",
        Value::I32(rules.protocol_defaults.gacha_category_2),
    );
    let mut gacha_category_3 = empty_message(proto, "blend.model.GachaNotification")?;
    gacha_category_3.set_field_by_name(
        "gacha_category",
        Value::I32(rules.protocol_defaults.gacha_category_3),
    );
    notifications.set_field_by_name(
        "gachas",
        Value::List(vec![
            Value::Message(gacha),
            Value::Message(gacha_category_2),
            Value::Message(gacha_category_3),
        ]),
    );
    resources.set_field_by_name("notifications", Value::Message(notifications));

    let mut status = empty_message(proto, "blend.model.Status")?;
    status.set_field_by_name("exp", Value::I32(rules.rank_one.exp));
    status.set_field_by_name("rank", Value::I32(rules.rank_one.id));
    let cole = rules
        .initial_rewards
        .iter()
        .find(|reward| reward.resource_type == 3)
        .map(|reward| reward.quantity)
        .ok_or_else(|| StateError::FreshRules("missing initial Cole reward".into()))?;
    status.set_field_by_name("cole", Value::I32(cole));
    status.set_field_by_name(
        "mana_when_updated",
        Value::I32(rules.constants.mana_recovery_limit),
    );
    status.set_field_by_name(
        "expedition_max_count",
        Value::I32(rules.protocol_defaults.expedition_max_count),
    );
    status.set_field_by_name("stamina_when_updated", Value::I32(rules.rank_one.stamina));
    status.set_field_by_name("stamina_updated_at", Value::Message(timestamp(proto, now)?));
    status.set_field_by_name("mana_updated_at", Value::Message(timestamp(proto, now)?));
    status.set_field_by_name(
        "dishes_when_updated",
        Value::I32(rules.protocol_defaults.dishes_when_updated),
    );
    status.set_field_by_name("dishes_updated_at", Value::Message(timestamp(proto, now)?));
    status.set_field_by_name(
        "party_max_battle_tool_count",
        Value::I32(rules.constants.initial_party_max_battle_tool_count),
    );
    status.set_field_by_name(
        "synthesis_rental_count_updated_at",
        Value::Message(timestamp(proto, now)?),
    );
    status.set_field_by_name(
        "tutorial_step",
        Value::I32(rules.protocol_defaults.tutorial_step),
    );
    resources.set_field_by_name("status", Value::Message(status));

    let mut party = empty_message(proto, "blend.model.Party")?;
    party.set_field_by_name("number", Value::I32(rules.protocol_defaults.party_number));
    party.set_field_by_name("party_type", Value::I32(rules.protocol_defaults.party_type));
    party.set_field_by_name(
        "battle_tool_entity_ids",
        Value::List(vec![Value::I32(rules.protocol_defaults.entity_id)]),
    );
    party.set_field_by_name(
        "leader_position",
        Value::I32(rules.protocol_defaults.leader_position),
    );
    resources.set_field_by_name("parties", Value::List(vec![Value::Message(party)]));

    let mut tool = empty_message(proto, "blend.model.BattleTool")?;
    tool.set_field_by_name("entity_id", Value::I32(rules.protocol_defaults.entity_id));
    tool.set_field_by_name("tool_id", Value::I32(rules.initial_battle_tool_id));
    tool.set_field_by_name("received_at", Value::Message(timestamp(proto, now)?));
    resources.set_field_by_name("battle_tools", Value::List(vec![Value::Message(tool)]));

    let mut recipes = Vec::with_capacity(rules.initial_recipe_ids.len());
    for &recipe_id in &rules.initial_recipe_ids {
        let mut recipe = empty_message(proto, "blend.model.Recipe")?;
        recipe.set_field_by_name("recipe_id", Value::I32(recipe_id));
        recipe.set_field_by_name("received_at", Value::Message(timestamp(proto, now)?));
        recipes.push(Value::Message(recipe));
    }
    resources.set_field_by_name("recipes", Value::List(recipes));

    let mut party_members =
        Vec::with_capacity(rules.protocol_defaults.party_member_positions as usize);
    for position in 1..=rules.protocol_defaults.party_member_positions {
        let mut member = empty_message(proto, "blend.model.PartyMember")?;
        member.set_field_by_name("party_type", Value::I32(1));
        member.set_field_by_name("number", Value::I32(1));
        member.set_field_by_name("position", Value::I32(position));
        if position == 1 {
            member.set_field_by_name(
                "character_id",
                Value::Message(int32_value(proto, rules.initial_character.id)?),
            );
        }
        party_members.push(Value::Message(member));
    }
    resources.set_field_by_name("party_members", Value::List(party_members));

    let mut task_counts = Vec::with_capacity(rules.task_condition_ids.len());
    for &condition_id in &rules.task_condition_ids {
        let count = rules
            .proven_task_counts
            .iter()
            .find(|rule| rule.condition_id == condition_id)
            .map(|rule| rule.count)
            .unwrap_or_default();
        let mut task = empty_message(proto, "blend.model.TotalTaskCount")?;
        task.set_field_by_name("condition_id", Value::I32(condition_id));
        task.set_field_by_name("count", Value::I32(count));
        task_counts.push(Value::Message(task));
    }
    resources.set_field_by_name("total_task_counts", Value::List(task_counts));

    let mut profile = empty_message(proto, "blend.model.Profile")?;
    profile.set_field_by_name(
        "name",
        Value::String(rules.protocol_defaults.profile_name.clone()),
    );
    profile.set_field_by_name(
        "memo",
        Value::String(rules.protocol_defaults.profile_memo.clone()),
    );
    profile.set_field_by_name(
        "favorite_character_id",
        Value::I32(rules.constants.profile_initial_favorite_character_id),
    );
    profile.set_field_by_name(
        "favorite_party_character1_id",
        Value::Message(int32_value(proto, rules.initial_character.id)?),
    );
    profile.set_field_by_name(
        "selected_home_id",
        Value::Message(empty_message(proto, "google.protobuf.Int32Value")?),
    );
    resources.set_field_by_name("profile", Value::Message(profile));
    Ok(resources)
}
