use crate::Ctx;
use crate::error::AyiouError;
use crate::middleware::auth::AuthUser;
use crate::models::user::{UserPageResponse, UserResponse};
use crate::services::user::{ChangePasswordPayload, UpdateUserPayload, UserService, UserStats};
use axum::{
    Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{delete, get, patch, post},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct SearchUsersQuery {
    pub q: String,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    20
}

#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub data: T,
}

#[derive(Debug, Serialize)]
pub struct MessageResponse {
    pub message: String,
}

pub fn mount() -> Router<Ctx> {
    Router::new()
        .route("/me", patch(update_current_user))
        .route("/me/password", post(change_password))
        .route("/me/stats", get(get_current_user_stats))
        .route("/{username}", get(get_user_by_username))
        .route("/{username}/page", get(get_user_page))
}

// Update current user info
async fn update_current_user(
    State(ctx): State<Ctx>,
    auth_user: AuthUser,
    Json(payload): Json<UpdateUserPayload>,
) -> Result<Json<ApiResponse<UserResponse>>, AyiouError> {
    let user = ctx
        .user_service
        .update_user(auth_user.user_id, payload)
        .await?;
    Ok(Json(ApiResponse { data: user }))
}

// Change current user password
async fn change_password(
    State(ctx): State<Ctx>,
    auth_user: AuthUser,
    Json(payload): Json<ChangePasswordPayload>,
) -> Result<Json<MessageResponse>, AyiouError> {
    ctx.user_service
        .change_password(auth_user.user_id, payload)
        .await?;
    Ok(Json(MessageResponse {
        message: "Password changed successfully".to_string(),
    }))
}

// Get current user stats
async fn get_current_user_stats(
    State(ctx): State<Ctx>,
    auth_user: AuthUser,
) -> Result<Json<ApiResponse<UserStats>>, AyiouError> {
    let stats = ctx.user_service.get_user_stats(auth_user.user_id).await?;
    Ok(Json(ApiResponse { data: stats }))
}

// Get user info by username
async fn get_user_by_username(
    State(ctx): State<Ctx>,
    Path(username): Path<String>,
) -> Result<Json<ApiResponse<UserResponse>>, AyiouError> {
    let user = ctx
        .user_service
        .get_user_by_username(&username)
        .await?
        .ok_or_else(|| AyiouError::bad_request("User not found"))?;

    Ok(Json(ApiResponse { data: user }))
}

// Get user page (includes user info and links)
async fn get_user_page(
    State(ctx): State<Ctx>,
    Path(username): Path<String>,
) -> Result<Json<ApiResponse<UserPageResponse>>, AyiouError> {
    let page = ctx.user_service.get_user_page(&username).await?;
    Ok(Json(ApiResponse { data: page }))
}
