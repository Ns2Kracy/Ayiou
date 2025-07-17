use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;

// Short link model
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Link {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub short_code: String,
    pub original_url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub click_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// Link-related payloads and responses
#[derive(Debug, Validate, Deserialize)]
pub struct CreateLinkPayload {
    #[validate(url(message = "Invalid URL format"))]
    pub original_url: String,

    #[validate(length(max = 20, message = "Custom code too long"))]
    pub custom_code: Option<String>,

    #[validate(length(max = 200, message = "Title too long"))]
    pub title: Option<String>,

    #[validate(length(max = 500, message = "Description too long"))]
    pub description: Option<String>,

    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Validate, Deserialize)]
pub struct UpdateLinkPayload {
    #[validate(url(message = "Invalid URL format"))]
    pub original_url: Option<String>,

    #[validate(length(max = 200, message = "Title too long"))]
    pub title: Option<String>,

    #[validate(length(max = 500, message = "Description too long"))]
    pub description: Option<String>,

    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct LinkResponse {
    pub id: Uuid,
    pub short_code: String,
    pub original_url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub short_url: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub click_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
