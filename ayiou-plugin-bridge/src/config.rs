//! Configuration for the external plugin bridge

use std::collections::HashMap;

use ayiou::core::Configurable;
use serde::Deserialize;

/// Bridge configuration
#[derive(Debug, Clone, Deserialize, Default)]
pub struct BridgeConfig {
    /// Whether the bridge is enabled
    #[serde(default)]
    pub enabled: bool,
    
    /// External plugins configuration
    #[serde(default)]
    pub plugins: HashMap<String, PluginConfig>,
}

impl Configurable for BridgeConfig {
    const PREFIX: &'static str = "external-plugin-bridge";
}

/// Configuration for a single external plugin
#[derive(Debug, Clone, Deserialize)]
pub struct PluginConfig {
    /// Command to run (e.g., "uv", "bun", "node")
    pub command: String,
    
    /// Arguments to pass to the command
    #[serde(default)]
    pub args: Vec<String>,
    
    /// Working directory for the plugin
    #[serde(default)]
    pub cwd: Option<String>,
    
    /// Environment variables to set
    #[serde(default)]
    pub env: HashMap<String, String>,
    
    /// Whether to restart on crash
    #[serde(default)]
    pub auto_restart: bool,
    
    /// Maximum restart attempts
    #[serde(default = "default_max_restarts")]
    pub max_restarts: u32,
}

fn default_max_restarts() -> u32 {
    3
}
