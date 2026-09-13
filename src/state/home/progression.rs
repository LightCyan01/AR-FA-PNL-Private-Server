use super::prelude::*;

/// Called inside the winning battle transaction, using its persisted participants,
/// never the editable Home party. Attempts, losses, retire and replay cannot tick it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn battle_clear_progress(
    proto: &ProtoRegistry,
    rules: &HomeRules,
    quest: &TutorialRules,
    state: &DynamicMessage,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    quest_id: i32,
    quest_cleared: bool,
    now: i64,
) -> Result<(), StateError> {
    let participants: BTreeSet<_> = message_list(state, "members")
        .iter()
        .filter_map(|m| message_i32_field(m, "ally", "character_id"))
        .collect();
    completed_battle_progress(
        proto,
        rules,
        quest,
        &participants,
        &i32_list(state, "wave_ids"),
        resources,
        changed,
        quest_id,
        quest_cleared,
        now,
    )
}

/// Shared clear predicates; an explicit skip supplies its resolved party and
/// master waves, not fabricated combat state, skill usage or damage results.
#[allow(clippy::too_many_arguments)]
pub(crate) fn completed_battle_progress(
    proto: &ProtoRegistry,
    rules: &HomeRules,
    quest: &TutorialRules,
    participants: &BTreeSet<i32>,
    wave_ids: &[i32],
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    quest_id: i32,
    quest_cleared: bool,
    now: i64,
) -> Result<(), StateError> {
    if quest_cleared {
        for id in participants {
            advance_missions(
                proto,
                rules,
                resources,
                changed,
                now,
                Some((&format!("quest_clear_character:{id}"), 1)),
            )?;
        }
        let matches = |rule: &BattleObjective| {
            rule.quest_ids.contains(&quest_id)
                && participants
                    .iter()
                    .filter(|id| rule.character_ids.contains(id))
                    .count()
                    >= rule.minimum
        };
        for task in &rules.total_tasks {
            if task.objective.battle.as_ref().is_some_and(&matches) {
                let count = task
                    .objective
                    .advance(total_task_count(resources, task.condition_id), 1);
                update_task(resources, changed, task.condition_id, count)?;
            }
        }
        for row in &rules.missions {
            if row.total_task_condition_id.is_none()
                && mission_active(rules, row, now)
                && row.objective.battle.as_ref().is_some_and(&matches)
            {
                let mut mission = message_list(resources, "missions")
                    .into_iter()
                    .find(|m| i32_field(m, "mission_id") == Some(row.id))
                    .unwrap_or(empty_message(proto, "blend.model.Mission")?);
                mission.set_field_by_name("mission_id", Value::I32(row.id));
                let count = row
                    .objective
                    .advance(i32_field(&mission, "count").unwrap_or(0), 1);
                mission.set_field_by_name("count", Value::I32(count));
                put(resources, "missions", "mission_id", mission.clone());
                put(changed, "missions", "mission_id", mission);
            }
        }
    }
    let mut species = BTreeMap::<i32, i32>::new();
    for wave_id in wave_ids {
        for enemy in &rule_wave(quest, *wave_id)?.enemies {
            let id = rule_enemy(quest, enemy.id)?.species_id;
            if id > 0 {
                *species.entry(id).or_default() += 1;
            }
        }
    }
    for (id, count) in species {
        advance_missions(
            proto,
            rules,
            resources,
            changed,
            now,
            Some((&format!("enemy_species:{id}"), count)),
        )?;
    }
    Ok(())
}

#[derive(Clone, Deserialize)]
pub(crate) struct TotalTaskRule {
    pub(crate) condition_id: i32,
    #[serde(flatten)]
    pub(crate) objective: MissionObjective,
}

