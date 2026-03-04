use axum::{
    body::{Body, to_bytes},
    http::header::{CONTENT_TYPE, LOCATION, SET_COOKIE},
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

#[tokio::test]
async fn login_sets_http_only_cookie_for_known_token() {
    let app = build_router(AppState::single_user(
        "admin",
        "admin-token",
        &["bot:start"],
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/ui/login")
                .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("token=admin-token"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get(LOCATION).unwrap(), "/ui/bots");
    let cookie = response
        .headers()
        .get(SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(cookie.contains("ayiou_token=admin-token"));
    assert!(cookie.contains("HttpOnly"));
}

#[tokio::test]
async fn login_rejects_unknown_token() {
    let app = build_router(AppState::single_user(
        "admin",
        "admin-token",
        &["bot:start"],
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/ui/login")
                .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("token=invalid"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8_lossy(&body);
    assert!(body.contains("Invalid token."));
}

#[tokio::test]
async fn cookie_authenticated_user_can_open_bots_page() {
    let app = build_router(AppState::single_user(
        "admin",
        "admin-token",
        &["bot:start"],
    ));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/ui/bots")
                .header("Cookie", "ayiou_token=admin-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
