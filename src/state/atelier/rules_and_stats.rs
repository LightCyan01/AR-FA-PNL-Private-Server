use super::prelude::*;

#[derive(Clone, Deserialize)]
pub(crate) struct AtelierRules {
    #[serde(rename = "format")]
    pub(crate) format_name: String,
    pub source_sha256: String,
    pub(crate) constants: AtelierConstants,
    pub(crate) research: Vec<ResearchRule>,
    pub(crate) research_effect_levels: Vec<ResearchEffectLevel>,
    pub(crate) communications: Vec<CommunicationRule>,
    pub(crate) illustrated_books: Vec<IllustratedBookRule>,
    pub(crate) collection_memoria: Vec<CollectionRule>,
    pub(crate) collection_valuable_items: Vec<CollectionRule>,
    pub(crate) collection_reward_sets: BTreeMap<i32, Vec<TutorialReward>>,
    pub(crate) valuable_item_ids: Vec<i32>,
    pub(crate) memorias: Vec<MemoriaRule>,
    pub(crate) memoria_buff_growths: Vec<MemoriaBuffGrowth>,
    pub(crate) memoria_levels: Vec<CharacterLevel>,
    pub(crate) memoria_rarities: Vec<MemoriaRarityRule>,
    pub(crate) memoria_exp_items: Vec<ValueRule>,
    pub(crate) memoria_limit_break_items: Vec<LimitBreakItemRule>,
    pub(crate) tool_rarities: Vec<ToolRarityRule>,
    pub(crate) trait_ranks: Vec<TraitRankRule>,
    pub(crate) trait_rank_totals: Vec<TraitTotalRule>,
    pub(crate) battle_tools: Vec<ToolRule>,
    pub(crate) equipment_tools: Vec<ToolRule>,
    pub(crate) equipment_traits: Vec<EquipmentTraitRule>,
    pub(crate) ships: Vec<i32>,
    pub(crate) ship_tools: Vec<i32>,
    pub(crate) ship_parts: Vec<ShipPartRule>,
    pub(crate) ship_levels: Vec<ShipLevelRule>,
    pub(crate) ship_tool_levels: Vec<ShipToolLevelRule>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct AtelierConstants {
    pub(crate) max_communication_story_number: i32,
    pub(crate) max_free_communication_story_number: i32,
    pub(crate) number_of_keys_required_communication_release: i32,
    pub(crate) memoria_enhancement_cole_per_exp_rate: i32,
    pub(crate) memoria_exp_return_rate: i32,
    pub(crate) tool_conversion_limit_count: i32,
    pub(crate) party_count_per_content: i32,
    pub(crate) research_task_condition_ids: BTreeMap<i32, i32>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct ResearchRule {
    pub(crate) group_id: i32,
    pub(crate) level: i32,
    pub(crate) minimum_character_level: Option<i32>,
    pub(crate) cost: RuleCost,
    pub(crate) requirements: Vec<TaskCountRule>,
    pub(crate) research_effect_ids: Vec<i32>,
    pub(crate) start_at: Option<i64>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct ResearchEffectLevel {
    pub(crate) research_effect_id: i32,
    pub(crate) level: i32,
    pub(crate) value: i32,
    pub(crate) status_buffs: Vec<ResearchStatusBuff>,
    pub(crate) equipment_tool_buffs: Vec<ResearchEquipmentBuff>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct ResearchStatusBuff {
    pub(crate) role: i32,
    pub(crate) status_type: i32,
}

#[derive(Clone, Deserialize)]
pub(crate) struct ResearchEquipmentBuff {
    pub(crate) status_type: i32,
}

#[derive(Clone, Deserialize)]
pub(crate) struct CommunicationRule {
    pub(crate) character_id: i32,
    pub(crate) story_number: i32,
    pub(crate) key_tasks: Vec<TaskCountRule>,
    pub(crate) rewards: Vec<TutorialReward>,
    pub(crate) reward_scene_id: Option<i32>,
    pub(crate) start_at: Option<i64>,
    pub(crate) end_at: Option<i64>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct IllustratedBookRule {
    pub(crate) id: i32,
    pub(crate) start_at: Option<i64>,
    pub(crate) end_at: Option<i64>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct CollectionRule {
    pub(crate) illustrated_book_reward_id: i32,
    pub(crate) reward_type: i32,
    pub(crate) count: i32,
    pub(crate) reward_set_ids: Vec<i32>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct MemoriaRule {
    pub(crate) id: i32,
    pub(crate) rarity: i32,
    pub(crate) item_limit_break_enabled: bool,
    pub(crate) status_buffs: Vec<MemoriaStatusBuff>,
    pub(crate) rank_ability_effects: Vec<Vec<AbilityEffect>>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct MemoriaStatusBuff {
    #[serde(rename = "type")]
    pub(crate) status_type: i32,
    pub(crate) growth_id: i32,
    pub(crate) initial_values: Vec<i32>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct MemoriaBuffGrowth {
    pub(crate) id: i32,
    pub(crate) values: Vec<i32>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct MemoriaRarityRule {
    pub(crate) id: i32,
    pub(crate) exp: i32,
    pub(crate) level_limit: i32,
    pub(crate) max_limit_break: i32,
    pub(crate) sold_cole: i32,
}

#[derive(Clone, Deserialize)]
pub(crate) struct ValueRule {
    pub(crate) id: i32,
    pub(crate) value: i32,
}

#[derive(Clone, Deserialize)]
pub(crate) struct LimitBreakItemRule {
    pub(crate) id: i32,
    pub(crate) rarity: i32,
    pub(crate) value: i32,
}

#[derive(Clone, Deserialize)]
pub(crate) struct ToolRarityRule {
    pub(crate) id: i32,
    pub(crate) disable_conversion: bool,
    pub(crate) battle_tool_conversion_rewards: Vec<TutorialReward>,
    pub(crate) equipment_tool_conversion_rewards: Vec<TutorialReward>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct TraitRankRule {
    pub(crate) id: i32,
    pub(crate) rank_up_cost: RuleCost,
}

#[derive(Clone, Deserialize)]
pub(crate) struct TraitTotalRule {
    pub(crate) id: i32,
    pub(crate) battle_tool_conversion_rewards: Vec<TutorialReward>,
    pub(crate) equipment_tool_conversion_rewards: Vec<TutorialReward>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct ToolRule {
    pub(crate) id: i32,
    pub(crate) rarity: i32,
    #[serde(default)]
    pub(crate) status_buffs: Vec<ToolStatusBuff>,
    #[serde(default)]
    pub(crate) ability_effects: Vec<AbilityEffect>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct ToolStatusBuff {
    pub(crate) change_method: i32,
    pub(crate) status_type: i32,
    pub(crate) value: i32,
}

#[derive(Clone, Deserialize)]
pub(crate) struct EquipmentTraitRule {
    pub(crate) id: i32,
    pub(crate) rank_ability_effects: Vec<Vec<AbilityEffect>>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct AbilityEffect {
    pub(crate) id: i32,
    pub(crate) value: i32,
}

#[derive(Clone, Deserialize)]
pub(crate) struct ShipPartRule {
    pub(crate) id: i32,
    pub(crate) enhance_target: i32,
    pub(crate) enhance_type: i32,
    pub(crate) target_id: i32,
    pub(crate) value: i32,
    pub(crate) costs: Vec<RuleCost>,
    pub(crate) start_at: Option<i64>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct ShipLevelRule {
    pub(crate) exp: i32,
    pub(crate) rank: i32,
    pub(crate) max_character_count: i32,
    pub(crate) max_sub_memoria_count: i32,
    pub(crate) max_tool_count: i32,
}

#[derive(Clone, Deserialize)]
pub(crate) struct ShipToolLevelRule {
    pub(crate) exp: i32,
    pub(crate) rank: i32,
}

pub(crate) fn load_rules() -> Result<AtelierRules, StateError> {
    let rules: AtelierRules = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../data/atelier_rules.json"
    )))
    .map_err(|error| StateError::MasterData(error.to_string()))?;
    if rules.format_name != "atelier-systems-v1"
        || rules.research.is_empty()
        || rules.communications.is_empty()
        || rules.memorias.is_empty()
        || rules.ship_levels.is_empty()
    {
        return Err(StateError::MasterData(
            "atelier rules are incomplete".into(),
        ));
    }
    Ok(rules)
}

pub(crate) fn is_route(route: &str) -> bool {
    route.starts_with("/atelier/")
        || route.starts_with("/tool/")
        || route.starts_with("/communication/")
        || route.starts_with("/illustrated_book/")
        || route.starts_with("/memoria/")
        || route.starts_with("/ship/")
}

pub(crate) fn stat_mut(stats: &mut BattleStats, status_type: i32) -> Result<&mut i32, StateError> {
    match status_type {
        1 => Ok(&mut stats.hp),
        2 => Ok(&mut stats.speed),
        3 => Ok(&mut stats.attack),
        4 => Ok(&mut stats.magic),
        5 => Ok(&mut stats.defense),
        6 => Ok(&mut stats.mental),
        _ => Err(StateError::InvalidRequest),
    }
}

pub(crate) fn add_stat(
    stats: &mut BattleStats,
    status_type: i32,
    value: i32,
) -> Result<(), StateError> {
    let stat = stat_mut(stats, status_type)?;
    *stat = stat.checked_add(value).ok_or(StateError::InvalidRequest)?;
    Ok(())
}

pub(crate) fn scale_stat(
    stats: &mut BattleStats,
    status_type: i32,
    rate: i32,
) -> Result<(), StateError> {
    let stat = stat_mut(stats, status_type)?;
    *stat = i32::try_from(i64::from(*stat) * i64::from(10_000 + rate) / 10_000)
        .map_err(|_| StateError::InvalidRequest)?;
    Ok(())
}

pub(crate) fn active_research_effects<'a>(
    rules: &'a AtelierRules,
    resources: &DynamicMessage,
) -> Vec<&'a ResearchEffectLevel> {
    let mut levels = BTreeMap::new();
    for group in message_list(resources, "research_groups") {
        let group_id = i32_field(&group, "group_id").unwrap_or_default();
        let level = i32_field(&group, "level").unwrap_or_default();
        for id in rules
            .research
            .iter()
            .filter(|row| row.group_id == group_id && row.level <= level)
            .flat_map(|row| row.research_effect_ids.iter())
        {
            *levels.entry(*id).or_insert(0) += 1;
        }
    }
    levels
        .into_iter()
        .filter_map(|(id, level)| {
            rules
                .research_effect_levels
                .iter()
                .find(|row| row.research_effect_id == id && row.level == level)
        })
        .collect()
}

pub(crate) fn combat_stats(
    rules: &AtelierRules,
    resources: &DynamicMessage,
    character: &DynamicMessage,
    member: &DynamicMessage,
    role: i32,
    mut stats: BattleStats,
) -> Result<(BattleStats, Vec<TutorialSkillEffect>), StateError> {
    for (field, status_type) in [
        ("growboard_hp", 1),
        ("growboard_speed", 2),
        ("growboard_attack", 3),
        ("growboard_magic", 4),
        ("growboard_defense", 5),
        ("growboard_mental", 6),
    ] {
        add_stat(
            &mut stats,
            status_type,
            i32_field(character, field).unwrap_or_default(),
        )?;
    }

    let research = active_research_effects(rules, resources);
    for effect in &research {
        for buff in effect.status_buffs.iter().filter(|buff| buff.role == role) {
            add_stat(&mut stats, buff.status_type, effect.value)?;
        }
    }
    let equipment_rates: BTreeMap<i32, i32> = research
        .iter()
        .flat_map(|effect| {
            effect
                .equipment_tool_buffs
                .iter()
                .map(move |buff| (buff.status_type, effect.value))
        })
        .fold(BTreeMap::new(), |mut values, (status, value)| {
            *values.entry(status).or_default() += value;
            values
        });

    let mut passives = Vec::new();
    for field in [
        "slot1_equipment_tool_entity_id",
        "slot2_equipment_tool_entity_id",
        "slot3_equipment_tool_entity_id",
    ] {
        let Some(entity_id) =
            optional_i32_field(member, field).or_else(|| optional_i32_field(character, field))
        else {
            continue;
        };
        let entity = message_list(resources, "equipment_tools")
            .into_iter()
            .find(|row| i32_field(row, "entity_id") == Some(entity_id))
            .ok_or(StateError::InvalidRequest)?;
        let tool = rules
            .equipment_tools
            .iter()
            .find(|row| i32_field(&entity, "tool_id") == Some(row.id))
            .ok_or(StateError::InvalidRequest)?;
        for buff in &tool.status_buffs {
            if buff.change_method != 1 {
                return Err(StateError::InvalidRequest);
            }
            let rate = equipment_rates
                .get(&buff.status_type)
                .copied()
                .unwrap_or_default();
            add_stat(
                &mut stats,
                buff.status_type,
                i32::try_from(i64::from(buff.value) * i64::from(10_000 + rate) / 10_000)
                    .map_err(|_| StateError::InvalidRequest)?,
            )?;
        }
        passives.extend(tool.ability_effects.iter().map(|e| TutorialSkillEffect {
            id: e.id,
            value: e.value,
        }));
        for trait_params in message_list(&entity, "traits") {
            let trait_rule = rules
                .equipment_traits
                .iter()
                .find(|row| i32_field(&trait_params, "id") == Some(row.id))
                .ok_or(StateError::InvalidRequest)?;
            let rank = i32_field(&trait_params, "rank").unwrap_or_default();
            let effects = trait_rule
                .rank_ability_effects
                .get(usize::try_from(rank - 1).map_err(|_| StateError::InvalidRequest)?)
                .ok_or(StateError::InvalidRequest)?;
            passives.extend(effects.iter().map(|e| TutorialSkillEffect {
                id: e.id,
                value: e.value,
            }));
        }
    }

    if let Some(entity_id) = optional_i32_field(member, "memoria_entity_id")
        .or_else(|| optional_i32_field(character, "memoria_entity_id"))
    {
        let entity = message_list(resources, "memorias")
            .into_iter()
            .find(|row| i32_field(row, "entity_id") == Some(entity_id))
            .ok_or(StateError::InvalidRequest)?;
        let memoria = rules
            .memorias
            .iter()
            .find(|row| i32_field(&entity, "memoria_id") == Some(row.id))
            .ok_or(StateError::InvalidRequest)?;
        let limit_break = i32_field(&entity, "limit_break").unwrap_or_default().max(0) as usize;
        let level = rules
            .memoria_levels
            .iter()
            .filter(|row| row.exp <= i32_field(&entity, "exp").unwrap_or_default())
            .map(|row| row.level)
            .max()
            .unwrap_or(1)
            .max(1) as usize;
        for buff in &memoria.status_buffs {
            let initial = buff
                .initial_values
                .get(limit_break)
                .copied()
                .unwrap_or_default();
            let growth = rules
                .memoria_buff_growths
                .iter()
                .find(|row| row.id == buff.growth_id)
                .and_then(|row| row.values.get(level - 1))
                .copied()
                .unwrap_or_default();
            scale_stat(&mut stats, buff.status_type, initial + growth)?;
        }
        if let Some(effects) = memoria.rank_ability_effects.get(limit_break) {
            passives.extend(effects.iter().map(|e| TutorialSkillEffect {
                id: e.id,
                value: e.value,
            }));
        }
    }

    let all_rate = i32_field(character, "growboard_all_status_rate").unwrap_or_default();
    let role_rate = message_list(resources, "growboard_role_rates")
        .into_iter()
        .find(|row| i32_field(row, "role") == Some(role))
        .and_then(|row| {
            row.get_field_by_name("rate")
                .and_then(|value| value.as_message().cloned())
        });
    for (field, status_type) in [
        ("hp", 1),
        ("speed", 2),
        ("attack", 3),
        ("magic", 4),
        ("defense", 5),
        ("mental", 6),
    ] {
        scale_stat(
            &mut stats,
            status_type,
            all_rate
                + role_rate
                    .as_ref()
                    .and_then(|rate| i32_field(rate, field))
                    .unwrap_or_default(),
        )?;
    }
    Ok((stats, passives))
}

pub(crate) fn in_period(start: Option<i64>, end: Option<i64>, now: i64) -> bool {
    start.is_none_or(|value| value <= now) && end.is_none_or(|value| now < value)
}

pub(crate) fn request_message(
    request: &DynamicMessage,
    field: &str,
) -> Result<DynamicMessage, StateError> {
    request
        .get_field_by_name(field)
        .and_then(|value| value.as_message().cloned())
        .ok_or(StateError::InvalidRequest)
}

pub(crate) fn set_messages(message: &mut DynamicMessage, field: &str, values: Vec<DynamicMessage>) {
    message.set_field_by_name(
        field,
        Value::List(values.into_iter().map(Value::Message).collect()),
    );
}

pub(crate) fn unique(values: &[i32]) -> bool {
    values
        .iter()
        .enumerate()
        .all(|(index, value)| *value > 0 && !values[..index].contains(value))
}

pub(crate) fn owned(resources: &DynamicMessage, field: &str, entity_id: i32) -> bool {
    message_list(resources, field)
        .iter()
        .any(|row| i32_field(row, "entity_id") == Some(entity_id))
}

pub(crate) fn assigned_memoria(resources: &DynamicMessage, entity_id: i32) -> bool {
    ["characters", "party_members", "equipment_presets"]
        .iter()
        .any(|field| {
            message_list(resources, field)
                .iter()
                .any(|row| optional_i32_field(row, "memoria_entity_id") == Some(entity_id))
        })
        || message_list(resources, "ship_parties").iter().any(|row| {
            optional_i32_field(row, "main_memoria_entity_id") == Some(entity_id)
                || i32_list(row, "sub_memoria_entity_ids").contains(&entity_id)
        })
}

pub(crate) fn assigned_tool(
    resources: &DynamicMessage,
    resource_type: i32,
    entity_id: i32,
) -> bool {
    match resource_type {
        6 => ["characters", "party_members", "equipment_presets"]
            .iter()
            .any(|field| {
                message_list(resources, field).iter().any(|row| {
                    [
                        "slot1_equipment_tool_entity_id",
                        "slot2_equipment_tool_entity_id",
                        "slot3_equipment_tool_entity_id",
                    ]
                    .iter()
                    .any(|name| optional_i32_field(row, name) == Some(entity_id))
                })
            }),
        14 => message_list(resources, "parties")
            .iter()
            .any(|row| i32_list(row, "battle_tool_entity_ids").contains(&entity_id)),
        _ => true,
    }
}

pub(crate) fn remove_entity(
    resources: &mut DynamicMessage,
    field: &str,
    entity_id: i32,
) -> Result<DynamicMessage, StateError> {
    let mut values = message_list(resources, field);
    let index = values
        .iter()
        .position(|row| i32_field(row, "entity_id") == Some(entity_id))
        .ok_or(StateError::InvalidRequest)?;
    let removed = values.remove(index);
    set_messages(resources, field, values);
    Ok(removed)
}

pub(crate) fn deleted_resources(
    proto: &ProtoRegistry,
    equipment: &[i32],
    battle: &[i32],
    memoria: &[i32],
) -> Result<DynamicMessage, StateError> {
    let mut deleted = empty_message(proto, "blend.model.ResourceEntities")?;
    deleted.set_field_by_name(
        "equipment_tool_entity_ids",
        Value::List(equipment.iter().copied().map(Value::I32).collect()),
    );
    deleted.set_field_by_name(
        "battle_tool_entity_ids",
        Value::List(battle.iter().copied().map(Value::I32).collect()),
    );
    deleted.set_field_by_name(
        "memoria_entity_ids",
        Value::List(memoria.iter().copied().map(Value::I32).collect()),
    );
    Ok(deleted)
}

pub(crate) fn aggregate_rewards(
    rewards: impl IntoIterator<Item = TutorialReward>,
) -> Result<Vec<TutorialReward>, StateError> {
    let mut totals = BTreeMap::<(i32, i32), i32>::new();
    for reward in rewards {
        if reward.resource_params.is_some() || reward.quantity <= 0 {
            return Err(StateError::InvalidRequest);
        }
        let total = totals.entry((reward.resource_type, reward.id)).or_default();
        *total = total
            .checked_add(reward.quantity)
            .ok_or(StateError::InvalidRequest)?;
    }
    Ok(totals
        .into_iter()
        .map(|((resource_type, id), quantity)| TutorialReward {
            resource_type,
            id,
            quantity,
            resource_params: None,
        })
        .collect())
}
