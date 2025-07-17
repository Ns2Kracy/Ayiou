use crate::error::AyiouError;
use crate::middleware::auth::JwtClaims;
use crate::models::user::{
    AuthResponse, LoginPayload, RegisterPayload, USERNAME_REGEX, User, UserResponse,
};
use crate::utils::crypto;
use anyhow::Result;
use sqlx::PgPool;
use validator::Validate;

pub struct AuthService {
    db: PgPool,
}

impl AuthService {
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }

    pub async fn register(&self, payload: RegisterPayload) -> Result<AuthResponse, AyiouError> {
        payload.validate()?;

        // Check if username or email already exists
        let existing_user = sqlx::query!(
            "SELECT id FROM users WHERE username = $1 OR email = $2",
            payload.username,
            payload.email
        )
        .fetch_optional(&self.db)
        .await?;

        if existing_user.is_some() {
            return Err(AyiouError::conflict("Username or email already exists"));
        }

        // Generate salt and password hash
        let salt = crypto::generate_salt();
        let password_hash = crypto::hash_password(&payload.password, &salt);

        // Store salt and hash together, format: "salt:hash"
        let stored_password = format!("{}:{}", salt, password_hash);

        // Create user
        let user = sqlx::query_as!(
            User,
            r#"
            INSERT INTO users (username, email, password_hash, display_name)
            VALUES ($1, $2, $3, $4)
            RETURNING *
            "#,
            payload.username,
            payload.email,
            stored_password,
            payload.display_name
        )
        .fetch_one(&self.db)
        .await?;

        // Generate JWT Token
        let claims = JwtClaims::new(user.id, user.username.clone(), user.email.clone());

        let token = claims.encode_jwt().map_err(|e| AyiouError::AuthError(e))?;

        Ok(AuthResponse {
            token,
            user: UserResponse {
                id: user.id,
                username: user.username,
                email: user.email,
                display_name: user.display_name,
                avatar_url: user.avatar_url,
                bio: user.bio,
                created_at: user.created_at,
            },
        })
    }

    pub async fn login(&self, payload: LoginPayload) -> Result<AuthResponse, AyiouError> {
        // Find user by username or email
        let user = if payload.username_or_email.contains('@') {
            sqlx::query_as!(
                User,
                "SELECT * FROM users WHERE email = $1",
                payload.username_or_email
            )
            .fetch_optional(&self.db)
            .await?
        } else {
            sqlx::query_as!(
                User,
                "SELECT * FROM users WHERE username = $1",
                payload.username_or_email
            )
            .fetch_optional(&self.db)
            .await?
        };

        let user = user.ok_or_else(|| AyiouError::unauthorized("Invalid username or password"))?;

        // Verify password
        let password_parts: Vec<&str> = user.password_hash.split(':').collect();
        if password_parts.len() != 2 {
            return Err(AyiouError::internal("Password format error"));
        }

        let salt = password_parts[0];
        let stored_hash = password_parts[1];

        if !crypto::verify_password(&payload.password, stored_hash, salt) {
            return Err(AyiouError::unauthorized("Invalid username or password"));
        }

        // Generate JWT Token
        let claims = JwtClaims::new(user.id, user.username.clone(), user.email.clone());

        let token = claims.encode_jwt().map_err(|e| AyiouError::AuthError(e))?;

        Ok(AuthResponse {
            token,
            user: UserResponse {
                id: user.id,
                username: user.username,
                email: user.email,
                display_name: user.display_name,
                avatar_url: user.avatar_url,
                bio: user.bio,
                created_at: user.created_at,
            },
        })
    }

    pub async fn verify_token(&self, token: &str) -> Result<JwtClaims, AyiouError> {
        let claims = JwtClaims::decode_jwt(token).map_err(|e| AyiouError::AuthError(e))?;

        // Check if user still exists and is active
        let user_id: i64 = claims
            .sub
            .parse()
            .map_err(|_| AyiouError::unauthorized("Invalid token"))?;

        let user_exists = sqlx::query!("SELECT id FROM users WHERE id = $1", user_id)
            .fetch_optional(&self.db)
            .await?;

        if user_exists.is_none() {
            return Err(AyiouError::unauthorized(
                "User does not exist or is disabled",
            ));
        }

        Ok(claims)
    }

    pub async fn get_user_by_id(&self, user_id: i64) -> Result<UserResponse, AyiouError> {
        let user = sqlx::query_as!(User, "SELECT * FROM users WHERE id = $1", user_id)
            .fetch_optional(&self.db)
            .await?
            .ok_or_else(|| AyiouError::unauthorized("User does not exist"))?;

        Ok(UserResponse {
            id: user.id,
            username: user.username,
            email: user.email,
            display_name: user.display_name,
            avatar_url: user.avatar_url,
            bio: user.bio,
            created_at: user.created_at,
        })
    }
}
