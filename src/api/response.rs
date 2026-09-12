// Protocol and domain response handlers.

use hyper::{body::Incoming, Request, Response, StatusCode};
use prost_reflect::Value;
use serde_json::json;
use uuid::Uuid;

use super::{
    request::{
        empty_encrypted_response, encrypted_bytes, json_error, json_response, proto_response,
        state_error, RequestHeaders,
    },
    ApiService,
};
use crate::{
    state::StateError,
    storage::GameplayMutationResult,
    transport::{decrypt_frame, AppBody},
};

impl ApiService {
    pub(super) fn home_request(
        &self,
        body: &[u8],
        headers: &RequestHeaders,
        route: &str,
        input: &str,
        output: &str,
    ) -> Response<AppBody> {
        let session = match headers
            .session_token
            .as_deref()
            .ok_or(StateError::InvalidSession)
            .and_then(|token| self.state.session(token, headers.user_id))
        {
            Ok(session) => session,
            Err(error) => return state_error(error),
        };
        let (prefix, plaintext) = if body.is_empty() && input == "google.protobuf.Empty" {
            (None, Vec::new())
        } else {
            match decrypt_frame(body) {
                Ok((prefix, bytes)) => (Some(prefix), bytes),
                Err(_) => return json_error(StatusCode::BAD_REQUEST, "invalid_format"),
            }
        };
        let request = match self.state.proto.decode(input, &plaintext) {
            Ok(value) => value,
            Err(_) => return json_error(StatusCode::BAD_REQUEST, "invalid_format"),
        };
        if route == "/web_session/token" {
            let mut response = match self.state.proto.empty(output) {
                Ok(value) => value,
                Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "server_error"),
            };
            response.set_field_by_name(
                "token",
                Value::String(format!("local-web-{}", Uuid::new_v4())),
            );
            return proto_response(
                &response,
                prefix,
                &self.state.config.config.transport.response_prefix_policy,
            );
        }
        let id = headers
            .request_id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        match self
            .state
            .home_request(&session, &id, &plaintext, route, &request, output)
        {
            Ok((response, GameplayMutationResult::Applied)) => proto_response(
                &response,
                prefix,
                &self.state.config.config.transport.response_prefix_policy,
            ),
            Ok((_, GameplayMutationResult::Replay(bytes))) => encrypted_bytes(
                bytes,
                prefix,
                &self.state.config.config.transport.response_prefix_policy,
            ),
            Err(error) => state_error(error),
        }
    }

    pub(super) fn status(&self) -> Response<AppBody> {
        let versions = &self.state.config.config.versions;
        json_response(
            StatusCode::OK,
            json!({
                "device_auth": "enabled",
                "terms_of_service_version": versions.terms_of_service,
                "gdpr_privacy_policy_version": versions.privacy_policy,
                "title": { "id": 2, "timeline_asset_path_hash": "4783682884224829538", "bgm_path_hash": "7852695906499225647" },
            }),
        )
    }

    pub(super) fn sign_in(&self, body: &[u8], request_name: &str) -> Response<AppBody> {
        let (_, plaintext) = match decrypt_frame(body) {
            Ok(value) => value,
            Err(_) => return json_error(StatusCode::BAD_REQUEST, "invalid_format"),
        };
        let request = match self.state.proto.decode(request_name, &plaintext) {
            Ok(request) => request,
            Err(_) => return json_error(StatusCode::BAD_REQUEST, "invalid_format"),
        };
        let result = match self.state.sign_in(&request) {
            Ok(result) => result,
            Err(error) => return state_error(error),
        };
        let mut response = match self.state.proto.empty("blend.api.AuthSignInResponse") {
            Ok(response) => response,
            Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "server_error"),
        };
        response.set_field_by_name("session_token", Value::String(result.session_token));
        response.set_field_by_name("user_id", Value::I64(result.user_id));
        response.set_field_by_name("language", Value::I32(result.language));
        proto_response(
            &response,
            None,
            &self.state.config.config.transport.response_prefix_policy,
        )
    }

    pub(super) fn user_login(&self, headers: &RequestHeaders) -> Response<AppBody> {
        let token = match headers.session_token.as_deref() {
            Some(token) => token,
            None => return json_error(StatusCode::UNAUTHORIZED, "unauthorized"),
        };
        let session = match self.state.session(token, headers.user_id) {
            Ok(session) => session,
            Err(error) => return state_error(error),
        };
        let versions = &self.state.config.config.versions;
        if versions.enforce
            && (headers.asset_version.as_deref() != Some(versions.asset.as_str())
                || headers.master_data_version.as_deref() != Some(versions.master_data.as_str()))
        {
            return json_response(
                StatusCode::CONFLICT,
                json!({ "code": "requires_assets_updates", "asset_version": versions.asset, "master_data_version": versions.master_data }),
            );
        }
        match self.state.login_state(session.account_id) {
            Ok(response) => proto_response(
                &response,
                None,
                &self.state.config.config.transport.response_prefix_policy,
            ),
            Err(error) => state_error(error),
        }
    }

    pub(super) fn session_empty_proto(
        &self,
        message: &str,
        headers: &RequestHeaders,
    ) -> Response<AppBody> {
        let Some(token) = headers.session_token.as_deref() else {
            return json_error(StatusCode::UNAUTHORIZED, "unauthorized");
        };
        if let Err(error) = self.state.session(token, headers.user_id) {
            return state_error(error);
        }
        self.empty_proto(message, true)
    }

    pub(super) fn empty_proto(&self, message: &str, no_op: bool) -> Response<AppBody> {
        if no_op {
            tracing::info!(event = "no_op", route = %message, no_op = true, redacted = true);
        }
        if message == "blend.api.RefundInfoGetCountryCodeResponse" {
            return empty_encrypted_response();
        }
        match self.state.proto.empty(message) {
            Ok(mut response) => {
                if matches!(
                    message,
                    "blend.api.LoginBonusReceiveResponse"
                        | "blend.api.ExternalPurchaseReceiveResponse"
                ) {
                    let resources = match self.state.proto.empty("blend.model.Resources") {
                        Ok(resources) => resources,
                        Err(_) => {
                            return json_error(StatusCode::INTERNAL_SERVER_ERROR, "server_error")
                        }
                    };
                    response.set_field_by_name("changed_resources", Value::Message(resources));
                }
                proto_response(
                    &response,
                    None,
                    &self.state.config.config.transport.response_prefix_policy,
                )
            }
            Err(_) => json_error(StatusCode::INTERNAL_SERVER_ERROR, "server_error"),
        }
    }

    pub(super) fn quest_talk_event_finish(
        &self,
        body: &[u8],
        headers: &RequestHeaders,
        request_name: &str,
    ) -> Response<AppBody> {
        let token = match headers.session_token.as_deref() {
            Some(token) => token,
            None => return json_error(StatusCode::UNAUTHORIZED, "unauthorized"),
        };
        let session = match self.state.session(token, headers.user_id) {
            Ok(session) => session,
            Err(error) => return state_error(error),
        };
        let (request_prefix, plaintext) = match decrypt_frame(body) {
            Ok(value) => value,
            Err(_) => return json_error(StatusCode::BAD_REQUEST, "invalid_format"),
        };
        let request = match self.state.proto.decode(request_name, &plaintext) {
            Ok(request) => request,
            Err(_) => return json_error(StatusCode::BAD_REQUEST, "invalid_format"),
        };
        let request_id = headers
            .request_id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        match self
            .state
            .quest_talk_event_finish(&session, &request_id, &plaintext, &request)
        {
            Ok((response, GameplayMutationResult::Applied)) => proto_response(
                &response,
                Some(request_prefix),
                &self.state.config.config.transport.response_prefix_policy,
            ),
            Ok((_, GameplayMutationResult::Replay(bytes))) => encrypted_bytes(
                bytes,
                Some(request_prefix),
                &self.state.config.config.transport.response_prefix_policy,
            ),
            Err(error) => state_error(error),
        }
    }

    pub(super) fn battle_attack(
        &self,
        body: &[u8],
        headers: &RequestHeaders,
        request_name: &str,
    ) -> Response<AppBody> {
        let token = match headers.session_token.as_deref() {
            Some(token) => token,
            None => return json_error(StatusCode::UNAUTHORIZED, "unauthorized"),
        };
        let session = match self.state.session(token, headers.user_id) {
            Ok(session) => session,
            Err(error) => return state_error(error),
        };
        let (request_prefix, plaintext) = match decrypt_frame(body) {
            Ok(value) => value,
            Err(_) => return json_error(StatusCode::BAD_REQUEST, "invalid_format"),
        };
        let request = match self.state.proto.decode(request_name, &plaintext) {
            Ok(request) => request,
            Err(_) => return json_error(StatusCode::BAD_REQUEST, "invalid_format"),
        };
        let request_id = headers
            .request_id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        match self
            .state
            .battle_attack(&session, &request_id, &plaintext, &request)
        {
            Ok((response, GameplayMutationResult::Applied)) => proto_response(
                &response,
                Some(request_prefix),
                &self.state.config.config.transport.response_prefix_policy,
            ),
            Ok((_, GameplayMutationResult::Replay(bytes))) => encrypted_bytes(
                bytes,
                Some(request_prefix),
                &self.state.config.config.transport.response_prefix_policy,
            ),
            Err(error) => state_error(error),
        }
    }

    pub(super) fn battle_finish(&self, body: &[u8], headers: &RequestHeaders) -> Response<AppBody> {
        let token = match headers.session_token.as_deref() {
            Some(token) => token,
            None => return json_error(StatusCode::UNAUTHORIZED, "unauthorized"),
        };
        let session = match self.state.session(token, headers.user_id) {
            Ok(session) => session,
            Err(error) => return state_error(error),
        };
        let (request_prefix, plaintext) = if body.is_empty() {
            (None, Vec::new())
        } else {
            let (prefix, plaintext) = match decrypt_frame(body) {
                Ok(value) => value,
                Err(_) => return json_error(StatusCode::BAD_REQUEST, "invalid_format"),
            };
            (Some(prefix), plaintext)
        };
        if !plaintext.is_empty() {
            return json_error(StatusCode::BAD_REQUEST, "invalid_format");
        }
        let request_id = headers
            .request_id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        match self.state.battle_finish(&session, &request_id, &plaintext) {
            Ok((response, GameplayMutationResult::Applied)) => proto_response(
                &response,
                request_prefix,
                &self.state.config.config.transport.response_prefix_policy,
            ),
            Ok((_, GameplayMutationResult::Replay(bytes))) => encrypted_bytes(
                bytes,
                request_prefix,
                &self.state.config.config.transport.response_prefix_policy,
            ),
            Err(error) => state_error(error),
        }
    }

    pub(super) fn battle_resume(&self, body: &[u8], headers: &RequestHeaders) -> Response<AppBody> {
        let token = match headers.session_token.as_deref() {
            Some(token) => token,
            None => return json_error(StatusCode::UNAUTHORIZED, "unauthorized"),
        };
        let session = match self.state.session(token, headers.user_id) {
            Ok(session) => session,
            Err(error) => return state_error(error),
        };
        let (prefix, plaintext) = if body.is_empty() {
            (None, Vec::new())
        } else {
            match decrypt_frame(body) {
                Ok((prefix, plaintext)) => (Some(prefix), plaintext),
                Err(_) => return json_error(StatusCode::BAD_REQUEST, "invalid_format"),
            }
        };
        if !plaintext.is_empty() {
            return json_error(StatusCode::BAD_REQUEST, "invalid_format");
        }
        match self.state.battle_resume(&session) {
            Ok(response) => proto_response(
                &response,
                prefix,
                &self.state.config.config.transport.response_prefix_policy,
            ),
            Err(error) => state_error(error),
        }
    }

    pub(super) fn battle_retire(&self, body: &[u8], headers: &RequestHeaders) -> Response<AppBody> {
        let token = match headers.session_token.as_deref() {
            Some(token) => token,
            None => return json_error(StatusCode::UNAUTHORIZED, "unauthorized"),
        };
        let session = match self.state.session(token, headers.user_id) {
            Ok(session) => session,
            Err(error) => return state_error(error),
        };
        let (prefix, plaintext) = if body.is_empty() {
            (None, Vec::new())
        } else {
            match decrypt_frame(body) {
                Ok((prefix, plaintext)) => (Some(prefix), plaintext),
                Err(_) => return json_error(StatusCode::BAD_REQUEST, "invalid_format"),
            }
        };
        if !plaintext.is_empty() {
            return json_error(StatusCode::BAD_REQUEST, "invalid_format");
        }
        let request_id = headers
            .request_id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        match self.state.battle_retire(&session, &request_id, &plaintext) {
            Ok((response, GameplayMutationResult::Applied)) => proto_response(
                &response,
                prefix,
                &self.state.config.config.transport.response_prefix_policy,
            ),
            Ok((_, GameplayMutationResult::Replay(bytes))) => encrypted_bytes(
                bytes,
                prefix,
                &self.state.config.config.transport.response_prefix_policy,
            ),
            Err(error) => state_error(error),
        }
    }

    pub(super) fn quest_battle_start(
        &self,
        body: &[u8],
        headers: &RequestHeaders,
        request_name: &str,
        exploration: bool,
    ) -> Response<AppBody> {
        let token = match headers.session_token.as_deref() {
            Some(token) => token,
            None => return json_error(StatusCode::UNAUTHORIZED, "unauthorized"),
        };
        let session = match self.state.session(token, headers.user_id) {
            Ok(session) => session,
            Err(error) => return state_error(error),
        };
        let (request_prefix, plaintext) = match decrypt_frame(body) {
            Ok(value) => value,
            Err(_) => return json_error(StatusCode::BAD_REQUEST, "invalid_format"),
        };
        let request = match self.state.proto.decode(request_name, &plaintext) {
            Ok(request) => request,
            Err(_) => return json_error(StatusCode::BAD_REQUEST, "invalid_format"),
        };
        let request_id = headers
            .request_id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let result = if exploration {
            self.state
                .exploration_battle_start(&session, &request_id, &plaintext, &request)
        } else {
            self.state
                .quest_battle_start(&session, &request_id, &plaintext, &request)
        };
        match result {
            Ok((response, GameplayMutationResult::Applied)) => proto_response(
                &response,
                Some(request_prefix),
                &self.state.config.config.transport.response_prefix_policy,
            ),
            Ok((_, GameplayMutationResult::Replay(bytes))) => encrypted_bytes(
                bytes,
                Some(request_prefix),
                &self.state.config.config.transport.response_prefix_policy,
            ),
            Err(error) => state_error(error),
        }
    }

    pub(super) fn specialized_battle_start(
        &self,
        body: &[u8],
        headers: &RequestHeaders,
        mode: crate::state::BattleStartMode,
        request_name: &str,
    ) -> Response<AppBody> {
        let token = match headers.session_token.as_deref() {
            Some(token) => token,
            None => return json_error(StatusCode::UNAUTHORIZED, "unauthorized"),
        };
        let session = match self.state.session(token, headers.user_id) {
            Ok(session) => session,
            Err(error) => return state_error(error),
        };
        let (prefix, plaintext) = match decrypt_frame(body) {
            Ok(value) => value,
            Err(_) => return json_error(StatusCode::BAD_REQUEST, "invalid_format"),
        };
        let request = match self.state.proto.decode(request_name, &plaintext) {
            Ok(request) => request,
            Err(_) => return json_error(StatusCode::BAD_REQUEST, "invalid_format"),
        };
        let request_id = headers
            .request_id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        match self
            .state
            .specialized_battle_start(&session, &request_id, &plaintext, &request, mode)
        {
            Ok((response, GameplayMutationResult::Applied)) => proto_response(
                &response,
                Some(prefix),
                &self.state.config.config.transport.response_prefix_policy,
            ),
            Ok((_, GameplayMutationResult::Replay(bytes))) => encrypted_bytes(
                bytes,
                Some(prefix),
                &self.state.config.config.transport.response_prefix_policy,
            ),
            Err(error) => state_error(error),
        }
    }

    pub(super) fn synthesis_execute(
        &self,
        body: &[u8],
        headers: &RequestHeaders,
        route: &str,
        request_name: &str,
    ) -> Response<AppBody> {
        let token = match headers.session_token.as_deref() {
            Some(token) => token,
            None => return json_error(StatusCode::UNAUTHORIZED, "unauthorized"),
        };
        let session = match self.state.session(token, headers.user_id) {
            Ok(session) => session,
            Err(error) => return state_error(error),
        };
        let (prefix, plaintext) = match decrypt_frame(body) {
            Ok(value) => value,
            Err(_) => return json_error(StatusCode::BAD_REQUEST, "invalid_format"),
        };
        let request = match self.state.proto.decode(request_name, &plaintext) {
            Ok(request) => request,
            Err(_) => return json_error(StatusCode::BAD_REQUEST, "invalid_format"),
        };
        let request_id = headers
            .request_id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        match self
            .state
            .synthesis_execute(&session, &request_id, &plaintext, &request, route)
        {
            Ok((response, GameplayMutationResult::Applied)) => proto_response(
                &response,
                Some(prefix),
                &self.state.config.config.transport.response_prefix_policy,
            ),
            Ok((_, GameplayMutationResult::Replay(bytes))) => encrypted_bytes(
                bytes,
                Some(prefix),
                &self.state.config.config.transport.response_prefix_policy,
            ),
            Err(error) => state_error(error),
        }
    }

    pub(super) fn synthesis_combination_ranking(
        &self,
        body: &[u8],
        headers: &RequestHeaders,
        request_name: &str,
    ) -> Response<AppBody> {
        let token = match headers.session_token.as_deref() {
            Some(token) => token,
            None => return json_error(StatusCode::UNAUTHORIZED, "unauthorized"),
        };
        let session = match self.state.session(token, headers.user_id) {
            Ok(session) => session,
            Err(error) => return state_error(error),
        };
        let (prefix, plaintext) = match decrypt_frame(body) {
            Ok(value) => value,
            Err(_) => return json_error(StatusCode::BAD_REQUEST, "invalid_format"),
        };
        let request = match self.state.proto.decode(request_name, &plaintext) {
            Ok(request) => request,
            Err(_) => return json_error(StatusCode::BAD_REQUEST, "invalid_format"),
        };
        match self.state.synthesis_combination_ranking(&session, &request) {
            Ok(response) => proto_response(
                &response,
                Some(prefix),
                &self.state.config.config.transport.response_prefix_policy,
            ),
            Err(error) => state_error(error),
        }
    }

    pub(super) fn party_update(
        &self,
        body: &[u8],
        headers: &RequestHeaders,
        message_name: &str,
        tools_only: bool,
    ) -> Response<AppBody> {
        let token = match headers.session_token.as_deref() {
            Some(token) => token,
            None => return json_error(StatusCode::UNAUTHORIZED, "unauthorized"),
        };
        let session = match self.state.session(token, headers.user_id) {
            Ok(session) => session,
            Err(error) => return state_error(error),
        };
        let (prefix, plaintext) = match decrypt_frame(body) {
            Ok(value) => value,
            Err(_) => return json_error(StatusCode::BAD_REQUEST, "invalid_format"),
        };
        let request = match self.state.proto.decode(message_name, &plaintext) {
            Ok(request) => request,
            Err(_) => return json_error(StatusCode::BAD_REQUEST, "invalid_format"),
        };
        let request_id = headers
            .request_id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let result = if tools_only {
            self.state
                .party_battle_tools_set(&session, &request_id, &plaintext, &request)
        } else {
            self.state
                .party_bulk_update(&session, &request_id, &plaintext, &request)
        };
        match result {
            Ok((response, GameplayMutationResult::Applied)) => proto_response(
                &response,
                Some(prefix),
                &self.state.config.config.transport.response_prefix_policy,
            ),
            Ok((_, GameplayMutationResult::Replay(bytes))) => encrypted_bytes(
                bytes,
                Some(prefix),
                &self.state.config.config.transport.response_prefix_policy,
            ),
            Err(error) => state_error(error),
        }
    }

    pub(super) fn gacha_list(
        &self,
        body: &[u8],
        headers: &RequestHeaders,
        request_name: &str,
    ) -> Response<AppBody> {
        let token = match headers.session_token.as_deref() {
            Some(token) => token,
            None => return json_error(StatusCode::UNAUTHORIZED, "unauthorized"),
        };
        let session = match self.state.session(token, headers.user_id) {
            Ok(session) => session,
            Err(error) => return state_error(error),
        };
        let (prefix, plaintext) = match decrypt_frame(body) {
            Ok(value) => value,
            Err(_) => return json_error(StatusCode::BAD_REQUEST, "invalid_format"),
        };
        let request = match self.state.proto.decode(request_name, &plaintext) {
            Ok(request) => request,
            Err(_) => return json_error(StatusCode::BAD_REQUEST, "invalid_format"),
        };
        match self.state.gacha_list(&session, &request) {
            Ok(response) => proto_response(
                &response,
                Some(prefix),
                &self.state.config.config.transport.response_prefix_policy,
            ),
            Err(error) => state_error(error),
        }
    }

    pub(super) fn gacha_execute(
        &self,
        body: &[u8],
        headers: &RequestHeaders,
        request_name: &str,
    ) -> Response<AppBody> {
        let token = match headers.session_token.as_deref() {
            Some(token) => token,
            None => return json_error(StatusCode::UNAUTHORIZED, "unauthorized"),
        };
        let session = match self.state.session(token, headers.user_id) {
            Ok(session) => session,
            Err(error) => return state_error(error),
        };
        let (prefix, plaintext) = match decrypt_frame(body) {
            Ok(value) => value,
            Err(_) => return json_error(StatusCode::BAD_REQUEST, "invalid_format"),
        };
        let request = match self.state.proto.decode(request_name, &plaintext) {
            Ok(request) => request,
            Err(_) => return json_error(StatusCode::BAD_REQUEST, "invalid_format"),
        };
        let request_id = headers
            .request_id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        match self
            .state
            .gacha_execute(&session, &request_id, &plaintext, &request)
        {
            Ok((response, GameplayMutationResult::Applied)) => proto_response(
                &response,
                Some(prefix),
                &self.state.config.config.transport.response_prefix_policy,
            ),
            Ok((_, GameplayMutationResult::Replay(bytes))) => encrypted_bytes(
                bytes,
                Some(prefix),
                &self.state.config.config.transport.response_prefix_policy,
            ),
            Err(error) => state_error(error),
        }
    }

    pub(super) fn gacha_wish_list_set(
        &self,
        body: &[u8],
        headers: &RequestHeaders,
        request_name: &str,
    ) -> Response<AppBody> {
        let token = match headers.session_token.as_deref() {
            Some(token) => token,
            None => return json_error(StatusCode::UNAUTHORIZED, "unauthorized"),
        };
        let session = match self.state.session(token, headers.user_id) {
            Ok(session) => session,
            Err(error) => return state_error(error),
        };
        let (prefix, plaintext) = match decrypt_frame(body) {
            Ok(value) => value,
            Err(_) => return json_error(StatusCode::BAD_REQUEST, "invalid_format"),
        };
        let request = match self.state.proto.decode(request_name, &plaintext) {
            Ok(request) => request,
            Err(_) => return json_error(StatusCode::BAD_REQUEST, "invalid_format"),
        };
        match self.state.gacha_wish_list_set(&session, &request) {
            Ok(response) => proto_response(
                &response,
                Some(prefix),
                &self.state.config.config.transport.response_prefix_policy,
            ),
            Err(error) => state_error(error),
        }
    }

    pub async fn asset(&self, request: Request<Incoming>) -> Response<AppBody> {
        self.assets.serve(request).await
    }
}
