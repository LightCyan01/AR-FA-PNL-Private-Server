use crate::state::service::prelude::*;

pub(crate) struct ScoreOutcome {
    pub(crate) detail: DynamicMessage,
    pub(crate) result: DynamicMessage,
    pub(crate) rank: i32,
}

pub(crate) fn apply_damage_contest_score(
    proto: &ProtoRegistry,
    quest: &TutorialQuest,
    progress: &home::BattleProgress,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
) -> Result<Option<DynamicMessage>, StateError> {
    // QuestEpisodeType::DamageContest in the client master-data enum.
    if quest.episode_type != 4 {
        return Ok(None);
    }
    let score = progress.damage.clamp(0, i64::from(i32::MAX));
    let mut quest_state = message_list(resources, "quest_states")
        .into_iter()
        .find(|row| i32_field(row, "quest_id") == Some(quest.id))
        .ok_or(StateError::InvalidRequest)?;
    let previous =
        i64::from(message_i32_field(&quest_state, "high_score_detail", "total_score").unwrap_or(0));
    let best = previous.max(score);
    if score > previous {
        let mut detail = empty_message(proto, "blend.model.BattleScoreDetail")?;
        detail.set_field_by_name("total_score", Value::I32(score as i32));
        quest_state.set_field_by_name("high_score_detail", Value::Message(detail));
        upsert_quest_state(resources, quest_state.clone());
        upsert_quest_state(changed, quest_state);
    }
    let mut result = empty_message(proto, "blend.model.BattleDamageContestResult")?;
    result.set_field_by_name("score", Value::I64(score));
    result.set_field_by_name("best_score", Value::I64(best));
    Ok(Some(result))
}

fn rounded_score_envelope(quest: &TutorialQuest, ranks: &[Json]) -> Result<i32, StateError> {
    let ss = ranks
        .iter()
        .max_by_key(|rank| number(rank, "rank"))
        .map(|rank| number(rank, "score"))
        .filter(|score| *score > 0)
        .ok_or(StateError::InvalidRequest)?;
    let fraction = quest
        .difficulty
        .filter(|difficulty| (1..=3).contains(difficulty))
        .map(|difficulty| difficulty + 5)
        .unwrap_or_else(|| {
            let b = ranks
                .iter()
                .find(|rank| number(rank, "rank") == 2)
                .map(|rank| number(rank, "score"))
                .unwrap_or(ss);
            (6..=8)
                .min_by_key(|fraction| {
                    (i64::from(b) * i64::from(*fraction) - i64::from(ss) * i64::from(*fraction - 3))
                        .abs()
                })
                .unwrap_or(8)
        });
    let raw = (i64::from(ss) * 10 + i64::from(fraction / 2)) / i64::from(fraction);
    checked_i32(((raw + 500) / 1_000 * 1_000).max(1_000))
}

fn weighted_max(envelope: i32, weight: i32) -> Result<i32, StateError> {
    checked_i32(i64::from(envelope) * i64::from(weight) / 100)
}

fn descending_score(maximum: i32, value: i64, full_at: i64, zero_at: i64) -> i32 {
    if value <= full_at {
        maximum
    } else if value >= zero_at || zero_at <= full_at {
        0
    } else {
        (i64::from(maximum) * (zero_at - value) / (zero_at - full_at)) as i32
    }
}

fn damage_score(maximum: i32, damage: i64, enemy_hp: i64, difficulty: Option<i32>) -> i32 {
    if damage <= 0 || enemy_hp <= 0 {
        return 0;
    }
    let (numerator, denominator) = difficulty
        .filter(|value| (1..=3).contains(value))
        .map(|value| (i64::from(value + 1), 2i64))
        .unwrap_or((1, 4));
    let constant = (enemy_hp.saturating_mul(numerator) / denominator).max(1);
    ((i128::from(maximum) * i128::from(damage)) / i128::from(damage.saturating_add(constant)))
        as i32
}

