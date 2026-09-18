use super::prelude::*;

pub(crate) fn current_panel_id(state: &DynamicMessage) -> i32 {
    message_list(state, "timeline_panels")
        .first()
        .and_then(|panel| optional_i32_field(panel, "panel_id"))
        .unwrap_or(11)
}

/// Both client timeline rows 14 and 17 are the canonical Burst panel type.
/// They are separate master-data rows because one is used by tutorial data and
/// the other by regular battle data; treating only row 14 as Burst locks the
/// Burst skill whenever a normal battle emits row 17.
pub(crate) fn is_burst_panel_id(panel_id: i32) -> bool {
    matches!(panel_id, 14 | 17)
}

pub(crate) fn consume_timeline_panel(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    state: &mut DynamicMessage,
) -> Result<(), StateError> {
    let mut panels = message_list(state, "timeline_panels");
    if panels.is_empty() {
        return Err(StateError::InvalidRequest);
    }
    let last_turn = panels
        .last()
        .and_then(|panel| i32_field(panel, "turn"))
        .ok_or(StateError::InvalidRequest)?;
    let battle_id = i32_field(state, "battle_id").ok_or(StateError::InvalidRequest)?;
    let wave = i32_field(state, "wave").unwrap_or(1).max(1);
    let sequence = tutorial_timeline_panels(rules, battle_id, wave)?;
    let next_turn = last_turn.saturating_add(1);
    let index = usize::try_from(next_turn.saturating_sub(1)).unwrap_or(0) % sequence.len();
    panels.remove(0);
    panels.push(build_timeline_panel(proto, sequence[index], next_turn)?);
    state.set_field_by_name(
        "timeline_panels",
        Value::List(panels.into_iter().map(Value::Message).collect()),
    );
    Ok(())
}

pub(crate) fn set_timeline_panels(
    proto: &ProtoRegistry,
    state: &mut DynamicMessage,
    ids: &[i32],
    count: i32,
    start_turn: i32,
) -> Result<(), StateError> {
    if ids.is_empty() || count <= 0 {
        return Err(StateError::TutorialRules(
            "timeline panel closure is empty".into(),
        ));
    }
    let panels = (0..count)
        .map(|offset| {
            let index = usize::try_from(start_turn.saturating_sub(1).saturating_add(offset).max(0))
                .unwrap_or(0)
                % ids.len();
            build_timeline_panel(proto, ids[index], start_turn.saturating_add(offset))
                .map(Value::Message)
        })
        .collect::<Result<Vec<_>, _>>()?;
    state.set_field_by_name("timeline_panels", Value::List(panels));
    Ok(())
}

pub(crate) fn tutorial_timeline_panels(
    rules: &TutorialRules,
    battle_id: i32,
    wave: i32,
) -> Result<Vec<i32>, StateError> {
    Ok(match (battle_id, wave) {
        (10000279, 1) => vec![11; 10],
        (10000279, 2) => vec![12, 11, 11, 11, 11, 11, 11, 11, 11, 11],
        (10000279, 3) => vec![11, 11, 11, 11, 14, 11, 11, 11, 11, 11],
        (10000280, 1) => vec![11, 14, 11, 11, 11, 11, 11, 11, 11, 11],
        (10000003, 1) => vec![11, 11, 14, 11, 11, 11, 11, 11, 11, 11],
        (10000004, 1) => vec![11, 12, 11, 14, 14, 13, 12, 11, 14, 11],
        _ => rule_battle(rules, battle_id)?.timeline_panel_ids.clone(),
    })
}

pub(crate) fn tutorial_timeline_units(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    battle_id: i32,
    wave: i32,
    members: &[DynamicMessage],
    initiative_members: &BTreeSet<i32>,
) -> Result<Vec<DynamicMessage>, StateError> {
    let rows: &[(i32, i32, i32)] = match (battle_id, wave) {
        (10000279, 1) => &[
            (1, 1, 0),
            (11, 1, 35),
            (12, 1, 35),
            (11, 2, 224),
            (12, 2, 224),
            (1, 2, 236),
            (11, 3, 413),
            (12, 3, 413),
        ],
        (10000279, 2) => &[
            (1, 1, 0),
            (11, 1, 35),
            (12, 1, 35),
            (1, 2, 236),
            (11, 2, 274),
            (12, 2, 274),
            (11, 3, 513),
            (12, 3, 513),
        ],
        (10000279, 3) => &[
            (1, 1, 0),
            (11, 1, 35),
            (12, 1, 35),
            (13, 1, 35),
            (1, 2, 236),
            (11, 2, 274),
            (12, 2, 274),
            (13, 2, 274),
            (11, 3, 513),
            (12, 3, 513),
            (13, 3, 513),
        ],
        (10000280, 1) => &[
            (1, 1, 0),
            (11, 1, 39),
            (12, 1, 39),
            (13, 1, 39),
            (1, 2, 241),
            (11, 2, 291),
            (12, 2, 291),
            (13, 2, 291),
            (11, 3, 543),
            (12, 3, 543),
            (13, 3, 543),
        ],
        (10000003, 1) => &[
            (1, 1, 0),
            (2, 1, 12),
            (13, 1, 53),
            (1, 2, 236),
            (2, 2, 260),
            (11, 1, 353),
            (12, 1, 353),
            (14, 1, 353),
            (15, 1, 353),
            (13, 2, 512),
            (11, 2, 843),
            (12, 2, 843),
            (14, 2, 843),
            (15, 2, 843),
        ],
        (10000004, 1) => &[
            (1, 1, 0),
            (2, 1, 14),
            (12, 1, 33),
            (1, 2, 234),
            (2, 2, 262),
            (11, 1, 277),
            (13, 1, 297),
            (12, 2, 444),
            (11, 2, 740),
            (13, 2, 760),
        ],
        _ => {
            let mut units = quest::initial_timeline(proto, rules, members)?;
            prioritize_initiative(&mut units, members, initiative_members)?;
            return Ok(units);
        }
    };
    let mut units = rows
        .iter()
        .map(|(member_id, number, wait)| build_timeline_unit(proto, *member_id, *number, *wait))
        .collect::<Result<Vec<_>, _>>()?;
    prioritize_initiative(&mut units, members, initiative_members)?;
    Ok(units)
}

