use anyhow::Result;
use async_trait::async_trait;
use ayiou_admin_proto::ConfigBackend;

use super::{ConfigRecord, ConfigStore, InMemoryConfigStore};

#[derive(Clone, Default)]
pub struct SqliteConfigStore {
    inner: InMemoryConfigStore,
    database_url: Option<String>,
}

impl SqliteConfigStore {
    pub fn in_memory() -> Self {
        Self {
            inner: InMemoryConfigStore::default(),
            database_url: Some("sqlite::memory:".to_string()),
        }
    }

    pub fn new(database_url: impl Into<String>) -> Self {
        Self {
            inner: InMemoryConfigStore::default(),
            database_url: Some(database_url.into()),
        }
    }

    pub fn database_url(&self) -> Option<&str> {
        self.database_url.as_deref()
    }
}

#[async_trait]
impl ConfigStore for SqliteConfigStore {
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
