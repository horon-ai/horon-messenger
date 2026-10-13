use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use horon_messenger_sdk::Client;
use horon_messenger_server::build_router;
use serde_json::Value;
use tower::ServiceExt;

fn router() -> Router {
    let dir = std::env::temp_dir().join(format!("horon-server-http-test-{}", std::process::id()));
    let client = Client::builder()
        .config_dir(dir)
        .build()
        .expect("build client");
    build_router(Arc::new(client))
}

fn get(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

async fn call(router: &Router, request: Request<Body>) -> (StatusCode, Value, String) {
    let response = router.clone().oneshot(request).await.expect("send request");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read body");
    let raw = String::from_utf8_lossy(&bytes).into_owned();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json, raw)
}

#[tokio::test]
async fn health_returns_ok() {
    let (status, body, raw) = call(&router(), get("/health")).await;
    assert_eq!(status, StatusCode::OK, "body: {raw}");
    assert_eq!(body["ok"], Value::Bool(true));
    assert_eq!(body["data"]["status"], "ok");
}

#[tokio::test]
async fn list_providers_includes_mock() {
    let (status, body, raw) = call(&router(), get("/v1/providers")).await;
    assert_eq!(status, StatusCode::OK, "body: {raw}");
    assert_eq!(body["ok"], Value::Bool(true));
    let providers = body["data"].as_array().expect("providers array");
    assert!(
        providers.iter().any(|provider| provider["kind"] == "mock"),
        "mock provider missing: {raw}"
    );
}

#[tokio::test]
async fn list_chats_returns_three() {
    let (status, body, raw) = call(&router(), get("/v1/providers/mock/chats")).await;
    assert_eq!(status, StatusCode::OK, "body: {raw}");
    assert_eq!(body["ok"], Value::Bool(true));
    let chats = body["data"].as_array().expect("chats array");
    assert_eq!(chats.len(), 3, "chats: {raw}");
}

#[tokio::test]
async fn send_message_is_accepted() {
    let request = Request::builder()
        .method("POST")
        .uri("/v1/providers/mock/chats/general/messages")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"text":"hello from the test"}"#))
        .unwrap();
    let (status, body, raw) = call(&router(), request).await;
    assert_eq!(status, StatusCode::OK, "body: {raw}");
    assert_eq!(body["ok"], Value::Bool(true));
    assert_eq!(body["data"]["accepted"], Value::Bool(true));
}

#[tokio::test]
async fn unknown_provider_is_rejected_before_handling() {
    let (status, body, raw) = call(&router(), get("/v1/providers/myspace/whoami")).await;
    println!("myspace whoami -> status={status} raw={raw}");
    assert_eq!(status, StatusCode::BAD_REQUEST, "raw: {raw}");
    assert_eq!(
        raw,
        "Invalid URL: unknown variant `myspace`, expected one of `kakaotalk`, `discord`, `telegram`, `channeltalk`, `mock`, `slack`, `slackbot`, `telegrambot`, `whatsapp`, `whatsappbot`, `line`, `imessage`, `instagram`"
    );
    assert!(
        body.get("ok").is_none(),
        "expected a path-extraction rejection, not an Envelope: {raw}"
    );
}
