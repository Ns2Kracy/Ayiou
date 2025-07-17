// User-related database operations and queries
// This module will contain user-specific database methods

use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use regex;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use validator::Validate;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub email: String,
    pub password_hash: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub bio: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// Simplified user link model - removed complex categorization system
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct UserLink {
    pub id: i64,
    pub user_id: i64,
    pub title: String,
    pub url: String,
    pub icon: Option<String>,
    pub position: i32,
    pub click_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Validate, Deserialize)]
pub struct RegisterPayload {
    #[validate(length(min = 3, max = 30, message = "Username must be 3-30 characters"))]
    #[validate(regex(
        path = "*USERNAME_REGEX",
        message = "Username can only contain letters, numbers, and underscores"
    ))]
    pub username: String,

    #[validate(email(message = "Invalid email format"))]
    pub email: String,

    #[validate(length(min = 8, message = "Password must be at least 8 characters"))]
    pub password: String,

    #[validate(length(max = 100, message = "Display name too long"))]
    pub display_name: Option<String>,
}

#[derive(Debug, Validate, Deserialize)]
pub struct LoginPayload {
    pub username_or_email: String,
    pub password: String,
}

// Simplified user link payload
#[derive(Debug, Validate, Deserialize)]
pub struct CreateUserLinkPayload {
    #[validate(length(min = 1, max = 100, message = "Title must be 1-100 characters"))]
    pub title: String,

    #[validate(url(message = "Invalid URL format"))]
    pub url: String,

    pub icon: Option<String>,
}

#[derive(Debug, Validate, Deserialize)]
pub struct UpdateUserLinkPayload {
    #[validate(length(min = 1, max = 100, message = "Title must be 1-100 characters"))]
    pub title: Option<String>,
    #[validate(url(message = "Invalid URL format"))]
    pub url: Option<String>,
    pub icon: Option<String>,
    pub position: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: i64,
    pub username: String,
    pub email: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub bio: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub user: UserResponse,
}

// Simplified user link response
#[derive(Debug, Serialize)]
pub struct UserLinkResponse {
    pub id: i64,
    pub title: String,
    pub url: String,
    pub icon: Option<String>,
    pub position: i32,
    pub click_count: i64,
    pub created_at: DateTime<Utc>,
}

// Simplified user page response
#[derive(Debug, Serialize)]
pub struct UserPageResponse {
    pub user: UserPageInfo,
    pub links: Vec<UserLinkResponse>,
    pub total_clicks: i64,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct UserPageInfo {
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub bio: Option<String>,
}

// Validation regex constants
pub static USERNAME_REGEX: Lazy<regex::Regex> =
    Lazy::new(|| regex::Regex::new(r"^[a-zA-Z0-9_]+$").unwrap());
