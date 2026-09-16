use super::prelude::*;

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_skill_result(
    proto: &ProtoRegistry,
    target_id: i32,
    hp_damage: i64,
    hp_damage_for_critical: i64,
    hp_heal: i32,
    break_damage: i32,
    deals_damage: bool,
    guard: bool,
    killed: bool,
    critical: bool,
    broken: bool,
    weak: bool,
    resisted: bool,
) -> Result<DynamicMessage, StateError> {
    let mut result = empty_message(proto, "blend.model.BattleSkillResult")?;
    result.set_field_by_name("target_id", Value::I32(target_id));
    if deals_damage {
        result.set_field_by_name("hp_damage", Value::Message(wrapper_i64(proto, hp_damage)?));
        result.set_field_by_name(
            "hp_damage_for_critical",
            Value::Message(wrapper_i64(proto, hp_damage_for_critical)?),
        );
        result.set_field_by_name("barrier_damage", Value::Message(wrapper_i64(proto, 0)?));
    }
    if hp_heal > 0 {
        result.set_field_by_name("hp_heal", Value::Message(wrapper_i32(proto, hp_heal)?));
    }
    if break_damage > 0 {
        result.set_field_by_name(
            "break_damage",
            Value::Message(wrapper_i32(proto, break_damage)?),
        );
    }
    result.set_field_by_name("break_type", Value::EnumNumber(if broken { 2 } else { 0 }));
    result.set_field_by_name("is_critical", Value::Bool(critical));
    result.set_field_by_name("is_guard", Value::Bool(guard));
    result.set_field_by_name("is_killed", Value::Bool(killed));
    result.set_field_by_name("is_weak", Value::Bool(weak));
    result.set_field_by_name("is_resist", Value::Bool(resisted));
    result.set_field_by_name("is_invalid", Value::Bool(false));
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_action(
    proto: &ProtoRegistry,
    number: i32,
    state: DynamicMessage,
    actor_type: i32,
    actor_id: i32,
    skill: &TutorialSkill,
    target_id: i32,
    results: Vec<DynamicMessage>,
    before_skill_effect_results: Vec<DynamicMessage>,
    effect_results: Vec<DynamicMessage>,
    timeline_moves: Vec<DynamicMessage>,
    mode: i32,
    command_value: Option<i32>,
    total_damage: i64,
    protection: Option<super::effects::Protection>,
) -> Result<DynamicMessage, StateError> {
    let action_type = match mode {
        0 | 7 => 0,
        1 => 1,
        8 => 2,
        9 => 3,
        _ => return Err(StateError::InvalidRequest),
    };
    let mut action = empty_message(proto, "blend.model.BattleAction")?;
    action.set_field_by_name("number", Value::I32(number));
    action.set_field_by_name("state", Value::Message(state));
    action.set_field_by_name("action_type", Value::EnumNumber(action_type));
    action.set_field_by_name("actor_type", Value::EnumNumber(actor_type));
    action.set_field_by_name("actor_id", Value::I32(actor_id));
    action.set_field_by_name("skill_id", Value::I32(skill.id));
    action.set_field_by_name(
        "skill_type",
        Value::Message(wrapper_i32(proto, skill.skill_type)?),
    );
    action.set_field_by_name("main_target_id", Value::I32(target_id));
    action.set_field_by_name("skill_target_mode", Value::EnumNumber(0));
    action.set_field_by_name(
        "skill_results",
        Value::List(results.into_iter().map(Value::Message).collect()),
    );
    action.set_field_by_name(
        "before_skill_effect_results",
        Value::List(
            before_skill_effect_results
                .into_iter()
                .map(Value::Message)
                .collect(),
        ),
    );
    action.set_field_by_name(
        "effect_results",
        Value::List(effect_results.into_iter().map(Value::Message).collect()),
    );
    action.set_field_by_name(
        "timeline_moves",
        Value::List(timeline_moves.into_iter().map(Value::Message).collect()),
    );
    if let Some(value) = command_value {
        let field = match mode {
            1 => "battle_tool_number",
            7 => "active_skill_type",
            9 => "ship_tool_number",
            _ => "",
        };
        if !field.is_empty() {
            action.set_field_by_name(field, Value::Message(wrapper_i32(proto, value)?));
        }
    }
    action.set_field_by_name("is_battle_tool_mix", Value::Bool(mode == 8));
    action.set_field_by_name("total_dealt_hp_damage", Value::I64(total_damage));
    if let Some(protection) = protection {
        action.set_field_by_name(
            "protector_id",
            Value::Message(wrapper_i32(proto, protection.protector_id)?),
        );
        action.set_field_by_name(
            "protection_state_change_id",
            Value::Message(wrapper_i32(proto, protection.state_change_id)?),
        );
    }
    Ok(action)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn action_effect_phases_use_protocol_fields() {
        let proto = ProtoRegistry::from_file(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../schemas/atelier-resleriana-2.16.0.protoset"
        )))
        .unwrap();
        let rules = load_tutorial_rules().unwrap();
        let state = empty_message(&proto, "blend.model.BattleState").unwrap();
        let mut before = empty_message(&proto, "blend.model.BattleEffectResult").unwrap();
        before.set_field_by_name("effect_id", Value::I32(1));
        let mut after = empty_message(&proto, "blend.model.BattleEffectResult").unwrap();
        after.set_field_by_name("effect_id", Value::I32(2));

        let action = build_action(
            &proto,
            1,
            state,
            0,
            1,
            rules.skills.first().unwrap(),
            11,
            Vec::new(),
            vec![before],
            vec![after],
            Vec::new(),
            0,
            None,
            0,
            Some(super::effects::Protection {
                protector_id: 12,
                state_change_id: 910038,
            }),
        )
        .unwrap();

        assert_eq!(
            i32_field(
                &message_list(&action, "before_skill_effect_results")[0],
                "effect_id"
            ),
            Some(1)
        );
        assert_eq!(
            i32_field(&message_list(&action, "effect_results")[0], "effect_id"),
            Some(2)
        );
        assert_eq!(optional_i32_field(&action, "protector_id"), Some(12));
        assert_eq!(
            optional_i32_field(&action, "protection_state_change_id"),
            Some(910038)
        );
    }
}

