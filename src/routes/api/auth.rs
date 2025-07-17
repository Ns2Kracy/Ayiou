use crate::Ctx;
use crate::error::AyiouError;
use crate::middleware::auth::AuthUser;
use crate::models::user::{AuthResponse, LoginPayload, RegisterPayload, UserResponse};
use crate::services::auth::AuthService;
use axum::{
    RequestPartsExt, Router,
    extract::{FromRequestParts, State},
    http::{StatusCode, request::Parts},
    response::Json,
    routing::{get, post},
};
use axum_extra::{
    TypedHeader,
    headers::{Authorization, authorization::Bearer},
};
use std::sync::Arc;

pub fn mount() -> Router<Ctx> {
    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/me", get(get_current_user))
        .route("/logout", post(logout))
}

// 用户注册
async fn register(
    State(ctx): State<Ctx>,
    Json(payload): Json<RegisterPayload>,
) -> Result<Json<AuthResponse>, AyiouError> {
    let response = ctx.auth_service.register(payload).await?;
    Ok(Json(response))
}

// 用户登录
async fn login(
    State(ctx): State<Ctx>,
    Json(payload): Json<LoginPayload>,
) -> Result<Json<AuthResponse>, AyiouError> {
    let response = ctx.auth_service.login(payload).await?;
    Ok(Json(response))
}

// 获取当前用户信息
async fn get_current_user(
    State(ctx): State<Ctx>,
    auth_user: AuthUser,
) -> Result<Json<UserResponse>, AyiouError> {
    let user = ctx.auth_service.get_user_by_id(auth_user.user_id).await?;
    Ok(Json(user))
}

// 用户注销 (客户端处理，服务端只返回成功)
async fn logout() -> Result<(StatusCode, Json<serde_json::Value>), AyiouError> {
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "message": "注销成功"
        })),
    ))
}
