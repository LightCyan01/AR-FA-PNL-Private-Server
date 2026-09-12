use crate::state::service::prelude::*;

use home::put;

pub(crate) fn value(message: &DynamicMessage, field: &str) -> i32 {
    i32_field(message, field).unwrap_or(0)
}

pub(crate) fn level(rules: &CharacterRules, character: &DynamicMessage) -> i32 {
    rules
        .levels
        .iter()
        .filter(|row| row.exp <= value(character, "exp"))
        .map(|row| row.level)
        .max()
        .unwrap_or(1)
}

pub(crate) fn fixed_max_character(
    proto: &ProtoRegistry,
    rules: &CharacterRules,
    id: i32,
    declared_level: i32,
) -> Result<DynamicMessage, StateError> {
    let spec = rules
        .characters
        .iter()
        .find(|c| c.id == id)
        .ok_or(StateError::InvalidRequest)?;
    let mut character = empty_message(proto, "blend.model.Character")?;
    character.set_field_by_name("character_id", Value::I32(id));
    character.set_field_by_name("rarity", Value::I32(spec.max_rarity));
    character.set_field_by_name("normal1_skill_rank", Value::I32(1));
    character.set_field_by_name("normal2_skill_rank", Value::I32(1));
    let mut resources = empty_message(proto, "blend.model.Resources")?;
    let mut changed = resources.clone();
    for (kind, ex) in [(1, false), (1, true), (2, false)] {
        if kind == 2
            && (spec.neo_growboard_id.is_none()
                || declared_level < rules.constants.required_character_level_for_neo)
        {
            continue;
        }
        let (board, _, _) = board(rules, spec, ex, kind)?;
        let page_limit = if kind == 2 {
            rules
                .rarities
                .iter()
                .find(|r| r.id == spec.max_rarity)
                .ok_or(StateError::InvalidRequest)?
                .growboard_neo_max_page as usize
        } else {
            board.page_ids.len()
        };
        for page in 1..=page_limit.min(board.page_ids.len()) {
            for panel in panels(rules, board, page as i32)? {
                panel_effect(
                    proto,
                    &mut resources,
                    &mut changed,
                    &mut character,
                    spec.role,
                    panel,
                    1,
                )?;
            }
        }
    }
    let limit =
        rules.constants.initial_character_level_limit + value(&character, "growboard_level_limit");
    let exp = rules
        .levels
        .iter()
        .find(|r| r.level == limit)
        .ok_or(StateError::InvalidRequest)?
        .exp;
    character.set_field_by_name("exp", Value::I32(exp));
    Ok(character)
}

pub(crate) fn board<'a>(
    rules: &'a CharacterRules,
    spec: &CharacterRule,
    is_ex: bool,
    kind: i32,
) -> Result<(&'a BoardRule, &'static str, &'static str), StateError> {
    let (id, page, bits) = match (kind, is_ex) {
        (1, false) => (
            spec.growboard_id,
            "growboard_current_page",
            "growboard_panel_bits",
        ),
        (1, true) => (
            spec.ex_growboard_id,
            "growboard_ex_current_page",
            "growboard_ex_panel_bits",
        ),
        (2, false) => (
            spec.neo_growboard_id.ok_or(StateError::InvalidRequest)?,
            "growboard_neo_current_page",
            "growboard_neo_panel_bits",
        ),
        _ => return Err(StateError::InvalidRequest),
    };
    let row = rules
        .boards
        .iter()
        .find(|row| row.id == id && row.board_type == kind && row.is_ex == is_ex)
        .ok_or(StateError::InvalidRequest)?;
    Ok((row, page, bits))
}

pub(crate) fn panels<'a>(
    rules: &'a CharacterRules,
    board: &BoardRule,
    page: i32,
) -> Result<Vec<&'a PanelRule>, StateError> {
    let id = board
        .page_ids
        .get(usize::try_from(page - 1).map_err(|_| StateError::InvalidRequest)?)
        .ok_or(StateError::InvalidRequest)?;
    let page = rules
        .pages
        .iter()
        .find(|row| row.id == *id)
        .ok_or(StateError::InvalidRequest)?;
    page.panel_ids
        .iter()
        .map(|id| {
            rules
                .panels
                .iter()
                .find(|row| row.id == *id)
                .ok_or(StateError::InvalidRequest)
        })
        .collect()
}

