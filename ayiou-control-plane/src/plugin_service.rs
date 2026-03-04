use std::{collections::HashMap, sync::Arc};

use anyhow::{Result, bail};
use async_trait::async_trait;
use ayiou_admin_proto::ConfigBackend;
use tokio::sync::Mutex;

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
