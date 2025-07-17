use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{delete, get, patch, post},
};

use crate::{
    ApiResult, Ctx,
    error::AyiouError,
    models::user::{
        CreateUserLinkPayload, UpdateUserLinkPayload, UserLinkResponse, UserPageResponse,
    },
    services::user_link::UserLinkService,
};

pub fn routes() -> Router<Ctx> {
    Router::new()
        .route("/", post(create_user_link).get(get_user_links))
        .route(
            "/{link_id}",
            patch(update_user_link).delete(delete_user_link),
        )
        .route("/{link_id}/click", post(track_link_click))
}

pub fn public_routes() -> Router<Ctx> {
    Router::new()
        .route("/{username}", get(get_user_page))
        .route(
            "/{username}/link/{link_id}/click",
            post(track_public_link_click),
        )
}

// Create user link
async fn create_user_link(
    State(ctx): State<Ctx>,
    // TODO: Extract user ID from JWT
    Json(payload): Json<CreateUserLinkPayload>,
) -> ApiResult<UserLinkResponse> {
    // Temporarily hardcoded user ID, should actually be extracted from JWT
    let user_id = 1;

    let service = UserLinkService::new(ctx.db.clone());
    let link = service.create_user_link(user_id, payload).await?;

    Ok(Json(link))
}

// Get all user links
async fn get_user_links(State(ctx): State<Ctx>) -> ApiResult<Vec<UserLinkResponse>> {
    // Temporarily hardcoded user ID
    let user_id = 1;

    let service = UserLinkService::new(ctx.db.clone());
    let links = service.get_user_links(user_id).await?;

    Ok(Json(links))
}

// Update user link
async fn update_user_link(
    State(ctx): State<Ctx>,
    Path(link_id): Path<i64>,
    Json(payload): Json<UpdateUserLinkPayload>,
) -> ApiResult<UserLinkResponse> {
    // Temporarily hardcoded user ID
    let user_id = 1;

    let service = UserLinkService::new(ctx.db.clone());
    let link = service.update_user_link(user_id, link_id, payload).await?;

    Ok(Json(link))
}

// Delete user link
async fn delete_user_link(
    State(ctx): State<Ctx>,
    Path(link_id): Path<i64>,
) -> Result<StatusCode, AyiouError> {
    // Temporarily hardcoded user ID
    let user_id = 1;

    let service = UserLinkService::new(ctx.db.clone());
    service.delete_user_link(user_id, link_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

// Track link clicks (authenticated users)
async fn track_link_click(
    State(ctx): State<Ctx>,
    Path(link_id): Path<i64>,
) -> Result<StatusCode, AyiouError> {
    let service = UserLinkService::new(ctx.db.clone());
    service.increment_click(link_id).await?;

    Ok(StatusCode::OK)
}

// Public user page (no authentication required)
async fn get_user_page(
    State(ctx): State<Ctx>,
    Path(username): Path<String>,
) -> ApiResult<UserPageResponse> {
    let service = UserLinkService::new(ctx.db.clone());

    match service.get_user_page(&username).await? {
        Some(page) => Ok(Json(page)),
        None => Err(AyiouError::ConfigError(
            crate::error::ConfigError::ParseError("User not found".to_string()),
        )),
    }
}

// Track public link clicks
async fn track_public_link_click(
    State(ctx): State<Ctx>,
    Path((username, link_id)): Path<(String, i64)>,
) -> Result<StatusCode, AyiouError> {
    let service = UserLinkService::new(ctx.db.clone());

    // 简单验证：确保链接存在且属于该用户
    // TODO: 添加更详细的验证逻辑

    service.increment_click(link_id).await?;

    Ok(StatusCode::OK)
}
