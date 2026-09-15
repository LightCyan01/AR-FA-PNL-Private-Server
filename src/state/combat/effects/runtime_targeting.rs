use super::registry::Rule;
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
