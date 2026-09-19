// Deserialized rule catalogs consumed by state domains.

use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(crate) struct TutorialTraitParam {
    pub(crate) id: i32,
    pub(crate) rank: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct FreshStateRules {
    pub(crate) format: String,
    pub(crate) source_sha256: String,
    pub(crate) initial_rewards: Vec<InitialReward>,
    pub(crate) initial_character: InitialCharacter,
    pub(crate) initial_battle_tool_id: i32,
    pub(crate) initial_recipe_ids: Vec<i32>,
    pub(crate) rank_one: RankOne,
    pub(crate) constants: FreshConstants,
    pub(crate) task_condition_ids: Vec<i32>,
    pub(crate) proven_task_counts: Vec<TaskCountRule>,
    pub(crate) protocol_defaults: ProtocolDefaults,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct CharacterRules {
    pub(crate) format: String,
    pub(crate) source_sha256: String,
    pub(crate) constants: CharacterConstants,
    pub(crate) characters: Vec<CharacterRule>,
    pub(crate) levels: Vec<CharacterLevel>,
    pub(crate) rarities: Vec<CharacterRarityRule>,
    pub(crate) level_limit_releases: Vec<LevelLimitReleaseRule>,
    pub(crate) skill_evolve: Vec<SkillEvolveRule>,
    pub(crate) skill_lock_release: Vec<SkillLockReleaseRule>,
    pub(crate) skins: Vec<SkinRule>,
    pub(crate) boards: Vec<BoardRule>,
    pub(crate) pages: Vec<PageRule>,
    pub(crate) panels: Vec<PanelRule>,
    pub(crate) exp_items: Vec<ExpItemRule>,
    pub(crate) equipment_tools: Vec<EquipmentRule>,
    pub(crate) development_research_group_ids: Vec<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct CharacterConstants {
    pub(crate) character_enhancement_cole_per_exp_rate: i32,
    pub(crate) character_enhancement_reset_item_id: i32,
    pub(crate) character_enhancement_reset_item_required_level: i32,
    pub(crate) initial_character_level_limit: i32,
    pub(crate) party_count_per_content: i32,
    pub(crate) party_params: Vec<PartyParamRule>,
    pub(crate) required_character_level_for_neo: i32,
    pub(crate) required_dev_research_level_for_neo: i32,
    pub(crate) equipment_preset_count: i32,
    pub(crate) equipment_preset_max_name_length: i32,
    pub(crate) equipment_preset_initial_name_i18n: BTreeMap<String, String>,
    pub(crate) skill_evolve_enable_rarity: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct PartyParamRule {
    pub(crate) battle_tool_count_offset: i32,
    pub(crate) party_type: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct CharacterRule {
    pub(crate) id: i32,
    pub(crate) base_character_id: i32,
    pub(crate) role: i32,
    #[serde(default)]
    pub(crate) attack_attributes: Vec<i32>,
    pub(crate) default_skin_id: Option<i32>,
    pub(crate) initial_rarity: i32,
    pub(crate) max_rarity: i32,
    pub(crate) growboard_id: i32,
    pub(crate) ex_growboard_id: i32,
    pub(crate) neo_growboard_id: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct CharacterRarityRule {
    pub(crate) id: i32,
    pub(crate) growboard_neo_max_page: i32,
    pub(crate) rarity_enhance_piece_costs: Vec<i32>,
    pub(crate) rarity_enhance_additional_costs: Vec<RarityAdditionalCost>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RarityAdditionalCost {
    pub(crate) costs: Vec<RuleCost>,
    pub(crate) required: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RuleCost {
    pub(crate) id: i32,
    pub(crate) quantity: i32,
    #[serde(rename = "type", default = "default_cost_type")]
    pub(crate) resource_type: i32,
}

fn default_cost_type() -> i32 {
    5
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct LevelLimitReleaseRule {
    pub(crate) id: i32,
    pub(crate) value: i32,
    pub(crate) item_costs: Vec<RuleCost>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SkillEvolveRule {
    pub(crate) character_id: i32,
    pub(crate) normal1_skill_evolve_costs: Vec<RuleCost>,
    pub(crate) normal2_skill_evolve_costs: Vec<RuleCost>,
    pub(crate) burst_skill_evolve_costs: Vec<RuleCost>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SkillLockReleaseRule {
    pub(crate) character_id: i32,
    pub(crate) normal1_skill_lock_release_costs: Vec<RuleCost>,
    pub(crate) normal2_skill_lock_release_costs: Vec<RuleCost>,
    pub(crate) burst_skill_lock_release_costs: Vec<RuleCost>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SkinRule {
    pub(crate) id: i32,
    pub(crate) character_id: Option<i32>,
    pub(crate) base_character_id: Option<i32>,
    pub(crate) reject_character_ids: Vec<i32>,
    pub(crate) start_at: Option<i64>,
    pub(crate) end_at: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct BoardRule {
    pub(crate) id: i32,
    pub(crate) board_type: i32,
    pub(crate) is_ex: bool,
    pub(crate) page_ids: Vec<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct PageRule {
    pub(crate) id: i32,
    pub(crate) panel_ids: Vec<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct PanelRule {
    pub(crate) id: i32,
    pub(crate) cole_cost: i32,
    pub(crate) item_costs: Vec<RuleCost>,
    pub(crate) status_type: Option<i32>,
    #[serde(rename = "type")]
    pub(crate) panel_type: i32,
    pub(crate) value: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ExpItemRule {
    pub(crate) id: i32,
    pub(crate) value: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct EquipmentRule {
    pub(crate) id: i32,
    pub(crate) slot_type: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct InitialReward {
    #[serde(rename = "type")]
    pub(crate) resource_type: i32,
    pub(crate) id: i32,
    pub(crate) quantity: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct InitialCharacter {
    pub(crate) id: i32,
    pub(crate) rarity: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RankOne {
    pub(crate) id: i32,
    pub(crate) exp: i32,
    pub(crate) stamina: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct FreshConstants {
    pub(crate) initial_character_level_limit: i32,
    pub(crate) initial_party_max_battle_tool_count: i32,
    pub(crate) mana_recovery_limit: i32,
    pub(crate) profile_initial_favorite_character_id: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TaskCountRule {
    pub(crate) condition_id: i32,
    pub(crate) count: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ProtocolDefaults {
    pub(crate) entity_id: i32,
    pub(crate) party_number: i32,
    pub(crate) party_type: i32,
    pub(crate) leader_position: i32,
    pub(crate) party_member_positions: i32,
    pub(crate) profile_name: String,
    pub(crate) profile_memo: String,
    pub(crate) dishes_when_updated: i32,
    pub(crate) expedition_max_count: i32,
    pub(crate) tutorial_step: i32,
    pub(crate) gacha_category: i32,
    pub(crate) gacha_category_2: i32,
    pub(crate) gacha_category_3: i32,
    pub(crate) gacha_id: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialRules {
    pub(crate) format: String,
    pub(crate) source_sha256: String,
    pub(crate) episode_id: i32,
    pub(crate) quests: Vec<TutorialQuest>,
    pub(crate) reward_sets: Vec<TutorialRewardSet>,
    pub(crate) reward_characters: Vec<TutorialCharacter>,
    #[serde(default)]
    pub(crate) reward_memorias: Vec<TutorialMemoria>,
    pub(crate) character_levels: Vec<CharacterLevel>,
    pub(crate) battles: Vec<TutorialBattle>,
    pub(crate) waves: Vec<TutorialWave>,
    pub(crate) stages: Vec<TutorialId>,
    pub(crate) enemies: Vec<TutorialEnemy>,
    pub(crate) base_enemies: Vec<TutorialId>,
    pub(crate) battle_characters: Vec<TutorialBattleCharacter>,
    pub(crate) character_growths: Vec<TutorialCharacterGrowth>,
    pub(crate) character_rarities: Vec<TutorialCharacterRarity>,
    pub(crate) skills: Vec<TutorialSkill>,
    pub(crate) abilities: Vec<TutorialAbility>,
    pub(crate) effects: Vec<TutorialEffect>,
    #[serde(default)]
    pub(crate) summons_effects: Vec<TutorialSummonsEffect>,
    pub(crate) timeline_panels: Vec<TutorialPanel>,
    #[serde(default)]
    pub(crate) enemy_ai_units: Vec<TutorialEnemyAiUnit>,
    pub(crate) constants: TutorialBattleConstants,
    pub(crate) fixed_parties: Vec<TutorialFixedParty>,
    pub(crate) battle_tools: Vec<TutorialBattleTool>,
    #[serde(default)]
    pub(crate) battle_tool_mixes: Vec<TutorialBattleToolMix>,
    #[serde(default)]
    pub(crate) ship_tools: Vec<TutorialShipTool>,
    pub(crate) recipes: Vec<TutorialRecipe>,
    pub(crate) synthesis_characters: Vec<TutorialSynthesisSource>,
    pub(crate) synthesis_ingredients: Vec<TutorialSynthesisSource>,
    pub(crate) trait_ranks: Vec<TutorialTraitRank>,
    pub(crate) battle_tool_traits: Vec<TutorialBattleToolTrait>,
    pub(crate) gachas: Vec<TutorialGacha>,
    pub(crate) gacha_buttons: Vec<TutorialGachaButton>,
    #[serde(default)]
    pub(crate) gacha_step_up_buttons: Vec<TutorialGachaButton>,
    pub(crate) gacha_rates: Vec<TutorialGachaRate>,
    pub(crate) gacha_decks: Vec<TutorialGachaDeck>,
    #[serde(default)]
    pub(crate) gacha_pools: Vec<Vec<TutorialGachaCard>>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialRecipe {
    pub(crate) id: i32,
    pub(crate) mana_cost: i32,
    pub(crate) costs: Vec<TutorialRecipeCost>,
    pub(crate) target_reward: TutorialReward,
    pub(crate) support_character_ids: Vec<i32>,
    pub(crate) trait_count: i32,
    pub(crate) tutorial_grade: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialSynthesisSource {
    pub(crate) id: i32,
    pub(crate) trait_ids: Vec<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialTraitRank {
    pub(crate) id: i32,
    pub(crate) weight: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialBattleToolTrait {
    pub(crate) id: i32,
    pub(crate) filter_ids: Vec<i32>,
    pub(crate) effects: Vec<TutorialTraitEffect>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialTraitEffect {
    pub(crate) id: i32,
    pub(crate) values: Vec<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialRecipeCost {
    #[serde(rename = "type")]
    pub(crate) resource_type: i32,
    pub(crate) id: i32,
    pub(crate) quantity: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialGacha {
    pub(crate) id: i32,
    #[serde(default)]
    pub(crate) gacha_battle_ids: Vec<i32>,
    pub(crate) category: i32,
    pub(crate) gacha_type: i32,
    pub(crate) rate_set_id: i32,
    pub(crate) start_at: Option<i64>,
    pub(crate) end_at: Option<i64>,
    pub(crate) priority: i32,
    pub(crate) medal_id: Option<i32>,
    pub(crate) button_ids: Vec<i32>,
    #[serde(default)]
    pub(crate) step_up_button_ids: Vec<i32>,
    #[serde(default)]
    pub(crate) step_up_loop_count: i32,
    #[serde(default)]
    pub(crate) wish_list_counts: [usize; 3],
    pub(crate) mixed_pickup_character_ids: Vec<i32>,
    pub(crate) mixed_pickup_memoria_ids: Vec<i32>,
    pub(crate) mixed_select_count: i32,
    pub(crate) duplicate_pieces: Vec<GachaDuplicatePieces>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct GachaDuplicatePieces {
    pub(crate) character_ids: Vec<i32>,
    pub(crate) piece_count: i32,
    pub(crate) generic_piece_count: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialGachaButton {
    pub(crate) id: i32,
    pub(crate) draw_count: i32,
    pub(crate) limit_count: Option<i32>,
    pub(crate) cost: Option<TutorialRecipeCost>,
    pub(crate) medal_quantity: i32,
    pub(crate) additional_medal: i32,
    #[serde(default)]
    pub(crate) rate_set_id: i32,
    #[serde(default)]
    pub(crate) is_wish_list: bool,
    #[serde(default)]
    pub(crate) duplicate_pieces: Vec<GachaDuplicatePieces>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialGachaRate {
    pub(crate) id: i32,
    pub(crate) rate_set_id: i32,
    pub(crate) deck_id: i32,
    pub(crate) priority: i32,
    pub(crate) total_basis_points: i32,
    #[serde(default)]
    pub(crate) cards: Vec<TutorialGachaCard>,
    #[serde(default)]
    pub(crate) pool_index: Option<usize>,
}

impl TutorialGachaRate {
    pub(crate) fn cards<'a>(&'a self, rules: &'a TutorialRules) -> &'a [TutorialGachaCard] {
        self.pool_index
            .and_then(|index| rules.gacha_pools.get(index))
            .map(Vec::as_slice)
            .unwrap_or(&self.cards)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialGachaCard {
    pub(crate) resource_type: i32,
    pub(crate) id: i32,
    pub(crate) rarity: i32,
    #[serde(default)]
    pub(crate) skin_id: Option<i32>,
    #[serde(default)]
    pub(crate) duplicate_rewards: Vec<TutorialReward>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialGachaDeck {
    pub(crate) id: i32,
    pub(crate) card_type: i32,
    pub(crate) rarity: i32,
}

#[derive(Debug, Default, Clone, Deserialize)]
pub(crate) struct TutorialQuest {
    pub(crate) id: i32,
    #[serde(default)]
    pub(crate) episode_id: i32,
    #[serde(default)]
    pub(crate) priority: i32,
    #[serde(default)]
    pub(crate) day: Option<i32>,
    #[serde(default)]
    pub(crate) skippable_type: i32,
    #[serde(default)]
    pub(crate) difficulty: Option<i32>,
    #[serde(default)]
    pub(crate) score_battle: serde_json::Value,
    #[serde(default)]
    pub(crate) episode_type: i32,
    #[serde(default)]
    pub(crate) key_story_id: Option<i32>,
    #[serde(default)]
    pub(crate) key_tasks: Vec<TaskCountRule>,
    #[serde(default)]
    pub(crate) stamina: i32,
    #[serde(default)]
    pub(crate) item_cost: Option<TutorialRecipeCost>,
    #[serde(default)]
    pub(crate) max_clear_count: i32,
    pub(crate) quest_type: i32,
    pub(crate) predecessor_id: Option<i32>,
    pub(crate) talk_event_id: Option<i32>,
    pub(crate) battle_id: Option<i32>,
    #[serde(default)]
    pub(crate) battle_ids: Vec<i32>,
    #[serde(default)]
    pub(crate) field_ability_ids: Vec<i32>,
    pub(crate) fixed_party_id: Option<i32>,
    #[serde(default)]
    pub(crate) rental_fixed_party_id: Option<i32>,
    #[serde(default)]
    pub(crate) solo_raid_id: Option<i32>,
    #[serde(default)]
    pub(crate) total_battle_panel_id: Option<i32>,
    #[serde(default)]
    pub(crate) total_battle_id: Option<i32>,
    pub(crate) first_clear_reward_set_id: Option<i32>,
    pub(crate) start_at: Option<i64>,
    pub(crate) end_at: Option<i64>,
    #[serde(default)]
    pub(crate) character_exp: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialId {
    pub(crate) id: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialPanel {
    pub(crate) id: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialEnemyAiUnit {
    pub(crate) enemy_ai_id: i32,
    #[allow(dead_code)]
    pub(crate) id: i32,
    pub(crate) skill_ids: Vec<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialBattleConstants {
    pub(crate) timeline_panel_count: i32,
    pub(crate) display_skill_wait_offset: i32,
    pub(crate) max_party_gauge: i32,
    pub(crate) turn_max_battle_tool_count: i32,
    pub(crate) battle_tool_mix_minimum_skill_power: i32,
    pub(crate) battle_tool_mix_rank_threshold_all: i32,
    pub(crate) battle_tool_mix_rank_threshold_single: i32,
    pub(crate) battle_tool_mix_skill_power_coefficient: i32,
    pub(crate) burst_gauge_required_for_one_burst_skill: i32,
    pub(crate) burst_gauge_heal_normal1: i32,
    pub(crate) burst_gauge_heal_normal2: i32,
    #[allow(dead_code)]
    pub(crate) burst_gauge_heal_extra: i32,
    #[allow(dead_code)]
    pub(crate) burst_gauge_heal_active_skill: i32,
    #[allow(dead_code)]
    pub(crate) burst_gauge_heal_additional_attack: i32,
    #[allow(dead_code)]
    pub(crate) burst_gauge_heal_counter: i32,
    pub(crate) initial_bomb_gauge: i32,
    pub(crate) bomb_gauge_recovery_amount: i32,
    pub(crate) max_bomb_gauge: i32,
    pub(crate) max_enemy_member_count: i32,
    pub(crate) total_turn_weight: i32,
    pub(crate) max_dealt_hp_damage_weight: i32,
    pub(crate) received_hp_damage_weight: i32,
    pub(crate) dead_allies_count_weight: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialBattle {
    pub(crate) id: i32,
    pub(crate) wave_ids: Vec<i32>,
    pub(crate) timeline_panel_ids: Vec<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialWave {
    pub(crate) id: i32,
    pub(crate) stage_id: i32,
    pub(crate) field_effect_id: Option<i32>,
    pub(crate) enemies: Vec<TutorialWaveEnemy>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialWaveEnemy {
    pub(crate) id: i32,
    pub(crate) level: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialEnemy {
    pub(crate) id: i32,
    pub(crate) base_enemy_id: i32,
    #[serde(default)]
    pub(crate) species_id: i32,
    pub(crate) status: BTreeMap<String, i32>,
    pub(crate) status_growth: BTreeMap<String, i32>,
    pub(crate) resistance: BTreeMap<String, i32>,
    pub(crate) break_gauge_coefficient: f64,
    pub(crate) break_gauges: Vec<i32>,
    #[serde(default)]
    pub(crate) break_phases: Vec<BreakPhase>,
    #[serde(default)]
    pub(crate) enemy_ai_id: i32,
    pub(crate) burst_skill_id: i32,
    pub(crate) extra_skill_ids: Vec<i32>,
    pub(crate) is_boss: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct BreakPhase {
    pub(crate) coefficient: f64,
    pub(crate) wait: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialBattleCharacter {
    pub(crate) id: i32,
    pub(crate) initial_rarity: i32,
    pub(crate) growth_id: i32,
    pub(crate) initial_status: BTreeMap<String, i32>,
    pub(crate) resistance: BTreeMap<String, i32>,
    #[serde(default)]
    pub(crate) tag_ids: Vec<i32>,
    pub(crate) skills: Vec<TutorialCharacterSkill>,
    #[serde(default)]
    pub(crate) active_skills: Vec<TutorialCharacterSkill>,
    #[serde(default)]
    pub(crate) extra_skill_ids: Vec<i32>,
    #[serde(default)]
    pub(crate) support_ability_ids: Vec<i32>,
    #[serde(default)]
    pub(crate) can_use_battle_tool_mix: bool,
    pub(crate) ability_ids: Vec<i32>,
    #[serde(default)]
    pub(crate) evolved_ability_ids: Vec<i32>,
    #[serde(default)]
    pub(crate) board_ability_ids: Vec<Vec<i32>>,
    #[serde(default)]
    pub(crate) leader_abilities: Vec<TutorialLeaderAbility>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialLeaderAbility {
    pub(crate) ability_id: i32,
    pub(crate) target_character_ids: Vec<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialCharacterSkill {
    pub(crate) id: i32,
    pub(crate) skill_type: i32,
    #[serde(default)]
    pub(crate) rank_ids: Vec<i32>,
    #[serde(default)]
    pub(crate) evolved_ids: Vec<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialCharacterGrowth {
    pub(crate) id: i32,
    pub(crate) level_coefficients: BTreeMap<String, i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialCharacterRarity {
    pub(crate) id: i32,
    pub(crate) ability_count: i32,
    pub(crate) status_coefficients: BTreeMap<String, i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialSkill {
    pub(crate) id: i32,
    pub(crate) skill_type: i32,
    pub(crate) skill_effect_type: i32,
    pub(crate) skill_power_type: i32,
    pub(crate) wait: i32,
    pub(crate) power: i32,
    pub(crate) break_power: i32,
    pub(crate) break_power_type: i32,
    pub(crate) attack_attributes: Vec<i32>,
    pub(crate) skill_target_type: Option<i32>,
    pub(crate) effects: Vec<TutorialSkillEffect>,
    #[serde(default)]
    pub(crate) limit_count: Option<i32>,
    #[serde(default)]
    pub(crate) max_lamp: i32,
    #[serde(default)]
    pub(crate) require_command_value: bool,
    #[serde(default)]
    pub(crate) skill_destination: Option<i32>,
    #[serde(default = "default_state_change_application_rate")]
    pub(crate) state_change_application_rate: i32,
    #[serde(default)]
    pub(crate) hp_damage_bonus: Option<TutorialHpDamageBonus>,
}

fn default_state_change_application_rate() -> i32 {
    10_000
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialHpDamageBonus {
    pub(crate) minimum: i32,
    pub(crate) maximum: i32,
    pub(crate) hp_minimum: i32,
    pub(crate) hp_maximum: i32,
    pub(crate) increases_with_hp: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialSkillEffect {
    pub(crate) id: i32,
    pub(crate) value: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialAbility {
    pub(crate) id: i32,
    pub(crate) effects: Vec<TutorialSkillEffect>,
    #[serde(default)]
    pub(crate) burst_gauge_max: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialEffect {
    pub(crate) id: i32,
    pub(crate) field_effect_id: Option<i32>,
    pub(crate) summons_effect_id: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialSummonsEffect {
    pub(crate) id: i32,
    pub(crate) enemies: Vec<TutorialWaveEnemy>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialFixedParty {
    pub(crate) id: i32,
    pub(crate) leader_position: i32,
    pub(crate) members: Vec<TutorialFixedMember>,
    pub(crate) battle_tools: Vec<TutorialFixedTool>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialFixedMember {
    pub(crate) character_id: i32,
    pub(crate) level: i32,
    pub(crate) rarity: i32,
    #[serde(default)]
    pub(crate) is_max_enhance: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialFixedTool {
    pub(crate) tool_id: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialBattleTool {
    pub(crate) id: i32,
    pub(crate) skill_id: i32,
    pub(crate) usage_count: i32,
    pub(crate) trait_filter_ids: Vec<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialRewardSet {
    pub(crate) id: i32,
    pub(crate) rewards: Vec<TutorialReward>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialReward {
    #[serde(rename = "type")]
    pub(crate) resource_type: i32,
    pub(crate) id: i32,
    pub(crate) quantity: i32,
    pub(crate) resource_params: Option<TutorialResourceParams>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialResourceParams {
    pub(crate) level: Option<i32>,
    pub(crate) rank: Option<i32>,
    pub(crate) skin: Option<i32>,
    #[serde(default)]
    pub(crate) traits: Vec<TutorialTraitParam>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialCharacter {
    pub(crate) id: i32,
    pub(crate) initial_rarity: i32,
    pub(crate) growboard_max_page: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SynthesisRules {
    pub(crate) format: String,
    pub(crate) source_sha256: String,
    pub(crate) constants: SynthesisConstants,
    pub(crate) local_policy: SynthesisLocalPolicy,
    pub(crate) recipes: Vec<SynthesisRecipe>,
    pub(crate) characters: Vec<SynthesisCharacter>,
    pub(crate) ingredients: Vec<SynthesisIngredient>,
    pub(crate) battle_tools: Vec<SynthesisTool>,
    pub(crate) equipment_tools: Vec<SynthesisTool>,
    pub(crate) battle_traits: Vec<SynthesisTrait>,
    pub(crate) equipment_traits: Vec<SynthesisTrait>,
    pub(crate) battle_lotteries: Vec<SynthesisLottery>,
    pub(crate) equipment_lotteries: Vec<SynthesisLottery>,
    pub(crate) trait_ranks: Vec<TutorialTraitRank>,
    pub(crate) character_rarity_bonuses: Vec<SynthesisWeightBonus>,
    pub(crate) user_rank_bonuses: Vec<SynthesisWeightBonus>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SynthesisConstants {
    pub(crate) mana_recovery_limit: i32,
    pub(crate) mana_recovery_interval_seconds: i64,
    pub(crate) battle_trait_count: i32,
    pub(crate) equipment_trait_count: i32,
    pub(crate) bulk_max_count: i32,
    pub(crate) easy_max_count: i32,
    pub(crate) max_rental_rank: i32,
    pub(crate) rental_daily_limit: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SynthesisLocalPolicy {
    pub(crate) result_slot_count: i32,
    pub(crate) extra_target_rate: u32,
    pub(crate) great_success_rate: u32,
    pub(crate) stimulator_weight: u32,
    pub(crate) stimulator_drop_rate: u32,
    pub(crate) high_rank_bonus_denominator: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SynthesisRecipe {
    pub(crate) id: i32,
    pub(crate) mana_cost: i32,
    pub(crate) costs: Vec<TutorialRecipeCost>,
    pub(crate) target: TutorialReward,
    pub(crate) bonus_rewards: Vec<TutorialReward>,
    pub(crate) support_character_ids: Vec<i32>,
    pub(crate) support_color_ids: Vec<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SynthesisCharacter {
    pub(crate) id: i32,
    pub(crate) trait_color_id: i32,
    pub(crate) support_color_id: i32,
    pub(crate) battle_trait_ids: Vec<i32>,
    pub(crate) equipment_trait_ids: Vec<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SynthesisIngredient {
    pub(crate) id: i32,
    pub(crate) item_type: i32,
    pub(crate) trait_color_id: i32,
    pub(crate) battle_trait_ids: Vec<i32>,
    pub(crate) equipment_trait_ids: Vec<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SynthesisTool {
    pub(crate) id: i32,
    pub(crate) easy_lottery_id: i32,
    #[serde(default)]
    pub(crate) trait_filter_ids: Vec<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SynthesisTrait {
    pub(crate) id: i32,
    pub(crate) disable_duplicate_assign: bool,
    #[serde(default)]
    pub(crate) filter_ids: Vec<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SynthesisWeightBonus {
    pub(crate) id: i32,
    pub(crate) weight_bonus: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct SynthesisTraitCandidate {
    pub(crate) id: i32,
    pub(crate) rank_weight_bonus: u32,
    pub(crate) active: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RewardRules {
    pub(crate) format: String,
    pub(crate) source_sha256: String,
    pub(crate) battle_missions: Vec<BattleMissionRule>,
    pub(crate) quests: Vec<RewardQuest>,
    pub(crate) episodes: Vec<RewardEpisode>,
    pub(crate) fixed_parties: Vec<RewardFixedParty>,
    pub(crate) reward_sets: Vec<TutorialRewardSet>,
    pub(crate) drop_reward_sets: Vec<DropRewardSet>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RewardQuest {
    pub(crate) id: i32,
    pub(crate) episode_id: i32,
    pub(crate) battle_id: Option<i32>,
    pub(crate) quest_type: i32,
    pub(crate) skippable_type: i32,
    pub(crate) stamina: i32,
    pub(crate) max_clear_count: i32,
    pub(crate) character_exp: i32,
    pub(crate) send_cleared_party: bool,
    pub(crate) battle_mission_ids: Vec<i32>,
    pub(crate) drop_reward_set_ids: Vec<i32>,
    pub(crate) score_ranks: Vec<ScoreRankDrops>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ScoreRankDrops {
    pub(crate) rank: i32,
    pub(crate) reward_set_ids: Vec<i32>,
    pub(crate) drop_reward_set_ids: Vec<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialBattleToolMix {
    pub(crate) id: i32,
    pub(crate) rank: i32,
    pub(crate) first_item_attack_attribute: i32,
    pub(crate) second_item_attack_attribute: i32,
    pub(crate) skill_id: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialShipTool {
    pub(crate) id: i32,
    pub(crate) skill_ids: Vec<i32>,
    pub(crate) usage_counts: Vec<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct BattleMissionRule {
    pub(crate) id: i32,
    pub(crate) kind: BattleMissionKind,
    pub(crate) turn_limit: Option<i32>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BattleMissionKind {
    Clear,
    NoIncapacitated,
    TurnLimit,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RewardEpisode {
    pub(crate) id: i32,
    pub(crate) max_daily_clear: i32,
    pub(crate) max_daily_clear_addition_gem_costs: Vec<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RewardFixedParty {
    pub(crate) id: i32,
    pub(crate) character_ids: Vec<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct DropRewardSet {
    pub(crate) id: i32,
    pub(crate) rolls: Vec<DropRewardRoll>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct DropRewardRoll {
    pub(crate) rate: u32,
    pub(crate) rewards: Vec<DropReward>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct DropReward {
    #[serde(rename = "type")]
    pub(crate) resource_type: i32,
    pub(crate) id: i32,
    pub(crate) min_quantity: i32,
    pub(crate) max_quantity: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SynthesisLottery {
    pub(crate) id: i32,
    pub(crate) trait_ids: Vec<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SynthesisMode {
    Execute,
    Bulk,
    Easy,
    Rental,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialMemoria {
    pub(crate) id: i32,
    pub(crate) status_buffs: Vec<TutorialMemoriaStatusBuff>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TutorialMemoriaStatusBuff {
    #[serde(rename = "type")]
    pub(crate) stat_type: i32,
    pub(crate) initial_values: Vec<i32>,
    pub(crate) growth_values: Vec<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct CharacterLevel {
    pub(crate) level: i32,
    pub(crate) exp: i32,
}
