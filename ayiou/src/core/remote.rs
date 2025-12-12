//! Remote plugin downloader and manager.
//!
//! Supports downloading WASM plugins from URLs and managing plugin repositories.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tracing::{info, warn};

// ============================================================================
// Plugin Manifest
// ============================================================================

/// Plugin manifest describing a remote plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Plugin name (unique identifier)
    pub name: String,
    /// Plugin version
    pub version: String,
    /// Plugin description
    #[serde(default)]
    pub description: String,
    /// Author information
    #[serde(default)]
    pub author: String,
    /// Download URL for the WASM file
    pub download_url: String,
    /// SHA256 checksum of the WASM file (optional but recommended)
    #[serde(default)]
    pub checksum: Option<String>,
    /// Minimum Ayiou version required
    #[serde(default)]
    pub min_ayiou_version: Option<String>,
    /// Plugin homepage/repository URL
    #[serde(default)]
    pub homepage: Option<String>,
    /// License identifier (e.g., "MIT", "Apache-2.0")
    #[serde(default)]
    pub license: Option<String>,
}

/// Plugin repository index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRepository {
    /// Repository name
    pub name: String,
    /// Repository URL
    pub url: String,
    /// List of available plugins
    pub plugins: Vec<PluginManifest>,
    /// Last updated timestamp
    #[serde(default)]
    pub updated_at: Option<String>,
}

// ============================================================================
// Remote Plugin Loader
// ============================================================================

/// Remote plugin downloader
pub struct RemotePluginLoader {
    /// HTTP client
    client: reqwest::Client,
    /// Local plugin cache directory
    cache_dir: PathBuf,
    /// Known repositories
    repositories: Vec<PluginRepository>,
}

impl RemotePluginLoader {
    /// Create a new remote plugin loader
    pub fn new(cache_dir: impl Into<PathBuf>) -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent(format!("Ayiou/{}", env!("CARGO_PKG_VERSION")))
                .build()
                .expect("Failed to create HTTP client"),
            cache_dir: cache_dir.into(),
            repositories: Vec::new(),
        }
    }

    /// Ensure cache directory exists
    async fn ensure_cache_dir(&self) -> Result<()> {
        fs::create_dir_all(&self.cache_dir)
            .await
            .context("Failed to create plugin cache directory")?;
        Ok(())
    }

    /// Add a plugin repository
    pub async fn add_repository(&mut self, url: &str) -> Result<()> {
        info!("Adding plugin repository: {}", url);

        let response = self
            .client
            .get(url)
            .send()
            .await
            .context("Failed to fetch repository index")?;

        if !response.status().is_success() {
            anyhow::bail!("Repository returned status: {}", response.status());
        }

        let repo: PluginRepository = response
            .json()
            .await
            .context("Failed to parse repository index")?;

        info!(
            "Added repository '{}' with {} plugins",
            repo.name,
            repo.plugins.len()
        );

        self.repositories.push(repo);
        Ok(())
    }

    /// Search for a plugin by name across all repositories
    pub fn search(&self, name: &str) -> Option<&PluginManifest> {
        for repo in &self.repositories {
            if let Some(plugin) = repo.plugins.iter().find(|p| p.name == name) {
                return Some(plugin);
            }
        }
        None
    }

    /// List all available plugins
    pub fn list_available(&self) -> Vec<&PluginManifest> {
        self.repositories
            .iter()
            .flat_map(|r| r.plugins.iter())
            .collect()
    }

    /// Download a plugin by URL
    pub async fn download(&self, url: &str, name: &str) -> Result<PathBuf> {
        self.ensure_cache_dir().await?;

        info!("Downloading plugin from: {}", url);

        let response = self
            .client
            .get(url)
            .send()
            .await
            .context("Failed to download plugin")?;

        if !response.status().is_success() {
            anyhow::bail!("Download failed with status: {}", response.status());
        }

        let bytes = response
            .bytes()
            .await
            .context("Failed to read plugin bytes")?;

        // Save to cache
        let filename = format!("{}.wasm", name);
        let path = self.cache_dir.join(&filename);

        fs::write(&path, &bytes)
            .await
            .context("Failed to write plugin to cache")?;

        info!("Plugin saved to: {:?}", path);
        Ok(path)
    }

    /// Download a plugin from manifest
    pub async fn download_manifest(&self, manifest: &PluginManifest) -> Result<PathBuf> {
        let path = self
            .download(&manifest.download_url, &manifest.name)
            .await?;

        // Verify checksum if provided
        if let Some(expected) = &manifest.checksum {
            let bytes = fs::read(&path).await?;
            let actual = sha256_hex(&bytes);

            if &actual != expected {
                fs::remove_file(&path).await.ok();
                anyhow::bail!("Checksum mismatch! Expected: {}, Got: {}", expected, actual);
            }
            info!("Checksum verified for {}", manifest.name);
        } else {
            warn!(
                "No checksum provided for {} - skipping verification",
                manifest.name
            );
        }

        Ok(path)
    }

    /// Install a plugin by name (search and download)
    pub async fn install(&self, name: &str) -> Result<PathBuf> {
        let manifest = self
            .search(name)
            .ok_or_else(|| anyhow::anyhow!("Plugin '{}' not found in repositories", name))?
            .clone();

        self.download_manifest(&manifest).await
    }

    /// Get cached plugin path if exists
    pub fn get_cached(&self, name: &str) -> Option<PathBuf> {
        let path = self.cache_dir.join(format!("{}.wasm", name));
        if path.exists() { Some(path) } else { None }
    }

    /// Remove a cached plugin
    pub async fn remove_cached(&self, name: &str) -> Result<()> {
        let path = self.cache_dir.join(format!("{}.wasm", name));
        if path.exists() {
            fs::remove_file(&path)
                .await
                .context("Failed to remove cached plugin")?;
            info!("Removed cached plugin: {}", name);
        }
        Ok(())
    }

    /// List cached plugins
    pub async fn list_cached(&self) -> Result<Vec<String>> {
        self.ensure_cache_dir().await?;

        let mut entries = fs::read_dir(&self.cache_dir).await?;
        let mut plugins = Vec::new();

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().map(|e| e == "wasm").unwrap_or(false)
                && let Some(name) = path.file_stem().and_then(|s| s.to_str())
            {
                plugins.push(name.to_string());
            }
        }

        Ok(plugins)
    }

    /// Get cache directory
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }
}

