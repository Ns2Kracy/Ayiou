use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use ayiou_admin_proto::ConfigBackend;
use redis::AsyncCommands;

use super::{ConfigRecord, ConfigStore, backend_as_str, backend_from_str};

const PUT_SCRIPT: &str = r#"
local key = KEYS[1]
local expected = ARGV[1]
local backend = ARGV[2]
local content = ARGV[3]

local current = redis.call("HGET", key, "version")
if current then
  current = tonumber(current)
else
  current = 0
end

if expected ~= "" and tonumber(expected) ~= current then
  return redis.error_reply("version conflict: expected " .. expected .. ", actual " .. tostring(current))
end

local next = current + 1
redis.call("HSET", key, "version", tostring(next), "backend", backend, "content", content)
return next
"#;

#[derive(Clone)]
pub struct RedisConfigStore {
    client: redis::Client,
    endpoint: String,
    namespace: String,
}

impl RedisConfigStore {
    pub fn new(endpoint: impl Into<String>) -> Result<Self> {
        let endpoint = endpoint.into();
        let client = redis::Client::open(endpoint.clone())
            .with_context(|| format!("open redis client {}", endpoint))?;
        Ok(Self {
            client,
            endpoint,
            namespace: "ayiou:config".to_string(),
        })
    }

    pub fn with_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = namespace.into();
        self
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    fn key_for(&self, bot_id: &str, plugin_name: &str) -> String {
        format!(
            "{}:{}:{}",
            self.namespace,
            sanitize_segment(bot_id),
            sanitize_segment(plugin_name)
        )
    }
}

#[async_trait]
impl ConfigStore for RedisConfigStore {
    async fn get(&self, bot_id: &str, plugin_name: &str) -> Result<Option<ConfigRecord>> {
        let mut conn = self
            .client
            .get_multiplexed_tokio_connection()
            .await
            .with_context(|| format!("connect redis {}", self.endpoint))?;

        let key = self.key_for(bot_id, plugin_name);
        let map: std::collections::HashMap<String, String> = conn
            .hgetall(key)
            .await
            .context("read redis config record")?;

        if map.is_empty() {
            return Ok(None);
        }

        let version = map
            .get("version")
            .context("redis config version is missing")?
            .parse::<u64>()
            .context("parse redis config version")?;
        let backend = map
            .get("backend")
            .context("redis config backend is missing")?;
        let content = map
            .get("content")
            .context("redis config content is missing")?
            .clone();

        Ok(Some(ConfigRecord {
            version,
            backend: backend_from_str(backend)?,
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
        let mut conn = self
            .client
            .get_multiplexed_tokio_connection()
            .await
            .with_context(|| format!("connect redis {}", self.endpoint))?;

        let key = self.key_for(bot_id, plugin_name);
        let expected = expected_version.map_or_else(String::new, |v| v.to_string());

        let next: i64 = redis::Script::new(PUT_SCRIPT)
            .key(key)
            .arg(expected)
            .arg(backend_as_str(&backend))
            .arg(content)
            .invoke_async(&mut conn)
            .await
            .context("write redis config record")?;

        if next < 0 {
            bail!("invalid redis version {}", next);
        }
        Ok(next as u64)
    }
}

fn sanitize_segment(input: &str) -> String {
    input
        .chars()
        .map(|ch| match ch {
            ':' | '/' | '\\' | ' ' => '_',
            _ => ch,
        })
        .collect()
}
