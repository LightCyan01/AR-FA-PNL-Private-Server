impl Player {
    pub(super) fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut config: Config =
            toml::from_str(include_str!("../../../../config.example.toml")).unwrap();
        config.paths.protoset = root.join("../schemas/atelier-resleriana-2.16.0.protoset");
        config.paths.master_data = root.join("../data/master_data.pb");
        config.paths.master_data_decoded = Some(root.join("../data/masterdata.jp.decoded"));
        let path = std::env::temp_dir().join(format!("atelier-natural-{}.sqlite3", Uuid::new_v4()));
        config.storage.path = path.clone();
        let loaded = LoadedConfig {
            api_addr: "127.0.0.1:0".parse().unwrap(),
            asset_addr: "127.0.0.1:0".parse().unwrap(),
            config,
            server_secret: b"isolated-natural-player-test".to_vec(),
            config_path: root.join("config.example.toml"),
        };
        let proto = ProtoRegistry::from_file(&loaded.config.paths.protoset).unwrap();
        let state = Arc::new(State::new(Store::open(&path).unwrap(), proto, loaded).unwrap());
        state
            .create_account("natural_player", "test-password")
            .unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let assets = runtime
            .block_on(AssetService::load(Arc::new(state.config.clone())))
            .unwrap();
        let api = ApiService::new(state.clone(), assets);
        let client_resources = empty_message(&state.proto, "blend.model.Resources").unwrap();
        let mut player = Self {
            state,
            api,
            runtime,
            token: String::new(),
            client_resources,
            history: None,
            session: Session {
                account_id: 0,
                user_id: 0,
            },
            path,
            actions: Vec::new(),
        };
        player.authenticate();
        player.read_api("/user/log_in", "blend.api.UserLogInResponse");
        player
    }

    pub(super) fn authenticate(&mut self) -> DynamicMessage {
        // Account creation and this grant are the ordinary local launcher boundary.
        // The client sign-in itself must use the encrypted production HTTP path.
        let grant = self
            .state
            .login_grant("natural_player", "test-password")
            .unwrap();
        let mut request = empty_message(&self.state.proto, "blend.api.AuthSignInRequest").unwrap();
        for (field, value) in [
            ("device_secret", grant.as_str()),
            ("device_unique_id", "simulation-device"),
            ("device_token", "disposable-steam-device-token"),
            ("device_model", "Steam simulation"),
        ] {
            request.set_field_by_name(field, Value::String(value.into()));
        }
        let auth = self
            .exchange(
                "/auth/sign_in",
                &Uuid::new_v4().to_string(),
                &request,
                "blend.api.AuthSignInResponse",
            )
            .unwrap();
        self.token = string_field(&auth, "session_token").unwrap();
        let user_id = auth.get_field_by_name("user_id").unwrap().as_i64().unwrap();
        self.session = Session {
            account_id: user_id,
            user_id,
        };
        auth
    }

    pub(super) fn resources(&self) -> DynamicMessage {
        self.client_resources.clone()
    }

    pub(super) fn exchange(
        &self,
        route: &str,
        id: &str,
        request: &DynamicMessage,
        output: &str,
    ) -> Result<DynamicMessage, StateError> {
        let plaintext = request.encode_to_vec();
        let body = if plaintext.is_empty() {
            Vec::new()
        } else {
            encrypt_frame(0x91, &plaintext).unwrap()
        };
        self.runtime.block_on(async {
            let versions = &self.state.config.config.versions;
            let mut http = Request::post(route)
                .header("content-type", "application/octet-stream")
                .header("x-platform", "Steam")
                .header("x-client-version", &versions.client)
                .header("x-asset-version", &versions.asset)
                .header("x-master-data-version", &versions.master_data)
                .header("x-request-id", id);
            if route != "/auth/sign_in" {
                http = http
                    .header("x-user-id", self.session.user_id)
                    .header("x-session-token", &self.token);
            }
            let response = self.api.handle(http.body(full(body)).unwrap()).await;
            let status = response.status();
            let encrypted = response
                .headers()
                .get("content-type")
                .is_some_and(|v| v.to_str().unwrap().contains("application/octet-stream"));
            assert!(
                response.headers().get("x-content-encoding").is_none(),
                "test decoder must not silently ignore compression"
            );
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            if status != 200 {
                return Err(StateError::Descriptor(format!(
                    "HTTP {status} {route}: {}",
                    String::from_utf8_lossy(&bytes)
                )));
            }
            assert!(encrypted, "{route} did not use encrypted protobuf");
            let decoded = if bytes.is_empty() {
                Vec::new()
            } else {
                decrypt_frame(&bytes).unwrap().1
            };
            self.state
                .proto
                .decode(output, &decoded)
                .map_err(|e| StateError::Descriptor(e.to_string()))
        })
    }

    pub(super) fn read_api(&mut self, route: &str, output: &str) -> DynamicMessage {
        let response = self
            .exchange(
                route,
                &Uuid::new_v4().to_string(),
                &self.message("google.protobuf.Empty", &[]),
                output,
            )
            .unwrap();
        self.merge_response(&response);
        response
    }

    pub(super) fn merge_response(&mut self, response: &DynamicMessage) {
        if let Ok(resources) = member_status(response, "resources") {
            self.client_resources = resources;
        } else if let Ok(changed) = member_status(response, "changed_resources") {
            for (field, value) in changed.fields() {
                if let Value::List(rows) = value {
                    let keys: Vec<&str> = match field.name() {
                        "party_members" => vec!["party_type", "number", "position"],
                        "parties" => vec!["party_type", "number"],
                        "rental_party_members" => {
                            vec!["party_type", "fixed_party_id", "character_id"]
                        }
                        "rental_parties" => vec!["party_type", "fixed_party_id"],
                        "exploration_progresses" => vec!["is_story"],
                        "shop_product_states" => vec!["shop_product_id"],
                        _ => vec![],
                    };
                    let mut current = message_list(&self.client_resources, field.name());
                    for value in rows {
                        let row = value.as_message().expect("resource row");
                        let descriptor = row.descriptor();
                        let first = descriptor.fields().next().unwrap();
                        let keys = if keys.is_empty() {
                            vec![first.name()]
                        } else {
                            keys.clone()
                        };
                        current.retain(|old| {
                            !keys
                                .iter()
                                .all(|key| old.get_field_by_name(key) == row.get_field_by_name(key))
                        });
                        // quest_id=0 is the explicit empty exploration slot, keyed by is_story.
                        current.push(row.clone());
                    }
                    self.client_resources.set_field(
                        &field,
                        Value::List(current.into_iter().map(Value::Message).collect()),
                    );
                } else {
                    // A present aggregate, especially Status, replaces the whole value including defaults.
                    self.client_resources.set_field(&field, value.clone());
                }
            }
        }
        if let Ok(deleted) = member_status(response, "deleted_resources") {
            for (field, ids) in [
                ("equipment_tools", "equipment_tool_entity_ids"),
                ("battle_tools", "battle_tool_entity_ids"),
                ("memorias", "memoria_entity_ids"),
            ] {
                let ids = i32_list(&deleted, ids);
                let rows = message_list(&self.client_resources, field)
                    .into_iter()
                    .filter(|row| !ids.contains(&i32_field(row, "entity_id").unwrap()))
                    .map(Value::Message)
                    .collect();
                self.client_resources
                    .set_field_by_name(field, Value::List(rows));
            }
        }
        if let Ok(history) = member_status(response, "history") {
            self.history = Some(history);
        }
    }

    pub(super) fn assert_client_resources(&self, persisted: &[u8], route: &str) {
        let persisted = self
            .state
            .proto
            .decode("blend.model.Resources", persisted)
            .unwrap();
        for field in persisted.descriptor().fields() {
            let canonical = |m: &DynamicMessage| -> Vec<Vec<u8>> {
                let mut rows = message_list(m, field.name())
                    .iter()
                    .map(Message::encode_to_vec)
                    .collect::<Vec<_>>();
                rows.sort();
                rows
            };
            if field.is_list() {
                if canonical(&self.client_resources) != canonical(&persisted) {
                    let actual = message_list(&persisted, field.name());
                    let received = message_list(&self.client_resources, field.name());
                    eprintln!(
                        "{route} field={} client_only={:?} stored_only={:?}",
                        field.name(),
                        received
                            .iter()
                            .filter(|r| !actual.contains(r))
                            .take(3)
                            .map(ToString::to_string)
                            .collect::<Vec<_>>(),
                        actual
                            .iter()
                            .filter(|r| !received.contains(r))
                            .take(3)
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                    );
                }
                assert!(
                    canonical(&self.client_resources) == canonical(&persisted),
                    "{route}: client/persistence mismatch in {}",
                    field.name()
                );
            } else {
                assert!(
                    self.client_resources.get_field(&field) == persisted.get_field(&field),
                    "{route}: client/persistence mismatch in {}",
                    field.name()
                );
            }
        }
    }

    pub(super) fn message(&self, name: &str, fields: &[(&str, Value)]) -> DynamicMessage {
        let mut request = empty_message(&self.state.proto, name).unwrap();
        for (field, value) in fields {
            request.set_field_by_name(field, value.clone());
        }
        request
    }

    pub(super) fn dispatch(
        &self,
        route: &'static str,
        id: &str,
        request: &DynamicMessage,
    ) -> Result<DynamicMessage, StateError> {
        let output = crate::api::route_spec(route)
            .and_then(|spec| spec.response)
            .expect("registered protobuf route");
        self.exchange(route, id, request, output)
    }

    pub(super) fn call(&mut self, route: &'static str, request: DynamicMessage) -> DynamicMessage {
        let id = Uuid::new_v4().to_string();
        let response = self.dispatch(route, &id, &request).unwrap_or_else(|e| {
            panic!(
                "{route} after {} actions: {e:?}; request={request}",
                self.actions.len()
            )
        });
        let committed = self
            .state
            .store
            .player_resources(self.session.account_id)
            .unwrap();
        let battle = self
            .state
            .store
            .active_battle(self.session.account_id)
            .unwrap()
            .map(|b| b.state_blob);
        let retry = self.dispatch(route, &id, &request).unwrap();
        assert_eq!(response.encode_to_vec(), retry.encode_to_vec());
        assert_eq!(
            committed,
            self.state
                .store
                .player_resources(self.session.account_id)
                .unwrap()
        );
        assert_eq!(
            battle,
            self.state
                .store
                .active_battle(self.session.account_id)
                .unwrap()
                .map(|b| b.state_blob)
        );
        self.merge_response(&response);
        self.assert_client_resources(&committed, route);
        self.actions.push(format!(
            "{route} request={request} response_bytes={} retry=identical persisted=matched",
            response.encoded_len()
        ));
        response
    }

    pub(super) fn home(&mut self, route: &'static str, fields: &[(&str, Value)]) -> DynamicMessage {
        let name = crate::api::route_spec(route)
            .and_then(|spec| spec.request)
            .expect("registered protobuf route");
        self.call(route, self.message(name, fields))
    }

    pub(super) fn talk(&mut self, quest: i32) {
        self.call(
            "/quest/talk_event/finish",
            self.message(
                "blend.api.QuestTalkEventFinishRequest",
                &[("quest_id", Value::I32(quest))],
            ),
        );
    }

    pub(super) fn synthesize(&mut self, recipe: i32) {
        self.call(
            "/synthesis/bulk_execute",
            self.message(
                "blend.api.SynthesisBulkExecuteRequest",
                &[
                    ("recipe_id", Value::I32(recipe)),
                    (
                        "character_ids",
                        Value::List(vec![Value::I32(32201), Value::I32(43101)]),
                    ),
                    (
                        "ingredient_id",
                        Value::Message(int32_value(&self.state.proto, 67).unwrap()),
                    ),
                    ("count", Value::I32(1)),
                ],
            ),
        );
    }

    pub(super) fn craft(&mut self, recipe: i32, count: i32) {
        self.home("/recipe/learn", &[]);
        let resources = self.resources();
        let catalog = load_synthesis_rules().unwrap();
        let rules = &catalog;
        let spec = rules.recipes.iter().find(|r| r.id == recipe).unwrap();
        let ingredient = rules
            .ingredients
            .iter()
            .find(|i| {
                item_quantity(&resources, i.id).unwrap_or(0) > count
                    && if spec.target.resource_type == 6 {
                        !i.equipment_trait_ids.is_empty()
                    } else {
                        !i.battle_trait_ids.is_empty()
                    }
            })
            .unwrap()
            .id;
        self.call(
            "/synthesis/bulk_execute",
            self.message(
                "blend.api.SynthesisBulkExecuteRequest",
                &[
                    ("recipe_id", Value::I32(recipe)),
                    (
                        "character_ids",
                        Value::List(vec![Value::I32(32201), Value::I32(43101)]),
                    ),
                    (
                        "ingredient_id",
                        Value::Message(int32_value(&self.state.proto, ingredient).unwrap()),
                    ),
                    ("count", Value::I32(count)),
                ],
            ),
        );
    }

    pub(super) fn party(&mut self, equip_memoria: bool) {
        let resources = self.resources();
        let mut members = Vec::new();
        for id in [43101, 32201] {
            let mut member = self.message(
                "blend.model.PartyMemberWithEquipment",
                &[(
                    "character_id",
                    Value::Message(int32_value(&self.state.proto, id).unwrap()),
                )],
            );
            if equip_memoria && id == 43101 {
                let entity =
                    i32_field(&message_list(&resources, "memorias")[0], "entity_id").unwrap();
                member.set_field_by_name(
                    "memoria_entity_id",
                    Value::Message(int32_value(&self.state.proto, entity).unwrap()),
                );
            }
            members.push(Value::Message(member));
        }
        let tools = [3, 1]
            .iter()
            .filter_map(|id| {
                message_list(&resources, "battle_tools")
                    .iter()
                    .filter(|t| i32_field(t, "tool_id") == Some(*id))
                    .filter_map(|t| i32_field(t, "entity_id"))
                    .max()
            })
            .map(Value::I32)
            .collect();
        self.call(
            "/party/bulk_update",
            self.message(
                "blend.api.PartyBulkUpdateRequest",
                &[
                    ("party_type", Value::I32(1)),
                    ("number", Value::I32(1)),
                    ("leader_position", Value::I32(1)),
                    ("members", Value::List(members)),
                    ("battle_tool_entity_ids", Value::List(tools)),
                ],
            ),
        );
    }

    pub(super) fn battle(&mut self, quest: i32) {
        self.call(
            "/quest/battle/start",
            self.message(
                "blend.api.QuestBattleStartRequest",
                &[
                    ("quest_id", Value::I32(quest)),
                    ("party_number", Value::I32(1)),
                    (
                        "enemy_weak_level",
                        Value::I32(if quest == 101002006 { 2 } else { 0 }),
                    ),
                ],
            ),
        );
        self.fight(quest);
        assert!(quest_clear_count(&self.resources(), quest) > 0);
    }

    pub(super) fn fight(&mut self, quest: i32) {
        self.read_api("/battle/resume", "blend.api.BattleResumeResponse");
        for turn in 0..500 {
            let history = self.history.as_ref().expect("decoded battle history");
            let status = i32_or_enum_field(history, "status").unwrap();
            if status == BATTLE_STATUS_WON {
                break;
            }
            assert_eq!(
                status, BATTLE_STATUS_IN_BATTLE,
                "quest {quest}, turn {turn}"
            );
            let setup = message_list(history, "action_setups")
                .pop()
                .expect("advertised player decision");
            let battle = member_status(&setup, "state").unwrap();
            let tools = message_list(&setup, "battle_tool_selections");
            if let Some(tool) = tools.iter().find(|tool| {
                message_list(tool, "targets").iter().any(|target| {
                    let id = i32_field(target, "target_id").unwrap();
                    message_i32_field(target, "hp_heal", "value").unwrap_or(0) > 0
                        && message_list(&battle, "members").iter().any(|m| {
                            member_id(m).unwrap() == id
                                && i32_field(m, "hp").unwrap() * 5
                                    < i32_field(m, "max_hp").unwrap() * 3
                        })
                })
            }) {
                let command = self.message(
                    "blend.model.BattleBattleToolCommand",
                    &[(
                        "battle_tool_numbers",
                        Value::List(vec![Value::I32(i32_field(tool, "number").unwrap())]),
                    )],
                );
                self.call(
                    "/battle/attack",
                    self.message(
                        "blend.api.BattleAttackRequest",
                        &[
                            ("mode", Value::EnumNumber(1)),
                            ("battle_tool_command", Value::Message(command)),
                        ],
                    ),
                );
                continue;
            }
            let selections = message_list(&setup, "skill_selections");
            let mut choice = None;
            for selection in selections {
                let kind = i32_field(&selection, "skill_type").unwrap();
                for target in message_list(&selection, "targets") {
                    let id = i32_field(&target, "target_id").unwrap();
                    let member = message_list(&battle, "members")
                        .into_iter()
                        .find(|m| member_id(m).unwrap() == id)
                        .unwrap();
                    let damage = message_i64_field(&target, "hp_damage", "value").unwrap_or(0);
                    let heal = message_i32_field(&target, "hp_heal", "value").unwrap_or(0);
                    let break_damage =
                        message_i32_field(&target, "break_damage", "value").unwrap_or(0);
                    let gauge = member_status(&member, "enemy")
                        .ok()
                        .and_then(|m| i32_field(&m, "break_gauge"))
                        .unwrap_or(0);
                    let score = damage.min(i64::from(i32_field(&member, "hp").unwrap_or(0)))
                        + if bool_field(&target, "is_killed") {
                            50_000
                        } else {
                            0
                        }
                        + if gauge > 0 && break_damage >= gauge {
                            10_000
                        } else {
                            i64::from(break_damage) * 4
                        }
                        + if heal > 0
                            && i32_field(&member, "hp").unwrap_or(0) * 2
                                < i32_field(&member, "max_hp").unwrap_or(0)
                        {
                            20_000 + i64::from(heal)
                        } else {
                            0
                        }
                        + if bool_field(&selection, "is_all") {
                            damage * 2
                        } else {
                            0
                        };
                    if choice.is_none_or(|(best, _, _)| score > best) {
                        choice = Some((score, kind, id));
                    }
                }
            }
            let (_, skill_type, target) = choice.expect("a legal action exists");
            let command = self.message(
                "blend.model.BattleSkillCommand",
                &[
                    ("skill_type", Value::I32(skill_type)),
                    ("main_target_id", Value::I32(target)),
                ],
            );
            self.call(
                "/battle/attack",
                self.message(
                    "blend.api.BattleAttackRequest",
                    &[
                        ("mode", Value::EnumNumber(0)),
                        ("skill_command", Value::Message(command)),
                    ],
                ),
            );
        }
        let resume = self.read_api("/battle/resume", "blend.api.BattleResumeResponse");
        assert_eq!(
            i32_or_enum_field(&member_status(&resume, "history").unwrap(), "status"),
            Some(BATTLE_STATUS_WON)
        );
        self.call("/battle/finish", self.message("google.protobuf.Empty", &[]));
    }

    pub(super) fn explore(&mut self, quest: i32) {
        self.home(
            "/exploration/start",
            &[
                ("quest_id", Value::I32(quest)),
                ("party_number", Value::I32(1)),
            ],
        );
        self.reopen();
        for _ in 0..200 {
            let progress = message_list(&self.resources(), "exploration_progresses")
                .into_iter()
                .find(|p| i32_field(p, "quest_id") == Some(quest))
                .unwrap();
            let types = progress
                .get_field_by_name("route_types")
                .unwrap()
                .as_list()
                .unwrap()
                .iter()
                .map(|v| v.as_enum_number().unwrap())
                .collect::<Vec<_>>();
            let indices = i32_list(&progress, "route_indices");
            let Some(current) = indices.iter().position(|i| *i >= 0) else {
                break;
            };
            let area = i32_field(&progress, "area_id").unwrap();
            match types[current] {
                0 => {
                    let point = i32_list(&progress, "gathering_states")
                        .iter()
                        .position(|v| *v == 0)
                        .unwrap();
                    self.home(
                        "/exploration/explore",
                        &[
                            ("area_id", Value::I32(area)),
                            ("gathering_index", Value::I32(point as i32)),
                        ],
                    );
                }
                1 => {
                    let party = member_status(&progress, "party_status").unwrap();
                    let response = self.call(
                        "/exploration/battle_start",
                        self.message(
                            "blend.api.ExplorationBattleStartRequest",
                            &[("area_id", Value::I32(area))],
                        ),
                    );
                    self.reopen();
                    assert_eq!(
                        member_status(&response, "context").unwrap(),
                        member_status(
                            &self.read_api("/battle/resume", "blend.api.BattleResumeResponse"),
                            "context"
                        )
                        .unwrap()
                    );
                    let battle =
                        member_status(self.history.as_ref().unwrap(), "previous_state").unwrap();
                    assert_eq!(
                        i32_field(&battle, "party_gauge"),
                        i32_field(&party, "party_gauge")
                    );
                    self.fight(quest);
                    assert_eq!(
                        quest_clear_count(&self.resources(), quest),
                        0,
                        "sub-battle cannot clear exploration"
                    );
                }
                _ => panic!("completed talks should be marked executed"),
            }
        }
        self.home("/exploration/finish", &[("quest_id", Value::I32(quest))]);
        assert_eq!(quest_clear_count(&self.resources(), quest), 1);
    }

    pub(super) fn reopen(&mut self) {
        // Independently verify the response-built cache before replacing it with relog state.
        self.assert_client_resources(
            &self
                .state
                .store
                .player_resources(self.session.account_id)
                .unwrap(),
            "before relog",
        );
        let store = Store::open(&self.path).unwrap();
        self.state = Arc::new(
            State::new(store, self.state.proto.clone(), self.state.config.clone()).unwrap(),
        );
        let assets = self
            .runtime
            .block_on(AssetService::load(Arc::new(self.state.config.clone())))
            .unwrap();
        self.api = ApiService::new(self.state.clone(), assets);
        self.authenticate();
        self.read_api("/user/log_in", "blend.api.UserLogInResponse");
        self.assert_client_resources(
            &self
                .state
                .store
                .player_resources(self.session.account_id)
                .unwrap(),
            "after relog",
        );
        self.actions
            .push("restart/relog encrypted persisted=matched".into());
    }

    pub(super) fn wait_hours(&mut self, hours: i64) {
        assert!(hours > 0);
        set_simulation_now(Some(unix_now() + hours * 3600));
        // Time advancement also expires genuine server sessions. Reacquire through
        // the same launcher/auth path instead of bypassing HTTP session validation.
        self.authenticate();
        self.home("/login_bonus/receive", &[]);
        self.actions.push(format!("wait {hours} hours"));
    }

    pub(super) fn tutorial(&mut self) {
        let player = self;
        assert_eq!(message_list(&player.resources(), "characters").len(), 1);
        player.talk(101001001);
        player.battle(101001002);
        player.talk(101001003);
        player.synthesize(6);
        for quest in 101001004..=101001007 {
            player.talk(quest);
        }
        player.battle(101001008);
        player.talk(101001009);
        player.talk(101001010);
        player.synthesize(2);
        player.talk(101001011);
        player.talk(101001012);
        player.party(false);
        player.battle(101001013);
        player.talk(101001014);
        player.party(true);
        player.battle(101001015);
        player.talk(101001016);
        player.talk(101001017);
        let catalog = player
            .dispatch(
                "/gacha/list",
                &Uuid::new_v4().to_string(),
                &player.message(
                    "blend.api.GachaListRequest",
                    &[("gacha_category", Value::I32(1))],
                ),
            )
            .unwrap();
        player.merge_response(&catalog);
        assert!(message_list(&catalog, "gachas")
            .iter()
            .any(|g| i32_field(g, "gacha_id") == Some(2)));
        player.call(
            "/gacha/execute",
            player.message(
                "blend.api.GachaExecuteRequest",
                &[
                    ("gacha_id", Value::I32(2)),
                    ("gacha_button_id", Value::I32(14)),
                ],
            ),
        );
        player.home("/login_bonus/receive", &[]);
        player.home("/tutorial/progress", &[]);
        player.home("/recipe/learn", &[]);
        player.reopen();
        assert!((101001001..=101001017).all(|id| quest_clear_count(&player.resources(), id) == 1));
    }

    pub(super) fn prepare(&mut self) {
        // Static master projections are read independently; player state comes only from responses.
        let character_catalog = load_character_rules().unwrap();
        let list = self.home("/mail/list", &[]);
        let ids = message_list(&member_status(&list, "list").unwrap(), "unopened")
            .iter()
            .filter_map(|m| i32_field(m, "entity_id"))
            .map(Value::I32)
            .collect::<Vec<_>>();
        if !ids.is_empty() {
            self.home("/mail/open", &[("entity_ids", Value::List(ids))]);
        }
        let catalog: serde_json::Value =
            serde_json::from_str(include_str!("../../../../../data/home_rules.json")).unwrap();
        for _ in 0..20 {
            let resources = self.resources();
            let missions = message_list(&resources, "missions");
            let ready = catalog["missions"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|r| {
                    let id = r["id"].as_i64().unwrap() as i32;
                    let Some(m) = missions
                        .iter()
                        .find(|m| i32_field(m, "mission_id") == Some(id))
                    else {
                        return false;
                    };
                    let index = i32_field(m, "received_step_count").unwrap_or(0) as usize;
                    let Some(step) = r["steps"].as_array().unwrap().get(index) else {
                        return false;
                    };
                    i64::from(i32_field(m, "count").unwrap_or(0)) >= step["count"].as_i64().unwrap()
                        && r["prev_mission_id"].as_i64().is_none_or(|prev| {
                            catalog["missions"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .find(|p| p["id"].as_i64() == Some(prev))
                                .is_some_and(|p| {
                                    missions
                                        .iter()
                                        .find(|s| i32_field(s, "mission_id") == Some(prev as i32))
                                        .is_some_and(|s| {
                                            i32_field(s, "received_step_count").unwrap_or(0)
                                                as usize
                                                >= p["steps"].as_array().unwrap().len()
                                        })
                                })
                        })
                        && r["start_at"].as_i64().is_none_or(|t| unix_now() >= t)
                        && r["end_at"].as_i64().is_none_or(|t| unix_now() < t)
                })
                .map(|r| Value::I32(r["id"].as_i64().unwrap() as i32))
                .collect::<Vec<_>>();
            if ready.is_empty() {
                break;
            }
            self.home(
                "/mission/receive",
                &[
                    ("mission_ids", Value::List(ready)),
                    ("bulk_receive", Value::Bool(true)),
                ],
            );
        }
        for character in message_list(&self.resources(), "characters") {
            let id = i32_field(&character, "character_id").unwrap();
            for _ in 0..24 {
                let resources = self.resources();
                let owned = resource_character(&resources, id).unwrap();
                let current = i32_field(&owned, "growboard_current_page").unwrap_or(1);
                let bits = i32_field(&owned, "growboard_panel_bits").unwrap_or(0);
                let spec = character_catalog
                    .characters
                    .iter()
                    .find(|c| c.id == id)
                    .unwrap();
                let board = character_catalog
                    .boards
                    .iter()
                    .find(|b| b.id == spec.growboard_id)
                    .unwrap();
                let page = character_catalog
                    .pages
                    .iter()
                    .find(|p| Some(&p.id) == board.page_ids.get(current as usize - 1))
                    .unwrap();
                if bits == (1 << page.panel_ids.len()) - 1 {
                    if current >= 3 {
                        break;
                    }
                    self.home(
                        "/character/growboard_page_release",
                        &[("character_id", Value::I32(id))],
                    );
                    continue;
                }
                let cole = i32_field(&status_message(&resources).unwrap(), "cole").unwrap_or(0);
                let next = page
                    .panel_ids
                    .iter()
                    .enumerate()
                    .filter(|(n, _)| bits & (1 << n) == 0)
                    .find(|(_, id)| {
                        let p = character_catalog
                            .panels
                            .iter()
                            .find(|p| p.id == **id)
                            .unwrap();
                        p.cole_cost <= cole
                            && p.item_costs.iter().all(|cost| {
                                item_quantity(&resources, cost.id).unwrap_or(0) >= cost.quantity
                            })
                    })
                    .map(|(n, _)| bits | (1 << n));
                let Some(next) = next else {
                    break;
                };
                self.home(
                    "/character/growboard_bulk_release",
                    &[
                        ("character_id", Value::I32(id)),
                        ("target_page", Value::I32(current)),
                        ("panel_bits", Value::I32(next)),
                    ],
                );
            }
            let cap = i32_field(
                &resource_character(&self.resources(), id).unwrap(),
                "growboard_level_limit",
            )
            .unwrap_or(10);
            let cap_exp = character_catalog
                .levels
                .iter()
                .find(|l| l.level == cap + 1)
                .unwrap()
                .exp
                - 1;
            for _ in 0..20 {
                let resources = self.resources();
                let exp =
                    i32_field(&resource_character(&resources, id).unwrap(), "exp").unwrap_or(0);
                if exp >= cap_exp {
                    break;
                }
                let cole = i32_field(&status_message(&resources).unwrap(), "cole").unwrap_or(0);
                let Some(item) = character_catalog
                    .exp_items
                    .iter()
                    .filter(|r| {
                        item_quantity(&resources, r.id).unwrap_or(0) > 0 && r.value / 10 <= cole
                    })
                    .min_by_key(|r| r.value)
                else {
                    break;
                };
                let consumed = self.message(
                    "blend.model.ConsumedItem",
                    &[
                        ("item_id", Value::I32(item.id)),
                        ("quantity", Value::I32(1)),
                    ],
                );
                self.home(
                    "/character/enhance",
                    &[
                        ("character_id", Value::I32(id)),
                        (
                            "consumed_items",
                            Value::List(vec![Value::Message(consumed)]),
                        ),
                    ],
                );
            }
        }
        let resources = self.resources();
        let memorias = message_list(&resources, "memorias");
        let members = message_list(&resources, "characters")
            .iter()
            .take(5)
            .enumerate()
            .map(|(index, c)| {
                let mut member = self.message(
                    "blend.model.PartyMemberWithEquipment",
                    &[(
                        "character_id",
                        Value::Message(
                            int32_value(&self.state.proto, i32_field(c, "character_id").unwrap())
                                .unwrap(),
                        ),
                    )],
                );
                if let Some(entity) = memorias.get(index).and_then(|m| i32_field(m, "entity_id")) {
                    member.set_field_by_name(
                        "memoria_entity_id",
                        Value::Message(int32_value(&self.state.proto, entity).unwrap()),
                    );
                }
                Value::Message(member)
            })
            .collect();
        let tools = [1, 3]
            .iter()
            .filter_map(|id| {
                message_list(&resources, "battle_tools")
                    .iter()
                    .filter(|t| i32_field(t, "tool_id") == Some(*id))
                    .filter_map(|t| i32_field(t, "entity_id"))
                    .max()
            })
            .map(Value::I32)
            .collect();
        self.call(
            "/party/bulk_update",
            self.message(
                "blend.api.PartyBulkUpdateRequest",
                &[
                    ("party_type", Value::I32(1)),
                    ("number", Value::I32(1)),
                    ("leader_position", Value::I32(1)),
                    ("members", Value::List(members)),
                    ("battle_tool_entity_ids", Value::List(tools)),
                ],
            ),
        );
    }
}
use super::*;
