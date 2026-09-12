// Request routing metadata and authenticated headers.

use std::time::Instant;

use hyper::http::{header, Method, Request, Response, StatusCode};
use prost::Message;
use prost_reflect::DynamicMessage;
use serde_json::json;

use super::{StorageDispatchError, MAX_REQUEST_ID_BYTES};
use crate::{
    state::StateError,
    transport::{encrypt_frame, full, response_prefix, AppBody},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RequestDisposition {
    MasterData,
    Post,
    MethodNotAllowed,
}

pub(super) fn classify_request(method: &Method, route: &str) -> RequestDisposition {
    if *method == Method::GET && route.starts_with("/master_data/") {
        RequestDisposition::MasterData
    } else if *method == Method::POST {
        RequestDisposition::Post
    } else {
        RequestDisposition::MethodNotAllowed
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RouteHandler {
    Home,
    Status,
    RefundCountryCode,
    SignIn,
    UserDelete,
    UserLogin,
    ExternalPurchaseReceive,
    QuestTalkEventFinish,
    QuestBattleStart { exploration: bool },
    SpecializedBattleStart(crate::state::BattleStartMode),
    BattleAttack,
    BattleFinish,
    BattleResume,
    BattleRetire,
    SynthesisExecute,
    SynthesisCombinationRanking,
    PartyUpdate { tools_only: bool },
    GachaList,
    GachaExecute,
    GachaWishListSet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RouteSpec {
    pub(crate) request: Option<&'static str>,
    pub(crate) response: Option<&'static str>,
    pub(super) handler: RouteHandler,
}

impl RouteSpec {
    const fn protobuf(
        handler: RouteHandler,
        request: &'static str,
        response: &'static str,
    ) -> Self {
        Self {
            request: Some(request),
            response: Some(response),
            handler,
        }
    }
}

pub(crate) fn route_spec(route: &str) -> Option<RouteSpec> {
    use crate::state::BattleStartMode;

    Some(match route {
        "/status" => RouteSpec {
            request: None,
            response: None,
            handler: RouteHandler::Status,
        },
        "/refund_info/get_country_code" => RouteSpec::protobuf(
            RouteHandler::RefundCountryCode,
            "google.protobuf.Empty",
            "blend.api.RefundInfoGetCountryCodeResponse",
        ),
        "/auth/sign_in" => RouteSpec::protobuf(
            RouteHandler::SignIn,
            "blend.api.AuthSignInRequest",
            "blend.api.AuthSignInResponse",
        ),
        "/user/delete" => RouteSpec::protobuf(
            RouteHandler::UserDelete,
            "google.protobuf.Empty",
            "google.protobuf.Empty",
        ),
        "/user/log_in" => RouteSpec::protobuf(
            RouteHandler::UserLogin,
            "google.protobuf.Empty",
            "blend.api.UserLogInResponse",
        ),
        "/external_purchase/receive" => RouteSpec::protobuf(
            RouteHandler::ExternalPurchaseReceive,
            "google.protobuf.Empty",
            "blend.api.ExternalPurchaseReceiveResponse",
        ),
        "/quest/talk_event/finish" => RouteSpec::protobuf(
            RouteHandler::QuestTalkEventFinish,
            "blend.api.QuestTalkEventFinishRequest",
            "blend.api.QuestTalkEventFinishResponse",
        ),
        "/quest/battle/start" => RouteSpec::protobuf(
            RouteHandler::QuestBattleStart { exploration: false },
            "blend.api.QuestBattleStartRequest",
            "blend.api.BattleStartResponse",
        ),
        "/exploration/battle_start" => RouteSpec::protobuf(
            RouteHandler::QuestBattleStart { exploration: true },
            "blend.api.ExplorationBattleStartRequest",
            "blend.api.BattleStartResponse",
        ),
        "/quest/battle/rental_party_start" => RouteSpec::protobuf(
            RouteHandler::SpecializedBattleStart(BattleStartMode::Rental),
            "blend.api.QuestBattleRentalPartyStartRequest",
            "blend.api.BattleStartResponse",
        ),
        "/quest/battle/solo_raid_battle_start" => RouteSpec::protobuf(
            RouteHandler::SpecializedBattleStart(BattleStartMode::SoloRaid),
            "blend.api.QuestBattleSoloRaidBattleStartRequest",
            "blend.api.BattleStartResponse",
        ),
        "/quest/battle/total_battle_start" => RouteSpec::protobuf(
            RouteHandler::SpecializedBattleStart(BattleStartMode::Total),
            "blend.api.QuestBattleTotalBattleStartRequest",
            "blend.api.BattleStartResponse",
        ),
        "/gacha/battle_start" => RouteSpec::protobuf(
            RouteHandler::SpecializedBattleStart(BattleStartMode::Gacha),
            "blend.api.GachaBattleStartRequest",
            "blend.api.BattleStartResponse",
        ),
        "/battle/attack" => RouteSpec::protobuf(
            RouteHandler::BattleAttack,
            "blend.api.BattleAttackRequest",
            "blend.api.BattleAttackResponse",
        ),
        "/battle/finish" => RouteSpec::protobuf(
            RouteHandler::BattleFinish,
            "google.protobuf.Empty",
            "blend.api.BattleFinishResponse",
        ),
        "/battle/resume" => RouteSpec::protobuf(
            RouteHandler::BattleResume,
            "google.protobuf.Empty",
            "blend.api.BattleResumeResponse",
        ),
        "/battle/retire" => RouteSpec::protobuf(
            RouteHandler::BattleRetire,
            "google.protobuf.Empty",
            "blend.api.ChangedResourcesResponse",
        ),
        "/synthesis/execute" => RouteSpec::protobuf(
            RouteHandler::SynthesisExecute,
            "blend.api.SynthesisExecuteRequest",
            "blend.api.SynthesisExecuteResponse",
        ),
        "/synthesis/bulk_execute" => RouteSpec::protobuf(
            RouteHandler::SynthesisExecute,
            "blend.api.SynthesisBulkExecuteRequest",
            "blend.api.SynthesisExecuteResponse",
        ),
        "/synthesis/execute_easy" => RouteSpec::protobuf(
            RouteHandler::SynthesisExecute,
            "blend.api.SynthesisExecuteEasyRequest",
            "blend.api.SynthesisExecuteEasyResponse",
        ),
        "/synthesis/execute_rental" => RouteSpec::protobuf(
            RouteHandler::SynthesisExecute,
            "blend.api.SynthesisExecuteRentalRequest",
            "blend.api.SynthesisExecuteResponse",
        ),
        "/synthesis/combination_ranking" => RouteSpec::protobuf(
            RouteHandler::SynthesisCombinationRanking,
            "blend.api.SynthesisCombinationRankingRequest",
            "blend.api.SynthesisCombinationRankingResponse",
        ),
        "/party/bulk_update" => RouteSpec::protobuf(
            RouteHandler::PartyUpdate { tools_only: false },
            "blend.api.PartyBulkUpdateRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/party/battle_tools_set" => RouteSpec::protobuf(
            RouteHandler::PartyUpdate { tools_only: true },
            "blend.api.PartyBattleToolsSetRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/gacha/list" => RouteSpec::protobuf(
            RouteHandler::GachaList,
            "blend.api.GachaListRequest",
            "blend.api.GachaListResponse",
        ),
        "/gacha/execute" => RouteSpec::protobuf(
            RouteHandler::GachaExecute,
            "blend.api.GachaExecuteRequest",
            "blend.api.GachaExecuteResponse",
        ),
        "/gacha/step_up_execute" => RouteSpec::protobuf(
            RouteHandler::GachaExecute,
            "blend.api.GachaStepUpExecuteRequest",
            "blend.api.GachaStepUpExecuteResponse",
        ),
        "/gacha/wish_list_set" => RouteSpec::protobuf(
            RouteHandler::GachaWishListSet,
            "blend.api.GachaWishListSetRequest",
            "blend.api.GachaWishListSetResponse",
        ),
        _ => {
            let (request, response) = home_wire_types(route)?;
            RouteSpec::protobuf(RouteHandler::Home, request, response)
        }
    })
}

fn home_wire_types(route: &str) -> Option<(&'static str, &'static str)> {
    Some(match route {
        "/exploration/skip" => (
            "blend.api.ExplorationSkipRequest",
            "blend.api.ExplorationSkipResponse",
        ),
        "/quest/talk_event/main_story_episode_skip"
        | "/quest/talk_event/main_story_season_skip"
        | "/quest/talk_event/prologue_skip" => (
            "google.protobuf.Empty",
            "blend.api.ChangedResourcesResponse",
        ),
        "/event/revive" => (
            "blend.api.EventReviveRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/event/damage_contest" => (
            "blend.api.EventDamageContestRequest",
            "blend.api.EventDamageContestResponse",
        ),
        "/event/legend_challenge" => (
            "blend.api.EventLegendChallengeRequest",
            "blend.api.EventLegendChallengeResponse",
        ),
        "/solo_raid/reset" => (
            "blend.api.SoloRaidResetRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/total_battle/reset_panel" => (
            "blend.api.TotalBattleResetPanelRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/quest/score_rank_first_reward_receive" => (
            "blend.api.QuestScoreRankFirstRewardReceiveRequest",
            "blend.api.QuestScoreRankFirstRewardReceiveResponse",
        ),
        "/mod_timeline/release" => (
            "blend.api.ModTimelineReleaseRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/emblem/acquisition_drama" => (
            "blend.api.EmblemAcquisitionDramaRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/total_battle/achieve_line_drama" => (
            "blend.api.TotalBattleAchieveLineDramaRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/user/update_birthdate" => (
            "blend.api.UserUpdateBirthdateRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/user/update_language" => (
            "blend.api.UserUpdateLanguageRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/profile/update_memo" => (
            "blend.api.ProfileUpdateMemoRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/profile/update_favorite_battle_tools" => (
            "blend.api.ProfileUpdateFavoriteBattleToolsRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/profile/update_chara_home_favorite_character_list" => (
            "blend.api.ProfileUpdateCharaHomeFavoriteCharacterListRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/shop/gem_list" => ("google.protobuf.Empty", "blend.api.ShopGemListResponse"),
        "/shop/purchase" => (
            "blend.api.ShopPurchaseRequest",
            "blend.api.ShopPurchaseResponse",
        ),
        "/shop/random_shop/list" => (
            "blend.api.ShopRandomShopListRequest",
            "blend.api.ShopRandomShopListResponse",
        ),
        "/shop/random_shop/purchase" => (
            "blend.api.ShopRandomShopPurchaseRequest",
            "blend.api.ShopRandomShopPurchaseResponse",
        ),
        "/shop/random_shop/refresh" => (
            "blend.api.ShopRandomShopRefreshRequest",
            "blend.api.ShopRandomShopRefreshResponse",
        ),
        "/shop/piece_exchange" => (
            "blend.api.ShopPieceExchangeRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/shop/box_gacha/execute" => (
            "blend.api.ShopBoxGachaExecuteRequest",
            "blend.api.ShopBoxGachaExecuteResponse",
        ),
        "/shop/receive_first_purchase_bonus" => (
            "google.protobuf.Empty",
            "blend.api.ShopReceiveFirstPurchaseBonusResponse",
        ),
        "/shop/wheel" => ("blend.api.ShopWheelRequest", "blend.api.ShopWheelResponse"),
        "/purchase/session_start" => (
            "blend.api.PurchaseSessionStartRequest",
            "blend.api.PurchaseSessionStartResponse",
        ),
        "/purchase/session_publish" => (
            "blend.api.PurchaseSessionPublishRequest",
            "google.protobuf.Empty",
        ),
        "/purchase/verify" => (
            "blend.api.PurchaseVerifyRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/daily_pass/receive" => (
            "blend.api.DailyPassReceiveRequest",
            "blend.api.DailyPassReceiveResponse",
        ),
        "/daily_pass/bulk_receive" => (
            "blend.api.DailyPassBulkReceiveRequest",
            "blend.api.DailyPassBulkReceiveResponse",
        ),
        "/growth_pack/purchase" => (
            "blend.api.GrowthPackPurchaseRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/growth_pack/point_purchase" => (
            "blend.api.GrowthPackPointPurchaseRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/growth_pack/receive" => (
            "blend.api.GrowthPackReceiveRequest",
            "blend.api.GrowthPackReceiveResponse",
        ),
        "/growth_pack/bulk_receive" => (
            "blend.api.GrowthPackBulkReceiveRequest",
            "blend.api.GrowthPackBulkReceiveResponse",
        ),
        "/special_offer/purchase" => (
            "blend.api.SpecialOfferPurchaseRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/item/bundle_open" => (
            "blend.api.ItemBundleOpenRequest",
            "blend.api.ItemBundleOpenResponse",
        ),
        "/item_challenge/execute" => (
            "blend.api.ItemChallengeExecuteRequest",
            "blend.api.ItemChallengeExecuteResponse",
        ),
        "/item_challenge/reward_receive" => (
            "blend.api.ItemChallengeRewardReceiveRequest",
            "blend.api.ItemChallengeRewardReceiveResponse",
        ),
        "/multi_mission/status" => (
            "blend.api.MultiMissionStatusRequest",
            "blend.api.MultiMissionStatusResponse",
        ),
        "/multi_mission/receive" => (
            "blend.api.MultiMissionReceiveRequest",
            "blend.api.MultiMissionReceiveResponse",
        ),
        "/quest/battle/skip" => (
            "blend.api.QuestBattleSkipRequest",
            "blend.api.QuestBattleSkipResponse",
        ),
        "/quest/daily_clear_add" => (
            "blend.api.QuestDailyClearAddRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/quest/cleared_party_list" => (
            "blend.api.QuestClearedPartyListRequest",
            "blend.api.QuestClearedPartyListResponse",
        ),
        "/rental_party/battle_tools_set" => (
            "blend.api.RentalPartyBattleToolsSetRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/rental_party/character_equip" => (
            "blend.api.RentalPartyCharacterEquipRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/rental_party/character_bulk_equip" => (
            "blend.api.RentalPartyCharacterBulkEquipRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/rental_party/bulk_update" => (
            "blend.api.RentalPartyBulkUpdateRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/atelier/research" => (
            "blend.api.AtelierResearchRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/tool/convert" => (
            "blend.api.ToolConvertRequest",
            "blend.api.ToolConvertResponse",
        ),
        "/tool/lock" => (
            "blend.api.ToolLockRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/tool/trait_rank_up" => (
            "blend.api.ToolTraitRankUpRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/communication/story_release" => (
            "blend.api.CommunicationStoryReleaseRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/communication/story_clear" => (
            "blend.api.CommunicationStoryClearRequest",
            "blend.api.CommunicationStoryClearResponse",
        ),
        "/illustrated_book/start" => (
            "blend.api.IllustratedBookStartRequest",
            "blend.api.IllustratedBookStartResponse",
        ),
        "/memoria/enhance" => (
            "blend.api.MemoriaEnhanceRequest",
            "blend.api.MemoriaEnhanceResponse",
        ),
        "/memoria/limit_break" => (
            "blend.api.MemoriaLimitBreakRequest",
            "blend.api.MemoriaLimitBreakResponse",
        ),
        "/memoria/lock" => (
            "blend.api.MemoriaLockRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/memoria/sell" => (
            "blend.api.MemoriaSellRequest",
            "blend.api.MemoriaSellResponse",
        ),
        "/ship/create" => (
            "blend.api.ShipCreateRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/ship/bulk_update" => (
            "blend.api.ShipBulkUpdateRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/ship/ship_tools_set" => (
            "blend.api.ShipShipToolsSetRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/ship/synthesize" => (
            "blend.api.ShipSynthesizeRequest",
            "blend.api.ShipSynthesizeResponse",
        ),
        "/equipment_preset/bulk_set" => (
            "blend.api.EquipmentPresetBulkSetRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/equipment_preset/equip" => (
            "blend.api.EquipmentPresetEquipRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/equipment_preset/memoria_set" => (
            "blend.api.EquipmentPresetMemoriaSetRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/equipment_preset/update_name" => (
            "blend.api.EquipmentPresetUpdateNameRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/character/enhance" => (
            "blend.api.CharacterEnhanceRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/character/rarity_enhance" => (
            "blend.api.CharacterRarityEnhanceRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/character/growboard_page_release" => (
            "blend.api.CharacterGrowboardPageReleaseRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/character/growboard_bulk_release" => (
            "blend.api.CharacterGrowboardBulkReleaseRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/character/equip" => (
            "blend.api.CharacterEquipRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/character/memoria_set" => (
            "blend.api.CharacterMemoriaSetRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/character/enhancement_reset" => (
            "blend.api.CharacterEnhancementResetRequest",
            "blend.api.CharacterEnhancementResetResponse",
        ),
        "/character/level_limit_release" => (
            "blend.api.CharacterLevelLimitReleaseRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/character/skill_evolve" => (
            "blend.api.CharacterSkillEvolveRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/character/skill_lock_release" => (
            "blend.api.CharacterSkillLockReleaseRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/character/skin_set" => (
            "blend.api.CharacterSkinSetRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/profile/update_favorite_character" => (
            "blend.api.ProfileUpdateFavoriteCharacterRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/profile/update_favorite_party" => (
            "blend.api.ProfileUpdateFavoritePartyRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/mana/purchase" => (
            "blend.api.ManaPurchaseRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/mana/use_item" => (
            "blend.api.ManaUseItemRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/stamina/purchase" => (
            "blend.api.StaminaPurchaseRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/stamina/use_item" => (
            "blend.api.StaminaUseItemRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/stamina/use_spare_stamina" => (
            "blend.api.StaminaUseSpareStaminaRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/login_bonus/receive" => (
            "google.protobuf.Empty",
            "blend.api.LoginBonusReceiveResponse",
        ),
        "/web_session/token" => ("google.protobuf.Empty", "blend.api.WebSessionTokenResponse"),
        "/tutorial/progress" => (
            "google.protobuf.Empty",
            "blend.api.ChangedResourcesResponse",
        ),
        "/event/top" => ("google.protobuf.Empty", "blend.api.EventTopResponse"),
        "/present/execute" => (
            "blend.api.PresentExecuteRequest",
            "blend.api.PresentExecuteResponse",
        ),
        "/mission/receive" => (
            "blend.api.MissionReceiveRequest",
            "blend.api.MissionReceiveResponse",
        ),
        "/mission/count_reward_receive" => (
            "blend.api.MissionCountRewardReceiveRequest",
            "blend.api.MissionCountRewardReceiveResponse",
        ),
        "/mission/event_tab_reward_receive" => (
            "blend.api.MissionEventTabRewardReceiveRequest",
            "blend.api.MissionEventTabRewardReceiveResponse",
        ),
        "/mission/navigation_task_proceed" => (
            "blend.api.MissionNavigationTaskProceedRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/dish/order" => ("blend.api.DishOrderRequest", "blend.api.DishOrderResponse"),
        "/house_building/start" => (
            "blend.api.HouseBuildingStartRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/house_building/enhance" => (
            "blend.api.HouseBuildingEnhanceRequest",
            "blend.api.HouseBuildingEnhanceResponse",
        ),
        "/house_building/reset" => (
            "blend.api.HouseBuildingResetRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/house_building/reward_receive" => (
            "blend.api.HouseBuildingRewardReceiveRequest",
            "blend.api.HouseBuildingRewardReceiveResponse",
        ),
        "/house_building/count_reward_receive" => (
            "blend.api.HouseBuildingCountRewardReceiveRequest",
            "blend.api.HouseBuildingCountRewardReceiveResponse",
        ),
        "/house_building/rent" => (
            "blend.api.HouseBuildingRentRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/house_building/rental_reward_receive" => (
            "blend.api.HouseBuildingRentalRewardReceiveRequest",
            "blend.api.HouseBuildingRentalRewardReceiveResponse",
        ),
        "/character_story/clear" => (
            "blend.api.CharacterStoryClearRequest",
            "blend.api.CharacterStoryClearResponse",
        ),
        "/expedition/start" => (
            "blend.api.ExpeditionStartRequest",
            "blend.api.ExpeditionStartResponse",
        ),
        "/exploration/start" => (
            "blend.api.ExplorationStartRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/exploration/explore" => (
            "blend.api.ExplorationExploreRequest",
            "blend.api.ExplorationExploreResponse",
        ),
        "/exploration/finish" => (
            "blend.api.ExplorationFinishRequest",
            "blend.api.ExplorationFinishResponse",
        ),
        "/exploration/retire" => (
            "blend.api.ExplorationRetireRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/exploration/update_party" => (
            "blend.api.ExplorationUpdatePartyRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/expedition/reward_receive" => (
            "google.protobuf.Empty",
            "blend.api.ExpeditionRewardReceiveResponse",
        ),
        "/quest/street/start" => (
            "blend.api.QuestStreetStartRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/quest/street/move" => (
            "blend.api.QuestStreetMoveRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/quest/street/talk" => (
            "blend.api.QuestStreetTalkRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/mail/list" => ("google.protobuf.Empty", "blend.api.MailListResponse"),
        "/mail/open" => ("blend.api.MailOpenRequest", "blend.api.MailOpenResponse"),
        "/mail/delete" => (
            "blend.api.MailDeleteRequest",
            "blend.api.MailDeleteResponse",
        ),
        "/recipe/learn" => ("google.protobuf.Empty", "blend.api.RecipeLearnResponse"),
        "/recipe/count_reward_receive" => (
            "blend.api.RecipeCountRewardReceiveRequest",
            "blend.api.RecipeCountRewardReceiveResponse",
        ),
        "/recipe/favorite" => (
            "blend.api.RecipeFavoriteRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/profile/update_name" => (
            "blend.api.ProfileUpdateNameRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/profile/update_selected_home_id" => (
            "blend.api.ProfileUpdateSelectedHomeIdRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/character/bulk_set" => (
            "blend.api.CharacterBulkSetRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        "/chara_home/register" => (
            "blend.api.CharaHomeRegisterRequest",
            "blend.api.ChangedResourcesResponse",
        ),
        _ => return None,
    })
}

#[derive(Clone, Default)]
pub(super) struct RequestHeaders {
    pub(super) session_token: Option<String>,
    pub(super) user_id: Option<i64>,
    pub(super) asset_version: Option<String>,
    pub(super) master_data_version: Option<String>,
    pub(super) request_id: Option<String>,
    pub(super) invalid_request_id: bool,
}

impl RequestHeaders {
    pub(super) fn from_request<B>(request: &Request<B>) -> Self {
        let request_id_header = request.headers().get("x-request-id");
        let request_id = header_string(request, "x-request-id");
        let invalid_request_id = match (request_id_header, request_id.as_deref()) {
            (None, _) => false,
            (Some(_), Some(value)) => value.is_empty() || value.len() > MAX_REQUEST_ID_BYTES,
            (Some(_), None) => true,
        };
        Self {
            session_token: header_string(request, "x-session-token"),
            user_id: header_string(request, "x-user-id").and_then(|value| value.parse().ok()),
            asset_version: header_string(request, "x-asset-version"),
            master_data_version: header_string(request, "x-master-data-version"),
            request_id,
            invalid_request_id,
        }
    }
}

pub(super) fn header_string<B>(request: &Request<B>, name: &str) -> Option<String> {
    request
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .filter(|value| name != "x-request-id" || value.len() <= MAX_REQUEST_ID_BYTES)
        .map(ToOwned::to_owned)
}
pub(super) fn proto_response(
    message: &DynamicMessage,
    request_prefix: Option<u8>,
    policy: &str,
) -> Response<AppBody> {
    let started = Instant::now();
    let response = encrypted_bytes(message.encode_to_vec(), request_prefix, policy);
    tracing::debug!(
        event = "response_serialization",
        elapsed_us = started.elapsed().as_micros() as u64,
        redacted = true
    );
    response
}
pub(super) fn empty_encrypted_response() -> Response<AppBody> {
    let mut response = Response::new(full(Vec::<u8>::new()));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/octet-stream"),
    );
    response.headers_mut().insert(
        header::CONTENT_LENGTH,
        header::HeaderValue::from_static("0"),
    );
    response
}

pub(super) fn encrypted_bytes(
    plaintext: Vec<u8>,
    request_prefix: Option<u8>,
    policy: &str,
) -> Response<AppBody> {
    let prefix = response_prefix(policy, request_prefix);
    let bytes = match encrypt_frame(prefix, &plaintext) {
        Ok(bytes) => bytes,
        Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "server_error"),
    };
    let length = bytes.len();
    let mut response = Response::new(full(bytes));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/octet-stream"),
    );
    response.headers_mut().insert(
        header::CONTENT_LENGTH,
        header::HeaderValue::from_str(&length.to_string()).unwrap(),
    );
    response
}
pub(super) fn json_response(status: StatusCode, value: serde_json::Value) -> Response<AppBody> {
    let bytes =
        serde_json::to_vec(&value).unwrap_or_else(|_| br#"{"code":"server_error"}"#.to_vec());
    let length = bytes.len();
    let mut response = Response::new(full(bytes));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/json; charset=utf-8"),
    );
    response.headers_mut().insert(
        header::CONTENT_LENGTH,
        header::HeaderValue::from_str(&length.to_string()).unwrap(),
    );
    response
}
pub(super) fn json_error(status: StatusCode, code: &str) -> Response<AppBody> {
    json_response(status, json!({ "code": code }))
}
pub(super) fn storage_dispatch_error(error: StorageDispatchError) -> Response<AppBody> {
    match error {
        StorageDispatchError::Busy => json_error(StatusCode::SERVICE_UNAVAILABLE, "storage_busy"),
        StorageDispatchError::Join(error) => {
            tracing::error!(event = "storage_dispatch_failed", error = %error, redacted = true);
            json_error(StatusCode::INTERNAL_SERVER_ERROR, "server_error")
        }
    }
}
pub(super) fn state_error(error: StateError) -> Response<AppBody> {
    match error {
        StateError::InvalidGrant | StateError::InvalidSession => {
            json_error(StatusCode::UNAUTHORIZED, "unauthorized")
        }
        StateError::InvalidRequest | StateError::Credentials => {
            json_error(StatusCode::BAD_REQUEST, "invalid_format")
        }
        StateError::OutOfSchedule => json_error(StatusCode::BAD_REQUEST, "out_of_schedule"),
        StateError::Storage(crate::storage::StorageError::RequestConflict) => {
            json_error(StatusCode::CONFLICT, "invalid_format")
        }
        StateError::Storage(crate::storage::StorageError::ActiveBattleExists) => {
            json_error(StatusCode::CONFLICT, "battle_already_active")
        }
        StateError::Storage(crate::storage::StorageError::BattleInactive) => {
            json_error(StatusCode::CONFLICT, "battle_not_active")
        }
        StateError::Storage(crate::storage::StorageError::BattleRejected) => {
            json_error(StatusCode::BAD_REQUEST, "invalid_format")
        }
        StateError::Storage(crate::storage::StorageError::GachaLimit) => {
            json_error(StatusCode::BAD_REQUEST, "gacha_limit_reached")
        }
        StateError::Storage(crate::storage::StorageError::NotFound) => {
            json_error(StatusCode::NOT_FOUND, "user_not_found")
        }
        _ => json_error(StatusCode::INTERNAL_SERVER_ERROR, "server_error"),
    }
}