pub(crate) fn battle_tool_effects(
    rules: &TutorialRules,
    tool: &BattlePartyTool,
) -> Result<Vec<TutorialSkillEffect>, StateError> {
    let spec = rule_tool(rules, tool.tool_id)?;
    let mut effects = Vec::new();
    for param in &tool.traits {
        let trait_rule = rule_battle_tool_trait(rules, param.id)?;
        if !trait_rule
            .filter_ids
            .iter()
            .any(|id| spec.trait_filter_ids.contains(id))
        {
            continue;
        }
        let rank = rules
            .trait_ranks
            .iter()
            .position(|r| r.id == param.rank)
            .ok_or(StateError::InvalidRequest)?;
        for effect in &trait_rule.effects {
            // These scalar tool modifiers are already consumed by the damage/heal formula.
            if matches!(
                effect.id,
                3000009
                    | 3000073
                    | 3000075
                    | 3000374
                    | 3000375
                    | 3000376
                    | 3000377
                    | 3000378
                    | 3000379
                    | 3000380
            ) {
                continue;
            }
            effects.push(TutorialSkillEffect {
                id: effect.id,
                value: *effect.values.get(rank).ok_or(StateError::InvalidRequest)?,
            });
        }
    }
    Ok(effects)
}

fn mixable_attribute(skill: &TutorialSkill) -> Option<i32> {
    (skill.skill_effect_type == 1)
        .then(|| {
            skill
                .attack_attributes
                .iter()
                .copied()
                .find(|value| (5..=8).contains(value))
        })
        .flatten()
}

pub(crate) fn battle_tool_mix_skill(
    rules: &TutorialRules,
    first: &BattlePartyTool,
    second: &BattlePartyTool,
) -> Result<TutorialSkill, StateError> {
    let first_skill = rule_skill(rules, rule_tool(rules, first.tool_id)?.skill_id)?;
    let second_skill = rule_skill(rules, rule_tool(rules, second.tool_id)?.skill_id)?;
    let first_attribute = mixable_attribute(first_skill).ok_or(StateError::InvalidRequest)?;
    let second_attribute = mixable_attribute(second_skill).ok_or(StateError::InvalidRequest)?;
    let source_power = |skill: &TutorialSkill| {
        if skill.skill_power_type == 3 {
            100
        } else {
            skill
                .power
                .max(rules.constants.battle_tool_mix_minimum_skill_power)
        }
    };
    let power = ((f64::from(source_power(first_skill)) * f64::from(source_power(second_skill)))
        .sqrt()
        * f64::from(rules.constants.battle_tool_mix_skill_power_coefficient)
        / 100.0)
        .floor() as i32;
    let all_target =
        first_skill.skill_target_type == Some(5) && second_skill.skill_target_type == Some(5);
    let threshold = if all_target {
        rules.constants.battle_tool_mix_rank_threshold_all
    } else {
        rules.constants.battle_tool_mix_rank_threshold_single
    };
    let rank = if power >= threshold { 2 } else { 1 };
    let mix = rules
        .battle_tool_mixes
        .iter()
        .find(|row| {
            row.rank == rank
                && ((row.first_item_attack_attribute == first_attribute
                    && row.second_item_attack_attribute == second_attribute)
                    || (row.first_item_attack_attribute == second_attribute
                        && row.second_item_attack_attribute == first_attribute))
        })
        .ok_or(StateError::InvalidRequest)?;
    let mut skill = rule_skill(rules, mix.skill_id)?.clone();
    skill.power = power;
    Ok(skill)
}

