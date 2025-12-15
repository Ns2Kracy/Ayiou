//! Configuration system for plugins.
//!
//! This module provides a TOML-based configuration system with support for
//! typed configuration sections and hot reloading.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use serde::de::DeserializeOwned;
use tracing::{debug, info};

// ============================================================================
// Configurable Trait
// ============================================================================

/// Trait for types that can be loaded from configuration
///
/// # Example
///
/// ```ignore
/// use serde::Deserialize;
/// use ayiou::core::config::Configurable;
///
/// #[derive(Debug, Deserialize, Default)]
/// pub struct MyPluginConfig {
///     pub enabled: bool,
///     pub timeout_ms: u64,
/// }
///
/// impl Configurable for MyPluginConfig {
///     const PREFIX: &'static str = "my-plugin";
/// }
/// ```
pub trait Configurable: DeserializeOwned + Default {
    /// Configuration section prefix (corresponds to TOML section name)
    ///
    /// For example, if PREFIX is "database", the configuration will be read
    /// from the `[database]` section in the TOML file.
    const PREFIX: &'static str;
}

// ============================================================================
// Configuration Store
// ============================================================================

/// Configuration storage with TOML support
///
/// # Example
///
/// ```ignore
/// use ayiou::core::config::{ConfigStore, Configurable};
///
/// let config = ConfigStore::from_file("config.toml")?;
/// let db_config: DatabaseConfig = config.get()?;
/// ```
pub struct ConfigStore {
    data: toml::Value,
    path: Option<PathBuf>,
}

impl Default for ConfigStore {
    fn default() -> Self {
        Self::empty()
    }
}

impl ConfigStore {
    /// Create an empty configuration store
    pub fn empty() -> Self {
        Self {
            data: toml::Value::Table(Default::default()),
            path: None,
        }
    }

    /// Create a configuration store from a TOML string
    pub fn parse(content: &str) -> Result<Self> {
        let data: toml::Value = toml::from_str(content)
            .map_err(|e| anyhow!("Failed to parse TOML: {}", e))?;
        Ok(Self { data, path: None })
    }

    /// Create a configuration store from a file
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        info!("Loading configuration from: {}", path.display());

        let content = std::fs::read_to_string(path)
            .map_err(|e| anyhow!("Failed to read config file '{}': {}", path.display(), e))?;

        let data: toml::Value = toml::from_str(&content)
            .map_err(|e| anyhow!("Failed to parse TOML in '{}': {}", path.display(), e))?;

        Ok(Self {
            data,
            path: Some(path.to_path_buf()),
        })
    }

    /// Get a typed configuration section
    ///
    /// If the section doesn't exist, returns the default value.
    pub fn get<C: Configurable>(&self) -> Result<C> {
        let section = self
            .data
            .get(C::PREFIX)
            .cloned()
            .unwrap_or(toml::Value::Table(Default::default()));

        debug!("Loading config section: {}", C::PREFIX);

        let config: C = section
            .try_into()
            .map_err(|e| anyhow!("Failed to deserialize config section '{}': {}", C::PREFIX, e))?;

        Ok(config)
    }

    /// Get a raw TOML value by key path
    ///
    /// Key path uses dot notation, e.g., "database.url"
    pub fn get_raw(&self, key: &str) -> Option<&toml::Value> {
        let parts: Vec<&str> = key.split('.').collect();
        let mut current = &self.data;

        for part in parts {
            current = current.get(part)?;
        }

        Some(current)
    }

    /// Check if a configuration section exists
    pub fn has_section(&self, prefix: &str) -> bool {
        self.data.get(prefix).is_some()
    }

    /// Reload configuration from file
    ///
    /// Only works if the configuration was loaded from a file.
    pub fn reload(&mut self) -> Result<()> {
        if let Some(ref path) = self.path {
            info!("Reloading configuration from: {}", path.display());

            let content = std::fs::read_to_string(path)
                .map_err(|e| anyhow!("Failed to read config file: {}", e))?;

            self.data = toml::from_str(&content)
                .map_err(|e| anyhow!("Failed to parse TOML: {}", e))?;

            Ok(())
        } else {
            Err(anyhow!("Cannot reload: configuration was not loaded from a file"))
        }
    }

    /// Get the configuration file path (if loaded from file)
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Merge another configuration into this one
    ///
    /// Values from `other` will override values in `self`.
    pub fn merge(&mut self, other: &ConfigStore) {
        merge_toml_values(&mut self.data, &other.data);
    }
}

