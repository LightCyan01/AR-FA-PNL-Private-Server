use super::prelude::*;

/// Empty-request story skips operate on the current contiguous story frontier.
/// They do not select a future chapter, waive quest prerequisites, or fabricate
/// battle actions. The account's default story party receives ordinary clear EXP.
#[allow(clippy::too_many_arguments)]
pub(crate) fn story_skip(
    proto: &ProtoRegistry,
    rules: &ActivityRules,
    tutorial_rules: &TutorialRules,
    atelier: &atelier::AtelierRules,
    characters: &CharacterRules,
    reward_rules: &RewardRules,
    home_rules: &home::HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    saved: &ActivityState,
    route: &str,
    now: i64,
) -> Result<(), StateError> {
    if !saved.explorations.is_empty() {
        return Err(StateError::InvalidRequest);
    }
    let episode_priority = |id| {
        rows(rules, "episode")
            .iter()
            .find(|e| number(e, "id") == id)
            .map(|e| number(e, "priority"))
    };
    let mut story = tutorial_rules
        .quests
        .iter()
        .filter(|q| q.episode_type == 1)
        .collect::<Vec<_>>();
    story.sort_by_key(|q| (episode_priority(q.episode_id), q.priority, q.id));
    let last = i32_field(&status_message(resources)?, "last_main_story_quest_id").unwrap_or(0);
    let prologue = route == "/quest/talk_event/prologue_skip";
    let start = if last == 0 {
        0
    } else {
        story
            .iter()
            .position(|q| q.id == last)
            .ok_or(StateError::InvalidRequest)?
            + 1
    };
    if story[..start]
        .iter()
        .any(|q| quest_clear_count(resources, q.id) == 0)
        || story[start..]
            .iter()
            .any(|q| quest_clear_count(resources, q.id) != 0)
    {
        return Err(StateError::InvalidRequest);
    }
    let end = if prologue {
        if last != 0 || tutorial_step(resources) != 0 {
            return Err(StateError::InvalidRequest);
        }
        let episode = story.first().ok_or(StateError::InvalidRequest)?.episode_id;
        story
            .iter()
            .rposition(|q| q.episode_id == episode && q.day == Some(0))
            .ok_or(StateError::InvalidRequest)?
    } else {
        if start == 0 || tutorial_step(resources) < home::TUTORIAL_STEP_HOME_READY {
            return Err(StateError::InvalidRequest);
        }
        match route {
            "/quest/talk_event/main_story_episode_skip" => {
                if rules.constants["episode_skip_start_at"]
                    .as_i64()
                    .is_none_or(|t| now < t)
                {
                    return Err(StateError::OutOfSchedule);
                }
                let episode = story[start - 1].episode_id;
                let priority = episode_priority(episode).ok_or(StateError::InvalidRequest)?;
                let latest = rows(rules, "episode")
                    .iter()
                    .filter(|e| number(e, "episode_type") == 1)
                    .filter(|e| home::in_period(e["start_at"].as_i64(), e["end_at"].as_i64(), now))
                    .map(|e| number(e, "priority"))
                    .max()
                    .ok_or(StateError::InvalidRequest)?;
                if priority > latest - number(&rules.constants, "skippable_episode_priority_diff") {
                    return Err(StateError::InvalidRequest);
                }
                story
                    .iter()
                    .rposition(|q| q.episode_id == episode)
                    .ok_or(StateError::InvalidRequest)?
            }
            "/quest/talk_event/main_story_season_skip" => {
                let section = rows(rules, "season")
                    .iter()
                    .filter(|s| {
                        s["key_quest_id"].is_null()
                            || quest_clear_count(resources, number(s, "key_quest_id")) > 0
                    })
                    .map(|s| number(s, "section"))
                    .max()
                    .ok_or(StateError::InvalidRequest)?;
                let next = rows(rules, "season")
                    .iter()
                    .filter(|s| number(s, "section") > section)
                    .min_by_key(|s| (number(s, "section"), number(s, "sub_section")))
                    .ok_or(StateError::InvalidRequest)?;
                story
                    .iter()
                    .position(|q| q.id == number(next, "key_quest_id"))
                    .ok_or(StateError::InvalidRequest)?
            }
            _ => return Err(StateError::InvalidRequest),
        }
    };
    if start > end {
        return Err(StateError::InvalidRequest);
    }
    let fresh = load_fresh_rules()?;
    for quest in &story[start..=end] {
        quest::validate_quest(quest, resources, now)?;
        if prologue && quest.id == story[0].id {
            let mutation = reduce_talk_event_with_rules(
                proto,
                &fresh,
                tutorial_rules,
                home_rules,
                resources.clone(),
                quest.id,
                now,
            )?;
            *resources = mutation.resources;
            merge(
                changed,
                member_status(&mutation.response, "changed_resources")?,
            );
            continue;
        }
        let mut exp = 0i32;
        let mut waves = Vec::new();
        if matches!(quest.quest_type, 1 | 3) {
            quest::charge_quest(proto, home_rules, quest, resources, changed, now)?;
            let mut inputs = roll_quest_rewards(reward_rules, quest.id)?;
            if quest.quest_type == 3 {
                for step in exploration_steps(rules, quest.id)? {
                    if step.kind == 1 {
                        exp = exp
                            .checked_add(step.exp)
                            .ok_or(StateError::InvalidRequest)?;
                        waves.extend_from_slice(
                            &rule_battle(
                                tutorial_rules,
                                step.battle_id.ok_or(StateError::InvalidRequest)?,
                            )?
                            .wave_ids,
                        );
                        if step.cole > 0 {
                            inputs.push(TutorialReward {
                                resource_type: 3,
                                id: 1,
                                quantity: step.cole,
                                resource_params: None,
                            });
                        }
                    }
                }
            } else {
                waves.extend_from_slice(
                    &rule_battle(
                        tutorial_rules,
                        quest.battle_id.ok_or(StateError::InvalidRequest)?,
                    )?
                    .wave_ids,
                );
                if quest.fixed_party_id.is_none() {
                    exp = quest.character_exp;
                }
            }
            home::grant(proto, home_rules, resources, changed, &inputs, now)?;
        }
        let (delta, _) = quest::clear(proto, tutorial_rules, home_rules, resources, quest, now)?;
        merge(changed, delta);
        if !waves.is_empty() {
            let (party, _) = match quest.fixed_party_id {
                Some(id) => {
                    resolve_fixed_party_with_rules(proto, tutorial_rules, atelier, characters, id)?
                }
                None => resolve_account_party(
                    tutorial_rules,
                    atelier,
                    characters,
                    resources,
                    fresh.protocol_defaults.party_number,
                )?,
            };
            let participants = party.iter().map(|m| m.character_id).collect();
            home::completed_battle_progress(
                proto,
                home_rules,
                tutorial_rules,
                &participants,
                &waves,
                resources,
                changed,
                quest.id,
                quest.quest_type == 1,
                now,
            )?;
            let defeated = waves.iter().try_fold(0i32, |count, id| {
                count
                    .checked_add(rule_wave(tutorial_rules, *id)?.enemies.len() as i32)
                    .ok_or(StateError::InvalidRequest)
            })?;
            home::advance_missions(
                proto,
                home_rules,
                resources,
                changed,
                now,
                Some(("enemy_defeat", defeated)),
            )?;
        }
        if exp > 0 {
            let (party, _) = resolve_account_party(
                tutorial_rules,
                atelier,
                characters,
                resources,
                fresh.protocol_defaults.party_number,
            )?;
            for member in party {
                quest::character_exp(
                    proto,
                    characters,
                    resources,
                    changed,
                    member.character_id,
                    exp,
                )?;
            }
        }
    }
    if prologue {
        // Skip the instructional checkpoints, not their unrelated synthesis or
        // one-time gacha mutations. The next real action remains the tutorial draw.
        let mut status = status_message(resources)?;
        status.set_field_by_name("tutorial_step", Value::I32(TUTORIAL_STEP_MEMORIA_EQUIPPED));
        resources.set_field_by_name("status", Value::Message(status));
    }
    changed.set_field_by_name("status", Value::Message(status_message(resources)?));
    Ok(())
}

