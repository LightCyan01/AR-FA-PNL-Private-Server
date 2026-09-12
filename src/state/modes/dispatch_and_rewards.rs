use crate::state::service::prelude::*;

// Mode state and rewards share the ordinary account transaction.
use activities::{row, rows};
use home::put;
use std::collections::BTreeSet;

pub(crate) fn grant_set(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    home: &home::HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    id: i32,
    now: i64,
) -> Result<Vec<Value>, StateError> {
    let rewards = rules
        .reward_sets
        .iter()
        .find(|r| r.id == id)
        .ok_or(StateError::InvalidRequest)?;
    home::grant(proto, home, resources, changed, &rewards.rewards, now)
}

pub(crate) fn is_route(route: &str) -> bool {
    matches!(
        route,
        "/event/revive"
            | "/event/damage_contest"
            | "/event/legend_challenge"
            | "/solo_raid/reset"
            | "/total_battle/reset_panel"
            | "/quest/score_rank_first_reward_receive"
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    activity: &activities::ActivityRules,
    home: &home::HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    response: &mut DynamicMessage,
    route: &str,
    request: &DynamicMessage,
    now: i64,
) -> Result<(), StateError> {
    match route {
        "/event/revive" => {
            let id = i32_field(request, "event_id").ok_or(StateError::InvalidRequest)?;
            let event = row(activity, "event", id)?;
            if event["revival_start_at"]
                .as_i64()
                .is_none_or(|start| start > now)
            {
                return Err(StateError::OutOfSchedule);
            }
            if message_list(resources, "revived_events")
                .iter()
                .any(|r| i32_field(r, "event_id") == Some(id))
            {
                return Err(StateError::InvalidRequest);
            }
            shop::pay(
                proto,
                resources,
                changed,
                5,
                number(&activity.constants, "event_revival_cost_item_id"),
                1,
            )?;
            let mut state = empty_message(proto, "blend.model.RevivedEvent")?;
            state.set_field_by_name("event_id", Value::I32(id));
            put(resources, "revived_events", "event_id", state.clone());
            put(changed, "revived_events", "event_id", state);
        }
        "/event/damage_contest" | "/event/legend_challenge" => {
            let id = i32_field(request, "episode_id").ok_or(StateError::InvalidRequest)?;
            let episode = row(activity, "episode", id)?;
            let legend = route == "/event/legend_challenge";
            if number(episode, "episode_category") != if legend { 19 } else { 16 } {
                return Err(StateError::InvalidRequest);
            }
            let mut rank = empty_message(
                proto,
                if legend {
                    "blend.model.LegendChallengeRank"
                } else {
                    "blend.model.DamageContestRank"
                },
            )?;
            // No cross-account ranking service: absent placement is not a fake rank 1.
            if legend {
                let mut episode_id = empty_message(proto, "google.protobuf.Int64Value")?;
                episode_id.set_field_by_name("value", Value::I64(i64::from(id)));
                rank.set_field_by_name("episode_id", Value::Message(episode_id));
                let total = rules
                    .quests
                    .iter()
                    .filter(|q| q.episode_id == id)
                    .filter_map(|q| {
                        message_list(resources, "quest_states")
                            .into_iter()
                            .find(|s| i32_field(s, "quest_id") == Some(q.id))
                    })
                    .map(|s| {
                        i64::from(
                            message_i32_field(&s, "high_score_detail", "total_score").unwrap_or(0),
                        )
                    })
                    .sum();
                let mut best = empty_message(proto, "google.protobuf.Int64Value")?;
                best.set_field_by_name("value", Value::I64(total));
                rank.set_field_by_name("best_total_score", Value::Message(best));
            }
            response.set_field_by_name("rank", Value::Message(rank));
        }
        "/quest/score_rank_first_reward_receive" => {
            let id = i32_field(request, "quest_id").ok_or(StateError::InvalidRequest)?;
            let quest = rules
                .quests
                .iter()
                .find(|q| q.id == id && !q.score_battle.is_null())
                .ok_or(StateError::InvalidRequest)?;
            let mut state = message_list(resources, "quest_states")
                .into_iter()
                .find(|s| i32_field(s, "quest_id") == Some(id))
                .ok_or(StateError::InvalidRequest)?;
            let rank = i32_field(&state, "score_rank").unwrap_or(0);
            let received = i32_field(&state, "first_reward_received_score_rank").unwrap_or(0);
            let ranks = values(&quest.score_battle, "ranks");
            if rank <= received || !ranks.iter().any(|r| number(r, "rank") == rank) {
                return Err(StateError::InvalidRequest);
            }
            let mut rewards = Vec::new();
            for r in ranks
                .iter()
                .filter(|r| received < number(r, "rank") && number(r, "rank") <= rank)
            {
                if let Some(id) = r["first_reward_set_id"].as_i64() {
                    rewards.extend(grant_set(
                        proto, rules, home, resources, changed, id as i32, now,
                    )?);
                }
            }
            state.set_field_by_name("first_reward_received_score_rank", Value::I32(rank));
            put(resources, "quest_states", "quest_id", state.clone());
            put(changed, "quest_states", "quest_id", state);
            response.set_field_by_name("rewards", Value::List(rewards));
        }
        "/total_battle/reset_panel" => {
            let id =
                i32_field(request, "total_battle_panel_id").ok_or(StateError::InvalidRequest)?;
            let spec = row(activity, "total_battle_panel", id)?;
            let mut state = message_list(resources, "total_battle_panel_states")
                .into_iter()
                .find(|s| {
                    i32_field(s, "total_battle_panel_id") == Some(id)
                        && !i32_list(s, "character_ids").is_empty()
                })
                .ok_or(StateError::InvalidRequest)?;
            if i32_field(&state, "total_battle_id") != Some(number(spec, "total_battle_id")) {
                return Err(StateError::InvalidRequest);
            }
            state.clear_field_by_name("character_ids");
            put(
                resources,
                "total_battle_panel_states",
                "total_battle_panel_id",
                state.clone(),
            );
            put(
                changed,
                "total_battle_panel_states",
                "total_battle_panel_id",
                state,
            );
            // Best lines and first-clear rewards remain claimed after a reset.
        }
        "/solo_raid/reset" => {
            let id = i32_field(request, "quest_id").ok_or(StateError::InvalidRequest)?;
            let quest = rules
                .quests
                .iter()
                .find(|q| q.id == id && q.solo_raid_id.is_some())
                .ok_or(StateError::InvalidRequest)?;
            let mut state = message_list(resources, "solo_raid_states")
                .into_iter()
                .find(|s| i32_field(s, "quest_id") == Some(id) && s.has_field_by_name("entered_at"))
                .ok_or(StateError::InvalidRequest)?;
            if let Some(mut episode) = message_list(resources, "episode_states")
                .into_iter()
                .find(|s| i32_field(s, "episode_id") == Some(quest.episode_id))
            {
                if message_i64_field(&state, "entered_at", "seconds").map(home::day)
                    == message_i64_field(&episode, "daily_updated_at", "seconds").map(home::day)
                {
                    let count = i32_field(&episode, "daily_clear_count").unwrap_or(0);
                    episode.set_field_by_name(
                        "daily_clear_count",
                        Value::I32(count.saturating_sub(1).max(0)),
                    );
                    put(resources, "episode_states", "episode_id", episode.clone());
                    put(changed, "episode_states", "episode_id", episode);
                }
            }
            state.clear_field_by_name("entered_at");
            state.clear_field_by_name("used_character_ids");
            state.clear_field_by_name("enemy_states");
            put(resources, "solo_raid_states", "quest_id", state.clone());
            put(changed, "solo_raid_states", "quest_id", state);
        }
        _ => return Err(StateError::InvalidRequest),
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn finish_total(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    activity: &activities::ActivityRules,
    home: &home::HomeRules,
    quest: &TutorialQuest,
    battle: &DynamicMessage,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    now: i64,
) -> Result<Vec<Value>, StateError> {
    let Some(panel_id) = quest.total_battle_panel_id else {
        return Ok(Vec::new());
    };
    let total_id = quest.total_battle_id.ok_or(StateError::InvalidRequest)?;
    let spec = row(activity, "total_battle", total_id)?;
    let previous_cleared: BTreeSet<_> = message_list(resources, "total_battle_panel_states")
        .iter()
        .filter(|p| {
            i32_field(p, "total_battle_id") == Some(total_id)
                && !i32_list(p, "character_ids").is_empty()
        })
        .filter_map(|p| i32_field(p, "total_battle_panel_id"))
        .filter_map(|id| row(activity, "total_battle_panel", id).ok())
        .map(|p| number(p, "panel_index"))
        .collect();
    let mut panel = empty_message(proto, "blend.model.TotalBattlePanelState")?;
    panel.set_field_by_name("total_battle_panel_id", Value::I32(panel_id));
    panel.set_field_by_name("total_battle_id", Value::I32(total_id));
    let ids = message_list(battle, "members")
        .iter()
        .filter_map(|m| message_i32_field(m, "ally", "character_id"))
        .filter(|id| *id > 0)
        .map(Value::I32)
        .collect();
    panel.set_field_by_name("character_ids", Value::List(ids));
    put(
        resources,
        "total_battle_panel_states",
        "total_battle_panel_id",
        panel.clone(),
    );
    put(
        changed,
        "total_battle_panel_states",
        "total_battle_panel_id",
        panel,
    );
    let cleared: BTreeSet<_> = message_list(resources, "total_battle_panel_states")
        .iter()
        .filter(|p| {
            i32_field(p, "total_battle_id") == Some(total_id)
                && !i32_list(p, "character_ids").is_empty()
        })
        .filter_map(|p| i32_field(p, "total_battle_panel_id"))
        .filter_map(|id| row(activity, "total_battle_panel", id).ok())
        .map(|p| number(p, "panel_index"))
        .collect();
    let lines = rows(activity, "total_battle_line")
        .iter()
        .filter(|l| number(l, "column_num") == number(spec, "panel_column_num"))
        .filter(|l| {
            values(l, "panel_index_pair")
                .iter()
                .all(|i| i.as_i64().is_some_and(|i| cleared.contains(&(i as i32))))
        })
        .count() as i32;
    let center_index = {
        let columns = number(spec, "panel_column_num");
        columns.saturating_mul(columns) / 2
    };
    let newly_cleared_center =
        !previous_cleared.contains(&center_index) && cleared.contains(&center_index);
    let mut state = message_list(resources, "total_battle_states")
        .into_iter()
        .find(|s| i32_field(s, "total_battle_id") == Some(total_id))
        .unwrap_or(empty_message(proto, "blend.model.TotalBattleState")?);
    let old = i32_field(&state, "best_achieved_line_count").unwrap_or(0);
    let mut rewards = Vec::new();
    for r in values(spec, "line_achieve_rewards")
        .iter()
        .filter(|r| old < number(r, "line_num") && number(r, "line_num") <= lines)
    {
        rewards.extend(grant_set(
            proto,
            rules,
            home,
            resources,
            changed,
            number(r, "reward_set_id"),
            now,
        )?);
    }
    let max_lines = rows(activity, "total_battle_line")
        .iter()
        .filter(|l| number(l, "column_num") == number(spec, "panel_column_num"))
        .count() as i32;
    if old < max_lines && lines == max_lines {
        rewards.extend(grant_set(
            proto,
            rules,
            home,
            resources,
            changed,
            number(spec, "complete_reward_set_id"),
            now,
        )?);
    }
    state.set_field_by_name("total_battle_id", Value::I32(total_id));
    state.set_field_by_name("best_achieved_line_count", Value::I32(old.max(lines)));
    put(
        resources,
        "total_battle_states",
        "total_battle_id",
        state.clone(),
    );
    put(changed, "total_battle_states", "total_battle_id", state);
    if newly_cleared_center {
        home::advance_missions(
            proto,
            home,
            resources,
            changed,
            now,
            Some((&format!("total_battle_center:{total_id}"), 1)),
        )?;
    }
    let new_lines = lines.saturating_sub(old);
    if new_lines > 0 {
        home::advance_missions(
            proto,
            home,
            resources,
            changed,
            now,
            Some((&format!("total_battle_line:{total_id}"), new_lines)),
        )?;
    }
    Ok(rewards)
}
