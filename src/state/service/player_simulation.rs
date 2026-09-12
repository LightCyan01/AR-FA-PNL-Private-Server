// Disposable player journeys through the production encrypted HTTP dispatcher.
// Storage reads are postcondition assertions, never inputs to player decisions.
use super::prelude::*;
use crate::config::Config;
use crate::{
    api::ApiService,
    assets::AssetService,
    transport::{decrypt_frame, encrypt_frame, full},
};
use http_body_util::BodyExt;
use hyper::Request;
use prost_reflect::ReflectMessage;
use std::path::{Path, PathBuf};
use std::sync::Arc;

struct Player {
    state: Arc<State>,
    api: ApiService,
    runtime: tokio::runtime::Runtime,
    token: String,
    client_resources: DynamicMessage,
    history: Option<DynamicMessage>,
    session: Session,
    path: PathBuf,
    actions: Vec<String>,
}

mod player_methods;
mod tests_a;
mod tests_b;
