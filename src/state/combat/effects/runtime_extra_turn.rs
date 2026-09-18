use super::registry::Rule;
use super::runtime::{PendingExtraTurn, Runtime};
use crate::state::combat::prelude::*;

impl Runtime {
    pub(super) fn queue_extra_turn(
        &mut self,
        source_id: i32,
        owner_skill_id: i32,
        rule: &Rule,
    ) -> Result<bool, StateError> {
        let uses = self
            .extra_turn_uses
            .entry(source_id)
            .or_default()
            .entry(owner_skill_id)
            .or_default();
        if rule.trigger_limit > 0 && *uses >= rule.trigger_limit {
            return Ok(false);
        }
        *uses = uses.checked_add(1).ok_or(StateError::InvalidRequest)?;
        self.pending_extra_turns.push(PendingExtraTurn {
            source_id,
            skill_id: rule.fixed.ok_or(StateError::InvalidRequest)?,
            slots: rule.duration,
        });
        Ok(true)
    }

    pub(crate) fn apply_pending_extra_turns(
        &mut self,
        proto: &ProtoRegistry,
        state: &mut DynamicMessage,
    ) -> Result<Vec<DynamicMessage>, StateError> {
        let pending = std::mem::take(&mut self.pending_extra_turns);
        let members = message_list(state, "members");
        let mut units = message_list(state, "timeline_units");
        let mut movements = Vec::new();
        for turn in pending {
            let (number, movement) = insert_extra_timeline_turn(
                proto,
                &mut units,
                &members,
                turn.source_id,
                turn.slots,
            )?;
            self.extra_skills
                .entry(turn.source_id)
                .or_default()
                .insert(number, turn.skill_id);
            movements.push(movement);
        }
        state.set_field_by_name(
            "timeline_units",
            Value::List(units.into_iter().map(Value::Message).collect()),
        );
        Ok(movements)
    }

    pub(crate) fn current_extra_skill(
        &self,
        state: &DynamicMessage,
    ) -> Result<Option<i32>, StateError> {
        let Some((actor_id, number)) = current_extra_timeline_turn(state)? else {
            return Ok(None);
        };
        self.extra_skills
            .get(&actor_id)
            .and_then(|skills| skills.get(&number))
            .copied()
            .map(Some)
            .ok_or(StateError::InvalidRequest)
    }

    pub(crate) fn take_current_extra_skill(
        &mut self,
        state: &DynamicMessage,
    ) -> Result<Option<i32>, StateError> {
        let Some((actor_id, number)) = current_extra_timeline_turn(state)? else {
            return Ok(None);
        };
        let skill = self
            .extra_skills
            .get_mut(&actor_id)
            .and_then(|skills| skills.remove(&number))
            .ok_or(StateError::InvalidRequest)?;
        if self
            .extra_skills
            .get(&actor_id)
            .is_some_and(|skills| skills.is_empty())
        {
            self.extra_skills.remove(&actor_id);
        }
        Ok(Some(skill))
    }

    pub(crate) fn discard_extra_turns(&mut self, actor_id: i32) {
        self.extra_skills.remove(&actor_id);
        self.extra_turn_uses.remove(&actor_id);
    }
}
