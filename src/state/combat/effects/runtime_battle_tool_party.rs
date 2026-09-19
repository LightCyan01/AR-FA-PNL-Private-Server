use super::registry::registry;
use super::runtime::Runtime;
use super::runtime_scaling::party_tag_count;
use crate::state::combat::prelude::*;

impl Runtime {
    pub(crate) fn battle_tool_party_bonus(
        &self,
        rules: &TutorialRules,
        members: &[DynamicMessage],
        kind: &str,
    ) -> Result<i32, StateError> {
        let families = &registry()?.battle_tool_party_rules;
        let mut total = 0i64;
        for passive in &self.passives {
            let Some(family) = families.iter().find(|family| {
                family.kind == kind
                    && family.owner_id == passive.rule.owner_id
                    && family.anchor_effect_id == passive.rule.id
            }) else {
                continue;
            };
            let Some(source) = members
                .iter()
                .find(|member| member_id(member).ok() == Some(passive.source))
            else {
                continue;
            };
            let count =
                party_tag_count(rules, members, source, family.tag_id)?.min(family.count_cap);
            let unscaled = i64::from(family.anchor_value)
                .saturating_add(i64::from(family.per_member) * i64::from(count))
                .min(i64::from(family.maximum));
            total = total
                .checked_add(i64::from(passive.value) * unscaled / i64::from(family.anchor_value))
                .ok_or(StateError::InvalidRequest)?;
        }
        i32::try_from(total).map_err(|_| StateError::InvalidRequest)
    }
}
