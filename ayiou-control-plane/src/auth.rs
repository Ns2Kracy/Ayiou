use std::collections::HashSet;

use axum::{
    extract::{FromRef, FromRequestParts},
    http::{
        StatusCode,
        header::{AUTHORIZATION, COOKIE},
        request::Parts,
    },
};

use crate::app::AppState;

const AUTH_COOKIE_NAME: &str = "ayiou_token";

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
        let token = extract_bearer_token(parts)
            .or_else(|| extract_cookie_token(parts, AUTH_COOKIE_NAME))
            .ok_or(StatusCode::UNAUTHORIZED)?;

        app_state
            .user_for_token(&token)
            .ok_or(StatusCode::UNAUTHORIZED)
    }
}

fn extract_bearer_token(parts: &Parts) -> Option<String> {
    let auth_header = parts
        .headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .unwrap_or(auth_header)
        .trim();
    if token.is_empty() {
        return None;
    }
    Some(token.to_string())
}

fn extract_cookie_token(parts: &Parts, name: &str) -> Option<String> {
    let raw_cookie = parts
        .headers
        .get(COOKIE)
        .and_then(|value| value.to_str().ok())?;

    for pair in raw_cookie.split(';') {
        let mut parts = pair.trim().splitn(2, '=');
        let key = parts.next()?.trim();
        let value = parts.next()?.trim();
        if key == name && !value.is_empty() {
            return Some(value.to_string());
        }
    }

    None
}
