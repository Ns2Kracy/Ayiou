use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

type AppResult<T> = anyhow::Result<T>;
use crate::models::link::{CreateLinkPayload, Link, LinkResponse, UpdateLinkPayload};
use crate::utils::shortener::{ShortCodeGenerator, ShortenerError, normalize_url, validate_url};

pub struct LinkService {
    db: PgPool,
    shortener: Arc<Mutex<ShortCodeGenerator>>,
    base_url: String,
}

impl LinkService {
    pub fn new(db: PgPool, base_url: String) -> Self {
        Self {
            db,
            shortener: Arc::new(Mutex::new(ShortCodeGenerator::new())),
            base_url,
        }
    }

    // Create short link
    pub async fn create_link(
        &self,
        payload: CreateLinkPayload,
        user_id: Option<Uuid>,
    ) -> AppResult<LinkResponse> {
        // Validate and normalize URL
        if !validate_url(&payload.original_url) {
            return Err(anyhow::anyhow!("Invalid URL format"));
        }

        let normalized_url = normalize_url(&payload.original_url)
            .map_err(|e| anyhow::anyhow!("URL normalization failed: {}", e))?;

        // Check if URL already exists (if needed)
        if let Some(existing_link) = self.find_by_url(&normalized_url, user_id).await? {
            return Ok(self.to_response(existing_link));
        }

        // Generate short code
        let short_code = self
            .generate_unique_code(&normalized_url, payload.custom_code.as_deref())
            .await?;

        // Create database record
        let link = sqlx::query_as::<_, Link>(
            r#"
            INSERT INTO links (id, user_id, short_code, original_url, title, description, expires_at, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), NOW())
            RETURNING *
            "#
        )
        .bind(Uuid::new_v4())
        .bind(user_id)
        .bind(&short_code)
        .bind(&normalized_url)
        .bind(payload.title.as_deref())
        .bind(payload.description.as_deref())
        .bind(payload.expires_at)
        .fetch_one(&self.db)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create short link: {}", e))?;

