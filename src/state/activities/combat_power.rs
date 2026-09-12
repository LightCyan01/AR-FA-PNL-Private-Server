use super::prelude::*;

/// Reproduces the installed client's party-power display from authoritative
/// final stats and master-data coefficients. The client does not accept this
/// value from the request; it derives it from the saved party.
pub(crate) fn account_party_combat_power(
    activity_rules: &ActivityRules,
    tutorial_rules: &TutorialRules,
    atelier_rules: &atelier::AtelierRules,
    character_rules: &CharacterRules,
    resources: &DynamicMessage,
    party_number: i32,
) -> Result<i32, StateError> {
    let (members, _) = resolve_account_party(
        tutorial_rules,
        atelier_rules,
        character_rules,
        resources,
        party_number,
    )?;
    let data = activity_rules
        .combat_power
        .as_object()
        .ok_or_else(|| StateError::MasterData("combat power catalog is missing".into()))?;
    let base = json_i64(data, "base")?;
    let hp_coefficient = json_i64(data, "hp_coefficient_permyriad")?;
    let status_coefficient = json_i64(data, "status_coefficient_permyriad")?;
    let coefficients = data
        .get("skill_level_sum_coefficients_permyriad")
        .and_then(Json::as_array)
        .ok_or_else(|| {
            StateError::MasterData("combat power skill coefficients are missing".into())
        })?;
    let mut total = 0i64;
    for member in members {
        let character = resource_character(resources, member.character_id)?;
        let party_member = message_list(resources, "party_members")
            .into_iter()
            .find(|row| {
                i32_field(row, "party_type") == Some(1)
                    && i32_field(row, "number") == Some(party_number)
                    && i32_field(row, "position") == Some(member.position)
            })
            .ok_or(StateError::InvalidRequest)?;
        let character_rule = character_rules
            .characters
            .iter()
            .find(|rule| rule.id == member.character_id)
            .ok_or(StateError::InvalidRequest)?;
        let stats = member.integrated_stats.ok_or(StateError::InvalidRequest)?;
        let weighted = i64::from(stats.hp)
            .checked_mul(hp_coefficient)
            .and_then(|value| {
                let other = i64::from(stats.speed)
                    .checked_add(i64::from(stats.attack))?
                    .checked_add(i64::from(stats.magic))?
                    .checked_add(i64::from(stats.defense))?
                    .checked_add(i64::from(stats.mental))?;
                value.checked_add(other.checked_mul(status_coefficient)?)
            })
            .ok_or(StateError::InvalidRequest)?;
        let normal_one = i32_field(&character, "normal1_skill_rank")
            .unwrap_or(1)
            .max(1);
        let normal_two = i32_field(&character, "normal2_skill_rank")
            .unwrap_or(1)
            .max(1);
        let burst = (member.rarity - 2).max(1);
        let coefficient_index = usize::try_from(
            normal_one
                .checked_add(normal_two)
                .and_then(|value| value.checked_add(burst))
                .and_then(|value| value.checked_sub(3))
                .ok_or(StateError::InvalidRequest)?,
        )
        .map_err(|_| StateError::InvalidRequest)?;
        let skill_coefficient = coefficients
            .get(coefficient_index)
            .and_then(Json::as_i64)
            .ok_or(StateError::InvalidRequest)?
            .checked_add(10_000)
            .ok_or(StateError::InvalidRequest)?;
        let character_power = base
            .checked_add(ceil_div(
                weighted
                    .checked_mul(skill_coefficient)
                    .ok_or(StateError::InvalidRequest)?,
                100_000_000,
            )?)
            .ok_or(StateError::InvalidRequest)?;
        let equipment_power = equipment_power(
            data,
            resources,
            &party_member,
            &character,
            character_rule.role,
            &character_rule.attack_attributes,
        )?;
        total = total
            .checked_add(character_power)
            .and_then(|value| value.checked_add(equipment_power))
            .ok_or(StateError::InvalidRequest)?;
    }
    i32::try_from(total).map_err(|_| StateError::InvalidRequest)
}

