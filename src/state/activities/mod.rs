mod battle;
mod combat_power;
mod dispatch;
mod exploration;
mod gifts;
mod housing;
mod rules;
mod story;
mod street_expedition;

pub(crate) use battle::*;
pub(crate) use combat_power::*;
pub(crate) use dispatch::*;
pub(crate) use rules::*;
pub(crate) use story::*;

pub(crate) mod prelude {
    pub(crate) use super::{
        exploration::*, gifts::*, housing::*, rules::*, story::*, street_expedition::*,
    };
    pub(crate) use crate::state::service::prelude::*;
}

#[cfg(test)]
mod tests;