        Ok(self.to_response(link))
    }

    // Get link by short code
    pub async fn get_by_short_code(&self, short_code: &str) -> AppResult<Option<Link>> {
        let link = sqlx::query_as::<_, Link>("SELECT * FROM links WHERE short_code = $1")
            .bind(short_code)
            .fetch_optional(&self.db)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to query short link: {}", e))?;

        // Check if expired
        if let Some(ref link) = link {
            if let Some(expires_at) = link.expires_at {
                if expires_at < Utc::now() {
                    return Ok(None);
                }
            }
        }

        Ok(link)
    }

    // Update click count
    pub async fn increment_click_count(&self, link_id: Uuid) -> AppResult<()> {
        sqlx::query(
            "UPDATE links SET click_count = click_count + 1, updated_at = NOW() WHERE id = $1",
        )
        .bind(link_id)
        .execute(&self.db)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to update click count: {}", e))?;

        Ok(())
    }

    // Get all links for user
    pub async fn get_user_links(
        &self,
        user_id: Uuid,
        page: i64,
        limit: i64,
    ) -> AppResult<Vec<LinkResponse>> {
        let offset = (page - 1) * limit;

        let links = sqlx::query_as::<_, Link>(
            r#"
            SELECT * FROM links
            WHERE user_id = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(user_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.db)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to query user links: {}", e))?;

        Ok(links
            .into_iter()
            .map(|link| self.to_response(link))
            .collect())
    }

    // Update link
    pub async fn update_link(
        &self,
        link_id: Uuid,
        payload: UpdateLinkPayload,
        user_id: Option<Uuid>,
    ) -> AppResult<LinkResponse> {
        // Verify permissions
        let existing_link = self
            .get_link_by_id(link_id, user_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Link does not exist or no permission to access"))?;

        // Execute update (simplified version)
        let updated_link = self.update_link_direct(link_id, payload).await?;

        Ok(self.to_response(updated_link))
    }

    // Delete link
    pub async fn delete_link(&self, link_id: Uuid, user_id: Option<Uuid>) -> AppResult<()> {
        let result = sqlx::query(
            "UPDATE links SET updated_at = NOW() WHERE id = $1 AND ($2::uuid IS NULL OR user_id = $2)"
        )
        .bind(link_id)
        .bind(user_id)
        .execute(&self.db)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to delete link: {}", e))?;

        if result.rows_affected() == 0 {
            return Err(anyhow::anyhow!(
                "Link does not exist or no permission to delete"
            ));
        }

        Ok(())
    }

    // Get link statistics
    pub async fn get_link_stats(
        &self,
        link_id: Uuid,
        user_id: Option<Uuid>,
    ) -> AppResult<serde_json::Value> {
        let link = self
            .get_link_by_id(link_id, user_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Link does not exist or no permission to access"))?;

        // This can be extended with more detailed statistics
        Ok(serde_json::json!({
            "click_count": link.click_count,
            "created_at": link.created_at,
            "last_updated": link.updated_at,
            "expires_at": link.expires_at
        }))
    }

    // Get generator statistics
    pub fn get_generator_stats(&self) -> (u64, u64, u64) {
        let shortener = self.shortener.lock().unwrap();
        let stats = shortener.get_stats();
        (
            stats.total_generated,
            stats.random_seed,
            stats.current_timestamp,
        )
    }

    // Private helper methods

    // Generate unique short code
    async fn generate_unique_code(
        &self,
        original_url: &str,
        custom_code: Option<&str>,
    ) -> AppResult<String> {
        // If custom code is provided, validate first
        if let Some(code) = custom_code {
            {
                let shortener = self.shortener.lock().unwrap();
                shortener.validate_custom_code(code).map_err(|e| match e {
                    ShortenerError::CodeTooShort => anyhow::anyhow!("Custom code too short"),
                    ShortenerError::CodeTooLong => anyhow::anyhow!("Custom code too long"),
                    ShortenerError::InvalidChar => {
                        anyhow::anyhow!("Custom code contains invalid characters")
                    }
                    ShortenerError::UnsafeContent => {
                        anyhow::anyhow!("Custom code contains inappropriate content")
                    }
                })?;
            }

            // Check if already exists in database
            let existing = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM links WHERE short_code = $1)",
            )
            .bind(code)
            .fetch_one(&self.db)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to check code duplication: {}", e))?;

            if existing {
                return Err(anyhow::anyhow!("Custom code already exists"));
            }

            return Ok(code.to_string());
        }

        // Generate system code and check uniqueness
        let mut attempts = 0;
        while attempts < 100 {
            let code = {
                let shortener = self.shortener.lock().unwrap();
                shortener.generate()
            };

            // Check if already exists in database
            let existing = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM links WHERE short_code = $1)",
            )
            .bind(&code)
            .fetch_one(&self.db)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to check code duplication: {}", e))?;

            if !existing {
                return Ok(code);
            }

            attempts += 1;
        }

        Err(anyhow::anyhow!(
            "Failed to generate short code: too many duplicates"
        ))
    }

    // Find existing link by URL
    async fn find_by_url(&self, url: &str, user_id: Option<Uuid>) -> AppResult<Option<Link>> {
        let link = sqlx::query_as::<_, Link>(
            "SELECT * FROM links WHERE original_url = $1 AND ($2::uuid IS NULL OR user_id = $2) LIMIT 1"
        )
        .bind(url)
        .bind(user_id)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to query existing link: {}", e))?;

        Ok(link)
    }

    // Get link by ID
    async fn get_link_by_id(
        &self,
        link_id: Uuid,
        user_id: Option<Uuid>,
    ) -> AppResult<Option<Link>> {
        let link = sqlx::query_as::<_, Link>(
            "SELECT * FROM links WHERE id = $1 AND ($2::uuid IS NULL OR user_id = $2)",
        )
        .bind(link_id)
        .bind(user_id)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to query link: {}", e))?;

        Ok(link)
    }

    // Direct link update (simplified version)
    async fn update_link_direct(
        &self,
        link_id: Uuid,
        payload: UpdateLinkPayload,
    ) -> AppResult<Link> {
        let link = sqlx::query_as::<_, Link>(
            r#"
            UPDATE links SET
                original_url = COALESCE($2, original_url),
                title = COALESCE($3, title),
                description = COALESCE($4, description),
                expires_at = COALESCE($5, expires_at),
                updated_at = NOW()
            WHERE id = $1
            RETURNING *
            "#,
        )
        .bind(link_id)
        .bind(payload.original_url.as_deref())
        .bind(payload.title.as_deref())
        .bind(payload.description.as_deref())
        .bind(payload.expires_at)
        .fetch_one(&self.db)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to update link: {}", e))?;

        Ok(link)
    }

    // Convert to response format
    fn to_response(&self, link: Link) -> LinkResponse {
        LinkResponse {
            id: link.id,
            short_code: link.short_code.clone(),
            original_url: link.original_url,
            title: link.title,
            description: link.description,
            short_url: format!(
                "{}/{}",
                self.base_url.trim_end_matches('/'),
                link.short_code
            ),
            expires_at: link.expires_at,
            click_count: link.click_count,
            created_at: link.created_at,
            updated_at: link.updated_at,
        }
    }
}

// Extended functionality: link analysis and management
impl LinkService {
    // Get popular links
    pub async fn get_popular_links(&self, limit: i64) -> AppResult<Vec<LinkResponse>> {
        let links = sqlx::query_as::<_, Link>(
            "SELECT * FROM links ORDER BY click_count DESC LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.db)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to query popular links: {}", e))?;

        Ok(links
            .into_iter()
            .map(|link| self.to_response(link))
            .collect())
    }

    // Clean up expired links
    pub async fn cleanup_expired_links(&self) -> AppResult<u64> {
        let result = sqlx::query(
            "UPDATE links SET expires_at = NOW() WHERE expires_at IS NOT NULL AND expires_at < NOW()"
        )
        .execute(&self.db)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to cleanup expired links: {}", e))?;

        Ok(result.rows_affected())
    }

    // Get system statistics
    pub async fn get_system_stats(&self) -> AppResult<serde_json::Value> {
        let total_links =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM links")
                .fetch_one(&self.db)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to query total links count: {}", e))?;

        let total_clicks = sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(SUM(click_count), 0) FROM links",
        )
        .fetch_one(&self.db)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to query total clicks count: {}", e))?;

        let (generated, random_seed, timestamp) = self.get_generator_stats();

        Ok(serde_json::json!({
            "total_links": total_links,
            "total_clicks": total_clicks,
            "generator_stats": {
                "total_generated": generated,
                "random_seed": random_seed,
                "current_timestamp": timestamp
            }
        }))
    }
}
