use super::prelude::*;

fn apply_timeline_effects(
    proto: &ProtoRegistry,
    units: &mut [DynamicMessage],
    members: &[DynamicMessage],
    source_id: i32,
    skill_id: i32,
    source_effects: &[TutorialSkillEffect],
    target_ids: &[i32],
) -> Result<Vec<DynamicMessage>, StateError> {
    let mut targets = Vec::new();
    for target_id in target_ids.iter().copied() {
        if !targets.contains(&target_id)
            && members.iter().any(|member| {
                i32_field(member, "member_id") == Some(target_id) && bool_field(member, "is_alive")
            })
        {
            targets.push(target_id);
        }
    }
    let mut movements = Vec::new();
    let source = members
        .iter()
        .find(|member| member_id(member).ok() == Some(source_id))
        .ok_or(StateError::InvalidRequest)?;
    for effect in source_effects {
        for target_id in targets.iter().copied() {
            let target = members
                .iter()
                .find(|member| member_id(member).ok() == Some(target_id))
                .ok_or(StateError::InvalidRequest)?;
            let Some(slots) = effects::timeline_slots(source, target, skill_id, effect)? else {
                continue;
            };
            movements.extend(if slots > 0 {
                delay_timeline_member_by_slots(proto, units, members, target_id, slots as usize)?
            } else {
                advance_timeline_member_by_slots(
                    proto,
                    units,
                    members,
                    target_id,
                    slots.unsigned_abs() as usize,
                )?
            });
        }
    }
    Ok(movements)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_attack_results(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    state: &mut DynamicMessage,
    actor_id: i32,
    actor_type: i32,
    skill: &TutorialSkill,
    source_effects: &[TutorialSkillEffect],
    resources: Option<&DynamicMessage>,
    runtime: &effects::Runtime,
    target_ids: &[i32],
    panel: (i128, i128),
    tools: Option<&[BattlePartyTool]>,
    consume_turn: bool,
    cancelled: bool,
    secret: &[u8],
    start_txid: &str,
    action_number: i32,
) -> Result<(Vec<DynamicMessage>, Vec<DynamicMessage>, i64), StateError> {
    let mut members = message_list(state, "members");
    let mut units = message_list(state, "timeline_units");
    let actor_index = members
        .iter()
        .position(|member| i32_field(member, "member_id") == Some(actor_id))
        .ok_or(StateError::InvalidRequest)?;
    let mut results = Vec::new();
    let mut movements = Vec::new();
    let mut total_damage = 0i64;
    let mut killed_ids = Vec::new();
    let is_tool = tools.is_some();
    let break_panel = runtime.panel_break_multiplier(state)?;
    let guaranteed_critical = battle_panel_guarantees_critical(state);
    let opponent_count = i32::try_from(
        members
            .iter()
            .filter(|member| {
                bool_field(member, "is_alive") && member_type(member).ok() != Some(actor_type)
            })
            .count(),
    )
    .map_err(|_| StateError::InvalidRequest)?;

    if cancelled {
        for target_id in target_ids {
            let mut result = build_skill_result(
                proto, *target_id, 0, 0, 0, 0, false, false, false, false, false, false, false,
            )?;
            result.set_field_by_name("is_invalid", Value::Bool(true));
            results.push(result);
        }
        if consume_turn {
            let speed = i32_field(
                &member_status(&members[actor_index], "current_status")?,
                "speed",
            )
            .ok_or(StateError::InvalidRequest)?;
            movements.push(consume_timeline_turn(
                proto,
                &mut units,
                &members,
                actor_id,
                action_wait(speed, skill.wait),
                base_wait(speed),
                actor_type == 1,
            )?);
        }
        state.set_field_by_name(
            "timeline_units",
            Value::List(units.into_iter().map(Value::Message).collect()),
        );
        return Ok((results, movements, 0));
    }

    // Supported effect rules run before/after this base HP/break operation.
    // Unrecovered catalog effects remain in the runtime's explicit unresolved set.
    for (draw_index, target_id) in target_ids.iter().copied().enumerate() {
        let target_index = members
            .iter()
            .position(|member| i32_field(member, "member_id") == Some(target_id))
            .ok_or(StateError::InvalidRequest)?;
        let target_enemy = if members[target_index].has_field_by_name("enemy") {
            Some(member_status(&members[target_index], "enemy")?)
        } else {
            None
        };
        let target_broken = target_enemy
            .as_ref()
            .is_some_and(|enemy| bool_field(enemy, "is_broken"));
        if skill.skill_effect_type == 2 {
            if member_type(&members[target_index])? != actor_type {
                return Err(StateError::TutorialRules(format!(
                    "unsupported tutorial heal skill {}",
                    skill.id
                )));
            }
            let hp = i32_field(&members[target_index], "hp")
                .ok_or(StateError::InvalidRequest)?
                .max(0);
            let max_hp = i32_field(&members[target_index], "max_hp")
                .ok_or(StateError::InvalidRequest)?
                .max(0);
            let heal = if let Some(tools) = tools {
                policy_tool_heal(
                    rules,
                    tools,
                    skill,
                    &members[actor_index],
                    &members[target_index],
                )?
            } else {
                effects::healing_amount(
                    quest::heal_amount(&members[actor_index], &members[target_index], skill)?,
                    &members[actor_index],
                    &members[target_index],
                )?
            };
            let next_hp = hp.saturating_add(heal).min(max_hp);
            let healed = next_hp - hp;
            members[target_index].set_field_by_name("hp", Value::I32(next_hp));
            results.push(build_skill_result(
                proto, target_id, 0, 0, healed, 0, false, false, false, false, false, false, false,
            )?);
            continue;
        }
        // Support and debuff skills have no base HP operation. Their effects run
        // in the shared post-skill pass, and they still consume a timeline turn.
        if matches!(skill.skill_effect_type, 3 | 4) {
            results.push(build_skill_result(
                proto, target_id, 0, 0, 0, 0, false, false, false, false, false, false, false,
            )?);
            continue;
        }
        if skill.skill_effect_type != 1 {
            return Err(StateError::TutorialRules(format!(
                "unsupported tutorial skill effect type {}",
                skill.skill_effect_type
            )));
        }
        let draw_index = u32::try_from(draw_index).map_err(|_| StateError::InvalidRequest)?;
        if !is_tool
            && (deterministic_roll(
                secret,
                start_txid,
                action_number,
                b"blind",
                target_id,
                draw_index,
            ) % 10_000
                < runtime.blind_rate(actor_id) as u32
                || deterministic_roll(
                    secret,
                    start_txid,
                    action_number,
                    b"evasion",
                    target_id,
                    draw_index,
                ) % 10_000
                    < runtime.evasion_rate(target_id) as u32)
        {
            let mut miss = build_skill_result(
                proto, target_id, 0, 0, 0, 0, false, false, false, false, false, false, false,
            )?;
            miss.set_field_by_name("is_miss", Value::Bool(true));
            results.push(miss);
            continue;
        }
        let critical = !is_tool
            && (guaranteed_critical
                || effects::receives_guaranteed_critical(&members[target_index])
                || (actor_type == 0
                    && deterministic_roll(
                        secret,
                        start_txid,
                        action_number,
                        b"critical",
                        target_id,
                        draw_index,
                    ) % 10_000
                        < (1_000i64
                            + i64::from(state_change_summary_value(&members[actor_index], 6)))
                        .clamp(0, 10_000) as u32));
        let variance = 10_000
            + deterministic_roll(
                secret,
                start_txid,
                action_number,
                b"variance",
                target_id,
                draw_index,
            ) % 101;
        let damage = if let Some(tools) = tools {
            policy_tool_damage(
                rules,
                tools,
                &members,
                &members[target_index],
                skill,
                Some(runtime),
                target_broken,
                variance,
            )?
        } else if actor_type == 1 {
            if matches!(
                i32_field(state, "battle_id"),
                Some(10000279 | 10000280 | 10000003 | 10000004)
            ) {
                policy_tutorial_enemy_damage(
                    &members[actor_index],
                    &members[target_index],
                    skill,
                    variance,
                )?
            } else {
                quest::enemy_damage(
                    proto,
                    rules,
                    &members[actor_index],
                    &members[target_index],
                    skill,
                    variance,
                )?
            }
        } else {
            policy_damage(
                proto,
                rules,
                &members[actor_index],
                &members[target_index],
                skill,
                resources,
                Some(runtime),
                opponent_count,
                panel,
                target_broken,
                critical,
                variance,
            )?
        };
        let damage = if !is_tool && actor_type == 1 {
            let panel_bonus = ((panel.0 - panel.1) * 10_000 / panel.1) as i64;
            let rate = (10_000i64
                + i64::from(state_change_summary_value(&members[actor_index], 1))
                - i64::from(state_change_summary_value(&members[actor_index], 2))
                + panel_bonus)
                .clamp(0, 1_000_000);
            let mut base =
                (i128::from(damage) * i128::from(rate) / 10_000).clamp(0, 9_999_999_999) as i64;
            if critical {
                base = base.saturating_mul(3).saturating_add(1) / 2;
            }
            effects::secondary_damage(
                base,
                &members[actor_index],
                &members[target_index],
                skill,
                Some(runtime),
                critical,
            )?
        } else {
            damage
        };
        let critical_damage = if is_tool {
            0
        } else if actor_type == 1 {
            if critical {
                damage
            } else {
                damage.saturating_mul(3).saturating_add(1) / 2
            }
        } else {
            policy_damage(
                proto,
                rules,
                &members[actor_index],
                &members[target_index],
                skill,
                resources,
                Some(runtime),
                opponent_count,
                panel,
                target_broken,
                true,
                variance,
            )?
        };
        let mut break_damage = if !is_tool && target_enemy.is_some() {
            policy_break_damage(
                &members[actor_index],
                &members[target_index],
                skill,
                Some(runtime),
                break_panel,
                critical,
                variance,
            )?
        } else {
            0
        };
        let hp = i32_field(&members[target_index], "hp")
            .ok_or(StateError::InvalidRequest)?
            .max(0);
        let damage_i32 = i32::try_from(damage).unwrap_or(i32::MAX);
        let new_hp = hp.saturating_sub(damage_i32).max(0);
        let killed = new_hp == 0;
        members[target_index].set_field_by_name("hp", Value::I32(new_hp));
        members[target_index].set_field_by_name("is_alive", Value::Bool(!killed));

        let mut newly_broken = false;
        if let Some(mut enemy) = target_enemy {
            let old_break = i32_field(&enemy, "break_gauge").unwrap_or(0).max(0);
            if !is_tool && effects::instant_break_gauge_zero(skill, &members[target_index])? {
                break_damage = break_damage.max(old_break);
            }
            let new_break = old_break.saturating_sub(break_damage).max(0);
            newly_broken = old_break > 0 && new_break == 0;
            enemy.set_field_by_name("break_gauge", Value::I32(new_break));
            enemy.set_field_by_name("is_broken", Value::Bool(target_broken || newly_broken));
            members[target_index].set_field_by_name("enemy", Value::Message(enemy));
        }

        if killed {
            killed_ids.push(target_id);
        } else if newly_broken {
            movements.extend(delay_timeline_member(
                proto,
                &mut units,
                &members,
                target_id,
                rules.constants.display_skill_wait_offset,
            )?);
        }
        let attribute = preferred_attack_attribute(&members[target_index], skill)?;
        let resistance = target_resistance(&members[target_index], attribute)?;
        total_damage = total_damage
            .checked_add(i64::from(hp.min(damage_i32.max(0))))
            .ok_or(StateError::InvalidRequest)?;
        results.push(build_skill_result(
            proto,
            target_id,
            damage,
            critical_damage,
            0,
            break_damage,
            true,
            is_tool && i32_field(state, "battle_id") == Some(10000279),
            killed,
            critical,
            newly_broken,
            resistance < 0,
            resistance > 0,
        )?);
    }

    let effect_targets = results
        .iter()
        .filter(|result| !bool_field(result, "is_miss") && !bool_field(result, "is_invalid"))
        .filter_map(|result| i32_field(result, "target_id"))
        .fold(Vec::new(), |mut ids, id| {
            if !ids.contains(&id) {
                ids.push(id);
            }
            ids
        });
    movements.extend(apply_timeline_effects(
        proto,
        &mut units,
        &members,
        actor_id,
        skill.id,
        source_effects,
        &effect_targets,
    )?);

    if !killed_ids.is_empty() {
        movements.extend(remove_timeline_members(
            proto,
            &mut units,
            &members,
            &killed_ids,
        )?);
    }

    if consume_turn {
        let actor = &members[actor_index];
        let speed = member_status(actor, "current_status")?
            .get_field_by_name("speed")
            .and_then(|value| value.as_i32())
            .ok_or(StateError::InvalidRequest)?;
        movements.push(consume_timeline_turn(
            proto,
            &mut units,
            &members,
            actor_id,
            action_wait(speed, skill.wait),
            base_wait(speed),
            actor_type == 1,
        )?);
    }
    state.set_field_by_name(
        "members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );
    state.set_field_by_name(
        "timeline_units",
        Value::List(units.into_iter().map(Value::Message).collect()),
    );
    Ok((results, movements, total_damage))
}

struct ResolvedSkillAction {
    skill_results: Vec<DynamicMessage>,
    before_effect_results: Vec<DynamicMessage>,
    effect_results: Vec<DynamicMessage>,
    timeline_moves: Vec<DynamicMessage>,
    total_damage: i64,
    pending_actions: Vec<effects::PendingAction>,
    protection: Option<effects::Protection>,
}

#[allow(clippy::too_many_arguments)]
fn resolve_skill_action(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    state: &mut DynamicMessage,
    actor_id: i32,
    actor_type: i32,
    skill: &TutorialSkill,
    source_effects: &[TutorialSkillEffect],
    resources: Option<&DynamicMessage>,
    runtime: &mut effects::Runtime,
    target_ids: &[i32],
    panel: (i128, i128),
    tools: Option<&[BattlePartyTool]>,
    consume_turn: bool,
    consume_panel: bool,
    allow_nested: bool,
    secret: &[u8],
    transaction: &str,
    action_number: i32,
) -> Result<ResolvedSkillAction, StateError> {
    let panel_context = state.clone();
    let actor = message_list(state, "members")
        .into_iter()
        .find(|member| member_id(member).ok() == Some(actor_id))
        .ok_or(StateError::InvalidRequest)?;
    let transformation = tools
        .is_none()
        .then(|| effects::skill_transformation(&actor, skill.id))
        .transpose()?
        .flatten();
    let transformed_skill = transformation
        .map(|(_, destination)| rule_skill(rules, destination).cloned())
        .transpose()?;
    let effective_skill = transformed_skill.as_ref().unwrap_or(skill);
    let effective_effects = transformed_skill
        .as_ref()
        .map_or(source_effects, |skill| skill.effects.as_slice());
    let (target_ids, protection) =
        runtime.redirect_targets(state, actor_type, effective_skill, target_ids)?;
    let effect_target_ids = target_ids.iter().copied().fold(Vec::new(), |mut ids, id| {
        if !ids.contains(&id) {
            ids.push(id);
        }
        ids
    });
    let special_counter = if allow_nested {
        runtime.collect_special_counter(
            rules,
            state,
            actor_id,
            effective_skill,
            &target_ids,
            secret,
            transaction,
            action_number,
        )?
    } else {
        None
    };
    let cancelled = special_counter.is_some();
    let mut before_effect_results = if cancelled {
        Vec::new()
    } else {
        if let Some((effect_id, _)) = transformation {
            let form_effects = source_effects
                .iter()
                .filter(|effect| effect.id == effect_id)
                .cloned()
                .collect::<Vec<_>>();
            runtime.apply_for_action_with_rules(
                proto,
                rules,
                state,
                actor_id,
                skill.id,
                &form_effects,
                &effect_target_ids,
                true,
                "before",
                Some(&panel_context),
                skill.state_change_application_rate,
                secret,
                transaction,
                action_number,
            )?
        } else {
            Vec::new()
        }
    };
    if !cancelled {
        before_effect_results.extend(runtime.apply_for_action_with_rules(
            proto,
            rules,
            state,
            actor_id,
            effective_skill.id,
            effective_effects,
            &effect_target_ids,
            tools.is_none(),
            "before",
            Some(&panel_context),
            effective_skill.state_change_application_rate,
            secret,
            transaction,
            action_number,
        )?);
    }
    let (skill_results, timeline_moves, total_damage) = apply_attack_results(
        proto,
        rules,
        state,
        actor_id,
        actor_type,
        effective_skill,
        effective_effects,
        resources,
        runtime,
        &target_ids,
        panel,
        tools,
        consume_turn,
        cancelled,
        secret,
        transaction,
        action_number,
    )?;
    runtime.expire_with_attributes(
        actor_id,
        &skill_results,
        consume_turn,
        !cancelled && tools.is_none() && effective_skill.skill_effect_type == 1,
        &effective_skill.attack_attributes,
    );
    if consume_panel {
        runtime.consume_panel_potency(&panel_context, actor_id)?;
    }
    let mut effect_results = Vec::new();
    let mut pending_actions = special_counter.into_iter().collect::<Vec<_>>();
    if !cancelled {
        let effect_targets = skill_results
            .iter()
            .filter(|result| !bool_field(result, "is_miss") && !bool_field(result, "is_invalid"))
            .filter_map(|result| i32_field(result, "target_id"))
            .fold(Vec::new(), |mut ids, id| {
                if !ids.contains(&id) {
                    ids.push(id);
                }
                ids
            });
        effect_results = runtime.apply_for_action_with_rules(
            proto,
            rules,
            state,
            actor_id,
            effective_skill.id,
            effective_effects,
            &effect_targets,
            tools.is_none(),
            "after",
            Some(&panel_context),
            effective_skill.state_change_application_rate,
            secret,
            transaction,
            action_number,
        )?;
        if consume_turn && tools.is_none() && transformation.is_none() {
            effects::advance_skill_lamp(state, actor_id, skill.id)?;
        }
        effect_results.extend(runtime.trigger_lamps_after_action(
            proto,
            rules,
            state,
            &panel_context,
            actor_id,
            effective_skill,
            &skill_results,
            tools.is_none(),
        )?);
        if tools.is_none() {
            effect_results.extend(runtime.trigger_attack_after(
                proto,
                rules,
                state,
                actor_id,
                effective_skill,
                &skill_results,
            )?);
            if allow_nested {
                pending_actions.extend(runtime.collect_nested_actions(
                    rules,
                    state,
                    actor_id,
                    effective_skill,
                    &skill_results,
                    secret,
                    transaction,
                    action_number,
                )?);
            }
        }
    }
    runtime.refresh(proto, state)?;
    Ok(ResolvedSkillAction {
        skill_results,
        before_effect_results,
        effect_results,
        timeline_moves,
        total_damage,
        pending_actions,
        protection: (!cancelled).then_some(protection).flatten(),
    })
}

#[allow(clippy::too_many_arguments)]
fn append_nested_actions(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    state: &mut DynamicMessage,
    runtime: &mut effects::Runtime,
    resources: Option<&DynamicMessage>,
    secret: &[u8],
    transaction: &str,
    total_turn: i32,
    pending: Vec<effects::PendingAction>,
    actions: &mut Vec<DynamicMessage>,
    action_number: &mut i32,
    generated_actions: &mut usize,
) -> Result<(), StateError> {
    for pending in pending {
        if *generated_actions >= 10_000 {
            return Err(StateError::TutorialRules(
                "battle scheduler exceeded 10,000 generated actions".into(),
            ));
        }
        let members = message_list(state, "members");
        let Some(actor) = members.iter().find(|member| {
            i32_field(member, "member_id") == Some(pending.actor_id)
                && bool_field(member, "is_alive")
        }) else {
            continue;
        };
        if !members.iter().any(|member| {
            i32_field(member, "member_id") == Some(pending.target_id)
                && bool_field(member, "is_alive")
        }) {
            continue;
        }
        let actor_type = member_type(actor)?;
        let skill = rule_skill(rules, pending.skill_id)?;
        let targets = select_target_ids(
            state,
            pending.actor_id,
            actor_type,
            pending.target_id,
            skill,
        )?;
        let resolved = resolve_skill_action(
            proto,
            rules,
            state,
            pending.actor_id,
            actor_type,
            skill,
            &skill.effects,
            resources,
            runtime,
            &targets,
            runtime.panel_multiplier(state)?,
            None,
            false,
            false,
            false,
            secret,
            transaction,
            *action_number,
        )?;
        heal_nested_burst_gauge(rules, state, pending.actor_id, pending.kind)?;
        state.set_field_by_name("total_turn", Value::I32(total_turn));
        refresh_burst_enable(rules, state)?;
        let mut action = build_action(
            proto,
            *action_number,
            state.clone(),
            actor_type,
            pending.actor_id,
            skill,
            pending.target_id,
            resolved.skill_results,
            resolved.before_effect_results,
            resolved.effect_results,
            resolved.timeline_moves,
            0,
            None,
            resolved.total_damage,
            resolved.protection,
        )?;
        match pending.kind {
            effects::NestedActionKind::Counter => {
                action.set_field_by_name("is_counter", Value::Bool(true));
            }
            effects::NestedActionKind::AdditionalAttack => {
                action.set_field_by_name("is_additional_attack", Value::Bool(true));
            }
            effects::NestedActionKind::SpecialCounter => {
                action.set_field_by_name("is_special_counter", Value::Bool(true));
            }
        }
        actions.push(action);
        *action_number = action_number
            .checked_add(1)
            .ok_or(StateError::InvalidRequest)?;
        *generated_actions += 1;
    }
    Ok(())
}

fn heal_nested_burst_gauge(
    rules: &TutorialRules,
    state: &mut DynamicMessage,
    actor_id: i32,
    kind: effects::NestedActionKind,
) -> Result<(), StateError> {
    let gain = match kind {
        effects::NestedActionKind::AdditionalAttack => {
            rules.constants.burst_gauge_heal_additional_attack
        }
        effects::NestedActionKind::Counter | effects::NestedActionKind::SpecialCounter => {
            rules.constants.burst_gauge_heal_counter
        }
    };
    let mut members = message_list(state, "members");
    let actor = members
        .iter_mut()
        .find(|member| i32_field(member, "member_id") == Some(actor_id))
        .ok_or(StateError::InvalidRequest)?;
    let mut gauge = member_status(actor, "burst_gauge")?;
    let current = i32_field(&gauge, "current_gauge").unwrap_or_default();
    let maximum = i32_field(&gauge, "max_gauge")
        .unwrap_or(rules.constants.burst_gauge_required_for_one_burst_skill);
    gauge.set_field_by_name(
        "current_gauge",
        Value::I32(current.saturating_add(gain).min(maximum)),
    );
    actor.set_field_by_name("burst_gauge", Value::Message(gauge));
    state.set_field_by_name(
        "members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );
    Ok(())
}

pub(crate) fn refresh_burst_enable(
    rules: &TutorialRules,
    state: &mut DynamicMessage,
) -> Result<(), StateError> {
    let current_actor_id = current_actor(state)
        .ok()
        .and_then(|member| member_id(&member).ok());
    let panel_burst = is_burst_panel_id(current_panel_id(state));
    let mut members = message_list(state, "members");
    for member in members
        .iter_mut()
        .filter(|member| member_type(member).ok() == Some(0))
    {
        let Some(mut gauge) = member
            .get_field_by_name("burst_gauge")
            .and_then(|value| value.as_message().cloned())
        else {
            continue;
        };
        let mut current = i32_field(&gauge, "current_gauge").unwrap_or(0);
        if panel_burst && current_actor_id == i32_field(member, "member_id") {
            current = current.max(rules.constants.burst_gauge_required_for_one_burst_skill);
            gauge.set_field_by_name("current_gauge", Value::I32(current));
        }
        gauge.set_field_by_name(
            "is_enable",
            Value::Bool(current >= rules.constants.burst_gauge_required_for_one_burst_skill),
        );
        member.set_field_by_name("burst_gauge", Value::Message(gauge));
    }
    state.set_field_by_name(
        "members",
        Value::List(members.into_iter().map(Value::Message).collect()),
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn reduce_battle_attack_with_effects(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    mut state: DynamicMessage,
    request: &DynamicMessage,
    secret: &[u8],
    start_txid: &str,
    resources: Option<&DynamicMessage>,
    effect_runtime: &mut effects::Runtime,
) -> Result<BattleAttackMutation, StateError> {
    effect_runtime.prepare(&state, start_txid)?;
    effect_runtime.refresh(proto, &mut state)?;
    effect_runtime.acquire_current_panel(proto, rules, &mut state)?;
    let mode = i32_or_enum_field(request, "mode").ok_or(StateError::InvalidRequest)?;
    let command_fields = [
        "skill_command",
        "active_skill_command",
        "battle_tool_command",
        "battle_tool_mix_command",
        "ship_tool_command",
        "start_auto_command",
        "stop_auto_command",
    ];
    let present = command_fields
        .iter()
        .filter(|field| request.has_field_by_name(field))
        .count();
    let valid_command = match mode {
        0 => present == 1 && request.has_field_by_name("skill_command"),
        1 => present == 1 && request.has_field_by_name("battle_tool_command"),
        6 => present == 0,
        7 => present == 1 && request.has_field_by_name("active_skill_command"),
        8 => present == 1 && request.has_field_by_name("battle_tool_mix_command"),
        9 => present == 1 && request.has_field_by_name("ship_tool_command"),
        _ => false,
    };
    if !valid_command {
        return Err(StateError::InvalidRequest);
    }
    validate_formation(request, &state)?;
    if current_battle_status(&state)? != BATTLE_STATUS_IN_BATTLE {
        return Err(StateError::InvalidRequest);
    }

    let before = state.clone();
    let mut history = empty_message(proto, "blend.model.BattleHistory")?;
    let mut setups = Vec::new();
    let mut actions = Vec::new();
    let mut wave_starts = Vec::new();
    // Action numbers include tool actions; total_turn counts timeline turns.
    let turn_number = i32_field(&state, "total_turn").unwrap_or(1).max(1);
    let mut action_number = effect_runtime.next_action_number.max(turn_number);
    if mode == 6 {
        let actor = current_actor(&state)?;
        if member_type(&actor)? != 0 {
            return Err(StateError::InvalidRequest);
        }
        let actor_id = member_id(&actor)?;
        state.set_field_by_name("party_gauge", Value::I32(rules.constants.max_party_gauge));
        refresh_burst_enable(rules, &mut state)?;
        // TutorialMaxPartyGauge is a state command.  It deliberately emits no
        // fabricated skill/action; the setup carries the computed new state.
        setups.push(build_action_setup(
            proto,
            rules,
            action_number,
            state.clone(),
            0,
            actor_id,
            resources,
            Some(effect_runtime),
        )?);
    } else {
        let mut total_turn = if mode == 7 {
            turn_number
        } else {
            turn_number
                .checked_add(1)
                .ok_or(StateError::InvalidRequest)?
        };
        let actor = current_actor(&state)?;
        let actor_id = member_id(&actor)?;
        let actor_type = member_type(&actor)?;
        if actor_type != 0 {
            return Err(StateError::InvalidRequest);
        }
        let mut panel_only_burst = false;
        struct ActionSpec {
            skill: TutorialSkill,
            target_id: i32,
            mode: i32,
            command_value: Option<i32>,
            battle_tool_numbers: Vec<i32>,
            battle_tools: Vec<BattlePartyTool>,
        }
        let mut action_specs = Vec::new();
        if mode == 0 {
            let command = request
                .get_field_by_name("skill_command")
                .and_then(|value| value.as_message().cloned())
                .ok_or(StateError::InvalidRequest)?;
            let skill_type = i32_field(&command, "skill_type").ok_or(StateError::InvalidRequest)?;
            let target_id =
                i32_field(&command, "main_target_id").ok_or(StateError::InvalidRequest)?;
            if !(1..=3).contains(&skill_type) {
                return Err(StateError::InvalidRequest);
            }
            let selected = member_skills(&actor)?
                .into_iter()
                .find(|selected| selected.skill_type == skill_type)
                .ok_or(StateError::InvalidRequest)?;
            let selected_skill = rule_skill(rules, selected.id)?;
            if selected_skill.skill_target_type == Some(3)
                && effect_runtime
                    .provocation_target(actor_id)
                    .is_some_and(|forced| forced != target_id)
            {
                return Err(StateError::InvalidRequest);
            }
            if skill_type == 3 {
                let gauge = member_status(&actor, "burst_gauge")?;
                let current = i32_field(&gauge, "current_gauge").unwrap_or(0);
                let panel_id = current_panel_id(&state);
                if current < rules.constants.burst_gauge_required_for_one_burst_skill
                    && !is_burst_panel_id(panel_id)
                {
                    return Err(StateError::InvalidRequest);
                }
                panel_only_burst = current
                    < rules.constants.burst_gauge_required_for_one_burst_skill
                    && is_burst_panel_id(panel_id);
            }
            action_specs.push(ActionSpec {
                skill: selected_skill.clone(),
                target_id,
                mode: 0,
                command_value: None,
                battle_tool_numbers: Vec::new(),
                battle_tools: Vec::new(),
            });
        } else if mode == 7 {
            let command = request
                .get_field_by_name("active_skill_command")
                .and_then(|value| value.as_message().cloned())
                .ok_or(StateError::InvalidRequest)?;
            let active_skill_type =
                i32_field(&command, "active_skill_type").ok_or(StateError::InvalidRequest)?;
            let target_id =
                i32_field(&command, "main_target_id").ok_or(StateError::InvalidRequest)?;
            let selected = member_active_skills(&actor)?
                .into_iter()
                .find(|selected| selected.skill_type == active_skill_type)
                .ok_or(StateError::InvalidRequest)?;
            let selected_skill = rule_skill(rules, selected.id)?;
            if selected_skill.require_command_value != command.has_field_by_name("value") {
                return Err(StateError::InvalidRequest);
            }
            let mut members = message_list(&state, "members");
            let member = members
                .iter_mut()
                .find(|member| i32_field(member, "member_id") == Some(actor_id))
                .ok_or(StateError::InvalidRequest)?;
            let mut ally = member_status(member, "ally")?;
            let mut active_skills = message_list(&ally, "active_skills");
            let active = active_skills
                .iter_mut()
                .find(|candidate| {
                    i32_field(candidate, "active_skill_type") == Some(active_skill_type)
                })
                .ok_or(StateError::InvalidRequest)?;
            let remaining = optional_i32_field(active, "rest_count").unwrap_or(0);
            if remaining <= 0 {
                return Err(StateError::InvalidRequest);
            }
            active.set_field_by_name(
                "rest_count",
                Value::Message(wrapper_i32(proto, remaining - 1)?),
            );
            ally.set_field_by_name(
                "active_skills",
                Value::List(active_skills.into_iter().map(Value::Message).collect()),
            );
            member.set_field_by_name("ally", Value::Message(ally));
            state.set_field_by_name(
                "members",
                Value::List(members.into_iter().map(Value::Message).collect()),
            );
            action_specs.push(ActionSpec {
                skill: selected_skill.clone(),
                target_id,
                mode: 7,
                command_value: Some(active_skill_type),
                battle_tool_numbers: Vec::new(),
                battle_tools: Vec::new(),
            });
        } else if mode == 8 {
            let command = request
                .get_field_by_name("battle_tool_mix_command")
                .and_then(|value| value.as_message().cloned())
                .ok_or(StateError::InvalidRequest)?;
            let numbers = i32_list(&command, "battle_tool_numbers");
            let battle_tools = message_list(&state, "battle_tools");
            if numbers.len() != 2
                || numbers[0] == numbers[1]
                || i32_field(&state, "party_gauge").unwrap_or(0) < rules.constants.max_party_gauge
                || !member_can_use_battle_tool_mix(rules, &state, actor_id)?
            {
                return Err(StateError::InvalidRequest);
            }
            let selected_tools = numbers
                .iter()
                .map(|number| {
                    let tool = battle_tools
                        .iter()
                        .find(|tool| i32_field(tool, "number") == Some(*number))
                        .ok_or(StateError::InvalidRequest)?;
                    if i32_field(tool, "usage_count").ok_or(StateError::InvalidRequest)? <= 0 {
                        return Err(StateError::InvalidRequest);
                    }
                    battle_party_tool_from_state(rules, tool)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let skill = battle_tool_mix_skill(rules, &selected_tools[0], &selected_tools[1])?;
            let target_id = member_id(&earliest_living_member(&state, Some(1))?)?;
            action_specs.push(ActionSpec {
                skill,
                target_id,
                mode: 8,
                command_value: None,
                battle_tool_numbers: numbers,
                battle_tools: selected_tools,
            });
            state.set_field_by_name("party_gauge", Value::I32(0));
        } else if mode == 1 {
            let command = request
                .get_field_by_name("battle_tool_command")
                .and_then(|value| value.as_message().cloned())
                .ok_or(StateError::InvalidRequest)?;
            let numbers = i32_list(&command, "battle_tool_numbers");
            let battle_tools = message_list(&state, "battle_tools");
            if numbers.is_empty()
                || numbers.len()
                    > usize::try_from(rules.constants.turn_max_battle_tool_count).unwrap_or(0)
                || numbers
                    .iter()
                    .enumerate()
                    .any(|(index, number)| numbers[..index].contains(number))
                || i32_field(&state, "party_gauge").unwrap_or(0) < rules.constants.max_party_gauge
            {
                return Err(StateError::InvalidRequest);
            }
            for number in numbers {
                let tool = battle_tools
                    .iter()
                    .find(|tool| i32_field(tool, "number") == Some(number))
                    .ok_or(StateError::InvalidRequest)?;
                if i32_field(tool, "usage_count").ok_or(StateError::InvalidRequest)? <= 0 {
                    return Err(StateError::InvalidRequest);
                }
                let selected_tool = battle_party_tool_from_state(rules, tool)?;
                let skill_id = i32_field(tool, "tool_id")
                    .and_then(|tool_id| rules.battle_tools.iter().find(|row| row.id == tool_id))
                    .map(|tool| tool.skill_id)
                    .ok_or(StateError::InvalidRequest)?;
                let skill = rule_skill(rules, skill_id)?;
                let target_id = if skill.skill_target_type == Some(2) {
                    member_id(&most_injured_living_member(&state, 0)?)?
                } else {
                    member_id(&earliest_living_member(&state, Some(1))?)?
                };
                action_specs.push(ActionSpec {
                    skill: skill.clone(),
                    target_id,
                    mode: 1,
                    command_value: Some(number),
                    battle_tool_numbers: vec![number],
                    battle_tools: vec![selected_tool],
                });
            }
            state.set_field_by_name("party_gauge", Value::I32(0));
        } else {
            let command = request
                .get_field_by_name("ship_tool_command")
                .and_then(|value| value.as_message().cloned())
                .ok_or(StateError::InvalidRequest)?;
            let number =
                i32_field(&command, "ship_tool_number").ok_or(StateError::InvalidRequest)?;
            if i32_field(&state, "bomb_gauge").unwrap_or_default() < rules.constants.max_bomb_gauge
            {
                return Err(StateError::InvalidRequest);
            }
            let tool = message_list(&state, "ship_tools")
                .into_iter()
                .find(|tool| i32_field(tool, "number") == Some(number))
                .filter(|tool| i32_field(tool, "usage_count").unwrap_or_default() > 0)
                .ok_or(StateError::InvalidRequest)?;
            let skill = rule_skill(
                rules,
                i32_field(&tool, "skill_id").ok_or(StateError::InvalidRequest)?,
            )?;
            let target_id = if matches!(skill.skill_target_type, Some(2 | 4)) {
                member_id(&most_injured_living_member(&state, 0)?)?
            } else {
                member_id(&earliest_living_member(&state, Some(1))?)?
            };
            action_specs.push(ActionSpec {
                skill: skill.clone(),
                target_id,
                mode: 9,
                command_value: Some(number),
                battle_tool_numbers: Vec::new(),
                battle_tools: Vec::new(),
            });
            state.set_field_by_name("bomb_gauge", Value::I32(0));
        }
        let action_count = action_specs.len();
        let mut generated_actions = 0usize;
        for (action_index, action) in action_specs.into_iter().enumerate() {
            let skill = &action.skill;
            let target_id = action.target_id;
            let action_mode = action.mode;
            let command_value = action.command_value;
            let action_targets = select_target_ids(&state, actor_id, actor_type, target_id, skill)?;
            let panel = effect_runtime.panel_multiplier(&state)?;
            let consumes_panel = action_mode != 7
                && (mode != 1 || action_index + 1 == action_count);
            if matches!(action_mode, 1 | 8) {
                let mut tools = message_list(&state, "battle_tools");
                for number in &action.battle_tool_numbers {
                    let tool = tools
                        .iter_mut()
                        .find(|tool| i32_field(tool, "number") == Some(*number))
                        .ok_or(StateError::InvalidRequest)?;
                    let usage = i32_field(tool, "usage_count").ok_or(StateError::InvalidRequest)?;
                    tool.set_field_by_name("usage_count", Value::I32(usage - 1));
                }
                state.set_field_by_name(
                    "battle_tools",
                    Value::List(tools.into_iter().map(Value::Message).collect()),
                );
            }
            if action_mode == 9 {
                let mut tools = message_list(&state, "ship_tools");
                let tool = tools
                    .iter_mut()
                    .find(|tool| i32_field(tool, "number") == command_value)
                    .ok_or(StateError::InvalidRequest)?;
                let usage = i32_field(tool, "usage_count").ok_or(StateError::InvalidRequest)?;
                tool.set_field_by_name("usage_count", Value::I32(usage - 1));
                state.set_field_by_name(
                    "ship_tools",
                    Value::List(tools.into_iter().map(Value::Message).collect()),
                );
            }
            let mut source_effects = skill.effects.clone();
            for tool in &action.battle_tools {
                source_effects.extend(battle_tool_effects(rules, tool)?);
            }
            let policy_tools =
                matches!(action_mode, 1 | 8 | 9).then_some(action.battle_tools.as_slice());
            let mut resolved = resolve_skill_action(
                proto,
                rules,
                &mut state,
                actor_id,
                actor_type,
                skill,
                &source_effects,
                resources,
                effect_runtime,
                &action_targets,
                panel,
                policy_tools,
                action_mode == 0,
                consumes_panel,
                policy_tools.is_none(),
                secret,
                start_txid,
                action_number,
            )?;
            // A multi-tool command emits one action per tool but consumes one panel.
            if consumes_panel {
                consume_timeline_panel(proto, rules, &mut state)?;
                effect_runtime.acquire_current_panel(proto, rules, &mut state)?;
            }

            let mut members = message_list(&state, "members");
            let member = members
                .iter_mut()
                .find(|member| i32_field(member, "member_id") == Some(actor_id))
                .ok_or(StateError::InvalidRequest)?;
            if action_mode == 0 {
                let mut gauge = member_status(member, "burst_gauge")?;
                let old = i32_field(&gauge, "current_gauge").unwrap_or(0);
                let maximum = i32_field(&gauge, "max_gauge")
                    .unwrap_or(rules.constants.burst_gauge_required_for_one_burst_skill);
                let gain = match skill.skill_type {
                    1 => rules.constants.burst_gauge_heal_normal1,
                    2 => rules.constants.burst_gauge_heal_normal2,
                    _ => 0,
                };
                let next = if skill.skill_type == 3 {
                    if panel_only_burst {
                        old
                    } else {
                        old.saturating_sub(rules.constants.burst_gauge_required_for_one_burst_skill)
                    }
                } else {
                    old.saturating_add(gain).min(maximum)
                };
                gauge.set_field_by_name("current_gauge", Value::I32(next));
                member.set_field_by_name("burst_gauge", Value::Message(gauge));
                state.set_field_by_name(
                    "party_gauge",
                    Value::I32(
                        i32_field(&state, "party_gauge")
                            .unwrap_or(0)
                            .saturating_add(100)
                            .min(rules.constants.max_party_gauge),
                    ),
                );
                state.set_field_by_name(
                    "bomb_gauge",
                    Value::I32(
                        i32_field(&state, "bomb_gauge")
                            .unwrap_or_default()
                            .saturating_add(rules.constants.bomb_gauge_recovery_amount)
                            .min(rules.constants.max_bomb_gauge),
                    ),
                );
            } else if action_mode == 7 {
                let mut gauge = member_status(member, "burst_gauge")?;
                let maximum = i32_field(&gauge, "max_gauge")
                    .unwrap_or(rules.constants.burst_gauge_required_for_one_burst_skill);
                let current = i32_field(&gauge, "current_gauge").unwrap_or_default();
                gauge.set_field_by_name(
                    "current_gauge",
                    Value::I32(
                        current
                            .saturating_add(rules.constants.burst_gauge_heal_active_skill)
                            .min(maximum),
                    ),
                );
                member.set_field_by_name("burst_gauge", Value::Message(gauge));
            }
            state.set_field_by_name(
                "members",
                Value::List(members.into_iter().map(Value::Message).collect()),
            );
            if matches!(mode, 1 | 8) && action_index + 1 == action_count {
                resolved.effect_results.extend(
                    effect_runtime.trigger_party_tool_effects(proto, rules, &mut state)?,
                );
            }
            state.set_field_by_name("total_turn", Value::I32(turn_number));
            refresh_burst_enable(rules, &mut state)?;
            actions.push(build_action(
                proto,
                action_number,
                state.clone(),
                actor_type,
                actor_id,
                skill,
                target_id,
                resolved.skill_results,
                resolved.before_effect_results,
                resolved.effect_results,
                resolved.timeline_moves,
                action_mode,
                command_value,
                resolved.total_damage,
                resolved.protection,
            )?);
            let pending_actions = resolved.pending_actions;
            action_number = action_number
                .checked_add(1)
                .ok_or(StateError::InvalidRequest)?;
            generated_actions += 1;
            append_nested_actions(
                proto,
                rules,
                &mut state,
                effect_runtime,
                resources,
                secret,
                start_txid,
                turn_number,
                pending_actions,
                &mut actions,
                &mut action_number,
                &mut generated_actions,
            )?;
        }
        state.set_field_by_name("total_turn", Value::I32(total_turn));

        loop {
            state.set_field_by_name("total_turn", Value::I32(total_turn));
            let status = current_battle_status(&state)?;
            if status == BATTLE_STATUS_LOST {
                break;
            }
            if status == BATTLE_STATUS_WON {
                let wave = i32_field(&state, "wave").unwrap_or(1);
                let wave_ids = i32_list(&state, "wave_ids");
                if usize::try_from(wave).unwrap_or(0) >= wave_ids.len() {
                    break;
                }
                let next_wave_number = wave + 1;
                let next_wave_id = wave_ids[usize::try_from(next_wave_number - 1).unwrap_or(0)];
                let wave_rule = rule_wave(rules, next_wave_id)?;
                state.set_field_by_name("wave", Value::I32(next_wave_number));
                append_wave_members(
                    proto,
                    rules,
                    &mut state,
                    wave_rule,
                    &effect_runtime.initiative_members(),
                )?;
                set_battle_field_effect(proto, &mut state, wave_rule.field_effect_id)?;
                let battle_id = i32_field(&state, "battle_id").unwrap_or_default();
                let panel_ids = tutorial_timeline_panels(rules, battle_id, next_wave_number)?;
                set_timeline_panels(
                    proto,
                    &mut state,
                    &panel_ids,
                    rules.constants.timeline_panel_count,
                    1,
                )?;
                effect_runtime.acquire_current_panel(proto, rules, &mut state)?;
                refresh_burst_enable(rules, &mut state)?;
                effect_runtime.refresh(proto, &mut state)?;
                let mut wave_start = empty_message(proto, "blend.model.BattleWaveStart")?;
                wave_start.set_field_by_name("action_number", Value::I32(action_number));
                wave_start.set_field_by_name("state", Value::Message(state.clone()));
                wave_starts.push(wave_start);
                continue;
            }

            if generated_actions >= 10_000 {
                return Err(StateError::TutorialRules(
                    "battle scheduler exceeded 10,000 generated actions".into(),
                ));
            }
            let actor = current_actor(&state)?;
            let actor_id = member_id(&actor)?;
            let actor_type = member_type(&actor)?;
            if actor_type == 1 {
                let mut members = message_list(&state, "members");
                let member = members
                    .iter_mut()
                    .find(|member| i32_field(member, "member_id") == Some(actor_id))
                    .ok_or(StateError::InvalidRequest)?;
                let mut enemy_state = member_status(member, "enemy")?;
                if bool_field(&enemy_state, "is_broken") {
                    let maximum = i32_field(&enemy_state, "max_break_gauge")
                        .unwrap_or(1)
                        .max(1);
                    enemy_state.set_field_by_name("break_gauge", Value::I32(maximum));
                    enemy_state.set_field_by_name("is_broken", Value::Bool(false));
                    member.set_field_by_name("enemy", Value::Message(enemy_state));
                    state.set_field_by_name(
                        "members",
                        Value::List(members.into_iter().map(Value::Message).collect()),
                    );
                }
            }

            let (state_change_results, disabled, killed) = effect_runtime.prepare_turn(
                proto,
                &mut state,
                actor_id,
                secret,
                start_txid,
                action_number,
            )?;
            let mut setup_timeline_moves = Vec::new();
            if killed {
                let members = message_list(&state, "members");
                let mut units = message_list(&state, "timeline_units");
                setup_timeline_moves.extend(remove_timeline_members(
                    proto,
                    &mut units,
                    &members,
                    &[actor_id],
                )?);
                state.set_field_by_name(
                    "timeline_units",
                    Value::List(units.into_iter().map(Value::Message).collect()),
                );
            } else if disabled {
                let members = message_list(&state, "members");
                let actor = members
                    .iter()
                    .find(|member| i32_field(member, "member_id") == Some(actor_id))
                    .ok_or(StateError::InvalidRequest)?;
                let speed = member_status(actor, "current_status")?
                    .get_field_by_name("speed")
                    .and_then(|value| value.as_i32())
                    .ok_or(StateError::InvalidRequest)?;
                let mut units = message_list(&state, "timeline_units");
                setup_timeline_moves.push(consume_timeline_turn(
                    proto,
                    &mut units,
                    &members,
                    actor_id,
                    base_wait(speed),
                    base_wait(speed),
                    actor_type == 1,
                )?);
                state.set_field_by_name(
                    "timeline_units",
                    Value::List(units.into_iter().map(Value::Message).collect()),
                );
            }
            if disabled || killed {
                effect_runtime.consume_panel_potency(&state, actor_id)?;
                consume_timeline_panel(proto, rules, &mut state)?;
                effect_runtime.expire(actor_id, &[], true, false);
                effect_runtime.acquire_current_panel(proto, rules, &mut state)?;
            }
            effect_runtime.refresh(proto, &mut state)?;
            refresh_burst_enable(rules, &mut state)?;
            let before_actor = state.clone();
            let mut setup = build_action_setup(
                proto,
                rules,
                action_number,
                before_actor.clone(),
                actor_type,
                actor_id,
                resources,
                Some(effect_runtime),
            )?;
            setup.set_field_by_name("is_disabled", Value::Bool(disabled || killed));
            setup.set_field_by_name(
                "state_change_results",
                Value::List(
                    state_change_results
                        .into_iter()
                        .map(Value::Message)
                        .collect(),
                ),
            );
            setup.set_field_by_name(
                "timeline_moves",
                Value::List(
                    setup_timeline_moves
                        .into_iter()
                        .map(Value::Message)
                        .collect(),
                ),
            );
            if disabled || killed {
                setup.set_field_by_name("skill_selections", Value::List(Vec::new()));
                setup.set_field_by_name("battle_tool_selections", Value::List(Vec::new()));
                setup.set_field_by_name("battle_tool_mix_selections", Value::List(Vec::new()));
                setup.set_field_by_name("ship_tool_selections", Value::List(Vec::new()));
                setup.set_field_by_name("active_skill_selections", Value::List(Vec::new()));
            }
            setups.push(setup);
            if disabled || killed {
                action_number = action_number
                    .checked_add(1)
                    .ok_or(StateError::InvalidRequest)?;
                total_turn = total_turn
                    .checked_add(1)
                    .ok_or(StateError::InvalidRequest)?;
                generated_actions += 1;
                continue;
            }
            if actor_type == 0 {
                break;
            }

            let enemy_member = message_list(&state, "members")
                .into_iter()
                .find(|member| i32_field(member, "member_id") == Some(actor_id))
                .ok_or(StateError::InvalidRequest)?;
            let enemy_rule = rule_enemy(rules, enemy_member_status_enemy_id(&enemy_member)?)?;
            let panel_id = current_panel_id(&state);
            let enemy_skill_id = if is_burst_panel_id(panel_id) {
                enemy_rule.burst_skill_id
            } else {
                let choices: Vec<i32> = rules
                    .enemy_ai_units
                    .iter()
                    .filter(|unit| unit.enemy_ai_id == enemy_rule.enemy_ai_id)
                    .flat_map(|unit| unit.skill_ids.iter().copied())
                    .collect();
                if choices.is_empty() {
                    return Err(StateError::TutorialRules(format!(
                        "enemy AI {} has no skill",
                        enemy_rule.enemy_ai_id
                    )));
                }
                let roll = deterministic_roll(
                    secret,
                    start_txid,
                    action_number,
                    b"enemy-skill",
                    actor_id,
                    0,
                );
                choices[(roll as usize) % choices.len()]
            };
            let enemy_skill = rule_skill(rules, enemy_skill_id)?;
            let target_id = if enemy_skill.skill_target_type == Some(1) {
                actor_id
            } else if matches!(enemy_skill.skill_target_type, Some(2 | 4)) {
                member_id(&most_injured_living_member(&state, 1)?)?
            } else if enemy_skill.skill_target_type == Some(3) {
                if let Some(target) = effect_runtime.provocation_target(actor_id) {
                    target
                } else {
                    effect_runtime.target_by_rate(
                        &state,
                        deterministic_roll(
                            secret,
                            start_txid,
                            action_number,
                            b"enemy-target",
                            actor_id,
                            0,
                        ),
                    )?
                }
            } else {
                member_id(&earliest_living_member(&state, Some(0))?)?
            };
            let enemy_targets = select_target_ids(&state, actor_id, 1, target_id, enemy_skill)?;
            let panel = effect_runtime.panel_multiplier(&state)?;
            let resolved = resolve_skill_action(
                proto,
                rules,
                &mut state,
                actor_id,
                1,
                enemy_skill,
                &enemy_skill.effects,
                resources,
                effect_runtime,
                &enemy_targets,
                panel,
                None,
                true,
                true,
                true,
                secret,
                start_txid,
                action_number,
            )?;
            consume_timeline_panel(proto, rules, &mut state)?;
            effect_runtime.acquire_current_panel(proto, rules, &mut state)?;
            state.set_field_by_name("total_turn", Value::I32(total_turn));
            refresh_burst_enable(rules, &mut state)?;
            actions.push(build_action(
                proto,
                action_number,
                state.clone(),
                1,
                actor_id,
                enemy_skill,
                target_id,
                resolved.skill_results,
                resolved.before_effect_results,
                resolved.effect_results,
                resolved.timeline_moves,
                0,
                None,
                resolved.total_damage,
                resolved.protection,
            )?);
            let pending_actions = resolved.pending_actions;
            action_number = action_number
                .checked_add(1)
                .ok_or(StateError::InvalidRequest)?;
            total_turn = total_turn
                .checked_add(1)
                .ok_or(StateError::InvalidRequest)?;
            generated_actions += 1;
            append_nested_actions(
                proto,
                rules,
                &mut state,
                effect_runtime,
                resources,
                secret,
                start_txid,
                total_turn,
                pending_actions,
                &mut actions,
                &mut action_number,
                &mut generated_actions,
            )?;
        }
    }
    effect_runtime.next_action_number = action_number;
    history.set_field_by_name("status", Value::EnumNumber(current_battle_status(&state)?));
    history.set_field_by_name(
        "action_setups",
        Value::List(setups.into_iter().map(Value::Message).collect()),
    );
    history.set_field_by_name(
        "actions",
        Value::List(actions.into_iter().map(Value::Message).collect()),
    );
    history.set_field_by_name(
        "wave_starts",
        Value::List(wave_starts.into_iter().map(Value::Message).collect()),
    );
    history.set_field_by_name("previous_state", Value::Message(before));
    history.set_field_by_name("auto_type", Value::EnumNumber(0));
    let mut response = empty_message(proto, "blend.api.BattleAttackResponse")?;
    response.set_field_by_name("history", Value::Message(history.clone()));
    Ok(BattleAttackMutation { state, response })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn timeline_delays_match_effect_slots_and_absolute_protocol_waits() {
        let proto = ProtoRegistry::from_file(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../schemas/atelier-resleriana-2.16.0.protoset"
        )))
        .unwrap();
        let members = [14, 4, 1, 3, 5, 2]
            .into_iter()
            .map(|member_id| {
                let mut member = empty_message(&proto, "blend.model.BattleMember").unwrap();
                member.set_field_by_name("member_id", Value::I32(member_id));
                member.set_field_by_name("is_alive", Value::Bool(true));
                member.set_field_by_name(
                    "current_status",
                    Value::Message(
                        battle_status_message(
                            &proto,
                            BattleStats {
                                hp: 1,
                                speed: 100,
                                attack: 1,
                                magic: 1,
                                defense: 1,
                                mental: 1,
                            },
                        )
                        .unwrap(),
                    ),
                );
                member
            })
            .collect::<Vec<_>>();
        let original = [
            (14, 1, 0),
            (4, 2, 64),
            (1, 2, 92),
            (3, 2, 92),
            (5, 2, 102),
            (2, 2, 126),
            (14, 2, 240),
            (4, 1, 257),
            (1, 1, 299),
            (3, 1, 299),
            (5, 1, 314),
            (2, 1, 343),
            (14, 3, 480),
        ]
        .into_iter()
        .map(|(member_id, number, wait)| {
            build_timeline_unit(&proto, member_id, number, wait).unwrap()
        })
        .collect::<Vec<_>>();

        let mut units = original.clone();
        let movements = apply_timeline_effects(
            &proto,
            &mut units,
            &members,
            14,
            0,
            &[TutorialSkillEffect {
                id: 780097002,
                value: 100,
            }],
            &[5],
        )
        .unwrap();
        assert_eq!(movements.len(), 2);
        assert_eq!(
            movements
                .iter()
                .map(|movement| (
                    i32_field(movement, "number").unwrap(),
                    optional_i32_field(movement, "wait").unwrap(),
                    i32_or_enum_field(movement, "reason").unwrap(),
                    optional_i32_field(movement, "from_index").unwrap(),
                    optional_i32_field(movement, "to_index").unwrap(),
                ))
                .collect::<Vec<_>>(),
            vec![(2, 183, 3, 4, 5), (1, 395, 3, 10, 11)]
        );

        let mut break_units = original;
        let break_moves =
            delay_timeline_member(&proto, &mut break_units, &members, 5, 200).unwrap();
        assert_eq!(
            break_moves
                .iter()
                .map(|movement| (
                    optional_i32_field(movement, "wait").unwrap(),
                    i32_or_enum_field(movement, "reason").unwrap(),
                ))
                .collect::<Vec<_>>(),
            vec![(302, 2), (514, 2)]
        );
    }
}
