use crate::state::combat::prelude::*;

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
