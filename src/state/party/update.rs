use crate::state::service::prelude::*;

pub(crate) fn party_member_message(
    proto: &ProtoRegistry,
    character_id: Option<i32>,
    equipment: [Option<i32>; 3],
    memoria: Option<i32>,
    position: i32,
) -> Result<DynamicMessage, StateError> {
    let mut member = empty_message(proto, "blend.model.PartyMember")?;
    member.set_field_by_name("party_type", Value::I32(1));
    member.set_field_by_name("number", Value::I32(1));
    member.set_field_by_name("position", Value::I32(position));
    if let Some(character_id) = character_id {
        member.set_field_by_name(
            "character_id",
            Value::Message(int32_value(proto, character_id)?),
        );
    }
    for (field, value) in [
        ("slot1_equipment_tool_entity_id", equipment[0]),
        ("slot2_equipment_tool_entity_id", equipment[1]),
        ("slot3_equipment_tool_entity_id", equipment[2]),
        ("memoria_entity_id", memoria),
    ] {
        if let Some(value) = value {
            member.set_field_by_name(field, Value::Message(int32_value(proto, value)?));
        }
    }
    Ok(member)
}

pub(crate) fn party_message(
    proto: &ProtoRegistry,
    party_type: i32,
    number: i32,
    tool_ids: &[i32],
    leader_position: i32,
) -> Result<DynamicMessage, StateError> {
    let mut party = empty_message(proto, "blend.model.Party")?;
    party.set_field_by_name("party_type", Value::I32(party_type));
    party.set_field_by_name("number", Value::I32(number));
    party.set_field_by_name(
        "battle_tool_entity_ids",
        Value::List(tool_ids.iter().copied().map(Value::I32).collect()),
    );
    party.set_field_by_name("leader_position", Value::I32(leader_position));
    Ok(party)
}

pub(crate) fn find_party(
    resources: &DynamicMessage,
    party_type: i32,
    number: i32,
) -> Option<DynamicMessage> {
    message_list(resources, "parties")
        .into_iter()
        .find(|party| {
            i32_field(party, "party_type") == Some(party_type)
                && i32_field(party, "number") == Some(number)
        })
}

pub(crate) fn replace_party(resources: &mut DynamicMessage, patch: DynamicMessage) {
    let mut parties = message_list(resources, "parties");
    let key = (i32_field(&patch, "party_type"), i32_field(&patch, "number"));
    if let Some(slot) = parties
        .iter_mut()
        .find(|party| (i32_field(party, "party_type"), i32_field(party, "number")) == key)
    {
        *slot = patch;
    } else {
        parties.push(patch);
    }
    resources.set_field_by_name(
        "parties",
        Value::List(parties.into_iter().map(Value::Message).collect()),
    );
}

