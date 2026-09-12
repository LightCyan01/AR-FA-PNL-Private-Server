use super::prelude::*;

#[allow(clippy::too_many_arguments)]
pub(crate) fn reduce_home_inner(
    proto: &ProtoRegistry,
    rules: &HomeRules,
    atelier_rules: &atelier::AtelierRules,
    activity_rules: &activities::ActivityRules,
    gameplay_rules: &TutorialRules,
    synthesis_rules: &SynthesisRules,
    characters: &CharacterRules,
    reward_rules: &RewardRules,
    shop_rules: &shop::ShopRules,
    master_data: &DynamicMessage,
    payment_provider_url: &str,
    mut resources: DynamicMessage,
    home: &mut HomeState,
    route: &str,
    request: &DynamicMessage,
    response_name: &str,
    now: i64,
) -> Result<GameplayCommit, StateError> {
    let before = resources.clone();
    if home.initialized_at == 0 {
        home.initialized_at = now;
    }
    let mut changed = empty_message(proto, "blend.model.Resources")?;
    let old_status = status_message(&resources)?;
    activities::refresh_dishes(proto, activity_rules, &mut resources, now)?;
    let mut status = status_message(&resources)?;
    let rank = i32_field(&status, "rank").unwrap_or(1);
    let stamina_limit = rules
        .user_ranks
        .iter()
        .find(|row| row.id == rank)
        .ok_or(StateError::InvalidRequest)?
        .stamina;
    energy::refresh(proto, &rules.energy, &mut status, stamina_limit, now)?;
    if status != old_status {
        resources.set_field_by_name("status", Value::Message(status.clone()));
        changed.set_field_by_name("status", Value::Message(status));
    }
    // Capture owned memoria before a sell/consume mutation, and before mission claims.
    // The set is persisted with Home state in the same resource transaction.
    home.memoria_history.extend(
        message_list(&resources, "memorias")
            .iter()
            .filter_map(|row| i32_field(row, "memoria_id")),
    );
    advance_missions(
        proto,
        rules,
        &mut resources,
        &mut changed,
        now,
        Some(("memoria_collection", home.memoria_history.len() as i32)),
    )?;
    let mut response = empty_message(proto, response_name)?;
    match route {
        "/local/home_refresh" | "/event/top" => {}
        "/user/update_language" => {
            home.language = Some(
                i32_field(request, "language")
                    .filter(|value| (1..=4).contains(value))
                    .ok_or(StateError::InvalidRequest)?,
            );
        }
        route if modes::is_route(route) => modes::apply(
            proto,
            gameplay_rules,
            activity_rules,
            rules,
            &mut resources,
            &mut changed,
            &mut response,
            route,
            request,
            now,
        )?,
        "/mana/purchase"
        | "/mana/use_item"
        | "/stamina/purchase"
        | "/stamina/use_item"
        | "/stamina/use_spare_stamina" => {
            energy::apply(
                proto,
                &rules.energy,
                &mut resources,
                &mut changed,
                route,
                request,
                now,
            )?;
        }
        "/tutorial/progress" => {
            if (TUTORIAL_STEP_HOME_READY..900).contains(&tutorial_step(&resources)) {
                let mut status = status_message(&resources)?;
                status.set_field_by_name("tutorial_step", Value::I32(900));
                resources.set_field_by_name("status", Value::Message(status.clone()));
                changed.set_field_by_name("status", Value::Message(status));
            }
        }
        "/multi_mission/status" => {
            let event_id = i32_field(request, "event_id").ok_or(StateError::InvalidRequest)?;
            multi_mission_status(proto, rules, &resources, &mut response, event_id, now)?;
        }
        "/multi_mission/receive" => {
            let step_id =
                i32_field(request, "multi_mission_step_id").ok_or(StateError::InvalidRequest)?;
            receive_multi_mission(
                proto,
                rules,
                &mut resources,
                &mut changed,
                &mut response,
                step_id,
                now,
            )?;
        }
        "/login_bonus/receive" => {
            let bonuses = login_bonus(proto, rules, &mut resources, &mut changed, home, now)?;
            response.set_field_by_name("login_bonuses", Value::List(bonuses));
        }
        "/mission/receive" => {
            let (rewards, step_rewards) = claim_missions(
                proto,
                rules,
                &mut resources,
                &mut changed,
                home,
                request,
                now,
            )?;
            response.set_field_by_name("rewards", Value::List(rewards));
            response.set_field_by_name("step_rewards", Value::List(step_rewards));
        }
        "/mission/count_reward_receive" => {
            let category = i32_field(request, "category").ok_or(StateError::InvalidRequest)?;
            if !rules
                .mission_count_rewards
                .iter()
                .any(|r| r.category == category && in_period(r.start_at, r.end_at, now))
            {
                return Err(StateError::OutOfSchedule);
            }
            response.set_field_by_name(
                "rewards",
                Value::List(claim_counts(
                    proto,
                    rules,
                    &mut resources,
                    &mut changed,
                    category,
                    now,
                )?),
            );
        }
        "/mission/event_tab_reward_receive" => {
            let id = i32_field(request, "event_tab_id").ok_or(StateError::InvalidRequest)?;
            let tab = rules
                .mission_event_tabs
                .iter()
                .find(|r| r.id == id && in_period(r.start_at, r.end_at, now))
                .ok_or(StateError::OutOfSchedule)?;
            let rule = tab
                .count_reward
                .as_ref()
                .ok_or(StateError::InvalidRequest)?;
            let count: usize = rules
                .missions
                .iter()
                .filter(|r| r.event_tab_id == Some(id) && mission_active(rules, r, now))
                .map(|r| received(&resources, r.id))
                .sum();
            let mut value = message_list(&resources, "mission_event_tab_reward_states")
                .into_iter()
                .find(|r| i32_field(r, "event_tab_id") == Some(id))
                .unwrap_or(empty_message(
                    proto,
                    "blend.model.MissionEventTabRewardState",
                )?);
            let mut index = i32_field(&value, "received_step_count").unwrap_or(0).max(0) as usize;
            let mut awarded = Vec::new();
            while let Some(step) = rule
                .steps
                .get(index)
                .filter(|s| s.count as usize <= count && in_period(s.start_at, None, now))
            {
                awarded.extend(grant(
                    proto,
                    rules,
                    &mut resources,
                    &mut changed,
                    rewards(rules, step.reward_set_id)?,
                    now,
                )?);
                index += 1;
            }
            value.set_field_by_name("event_tab_id", Value::I32(id));
            value.set_field_by_name("received_step_count", Value::I32(index as i32));
            put(
                &mut resources,
                "mission_event_tab_reward_states",
                "event_tab_id",
                value.clone(),
            );
            put(
                &mut changed,
                "mission_event_tab_reward_states",
                "event_tab_id",
                value,
            );
            response.set_field_by_name("rewards", Value::List(awarded));
        }
        "/recipe/count_reward_receive" => {
            response.set_field_by_name(
                "rewards",
                Value::List(claim_recipe_counts(
                    proto,
                    rules,
                    &mut resources,
                    &mut changed,
                    request,
                    now,
                )?),
            );
        }
        "/mission/navigation_task_proceed" => {
            let id =
                i32_field(request, "navigation_task_url_id").ok_or(StateError::InvalidRequest)?;
            let mut found = false;
            for row in rules
                .missions
                .iter()
                .filter(|r| r.navigation_task_url_id == Some(id) && mission_active(rules, r, now))
            {
                found = true;
                let mut value = message_list(&resources, "missions")
                    .into_iter()
                    .find(|r| i32_field(r, "mission_id") == Some(row.id))
                    .ok_or(StateError::InvalidRequest)?;
                value.set_field_by_name("count", Value::I32(1));
                put(&mut resources, "missions", "mission_id", value.clone());
                put(&mut changed, "missions", "mission_id", value);
            }
            if !found {
                return Err(StateError::InvalidRequest);
            }
        }
        "/mail/list" => {}
        "/mail/open" | "/mail/delete" => {
            let awarded = open_mail(
                proto,
                rules,
                &mut resources,
                &mut changed,
                home,
                request,
                now,
                route == "/mail/delete",
            )?;
            if route == "/mail/open" {
                response.set_field_by_name("rewards", Value::List(awarded));
            }
        }
        "/recipe/favorite" => {
            let id = i32_field(request, "recipe_id").ok_or(StateError::InvalidRequest)?;
            let mut value = message_list(&resources, "recipes")
                .into_iter()
                .find(|r| i32_field(r, "recipe_id") == Some(id))
                .ok_or(StateError::InvalidRequest)?;
            let favorite = request
                .get_field_by_name("is_favorite")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            value.set_field_by_name("is_favorite", Value::Bool(favorite));
            put(&mut resources, "recipes", "recipe_id", value.clone());
            put(&mut changed, "recipes", "recipe_id", value);
        }
        "/quest/battle/skip" => quest_skip(
            proto,
            rules,
            reward_rules,
            &mut resources,
            &mut changed,
            request,
            &mut response,
            now,
        )?,
        "/quest/daily_clear_add" => daily_clear_add(
            proto,
            reward_rules,
            &mut resources,
            &mut changed,
            request,
            now,
        )?,
        "/quest/cleared_party_list" => {
            cleared_party_list(proto, reward_rules, &resources, request, &mut response, now)?
        }
        route if route.starts_with("/rental_party/") => rental_party_update(
            proto,
            reward_rules,
            characters,
            &mut resources,
            &mut changed,
            route,
            request,
        )?,
        "/profile/update_name" => apply_profile_name(&mut resources, &mut changed, request)?,
        "/profile/update_memo" => {
            let memo = request
                .get_field_by_name("memo")
                .and_then(|v| v.as_str().map(str::to_owned))
                .ok_or(StateError::InvalidRequest)?;
            if memo.chars().count() > 60 {
                return Err(StateError::InvalidRequest);
            }
            let mut profile = member_status(&resources, "profile")?;
            profile.set_field_by_name("memo", Value::String(memo));
            resources.set_field_by_name("profile", Value::Message(profile.clone()));
            changed.set_field_by_name("profile", Value::Message(profile));
        }
        "/user/update_birthdate" => {
            let year = i32_field(request, "year").filter(|year| *year > 0);
            let month = i32_field(request, "month").filter(|month| (1..=12).contains(month));
            let mut status = status_message(&resources)?;
            if year.is_none()
                || month.is_none()
                || status.has_field_by_name("birth_year")
                || status.has_field_by_name("birth_month")
            {
                return Err(StateError::InvalidRequest);
            }
            status.set_field_by_name(
                "birth_year",
                Value::Message(int32_value(proto, year.unwrap())?),
            );
            status.set_field_by_name(
                "birth_month",
                Value::Message(int32_value(proto, month.unwrap())?),
            );
            resources.set_field_by_name("status", Value::Message(status.clone()));
            changed.set_field_by_name("status", Value::Message(status));
        }
        "/profile/update_selected_home_id" => {
            apply_selected_home(proto, rules, &mut resources, &mut changed, request)?
        }
        "/character/bulk_set" => {
            apply_character_bulk_set(rules, &mut resources, &mut changed, request)?
        }
        route if route.starts_with("/character/") => {
            character::apply(
                proto,
                characters,
                rules,
                &mut resources,
                &mut changed,
                &mut response,
                route,
                request,
                now,
            )?;
        }
        "/profile/update_favorite_character"
        | "/profile/update_favorite_party"
        | "/profile/update_favorite_battle_tools"
        | "/profile/update_chara_home_favorite_character_list" => {
            character::favorite(&mut resources, &mut changed, route, request)?;
        }
        route if route.starts_with("/equipment_preset/") => {
            character::preset(
                proto,
                characters,
                &mut resources,
                &mut changed,
                route,
                request,
            )?;
        }
        route if atelier::is_route(route) => {
            atelier::apply(
                proto,
                atelier_rules,
                synthesis_rules,
                rules,
                &mut resources,
                &mut changed,
                &mut response,
                home,
                route,
                request,
                now,
            )?;
        }
        "/emblem/acquisition_drama" => {
            let ids = i32_list(request, "emblem_ids");
            if ids.is_empty() || ids.iter().collect::<BTreeSet<_>>().len() != ids.len() {
                return Err(StateError::InvalidRequest);
            }
            for id in ids {
                let mut emblem = message_list(&resources, "emblems")
                    .into_iter()
                    .find(|row| i32_field(row, "emblem_id") == Some(id))
                    .ok_or(StateError::InvalidRequest)?;
                let rarity = i32_field(&emblem, "rarity").ok_or(StateError::InvalidRequest)?;
                emblem.set_field_by_name("drama_rarity", Value::I32(rarity));
                put(&mut resources, "emblems", "emblem_id", emblem.clone());
                put(&mut changed, "emblems", "emblem_id", emblem);
            }
        }
        "/total_battle/achieve_line_drama" => {
            let id = i32_field(request, "total_battle_id").ok_or(StateError::InvalidRequest)?;
            let mut state = message_list(&resources, "total_battle_states")
                .into_iter()
                .find(|row| i32_field(row, "total_battle_id") == Some(id))
                .ok_or(StateError::InvalidRequest)?;
            let count = i32_field(&state, "best_achieved_line_count").unwrap_or_default();
            state.set_field_by_name("drama_achieved_line_count", Value::I32(count));
            put(
                &mut resources,
                "total_battle_states",
                "total_battle_id",
                state.clone(),
            );
            put(
                &mut changed,
                "total_battle_states",
                "total_battle_id",
                state,
            );
        }
        route if activities::is_route(route) => {
            activities::apply(
                proto,
                activity_rules,
                gameplay_rules,
                atelier_rules,
                characters,
                reward_rules,
                rules,
                &mut resources,
                &mut changed,
                &mut response,
                &mut home.activities,
                route,
                request,
                now,
            )?;
        }
        route if shop::is_route(route) => {
            shop::apply(
                proto,
                shop_rules,
                rules,
                master_data,
                &mut resources,
                &mut changed,
                &mut response,
                &mut home.shop,
                route,
                request,
                payment_provider_url,
                now,
            )?;
        }
        "/chara_home/register" => {
            apply_chara_home_register(proto, rules, &mut resources, &mut changed, request, now)?
        }
        "/recipe/learn" => {
            // Rebuild counters from persisted quest/item state before checking requirements.
            // This repairs accounts that cleared content before the corresponding mission tick.
            advance_missions(proto, rules, &mut resources, &mut changed, now, None)?;
            let mut awarded = Vec::new();
            let mut learned = Vec::new();
            for row in &rules.recipes {
                if row.requirements.is_empty()
                    || !row
                        .requirements
                        .iter()
                        .all(|r| total_task_count(&resources, r.condition_id) >= r.count)
                    || message_list(&resources, "recipes")
                        .iter()
                        .any(|r| i32_field(r, "recipe_id") == Some(row.id))
                {
                    continue;
                }
                let mut value = empty_message(proto, "blend.model.Recipe")?;
                value.set_field_by_name("recipe_id", Value::I32(row.id));
                value.set_field_by_name("received_at", Value::Message(timestamp(proto, now)?));
                put(&mut resources, "recipes", "recipe_id", value.clone());
                put(&mut changed, "recipes", "recipe_id", value);
                learned.push(Value::I32(row.id));
                awarded.extend(grant(
                    proto,
                    rules,
                    &mut resources,
                    &mut changed,
                    rewards(rules, row.learning_reward_set_id)?,
                    now,
                )?);
            }
            let recipe_count = message_list(&resources, "recipes").len() as i32;
            update_task(&mut resources, &mut changed, 134, recipe_count)?;
            response.set_field_by_name("learning_rewards", Value::List(awarded));
            response.set_field_by_name("learned_recipe_ids", Value::List(learned));
        }
        _ => return Err(StateError::InvalidRequest),
    }
    if route.starts_with("/mail/") {
        response.set_field_by_name("list", Value::Message(mail_list(proto, home, now)?));
    }
    let event = match route {
        "/tool/convert" => Some((
            "item_convert",
            message_list(request, "consumed_tools").len() as i32,
        )),
        "/memoria/limit_break" => Some(("memoria_limit_break", 1)),
        "/shop/purchase" | "/shop/random_shop/purchase" => Some((
            "shop_cole_spent",
            (message_i32_field(&before, "status", "cole").unwrap_or(0)
                - message_i32_field(&resources, "status", "cole").unwrap_or(0))
            .max(0),
        )),
        _ => None,
    };
    if let Some(event) = event {
        advance_missions(proto, rules, &mut resources, &mut changed, now, Some(event))?;
    }
    let extra = match route {
        "/character/rarity_enhance" => Some(("character_awaken", 1)),
        "/expedition/start" => Some((
            "expedition_set",
            message_list(request, "new_expeditions").len() as i32,
        )),
        "/profile/update_name"
            if member_status(&before, "profile")? != member_status(&resources, "profile")? =>
        {
            Some(("profile_name", 1))
        }
        "/shop/purchase" | "/shop/random_shop/purchase" | "/shop/piece_exchange" => Some((
            "shop_exchange",
            i32_field(request, "quantity").unwrap_or(1).max(1),
        )),
        _ => None,
    };
    if let Some(event) = extra {
        advance_missions(proto, rules, &mut resources, &mut changed, now, Some(event))?;
    }
    for ship in message_list(&resources, "ships") {
        let previous = message_list(&before, "ships")
            .into_iter()
            .find(|s| i32_field(s, "ship_id") == i32_field(&ship, "ship_id"));
        for (event, field, default) in [("ship_enhance", "exp", 0), ("ship_rank", "rank", 1)] {
            let old = previous
                .as_ref()
                .and_then(|s| i32_field(s, field))
                .unwrap_or(default);
            if i32_field(&ship, field).unwrap_or(default) > old {
                advance_missions(
                    proto,
                    rules,
                    &mut resources,
                    &mut changed,
                    now,
                    Some((event, 1)),
                )?;
            }
        }
    }
    home.memoria_history.extend(
        message_list(&resources, "memorias")
            .iter()
            .filter_map(|row| i32_field(row, "memoria_id")),
    );
    advance_missions(
        proto,
        rules,
        &mut resources,
        &mut changed,
        now,
        Some(("memoria_collection", home.memoria_history.len() as i32)),
    )?;
    resource_progress(proto, rules, &before, &mut resources, &mut changed, now)?;
    notifications(proto, &mut resources, &mut changed, home, now)?;
    if response
        .descriptor()
        .get_field_by_name("changed_resources")
        .is_some()
    {
        response.set_field_by_name("changed_resources", Value::Message(changed));
    }
    let character_index = message_list(&resources, "characters")
        .iter()
        .filter_map(|r| {
            Some((
                i64::from(i32_field(r, "character_id")?),
                optional_i32_field(r, "memoria_entity_id").map(i64::from),
            ))
        })
        .collect();
    Ok(GameplayCommit {
        resources_blob: resources.encode_to_vec(),
        response_plaintext: response.encode_to_vec(),
        character_index,
    })
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn reduce_home(
    proto: &ProtoRegistry,
    rules: &HomeRules,
    characters: &CharacterRules,
    resources: DynamicMessage,
    home: &mut HomeState,
    route: &str,
    request: &DynamicMessage,
    response_name: &str,
    now: i64,
) -> Result<GameplayCommit, StateError> {
    reduce_home_inner(
        proto,
        rules,
        &atelier::load_rules()?,
        &activities::load_rules()?,
        &load_gameplay_rules()?,
        &load_synthesis_rules()?,
        characters,
        &load_reward_rules()?,
        &shop::load_rules()?,
        &proto
            .decode(
                "blend.model.MasterData",
                &std::fs::read(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../data/master_data.pb"
                ))
                .map_err(|error| StateError::MasterData(error.to_string()))?,
            )
            .map_err(|error| StateError::MasterData(error.to_string()))?,
        "",
        resources,
        home,
        route,
        request,
        response_name,
        now,
    )
}