fn prioritize_initiative(
    units: &mut [DynamicMessage],
    members: &[DynamicMessage],
    initiative_members: &BTreeSet<i32>,
) -> Result<(), StateError> {
    if initiative_members.is_empty() {
        return Ok(());
    }
    let mut initiative_max = None;
    let mut other_min = None;
    for unit in units
        .iter()
        .filter(|unit| i32_field(unit, "number") == Some(1))
    {
        let id = member_id(unit)?;
        let wait = i32_field(unit, "wait").ok_or(StateError::InvalidRequest)?;
        if initiative_members.contains(&id) {
            initiative_max = Some(initiative_max.map_or(wait, |value: i32| value.max(wait)));
        } else {
            other_min = Some(other_min.map_or(wait, |value: i32| value.min(wait)));
        }
    }
    let (Some(initiative_max), Some(other_min)) = (initiative_max, other_min) else {
        return Ok(());
    };
    if initiative_max < other_min {
        return Ok(());
    }
    let delay = initiative_max
        .checked_add(1)
        .and_then(|value| value.checked_sub(other_min))
        .ok_or(StateError::InvalidRequest)?;
    for unit in units.iter_mut() {
        if !initiative_members.contains(&member_id(unit)?) {
            let wait = i32_field(unit, "wait")
                .and_then(|value| value.checked_add(delay))
                .ok_or(StateError::InvalidRequest)?;
            unit.set_field_by_name("wait", Value::I32(wait));
        }
    }
    sort_timeline_units(units, members);
    Ok(())
}

pub(crate) fn member_offense_and_defense(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    attacker: &DynamicMessage,
    target: &DynamicMessage,
    runtime: Option<&effects::Runtime>,
    skill: &TutorialSkill,
) -> Result<(i32, i32, i32), StateError> {
    let attribute = preferred_attack_attribute(target, skill)?;
    let (offense_field, defense_field) = match attribute {
        1..=3 => ("attack", "defense"),
        5..=8 => ("magic", "mental"),
        _ => {
            return Err(StateError::TutorialRules(format!(
                "unsupported tutorial attribute {attribute}"
            )))
        }
    };
    let attacker_status = member_status(attacker, "current_status")?;
    let offense = i32_field(&attacker_status, offense_field)
        .ok_or(StateError::InvalidRequest)?
        .max(1);
    let defense = if member_type(target)? == 1 {
        let enemy_id = enemy_member_status_enemy_id(target)?;
        let enemy = rule_enemy(rules, enemy_id)?;
        let (stats, _) = enemy_stats(enemy, member_level(proto, rules, target, None)?)?;
        let value = if defense_field == "defense" {
            stats.defense
        } else {
            stats.mental
        };
        let value = if matches!(enemy_id, 80001050 | 80001051) {
            value * 4 / 5
        } else {
            value
        };
        let target_id = member_id(target)?;
        let rate = (10_000
            + runtime.map_or(0, |runtime| runtime.stat_rate(target_id, defense_field)))
        .clamp(0, 1_000_000);
        checked_i32(i64::from(value) * rate / 10_000)?
    } else {
        i32_field(&member_status(target, "current_status")?, defense_field)
            .ok_or(StateError::InvalidRequest)?
    }
    .max(1);
    Ok((offense, defense, attribute))
}

pub(crate) fn state_change_summary_value(member: &DynamicMessage, id: i32) -> i32 {
    message_list(member, "state_change_summaries")
        .into_iter()
        .find(|summary| i32_field(summary, "id") == Some(id))
        .and_then(|summary| i32_field(&summary, "value"))
        .unwrap_or(0)
}

pub(crate) fn resistance_name(attribute: i32) -> Result<&'static str, StateError> {
    match attribute {
        1 => Ok("slashing"),
        2 => Ok("impact"),
        3 => Ok("piercing"),
        5 => Ok("fire"),
        6 => Ok("ice"),
        7 => Ok("lightning"),
        8 => Ok("wind"),
        _ => Err(StateError::TutorialRules(format!(
            "unsupported tutorial attribute {attribute}"
        ))),
    }
}

