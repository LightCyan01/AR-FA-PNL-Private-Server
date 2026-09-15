use super::registry::{NestedActionRule, Rule};
use crate::state::combat::prelude::*;

pub(crate) fn display(
    proto: &ProtoRegistry,
    id: i32,
    value: i32,
    remaining: i32,
) -> Result<DynamicMessage, StateError> {
    let mut row = empty_message(proto, "blend.model.BattleStateChange")?;
    row.set_field_by_name("state_change_id", Value::I32(id));
    row.set_field_by_name("value", Value::I32(value));
    row.set_field_by_name("rest_count", Value::I32(remaining));
    Ok(row)
}

pub(super) fn effect_result(
    proto: &ProtoRegistry,
    effect_id: i32,
    source_id: i32,
    target_id: i32,
    is_skill: bool,
    rule: &Rule,
    value: i32,
) -> Result<DynamicMessage, StateError> {
    let mut result = empty_message(proto, "blend.model.BattleEffectResult")?;
    result.set_field_by_name("effect_id", Value::I32(effect_id));
    result.set_field_by_name("effector_id", Value::I32(source_id));
    result.set_field_by_name(
        "effect_target_id",
        Value::Message(wrapper_i32(proto, target_id)?),
    );
    result.set_field_by_name("is_skill", Value::Bool(is_skill));
    if rule.state_id > 0 {
        result.set_field_by_name(
            "deal_state_change_id",
            Value::Message(wrapper_i32(proto, rule.state_id)?),
        );
        result.set_field_by_name("deal_state_change_result", Value::EnumNumber(1));
        result.set_field_by_name(
            "dealt_state_change",
            Value::Message(display(proto, rule.state_id, value, rule.duration)?),
        );
    }
    Ok(result)
}

pub(super) fn nested_effect_result(
    proto: &ProtoRegistry,
    effect_id: i32,
    source_id: i32,
    target_id: i32,
    is_skill: bool,
    rule: &NestedActionRule,
    outcome: i32,
) -> Result<DynamicMessage, StateError> {
    let mut result = empty_message(proto, "blend.model.BattleEffectResult")?;
    result.set_field_by_name("effect_id", Value::I32(effect_id));
    result.set_field_by_name("effector_id", Value::I32(source_id));
    result.set_field_by_name(
        "effect_target_id",
        Value::Message(wrapper_i32(proto, target_id)?),
    );
    result.set_field_by_name("is_skill", Value::Bool(is_skill));
    if rule.state_id > 0 {
        result.set_field_by_name(
            "deal_state_change_id",
            Value::Message(wrapper_i32(proto, rule.state_id)?),
        );
        result.set_field_by_name("deal_state_change_result", Value::EnumNumber(outcome));
        if outcome == 1 {
            result.set_field_by_name(
                "dealt_state_change",
                Value::Message(display(proto, rule.state_id, 0, rule.duration)?),
            );
        }
    }
    Ok(result)
}

pub(super) fn status_display(
    proto: &ProtoRegistry,
    state_id: i32,
    value: i32,
    remaining: i32,
    source_id: i32,
) -> Result<DynamicMessage, StateError> {
    let mut row = display(proto, state_id, value, remaining)?;
    if state_id == 940001 {
        row.set_field_by_name("is_provocation", Value::Bool(true));
        row.set_field_by_name("target_id", Value::Message(wrapper_i32(proto, source_id)?));
    }
    Ok(row)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn status_effect_result(
    proto: &ProtoRegistry,
    effect_id: i32,
    source_id: i32,
    target_id: i32,
    is_skill: bool,
    rule: &Rule,
    value: i32,
    outcome: i32,
) -> Result<DynamicMessage, StateError> {
    let mut result = empty_message(proto, "blend.model.BattleEffectResult")?;
    result.set_field_by_name("effect_id", Value::I32(effect_id));
    result.set_field_by_name("effector_id", Value::I32(source_id));
    result.set_field_by_name(
        "effect_target_id",
        Value::Message(wrapper_i32(proto, target_id)?),
    );
    result.set_field_by_name("is_skill", Value::Bool(is_skill));
    result.set_field_by_name(
        "deal_state_change_id",
        Value::Message(wrapper_i32(proto, rule.state_id)?),
    );
    result.set_field_by_name("deal_state_change_result", Value::EnumNumber(outcome));
    if outcome == 1 {
        result.set_field_by_name(
            "dealt_state_change",
            Value::Message(status_display(
                proto,
                rule.state_id,
                value,
                rule.duration,
                source_id,
            )?),
        );
    }
    Ok(result)
}

pub(super) fn turn_state_change_result(
    proto: &ProtoRegistry,
    member_id: i32,
    state_id: i32,
    hp_damage: i64,
    hp_heal: i32,
    disabled_action: bool,
) -> Result<DynamicMessage, StateError> {
    let mut result = empty_message(proto, "blend.model.BattleStateChangeResult")?;
    result.set_field_by_name("member_id", Value::I32(member_id));
    result.set_field_by_name("state_change_id", Value::I32(state_id));
    if hp_damage > 0 {
        result.set_field_by_name("hp_damage", Value::Message(wrapper_i64(proto, hp_damage)?));
    }
    if hp_heal > 0 {
        result.set_field_by_name("hp_heal", Value::Message(wrapper_i32(proto, hp_heal)?));
    }
    result.set_field_by_name("is_invalid", Value::Bool(false));
    result.set_field_by_name("disabled_action", Value::Bool(disabled_action));
    Ok(result)
}