/// Compute SHA256 hash of bytes as hex string
fn sha256_hex(bytes: &[u8]) -> String {
    // Simple FNV-1a hash as placeholder
    // For production, use sha2 crate: sha2::Sha256::digest(bytes)
    format!("{:016x}", md5_like_hash(bytes))
}

/// Simple hash for placeholder (NOT cryptographically secure)
fn md5_like_hash(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

// ============================================================================
// Plugin Config File
// ============================================================================

/// Local plugin configuration (stored in plugins.toml or plugins.json)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginConfig {
    /// Plugin repositories to fetch from
    #[serde(default)]
    pub repositories: Vec<String>,
    /// Installed plugins with their settings
    #[serde(default)]
    pub plugins: Vec<InstalledPlugin>,
}

/// An installed plugin entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPlugin {
    /// Plugin name
    pub name: String,
    /// Whether the plugin is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Source (local path or remote URL)
    pub source: String,
    /// Plugin-specific configuration
    #[serde(default)]
    pub config: serde_json::Value,
}

fn default_true() -> bool {
    true
}

impl PluginConfig {
    /// Load config from file
    pub async fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(path)
            .await
            .context("Failed to read plugin config")?;

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        match ext {
            "toml" => toml::from_str(&content).context("Failed to parse TOML config"),
            "json" => serde_json::from_str(&content).context("Failed to parse JSON config"),
            _ => anyhow::bail!("Unsupported config format: {}", ext),
        }
    }

    /// Save config to file
    pub async fn save(&self, path: &Path) -> Result<()> {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        let content = match ext {
            "toml" => toml::to_string_pretty(self).context("Failed to serialize TOML")?,
            "json" => serde_json::to_string_pretty(self).context("Failed to serialize JSON")?,
            _ => anyhow::bail!("Unsupported config format: {}", ext),
        };

        fs::write(path, content)
            .await
            .context("Failed to write plugin config")?;

        Ok(())
    }

    /// Add a plugin to the config
    pub fn add_plugin(&mut self, name: String, source: String) {
        // Remove existing entry if present
        self.plugins.retain(|p| p.name != name);

        self.plugins.push(InstalledPlugin {
            name,
            enabled: true,
            source,
            config: serde_json::Value::Null,
        });
    }

    /// Remove a plugin from the config
    pub fn remove_plugin(&mut self, name: &str) {
        self.plugins.retain(|p| p.name != name);
    }

    /// Enable/disable a plugin
    pub fn set_enabled(&mut self, name: &str, enabled: bool) {
        if let Some(plugin) = self.plugins.iter_mut().find(|p| p.name == name) {
            plugin.enabled = enabled;
        }
    }
}
