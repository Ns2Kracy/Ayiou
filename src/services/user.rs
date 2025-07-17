use crate::error::AyiouError;
use crate::models::user::{User, UserLinkResponse, UserPageInfo, UserPageResponse, UserResponse};
use crate::utils::crypto;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use validator::Validate;

#[derive(Debug, Validate, Deserialize)]
pub struct UpdateUserPayload {
    #[validate(length(max = 100, message = "Display name too long"))]
    pub display_name: Option<String>,

    #[validate(url(message = "Invalid avatar URL format"))]
    pub avatar_url: Option<String>,

    #[validate(length(max = 500, message = "Bio too long"))]
    pub bio: Option<String>,
}

#[derive(Debug, Validate, Deserialize)]
pub struct ChangePasswordPayload {
    pub current_password: String,

    #[validate(length(min = 8, message = "Password must be at least 8 characters"))]
    pub new_password: String,
}

#[derive(Debug, Serialize)]
pub struct UserStats {
    pub total_links: i64,
    pub total_clicks: i64,
    pub active_links: i64,
}

pub struct UserService {
    db: PgPool,
}

impl UserService {
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }

    pub async fn get_user_by_id(&self, user_id: i64) -> Result<Option<UserResponse>, AyiouError> {
        let user = sqlx::query_as!(User, "SELECT * FROM users WHERE id = $1", user_id)
            .fetch_optional(&self.db)
            .await?;

        Ok(user.map(|u| UserResponse {
            id: u.id,
            username: u.username,
            email: u.email,
            display_name: u.display_name,
            avatar_url: u.avatar_url,
            bio: u.bio,
            created_at: u.created_at,
        }))
    }

    pub async fn get_user_by_username(
        &self,
        username: &str,
    ) -> Result<Option<UserResponse>, AyiouError> {
        let user = sqlx::query_as!(User, "SELECT * FROM users WHERE username = $1", username)
            .fetch_optional(&self.db)
            .await?;

        Ok(user.map(|u| UserResponse {
            id: u.id,
            username: u.username,
            email: u.email,
            display_name: u.display_name,
            avatar_url: u.avatar_url,
            bio: u.bio,
            created_at: u.created_at,
        }))
    }

    pub async fn update_user(
        &self,
        user_id: i64,
        payload: UpdateUserPayload,
    ) -> Result<UserResponse, AyiouError> {
        payload.validate()?;

        let user = sqlx::query_as!(
            User,
            r#"
            UPDATE users
            SET display_name = COALESCE($2, display_name),
                avatar_url = COALESCE($3, avatar_url),
                bio = COALESCE($4, bio),
                updated_at = NOW()
            WHERE id = $1
            RETURNING *
            "#,
            user_id,
            payload.display_name,
            payload.avatar_url,
            payload.bio
        )
        .fetch_optional(&self.db)
        .await?;

        let user = user.ok_or_else(|| AyiouError::bad_request("用户不存在"))?;

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

    pub async fn change_password(
        &self,
        user_id: i64,
        payload: ChangePasswordPayload,
    ) -> Result<(), AyiouError> {
        payload.validate()?;

        // 获取当前用户信息
        let user = sqlx::query_as!(User, "SELECT * FROM users WHERE id = $1", user_id)
            .fetch_optional(&self.db)
            .await?;

        let user = user.ok_or_else(|| AyiouError::bad_request("用户不存在"))?;

        // 验证当前密码
        let password_parts: Vec<&str> = user.password_hash.split(':').collect();
        if password_parts.len() != 2 {
            return Err(AyiouError::internal("密码格式错误"));
        }

        let salt = password_parts[0];
        let stored_hash = password_parts[1];

        if !crypto::verify_password(&payload.current_password, stored_hash, salt) {
            return Err(AyiouError::unauthorized("当前密码错误"));
        }

        // 生成新的盐和哈希
        let new_salt = crypto::generate_salt();
        let new_password_hash = crypto::hash_password(&payload.new_password, &new_salt);
        let new_stored_password = format!("{}:{}", new_salt, new_password_hash);

        // 更新密码
        sqlx::query!(
            "UPDATE users SET password_hash = $2, updated_at = NOW() WHERE id = $1",
            user_id,
            new_stored_password
        )
        .execute(&self.db)
        .await?;

        Ok(())
    }

    pub async fn get_user_stats(&self, user_id: i64) -> Result<UserStats, AyiouError> {
        let stats = sqlx::query!(
            r#"
            SELECT
                COUNT(ul.id) as total_links,
                COALESCE(SUM(ul.click_count), 0) as total_clicks,
            FROM user_links ul
            WHERE ul.user_id = $1
            "#,
            user_id
        )
        .fetch_one(&self.db)
        .await?;

        Ok(UserStats {
            total_links: stats.total_links.unwrap_or(0),
            total_clicks: stats.total_clicks.unwrap_or(0),
            active_links: stats.active_links.unwrap_or(0),
        })
    }

    pub async fn get_user_page(&self, username: &str) -> Result<UserPageResponse, AyiouError> {
        // 获取用户信息
        let user_info = sqlx::query_as!(
            UserPageInfo,
            r#"
            SELECT username, display_name, avatar_url, bio
            FROM users
            WHERE username = $1
            "#,
            username
        )
        .fetch_optional(&self.db)
        .await?;

        let user_info = user_info.ok_or_else(|| AyiouError::bad_request("用户不存在"))?;

        // 获取用户链接
        let links = sqlx::query_as!(
            UserLinkResponse,
            r#"
            SELECT id, title, url, icon, position, click_count, created_at
            FROM user_links
            WHERE user_id = (SELECT id FROM users WHERE username = $1)
            ORDER BY position ASC
            "#,
            username
        )
        .fetch_all(&self.db)
        .await?;

        // 计算总点击数
        let total_clicks: i64 = links.iter().map(|link| link.click_count).sum();

        Ok(UserPageResponse {
            user: user_info,
            links,
            total_clicks,
        })
    }

    pub async fn search_users(
        &self,
        query: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<UserResponse>, AyiouError> {
        let users = sqlx::query_as!(
            User,
            r#"
            SELECT * FROM users
            WHERE (username ILIKE $1 OR display_name ILIKE $1)
            ORDER BY
                CASE WHEN username ILIKE $1 THEN 1 ELSE 2 END,
                username
            LIMIT $2 OFFSET $3
            "#,
            format!("%{}%", query),
            limit,
            offset
        )
        .fetch_all(&self.db)
        .await?;

        Ok(users
            .into_iter()
            .map(|u| UserResponse {
                id: u.id,
                username: u.username,
                email: u.email,
                display_name: u.display_name,
                avatar_url: u.avatar_url,
                bio: u.bio,
                created_at: u.created_at,
            })
            .collect())
    }
}
