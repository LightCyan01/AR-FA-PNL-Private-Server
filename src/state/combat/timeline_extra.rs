use super::prelude::*;

const EXTRA_TURN: i32 = 1;

pub(crate) fn current_extra_timeline_turn(
    state: &DynamicMessage,
) -> Result<Option<(i32, i32)>, StateError> {
    let members = message_list(state, "members");
    let mut units = message_list(state, "timeline_units");
    sort_timeline_units(&mut units, &members);
    let Some(unit) = units.first() else {
        return Ok(None);
    };
    if i32_or_enum_field(unit, "type") != Some(EXTRA_TURN) {
        return Ok(None);
    }
    Ok(Some((
        member_id(unit)?,
        i32_field(unit, "number").ok_or(StateError::InvalidRequest)?,
    )))
}

pub(crate) fn insert_extra_timeline_turn(
    proto: &ProtoRegistry,
    units: &mut Vec<DynamicMessage>,
    members: &[DynamicMessage],
    id: i32,
    slots: i32,
) -> Result<(i32, DynamicMessage), StateError> {
    if slots <= 0
        || !members
            .iter()
            .any(|member| member_id(member).ok() == Some(id))
    {
        return Err(StateError::InvalidRequest);
    }
    sort_timeline_units(units, members);
    let index = usize::try_from(slots - 1)
        .map_err(|_| StateError::InvalidRequest)?
        .min(units.len());
    let wait = if units.is_empty() {
        0
    } else if index == 0 {
        let first = i32_field(&units[0], "wait").ok_or(StateError::InvalidRequest)?;
        if first > 0 {
            first - 1
        } else {
            shift_waits(&mut units[index..])?;
            0
        }
    } else if index == units.len() {
        i32_field(&units[index - 1], "wait")
            .ok_or(StateError::InvalidRequest)?
            .checked_add(1)
            .ok_or(StateError::InvalidRequest)?
    } else {
        let lower = i32_field(&units[index - 1], "wait").ok_or(StateError::InvalidRequest)?;
        let upper = i32_field(&units[index], "wait").ok_or(StateError::InvalidRequest)?;
        if upper - lower > 1 {
            lower + (upper - lower) / 2
        } else {
            shift_waits(&mut units[index..])?;
            lower.checked_add(1).ok_or(StateError::InvalidRequest)?
        }
    };
    let number = units
        .iter()
        .filter(|unit| member_id(unit).ok() == Some(id))
        .filter_map(|unit| i32_field(unit, "number"))
        .max()
        .unwrap_or_default()
        .checked_add(1)
        .ok_or(StateError::InvalidRequest)?;
    let mut unit = build_timeline_unit(proto, id, number, wait)?;
    unit.set_field_by_name("type", Value::EnumNumber(EXTRA_TURN));
    units.push(unit);
    sort_timeline_units(units, members);
    let to_index = units
        .iter()
        .position(|unit| {
            member_id(unit).ok() == Some(id)
                && i32_field(unit, "number") == Some(number)
                && i32_or_enum_field(unit, "type") == Some(EXTRA_TURN)
        })
        .ok_or(StateError::InvalidRequest)?;
    Ok((
        number,
        timeline_move(proto, id, number, Some(wait), 6, 0, Some(to_index))?,
    ))
}

pub(crate) fn consume_extra_timeline_turn(
    proto: &ProtoRegistry,
    units: &mut Vec<DynamicMessage>,
    members: &[DynamicMessage],
    id: i32,
) -> Result<Option<DynamicMessage>, StateError> {
    sort_timeline_units(units, members);
    let Some(unit) = units.first() else {
        return Ok(None);
    };
    if i32_or_enum_field(unit, "type") != Some(EXTRA_TURN) {
        return Ok(None);
    }
    if member_id(unit)? != id {
        return Err(StateError::InvalidRequest);
    }
    let wait = i32_field(unit, "wait").ok_or(StateError::InvalidRequest)?;
    let number = i32_field(unit, "number").ok_or(StateError::InvalidRequest)?;
    for unit in units.iter_mut() {
        let current = i32_field(unit, "wait").ok_or(StateError::InvalidRequest)?;
        unit.set_field_by_name("wait", Value::I32(current.saturating_sub(wait)));
    }
    units.remove(0);
    sort_timeline_units(units, members);
    if let Some(next_wait) = units.first().and_then(|unit| i32_field(unit, "wait")) {
        for unit in units.iter_mut() {
            let current = i32_field(unit, "wait").ok_or(StateError::InvalidRequest)?;
            unit.set_field_by_name("wait", Value::I32(current.saturating_sub(next_wait)));
        }
    }
    Ok(Some(timeline_move(proto, id, number, None, 1, 0, None)?))
}

fn shift_waits(units: &mut [DynamicMessage]) -> Result<(), StateError> {
    for unit in units {
        let wait = i32_field(unit, "wait")
            .ok_or(StateError::InvalidRequest)?
            .checked_add(1)
            .ok_or(StateError::InvalidRequest)?;
        unit.set_field_by_name("wait", Value::I32(wait));
    }
    Ok(())
}
