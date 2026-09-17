use super::registry::registry;
use super::runtime::Passive;
use super::runtime_results::effect_result;
use crate::state::combat::prelude::*;

pub(super) fn add_party_gauge(
    state: &mut DynamicMessage,
    value: i32,
) -> Result<i32, StateError> {
    let maximum = registry()?.max_party_gauge;
    let heal = i64::from(maximum).saturating_mul(i64::from(value.max(0))) / 10_000;
    let heal = i32::try_from(heal).map_err(|_| StateError::InvalidRequest)?;
    let current = i32_field(state, "party_gauge").unwrap_or_default().max(0);
    state.set_field_by_name(
        "party_gauge",
        Value::I32(current.saturating_add(heal).min(maximum)),
    );
    Ok(heal)
}

pub(super) fn apply_party_gauge_passive(
    proto: &ProtoRegistry,
    state: &mut DynamicMessage,
    passive: &Passive,
) -> Result<DynamicMessage, StateError> {
    let heal = add_party_gauge(state, passive.value)?;
    let mut result = effect_result(
        proto,
        passive.rule.id,
        passive.source,
        passive.source,
        false,
        &passive.rule,
        passive.value,
    )?;
    if heal > 0 {
        result.set_field_by_name(
            "party_gauge_heal",
            Value::Message(wrapper_i32(proto, heal)?),
        );
    }
    Ok(result)
}

pub(super) fn add_burst_gauge(
    member: &mut DynamicMessage,
    value: i32,
    required: i32,
) -> Result<i32, StateError> {
    let mut gauge = member_status(member, "burst_gauge")?;
    let maximum = i32_field(&gauge, "max_gauge")
        .ok_or(StateError::InvalidRequest)?
        .max(0);
    let current = i32_field(&gauge, "current_gauge")
        .unwrap_or_default()
        .max(0);
    let next = current
        .saturating_add(value.saturating_div(100))
        .clamp(0, maximum);
    gauge.set_field_by_name("current_gauge", Value::I32(next));
    gauge.set_field_by_name("is_enable", Value::Bool(next >= required));
    member.set_field_by_name("burst_gauge", Value::Message(gauge));
    Ok(next - current)
}
