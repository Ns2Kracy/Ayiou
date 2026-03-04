use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

use ayiou_control_plane::app::{AppState, build_router};

#[tokio::test]
async fn user_without_plugin_disable_permission_gets_403() {
    let app = build_router(AppState::single_user(
        "viewer",
        "viewer-token",
        &["logs:read"],
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/bots/bot-a/plugins/echo/disable")
                .header("Authorization", "Bearer viewer-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
