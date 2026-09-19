use super::registry::Rule;
use super::runtime_results::effect_result;
use crate::state::combat::prelude::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_skill_form(
    proto: &ProtoRegistry,
    rules: &TutorialRules,
    members: &mut [DynamicMessage],
    source_id: i32,
    skill_id: i32,
    effect: &TutorialSkillEffect,
    is_skill: bool,
    rule: &Rule,
) -> Result<DynamicMessage, StateError> {
    if let Some(destination) = rule.fixed {
        rule_skill(rules, destination)?;
        let member = members
            .iter_mut()
            .find(|member| member_id(member).ok() == Some(source_id))
            .ok_or(StateError::InvalidRequest)?;
        let mut ally = member_status(member, "ally")?;
        let mut skills = message_list(&ally, "skills");
        if let Some(selected) = skills
            .iter_mut()
            .find(|skill| i32_field(skill, "skill_id") == Some(skill_id))
        {
            selected.set_field_by_name("skill_id", Value::I32(destination));
            ally.set_field_by_name(
                "skills",
                Value::List(skills.into_iter().map(Value::Message).collect()),
            );
            member.set_field_by_name("ally", Value::Message(ally));
        } else if !skills
            .iter()
            .any(|skill| i32_field(skill, "skill_id") == Some(destination))
        {
            return Err(StateError::InvalidRequest);
        }
    }
    effect_result(
        proto,
        effect.id,
        source_id,
        source_id,
        is_skill,
        rule,
        effect.value,
    )
}
