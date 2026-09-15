use super::registry::{registry, Rule};
use super::runtime::Runtime;
use crate::state::combat::prelude::*;

impl Runtime {
    pub(super) fn apply_negative_potency(
        &mut self,
        target: i32,
        rule: &Rule,
        value: i32,
    ) -> Result<i32, StateError> {
        if !registry()?.negative_state_ids.contains(&rule.state_id) {
            return Ok(value);
        }
        let rate = self
            .instances
            .iter()
            .filter(|instance| {
                instance.target == target && instance.rule.operation == "negative_potency"
            })
            .fold(0i64, |total, instance| total.saturating_add(i64::from(instance.value)));
        for instance in self.instances.iter_mut().filter(|instance| {
            instance.target == target
                && instance.rule.operation == "negative_potency"
                && instance.remaining > 0
        }) {
            instance.remaining -= 1;
        }
        self.instances.retain(|instance| instance.remaining != 0);
        i32::try_from(
            i64::from(value).saturating_mul(10_000i64.saturating_add(rate)) / 10_000,
        )
        .map_err(|_| StateError::InvalidRequest)
    }
}
