use super::prelude::*;

pub(crate) fn progress(
    proto: &ProtoRegistry,
    tutorial_rules: &TutorialRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    run: &ExplorationRun,
) -> Result<(), StateError> {
    let quest = tutorial_rules
        .quests
        .iter()
        .find(|q| q.id == run.quest_id)
        .ok_or(StateError::InvalidRequest)?;
    let story = quest.episode_type == 1;
    let mut value = empty_message(proto, "blend.model.ExplorationProgress")?;
    value.set_field_by_name("is_story", Value::Bool(story));
    value.set_field_by_name("quest_id", Value::I32(run.quest_id));
    value.set_field_by_name(
        "area_id",
        Value::I32(
            run.steps
                .get(run.next)
                .or_else(|| run.steps.last())
                .ok_or(StateError::InvalidRequest)?
                .area_id,
        ),
    );
    value.set_field_by_name(
        "route_types",
        Value::List(
            run.steps
                .iter()
                .map(|s| Value::EnumNumber(s.kind))
                .collect(),
        ),
    );
    value.set_field_by_name(
        "route_indices",
        Value::List(
            run.steps
                .iter()
                .enumerate()
                .map(|(i, s)| Value::I32(if i < run.next { -1 } else { s.index as i32 }))
                .collect(),
        ),
    );
    value.set_field_by_name(
        "gathering_states",
        Value::List(
            (0..run.steps.get(run.next).map(|s| s.points).unwrap_or(0))
                .map(|p| Value::I32(i32::from(run.gathered.contains(&p))))
                .collect(),
        ),
    );
    value.set_field_by_name("party_number", Value::I32(run.party_number));
    value.set_field_by_name("character_exp", Value::I32(run.character_exp));
    value.set_field_by_name("total_turn", Value::I32(run.total_turn));
    if let Some(id) = run.control_character_id {
        value.set_field_by_name(
            "control_character_id",
            Value::Message(int32_value(proto, id)?),
        );
    }
    let mut party = empty_message(proto, "blend.model.ExplorationPartyStatus")?;
    party.set_field_by_name(
        "hps",
        Value::List(run.hps.iter().copied().map(Value::I32).collect()),
    );
    party.set_field_by_name("party_gauge", Value::I32(run.party_gauge));
    party.set_field_by_name(
        "battle_tool_usage_counts",
        Value::List(run.tool_counts.iter().copied().map(Value::I32).collect()),
    );
    value.set_field_by_name("party_status", Value::Message(party));
    let rewards = run
        .rewards
        .iter()
        .map(|(kind, id, quantity)| {
            reward_message(
                proto,
                &TutorialReward {
                    resource_type: *kind,
                    id: *id,
                    quantity: *quantity,
                    resource_params: None,
                },
                false,
            )
            .map(Value::Message)
        })
        .collect::<Result<Vec<_>, _>>()?;
    value.set_field_by_name("rewards", Value::List(rewards));
    for target in [resources, changed] {
        let mut list = message_list(target, "exploration_progresses");
        list.retain(|p| bool_field(p, "is_story") != story);
        list.push(value.clone());
        target.set_field_by_name(
            "exploration_progresses",
            Value::List(list.into_iter().map(Value::Message).collect()),
        );
    }
    Ok(())
}

