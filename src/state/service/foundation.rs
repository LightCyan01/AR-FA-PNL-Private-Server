use super::prelude::*;

pub(crate) const TUTORIAL_STEP_FIRST_SYNTHESIS: i32 = 50;
pub(crate) const TUTORIAL_STEP_SECOND_SYNTHESIS: i32 = 100;
pub(crate) const TUTORIAL_STEP_MEMORIA_EQUIPPED: i32 = 200;
pub(crate) const TUTORIAL_STEP_GACHA_COMPLETE: i32 = 500;

#[derive(Clone)]
pub struct State {
    pub store: Store,
    pub proto: ProtoRegistry,
    pub config: LoadedConfig,
    pub(crate) master_data: DynamicMessage,
    pub(crate) fresh_rules: FreshStateRules,
    pub(crate) tutorial_rules: TutorialRules,
    pub(crate) synthesis_rules: SynthesisRules,
    pub(crate) reward_rules: RewardRules,
    pub(crate) home_rules: home::HomeRules,
    pub(crate) atelier_rules: atelier::AtelierRules,
    pub(crate) activity_rules: activities::ActivityRules,
    pub(crate) shop_rules: shop::ShopRules,
    pub(crate) character_rules: &'static CharacterRules,
}
