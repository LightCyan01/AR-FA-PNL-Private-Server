use super::prelude::*;

impl State {
    pub fn quest_talk_event_finish(
        &self,
        session: &Session,
        request_id: &str,
        request_bytes: &[u8],
        request: &DynamicMessage,
    ) -> Result<(DynamicMessage, GameplayMutationResult), StateError> {
        const ROUTE: &str = "/quest/talk_event/finish";

        let quest_id = i32_field(request, "quest_id").ok_or(StateError::InvalidRequest)?;
        if quest_id <= 0 || request_id.is_empty() {
            return Err(StateError::InvalidRequest);
        }
        self.ensure_noncombat_mutation_allowed(session.account_id)?;
        let fingerprint = request_fingerprint(ROUTE, request_bytes);
        let now = unix_now();
        let result = self.store.apply_gameplay_reducer(
            session.account_id,
            request_id,
            ROUTE,
            &fingerprint,
            now,
            |stored| {
                let before = self
                    .proto
                    .decode("blend.model.Resources", stored)
                    .map_err(|e| StorageError::Battle(e.to_string()))?;
                let mut mutation = reduce_talk_event_with_rules(
                    &self.proto,
                    &self.fresh_rules,
                    &self.tutorial_rules,
                    &self.home_rules,
                    before.clone(),
                    quest_id,
                    now,
                )
                .map_err(gameplay_storage_error)?;
                let mut changed = member_status(&mutation.response, "changed_resources")
                    .map_err(gameplay_storage_error)?;
                home::resource_progress(
                    &self.proto,
                    &self.home_rules,
                    &before,
                    &mut mutation.resources,
                    &mut changed,
                    now,
                )
                .map_err(gameplay_storage_error)?;
                mutation
                    .response
                    .set_field_by_name("changed_resources", Value::Message(changed));
                Ok(GameplayCommit {
                    resources_blob: mutation.resources.encode_to_vec(),
                    response_plaintext: mutation.response.encode_to_vec(),
                    character_index: message_list(&mutation.resources, "characters")
                        .iter()
                        .filter_map(|c| {
                            Some((
                                i64::from(i32_field(c, "character_id")?),
                                optional_i32_field(c, "memoria_entity_id").map(i64::from),
                            ))
                        })
                        .collect(),
                })
            },
        )?;
        gameplay_response(
            &self.store,
            &self.proto,
            session.account_id,
            request_id,
            ROUTE,
            &fingerprint,
            "blend.api.QuestTalkEventFinishResponse",
            result,
        )
    }

