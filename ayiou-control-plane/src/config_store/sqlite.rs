use std::sync::Arc;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use ayiou_admin_proto::ConfigBackend;
use sqlx::{
    Row, SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use tokio::sync::OnceCell;

use super::{ConfigRecord, ConfigStore, backend_as_str, backend_from_str};

#[derive(Clone)]
pub struct SqliteConfigStore {
    inner: Arc<SqliteInner>,
}

struct SqliteInner {
    database_url: String,
    pool: OnceCell<SqlitePool>,
    schema_ready: OnceCell<()>,
}

impl SqliteConfigStore {
    pub fn in_memory() -> Self {
        // Use a single pooled connection so one in-memory database is shared for this store.
        Self::new("sqlite::memory:")
    }

    pub fn new(database_url: impl Into<String>) -> Self {
        Self {
            inner: Arc::new(SqliteInner {
                database_url: database_url.into(),
                pool: OnceCell::new(),
                schema_ready: OnceCell::new(),
            }),
        }
    }

    pub fn database_url(&self) -> &str {
        &self.inner.database_url
    }

    async fn pool(&self) -> Result<&SqlitePool> {
        self.inner
            .pool
            .get_or_try_init(|| async {
                let options = self
                    .inner
                    .database_url
                    .parse::<SqliteConnectOptions>()
                    .with_context(|| format!("parse sqlite url {}", self.inner.database_url))?
                    .create_if_missing(true);

                SqlitePoolOptions::new()
                    .max_connections(1)
                    .connect_with(options)
                    .await
                    .with_context(|| format!("connect sqlite database {}", self.inner.database_url))
            })
            .await
    }

    async fn ensure_schema(&self) -> Result<()> {
        let pool = self.pool().await?;
        self.inner
            .schema_ready
            .get_or_try_init(|| async {
                sqlx::query(
                    r#"
CREATE TABLE IF NOT EXISTS plugin_configs (
    bot_id TEXT NOT NULL,
    plugin_name TEXT NOT NULL,
    version INTEGER NOT NULL,
    backend TEXT NOT NULL,
    content TEXT NOT NULL,
    PRIMARY KEY (bot_id, plugin_name)
)
"#,
                )
                .execute(pool)
                .await
                .context("create sqlite plugin_configs table")?;

                Ok(())
            })
            .await
            .map(|_| ())
    }
}

#[async_trait]
impl ConfigStore for SqliteConfigStore {
    async fn get(&self, bot_id: &str, plugin_name: &str) -> Result<Option<ConfigRecord>> {
        self.ensure_schema().await?;
        let pool = self.pool().await?;

        let row = sqlx::query(
            r#"
SELECT version, backend, content
FROM plugin_configs
WHERE bot_id = ?1 AND plugin_name = ?2
"#,
        )
        .bind(bot_id)
        .bind(plugin_name)
        .fetch_optional(pool)
        .await
        .context("query sqlite config record")?;

        let Some(row) = row else {
            return Ok(None);
        };

        let version = row
            .try_get::<i64, _>("version")
            .context("read sqlite version")?;
        if version < 0 {
            bail!("invalid sqlite version {}", version);
        }

        let backend = row
            .try_get::<String, _>("backend")
            .context("read sqlite backend")?;
        let content = row
            .try_get::<String, _>("content")
            .context("read sqlite content")?;

        Ok(Some(ConfigRecord {
            version: version as u64,
            backend: backend_from_str(&backend)?,
            content,
        }))
    }

    async fn put(
        &self,
        bot_id: &str,
        plugin_name: &str,
        backend: ConfigBackend,
        content: &str,
        expected_version: Option<u64>,
    ) -> Result<u64> {
        self.ensure_schema().await?;
        let pool = self.pool().await?;

        let mut tx = pool.begin().await.context("begin sqlite transaction")?;

        let actual = sqlx::query_scalar::<_, i64>(
            r#"
SELECT version
FROM plugin_configs
WHERE bot_id = ?1 AND plugin_name = ?2
"#,
        )
        .bind(bot_id)
        .bind(plugin_name)
        .fetch_optional(&mut *tx)
        .await
        .context("read sqlite current version")?
        .unwrap_or(0);

        if actual < 0 {
            bail!("invalid sqlite version {}", actual);
        }
        let actual = actual as u64;

        if let Some(expected) = expected_version
            && expected != actual
        {
            bail!("version conflict: expected {}, actual {}", expected, actual);
        }

        let next = actual
            .checked_add(1)
            .context("sqlite config version overflow")?;

        sqlx::query(
            r#"
INSERT INTO plugin_configs (bot_id, plugin_name, version, backend, content)
VALUES (?1, ?2, ?3, ?4, ?5)
ON CONFLICT(bot_id, plugin_name) DO UPDATE SET
    version = excluded.version,
    backend = excluded.backend,
    content = excluded.content
"#,
        )
        .bind(bot_id)
        .bind(plugin_name)
        .bind(next as i64)
        .bind(backend_as_str(&backend))
        .bind(content)
        .execute(&mut *tx)
        .await
        .context("upsert sqlite config record")?;

        tx.commit().await.context("commit sqlite transaction")?;
        Ok(next)
    }
}