pub(crate) fn build_action_setup(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    number: i32,
    state: DynamicMessage,
    actor_type: i32,
    actor_id: i32,
    resources: Option<&DynamicMessage>,
    runtime: Option<&effects::Runtime>,
) -> Result<DynamicMessage, StateError> {
    let mut setup = empty_message(proto, "blend.model.BattleActionSetup")?;
    setup.set_field_by_name("number", Value::I32(number));
    setup.set_field_by_name("state", Value::Message(state.clone()));
    setup.set_field_by_name("actor_type", Value::EnumNumber(actor_type));
    setup.set_field_by_name("actor_id", Value::I32(actor_id));
    setup.set_field_by_name("is_disabled", Value::Bool(false));
    setup.set_field_by_name(
        "timeline_result",
        Value::Message(empty_message(proto, "blend.model.BattleTimelineResult")?),
    );
    if actor_type == 0 {
        setup.set_field_by_name(
            "skill_selections",
            Value::List(
                build_skill_selections(proto, rules, &state, actor_id, resources, runtime)?
                    .into_iter()
                    .map(Value::Message)
                    .collect(),
            ),
        );
        if i32_field(&state, "party_gauge").unwrap_or_default() >= rules.constants.max_party_gauge {
            let battle_tool_selections =
                build_battle_tool_selections(proto, rules, &state, runtime)?;
            setup.set_field_by_name(
                "battle_tool_selections",
                Value::List(
                    battle_tool_selections
                        .iter()
                        .cloned()
                        .into_iter()
                        .map(Value::Message)
                        .collect(),
                ),
            );
            setup.set_field_by_name(
                "battle_tool_mix_selections",
                Value::List(
                    build_battle_tool_mix_selections(
                        proto,
                        rules,
                        &state,
                        actor_id,
                        battle_tool_selections,
                    )?
                    .into_iter()
                    .map(Value::Message)
                    .collect(),
                ),
            );
        }
        if i32_field(&state, "bomb_gauge").unwrap_or_default() >= rules.constants.max_bomb_gauge {
            setup.set_field_by_name(
                "ship_tool_selections",
                Value::List(
                    build_ship_tool_selections(proto, rules, &state, runtime)?
                        .into_iter()
                        .map(Value::Message)
                        .collect(),
                ),
            );
        }
        setup.set_field_by_name(
            "active_skill_selections",
            Value::List(
                build_active_skill_selections(proto, rules, &state, actor_id, resources, runtime)?
                    .into_iter()
                    .map(Value::Message)
                    .collect(),
            ),
        );
    }
    Ok(setup)
}

fn build_battle_tool_mix_selections(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    state: &DynamicMessage,
    actor_id: i32,
    ordinary: Vec<DynamicMessage>,
) -> Result<Vec<DynamicMessage>, StateError> {
    if !member_can_use_battle_tool_mix(rules, state, actor_id)? {
        return Ok(Vec::new());
    }

    let tool_messages = message_list(state, "battle_tools");
    let mut eligible = Vec::new();
    for selection in ordinary {
        let number = i32_field(&selection, "number").ok_or(StateError::InvalidRequest)?;
        let tool = tool_messages
            .iter()
            .find(|tool| i32_field(tool, "number") == Some(number))
            .ok_or(StateError::InvalidRequest)?;
        let tool = battle_party_tool_from_state(rules, tool)?;
        let skill = rule_skill(rules, rule_tool(rules, tool.tool_id)?.skill_id)?;
        if mixable_attribute(skill).is_some() {
            let mut mix = empty_message(proto, "blend.model.BattleSelectionBattleToolMix")?;
            mix.set_field_by_name("number", Value::I32(number));
            mix.set_field_by_name("is_all", Value::Bool(bool_field(&selection, "is_all")));
            mix.set_field_by_name(
                "targets",
                Value::List(
                    message_list(&selection, "targets")
                        .into_iter()
                        .map(Value::Message)
                        .collect(),
                ),
            );
            eligible.push(mix);
        }
    }
    if eligible.len() < 2 {
        eligible.clear();
    }
    Ok(eligible)
}