pub(crate) fn target_resistance(
    target: &DynamicMessage,
    attribute: i32,
) -> Result<i32, StateError> {
    let resistance = member_status(target, "resistance")?;
    Ok(i32_field(&resistance, resistance_name(attribute)?).unwrap_or(0))
}

pub(crate) fn preferred_attack_attribute(
    target: &DynamicMessage,
    skill: &TutorialSkill,
) -> Result<i32, StateError> {
    let mut attributes = skill.attack_attributes.iter().copied();
    let mut preferred = attributes
        .next()
        .ok_or_else(|| StateError::TutorialRules(format!("skill {} has no attribute", skill.id)))?;
    let mut lowest_resistance = target_resistance(target, preferred)?;
    for attribute in attributes {
        let resistance = target_resistance(target, attribute)?;
        if resistance < lowest_resistance {
            preferred = attribute;
            lowest_resistance = resistance;
        }
    }
    Ok(preferred)
}

pub(crate) fn deterministic_roll(
    secret: &[u8],
    start_txid: &str,
    action_number: i32,
    roll_kind: &[u8],
    target_id: i32,
    draw_index: u32,
) -> u32 {
    let mut hasher = Sha256::new();
    hasher.update(secret);
    hasher.update(start_txid.as_bytes());
    hasher.update(action_number.to_le_bytes());
    hasher.update(roll_kind);
    hasher.update(target_id.to_le_bytes());
    hasher.update(draw_index.to_le_bytes());
    let digest = hasher.finalize();
    u32::from_le_bytes([digest[0], digest[1], digest[2], digest[3]])
}

