use crate::{
    app::config::CONFIG,
    error::{AuthError, AyiouError},
};
use axum::{RequestPartsExt, extract::FromRequestParts, http::request::Parts};
use axum_extra::{
    TypedHeader,
    headers::{Authorization, authorization::Bearer},
};
use chrono::{DateTime, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, decode, encode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct JwtClaims {
    // Subject (user ID)
    pub sub: String,
    // Expiration Time
    pub exp: i64,
    // Issuer
    pub iss: String,
    // Not Before
    pub nbf: i64,
    // Username
    pub username: String,
    // User email
    pub email: String,
}

/// Authenticated user info - extracted from JWT token
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: i64,
    pub username: String,
    pub email: String,
}

impl JwtClaims {
    pub fn new(user_id: i64, username: String, email: String) -> Self {
        let now = chrono::Utc::now();

        JwtClaims {
            sub: user_id.to_string(),
            exp: (now + chrono::Duration::hours(CONFIG.jwt.expires_in_hours as i64)).timestamp(),
            iss: "Ayiou".to_string(),
            nbf: now.timestamp(),
            username,
            email,
        }
    }

    pub fn encode_jwt(&self) -> Result<String, AuthError> {
        encode(
            &Header::new(Algorithm::default()),
            &self,
            &EncodingKey::from_secret(CONFIG.jwt.secret.as_bytes()),
        )
        .map_err(|_| AuthError::TokenCreation)
    }

    pub fn decode_jwt(token: &str) -> Result<Self, AuthError> {
        decode::<Self>(
            token,
            &DecodingKey::from_secret(CONFIG.jwt.secret.as_bytes()),
            &jsonwebtoken::Validation::new(Algorithm::default()),
        )
        .map(|data| data.claims)
        .map_err(|_| AuthError::InvalidToken)
    }

    /// Convert to AuthUser struct
    pub fn to_auth_user(&self) -> Result<AuthUser, AuthError> {
        let user_id = self
            .sub
            .parse::<i64>()
            .map_err(|_| AuthError::InvalidToken)?;

        Ok(AuthUser {
            user_id,
            username: self.username.clone(),
            email: self.email.clone(),
        })
    }
}

impl<S> FromRequestParts<S> for JwtClaims
where
    S: Send + Sync,
{
    type Rejection = AyiouError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // Extract the token from the authorization header
        let TypedHeader(Authorization(bearer)) = parts
            .extract::<TypedHeader<Authorization<Bearer>>>()
            .await
            .map_err(|_| AyiouError::AuthError(AuthError::MissingAuth))?;

        // Decode the user data
        let token_data =
            JwtClaims::decode_jwt(bearer.token()).map_err(|e| AyiouError::AuthError(e))?;

        // Check if token is expired
        let now = chrono::Utc::now().timestamp();
        if token_data.exp < now {
            return Err(AyiouError::AuthError(AuthError::InvalidToken));
        }

        // Check if token is valid yet
        if token_data.nbf > now {
            return Err(AyiouError::AuthError(AuthError::InvalidToken));
        }

        Ok(token_data)
    }
}

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = AyiouError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let claims = JwtClaims::from_request_parts(parts, state).await?;
        claims.to_auth_user().map_err(|e| AyiouError::AuthError(e))
    }
}
