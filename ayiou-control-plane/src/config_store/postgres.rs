use std::sync::Arc;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use ayiou_admin_proto::ConfigBackend;
use sqlx::{PgPool, Row, postgres::PgPoolOptions};
use tokio::sync::OnceCell;

use super::{ConfigRecord, ConfigStore, backend_as_str, backend_from_str};

#[derive(Clone)]
pub struct PostgresConfigStore {
    inner: Arc<PostgresInner>,
}

struct PostgresInner {
    dsn: String,
    pool: OnceCell<PgPool>,
    schema_ready: OnceCell<()>,
}

impl PostgresConfigStore {
    pub fn new(dsn: impl Into<String>) -> Self {
        Self {
            inner: Arc::new(PostgresInner {
                dsn: dsn.into(),
                pool: OnceCell::new(),
                schema_ready: OnceCell::new(),
            }),
        }
    }

    pub fn dsn(&self) -> &str {
        &self.inner.dsn
    }

    async fn pool(&self) -> Result<&PgPool> {
        self.inner
            .pool
            .get_or_try_init(|| async {
                PgPoolOptions::new()
                    .max_connections(5)
                    .connect(&self.inner.dsn)
                    .await
                    .with_context(|| format!("connect postgres {}", self.inner.dsn))
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
    version BIGINT NOT NULL,
    backend TEXT NOT NULL,
    content TEXT NOT NULL,
    PRIMARY KEY (bot_id, plugin_name)
)
"#,
                )
                .execute(pool)
                .await
                .context("create postgres plugin_configs table")?;
                Ok(())
            })
            .await
            .map(|_| ())
    }
}

#[async_trait]
impl ConfigStore for PostgresConfigStore {
    async fn get(&self, bot_id: &str, plugin_name: &str) -> Result<Option<ConfigRecord>> {
        self.ensure_schema().await?;
        let pool = self.pool().await?;

        let row = sqlx::query(
            r#"
SELECT version, backend, content
FROM plugin_configs
WHERE bot_id = $1 AND plugin_name = $2
"#,
        )
        .bind(bot_id)
        .bind(plugin_name)
        .fetch_optional(pool)
        .await
        .context("query postgres config record")?;

        let Some(row) = row else {
            return Ok(None);
        };

        let version = row
            .try_get::<i64, _>("version")
            .context("read postgres version")?;
        if version < 0 {
            bail!("invalid postgres version {}", version);
        }
        let backend = row
            .try_get::<String, _>("backend")
            .context("read postgres backend")?;
        let content = row
            .try_get::<String, _>("content")
            .context("read postgres content")?;

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

        let mut tx = pool.begin().await.context("begin postgres transaction")?;

        let actual = sqlx::query_scalar::<_, i64>(
            r#"
SELECT version
FROM plugin_configs
WHERE bot_id = $1 AND plugin_name = $2
FOR UPDATE
"#,
        )
        .bind(bot_id)
        .bind(plugin_name)
        .fetch_optional(&mut *tx)
        .await
        .context("read postgres current version")?
        .unwrap_or(0);

        if actual < 0 {
            bail!("invalid postgres version {}", actual);
        }
        let actual = actual as u64;

        if let Some(expected) = expected_version
            && expected != actual
        {
            bail!("version conflict: expected {}, actual {}", expected, actual);
        }

        let next = actual
            .checked_add(1)
            .context("postgres config version overflow")?;

        sqlx::query(
            r#"
INSERT INTO plugin_configs (bot_id, plugin_name, version, backend, content)
VALUES ($1, $2, $3, $4, $5)
ON CONFLICT (bot_id, plugin_name) DO UPDATE SET
    version = EXCLUDED.version,
    backend = EXCLUDED.backend,
    content = EXCLUDED.content
"#,
        )
        .bind(bot_id)
        .bind(plugin_name)
        .bind(next as i64)
        .bind(backend_as_str(&backend))
        .bind(content)
        .execute(&mut *tx)
        .await
        .context("upsert postgres config record")?;

        tx.commit().await.context("commit postgres transaction")?;
        Ok(next)
    }
}
