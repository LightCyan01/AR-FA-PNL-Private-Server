use super::runtime::Runtime;
use crate::state::combat::prelude::*;

impl Runtime {
    pub(crate) fn reflect_magic_damage(
        &mut self,
        proto: &ProtoRegistry,
        state: &mut DynamicMessage,
        actor_id: i32,
        skill: &TutorialSkill,
        results: &[DynamicMessage],
    ) -> Result<(Vec<DynamicMessage>, Vec<DynamicMessage>), StateError> {
        if skill.skill_effect_type != 1 {
            return Ok((Vec::new(), Vec::new()));
        }
        let mut members = message_list(state, "members");
        let actor_index = members
            .iter()
            .position(|member| member_id(member).ok() == Some(actor_id))
            .ok_or(StateError::InvalidRequest)?;
        if !bool_field(&members[actor_index], "is_alive") {
            return Ok((Vec::new(), Vec::new()));
        }

        let mut reflected = 0i128;
        let mut matched = false;
        for result in results
            .iter()
            .filter(|result| !bool_field(result, "is_miss") && !bool_field(result, "is_invalid"))
        {
            let target_id = i32_field(result, "target_id").ok_or(StateError::InvalidRequest)?;
            if target_id == actor_id {
                continue;
            }
            let Some(target) = members
                .iter()
                .find(|member| member_id(member).ok() == Some(target_id))
            else {
                return Err(StateError::InvalidRequest);
            };
            if !bool_field(target, "is_alive") {
                continue;
            }
            let attribute = preferred_attack_attribute(target, skill)?;
            if !(5..=8).contains(&attribute) {
                continue;
            }
            let damage = message_i64_field(result, "hp_damage", "value")
                .unwrap_or_default()
                .max(0);
            if damage == 0 {
                continue;
            }
            for instance in self.instances.iter().filter(|instance| {
                instance.target == target_id
                    && instance.remaining != 0
                    && instance.rule.operation == "reflection"
                    && (instance.rule.attack_attributes.is_empty()
                        || instance.rule.attack_attributes.contains(&attribute))
            }) {
                reflected = reflected.saturating_add(
                    i128::from(damage).saturating_mul(i128::from(instance.value.max(0))) / 10_000,
                );
                matched = true;
            }
        }
        if !matched {
            return Ok((Vec::new(), Vec::new()));
        }

        let reflected = i64::try_from(reflected).unwrap_or(i64::MAX);
        let hp = i32_field(&members[actor_index], "hp")
            .ok_or(StateError::InvalidRequest)?
            .max(0);
        let next_hp = hp.saturating_sub(i32::try_from(reflected).unwrap_or(i32::MAX));
        let killed = next_hp == 0;
        members[actor_index].set_field_by_name("hp", Value::I32(next_hp));
        members[actor_index].set_field_by_name("is_alive", Value::Bool(!killed));
        state.set_field_by_name(
            "members",
            Value::List(members.iter().cloned().map(Value::Message).collect()),
        );

        let mut movements = Vec::new();
        if killed {
            let mut units = message_list(state, "timeline_units");
            if units
                .iter()
                .any(|unit| i32_field(unit, "member_id") == Some(actor_id))
            {
                movements = remove_timeline_members(proto, &mut units, &members, &[actor_id])?;
                state.set_field_by_name(
                    "timeline_units",
                    Value::List(units.into_iter().map(Value::Message).collect()),
                );
            }
        }
        if reflected == 0 {
            return Ok((Vec::new(), movements));
        }
        Ok((
            vec![build_skill_result(
                proto, actor_id, reflected, 0, 0, 0, true, false, killed, false, false, false,
                false,
            )?],
            movements,
        ))
    }
}