pub(crate) fn apply_battle_score(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    quest: &TutorialQuest,
    state: &DynamicMessage,
    progress: &home::BattleProgress,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
) -> Result<Option<ScoreOutcome>, StateError> {
    let ranks = values(&quest.score_battle, "ranks");
    if ranks.is_empty() {
        return Ok(None);
    }
    let weights = [
        rules.constants.total_turn_weight,
        rules.constants.max_dealt_hp_damage_weight,
        rules.constants.received_hp_damage_weight,
        rules.constants.dead_allies_count_weight,
    ];
    if weights.iter().any(|weight| *weight < 0) || weights.iter().sum::<i32>() != 100 {
        return Err(StateError::TutorialRules("invalid score weights".into()));
    }

    let envelope = rounded_score_envelope(quest, ranks)?;
    let turn = i32_field(state, "total_turn").unwrap_or(1).max(1);
    let dead = message_list(state, "members")
        .iter()
        .filter(|member| member_type(member).ok() == Some(0) && !bool_field(member, "is_alive"))
        .count()
        .min(i32::MAX as usize) as i32;
    let turn_max = weighted_max(envelope, weights[0])?;
    let damage_max = weighted_max(envelope, weights[1])?;
    let received_max = weighted_max(envelope, weights[2])?;
    let dead_max = weighted_max(envelope, weights[3])?;

    // ponytail: the publisher keeps per-stage turn/received curves server-side;
    // replace these shared fallbacks if that tuning table is recovered.
    let turn_score = descending_score(turn_max, i64::from(turn), 20, 100);
    let max_dealt_hp_damage_score = damage_score(
        damage_max,
        progress.max_dealt_hp_damage,
        progress.initial_enemy_hp,
        quest.difficulty,
    );
    let received_full = progress.initial_ally_hp / 3;
    let received_score = descending_score(
        received_max,
        progress.received_hp_damage,
        received_full,
        progress.initial_ally_hp.max(received_full + 1),
    );
    let dead_score = descending_score(dead_max, i64::from(dead), 0, 2);
    let total = turn_score
        .checked_add(max_dealt_hp_damage_score)
        .and_then(|value| value.checked_add(received_score))
        .and_then(|value| value.checked_add(dead_score))
        .ok_or(StateError::InvalidRequest)?;
    let rank = ranks
        .iter()
        .filter(|row| number(row, "score") <= total)
        .map(|row| number(row, "rank"))
        .max()
        .or_else(|| ranks.iter().map(|row| number(row, "rank")).min())
        .ok_or(StateError::InvalidRequest)?;

    let mut detail = empty_message(proto, "blend.model.BattleScoreDetail")?;
    detail.set_field_by_name("total_turn", Value::I32(turn));
    detail.set_field_by_name("total_turn_score", Value::I32(turn_score));
    detail.set_field_by_name(
        "max_dealt_hp_damage",
        Value::I64(progress.max_dealt_hp_damage),
    );
    detail.set_field_by_name(
        "max_dealt_hp_damage_score",
        Value::I32(max_dealt_hp_damage_score),
    );
    detail.set_field_by_name(
        "received_hp_damage",
        Value::I64(progress.received_hp_damage),
    );
    detail.set_field_by_name("received_hp_damage_score", Value::I32(received_score));
    detail.set_field_by_name("dead_allies_count", Value::I32(dead));
    detail.set_field_by_name("dead_allies_count_score", Value::I32(dead_score));
    detail.set_field_by_name("total_score", Value::I32(total));

    let mut result = empty_message(proto, "blend.model.BattleScoreResult")?;
    result.set_field_by_name("score", Value::I32(total));
    result.set_field_by_name("rank", Value::I32(rank));

    let mut quest_state = message_list(resources, "quest_states")
        .into_iter()
        .find(|row| i32_field(row, "quest_id") == Some(quest.id))
        .ok_or(StateError::InvalidRequest)?;
    if rank > i32_field(&quest_state, "score_rank").unwrap_or(0) {
        quest_state.set_field_by_name("score_rank", Value::I32(rank));
    }
    if total > message_i32_field(&quest_state, "high_score_detail", "total_score").unwrap_or(0) {
        quest_state.set_field_by_name("high_score_detail", Value::Message(detail.clone()));
    }
    let previous_turn = i32_field(&quest_state, "min_total_turn").unwrap_or(0);
    if previous_turn == 0 || turn < previous_turn {
        quest_state.set_field_by_name("min_total_turn", Value::I32(turn));
    }
    upsert_quest_state(resources, quest_state.clone());
    upsert_quest_state(changed, quest_state);

    Ok(Some(ScoreOutcome {
        detail,
        result,
        rank,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn damage_contest_persists_best_and_reports_current_score() {
        let proto = ProtoRegistry::from_file(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../schemas/atelier-resleriana-2.16.0.protoset"
        )))
        .unwrap();
        let rules = load_gameplay_rules().unwrap();
        let quest = rules.quests.iter().find(|quest| quest.id == 701).unwrap();
        let mut resources = empty_message(&proto, "blend.model.Resources").unwrap();
        let mut quest_state = empty_message(&proto, "blend.model.QuestState").unwrap();
        quest_state.set_field_by_name("quest_id", Value::I32(quest.id));
        upsert_quest_state(&mut resources, quest_state);
        let mut changed = empty_message(&proto, "blend.model.Resources").unwrap();

        let first = apply_damage_contest_score(
            &proto,
            quest,
            &home::BattleProgress {
                damage: 12_345,
                ..Default::default()
            },
            &mut resources,
            &mut changed,
        )
        .unwrap()
        .unwrap();
        let i64_value = |message: &DynamicMessage, name| {
            message
                .get_field_by_name(name)
                .and_then(|value| value.as_i64())
        };
        assert_eq!(i64_value(&first, "score"), Some(12_345));
        assert_eq!(i64_value(&first, "best_score"), Some(12_345));

        let second = apply_damage_contest_score(
            &proto,
            quest,
            &home::BattleProgress {
                damage: 100,
                ..Default::default()
            },
            &mut resources,
            &mut changed,
        )
        .unwrap()
        .unwrap();
        assert_eq!(i64_value(&second, "score"), Some(100));
        assert_eq!(i64_value(&second, "best_score"), Some(12_345));
        let persisted = message_list(&resources, "quest_states").pop().unwrap();
        assert_eq!(
            message_i32_field(&persisted, "high_score_detail", "total_score"),
            Some(12_345)
        );
        assert_eq!(
            home::prelude::objective_count(
                &home::load_rules().unwrap(),
                &resources,
                &home::ResourceObjective::QuestHighScoreTotal {
                    quest_ids: vec![701, 702, 703],
                },
            ),
            12_345
        );
    }

    #[test]
    fn score_battle_persists_rank_detail_and_turn() {
        let proto = ProtoRegistry::from_file(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../schemas/atelier-resleriana-2.16.0.protoset"
        )))
        .unwrap();
        let rules = load_gameplay_rules().unwrap();
        let quest = rules
            .quests
            .iter()
            .find(|quest| quest.id == 204106019)
            .unwrap();
        let mut resources = empty_message(&proto, "blend.model.Resources").unwrap();
        let mut quest_state = empty_message(&proto, "blend.model.QuestState").unwrap();
        quest_state.set_field_by_name("quest_id", Value::I32(quest.id));
        quest_state.set_field_by_name("clear_count", Value::I32(1));
        upsert_quest_state(&mut resources, quest_state);
        let mut changed = empty_message(&proto, "blend.model.Resources").unwrap();
        let mut state = empty_message(&proto, "blend.model.BattleState").unwrap();
        state.set_field_by_name("total_turn", Value::I32(12));
        let progress = home::BattleProgress {
            max_dealt_hp_damage: 100_000,
            initial_enemy_hp: 100_000,
            initial_ally_hp: 10_000,
            ..Default::default()
        };

        let outcome = apply_battle_score(
            &proto,
            &rules,
            quest,
            &state,
            &progress,
            &mut resources,
            &mut changed,
        )
        .unwrap()
        .unwrap();
        let persisted = message_list(&resources, "quest_states")
            .into_iter()
            .find(|row| i32_field(row, "quest_id") == Some(quest.id))
            .unwrap();
        assert_eq!(i32_field(&persisted, "score_rank"), Some(outcome.rank));
        assert_eq!(i32_field(&persisted, "min_total_turn"), Some(12));
        assert_eq!(
            message_i32_field(&persisted, "high_score_detail", "total_score"),
            i32_field(&outcome.detail, "total_score")
        );
        assert_eq!(
            i32_field(&outcome.result, "score"),
            i32_field(&outcome.detail, "total_score")
        );
        let rewards =
            roll_ranked_quest_rewards(&load_reward_rules().unwrap(), quest.id, Some(outcome.rank))
                .unwrap();
        assert!(rewards
            .iter()
            .any(|reward| reward.resource_type == 3 && reward.quantity > 0));
        assert!(rewards
            .iter()
            .any(|reward| reward.resource_type == 5 && reward.quantity > 0));
    }
}
