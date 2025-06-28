use axum::{
    Json,
    response::{IntoResponse, Response},
};
use hyper::StatusCode;
use serde_json::json;

#[derive(thiserror::Error, Debug)]
pub enum AyiouError {
    #[error("{0}")]
    ConfigError(#[from] ConfigError),

    #[error("{0}")]
    DbError(#[from] sqlx::Error),

    #[error("{0}")]
    BcryptError(#[from] bcrypt::BcryptError),

    #[error("{0}")]
    SerdeJsonError(#[from] serde_json::Error),

    #[error("{0}")]
    ValidationError(#[from] validator::ValidationErrors),

    #[error("{0}")]
    JwtError(#[from] jsonwebtoken::errors::Error),
}

impl AyiouError {
    fn code(&self) -> (StatusCode, String) {
        match self {
            Self::ConfigError(err) => err.code(),
            Self::DbError(err) => {
                tracing::error!("Database error: {}", err);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "An error occurred while accessing the database".to_string(),
                )
            }
            Self::BcryptError(err) => {
                tracing::error!("Bcrypt error: {}", err);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "An error occurred while processing authentication".to_string(),
                )
            }
            Self::SerdeJsonError(err) => {
                tracing::error!("Serde JSON error: {}", err);
                (StatusCode::BAD_REQUEST, "Invalid JSON format".to_string())
            }
            Self::ValidationError(err) => {
                (StatusCode::BAD_REQUEST, format!("Validation error: {err}"))
            }
            Self::JwtError(err) => {
                tracing::error!("JWT error: {}", err);
                (
                    StatusCode::UNAUTHORIZED,
                    "Authentication token error".to_string(),
                )
            }
        }
    }
}

// Implement Axum's IntoResponse for our error type
impl IntoResponse for AyiouError {
    fn into_response(self) -> Response {
        let (status_code, message) = self.code();
        let body = Json(json!({
            "error": message,
        }));

        (status_code, body).into_response()
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("Failed to load configuration: {0}")]
    LoadError(#[from] config::ConfigError),

    #[error("Failed to parse configuration: {0}")]
    ParseError(String),

    #[error("Failed to write configuration: {0}")]
    WriteError(String),

    #[error("Configuration not initialized")]
    NotInitialized,
}

impl ConfigError {
    fn code(&self) -> (StatusCode, String) {
        match self {
            Self::LoadError(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to load configuration: {err}"),
            ),
            Self::ParseError(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to parse configuration: {msg}"),
            ),
            Self::WriteError(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to write configuration: {msg}"),
            ),
            Self::NotInitialized => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Configuration not initialized".to_string(),
            ),
        }
    }
}
