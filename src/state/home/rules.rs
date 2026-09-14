use super::prelude::*;

use crate::state::context::StateContext;

pub(crate) const TUTORIAL_STEP_HOME_READY: i32 = 600;

#[derive(Clone, Deserialize)]
pub(crate) struct HomeRules {
    pub source_sha256: String,
    pub(crate) energy: energy::EnergyRules,
    pub(crate) missions: Vec<MissionRule>,
    pub(crate) mission_battle_rewards: Vec<MissionBattleRewardRule>,
    pub(crate) total_tasks: Vec<TotalTaskRule>,
    pub(crate) quest_kinds: Vec<QuestKind>,
    pub(crate) material_ids: Vec<i32>,
    pub(crate) synthesis_targets: Vec<SynthesisTarget>,
    pub(crate) battle_tools: Vec<MissionTool>,
    pub(crate) equipment_tools: Vec<MissionTool>,
    pub(crate) character_levels: Vec<CharacterLevel>,
    pub(crate) memoria_levels: Vec<CharacterLevel>,
    pub(crate) ship_levels: Vec<CharacterLevel>,
    pub(crate) mission_count_rewards: Vec<CountRule>,
    pub(crate) mission_event_tabs: Vec<EventTab>,
    pub(crate) guide_steps: Vec<GuideStep>,
    pub(crate) login_bonuses: Vec<LoginRule>,
    pub(crate) login_days: Vec<LoginDay>,
    pub(crate) login_regular_days: Vec<RegularDay>,
    pub(crate) reward_sets: Vec<TutorialRewardSet>,
    pub(crate) user_ranks: Vec<RankRule>,
    pub(crate) initial_character_level_limit: i32,
    pub(crate) characters: Vec<HomeCharacter>,
    pub(crate) memoria_ids: Vec<i32>,
    pub(crate) home_ids: Vec<i32>,
    pub(crate) chara_home_camera_ids: Vec<i32>,
    pub(crate) chara_home_motion_ids: Vec<i32>,
    pub(crate) chara_home_bgm_ids: Vec<i32>,
    pub(crate) recipes: Vec<RecipeRule>,
    pub(crate) recipe_plans: Vec<RecipePlanRule>,
    #[serde(default)]
    pub(crate) multi_missions: Vec<MultiMissionRule>,
    #[serde(default)]
    pub(crate) multi_mission_steps: Vec<MultiMissionStep>,
    #[serde(skip)]
    pub(crate) mission_index: StateContext,
}

#[derive(Clone, Deserialize)]
pub(crate) struct QuestKind {
    pub(crate) id: i32,
    pub(crate) kinds: Vec<String>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct HomeCharacter {
    pub(crate) id: i32,
    pub(crate) rarity: i32,
    pub(crate) duplicated_piece_count: i32,
    pub(crate) duplicated_generic_piece_count: i32,
}

#[derive(Clone, Deserialize)]
pub(crate) struct MissionRule {
    pub(crate) id: i32,
    pub(crate) category: i32,
    pub(crate) reset_cycle: Option<i32>,
    pub(crate) total_task_condition_id: Option<i32>,
    pub(crate) prev_mission_id: Option<i32>,
    pub(crate) guide_mission_step_id: Option<i32>,
    pub(crate) navigation_task_url_id: Option<i32>,
    pub(crate) event_tab_id: Option<i32>,
    pub(crate) start_at: Option<i64>,
    pub(crate) end_at: Option<i64>,
    #[serde(flatten)]
    pub(crate) objective: MissionObjective,
    pub(crate) steps: Vec<MissionStep>,
}

#[derive(Clone, Default, Deserialize)]
pub(crate) struct MissionObjective {
    pub(crate) counter: Option<String>,
    #[serde(default)]
    pub(crate) counters: Vec<String>,
    #[serde(default)]
    pub(crate) maximum: bool,
    pub(crate) synthesis: Option<SynthesisObjective>,
    pub(crate) state: Option<ResourceObjective>,
    pub(crate) battle: Option<BattleObjective>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct BattleObjective {
    pub(crate) quest_ids: Vec<i32>,
    pub(crate) character_ids: Vec<i32>,
    pub(crate) minimum: usize,
}

#[derive(Clone, Deserialize)]
pub(crate) struct MissionFloor {
    pub(crate) quest_id: i32,
    pub(crate) floor: i32,
}

impl MissionObjective {
    pub(crate) fn advance(&self, current: i32, delta: i32) -> i32 {
        if self.maximum {
            current.max(delta)
        } else {
            current.saturating_add(delta)
        }
    }
}

#[derive(Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum ResourceObjective {
    Rank,
    Characters,
    Recipes,
    CharacterLevel {
        minimum: Option<i32>,
        #[serde(default)]
        character_ids: Vec<i32>,
    },
    MemoriaLevel {
        minimum: Option<i32>,
        memoria_id: Option<i32>,
    },
    Board {
        field: String,
        minimum: i32,
    },
    Equipment {
        character_id: i32,
        tool_ids: Vec<i32>,
        trait_ids: Vec<i32>,
    },
    EquippedMemoria {
        character_id: i32,
        memoria_id: i32,
    },
    OwnedCharacter {
        character_ids: Vec<i32>,
    },
    CharacterRarity {
        character_ids: Vec<i32>,
        initial_rarity: i32,
    },
    OwnedMemoria {
        memoria_id: i32,
    },
    MemoriaLimit {
        memoria_id: i32,
    },
    Research {
        group_id: i32,
    },
    Communication {
        character_ids: Vec<i32>,
    },
    NeoOpen,
    Tower {
        floors: Vec<MissionFloor>,
    },
    Housing {
        house_building_id: i32,
    },
    PresentCloseness {
        present_ids: Vec<i32>,
    },
    StreetPhase {
        quest_id: i32,
    },
    ScoreRank {
        #[serde(default)]
        quest_ids: Vec<i32>,
        minimum_rank: i32,
    },
    OwnedHomeMotion {
        motion_ids: Vec<i32>,
    },
    CompletedMissions {
        mission_ids: Vec<i32>,
    },
    QuestHighScoreTotal {
        quest_ids: Vec<i32>,
    },
    QuestClearAny {
        quest_ids: Vec<i32>,
    },
    MultiMission {
        multi_mission_id: i32,
    },
    ShipLevel,
    ShipParty,
}

#[derive(Clone, Deserialize)]
pub(crate) struct MultiMissionRule {
    pub(crate) id: i32,
    pub(crate) event_id: i32,
    pub(crate) progress_kind: String,
    pub(crate) item_id: Option<i32>,
    #[serde(default)]
    pub(crate) quest_ids: Vec<i32>,
    pub(crate) start_at: Option<i64>,
    pub(crate) end_at: Option<i64>,
}

#[derive(Clone, Deserialize)]
pub(crate) struct MultiMissionStep {
    pub(crate) id: i32,
    pub(crate) multi_mission_id: i32,
    pub(crate) step: i32,
    pub(crate) count: i64,
    pub(crate) start_at: Option<i64>,
    #[serde(default)]
    pub(crate) rewards: Vec<TutorialReward>,
    #[serde(default)]
    pub(crate) learnable_recipe_ids: Vec<i32>,
}
