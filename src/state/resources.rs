// Small mutation records shared by state reducers.

use super::{
    catalog::{TutorialCharacterSkill, TutorialSkillEffect, TutorialTraitParam},
    combat::BattleStats,
};
use prost_reflect::DynamicMessage;

#[derive(Debug, Clone)]
pub(crate) struct ResourceMutation {
    pub(crate) resources: DynamicMessage,
    pub(crate) response: DynamicMessage,
}

#[derive(Debug, Clone)]
pub(crate) struct GachaMutation {
    pub(crate) resources: DynamicMessage,
    pub(crate) response: DynamicMessage,
    pub(crate) character_indexes: Vec<(i64, Option<i64>)>,
}

#[derive(Debug, Clone)]
pub(crate) struct BattlePassiveEffect {
    pub(crate) ability_id: i32,
    pub(crate) effect_index: Option<usize>,
    pub(crate) effect: TutorialSkillEffect,
}

#[derive(Debug, Clone)]
pub(crate) struct BattlePartyMember {
    pub(crate) character_id: i32,
    pub(crate) level: i32,
    pub(crate) rarity: i32,
    pub(crate) memoria_id: Option<i32>,
    pub(crate) position: i32,
    pub(crate) is_leader: bool,
    pub(crate) integrated_stats: Option<BattleStats>,
    pub(crate) damage_bonus: i32,
    pub(crate) skills: Vec<TutorialCharacterSkill>,
    pub(crate) ability_ids: Vec<i32>,
    pub(crate) passives: Vec<BattlePassiveEffect>,
    pub(crate) leader_passives: Vec<BattlePassiveEffect>,
}

#[derive(Debug, Clone)]
pub(crate) struct BattlePartyTool {
    pub(crate) tool_id: i32,
    pub(crate) usage_count: i32,
    pub(crate) traits: Vec<TutorialTraitParam>,
}
