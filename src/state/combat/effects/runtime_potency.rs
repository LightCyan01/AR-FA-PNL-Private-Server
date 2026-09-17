use super::registry::{registry, Rule};
use super::runtime::Runtime;
use super::runtime_match::{condition, selected_for_source_character};
use crate::state::combat::prelude::*;

impl Runtime {
    pub(super) fn apply_potency(
        &mut self,
        members: &[DynamicMessage],
        source: &DynamicMessage,
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
        let source_id = member_id(source)?;
        let target_member = members
            .iter()
            .find(|member| member_id(member).ok() == Some(target))
            .ok_or(StateError::InvalidRequest)?;
        let given_operation = if operation == "negative_potency" {
            "given_negative_potency"
        } else {
            "given_positive_potency"
        };
        let instance_applies = |owner, candidate: &str| {
            (owner == target && candidate == operation)
                || (owner == source_id && candidate == given_operation)
        };
        let passive_applies = |passive: &super::runtime::Passive| {
            let (recipient, candidate) = if passive.rule.operation == operation {
                (target_member, operation)
            } else {
                (source, given_operation)
            };
            if passive.rule.operation != candidate {
                return false;
            }
            members
                .iter()
                .find(|member| member_id(member).ok() == Some(passive.source))
                .is_some_and(|owner| {
                    bool_field(owner, "is_alive")
                        && condition(&passive.rule, owner)
                        && selected_for_source_character(
                            &passive.rule,
                            owner,
                            passive.source_character_id,
                            recipient,
                            &[],
                        )
                })
        };
        let rate = self
            .instances
            .iter()
            .filter(|instance| instance_applies(instance.target, &instance.rule.operation))
            .fold(0i64, |total, instance| {
                total.saturating_add(i64::from(instance.value))
            })
            .saturating_add(
                self.passives
                    .iter()
                    .filter(|passive| passive_applies(passive))
                    .fold(0i64, |total, passive| {
                        total.saturating_add(i64::from(passive.value))
                    }),
            );
        for instance in self.instances.iter_mut().filter(|instance| {
            instance_applies(instance.target, &instance.rule.operation) && instance.remaining > 0
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
