use super::prelude::*;

impl State {
    pub fn new(
        store: Store,
        proto: ProtoRegistry,
        config: LoadedConfig,
    ) -> Result<Self, StateError> {
        let fresh_rules = load_fresh_rules()?;
        let tutorial_rules = load_gameplay_rules()?;
        let synthesis_rules = load_synthesis_rules()?;
        let reward_rules = load_reward_rules()?;
        let home_rules = home::load_rules()?;
        let atelier_rules = atelier::load_rules()?;
        let activity_rules = activities::load_rules()?;
        let shop_rules = shop::load_rules()?;
        let character_rules = load_character_rules()?;
        let decoded_path = config
            .config
            .paths
            .master_data_decoded
            .as_ref()
            .ok_or_else(|| StateError::FreshRules("master_data_decoded path is required".into()))?;
        let decoded = std::fs::read(decoded_path)
            .map_err(|error| StateError::FreshRules(error.to_string()))?;
        let actual_hash = sha256_hex(&decoded);
        if actual_hash != home_rules.source_sha256 {
            return Err(StateError::MasterData("home rules source mismatch".into()));
        }
        if actual_hash != atelier_rules.source_sha256 {
            return Err(StateError::MasterData(
                "atelier rules source mismatch".into(),
            ));
        }
        if actual_hash != activity_rules.source_sha256 {
            return Err(StateError::MasterData(
                "activity rules source mismatch".into(),
            ));
        }
        if actual_hash != shop_rules.source_sha256 {
            return Err(StateError::MasterData("shop rules source mismatch".into()));
        }
        if actual_hash != fresh_rules.source_sha256 {
            return Err(StateError::FreshRules(format!(
                "master-data hash {actual_hash} does not match rules {}",
                fresh_rules.source_sha256
            )));
        }
        if actual_hash != tutorial_rules.source_sha256 {
            return Err(StateError::TutorialRules(format!(
                "master-data hash {actual_hash} does not match rules {}",
                tutorial_rules.source_sha256
            )));
        }
        if actual_hash != synthesis_rules.source_sha256 {
            return Err(StateError::SynthesisRules(format!(
                "master-data hash {actual_hash} does not match rules {}",
                synthesis_rules.source_sha256
            )));
        }
        if actual_hash != reward_rules.source_sha256 {
            return Err(StateError::RewardRules(format!(
                "master-data hash {actual_hash} does not match rules {}",
                reward_rules.source_sha256
            )));
        }
        if actual_hash != character_rules.source_sha256 {
            return Err(StateError::CharacterRules(format!(
                "master-data hash {actual_hash} does not match rules {}",
                character_rules.source_sha256
            )));
        }
        let bytes = std::fs::read(&config.config.paths.master_data)
            .map_err(|error| StateError::MasterData(error.to_string()))?;
        let mut master_data = proto
            .decode("blend.model.MasterData", &bytes)
            .map_err(|error| StateError::MasterData(error.to_string()))?;
        shop::local_master(&mut master_data, &shop_rules);
        Ok(Self {
            store,
            proto,
            config,
            master_data,
            fresh_rules,
            tutorial_rules,
            synthesis_rules,
            reward_rules,
            home_rules,
            atelier_rules,
            activity_rules,
            shop_rules,
            character_rules,
        })
    }

    pub(crate) fn ensure_noncombat_mutation_allowed(
        &self,
        account_id: i64,
    ) -> Result<(), StateError> {
        let active = self.store.active_battle(account_id)?.is_some();
        if BattleMutationPolicy::RejectWhileActive.permits_noncombat(active)
            == BattleResult::Rejected
        {
            return Err(StateError::Storage(StorageError::ActiveBattleExists));
        }
        Ok(())
    }

