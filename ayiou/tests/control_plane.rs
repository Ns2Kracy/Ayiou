use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use ayiou::{
    Context, control_plane,
    core::{
        control::RuntimeControlHandle,
        plugin::{
            HandleOutcome, PluginRuntimeState, RuntimePlugin, RuntimePluginEngine,
            RuntimePluginServices,
        },
    },
};
use serde_json::Value;
use tokio::sync::RwLock;
use tower::ServiceExt;

struct TestPlugin;

#[async_trait]
impl RuntimePlugin for TestPlugin {
    fn kind(&self) -> &'static str {
        "test-plugin"
    }
    async fn handle(&self, _ctx: &Context) -> Result<HandleOutcome> {
        Ok(HandleOutcome::pass())
    }
}

fn app() -> Router {
    let services = RuntimePluginServices::new();
    let state = PluginRuntimeState::default();
    let mut engine = RuntimePluginEngine::new(services, state);
    engine.push_as("test-plugin", Box::new(TestPlugin));
    let handle = RuntimeControlHandle::new(Arc::new(RwLock::new(engine)));
    control_plane::router(handle, "secret")
}

async fn json_response(request: Request<Body>) -> (StatusCode, Value) {
    let response = app().oneshot(request).await.unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let value = serde_json::from_slice(&body).unwrap();
    (status, value)
}

#[cfg(feature = "embedded-webui")]
async fn raw_response(request: Request<Body>) -> (StatusCode, String) {
    let response = app().oneshot(request).await.unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let value = String::from_utf8(body.to_vec()).unwrap();
    (status, value)
}

#[tokio::test]
async fn control_plane_rejects_missing_bearer_token() {
    let (status, body) = json_response(
        Request::builder()
            .uri("/api/plugins")
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"]["code"], "unauthorized");
}

#[tokio::test]
async fn control_plane_rejects_wrong_bearer_token() {
    let (status, body) = json_response(
        Request::builder()
            .uri("/api/plugins")
            .header(header::AUTHORIZATION, "Bearer wrong")
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "unauthorized");
}

#[tokio::test]
async fn control_plane_lists_plugins_with_bearer_token() {
    let (status, body) = json_response(
        Request::builder()
            .uri("/api/plugins")
            .header(header::AUTHORIZATION, "Bearer secret")
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["ok"], true);
    assert_eq!(body["data"][0]["instance_id"], "test-plugin");
    assert_eq!(body["data"][0]["reloadable"], false);
}

#[tokio::test]
async fn control_plane_reports_non_reloadable_plugin() {
    let (status, body) = json_response(
        Request::builder()
            .method("POST")
            .uri("/api/plugins/test-plugin/reload")
            .header(header::AUTHORIZATION, "Bearer secret")
            .body(Body::empty())
            .unwrap(),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"]["code"], "not_reloadable");
}

#[cfg(feature = "embedded-webui")]
#[tokio::test]
async fn embedded_control_plane_serves_webui_index() {
    let (status, body) =
        raw_response(Request::builder().uri("/").body(Body::empty()).unwrap()).await;

    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("<html"));
    assert!(body.contains("/assets/"));
}