    pub fn quest_battle_start(
        &self,
        session: &Session,
        request_id: &str,
        request_bytes: &[u8],
        request: &DynamicMessage,
    ) -> Result<(DynamicMessage, GameplayMutationResult), StateError> {
        const ROUTE: &str = "/quest/battle/start";

        let quest_id = i32_field(request, "quest_id").ok_or(StateError::InvalidRequest)?;
        if quest_id <= 0 || request_id.is_empty() {
            return Err(StateError::InvalidRequest);
        }
        let party_number = i32_field(request, "party_number").unwrap_or_default();
        let ship_id = optional_i32_field(request, "ship_id");
        // ponytail: accept client weakening selections; add scaling when its formula is proven.
        if !(1..=20).contains(&party_number)
            || i32_field(request, "enemy_weak_level").unwrap_or_default() < 0
        {
            return Err(StateError::InvalidRequest);
        }
        let fingerprint = request_fingerprint(ROUTE, request_bytes);
        let now = unix_now();
        let result = self.store.apply_battle_start_reducer(
            session.account_id,
            request_id,
            ROUTE,
            &fingerprint,
            now,
            |stored, saved| {
                let mut home: home::HomeState = serde_json::from_slice(saved)
                    .map_err(|e| StorageError::Battle(e.to_string()))?;
                let mut resources = self
                    .proto
                    .decode("blend.model.Resources", stored)
                    .map_err(|e| StorageError::Battle(e.to_string()))?;
                let quest = self
                    .tutorial_rules
                    .quests
                    .iter()
                    .find(|q| q.id == quest_id)
                    .ok_or_else(|| gameplay_storage_error(StateError::InvalidRequest))?;
                let mut changed = empty_message(&self.proto, "blend.model.Resources")
                    .map_err(gameplay_storage_error)?;
                quest::charge_quest(
                    &self.proto,
                    &self.home_rules,
                    quest,
                    &mut resources,
                    &mut changed,
                    now,
                )
                .map_err(gameplay_storage_error)?;
                let mut mutation = reduce_battle_start_with_progression(
                    &self.proto,
                    &self.tutorial_rules,
                    &self.atelier_rules,
                    self.character_rules,
                    resources.clone(),
                    quest_id,
                    party_number,
                    ship_id,
                    None,
                    BattleStartMode::Standard,
                    now,
                )
                .map_err(gameplay_storage_error)?;
                let party_power = activities::account_party_combat_power(
                    &self.activity_rules,
                    &self.tutorial_rules,
                    &self.atelier_rules,
                    self.character_rules,
                    &resources,
                    party_number,
                )
                .map_err(gameplay_storage_error)?;
                home::advance_missions(
                    &self.proto,
                    &self.home_rules,
                    &mut resources,
                    &mut changed,
                    now,
                    Some(("party_combat_power", party_power)),
                )
                .map_err(gameplay_storage_error)?;
                mutation
                    .response
                    .set_field_by_name("changed_resources", Value::Message(resources.clone()));
                home.combat_effects = mutation.effects;
                Ok(BattleStartCommit {
                    resources_blob: resources.encode_to_vec(),
                    quest_id: i64::from(mutation.quest_id),
                    battle_id: i64::from(mutation.battle_id),
                    start_txid: mutation.start_txid,
                    state_blob: mutation.state.encode_to_vec(),
                    response_plaintext: mutation.response.encode_to_vec(),
                    home_state: Some(
                        serde_json::to_vec(&home)
                            .map_err(|e| StorageError::Battle(e.to_string()))?,
                    ),
                })
            },
        )?;
        gameplay_response(
            &self.store,
            &self.proto,
            session.account_id,
            request_id,
            ROUTE,
            &fingerprint,
            "blend.api.BattleStartResponse",
            result,
        )
    }

