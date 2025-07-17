use anyhow::Result;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::user::{
    CreateUserLinkPayload, UpdateUserLinkPayload, UserLink, UserLinkResponse, UserPageInfo,
    UserPageResponse,
};

pub struct UserLinkService {
    db: PgPool,
}

impl UserLinkService {
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }

    // Create user link
    pub async fn create_user_link(
        &self,
        user_id: i64,
        payload: CreateUserLinkPayload,
    ) -> Result<UserLinkResponse> {
        let next_position = self.get_next_position(user_id).await?;

        let link = sqlx::query_as::<_, UserLink>(
            r#"
            INSERT INTO user_links (id, user_id, title, url, icon, position)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING *
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(user_id)
        .bind(&payload.title)
        .bind(&payload.url)
        .bind(payload.icon.as_deref())
        .bind(next_position)
        .fetch_one(&self.db)
        .await?;

        Ok(self.to_response(link))
    }

    // Get all user links
    pub async fn get_user_links(&self, user_id: i64) -> Result<Vec<UserLinkResponse>> {
        let links = sqlx::query_as::<_, UserLink>(
            r#"
            SELECT * FROM user_links
            WHERE user_id = $1
            ORDER BY position ASC, created_at ASC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.db)
        .await?;

        Ok(links
            .into_iter()
            .map(|link| self.to_response(link))
            .collect())
    }

    // Update user link
    pub async fn update_user_link(
        &self,
        user_id: i64,
        link_id: i64,
        payload: UpdateUserLinkPayload,
    ) -> Result<UserLinkResponse> {
        let link = sqlx::query_as::<_, UserLink>(
            r#"
            UPDATE user_links
            SET title = COALESCE($3, title),
                url = COALESCE($4, url),
                icon = COALESCE($5, icon),
                position = COALESCE($6, position),
                updated_at = NOW()
            WHERE id = $1 AND user_id = $2
            RETURNING *
            "#,
        )
        .bind(link_id)
        .bind(user_id)
        .bind(&payload.title)
        .bind(&payload.url)
        .bind(&payload.icon)
        .bind(payload.position)
        .fetch_one(&self.db)
        .await?;

        Ok(self.to_response(link))
    }

    // Delete user link
    pub async fn delete_user_link(&self, user_id: i64, link_id: i64) -> Result<()> {
        sqlx::query("DELETE FROM user_links WHERE id = $1 AND user_id = $2")
            .bind(link_id)
            .bind(user_id)
            .execute(&self.db)
            .await?;

        Ok(())
    }

    // Increment click count
    pub async fn increment_click(&self, link_id: i64) -> Result<()> {
        sqlx::query("UPDATE user_links SET click_count = click_count + 1 WHERE id = $1")
            .bind(link_id)
            .execute(&self.db)
            .await?;

        Ok(())
    }

    // Get user page data
    pub async fn get_user_page(&self, username: &str) -> Result<Option<UserPageResponse>> {
        // Get user information
        let user = sqlx::query_as::<_, UserPageInfo>(
            r#"
            SELECT username, display_name, avatar_url, bio, is_verified
            FROM users
            WHERE username = $1
            "#,
        )
        .bind(username)
        .fetch_optional(&self.db)
        .await?;

        let user = match user {
            Some(u) => u,
            None => return Ok(None),
        };

        // Get user ID for querying links
        let user_id: i64 = sqlx::query_scalar("SELECT id FROM users WHERE username = $1")
            .bind(username)
            .fetch_one(&self.db)
            .await?;

        // Get links
        let links = self.get_user_links(user_id).await?;

        // Calculate total click count
        let total_clicks: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(click_count), 0) FROM user_links WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_one(&self.db)
        .await?;

        Ok(Some(UserPageResponse {
            user,
            links,
            total_clicks,
        }))
    }

    // Helper method
    async fn get_next_position(&self, user_id: i64) -> Result<i32> {
        let max_position: Option<i32> =
            sqlx::query_scalar("SELECT MAX(position) FROM user_links WHERE user_id = $1")
                .bind(user_id)
                .fetch_one(&self.db)
                .await?;

        Ok(max_position.unwrap_or(0) + 1)
    }

    fn to_response(&self, link: UserLink) -> UserLinkResponse {
        UserLinkResponse {
            id: link.id,
            title: link.title,
            url: link.url,
            icon: link.icon,
            position: link.position,
            click_count: link.click_count,
            created_at: link.created_at,
        }
    }
}