impl State {
    fn home_operation(
        &self,
        account_id: i64,
        request_id: &str,
        bytes: &[u8],
        route: &str,
        request: &DynamicMessage,
        response_name: &str,
    ) -> Result<(DynamicMessage, GameplayMutationResult), StateError> {
        let fingerprint = request_fingerprint(route, bytes);
        let now = unix_now();
        let result = self.store.apply_home_reducer(
            account_id,
            request_id,
            route,
            &fingerprint,
            now,
            |stored, saved| {
                let resources = self
                    .proto
                    .decode("blend.model.Resources", stored)
                    .map_err(|e| StorageError::Battle(e.to_string()))?;
                let mut home: HomeState = serde_json::from_slice(saved)
                    .map_err(|e| StorageError::Battle(e.to_string()))?;
                let commit = reduce_home_inner(
                    &self.proto,
                    &self.home_rules,
                    &self.atelier_rules,
                    &self.activity_rules,
                    &self.tutorial_rules,
                    &self.synthesis_rules,
                    self.character_rules,
                    &self.reward_rules,
                    &self.shop_rules,
                    &self.master_data,
                    &self.config.config.payment.provider_url,
                    resources,
                    &mut home,
                    route,
                    request,
                    response_name,
                    now,
                )
                .map_err(gameplay_storage_error)?;
                Ok((
                    commit,
                    serde_json::to_vec(&home).map_err(|e| StorageError::Battle(e.to_string()))?,
                ))
            },
        )?;
        gameplay_response(
            &self.store,
            &self.proto,
            account_id,
            request_id,
            route,
            &fingerprint,
            response_name,
            result,
        )
    }

