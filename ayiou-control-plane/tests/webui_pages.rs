use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use tower::ServiceExt;

use ayiou_control_plane::app::test_app_with_bot;

#[tokio::test]
async fn bot_detail_page_contains_runtime_actions() {
    let app = test_app_with_bot("bot-a");
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/ui/bots/bot-a")
                .header("Authorization", "Bearer admin-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8_lossy(&body);
    assert!(body.contains("Start Bot"));
    assert!(body.contains("Stop Bot"));
    assert!(body.contains("Enable Plugin"));
    assert!(body.contains("Save Config"));
    assert!(body.contains("Load Wasm Plugin"));
    assert!(body.contains("Unload Wasm Plugin"));
}
