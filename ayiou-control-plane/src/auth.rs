use std::collections::HashSet;

use axum::{
    extract::{FromRef, FromRequestParts},
    http::{StatusCode, header::AUTHORIZATION, request::Parts},
};

use crate::app::AppState;

#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub username: String,
    pub permissions: HashSet<String>,
}

impl AuthenticatedUser {
    pub fn new(username: impl Into<String>, permissions: &[&str]) -> Self {
        Self {
            username: username.into(),
            permissions: permissions.iter().map(|p| (*p).to_string()).collect(),
        }
    }
}

impl<S> FromRequestParts<S> for AuthenticatedUser
where
    AppState: axum::extract::FromRef<S>,
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);
        let auth_header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .ok_or(StatusCode::UNAUTHORIZED)?;

        let token = auth_header
            .strip_prefix("Bearer ")
            .unwrap_or(auth_header)
            .trim();

        app_state
            .user_for_token(token)
            .ok_or(StatusCode::UNAUTHORIZED)
    }
}
