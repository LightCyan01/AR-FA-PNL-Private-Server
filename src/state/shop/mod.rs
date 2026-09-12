mod challenges;
mod dispatch;
mod passes_and_growth;
mod random_and_box;
mod rules_catalog_purchase;

pub(crate) use dispatch::*;
pub(crate) use rules_catalog_purchase::*;

pub(crate) mod prelude {
    pub(crate) use super::{
        challenges::*, passes_and_growth::*, random_and_box::*, rules_catalog_purchase::*,
    };
    pub(crate) use crate::state::service::prelude::*;
}

#[cfg(test)]
mod tests;