pub(crate) fn member_can_use_battle_tool_mix(
    rules: &TutorialRules,
    state: &DynamicMessage,
    actor_id: i32,
) -> Result<bool, StateError> {
    let actor = message_list(state, "members")
        .into_iter()
        .find(|member| i32_field(member, "member_id") == Some(actor_id))
        .ok_or(StateError::InvalidRequest)?;
    let ally = member_status(&actor, "ally")?;
    let character_id = i32_field(&ally, "current_character_id")
        .or_else(|| i32_field(&ally, "character_id"))
        .ok_or(StateError::InvalidRequest)?;
    Ok(rules
        .battle_characters
        .iter()
        .find(|character| character.id == character_id)
        .is_some_and(|character| character.can_use_battle_tool_mix))
}

pub(crate) fn build_battle_tool_selections(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    state: &DynamicMessage,
    runtime: Option<&effects::Runtime>,
) -> Result<Vec<DynamicMessage>, StateError> {
    let mut selections = Vec::new();
    for tool_message in message_list(state, "battle_tools") {
        if i32_field(&tool_message, "usage_count").unwrap_or_default() <= 0 {
            continue;
        }
        let number = i32_field(&tool_message, "number").ok_or(StateError::InvalidRequest)?;
        let tool = battle_party_tool_from_state(rules, &tool_message)?;
        let skill = rule_skill(rules, rule_tool(rules, tool.tool_id)?.skill_id)?;
        let target_type = skill.skill_target_type.ok_or(StateError::InvalidRequest)?;
        let targets = build_tool_selection_targets(
            proto,
            rules,
            state,
            skill,
            std::slice::from_ref(&tool),
            runtime,
        )?;
        if !targets.is_empty() {
            let mut selection = empty_message(proto, "blend.model.BattleSelectionBattleTool")?;
            selection.set_field_by_name("number", Value::I32(number));
            selection.set_field_by_name("is_all", Value::Bool(matches!(target_type, 4 | 5)));
            selection.set_field_by_name("targets", Value::List(targets));
            selections.push(selection);
        }
    }
    Ok(selections)
}

