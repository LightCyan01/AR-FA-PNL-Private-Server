use super::registry::Rule;
use super::runtime::Runtime;
use super::runtime_levels::apply_level_state;
use super::runtime_match::selected;
use crate::state::combat::prelude::*;

impl Runtime {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_random_level_state(
        &mut self,
        proto: &ProtoRegistry,
        members: &[DynamicMessage],
        source: &DynamicMessage,
        source_id: i32,
        effect: &TutorialSkillEffect,
        targets: &[i32],
        is_skill: bool,
        rule: &Rule,
        secret: &[u8],
        transaction: &str,
        action_number: i32,
    ) -> Result<Vec<DynamicMessage>, StateError> {
        let minimum = rule.scale_input_min;
        let maximum = rule.scale_input_max;
        if rule.scale_by != "random" || minimum <= 0 || maximum < minimum {
            return Err(StateError::InvalidRequest);
        }
        let width = u32::try_from(maximum.saturating_sub(minimum).saturating_add(1))
            .map_err(|_| StateError::InvalidRequest)?;
        if width == 0 {
            return Err(StateError::InvalidRequest);
        }
        let mut roll_kind = b"random-level-state".to_vec();
        roll_kind.extend_from_slice(&effect.id.to_le_bytes());
        let mut level_rule = rule.clone();
        level_rule.operation = "level_state".into();
        let mut results = Vec::new();
        for target in members.iter().filter(|target| {
            bool_field(target, "is_alive") && selected(rule, source, target, targets)
        }) {
            let target_id = member_id(target)?;
            let increment = minimum.saturating_add(
                i32::try_from(
                    deterministic_roll(
                        secret,
                        transaction,
                        action_number,
                        &roll_kind,
                        target_id,
                        0,
                    ) % width,
                )
                .map_err(|_| StateError::InvalidRequest)?,
            );
            results.extend(apply_level_state(
                self,
                proto,
                source,
                std::slice::from_ref(target),
                source_id,
                effect,
                targets,
                is_skill,
                &level_rule,
                increment,
            )?);
        }
        Ok(results)
    }
}
