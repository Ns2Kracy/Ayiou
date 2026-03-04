use axum::http::StatusCode;

use crate::auth::AuthenticatedUser;

pub fn require(user: &AuthenticatedUser, permission: &str) -> Result<(), StatusCode> {
    if user.permissions.contains(permission) {
        return Ok(());
    }

    Err(StatusCode::FORBIDDEN)
}
