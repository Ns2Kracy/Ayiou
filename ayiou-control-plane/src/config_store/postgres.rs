use anyhow::Result;
use async_trait::async_trait;
use ayiou_admin_proto::ConfigBackend;

use super::{ConfigRecord, ConfigStore, InMemoryConfigStore};

#[derive(Clone, Default)]
pub struct PostgresConfigStore {
    inner: InMemoryConfigStore,
    dsn: Option<String>,
}

impl PostgresConfigStore {
    pub fn in_memory() -> Self {
        Self {
            inner: InMemoryConfigStore::default(),
            dsn: Some("postgres://localhost/ayiou".to_string()),
        }
    }

    pub fn new(dsn: impl Into<String>) -> Self {
        Self {
            inner: InMemoryConfigStore::default(),
            dsn: Some(dsn.into()),
        }
    }

    pub fn dsn(&self) -> Option<&str> {
        self.dsn.as_deref()
    }
}

#[async_trait]
impl ConfigStore for PostgresConfigStore {
    async fn get(&self, bot_id: &str, plugin_name: &str) -> Result<Option<ConfigRecord>> {
        self.inner.get(bot_id, plugin_name).await
    }

    async fn put(
        &self,
        bot_id: &str,
        plugin_name: &str,
        backend: ConfigBackend,
        content: &str,
        expected_version: Option<u64>,
    ) -> Result<u64> {
        self.inner
            .put(bot_id, plugin_name, backend, content, expected_version)
            .await
    }
}