#[derive(Clone, Deserialize)]
pub(crate) struct SynthesisObjective {
    pub(crate) per_craft: bool,
    pub(crate) resource_type: Option<i32>,
    pub(crate) recipe_id: Option<i32>,
    pub(crate) slot_type: Option<i32>,
    pub(crate) role: Option<i32>,
    pub(crate) rarity: Option<i32>,
    pub(crate) min_trait_level: Option<i32>,
    #[serde(default)]
    pub(crate) trait_ids: Vec<i32>,
    pub(crate) trait_rank: Option<i32>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct SynthesisTarget {
    pub(crate) recipe_id: i32,
    pub(crate) resource_type: i32,
    pub(crate) id: i32,
}

#[derive(Clone, Deserialize)]
pub(crate) struct MissionTool {
    pub(crate) id: i32,
    pub(crate) rarity: i32,
    pub(crate) slot_type: Option<i32>,
    #[serde(default)]
    pub(crate) roles: Vec<i32>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct MissionStep {
    pub(crate) count: i32,
    pub(crate) reward_set_id: i32,
    #[serde(default)]
    pub(crate) start_at: Option<i64>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct MissionBattleRewardRule {
    pub(crate) quest_id: i32,
    pub(crate) mission_ids: Vec<i32>,
    pub(crate) steps: Vec<MissionStep>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct CountRule {
    pub(crate) category: i32,
    pub(crate) reset_cycle: Option<i32>,
    pub(crate) start_at: Option<i64>,
    pub(crate) end_at: Option<i64>,
    pub(crate) steps: Vec<MissionStep>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct EventTab {
    pub(crate) id: i32,
    pub(crate) start_at: Option<i64>,
    pub(crate) end_at: Option<i64>,
    pub(crate) count_reward: Option<EventReward>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct EventReward {
    pub(crate) steps: Vec<MissionStep>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct GuideStep {
    pub(crate) id: i32,
    pub(crate) reward_set_id: i32,
}

#[derive(Clone, Deserialize)]
pub(crate) struct LoginRule {
    pub(crate) id: i32,
    #[serde(rename = "type")]
    pub(crate) kind: i32,
    pub(crate) start_at: Option<i64>,
    pub(crate) end_at: Option<i64>,
    pub(crate) elapsed_days: Option<i32>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct LoginDay {
    pub(crate) day: i32,
    pub(crate) login_bonus_id: i32,
    pub(crate) reward_set_id: i32,
}

#[derive(Clone, Deserialize)]
pub(crate) struct RegularDay {
    pub(crate) id: i32,
    pub(crate) login_bonus_id: i32,
    pub(crate) start_at: Option<i64>,
    pub(crate) reward_set_ids: Vec<i32>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct RankRule {
    pub(crate) id: i32,
    pub(crate) exp: i32,
    pub(crate) stamina: i32,
}

#[derive(Clone, Deserialize)]
pub(crate) struct RecipeRule {
    pub(crate) id: i32,
    pub(crate) recipe_plan_id: i32,
    pub(crate) learning_reward_set_id: i32,
    pub(crate) requirements: Vec<TaskCountRule>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct RecipePlanRule {
    pub(crate) id: i32,
    pub(crate) start_at: Option<i64>,
    pub(crate) end_at: Option<i64>,
    pub(crate) recipe_count_rewards: Vec<MissionStep>,
}

fn record_score_state(
    progress: &mut BattleProgress,
    state: &DynamicMessage,
) -> Result<(), StateError> {
    if progress.initial_ally_hp == 0 {
        progress.initial_ally_hp = message_list(state, "members")
            .iter()
            .filter(|member| member_type(member).ok() == Some(0))
            .map(|member| i64::from(i32_field(member, "max_hp").unwrap_or(0).max(0)))
            .sum();
    }
    let wave = i32_field(state, "wave").unwrap_or(1);
    if progress.score_waves.insert(wave) {
        progress.initial_enemy_hp = progress.initial_enemy_hp.saturating_add(
            message_list(state, "members")
                .iter()
                .filter(|member| member_type(member).ok() == Some(1))
                .map(|member| i64::from(i32_field(member, "max_hp").unwrap_or(0).max(0)))
                .sum(),
        );
    }
    Ok(())
}

fn battle_member_is_broken(member: &DynamicMessage) -> bool {
    member
        .get_field_by_name("enemy")
        .and_then(|value| value.as_message().cloned())
        .is_some_and(|enemy| bool_field(&enemy, "is_broken"))
        || bool_field(member, "is_stun")
}

pub(crate) fn record_battle_progress(
    saved: &mut HomeState,
    start_txid: &str,
    history: &DynamicMessage,
) -> Result<(), StateError> {
    if saved.battle_progress.start_txid != start_txid {
        saved.battle_progress = BattleProgress {
            start_txid: start_txid.into(),
            ..Default::default()
        };
    }
    let progress = &mut saved.battle_progress;
    let mut before = member_status(history, "previous_state")?;
    record_score_state(progress, &before)?;
    for wave_start in message_list(history, "wave_starts") {
        record_score_state(progress, &member_status(&wave_start, "state")?)?;
    }
    for action in message_list(history, "actions") {
        let after = member_status(&action, "state")?;
        let actor = i32_field(&action, "actor_id");
        let actor_member = message_list(&before, "members")
            .iter()
            .find(|m| i32_field(m, "member_id") == actor)
            .cloned();
        let actor_type = actor_member.as_ref().map(member_type).transpose()?;
        let dealt = action
            .get_field_by_name("total_dealt_hp_damage")
            .and_then(|value| value.as_i64())
            .unwrap_or(0)
            .max(0);
        if actor_type == Some(1) {
            progress.received_hp_damage = progress.received_hp_damage.saturating_add(dealt);
        }
        let character = actor_member
            .as_ref()
            .and_then(|m| message_i32_field(m, "ally", "character_id"));
        if let Some(id) = character.filter(|id| *id > 0) {
            if !bool_field(&action, "is_skipped")
                && optional_i32_field(&action, "skill_type").is_some()
            {
                if let Some(skill) = i32_field(&action, "skill_id").filter(|id| *id > 0) {
                    progress.skill_records.entry(id).or_default().insert(skill);
                }
            }
            progress.damage = progress.damage.saturating_add(dealt);
            progress.max_dealt_hp_damage = progress.max_dealt_hp_damage.max(dealt);
            if let Some(number) = optional_i32_field(&action, "battle_tool_number") {
                if let Some(tool) = message_list(&before, "battle_tools")
                    .iter()
                    .find(|t| i32_field(t, "number") == Some(number))
                    .and_then(|t| i32_field(t, "tool_id"))
                {
                    progress.events.insert(format!("battle_win_tool:{tool}"));
                    let uses = progress.tool_uses.entry(tool).or_default();
                    *uses = uses.saturating_add(1);
                }
            } else if let Some(kind) = optional_i32_field(&action, "skill_type") {
                let action = if kind == 3 {
                    "burst".into()
                } else {
                    format!("skill{kind}")
                };
                progress.events.insert(format!("battle_win_{action}:{id}"));
            }
            for result in message_list(&action, "skill_results") {
                let target = i32_field(&result, "target_id");
                let was_broken = message_list(&before, "members")
                    .iter()
                    .any(|m| i32_field(m, "member_id") == target && battle_member_is_broken(m));
                let now_broken = message_list(&after, "members")
                    .iter()
                    .any(|m| i32_field(m, "member_id") == target && battle_member_is_broken(m));
                for (event, achieved) in [
                    ("break", !was_broken && now_broken),
                    (
                        "broken_hit",
                        was_broken
                            && message_i64_field(&result, "hp_damage", "value").unwrap_or(0) > 0,
                    ),
                    ("critical", bool_field(&result, "is_critical")),
                ] {
                    if achieved {
                        progress.events.insert(format!("battle_win_{event}:{id}"));
                    }
                }
            }
        }
        before = after;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn battle_state(proto: &ProtoRegistry, broken: bool) -> Result<DynamicMessage, StateError> {
        let mut ally_status = empty_message(proto, "blend.model.BattleAlly")?;
        ally_status.set_field_by_name("character_id", Value::I32(123));
        let mut ally = empty_message(proto, "blend.model.BattleMember")?;
        ally.set_field_by_name("member_id", Value::I32(1));
        ally.set_field_by_name("type", Value::EnumNumber(0));
        ally.set_field_by_name("max_hp", Value::I32(100));
        ally.set_field_by_name("hp", Value::I32(100));
        ally.set_field_by_name("is_alive", Value::Bool(true));
        ally.set_field_by_name("ally", Value::Message(ally_status));

        let mut enemy_status = empty_message(proto, "blend.model.BattleEnemy")?;
        enemy_status.set_field_by_name("is_broken", Value::Bool(broken));
        let mut enemy = empty_message(proto, "blend.model.BattleMember")?;
        enemy.set_field_by_name("member_id", Value::I32(2));
        enemy.set_field_by_name("type", Value::EnumNumber(1));
        enemy.set_field_by_name("max_hp", Value::I32(100));
        enemy.set_field_by_name("hp", Value::I32(100));
        enemy.set_field_by_name("is_alive", Value::Bool(true));
        enemy.set_field_by_name("enemy", Value::Message(enemy_status));

        let mut state = empty_message(proto, "blend.model.BattleState")?;
        state.set_field_by_name(
            "members",
            Value::List(vec![Value::Message(ally), Value::Message(enemy)]),
        );
        state.set_field_by_name("wave", Value::I32(1));
        Ok(state)
    }

    fn action(
        proto: &ProtoRegistry,
        state: DynamicMessage,
        result: DynamicMessage,
    ) -> Result<DynamicMessage, StateError> {
        let mut action = empty_message(proto, "blend.model.BattleAction")?;
        action.set_field_by_name("actor_id", Value::I32(1));
        action.set_field_by_name("skill_id", Value::I32(900));
        action.set_field_by_name("skill_type", Value::Message(wrapper_i32(proto, 1)?));
        action.set_field_by_name("state", Value::Message(state));
        action.set_field_by_name("skill_results", Value::List(vec![Value::Message(result)]));
        action.set_field_by_name("total_dealt_hp_damage", Value::I64(10));
        Ok(action)
    }

    #[test]
    fn battle_progress_reads_nested_enemy_broken_state() {
        let proto = ProtoRegistry::from_file(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../schemas/atelier-resleriana-2.16.0.protoset"
        )))
        .unwrap();
        let before = battle_state(&proto, false).unwrap();
        let after = battle_state(&proto, true).unwrap();
        let result = build_skill_result(
            &proto, 2, 10, 10, 0, 10, true, false, false, false, true, false, false,
        )
        .unwrap();
        let mut history = empty_message(&proto, "blend.model.BattleHistory").unwrap();
        history.set_field_by_name("previous_state", Value::Message(before));
        history.set_field_by_name(
            "actions",
            Value::List(vec![Value::Message(
                action(&proto, after.clone(), result.clone()).unwrap(),
            )]),
        );
        let mut saved = HomeState::default();
        record_battle_progress(&mut saved, "tx", &history).unwrap();
        assert!(saved
            .battle_progress
            .events
            .contains("battle_win_break:123"));

        let mut second_history = empty_message(&proto, "blend.model.BattleHistory").unwrap();
        second_history.set_field_by_name("previous_state", Value::Message(after.clone()));
        second_history.set_field_by_name(
            "actions",
            Value::List(vec![Value::Message(action(&proto, after, result).unwrap())]),
        );
        record_battle_progress(&mut saved, "tx-2", &second_history).unwrap();
        assert!(saved
            .battle_progress
            .events
            .contains("battle_win_broken_hit:123"));
    }
}

pub(crate) fn finish_battle_progress(
    proto: &ProtoRegistry,
    rules: &HomeRules,
    saved: &mut HomeState,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    quest_id: Option<i32>,
    now: i64,
) -> Result<(), StateError> {
    let progress = std::mem::take(&mut saved.battle_progress);
    saved.combat_effects = effects::Runtime::default();
    for (character, skills) in progress.skill_records {
        let mut record = message_list(resources, "character_skill_records")
            .into_iter()
            .find(|r| i32_field(r, "character_id") == Some(character))
            .unwrap_or(empty_message(proto, "blend.model.CharacterSkillRecord")?);
        let mut known: BTreeSet<i32> = i32_list(&record, "skill_ids").into_iter().collect();
        let previous = known.len();
        known.extend(skills);
        if known.len() != previous {
            record.set_field_by_name("character_id", Value::I32(character));
            record.set_field_by_name(
                "skill_ids",
                Value::List(known.into_iter().map(Value::I32).collect()),
            );
            put(
                resources,
                "character_skill_records",
                "character_id",
                record.clone(),
            );
            put(changed, "character_skill_records", "character_id", record);
        }
    }
    let Some(quest_id) = quest_id else {
        return Ok(());
    };
    for (tool, count) in progress.tool_uses {
        advance_missions(
            proto,
            rules,
            resources,
            changed,
            now,
            Some((&format!("battle_tool_use:{tool}"), count)),
        )?;
    }
    for event in progress.events {
        advance_missions(proto, rules, resources, changed, now, Some((&event, 1)))?;
    }
    advance_missions(
        proto,
        rules,
        resources,
        changed,
        now,
        Some((
            &format!("quest_damage:{quest_id}"),
            progress.damage.min(i64::from(i32::MAX)) as i32,
        )),
    )
}

pub(crate) fn load_rules() -> Result<HomeRules, StateError> {
    let mut rules: HomeRules = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../data/home_rules.json"
    )))
    .map_err(|error| StateError::MasterData(error.to_string()))?;
    rules.mission_index = StateContext::from_entries(rules.context_entries());
    Ok(rules)
}

impl HomeRules {
    fn context_entries(&self) -> Vec<(i32, i32, String)> {
        let mut entries = Vec::new();
        for task in &self.total_tasks {
            let events = task
                .objective
                .counter
                .iter()
                .chain(&task.objective.counters)
                .cloned()
                .collect::<Vec<_>>();
            if events.is_empty() {
                entries.push((task.condition_id, 0, String::new()));
            } else {
                entries.extend(
                    events
                        .into_iter()
                        .map(|event| (task.condition_id, 0, event)),
                );
            }
        }
        for mission in &self.missions {
            let condition_id = mission.total_task_condition_id.unwrap_or(0);
            let events = mission
                .objective
                .counter
                .iter()
                .chain(&mission.objective.counters)
                .cloned()
                .collect::<Vec<_>>();
            if events.is_empty() {
                if condition_id > 0 {
                    entries.push((condition_id, mission.id, String::new()));
                }
            } else {
                entries.extend(
                    events
                        .into_iter()
                        .map(|event| (condition_id, mission.id, event)),
                );
            }
        }
        entries
    }
}

pub(crate) fn in_period(start: Option<i64>, end: Option<i64>, now: i64) -> bool {
    start.is_none_or(|value| value <= now) && end.is_none_or(|value| now < value)
}

pub(crate) fn day(now: i64) -> i64 {
    (now - 3 * 3600).div_euclid(86400)
}

pub(crate) fn reset_at(cycle: Option<i32>, now: i64) -> Option<i64> {
    match cycle {
        Some(1) => Some((day(now) + 1) * 86400 + 3 * 3600),
        Some(2) => Some((day(now) + 7 - (day(now) + 3).rem_euclid(7)) * 86400 + 3 * 3600),
        _ => None,
    }
}

pub(crate) fn mission_active(rules: &HomeRules, row: &MissionRule, now: i64) -> bool {
    in_period(row.start_at, row.end_at, now)
        && row.event_tab_id.is_none_or(|id| {
            rules
                .mission_event_tabs
                .iter()
                .any(|tab| tab.id == id && in_period(tab.start_at, tab.end_at, now))
        })
}

pub(crate) fn put(resources: &mut DynamicMessage, field: &str, key: &str, value: DynamicMessage) {
    let id = i32_field(&value, key);
    let mut rows = message_list(resources, field);
    if let Some(old) = rows.iter_mut().find(|row| i32_field(row, key) == id) {
        *old = value;
    } else {
        rows.push(value);
    }
    resources.set_field_by_name(
        field,
        Value::List(rows.into_iter().map(Value::Message).collect()),
    );
}

pub(crate) fn update_task(
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    id: i32,
    count: i32,
) -> Result<(), StateError> {
    let value = set_total_task_count(resources, id, count)?;
    put(changed, "total_task_counts", "condition_id", value);
    Ok(())
}

pub(crate) fn advance_missions(
    proto: &ProtoRegistry,
    rules: &HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    now: i64,
    event: Option<(&str, i32)>,
) -> Result<(), StateError> {
    let item_totals: BTreeMap<_, _> = message_list(resources, "items")
        .iter()
        .filter_map(|r| {
            Some((
                i32_field(r, "item_id")?,
                i32_field(r, "total_quantity").unwrap_or(0),
            ))
        })
        .collect();
    let quest_totals: BTreeMap<_, _> = message_list(resources, "quest_states")
        .iter()
        .filter_map(|r| {
            Some((
                i32_field(r, "quest_id")?,
                i32_field(r, "clear_count").unwrap_or(0),
            ))
        })
        .collect();
    if let Some((event, delta)) = event {
        if delta < 0 {
            return Err(StateError::InvalidRequest);
        }
        let indexed_conditions = rules.mission_index.conditions_for_event(event);
        if !indexed_conditions.is_empty()
            && !event.starts_with("item_received:")
            && !event.starts_with("quest_clear:")
        {
            // The index supplies the candidate conditions; aggregation keeps one
            // deterministic update per condition when multiple rules reference
            // the same event.
            let deltas = progression::aggregate_deltas(
                indexed_conditions
                    .iter()
                    .copied()
                    .map(|condition_id| (condition_id, delta)),
            );
            for change in deltas {
                if let Some(task) = rules
                    .total_tasks
                    .iter()
                    .find(|task| task.condition_id == change.condition_id)
                {
                    if task.condition_id != 137 {
                        let next = task.objective.advance(
                            total_task_count(resources, task.condition_id),
                            change.amount,
                        );
                        update_task(resources, changed, task.condition_id, next)?;
                    }
                }
            }
        } else {
            for task in &rules.total_tasks {
                if task.condition_id != 137 && task.objective.matches(event) {
                    let next = task
                        .objective
                        .advance(total_task_count(resources, task.condition_id), delta);
                    update_task(resources, changed, task.condition_id, next)?;
                }
            }
        }
    }
    for task in &rules.total_tasks {
        if let Some(state) = &task.objective.state {
            let count = objective_count(rules, resources, state);
            if count > total_task_count(resources, task.condition_id) {
                update_task(resources, changed, task.condition_id, count)?;
            }
        }
        let current = task
            .objective
            .counter
            .iter()
            .chain(&task.objective.counters)
            .filter_map(|counter| {
                if let Some(id) = counter
                    .strip_prefix("item_received:")
                    .and_then(|id| id.parse::<i32>().ok())
                {
                    item_totals.get(&id).copied()
                } else {
                    counter
                        .strip_prefix("quest_clear:")
                        .and_then(|id| id.parse::<i32>().ok())
                        .and_then(|id| quest_totals.get(&id).copied())
                }
            })
            .reduce(i32::saturating_add);
        if let Some(count) =
            current.filter(|count| *count > total_task_count(resources, task.condition_id))
        {
            update_task(resources, changed, task.condition_id, count)?;
        }
    }
    let mut missions: BTreeMap<i32, DynamicMessage> = message_list(resources, "missions")
        .into_iter()
        .filter_map(|row| Some((i32_field(&row, "mission_id")?, row)))
        .collect();
    for row in &rules.missions {
        if !mission_active(rules, row, now) {
            continue;
        }
        let old = missions.get(&row.id).cloned();
        let mut state = old
            .clone()
            .unwrap_or(empty_message(proto, "blend.model.Mission")?);
        state.set_field_by_name("mission_id", Value::I32(row.id));
        if let Some(reset) = reset_at(row.reset_cycle, now) {
            if message_i64_field(&state, "reset_at", "seconds") != Some(reset) {
                state.set_field_by_name("count", Value::I32(0));
                state.set_field_by_name("received_step_count", Value::I32(0));
                state.set_field_by_name("reset_at", Value::Message(timestamp(proto, reset)?));
            }
        }
        if let Some((event, delta)) = event {
            let indexed_missions = rules.mission_index.missions_for_event(event);
            let matches_event = if indexed_missions.is_empty() {
                row.objective.matches(event)
            } else {
                indexed_missions.contains(&row.id)
            };
            if row.total_task_condition_id.is_none() && matches_event {
                let cap = row.steps.last().map(|step| step.count).unwrap_or(0);
                let count = row
                    .objective
                    .advance(i32_field(&state, "count").unwrap_or(0), delta)
                    .min(cap);
                state.set_field_by_name("count", Value::I32(count));
            }
        }
        if let Some(objective) = row
            .objective
            .state
            .as_ref()
            .filter(|_| row.total_task_condition_id.is_none())
        {
            let count = objective_count(rules, resources, objective)
                .max(i32_field(&state, "count").unwrap_or(0));
            state.set_field_by_name("count", Value::I32(count));
        }
        if old.as_ref() != Some(&state) {
            put(changed, "missions", "mission_id", state.clone());
        }
        missions.insert(row.id, state);
    }
    resources.set_field_by_name(
        "missions",
        Value::List(missions.into_values().map(Value::Message).collect()),
    );
    for row in &rules.mission_count_rewards {
        if !in_period(row.start_at, row.end_at, now) {
            continue;
        }
        let Some(reset) = reset_at(row.reset_cycle, now) else {
            continue;
        };
        let old = message_list(resources, "mission_count_reward_states")
            .into_iter()
            .find(|state| i32_field(state, "category") == Some(row.category));
        if old
            .as_ref()
            .and_then(|s| message_i64_field(s, "reset_at", "seconds"))
            == Some(reset)
        {
            continue;
        }
        let mut value = empty_message(proto, "blend.model.MissionCountRewardState")?;
        value.set_field_by_name("category", Value::I32(row.category));
        value.set_field_by_name("reset_at", Value::Message(timestamp(proto, reset)?));
        put(
            resources,
            "mission_count_reward_states",
            "category",
            value.clone(),
        );
        put(changed, "mission_count_reward_states", "category", value);
    }
    Ok(())
}

pub(crate) fn resource_progress(
    proto: &ProtoRegistry,
    rules: &HomeRules,
    before: &DynamicMessage,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    now: i64,
) -> Result<(), StateError> {
    let old_items: BTreeMap<_, _> = message_list(before, "items")
        .iter()
        .filter_map(|row| {
            Some((
                i32_field(row, "item_id")?,
                i32_field(row, "total_quantity").unwrap_or(0),
            ))
        })
        .collect();
    for item in message_list(resources, "items") {
        let id = i32_field(&item, "item_id").ok_or(StateError::InvalidRequest)?;
        let delta = i32_field(&item, "total_quantity").unwrap_or(0)
            - old_items.get(&id).copied().unwrap_or(0);
        if delta > 0 {
            advance_missions(
                proto,
                rules,
                resources,
                changed,
                now,
                Some((&format!("item_received:{id}"), delta)),
            )?;
            if rules.material_ids.contains(&id) {
                advance_missions(
                    proto,
                    rules,
                    resources,
                    changed,
                    now,
                    Some(("material_received", delta)),
                )?;
            }
            advance_multi_mission_item(proto, rules, resources, changed, id, delta, now)?;
        }
    }
    for quest in message_list(resources, "quest_states") {
        let id = i32_field(&quest, "quest_id").ok_or(StateError::InvalidRequest)?;
        let delta = i32_field(&quest, "clear_count").unwrap_or(0) - quest_clear_count(before, id);
        if delta > 0 {
            advance_missions(
                proto,
                rules,
                resources,
                changed,
                now,
                Some((&format!("quest_clear:{id}"), delta)),
            )?;
            if let Some(row) = rules.quest_kinds.iter().find(|q| q.id == id) {
                for kind in &row.kinds {
                    advance_missions(
                        proto,
                        rules,
                        resources,
                        changed,
                        now,
                        Some((&format!("quest_clear_kind:{kind}"), delta)),
                    )?;
                }
            }
        }
    }
    let cole = message_i32_field(resources, "status", "cole").unwrap_or(0)
        - message_i32_field(before, "status", "cole").unwrap_or(0);
    if cole > 0 {
        advance_missions(
            proto,
            rules,
            resources,
            changed,
            now,
            Some(("cole_received", cole)),
        )?;
    }
    for present in message_list(resources, "present_states") {
        let id = i32_field(&present, "present_id").ok_or(StateError::InvalidRequest)?;
        let old = message_list(before, "present_states")
            .iter()
            .find(|r| i32_field(r, "present_id") == Some(id))
            .and_then(|r| i32_field(r, "total_friendship_point"))
            .unwrap_or(0);
        let delta = i32_field(&present, "total_friendship_point").unwrap_or(0) - old;
        if delta > 0 {
            advance_missions(
                proto,
                rules,
                resources,
                changed,
                now,
                Some((&format!("present_points:{id}"), delta)),
            )?;
        }
    }
    for story in message_list(resources, "character_story_states") {
        let id = i32_field(&story, "character_story_id").ok_or(StateError::InvalidRequest)?;
        let old = message_list(before, "character_story_states")
            .iter()
            .find(|r| i32_field(r, "character_story_id") == Some(id))
            .and_then(|r| i32_field(r, "clear_count"))
            .unwrap_or(0);
        let delta = i32_field(&story, "clear_count").unwrap_or(0) - old;
        if delta > 0 {
            advance_missions(
                proto,
                rules,
                resources,
                changed,
                now,
                Some((&format!("character_story:{id}"), delta)),
            )?;
        }
    }
    advance_missions(proto, rules, resources, changed, now, None)
}

pub(crate) fn quest_attempt_progress(
    proto: &ProtoRegistry,
    rules: &HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    quest: (i32, i32),
    count: i32,
    now: i64,
) -> Result<(), StateError> {
    advance_missions(
        proto,
        rules,
        resources,
        changed,
        now,
        Some((&format!("quest_attempt:{}", quest.0), count)),
    )?;
    if let Some(row) = rules.quest_kinds.iter().find(|r| r.id == quest.0) {
        for kind in &row.kinds {
            advance_missions(
                proto,
                rules,
                resources,
                changed,
                now,
                Some((&format!("quest_attempt_kind:{kind}"), count)),
            )?;
        }
    }
    advance_missions(
        proto,
        rules,
        resources,
        changed,
        now,
        Some(("stamina", quest.1.saturating_mul(count))),
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn synthesis_progress(
    proto: &ProtoRegistry,
    rules: &HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    recipe_id: i32,
    count: i32,
    now: i64,
) -> Result<(), StateError> {
    advance_missions(
        proto,
        rules,
        resources,
        changed,
        now,
        Some(("synthesis", count)),
    )?;
    let target = rules
        .synthesis_targets
        .iter()
        .find(|r| r.recipe_id == recipe_id)
        .ok_or(StateError::InvalidRequest)?;
    let recipe_tool = match target.resource_type {
        14 => rules.battle_tools.iter().find(|r| r.id == target.id),
        6 => rules.equipment_tools.iter().find(|r| r.id == target.id),
        _ => None,
    };
    let outputs = message_list(changed, synthesis_output_field(target.resource_type)?);
    let matches = |rule: &SynthesisObjective| -> i32 {
        if rule
            .resource_type
            .is_some_and(|t| t != target.resource_type)
            || rule.recipe_id.is_some_and(|id| id != recipe_id)
        {
            return 0;
        }
        let tool_matches = |tool: Option<&MissionTool>| {
            !rule
                .slot_type
                .is_some_and(|slot| tool.and_then(|t| t.slot_type) != Some(slot))
                && !rule
                    .role
                    .is_some_and(|role| tool.is_none_or(|t| !t.roles.contains(&role)))
                && !rule
                    .rarity
                    .is_some_and(|rarity| tool.is_none_or(|t| t.rarity != rarity))
        };
        if rule.per_craft
            && rule.min_trait_level.is_none()
            && rule.trait_ids.is_empty()
            && rule.trait_rank.is_none()
        {
            return if tool_matches(recipe_tool) { count } else { 0 };
        }
        let matching = outputs
            .iter()
            .filter(|output| {
                let output_tool =
                    i32_field(output, "tool_id").and_then(|id| match target.resource_type {
                        14 => rules.battle_tools.iter().find(|r| r.id == id),
                        6 => rules.equipment_tools.iter().find(|r| r.id == id),
                        _ => None,
                    });
                let traits = message_list(output, "traits");
                let sum: i32 = traits
                    .iter()
                    .map(|t| i32_field(t, "rank").unwrap_or(0))
                    .sum();
                tool_matches(output_tool)
                    && rule.min_trait_level.is_none_or(|minimum| sum >= minimum)
                    && ((rule.trait_ids.is_empty() && rule.trait_rank.is_none())
                        || traits.iter().any(|trait_| {
                            (rule.trait_ids.is_empty()
                                || i32_field(trait_, "id")
                                    .is_some_and(|id| rule.trait_ids.contains(&id)))
                                && i32_field(trait_, "rank").unwrap_or(0)
                                    >= rule.trait_rank.unwrap_or(0)
                        }))
            })
            .count() as i32;
        matching
    };
    for task in &rules.total_tasks {
        if let Some(rule) = &task.objective.synthesis {
            let delta = matches(rule);
            if delta > 0 {
                update_task(
                    resources,
                    changed,
                    task.condition_id,
                    total_task_count(resources, task.condition_id)
                        .checked_add(delta)
                        .ok_or(StateError::InvalidRequest)?,
                )?;
            }
        }
    }
    for mission in &rules.missions {
        if mission.total_task_condition_id.is_some() || !mission_active(rules, mission, now) {
            continue;
        }
        if let Some(rule) = &mission.objective.synthesis {
            let delta = matches(rule);
            if delta > 0 {
                let mut value = message_list(resources, "missions")
                    .into_iter()
                    .find(|r| i32_field(r, "mission_id") == Some(mission.id))
                    .ok_or(StateError::InvalidRequest)?;
                let cap = mission.steps.last().map(|s| s.count).unwrap_or(0);
                value.set_field_by_name(
                    "count",
                    Value::I32(
                        i32_field(&value, "count")
                            .unwrap_or(0)
                            .saturating_add(delta)
                            .min(cap),
                    ),
                );
                put(resources, "missions", "mission_id", value.clone());
                put(changed, "missions", "mission_id", value);
            }
        }
    }
    Ok(())
}
