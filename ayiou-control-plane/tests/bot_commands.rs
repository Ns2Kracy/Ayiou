use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use ayiou_admin_proto::AdminCommand;
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
    assert_eq!(fake_agent.last_command().await.command, AdminCommand::StartBot);
}