    pub fn home_request(
        &self,
        session: &Session,
        request_id: &str,
        bytes: &[u8],
        route: &str,
        request: &DynamicMessage,
        response_name: &str,
    ) -> Result<(DynamicMessage, GameplayMutationResult), StateError> {
        if !crate::state::combat::home_route_is_read_only(route) {
            self.ensure_noncombat_mutation_allowed(session.account_id)?;
        }
        self.home_operation(
            session.account_id,
            request_id,
            bytes,
            route,
            request,
            response_name,
        )
    }

    pub(crate) fn refresh_home(&self, account_id: i64) -> Result<(), StateError> {
        let now = unix_now();
        let request = empty_message(&self.proto, "google.protobuf.Empty")?;
        self.store
            .apply_home_refresh_reducer(account_id, now, |stored, saved| {
                let resources = self
                    .proto
                    .decode("blend.model.Resources", stored)
                    .map_err(|error| StorageError::Battle(error.to_string()))?;
                let mut home: HomeState = serde_json::from_slice(saved)
                    .map_err(|error| StorageError::Battle(error.to_string()))?;
                let commit = reduce_home_inner(
                    &self.proto,
                    &self.home_rules,
                    &self.atelier_rules,
                    &self.activity_rules,
                    &self.tutorial_rules,
                    &self.synthesis_rules,
                    self.character_rules,
                    &self.reward_rules,
                    &self.shop_rules,
                    &self.master_data,
                    &self.config.config.payment.provider_url,
                    resources,
                    &mut home,
                    "/local/home_refresh",
                    &request,
                    "blend.api.ChangedResourcesResponse",
                    now,
                )
                .map_err(gameplay_storage_error)?;
                Ok((
                    commit,
                    serde_json::to_vec(&home)
                        .map_err(|error| StorageError::Battle(error.to_string()))?,
                ))
            })?;
        Ok(())
    }
}
