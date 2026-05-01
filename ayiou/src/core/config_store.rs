use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigRecord {
    pub version: u64,
    pub content: String,
}

#[async_trait::async_trait]
pub trait ConfigStore: Send + Sync {
    async fn get(&self, bot_id: &str, plugin: &str) -> Result<Option<ConfigRecord>>;

    async fn put(
        &self,
        bot_id: &str,
        plugin: &str,
        content: &str,
        expected_version: Option<u64>,
    ) -> Result<u64>;
}

#[derive(Debug, Clone)]
pub struct TomlConfigStore {
    root: PathBuf,
    write_lock: Arc<Mutex<()>>,
}

impl TomlConfigStore {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            write_lock: Arc::new(Mutex::new(())),
        }
    }

    fn record_path(&self, bot_id: &str, plugin: &str) -> PathBuf {
        let safe_bot = sanitize_segment(bot_id);
        let safe_plugin = sanitize_segment(plugin);
        self.root.join(safe_bot).join(format!("{safe_plugin}.toml"))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredRecord {
    version: u64,
    content: String,
}

#[async_trait::async_trait]
impl ConfigStore for TomlConfigStore {
    async fn get(&self, bot_id: &str, plugin: &str) -> Result<Option<ConfigRecord>> {
        let path = self.record_path(bot_id, plugin);
        let raw = match tokio::fs::read_to_string(&path).await {
            Ok(value) => value,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) => {
                return Err(err).with_context(|| format!("read config file {}", path.display()));
            }
        };

        let parsed: StoredRecord = toml::from_str(&raw)
            .with_context(|| format!("parse config file {}", path.display()))?;

        Ok(Some(ConfigRecord {
            version: parsed.version,
            content: parsed.content,
        }))
    }

    async fn put(
        &self,
        bot_id: &str,
        plugin: &str,
        content: &str,
        expected_version: Option<u64>,
    ) -> Result<u64> {
        let _guard = self.write_lock.lock().await;
        let path = self.record_path(bot_id, plugin);
        let current = self.get(bot_id, plugin).await?;
        let actual_version = current.as_ref().map_or(0, |record| record.version);

        if let Some(expected) = expected_version
            && expected != actual_version
        {
            bail!("version conflict: expected {expected}, actual {actual_version}");
        }

        let next_version = actual_version
            .checked_add(1)
            .ok_or_else(|| anyhow!("version overflow"))?;

        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("create config directory {}", parent.display()))?;
        }

        let payload = toml::to_string(&StoredRecord {
            version: next_version,
            content: content.to_string(),
        })
        .context("serialize config record to toml")?;

        tokio::fs::write(&path, payload)
            .await
            .with_context(|| format!("write config file {}", path.display()))?;

        Ok(next_version)
    }
}

fn sanitize_segment(input: &str) -> String {
    input
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => ch,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn toml_store_persists_and_versions_config() {
        let dir = tempfile::tempdir().unwrap();
        let store = TomlConfigStore::new(dir.path());

        let v1 = store.put("bot-a", "echo", "key='v1'", None).await.unwrap();
        let err = store
            .put("bot-a", "echo", "key='v2'", Some(v1 + 1))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("version conflict"));
    }

    #[tokio::test]
    async fn toml_store_reads_latest_content() {
        let dir = tempfile::tempdir().unwrap();
        let store = TomlConfigStore::new(dir.path());

        let v1 = store.put("bot-a", "echo", "key='v1'", None).await.unwrap();
        let v2 = store
            .put("bot-a", "echo", "key='v2'", Some(v1))
            .await
            .unwrap();

        let loaded = store.get("bot-a", "echo").await.unwrap().unwrap();
        assert_eq!(loaded.version, v2);
        assert_eq!(loaded.content, "key='v2'");
    }
}