pub(crate) fn board_complete(
    rules: &CharacterRules,
    spec: &CharacterRule,
    character: &DynamicMessage,
    is_ex: bool,
) -> Result<bool, StateError> {
    let (board, page_field, bits_field) = board(rules, spec, is_ex, 1)?;
    let last = board.page_ids.len() as i32;
    Ok(value(character, page_field) == last
        && value(character, bits_field) == (1 << panels(rules, board, last)?.len()) - 1)
}

pub(crate) fn panel_effect(
    proto: &ProtoRegistry,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    character: &mut DynamicMessage,
    role: i32,
    panel: &PanelRule,
    sign: i32,
) -> Result<(), StateError> {
    let stat = match panel.status_type {
        Some(1) => "hp",
        Some(2) => "speed",
        Some(3) => "attack",
        Some(4) => "magic",
        Some(5) => "defense",
        Some(6) => "mental",
        _ => "",
    };
    if panel.panel_type == 2 {
        if stat.is_empty() {
            return Err(StateError::InvalidRequest);
        }
        let mut row = message_list(resources, "growboard_role_rates")
            .into_iter()
            .find(|row| value(row, "role") == role)
            .unwrap_or(empty_message(proto, "blend.model.GrowboardRoleRate")?);
        row.set_field_by_name("role", Value::I32(role));
        let mut rate = member_status(&row, "rate")?;
        let next = value(&rate, stat)
            .checked_add(sign * panel.value)
            .filter(|v| *v >= 0)
            .ok_or(StateError::InvalidRequest)?;
        rate.set_field_by_name(stat, Value::I32(next));
        row.set_field_by_name("rate", Value::Message(rate));
        put(resources, "growboard_role_rates", "role", row.clone());
        put(changed, "growboard_role_rates", "role", row);
        return Ok(());
    }
    let field = match panel.panel_type {
        1 if !stat.is_empty() => format!("growboard_{stat}"),
        3 => "growboard_all_status_rate".into(),
        4 => "normal1_skill_rank".into(),
        5 => "normal2_skill_rank".into(),
        6 => "growboard_trait_rank_weight_bonus".into(),
        7 => "growboard_level_limit".into(),
        8 => "board_ability1_rank".into(),
        9 => "board_ability2_rank".into(),
        10 => "board_ability3_rank".into(),
        _ => return Err(StateError::InvalidRequest),
    };
    let next = value(character, &field)
        .checked_add(sign * panel.value)
        .filter(|v| *v >= 0)
        .ok_or(StateError::InvalidRequest)?;
    character.set_field_by_name(&field, Value::I32(next));
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn growboard(
    proto: &ProtoRegistry,
    rules: &CharacterRules,
    spec: &CharacterRule,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    character: &mut DynamicMessage,
    route: &str,
    request: &DynamicMessage,
) -> Result<i32, StateError> {
    let kind = optional_i32_field(request, "board_type").unwrap_or(1);
    let is_ex = bool_field(request, "is_ex");
    let (board, page_field, bits_field) = board(rules, spec, is_ex, kind)?;
    let current = value(character, page_field);
    let old_bits = value(character, bits_field);
    let status = status_message(resources)?;
    let limit = if kind == 2 {
        if current == 1 && old_bits & 1 == 0 {
            let research = message_list(resources, "research_groups")
                .iter()
                .filter(|row| {
                    rules
                        .development_research_group_ids
                        .contains(&value(row, "group_id"))
                })
                .map(|row| value(row, "level"))
                .max()
                .unwrap_or(0);
            if level(rules, character) < rules.constants.required_character_level_for_neo
                || research < rules.constants.required_dev_research_level_for_neo
                || !board_complete(rules, spec, character, false)?
                || !board_complete(rules, spec, character, true)?
            {
                return Err(StateError::InvalidRequest);
            }
        }
        rules
            .rarities
            .iter()
            .find(|row| row.id == value(character, "rarity"))
            .ok_or(StateError::InvalidRequest)?
            .growboard_neo_max_page
            .min(board.page_ids.len() as i32)
    } else if is_ex {
        board.page_ids.len() as i32
    } else {
        value(&status, "growboard_max_page").min(board.page_ids.len() as i32)
    };
    if current <= 0 || current > limit {
        return Err(StateError::InvalidRequest);
    }
    let target = if route.ends_with("/growboard_page_release") {
        current + 1
    } else {
        value(request, "target_page")
    };
    let incoming = if route.ends_with("/growboard_page_release") {
        let current_panels = panels(rules, board, current)?;
        if old_bits != (1 << current_panels.len()) - 1 {
            return Err(StateError::InvalidRequest);
        }
        0
    } else {
        value(request, "panel_bits")
    };
    if target < current || target > limit || incoming < 0 {
        return Err(StateError::InvalidRequest);
    }
    let selected = panels(rules, board, target)?;
    if incoming >> selected.len() != 0 {
        return Err(StateError::InvalidRequest);
    }
    let new_bits = if target == current {
        old_bits | incoming
    } else {
        incoming
    };
    if target == current && new_bits == old_bits {
        return Err(StateError::InvalidRequest);
    }
    if is_ex && new_bits != 0 && new_bits & 1 == 0 {
        return Err(StateError::InvalidRequest);
    }
    let mut purchased = 0;
    for number in current..=target {
        for (index, panel) in panels(rules, board, number)?.iter().enumerate() {
            let bit = 1 << index;
            let already = number == current && old_bits & bit != 0;
            let wanted = number < target || new_bits & bit != 0;
            if wanted && !already {
                spend(proto, resources, changed, &panel.item_costs)?;
                spend(
                    proto,
                    resources,
                    changed,
                    &[RuleCost {
                        resource_type: 3,
                        id: 1,
                        quantity: panel.cole_cost,
                    }],
                )?;
                panel_effect(proto, resources, changed, character, spec.role, panel, 1)?;
                purchased += 1;
            }
        }
    }
    character.set_field_by_name(page_field, Value::I32(target));
    character.set_field_by_name(bits_field, Value::I32(new_bits));
    Ok(purchased)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn reset(
    proto: &ProtoRegistry,
    rules: &CharacterRules,
    spec: &CharacterRule,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    response: &mut DynamicMessage,
    character: &mut DynamicMessage,
    only_level: bool,
) -> Result<(), StateError> {
    let old_exp = value(character, "exp");
    if old_exp == 0
        && (only_level
            || (value(character, "growboard_current_page") == 1
                && value(character, "growboard_panel_bits") == 0
                && value(character, "growboard_ex_current_page") == 1
                && value(character, "growboard_ex_panel_bits") == 0))
    {
        return Err(StateError::InvalidRequest);
    }
    if level(rules, character)
        >= rules
            .constants
            .character_enhancement_reset_item_required_level
    {
        spend(
            proto,
            resources,
            changed,
            &[RuleCost {
                resource_type: 5,
                id: rules.constants.character_enhancement_reset_item_id,
                quantity: 1,
            }],
        )?;
    }
    let mut refund = BTreeMap::<(i32, i32), i32>::new();
    let mut remainder = old_exp;
    let mut exp_items: Vec<_> = rules.exp_items.iter().collect();
    exp_items.sort_by_key(|row| std::cmp::Reverse(row.value));
    for item in exp_items {
        let quantity = remainder / item.value;
        if quantity > 0 {
            refund.insert((5, item.id), quantity);
        }
        remainder %= item.value;
    }
    let cole = i64::from(old_exp - remainder)
        * i64::from(rules.constants.character_enhancement_cole_per_exp_rate)
        / 100;
    refund.insert(
        (3, 1),
        i32::try_from(cole).map_err(|_| StateError::InvalidRequest)?,
    );
    if !only_level {
        for is_ex in [false, true] {
            let (board, page_field, bits_field) = board(rules, spec, is_ex, 1)?;
            let current = value(character, page_field);
            let bits = value(character, bits_field);
            for page in 1..=current {
                for (index, panel) in panels(rules, board, page)?.iter().enumerate() {
                    if page == current && bits & (1 << index) == 0 {
                        continue;
                    }
                    *refund.entry((3, 1)).or_default() += panel.cole_cost;
                    for cost in &panel.item_costs {
                        *refund.entry((cost.resource_type, cost.id)).or_default() += cost.quantity;
                    }
                    panel_effect(proto, resources, changed, character, spec.role, panel, -1)?;
                }
            }
            character.set_field_by_name(page_field, Value::I32(1));
            character.set_field_by_name(bits_field, Value::I32(0));
        }
    }
    character.set_field_by_name("exp", Value::I32(0));
    let rewards: Vec<_> = refund
        .into_iter()
        .filter(|(_, quantity)| *quantity > 0)
        .map(|((resource_type, id), quantity)| TutorialReward {
            resource_type,
            id,
            quantity,
            resource_params: None,
        })
        .collect();
    let mut delta = empty_message(proto, "blend.model.Resources")?;
    let rewards = apply_quest_rewards(proto, resources, &mut delta, &rewards)?;
    for item in message_list(&delta, "items") {
        put(changed, "items", "item_id", item);
    }
    if delta.has_field_by_name("status") {
        changed.set_field_by_name(
            "status",
            delta.get_field_by_name("status").unwrap().into_owned(),
        );
    }
    response.set_field_by_name("rewards", Value::List(rewards));
    Ok(())
}

pub(crate) fn spend(
    proto: &ProtoRegistry,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    costs: &[RuleCost],
) -> Result<(), StateError> {
    for cost in costs {
        let cost = TutorialRecipeCost {
            resource_type: cost.resource_type,
            id: cost.id,
            quantity: cost.quantity,
        };
        if let Some((field, row)) = pay_resource_cost(proto, resources, Some(&cost))? {
            match field {
                "items" => put(changed, field, "item_id", row),
                "character_pieces" => put(changed, field, "character_id", row),
                _ => changed.set_field_by_name(field, Value::Message(row)),
            }
        }
    }
    Ok(())
}

pub(crate) fn valid_equipment(
    rules: &CharacterRules,
    resources: &DynamicMessage,
    entity: i32,
    slot: i32,
) -> bool {
    entity > 0
        && message_list(resources, "equipment_tools")
            .iter()
            .any(|tool| {
                value(tool, "entity_id") == entity
                    && rules
                        .equipment_tools
                        .iter()
                        .any(|rule| rule.id == value(tool, "tool_id") && rule.slot_type == slot)
            })
}

pub(crate) fn favorite(
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    route: &str,
    request: &DynamicMessage,
) -> Result<(), StateError> {
    let mut profile = member_status(resources, "profile")?;
    if route == "/profile/update_favorite_character" {
        let id = value(request, "character_id");
        resource_character(resources, id)?;
        profile.set_field_by_name("favorite_character_id", Value::I32(id));
    } else if route == "/profile/update_favorite_battle_tools" {
        let ids = i32_list(request, "battle_tool_entity_ids");
        if ids.iter().any(|id| {
            !message_list(resources, "battle_tools")
                .iter()
                .any(|t| i32_field(t, "entity_id") == Some(*id))
        }) || ids.len() > 5
        {
            return Err(StateError::InvalidRequest);
        }
        profile.set_field_by_name(
            "favorite_battle_tool_entity_ids",
            Value::List(ids.into_iter().map(Value::I32).collect()),
        );
    } else if route == "/profile/update_chara_home_favorite_character_list" {
        let ids = i32_list(request, "character_id");
        if ids
            .iter()
            .any(|id| resource_character(resources, *id).is_err())
            || ids.len() > 5
        {
            return Err(StateError::InvalidRequest);
        }
        profile.set_field_by_name(
            "chara_home_favorite_character_ids",
            Value::List(ids.into_iter().map(Value::I32).collect()),
        );
    } else {
        let mut selected = Vec::new();
        for position in 1..=5 {
            let input = format!("character{position}_id");
            let output = format!("favorite_party_character{position}_id");
            if let Some(id) = optional_i32_field(request, &input) {
                resource_character(resources, id)?;
                if selected.contains(&id) {
                    return Err(StateError::InvalidRequest);
                }
                selected.push(id);
                profile.set_field_by_name(
                    &output,
                    request.get_field_by_name(&input).unwrap().into_owned(),
                );
            } else {
                profile.clear_field_by_name(&output);
            }
        }
    }
    resources.set_field_by_name("profile", Value::Message(profile.clone()));
    changed.set_field_by_name("profile", Value::Message(profile));
    Ok(())
}

pub(crate) fn preset(
    proto: &ProtoRegistry,
    rules: &CharacterRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    route: &str,
    request: &DynamicMessage,
) -> Result<(), StateError> {
    let number = value(request, "number");
    if !(1..=rules.constants.equipment_preset_count).contains(&number) {
        return Err(StateError::InvalidRequest);
    }
    let mut preset = match message_list(resources, "equipment_presets")
        .into_iter()
        .find(|row| value(row, "number") == number)
    {
        Some(row) => row,
        None => {
            let mut row = empty_message(proto, "blend.model.EquipmentPreset")?;
            row.set_field_by_name("number", Value::I32(number));
            let name = rules
                .constants
                .equipment_preset_initial_name_i18n
                .get("ja")
                .ok_or_else(|| StateError::CharacterRules("missing preset name".into()))?;
            row.set_field_by_name(
                "name",
                Value::String(name.replace("{}", &number.to_string())),
            );
            row
        }
    };
    if route.ends_with("/update_name") {
        let name = request
            .get_field_by_name("name")
            .and_then(|v| v.as_str().map(str::to_owned))
            .ok_or(StateError::InvalidRequest)?;
        if name.trim().is_empty()
            || name.encode_utf16().count()
                > rules.constants.equipment_preset_max_name_length as usize
            || name.chars().any(char::is_control)
        {
            return Err(StateError::InvalidRequest);
        }
        preset.set_field_by_name("name", Value::String(name));
    } else {
        let slot = if route.ends_with("/equip") {
            value(request, "slot_type")
        } else {
            0
        };
        if route.ends_with("/equip") && !(1..=3).contains(&slot) {
            return Err(StateError::InvalidRequest);
        }
        for (field, position) in [
            ("slot1_equipment_tool_entity_id", 1),
            ("slot2_equipment_tool_entity_id", 2),
            ("slot3_equipment_tool_entity_id", 3),
            ("memoria_entity_id", 0),
        ] {
            if !route.ends_with("/bulk_set") && slot != position {
                continue;
            }
            let input = if route.ends_with("/equip") {
                "equipment_tool_entity_id"
            } else {
                field
            };
            if let Some(id) = optional_i32_field(request, input) {
                if (position == 0 && (id <= 0 || !owned_memoria(resources, id)))
                    || (position != 0 && !valid_equipment(rules, resources, id, position))
                {
                    return Err(StateError::InvalidRequest);
                }
                preset.set_field_by_name(field, Value::Message(int32_value(proto, id)?));
            } else {
                preset.clear_field_by_name(field);
            }
        }
    }
    put(resources, "equipment_presets", "number", preset.clone());
    put(changed, "equipment_presets", "number", preset);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply(
    proto: &ProtoRegistry,
    rules: &CharacterRules,
    home_rules: &home::HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    response: &mut DynamicMessage,
    route: &str,
    request: &DynamicMessage,
    now: i64,
) -> Result<(), StateError> {
    let id = value(request, "character_id");
    let spec = rules
        .characters
        .iter()
        .find(|row| row.id == id)
        .ok_or(StateError::InvalidRequest)?;
    let mut character = resource_character(resources, id)?;
    match route {
        "/character/equip" | "/character/memoria_set" => {
            let mut bulk = empty_message(proto, "blend.api.CharacterBulkSetRequest")?;
            bulk.set_field_by_name("character_id", Value::I32(id));
            for field in [
                "slot1_equipment_tool_entity_id",
                "slot2_equipment_tool_entity_id",
                "slot3_equipment_tool_entity_id",
                "memoria_entity_id",
            ] {
                if character.has_field_by_name(field) {
                    bulk.set_field_by_name(
                        field,
                        character.get_field_by_name(field).unwrap().into_owned(),
                    );
                }
            }
            let (input, output) = if route.ends_with("/equip") {
                let slot = value(request, "slot_type");
                if !(1..=3).contains(&slot) {
                    return Err(StateError::InvalidRequest);
                }
                (
                    "equipment_tool_entity_id",
                    format!("slot{slot}_equipment_tool_entity_id"),
                )
            } else {
                ("memoria_entity_id", "memoria_entity_id".into())
            };
            if request.has_field_by_name(input) {
                bulk.set_field_by_name(
                    &output,
                    request.get_field_by_name(input).unwrap().into_owned(),
                );
            } else {
                bulk.clear_field_by_name(&output);
            }
            return home::apply_character_bulk_set(home_rules, resources, changed, &bulk);
        }
        "/character/enhance" => {
            let items = message_list(request, "consumed_items");
            if items.is_empty() {
                return Err(StateError::InvalidRequest);
            }
            let cap = value(&character, "growboard_level_limit")
                .checked_add(value(&character, "level_limit_increase_value"))
                .ok_or(StateError::InvalidRequest)?
                .max(rules.constants.initial_character_level_limit);
            let max_exp = rules
                .levels
                .iter()
                .find(|row| row.level == cap + 1)
                .ok_or(StateError::InvalidRequest)?
                .exp
                - 1;
            let old = value(&character, "exp");
            if old >= max_exp {
                return Err(StateError::InvalidRequest);
            }
            let mut exp = 0_i64;
            let mut costs = Vec::new();
            for item in items {
                let item_id = value(&item, "item_id");
                let quantity = value(&item, "quantity");
                let rule = rules
                    .exp_items
                    .iter()
                    .find(|row| row.id == item_id)
                    .ok_or(StateError::InvalidRequest)?;
                if quantity <= 0 {
                    return Err(StateError::InvalidRequest);
                }
                exp = exp
                    .checked_add(i64::from(rule.value) * i64::from(quantity))
                    .ok_or(StateError::InvalidRequest)?;
                costs.push(RuleCost {
                    resource_type: 5,
                    id: item_id,
                    quantity,
                });
            }
            let cole = exp
                .checked_mul(i64::from(
                    rules.constants.character_enhancement_cole_per_exp_rate,
                ))
                .ok_or(StateError::InvalidRequest)?
                / 100;
            costs.push(RuleCost {
                resource_type: 3,
                id: 1,
                quantity: i32::try_from(cole).map_err(|_| StateError::InvalidRequest)?,
            });
            spend(proto, resources, changed, &costs)?;
            character.set_field_by_name(
                "exp",
                Value::I32((i64::from(old) + exp).min(i64::from(max_exp)) as i32),
            );
        }
        "/character/rarity_enhance" => {
            let count = value(request, "rarity_count");
            let old = value(&character, "rarity");
            let target = old.checked_add(count).ok_or(StateError::InvalidRequest)?;
            if count <= 0 || old < spec.initial_rarity || target > spec.max_rarity {
                return Err(StateError::InvalidRequest);
            }
            let table = rules
                .rarities
                .iter()
                .find(|row| row.id == spec.initial_rarity)
                .ok_or(StateError::InvalidRequest)?;
            for step in old..target {
                let index = (step - spec.initial_rarity) as usize;
                let quantity = *table
                    .rarity_enhance_piece_costs
                    .get(index)
                    .ok_or(StateError::InvalidRequest)?;
                spend(
                    proto,
                    resources,
                    changed,
                    &[RuleCost {
                        resource_type: 8,
                        id,
                        quantity,
                    }],
                )?;
                let extra = table
                    .rarity_enhance_additional_costs
                    .get(index)
                    .ok_or(StateError::InvalidRequest)?;
                if extra.required {
                    spend(proto, resources, changed, &extra.costs)?;
                }
            }
            character.set_field_by_name("rarity", Value::I32(target));
            let max_page = rules
                .rarities
                .iter()
                .find(|row| row.id == target)
                .ok_or(StateError::InvalidRequest)?
                .growboard_neo_max_page;
            character.set_field_by_name("growboard_neo_max_page", Value::I32(max_page));
        }
        "/character/level_limit_release" => {
            let current = value(&character, "level_limit_increase_value");
            let mut total = 0;
            let mut releases: Vec<_> = rules.level_limit_releases.iter().collect();
            releases.sort_by_key(|row| row.id);
            let next = releases
                .into_iter()
                .find(|row| {
                    if total == current {
                        true
                    } else {
                        total += row.value;
                        false
                    }
                })
                .ok_or(StateError::InvalidRequest)?;
            spend(proto, resources, changed, &next.item_costs)?;
            character.set_field_by_name(
                "level_limit_increase_value",
                Value::I32(current + next.value),
            );
        }
        "/character/skill_evolve" | "/character/skill_lock_release" => {
            let skill = value(request, "skill_type");
            let prefix = match skill {
                1 => "normal1",
                2 => "normal2",
                3 => "burst",
                _ => return Err(StateError::InvalidRequest),
            };
            let evolve = route.ends_with("/skill_evolve");
            let field = format!(
                "is_{prefix}_skill_{}",
                if evolve { "evolved" } else { "locked" }
            );
            if bool_field(&character, &field) == evolve {
                return Err(StateError::InvalidRequest);
            }
            let costs = if evolve {
                if value(&character, "rarity") < rules.constants.skill_evolve_enable_rarity {
                    return Err(StateError::InvalidRequest);
                }
                if [
                    "is_normal1_skill_locked",
                    "is_normal2_skill_locked",
                    "is_burst_skill_locked",
                ]
                .iter()
                .any(|field| bool_field(&character, field))
                {
                    return Err(StateError::InvalidRequest);
                }
                let row = rules
                    .skill_evolve
                    .iter()
                    .find(|row| row.character_id == id)
                    .ok_or(StateError::InvalidRequest)?;
                match skill {
                    1 => &row.normal1_skill_evolve_costs,
                    2 => &row.normal2_skill_evolve_costs,
                    _ => &row.burst_skill_evolve_costs,
                }
            } else {
                let row = rules
                    .skill_lock_release
                    .iter()
                    .find(|row| row.character_id == id)
                    .ok_or(StateError::InvalidRequest)?;
                match skill {
                    1 => &row.normal1_skill_lock_release_costs,
                    2 => &row.normal2_skill_lock_release_costs,
                    _ => &row.burst_skill_lock_release_costs,
                }
            };
            if costs.is_empty() {
                return Err(StateError::InvalidRequest);
            }
            spend(proto, resources, changed, costs)?;
            character.set_field_by_name(&field, Value::Bool(evolve));
        }
        "/character/skin_set" => {
            if let Some(skin) = optional_i32_field(request, "character_skin_id") {
                let rule = rules
                    .skins
                    .iter()
                    .find(|row| row.id == skin)
                    .ok_or(StateError::InvalidRequest)?;
                if (rule.character_id != Some(id)
                    && rule.base_character_id != Some(spec.base_character_id))
                    || rule.reject_character_ids.contains(&id)
                    || rule.start_at.is_some_and(|time| now < time)
                    || rule.end_at.is_some_and(|time| now >= time)
                    || (spec.default_skin_id != Some(skin)
                        && !message_list(resources, "character_skins")
                            .iter()
                            .any(|row| value(row, "character_skin_id") == skin))
                {
                    return Err(StateError::InvalidRequest);
                }
                character.set_field_by_name("skin_id", Value::Message(int32_value(proto, skin)?));
            } else {
                character.clear_field_by_name("skin_id");
            }
        }
        "/character/growboard_page_release" | "/character/growboard_bulk_release" => {
            let purchased = growboard(
                proto,
                rules,
                spec,
                resources,
                changed,
                &mut character,
                route,
                request,
            )?;
            // Count newly purchased panels, not calls/page navigation or reset refunds.
            let event = if bool_field(request, "is_ex") {
                "growboard_ex"
            } else {
                "growboard"
            };
            if purchased > 0 {
                home::advance_missions(
                    proto,
                    home_rules,
                    resources,
                    changed,
                    now,
                    Some((event, purchased)),
                )?;
            }
        }
        "/character/enhancement_reset" => {
            reset(
                proto,
                rules,
                spec,
                resources,
                changed,
                response,
                &mut character,
                bool_field(request, "only_reset_level"),
            )?;
        }
        _ => return Err(StateError::InvalidRequest),
    }
    upsert_character(resources, character.clone(), false);
    put(changed, "characters", "character_id", character);
    Ok(())
}