fn build_character_selection_targets(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    state: &DynamicMessage,
    actor_id: i32,
    skill: &TutorialSkill,
    resources: Option<&DynamicMessage>,
    runtime: Option<&effects::Runtime>,
) -> Result<(i32, Vec<Value>), StateError> {
    let actor = message_list(state, "members")
        .into_iter()
        .find(|member| i32_field(member, "member_id") == Some(actor_id))
        .ok_or(StateError::InvalidRequest)?;
    let actor_type = member_type(&actor)?;
    let opponent_count = i32::try_from(
        message_list(state, "members")
            .iter()
            .filter(|member| {
                bool_field(member, "is_alive") && member_type(member).ok() != Some(actor_type)
            })
            .count(),
    )
    .map_err(|_| StateError::InvalidRequest)?;
    let (panel, break_panel) = if let Some(runtime) = runtime {
        (runtime.panel_multiplier(state)?, runtime.panel_break_multiplier(state)?)
    } else {
        (battle_panel_multiplier(state), battle_panel_break_multiplier(state))
    };
    let target_type = skill.skill_target_type.ok_or(StateError::InvalidRequest)?;
    let target_member_type = if matches!(target_type, 1 | 2 | 4) {
        0
    } else {
        1
    };
    let provocation_target = runtime.and_then(|runtime| runtime.provocation_target(actor_id));
    let targets = message_list(state, "members")
        .into_iter()
        .filter(|member| {
            (target_type == 6 || member_type(member).ok() == Some(target_member_type))
                && (target_type != 1 || member_id(member).ok() == Some(actor_id))
                && (target_type != 3
                    || provocation_target
                        .is_none_or(|target| member_id(member).ok() == Some(target)))
                && bool_field(member, "is_alive")
        })
        .collect::<Vec<_>>();
    let mut target_values = Vec::new();
    for target in targets {
        let target_id = member_id(&target)?;
        let mut preview = empty_message(proto, "blend.model.BattleSelectionTarget")?;
        preview.set_field_by_name("target_id", Value::I32(target_id));
        if skill.skill_effect_type == 2 {
            let hp = i32_field(&target, "hp").unwrap_or(0).max(0);
            let max_hp = i32_field(&target, "max_hp").unwrap_or(0).max(0);
            preview.set_field_by_name(
                "hp_heal",
                Value::Message(wrapper_i32(
                    proto,
                    effects::healing_amount(
                        quest::heal_amount(&actor, &target, skill)?,
                        &actor,
                        &target,
                    )?
                    .min(max_hp - hp),
                )?),
            );
        } else if skill.skill_effect_type == 1 {
            let broken = member_status(&target, "enemy")
                .ok()
                .is_some_and(|enemy| bool_field(&enemy, "is_broken"));
            let damage = policy_damage(
                proto,
                rules,
                &actor,
                &target,
                skill,
                resources,
                runtime,
                opponent_count,
                panel,
                broken,
                false,
                10_000,
            )?;
            let critical = policy_damage(
                proto,
                rules,
                &actor,
                &target,
                skill,
                resources,
                runtime,
                opponent_count,
                panel,
                broken,
                true,
                10_000,
            )?;
            let break_damage =
                policy_break_damage(&actor, &target, skill, runtime, break_panel, false, 10_000)?;
            let attribute = preferred_attack_attribute(&target, skill)?;
            let resistance = target_resistance(&target, attribute)?;
            preview.set_field_by_name("hp_damage", Value::Message(wrapper_i64(proto, damage)?));
            preview.set_field_by_name(
                "hp_damage_for_critical",
                Value::Message(wrapper_i64(proto, critical)?),
            );
            preview.set_field_by_name(
                "break_damage",
                Value::Message(wrapper_i32(proto, break_damage)?),
            );
            preview.set_field_by_name(
                "is_killed",
                Value::Bool(damage >= i64::from(i32_field(&target, "hp").unwrap_or(0))),
            );
            preview.set_field_by_name("is_weak", Value::Bool(resistance < 0));
            preview.set_field_by_name("is_resist", Value::Bool(resistance > 0));
        }
        target_values.push(Value::Message(preview));
    }
    Ok((target_type, target_values))
}

pub(crate) fn build_skill_selections(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    state: &DynamicMessage,
    actor_id: i32,
    resources: Option<&DynamicMessage>,
    runtime: Option<&effects::Runtime>,
) -> Result<Vec<DynamicMessage>, StateError> {
    let actor = message_list(state, "members")
        .into_iter()
        .find(|member| i32_field(member, "member_id") == Some(actor_id))
        .ok_or(StateError::InvalidRequest)?;
    let burst_enabled = member_status(&actor, "burst_gauge")
        .ok()
        .and_then(|gauge| i32_field(&gauge, "current_gauge"))
        .unwrap_or(0)
        >= rules.constants.burst_gauge_required_for_one_burst_skill;
    let mut selections = Vec::new();
    for selected in member_skills(&actor)? {
        if selected.skill_type == 3 && !burst_enabled {
            continue;
        }
        let skill = rule_skill(rules, selected.id)?;
        let transformed = effects::skill_transformation(&actor, selected.id)?
            .map(|(_, destination)| rule_skill(rules, destination))
            .transpose()?;
        let preview_skill = transformed.unwrap_or(skill);
        let (target_type, target_values) = build_character_selection_targets(
            proto, rules, state, actor_id, preview_skill, resources, runtime,
        )?;
        if target_values.is_empty() {
            continue;
        }
        let mut selection = empty_message(proto, "blend.model.BattleSelectionSkill")?;
        selection.set_field_by_name("skill_type", Value::I32(selected.skill_type));
        selection.set_field_by_name("skill_id", Value::I32(selected.id));
        selection.set_field_by_name("is_all", Value::Bool(matches!(target_type, 4 | 5)));
        selection.set_field_by_name("targets", Value::List(target_values));
        if let Some(lamp) = effects::predicted_skill_lamp(&actor, selected.id)? {
            selection.set_field_by_name("all_skill_lamp", Value::I32(lamp));
        }
        selections.push(selection);
    }
    Ok(selections)
}