pub(crate) fn replace_party_members(resources: &mut DynamicMessage, patches: &[DynamicMessage]) {
    let mut members = message_list(resources, "party_members");
    for patch in patches {
        let key = (
            i32_field(patch, "party_type"),
            i32_field(patch, "number"),
            i32_field(patch, "position"),
        );
        if let Some(slot) = members.iter_mut().find(|member| {
            (
                i32_field(member, "party_type"),
                i32_field(member, "number"),
                i32_field(member, "position"),
            ) == key
        }) {
            *slot = patch.clone();
        } else {
            members.push(patch.clone());
        }
    }
    resources.set_field_by_name(
        "party_members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );
}

pub(crate) fn reduce_party(
    proto: &ProtoRegistry,
    fresh_rules: &FreshStateRules,
    mut resources: DynamicMessage,
    request: &DynamicMessage,
    tools_only: bool,
) -> Result<ResourceMutation, StateError> {
    let party_type = i32_field(request, "party_type").unwrap_or(1);
    let number = i32_field(request, "number").unwrap_or(1);
    let character_rules = load_character_rules()?;
    let party_rule = character_rules
        .constants
        .party_params
        .iter()
        .find(|row| row.party_type == party_type)
        .ok_or(StateError::InvalidRequest)?;
    if !(1..=character_rules.constants.party_count_per_content).contains(&number) {
        return Err(StateError::InvalidRequest);
    }
    let current = match find_party(&resources, party_type, number) {
        Some(party) => party,
        None if !tools_only => party_message(proto, party_type, number, &[], 1)?,
        None => return Err(StateError::InvalidRequest),
    };
    let leader_position = i32_field(request, "leader_position")
        .or_else(|| i32_field(&current, "leader_position"))
        .unwrap_or(1);
    if !(1..=5).contains(&leader_position) {
        return Err(StateError::InvalidRequest);
    }
    let tool_ids = i32_list(request, "battle_tool_entity_ids");
    let max_tools = status_message(&resources)
        .ok()
        .and_then(|status| i32_field(&status, "party_max_battle_tool_count"))
        .unwrap_or(fresh_rules.constants.initial_party_max_battle_tool_count)
        + party_rule.battle_tool_count_offset;
    if tool_ids.len() > usize::try_from(max_tools.max(0)).unwrap_or(0)
        || tool_ids
            .iter()
            .any(|id| *id <= 0 || !owned_battle_tool(&resources, *id))
        || tool_ids
            .iter()
            .enumerate()
            .any(|(index, id)| tool_ids[..index].contains(id))
    {
        return Err(StateError::InvalidRequest);
    }

    let mut member_patches = Vec::new();
    if !tools_only {
        let requested_members = message_list(request, "members");
        if requested_members.is_empty() || requested_members.len() > 5 {
            return Err(StateError::InvalidRequest);
        }
        for position in 1..=5 {
            let source = requested_members
                .get(usize::try_from(position - 1).unwrap_or(0))
                .cloned();
            let Some(source) = source else {
                member_patches.push(party_member_message(
                    proto, None, [None; 3], None, position,
                )?);
                continue;
            };
            let character_id = optional_i32_field(&source, "character_id");
            let equipment = [
                optional_i32_field(&source, "slot1_equipment_tool_entity_id"),
                optional_i32_field(&source, "slot2_equipment_tool_entity_id"),
                optional_i32_field(&source, "slot3_equipment_tool_entity_id"),
            ];
            let memoria = optional_i32_field(&source, "memoria_entity_id");
            if character_id.is_some_and(|id| id <= 0 || !character_present(&resources, id))
                || equipment.iter().enumerate().any(|(slot, id)| {
                    id.is_some_and(|id| {
                        !character::valid_equipment(
                            character_rules,
                            &resources,
                            id,
                            slot as i32 + 1,
                        )
                    })
                })
                || memoria.is_some_and(|id| id <= 0 || !owned_memoria(&resources, id))
                || (character_id.is_none()
                    && (memoria.is_some() || equipment.iter().any(Option::is_some)))
            {
                return Err(StateError::InvalidRequest);
            }
            member_patches.push(party_member_message(
                proto,
                character_id,
                equipment,
                memoria,
                position,
            )?);
        }
        let occupied: Vec<i32> = member_patches
            .iter()
            .filter_map(|member| optional_i32_field(member, "character_id"))
            .collect();
        let leader_has_member = member_patches.iter().any(|member| {
            i32_field(member, "position") == Some(leader_position)
                && optional_i32_field(member, "character_id").is_some()
        });
        if occupied.is_empty()
            || occupied
                .iter()
                .enumerate()
                .any(|(index, id)| occupied[..index].contains(id))
            || !leader_has_member
        {
            return Err(StateError::InvalidRequest);
        }
    }

    for member in &mut member_patches {
        member.set_field_by_name("party_type", Value::I32(party_type));
        member.set_field_by_name("number", Value::I32(number));
    }

    let completes_memoria_tutorial = party_type == 1
        && number == 1
        && !tools_only
        && tutorial_step(&resources) == TUTORIAL_STEP_SECOND_SYNTHESIS
        && quest_clear_count(&resources, 101001014) == 1
        && quest_clear_count(&resources, 101001015) == 0
        && member_patches
            .iter()
            .any(|member| optional_i32_field(member, "memoria_entity_id").is_some());
    let party_patch = party_message(proto, party_type, number, &tool_ids, leader_position)?;
    replace_party(&mut resources, party_patch.clone());
    if !member_patches.is_empty() {
        replace_party_members(&mut resources, &member_patches);
    }

    let mut changed = empty_message(proto, "blend.model.Resources")?;
    changed.set_field_by_name("parties", Value::List(vec![Value::Message(party_patch)]));
    if !member_patches.is_empty() {
        changed.set_field_by_name(
            "party_members",
            Value::List(member_patches.into_iter().map(Value::Message).collect()),
        );
    }
    if completes_memoria_tutorial {
        let mut status = status_message(&resources)?;
        status.set_field_by_name("tutorial_step", Value::I32(TUTORIAL_STEP_MEMORIA_EQUIPPED));
        resources.set_field_by_name("status", Value::Message(status.clone()));
        changed.set_field_by_name("status", Value::Message(status));
    }
    let mut response = empty_message(proto, "blend.api.ChangedResourcesResponse")?;
    response.set_field_by_name("changed_resources", Value::Message(changed));
    Ok(ResourceMutation {
        resources,
        response,
    })
}