    pub(crate) fn specialized_battle_start(
        &self,
        session: &Session,
        request_id: &str,
        request_bytes: &[u8],
        request: &DynamicMessage,
        mode: BattleStartMode,
    ) -> Result<(DynamicMessage, GameplayMutationResult), StateError> {
        let route = match mode {
            BattleStartMode::Rental => "/quest/battle/rental_party_start",
            BattleStartMode::SoloRaid => "/quest/battle/solo_raid_battle_start",
            BattleStartMode::Total => "/quest/battle/total_battle_start",
            BattleStartMode::Gacha => "/gacha/battle_start",
            BattleStartMode::Standard => return Err(StateError::InvalidRequest),
        };
        let quest_id = i32_field(
            request,
            if mode == BattleStartMode::Gacha {
                "gacha_battle_id"
            } else {
                "quest_id"
            },
        )
        .ok_or(StateError::InvalidRequest)?;
        if quest_id <= 0 || request_id.is_empty() {
            return Err(StateError::InvalidRequest);
        }
        let fingerprint = request_fingerprint(route, request_bytes);
        let now = unix_now();
        let result = self.store.apply_battle_start_reducer(
            session.account_id,
            request_id,
            route,
            &fingerprint,
            now,
            |stored, saved| {
                let mut home: home::HomeState = serde_json::from_slice(saved)
                    .map_err(|e| StorageError::Battle(e.to_string()))?;
                let mut resources = self
                    .proto
                    .decode("blend.model.Resources", stored)
                    .map_err(|e| StorageError::Battle(e.to_string()))?;
                let trial_quest;
                let quest = if mode == BattleStartMode::Gacha {
                    let gacha_id =
                        i32_field(request, "gacha_id").ok_or(StorageError::BattleRejected)?;
                    let gacha = gacha_rule(&self.tutorial_rules, gacha_id)
                        .map_err(gameplay_storage_error)?;
                    if !gacha.gacha_battle_ids.contains(&quest_id)
                        || !home::in_period(gacha.start_at, gacha.end_at, now)
                    {
                        return Err(StorageError::BattleRejected);
                    }
                    let trial = activities::row(&self.activity_rules, "gacha_battle", quest_id)
                        .map_err(gameplay_storage_error)?;
                    // This storage column identifies the parent; context keeps its gacha oneof.
                    trial_quest = TutorialQuest {
                        id: quest_id,
                        quest_type: 1,
                        battle_id: Some(number(trial, "battle_id")),
                        fixed_party_id: Some(number(trial, "fixed_party_id")),
                        ..TutorialQuest::default()
                    };
                    home.gacha_battle_id = Some(quest_id);
                    &trial_quest
                } else {
                    home.gacha_battle_id = None;
                    self.tutorial_rules
                        .quests
                        .iter()
                        .find(|q| q.id == quest_id)
                        .ok_or(StorageError::BattleRejected)?
                };
                if (mode == BattleStartMode::Rental && quest.rental_fixed_party_id.is_none())
                    || (mode == BattleStartMode::SoloRaid && quest.solo_raid_id.is_none())
                    || (mode == BattleStartMode::Total && quest.total_battle_panel_id.is_none())
                {
                    return Err(StorageError::BattleRejected);
                }
                let mut changed = empty_message(&self.proto, "blend.model.Resources")
                    .map_err(gameplay_storage_error)?;
                if mode != BattleStartMode::Gacha {
                    quest::charge_quest(
                        &self.proto,
                        &self.home_rules,
                        quest,
                        &mut resources,
                        &mut changed,
                        now,
                    )
                    .map_err(gameplay_storage_error)?;
                }
                register_special_battle_state(&self.proto, &mut resources, quest, mode, now)
                    .map_err(gameplay_storage_error)?;
                let mut mutation = build_battle_start(
                    &self.proto,
                    &self.tutorial_rules,
                    &self.atelier_rules,
                    self.character_rules,
                    resources.clone(),
                    quest,
                    1,
                    None,
                    None,
                    mode,
                    now,
                )
                .map_err(gameplay_storage_error)?;
                if mode == BattleStartMode::Gacha {
                    let mut context = member_status(&mutation.response, "context")
                        .map_err(gameplay_storage_error)?;
                    context.set_field_by_name("gacha_battle_id", Value::I32(quest_id));
                    context.clear_field_by_name("party_number");
                    context.set_field_by_name(
                        "gacha_id",
                        Value::Message(
                            int32_value(
                                &self.proto,
                                i32_field(request, "gacha_id")
                                    .ok_or(StorageError::BattleRejected)?,
                            )
                            .map_err(gameplay_storage_error)?,
                        ),
                    );
                    mutation
                        .response
                        .set_field_by_name("context", Value::Message(context));
                }
                mutation
                    .response
                    .set_field_by_name("changed_resources", Value::Message(resources.clone()));
                home.combat_effects = mutation.effects;
                Ok(BattleStartCommit {
                    resources_blob: resources.encode_to_vec(),
                    quest_id: i64::from(mutation.quest_id),
                    battle_id: i64::from(mutation.battle_id),
                    start_txid: mutation.start_txid,
                    state_blob: mutation.state.encode_to_vec(),
                    response_plaintext: mutation.response.encode_to_vec(),
                    home_state: Some(
                        serde_json::to_vec(&home)
                            .map_err(|e| StorageError::Battle(e.to_string()))?,
                    ),
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
            "blend.api.BattleStartResponse",
            result,
        )
    }

    pub fn exploration_battle_start(
        &self,
        session: &Session,
        request_id: &str,
        request_bytes: &[u8],
        request: &DynamicMessage,
    ) -> Result<(DynamicMessage, GameplayMutationResult), StateError> {
        const ROUTE: &str = "/exploration/battle_start";
        if request_id.is_empty() {
            return Err(StateError::InvalidRequest);
        }
        let fingerprint = request_fingerprint(ROUTE, request_bytes);
        let now = unix_now();
        let result = self.store.apply_battle_start_reducer(
            session.account_id,
            request_id,
            ROUTE,
            &fingerprint,
            now,
            |stored, saved| {
                let mut resources = self
                    .proto
                    .decode("blend.model.Resources", stored)
                    .map_err(|e| StorageError::Battle(e.to_string()))?;
                let mut home: home::HomeState = serde_json::from_slice(saved)
                    .map_err(|e| StorageError::Battle(e.to_string()))?;
                let area_id = i32_field(request, "area_id").ok_or(StorageError::BattleRejected)?;
                let quest_id = number(
                    activities::row(&self.activity_rules, "exploration_area", area_id)
                        .map_err(gameplay_storage_error)?,
                    "quest_id",
                );
                let party_number = home
                    .activities
                    .explorations
                    .get(&quest_id)
                    .map(|run| run.party_number)
                    .ok_or(StorageError::BattleRejected)?;
                let party_power = activities::account_party_combat_power(
                    &self.activity_rules,
                    &self.tutorial_rules,
                    &self.atelier_rules,
                    self.character_rules,
                    &resources,
                    party_number,
                )
                .map_err(gameplay_storage_error)?;
                let mut mutation = activities::battle_start(
                    &self.proto,
                    &self.activity_rules,
                    &self.tutorial_rules,
                    &self.atelier_rules,
                    self.character_rules,
                    &mut resources,
                    &mut home.activities,
                    request,
                    now,
                )
                .map_err(gameplay_storage_error)?;
                let mut changed = member_status(&mutation.response, "changed_resources")
                    .map_err(gameplay_storage_error)?;
                home::advance_missions(
                    &self.proto,
                    &self.home_rules,
                    &mut resources,
                    &mut changed,
                    now,
                    Some(("party_combat_power", party_power)),
                )
                .map_err(gameplay_storage_error)?;
                mutation
                    .response
                    .set_field_by_name("changed_resources", Value::Message(changed));
                home.combat_effects = mutation.effects;
                Ok(BattleStartCommit {
                    resources_blob: resources.encode_to_vec(),
                    quest_id: i64::from(mutation.quest_id),
                    battle_id: i64::from(mutation.battle_id),
                    start_txid: mutation.start_txid,
                    state_blob: mutation.state.encode_to_vec(),
                    response_plaintext: mutation.response.encode_to_vec(),
                    home_state: Some(
                        serde_json::to_vec(&home)
                            .map_err(|e| StorageError::Battle(e.to_string()))?,
                    ),
                })
            },
        )?;
        gameplay_response(
            &self.store,
            &self.proto,
            session.account_id,
            request_id,
            ROUTE,
            &fingerprint,
            "blend.api.BattleStartResponse",
            result,
        )
    }

    pub fn battle_attack(
        &self,
        session: &Session,
        request_id: &str,
        request_bytes: &[u8],
        request: &DynamicMessage,
    ) -> Result<(DynamicMessage, GameplayMutationResult), StateError> {
        const ROUTE: &str = "/battle/attack";
        if request_id.is_empty() {
            return Err(StateError::InvalidRequest);
        }
        let fingerprint = request_fingerprint(ROUTE, request_bytes);
        let proto = self.proto.clone();
        let rules = self.tutorial_rules.clone();
        let secret = self.config.server_secret.clone();
        let request = request.clone();
        let result = self.store.apply_battle_attack(
            session.account_id,
            request_id,
            ROUTE,
            &fingerprint,
            unix_now(),
            move |active, resources_blob, saved| {
                let mut home: home::HomeState = serde_json::from_slice(saved)
                    .map_err(|e| StorageError::Battle(e.to_string()))?;
                let state = proto
                    .decode("blend.model.BattleState", &active.state_blob)
                    .map_err(|error| StorageError::Battle(error.to_string()))?;
                let resources = proto
                    .decode("blend.model.Resources", resources_blob)
                    .map_err(|error| StorageError::Battle(error.to_string()))?;
                let mutation = reduce_battle_attack_with_effects(
                    &proto,
                    &rules,
                    state,
                    &request,
                    &secret,
                    &active.start_txid,
                    Some(&resources),
                    &mut home.combat_effects,
                )
                .map_err(battle_storage_error)?;
                home::record_battle_progress(
                    &mut home,
                    &active.start_txid,
                    &member_status(&mutation.response, "history")
                        .map_err(gameplay_storage_error)?,
                )
                .map_err(gameplay_storage_error)?;
                Ok((
                    mutation.state.encode_to_vec(),
                    mutation.response.encode_to_vec(),
                    serde_json::to_vec(&home).map_err(|e| StorageError::Battle(e.to_string()))?,
                ))
            },
        )?;
        match result {
            GameplayMutationResult::Applied => {
                let response = self
                    .proto
                    .decode(
                        "blend.api.BattleAttackResponse",
                        &self
                            .store
                            .gameplay_replay(session.account_id, request_id, ROUTE, &fingerprint)?
                            .ok_or_else(|| StateError::Storage(StorageError::NotFound))?,
                    )
                    .map_err(|error| StateError::Descriptor(error.to_string()))?;
                Ok((response, GameplayMutationResult::Applied))
            }
            GameplayMutationResult::Replay(bytes) => {
                let response = self
                    .proto
                    .decode("blend.api.BattleAttackResponse", &bytes)
                    .map_err(|error| StateError::Descriptor(error.to_string()))?;
                Ok((response, GameplayMutationResult::Replay(bytes)))
            }
        }
    }

    pub fn battle_finish(
        &self,
        session: &Session,
        request_id: &str,
        request_bytes: &[u8],
    ) -> Result<(DynamicMessage, GameplayMutationResult), StateError> {
        const ROUTE: &str = "/battle/finish";
        if request_id.is_empty() || !request_bytes.is_empty() {
            return Err(StateError::InvalidRequest);
        }
        let fingerprint = request_fingerprint(ROUTE, request_bytes);
        let proto = self.proto.clone();
        let rules = self.tutorial_rules.clone();
        let reward_rules = self.reward_rules.clone();
        let home_rules = self.home_rules.clone();
        let activity_rules = self.activity_rules.clone();
        let result = self.store.apply_battle_finish(
            session.account_id,
            request_id,
            ROUTE,
            &fingerprint,
            unix_now(),
            move |active, resources_blob, saved| {
                let mut home: home::HomeState = serde_json::from_slice(saved)
                    .map_err(|e| StorageError::Battle(e.to_string()))?;
                let state = proto
                    .decode("blend.model.BattleState", &active.state_blob)
                    .map_err(|error| StorageError::Battle(error.to_string()))?;
                let resources = proto
                    .decode("blend.model.Resources", resources_blob)
                    .map_err(|error| StorageError::Battle(error.to_string()))?;
                let before = resources.clone();
                let defeated = i32_list(&state, "wave_ids")
                    .iter()
                    .try_fold(0i32, |sum, id| {
                        rule_wave(&rules, *id).map(|wave| sum + wave.enemies.len() as i32)
                    })
                    .map_err(gameplay_storage_error)?;
                let trial = home.gacha_battle_id.take();
                let mut mutation = if let Some(id) = trial {
                    if i64::from(id) != active.quest_id {
                        return Err(StorageError::BattleRejected);
                    }
                    activities::finish_gacha_battle(
                        &proto,
                        &activity_rules,
                        &home_rules,
                        &state,
                        resources,
                        id,
                        unix_now(),
                    )
                } else if rules
                    .quests
                    .iter()
                    .any(|q| q.id == active.quest_id as i32 && q.quest_type == 3)
                {
                    activities::battle_finish(
                        &proto,
                        &rules,
                        &state,
                        resources,
                        &mut home.activities,
                        active.quest_id as i32,
                    )
                } else {
                    reduce_battle_finish_with_rules(
                        &proto,
                        &rules,
                        &reward_rules,
                        &home_rules,
                        self.character_rules,
                        &home.battle_progress,
                        state.clone(),
                        resources,
                        active.quest_id as i32,
                    )
                }
                .map_err(battle_storage_error)?;
                let mut changed = member_status(&mutation.response, "changed_resources")
                    .map_err(gameplay_storage_error)?;
                if trial.is_none() {
                    if let Some(quest) =
                        rules.quests.iter().find(|q| q.id == active.quest_id as i32)
                    {
                        let rewards = modes::finish_total(
                            &proto,
                            &rules,
                            &activity_rules,
                            &home_rules,
                            quest,
                            &state,
                            &mut mutation.resources,
                            &mut changed,
                            unix_now(),
                        )
                        .map_err(gameplay_storage_error)?;
                        if !rewards.is_empty() {
                            let mut result = member_status(&mutation.response, "quest_result")
                                .map_err(gameplay_storage_error)?;
                            let mut all = message_list(&result, "rewards")
                                .into_iter()
                                .map(Value::Message)
                                .collect::<Vec<_>>();
                            all.extend(rewards);
                            result.set_field_by_name("rewards", Value::List(all));
                            mutation
                                .response
                                .set_field_by_name("quest_result", Value::Message(result));
                        }
                    }
                    let quest_cleared =
                        quest_clear_count(&mutation.resources, active.quest_id as i32)
                            > quest_clear_count(&before, active.quest_id as i32);
                    home::battle_clear_progress(
                        &proto,
                        &home_rules,
                        &rules,
                        &state,
                        &mut mutation.resources,
                        &mut changed,
                        active.quest_id as i32,
                        quest_cleared,
                        unix_now(),
                    )
                    .map_err(gameplay_storage_error)?;
                }
                if home.battle_progress.start_txid == active.start_txid {
                    if trial.is_none() {
                        home::advance_multi_mission_battle(
                            &proto,
                            &home_rules,
                            &mut mutation.resources,
                            &mut changed,
                            active.quest_id as i32,
                            home.battle_progress.damage,
                            unix_now(),
                        )
                        .map_err(gameplay_storage_error)?;
                    }
                    home::finish_battle_progress(
                        &proto,
                        &home_rules,
                        &mut home,
                        &mut mutation.resources,
                        &mut changed,
                        trial.is_none().then_some(active.quest_id as i32),
                        unix_now(),
                    )
                    .map_err(gameplay_storage_error)?;
                }
                if trial.is_none() {
                    home::advance_missions(
                        &proto,
                        &home_rules,
                        &mut mutation.resources,
                        &mut changed,
                        unix_now(),
                        Some(("enemy_defeat", defeated)),
                    )
                    .map_err(gameplay_storage_error)?;
                    let mission_battle_rewards = home::grant_mission_battle_rewards(
                        &proto,
                        &home_rules,
                        &mut mutation.resources,
                        &mut changed,
                        active.quest_id as i32,
                        unix_now(),
                    )
                    .map_err(gameplay_storage_error)?;
                    if !mission_battle_rewards.is_empty() {
                        let mut result = member_status(&mutation.response, "quest_result")
                            .map_err(gameplay_storage_error)?;
                        result.set_field_by_name(
                            "mission_battle_rewards",
                            Value::List(mission_battle_rewards),
                        );
                        mutation
                            .response
                            .set_field_by_name("quest_result", Value::Message(result));
                    }
                }
                home::resource_progress(
                    &proto,
                    &home_rules,
                    &before,
                    &mut mutation.resources,
                    &mut changed,
                    unix_now(),
                )
                .map_err(gameplay_storage_error)?;
                home.combat_effects = effects::Runtime::default();
                mutation
                    .response
                    .set_field_by_name("changed_resources", Value::Message(changed));
                Ok(crate::storage::BattleFinishCommit {
                    resources_blob: mutation.resources.encode_to_vec(),
                    response_plaintext: mutation.response.encode_to_vec(),
                    character_index: mutation.character_indexes,
                    home_state: Some(
                        serde_json::to_vec(&home)
                            .map_err(|e| StorageError::Battle(e.to_string()))?,
                    ),
                })
            },
        )?;
        match result {
            GameplayMutationResult::Applied => {
                let bytes = self
                    .store
                    .gameplay_replay(session.account_id, request_id, ROUTE, &fingerprint)?
                    .ok_or_else(|| StateError::Storage(StorageError::NotFound))?;
                let response = self
                    .proto
                    .decode("blend.api.BattleFinishResponse", &bytes)
                    .map_err(|error| StateError::Descriptor(error.to_string()))?;
                Ok((response, GameplayMutationResult::Applied))
            }
            GameplayMutationResult::Replay(bytes) => {
                let response = self
                    .proto
                    .decode("blend.api.BattleFinishResponse", &bytes)
                    .map_err(|error| StateError::Descriptor(error.to_string()))?;
                Ok((response, GameplayMutationResult::Replay(bytes)))
            }
        }
    }

    pub fn battle_resume(&self, session: &Session) -> Result<DynamicMessage, StateError> {
        let active = self
            .store
            .active_battle(session.account_id)?
            .ok_or(StorageError::BattleInactive)?;
        let mut response = battle_resume_response(&self.proto, &active)?;
        let mut history = member_status(&response, "history")?;
        let mut state = member_status(&history, "previous_state")?;
        if current_battle_status(&state)? == BATTLE_STATUS_IN_BATTLE {
            let resources = self
                .proto
                .decode(
                    "blend.model.Resources",
                    &self.store.player_resources(session.account_id)?,
                )
                .map_err(|e| StateError::Descriptor(e.to_string()))?;
            let mut saved: home::HomeState =
                serde_json::from_slice(&self.store.home_state(session.account_id)?)
                    .map_err(|e| StateError::Descriptor(e.to_string()))?;
            saved.combat_effects.prepare(&state, &active.start_txid)?;
            saved.combat_effects.refresh(&self.proto, &mut state)?;
            let actor = current_actor(&state)?;
            let number = saved
                .combat_effects
                .next_action_number
                .max(i32_field(&state, "total_turn").ok_or(StateError::InvalidRequest)?);
            // The client resolves the displayed decision and wave by the same action number.
            // Rebuild previews from the persisted battle, without advancing or replaying an action.
            let setup = build_action_setup(
                &self.proto,
                &self.tutorial_rules,
                number,
                state.clone(),
                member_type(&actor)?,
                member_id(&actor)?,
                Some(&resources),
                Some(&saved.combat_effects),
            )?;
            let mut wave = empty_message(&self.proto, "blend.model.BattleWaveStart")?;
            wave.set_field_by_name("action_number", Value::I32(number));
            wave.set_field_by_name("state", Value::Message(state.clone()));
            history.set_field_by_name("wave_starts", Value::List(vec![Value::Message(wave)]));
            history.set_field_by_name("action_setups", Value::List(vec![Value::Message(setup)]));
            history.set_field_by_name("previous_state", Value::Message(state));
            response.set_field_by_name("history", Value::Message(history));
        }
        if let Some(context) = self
            .store
            .latest_battle_start_response(session.account_id)?
        {
            let start = self
                .proto
                .decode("blend.api.BattleStartResponse", &context)
                .map_err(|e| StateError::Descriptor(e.to_string()))?;
            let context = member_status(&start, "context")?;
            if string_field(&context, "start_txid").as_deref() != Some(active.start_txid.as_str()) {
                return Err(StateError::InvalidRequest);
            }
            response.set_field_by_name("context", Value::Message(context));
        }
        Ok(response)
    }

    pub fn battle_retire(
        &self,
        session: &Session,
        request_id: &str,
        request_bytes: &[u8],
    ) -> Result<(DynamicMessage, GameplayMutationResult), StateError> {
        const ROUTE: &str = "/battle/retire";
        if request_id.is_empty() || !request_bytes.is_empty() {
            return Err(StateError::InvalidRequest);
        }
        let fingerprint = request_fingerprint(ROUTE, request_bytes);
        let proto = self.proto.clone();
        let result = self.store.apply_battle_finish(
            session.account_id,
            request_id,
            ROUTE,
            &fingerprint,
            unix_now(),
            move |active, resources_blob, saved| {
                let mut home: home::HomeState = serde_json::from_slice(saved)
                    .map_err(|e| StorageError::Battle(e.to_string()))?;
                activities::retire_battle(&mut home.activities, active.quest_id as i32);
                home.gacha_battle_id = None;
                home.combat_effects = effects::Runtime::default();
                home.battle_progress = home::BattleProgress::default();
                let resources = proto
                    .decode("blend.model.Resources", resources_blob)
                    .map_err(|error| StorageError::Battle(error.to_string()))?;
                let response = empty_message(&proto, "blend.api.ChangedResourcesResponse")
                    .and_then(|mut response| {
                        response.set_field_by_name(
                            "changed_resources",
                            Value::Message(empty_message(&proto, "blend.model.Resources")?),
                        );
                        Ok(response)
                    })
                    .map_err(gameplay_storage_error)?;
                let character_index = message_list(&resources, "characters")
                    .iter()
                    .filter_map(|row| {
                        Some((
                            i64::from(i32_field(row, "character_id")?),
                            optional_i32_field(row, "memoria_entity_id").map(i64::from),
                        ))
                    })
                    .collect();
                Ok(BattleFinishCommit {
                    resources_blob: resources.encode_to_vec(),
                    response_plaintext: response.encode_to_vec(),
                    character_index,
                    home_state: Some(
                        serde_json::to_vec(&home)
                            .map_err(|e| StorageError::Battle(e.to_string()))?,
                    ),
                })
            },
        )?;
        gameplay_response(
            &self.store,
            &self.proto,
            session.account_id,
            request_id,
            ROUTE,
            &fingerprint,
            "blend.api.ChangedResourcesResponse",
            result,
        )
    }
}
