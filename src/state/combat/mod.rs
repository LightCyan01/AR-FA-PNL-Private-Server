//! Battle state, timeline, effects, and lifecycle.

mod actions;
mod attack;
pub(crate) mod effects;
mod lifecycle;
mod model;
mod timeline;

pub(crate) use actions::*;
pub(crate) use attack::*;
pub(crate) use lifecycle::*;
pub(crate) use model::*;
pub(crate) use timeline::*;

pub(crate) mod prelude {
    pub(crate) use super::{actions::*, attack::*, lifecycle::*, model::*, timeline::*};
    pub(crate) use crate::state::service::prelude::*;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BattleResult {
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BattleMutationPolicy {
    RejectWhileActive,
}

impl BattleMutationPolicy {
    pub(crate) fn permits_noncombat(self, active: bool) -> BattleResult {
        if active {
            BattleResult::Rejected
        } else {
            BattleResult::Accepted
        }
    }
}

pub(crate) fn home_route_is_read_only(route: &str) -> bool {
    matches!(
        route,
        "/local/home_refresh"
            | "/event/top"
            | "/mail/list"
            | "/shop/gem_list"
            | "/shop/random_shop/list"
            | "/quest/cleared_party_list"
            | "/multi_mission/status"
    )
}

#[cfg(test)]
mod tests {
    use super::{home_route_is_read_only, BattleMutationPolicy, BattleResult};

    #[test]
    fn battle_policy_rejects_noncombat_mutations_while_active() {
        assert_eq!(
            BattleMutationPolicy::RejectWhileActive.permits_noncombat(true),
            BattleResult::Rejected,
        );
        assert_eq!(
            BattleMutationPolicy::RejectWhileActive.permits_noncombat(false),
            BattleResult::Accepted,
        );
        assert!(home_route_is_read_only("/mail/list"));
        assert!(!home_route_is_read_only("/character/enhance"));
    }
}