pub(crate) fn member_level(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    member: &DynamicMessage,
    resources: Option<&DynamicMessage>,
) -> Result<i32, StateError> {
    let initial = member_status(member, "initial_status")?;
    let matches = |stats: BattleStats| {
        i32_field(&initial, "speed") == Some(stats.speed)
            && i32_field(member, "max_hp") == Some(stats.hp)
    };
    if member_type(member)? == 0 {
        let character_id = member_status(member, "ally")
            .ok()
            .and_then(|ally| i32_field(&ally, "current_character_id"))
            .ok_or(StateError::InvalidRequest)?;
        let enhanced_levels: std::collections::BTreeSet<_> = rules
            .fixed_parties
            .iter()
            .flat_map(|p| &p.members)
            .filter(|m| m.character_id == character_id && m.is_max_enhance)
            .map(|m| m.level)
            .collect();
        for declared_level in enhanced_levels {
            let character = character::fixed_max_character(
                proto,
                load_character_rules()?,
                character_id,
                declared_level,
            )?;
            let level = level_for_exp(rules, i32_field(&character, "exp").unwrap_or(0))?;
            let rarity = i32_field(&character, "rarity").ok_or(StateError::InvalidRequest)?;
            let stats = atelier::combat_stats(
                &atelier::load_rules()?,
                &empty_message(proto, "blend.model.Resources")?,
                &character,
                &empty_message(proto, "blend.model.PartyMember")?,
                0,
                character_stats(rules, character_id, level, rarity)?,
            )?
            .0;
            if matches(stats) {
                return Ok(level);
            }
        }
        for fixed in rules
            .fixed_parties
            .iter()
            .flat_map(|p| p.members.iter())
            .filter(|m| m.character_id == character_id)
        {
            if matches(tutorial_ally_stats(
                rules,
                character_id,
                fixed.level,
                fixed.rarity,
                None,
            )?) {
                return Ok(fixed.level);
            }
        }
        if let Some(character) = resources.and_then(|resources| {
            message_list(resources, "characters")
                .into_iter()
                .find(|character| i32_field(character, "character_id") == Some(character_id))
        }) {
            return level_for_exp(rules, i32_field(&character, "exp").unwrap_or(0));
        }
        for rarity in rules.character_rarities.iter().map(|row| row.id) {
            for level in rules.character_levels.iter().map(|r| r.level) {
                for memoria_id in std::iter::once(None)
                    .chain(rules.reward_memorias.iter().map(|memoria| Some(memoria.id)))
                {
                    if matches(tutorial_ally_stats(
                        rules,
                        character_id,
                        level,
                        rarity,
                        memoria_id,
                    )?) {
                        return Ok(level);
                    }
                }
            }
        }
    } else {
        let enemy_id = enemy_member_status_enemy_id(member)?;
        return rules
            .waves
            .iter()
            .flat_map(|wave| wave.enemies.iter().map(move |e| (wave.id, e)))
            .find(|(wave_id, enemy)| {
                enemy.id == enemy_id
                    && rule_enemy(rules, enemy_id)
                        .ok()
                        .and_then(|r| tutorial_enemy_stats(r, enemy, *wave_id).ok())
                        .is_some_and(|(s, _, _)| matches(s))
            })
            .map(|(_, enemy)| enemy.level)
            .ok_or(StateError::InvalidRequest);
    }
    Err(StateError::TutorialRules(
        "member level is not rule-resolvable".into(),
    ))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn policy_damage_from_stats(
    skill: &TutorialSkill,
    offense: i32,
    level: i32,
    defense: i32,
    resistance: i32,
    panel: (i128, i128),
    skill_damage_bonus: i32,
    broken: bool,
    critical: bool,
    variance: u32,
) -> i64 {
    let resistance_multiplier = (100 - resistance + if broken { 50 } else { 0 }).max(0) as i128;
    let critical_multiplier = if critical { 150 } else { 100 } as i128;
    let broken_multiplier = if broken { 2 } else { 1 } as i128;
    let skill_damage_multiplier = panel.0 * 10_000 + panel.1 * i128::from(skill_damage_bonus);
    let numerator = 112i128
        * i128::from(offense)
        * i128::from(level + 9)
        * i128::from(skill.power.max(0))
        * skill_damage_multiplier
        * resistance_multiplier
        * critical_multiplier
        * broken_multiplier
        * i128::from(variance);
    let denominator = 9i128 * 100 * i128::from(defense) * panel.1 * 10_000 * 100 * 100 * 10000;
    (numerator / denominator).clamp(0, 9_999_999_999) as i64
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn policy_damage(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    attacker: &DynamicMessage,
    target: &DynamicMessage,
    skill: &TutorialSkill,
    resources: Option<&DynamicMessage>,
    runtime: Option<&effects::Runtime>,
    opponent_count: i32,
    panel: (i128, i128),
    broken: bool,
    critical: bool,
    variance: u32,
) -> Result<i64, StateError> {
    let (offense, defense, attribute) =
        member_offense_and_defense(proto, rules, attacker, target, runtime, skill)?;
    let skill_damage_bonus = checked_i32(
        i64::from(state_change_summary_value(attacker, 1))
            + runtime.map_or(0, |runtime| {
                runtime.contextual_summary_against(attacker, Some(target), skill, critical, 1)
            })
            + effects::instant_summary_for_source(Some(attacker), skill, target, critical, 1)?
            + effects::scaled_skill_damage(skill, attacker, opponent_count)?
            + policy_hp_damage_bonus(attacker, skill)?
            - i64::from(state_change_summary_value(attacker, 2))
            - runtime.map_or(0, |runtime| {
                runtime.contextual_summary_against(attacker, Some(target), skill, critical, 2)
            }),
    )?;
    let base = policy_damage_from_stats(
        skill,
        offense,
        member_level(proto, rules, attacker, resources)?,
        defense,
        target_resistance(target, attribute)?,
        panel,
        skill_damage_bonus,
        broken,
        critical,
        variance,
    );
    effects::secondary_damage(
        base,
        attacker,
        target,
        skill,
        runtime,
        opponent_count,
        critical,
    )
}

pub(crate) fn policy_hp_damage_bonus(
    attacker: &DynamicMessage,
    skill: &TutorialSkill,
) -> Result<i64, StateError> {
    let Some(rule) = &skill.hp_damage_bonus else {
        return Ok(0);
    };
    if rule.minimum < 0
        || rule.maximum < rule.minimum
        || rule.hp_minimum < 0
        || rule.hp_maximum <= rule.hp_minimum
    {
        return Err(StateError::TutorialRules(format!(
            "invalid HP damage scaling for skill {}",
            skill.id
        )));
    }
    let hp = i64::from(i32_field(attacker, "hp").unwrap_or(0).max(0));
    let maximum_hp = i64::from(i32_field(attacker, "max_hp").unwrap_or(1).max(1));
    let lower = maximum_hp * i64::from(rule.hp_minimum);
    let upper = maximum_hp * i64::from(rule.hp_maximum);
    let progress = (hp * 100).clamp(lower, upper) - lower;
    let span = upper - lower;
    let range = i64::from(rule.maximum - rule.minimum);
    let increase = range * progress / span;
    Ok(if rule.increases_with_hp {
        i64::from(rule.minimum) + increase
    } else {
        i64::from(rule.maximum) - increase
    })
}

pub(crate) fn policy_break_damage(
    attacker: &DynamicMessage,
    target: &DynamicMessage,
    skill: &TutorialSkill,
    runtime: Option<&effects::Runtime>,
    panel_multiplier: i128,
    critical: bool,
    variance: u32,
) -> Result<i32, StateError> {
    let status = member_status(attacker, "current_status")?;
    let attribute = preferred_attack_attribute(target, skill)?;
    let offense_name = if (1..=3).contains(&attribute) {
        "attack"
    } else {
        "magic"
    };
    let offense = i32_field(&status, offense_name)
        .ok_or(StateError::InvalidRequest)?
        .max(1);
    let resistance = (100 - target_resistance(target, attribute)?).max(0);
    let numerator = 125i128
        * i128::from(offense)
        * i128::from(skill.break_power.max(0))
        * i128::from(resistance)
        * i128::from(variance);
    let denominator = i128::from(offense + 100) * 100 * 100 * 10000;
    let rate = (10_000i64
        + i64::from(state_change_summary_value(attacker, 3))
        + runtime.map_or(0, |runtime| {
            runtime.contextual_summary_against(attacker, Some(target), skill, critical, 3)
        })
        + effects::instant_summary_for_source(Some(attacker), skill, target, critical, 3)?
        + effects::scaled_break_damage(skill, attacker)?)
    .clamp(0, 1_000_000);
    let incoming = (10_000i64
        + i64::from(state_change_summary_value(target, 13))
        + runtime.map_or(0, |runtime| {
            runtime.contextual_summary(target, skill, false, 13)
        }))
    .clamp(0, 1_000_000);
    checked_i32(
        (numerator * i128::from(rate) * i128::from(incoming) * panel_multiplier
            / (denominator * 10_000 * 10_000 * 100))
            .max(0) as i64,
    )
}

pub(crate) fn sort_timeline_units(units: &mut [DynamicMessage], members: &[DynamicMessage]) {
    units.sort_by_key(|unit| {
        let id = i32_field(unit, "member_id").unwrap_or(i32::MAX);
        let speed = members
            .iter()
            .find(|member| i32_field(member, "member_id") == Some(id))
            .and_then(|member| member_status(member, "current_status").ok())
            .and_then(|status| i32_field(&status, "speed"))
            .unwrap_or(0);
        (
            i32_field(unit, "wait").unwrap_or(i32::MAX),
            speed.saturating_neg(),
            i32_field(unit, "number").unwrap_or(i32::MAX),
            id,
        )
    });
}

pub(crate) fn timeline_move(
    proto: &ProtoRegistry,
    id: i32,
    number: i32,
    wait: Option<i32>,
    reason: i32,
    from_index: usize,
    to_index: Option<usize>,
) -> Result<DynamicMessage, StateError> {
    let mut movement = empty_message(proto, "blend.model.BattleTimelineMove")?;
    movement.set_field_by_name("member_id", Value::I32(id));
    movement.set_field_by_name("number", Value::I32(number));
    if let Some(wait) = wait {
        movement.set_field_by_name("wait", Value::Message(wrapper_i32(proto, wait.max(0))?));
    }
    movement.set_field_by_name("reason", Value::EnumNumber(reason));
    movement.set_field_by_name(
        "from_index",
        Value::Message(wrapper_i32(
            proto,
            i32::try_from(from_index).map_err(|_| StateError::InvalidRequest)?,
        )?),
    );
    if let Some(to_index) = to_index {
        movement.set_field_by_name(
            "to_index",
            Value::Message(wrapper_i32(
                proto,
                i32::try_from(to_index).map_err(|_| StateError::InvalidRequest)?,
            )?),
        );
    }
    Ok(movement)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn consume_timeline_turn(
    proto: &ProtoRegistry,
    units: &mut Vec<DynamicMessage>,
    members: &[DynamicMessage],
    id: i32,
    selected_wait: i32,
    default_wait: i32,
    preserve_future_turns: bool,
) -> Result<DynamicMessage, StateError> {
    sort_timeline_units(units, members);
    if let Some(movement) = consume_extra_timeline_turn(proto, units, members, id)? {
        return Ok(movement);
    }
    let from_index = units
        .iter()
        .position(|unit| i32_field(unit, "member_id") == Some(id))
        .ok_or(StateError::InvalidRequest)?;
    if from_index != 0 {
        return Err(StateError::InvalidRequest);
    }
    let current_wait = i32_field(&units[from_index], "wait")
        .ok_or(StateError::InvalidRequest)?
        .max(0);
    let old_number = i32_field(&units[from_index], "number").ok_or(StateError::InvalidRequest)?;
    for unit in units.iter_mut() {
        let wait = i32_field(unit, "wait").ok_or(StateError::InvalidRequest)?;
        unit.set_field_by_name("wait", Value::I32(wait.saturating_sub(current_wait)));
    }
    let actor_indexes: Vec<usize> = units
        .iter()
        .enumerate()
        .filter_map(|(index, unit)| (i32_field(unit, "member_id") == Some(id)).then_some(index))
        .collect();
    if !(2..=3).contains(&actor_indexes.len()) {
        return Err(StateError::InvalidRequest);
    }
    let replacement_index = actor_indexes.last().copied();
    if !preserve_future_turns {
        units[replacement_index.ok_or(StateError::InvalidRequest)?]
            .set_field_by_name("wait", Value::I32(selected_wait.max(1)));
    }
    units.remove(from_index);
    let future_waits: Vec<i32> = units
        .iter()
        .filter(|unit| i32_field(unit, "member_id") == Some(id))
        .filter_map(|unit| i32_field(unit, "wait"))
        .collect();
    let next_wait = if preserve_future_turns {
        future_waits
            .iter()
            .copied()
            .max()
            .ok_or(StateError::InvalidRequest)?
            .saturating_add(selected_wait.max(1))
    } else {
        future_waits
            .iter()
            .copied()
            .min()
            .ok_or(StateError::InvalidRequest)?
            .saturating_add(default_wait.max(1))
    };
    units.push(build_timeline_unit(proto, id, old_number, next_wait)?);
    sort_timeline_units(units, members);
    let next_current_wait = units
        .first()
        .and_then(|unit| i32_field(unit, "wait"))
        .ok_or(StateError::InvalidRequest)?;
    for unit in units.iter_mut() {
        let wait = i32_field(unit, "wait").ok_or(StateError::InvalidRequest)?;
        unit.set_field_by_name("wait", Value::I32(wait.saturating_sub(next_current_wait)));
    }
    let to_index = units
        .iter()
        .position(|unit| {
            i32_field(unit, "member_id") == Some(id)
                && i32_field(unit, "number") == Some(old_number)
        })
        .ok_or(StateError::InvalidRequest)?;
    timeline_move(
        proto,
        id,
        old_number,
        Some(
            i32_field(&units[to_index], "wait")
                .ok_or(StateError::InvalidRequest)?
                .max(0),
        ),
        0,
        from_index,
        Some(to_index),
    )
}

pub(crate) fn delay_timeline_member(
    proto: &ProtoRegistry,
    units: &mut [DynamicMessage],
    members: &[DynamicMessage],
    id: i32,
    wait: i32,
) -> Result<Vec<DynamicMessage>, StateError> {
    sort_timeline_units(units, members);
    let moved: Vec<(i32, usize)> = units
        .iter()
        .enumerate()
        .filter_map(|(index, unit)| {
            (i32_field(unit, "member_id") == Some(id))
                .then(|| i32_field(unit, "number").map(|number| (number, index)))?
        })
        .collect();
    if !(2..=3).contains(&moved.len()) {
        return Err(StateError::InvalidRequest);
    }
    for unit in units
        .iter_mut()
        .filter(|unit| i32_field(unit, "member_id") == Some(id))
    {
        let old_wait = i32_field(unit, "wait").ok_or(StateError::InvalidRequest)?;
        unit.set_field_by_name("wait", Value::I32(old_wait.saturating_add(wait.max(0))));
    }
    sort_timeline_units(units, members);
    moved
        .into_iter()
        .map(|(number, from_index)| {
            let to_index = units
                .iter()
                .position(|unit| {
                    i32_field(unit, "member_id") == Some(id)
                        && i32_field(unit, "number") == Some(number)
                })
                .ok_or(StateError::InvalidRequest)?;
            let updated_wait = i32_field(&units[to_index], "wait")
                .ok_or(StateError::InvalidRequest)?
                .max(0);
            timeline_move(
                proto,
                id,
                number,
                Some(updated_wait),
                2,
                from_index,
                Some(to_index),
            )
        })
        .collect()
}

pub(crate) fn delay_timeline_member_by_slots(
    proto: &ProtoRegistry,
    units: &mut [DynamicMessage],
    members: &[DynamicMessage],
    id: i32,
    slots: usize,
) -> Result<Vec<DynamicMessage>, StateError> {
    shift_timeline_member_by_slots(proto, units, members, id, slots, false)
}

pub(crate) fn advance_timeline_member_by_slots(
    proto: &ProtoRegistry,
    units: &mut [DynamicMessage],
    members: &[DynamicMessage],
    id: i32,
    slots: usize,
) -> Result<Vec<DynamicMessage>, StateError> {
    shift_timeline_member_by_slots(proto, units, members, id, slots, true)
}

fn shift_timeline_member_by_slots(
    proto: &ProtoRegistry,
    units: &mut [DynamicMessage],
    members: &[DynamicMessage],
    id: i32,
    slots: usize,
    advance: bool,
) -> Result<Vec<DynamicMessage>, StateError> {
    if slots == 0 {
        return Err(StateError::InvalidRequest);
    }
    sort_timeline_units(units, members);
    let moved = units
        .iter()
        .enumerate()
        .filter_map(|(index, unit)| {
            (i32_field(unit, "member_id") == Some(id)).then(|| {
                Ok((
                    i32_field(unit, "number").ok_or(StateError::InvalidRequest)?,
                    index,
                ))
            })
        })
        .collect::<Result<Vec<_>, StateError>>()?;
    if !(2..=3).contains(&moved.len()) {
        return Err(StateError::InvalidRequest);
    }
    let first_index = moved[0].1;
    let first_wait = i32_field(&units[first_index], "wait").ok_or(StateError::InvalidRequest)?;
    let other_waits = units
        .iter()
        .filter(|unit| i32_field(unit, "member_id") != Some(id))
        .map(|unit| i32_field(unit, "wait").ok_or(StateError::InvalidRequest))
        .collect::<Result<Vec<_>, _>>()?;
    let insertion_index = if advance {
        first_index.saturating_sub(slots).max(1).min(other_waits.len())
    } else {
        first_index.saturating_add(slots).min(other_waits.len())
    };
    let lower = insertion_index
        .checked_sub(1)
        .and_then(|index| other_waits.get(index))
        .copied()
        .ok_or(StateError::InvalidRequest)?;
    let shifted_wait = other_waits.get(insertion_index).map_or_else(
        || lower.saturating_add(1),
        |upper| lower.saturating_add(upper.saturating_sub(lower) / 2),
    );
    let delta = shifted_wait.saturating_sub(first_wait);
    if (advance && delta >= 0) || (!advance && delta <= 0) {
        return Ok(Vec::new());
    }
    for unit in units
        .iter_mut()
        .filter(|unit| i32_field(unit, "member_id") == Some(id))
    {
        let old_wait = i32_field(unit, "wait").ok_or(StateError::InvalidRequest)?;
        unit.set_field_by_name("wait", Value::I32(old_wait.saturating_add(delta).max(0)));
    }
    sort_timeline_units(units, members);
    moved
        .into_iter()
        .map(|(number, from_index)| {
            let to_index = units
                .iter()
                .position(|unit| {
                    i32_field(unit, "member_id") == Some(id)
                        && i32_field(unit, "number") == Some(number)
                })
                .ok_or(StateError::InvalidRequest)?;
            let wait = i32_field(&units[to_index], "wait").ok_or(StateError::InvalidRequest)?;
            timeline_move(proto, id, number, Some(wait), 3, from_index, Some(to_index))
        })
        .collect()
}

pub(crate) fn remove_timeline_members(
    proto: &ProtoRegistry,
    units: &mut Vec<DynamicMessage>,
    members: &[DynamicMessage],
    ids: &[i32],
) -> Result<Vec<DynamicMessage>, StateError> {
    sort_timeline_units(units, members);
    let mut moves = Vec::new();
    while let Some(index) = units
        .iter()
        .position(|unit| i32_field(unit, "member_id").is_some_and(|id| ids.contains(&id)))
    {
        let unit = units.remove(index);
        let id = i32_field(&unit, "member_id").ok_or(StateError::InvalidRequest)?;
        let number = i32_field(&unit, "number").ok_or(StateError::InvalidRequest)?;
        moves.push(timeline_move(proto, id, number, None, 1, index, None)?);
    }
    if moves.is_empty() {
        return Err(StateError::InvalidRequest);
    }
    Ok(moves)
}

pub(crate) fn earliest_living_member(
    state: &DynamicMessage,
    expected_type: Option<i32>,
) -> Result<DynamicMessage, StateError> {
    let members = message_list(state, "members");
    let units = message_list(state, "timeline_units");
    members
        .into_iter()
        .filter(|member| {
            bool_field(member, "is_alive")
                && expected_type.is_none_or(|kind| member_type(member).ok() == Some(kind))
        })
        .filter_map(|member| {
            let id = i32_field(&member, "member_id")?;
            let number = units
                .iter()
                .filter(|unit| i32_field(unit, "member_id") == Some(id))
                .min_by_key(|unit| {
                    (
                        i32_field(unit, "wait").unwrap_or(i32::MAX),
                        i32_field(unit, "number").unwrap_or(i32::MAX),
                    )
                })
                .and_then(|unit| i32_field(unit, "number"))
                .unwrap_or(i32::MAX);
            let wait = units
                .iter()
                .filter(|unit| i32_field(unit, "member_id") == Some(id))
                .filter_map(|unit| i32_field(unit, "wait"))
                .min()
                .unwrap_or(i32::MAX);
            let speed = member_status(&member, "current_status")
                .ok()
                .and_then(|status| i32_field(&status, "speed"))
                .unwrap_or(0);
            Some((wait, speed.saturating_neg(), number, id, member))
        })
        .min_by_key(|(wait, speed, number, id, _)| (*wait, *speed, *number, *id))
        .map(|(_, _, _, _, member)| member)
        .ok_or(StateError::InvalidRequest)
}

pub(crate) fn current_actor(state: &DynamicMessage) -> Result<DynamicMessage, StateError> {
    earliest_living_member(state, None)
}

pub(crate) fn select_target_ids(
    state: &DynamicMessage,
    actor_id: i32,
    actor_type: i32,
    target_id: i32,
    skill: &TutorialSkill,
) -> Result<Vec<i32>, StateError> {
    let opposing_type = if actor_type == 0 { 1 } else { 0 };
    let target_type = skill.skill_target_type.ok_or(StateError::InvalidRequest)?;
    let selected_type = if matches!(target_type, 1 | 2 | 4) {
        actor_type
    } else {
        opposing_type
    };
    let living: Vec<i32> = message_list(state, "members")
        .into_iter()
        .filter(|member| {
            (target_type == 6 || member_type(member).ok() == Some(selected_type))
                && bool_field(member, "is_alive")
        })
        .filter_map(|member| i32_field(&member, "member_id"))
        .collect();
    if matches!(target_type, 4 | 5) {
        if living.is_empty() {
            return Err(StateError::InvalidRequest);
        }
        Ok(living)
    } else if matches!(target_type, 1 | 2 | 3 | 6) {
        if target_type == 1 && actor_id != target_id {
            return Err(StateError::InvalidRequest);
        }
        if !living.contains(&target_id) {
            return Err(StateError::InvalidRequest);
        }
        Ok(vec![target_id])
    } else {
        Err(StateError::TutorialRules(format!(
            "unsupported skill target type for {}",
            skill.id
        )))
    }
}

pub(crate) fn most_injured_living_member(
    state: &DynamicMessage,
    selected_type: i32,
) -> Result<DynamicMessage, StateError> {
    message_list(state, "members")
        .into_iter()
        .filter(|member| {
            member_type(member).ok() == Some(selected_type) && bool_field(member, "is_alive")
        })
        .min_by_key(|member| {
            (
                i32_field(member, "hp").unwrap_or_default()
                    - i32_field(member, "max_hp").unwrap_or_default(),
                i32_field(member, "member_id").unwrap_or(i32::MAX),
            )
        })
        .ok_or(StateError::InvalidRequest)
}

pub(crate) fn validate_formation(
    request: &DynamicMessage,
    state: &DynamicMessage,
) -> Result<(), StateError> {
    if !request.has_field_by_name("formation") {
        return Ok(());
    }
    let Some(formation) = request
        .get_field_by_name("formation")
        .and_then(|value| value.as_message().cloned())
    else {
        return Ok(());
    };
    let formation_members = formation
        .get_field_by_name("members")
        .and_then(|value| value.as_list().map(|values| values.to_vec()))
        .ok_or(StateError::InvalidRequest)?;
    let ids: Vec<i32> = formation_members
        .iter()
        .map(|value| {
            value
                .as_message()
                .and_then(|member| i32_field(member, "member_id"))
                .ok_or(StateError::InvalidRequest)
        })
        .collect::<Result<_, _>>()?;
    let known: Vec<i32> = message_list(state, "members")
        .into_iter()
        .filter_map(|member| {
            member
                .get_field_by_name("member_id")
                .and_then(|value| value.as_i32())
        })
        .collect();
    let has_duplicate = ids
        .iter()
        .enumerate()
        .any(|(index, id)| ids[index + 1..].contains(id));
    // The client sends the living subset after a death; membership is not a
    // full snapshot.  Validate only IDs it actually supplied.
    if has_duplicate || ids.iter().any(|id| !known.contains(id)) {
        return Err(StateError::InvalidRequest);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn multi_attribute_uses_lowest_resistance() {
        let proto = ProtoRegistry::from_file(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../schemas/atelier-resleriana-2.16.0.protoset"
        )))
        .unwrap();
        let mut resistance = empty_message(&proto, "blend.model.BattleResistance").unwrap();
        for field in [
            "slashing",
            "impact",
            "piercing",
            "fire",
            "ice",
            "lightning",
            "wind",
        ] {
            resistance.set_field_by_name(field, Value::I32(0));
        }
        resistance.set_field_by_name("fire", Value::I32(25));
        resistance.set_field_by_name("ice", Value::I32(-50));
        let mut target = empty_message(&proto, "blend.model.BattleMember").unwrap();
        target.set_field_by_name("resistance", Value::Message(resistance));
        let skill = TutorialSkill {
            id: 1,
            skill_type: 1,
            skill_effect_type: 1,
            skill_power_type: 1,
            wait: 100,
            power: 100,
            break_power: 100,
            break_power_type: 1,
            attack_attributes: vec![5, 6],
            skill_target_type: Some(3),
            effects: Vec::new(),
            limit_count: None,
            max_lamp: 0,
            require_command_value: false,
            skill_destination: None,
            state_change_application_rate: 10_000,
            hp_damage_bonus: None,
        };

        assert_eq!(preferred_attack_attribute(&target, &skill).unwrap(), 6);
    }

    #[test]
    fn hp_damage_bonus_scales_and_clamps() {
        let proto = ProtoRegistry::from_file(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../schemas/atelier-resleriana-2.16.0.protoset"
        )))
        .unwrap();
        let mut attacker = empty_message(&proto, "blend.model.BattleMember").unwrap();
        attacker.set_field_by_name("max_hp", Value::I32(100));
        let rules = load_gameplay_rules().unwrap();
        let skill = rule_skill(&rules, 12000111).unwrap();
        let bonus = skill.hp_damage_bonus.as_ref().unwrap();
        assert_eq!(
            (
                bonus.minimum,
                bonus.maximum,
                bonus.hp_minimum,
                bonus.hp_maximum,
                bonus.increases_with_hp,
            ),
            (500, 2_500, 20, 100, false)
        );
        for (hp, expected) in [(0, 2_500), (20, 2_500), (60, 1_500), (100, 500)] {
            attacker.set_field_by_name("hp", Value::I32(hp));
            assert_eq!(policy_hp_damage_bonus(&attacker, skill).unwrap(), expected);
        }
    }

    #[test]
    fn consumed_panel_continues_the_battle_sequence() {
        let proto = ProtoRegistry::from_file(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../schemas/atelier-resleriana-2.16.0.protoset"
        )))
        .unwrap();
        let rules = load_tutorial_rules().unwrap();
        let mut state = empty_message(&proto, "blend.model.BattleState").unwrap();
        state.set_field_by_name("battle_id", Value::I32(10000004));
        state.set_field_by_name("wave", Value::I32(1));
        set_timeline_panels(
            &proto,
            &mut state,
            &tutorial_timeline_panels(&rules, 10000004, 1).unwrap(),
            10,
            1,
        )
        .unwrap();

        consume_timeline_panel(&proto, &rules, &mut state).unwrap();
        consume_timeline_panel(&proto, &rules, &mut state).unwrap();

        let panels = message_list(&state, "timeline_panels");
        assert_eq!(i32_field(panels.last().unwrap(), "turn"), Some(12));
        assert_eq!(current_panel_id(&state), 11);
        assert_eq!(
            optional_i32_field(panels.last().unwrap(), "panel_id"),
            Some(12)
        );
    }

    #[test]
    fn both_client_burst_panel_rows_unlock_burst() {
        assert!(is_burst_panel_id(14));
        assert!(is_burst_panel_id(17));
        assert!(!is_burst_panel_id(11));
    }
}