fn build_ship_tool_selections(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    state: &DynamicMessage,
    runtime: Option<&effects::Runtime>,
) -> Result<Vec<DynamicMessage>, StateError> {
    let mut selections = Vec::new();
    for tool in message_list(state, "ship_tools") {
        if i32_field(&tool, "usage_count").unwrap_or_default() <= 0 {
            continue;
        }
        let number = i32_field(&tool, "number").ok_or(StateError::InvalidRequest)?;
        let skill = rule_skill(
            rules,
            i32_field(&tool, "skill_id").ok_or(StateError::InvalidRequest)?,
        )?;
        let target_type = skill.skill_target_type.ok_or(StateError::InvalidRequest)?;
        let targets = build_tool_selection_targets(proto, rules, state, skill, &[], runtime)?;
        if targets.is_empty() {
            continue;
        }
        let mut selection = empty_message(proto, "blend.model.BattleSelectionShipTool")?;
        selection.set_field_by_name("number", Value::I32(number));
        selection.set_field_by_name("is_all", Value::Bool(matches!(target_type, 4 | 5)));
        selection.set_field_by_name("targets", Value::List(targets));
        selections.push(selection);
    }
    Ok(selections)
}

fn build_tool_selection_targets(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    state: &DynamicMessage,
    skill: &TutorialSkill,
    tools: &[BattlePartyTool],
    runtime: Option<&effects::Runtime>,
) -> Result<Vec<Value>, StateError> {
    let members = message_list(state, "members");
    let target_type = skill.skill_target_type.ok_or(StateError::InvalidRequest)?;
    let target_member_type = if matches!(target_type, 1 | 2 | 4) {
        0
    } else {
        1
    };
    let source = current_actor(state)?;
    let mut targets = Vec::new();
    for target in members.iter().filter(|member| {
        member_type(member).ok() == Some(target_member_type) && bool_field(member, "is_alive")
    }) {
        let mut preview = empty_message(proto, "blend.model.BattleSelectionTarget")?;
        preview.set_field_by_name("target_id", Value::I32(member_id(target)?));
        if skill.skill_effect_type == 2 {
            let hp = i32_field(target, "hp").unwrap_or_default().max(0);
            let max_hp = i32_field(target, "max_hp").unwrap_or_default().max(0);
            let heal = policy_tool_heal(rules, tools, skill, &source, target)?.min(max_hp - hp);
            preview.set_field_by_name("hp_heal", Value::Message(wrapper_i32(proto, heal)?));
        } else if skill.skill_effect_type == 1 {
            let broken = target.has_field_by_name("enemy")
                && member_status(target, "enemy")
                    .ok()
                    .is_some_and(|enemy| bool_field(&enemy, "is_broken"));
            let damage = policy_tool_damage(
                rules, tools, &members, target, skill, runtime, broken, 10_000,
            )?;
            let attribute = preferred_attack_attribute(target, skill)?;
            let resistance = target_resistance(target, attribute)?;
            preview.set_field_by_name("hp_damage", Value::Message(wrapper_i64(proto, damage)?));
            preview.set_field_by_name(
                "is_killed",
                Value::Bool(damage >= i64::from(i32_field(target, "hp").unwrap_or_default())),
            );
            preview.set_field_by_name("is_weak", Value::Bool(resistance < 0));
            preview.set_field_by_name("is_resist", Value::Bool(resistance > 0));
        }
        targets.push(Value::Message(preview));
    }
    Ok(targets)
}

pub(crate) fn build_active_skill_selections(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    state: &DynamicMessage,
    actor_id: i32,
    resources: Option<&DynamicMessage>,
    runtime: Option<&effects::Runtime>,
) -> Result<Vec<DynamicMessage>, StateError> {
    let actor = message_list(state, "members")
        .into_iter()
        .find(|member| i32_field(member, "member_id") == Some(actor_id))
        .ok_or(StateError::InvalidRequest)?;
    let active = message_list(&member_status(&actor, "ally")?, "active_skills");
    let mut selections = Vec::new();
    for selected in member_active_skills(&actor)? {
        let available = active.iter().any(|candidate| {
            i32_field(candidate, "active_skill_type") == Some(selected.skill_type)
                && optional_i32_field(candidate, "rest_count").unwrap_or(0) > 0
        });
        if !available {
            continue;
        }
        let skill = rule_skill(rules, selected.id)?;
        let (target_type, target_values) = build_character_selection_targets(
            proto, rules, state, actor_id, skill, resources, runtime,
        )?;
        if target_values.is_empty() {
            continue;
        }
        let mut selection = empty_message(proto, "blend.model.BattleSelectionActiveSkill")?;
        selection.set_field_by_name("active_skill_type", Value::I32(selected.skill_type));
        selection.set_field_by_name("skill_id", Value::I32(selected.id));
        selection.set_field_by_name("is_all", Value::Bool(matches!(target_type, 4 | 5)));
        selection.set_field_by_name("targets", Value::List(target_values));
        selections.push(selection);
    }
    Ok(selections)
}

