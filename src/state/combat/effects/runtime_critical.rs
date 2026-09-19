use super::registry::rule_for;
use super::runtime::Runtime;
use crate::state::combat::prelude::*;
use std::collections::BTreeSet;

impl Runtime {
    pub(super) fn apply_critical_skill_effects(
        &mut self,
        proto: &ProtoRegistry,
        rules: &TutorialRules,
        state: &mut DynamicMessage,
        source_id: i32,
        skill: &TutorialSkill,
        results: &[DynamicMessage],
    ) -> Result<Vec<DynamicMessage>, StateError> {
        let targets = results
            .iter()
            .filter(|result| {
                bool_field(result, "is_critical")
                    && !bool_field(result, "is_miss")
                    && !bool_field(result, "is_invalid")
            })
            .filter_map(|result| i32_field(result, "target_id"))
            .collect::<BTreeSet<_>>();
        if targets.is_empty() {
            return Ok(Vec::new());
        }
        let mut effects = Vec::new();
        for effect in &skill.effects {
            if rule_for(effect.id, "active", "skill", skill.id)?
                .is_some_and(|rule| rule.critical_only && rule.phase == "after")
            {
                effects.push(effect.clone());
            }
        }
        if effects.is_empty() {
            return Ok(Vec::new());
        }
        self.apply_inner_with_rules(
            proto,
            Some(rules),
            state,
            source_id,
            skill.id,
            &effects,
            &targets.into_iter().collect::<Vec<_>>(),
            true,
            "after",
            None,
            skill.state_change_application_rate,
            None,
            true,
        )
    }
}
