use super::prelude::*;

impl State {
    pub fn synthesis_execute(
        &self,
        session: &Session,
        request_id: &str,
        request_bytes: &[u8],
        request: &DynamicMessage,
        route: &str,
    ) -> Result<(DynamicMessage, GameplayMutationResult), StateError> {
        if request_id.is_empty() {
            return Err(StateError::InvalidRequest);
        }
        let (mode, response_name) = match route {
            "/synthesis/execute" => (SynthesisMode::Execute, "blend.api.SynthesisExecuteResponse"),
            "/synthesis/bulk_execute" => {
                (SynthesisMode::Bulk, "blend.api.SynthesisExecuteResponse")
            }
            "/synthesis/execute_easy" => (
                SynthesisMode::Easy,
                "blend.api.SynthesisExecuteEasyResponse",
            ),
            "/synthesis/execute_rental" => {
                (SynthesisMode::Rental, "blend.api.SynthesisExecuteResponse")
            }
            _ => return Err(StateError::InvalidRequest),
        };
        self.ensure_noncombat_mutation_allowed(session.account_id)?;
        let fingerprint = request_fingerprint(route, request_bytes);
        let proto = self.proto.clone();
        let rules = self.synthesis_rules.clone();
        let home_rules = self.home_rules.clone();
        let activity_rules = self.activity_rules.clone();
        let request = request.clone();
        let result = self.store.apply_gameplay_reducer(
            session.account_id,
            request_id,
            route,
            &fingerprint,
            unix_now(),
            move |stored| {
                let resources = proto
                    .decode("blend.model.Resources", stored)
                    .map_err(|error| StorageError::Battle(error.to_string()))?;
                let mutation = reduce_synthesis(
                    &proto,
                    &rules,
                    &home_rules,
                    &activity_rules,
                    resources,
                    &request,
                    mode,
                    unix_now(),
                )
                .map_err(gameplay_storage_error)?;
                Ok(GameplayCommit {
                    resources_blob: mutation.resources.encode_to_vec(),
                    response_plaintext: mutation.response.encode_to_vec(),
                    character_index: Vec::new(),
                })
            },
        )?;
        gameplay_response(
            &self.store,
            &self.proto,
            session.account_id,
            request_id,
            route,
            &fingerprint,
            response_name,
            result,
        )
    }

    pub fn synthesis_combination_ranking(
        &self,
        session: &Session,
        request: &DynamicMessage,
    ) -> Result<DynamicMessage, StateError> {
        let resources = self
            .proto
            .decode(
                "blend.model.Resources",
                &self.store.player_resources(session.account_id)?,
            )
            .map_err(|error| StateError::Descriptor(error.to_string()))?;
        reduce_synthesis_ranking(
            &self.proto,
            &self.synthesis_rules,
            &resources,
            request,
            unix_now(),
        )
    }

    pub fn party_bulk_update(
        &self,
        session: &Session,
        request_id: &str,
        request_bytes: &[u8],
        request: &DynamicMessage,
    ) -> Result<(DynamicMessage, GameplayMutationResult), StateError> {
        self.party_update(
            session,
            request_id,
            request_bytes,
            request,
            false,
            "/party/bulk_update",
        )
    }

    pub fn party_battle_tools_set(
        &self,
        session: &Session,
        request_id: &str,
        request_bytes: &[u8],
        request: &DynamicMessage,
    ) -> Result<(DynamicMessage, GameplayMutationResult), StateError> {
        self.party_update(
            session,
            request_id,
            request_bytes,
            request,
            true,
            "/party/battle_tools_set",
        )
    }

    fn party_update(
        &self,
        session: &Session,
        request_id: &str,
        request_bytes: &[u8],
        request: &DynamicMessage,
        tools_only: bool,
        route: &str,
    ) -> Result<(DynamicMessage, GameplayMutationResult), StateError> {
        if request_id.is_empty() {
            return Err(StateError::InvalidRequest);
        }
        self.ensure_noncombat_mutation_allowed(session.account_id)?;
        let fingerprint = request_fingerprint(route, request_bytes);
        let proto = self.proto.clone();
        let fresh_rules = self.fresh_rules.clone();
        let request = request.clone();
        let result = self.store.apply_gameplay_reducer(
            session.account_id,
            request_id,
            route,
            &fingerprint,
            unix_now(),
            move |stored| {
                let resources = proto
                    .decode("blend.model.Resources", stored)
                    .map_err(|error| StorageError::Battle(error.to_string()))?;
                let mutation = reduce_party(&proto, &fresh_rules, resources, &request, tools_only)
                    .map_err(gameplay_storage_error)?;
                Ok(GameplayCommit {
                    resources_blob: mutation.resources.encode_to_vec(),
                    response_plaintext: mutation.response.encode_to_vec(),
                    character_index: Vec::new(),
                })
            },
        )?;
        gameplay_response(
            &self.store,
            &self.proto,
            session.account_id,
            request_id,
            route,
            &fingerprint,
            "blend.api.ChangedResourcesResponse",
            result,
        )
    }

