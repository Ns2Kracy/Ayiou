use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use serde_json::json;
use tower::ServiceExt;

use ayiou_control_plane::app::test_app;

#[tokio::test]
async fn metrics_query_returns_latest_plugin_counters() {
    let app = test_app();

    let ingest = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/internal/v1/ingest/metrics")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "bot_id":"bot-a",
                        "name":"plugin_errors_total",
                        "value":2,
                        "labels":{"plugin":"echo"}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ingest.status(), StatusCode::ACCEPTED);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/bots/bot-a/metrics")
                .header("Authorization", "Bearer viewer-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(String::from_utf8_lossy(&body).contains("plugin_errors_total"));
}
