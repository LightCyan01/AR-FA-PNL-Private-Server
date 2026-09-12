use super::request::{
    classify_request, json_error, route_spec, state_error, storage_dispatch_error,
    RequestDisposition, RequestHeaders,
};
use super::*;
use crate::state::StateError;
use http_body_util::BodyExt;
use hyper::{header, Method, Request, StatusCode};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn async_storage_keeps_runtime_schedulable_and_serializes_operations() {
    let gate = Arc::new(tokio::sync::Semaphore::new(1));
    let order = Arc::new(AtomicUsize::new(0));
    let first_order = Arc::clone(&order);
    let (started, started_rx) = tokio::sync::oneshot::channel();
    let first = tokio::spawn(run_bounded_storage(Arc::clone(&gate), move || {
        let _ = started.send(());
        std::thread::sleep(Duration::from_millis(30));
        first_order.fetch_add(1, Ordering::SeqCst)
    }));
    started_rx.await.unwrap();
    let second_order = Arc::clone(&order);
    assert!(matches!(
        run_bounded_storage(Arc::clone(&gate), move || {
            second_order.fetch_add(1, Ordering::SeqCst)
        })
        .await,
        Err(StorageDispatchError::Busy)
    ));
    assert_eq!(
        storage_dispatch_error(StorageDispatchError::Busy).status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    let tick = tokio::spawn(async {
        tokio::task::yield_now().await;
        7usize
    });
    assert_eq!(tick.await.unwrap(), 7);
    assert_eq!(first.await.unwrap().unwrap().0, 0);
    assert_eq!(
        run_bounded_storage(Arc::clone(&gate), move || {
            order.fetch_add(1, Ordering::SeqCst)
        })
        .await
        .unwrap()
        .0,
        1
    );
    let mut admission = Vec::new();
    let mut operation = Vec::new();
    for _ in 0..16 {
        let (_, queue_ms, operation_ms) = run_bounded_storage(Arc::clone(&gate), || 0usize)
            .await
            .unwrap();
        admission.push(queue_ms);
        operation.push(operation_ms);
    }
    admission.sort_unstable();
    operation.sort_unstable();
    println!(
        "async_storage admission_ms p50={} p95={} operation_ms p50={} p95={}",
        admission[admission.len() / 2],
        admission[admission.len() * 95 / 100],
        operation[operation.len() / 2],
        operation[operation.len() * 95 / 100],
    );
}

#[test]
fn api_boundary_method_and_route_dispositions_are_stable() {
    assert_eq!(
        classify_request(&Method::GET, "/master_data/v1"),
        RequestDisposition::MasterData
    );
    assert_eq!(
        classify_request(&Method::POST, "/master_data/v1"),
        RequestDisposition::Post
    );
    assert_eq!(
        classify_request(&Method::GET, "/status"),
        RequestDisposition::MethodNotAllowed
    );
    assert_eq!(
        route_spec("/web_session/token").unwrap().request,
        Some("google.protobuf.Empty")
    );
    assert_eq!(
        route_spec("/synthesis/execute_easy").unwrap().response,
        Some("blend.api.SynthesisExecuteEasyResponse")
    );
    assert_eq!(
        route_spec("/gacha/step_up_execute").unwrap().request,
        Some("blend.api.GachaStepUpExecuteRequest")
    );
    assert_eq!(
        route_spec("/multi_mission/status").unwrap().response,
        Some("blend.api.MultiMissionStatusResponse")
    );
    assert_eq!(
        route_spec("/multi_mission/receive").unwrap().request,
        Some("blend.api.MultiMissionReceiveRequest")
    );
    assert!(route_spec("/unsupported").is_none());
}

#[test]
fn api_boundary_request_id_validation_is_stable() {
    let empty = Request::builder()
        .header("x-request-id", "")
        .body(())
        .unwrap();
    assert!(RequestHeaders::from_request(&empty).invalid_request_id);

    let too_long = "x".repeat(MAX_REQUEST_ID_BYTES + 1);
    let request = Request::builder()
        .header("x-request-id", too_long)
        .body(())
        .unwrap();
    assert!(RequestHeaders::from_request(&request).invalid_request_id);

    let valid = Request::builder()
        .header("x-request-id", "request-1")
        .header("x-user-id", "42")
        .body(())
        .unwrap();
    let headers = RequestHeaders::from_request(&valid);
    assert_eq!(headers.request_id.as_deref(), Some("request-1"));
    assert_eq!(headers.user_id, Some(42));
}

#[tokio::test]
async fn api_boundary_error_status_body_and_redaction_are_stable() {
    let response = json_error(StatusCode::NOT_FOUND, "unsupported_route");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "application/json; charset=utf-8"
    );
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(bytes.as_ref(), br#"{"code":"unsupported_route"}"#);
    assert!(!String::from_utf8_lossy(&bytes).contains("token"));

    let unauthorized = state_error(StateError::InvalidSession);
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
}

#[test]
fn api_boundary_encrypted_frame_errors_are_stable() {
    assert!(crate::transport::decrypt_frame(&[0; 17]).is_err());
    let frame = crate::transport::encrypt_frame(46, b"request").unwrap();
    assert_eq!(
        crate::transport::decrypt_frame(&frame).unwrap().1,
        b"request"
    );
}

#[test]
fn password_argv_uses_protected_input_channel() {
    let source =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../launcher.py")).unwrap();
    assert!(source.contains("\"--password-stdin\""));
    assert!(source.contains("input=password"));
    assert!(!source.contains("\"--password\",\n            password"));
}
