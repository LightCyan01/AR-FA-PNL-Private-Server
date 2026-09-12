use std::{sync::Arc, time::Instant};

const MAX_API_BODY_BYTES: usize = 16 * 1024 * 1024;
const MAX_REQUEST_ID_BYTES: usize = 128;

#[derive(Debug)]
enum StorageDispatchError {
    Busy,
    Join(tokio::task::JoinError),
}

async fn run_bounded_storage<T, F>(
    gate: Arc<tokio::sync::Semaphore>,
    operation: F,
) -> Result<(T, u64, u64), StorageDispatchError>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let admission_started = Instant::now();
    let permit = gate
        .try_acquire_owned()
        .map_err(|_| StorageDispatchError::Busy)?;
    let queue_ms = admission_started.elapsed().as_millis() as u64;
    let (result, operation_ms) = tokio::task::spawn_blocking(move || {
        let started = Instant::now();
        let result = operation();
        let operation_ms = started.elapsed().as_millis() as u64;
        drop(permit);
        (result, operation_ms)
    })
    .await
    .map_err(StorageDispatchError::Join)?;
    Ok((result, queue_ms, operation_ms))
}

use crate::{assets::AssetService, state::State};

#[derive(Clone)]
pub struct ApiService {
    state: Arc<State>,
    assets: AssetService,
    storage_gate: Arc<tokio::sync::Semaphore>,
}

mod request;
mod response;
mod router;
#[cfg(test)]
pub(crate) use request::route_spec;
#[cfg(test)]
mod tests;
