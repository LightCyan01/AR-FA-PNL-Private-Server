// Immutable indexes built once when the state rule catalogs are parsed.

use std::collections::BTreeMap;
#[cfg(test)]
use std::collections::BTreeSet;

/// Read-only lookup data shared by reducers. The vectors retain catalog order;
/// duplicate entries are removed while building the index.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct StateContext {
    missions_by_condition: BTreeMap<i32, Vec<i32>>,
    missions_by_event: BTreeMap<String, Vec<i32>>,
    conditions_by_event: BTreeMap<String, Vec<i32>>,
}

impl StateContext {
    /// Build the index from `(condition_id, mission_id, event_kind)` entries.
    /// A zero mission id represents a total-task-only entry.
    pub(crate) fn from_entries<I>(entries: I) -> Self
    where
        I: IntoIterator<Item = (i32, i32, String)>,
    {
        let mut context = Self::default();
        for (condition_id, mission_id, event_kind) in entries {
            if condition_id > 0 && mission_id > 0 {
                push_unique(
                    context
                        .missions_by_condition
                        .entry(condition_id)
                        .or_default(),
                    mission_id,
                );
            }
            if !event_kind.is_empty() {
                if condition_id > 0 {
                    push_unique(
                        context
                            .conditions_by_event
                            .entry(event_kind.clone())
                            .or_default(),
                        condition_id,
                    );
                }
                if mission_id > 0 {
                    push_unique(
                        context.missions_by_event.entry(event_kind).or_default(),
                        mission_id,
                    );
                }
            }
        }
        context
    }

    #[cfg(test)]
    pub(crate) fn missions_for_condition(&self, condition_id: i32) -> &[i32] {
        self.missions_by_condition
            .get(&condition_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub(crate) fn missions_for_event(&self, event_kind: &str) -> &[i32] {
        self.missions_by_event
            .get(event_kind)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub(crate) fn conditions_for_event(&self, event_kind: &str) -> &[i32] {
        self.conditions_by_event
            .get(event_kind)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    #[cfg(test)]
    pub(crate) fn indexed_condition_count(&self) -> usize {
        self.conditions_by_event
            .values()
            .flat_map(|ids| ids.iter().copied())
            .collect::<BTreeSet<_>>()
            .len()
    }
}

fn push_unique(values: &mut Vec<i32>, value: i32) {
    if !values.contains(&value) {
        values.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::StateContext;

    #[test]
    fn atelier_index_is_ordered_and_deduplicated() {
        let context = StateContext::from_entries([
            (10, 21, "item_received:7".into()),
            (10, 21, "item_received:7".into()),
            (11, 22, "item_received:7".into()),
            (12, 0, "quest_clear:9".into()),
        ]);
        assert_eq!(context.missions_for_event("item_received:7"), &[21, 22]);
        assert_eq!(context.conditions_for_event("item_received:7"), &[10, 11]);
        assert_eq!(context.missions_for_condition(10), &[21]);
        assert_eq!(context.indexed_condition_count(), 3);
    }
}
