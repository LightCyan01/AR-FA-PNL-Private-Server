mod collection;
mod dispatch;
mod memoria;
mod research;
mod rules_and_stats;
mod ship;

pub(crate) use collection::*;
pub(crate) use dispatch::*;
pub(crate) use rules_and_stats::*;

pub(crate) mod prelude {
    pub(crate) use super::{collection::*, memoria::*, research::*, rules_and_stats::*, ship::*};
    pub(crate) use crate::state::service::prelude::*;
}

#[cfg(test)]
mod tests;
