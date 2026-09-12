use super::prelude::*;

#[allow(clippy::too_many_arguments)]
pub(crate) fn street(
    proto: &ProtoRegistry,
    rules: &ActivityRules,
    tutorial_rules: &TutorialRules,
    home_rules: &home::HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    route: &str,
    request: &DynamicMessage,
    now: i64,
) -> Result<(), StateError> {
    let id = if route.ends_with("/talk") {
        number(
            row(
                rules,
                "street_talk",
                i32_field(request, "talk_id").ok_or(StateError::InvalidRequest)?,
            )?,
            "quest_id",
        )
    } else {
        i32_field(request, "quest_id").ok_or(StateError::InvalidRequest)?
    };
    let quest = tutorial_rules
        .quests
        .iter()
        .find(|q| q.id == id && q.quest_type == 4)
        .ok_or(StateError::InvalidRequest)?;
    quest::validate_quest(quest, resources, now)?;
    if quest_clear_count(resources, id) > 0 {
        return Err(StateError::InvalidRequest);
    }
    let phases = rows(rules, "street_phase")
        .iter()
        .filter(|r| number(r, "quest_id") == id)
        .collect::<Vec<_>>();
    let first = phases
        .iter()
        .map(|p| number(p, "phase"))
        .min()
        .ok_or(StateError::InvalidRequest)?;
    let mut state = message_list(resources, "street_states")
        .into_iter()
        .find(|s| i32_field(s, "quest_id") == Some(id));
    if route.ends_with("/start") {
        if state.is_none() {
            let mut s = empty_message(proto, "blend.model.StreetState")?;
            s.set_field_by_name("quest_id", Value::I32(id));
            s.set_field_by_name("phase", Value::I32(first));
            state = Some(s);
        }
    } else {
        let s = state.as_mut().ok_or(StateError::InvalidRequest)?;
        let phase = phases
            .iter()
            .find(|p| number(p, "phase") == i32_field(s, "phase").unwrap_or(0))
            .ok_or(StateError::InvalidRequest)?;
        let key = if route.ends_with("/talk") {
            "talk_id"
        } else {
            "move_part_id"
        };
        let requested = i32_field(request, key).ok_or(StateError::InvalidRequest)?;
        let index = values(phase, "triggers")
            .iter()
            .position(|t| number(t, key) == requested && requested > 0)
            .ok_or(StateError::InvalidRequest)? as i32;
        let mut triggered = i32_list(s, "trigger_indices");
        if !triggered.contains(&index) {
            triggered.push(index);
        }
        if key == "talk_id" {
            let mut played = i32_list(s, "played_talk_ids");
            if !played.contains(&requested) {
                played.push(requested);
            }
            s.set_field_by_name(
                "played_talk_ids",
                Value::List(played.into_iter().map(Value::I32).collect()),
            );
        }
        if triggered.len() == values(phase, "triggers").len() {
            if let Some(next) = phases
                .iter()
                .map(|p| number(p, "phase"))
                .filter(|p| *p > number(phase, "phase"))
                .min()
            {
                s.set_field_by_name("phase", Value::I32(next));
                triggered.clear();
            } else {
                let (delta, _) =
                    quest::clear(proto, tutorial_rules, home_rules, resources, quest, now)?;
                merge(changed, delta);
            }
        }
        s.set_field_by_name(
            "trigger_indices",
            Value::List(triggered.into_iter().map(Value::I32).collect()),
        );
    }
    save(
        resources,
        changed,
        "street_states",
        "quest_id",
        state.ok_or(StateError::InvalidRequest)?,
    );
    Ok(())
}

