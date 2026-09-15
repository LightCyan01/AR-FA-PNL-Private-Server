use super::prelude::*;

// Battle domain: typed battle mutations, timeline/effects, and deterministic policy helpers.
//
// This module owns combat state transitions; protocol/storage adapters remain in the parent facade.

pub(crate) struct BattleStartMutation {
    pub(crate) state: DynamicMessage,
    pub(crate) response: DynamicMessage,
    pub(crate) start_txid: String,
    pub(crate) quest_id: i32,
    pub(crate) battle_id: i32,
    pub(crate) effects: effects::Runtime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BattleStartMode {
    Gacha,
    Standard,
    Rental,
    SoloRaid,
    Total,
}

pub(crate) struct BattleAttackMutation {
    pub(crate) state: DynamicMessage,
    pub(crate) response: DynamicMessage,
}

pub(crate) struct BattleFinishMutation {
    pub(crate) resources: DynamicMessage,
    pub(crate) response: DynamicMessage,
    pub(crate) character_indexes: Vec<(i64, Option<i64>)>,
}

pub(crate) fn battle_resume_response(
    proto: &ProtoRegistry,
    active: &ActiveBattle,
) -> Result<DynamicMessage, StateError> {
    let state = proto
        .decode("blend.model.BattleState", &active.state_blob)
        .map_err(|error| StateError::Descriptor(error.to_string()))?;
    let mut start = empty_message(proto, "blend.model.BattleStart")?;
    start.set_field_by_name("state", Value::Message(state.clone()));
    let mut history = empty_message(proto, "blend.model.BattleHistory")?;
    history.set_field_by_name("status", Value::EnumNumber(current_battle_status(&state)?));
    history.set_field_by_name("start", Value::Message(start));
    history.set_field_by_name("previous_state", Value::Message(state));
    let mut context = empty_message(proto, "blend.model.BattleContext")?;
    context.set_field_by_name("quest_id", Value::I32(active.quest_id as i32));
    context.set_field_by_name("start_txid", Value::String(active.start_txid.clone()));
    context.set_field_by_name("party_number", Value::Message(int32_value(proto, 1)?));
    let mut response = empty_message(proto, "blend.api.BattleResumeResponse")?;
    response.set_field_by_name("history", Value::Message(history));
    response.set_field_by_name("context", Value::Message(context));
    response.set_field_by_name(
        "changed_resources",
        Value::Message(empty_message(proto, "blend.model.Resources")?),
    );
    Ok(response)
}

pub(crate) fn battle_storage_error(error: StateError) -> StorageError {
    match error {
        StateError::InvalidRequest | StateError::OutOfSchedule => StorageError::BattleRejected,
        other => StorageError::Battle(other.to_string()),
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BattleStats {
    pub(crate) hp: i32,
    pub(crate) speed: i32,
    pub(crate) attack: i32,
    pub(crate) magic: i32,
    pub(crate) defense: i32,
    pub(crate) mental: i32,
}

pub(crate) fn character_stats(
    rules: &TutorialRules,
    character_id: i32,
    level: i32,
    rarity_id: i32,
) -> Result<BattleStats, StateError> {
    if level < 1 {
        return Err(StateError::InvalidRequest);
    }
    let character = rule_character(rules, character_id)?;
    let growth = rule_growth(rules, character.growth_id)?;
    let rarity = rule_rarity(rules, rarity_id)?;
    let value = |name: &str| -> Result<i32, StateError> {
        let initial = i64::from(map_value(&character.initial_status, name)?);
        let growth = i64::from(map_value(&growth.level_coefficients, name)?);
        let coefficient = i64::from(map_value(&rarity.status_coefficients, name)?);
        checked_i32(((growth * i64::from(level - 1)) + initial * 100) * coefficient / 10_000)
    };
    Ok(BattleStats {
        hp: value("hp")?,
        speed: value("speed")?,
        attack: value("attack")?,
        magic: value("magic")?,
        defense: value("defense")?,
        mental: value("mental")?,
    })
}

pub(crate) fn tutorial_ally_stats(
    rules: &TutorialRules,
    character_id: i32,
    level: i32,
    rarity_id: i32,
    memoria_id: Option<i32>,
) -> Result<BattleStats, StateError> {
    let mut stats = character_stats(rules, character_id, level, rarity_id)?;
    if let Some(memoria_id) = memoria_id {
        let memoria = rules
            .reward_memorias
            .iter()
            .find(|row| row.id == memoria_id)
            .ok_or(StateError::InvalidRequest)?;
        for buff in &memoria.status_buffs {
            let basis_points = buff.initial_values.first().copied().unwrap_or(0)
                + buff.growth_values.first().copied().unwrap_or(0);
            let stat = match buff.stat_type {
                1 => &mut stats.hp,
                2 => &mut stats.speed,
                3 => &mut stats.attack,
                4 => &mut stats.magic,
                5 => &mut stats.defense,
                6 => &mut stats.mental,
                _ => return Err(StateError::InvalidRequest),
            };
            *stat = checked_i32(i64::from(*stat) * i64::from(10_000 + basis_points) / 10_000)?;
        }
    }
    Ok(stats)
}

pub(crate) fn enemy_stats(
    enemy: &TutorialEnemy,
    level: i32,
) -> Result<(BattleStats, i32), StateError> {
    if level < 1
        || !enemy.break_gauge_coefficient.is_finite()
        || enemy.break_gauge_coefficient < 0.0
    {
        return Err(StateError::InvalidRequest);
    }
    // Enemy status is a server-owned master-data rule: base + the integer
    // level-growth delta.  The client only consumes the resulting status.
    let value = |name: &str| -> Result<i32, StateError> {
        let base = i64::from(map_value(&enemy.status, name)?);
        let growth = i64::from(map_value(&enemy.status_growth, name)?);
        checked_i32(base + growth * i64::from(level - 1) / 100)
    };
    let raw_break = value("break_gauge")?;
    let scaled_break = f64::from(raw_break) * enemy.break_gauge_coefficient;
    if !scaled_break.is_finite() || scaled_break < 0.0 || scaled_break > f64::from(i32::MAX) {
        return Err(StateError::TutorialRules(
            "enemy break gauge overflows int32".into(),
        ));
    }
    let break_gauge = checked_i32(scaled_break.floor() as i64)?;
    Ok((
        BattleStats {
            hp: value("hp")?,
            speed: value("speed")?,
            attack: value("attack")?,
            magic: value("attack")?,
            defense: value("defense")?,
            mental: value("defense")?,
        },
        break_gauge.max(1),
    ))
}

pub(crate) fn tutorial_enemy_stats(
    enemy: &TutorialEnemy,
    wave_enemy: &TutorialWaveEnemy,
    wave_id: i32,
) -> Result<(BattleStats, i32, i32), StateError> {
    let (mut stats, base_break) = enemy_stats(enemy, wave_enemy.level)?;
    let (break_gauge, break_wait) = match wave_id {
        100002791..=100002793 => {
            stats.hp /= 100;
            (15_468, 220)
        }
        100002801 => {
            stats.hp /= 10;
            (9_770, 220)
        }
        100000031 => (if wave_enemy.id == 80003006 { 7 } else { 8 }, 220),
        100000041 => {
            if wave_enemy.id == 80003007 {
                stats.hp = stats.hp * 4 / 5;
                (46, 350)
            } else {
                (30, 350)
            }
        }
        _ => return Ok((stats, base_break, 220)),
    };
    stats.attack = 0;
    stats.magic = 0;
    stats.defense = 0;
    stats.mental = 0;
    Ok((stats, break_gauge, break_wait))
}

pub(crate) fn battle_status_message(
    proto: &ProtoRegistry,
    stats: BattleStats,
) -> Result<DynamicMessage, StateError> {
    let mut status = empty_message(proto, "blend.model.BattleCharacterStatus")?;
    for (field, value) in [
        ("speed", stats.speed),
        ("attack", stats.attack),
        ("magic", stats.magic),
        ("defense", stats.defense),
        ("mental", stats.mental),
    ] {
        status.set_field_by_name(field, Value::I32(value));
    }
    Ok(status)
}

pub(crate) fn battle_resistance_message(
    proto: &ProtoRegistry,
    resistance: &BTreeMap<String, i32>,
) -> Result<DynamicMessage, StateError> {
    let mut message = empty_message(proto, "blend.model.BattleResistance")?;
    for field in [
        "slashing",
        "impact",
        "piercing",
        "fire",
        "ice",
        "lightning",
        "wind",
    ] {
        message.set_field_by_name(field, Value::I32(map_value(resistance, field)?));
    }
    Ok(message)
}

pub(crate) fn set_battle_field_effect(
    proto: &ProtoRegistry,
    state: &mut DynamicMessage,
    field_effect_id: Option<i32>,
) -> Result<(), StateError> {
    let Some(field_effect_id) = field_effect_id else {
        state.clear_field_by_name("field_effect");
        return Ok(());
    };
    let mut field_effect = empty_message(proto, "blend.model.BattleFieldEffect")?;
    field_effect.set_field_by_name("field_effect_id", Value::I32(field_effect_id));
    state.set_field_by_name("field_effect", Value::Message(field_effect));
    Ok(())
}

pub(crate) fn resource_character(
    resources: &DynamicMessage,
    character_id: i32,
) -> Result<DynamicMessage, StateError> {
    message_list(resources, "characters")
        .into_iter()
        .find(|character| i32_field(character, "character_id") == Some(character_id))
        .ok_or(StateError::InvalidRequest)
}

pub(crate) fn level_for_exp(rules: &TutorialRules, exp: i32) -> Result<i32, StateError> {
    if exp < 0 {
        return Err(StateError::InvalidRequest);
    }
    rules
        .character_levels
        .iter()
        .filter(|row| row.exp <= exp)
        .max_by_key(|row| row.level)
        .map(|row| row.level)
        .ok_or_else(|| StateError::TutorialRules("character level table has no level 1".into()))
}

pub(crate) type BattleExternalPassive = (Option<i32>, i32, TutorialSkillEffect);

pub(crate) fn scaled_ability_effects(
    rules: &TutorialRules,
    ability_ids: &[i32],
    coefficient: i32,
    source_character_id: Option<i32>,
) -> Result<Vec<BattleExternalPassive>, StateError> {
    if coefficient < 0 {
        return Err(StateError::InvalidRequest);
    }
    let mut effects = Vec::new();
    for ability_id in ability_ids {
        let ability = rules
            .abilities
            .iter()
            .find(|ability| ability.id == *ability_id)
            .ok_or(StateError::InvalidRequest)?;
        for effect in &ability.effects {
            effects.push((
                source_character_id,
                *ability_id,
                TutorialSkillEffect {
                    id: effect.id,
                    value: checked_i32(i64::from(effect.value) * i64::from(coefficient) / 10_000)?,
                },
            ));
        }
    }
    Ok(effects)
}

fn main_memoria_coefficient(
    rules: &atelier::AtelierRules,
    main: &atelier::MemoriaRule,
    sub_memorias: &[DynamicMessage],
) -> Result<i32, StateError> {
    let target_count = main.attack_attributes.len() + main.roles.len();
    let discount_index = target_count.saturating_sub(1).min(
        rules
            .constants
            .main_memoria_multi_discount_coef
            .len()
            .saturating_sub(1),
    );
    let discount = *rules
        .constants
        .main_memoria_multi_discount_coef
        .get(discount_index)
        .ok_or_else(|| StateError::TutorialRules("missing memoria discount coefficients".into()))?;
    let mut total = rules.constants.base_coef_for_main_memoria;
    for entity in sub_memorias {
        let sub = rules
            .memorias
            .iter()
            .find(|row| i32_field(entity, "memoria_id") == Some(row.id))
            .ok_or(StateError::InvalidRequest)?;
        let limit_break = usize::try_from(i32_field(entity, "limit_break").unwrap_or_default())
            .map_err(|_| StateError::InvalidRequest)?;
        let match_count = usize::from(
            main.attack_attributes
                .iter()
                .any(|value| sub.attack_attributes.contains(value)),
        ) + usize::from(main.roles.iter().any(|value| sub.roles.contains(value)));
        let limit_break_bonus = *rules
            .constants
            .sub_memoria_limit_break_bonus_coef
            .get(limit_break)
            .ok_or(StateError::InvalidRequest)?;
        let match_bonus = *rules
            .constants
            .memoria_match_bonus_coef
            .get(match_count)
            .ok_or(StateError::InvalidRequest)?;
        let contribution = rules
            .constants
            .base_coef_for_main_memoria
            .checked_add(limit_break_bonus)
            .and_then(|value| value.checked_add(match_bonus))
            .ok_or(StateError::InvalidRequest)?
            .min(rules.constants.max_coef_per_one_memoria);
        total = total
            .checked_add(checked_i32(
                i64::from(contribution) * i64::from(discount) / 10_000,
            )?)
            .ok_or(StateError::InvalidRequest)?;
    }
    Ok(total)
}

pub(crate) fn resolve_battle_ship(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    atelier_rules: &atelier::AtelierRules,
    resources: &DynamicMessage,
    ship_id: Option<i32>,
) -> Result<
    (
        DynamicMessage,
        Vec<DynamicMessage>,
        Vec<BattleExternalPassive>,
    ),
    StateError,
> {
    let mut battle_ship = empty_message(proto, "blend.model.BattleShip")?;
    let Some(ship_id) = ship_id else {
        return Ok((battle_ship, Vec::new(), Vec::new()));
    };
    let ship = message_list(resources, "ships")
        .into_iter()
        .find(|row| i32_field(row, "ship_id") == Some(ship_id))
        .ok_or(StateError::InvalidRequest)?;
    let rank = i32_field(&ship, "rank")
        .filter(|value| *value > 0)
        .unwrap_or(1);
    let level = atelier::prelude::ship_level(
        atelier_rules,
        i32_field(&ship, "exp").unwrap_or_default(),
        rank,
    )?;
    let party_number = i32_field(&ship, "party_number")
        .filter(|value| *value > 0)
        .ok_or(StateError::InvalidRequest)?;
    let party = message_list(resources, "ship_parties")
        .into_iter()
        .find(|row| i32_field(row, "number") == Some(party_number))
        .ok_or(StateError::InvalidRequest)?;

    let character_ids = i32_list(&party, "character_ids");
    if character_ids.len() > usize::try_from(level.max_character_count).unwrap_or(0)
        || character_ids
            .iter()
            .enumerate()
            .any(|(index, id)| character_ids[..index].contains(id))
    {
        return Err(StateError::InvalidRequest);
    }
    let mut members = Vec::new();
    let mut support_ability_ids = Vec::new();
    let mut passives = scaled_ability_effects(rules, &level.ability_ids, 10_000, None)?;
    for character_id in character_ids {
        let character = resource_character(resources, character_id)?;
        let rarity = i32_field(&character, "rarity")
            .filter(|value| *value > 0)
            .ok_or(StateError::InvalidRequest)?;
        let character_rule = rule_character(rules, character_id)?;
        let support_ability_id = *character_rule
            .support_ability_ids
            .get(usize::try_from(rarity - 1).map_err(|_| StateError::InvalidRequest)?)
            .ok_or(StateError::InvalidRequest)?;
        support_ability_ids.push(support_ability_id);
        passives.extend(scaled_ability_effects(
            rules,
            std::slice::from_ref(&support_ability_id),
            level.support_ability_apply_rate,
            Some(character_id),
        )?);
        let mut member = empty_message(proto, "blend.model.BattleShipMember")?;
        member.set_field_by_name("character_id", Value::I32(character_id));
        member.set_field_by_name("rarity", Value::I32(rarity));
        members.push(Value::Message(member));
    }
    battle_ship.set_field_by_name("members", Value::List(members));
    battle_ship.set_field_by_name(
        "ship_support_ability_ids",
        Value::List(level.ability_ids.iter().copied().map(Value::I32).collect()),
    );
    battle_ship.set_field_by_name(
        "support_ability_ids",
        Value::List(support_ability_ids.into_iter().map(Value::I32).collect()),
    );
    battle_ship.set_field_by_name(
        "support_ability_value_coef",
        Value::Message(wrapper_i32(proto, level.support_ability_apply_rate)?),
    );

    let sub_ids = i32_list(&party, "sub_memoria_entity_ids");
    if sub_ids.len() > usize::try_from(level.max_sub_memoria_count).unwrap_or(0)
        || sub_ids
            .iter()
            .enumerate()
            .any(|(index, id)| sub_ids[..index].contains(id))
    {
        return Err(StateError::InvalidRequest);
    }
    let memorias = message_list(resources, "memorias");
    let sub_memorias = sub_ids
        .iter()
        .map(|entity_id| {
            memorias
                .iter()
                .find(|row| i32_field(row, "entity_id") == Some(*entity_id))
                .cloned()
                .ok_or(StateError::InvalidRequest)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(entity_id) = optional_i32_field(&party, "main_memoria_entity_id") {
        if sub_ids.contains(&entity_id) {
            return Err(StateError::InvalidRequest);
        }
        let entity = memorias
            .iter()
            .find(|row| i32_field(row, "entity_id") == Some(entity_id))
            .ok_or(StateError::InvalidRequest)?;
        let memoria = atelier_rules
            .memorias
            .iter()
            .find(|row| i32_field(entity, "memoria_id") == Some(row.id))
            .ok_or(StateError::InvalidRequest)?;
        let limit_break = usize::try_from(i32_field(entity, "limit_break").unwrap_or_default())
            .map_err(|_| StateError::InvalidRequest)?;
        let ability_id = *memoria
            .ability_ids
            .get(limit_break)
            .ok_or(StateError::InvalidRequest)?;
        let coefficient = main_memoria_coefficient(atelier_rules, memoria, &sub_memorias)?;
        battle_ship.set_field_by_name(
            "main_memoria_ability_ids",
            Value::List(vec![Value::I32(ability_id)]),
        );
        battle_ship.set_field_by_name(
            "main_memoria_value_coef",
            Value::Message(wrapper_i32(proto, coefficient)?),
        );
        passives.extend(scaled_ability_effects(
            rules,
            std::slice::from_ref(&ability_id),
            coefficient,
            None,
        )?);
    }

    let tool_ids = i32_list(&party, "ship_tool_entity_ids");
    if tool_ids.len() > usize::try_from(level.max_tool_count).unwrap_or(0)
        || tool_ids
            .iter()
            .enumerate()
            .any(|(index, id)| tool_ids[..index].contains(id))
    {
        return Err(StateError::InvalidRequest);
    }
    let owned_tools = message_list(resources, "ship_tools");
    let mut tools = Vec::new();
    for (index, entity_id) in tool_ids.into_iter().enumerate() {
        let entity = owned_tools
            .iter()
            .find(|row| i32_field(row, "entity_id") == Some(entity_id))
            .ok_or(StateError::InvalidRequest)?;
        let tool_id = i32_field(entity, "tool_id").ok_or(StateError::InvalidRequest)?;
        let tool_rule = rules
            .ship_tools
            .iter()
            .find(|row| row.id == tool_id)
            .ok_or(StateError::InvalidRequest)?;
        let rank = i32_field(entity, "rank")
            .filter(|value| *value > 0)
            .unwrap_or(1);
        let tool_level = atelier_rules
            .ship_tool_levels
            .iter()
            .filter(|row| {
                row.rank <= rank && row.exp <= i32_field(entity, "exp").unwrap_or_default()
            })
            .max_by_key(|row| row.level)
            .ok_or(StateError::InvalidRequest)?;
        let level_index =
            usize::try_from(tool_level.level - 1).map_err(|_| StateError::InvalidRequest)?;
        let mut tool = empty_message(proto, "blend.model.BattleShipTool")?;
        tool.set_field_by_name(
            "number",
            Value::I32(i32::try_from(index + 1).map_err(|_| StateError::InvalidRequest)?),
        );
        tool.set_field_by_name("tool_id", Value::I32(tool_id));
        tool.set_field_by_name(
            "skill_id",
            Value::I32(
                *tool_rule
                    .skill_ids
                    .get(level_index)
                    .ok_or(StateError::InvalidRequest)?,
            ),
        );
        tool.set_field_by_name(
            "usage_count",
            Value::I32(
                *tool_rule
                    .usage_counts
                    .get(level_index)
                    .ok_or(StateError::InvalidRequest)?,
            ),
        );
        tools.push(tool);
    }
    Ok((battle_ship, tools, passives))
}

pub(crate) fn resolve_account_party(
    rules: &TutorialRules,
    atelier_rules: &atelier::AtelierRules,
    character_rules: &CharacterRules,
    resources: &DynamicMessage,
    party_number: i32,
) -> Result<(Vec<BattlePartyMember>, Vec<BattlePartyTool>), StateError> {
    let parties = message_list(resources, "parties");
    let matches: Vec<_> = parties
        .into_iter()
        .filter(|party| {
            i32_field(party, "party_type") == Some(1)
                && i32_field(party, "number") == Some(party_number)
        })
        .collect();
    if matches.len() != 1 {
        return Err(StateError::InvalidRequest);
    }
    let party = &matches[0];
    let leader_position = i32_field(party, "leader_position").unwrap_or_default();
    if !(1..=5).contains(&leader_position) {
        return Err(StateError::InvalidRequest);
    }
    let mut members = Vec::new();
    for row in message_list(resources, "party_members")
        .into_iter()
        .filter(|row| {
            i32_field(row, "party_type") == Some(1)
                && i32_field(row, "number") == Some(party_number)
        })
    {
        let Some(character_id) = optional_i32_field(&row, "character_id") else {
            continue;
        };
        if character_id <= 0 || !(1..=5).contains(&i32_field(&row, "position").unwrap_or_default())
        {
            return Err(StateError::InvalidRequest);
        }
        let position = i32_field(&row, "position").unwrap_or_default();
        if members
            .iter()
            .any(|member: &BattlePartyMember| member.position == position)
        {
            return Err(StateError::InvalidRequest);
        }
        let character = resource_character(resources, character_id)?;
        let rarity = i32_field(&character, "rarity").ok_or(StateError::InvalidRequest)?;
        let exp = i32_field(&character, "exp").unwrap_or_default();
        let level = level_for_exp(rules, exp)?;
        rule_character(rules, character_id)?;
        let memoria_id = optional_i32_field(&row, "memoria_entity_id")
            .or_else(|| optional_i32_field(&character, "memoria_entity_id"))
            .map(|entity_id| {
                let memoria = message_list(resources, "memorias")
                    .into_iter()
                    .find(|memoria| i32_field(memoria, "entity_id") == Some(entity_id))
                    .ok_or(StateError::InvalidRequest)?;
                let memoria_id =
                    i32_field(&memoria, "memoria_id").ok_or(StateError::InvalidRequest)?;
                Ok::<i32, StateError>(memoria_id)
            })
            .transpose()?;
        let role = character_rules
            .characters
            .iter()
            .find(|rule| rule.id == character_id)
            .map(|rule| rule.role)
            .ok_or(StateError::InvalidRequest)?;
        let (integrated_stats, mut passives) = atelier::combat_stats(
            atelier_rules,
            resources,
            &character,
            &row,
            role,
            character_stats(rules, character_id, level, rarity)?,
        )?;
        passives.extend(character_passives(
            rules,
            character_id,
            Some(&character),
            rarity,
        )?);
        members.push(BattlePartyMember {
            character_id,
            level,
            rarity,
            memoria_id,
            position,
            is_leader: position == leader_position,
            integrated_stats: Some(integrated_stats),
            damage_bonus: 0,
            skills: selected_character_skills(rules, character_id, Some(&character), rarity)?,
            ability_ids: character_ability_ids(rules, character_id, Some(&character), rarity)?,
            passives,
            leader_passives: Vec::new(),
        });
    }
    if members.is_empty() || members.len() > 5 || !members.iter().any(|member| member.is_leader) {
        return Err(StateError::InvalidRequest);
    }
    members.sort_by_key(|member| member.position);

    let mut tools = Vec::new();
    let mut tool_entity_ids = Vec::new();
    for entity_id in i32_list(party, "battle_tool_entity_ids") {
        if tool_entity_ids.contains(&entity_id) {
            return Err(StateError::InvalidRequest);
        }
        tool_entity_ids.push(entity_id);
        let tool = message_list(resources, "battle_tools")
            .into_iter()
            .find(|tool| i32_field(tool, "entity_id") == Some(entity_id))
            .ok_or(StateError::InvalidRequest)?;
        let tool_id = i32_field(&tool, "tool_id").ok_or(StateError::InvalidRequest)?;
        let rule = rule_tool(rules, tool_id)?;
        let traits = message_list(&tool, "traits")
            .into_iter()
            .map(|value| {
                let trait_param = TutorialTraitParam {
                    id: i32_field(&value, "id").ok_or(StateError::InvalidRequest)?,
                    rank: i32_field(&value, "rank").ok_or(StateError::InvalidRequest)?,
                };
                rule_battle_tool_trait(rules, trait_param.id)?;
                if !rules
                    .trait_ranks
                    .iter()
                    .any(|rank| rank.id == trait_param.rank)
                {
                    return Err(StateError::InvalidRequest);
                }
                Ok(trait_param)
            })
            .collect::<Result<Vec<_>, StateError>>()?;
        tools.push(BattlePartyTool {
            tool_id,
            usage_count: rule.usage_count,
            traits,
        });
    }
    Ok((members, tools))
}

pub(crate) fn resolve_fixed_party_with_rules(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    atelier_rules: &atelier::AtelierRules,
    character_rules: &CharacterRules,
    fixed_party_id: i32,
) -> Result<(Vec<BattlePartyMember>, Vec<BattlePartyTool>), StateError> {
    let party = rules
        .fixed_parties
        .iter()
        .find(|party| party.id == fixed_party_id)
        .ok_or_else(|| {
            StateError::TutorialRules(format!("missing fixed party {fixed_party_id}"))
        })?;
    let mut members = Vec::new();
    for (index, member) in party.members.iter().enumerate() {
        if member.level < 1 || member.rarity < 1 {
            return Err(StateError::TutorialRules(
                "invalid fixed party member".into(),
            ));
        }
        rule_character(rules, member.character_id)?;
        let position = i32::try_from(index + 1).map_err(|_| StateError::InvalidRequest)?;
        let enhanced = member
            .is_max_enhance
            .then(|| {
                character::fixed_max_character(
                    proto,
                    character_rules,
                    member.character_id,
                    member.level,
                )
            })
            .transpose()?;
        let (level, rarity) = if let Some(character) = &enhanced {
            (
                level_for_exp(rules, i32_field(character, "exp").unwrap_or(0))?,
                i32_field(character, "rarity").ok_or(StateError::InvalidRequest)?,
            )
        } else {
            (member.level, member.rarity)
        };
        let integrated_stats = if let Some(character) = &enhanced {
            Some(
                atelier::combat_stats(
                    atelier_rules,
                    &empty_message(proto, "blend.model.Resources")?,
                    character,
                    &empty_message(proto, "blend.model.PartyMember")?,
                    0,
                    character_stats(rules, member.character_id, level, rarity)?,
                )?
                .0,
            )
        } else {
            None
        };
        members.push(BattlePartyMember {
            character_id: member.character_id,
            level,
            rarity,
            memoria_id: None,
            position,
            is_leader: position == party.leader_position,
            integrated_stats,
            damage_bonus: 0,
            skills: selected_character_skills(
                rules,
                member.character_id,
                enhanced.as_ref(),
                rarity,
            )?,
            ability_ids: character_ability_ids(
                rules,
                member.character_id,
                enhanced.as_ref(),
                rarity,
            )?,
            passives: character_passives(rules, member.character_id, enhanced.as_ref(), rarity)?,
            leader_passives: Vec::new(),
        });
    }
    if members.is_empty() || !members.iter().any(|member| member.is_leader) {
        return Err(StateError::InvalidRequest);
    }
    let tools = party
        .battle_tools
        .iter()
        .map(|tool| {
            let rule = rule_tool(rules, tool.tool_id)?;
            Ok(BattlePartyTool {
                tool_id: tool.tool_id,
                usage_count: rule.usage_count,
                traits: Vec::new(),
            })
        })
        .collect::<Result<Vec<_>, StateError>>()?;
    Ok((members, tools))
}

pub(crate) fn apply_leader_passives(
    rules: &TutorialRules,
    party: &mut [BattlePartyMember],
) -> Result<(), StateError> {
    let leader_id = party
        .iter()
        .find(|member| member.is_leader)
        .map(|member| member.character_id)
        .ok_or(StateError::InvalidRequest)?;
    for leader in &rule_character(rules, leader_id)?.leader_abilities {
        let effects = &rules
            .abilities
            .iter()
            .find(|ability| ability.id == leader.ability_id)
            .ok_or(StateError::InvalidRequest)?
            .effects;
        let effects = effects
            .iter()
            .map(|effect| {
                let multiplier =
                    effects::rule_for(effect.id, "passive", "ability", leader.ability_id)?
                        .and_then(|rule| rule.condition.get("party_tag_id"))
                        .map(|tag_id| {
                            party
                                .iter()
                                .filter(|member| {
                                    rule_character(rules, member.character_id)
                                        .is_ok_and(|character| character.tag_ids.contains(tag_id))
                                })
                                .count()
                        })
                        .unwrap_or(1);
                Ok(BattlePassiveEffect {
                    ability_id: leader.ability_id,
                    effect: TutorialSkillEffect {
                        id: effect.id,
                        value: effect
                            .value
                            .checked_mul(
                                i32::try_from(multiplier)
                                    .map_err(|_| StateError::InvalidRequest)?,
                            )
                            .ok_or(StateError::InvalidRequest)?,
                    },
                })
            })
            .collect::<Result<Vec<_>, StateError>>()?;
        for member in party
            .iter_mut()
            .filter(|member| leader.target_character_ids.contains(&member.character_id))
        {
            member.leader_passives.extend(effects.iter().cloned());
        }
    }
    Ok(())
}

/// Compatibility wrapper for isolated fixtures that do not own the parsed
/// catalog context yet.
#[cfg(test)]
pub(crate) fn resolve_fixed_party(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    fixed_party_id: i32,
) -> Result<(Vec<BattlePartyMember>, Vec<BattlePartyTool>), StateError> {
    let atelier_rules = atelier::load_rules()?;
    let character_rules = load_character_rules()?;
    resolve_fixed_party_with_rules(
        proto,
        rules,
        &atelier_rules,
        character_rules,
        fixed_party_id,
    )
}

pub(crate) fn rule_skill(rules: &TutorialRules, id: i32) -> Result<&TutorialSkill, StateError> {
    rules
        .skills
        .iter()
        .find(|skill| skill.id == id)
        .ok_or_else(|| StateError::TutorialRules(format!("missing skill {id}")))
}

pub(crate) fn selected_character_skills(
    rules: &TutorialRules,
    character_id: i32,
    owned: Option<&DynamicMessage>,
    rarity: i32,
) -> Result<Vec<TutorialCharacterSkill>, StateError> {
    let mut result = Vec::new();
    for definition in &rule_character(rules, character_id)?.skills {
        let prefix = match definition.skill_type {
            1 => "normal1",
            2 => "normal2",
            3 => "burst",
            _ => return Err(StateError::InvalidRequest),
        };
        if owned.is_some_and(|c| bool_field(c, &format!("is_{prefix}_skill_locked"))) {
            continue;
        }
        let evolved = owned.is_some_and(|c| bool_field(c, &format!("is_{prefix}_skill_evolved")));
        let ranks = if evolved {
            &definition.evolved_ids
        } else {
            &definition.rank_ids
        };
        let rank = if definition.skill_type == 3 {
            rarity
        } else {
            owned
                .and_then(|c| i32_field(c, &format!("{prefix}_skill_rank")))
                .unwrap_or(1)
                .max(1)
        };
        let mut selected = definition.clone();
        if !ranks.is_empty() {
            let index = usize::try_from(rank - 1).map_err(|_| StateError::InvalidRequest)?;
            selected.id = *ranks.get(index).ok_or(StateError::InvalidRequest)?;
        } else if evolved {
            return Err(StateError::InvalidRequest);
        }
        rule_skill(rules, selected.id)?;
        result.push(selected);
    }
    if result.is_empty() {
        return Err(StateError::InvalidRequest);
    }
    Ok(result)
}

pub(crate) fn character_passives(
    rules: &TutorialRules,
    character_id: i32,
    owned: Option<&DynamicMessage>,
    rarity: i32,
) -> Result<Vec<BattlePassiveEffect>, StateError> {
    let ids = character_ability_ids(rules, character_id, owned, rarity)?;
    let mut effects = Vec::new();
    for id in ids {
        let ability = rules
            .abilities
            .iter()
            .find(|a| a.id == id)
            .ok_or(StateError::InvalidRequest)?;
        effects.extend(
            ability
                .effects
                .iter()
                .cloned()
                .map(|effect| BattlePassiveEffect {
                    ability_id: id,
                    effect,
                }),
        );
    }
    Ok(effects)
}

pub(crate) fn character_ability_ids(
    rules: &TutorialRules,
    character_id: i32,
    owned: Option<&DynamicMessage>,
    rarity: i32,
) -> Result<Vec<i32>, StateError> {
    let character = rule_character(rules, character_id)?;
    let count = rule_rarity(rules, rarity)?.ability_count.max(0) as usize;
    let mut ids: std::collections::BTreeSet<i32> =
        character.ability_ids.iter().take(count).copied().collect();
    if owned.is_some_and(|c| {
        ["normal1", "normal2", "burst"]
            .iter()
            .all(|p| bool_field(c, &format!("is_{p}_skill_evolved")))
    }) {
        ids.extend(&character.evolved_ability_ids);
    }
    if let Some(owned) = owned {
        for (index, choices) in character.board_ability_ids.iter().enumerate() {
            let rank = i32_field(owned, &format!("board_ability{}_rank", index + 1)).unwrap_or(0);
            if rank > 0 {
                ids.insert(
                    *choices
                        .get((rank - 1) as usize)
                        .ok_or(StateError::InvalidRequest)?,
                );
            }
        }
    }
    Ok(ids.into_iter().collect())
}

/// The battle snapshot is authoritative after start, including on resume.
pub(crate) fn member_skills(
    member: &DynamicMessage,
) -> Result<Vec<TutorialCharacterSkill>, StateError> {
    message_list(&member_status(member, "ally")?, "skills")
        .iter()
        .map(|skill| {
            Ok(TutorialCharacterSkill {
                id: i32_field(skill, "skill_id").ok_or(StateError::InvalidRequest)?,
                skill_type: i32_field(skill, "skill_type").ok_or(StateError::InvalidRequest)?,
                rank_ids: Vec::new(),
                evolved_ids: Vec::new(),
            })
        })
        .collect()
}

pub(crate) fn member_active_skills(
    member: &DynamicMessage,
) -> Result<Vec<TutorialCharacterSkill>, StateError> {
    message_list(&member_status(member, "ally")?, "active_skills")
        .iter()
        .map(|skill| {
            Ok(TutorialCharacterSkill {
                id: i32_field(skill, "skill_id").ok_or(StateError::InvalidRequest)?,
                skill_type: i32_field(skill, "active_skill_type")
                    .ok_or(StateError::InvalidRequest)?,
                rank_ids: Vec::new(),
                evolved_ids: Vec::new(),
            })
        })
        .collect()
}

pub(crate) fn build_ally_member(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    party_member: &BattlePartyMember,
    member_id: i32,
    battle_id: i32,
    mental_buff: i32,
) -> Result<DynamicMessage, StateError> {
    let character = rule_character(rules, party_member.character_id)?;
    let stats = party_member.integrated_stats.unwrap_or(tutorial_ally_stats(
        rules,
        party_member.character_id,
        party_member.level,
        party_member.rarity,
        party_member.memoria_id,
    )?);
    let initial_status = battle_status_message(proto, stats)?;
    let mut current_stats = stats;
    if battle_id == 10000280 {
        for value in [
            &mut current_stats.attack,
            &mut current_stats.magic,
            &mut current_stats.defense,
            &mut current_stats.mental,
        ] {
            *value = checked_i32(i64::from(*value) * 115 / 100)?;
        }
    }
    current_stats.mental =
        checked_i32(i64::from(current_stats.mental) * i64::from(10_000 + mental_buff) / 10_000)?;
    let current_status = battle_status_message(proto, current_stats)?;
    let resistance = battle_resistance_message(proto, &character.resistance)?;
    let mut ally = empty_message(proto, "blend.model.BattleAlly")?;
    ally.set_field_by_name("character_id", Value::I32(party_member.character_id));
    ally.set_field_by_name(
        "current_character_id",
        Value::I32(party_member.character_id),
    );
    ally.set_field_by_name("is_leader", Value::Bool(party_member.is_leader));
    let mut skills = Vec::new();
    for selected in &party_member.skills {
        if rule_skill(rules, selected.id)?
            .skill_destination
            .is_some_and(|destination| !character.extra_skill_ids.contains(&destination))
        {
            return Err(StateError::InvalidRequest);
        }
        let mut message = empty_message(proto, "blend.model.BattleSkill")?;
        message.set_field_by_name("skill_id", Value::I32(selected.id));
        message.set_field_by_name("skill_type", Value::I32(selected.skill_type));
        message.set_field_by_name("rest_count", Value::Message(int32_value(proto, 0)?));
        message.set_field_by_name("lamp", Value::I32(0));
        skills.push(Value::Message(message));
    }
    ally.set_field_by_name("skills", Value::List(skills));
    let active_skills = character
        .active_skills
        .iter()
        .map(|selected| {
            let skill = rule_skill(rules, selected.id)?;
            let mut message = empty_message(proto, "blend.model.BattleActiveSkill")?;
            message.set_field_by_name("skill_id", Value::I32(selected.id));
            message.set_field_by_name("active_skill_type", Value::I32(selected.skill_type));
            message.set_field_by_name(
                "rest_count",
                Value::Message(int32_value(proto, skill.limit_count.unwrap_or(0))?),
            );
            Ok(Value::Message(message))
        })
        .collect::<Result<Vec<_>, StateError>>()?;
    ally.set_field_by_name("active_skills", Value::List(active_skills));

    let mut member = empty_message(proto, "blend.model.BattleMember")?;
    member.set_field_by_name("member_id", Value::I32(member_id));
    member.set_field_by_name("type", Value::EnumNumber(0));
    member.set_field_by_name("max_hp", Value::I32(stats.hp));
    member.set_field_by_name("hp", Value::I32(stats.hp));
    member.set_field_by_name("initial_hp", Value::I32(stats.hp));
    member.set_field_by_name("is_alive", Value::Bool(true));
    member.set_field_by_name("is_stun", Value::Bool(false));
    member.set_field_by_name("is_barrier_broken", Value::Bool(false));
    member.set_field_by_name("initial_status", Value::Message(initial_status));
    member.set_field_by_name("current_status", Value::Message(current_status));
    member.set_field_by_name("resistance", Value::Message(resistance));
    member.set_field_by_name(
        "burst_gauge",
        Value::Message(burst_gauge(
            proto,
            party_member
                .ability_ids
                .iter()
                .filter_map(|ability_id| {
                    rules
                        .abilities
                        .iter()
                        .find(|ability| ability.id == *ability_id)
                        .and_then(|ability| ability.burst_gauge_max)
                })
                .max()
                .unwrap_or(rules.constants.burst_gauge_required_for_one_burst_skill),
        )?),
    );
    member.set_field_by_name("ally", Value::Message(ally));
    let mut summaries = if battle_id == 10000280 {
        vec![(7, 5000), (8, 1500), (9, 1500), (33, 5000)]
    } else if matches!(battle_id, 10000279 | 10000003 | 10000004) {
        vec![(7, 5000), (33, 5000)]
    } else {
        Vec::new()
    };
    if party_member.damage_bonus != 0 {
        summaries.push((1, party_member.damage_bonus));
    }
    member.set_field_by_name(
        "state_change_summaries",
        Value::List(
            summaries
                .iter()
                .map(|(id, value)| {
                    let mut summary = empty_message(proto, "blend.model.BattleStateChangeSummary")?;
                    summary.set_field_by_name("id", Value::I32(*id));
                    summary.set_field_by_name("value", Value::I32(*value));
                    Ok(Value::Message(summary))
                })
                .collect::<Result<Vec<_>, StateError>>()?,
        ),
    );
    Ok(member)
}

pub(crate) fn burst_gauge(
    proto: &ProtoRegistry,
    max_gauge: i32,
) -> Result<DynamicMessage, StateError> {
    let mut gauge = empty_message(proto, "blend.model.BattleBurstGauge")?;
    gauge.set_field_by_name("current_gauge", Value::I32(0));
    gauge.set_field_by_name("max_gauge", Value::I32(max_gauge));
    gauge.set_field_by_name("is_enable", Value::Bool(false));
    Ok(gauge)
}

pub(crate) fn build_enemy_member(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    wave_enemy: &TutorialWaveEnemy,
    wave_id: i32,
    member_id: i32,
    base_enemy_number: i32,
) -> Result<DynamicMessage, StateError> {
    let enemy = rule_enemy(rules, wave_enemy.id)?;
    let (stats, break_gauge, break_wait) = tutorial_enemy_stats(enemy, wave_enemy, wave_id)?;
    let status = battle_status_message(proto, stats)?;
    let resistance = battle_resistance_message(proto, &enemy.resistance)?;
    let mut battle_enemy = empty_message(proto, "blend.model.BattleEnemy")?;
    battle_enemy.set_field_by_name("enemy_id", Value::I32(wave_enemy.id));
    battle_enemy.set_field_by_name("is_boss", Value::Bool(enemy.is_boss));
    // `base_enemy_number` is the occurrence within a base-enemy group.  The
    // tutorial rows have one break gauge; keeping these fields distinct is
    // important when several copies share a base row.
    battle_enemy.set_field_by_name("break_gauge_number", Value::I32(1));
    battle_enemy.set_field_by_name("max_break_gauge", Value::I32(break_gauge));
    battle_enemy.set_field_by_name("break_gauge", Value::I32(break_gauge));
    battle_enemy.set_field_by_name("is_broken", Value::Bool(false));
    if !enemy.break_gauges.is_empty() {
        battle_enemy.set_field_by_name(
            "initial_break_gauges",
            Value::List(enemy.break_gauges.iter().copied().map(Value::I32).collect()),
        );
    }
    battle_enemy.set_field_by_name("break_waits", Value::List(vec![Value::I32(break_wait)]));
    if !enemy.break_phases.is_empty() {
        let gauges = enemy
            .break_phases
            .iter()
            .map(|p| checked_i32((f64::from(break_gauge) * p.coefficient).floor() as i64))
            .collect::<Result<Vec<_>, _>>()?;
        battle_enemy.set_field_by_name("max_break_gauge", Value::I32(gauges[0]));
        battle_enemy.set_field_by_name("break_gauge", Value::I32(gauges[0]));
        battle_enemy.set_field_by_name(
            "initial_break_gauges",
            Value::List(gauges.into_iter().map(Value::I32).collect()),
        );
        battle_enemy.set_field_by_name(
            "break_waits",
            Value::List(
                enemy
                    .break_phases
                    .iter()
                    .map(|p| Value::I32(p.wait))
                    .collect(),
            ),
        );
    }
    battle_enemy.set_field_by_name(
        "base_enemy_number",
        Value::Message(int32_value(proto, base_enemy_number)?),
    );

    let mut member = empty_message(proto, "blend.model.BattleMember")?;
    member.set_field_by_name("member_id", Value::I32(member_id));
    member.set_field_by_name("type", Value::EnumNumber(1));
    member.set_field_by_name("max_hp", Value::I32(stats.hp));
    member.set_field_by_name("hp", Value::I32(stats.hp));
    member.set_field_by_name("initial_hp", Value::I32(stats.hp));
    member.set_field_by_name("is_alive", Value::Bool(true));
    member.set_field_by_name("is_stun", Value::Bool(false));
    member.set_field_by_name("is_barrier_broken", Value::Bool(false));
    member.set_field_by_name("initial_status", Value::Message(status.clone()));
    member.set_field_by_name("current_status", Value::Message(status));
    member.set_field_by_name("resistance", Value::Message(resistance));
    member.set_field_by_name(
        "burst_gauge",
        Value::Message(burst_gauge(
            proto,
            rules.constants.burst_gauge_required_for_one_burst_skill,
        )?),
    );
    member.set_field_by_name("enemy", Value::Message(battle_enemy));
    Ok(member)
}

pub(crate) fn build_battle_tool(
    proto: &ProtoRegistry,
    tool: &BattlePartyTool,
    number: i32,
) -> Result<DynamicMessage, StateError> {
    let mut message = empty_message(proto, "blend.model.BattleBattleTool")?;
    message.set_field_by_name("number", Value::I32(number));
    message.set_field_by_name("tool_id", Value::I32(tool.tool_id));
    message.set_field_by_name("usage_count", Value::I32(tool.usage_count));
    message.set_field_by_name(
        "traits",
        Value::List(
            tool.traits
                .iter()
                .map(|trait_param| battle_trait_message(proto, trait_param).map(Value::Message))
                .collect::<Result<Vec<_>, _>>()?,
        ),
    );
    Ok(message)
}

pub(crate) fn battle_trait_message(
    proto: &ProtoRegistry,
    trait_param: &TutorialTraitParam,
) -> Result<DynamicMessage, StateError> {
    let mut message = empty_message(proto, "blend.model.BattleTrait")?;
    message.set_field_by_name("trait_id", Value::I32(trait_param.id));
    message.set_field_by_name("rank", Value::I32(trait_param.rank));
    Ok(message)
}

pub(crate) fn battle_party_tool_from_state(
    rules: &TutorialRules,
    message: &DynamicMessage,
) -> Result<BattlePartyTool, StateError> {
    let tool_id = i32_field(message, "tool_id").ok_or(StateError::InvalidRequest)?;
    let rule = rule_tool(rules, tool_id)?;
    let traits = message_list(message, "traits")
        .into_iter()
        .map(|value| {
            let trait_param = TutorialTraitParam {
                id: i32_field(&value, "trait_id").ok_or(StateError::InvalidRequest)?,
                rank: i32_field(&value, "rank").ok_or(StateError::InvalidRequest)?,
            };
            rule_battle_tool_trait(rules, trait_param.id)?;
            Ok(trait_param)
        })
        .collect::<Result<Vec<_>, StateError>>()?;
    Ok(BattlePartyTool {
        tool_id,
        usage_count: rule.usage_count,
        traits,
    })
}

pub(crate) fn build_timeline_unit(
    proto: &ProtoRegistry,
    member_id: i32,
    number: i32,
    wait: i32,
) -> Result<DynamicMessage, StateError> {
    let mut unit = empty_message(proto, "blend.model.BattleTimelineUnit")?;
    unit.set_field_by_name("member_id", Value::I32(member_id));
    unit.set_field_by_name("number", Value::I32(number));
    unit.set_field_by_name("wait", Value::I32(wait));
    unit.set_field_by_name("type", Value::EnumNumber(0));
    unit.set_field_by_name("is_enemy_strong", Value::Bool(false));
    Ok(unit)
}

pub(crate) fn base_wait(speed: i32) -> i32 {
    57_600 / speed.max(1)
}

pub(crate) fn action_wait(speed: i32, skill_wait_adjustment: i32) -> i32 {
    // Master-data `skill.wait` is the adjustment from the displayed WAIT 200,
    // so this is equivalent to floor(57600 / speed) + WAIT - 200.
    (base_wait(speed) + skill_wait_adjustment).max(0)
}

pub(crate) fn build_timeline_panel(
    proto: &ProtoRegistry,
    panel_id: i32,
    turn: i32,
) -> Result<DynamicMessage, StateError> {
    let mut panel = empty_message(proto, "blend.model.BattleTimelinePanel")?;
    panel.set_field_by_name("turn", Value::I32(turn));
    // Neutral panel 11 is represented by an absent optional panel_id in the
    // client state; emitting an Int32Value(11) changes the merge semantics.
    if panel_id != 11 {
        panel.set_field_by_name("panel_id", Value::Message(int32_value(proto, panel_id)?));
    }
    Ok(panel)
}

pub(crate) const BATTLE_STATUS_IN_BATTLE: i32 = 0;
pub(crate) const BATTLE_STATUS_WON: i32 = 1;
pub(crate) const BATTLE_STATUS_LOST: i32 = 2;

pub(crate) fn bool_field(message: &DynamicMessage, name: &str) -> bool {
    message
        .get_field_by_name(name)
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

pub(crate) fn wrapper_i32(proto: &ProtoRegistry, value: i32) -> Result<DynamicMessage, StateError> {
    int32_value(proto, value)
}

pub(crate) fn wrapper_i64(proto: &ProtoRegistry, value: i64) -> Result<DynamicMessage, StateError> {
    let mut wrapper = empty_message(proto, "google.protobuf.Int64Value")?;
    wrapper.set_field_by_name("value", Value::I64(value));
    Ok(wrapper)
}

pub(crate) fn member_status(
    message: &DynamicMessage,
    name: &str,
) -> Result<DynamicMessage, StateError> {
    message
        .get_field_by_name(name)
        .and_then(|value| value.as_message().cloned())
        .ok_or(StateError::InvalidRequest)
}

pub(crate) fn member_id(message: &DynamicMessage) -> Result<i32, StateError> {
    i32_field(message, "member_id").ok_or(StateError::InvalidRequest)
}

pub(crate) fn member_type(message: &DynamicMessage) -> Result<i32, StateError> {
    i32_or_enum_field(message, "type").ok_or(StateError::InvalidRequest)
}

pub(crate) fn current_battle_status(state: &DynamicMessage) -> Result<i32, StateError> {
    let members = message_list(state, "members");
    if members.is_empty() {
        return Err(StateError::InvalidRequest);
    }
    let allies_alive = members
        .iter()
        .any(|member| member_type(member).ok() == Some(0) && bool_field(member, "is_alive"));
    let enemies_alive = members
        .iter()
        .any(|member| member_type(member).ok() == Some(1) && bool_field(member, "is_alive"));
    Ok(if !allies_alive {
        BATTLE_STATUS_LOST
    } else if !enemies_alive {
        BATTLE_STATUS_WON
    } else {
        BATTLE_STATUS_IN_BATTLE
    })
}

pub(crate) fn battle_panel_multiplier(state: &DynamicMessage) -> (i128, i128) {
    match effective_battle_panel_id(state) {
        12 | 22 => (140, 100),
        13 => (60, 100),
        24 => (200, 100),
        26 => (40, 100),
        _ => (100, 100),
    }
}

pub(crate) fn battle_panel_break_multiplier(state: &DynamicMessage) -> i128 {
    match effective_battle_panel_id(state) {
        22 | 39 => 140,
        _ => 100,
    }
}

pub(crate) fn battle_panel_guarantees_critical(state: &DynamicMessage) -> bool {
    effective_battle_panel_id(state) == 30
}

pub(crate) fn effective_battle_panel_id(state: &DynamicMessage) -> i32 {
    let panel_id = current_panel_id(state);
    if !is_burst_panel_id(panel_id)
        && current_actor(state).ok().is_some_and(|actor| {
            message_list(&actor, "state_changes")
                .iter()
                .any(|change| i32_field(change, "state_change_id") == Some(610093))
        })
    {
        return 11;
    }
    panel_id
}
