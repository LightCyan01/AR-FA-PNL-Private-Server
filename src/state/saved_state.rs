// Persisted cross-domain snapshot. Domain reducers own the mutations; this
// module owns the serialized shape shared by storage, Home, Atelier, and battle.

use super::{activities, combat::effects, shop};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct HomeState {
    pub(crate) language: Option<i32>,
    pub(crate) initialized_at: i64,
    pub(crate) login_day: Option<i64>,
    pub(crate) bonuses: BTreeMap<i32, BonusState>,
    pub(crate) guide_rewards: BTreeSet<i32>,
    pub(crate) mails: Vec<Vec<u8>>,
    pub(crate) next_mail_id: i32,
    pub(crate) memoria_history: BTreeSet<i32>,
    pub(crate) shop: shop::ShopState,
    pub(crate) activities: activities::ActivityState,
    pub(crate) battle_progress: BattleProgress,
    pub(crate) combat_effects: effects::Runtime,
    pub(crate) gacha_battle_id: Option<i32>,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct BattleProgress {
    pub(crate) start_txid: String,
    pub(crate) damage: i64,
    pub(crate) max_dealt_hp_damage: i64,
    pub(crate) received_hp_damage: i64,
    pub(crate) initial_ally_hp: i64,
    pub(crate) initial_enemy_hp: i64,
    pub(crate) score_waves: BTreeSet<i32>,
    pub(crate) events: BTreeSet<String>,
    pub(crate) tool_uses: BTreeMap<i32, i32>,
    pub(crate) skill_records: BTreeMap<i32, BTreeSet<i32>>,
}

#[derive(Default, Deserialize, Serialize)]
pub(crate) struct BonusState {
    pub(crate) day: i32,
    pub(crate) last_day: i64,
}