pub(crate) fn expedition_rewards(
    proto: &ProtoRegistry,
    rules: &ActivityRules,
    home_rules: &home::HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    response: &mut DynamicMessage,
    now: i64,
) -> Result<(), StateError> {
    let interval = i64::from(number(&rules.constants, "expedition_interval_seconds")).max(1);
    let cap = i64::from(number(&rules.constants, "expedition_max_hours")) * 3600;
    let rank = i32_field(&status_message(resources)?, "rank").unwrap_or(1);
    let rank_reward = rows(rules, "expedition_user_rank_reward")
        .iter()
        .filter(|r| number(r, "user_rank") <= rank)
        .max_by_key(|r| number(r, "user_rank"))
        .ok_or(StateError::InvalidRequest)?;
    let mut inputs = Vec::new();
    let mut special = Vec::new();
    for mut state in message_list(resources, "expedition_states") {
        let spec = row(
            rules,
            "expedition",
            i32_field(&state, "expedition_id").ok_or(StateError::InvalidRequest)?,
        )?;
        let started = message_i64_field(&state, "started_at", "seconds").unwrap_or(now);
        let end = spec["end_at"].as_i64().unwrap_or(now).min(now);
        let elapsed = (end - started).max(0).min(cap);
        let ticks = elapsed / interval;
        if ticks == 0 {
            continue;
        }
        inputs.push(TutorialReward {
            resource_type: 3,
            id: 1,
            quantity: checked_i32(ticks * i64::from(number(rank_reward, "cole")))?,
            resource_params: None,
        });
        // Local expedition policy: one uniform destination item per complete hour,
        // plus one rank-table supply every six hours. Fractional hours are retained.
        let hours = (end.div_euclid(3600) - started.div_euclid(3600))
            .max(0)
            .min(cap / 3600);
        let items = values(spec, "items");
        for _ in 0..hours {
            if !items.is_empty() {
                let selected = &items[random_below(items.len() as u32)? as usize];
                inputs.push(TutorialReward {
                    resource_type: 5,
                    id: number(selected, "id"),
                    quantity: 1,
                    resource_params: None,
                });
            }
        }
        let supplies = values(rank_reward, "items");
        let supply_count = (end.div_euclid(21_600) - started.div_euclid(21_600))
            .max(0)
            .min(cap / 21_600);
        for _ in 0..supply_count {
            if !supplies.is_empty() {
                let selected = &supplies[random_below(supplies.len() as u32)? as usize];
                special.push(TutorialReward {
                    resource_type: 5,
                    id: number(selected, "id"),
                    quantity: number(selected, "quantity"),
                    resource_params: None,
                });
            }
        }
        state.set_field_by_name(
            "started_at",
            Value::Message(timestamp(
                proto,
                if end - started > cap {
                    end
                } else {
                    started + ticks * interval
                },
            )?),
        );
        save(resources, changed, "expedition_states", "number", state);
    }
    inputs.retain(|r| r.quantity > 0);
    response.set_field_by_name(
        "rewards",
        Value::List(home::grant(
            proto, home_rules, resources, changed, &inputs, now,
        )?),
    );
    response.set_field_by_name(
        "special_rewards",
        Value::List(home::grant(
            proto, home_rules, resources, changed, &special, now,
        )?),
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn expedition(
    proto: &ProtoRegistry,
    rules: &ActivityRules,
    home_rules: &home::HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    response: &mut DynamicMessage,
    route: &str,
    request: &DynamicMessage,
    now: i64,
) -> Result<(), StateError> {
    if route.ends_with("/start") {
        let entries = message_list(request, "new_expeditions");
        let max = i32_field(&status_message(resources)?, "expedition_max_count").unwrap_or(1);
        if entries.is_empty() || entries.len() > max as usize {
            return Err(StateError::InvalidRequest);
        }
        let mut numbers = BTreeSet::new();
        let mut assigned = BTreeSet::new();
        for entry in &entries {
            let slot = i32_field(entry, "number").unwrap_or(0);
            let ids = i32_list(entry, "character_ids");
            if !(1..=max).contains(&slot)
                || !numbers.insert(slot)
                || ids.is_empty()
                || ids.len() > 5
                || ids
                    .iter()
                    .any(|id| !character_present(resources, *id) || !assigned.insert(*id))
            {
                return Err(StateError::InvalidRequest);
            }
            eligible(
                row(
                    rules,
                    "expedition",
                    i32_field(entry, "expedition_id").ok_or(StateError::InvalidRequest)?,
                )?,
                resources,
                now,
            )?;
        }
        for current in message_list(resources, "expedition_states") {
            if !numbers.contains(&i32_field(&current, "number").unwrap_or(0))
                && i32_list(&current, "character_ids")
                    .iter()
                    .any(|id| assigned.contains(id))
            {
                return Err(StateError::InvalidRequest);
            }
        }
        expedition_rewards(proto, rules, home_rules, resources, changed, response, now)?;
        for entry in entries {
            let mut state = empty_message(proto, "blend.model.ExpeditionState")?;
            for name in ["number", "expedition_id", "character_ids"] {
                state.set_field_by_name(
                    name,
                    entry
                        .get_field_by_name(name)
                        .ok_or(StateError::InvalidRequest)?
                        .into_owned(),
                );
            }
            state.set_field_by_name("started_at", Value::Message(timestamp(proto, now)?));
            save(resources, changed, "expedition_states", "number", state);
        }
    } else {
        expedition_rewards(proto, rules, home_rules, resources, changed, response, now)?;
    }
    Ok(())
}

pub(crate) fn skip_talks(run: &mut ExplorationRun) {
    while run.steps.get(run.next).is_some_and(|s| s.kind == 2) {
        run.next += 1;
    }
}

pub(crate) fn exploration_steps(
    rules: &ActivityRules,
    quest_id: i32,
) -> Result<Vec<ExplorationStep>, StateError> {
    let mut areas = rows(rules, "exploration_area")
        .iter()
        .filter(|a| number(a, "quest_id") == quest_id)
        .collect::<Vec<_>>();
    areas.sort_by_key(|a| (number(a, "floor_index"), number(a, "number")));
    let mut steps = Vec::new();
    for area in areas {
        let mut options = Vec::new();
        let mut weights = Vec::new();
        for (kind, key) in [(0, "gatherings"), (1, "battles"), (2, "talks")] {
            for (index, option) in values(area, key).iter().enumerate() {
                options.push((kind, index, option));
                weights.push(if kind == 2 {
                    1
                } else {
                    number(option, "weight").max(0) as u32
                });
            }
        }
        let (kind, index, option) = options[weighted_index(&weights)?];
        steps.push(ExplorationStep {
            area_id: number(area, "id"),
            kind,
            index,
            points: if kind == 0 {
                number(option, "point_count")
            } else {
                0
            },
            battle_id: option["battle_id"].as_i64().map(|v| v as i32),
            exp: number(option, "character_exp"),
            cole: number(option, "cole"),
        });
    }
    if steps.is_empty() {
        return Err(StateError::InvalidRequest);
    }
    Ok(steps)
}
