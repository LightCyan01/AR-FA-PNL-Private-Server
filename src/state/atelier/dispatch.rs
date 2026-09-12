use super::prelude::*;

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply(
    proto: &ProtoRegistry,
    rules: &AtelierRules,
    synthesis: &SynthesisRules,
    home_rules: &home::HomeRules,
    resources: &mut DynamicMessage,
    changed: &mut DynamicMessage,
    response: &mut DynamicMessage,
    home: &mut home::HomeState,
    route: &str,
    request: &DynamicMessage,
    now: i64,
) -> Result<(), StateError> {
    for memoria in message_list(resources, "memorias") {
        if let Some(id) = i32_field(&memoria, "memoria_id") {
            home.memoria_history.insert(id);
        }
    }
    match route {
        "/atelier/research" => research(proto, rules, resources, changed, request, now),
        "/tool/convert" => tool_convert(
            proto, rules, home_rules, resources, changed, response, request, now,
        ),
        "/tool/lock" | "/tool/trait_rank_up" => {
            tool_update(proto, rules, resources, changed, route, request)
        }
        "/communication/story_release" | "/communication/story_clear" => communication(
            proto, rules, home_rules, resources, changed, response, route, request, now,
        ),
        "/illustrated_book/start" => collection(
            proto, rules, home_rules, resources, changed, response, home, request, now,
        ),
        "/memoria/enhance" => memoria_enhance(proto, rules, resources, changed, response, request),
        "/memoria/limit_break" => {
            memoria_limit_break(proto, rules, resources, changed, response, request)
        }
        "/memoria/lock" => memoria_lock(resources, changed, request),
        "/memoria/sell" => memoria_sell(
            proto, rules, home_rules, resources, changed, response, home, request, now,
        ),
        "/ship/create" => ship_create(proto, rules, synthesis, resources, changed, request, now),
        "/ship/bulk_update" | "/ship/ship_tools_set" => {
            ship_party(proto, rules, resources, changed, route, request)
        }
        "/ship/synthesize" => {
            ship_synthesize(proto, synthesis, resources, changed, response, request, now)
        }
        _ => Err(StateError::InvalidRequest),
    }
}