pub(crate) fn weighted_index(weights: &[u32]) -> Result<usize, StateError> {
    let total = weights
        .iter()
        .try_fold(0_u32, |sum, w| sum.checked_add(*w))
        .filter(|sum| *sum > 0)
        .ok_or(StateError::InvalidRequest)?;
    let mut draw = random_below(total)?;
    for (index, weight) in weights.iter().enumerate() {
        if draw < *weight {
            return Ok(index);
        }
        draw -= weight;
    }
    Err(StateError::InvalidRequest)
}

pub(crate) fn refresh_dishes(
    proto: &ProtoRegistry,
    rules: &ActivityRules,
    resources: &mut DynamicMessage,
    now: i64,
) -> Result<(), StateError> {
    let mut status = status_message(resources)?;
    let updated = message_i64_field(&status, "dishes_updated_at", "seconds").unwrap_or(now);
    if now <= updated {
        return Ok(());
    }
    let times = values(&rules.constants, "dish_charge_minutes");
    let charges = times
        .iter()
        .filter_map(Json::as_i64)
        .map(|minute| {
            ((now + 32_400 - minute * 60).div_euclid(86_400)
                - (updated + 32_400 - minute * 60).div_euclid(86_400))
            .max(0)
        })
        .sum::<i64>();
    let total = i64::from(i32_field(&status, "dishes_when_updated").unwrap_or(0)) + charges;
    let cap = times.len() as i64;
    let idle = i64::from(i32_field(&status, "dish_idleness").unwrap_or(0)) + (total - cap).max(0);
    status.set_field_by_name(
        "dishes_when_updated",
        Value::I32(total.min(cap).max(0) as i32),
    );
    status.set_field_by_name(
        "dish_idleness",
        Value::I32(idle.min(i64::from(number(
            &rules.constants,
            "dish_threshold_idleness",
        ))) as i32),
    );
    status.set_field_by_name("dishes_updated_at", Value::Message(timestamp(proto, now)?));
    resources.set_field_by_name("status", Value::Message(status));
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn dish(
    proto: &ProtoRegistry,
    rules: &ActivityRules,
    home_rules: &home::HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    response: &mut DynamicMessage,
    request: &DynamicMessage,
    now: i64,
) -> Result<(), StateError> {
    let spec = row(
        rules,
        "dish",
        i32_field(request, "dish_id").ok_or(StateError::InvalidRequest)?,
    )?;
    eligible(spec, resources, now)?;
    if let Some(season) = spec["season_id"].as_i64() {
        eligible(row(rules, "season", season as i32)?, resources, now)?;
    }
    refresh_dishes(proto, rules, resources, now)?;
    let mut status = status_message(resources)?;
    let count = i32_field(&status, "dishes_when_updated")
        .unwrap_or(0)
        .checked_sub(1)
        .filter(|v| *v >= 0)
        .ok_or(StateError::InvalidRequest)?;
    status.set_field_by_name("dishes_when_updated", Value::I32(count));
    status.set_field_by_name("dish_idleness", Value::I32(0));
    resources.set_field_by_name("status", Value::Message(status.clone()));
    changed.set_field_by_name("status", Value::Message(status));
    let awarded = home::grant(
        proto,
        home_rules,
        resources,
        changed,
        &rewards(spec, "rewards")?,
        now,
    )?;
    response.set_field_by_name("rewards", Value::List(awarded));
    home::advance_missions(
        proto,
        home_rules,
        resources,
        changed,
        now,
        Some(("order_receive", 1)),
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn character_story(
    proto: &ProtoRegistry,
    rules: &ActivityRules,
    home_rules: &home::HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    response: &mut DynamicMessage,
    request: &DynamicMessage,
    now: i64,
) -> Result<(), StateError> {
    let id = i32_field(request, "character_story_id").ok_or(StateError::InvalidRequest)?;
    let spec = row(rules, "character_story", id)?;
    eligible(spec, resources, now)?;
    for key in values(spec, "key_stories") {
        let prior = message_list(resources, "character_story_states")
            .into_iter()
            .find(|s| {
                i32_field(s, "character_story_id") == Some(number(key, "id"))
                    && i32_field(s, "clear_count").unwrap_or(0) > 0
            })
            .ok_or(StateError::InvalidRequest)?;
        if let Some(condition) = key["condition"].as_i64() {
            if !i32_list(&prior, "condition_states").contains(&(condition as i32)) {
                return Err(StateError::InvalidRequest);
            }
        }
    }
    let mut state = message_list(resources, "character_story_states")
        .into_iter()
        .find(|s| i32_field(s, "character_story_id") == Some(id))
        .unwrap_or(empty_message(proto, "blend.model.CharacterStoryState")?);
    let condition = optional_i32_field(request, "condition");
    let allowed = rows(rules, "character_story")
        .iter()
        .flat_map(|s| values(s, "key_stories"))
        .filter(|k| number(k, "id") == id)
        .filter_map(|k| k["condition"].as_i64())
        .map(|c| c as i32)
        .collect::<BTreeSet<_>>();
    if condition.is_some_and(|c| !allowed.contains(&c))
        || (spec["subsequent_branch"].as_bool() == Some(true)
            && !allowed.is_empty()
            && condition.is_none())
    {
        return Err(StateError::InvalidRequest);
    }
    let first = i32_field(&state, "clear_count").unwrap_or(0) == 0;
    state.set_field_by_name("character_story_id", Value::I32(id));
    state.set_field_by_name(
        "clear_count",
        Value::I32(
            i32_field(&state, "clear_count")
                .unwrap_or(0)
                .checked_add(1)
                .ok_or(StateError::InvalidRequest)?,
        ),
    );
    if let Some(condition) = condition {
        let mut conditions = i32_list(&state, "condition_states");
        if !conditions.contains(&condition) {
            conditions.push(condition);
        }
        state.set_field_by_name(
            "condition_states",
            Value::List(conditions.into_iter().map(Value::I32).collect()),
        );
    }
    save(
        resources,
        changed,
        "character_story_states",
        "character_story_id",
        state,
    );
    if first {
        response.set_field_by_name(
            "rewards",
            Value::List(home::grant(
                proto,
                home_rules,
                resources,
                changed,
                &rewards(spec, "rewards")?,
                now,
            )?),
        );
    }
    Ok(())
}