/// Recursively merge TOML values
fn merge_toml_values(base: &mut toml::Value, other: &toml::Value) {
    match (base, other) {
        (toml::Value::Table(base_table), toml::Value::Table(other_table)) => {
            for (key, value) in other_table {
                if let Some(base_value) = base_table.get_mut(key) {
                    merge_toml_values(base_value, value);
                } else {
                    base_table.insert(key.clone(), value.clone());
                }
            }
        }
        (base, other) => {
            *base = other.clone();
        }
    }
}

// ============================================================================
// Common Configuration Types
// ============================================================================

/// Bot configuration section
#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct BotConfig {
    /// WebSocket URL for OneBot connection
    #[serde(default = "default_ws_url")]
    pub ws_url: String,

    /// Access token for OneBot authentication
    #[serde(default)]
    pub access_token: Option<String>,

    /// Bot name for logging
    #[serde(default)]
    pub name: Option<String>,
}

impl BotConfig {
    /// Get WebSocket URL with access_token appended if configured
    pub fn ws_url_with_token(&self) -> String {
        match &self.access_token {
            Some(token) if !token.is_empty() => {
                if self.ws_url.contains('?') {
                    format!("{}&access_token={}", self.ws_url, token)
                } else {
                    format!("{}?access_token={}", self.ws_url, token)
                }
            }
            _ => self.ws_url.clone(),
        }
    }
}

fn default_ws_url() -> String {
    "ws://127.0.0.1:8080".to_string()
}

impl Configurable for BotConfig {
    const PREFIX: &'static str = "bot";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, serde::Deserialize, Default, PartialEq)]
    struct TestConfig {
        #[serde(default)]
        value: String,
        #[serde(default)]
        number: i32,
    }

    impl Configurable for TestConfig {
        const PREFIX: &'static str = "test";
    }

    #[test]
    fn test_empty_config() {
        let store = ConfigStore::empty();
        let config: TestConfig = store.get().unwrap();
        assert_eq!(config.value, "");
        assert_eq!(config.number, 0);
    }

    #[test]
    fn test_parse() {
        let toml = r#"
            [test]
            value = "hello"
            number = 42
        "#;

        let store = ConfigStore::parse(toml).unwrap();
        let config: TestConfig = store.get().unwrap();
        assert_eq!(config.value, "hello");
        assert_eq!(config.number, 42);
    }

    #[test]
    fn test_missing_section() {
        let toml = r#"
            [other]
            value = "world"
        "#;

        let store = ConfigStore::parse(toml).unwrap();
        let config: TestConfig = store.get().unwrap();
        assert_eq!(config.value, "");
        assert_eq!(config.number, 0);
    }

    #[test]
    fn test_has_section() {
        let toml = r#"
            [test]
            value = "hello"
        "#;

        let store = ConfigStore::parse(toml).unwrap();
        assert!(store.has_section("test"));
        assert!(!store.has_section("other"));
    }

    #[test]
    fn test_merge() {
        let toml1 = r#"
            [test]
            value = "original"
            number = 1
        "#;

        let toml2 = r#"
            [test]
            value = "overridden"
        "#;

        let mut store1 = ConfigStore::parse(toml1).unwrap();
        let store2 = ConfigStore::parse(toml2).unwrap();

        store1.merge(&store2);

        let config: TestConfig = store1.get().unwrap();
        assert_eq!(config.value, "overridden");
        assert_eq!(config.number, 1);
    }
}
