use super::registry::registry;
use super::runtime::Runtime;
use super::runtime_match::{
    amount, context_matches, contextual_recipient, contextual_rule, target_condition,
};
use crate::state::combat::prelude::*;

pub(crate) fn timeline_slots(effect: &TutorialSkillEffect) -> Result<Option<usize>, StateError> {
    let Some(rule) = registry()?
        .rules
        .iter()
        .find(|rule| rule.id == effect.id && rule.operation == "timeline_shift")
    else {
        return Ok(None);
    };
    let value = amount(rule, effect.value)?;
    if value <= 0 || value % 100 != 0 {
        return Err(StateError::MasterData(format!(
            "invalid timeline shift for effect {}",
            effect.id
        )));
    }
    Ok(Some(
        usize::try_from(value / 100).map_err(|_| StateError::InvalidRequest)?,
    ))
}

pub(crate) fn instant_summary(
    skill: &TutorialSkill,
    target: &DynamicMessage,
    critical: bool,
    summary: i32,
) -> Result<i64, StateError> {
    let weak = target_resistance(target, preferred_attack_attribute(target, skill)?)? < 0;
    skill.effects.iter().try_fold(0i64, |total, effect| {
        let Some(rule) = registry()?.rules.iter().find(|rule| {
            rule.id == effect.id
                && rule.mode == "instant"
                && rule.operation == "summary"
                && rule.summary == summary
        }) else {
            return Ok(total);
        };
        if (rule.weak_only && !weak)
            || (rule.target_broken
                && !member_status(target, "enemy")
                    .ok()
                    .is_some_and(|enemy| bool_field(&enemy, "is_broken")))
            || !target_condition(rule, target)
            || !context_matches(rule, 0, skill, critical)
        {
            return Ok(total);
        }
        Ok(total.saturating_add(i64::from(amount(rule, effect.value)?)))
    })
}

impl Runtime {
    pub(crate) fn contextual_summary(
        &self,
        target: &DynamicMessage,
        skill: &TutorialSkill,
        critical: bool,
        summary: i32,
    ) -> i64 {
        let passive = self
            .passives
            .iter()
            .filter(|passive| {
                passive.rule.operation == "summary"
                    && passive.rule.summary == summary
                    && passive.rule.trigger.is_none()
                    && contextual_rule(&passive.rule)
                    && contextual_recipient(passive, target)
                    && context_matches(&passive.rule, passive.source_character_id, skill, critical)
            })
            .fold(0i64, |total, passive| {
                total.saturating_add(i64::from(passive.value))
            });
        let target_id = i32_field(target, "member_id").unwrap_or_default();
        self.instances
            .iter()
            .filter(|instance| {
                instance.target == target_id
                    && instance.rule.operation == "summary"
                    && instance.rule.summary == summary
                    && contextual_rule(&instance.rule)
                    && instance.rule.trigger.is_none()
                    && context_matches(
                        &instance.rule,
                        instance.source_character_id,
                        skill,
                        critical,
                    )
            })
            .fold(passive, |total, instance| {
                total.saturating_add(i64::from(instance.value))
            })
    }
}
