use super::registry::{registry, Rule};
use super::runtime::Runtime;
use crate::state::combat::prelude::*;

impl Runtime {
    pub(super) fn apply_potency(
        &mut self,
        target: i32,
        rule: &Rule,
        value: i32,
    ) -> Result<i32, StateError> {
        let data = registry()?;
        let operation = if data.negative_state_ids.contains(&rule.state_id) {
            "negative_potency"
        } else if data.positive_state_ids.contains(&rule.state_id) {
            "positive_potency"
        } else {
            return Ok(value);
        };
        let rate = self
            .instances
            .iter()
            .filter(|instance| instance.target == target && instance.rule.operation == operation)
            .fold(0i64, |total, instance| {
                total.saturating_add(i64::from(instance.value))
            });
        for instance in self.instances.iter_mut().filter(|instance| {
            instance.target == target
                && instance.rule.operation == operation
                && instance.remaining > 0
        }) {
            instance.remaining -= 1;
        }
        self.instances.retain(|instance| instance.remaining != 0);
        i32::try_from(
            i64::from(value).saturating_mul(10_000i64.saturating_add(rate).max(0)) / 10_000,
        )
        .map_err(|_| StateError::InvalidRequest)
    }
}