    pub fn gacha_list(
        &self,
        session: &Session,
        request: &DynamicMessage,
    ) -> Result<DynamicMessage, StateError> {
        let category = i32_field(request, "gacha_category").ok_or(StateError::InvalidRequest)?;
        if !self
            .tutorial_rules
            .gachas
            .iter()
            .any(|gacha| gacha.category == category)
        {
            return Err(StateError::InvalidRequest);
        }
        let resources = self
            .proto
            .decode(
                "blend.model.Resources",
                &self.store.player_resources(session.account_id)?,
            )
            .map_err(|error| StateError::Descriptor(error.to_string()))?;
        let button_states = self.store.gacha_button_states(session.account_id)?;
        let wish_lists = self.store.gacha_wish_lists(session.account_id)?;
        reduce_gacha_list(
            &self.proto,
            &self.tutorial_rules,
            &resources,
            category,
            &button_states,
            &wish_lists,
            unix_now(),
        )
    }

    pub fn gacha_execute(
        &self,
        session: &Session,
        request_id: &str,
        request_bytes: &[u8],
        request: &DynamicMessage,
    ) -> Result<(DynamicMessage, GameplayMutationResult), StateError> {
        let step_up = request.descriptor().full_name() == "blend.api.GachaStepUpExecuteRequest";
        let route = if step_up {
            "/gacha/step_up_execute"
        } else {
            "/gacha/execute"
        };
        if request_id.is_empty() {
            return Err(StateError::InvalidRequest);
        }
        let gacha_id = i32_field(request, "gacha_id").ok_or(StateError::InvalidRequest)?;
        let button_id = i32_field(
            request,
            if step_up {
                "gacha_step_up_button_id"
            } else {
                "gacha_button_id"
            },
        )
        .ok_or(StateError::InvalidRequest)?;
        self.ensure_noncombat_mutation_allowed(session.account_id)?;
        let home_rules = self.home_rules.clone();
        let fingerprint = request_fingerprint(route, request_bytes);
        let proto = self.proto.clone();
        let rules = self.tutorial_rules.clone();
        let request = request.clone();
        let result = self.store.apply_gacha(
            session.account_id,
            request_id,
            route,
            &fingerprint,
            i64::from(gacha_id),
            if step_up { -1 } else { i64::from(button_id) },
            unix_now(),
            move |stored, execution_count, wish_list| {
                let resources = proto
                    .decode("blend.model.Resources", stored)
                    .map_err(|error| StorageError::Battle(error.to_string()))?;
                let before = resources.clone();
                let button = selected_gacha_button(&rules, button_id, step_up)
                    .map_err(gameplay_storage_error)?;
                if button
                    .limit_count
                    .is_some_and(|limit| execution_count >= i64::from(limit))
                {
                    return Err(StorageError::GachaLimit);
                }
                let mut mutation = reduce_gacha_execute(
                    &proto,
                    &rules,
                    resources,
                    &request,
                    execution_count,
                    wish_list.as_ref(),
                    unix_now(),
                )
                .map_err(gameplay_storage_error)?;
                let mut changed = member_status(&mutation.response, "changed_resources")
                    .map_err(gameplay_storage_error)?;
                home::advance_missions(
                    &proto,
                    &home_rules,
                    &mut mutation.resources,
                    &mut changed,
                    unix_now(),
                    Some(("gacha", button.draw_count)),
                )
                .map_err(gameplay_storage_error)?;
                home::advance_missions(
                    &proto,
                    &home_rules,
                    &mut mutation.resources,
                    &mut changed,
                    unix_now(),
                    Some((&format!("gacha:{gacha_id}"), button.draw_count)),
                )
                .map_err(gameplay_storage_error)?;
                home::resource_progress(
                    &proto,
                    &home_rules,
                    &before,
                    &mut mutation.resources,
                    &mut changed,
                    unix_now(),
                )
                .map_err(gameplay_storage_error)?;
                mutation
                    .response
                    .set_field_by_name("changed_resources", Value::Message(changed));
                Ok(GachaCommit {
                    gameplay: GameplayCommit {
                        resources_blob: mutation.resources.encode_to_vec(),
                        response_plaintext: mutation.response.encode_to_vec(),
                        character_index: mutation.character_indexes,
                    },
                })
            },
        )?;
        gameplay_response(
            &self.store,
            &self.proto,
            session.account_id,
            request_id,
            route,
            &fingerprint,
            if step_up {
                "blend.api.GachaStepUpExecuteResponse"
            } else {
                "blend.api.GachaExecuteResponse"
            },
            result,
        )
    }

    pub fn gacha_wish_list_set(
        &self,
        session: &Session,
        request: &DynamicMessage,
    ) -> Result<DynamicMessage, StateError> {
        let gacha_id = i32_field(request, "gacha_id").ok_or(StateError::InvalidRequest)?;
        let gacha = gacha_rule(&self.tutorial_rules, gacha_id)?;
        let wish_list = GachaWishList {
            gacha_id: i64::from(gacha_id),
            character_ids: i32_list(request, "character_ids"),
            memoria_ids: i32_list(request, "memoria_ids"),
            character_skin_ids: i32_list(request, "character_skin_ids"),
        };
        validate_wish_list(gacha, &wish_list)?;
        self.ensure_noncombat_mutation_allowed(session.account_id)?;
        self.store
            .set_gacha_wish_list(session.account_id, &wish_list, unix_now())?;
        let mut response = empty_message(&self.proto, "blend.api.GachaWishListSetResponse")?;
        response.set_field_by_name(
            "wish_list_state",
            Value::Message(gacha_wish_list_message(&self.proto, &wish_list)?),
        );
        Ok(response)
    }
}