    pub fn create_account(&self, username: &str, password: &str) -> Result<i64, StateError> {
        validate_username(username)?;
        if password.len() < 8 {
            return Err(StateError::Credentials);
        }
        let salt = SaltString::generate(&mut OsRng);
        let hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|_| StateError::Credentials)?
            .to_string();
        Ok(self.store.create_account(username, &hash)?)
    }

    pub fn login_grant(&self, username: &str, password: &str) -> Result<String, StateError> {
        let Some((account_id, encoded)) = self.store.account_by_username(username)? else {
            return Err(StateError::Credentials);
        };
        let parsed = PasswordHash::new(&encoded).map_err(|_| StateError::Credentials)?;
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .map_err(|_| StateError::Credentials)?;
        let grant = Uuid::new_v4().to_string();
        let now = unix_now();
        self.store.issue_grant(
            account_id,
            grant.as_bytes(),
            now,
            now + self.config.config.account.handoff_ttl_seconds as i64,
        )?;
        Ok(grant)
    }

    pub fn sign_in(&self, request: &DynamicMessage) -> Result<SignInResult, StateError> {
        let secret = string_field(request, "device_secret").ok_or(StateError::InvalidRequest)?;
        let device_id =
            string_field(request, "device_unique_id").ok_or(StateError::InvalidRequest)?;
        let device_model =
            string_field(request, "device_model").ok_or(StateError::InvalidRequest)?;
        if secret.is_empty() || device_id.is_empty() || device_model.is_empty() {
            return Err(StateError::InvalidRequest);
        }

        let now = unix_now();
        let binding = device_binding_hash(&self.config.server_secret, &device_id);
        let resources = starter_resources(&self.proto, &self.fresh_rules)?;
        let token = Uuid::new_v4().to_string();
        let session = self
            .store
            .consume_grant_and_initialize(
                secret.as_bytes(),
                &binding,
                &resources.encode_to_vec(),
                &self.config.config.versions.master_data,
                i64::from(self.fresh_rules.initial_character.id),
                token.as_bytes(),
                now,
                now + self.config.config.session.ttl_seconds as i64,
            )?
            .ok_or(StateError::InvalidGrant)?;
        let saved: home::HomeState =
            serde_json::from_slice(&self.store.home_state(session.account_id)?)
                .map_err(|error| StateError::Descriptor(error.to_string()))?;
        Ok(SignInResult {
            session_token: token,
            user_id: session.user_id,
            language: saved.language.unwrap_or(1),
        })
    }

    pub fn session(&self, token: &str, user_id: Option<i64>) -> Result<Session, StateError> {
        let session = self
            .store
            .resolve_session(token.as_bytes(), unix_now())?
            .ok_or(StateError::InvalidSession)?;
        if user_id != Some(session.user_id) {
            return Err(StateError::InvalidSession);
        }
        Ok(session)
    }

    pub fn login_state(&self, account_id: i64) -> Result<DynamicMessage, StateError> {
        self.refresh_home(account_id)?;
        let stored = self.store.player_resources(account_id)?;
        let mut resources = if stored.is_empty() {
            starter_resources(&self.proto, &self.fresh_rules)?
        } else {
            self.proto
                .decode("blend.model.Resources", &stored)
                .map_err(|error| StateError::Descriptor(error.to_string()))?
        };
        for (character_id, memoria) in self.store.characters(account_id)? {
            upsert_character(
                &mut resources,
                character_message(&self.proto, character_id, memoria, None, 1, 10)?,
                true,
            );
        }

        let mut response = self
            .proto
            .empty("blend.api.UserLogInResponse")
            .map_err(|error| StateError::Descriptor(error.to_string()))?;
        response.set_field_by_name("resources", Value::Message(resources));
        response.set_field_by_name("master_data", Value::Message(self.master_data.clone()));
        response.set_field_by_name(
            "created_at",
            Value::Message(timestamp(&self.proto, unix_now())?),
        );
        response.set_field_by_name("tool_conversion_limit_count", Value::I32(100));
        response.set_field_by_name("auto_download_asset_size_limit_mb", Value::I32(20));
        if self.store.active_battle(account_id)?.is_some() {
            response.set_field_by_name("battle_resume_status", Value::EnumNumber(1));
        }
        Ok(response)
    }
}