pub(crate) fn append_wave_members(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    state: &mut DynamicMessage,
    wave: &TutorialWave,
) -> Result<(), StateError> {
    let mut members: Vec<_> = message_list(state, "members")
        .into_iter()
        .filter(|member| member_type(member).ok() == Some(0))
        .collect();
    let mut base_numbers: Vec<(i32, i32)> = Vec::new();
    for (index, wave_enemy) in wave.enemies.iter().enumerate() {
        // Battle responses use 1-based ally IDs and 11-based enemy IDs; IDs
        // are reused for the replacement group at each wave.
        let member_id = 11 + i32::try_from(index).map_err(|_| StateError::InvalidRequest)?;
        let enemy = rule_enemy(rules, wave_enemy.id)?;
        let number = if let Some((_, count)) = base_numbers
            .iter_mut()
            .find(|(base_id, _)| *base_id == enemy.base_enemy_id)
        {
            *count += 1;
            *count
        } else {
            base_numbers.push((enemy.base_enemy_id, 1));
            1
        };
        let enemy_member =
            build_enemy_member(proto, rules, wave_enemy, wave.id, member_id, number)?;
        members.push(enemy_member);
    }
    let battle_id = i32_field(state, "battle_id").ok_or(StateError::InvalidRequest)?;
    let wave_number = i32_field(state, "wave").ok_or(StateError::InvalidRequest)?;
    let units = tutorial_timeline_units(proto, rules, battle_id, wave_number, &members)?;
    state.set_field_by_name(
        "members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );
    state.set_field_by_name(
        "timeline_units",
        Value::List(units.into_iter().map(Value::Message).collect()),
    );
    set_base_enemy_numbers(proto, state, &base_numbers)?;
    Ok(())
}

pub(crate) fn set_base_enemy_numbers(
    proto: &ProtoRegistry,
    state: &mut DynamicMessage,
    base_numbers: &[(i32, i32)],
) -> Result<(), StateError> {
    state.set_field_by_name(
        "base_enemy_numbers",
        Value::List(
            base_numbers
                .iter()
                .map(|(base_id, count)| {
                    let mut row = empty_message(proto, "blend.model.BattleBaseEnemyNumber")?;
                    row.set_field_by_name("base_enemy_id", Value::I32(*base_id));
                    row.set_field_by_name(
                        "current_number",
                        Value::Message(int32_value(proto, *count)?),
                    );
                    Ok(Value::Message(row))
                })
                .collect::<Result<Vec<_>, StateError>>()?,
        ),
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn policy_tool_damage(
    rules: &TutorialRules,
    tools: &[BattlePartyTool],
    members: &[DynamicMessage],
    target: &DynamicMessage,
    skill: &TutorialSkill,
    runtime: Option<&effects::Runtime>,
    broken: bool,
    variance: u32,
) -> Result<i64, StateError> {
    let attribute = preferred_attack_attribute(target, skill)?;
    let item_bonus = members
        .iter()
        .filter(|member| member_type(member).ok() == Some(0) && bool_field(member, "is_alive"))
        .map(|member| {
            i64::from(state_change_summary_value(member, 27))
                .saturating_sub(i64::from(state_change_summary_value(member, 28)))
                .saturating_add(runtime.map_or(0, |runtime| {
                    runtime
                        .contextual_summary(member, skill, false, 27)
                        .saturating_sub(runtime.contextual_summary(member, skill, false, 28))
                }))
        })
        .max()
        .ok_or(StateError::InvalidRequest)?;
    let item_penetration = members
        .iter()
        .filter(|member| member_type(member).ok() == Some(0) && bool_field(member, "is_alive"))
        .map(|member| {
            i64::from(state_change_summary_value(member, 34))
                + runtime.map_or(0, |runtime| {
                    runtime.contextual_summary(member, skill, false, 34)
                })
        })
        .max()
        .unwrap_or(0)
        + effects::instant_summary(skill, target, false, 34)?
        + i64::from(state_change_summary_value(target, 19))
        + runtime.map_or(0, |runtime| {
            runtime.contextual_summary(target, skill, false, 19)
        });
    let (penetration_numerator, penetration_denominator) =
        effects::penetration_factor(item_penetration);
    let resistance =
        (100 - target_resistance(target, attribute)? + if broken { 50 } else { 0 }).max(0) as i128;
    let magic_trait_bonus = if (5..=8).contains(&attribute) {
        tool_trait_effects_total(rules, tools, 3_000_009)?
    } else {
        0
    };
    let attribute_trait_bonus = match attribute {
        1 => tool_trait_effects_total(rules, tools, 3_000_374)?,
        2 => tool_trait_effects_total(rules, tools, 3_000_375)?,
        3 => tool_trait_effects_total(rules, tools, 3_000_376)?,
        5 => tool_trait_effects_total(rules, tools, 3_000_377)?,
        6 => tool_trait_effects_total(rules, tools, 3_000_379)?,
        7 => tool_trait_effects_total(rules, tools, 3_000_378)?,
        8 => tool_trait_effects_total(rules, tools, 3_000_380)?,
        _ => 0,
    };
    let trait_bonus = magic_trait_bonus
        .checked_add(attribute_trait_bonus)
        .ok_or(StateError::InvalidRequest)?;
    let incoming = effects::incoming_multiplier_for_skill(target, skill, runtime, false)?;
    let numerator = i128::from(skill.power.max(0))
        * i128::from(10_000i64 + item_bonus + i64::from(trait_bonus))
        * resistance
        * i128::from(variance)
        * penetration_numerator;
    let denominator = 10i128 * 10_000 * 100 * 10_000;
    Ok(
        (numerator * i128::from(incoming) / (denominator * 10_000 * penetration_denominator))
            .clamp(0, 9_999_999_999) as i64,
    )
}

fn tool_trait_effects_total(
    rules: &TutorialRules,
    tools: &[BattlePartyTool],
    effect_id: i32,
) -> Result<i32, StateError> {
    tools.iter().try_fold(0i32, |total, tool| {
        total
            .checked_add(tool_trait_effect_total(rules, tool, effect_id)?)
            .ok_or(StateError::InvalidRequest)
    })
}

pub(crate) fn tool_trait_effect_total(
    rules: &TutorialRules,
    tool: &BattlePartyTool,
    effect_id: i32,
) -> Result<i32, StateError> {
    let tool_rule = rule_tool(rules, tool.tool_id)?;
    tool.traits.iter().try_fold(0i32, |total, trait_param| {
        let trait_rule = rule_battle_tool_trait(rules, trait_param.id)?;
        if !trait_rule
            .filter_ids
            .iter()
            .any(|id| tool_rule.trait_filter_ids.contains(id))
        {
            return Ok(total);
        }
        let rank_index = rules
            .trait_ranks
            .iter()
            .position(|rank| rank.id == trait_param.rank)
            .ok_or(StateError::InvalidRequest)?;
        let value = trait_rule
            .effects
            .iter()
            .find(|effect| effect.id == effect_id)
            .and_then(|effect| effect.values.get(rank_index))
            .copied()
            .unwrap_or(0);
        total.checked_add(value).ok_or(StateError::InvalidRequest)
    })
}

pub(crate) fn policy_tool_heal(
    rules: &TutorialRules,
    tools: &[BattlePartyTool],
    skill: &TutorialSkill,
    source: &DynamicMessage,
    target: &DynamicMessage,
) -> Result<i32, StateError> {
    let mut bonus = tool_trait_effects_total(rules, tools, 3_000_075)?;
    if skill.skill_target_type == Some(2) {
        bonus = bonus
            .checked_add(tool_trait_effects_total(rules, tools, 3_000_073)?)
            .ok_or(StateError::InvalidRequest)?;
    }
    let base = checked_i32(i64::from(skill.power.max(0)) * i64::from(10_000 + bonus) / 10_000)?;
    effects::healing_amount(base, source, target)
}

pub(crate) fn policy_tutorial_enemy_damage(
    actor: &DynamicMessage,
    target: &DynamicMessage,
    skill: &TutorialSkill,
    variance: u32,
) -> Result<i64, StateError> {
    let enemy_id = enemy_member_status_enemy_id(actor)?;
    let scale = match enemy_id {
        80009036 => 320,
        80003007 => 2_000,
        _ => 100,
    };
    let attribute = preferred_attack_attribute(target, skill)?;
    let resistance = (100 - target_resistance(target, attribute)?).max(0) as i128;
    let numerator =
        i128::from(skill.power.max(0)) * i128::from(scale) * resistance * i128::from(variance);
    let denominator = 10_000i128 * 100 * 10_000;
    Ok((numerator / denominator).clamp(1, 9_999_999_999) as i64)
}
