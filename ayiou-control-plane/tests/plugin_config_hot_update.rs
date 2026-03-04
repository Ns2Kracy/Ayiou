use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use ayiou_admin_proto::AdminCommand;
use serde_json::json;
use tower::ServiceExt;

use ayiou_control_plane::app::test_app_with_store_and_agent;

#[tokio::test]
async fn update_plugin_config_persists_and_dispatches_command() {
    let (app, fake_agent, store) = test_app_with_store_and_agent();

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/bots/bot-a/plugins/echo/config")
                .header("Authorization", "Bearer admin-token")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "backend": "toml",
                        "content": "threshold=3",
                        "expected_version": null
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert!(store.get("bot-a", "echo").await.unwrap().is_some());
    assert!(matches!(
        fake_agent.last_command().await.command,
        AdminCommand::UpdatePluginConfig { .. }
    ));
}
