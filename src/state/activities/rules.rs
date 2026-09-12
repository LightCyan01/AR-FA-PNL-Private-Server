use super::prelude::*;

// Account activities use the same resource transaction as Home and battle rewards.
use home::put;
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use std::collections::BTreeSet;

#[derive(Clone, Deserialize)]
pub(crate) struct ActivityRules {
    #[serde(default)]
    pub(crate) present_quality: Json,
    #[serde(default)]
    pub(crate) combat_power: Json,
    pub source_sha256: String,
    pub(crate) constants: Json,
    pub(crate) tables: BTreeMap<String, Vec<Json>>,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct ActivityState {
    pub(crate) explorations: BTreeMap<i32, ExplorationRun>,
    pub(crate) housing_counts: BTreeMap<i32, i32>,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct ExplorationRun {
    pub(crate) quest_id: i32,
    pub(crate) party_number: i32,
    pub(crate) steps: Vec<ExplorationStep>,
    pub(crate) next: usize,
    pub(crate) gathered: BTreeSet<i32>,
    pub(crate) rewards: Vec<(i32, i32, i32)>,
    pub(crate) character_exp: i32,
    pub(crate) total_turn: i32,
    pub(crate) hps: Vec<i32>,
    pub(crate) tool_counts: Vec<i32>,
    pub(crate) party_gauge: i32,
    pub(crate) pending: bool,
    pub(crate) character_ids: Vec<i32>,
    pub(crate) earned_exp: BTreeMap<i32, i32>,
    pub(crate) control_character_id: Option<i32>,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct ExplorationStep {
    pub(crate) area_id: i32,
    pub(crate) kind: i32,
    pub(crate) index: usize,
    pub(crate) points: i32,
    pub(crate) battle_id: Option<i32>,
    pub(crate) exp: i32,
    pub(crate) cole: i32,
}

pub(crate) fn load_rules() -> Result<ActivityRules, StateError> {
    serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../data/activity_rules.json"
    )))
    .map_err(|e| StateError::MasterData(e.to_string()))
}

pub(crate) fn rows<'a>(rules: &'a ActivityRules, table: &str) -> &'a [Json] {
    rules.tables.get(table).map(Vec::as_slice).unwrap_or(&[])
}
pub(crate) fn row<'a>(
    rules: &'a ActivityRules,
    table: &str,
    id: i32,
) -> Result<&'a Json, StateError> {
    rows(rules, table)
        .iter()
        .find(|r| number(r, "id") == id)
        .ok_or(StateError::InvalidRequest)
}
pub(crate) fn rewards(value: &Json, key: &str) -> Result<Vec<TutorialReward>, StateError> {
    serde_json::from_value(Json::Array(values(value, key).to_vec()))
        .map_err(|e| StateError::MasterData(e.to_string()))
}
pub(crate) fn eligible(
    value: &Json,
    resources: &DynamicMessage,
    now: i64,
) -> Result<(), StateError> {
    if !home::in_period(value["start_at"].as_i64(), value["end_at"].as_i64(), now) {
        return Err(StateError::OutOfSchedule);
    }
    if value["key_quest_id"]
        .as_i64()
        .is_some_and(|id| quest_clear_count(resources, id as i32) == 0)
        || values(value, "key_tasks").iter().any(|task| {
            total_task_count(resources, number(task, "condition_id")) < number(task, "count")
        })
    {
        return Err(StateError::InvalidRequest);
    }
    Ok(())
}
pub(crate) fn save(
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    field: &str,
    key: &str,
    value: DynamicMessage,
) {
    put(resources, field, key, value.clone());
    put(changed, field, key, value);
}
pub(crate) fn merge(changed: &mut DynamicMessage, other: DynamicMessage) {
    for (field, value) in other.fields() {
        if field.is_list() {
            // A batch may clear/grant several rows of the same resource type.
            // Replacing the list would hide earlier committed rows from the client.
            for row in message_list(&other, field.name()) {
                let descriptor = row.descriptor();
                let key = descriptor.fields().next().expect("resource key");
                let keys = match field.name() {
                    "party_members" => vec!["party_type", "number", "position"],
                    "parties" => vec!["party_type", "number"],
                    "exploration_progresses" => vec!["is_story"],
                    _ => vec![key.name()],
                };
                let mut rows = message_list(changed, field.name());
                rows.retain(|old| {
                    !keys
                        .iter()
                        .all(|name| old.get_field_by_name(name) == row.get_field_by_name(name))
                });
                rows.push(row);
                changed.set_field(
                    &field,
                    Value::List(rows.into_iter().map(Value::Message).collect()),
                );
            }
        } else {
            changed.set_field(&field, value.clone());
        }
    }
}
