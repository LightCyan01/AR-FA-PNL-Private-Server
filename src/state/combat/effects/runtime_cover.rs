use super::runtime::Runtime;
use crate::state::combat::prelude::*;

pub(crate) const COVER_STATE_ID: i32 = 910038;

#[derive(Clone, Copy)]
pub(crate) struct Protection {
    pub(crate) protector_id: i32,
    pub(crate) state_change_id: i32,
}

impl Runtime {
    pub(crate) fn redirect_targets(
        &self,
        state: &DynamicMessage,
        actor_type: i32,
        skill: &TutorialSkill,
        targets: &[i32],
    ) -> Result<(Vec<i32>, Option<Protection>), StateError> {
        let all = skill.skill_target_type == Some(5);
        if skill.skill_effect_type != 1
            || (!all && skill.skill_target_type != Some(3))
            || targets.is_empty()
        {
            return Ok((targets.to_vec(), None));
        }

        let members = message_list(state, "members");
        let protector = members.iter().find_map(|member| {
            let id = member_id(member).ok()?;
            (bool_field(member, "is_alive")
                && member_type(member).ok() != Some(actor_type)
                && (all || !targets.contains(&id))
                && self.instances.iter().any(|instance| {
                    instance.target == id
                        && instance.remaining != 0
                        && instance.rule.operation == "cover"
                        && (!all || instance.rule.cover_all)
                }))
            .then_some(id)
        });
        let Some(protector_id) = protector else {
            return Ok((targets.to_vec(), None));
        };

        Ok((
            vec![protector_id; targets.len()],
            Some(Protection {
                protector_id,
                state_change_id: COVER_STATE_ID,
            }),
        ))
    }
}
