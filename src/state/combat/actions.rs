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
    action_type: i32,
    battle_tool_number: Option<i32>,
    total_damage: i64,
) -> Result<DynamicMessage, StateError> {
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
    if let Some(number) = battle_tool_number {
        action.set_field_by_name(
            "battle_tool_number",
            Value::Message(wrapper_i32(proto, number)?),
        );
    }
    action.set_field_by_name("total_dealt_hp_damage", Value::I64(total_damage));
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
            if matches!(effect.id, 3000009 | 3000375 | 3000377 | 3000075 | 3000073) {
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
            setup.set_field_by_name(
                "battle_tool_selections",
                Value::List(
                    build_battle_tool_selections(proto, rules, &state, runtime)?
                        .into_iter()
                        .map(Value::Message)
                        .collect(),
                ),
            );
        }
    }
    Ok(setup)
}

pub(crate) fn build_battle_tool_selections(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    state: &DynamicMessage,
    runtime: Option<&effects::Runtime>,
) -> Result<Vec<DynamicMessage>, StateError> {
    let members = message_list(state, "members");
    let mut selections = Vec::new();
    for tool_message in message_list(state, "battle_tools") {
        if i32_field(&tool_message, "usage_count").unwrap_or_default() <= 0 {
            continue;
        }
        let number = i32_field(&tool_message, "number").ok_or(StateError::InvalidRequest)?;
        let tool = battle_party_tool_from_state(rules, &tool_message)?;
        let skill = rule_skill(rules, rule_tool(rules, tool.tool_id)?.skill_id)?;
        let target_type = skill.skill_target_type.ok_or(StateError::InvalidRequest)?;
        let target_member_type = if matches!(target_type, 1 | 2 | 4) {
            0
        } else {
            1
        };
        let mut targets = Vec::new();
        for target in members.iter().filter(|member| {
            member_type(member).ok() == Some(target_member_type) && bool_field(member, "is_alive")
        }) {
            let mut preview = empty_message(proto, "blend.model.BattleSelectionTarget")?;
            preview.set_field_by_name("target_id", Value::I32(member_id(target)?));
            if skill.skill_effect_type == 2 {
                let hp = i32_field(target, "hp").unwrap_or_default().max(0);
                let max_hp = i32_field(target, "max_hp").unwrap_or_default().max(0);
                let heal = policy_tool_heal(rules, &tool, skill)?.min(max_hp - hp);
                preview.set_field_by_name("hp_heal", Value::Message(wrapper_i32(proto, heal)?));
            } else if skill.skill_effect_type == 1 {
                let broken = target.has_field_by_name("enemy")
                    && member_status(target, "enemy")
                        .ok()
                        .is_some_and(|enemy| bool_field(&enemy, "is_broken"));
                let damage = policy_tool_damage(
                    rules, &tool, &members, target, skill, runtime, broken, 10_000,
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
    let panel = battle_panel_multiplier(state);
    let break_panel = battle_panel_break_multiplier(state);
    let mut selections = Vec::new();
    for selected in member_skills(&actor)? {
        if selected.skill_type == 3 && !burst_enabled {
            continue;
        }
        let skill = rule_skill(rules, selected.id)?;
        let target_type = skill.skill_target_type.ok_or(StateError::InvalidRequest)?;
        let target_member_type = if matches!(target_type, 1 | 2 | 4) {
            0
        } else {
            1
        };
        let provocation_target = runtime.and_then(|runtime| runtime.provocation_target(actor_id));
        let targets: Vec<_> = message_list(state, "members")
            .into_iter()
            .filter(|member| {
                (target_type == 6 || member_type(member).ok() == Some(target_member_type))
                    && (target_type != 1 || member_id(member).ok() == Some(actor_id))
                    && (target_type != 3
                        || provocation_target
                            .is_none_or(|target| member_id(member).ok() == Some(target)))
                    && bool_field(member, "is_alive")
            })
            .collect();
        if targets.is_empty() {
            continue;
        }
        let mut selection = empty_message(proto, "blend.model.BattleSelectionSkill")?;
        selection.set_field_by_name("skill_type", Value::I32(selected.skill_type));
        selection.set_field_by_name("skill_id", Value::I32(selected.id));
        selection.set_field_by_name("is_all", Value::Bool(matches!(target_type, 4 | 5)));
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
                    proto, rules, &actor, &target, skill, resources, runtime, panel, broken, false,
                    10_000,
                )?;
                let critical = policy_damage(
                    proto, rules, &actor, &target, skill, resources, runtime, panel, broken, true,
                    10_000,
                )?;
                let break_damage =
                    policy_break_damage(&actor, &target, skill, runtime, break_panel, 10_000)?;
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
    tool: &BattlePartyTool,
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
            state_change_summary_value(member, 27)
                .saturating_sub(state_change_summary_value(member, 28))
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
    let trait_bonus = tool_trait_effect_total(rules, tool, 3_000_009)?
        + if attribute == 2 {
            tool_trait_effect_total(rules, tool, 3_000_375)?
        } else if attribute == 4 {
            tool_trait_effect_total(rules, tool, 3_000_377)?
        } else {
            0
        };
    let numerator = i128::from(skill.power.max(0))
        * i128::from(10_000 + item_bonus + trait_bonus)
        * resistance
        * i128::from(variance)
        * penetration_numerator;
    let denominator = 10i128 * 10_000 * 100 * 10_000;
    Ok((numerator
        * i128::from(effects::incoming_multiplier_with_runtime(
            target, attribute, runtime,
        ))
        / (denominator * 10_000 * penetration_denominator))
        .clamp(0, 9_999_999_999) as i64)
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
    tool: &BattlePartyTool,
    skill: &TutorialSkill,
) -> Result<i32, StateError> {
    let mut bonus = tool_trait_effect_total(rules, tool, 3_000_075)?;
    if skill.skill_target_type == Some(2) {
        bonus = bonus
            .checked_add(tool_trait_effect_total(rules, tool, 3_000_073)?)
            .ok_or(StateError::InvalidRequest)?;
    }
    checked_i32(i64::from(skill.power.max(0)) * i64::from(10_000 + bonus) / 10_000)
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
