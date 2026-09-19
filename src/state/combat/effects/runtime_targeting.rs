use super::registry::Rule;
use super::runtime::Runtime;
use super::runtime_match::condition;
use crate::state::combat::prelude::*;
use std::cmp::Reverse;

pub(super) fn resolved_targets(
    rule: &Rule,
    source: &DynamicMessage,
    members: &[DynamicMessage],
    snapshot: Option<&DynamicMessage>,
    requested: &[i32],
) -> Result<Vec<i32>, StateError> {
    let field = match rule.target.as_str() {
        "highest_attack_ally" => "attack",
        "highest_magic_ally" => "magic",
        _ => return Ok(requested.to_vec()),
    };
    let source_type = member_type(source)?;
    let snapshot_members = snapshot
        .map(|state| message_list(state, "members"))
        .unwrap_or_else(|| members.to_vec());
    snapshot_members
        .into_iter()
        .filter(|member| {
            bool_field(member, "is_alive") && member_type(member).ok() == Some(source_type)
        })
        .max_by_key(|member| {
            (
                member_status(member, "current_status")
                    .ok()
                    .and_then(|status| i32_field(&status, field))
                    .unwrap_or_default(),
                Reverse(member_id(member).unwrap_or(i32::MAX)),
            )
        })
        .map(|member| member_id(&member).map(|id| vec![id]))
        .transpose()
        .map(Option::unwrap_or_default)
}

impl Runtime {
    pub(crate) fn target_by_rate(
        &self,
        state: &DynamicMessage,
        roll: u32,
    ) -> Result<i32, StateError> {
        let candidates: Vec<_> = message_list(state, "members")
            .into_iter()
            .filter(|member| member_type(member).ok() == Some(0) && bool_field(member, "is_alive"))
            .collect();
        let weights: Vec<i64> = candidates
            .iter()
            .map(|member| {
                let id = member_id(member).unwrap_or_default();
                10_000
                    + self
                        .passives
                        .iter()
                        .filter(|passive| {
                            passive.source == id
                                && passive.rule.operation == "target_rate"
                                && condition(&passive.rule, member)
                        })
                        .map(|passive| i64::from(passive.value))
                        .sum::<i64>()
            })
            .map(|weight| weight.max(0))
            .collect();
        let total = weights.iter().sum::<i64>();
        if total <= 0 {
            return candidates
                .first()
                .map(member_id)
                .transpose()?
                .ok_or(StateError::InvalidRequest);
        }
        let mut offset = i64::from(roll) % total;
        for (member, weight) in candidates.iter().zip(weights) {
            if offset < weight {
                return member_id(member);
            }
            offset -= weight;
        }
        Err(StateError::InvalidRequest)
    }
}
