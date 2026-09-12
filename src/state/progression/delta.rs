//! Deterministic aggregation of progression events.

use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProgressionDelta {
    pub(crate) condition_id: i32,
    pub(crate) amount: i32,
}

/// Combine repeated condition updates without changing the first-seen order.
pub(crate) fn aggregate_deltas<I>(events: I) -> Vec<ProgressionDelta>
where
    I: IntoIterator<Item = (i32, i32)>,
{
    let mut totals = BTreeMap::<i32, i32>::new();
    let mut order = Vec::new();
    for (condition_id, amount) in events {
        if !totals.contains_key(&condition_id) {
            order.push(condition_id);
        }
        let entry = totals.entry(condition_id).or_default();
        *entry = entry.saturating_add(amount);
    }
    order
        .into_iter()
        .filter_map(|condition_id| {
            totals.remove(&condition_id).map(|amount| ProgressionDelta {
                condition_id,
                amount,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::aggregate_deltas;

    #[test]
    fn progression_delta_combines_duplicates_in_first_seen_order() {
        assert_eq!(
            aggregate_deltas([(9, 2), (4, 3), (9, 5), (4, -1)]),
            vec![
                super::ProgressionDelta {
                    condition_id: 9,
                    amount: 7,
                },
                super::ProgressionDelta {
                    condition_id: 4,
                    amount: 2,
                },
            ]
        );
    }
}