fn equipment_power(
    data: &serde_json::Map<String, Json>,
    resources: &DynamicMessage,
    party_member: &DynamicMessage,
    character: &DynamicMessage,
    role: i32,
    attack_attributes: &[i32],
) -> Result<i64, StateError> {
    let tools = data
        .get("tools")
        .and_then(Json::as_object)
        .ok_or_else(|| StateError::MasterData("combat power tools are missing".into()))?;
    let traits = data
        .get("traits")
        .and_then(Json::as_object)
        .ok_or_else(|| StateError::MasterData("combat power traits are missing".into()))?;
    let quality = data
        .get("quality")
        .and_then(Json::as_object)
        .ok_or_else(|| StateError::MasterData("combat power quality is missing".into()))?;
    let mut total = 0i64;
    for slot in [
        "slot1_equipment_tool_entity_id",
        "slot2_equipment_tool_entity_id",
        "slot3_equipment_tool_entity_id",
    ] {
        let Some(entity_id) =
            optional_i32_field(party_member, slot).or_else(|| optional_i32_field(character, slot))
        else {
            continue;
        };
        let entity = message_list(resources, "equipment_tools")
            .into_iter()
            .find(|row| i32_field(row, "entity_id") == Some(entity_id))
            .ok_or(StateError::InvalidRequest)?;
        let tool_id = i32_field(&entity, "tool_id").ok_or(StateError::InvalidRequest)?;
        let tool = tools
            .get(&tool_id.to_string())
            .and_then(Json::as_object)
            .ok_or(StateError::InvalidRequest)?;
        if compatible(tool, role, attack_attributes) {
            total = total
                .checked_add(
                    tool.get("role_bonuses")
                        .and_then(Json::as_object)
                        .and_then(|values| values.get(&role.to_string()))
                        .and_then(Json::as_i64)
                        .unwrap_or(0),
                )
                .and_then(|value| {
                    let bonus = attack_attributes
                        .iter()
                        .filter_map(|attribute| {
                            tool.get("attribute_bonuses")
                                .and_then(Json::as_object)
                                .and_then(|values| values.get(&attribute.to_string()))
                                .and_then(Json::as_i64)
                        })
                        .max()
                        .unwrap_or(0);
                    value.checked_add(bonus)
                })
                .ok_or(StateError::InvalidRequest)?;
        }
        let tool_group = tool
            .get("quality_group_id")
            .and_then(Json::as_i64)
            .ok_or(StateError::InvalidRequest)?;
        for trait_row in message_list(&entity, "traits") {
            let trait_id = i32_field(&trait_row, "id").ok_or(StateError::InvalidRequest)?;
            let rank = i32_field(&trait_row, "rank")
                .filter(|rank| *rank > 0)
                .ok_or(StateError::InvalidRequest)?;
            let trait_groups = traits
                .get(&trait_id.to_string())
                .and_then(Json::as_object)
                .and_then(|row| row.get("quality_group_ids"))
                .and_then(Json::as_array)
                .ok_or(StateError::InvalidRequest)?;
            let trait_group = trait_groups
                .get(usize::try_from(rank - 1).map_err(|_| StateError::InvalidRequest)?)
                .and_then(Json::as_i64)
                .ok_or(StateError::InvalidRequest)?;
            total = total
                .checked_add(
                    quality
                        .get(&format!("{tool_group}:{trait_group}"))
                        .and_then(Json::as_i64)
                        .ok_or(StateError::InvalidRequest)?,
                )
                .ok_or(StateError::InvalidRequest)?;
        }
    }
    Ok(total)
}

fn compatible(tool: &serde_json::Map<String, Json>, role: i32, attributes: &[i32]) -> bool {
    let roles = tool
        .get("roles")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
        .filter_map(Json::as_i64)
        .collect::<Vec<_>>();
    let tool_attributes = tool
        .get("attack_attributes")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
        .filter_map(Json::as_i64)
        .collect::<Vec<_>>();
    (roles.is_empty() || roles.contains(&i64::from(role)))
        && (tool_attributes.is_empty()
            || attributes
                .iter()
                .any(|attribute| tool_attributes.contains(&i64::from(*attribute))))
}

fn ceil_div(value: i64, divisor: i64) -> Result<i64, StateError> {
    if value < 0 || divisor <= 0 {
        return Err(StateError::InvalidRequest);
    }
    value
        .checked_add(divisor - 1)
        .map(|value| value / divisor)
        .ok_or(StateError::InvalidRequest)
}

fn json_i64(data: &serde_json::Map<String, Json>, key: &str) -> Result<i64, StateError> {
    data.get(key)
        .and_then(Json::as_i64)
        .ok_or_else(|| StateError::MasterData(format!("combat power {key} is missing")))
}
