use super::registry::registry;
use super::runtime::Runtime;
use super::runtime_match::contextual_recipient;
use crate::state::combat::prelude::*;

pub(super) fn scale_panel_value(value: i32, rate: i128) -> i32 {
    i32::try_from(i128::from(value) * rate / 10_000).unwrap_or_else(|_| {
        if value.is_negative() {
            i32::MIN
        } else {
            i32::MAX
        }
    })
}

impl Runtime {
    pub(super) fn panel_effect_rate(&self, state: &DynamicMessage) -> Result<i128, StateError> {
        if !registry()?
            .enhancement_panel_ids
            .contains(&effective_battle_panel_id(state))
        {
            return Ok(10_000);
        }
        let actor = current_actor(state)?;
        let actor_id = member_id(&actor)?;
        let instance = self
            .instances
            .iter()
            .filter(|instance| {
                instance.target == actor_id && instance.rule.operation == "panel_potency"
            })
            .map(|instance| instance.value)
            .max()
            .unwrap_or_default();
        let passive = self
            .passives
            .iter()
            .filter(|passive| {
                passive.rule.operation == "panel_potency" && contextual_recipient(passive, &actor)
            })
            .map(|passive| passive.value)
            .max()
            .unwrap_or_default();
        Ok(i128::from(
            10_000i32.saturating_add(instance.max(passive)).max(0),
        ))
    }

    pub(crate) fn panel_multiplier(
        &self,
        state: &DynamicMessage,
    ) -> Result<(i128, i128), StateError> {
        let (numerator, denominator) = battle_panel_multiplier(state);
        let bonus = (numerator - denominator) * self.panel_effect_rate(state)? / 10_000;
        Ok((denominator + bonus, denominator))
    }

    pub(crate) fn panel_break_multiplier(
        &self,
        state: &DynamicMessage,
    ) -> Result<i128, StateError> {
        let base = battle_panel_break_multiplier(state);
        Ok(100 + (base - 100) * self.panel_effect_rate(state)? / 10_000)
    }

    pub(crate) fn consume_panel_potency(
        &mut self,
        state: &DynamicMessage,
        actor_id: i32,
    ) -> Result<(), StateError> {
        if !registry()?
            .enhancement_panel_ids
            .contains(&effective_battle_panel_id(state))
        {
            return Ok(());
        }
        for instance in self.instances.iter_mut().filter(|instance| {
            instance.target == actor_id && instance.rule.operation == "panel_potency"
        }) {
            if instance.remaining > 0 {
                instance.remaining -= 1;
            }
        }
        self.instances.retain(|instance| instance.remaining != 0);
        Ok(())
    }
}