pub(crate) fn remove_progress(
    proto: &ProtoRegistry,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    story: bool,
) -> Result<(), StateError> {
    let mut empty = empty_message(proto, "blend.model.ExplorationProgress")?;
    empty.set_field_by_name("is_story", Value::Bool(story));
    for target in [resources, changed] {
        let mut list = message_list(target, "exploration_progresses");
        list.retain(|p| bool_field(p, "is_story") != story);
        list.push(empty.clone());
        target.set_field_by_name(
            "exploration_progresses",
            Value::List(list.into_iter().map(Value::Message).collect()),
        );
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn exploration(
    proto: &ProtoRegistry,
    rules: &ActivityRules,
    tutorial_rules: &TutorialRules,
    atelier: &atelier::AtelierRules,
    characters: &CharacterRules,
    reward_rules: &RewardRules,
    home_rules: &home::HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    response: &mut DynamicMessage,
    saved: &mut ActivityState,
    route: &str,
    request: &DynamicMessage,
    now: i64,
) -> Result<(), StateError> {
    if route == "/exploration/retire" {
        let story = bool_field(request, "is_story");
        let id = saved
            .explorations
            .keys()
            .copied()
            .find(|id| {
                tutorial_rules
                    .quests
                    .iter()
                    .any(|q| q.id == *id && (q.episode_type == 1) == story)
            })
            .ok_or(StateError::InvalidRequest)?;
        if saved.explorations[&id].pending {
            return Err(StateError::InvalidRequest);
        }
        saved.explorations.remove(&id);
        return remove_progress(proto, resources, changed, story);
    }
    let quest_id = if let Some(id) = i32_field(request, "quest_id").filter(|id| *id > 0) {
        id
    } else if let Some(id) = i32_field(request, "area_id").filter(|id| *id > 0) {
        number(row(rules, "exploration_area", id)?, "quest_id")
    } else {
        *saved
            .explorations
            .keys()
            .next()
            .ok_or(StateError::InvalidRequest)?
    };
    let quest = tutorial_rules
        .quests
        .iter()
        .find(|q| q.id == quest_id && q.quest_type == 3)
        .ok_or(StateError::InvalidRequest)?;
    quest::validate_quest(quest, resources, now)?;
    match route {
        "/exploration/skip" => {
            let count = i32_field(request, "repeat_count")
                .filter(|n| {
                    (1..=number(&rules.constants, "max_exploration_repeat_count")).contains(n)
                })
                .ok_or(StateError::InvalidRequest)?;
            if !matches!(quest.skippable_type, 2 | 3)
                || quest_clear_count(resources, quest_id) == 0
                || saved.explorations.contains_key(&quest_id)
            {
                return Err(StateError::InvalidRequest);
            }
            let party = i32_field(request, "party_number").ok_or(StateError::InvalidRequest)?;
            let (members, _) =
                resolve_account_party(tutorial_rules, atelier, characters, resources, party)?;
            let mut per_clear = Vec::new();
            let mut awarded = Vec::new();
            for _ in 0..count {
                quest::charge_quest(proto, home_rules, quest, resources, changed, now)?;
                let mut inputs = roll_quest_rewards(reward_rules, quest_id)?;
                let mut exp = 0i32;
                for step in exploration_steps(rules, quest_id)? {
                    if step.kind == 1 {
                        exp = exp
                            .checked_add(step.exp)
                            .ok_or(StateError::InvalidRequest)?;
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
                per_clear.push(Value::Message(home::reward_list(proto, &inputs)?));
                awarded.extend(home::grant(
                    proto, home_rules, resources, changed, &inputs, now,
                )?);
                for member in &members {
                    quest::character_exp(
                        proto,
                        characters,
                        resources,
                        changed,
                        member.character_id,
                        exp,
                    )?;
                }
                let (delta, rewards) =
                    quest::clear(proto, tutorial_rules, home_rules, resources, quest, now)?;
                merge(changed, delta);
                awarded.extend(rewards);
            }
            changed.set_field_by_name("status", Value::Message(status_message(resources)?));
            response.set_field_by_name("rewards_per_clear", Value::List(per_clear));
            response.set_field_by_name("rewards", Value::List(awarded));
        }
        "/exploration/start" => {
            if saved.explorations.values().any(|r| {
                tutorial_rules.quests.iter().any(|q| {
                    q.id == r.quest_id && (q.episode_type == 1) == (quest.episode_type == 1)
                })
            }) {
                return Err(StateError::InvalidRequest);
            }
            let party_number =
                i32_field(request, "party_number").ok_or(StateError::InvalidRequest)?;
            let (members, tools) = resolve_account_party(
                tutorial_rules,
                atelier,
                characters,
                resources,
                party_number,
            )?;
            let control = optional_i32_field(request, "control_character_id");
            if control.is_some_and(|id| !members.iter().any(|c| c.character_id == id)) {
                return Err(StateError::InvalidRequest);
            }
            quest::charge_quest(proto, home_rules, quest, resources, changed, now)?;
            let steps = exploration_steps(rules, quest_id)?;
            let mut run = ExplorationRun {
                quest_id,
                party_number,
                steps,
                next: 0,
                gathered: BTreeSet::new(),
                rewards: Vec::new(),
                character_exp: 0,
                total_turn: 0,
                hps: members
                    .iter()
                    .map(|m| {
                        m.integrated_stats
                            .map(|s| s.hp)
                            .ok_or(StateError::InvalidRequest)
                    })
                    .collect::<Result<Vec<_>, _>>()?,
                tool_counts: tools.iter().map(|t| t.usage_count).collect(),
                party_gauge: 500,
                pending: false,
                character_ids: members.iter().map(|m| m.character_id).collect(),
                earned_exp: BTreeMap::new(),
                control_character_id: control,
            };
            skip_talks(&mut run);
            progress(proto, tutorial_rules, resources, changed, &run)?;
            saved.explorations.insert(quest_id, run);
            changed.set_field_by_name("status", Value::Message(status_message(resources)?));
        }
        "/exploration/explore" => {
            let run = saved
                .explorations
                .get_mut(&quest_id)
                .ok_or(StateError::InvalidRequest)?;
            if run.pending {
                return Err(StateError::InvalidRequest);
            }
            skip_talks(run);
            let step = run.steps.get(run.next).ok_or(StateError::InvalidRequest)?;
            let point = i32_field(request, "gathering_index").ok_or(StateError::InvalidRequest)?;
            if step.area_id != i32_field(request, "area_id").unwrap_or(0)
                || step.kind != 0
                || !(0..step.points).contains(&point)
                || !run.gathered.insert(point)
            {
                return Err(StateError::InvalidRequest);
            }
            if run.gathered.len() == step.points as usize {
                run.next += 1;
                run.gathered.clear();
                skip_talks(run);
            }
            progress(proto, tutorial_rules, resources, changed, run)?;
            response.set_field_by_name(
                "result",
                Value::Message(empty_message(proto, "blend.model.ExplorationResult")?),
            );
        }
        "/exploration/finish" => {
            let run = saved
                .explorations
                .get_mut(&quest_id)
                .ok_or(StateError::InvalidRequest)?;
            skip_talks(run);
            if run.pending || run.next != run.steps.len() {
                return Err(StateError::InvalidRequest);
            }
            let (delta, mut awarded) =
                quest::clear(proto, tutorial_rules, home_rules, resources, quest, now)?;
            merge(changed, delta);
            let mut inputs = run
                .rewards
                .iter()
                .map(|(kind, id, quantity)| TutorialReward {
                    resource_type: *kind,
                    id: *id,
                    quantity: *quantity,
                    resource_params: None,
                })
                .collect::<Vec<_>>();
            inputs.extend(roll_quest_rewards(reward_rules, quest_id)?);
            awarded.extend(home::grant(
                proto, home_rules, resources, changed, &inputs, now,
            )?);
            for (id, exp) in &run.earned_exp {
                quest::character_exp(proto, characters, resources, changed, *id, *exp)?;
            }
            response.set_field_by_name("rewards", Value::List(awarded));
            saved.explorations.remove(&quest_id);
            remove_progress(proto, resources, changed, quest.episode_type == 1)?;
        }
        "/exploration/update_party" => {
            let run = saved
                .explorations
                .get_mut(&quest_id)
                .ok_or(StateError::InvalidRequest)?;
            // Preserve health: party changes are permitted before the first fight.
            if run.pending || run.total_turn > 0 {
                return Err(StateError::InvalidRequest);
            }
            let number = i32_field(request, "party_number").ok_or(StateError::InvalidRequest)?;
            let (members, tools) =
                resolve_account_party(tutorial_rules, atelier, characters, resources, number)?;
            run.party_number = number;
            run.character_ids = members.iter().map(|m| m.character_id).collect();
            run.hps = members
                .iter()
                .map(|m| {
                    m.integrated_stats
                        .map(|s| s.hp)
                        .ok_or(StateError::InvalidRequest)
                })
                .collect::<Result<Vec<_>, _>>()?;
            run.tool_counts = tools.iter().map(|t| t.usage_count).collect();
            progress(proto, tutorial_rules, resources, changed, run)?;
        }
        _ => return Err(StateError::InvalidRequest),
    }
    Ok(())
}
