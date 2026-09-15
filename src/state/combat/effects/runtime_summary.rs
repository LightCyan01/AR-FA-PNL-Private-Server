use super::registry::rule_for;
use super::runtime::Runtime;
use super::runtime_lamp::lamp_condition_matches;
use super::runtime_match::{
    amount, condition, context_matches, contextual_recipient, contextual_rule, target_condition,
};
use crate::state::combat::prelude::*;

pub(crate) fn timeline_slots(
    source: &DynamicMessage,
    target: &DynamicMessage,
    skill_id: i32,
    effect: &TutorialSkillEffect,
) -> Result<Option<i32>, StateError> {
    let Some(rule) = rule_for(effect.id, "active", "skill", skill_id)? else {
        return Ok(None);
    };
    if rule.operation != "timeline_shift"
        || !condition(rule, source)
        || !target_condition(rule, target)
        || !lamp_condition_matches(source, skill_id, rule)?
    {
        return Ok(None);
    }
    let value = amount(rule, effect.value)?;
    if value == 0 || value % 100 != 0 {
        return Err(StateError::MasterData(format!(
            "invalid timeline shift for effect {}",
            effect.id
        )));
    }
    Ok(Some(value / 100))
}

pub(crate) fn instant_summary(
    skill: &TutorialSkill,
    target: &DynamicMessage,
    critical: bool,
    summary: i32,
) -> Result<i64, StateError> {
    instant_summary_for_source(None, skill, target, critical, summary)
}

pub(crate) fn instant_summary_for_source(
    source: Option<&DynamicMessage>,
    skill: &TutorialSkill,
    target: &DynamicMessage,
    critical: bool,
    summary: i32,
) -> Result<i64, StateError> {
    let weak = target_resistance(target, preferred_attack_attribute(target, skill)?)? < 0;
    skill.effects.iter().try_fold(0i64, |total, effect| {
        if rule_for(effect.id, "catalog", "skill", skill.id)?.is_some() {
            return Ok(total);
        }
        let Some(rule) = rule_for(effect.id, "instant", "skill", skill.id)? else {
            return Ok(total);
        };
        if rule.operation != "summary" || rule.summary != summary {
            return Ok(total);
        }
        if (rule.weak_only && !weak)
            || (rule.target_broken
                && !member_status(target, "enemy")
                    .ok()
                    .is_some_and(|enemy| bool_field(&enemy, "is_broken")))
            || !target_condition(rule, target)
            || source.is_some_and(|member| !condition(rule, member))
            || !context_matches(rule, 0, skill, critical)
            || (rule.condition.contains_key("skill_lamp_full")
                && source.is_none_or(|member| {
                    !lamp_condition_matches(member, skill.id, rule).unwrap_or(false)
                }))
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
        let passive = if summary == 1 {
            self.lamp_skill_damage(target)
        } else {
            0
        } + self
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
