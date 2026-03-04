use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use ayiou_admin_proto::AdminCommand;
use serde_json::json;
use tower::ServiceExt;

use ayiou_control_plane::app::test_app_with_connected_agent;

#[tokio::test]
async fn start_bot_endpoint_pushes_command_to_connected_agent() {
    let (app, fake_agent) = test_app_with_connected_agent("bot-a");
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/bots/bot-a/start")
                .header("Authorization", "Bearer admin-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(
        fake_agent.last_command().await.command,
        AdminCommand::StartBot
    );
}

#[tokio::test]
async fn load_wasm_endpoint_pushes_command_to_connected_agent() {
    let (app, fake_agent) = test_app_with_connected_agent("bot-a");
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/bots/bot-a/plugins/echo/wasm/load")
                .header("Authorization", "Bearer admin-token")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "module_path": "/tmp/echo.wasm"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(
        fake_agent.last_command().await.command,
        AdminCommand::LoadWasmPlugin {
            plugin_name: "echo".to_string(),
            module_path: "/tmp/echo.wasm".to_string(),
        }
    );
}

#[tokio::test]
async fn unload_wasm_endpoint_pushes_command_to_connected_agent() {
    let (app, fake_agent) = test_app_with_connected_agent("bot-a");
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/bots/bot-a/plugins/echo/wasm/unload")
                .header("Authorization", "Bearer admin-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(
        fake_agent.last_command().await.command,
        AdminCommand::UnloadWasmPlugin {
            plugin_name: "echo".to_string(),
        }
    );
}
