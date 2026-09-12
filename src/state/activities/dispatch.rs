use super::prelude::*;

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply(
    proto: &ProtoRegistry,
    rules: &ActivityRules,
    tutorial_rules: &TutorialRules,
    atelier: &atelier::AtelierRules,
    characters: &CharacterRules,
    reward_rules: &RewardRules,
    home_rules: &home::HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    response: &mut DynamicMessage,
    saved: &mut ActivityState,
    route: &str,
    request: &DynamicMessage,
    now: i64,
) -> Result<(), StateError> {
    match route {
        "/quest/talk_event/prologue_skip"
        | "/quest/talk_event/main_story_episode_skip"
        | "/quest/talk_event/main_story_season_skip" => story_skip(
            proto,
            rules,
            tutorial_rules,
            atelier,
            characters,
            reward_rules,
            home_rules,
            resources,
            changed,
            saved,
            route,
            now,
        ),
        "/mod_timeline/release" => {
            let id = i32_field(request, "mod_timeline_id").ok_or(StateError::InvalidRequest)?;
            let spec = row(rules, "mod_timeline", id)?;
            eligible(spec, resources, now)?;
            if message_list(resources, "mod_timeline_states")
                .iter()
                .any(|state| i32_field(state, "mod_timeline_id") == Some(id))
            {
                return Err(StateError::InvalidRequest);
            }
            for cost in values(spec, "costs") {
                let cost: TutorialRecipeCost = serde_json::from_value(cost.clone())
                    .map_err(|e| StateError::MasterData(e.to_string()))?;
                shop::pay(
                    proto,
                    resources,
                    changed,
                    cost.resource_type,
                    cost.id,
                    cost.quantity,
                )?;
            }
            let mut state = empty_message(proto, "blend.model.ModTimelineState")?;
            state.set_field_by_name("mod_timeline_id", Value::I32(id));
            state.set_field_by_name("received_at", Value::Message(timestamp(proto, now)?));
            save(
                resources,
                changed,
                "mod_timeline_states",
                "mod_timeline_id",
                state,
            );
            Ok(())
        }
        "/present/execute" => present_execute(
            proto, rules, home_rules, resources, changed, response, request, now,
        ),
        "/dish/order" => dish(
            proto, rules, home_rules, resources, changed, response, request, now,
        ),
        "/character_story/clear" => character_story(
            proto, rules, home_rules, resources, changed, response, request, now,
        ),
        route if route.starts_with("/quest/street/") => street(
            proto,
            rules,
            tutorial_rules,
            home_rules,
            resources,
            changed,
            route,
            request,
            now,
        ),
        route if route.starts_with("/expedition/") => expedition(
            proto, rules, home_rules, resources, changed, response, route, request, now,
        ),
        route if route.starts_with("/exploration/") => exploration(
            proto,
            rules,
            tutorial_rules,
            atelier,
            characters,
            reward_rules,
            home_rules,
            resources,
            changed,
            response,
            saved,
            route,
            request,
            now,
        ),
        route if route.starts_with("/house_building/") => housing(
            proto, rules, atelier, home_rules, resources, changed, response, saved, route, request,
            now,
        ),
        _ => Err(StateError::InvalidRequest),
    }
}

pub(crate) fn is_route(route: &str) -> bool {
    matches!(
        route,
        "/quest/talk_event/prologue_skip"
            | "/quest/talk_event/main_story_episode_skip"
            | "/quest/talk_event/main_story_season_skip"
    ) || route == "/mod_timeline/release"
        || route == "/present/execute"
        || route == "/dish/order"
        || route.starts_with("/character_story/")
        || route.starts_with("/quest/street/")
        || route.starts_with("/expedition/")
        || route.starts_with("/exploration/")
        || route.starts_with("/house_building/")
}
