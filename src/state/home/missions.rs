use super::prelude::*;

// Mission objective evaluation and claim helpers.

pub(crate) fn completed_mission_count(
    rules: &HomeRules,
    resources: &DynamicMessage,
    mission_ids: &[i32],
) -> usize {
    mission_ids
        .iter()
        .filter(|id| {
            rules
                .missions
                .iter()
                .find(|mission| mission.id == **id)
                .and_then(|mission| mission.steps.last().map(|step| (mission, step)))
                .is_some_and(|(mission, step)| mission_count(resources, mission) >= step.count)
        })
        .count()
}

pub(crate) fn objective_count(
    rules: &HomeRules,
    resources: &DynamicMessage,
    objective: &ResourceObjective,
) -> i32 {
    match objective {
        ResourceObjective::Rank => message_i32_field(resources, "status", "rank").unwrap_or(1),
        ResourceObjective::Characters => message_iter(resources, "characters").count() as i32,
        ResourceObjective::Recipes => message_iter(resources, "recipes").count() as i32,
        ResourceObjective::CharacterLevel { minimum, .. }
        | ResourceObjective::MemoriaLevel { minimum, .. } => {
            let character = matches!(objective, ResourceObjective::CharacterLevel { .. });
            let levels = if character {
                &rules.character_levels
            } else {
                &rules.memoria_levels
            };
            let values: Vec<_> =
                message_iter(resources, if character { "characters" } else { "memorias" })
                    .filter(|row| match objective {
                        ResourceObjective::CharacterLevel { character_ids, .. }
                            if !character_ids.is_empty() =>
                        {
                            i32_field(row, "character_id")
                                .is_some_and(|id| character_ids.contains(&id))
                        }
                        ResourceObjective::MemoriaLevel {
                            memoria_id: Some(id),
                            ..
                        } => i32_field(row, "memoria_id") == Some(*id),
                        _ => true,
                    })
                    .map(|row| {
                        levels
                            .iter()
                            .filter(|level| level.exp <= i32_field(row, "exp").unwrap_or(0))
                            .map(|level| level.level)
                            .max()
                            .unwrap_or(1)
                    })
                    .collect();
            if let Some(minimum) = minimum {
                values.iter().filter(|value| **value >= *minimum).count() as i32
            } else {
                values.into_iter().max().unwrap_or(0)
            }
        }
        ResourceObjective::Board { field, minimum } => message_iter(resources, "characters")
            .filter(|row| i32_field(row, field).unwrap_or(0) >= *minimum)
            .count() as i32,
        ResourceObjective::Equipment {
            character_id,
            tool_ids,
            trait_ids,
        } => {
            let Some(character) = message_iter(resources, "characters")
                .find(|r| i32_field(r, "character_id") == Some(*character_id))
            else {
                return 0;
            };
            i32::from(
                [
                    "slot1_equipment_tool_entity_id",
                    "slot2_equipment_tool_entity_id",
                    "slot3_equipment_tool_entity_id",
                ]
                .iter()
                .any(|field| {
                    let entity = message_i32_field(character, field, "value");
                    message_iter(resources, "equipment_tools").any(|tool| {
                        entity.is_some()
                            && i32_field(tool, "entity_id") == entity
                            && i32_field(tool, "tool_id").is_some_and(|id| tool_ids.contains(&id))
                            && message_iter(tool, "traits").any(|t| {
                                i32_field(t, "id").is_some_and(|id| trait_ids.contains(&id))
                            })
                    })
                }),
            )
        }
        ResourceObjective::EquippedMemoria {
            character_id,
            memoria_id,
        } => {
            let entity = message_iter(resources, "characters")
                .find(|r| i32_field(r, "character_id") == Some(*character_id))
                .and_then(|r| message_i32_field(r, "memoria_entity_id", "value"));
            i32::from(
                entity.is_some()
                    && message_iter(resources, "memorias").any(|r| {
                        i32_field(r, "entity_id") == entity
                            && i32_field(r, "memoria_id") == Some(*memoria_id)
                    }),
            )
        }
        ResourceObjective::OwnedCharacter { character_ids } => {
            message_iter(resources, "characters")
                .filter(|row| {
                    i32_field(row, "character_id").is_some_and(|id| character_ids.contains(&id))
                })
                .count() as i32
        }
        ResourceObjective::CharacterRarity {
            character_ids,
            initial_rarity,
        } => message_iter(resources, "characters")
            .filter(|row| {
                i32_field(row, "character_id").is_some_and(|id| character_ids.contains(&id))
            })
            .filter_map(|row| i32_field(row, "rarity"))
            .map(|r| r.saturating_sub(*initial_rarity).max(0))
            .max()
            .unwrap_or(0),
        ResourceObjective::OwnedMemoria { memoria_id } => i32::from(
            message_iter(resources, "memorias")
                .any(|row| i32_field(row, "memoria_id") == Some(*memoria_id)),
        ),
        ResourceObjective::MemoriaLimit { memoria_id } => message_iter(resources, "memorias")
            .filter(|row| i32_field(row, "memoria_id") == Some(*memoria_id))
            .filter_map(|row| i32_field(row, "limit_break"))
            .max()
            .unwrap_or(0),
        ResourceObjective::Research { group_id } => message_iter(resources, "research_groups")
            .find(|row| i32_field(row, "group_id") == Some(*group_id))
            .and_then(|row| i32_field(row, "level"))
            .unwrap_or(0),
        ResourceObjective::Communication { character_ids } => {
            message_iter(resources, "communication_states")
                .filter(|row| {
                    i32_field(row, "character_id").is_some_and(|id| character_ids.contains(&id))
                })
                .flat_map(|row| i32_list(row, "cleared_story_numbers"))
                .max()
                .unwrap_or(0)
        }
        ResourceObjective::NeoOpen => message_iter(resources, "characters")
            .filter(|row| {
                i32_field(row, "growboard_neo_current_page").unwrap_or(1) > 1
                    || i32_field(row, "growboard_neo_panel_bits").unwrap_or(0) != 0
            })
            .count() as i32,
        ResourceObjective::Tower { floors } => floors
            .iter()
            .filter(|row| quest_clear_count(resources, row.quest_id) > 0)
            .map(|row| row.floor)
            .max()
            .unwrap_or(0),
        ResourceObjective::Housing { house_building_id } => {
            message_iter(resources, "house_building_states")
                .find(|row| i32_field(row, "house_building_id") == Some(*house_building_id))
                .and_then(|row| i32_field(row, "level"))
                .unwrap_or(0)
        }
        ResourceObjective::PresentCloseness { present_ids } => {
            message_iter(resources, "present_states")
                .filter(|row| {
                    i32_field(row, "present_id").is_some_and(|id| present_ids.contains(&id))
                })
                .filter_map(|row| i32_field(row, "closeness"))
                .max()
                .unwrap_or(0)
        }
        ResourceObjective::StreetPhase { quest_id } => message_iter(resources, "street_states")
            .find(|row| i32_field(row, "quest_id") == Some(*quest_id))
            .and_then(|row| i32_field(row, "phase"))
            .unwrap_or(0),
        ResourceObjective::ScoreRank {
            quest_ids,
            minimum_rank,
        } => message_iter(resources, "quest_states")
            .filter(|row| {
                i32_field(row, "quest_id")
                    .is_some_and(|id| quest_ids.is_empty() || quest_ids.contains(&id))
                    && i32_field(row, "score_rank").unwrap_or(0) >= *minimum_rank
            })
            .count() as i32,
        ResourceObjective::OwnedHomeMotion { motion_ids } => {
            message_iter(resources, "chara_home_motions")
                .filter(|row| {
                    i32_field(row, "motion_id").is_some_and(|id| motion_ids.contains(&id))
                })
                .count() as i32
        }
        ResourceObjective::CompletedMissions { mission_ids } => {
            completed_mission_count(rules, resources, mission_ids).min(i32::MAX as usize) as i32
        }
        ResourceObjective::QuestHighScoreTotal { quest_ids } => {
            message_iter(resources, "quest_states")
                .filter(|state| {
                    i32_field(state, "quest_id").is_some_and(|id| quest_ids.contains(&id))
                })
                .filter_map(|state| message_i32_field(state, "high_score_detail", "total_score"))
                .fold(0, i32::saturating_add)
        }
        ResourceObjective::QuestClearAny { quest_ids } => quest_ids
            .iter()
            .map(|id| quest_clear_count(resources, *id))
            .max()
            .unwrap_or(0),
        ResourceObjective::MultiMission { multi_mission_id } => {
            let count = message_iter(resources, "multi_missions")
                .find(|row| i32_field(row, "multi_mission_id") == Some(*multi_mission_id))
                .and_then(|row| i32_field(row, "count"))
                .unwrap_or(0) as i64;
            rules
                .multi_mission_steps
                .iter()
                .filter(|step| step.multi_mission_id == *multi_mission_id && step.count <= count)
                .map(|step| step.step)
                .max()
                .unwrap_or(0)
        }
        ResourceObjective::ShipLevel => message_iter(resources, "ships")
            .flat_map(|ship| {
                rules
                    .ship_levels
                    .iter()
                    .filter(move |level| level.exp <= i32_field(ship, "exp").unwrap_or(0))
            })
            .map(|level| level.level)
            .max()
            .unwrap_or(0),
        ResourceObjective::ShipParty => {
            i32::from(message_iter(resources, "ship_parties").any(|row| {
                !i32_list(row, "character_ids").is_empty()
                    || optional_i32_field(row, "main_memoria_entity_id").is_some()
                    || !i32_list(row, "sub_memoria_entity_ids").is_empty()
            }))
        }
    }
}

pub(crate) fn mission_count(resources: &DynamicMessage, rule: &MissionRule) -> i32 {
    rule.total_task_condition_id
        .map(|id| total_task_count(resources, id))
        .unwrap_or_else(|| {
            message_iter(resources, "missions")
                .find(|s| i32_field(s, "mission_id") == Some(rule.id))
                .and_then(|s| i32_field(s, "count"))
                .unwrap_or(0)
        })
}

pub(crate) fn received(resources: &DynamicMessage, id: i32) -> usize {
    message_iter(resources, "missions")
        .find(|s| i32_field(s, "mission_id") == Some(id))
        .and_then(|s| i32_field(s, "received_step_count"))
        .unwrap_or(0)
        .max(0) as usize
}

pub(crate) fn rewards(rules: &HomeRules, id: i32) -> Result<&[TutorialReward], StateError> {
    rules
        .reward_sets
        .iter()
        .find(|row| row.id == id)
        .map(|row| row.rewards.as_slice())
        .ok_or_else(|| StateError::RewardRules(format!("missing home reward set {id}")))
}
