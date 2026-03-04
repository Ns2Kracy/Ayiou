use anyhow::Result;
use async_trait::async_trait;
use ayiou_admin_proto::ConfigBackend;

use super::{ConfigRecord, ConfigStore, InMemoryConfigStore};

#[derive(Clone, Default)]
pub struct RedisConfigStore {
    inner: InMemoryConfigStore,
    endpoint: Option<String>,
}

impl RedisConfigStore {
    pub fn in_memory() -> Self {
        Self {
            inner: InMemoryConfigStore::default(),
            endpoint: Some("redis://127.0.0.1:6379".to_string()),
        }
    }

    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            inner: InMemoryConfigStore::default(),
            endpoint: Some(endpoint.into()),
        }
    }

    pub fn endpoint(&self) -> Option<&str> {
        self.endpoint.as_deref()
    }
}

#[async_trait]
impl ConfigStore for RedisConfigStore {
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
