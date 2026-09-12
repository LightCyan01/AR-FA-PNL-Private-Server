// HTTP lifecycle and blocking dispatch.

use std::{sync::Arc, time::Instant};

use http_body_util::{BodyExt, Limited};
use hyper::http::{header, Request, Response, StatusCode};

use super::{
    request::{
        empty_encrypted_response, json_error, route_spec, state_error, storage_dispatch_error,
        RequestDisposition, RequestHeaders, RouteHandler,
    },
    run_bounded_storage, ApiService, MAX_API_BODY_BYTES, MAX_REQUEST_ID_BYTES,
};
use crate::{
    assets::AssetService,
    state::{State, StateError},
    transport::{full, AppBody},
};

impl ApiService {
    pub fn new(state: Arc<State>, assets: AssetService) -> Self {
        Self {
            state,
            assets,
            storage_gate: Arc::new(tokio::sync::Semaphore::new(1)),
        }
    }

    pub async fn handle<B: http_body::Body<Data = bytes::Bytes>>(
        &self,
        request: Request<B>,
    ) -> Response<AppBody>
    where
        B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        let started = Instant::now();
        let method = request.method().clone();
        let route = request.uri().path().to_owned();
        let session_present = request.headers().contains_key("x-session-token");
        let request_id_len = request
            .headers()
            .get("x-request-id")
            .filter(|value| value.as_bytes().len() <= MAX_REQUEST_ID_BYTES)
            .map(|value| value.as_bytes().len())
            .unwrap_or(0);
        let result = self.handle_inner(request).await;
        let response_bytes = result
            .headers()
            .get(header::CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);
        tracing::info!(event = "request", method = %method, route = %route, status = result.status().as_u16(), elapsed_ms = started.elapsed().as_millis() as u64, session_present, request_id_present = request_id_len != 0, request_id_len, response_bytes, redacted = true);
        result
    }

    async fn handle_inner<B: http_body::Body<Data = bytes::Bytes>>(
        &self,
        request: Request<B>,
    ) -> Response<AppBody>
    where
        B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        let route = request.uri().path().to_owned();
        let headers = RequestHeaders::from_request(&request);
        if headers.invalid_request_id {
            return json_error(StatusCode::BAD_REQUEST, "invalid_format");
        }
        match super::request::classify_request(request.method(), &route) {
            RequestDisposition::MasterData => return self.master_data(&route).await,
            RequestDisposition::MethodNotAllowed => {
                return json_error(StatusCode::METHOD_NOT_ALLOWED, "method_not_allowed")
            }
            RequestDisposition::Post => {}
        }
        let body = match Limited::new(request.into_body(), MAX_API_BODY_BYTES)
            .collect()
            .await
        {
            Ok(collected) => collected.to_bytes(),
            Err(_) => return json_error(StatusCode::PAYLOAD_TOO_LARGE, "invalid_format"),
        };
        self.dispatch_blocking(body.to_vec(), headers, route).await
    }

    async fn dispatch_blocking(
        &self,
        body: Vec<u8>,
        headers: RequestHeaders,
        route: String,
    ) -> Response<AppBody> {
        let service = self.clone();
        #[cfg(test)]
        let simulation_now = crate::state::simulation_now();
        match run_bounded_storage(self.storage_gate.clone(), move || {
            #[cfg(test)]
            crate::state::set_simulation_now(simulation_now);
            let response = service.dispatch(body, headers, route);
            if service.state.store.maintenance_due() {
                if let Err(error) = service.state.store.prune_expired(crate::state::unix_now()) {
                    tracing::warn!(event = "storage_maintenance_failed", error = %error, redacted = true);
                }
            }
            response
        })
        .await
        {
            Ok((response, queue_ms, operation_ms)) => {
                tracing::info!(event = "storage_dispatch", queue_ms, operation_ms, redacted = true);
                response
            }
            Err(error) => storage_dispatch_error(error),
        }
    }

    fn dispatch(&self, body: Vec<u8>, headers: RequestHeaders, route: String) -> Response<AppBody> {
        let Some(spec) = route_spec(&route) else {
            tracing::warn!(event = "unsupported_route", route = %route, method = "POST", status = 404, unsupported = true, redacted = true);
            return json_error(StatusCode::NOT_FOUND, "unsupported_route");
        };
        match spec.handler {
            RouteHandler::Home => self.home_request(
                &body,
                &headers,
                &route,
                spec.request.expect("protobuf request"),
                spec.response.expect("protobuf response"),
            ),
            RouteHandler::Status => {
                if !body.is_empty() {
                    return json_error(StatusCode::BAD_REQUEST, "invalid_format");
                }
                self.status()
            }
            RouteHandler::RefundCountryCode => {
                if !body.is_empty() {
                    return json_error(StatusCode::BAD_REQUEST, "invalid_format");
                }
                self.empty_proto(spec.response.expect("protobuf response"), false)
            }
            RouteHandler::SignIn => self.sign_in(&body, spec.request.expect("protobuf request")),
            RouteHandler::UserDelete => {
                if !body.is_empty() {
                    return json_error(StatusCode::BAD_REQUEST, "invalid_format");
                }
                let Some(token) = headers.session_token.as_deref() else {
                    return json_error(StatusCode::UNAUTHORIZED, "unauthorized");
                };
                let session = match self.state.session(token, headers.user_id) {
                    Ok(session) => session,
                    Err(error) => return state_error(error),
                };
                match self.state.store.delete_account(session.account_id) {
                    Ok(()) => empty_encrypted_response(),
                    Err(error) => state_error(StateError::Storage(error)),
                }
            }
            RouteHandler::UserLogin => {
                if !body.is_empty() {
                    return json_error(StatusCode::BAD_REQUEST, "invalid_format");
                }
                self.user_login(&headers)
            }
            RouteHandler::ExternalPurchaseReceive => {
                if !body.is_empty() {
                    return json_error(StatusCode::BAD_REQUEST, "invalid_format");
                }
                self.session_empty_proto(spec.response.expect("protobuf response"), &headers)
            }
            RouteHandler::QuestTalkEventFinish => self.quest_talk_event_finish(
                &body,
                &headers,
                spec.request.expect("protobuf request"),
            ),
            RouteHandler::QuestBattleStart { exploration } => self.quest_battle_start(
                &body,
                &headers,
                spec.request.expect("protobuf request"),
                exploration,
            ),
            RouteHandler::SpecializedBattleStart(mode) => self.specialized_battle_start(
                &body,
                &headers,
                mode,
                spec.request.expect("protobuf request"),
            ),
            RouteHandler::BattleAttack => {
                self.battle_attack(&body, &headers, spec.request.expect("protobuf request"))
            }
            RouteHandler::BattleFinish => self.battle_finish(&body, &headers),
            RouteHandler::BattleResume => self.battle_resume(&body, &headers),
            RouteHandler::BattleRetire => self.battle_retire(&body, &headers),
            RouteHandler::SynthesisExecute => self.synthesis_execute(
                &body,
                &headers,
                &route,
                spec.request.expect("protobuf request"),
            ),
            RouteHandler::SynthesisCombinationRanking => self.synthesis_combination_ranking(
                &body,
                &headers,
                spec.request.expect("protobuf request"),
            ),
            RouteHandler::PartyUpdate { tools_only } => self.party_update(
                &body,
                &headers,
                spec.request.expect("protobuf request"),
                tools_only,
            ),
            RouteHandler::GachaList => {
                self.gacha_list(&body, &headers, spec.request.expect("protobuf request"))
            }
            RouteHandler::GachaExecute => {
                self.gacha_execute(&body, &headers, spec.request.expect("protobuf request"))
            }
            RouteHandler::GachaWishListSet => {
                self.gacha_wish_list_set(&body, &headers, spec.request.expect("protobuf request"))
            }
        }
    }

    async fn master_data(&self, route: &str) -> Response<AppBody> {
        let expected = format!(
            "/master_data/{}",
            self.state.config.config.versions.master_data
        );
        if route != expected {
            return json_error(StatusCode::NOT_FOUND, "unsupported_route");
        }
        match tokio::fs::read(&self.state.config.config.paths.master_data_download).await {
            Ok(bytes) => {
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
            Err(_) => json_error(StatusCode::NOT_FOUND, "asset_not_found"),
        }
    }
}
