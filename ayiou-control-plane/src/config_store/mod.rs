use std::{collections::HashMap, sync::Arc};

use anyhow::{Result, bail};
use async_trait::async_trait;
use ayiou_admin_proto::ConfigBackend;
use tokio::sync::Mutex;

#[cfg(feature = "postgres-backend")]
pub mod postgres;
#[cfg(feature = "redis-backend")]
pub mod redis;
#[cfg(feature = "sqlite-backend")]
pub mod sqlite;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigRecord {
    pub version: u64,
    pub backend: ConfigBackend,
    pub content: String,
}

#[async_trait]
pub trait ConfigStore: Send + Sync {
    async fn get(&self, bot_id: &str, plugin_name: &str) -> Result<Option<ConfigRecord>>;
    async fn put(
        &self,
        bot_id: &str,
        plugin_name: &str,
        backend: ConfigBackend,
        content: &str,
        expected_version: Option<u64>,
    ) -> Result<u64>;
}

#[derive(Clone, Default)]
pub struct InMemoryConfigStore {
    entries: Arc<Mutex<HashMap<(String, String), ConfigRecord>>>,
}

impl InMemoryConfigStore {
    pub async fn get(&self, bot_id: &str, plugin_name: &str) -> Result<Option<ConfigRecord>> {
        <Self as ConfigStore>::get(self, bot_id, plugin_name).await
    }
}

#[async_trait]
impl ConfigStore for InMemoryConfigStore {
    async fn get(&self, bot_id: &str, plugin_name: &str) -> Result<Option<ConfigRecord>> {
        let map = self.entries.lock().await;
        Ok(map
            .get(&(bot_id.to_string(), plugin_name.to_string()))
            .cloned())
    }

    async fn put(
        &self,
        bot_id: &str,
        plugin_name: &str,
        backend: ConfigBackend,
        content: &str,
        expected_version: Option<u64>,
    ) -> Result<u64> {
        let mut map = self.entries.lock().await;
        let key = (bot_id.to_string(), plugin_name.to_string());
        let current = map.get(&key).cloned();
        let actual = current.as_ref().map_or(0, |entry| entry.version);

        if let Some(expected) = expected_version
            && expected != actual
        {
            bail!("version conflict: expected {}, actual {}", expected, actual);
        }

        let next = actual + 1;
        map.insert(
            key,
            ConfigRecord {
                version: next,
                backend,
                content: content.to_string(),
            },
        );

        Ok(next)
    }
}

#[derive(Clone)]
pub enum StoreBackend {
    InMemory(InMemoryConfigStore),
    #[cfg(feature = "sqlite-backend")]
    Sqlite(sqlite::SqliteConfigStore),
    #[cfg(feature = "redis-backend")]
    Redis(redis::RedisConfigStore),
    #[cfg(feature = "postgres-backend")]
    Postgres(postgres::PostgresConfigStore),
}

impl Default for StoreBackend {
    fn default() -> Self {
        Self::InMemory(InMemoryConfigStore::default())
    }
}

#[async_trait]
impl ConfigStore for StoreBackend {
    async fn get(&self, bot_id: &str, plugin_name: &str) -> Result<Option<ConfigRecord>> {
        match self {
            StoreBackend::InMemory(store) => store.get(bot_id, plugin_name).await,
            #[cfg(feature = "sqlite-backend")]
            StoreBackend::Sqlite(store) => store.get(bot_id, plugin_name).await,
            #[cfg(feature = "redis-backend")]
            StoreBackend::Redis(store) => store.get(bot_id, plugin_name).await,
            #[cfg(feature = "postgres-backend")]
            StoreBackend::Postgres(store) => store.get(bot_id, plugin_name).await,
        }
    }

    async fn put(
        &self,
        bot_id: &str,
        plugin_name: &str,
        backend: ConfigBackend,
        content: &str,
        expected_version: Option<u64>,
    ) -> Result<u64> {
        match self {
            StoreBackend::InMemory(store) => {
                store
                    .put(bot_id, plugin_name, backend, content, expected_version)
                    .await
            }
            #[cfg(feature = "sqlite-backend")]
            StoreBackend::Sqlite(store) => {
                store
                    .put(bot_id, plugin_name, backend, content, expected_version)
                    .await
            }
            #[cfg(feature = "redis-backend")]
            StoreBackend::Redis(store) => {
                store
                    .put(bot_id, plugin_name, backend, content, expected_version)
                    .await
            }
            #[cfg(feature = "postgres-backend")]
            StoreBackend::Postgres(store) => {
                store
                    .put(bot_id, plugin_name, backend, content, expected_version)
                    .await
            }
        }
    }
}
